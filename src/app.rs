use crate::pdf::{Layout, Style};
use crate::{
    fonts,
    pdf::*,
    session::{self, BookmarkOp, Session},
    toolbar::icon,
};
use anyhow::{ensure, Context as _, Result};
use eframe::egui::{self, emath::GuiRounding as _, *};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    View,
    Edit,
    Text,
    AddImage,
    EditImage,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::View => "보기",
            Self::Edit => "문구 수정",
            Self::Text => "텍스트 추가",
            Self::AddImage => "이미지 추가",
            Self::EditImage => "이미지 편집",
        }
    }
}
#[derive(Clone)]
enum Cmd {
    Open,
    Unlock(PathBuf, String),
    SelectTab(usize),
    CloseTab(usize),
    CloseCurrent,
    New,
    Save,
    Undo,
    Redo,
    Merge,
    Rotate,
    Move(isize),
    Delete,
    Export(&'static str),
    Images,
    InsertImage(usize, Rect),
    ReplaceImage(usize, crate::pictures::Picture),
    ExportPicture(usize, usize, crate::pictures::Picture),
    Office,
    Bookmark(Option<usize>),
    Rename(usize),
    Remove(usize),
    Search,
    Spread,
    Fit,
    Width,
}
enum Prompt {
    Password(PathBuf),
    Pages(&'static str),
    Bookmark(Option<usize>),
    Rename(usize),
}
struct Job {
    child: Child,
    _temp: tempfile::TempDir,
    err: PathBuf,
}
struct Find {
    open: bool,
    query: String,
    options: session::SearchOptions,
    scan: usize,
    found: Vec<(usize, Vec<Rect>)>,
    active: usize,
    focus: bool,
    deadline: Instant,
}
impl Default for Find {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            options: session::SearchOptions::default(),
            scan: 0,
            found: vec![],
            active: 0,
            focus: false,
            deadline: Instant::now(),
        }
    }
}
struct DocumentTab {
    id: u64,
    chosen_bookmark: Option<usize>,
    doc: Option<Session>,
    page: usize,
    page_text: String,
    spread: bool,
    book: bool,
    props: bool,
    mode: Mode,
    zoom: f32,
    width_fit: bool,
    pan: Vec2,
    view: Rect,
    content: Vec2,
    scale: f32,
    quality: Instant,
    wheel_fraction: f64,
    page_sizes: HashMap<usize, Vec2>,
    surfaces: HashMap<usize, TextureHandle>,
    layouts: HashMap<usize, Layout>,
    toc: HashMap<usize, Vec<(Rect, Destination)>>,
    edit: Option<Edit>,
    edit_focus: bool,
    preview: Option<Document>,
    edit_error: String,
    drag: Option<(usize, Pos2)>,
    selection: Option<TextSelection>,
    chosen_face: String,
    font_query: String,
    font_picker: bool,
    font_cursor: usize,
    original_editor_font: Option<std::sync::Arc<FontData>>,
    pictures: HashMap<usize, Result<Vec<crate::pictures::Picture>, String>>,
    picture_source: Option<lopdf::Document>,
    picture_menu: Option<(usize, usize, crate::pictures::Picture)>,
    edit_base: Option<Vec<u8>>,
    base_fonts: FontDefinitions,
    editor_font: Option<std::sync::Arc<FontData>>,
    refresh_editor_font: bool,
    cursor_at: Option<usize>,
    find: Find,
    collapsed: HashSet<usize>,
    prompt: Option<Prompt>,
    input: String,
    focus_prompt: bool,
    error: String,
    cmd: Option<Cmd>,
    job: Option<Job>,
    pending_dest: Option<Destination>,
}
// Use destination pages, since an outline can contain containers and out-of-order links.
fn current_bookmark(
    list: &[Bookmark],
    page: usize,
    chosen: Option<usize>,
    collapsed: &HashSet<usize>,
) -> Option<usize> {
    let (index, entry) = list
        .iter()
        .enumerate()
        .filter_map(|(i, b)| Some((i, b, b.dest.as_ref()?.page)))
        .filter(|(_, _, start)| *start <= page)
        .max_by_key(|(i, b, start)| (*start, Some(*i) == chosen, b.depth, *i))
        .map(|(i, b, _)| (i, b))?;
    let mut visible = index;
    let mut depth = entry.depth;
    for (i, parent) in list[..index].iter().enumerate().rev() {
        if depth == 0 {
            break;
        }
        if parent.depth < depth {
            if collapsed.contains(&i) {
                visible = i;
            }
            depth = parent.depth;
        }
    }
    Some(visible)
}

