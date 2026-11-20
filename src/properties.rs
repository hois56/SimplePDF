use eframe::egui::text::LayoutJob;

impl DocumentTab {
    fn select_face(&mut self, face: Option<&fonts::Face>) -> Result<()> {
        let edit = self.edit.as_mut().context("편집할 문구")?;
        self.editor_font = if let Some(f) = face {
            let mut data = FontData::from_owned(std::fs::read(&f.path)?);
            data.index = f.index;
            Some(std::sync::Arc::new(data))
        } else {
            self.original_editor_font.clone()
        };
        edit.style.font = face.map(|f| f.path.clone());
        self.chosen_face = face.map(|f| f.label.clone()).unwrap_or_default();
        self.refresh_editor_font = true;
        self.font_picker = false;
        self.edit_focus = true;
        self.update_preview();
        Ok(())
    }
    fn face_list(&mut self, ui: &mut Ui, opened: bool) {
        let search_id = Id::new(("font_query", self.id));
        let focused = ui.memory(|m| m.has_focus(search_id));
        let key = |k| focused && ui.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.font_picker = false;
            self.edit_focus = true;
            return;
        }
        let up = key(Key::ArrowUp);
        let down = key(Key::ArrowDown);
        let enter = key(Key::Enter);
        let response = ui.add(
            TextEdit::singleline(&mut self.font_query)
                .id(search_id)
                .hint_text("글꼴 검색")
                .return_key(None)
                .desired_width(f32::INFINITY)
                .margin(Margin::symmetric(8, 6)),
        );
        if opened {
            response.request_focus();
        }
        if response.changed() {
            self.font_cursor = 0;
        }
        let faces = fonts::matches(&self.font_query);
        let original = self.font_query.trim().is_empty();
        let total = faces.len() + usize::from(original);
        self.font_cursor = self
            .font_cursor
            .saturating_add_signed(down as isize - up as isize)
            .min(total.saturating_sub(1));
        let current = self.edit.as_ref().and_then(|e| e.style.font.as_deref());
        let mut pick = None;
        if total == 0 {
            ui.weak("검색 결과 없음");
        } else {
            ScrollArea::vertical()
                .id_salt(("font_results", self.id))
                .max_height(146.)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
                    ui.spacing_mut().item_spacing.y = 1.;
                    for n in 0..total {
                        let face = n.checked_sub(usize::from(original)).map(|j| faces[j]);
                        let label = face.map(|f| f.label.as_str()).unwrap_or("원본 글꼴");
                        let selected = face.map(|f| f.path.as_path()) == current;
                        let active = n == self.font_cursor;
                        let (rect, row) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 24.), Sense::click());
                        row.widget_info(|| {
                            WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, label)
                        });
                        if ui.is_rect_visible(rect) {
                            if active || row.hovered() {
                                let fill = if active {
                                    Color32::from_rgb(224, 235, 248)
                                } else {
                                    Color32::from_rgb(237, 242, 247)
                                };
                                ui.painter().rect_filled(rect, 4., fill);
                            }
                            let area = rect.shrink2(vec2(8., 0.));
                            let ink = Color32::from_rgb(42, 60, 79);
                            let clip = Rect::from_min_max(
                                area.min,
                                pos2(area.right() - 13., area.bottom()),
                            );
                            ui.painter()
                                .with_clip_rect(ui.clip_rect().intersect(clip))
                                .text(
                                    area.left_center(),
                                    Align2::LEFT_CENTER,
                                    label,
                                    FontId::proportional(13.),
                                    ink,
                                );
                            if selected {
                                let p = area.right_center() - vec2(4., 0.);
                                ui.painter().add(Shape::line(
                                    vec![p + vec2(-4., 0.), p + vec2(-1., 3.), p + vec2(4., -3.)],
                                    Stroke::new(1.5_f32, ink),
                                ));
                            }
                        }
                        if active && (up || down || response.changed()) {
                            row.scroll_to_me(None);
                        }
                        if row.on_hover_text(label).clicked() || (active && enter) {
                            pick = Some(face);
                        }
                    }
                });
        }
        if let Some(face) = pick {
            if let Err(err) = self.select_face(face) {
                self.error = err.to_string();
            }
        }
    }
    fn properties(&mut self, ctx: &egui::Context) {
        let ink = Color32::from_rgb(35, 48, 62);
        let muted = Color32::from_rgb(82, 97, 113);
        let edge = Stroke::new(1_f32, Color32::from_rgb(209, 218, 227));
        let accent = Color32::from_rgb(45, 86, 134);
        SidePanel::right("properties")
            .default_width(238.)
            .width_range(202. ..=340.)
            .frame(
                Frame::new()
                    .fill(Color32::from_rgb(249, 250, 252))
                    .inner_margin(Margin::symmetric(13, 10)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(6., 6.);
                ui.visuals_mut().override_text_color = Some(ink);
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::WHITE;
                ui.visuals_mut().widgets.inactive.bg_stroke = edge;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("텍스트 속성").size(16.).strong());
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if icon(ui, "close", "속성 닫기", false).clicked() {
                            self.props = false;
                            self.font_picker = false;
                        }
                    });
                });
                ui.separator();
                if self.edit.is_none() {
                    ui.add_space(6.);
                    ui.label(
                        RichText::new(if self.mode == Mode::Text {
                            "페이지에서 입력할 위치를 선택하세요."
                        } else {
                            "편집할 문구를 클릭하세요."
                        })
                        .color(muted),
                    );
                    return;
                }
                ScrollArea::vertical()
                    .id_salt(("text_properties", self.id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(5.);
                        ui.label(RichText::new("글꼴").color(muted));
                        let edit = self.edit.as_ref().unwrap();
                        let name = if self.chosen_face.is_empty() {
                            edit.runs
                                .first()
                                .map(|r| r.family.as_str())
                                .filter(|s| !s.is_empty())
                                .unwrap_or("맑은 고딕")
                        } else {
                            &self.chosen_face
                        };
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(ui.available_width(), 32.), Sense::hover());
                        let choice =
                            ui.interact(rect, Id::new(("font_family", self.id)), Sense::click());
                        choice.widget_info(|| {
                            WidgetInfo::labeled(WidgetType::ComboBox, true, format!("글꼴 {name}"))
                        });
                        let fill = if choice.hovered() {
                            Color32::from_rgb(241, 246, 251)
                        } else {
                            Color32::WHITE
                        };
                        ui.painter().rect(
                            rect,
                            5.,
                            fill,
                            if self.font_picker || choice.has_focus() {
                                Stroke::new(1.4_f32, accent)
                            } else {
                                edge
                            },
                            StrokeKind::Inside,
                        );
                        let area = rect.shrink2(vec2(9., 0.));
                        let mut job = LayoutJob::simple_singleline(
                            name.to_owned(),
                            FontId::proportional(13.),
                            ink,
                        );
                        job.wrap.max_width = (area.width() - 19.).max(1.);
                        job.wrap.max_rows = 1;
                        let galley = ui.painter().layout_job(job);
                        ui.painter().galley(
                            pos2(area.left(), area.center().y - galley.size().y * 0.5),
                            galley,
                            ink,
                        );
                        let c = pos2(area.right() - 4., area.center().y);
                        let dy = if self.font_picker { -1.8 } else { 1.8 };
                        ui.painter().add(Shape::line(
                            vec![c + vec2(-4., -dy), c + vec2(0., dy), c + vec2(4., -dy)],
                            Stroke::new(1.4_f32, muted),
                        ));
                        let toggled = choice.on_hover_text(name).clicked();
                        if toggled {
                            self.font_picker = !self.font_picker;
                            self.font_query.clear();
                            self.font_cursor = 0;
                        }
                        if self.font_picker {
                            self.face_list(ui, toggled);
                        }
                        ui.add_space(5.);
                        let edit = self.edit.as_mut().unwrap();
                        let mut changed = false;
                        ui.columns(2, |cols| {
                            cols[0].label(RichText::new("크기").color(muted));
                            let width = cols[0].available_width();
                            changed |= cols[0]
                                .add_sized(
                                    [width, 29.],
                                    DragValue::new(&mut edit.style.size)
                                        .speed(0.25)
                                        .range(1. ..=400.)
                                        .suffix(" pt"),
                                )
                                .changed();
                            cols[1].label(RichText::new("색상").color(muted));
                            cols[1].horizontal(|ui| {
                                ui.spacing_mut().interact_size = vec2(25., 29.);
                                changed |= ui.color_edit_button_srgb(&mut edit.style.rgb).changed();
                                let [r, g, b] = edit.style.rgb;
                                ui.label(
                                    RichText::new(format!("#{r:02X}{g:02X}{b:02X}"))
                                        .size(11.)
                                        .color(muted),
                                );
                            });
                        });
                        if changed {
                            self.update_preview();
                        }
                        if !self.edit_error.is_empty() {
                            ui.colored_label(Color32::DARK_RED, &self.edit_error);
                        }
                        ui.add_space(6.);
                        ui.separator();
                        ui.add_space(3.);
                        ui.horizontal(|ui| {
                            let w = (ui.available_width() - ui.spacing().item_spacing.x) * 0.5;
                            if ui
                                .add_sized(
                                    [w, 30.],
                                    Button::new(RichText::new("적용").color(Color32::WHITE))
                                        .fill(accent)
                                        .corner_radius(5.),
                                )
                                .clicked()
                            {
                                if let Err(err) = self.commit() {
                                    self.error = err.to_string();
                                }
                            }
                            if ui
                                .add_sized([w, 30.], Button::new("취소").corner_radius(5.))
                                .clicked()
                            {
                                self.cancel_edit();
                            }
                        });
                    });
            });
    }
}
