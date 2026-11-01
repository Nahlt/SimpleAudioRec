//! Elementi grafici: pulsanti con icona disegnata, indicatori, colori.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

pub const RED: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
pub const GREEN: Color32 = Color32::from_rgb(0x2E, 0xA0, 0x43);
pub const ORANGE: Color32 = Color32::from_rgb(0xF2, 0x8C, 0x28);
pub const ACCENT: Color32 = Color32::from_rgb(0x2B, 0x88, 0xD8);

pub fn dim(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode { Color32::from_gray(70) } else { Color32::from_gray(200) }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Rec,
    Pause,
    Play,
    Stop,
    Mic,
    MicOff,
    Bookmark,
    Gear,
}

pub fn paint_icon(p: &Painter, icon: Icon, r: Rect, color: Color32) {
    let c = r.center();
    let s = r.height().min(r.width());
    match icon {
        Icon::Rec => {
            p.circle_filled(c, s * 0.36, RED);
        }
        Icon::Pause => {
            let w = s * 0.18;
            let h = s * 0.62;
            for dx in [-s * 0.16, s * 0.16] {
                p.rect_filled(Rect::from_center_size(pos2(c.x + dx, c.y), vec2(w, h)), 1.5, color);
            }
        }
        Icon::Play => {
            let h = s * 0.32;
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - h * 0.8, c.y - h), pos2(c.x + h, c.y), pos2(c.x - h * 0.8, c.y + h)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, Vec2::splat(s * 0.5)), 2.0, color);
        }
        Icon::Mic | Icon::MicOff => {
            let w = s * 0.26;
            let body = Rect::from_center_size(pos2(c.x, c.y - s * 0.1), vec2(w, s * 0.44));
            p.rect_filled(body, w / 2.0, color);
            let st = Stroke::new(s * 0.07, color);
            let arc: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 12.0);
                    pos2(c.x + a.cos() * s * 0.24, c.y - s * 0.02 + a.sin() * s * 0.2)
                })
                .collect();
            p.add(Shape::line(arc, st));
            p.line_segment([pos2(c.x, c.y + s * 0.18), pos2(c.x, c.y + s * 0.32)], st);
            if icon == Icon::MicOff {
                p.line_segment(
                    [pos2(c.x - s * 0.3, c.y - s * 0.34), pos2(c.x + s * 0.3, c.y + s * 0.3)],
                    Stroke::new(s * 0.09, RED),
                );
            }
        }
        Icon::Bookmark => {
            let w = s * 0.24;
            let top = c.y - s * 0.32;
            let bot = c.y + s * 0.32;
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - w, top), pos2(c.x + w, top), pos2(c.x + w, bot), pos2(c.x, bot - s * 0.18)],
                color,
                Stroke::NONE,
            ));
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - w, top), pos2(c.x, bot - s * 0.18), pos2(c.x - w, bot)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Gear => {
            let st = Stroke::new(s * 0.12, color);
            for i in 0..8 {
                let a = std::f32::consts::TAU * i as f32 / 8.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * s * 0.2, c + d * s * 0.4], st);
            }
            p.circle_stroke(c, s * 0.22, Stroke::new(s * 0.12, color));
        }
    }
}

/// Pulsante con icona e testo.
pub fn icon_button(ui: &mut Ui, icon: Icon, text: &str, enabled: bool, highlight: bool, width: f32) -> Response {
    let height = 34.0;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), sense);
    if ui.is_rect_visible(rect) {
        let v = &ui.visuals().widgets;
        let w = if !enabled { &v.noninteractive } else { ui.style().interact(&resp) };
        let fill = if highlight && enabled { ACCENT.gamma_multiply(0.35) } else { w.weak_bg_fill };
        ui.painter().rect(rect, 6.0, fill, w.bg_stroke, egui::StrokeKind::Inside);
        let fg = if enabled { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
        let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(14.0), fg);
        let icon_size = 18.0;
        let total = icon_size + 6.0 + galley.size().x;
        let x0 = rect.center().x - total / 2.0;
        let icon_rect = Rect::from_min_size(pos2(x0, rect.center().y - icon_size / 2.0), Vec2::splat(icon_size));
        let icol = if enabled { fg } else { fg.gamma_multiply(0.6) };
        if icon == Icon::Rec && !enabled {
            ui.painter().circle_filled(icon_rect.center(), icon_size * 0.36, RED.gamma_multiply(0.4));
        } else {
            paint_icon(ui.painter(), icon, icon_rect, icol);
        }
        ui.painter().galley(pos2(x0 + icon_size + 6.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    }
    resp.on_hover_cursor(if enabled { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default })
}

/// Pulsante quadrato solo icona (es. ingranaggio).
pub fn small_icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    let w = ui.style().interact(&resp);
    if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, w.weak_bg_fill);
    }
    paint_icon(ui.painter(), icon, rect.shrink(4.0), ui.visuals().text_color());
    resp.on_hover_text(tooltip)
}

/// Indicatore circolare con etichetta.
pub fn indicator(ui: &mut Ui, on: bool, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
        let col = if on { color } else { dim(ui) };
        if on {
            ui.painter().circle_filled(rect.center(), 7.5, color.gamma_multiply(0.3));
        }
        ui.painter().circle_filled(rect.center(), 5.5, col);
        ui.label(label);
    });
}
