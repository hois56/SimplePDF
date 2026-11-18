use eframe::egui::*;
pub fn icon(ui: &mut Ui, name: &str, tip: &str, active: bool) -> Response {
    let (r, response) = ui.allocate_exact_size(vec2(29., 28.), Sense::click());
    if active || response.hovered() {
        ui.painter().rect_filled(
            r,
            5.,
            if active {
                Color32::from_rgb(226, 237, 249)
            } else {
                Color32::from_gray(235)
            },
        );
    }
    let base = r.center() - vec2(9., 9.);
    let pt = |x: f32, y: f32| pos2(base.x + x, base.y + y);
    let p = ui.painter();
    let s = Stroke::new(1.45_f32, Color32::from_rgb(58, 70, 85));
    let line = |v: &[(f32, f32)]| {
        p.add(Shape::line(v.iter().map(|&(x, y)| pt(x, y)).collect(), s));
    };
    let rect = |a, b, c, d| {
        p.rect_stroke(
            Rect::from_two_pos(pt(a, b), pt(c, d)),
            1.,
            s,
            StrokeKind::Inside,
        );
    };
    match name {
        "book" => {
            rect(1., 2., 17., 16.);
            line(&[(6., 2.), (6., 16.)]);
            if active {
                p.rect_filled(
                    Rect::from_two_pos(pt(2., 3.), pt(5., 15.)),
                    0.,
                    Color32::from_rgb(122, 151, 188),
                );
                line(&[(13., 5.), (9., 9.), (13., 13.)]);
            } else {
                line(&[(10., 5.), (14., 9.), (10., 13.)]);
            }
        }
        "open" => {
            line(&[
                (2., 14.),
                (2., 5.),
                (7., 5.),
                (9., 7.),
                (16., 7.),
                (14., 14.),
                (2., 14.),
            ]);
        }
        "save" => {
            line(&[
                (3., 3.),
                (12., 3.),
                (15., 6.),
                (15., 15.),
                (3., 15.),
                (3., 3.),
            ]);
            rect(6., 3., 12., 7.);
            rect(6., 11., 12., 15.);
        }
        "find" => {
            p.circle_stroke(pt(7., 7.), 4.5, s);
            line(&[(10., 10.), (15., 15.)]);
        }
        "prev" => line(&[(11., 4.), (6., 9.), (11., 14.)]),
        "next" => line(&[(7., 4.), (12., 9.), (7., 14.)]),
        "fit" => {
            rect(6., 2., 12., 16.);
            line(&[(4., 3.), (2., 3.), (2., 6.)]);
            line(&[(14., 3.), (16., 3.), (16., 6.)]);
            line(&[(2., 12.), (2., 15.), (4., 15.)]);
            line(&[(14., 15.), (16., 15.), (16., 12.)]);
        }
        "spread" => {
            rect(1., 3., 8., 15.);
            rect(10., 3., 17., 15.);
        }
        "min" => line(&[(4., 9.), (14., 9.)]),
        "max" => rect(4., 4., 14., 14.),
        "close" => {
            line(&[(4., 4.), (14., 14.)]);
            line(&[(14., 4.), (4., 14.)]);
        }
        "plus" => {
            line(&[(9., 3.), (9., 15.)]);
            line(&[(3., 9.), (15., 9.)]);
        }
        _ => {}
    }
    response.on_hover_text(tip)
}

pub fn disclosure(ui: &mut Ui, collapsed: bool) -> Response {
    let (r, response) = ui.allocate_exact_size(vec2(18., 18.), Sense::click());
    if ui.is_rect_visible(r) {
        let p = ui.painter();
        let c = r.center();
        let box_rect = Rect::from_center_size(c, vec2(13., 13.));
        let fg = Color32::from_rgb(61, 79, 101);
        p.rect(
            box_rect,
            3.,
            if response.hovered() {
                Color32::from_rgb(216, 229, 246)
            } else {
                Color32::from_rgb(241, 244, 248)
            },
            Stroke::new(1_f32, Color32::from_rgb(165, 179, 195)),
            StrokeKind::Inside,
        );
        let stroke = Stroke::new(1.6_f32, fg);
        p.line_segment([c - vec2(3.4, 0.), c + vec2(3.4, 0.)], stroke);
        if collapsed {
            p.line_segment([c - vec2(0., 3.4), c + vec2(0., 3.4)], stroke);
        }
    }
    response.on_hover_text(if collapsed {
        "하위 책갈피 펼치기"
    } else {
        "하위 책갈피 접기"
    })
}