impl DocumentTab {
    fn new(id: u64, defs: FontDefinitions) -> Self {
        Self {
            id,
            chosen_bookmark: None,
            doc: None,
            page: 0,
            page_text: "1".into(),
            spread: false,
            book: true,
            props: false,
            mode: Mode::View,
            zoom: 1.,
            width_fit: false,
            pan: Vec2::ZERO,
            view: Rect::NOTHING,
            content: Vec2::ZERO,
            scale: 1.,
            quality: Instant::now(),
            wheel_fraction: 0.,
            page_sizes: HashMap::new(),
            surfaces: Default::default(),
            layouts: Default::default(),
            toc: Default::default(),
            edit: None,
            edit_focus: false,
            preview: None,
            edit_error: String::new(),
            drag: None,
            selection: None,
            chosen_face: String::new(),
            font_query: String::new(),
            font_picker: false,
            font_cursor: 0,
            original_editor_font: None,
            pictures: HashMap::new(),
            picture_source: None,
            picture_menu: None,
            edit_base: None,
            base_fonts: defs,
            editor_font: None,
            refresh_editor_font: false,
            cursor_at: None,
            find: Default::default(),
            collapsed: Default::default(),
            prompt: None,
            input: String::new(),
            focus_prompt: false,
            error: String::new(),
            cmd: None,
            job: None,
            pending_dest: None,
        }
    }
    fn title(&self) -> String {
        self.doc
            .as_ref()
            .map(|s| {
                let name = s
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "새 문서.pdf".into());
                if s.dirty() || self.has_pending_edit() {
                    format!("{name} *")
                } else {
                    name
                }
            })
            .unwrap_or_else(|| "SimplePDF".into())
    }
    fn session(&self) -> Result<&Session> {
        self.doc.as_ref().context("PDF를 먼저 열어 주세요.")
    }
    fn count(&self) -> usize {
        self.doc.as_ref().map(|s| s.doc.count()).unwrap_or(0)
    }
    fn invalidate(&mut self) {
        self.wheel_fraction = 0.;
        self.pending_dest = None;
        self.page_sizes.clear();
        self.surfaces.clear();
        self.layouts.clear();
        self.selection = None;
        self.toc.clear();
        self.pictures.clear();
        self.picture_source = None;
        self.picture_menu = None;
        self.font_picker = false;
        self.original_editor_font = None;
        self.preview = None;
        self.edit_base = None;
        self.edit = None;
        self.edit_error.clear();
        self.collapsed.clear();
        self.chosen_bookmark = None;
        self.find.scan = 0;
        self.find.found.clear();
        self.find.active = 0;
        self.page = self.page.min(self.count().saturating_sub(1));
        self.page_text = (self.page + 1).to_string();
    }
    fn install_doc(&mut self, s: Session) {
        self.doc = Some(s);
        self.page = 0;
        self.zoom = 1.;
        self.width_fit = false;
        self.pan = Vec2::ZERO;
        self.find = Default::default();
        self.mode = Mode::View;
        self.props = false;
        self.invalidate();
    }
    fn save(&mut self) -> Result<bool> {
        self.commit()?;
        let s = self.session()?;
        let name = s
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "문서.pdf".into());
        let Some(file) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(name)
            .save_file()
        else {
            return Ok(false);
        };
        self.doc.as_mut().unwrap().save(&file)?;
        Ok(true)
    }
    fn leave(&mut self) -> Result<bool> {
        self.commit()?;
        if !self.doc.as_ref().is_some_and(Session::dirty) {
            return Ok(true);
        }
        use rfd::{MessageButtons, MessageDialog, MessageDialogResult};
        match MessageDialog::new()
            .set_title("SimplePDF")
            .set_description(format!(
                "{}의 변경 내용을 저장할까요?",
                self.title().trim_end_matches(" *")
            ))
            .set_buttons(MessageButtons::YesNoCancel)
            .show()
        {
            MessageDialogResult::Yes => self.save(),
            MessageDialogResult::No => Ok(true),
            _ => Ok(false),
        }
    }
    fn ask(&mut self, p: Prompt, s: &str) {
        self.prompt = Some(p);
        self.input = s.into();
        self.focus_prompt = true;
    }
    fn apply(&mut self, f: impl FnOnce(&Document) -> Result<()>) -> Result<()> {
        self.commit()?;
        self.doc
            .as_mut()
            .context("PDF를 먼저 열어 주세요.")?
            .apply(f)?;
        self.invalidate();
        Ok(())
    }
    fn has_pending_edit(&self) -> bool {
        self.edit
            .as_ref()
            .is_some_and(|edit| match edit.runs.first() {
                None => !edit.text.is_empty(),
                Some(first) => {
                    edit.style.font.is_some()
                        || edit.style.size != first.size
                        || edit.style.rgb != first.rgb
                        || !edit
                            .text
                            .chars()
                            .eq(edit.runs.iter().flat_map(|r| r.text.chars()))
                }
            })
    }
    fn commit(&mut self) -> Result<()> {
        if let Some(edit) = &self.edit {
            if !self.has_pending_edit() {
                self.cancel_edit();
                return Ok(());
            }
            self.doc
                .as_mut()
                .context("문서")?
                .edit(edit, self.edit_base.as_deref())?;
            self.invalidate();
        }
        Ok(())
    }
    fn cancel_edit(&mut self) {
        self.font_picker = false;
        self.original_editor_font = None;
        self.edit = None;
        self.preview = None;
        self.edit_base = None;
        self.surfaces.clear();
        self.edit_error.clear();
    }
    fn update_preview(&mut self) {
        let Some(edit) = &self.edit else { return };
        let s = self.doc.as_ref().unwrap();
        let res = (|| -> Result<Document> {
            let d = Document::open(
                self.edit_base
                    .as_deref()
                    .unwrap_or_else(|| s.bytes())
                    .to_vec(),
                &s.password,
            )?;
            d.page(edit.page)?.edit(edit)?;
            Ok(d)
        })();
        match res {
            Ok(d) => {
                self.preview = Some(d);
                self.edit_error.clear();
                self.surfaces.remove(&edit.page);
            }
            Err(e) => self.edit_error = e.to_string(),
        }
    }
    fn go(&mut self, page: usize) -> Result<()> {
        self.commit()?;
        self.selection = None;
        self.page = page.min(self.count().saturating_sub(1));
        self.page_text = (self.page + 1).to_string();
        self.pan = Vec2::ZERO;
        self.wheel_fraction = 0.;
        self.pending_dest = None;
        Ok(())
    }
    fn step(&mut self, d: isize) -> Result<()> {
        let k = if self.spread { 2 } else { 1 };
        self.go(((self.page / k) * k).saturating_add_signed(d * k as isize))
    }
    fn dest(&mut self, d: Destination) -> Result<()> {
        self.go(d.page)?;
        self.pending_dest = Some(d);
        Ok(())
    }
    fn change_zoom(&mut self, next: f32) {
        self.wheel_fraction = 0.;
        let old = self.zoom;
        self.width_fit = false;
        self.zoom = next.clamp(0.15, 12.);
        self.pan *= self.zoom / old;
        self.quality = Instant::now() + Duration::from_millis(120);
    }
    fn choose_mode(&mut self, m: Mode) -> Result<()> {
        self.commit()?;
        if m != Mode::View {
            ensure!(
                self.session()?.doc.editable(),
                "편집 권한이 제한되어 있습니다."
            )
        }
        if m != Mode::EditImage {
            self.picture_source = None;
            self.pictures.clear();
        }
        self.drag = None;
        self.selection = None;
        self.picture_menu = None;
        self.mode = m;
        self.props = matches!(m, Mode::Edit | Mode::Text);
        Ok(())
    }
    fn execute(&mut self, cmd: Cmd) -> Result<()> {
        if self.job.is_some() {
            return Ok(());
        }
        let page = self.page;
        match cmd {
            Cmd::Open
            | Cmd::New
            | Cmd::Unlock(..)
            | Cmd::SelectTab(_)
            | Cmd::CloseTab(_)
            | Cmd::CloseCurrent => self.cmd = Some(cmd),
            Cmd::Save => {
                self.save()?;
            }
            Cmd::Undo | Cmd::Redo => {
                self.cancel_edit();
                if let Some(s) = self.doc.as_mut() {
                    s.undo(matches!(cmd, Cmd::Redo))?;
                    self.invalidate();
                }
            }
            Cmd::Rotate => self.apply(|d| d.rotate(page))?,
            Cmd::Move(delta) => {
                let to = page.saturating_add_signed(delta);
                if to != page && to < self.count() {
                    self.apply(|d| d.move_page(page, to))?;
                    self.go(to)?;
                }
            }
            Cmd::Delete => {
                if rfd::MessageDialog::new()
                    .set_title("페이지 삭제")
                    .set_description(format!("{}페이지를 삭제할까요?", page + 1))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    == rfd::MessageDialogResult::Yes
                {
                    self.apply(|d| d.delete(page))?;
                }
            }
            Cmd::Merge => {
                self.session()?;
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .pick_files()
                {
                    self.apply(|d| {
                        for path in paths {
                            let other = Document::open(std::fs::read(path)?, "")?;
                            ensure!(other.extractable(), "페이지 추출 권한이 필요합니다.");
                            d.import(&other, &(0..other.count()).collect::<Vec<_>>())?;
                        }
                        Ok(())
                    })?;
                }
            }
            Cmd::Export(kind) => {
                self.commit()?;
                self.session()?;
                self.ask(Prompt::Pages(kind), "");
            }
            Cmd::InsertImage(page, rect) => {
                if let Some(img) = Self::pick_image("선택 영역에 이미지 추가")? {
                    let fit = crate::pictures::fill_frame(&img, rect.size());
                    self.apply(|d| d.page(page)?.add_image(rect, &fit))?;
                }
            }
            Cmd::ExportPicture(page, index, picture) => {
                let s = self.session()?;
                ensure!(
                    s.doc.extractable(),
                    "이 PDF는 이미지 추출 권한이 제한되어 있습니다."
                );
                let stem = s
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .and_then(|s| s.to_str())
                    .unwrap_or("문서");
                let name = format!("{stem}-{}쪽-이미지{}.png", page + 1, index + 1);
                if let Some(mut path) = rfd::FileDialog::new()
                    .set_title("이미지 내보내기")
                    .add_filter("PNG 이미지", &["png"])
                    .set_file_name(name)
                    .save_file()
                {
                    if path.extension().is_none() {
                        path.set_extension("png");
                    }
                    ensure!(
                        path.extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("png")),
                        "파일 확장자를 .png로 지정해 주세요."
                    );
                    s.export_picture(page, &picture, &path)?;
                }
            }
            Cmd::ReplaceImage(page, picture) => {
                if let Some(image) = Self::pick_image("선택한 이미지 교체")? {
                    self.doc
                        .as_mut()
                        .context("문서")?
                        .replace_picture(page, &picture, &image)?;
                    self.invalidate();
                }
            }
            Cmd::Images => {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("이미지", &["png", "jpg", "jpeg", "bmp", "tif", "tiff"])
                    .pick_files()
                {
                    if let Some(out) = rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .set_file_name("이미지.pdf")
                        .save_file()
                    {
                        save_atomic(&out, &Document::images(&paths)?)?;
                    }
                }
            }
            Cmd::Office => {
                if let Some(src) = rfd::FileDialog::new()
                    .set_title("PDF로 변환할 문서 선택")
                    .add_filter(
                        "Word · PowerPoint · Excel",
                        crate::export::OFFICE_EXTENSIONS,
                    )
                    .pick_file()
                {
                    if let Some(out) = rfd::FileDialog::new()
                        .set_directory(src.parent().context("문서 폴더")?)
                        .add_filter("PDF", &["pdf"])
                        .set_file_name(
                            src.with_extension("pdf")
                                .file_name()
                                .unwrap()
                                .to_string_lossy(),
                        )
                        .save_file()
                    {
                        self.start_job("office", &src, &out, "", "")?;
                    }
                }
            }
            Cmd::Bookmark(parent) => {
                self.session()?;
                self.ask(Prompt::Bookmark(parent), &format!("{}페이지", page + 1));
            }
            Cmd::Rename(i) => {
                let title = self.session()?.bookmarks[i].title.clone();
                self.ask(Prompt::Rename(i), &title);
            }
            Cmd::Remove(i) => {
                self.commit()?;
                self.doc
                    .as_mut()
                    .context("문서")?
                    .bookmark(BookmarkOp::Remove(i))?;
                self.invalidate();
            }
            Cmd::Search => {
                self.find.open = true;
                self.find.focus = true;
            }
            Cmd::Spread => {
                self.commit()?;
                self.wheel_fraction = 0.;
                self.spread = !self.spread;
                self.pan = Vec2::ZERO;
                self.surfaces.clear();
            }
            Cmd::Fit => {
                self.wheel_fraction = 0.;
                self.zoom = 1.;
                self.width_fit = false;
                self.pan = Vec2::ZERO;
            }
            Cmd::Width => {
                self.wheel_fraction = 0.;
                self.width_fit = true;
                self.pan = Vec2::ZERO;
            }
        }
        Ok(())
    }
    fn start_job(
        &mut self,
        kind: &str,
        src: &Path,
        target: &Path,
        pages: &str,
        password: &str,
    ) -> Result<()> {
        use std::os::windows::process::CommandExt;
        let temp = tempfile::tempdir()?;
        let mut input = src.to_path_buf();
        if kind != "office" {
            input = temp.path().join("document.pdf");
            std::fs::write(&input, self.session()?.bytes())?;
        }
        let err = temp.path().join("error.txt");
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--convert")
            .arg(kind)
            .arg(&input)
            .arg(target)
            .arg(pages)
            .arg(&err)
            .stdin(Stdio::piped())
            .creation_flags(0x08000000)
            .spawn()?;
        if let Some(mut pipe) = child.stdin.take() {
            pipe.write_all(password.as_bytes())?;
        }
        self.job = Some(Job {
            child,
            _temp: temp,
            err,
        });
        Ok(())
    }
    fn export(&mut self, kind: &str, range: &str) -> Result<()> {
        let s = self.session()?;
        ensure!(
            s.doc.extractable(),
            "이 PDF는 추출 권한이 제한되어 있습니다."
        );
        session::pages(range, self.count())?;
        let name = s
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .unwrap_or(std::ffi::OsStr::new("문서"))
            .to_string_lossy();
        let password = s.password.clone();
        let out = if matches!(kind, "png" | "jpg") {
            rfd::FileDialog::new()
                .pick_folder()
                .map(|f| f.join(format!("{name}_{kind}")))
        } else {
            rfd::FileDialog::new()
                .add_filter(kind, &[kind])
                .set_file_name(format!("{name}.{kind}"))
                .save_file()
        };
        if let Some(out) = out {
            self.start_job(kind, Path::new(""), &out, range, &password)?;
        }
        Ok(())
    }
    fn finish_prompt(&mut self) -> Result<()> {
        let Some(p) = self.prompt.take() else {
            return Ok(());
        };
        let input = std::mem::take(&mut self.input);
        match p {
            Prompt::Password(path) => self.cmd = Some(Cmd::Unlock(path, input)),
            Prompt::Pages(kind) => self.export(kind, &input)?,
            Prompt::Bookmark(parent) => {
                self.commit()?;
                ensure!(!input.trim().is_empty(), "책갈피 이름을 입력하세요.");
                self.doc
                    .as_mut()
                    .unwrap()
                    .bookmark(BookmarkOp::Add(input, self.page, parent))?;
                self.invalidate();
            }
            Prompt::Rename(i) => {
                self.commit()?;
                ensure!(!input.trim().is_empty(), "책갈피 이름을 입력하세요.");
                self.doc
                    .as_mut()
                    .unwrap()
                    .bookmark(BookmarkOp::Rename(i, input))?;
                self.invalidate();
            }
        }
        Ok(())
    }
    fn menu(&mut self, ui: &mut Ui) {
        for (name, cmd) in [
            ("열기   Ctrl+O", Cmd::Open),
            ("새 PDF", Cmd::New),
            ("탭 닫기   Ctrl+W", Cmd::CloseCurrent),
            ("다른 이름으로 저장   Ctrl+S", Cmd::Save),
            ("실행 취소   Ctrl+Z", Cmd::Undo),
            ("다시 실행   Ctrl+Y", Cmd::Redo),
        ] {
            self.menu_item(ui, name, cmd)
        }
        ui.menu_button("페이지", |ui| {
            for (n, c) in [
                ("PDF 추가·병합", Cmd::Merge),
                ("페이지 추출", Cmd::Export("pdf")),
                ("시계 방향 회전", Cmd::Rotate),
                ("앞으로 이동", Cmd::Move(-1)),
                ("뒤로 이동", Cmd::Move(1)),
                ("삭제", Cmd::Delete),
            ] {
                self.menu_item(ui, n, c)
            }
        });
        ui.menu_button("변환", |ui| {
            for (n, c) in [
                ("PDF → Word", Cmd::Export("docx")),
                ("PDF 표 → Excel", Cmd::Export("xlsx")),
                ("PDF → PNG", Cmd::Export("png")),
                ("PDF → JPG", Cmd::Export("jpg")),
                ("이미지 → PDF", Cmd::Images),
                ("Word · PowerPoint · Excel → PDF", Cmd::Office),
            ] {
                self.menu_item(ui, n, c)
            }
        });
        self.menu_item(ui, "너비 맞춤", Cmd::Width);
        self.menu_item(ui, "현재 페이지 책갈피 추가", Cmd::Bookmark(None));
        if ui.checkbox(&mut self.props, "텍스트 속성").clicked() {
            ui.close_menu();
        }
    }
    fn menu_item(&mut self, ui: &mut Ui, n: &str, c: Cmd) {
        if ui.button(n).clicked() {
            self.cmd = Some(c);
            ui.close_menu()
        }
    }
    fn toolbar(&mut self, ctx: &egui::Context, titles: &[TabLabel], active: usize) {
        TopBottomPanel::top("bar")
            .exact_height(40.)
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(252, 252, 253))
                    .inner_margin(Margin::symmetric(6, 4)),
            )
            .show(ctx, |ui| {
                let r = ui.max_rect();
                let left = ui
                    .horizontal_centered(|ui| {
                        ui.menu_button("☰", |ui| self.menu(ui));
                        if icon(
                            ui,
                            "book",
                            if self.book {
                                "책갈피 접기 · Ctrl+B"
                            } else {
                                "책갈피 펼치기 · Ctrl+B"
                            },
                            self.book,
                        )
                        .clicked()
                        {
                            self.book = !self.book
                        }
                        if icon(ui, "open", "열기 · Ctrl+O", false).clicked() {
                            self.cmd = Some(Cmd::Open)
                        }
                        if icon(ui, "save", "저장 · Ctrl+S", false).clicked() {
                            self.cmd = Some(Cmd::Save)
                        }
                        if icon(ui, "find", "검색 · Ctrl+F", self.find.open).clicked() {
                            self.cmd = Some(Cmd::Search)
                        }
                        ComboBox::from_id_salt("mode")
                            .selected_text(self.mode.label())
                            .width(82.)
                            .show_ui(ui, |ui| {
                                for m in [
                                    Mode::View,
                                    Mode::Edit,
                                    Mode::Text,
                                    Mode::AddImage,
                                    Mode::EditImage,
                                ] {
                                    if ui.selectable_label(m == self.mode, m.label()).clicked() {
                                        if let Err(e) = self.choose_mode(m) {
                                            self.error = e.to_string()
                                        }
                                    }
                                }
                            });
                        ui.min_rect().right()
                    })
                    .inner;
                let mut right = r.right();
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(r)
                        .layout(egui::Layout::right_to_left(Align::Center)),
                    |ui| {
                        if icon(ui, "close", "닫기", false).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close)
                        }
                        if icon(ui, "max", "최대화 / 복원", false).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(
                                !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                            ))
                        }
                        if icon(ui, "min", "최소화", false).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true))
                        }
                        ui.add_space(5.);
                        if icon(ui, "spread", "두 페이지 · Ctrl+2", self.spread).clicked() {
                            self.cmd = Some(Cmd::Spread)
                        }
                        if icon(
                            ui,
                            "fit",
                            "페이지 맞춤 · Ctrl+9",
                            !self.width_fit && self.zoom == 1.,
                        )
                        .clicked()
                        {
                            self.cmd = Some(Cmd::Fit)
                        }
                        ui.menu_button(format!("{}%", (self.zoom * 100.).round() as i32), |ui| {
                            for n in [50, 75, 100, 125, 150, 200, 300] {
                                if ui.button(format!("{n}%")).clicked() {
                                    self.change_zoom(n as f32 / 100.);
                                    ui.close_menu()
                                }
                            }
                            self.menu_item(ui, "너비 맞춤", Cmd::Width)
                        });
                        if icon(ui, "next", "다음 페이지", false).clicked() {
                            if let Err(e) = self.step(1) {
                                self.error = e.to_string()
                            }
                        }
                        ui.label(format!("/ {}", self.count()));
                        let entry = ui.add(
                            TextEdit::singleline(&mut self.page_text)
                                .id(Id::new(("page_entry", self.id)))
                                .desired_width(30.),
                        );
                        if entry.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            if let Ok(n) = self.page_text.parse::<usize>() {
                                if let Err(e) = self.go(n.saturating_sub(1)) {
                                    self.error = e.to_string()
                                }
                            }
                        }
                        if icon(ui, "prev", "이전 페이지", false).clicked() {
                            if let Err(e) = self.step(-1) {
                                self.error = e.to_string()
                            }
                        }
                        right = ui.cursor().right();
                    },
                );
                if right > left + 8. {
                    let free =
                        Rect::from_min_max(pos2(left + 3., r.top()), pos2(right - 3., r.bottom()));
                    if titles.len() > 1 {
                        if let Some(command) = tab_strip(ui, free, titles, active) {
                            self.cmd = Some(command);
                        }
                        return;
                    }
                    let center = r.center().x.clamp(free.left(), free.right());
                    let width = 2. * (center - free.left()).min(free.right() - center);
                    let name = self.title();
                    if width > 18. {
                        let galley = ui.painter().layout_no_wrap(
                            name.clone(),
                            FontId::proportional(13.),
                            Color32::BLACK,
                        );
                        let clip = ui.painter().with_clip_rect(Rect::from_center_size(
                            pos2(center, r.center().y),
                            vec2(width, r.height()),
                        ));
                        clip.galley(
                            pos2(
                                center - galley.size().x / 2.,
                                r.center().y - galley.size().y / 2.,
                            ),
                            galley,
                            Color32::BLACK,
                        );
                    }
                    let response = ui
                        .interact(free, Id::new("title_drag"), Sense::click_and_drag())
                        .on_hover_text(name);
                    drag_window(ctx, &response);
                }
            });
    }
    fn bookmarks(&mut self, ctx: &egui::Context) {
        SidePanel::left("bookmarks")
            .default_width(176.)
            .width_range(
                105. ..=((ctx.available_rect().width() - if self.props { 240. } else { 0. })
                    * 0.72)
                    .clamp(176., 960.),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("책갈피");
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if icon(ui, "plus", "현재 페이지 추가", false).clicked() {
                            self.cmd = Some(Cmd::Bookmark(None))
                        }
                    });
                });
                let mut navigate = None;
                let mut menu_command = None;
                let mut hidden = None;
                let list = self
                    .doc
                    .as_ref()
                    .map(|s| s.bookmarks.as_slice())
                    .unwrap_or_default();
                let selected =
                    current_bookmark(list, self.page, self.chosen_bookmark, &self.collapsed);
                ScrollArea::vertical()
                    .id_salt(("outline", self.id))
                    .show(ui, |ui| {
                        for (i, b) in list.iter().enumerate() {
                            if hidden.is_some_and(|d| b.depth > d) {
                                continue;
                            }
                            hidden = None;
                            let has_child = list.get(i + 1).is_some_and(|n| n.depth > b.depth);
                            ui.horizontal(|ui| {
                                // Leave room for the disclosure button and title even at the minimum panel width.
                                let indent = (b.depth.min(10) as f32 * 10.)
                                    .min((ui.available_width() - 64.).max(0.));
                                ui.add_space(indent);
                                if has_child {
                                    let closed = self.collapsed.contains(&i);
                                    if crate::toolbar::disclosure(ui, closed).clicked() {
                                        if closed {
                                            self.collapsed.remove(&i);
                                        } else {
                                            self.collapsed.insert(i);
                                        }
                                    }
                                } else {
                                    ui.allocate_exact_size(vec2(18., 18.), Sense::hover());
                                }
                                ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
                                let res = ui
                                    .selectable_label(selected == Some(i), &b.title)
                                    .on_hover_text(&b.title);
                                if res.clicked() {
                                    navigate = b.dest.clone().map(|d| (i, d))
                                }
                                res.context_menu(|ui| {
                                    for (label, cmd) in [
                                        ("하위 책갈피 추가", Cmd::Bookmark(Some(i))),
                                        ("이름 변경", Cmd::Rename(i)),
                                        ("삭제", Cmd::Remove(i)),
                                    ] {
                                        if ui.button(label).clicked() {
                                            menu_command = Some(cmd);
                                            ui.close_menu();
                                        }
                                    }
                                });
                            });
                            if self.collapsed.contains(&i) {
                                hidden = Some(b.depth)
                            }
                        }
                    });
                if let Some(cmd) = menu_command {
                    self.cmd = Some(cmd);
                }
                if let Some((i, d)) = navigate {
                    match self.dest(d) {
                        Ok(()) => self.chosen_bookmark = Some(i),
                        Err(e) => self.error = e.to_string(),
                    }
                }
            });
    }
}
include!("properties.rs");
include!("view.rs");
include!("tabs.rs");
pub fn verify_image_modes() -> Result<()> {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = App::new(&cc, vec![]);
    let tab = &mut app.tabs[0];
    let doc = Document::blank()?;
    let rect = Rect::from_min_size(pos2(80., 100.), vec2(150., 110.));
    let bitmap = image::DynamicImage::new_rgb8(8, 5);
    doc.page(0)?.add_image(rect, &bitmap)?;
    let other = Rect::from_min_size(pos2(320., 465.), vec2(18., 90.));
    doc.page(0)?
        .add_image(other, &image::DynamicImage::new_rgb8(1158, 793))?;
    tab.install_doc(Session::open(doc.bytes()?, None, String::new())?);
    let draw = |tab: &mut DocumentTab, events: Vec<Event>| {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1080., 720.))),
            events,
            ..Default::default()
        };
        ctx.run(input, |ctx| tab.draw(ctx, &[], 0, &mut false))
    };
    let tick = |tab: &mut DocumentTab, events: Vec<Event>| {
        let _ = draw(tab, events);
        tab.cmd.take()
    };
    let check_pixel_label = |output: &FullOutput, label: &str| -> Result<()> {
        let (text, clip) = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                Shape::Text(t) if t.galley.job.text == label => Some((t, s.clip_rect)),
                _ => None,
            })
            .context("이미지 픽셀 표시 누락")?;
        let bounds = Rect::from_min_size(text.pos, text.galley.size());
        ensure!(
            text.galley.rows.len() == 1,
            "{label} 줄 수: {}",
            text.galley.rows.len()
        );
        ensure!(
            clip.intersect(ctx.screen_rect()).contains_rect(bounds),
            "{label} 표시가 창 밖으로 잘림: {bounds:?}"
        );
        Ok(())
    };
    let gesture = |tab: &mut DocumentTab, from: Pos2, to: Pos2| {
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        tick(tab, vec![Event::PointerMoved(from)]);
        tick(tab, vec![button(from, true)]);
        if from != to {
            tick(tab, vec![Event::PointerMoved(to)]);
        }
        tick(tab, vec![button(to, false)])
    };
    tab.choose_mode(Mode::EditImage)?;
    tick(tab, vec![]);
    tick(tab, vec![]);
    let layout = tab.row_layout(0, tab.view)?;
    let r = layout.visible_pages(tab.view, tab.pan, ctx.pixels_per_point())[0].rect;
    let screen = |p: Pos2| r.min + p.to_vec2() * layout.scale;
    let center = screen(rect.center());
    let blank = screen(pos2(330., 340.));
    let end = screen(pos2(420., 430.));
    tick(tab, vec![Event::PointerMoved(center)]);
    let hovered = draw(tab, vec![]);
    check_pixel_label(&hovered, "8 × 5 px")?;
    let next = draw(tab, vec![Event::PointerMoved(screen(other.center()))]);
    let long_label = "1158 × 793 px";
    check_pixel_label(&next, long_label)?;
    ensure!(
        label_position(&next, "8 × 5 px").is_none(),
        "가리키는 이미지에 따라 크기 변경"
    );
    tab.change_zoom(1.5);
    tick(tab, vec![]);
    let zoomed = tab.row_layout(0, tab.view)?;
    let origin = zoomed.visible_pages(tab.view, tab.pan, ctx.pixels_per_point())[0]
        .rect
        .min;
    let frame = draw(
        tab,
        vec![Event::PointerMoved(
            origin + other.center().to_vec2() * zoomed.scale,
        )],
    );
    check_pixel_label(&frame, long_label)?;
    let short = origin + rect.center().to_vec2() * zoomed.scale;
    let previous = draw(tab, vec![Event::PointerMoved(short)]);
    check_pixel_label(&previous, "8 × 5 px")?;
    tab.change_zoom(4.);
    let enlarged = tab.row_layout(0, tab.view)?;
    let page = enlarged.visible_pages(tab.view, Vec2::ZERO, ctx.pixels_per_point())[0].rect;
    let corner = tab.view.right_bottom() - vec2(16., 6.);
    tab.pan = corner - (page.min + other.center().to_vec2() * enlarged.scale);
    let edge = draw(tab, vec![Event::PointerMoved(corner)]);
    check_pixel_label(&edge, long_label)?;
    tab.execute(Cmd::Fit)?;
    tick(tab, vec![]);
    let away = draw(tab, vec![Event::PointerMoved(blank)]);
    ensure!(
        label_position(&away, long_label).is_none(),
        "이미지 밖에서 크기 표시 닫기"
    );
    ensure!(
        matches!(gesture(tab,center,center),Some(Cmd::ReplaceImage(0,p)) if p.contains(rect.center())),
        "기존 사진 클릭으로 교체"
    );
    ensure!(
        gesture(tab, blank, blank).is_none()
            && gesture(tab, blank, end).is_none()
            && gesture(tab, center, end).is_none(),
        "이미지 편집의 빈 곳 클릭과 드래그"
    );
    tab.choose_mode(Mode::AddImage)?;
    tick(tab, vec![]);
    ensure!(
        gesture(tab, center, center).is_none(),
        "이미지 추가의 기존 사진 클릭"
    );
    ensure!(
        matches!(gesture(tab,blank,end),Some(Cmd::InsertImage(0,area)) if area.min.distance(pos2(330.,340.))<0.1 && area.max.distance(pos2(420.,430.))<0.1),
        "드래그 영역에만 이미지 추가"
    );
    for (mode, center, number, pixels) in [
        (Mode::View, center, 0, [8, 5]),
        (Mode::EditImage, screen(other.center()), 1, [1158, 793]),
    ] {
        tab.choose_mode(mode)?;
        tick(tab, vec![Event::PointerMoved(center)]);
        for pressed in [true, false] {
            tick(
                tab,
                vec![Event::PointerButton {
                    pos: center,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        let menu = draw(tab, vec![]);
        let item =
            label_position(&menu, "이미지 내보내기…").context("사진 우클릭 내보내기 메뉴")?;
        ensure!(
            matches!(gesture(tab,item,item),Some(Cmd::ExportPicture(0,idx,p)) if idx==number && p.pixels==pixels),
            "우클릭한 사진 내보내기"
        );
        tick(tab, vec![]);
    }
    ensure!(tab.error.is_empty(), "이미지 도구 오류: {}", tab.error);
    Ok(())
}
fn label_position(output: &FullOutput, label: &str) -> Option<Pos2> {
    output.shapes.iter().find_map(|s| match &s.shape {
        Shape::Text(t) if t.galley.job.text == label => Some(t.pos + t.galley.size() * 0.5),
        _ => None,
    })
}

pub fn verify_controls(path: &Path) -> Result<()> {
    use eframe::App as _;
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = App::new(&cc, vec![]);
    app.open_path(path.to_path_buf(), "", &ctx)?;
    let mut frame = eframe::Frame::_new_kittest();
    let tick = |a: &mut App, events: Vec<Event>, mods: Modifiers, frame: &mut eframe::Frame| {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1080., 720.))),
            modifiers: mods,
            events,
            ..Default::default()
        };
        ctx.run(raw, |ctx| a.update(ctx, frame))
    };
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    let center = app.tabs[0].view.center();
    ensure!(
        app.tabs[0].book
            && app.tabs[0].zoom == 1.
            && app.tabs[0].content.y <= app.tabs[0].view.height() + 1.,
        "기본 화면 맞춤"
    );
    for (zoom, reverse) in [(1., false), (2., true)] {
        let tab = &mut app.tabs[0];
        tab.ensure_layout(0)?;
        let glyphs = &tab.layouts[&0].glyphs;
        let start = glyphs
            .iter()
            .position(|g| g.ch == 'S')
            .context("선택 시작 글자")?;
        let first = glyphs[start].rect.center();
        let last = glyphs[start + 8].rect.center();
        tab.change_zoom(zoom);
        tab.dest(Destination {
            page: 0,
            point: Some(first),
        })?;
        tick(&mut app, vec![], Modifiers::NONE, &mut frame);
        let tab = &mut app.tabs[0];
        let pan = tab.pan;
        let row = tab.row_layout(0, tab.view)?;
        let page = row.visible_pages(tab.view, pan, ctx.pixels_per_point())[0].rect;
        let screen = |p: Pos2| page.min + p.to_vec2() * row.scale;
        let (from, to) = if reverse {
            (screen(last), screen(first))
        } else {
            (screen(first), screen(last))
        };
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [
            vec![Event::PointerMoved(from)],
            vec![button(from, true)],
            vec![Event::PointerMoved(to)],
            vec![button(to, false)],
        ] {
            tick(&mut app, events, Modifiers::NONE, &mut frame);
        }
        let output = tick(&mut app, vec![Event::Copy], Modifiers::CTRL, &mut frame);
        ensure!(
            output
                .shapes
                .iter()
                .filter(|s| matches!(&s.shape, Shape::Rect(r) if r.fill == SELECTION_COLOR))
                .count()
                == 1,
            "{zoom}배에서 한 줄 선택은 하나의 형광펜 띠"
        );
        ensure!(
            app.tabs[0].pan == pan && app.tabs[0].page == 0,
            "본문 드래그 중 화면 이동"
        );
        ensure!(
            output.platform_output.commands.iter().any(
                |cmd| matches!(cmd, egui::OutputCommand::CopyText(text) if text == "SimplePDF")
            ),
            "{zoom}배 보기 모드 드래그로 선택한 본문 복사"
        );
    }
    app.tabs[0].execute(Cmd::Fit)?;
    app.tabs[0].go(0)?;
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    let wheel = |y, mods| {
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Line,
                delta: vec2(0., y),
                modifiers: mods,
            },
        ]
    };
    tick(
        &mut app,
        wheel(2., Modifiers::CTRL),
        Modifiers::CTRL,
        &mut frame,
    );
    ensure!(app.tabs[0].zoom > 1.6, "휠 확대");
    tick(
        &mut app,
        wheel(-1., Modifiers::NONE),
        Modifiers::NONE,
        &mut frame,
    );
    ensure!(app.tabs[0].pan.y < 0., "확대 후 스크롤");
    let press = |app: &mut App, key, modifiers, frame: &mut eframe::Frame| {
        let events = [true, false]
            .into_iter()
            .map(|pressed| Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            })
            .collect();
        tick(app, events, modifiers, frame);
    };
    press(&mut app, Key::Num2, Modifiers::COMMAND, &mut frame);
    ensure!(app.tabs[0].spread, "두 페이지 전환");
    press(&mut app, Key::F, Modifiers::COMMAND, &mut frame);
    ensure!(app.tabs[0].find.open, "검색창");
    tick(
        &mut app,
        vec![Event::Text("p".into())],
        Modifiers::NONE,
        &mut frame,
    );
    app.tabs[0].find.deadline = Instant::now();
    let mut output = tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    ensure!(
        !app.tabs[0].find.options.match_case && app.tabs[0].find.found.len() == 2,
        "기본 검색은 p와 P를 모두 찾아야 함"
    );
    for (label, count) in [("대소문자 구분", 1), ("대소문자 구분", 2)] {
        let pos = label_position(&output, label).context("검색 옵션 위치")?;
        tick(
            &mut app,
            vec![Event::PointerMoved(pos)],
            Modifiers::NONE,
            &mut frame,
        );
        for pressed in [true, false] {
            let event = Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            tick(&mut app, vec![event], Modifiers::NONE, &mut frame);
        }
        app.tabs[0].find.deadline = Instant::now();
        output = tick(&mut app, vec![], Modifiers::NONE, &mut frame);
        ensure!(
            app.tabs[0].find.found.len() == count && app.tabs[0].find.active == 0,
            "{label} 변경 후 검색 결과: {} / {count}",
            app.tabs[0].find.found.len()
        );
    }
    for (modifiers, expected) in [
        (Modifiers::NONE, 1),
        (Modifiers::NONE, 0),
        (Modifiers::NONE, 1),
        (Modifiers::SHIFT, 0),
        (Modifiers::SHIFT, 1),
    ] {
        press(&mut app, Key::Enter, modifiers, &mut frame);
        ensure!(app.tabs[0].find.active == expected, "연속 Enter 검색 순서");
        ensure!(
            ctx.memory(|m| m.has_focus(Id::new(("search_entry", app.tabs[0].id)))),
            "Enter로 검색 결과 이동 후 입력 포커스 유지"
        );
    }
    app.tabs[0].find.open = false;
    app.tabs[0].execute(Cmd::Fit)?;
    app.tabs[0].spread = false;
    app.tabs[0].choose_mode(Mode::Edit)?;
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    let page = app.tabs[0].session()?.doc.page(0)?;
    let size = page.size;
    let run = page.layout()?.runs[0].clone();
    drop(page);
    let top_left = app.tabs[0].view.center() - size * app.tabs[0].scale * 0.5;
    let pt = top_left + run.rect.center().to_vec2() * app.tabs[0].scale;
    let click = |app: &mut App, pt: Pos2, frame: &mut eframe::Frame| {
        for down in [true, false] {
            tick(
                app,
                vec![
                    Event::PointerMoved(pt),
                    Event::PointerButton {
                        pos: pt,
                        button: PointerButton::Primary,
                        pressed: down,
                        modifiers: Modifiers::NONE,
                    },
                ],
                Modifiers::NONE,
                frame,
            );
        }
    };
    click(&mut app, pt, &mut frame);
    ensure!(
        app.tabs[0]
            .edit
            .as_ref()
            .is_some_and(|e| e.text == run.text),
        "행 클릭과 인라인 커서"
    );
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    let family = Id::new(("font_family", app.tabs[0].id));
    let chooser = ctx
        .read_response(family)
        .context("글꼴 선택 버튼")?
        .rect
        .center();
    click(&mut app, chooser, &mut frame);
    tick(
        &mut app,
        vec![Event::Text("MALgun".into())],
        Modifiers::NONE,
        &mut frame,
    );
    let matches = fonts::matches(&app.tabs[0].font_query);
    ensure!(
        app.tabs[0].font_picker && app.tabs[0].font_query == "MALgun" && matches.len() > 1,
        "영문 글꼴 검색"
    );
    ensure!(
        fonts::matches("맑은 고딕")
            .iter()
            .any(|f| f.path == matches[0].path),
        "한글 글꼴 이름 검색"
    );
    press(&mut app, Key::ArrowDown, Modifiers::NONE, &mut frame);
    press(&mut app, Key::Enter, Modifiers::NONE, &mut frame);
    ensure!(
        !app.tabs[0].font_picker
            && app.tabs[0]
                .edit
                .as_ref()
                .is_some_and(
                    |e| e.style.font.as_ref() == Some(&matches[1].path) && e.text == run.text
                ),
        "검색한 글꼴 선택과 본문 포커스"
    );
    click(&mut app, chooser, &mut frame);
    press(&mut app, Key::Enter, Modifiers::NONE, &mut frame);
    ensure!(
        app.tabs[0]
            .edit
            .as_ref()
            .is_some_and(|e| e.style.font.is_none()),
        "원본 글꼴 복원"
    );
    tick(
        &mut app,
        vec![Event::Text("A".into())],
        Modifiers::NONE,
        &mut frame,
    );
    let edited = app.tabs[0].edit.as_ref().unwrap().text.clone();
    ensure!(
        edited.contains('A') && app.tabs[0].preview.is_some(),
        "글꼴 미리보기"
    );
    tick(
        &mut app,
        vec![Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
        &mut frame,
    );
    ensure!(
        app.tabs[0].edit.is_none()
            && app.tabs[0].session()?.doc.page(0)?.layout()?.runs[0].text == edited,
        "직접 편집 적용"
    );
    let second = Edit {
        page: 0,
        runs: vec![],
        rect: Rect::from_min_size(pos2(50., 180.), vec2(240., 40.)),
        text: "두 번째 문구".into(),
        style: Style {
            size: 16.,
            rgb: [0; 3],
            font: None,
        },
    };
    app.tabs[0].apply(|d| d.page(0)?.edit(&second))?;
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    let other = app.tabs[0]
        .session()?
        .doc
        .page(0)?
        .layout()?
        .runs
        .into_iter()
        .find(|r| r.text == second.text)
        .context("다른 행")?;
    let other_pt = top_left + other.rect.center().to_vec2() * app.tabs[0].scale;
    click(&mut app, other_pt, &mut frame);
    let active_text = |app: &App| app.tabs[0].edit.as_ref().map(|e| e.text.clone());
    ensure!(
        active_text(&app).as_deref() == Some(second.text.as_str()),
        "두 번째 행 선택"
    );
    press(&mut app, Key::Z, Modifiers::COMMAND, &mut frame);
    ensure!(
        active_text(&app).as_deref() == Some(second.text.as_str()),
        "편집을 시작한 행에 이전 행의 실행 취소 기록이 들어왔습니다."
    );
    tick(
        &mut app,
        vec![Event::Text("B".into())],
        Modifiers::NONE,
        &mut frame,
    );
    let changed = active_text(&app).context("두 번째 문구 입력")?;
    ensure!(changed.contains('B'));
    for _ in 0..3 {
        press(&mut app, Key::Z, Modifiers::COMMAND, &mut frame);
        ensure!(
            active_text(&app).as_deref() == Some(second.text.as_str()),
            "연속 실행 취소 시 현재 행 유지"
        );
    }
    press(&mut app, Key::Y, Modifiers::COMMAND, &mut frame);
    ensure!(
        active_text(&app).as_deref() == Some(changed.as_str()),
        "현재 행 다시 실행"
    );
    press(&mut app, Key::Enter, Modifiers::NONE, &mut frame);
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    press(&mut app, Key::Z, Modifiers::COMMAND, &mut frame);
    let lines = app.tabs[0].session()?.doc.page(0)?.layout()?.runs;
    ensure!(
        lines.iter().any(|r| r.text == edited) && lines.iter().any(|r| r.text == second.text),
        "확정 후 문서 실행 취소 범위"
    );
    let bookmarks = &mut app.tabs[0].doc.as_mut().unwrap().bookmarks;
    for i in 0..80 {
        bookmarks.push(Bookmark {
            title: "Safety System Parameters 긴 책갈피 제목 ".repeat(6),
            depth: i % 12,
            dest: None,
        });
    }
    app.tabs[0].book = false;
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    ensure!(app.tabs[0].view.left().abs() < 1., "책갈피 닫기");
    app.tabs[0].book = true;
    for width in [176., 105., 340., 550.] {
        let state = egui::containers::panel::PanelState {
            rect: Rect::from_min_size(Pos2::ZERO, vec2(width, 640.)),
        };
        ctx.data_mut(|d| d.insert_persisted(Id::new("bookmarks"), state));
        for _ in 0..2 {
            tick(&mut app, vec![], Modifiers::NONE, &mut frame);
            ensure!(
                (app.tabs[0].view.left() - width).abs() <= 1.,
                "책갈피 너비 {width}에서 문서 영역 시작점 {}",
                app.tabs[0].view.left()
            );
        }
    }
    ensure!(
        app.tabs[0].error.is_empty(),
        "화면 오류: {}",
        app.tabs[0].error
    );
    Ok(())
}

