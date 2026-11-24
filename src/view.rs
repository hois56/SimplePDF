struct RowLayout {
    first: usize,
    sizes: Vec<Vec2>,
    scale: f32,
    zoom: f32,
    content: Vec2,
}
#[derive(Clone, Copy)]
struct VisiblePage {
    number: usize,
    rect: Rect,
}
impl RowLayout {
    fn limit(&self, viewport: Rect) -> Vec2 {
        ((self.content - viewport.size()) * 0.5).max(Vec2::ZERO)
    }
    fn visible_pages(&self, viewport: Rect, pan: Vec2, pixels_per_point: f32) -> Vec<VisiblePage> {
        let mut pages = Vec::with_capacity(self.sizes.len());
        let mut x = viewport.center().x + pan.x - self.content.x * 0.5;
        for (i, size) in self.sizes.iter().enumerate() {
            let size = *size * self.scale;
            let origin = pos2(x, viewport.center().y + pan.y - size.y * 0.5);
            let rect = Rect::from_min_size(
                origin.round_to_pixels(pixels_per_point),
                size.round_to_pixels(pixels_per_point),
            );
            if viewport.intersect(rect).is_positive() {
                pages.push(VisiblePage {
                    number: self.first + i,
                    rect,
                });
            }
            x += size.x + 10.;
        }
        pages
    }
}
impl DocumentTab {
    fn search_step(&mut self, delta: isize) -> Result<()> {
        if self.find.found.is_empty() {
            return Ok(());
        }
        self.find.active =
            (self.find.active as isize + delta).rem_euclid(self.find.found.len() as isize) as usize;
        let (p, boxes) = &self.find.found[self.find.active];
        self.dest(Destination {
            page: *p,
            point: boxes.first().map(Rect::center),
        })
    }
    fn search_ui(&mut self, ctx: &egui::Context) -> Result<()> {
        let width = (self.view.width() - 33.).clamp(146., 344.);
        Area::new(Id::new("find_overlay"))
            .order(Order::Foreground)
            .fixed_pos(pos2(self.view.right() - width - 19., self.view.top() + 7.))
            .show(ctx, |ui| {
                Frame::popup(ui.style()).inner_margin(5).show(ui, |ui| {
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        let res = ui.add(
                            TextEdit::singleline(&mut self.find.query)
                                .return_key(None)
                                .id(Id::new(("search_entry", self.id)))
                                .hint_text("문서 검색")
                                .desired_width((width - 154.).max(32.)),
                        );
                        if self.find.focus {
                            res.request_focus();
                            self.find.focus = false
                        }
                        changed = res.changed();
                        if width > 240. {
                            ui.label(format!(
                                "{} / {}",
                                if self.find.found.is_empty() {
                                    0
                                } else {
                                    self.find.active + 1
                                },
                                self.find.found.len()
                            ));
                        }
                        if icon(ui, "prev", "이전 결과", false).clicked() {
                            if let Err(e) = self.search_step(-1) {
                                self.error = e.to_string()
                            }
                        }
                        if icon(ui, "next", "다음 결과", false).clicked() {
                            if let Err(e) = self.search_step(1) {
                                self.error = e.to_string()
                            }
                        }
                        if icon(ui, "close", "검색 닫기", false).clicked() {
                            self.find.open = false
                        }
                    });
                    if ui
                        .checkbox(&mut self.find.options.match_case, "대소문자 구분")
                        .changed()
                    {
                        changed = true;
                        ui.memory_mut(|m| m.request_focus(Id::new(("search_entry", self.id))));
                    }
                    if changed {
                        self.find.scan = 0;
                        self.find.found.clear();
                        self.find.active = 0;
                        self.find.deadline = Instant::now() + Duration::from_millis(160);
                    }
                });
            });
        if !self.find.query.trim().is_empty() && self.find.scan < self.count() {
            if Instant::now() < self.find.deadline {
                ctx.request_repaint_after(
                    self.find.deadline.saturating_duration_since(Instant::now()),
                );
                return Ok(());
            }
            let start = Instant::now();
            let initial = self.find.found.is_empty();
            while self.find.scan < self.count() {
                let idx = self.find.scan;
                let layout = self.session()?.doc.page(idx)?.layout()?;
                for boxes in session::hits(&layout.glyphs, &self.find.query, self.find.options) {
                    self.find.found.push((idx, boxes))
                }
                self.find.scan += 1;
                if start.elapsed() > Duration::from_millis(8) {
                    break;
                }
            }
            if initial && !self.find.found.is_empty() {
                self.search_step(0)?
            }
            if self.find.scan < self.count() {
                ctx.request_repaint()
            }
        }
        Ok(())
    }
    fn shortcuts(&mut self, ctx: &egui::Context, full: &mut bool) -> Result<()> {
        let key = |k| ctx.input(|i| i.key_pressed(k));
        let m = ctx.input(|i| i.modifiers);
        if self.props
            && self.font_picker
            && self.edit.is_some()
            && self.prompt.is_none()
            && self.error.is_empty()
            && (key(Key::Escape) || key(Key::Enter))
        {
            return Ok(());
        }
        if key(Key::Escape) {
            if self.prompt.is_some() {
                self.prompt = None
            } else if self.edit.is_some() {
                self.cancel_edit()
            } else if self.find.open {
                self.find.open = false
            } else if *full {
                *full = false;
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false))
            }
            return Ok(());
        }
        if self.prompt.is_some() || !self.error.is_empty() {
            return Ok(());
        }
        if key(Key::F11) {
            *full = !*full;
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(*full))
        }
        if m.command && key(Key::F) {
            self.execute(Cmd::Search)?;
            return Ok(());
        }
        if self.find.open
            && (key(Key::F3)
                || (key(Key::Enter)
                    && ctx.memory(|m| m.has_focus(Id::new(("search_entry", self.id))))))
        {
            self.search_step(if m.shift { -1 } else { 1 })?;
            return Ok(());
        }
        if self.edit.is_some()
            && key(Key::Enter)
            && (m.command
                || (self.mode == Mode::Edit
                    && ctx.memory(|m| m.has_focus(Id::new(("inline_text", self.id))))))
        {
            self.commit()?;
            return Ok(());
        }
        if m.command {
            if key(Key::O) {
                self.cmd = Some(Cmd::Open)
            } else if key(Key::S) {
                self.cmd = Some(Cmd::Save)
            } else if key(Key::B) {
                self.book = !self.book
            } else if key(Key::D) {
                self.cmd = Some(Cmd::Bookmark(None))
            } else if key(Key::P) && m.shift {
                self.props = !self.props
            } else if key(Key::L) {
                ctx.memory_mut(|m| m.request_focus(Id::new(("page_entry", self.id))))
            } else if key(Key::Num2) {
                self.cmd = Some(Cmd::Spread)
            } else if key(Key::Num9) || key(Key::Num1) {
                self.cmd = Some(Cmd::Fit)
            } else if key(Key::Num0) {
                self.cmd = Some(Cmd::Width)
            } else if key(Key::PageUp) {
                self.step(-1)?
            } else if key(Key::PageDown) {
                self.step(1)?
            } else if self.edit.is_none() && !ctx.wants_keyboard_input() {
                if key(Key::Z) {
                    self.cmd = Some(Cmd::Undo)
                } else if key(Key::Y) {
                    self.cmd = Some(Cmd::Redo)
                }
            }
        } else if !ctx.wants_keyboard_input() && self.doc.is_some() {
            for (k, mode) in [
                (Key::V, Mode::View),
                (Key::E, Mode::Edit),
                (Key::T, Mode::Text),
                (
                    Key::I,
                    if m.shift {
                        Mode::AddImage
                    } else {
                        Mode::EditImage
                    },
                ),
            ] {
                if key(k) {
                    self.choose_mode(mode)?
                }
            }
        }
        Ok(())
    }
    fn ensure_layout(&mut self, p: usize) -> Result<()> {
        if !self.layouts.contains_key(&p) {
            let s = self.session()?;
            let page = s.doc.page(p)?;
            let layout = page.layout()?;
            let toc = session::inferred(&layout, page.size.y, &s.doc, &s.bookmarks);
            drop(page);
            if self.layouts.len() > 24 {
                self.layouts.clear();
                self.toc.clear()
            }
            self.toc.insert(p, toc);
            self.layouts.insert(p, layout);
        }
        Ok(())
    }
    fn begin_edit(&mut self, p: usize, point: Pos2, rect: Option<Rect>) -> Result<()> {
        self.commit()?;
        self.ensure_layout(p)?;
        let mut runs = if rect.is_some() {
            vec![]
        } else {
            session::edit_line(&self.layouts[&p], point)
        };
        let mut expanded = None;
        if rect.is_none()
            && runs.is_empty()
            && self.layouts[&p]
                .graphics
                .iter()
                .any(|(_, r, t)| *t == 5 && r.contains(point))
            && self.layouts[&p]
                .glyphs
                .iter()
                .any(|g| g.rect.expand(2.).contains(point))
        {
            let s = self.session()?;
            let bytes = crate::forms::page_copy(s.bytes(), &s.password, p)?;
            let doc = Document::open(bytes.clone(), &s.password)?;
            // A page with unsupported transparency or clipping must keep its original appearance.
            let original = s.doc.page(p)?.render(1.)?;
            let normalized = doc.page(p)?.render(1.)?;
            ensure!(
                original == normalized,
                "이 양식의 투명도 또는 자르기 영역을 유지하면서 문구를 변경할 수 없습니다."
            );
            runs = session::edit_line(&doc.page(p)?.layout()?, point);
            self.edit_base = Some(bytes);
            expanded = Some(doc);
        }
        if rect.is_none() && runs.is_empty() {
            self.edit_base = None;
            return Ok(());
        }
        let rect = rect.unwrap_or_else(|| runs.iter().fold(Rect::NOTHING, |b, r| b.union(r.rect)));
        let style = Style {
            size: runs.first().map(|r| r.size).unwrap_or(12.),
            rgb: runs.first().map(|r| r.rgb).unwrap_or([0, 0, 0]),
            font: None,
        };
        self.editor_font = None;
        self.refresh_editor_font = true;
        self.cursor_at = Some(0);
        if let Some(first) = runs.first() {
            let doc = expanded.as_ref().unwrap_or(&self.session()?.doc);
            let page = doc.page(p)?;
            let bytes = page.font_bytes(first.index);
            drop(page);
            let data = bytes
                .filter(|v| {
                    ttf_parser::Face::parse(v, 0)
                        .ok()
                        .is_some_and(|f| f.tables().cmap.is_some())
                })
                .map(FontData::from_owned)
                .or_else(|| {
                    let f = fonts::find(&first.family, first.bold, first.italic)?;
                    let mut data = FontData::from_owned(std::fs::read(&f.path).ok()?);
                    data.index = f.index;
                    Some(data)
                });
            self.editor_font = data.map(std::sync::Arc::new);
            let mut at = 0;
            for r in &runs {
                let n = r.text.chars().count();
                if point.x <= r.rect.right() {
                    at += ((point.x - r.rect.left()) / r.rect.width().max(0.01) * n as f32)
                        .round()
                        .clamp(0., n as f32) as usize;
                    break;
                }
                at += n;
            }
            self.cursor_at = Some(at);
        }
        self.original_editor_font = self.editor_font.clone();
        self.font_picker = false;
        self.chosen_face.clear();
        self.page = p;
        self.page_text = (p + 1).to_string();
        self.edit = Some(Edit {
            page: p,
            text: runs.iter().map(|r| r.text.as_str()).collect(),
            runs,
            rect,
            style,
        });
        self.edit_focus = true;
        self.props = true;
        Ok(())
    }
    fn inline(&mut self, ui: &mut Ui, p: usize, page_rect: Rect, scale: f32) -> Result<()> {
        let Some(e) = self.edit.as_mut().filter(|e| e.page == p) else {
            return Ok(());
        };
        let mut r = Rect::from_min_max(
            page_rect.min + e.rect.min.to_vec2() * scale,
            page_rect.min + e.rect.max.to_vec2() * scale,
        );
        r.max.x = (r.max.x + 55.).min(page_rect.right()).max(r.min.x + 6.);
        r.max.y = r.max.y.max(r.min.y + e.style.size * scale * 1.4);
        let needs_focus = self.edit_focus;
        let family = FontFamily::Name("PDFInline".into());
        if std::mem::take(&mut self.refresh_editor_font) {
            if let Some(font) = &self.editor_font {
                let mut defs = self.base_fonts.clone();
                defs.font_data.insert("PDFInline".into(), font.clone());
                let mut names = vec!["PDFInline".into()];
                names.extend(defs.families[&FontFamily::Proportional].clone());
                defs.families.insert(family.clone(), names);
                ui.ctx().set_fonts(defs);
            }
        }
        let family =
            if self.editor_font.is_some() && ui.ctx().fonts(|f| f.families().contains(&family)) {
                family
            } else {
                FontFamily::Proportional
            };
        let transform = e
            .runs
            .first()
            .map(|r| r.matrix.a.hypot(r.matrix.b))
            .unwrap_or(1.);
        let font = FontId::new((e.style.size * transform * scale).clamp(3., 220.), family);
        let id = Id::new(("inline_text", self.id));
        if self.cursor_at.is_some() {
            // Each opened text range starts its own undo history, selection and IME state.
            egui::text_edit::TextEditState::default().store(ui.ctx(), id);
        }
        let mut field = if e.runs.is_empty() {
            TextEdit::multiline(&mut e.text)
        } else {
            TextEdit::singleline(&mut e.text)
        };
        field = field
            .id(id)
            .margin(Margin::ZERO)
            .frame(false)
            .font(font)
            .text_color(Color32::TRANSPARENT)
            .desired_width(r.width());
        let response = ui.put(r, field);
        if needs_focus {
            response.request_focus();
            self.edit_focus = false;
            if let Some(i) = self.cursor_at.take() {
                if let Some(mut state) = TextEdit::load_state(ui.ctx(), response.id) {
                    let c = egui::text::CCursor::new(i);
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(c)));
                    state.store(ui.ctx(), response.id);
                }
            }
        }
        ui.painter().rect_stroke(
            r,
            1.,
            Stroke::new(1.0_f32, Color32::from_rgb(81, 128, 182)),
            StrokeKind::Outside,
        );
        if response.changed() {
            self.update_preview();
        }
        Ok(())
    }
    fn page_pictures(&mut self, page: usize) -> Result<&[crate::pictures::Picture], &str> {
        if !self.pictures.contains_key(&page) {
            let found = (|| -> Result<Vec<crate::pictures::Picture>> {
                let session = self.doc.as_ref().context("문서")?;
                if self.picture_source.is_none() {
                    self.picture_source =
                        Some(crate::pictures::open(session.bytes(), &session.password)?);
                }
                crate::pictures::placements(
                    self.picture_source.as_ref().unwrap(),
                    page,
                    &session.doc.page(page)?,
                )
            })();
            self.pictures.insert(
                page,
                found.map_err(|e| format!("이 페이지의 이미지 위치를 읽을 수 없습니다. {e}")),
            );
        }
        self.pictures[&page].as_deref().map_err(String::as_str)
    }
    fn pick_image(title: &str) -> Result<Option<image::DynamicImage>> {
        rfd::FileDialog::new()
            .set_title(title)
            .add_filter("이미지", &["png", "jpg", "jpeg", "bmp", "tif", "tiff"])
            .pick_file()
            .map(image::open)
            .transpose()
            .map_err(Into::into)
    }
    fn group_size(&self) -> usize {
        if self.spread {
            2
        } else {
            1
        }
    }

    fn row_layout(&mut self, row: usize, viewport: Rect) -> Result<RowLayout> {
        let group = self.group_size();
        let first = row * group;
        let mut sizes = Vec::with_capacity(group);
        for number in first..(first + group).min(self.count()) {
            let size = if let Some(size) = self.page_sizes.get(&number) {
                *size
            } else {
                let size = self.session()?.doc.page(number)?.size;
                self.page_sizes.insert(number, size);
                size
            };
            sizes.push(size);
        }
        let gap = if sizes.len() > 1 { 10. } else { 0. };
        let w = sizes.iter().map(|s| s.x).sum::<f32>();
        let h = sizes.iter().map(|s| s.y).fold(0., f32::max);
        let width_scale = (viewport.width() - gap - 12.).max(1.) / w;
        let fit = width_scale
            .min((viewport.height() - 8.).max(1.) / h)
            .max(0.001);
        let zoom = if self.width_fit {
            width_scale / fit
        } else {
            self.zoom
        };
        let scale = zoom * fit;
        Ok(RowLayout {
            first,
            sizes,
            scale,
            zoom,
            content: vec2(w * scale + gap, h * scale),
        })
    }
    fn wheel(&mut self, ctx: &egui::Context, viewport: Rect) -> Result<()> {
        if !ctx.input(|i| i.pointer.hover_pos().is_some_and(|p| viewport.contains(p))) {
            return Ok(());
        }
        let group = self.group_size();
        let last_row = (self.count() - 1) / group;
        for event in ctx.input(|i| i.events.clone()) {
            let Event::MouseWheel {
                unit,
                delta,
                modifiers,
            } = event
            else {
                continue;
            };
            if modifiers.ctrl || modifiers.command {
                let steps = match unit {
                    MouseWheelUnit::Point => delta.y / 50.,
                    _ => delta.y,
                }
                .clamp(-5., 5.);
                self.change_zoom(self.zoom * 1.30f32.powf(steps));
                continue;
            }
            let row = self.page / group;
            let bound = self.row_layout(row, viewport)?.limit(viewport);
            let pixels = delta
                * match unit {
                    MouseWheelUnit::Point => 1.,
                    MouseWheelUnit::Line => 54.,
                    MouseWheelUnit::Page => viewport.height() * 0.9,
                };
            self.pan.x = (self.pan.x + pixels.x).clamp(-bound.x, bound.x);
            if delta.y == 0. {
                continue;
            }
            if bound.y <= f32::EPSILON {
                let steps = match unit {
                    MouseWheelUnit::Point => -delta.y / viewport.height().max(1.),
                    _ => -delta.y,
                };
                // Keep fractional wheel input between events; only whole rows are painted.
                let position = (row as f64 + self.wheel_fraction + f64::from(steps))
                    .clamp(0., last_row as f64);
                let target = position.round() as usize;
                if target != row {
                    self.commit()?;
                    self.page = target * group;
                    self.page_text = (self.page + 1).to_string();
                    self.pending_dest = None;
                    let next_limit = self.row_layout(target, viewport)?.limit(viewport);
                    self.pan.y = if target > row {
                        next_limit.y
                    } else {
                        -next_limit.y
                    };
                    ctx.request_repaint();
                }
                self.wheel_fraction = position - target as f64;
            } else {
                self.wheel_fraction = 0.;
                let old = self.pan.y.clamp(-bound.y, bound.y);
                self.pan.y = (old + pixels.y).clamp(-bound.y, bound.y);
                if self.pan.y != old {
                    continue;
                }
                let down = pixels.y < 0.;
                let next = row
                    .saturating_add_signed(if down { 1 } else { -1 })
                    .min(last_row);
                if next != row {
                    self.go(next * group)?;
                    let edge = self.row_layout(next, viewport)?.limit(viewport).y;
                    self.pan.y = if down { edge } else { -edge };
                }
            }
        }
        Ok(())
    }
    fn canvas(&mut self, ui: &mut Ui) -> Result<()> {
        let viewport = ui.max_rect();
        self.view = viewport;
        let ctx = ui.ctx().clone();
        if self.doc.is_none() {
            ui.centered_and_justified(|ui| {
                if ui.button("PDF 열기").clicked() {
                    self.cmd = Some(Cmd::Open)
                }
            });
            return Ok(());
        }
        let blocked = self.prompt.is_some() || !self.error.is_empty() || self.job.is_some();
        if !blocked {
            self.wheel(&ctx, viewport)?;
        }
        let group = self.group_size();
        let row = self.page / group;
        let layout = self.row_layout(row, viewport)?;
        let first = layout.first;
        self.scale = layout.scale;
        self.zoom = layout.zoom;
        self.content = layout.content;
        let limit = layout.limit(viewport);
        if let Some(d) = self.pending_dest.take() {
            if let Some(point) = d.point {
                let xoff = if d.page > first {
                    layout.sizes[0].x * self.scale + 10.
                } else {
                    0.
                };
                self.pan = vec2(
                    self.content.x * 0.5 - xoff - point.x * self.scale,
                    (layout.sizes[d.page - first].y * 0.5 - point.y) * self.scale,
                );
            }
        }
        self.pan = self.pan.clamp(-limit, limit);
        let ppp = ctx.pixels_per_point();
        let page_rects = layout.visible_pages(viewport, self.pan, ppp);
        let page_scale = layout.scale;
        let sense = if self.mode == Mode::EditImage {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        let bg = ui.interact(viewport, Id::new("pdf_canvas"), sense);
        let open_menu = bg.secondary_clicked();
        if open_menu {
            self.picture_menu = None;
        }
        let pointer = if bg.clicked() || open_menu {
            bg.interact_pointer_pos()
        } else {
            ctx.pointer_hover_pos()
        };
        let over = pointer.and_then(|p| {
            page_rects
                .iter()
                .find(|v| viewport.contains(p) && v.rect.contains(p))
                .map(|v| (v.number, ((p - v.rect.min) / page_scale).to_pos2()))
        });
        let mut hovered_picture = None;
        if !blocked {
            if self.mode == Mode::View && bg.dragged() {
                self.pan += ctx.input(|i| i.pointer.delta());
            }
            if matches!(self.mode, Mode::Text | Mode::AddImage)
                && bg.drag_started_by(PointerButton::Primary)
            {
                if let Some(start) = ctx.input(|i| i.pointer.press_origin()) {
                    self.drag = page_rects
                        .iter()
                        .find(|v| viewport.contains(start) && v.rect.contains(start))
                        .map(|v| (v.number, ((start - v.rect.min) / page_scale).to_pos2()))
                }
            }
            if let Some((i, point)) = over {
                let link = if self.mode == Mode::View {
                    self.ensure_layout(i)?;
                    self.session()?.doc.page(i)?.link(point).or_else(|| {
                        self.toc[&i]
                            .iter()
                            .find(|(r, _)| r.contains(point))
                            .map(|(_, d)| d.clone())
                    })
                } else {
                    None
                };
                let editing_image = self.mode == Mode::EditImage;
                if open_menu || (editing_image && (bg.hovered() || bg.clicked())) {
                    match self.page_pictures(i) {
                        Ok(pictures) => {
                            if let Some((idx, picture)) = pictures
                                .iter()
                                .enumerate()
                                .rev()
                                .find(|(_, p)| p.contains(point))
                            {
                                if open_menu {
                                    self.picture_menu = Some((i, idx, picture.clone()));
                                } else if editing_image {
                                    hovered_picture = Some((i, idx));
                                    if bg.clicked() {
                                        self.cmd = Some(Cmd::ReplaceImage(i, picture.clone()));
                                    } else if !bg.context_menu_opened() {
                                        let [w, h] = picture.pixels;
                                        egui::show_tooltip_at_pointer(
                                            &ctx,
                                            ui.layer_id(),
                                            Id::new(("picture_pixels", self.id)),
                                            |ui| {
                                                let label =
                                                    Label::new(format!("{w} × {h} px")).extend();
                                                let rect = ui.add(label).rect;
                                                // egui positions a tooltip using its previous size.
                                                if !ui.clip_rect().contains_rect(rect) {
                                                    ui.ctx().request_discard(
                                                        "resize image dimensions tooltip",
                                                    );
                                                }
                                            },
                                        );
                                    }
                                }
                            }
                        }
                        Err(err) if bg.clicked() => anyhow::bail!("{err}"),
                        _ => {}
                    }
                }
                if self.mode == Mode::EditImage {
                    if hovered_picture.is_some() {
                        bg.clone().on_hover_cursor(CursorIcon::PointingHand);
                    }
                } else if self.mode == Mode::AddImage {
                    bg.clone().on_hover_cursor(CursorIcon::Crosshair);
                } else if link.is_some() {
                    bg.clone().on_hover_cursor(CursorIcon::PointingHand);
                } else if self.mode != Mode::View {
                    bg.clone().on_hover_cursor(CursorIcon::Text);
                }
                if bg.clicked() {
                    let inside = self
                        .edit
                        .as_ref()
                        .is_some_and(|e| e.page == i && e.rect.contains(point));
                    if !inside {
                        self.commit()?;
                        self.wheel_fraction = 0.;
                        self.page = i;
                        self.page_text = (i + 1).to_string();
                        match self.mode {
                            Mode::View => {
                                if let Some(d) = link {
                                    self.dest(d)?;
                                    ctx.request_repaint();
                                    return Ok(());
                                }
                            }
                            Mode::Edit => self.begin_edit(i, point, None)?,
                            Mode::Text => self.begin_edit(
                                i,
                                point,
                                Some(Rect::from_min_size(point, vec2(180., 44.))),
                            )?,
                            Mode::EditImage | Mode::AddImage => {}
                        }
                    }
                }
            }
            bg.context_menu(|ui| {
                if let Some((page, index, picture)) = &self.picture_menu {
                    let allowed = self.doc.as_ref().is_some_and(|s| s.doc.extractable());
                    if ui
                        .add_enabled(allowed, Button::new("이미지 내보내기…"))
                        .on_disabled_hover_text("이 PDF는 이미지 추출 권한이 제한되어 있습니다.")
                        .clicked()
                    {
                        self.cmd = Some(Cmd::ExportPicture(*page, *index, picture.clone()));
                        ui.close_menu();
                    }
                    ui.separator();
                }
                self.menu(ui);
            });
            if bg.drag_stopped_by(PointerButton::Primary) {
                if let (Some((p, start)), Some((i, end))) = (self.drag.take(), over) {
                    if p == i {
                        let rect = Rect::from_two_pos(start, end);
                        if rect.width() > 4. && rect.height() > 4. {
                            if self.mode == Mode::Text {
                                self.begin_edit(p, start, Some(rect))?
                            } else if self.mode == Mode::AddImage {
                                self.cmd = Some(Cmd::InsertImage(p, rect));
                            }
                        }
                    }
                }
            }
        }
        // Reuse recently visible textures on reversal. Cap the offscreen cache
        // separately so that large zoomed pages cannot evict a visible page.
        let mut spare_bytes = 0usize;
        self.surfaces.retain(|p, s| {
            if page_rects.iter().any(|v| v.number == *p) {
                return true;
            }
            if p.abs_diff(first) > group * 2 {
                return false;
            }
            let bytes = s.byte_size();
            if spare_bytes.saturating_add(bytes) > 48 * 1024 * 1024 {
                return false;
            }
            spare_bytes += bytes;
            true
        });
        let max_texture_side = ctx.input(|i| i.max_texture_side);
        for &VisiblePage { number: i, rect: r } in &page_rects {
            let pixels = raster_dimensions(r.size() * ppp, max_texture_side);
            let update = self
                .surfaces
                .get(&i)
                .is_none_or(|tex| tex.size() != pixels && Instant::now() >= self.quality);
            if update {
                let doc = self.preview.as_ref().unwrap_or(&self.session()?.doc);
                let bitmap = doc.page(i)?.render_for_screen(pixels)?;
                let img = ColorImage::from_rgba_unmultiplied(pixels, bitmap.as_raw());
                let tex = ctx.load_texture(format!("page{i}"), img, TextureOptions::LINEAR);
                self.surfaces.insert(i, tex);
            }
            let painter = ui
                .painter()
                .with_clip_rect(ui.clip_rect().intersect(viewport));
            painter.image(
                self.surfaces[&i].id(),
                r,
                Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
                Color32::WHITE,
            );
            if self.find.open {
                for (index, (_, boxes)) in
                    self.find.found.iter().enumerate().filter(|(_, a)| a.0 == i)
                {
                    for b in boxes {
                        let mark = Rect::from_min_max(
                            r.min + b.min.to_vec2() * page_scale,
                            r.min + b.max.to_vec2() * page_scale,
                        );
                        painter.rect_filled(
                            mark,
                            1.,
                            if index == self.find.active {
                                Color32::from_rgba_unmultiplied(255, 130, 25, 120)
                            } else {
                                Color32::from_rgba_unmultiplied(255, 224, 45, 95)
                            },
                        );
                    }
                }
            }
            if self.mode == Mode::EditImage {
                if let Ok(pictures) = self.page_pictures(i) {
                    let to_screen = |p: Pos2| r.min + p.to_vec2() * page_scale;
                    for (idx, pic) in pictures.iter().enumerate() {
                        let bounds = pic.bounds();
                        let clip = Rect::from_min_max(to_screen(bounds.min), to_screen(bounds.max))
                            .expand(1.);
                        if !painter.clip_rect().intersects(clip) {
                            continue;
                        }
                        let stroke = if hovered_picture == Some((i, idx)) {
                            Stroke::new(2_f32, Color32::from_rgb(39, 99, 178))
                        } else {
                            Stroke::new(1_f32, Color32::from_rgba_unmultiplied(64, 116, 180, 135))
                        };
                        painter
                            .with_clip_rect(painter.clip_rect().intersect(clip))
                            .add(Shape::closed_line(
                                pic.corners.map(to_screen).to_vec(),
                                stroke,
                            ));
                    }
                }
            }
            if let (Some((p, start)), Some((q, end))) = (self.drag, over) {
                if i == p && i == q {
                    let b = Rect::from_two_pos(
                        r.min + start.to_vec2() * page_scale,
                        r.min + end.to_vec2() * page_scale,
                    );
                    painter.rect_stroke(
                        b,
                        1.,
                        Stroke::new(1.0_f32, Color32::DARK_BLUE),
                        StrokeKind::Inside,
                    );
                }
            }
            self.inline(ui, i, r, page_scale)?;
        }
        if Instant::now() < self.quality {
            ctx.request_repaint_after(self.quality.saturating_duration_since(Instant::now()));
        }
        let groups = self.count().div_ceil(group);
        let thumb_h = (viewport.height() / groups as f32
            * (viewport.height() / self.content.y).min(1.))
        .clamp(24., viewport.height().max(24.));
        if groups > 1 || limit.y > 0. {
            let track =
                Rect::from_min_max(pos2(viewport.right() - 12., viewport.top()), viewport.max);
            let travel = (track.height() - thumb_h).max(1.);
            let local = if limit.y > 0. {
                (limit.y - self.pan.y) / (2. * limit.y)
            } else {
                0.
            };
            let intervals = if limit.y > 0. {
                groups
            } else {
                groups.saturating_sub(1).max(1)
            } as f32;
            let y = track.top() + (row as f32 + local) / intervals * travel;
            let thumb = Rect::from_min_size(pos2(track.left() + 3., y), vec2(6., thumb_h));
            let rb = ui.interact(track, Id::new("overlay_scroll"), Sense::click_and_drag());
            ui.painter().rect_filled(
                thumb,
                3.,
                Color32::from_black_alpha(if rb.hovered() || rb.dragged() {
                    115
                } else {
                    62
                }),
            );
            if !blocked && (rb.clicked() || rb.dragged()) {
                if let Some(pos) = rb.interact_pointer_pos() {
                    let t =
                        ((pos.y - track.top() - thumb_h * 0.5) / travel).clamp(0., 1.) * intervals;
                    let idx = if limit.y > 0. {
                        t.floor() as usize
                    } else {
                        t.round() as usize
                    };
                    let idx = idx.min(groups - 1);
                    self.wheel_fraction = 0.;
                    if idx != first / group {
                        self.go(idx * group)?;
                    }
                    if limit.y > 0. {
                        self.pan.y = limit.y * (1. - 2. * (t - idx as f32).clamp(0., 1.));
                    }
                    ctx.request_repaint();
                }
            }
        }
        Ok(())
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if !self.error.is_empty() {
            Window::new("SimplePDF")
                .id(Id::new("error"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(&self.error);
                    if ui.button("확인").clicked() {
                        self.error.clear()
                    }
                });
            return;
        }
        if let Some(p) = &self.prompt {
            let title = match p {
                Prompt::Password(_) => "PDF 암호",
                Prompt::Pages(_) => "페이지 범위",
                Prompt::Bookmark(_) => "책갈피 추가",
                Prompt::Rename(_) => "책갈피 이름 변경",
            };
            let password = matches!(p, Prompt::Password(_));
            let is_range = matches!(p, Prompt::Pages(_));
            let mut accept = false;
            let mut cancel = false;
            Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    let r = ui.add(
                        TextEdit::singleline(&mut self.input)
                            .password(password)
                            .hint_text(if is_range { "전체 또는 1,3-5" } else { "" }),
                    );
                    if self.focus_prompt {
                        r.request_focus();
                        self.focus_prompt = false
                    }
                    ui.horizontal(|ui| {
                        accept =
                            ui.button("확인").clicked() || ui.input(|i| i.key_pressed(Key::Enter));
                        cancel = ui.button("취소").clicked();
                    });
                });
            if accept {
                if let Err(e) = self.finish_prompt() {
                    self.error = e.to_string()
                }
            } else if cancel {
                self.prompt = None
            }
        }
    }
}
impl DocumentTab {
    fn poll_job(&mut self, ctx: &egui::Context) {
        if let Some(job) = &mut self.job {
            match job.child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        self.error = std::fs::read_to_string(&job.err)
                            .unwrap_or_else(|_| format!("변환 실패 ({status})"));
                    }
                    self.job = None;
                }
                Err(e) => {
                    self.error = e.to_string();
                    self.job = None;
                }
                _ => ctx.request_repaint_after(Duration::from_millis(180)),
            }
        }
    }
    fn draw(&mut self, ctx: &egui::Context, titles: &[TabLabel], active: usize, full: &mut bool) {
        if let Err(e) = self.shortcuts(ctx, full) {
            self.error = e.to_string()
        }
        if !*full {
            self.toolbar(ctx, titles, active);
        }
        if self.book {
            self.bookmarks(ctx)
        }
        if self.props {
            self.properties(ctx)
        }
        CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::from_gray(231)))
            .show(ctx, |ui| {
                if let Err(e) = self.canvas(ui) {
                    self.error = e.to_string()
                }
            });
        if self.find.open {
            if let Err(e) = self.search_ui(ctx) {
                self.error = e.to_string()
            }
        }
        if self.job.is_some() {
            Area::new(Id::new("busy"))
                .anchor(Align2::CENTER_TOP, vec2(0., 43.))
                .show(ctx, |ui| {
                    Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("변환 중");
                        });
                    });
                });
        }
        self.dialogs(ctx);
    }
}
