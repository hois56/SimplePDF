struct TabLabel {
    title: String,
    detail: String,
}

fn drag_window(ctx: &egui::Context, response: &Response) {
    if response.double_clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(
            !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
        ));
    } else if response.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

fn tab_strip(ui: &mut Ui, area: Rect, labels: &[TabLabel], active: usize) -> Option<Cmd> {
    let mut action = None;
    let overflow = area.width() < labels.len() as f32 * 96.;
    let list_width = if overflow { 28. } else { 0. };
    let available = (area.width() - list_width - 6.).max(1.);
    let count = ((available / 96.).floor() as usize).clamp(1, labels.len());
    let count = if overflow { count } else { labels.len() };
    let width = (available / count as f32).min(184.);
    let first = active.saturating_sub(count / 2).min(labels.len() - count);
    let used = width * count as f32 + list_width;
    let start = (area.center().x - used * 0.5).max(area.left());
    let y = area.center().y;
    let background = ui.interact(area, Id::new("tab_drag_strip"), Sense::click_and_drag());
    drag_window(ui.ctx(), &background);
    for (n, label) in labels.iter().enumerate().skip(first).take(count) {
        let x = start + (n - first) as f32 * width;
        let rect = Rect::from_min_size(pos2(x + 1., y - 14.), vec2((width - 2.).max(1.), 28.));
        let close = Rect::from_min_max(
            pos2((rect.right() - 21.).max(rect.left()), rect.top()),
            rect.max,
        );
        let body = Rect::from_min_max(rect.min, pos2(close.left(), rect.bottom()));
        let tab = ui
            .interact(body, Id::new(("document_tab", n)), Sense::click_and_drag())
            .on_hover_text(&label.detail);
        let cross = ui
            .interact(close, Id::new(("close_document", n)), Sense::click())
            .on_hover_text("탭 닫기");
        if tab.clicked() {
            action = Some(Cmd::SelectTab(n));
        }
        if tab.middle_clicked() || cross.clicked() {
            action = Some(Cmd::CloseTab(n));
        }
        drag_window(ui.ctx(), &tab);
        let paint = ui.painter().with_clip_rect(rect.intersect(area));
        if n == active || tab.hovered() || cross.hovered() {
            paint.rect_filled(
                rect,
                5.,
                if n == active {
                    Color32::from_rgb(228, 236, 247)
                } else {
                    Color32::from_gray(242)
                },
            );
        }
        if n == active {
            paint.line_segment(
                [
                    pos2(rect.left() + 5., rect.bottom() - 1.),
                    pos2(rect.right() - 5., rect.bottom() - 1.),
                ],
                Stroke::new(2_f32, Color32::from_rgb(55, 98, 153)),
            );
        }
        let galley = WidgetText::from(label.title.as_str()).into_galley(
            ui,
            Some(TextWrapMode::Truncate),
            (body.width() - 8.).max(1.),
            TextStyle::Button,
        );
        paint.with_clip_rect(body.intersect(area)).galley(
            pos2(
                body.center().x - galley.size().x * 0.5,
                y - galley.size().y * 0.5,
            ),
            galley,
            Color32::BLACK,
        );
        if cross.hovered() {
            paint.rect_filled(
                Rect::from_center_size(close.center(), vec2(18., 18.)),
                4.,
                Color32::from_rgb(211, 220, 232),
            );
        }
        let c = close.center();
        let stroke = Stroke::new(1.4_f32, Color32::from_rgb(63, 73, 88));
        paint.line_segment([c - vec2(3., 3.), c + vec2(3., 3.)], stroke);
        paint.line_segment([c + vec2(3., -3.), c + vec2(-3., 3.)], stroke);
    }
    if overflow {
        let rect = Rect::from_min_size(
            pos2(start + width * count as f32, y - 14.),
            vec2(list_width, 28.),
        );
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(Align::Center)),
            |ui| {
                ui.menu_button("···", |ui| {
                    ScrollArea::vertical().max_height(360.).show(ui, |ui| {
                        for (i, label) in labels.iter().enumerate() {
                            if ui
                                .selectable_label(i == active, &label.title)
                                .on_hover_text(&label.detail)
                                .clicked()
                            {
                                action = Some(Cmd::SelectTab(i));
                                ui.close_menu();
                            }
                        }
                    });
                })
                .response
                .on_hover_text("열린 문서 목록");
            },
        );
    }
    action
}

