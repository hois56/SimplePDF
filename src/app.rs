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
    Image,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::View => "보기",
            Self::Edit => "문구 수정",
            Self::Text => "텍스트 추가",
            Self::Image => "이미지 추가",
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
    chosen_face: String,
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
            chosen_face: String::new(),
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
        self.toc.clear();
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
                    .add_filter(
                        "Office",
                        &[
                            "docx", "doc", "odt", "rtf", "xls", "xlsx", "xlsm", "csv", "ppt",
                            "pptx",
                        ],
                    )
                    .pick_file()
                {
                    if let Some(out) = rfd::FileDialog::new()
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
                ("Office → PDF", Cmd::Office),
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
                                for m in [Mode::View, Mode::Edit, Mode::Text, Mode::Image] {
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
    fn properties(&mut self, ctx: &egui::Context) {
        SidePanel::right("properties")
            .default_width(196.)
            .width_range(156. ..=300.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("텍스트");
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if icon(ui, "close", "속성 닫기", false).clicked() {
                            self.props = false;
                        }
                    });
                });
                ui.separator();
                if let Some(e) = &mut self.edit {
                    let mut changed = false;
                    ui.label("글꼴");
                    let family = e
                        .runs
                        .first()
                        .map(|r| r.family.as_str())
                        .unwrap_or("맑은 고딕");
                    let selected = if self.chosen_face.is_empty() {
                        family
                    } else {
                        &self.chosen_face
                    };
                    ComboBox::from_id_salt("font")
                        .selected_text(selected)
                        .width(ui.available_width() - 5.)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(e.style.font.is_none(), "원본 글꼴")
                                .clicked()
                            {
                                e.style.font = None;
                                self.chosen_face.clear();
                                changed = true
                            }
                            for f in fonts::all().iter().filter(|f| {
                                f.index == 0
                                    && f.path.extension().is_some_and(|e| {
                                        e.eq_ignore_ascii_case("ttf")
                                            || e.eq_ignore_ascii_case("otf")
                                    })
                            }) {
                                let n = format!("{} {}", f.family, f.style);
                                if ui.selectable_label(self.chosen_face == n, &n).clicked() {
                                    e.style.font = Some(f.path.clone());
                                    self.chosen_face = n;
                                    changed = true
                                }
                            }
                        });
                    ui.label("크기");
                    changed |= ui
                        .add(
                            DragValue::new(&mut e.style.size)
                                .speed(0.25)
                                .range(1. ..=400.)
                                .suffix(" pt"),
                        )
                        .changed();
                    ui.label("색상");
                    changed |= ui.color_edit_button_srgb(&mut e.style.rgb).changed();
                    if changed {
                        self.update_preview();
                    }
                    if !self.edit_error.is_empty() {
                        ui.colored_label(Color32::DARK_RED, &self.edit_error);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("적용").clicked() {
                            if let Err(e) = self.commit() {
                                self.error = e.to_string()
                            }
                        }
                        if ui.button("취소").clicked() {
                            self.cancel_edit()
                        }
                    });
                }
            });
    }
}
include!("view.rs");
include!("tabs.rs");
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
        let _ = ctx.run(raw, |ctx| a.update(ctx, frame));
    };
    tick(&mut app, vec![], Modifiers::NONE, &mut frame);
    let center = app.tabs[0].view.center();
    ensure!(
        app.tabs[0].book
            && app.tabs[0].zoom == 1.
            && app.tabs[0].content.y <= app.tabs[0].view.height() + 1.,
        "기본 화면 맞춤"
    );
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
    let key = |k| {
        vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]
    };
    tick(&mut app, key(Key::Num2), Modifiers::COMMAND, &mut frame);
    ensure!(app.tabs[0].spread, "두 페이지 전환");
    tick(&mut app, key(Key::F), Modifiers::COMMAND, &mut frame);
    ensure!(app.tabs[0].find.open, "검색창");
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
    for down in [true, false] {
        tick(
            &mut app,
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
            &mut frame,
        );
    }
    ensure!(
        app.tabs[0]
            .edit
            .as_ref()
            .is_some_and(|e| e.text == run.text),
        "행 클릭과 인라인 커서"
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