pub fn verify_page_scroll() -> Result<()> {
    use eframe::App as _;
    let source = Document::blank()?;
    let doc = Document::blank()?;
    doc.import(&source, &[0; 8])?;
    doc.rotate(1)?;
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = App::new(&cc, vec![]);
    app.tabs[0].install_doc(Session::open(doc.bytes()?, None, String::new())?);
    let mut frame = eframe::Frame::_new_kittest();
    let mut now = 0.0;
    let dpi = std::cell::Cell::new(1.0);
    let mut tick = |app: &mut App, unit: MouseWheelUnit, deltas: &[f32], dt: f32| {
        now += f64::from(dt);
        let mut events = vec![Event::PointerMoved(pos2(650., 335.))];
        events.extend(deltas.iter().copied().map(|y| Event::MouseWheel {
            unit,
            delta: vec2(0., y),
            modifiers: Modifiers::NONE,
        }));
        let mut raw = RawInput {
            time: Some(now),
            predicted_dt: dt,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1080., 720.))),
            events,
            ..Default::default()
        };
        raw.viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(dpi.get());
        ctx.run(raw, |ctx| app.update(ctx, &mut frame))
    };
    let line = MouseWheelUnit::Line;
    tick(&mut app, line, &[], 0.016);
    let original = app.tabs[0].surfaces[&0].id();
    let painted = tick(&mut app, line, &[-1.], 0.004);
    // Inspect the actual draw list on the input frame, before any follow-up repaint.
    let check_painted = |app: &App, output: &FullOutput, expected: &[usize]| -> Result<()> {
        let mut pages = Vec::new();
        for shape in &output.shapes {
            if let Shape::Mesh(mesh) = &shape.shape {
                if let Some((&page, texture)) = app.tabs[0]
                    .surfaces
                    .iter()
                    .find(|(_, s)| s.id() == mesh.texture_id)
                {
                    let rect = mesh.calc_bounds();
                    ensure!(
                        (rect.center().y - app.tabs[0].view.center().y).abs() < 1.,
                        "페이지 세로 중심"
                    );
                    ensure!(shape.clip_rect.contains_rect(rect), "맞춤 페이지 전체 표시");
                    let ppp = ctx.pixels_per_point();
                    let origin = rect.min.to_vec2() * ppp;
                    let [w, h] = texture.size();
                    ensure!(
                        (origin - origin.round()).length() < 0.001
                            && (rect.size() * ppp - vec2(w as f32, h as f32)).length() < 0.001,
                        "실제 픽셀과 페이지 텍스처 일치"
                    );
                    pages.push(page);
                }
            }
        }
        ensure!(
            pages == expected,
            "첫 프레임 표시 페이지 {pages:?}, 예상 {expected:?}"
        );
        Ok(())
    };
    ensure!(app.tabs[0].page == 1, "첫 휠 입력 즉시 이동");
    check_painted(&app, &painted, &[1])?;
    tick(&mut app, line, &[-1., -1.], 0.004);
    ensure!(app.tabs[0].page == 3, "빠르게 이어진 휠 입력 모두 반영");
    tick(&mut app, line, &[-3.], 0.004);
    ensure!(app.tabs[0].page == 6, "여러 칸이 합쳐진 휠 입력");
    let painted = tick(&mut app, line, &[2.], 0.004);
    ensure!(app.tabs[0].page == 4, "빠른 방향 반전");
    check_painted(&app, &painted, &[4])?;
    tick(&mut app, line, &[-30., 1.], 0.004);
    ensure!(app.tabs[0].page == 7, "끝에서 바로 반대 방향으로 이동");
    let at_rest = tick(&mut app, line, &[], 0.016);
    check_painted(&app, &at_rest, &[7])?;
    app.tabs[0].go(0)?;
    tick(&mut app, line, &[], 0.016);
    let revisit = app.tabs[0].surfaces[&0].id();
    tick(&mut app, line, &[-1., 1.], 0.004);
    ensure!(
        app.tabs[0].page == 0 && app.tabs[0].surfaces[&0].id() == revisit && revisit != original,
        "되돌림 시 화면 재사용"
    );

    app.tabs[0].change_zoom(0.75);
    tick(&mut app, line, &[-1.], 0.004);
    ensure!(app.tabs[0].page == 1, "전체 내용이 보이는 숫자 배율");
    app.tabs[0].execute(Cmd::Fit)?;
    app.tabs[0].go(0)?;
    tick(&mut app, line, &[-0.25, -0.25, -0.25, -0.25], 0.004);
    ensure!(
        app.tabs[0].page == 1 && app.tabs[0].wheel_fraction == 0.,
        "고해상도 휠 누적"
    );
    app.tabs[0].execute(Cmd::Spread)?;
    app.tabs[0].go(0)?;
    let painted = tick(&mut app, line, &[-2., -1.], 0.004);
    check_painted(&app, &painted, &[6, 7])?;
    ensure!(app.tabs[0].page == 6, "두 페이지 연속 전환");
    let painted = tick(&mut app, line, &[-5.], 0.004);
    check_painted(&app, &painted, &[8])?;
    ensure!(app.tabs[0].page == 8, "홀수 마지막 페이지");
    tick(&mut app, line, &[1.], 0.004);
    ensure!(app.tabs[0].page == 6, "두 페이지 역방향");
    app.tabs[0].execute(Cmd::Spread)?;
    app.tabs[0].go(0)?;
    let quarter = app.tabs[0].view.height() / 4.;
    let painted = tick(&mut app, MouseWheelUnit::Point, &[-quarter], 0.004);
    check_painted(&app, &painted, &[0])?;
    ensure!(
        (app.tabs[0].wheel_fraction - 0.25).abs() < 0.001,
        "터치패드 픽셀 이동량"
    );
    app.tabs[0].go(1)?;
    app.tabs[0].execute(Cmd::Width)?;
    tick(&mut app, line, &[-1.], 0.004);
    ensure!(app.tabs[0].page == 2, "가로 페이지 너비 맞춤에서 이동");
    tick(&mut app, line, &[-1.], 0.004);
    let viewport = app.tabs[0].view;
    ensure!(
        app.tabs[0].page == 2
            && app.tabs[0].pan.y
                < app.tabs[0]
                    .row_layout(2, viewport)?
                    .limit(app.tabs[0].view)
                    .y,
        "세로 페이지 진입 후 내부 스크롤"
    );
    app.tabs[0].execute(Cmd::Fit)?;
    app.tabs[0].go(0)?;
    for density in [1., 1.25, 1.5, 2.] {
        dpi.set(density);
        // Include a 1% resize to catch reusing an almost-matching bitmap after zoom settles.
        for zoom in [1., 0.75, 0.76] {
            app.tabs[0].change_zoom(zoom);
            app.tabs[0].quality = Instant::now();
            let painted = tick(&mut app, line, &[], 0.016);
            ensure!(
                (ctx.pixels_per_point() - density).abs() < 0.001,
                "화면 배율 적용"
            );
            check_painted(&app, &painted, &[0])?;
        }
    }
    ensure!(
        app.tabs[0].error.is_empty(),
        "휠 검증 중 화면 오류: {}",
        app.tabs[0].error
    );
    Ok(())
}