pub struct App {
    tabs: Vec<DocumentTab>,
    active: usize,
    next_id: u64,
    pending: VecDeque<PathBuf>,
    full: bool,
    closing: bool,
    pub(crate) inbox: Option<crate::instance::Inbox>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_visuals(Visuals::light());
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut defs = FontDefinitions::default();
        if let Ok(data) = std::fs::read(default_font()) {
            defs.font_data.insert(
                "Malgun".into(),
                std::sync::Arc::new(FontData::from_owned(data)),
            );
            defs.families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "Malgun".into());
        }
        ctx.set_fonts(defs.clone());
        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = Color32::from_rgb(249, 249, 250);
        style.spacing.item_spacing = vec2(3., 3.);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(238, 240, 243);
        ctx.set_style(style);
        Self {
            tabs: vec![DocumentTab::new(0, defs)],
            active: 0,
            next_id: 1,
            pending: paths.into(),
            full: false,
            closing: false,
            inbox: None,
        }
    }

    fn select(&mut self, index: usize, ctx: &egui::Context) {
        if index >= self.tabs.len() || index == self.active {
            return;
        }
        // Hidden tabs retain document/edit/search state but release their GPU pages.
        let old = &mut self.tabs[self.active];
        old.surfaces.clear();
        old.drag = None;
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        self.active = index;
        let tab = &mut self.tabs[index];
        tab.refresh_editor_font = tab.edit.is_some();
        tab.edit_focus = tab.edit.is_some();
        ctx.request_repaint();
    }

    fn add(&mut self, doc: Session, ctx: &egui::Context) {
        if self.tabs[self.active].doc.is_some() {
            let current = &self.tabs[self.active];
            let mut tab = DocumentTab::new(self.next_id, current.base_fonts.clone());
            self.next_id += 1;
            tab.book = current.book;
            self.tabs.push(tab);
            self.select(self.tabs.len() - 1, ctx);
        }
        self.tabs[self.active].install_doc(doc);
    }

    fn open_path(&mut self, path: PathBuf, password: &str, ctx: &egui::Context) -> Result<()> {
        let path = std::path::absolute(path)?;
        let canonical = path.canonicalize()?;
        if let Some(i) = self.tabs.iter().position(|t| {
            t.doc
                .as_ref()
                .and_then(|s| s.path.as_ref())
                .is_some_and(|p| p == &path || p.canonicalize().ok().as_ref() == Some(&canonical))
        }) {
            self.select(i, ctx);
            return Ok(());
        }
        match Session::open(std::fs::read(&path)?, Some(path.clone()), password.into()) {
            Ok(doc) => self.add(doc, ctx),
            Err(e) if e.to_string().contains("암호") => {
                let tab = &mut self.tabs[self.active];
                tab.ask(Prompt::Password(path), "");
                if !password.is_empty() {
                    tab.error = e.to_string();
                }
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }

    fn close_tab(&mut self, index: usize, ctx: &egui::Context) -> Result<()> {
        if index >= self.tabs.len() {
            return Ok(());
        }
        ensure!(
            self.tabs[index].job.is_none(),
            "변환을 마친 뒤 닫아 주세요."
        );
        if !self.tabs[index].leave()? {
            return Ok(());
        }
        if self.tabs.len() == 1 {
            let tab = &mut self.tabs[0];
            let mut empty = DocumentTab::new(self.next_id, tab.base_fonts.clone());
            self.next_id += 1;
            empty.book = tab.book;
            *tab = empty;
        } else {
            if self.active == index {
                self.select(
                    if index + 1 < self.tabs.len() {
                        index + 1
                    } else {
                        index - 1
                    },
                    ctx,
                );
            }
            self.tabs.remove(index);
            if self.active > index {
                self.active -= 1;
            }
        }
        ctx.request_repaint();
        Ok(())
    }

    fn exit_window(&mut self, ctx: &egui::Context) -> Result<bool> {
        ensure!(
            self.tabs.iter().all(|t| t.job.is_none()),
            "변환을 마친 뒤 닫아 주세요."
        );
        let start = self.active;
        for offset in 0..self.tabs.len() {
            let i = (start + offset) % self.tabs.len();
            if self.tabs[i].edit.is_some() || self.tabs[i].doc.as_ref().is_some_and(Session::dirty)
            {
                self.select(i, ctx);
                if !self.tabs[i].leave()? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn dispatch(&mut self, cmd: Cmd, ctx: &egui::Context) -> Result<()> {
        match cmd {
            Cmd::Open => {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .pick_files()
                {
                    self.pending.extend(files);
                }
            }
            Cmd::New => self.add(
                Session::open(Document::blank()?.bytes()?, None, String::new())?,
                ctx,
            ),
            Cmd::Unlock(path, pass) => self.open_path(path, &pass, ctx)?,
            Cmd::SelectTab(i) => self.select(i, ctx),
            Cmd::CloseTab(i) => self.close_tab(i, ctx)?,
            Cmd::CloseCurrent => self.close_tab(self.active, ctx)?,
            other => self.tabs[self.active].execute(other)?,
        }
        Ok(())
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pending.extend(ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.as_ref())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
                .cloned()
                .collect::<Vec<_>>()
        }));
        for tab in &mut self.tabs {
            tab.poll_job(ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.closing {
            match self.exit_window(ctx) {
                Ok(true) => {
                    self.closing = true;
                    return;
                }
                Ok(false) => ctx.send_viewport_cmd(ViewportCommand::CancelClose),
                Err(e) => {
                    self.tabs[self.active].error = e.to_string();
                    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                }
            }
        }
        if let Some(inbox) = &self.inbox {
            match inbox.receive(&mut self.pending) {
                Ok(true) => {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                Err(e) => self.tabs[self.active].error = e.to_string(),
                Ok(false) => {}
            }
        }
        if self.tabs[self.active].prompt.is_none() && self.tabs[self.active].error.is_empty() {
            if let Some(path) = self.pending.pop_front() {
                if let Err(e) = self.open_path(path, "", ctx) {
                    self.tabs[self.active].error = e.to_string();
                }
                ctx.request_repaint();
            }
            if self.tabs[self.active].prompt.is_none() && self.tabs[self.active].error.is_empty() {
                let cmd = ctx.input_mut(|i| {
                    if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Tab) {
                        Some(Cmd::SelectTab(
                            (self.active + self.tabs.len() - 1) % self.tabs.len(),
                        ))
                    } else if i.consume_key(Modifiers::COMMAND, Key::Tab) {
                        Some(Cmd::SelectTab((self.active + 1) % self.tabs.len()))
                    } else if i.consume_key(Modifiers::COMMAND, Key::W) {
                        Some(Cmd::CloseCurrent)
                    } else {
                        None
                    }
                });
                if let Some(cmd) = cmd {
                    if let Err(e) = self.dispatch(cmd, ctx) {
                        self.tabs[self.active].error = e.to_string();
                    }
                }
            }
        }
        let titles = if self.tabs.len() > 1 {
            self.tabs
                .iter()
                .map(|t| {
                    let title = t.title();
                    let detail = t
                        .doc
                        .as_ref()
                        .and_then(|s| s.path.as_ref())
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| title.clone());
                    TabLabel { title, detail }
                })
                .collect()
        } else {
            vec![]
        };
        let tab = &mut self.tabs[self.active];
        tab.draw(ctx, &titles, self.active, &mut self.full);
        let command = tab.cmd.take();
        if tab.prompt.is_none() && tab.error.is_empty() {
            if let Some(cmd) = command {
                if let Err(e) = self.dispatch(cmd, ctx) {
                    self.tabs[self.active].error = e.to_string();
                }
                ctx.request_repaint();
            }
        }
        ctx.send_viewport_cmd(ViewportCommand::Title(self.tabs[self.active].title()));
    }
}

pub fn verify_tabs_and_outline(first: &Path, other: &Path) -> Result<()> {
    use eframe::App as _;
    let list: Vec<_> = [
        (None, 0),
        (Some(2), 1),
        (Some(2), 2),
        (Some(7), 1),
        (Some(15), 0),
        (Some(12), 0),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (page, depth))| Bookmark {
        title: i.to_string(),
        depth,
        dest: page.map(|page| Destination { page, point: None }),
    })
    .collect();
    let open = HashSet::new();
    for (page, expected) in [
        (0, None),
        (2, Some(2)),
        (4, Some(2)),
        (7, Some(3)),
        (11, Some(3)),
        (14, Some(5)),
        (20, Some(4)),
    ] {
        ensure!(
            current_bookmark(&list, page, None, &open) == expected,
            "절 사이의 강조 {page}"
        );
    }
    ensure!(
        current_bookmark(&list, 4, Some(1), &open) == Some(1),
        "같은 페이지에서 선택한 책갈피"
    );
    ensure!(
        current_bookmark(&list, 5, None, &HashSet::from([0, 1])) == Some(0),
        "접힌 상위 항목 강조"
    );

    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = App::new(&cc, vec![]);
    let mut frame = eframe::Frame::_new_kittest();
    let mut tick = |app: &mut App, events: Vec<Event>, dropped: Vec<PathBuf>, mods: Modifiers| {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(920., 660.))),
            modifiers: mods,
            events,
            dropped_files: dropped
                .into_iter()
                .map(|path| DroppedFile {
                    path: Some(path),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let _ = ctx.run(raw, |ctx| app.update(ctx, &mut frame));
    };
    tick(
        &mut app,
        vec![],
        vec![first.into(), other.into()],
        Modifiers::NONE,
    );
    tick(&mut app, vec![], vec![], Modifiers::NONE);
    tick(&mut app, vec![], vec![], Modifiers::NONE);
    ensure!(
        app.tabs.len() == 2 && app.active == 1 && app.tabs[1].view.top() == 40.,
        "복수 드롭 및 한 줄 헤더"
    );
    let click = |pt: Pos2, down: bool| {
        vec![
            Event::PointerMoved(pt),
            Event::PointerButton {
                pos: pt,
                button: PointerButton::Primary,
                pressed: down,
                modifiers: Modifiers::NONE,
            },
        ]
    };
    let tab = ctx
        .read_response(Id::new(("document_tab", 0usize)))
        .context("첫 번째 탭")?
        .rect
        .center();
    for down in [true, false] {
        tick(&mut app, click(tab, down), vec![], Modifiers::NONE);
    }
    ensure!(app.active == 0, "탭 클릭");
    let a = &mut app.tabs[0];
    a.change_zoom(1.5);
    a.find.query = "문서별 검색".into();
    a.collapsed.insert(2);
    a.begin_edit(
        0,
        pos2(50., 50.),
        Some(Rect::from_min_size(pos2(50., 50.), vec2(150., 40.))),
    )?;
    a.edit.as_mut().unwrap().text = "미완료 편집".into();
    a.update_preview();
    let key = |k, mods| {
        vec![Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: mods,
        }]
    };
    tick(
        &mut app,
        key(Key::Tab, Modifiers::COMMAND),
        vec![],
        Modifiers::COMMAND,
    );
    ensure!(
        app.active == 1
            && app.tabs[1].zoom == 1.
            && app.tabs[1].edit.is_none()
            && app.tabs[1].find.query.is_empty(),
        "탭 격리"
    );
    app.tabs[1].go(1)?;
    app.open_path(first.into(), "", &ctx)?;
    ensure!(
        app.tabs.len() == 2
            && app.active == 0
            && app.tabs[0].zoom == 1.5
            && app.tabs[0].collapsed.contains(&2)
            && app.tabs[0].find.query == "문서별 검색"
            && app.tabs[0]
                .edit
                .as_ref()
                .is_some_and(|e| e.text == "미완료 편집")
            && app.tabs[0].preview.is_some(),
        "다시 연 탭의 편집과 보기 복원"
    );
    app.tabs[0].commit()?;
    ensure!(
        app.tabs[0].session()?.dirty() && !app.tabs[1].session()?.dirty(),
        "저장 상태 격리"
    );
    app.select(1, &ctx);
    ensure!(app.tabs[1].page == 1, "탭별 현재 페이지");
    tick(&mut app, vec![], vec![], Modifiers::NONE);
    let close = ctx
        .read_response(Id::new(("close_document", 1usize)))
        .context("탭 닫기")?
        .rect
        .center();
    for down in [true, false] {
        tick(&mut app, click(close, down), vec![], Modifiers::NONE);
    }
    ensure!(
        app.tabs.len() == 1 && app.active == 0 && app.tabs[0].session()?.dirty(),
        "다른 탭을 닫은 뒤 편집 보존"
    );
    app.tabs[0].execute(Cmd::Undo)?;
    ensure!(!app.tabs[0].session()?.dirty(), "탭 복귀 후 실행 취소");
    tick(
        &mut app,
        key(Key::W, Modifiers::COMMAND),
        vec![],
        Modifiers::COMMAND,
    );
    ensure!(
        app.tabs.len() == 1 && app.tabs[0].doc.is_none(),
        "마지막 탭 닫기"
    );
    Ok(())
}
