//! Widgets UI modernes (toggle, drop zone, progress).

use egui::{Color32, CornerRadius, Margin, Pos2, Response, Sense, Stroke, Ui, Vec2};

/// Toggle switch lisible (contraste fort sombre / clair).
pub fn toggle_switch(ui: &mut Ui, on: &mut bool, enabled: bool) -> Response {
    let desired = Vec2::new(40.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(desired, Sense::click());
    if enabled && response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if !enabled {
        response = response.on_disabled_hover_text("");
    }

    if ui.is_rect_visible(rect) {
        let how = if !enabled {
            0.4
        } else if response.hovered() {
            1.0
        } else {
            0.92
        };

        let track_on = Color32::from_rgb(0, 140, 240);
        let track_off = if ui.visuals().dark_mode {
            Color32::from_rgb(72, 72, 78)
        } else {
            Color32::from_rgb(170, 172, 180)
        };
        let track = fade(if *on { track_on } else { track_off }, how);

        ui.painter()
            .rect_filled(rect, CornerRadius::same(11), track);

        let knob_r = 8.0;
        let pad = 3.0;
        let cx = if *on {
            rect.right() - pad - knob_r
        } else {
            rect.left() + pad + knob_r
        };
        let cy = rect.center().y;
        let knob = Color32::from_rgb(250, 250, 252);
        ui.painter()
            .circle_filled(Pos2::new(cx, cy), knob_r, fade(knob, how.max(0.55)));
    }
    response
}

fn fade(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        c.r(),
        c.g(),
        c.b(),
        ((c.a() as f32) * a.clamp(0.0, 1.0)) as u8,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropZoneAction {
    None,
    Browse,
}

/// Zone drop visuelle + bouton ouvrir fichiers.
pub fn drop_zone(ui: &mut Ui, label: &str, button: &str) -> DropZoneAction {
    let mut action = DropZoneAction::None;
    let hovering_files = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());

    let stroke = if hovering_files {
        Stroke::new(1.5_f32, Color32::from_rgb(0, 140, 240))
    } else {
        Stroke::new(
            1.0_f32,
            if ui.visuals().dark_mode {
                Color32::from_rgb(70, 70, 78)
            } else {
                Color32::from_rgb(200, 202, 210)
            },
        )
    };
    let fill = if hovering_files {
        if ui.visuals().dark_mode {
            Color32::from_rgb(36, 48, 64)
        } else {
            Color32::from_rgb(228, 238, 252)
        }
    } else if ui.visuals().dark_mode {
        Color32::from_rgb(34, 34, 38)
    } else {
        Color32::from_rgb(246, 247, 250)
    };

    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            let w = ui.available_width();
            ui.set_width(w);
            ui.set_max_width(w);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(label).weak());
                ui.add_space(8.0);
                if ui
                    .add(egui::Button::new(button).min_size(Vec2::new(180.0, 30.0)))
                    .clicked()
                {
                    action = DropZoneAction::Browse;
                }
            });
        });

    action
}

pub fn progress_ring(ui: &mut Ui, progress: u8, spinning: bool) {
    let size = Vec2::splat(22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let center = rect.center();
    let r = 8.0;
    let bg = if ui.visuals().dark_mode {
        Color32::from_rgb(60, 60, 66)
    } else {
        Color32::from_rgb(210, 212, 218)
    };
    ui.painter()
        .circle_stroke(center, r, Stroke::new(2.0_f32, bg));
    if spinning {
        let t = ui.input(|i| i.time) as f32;
        let start = t * 4.0;
        draw_arc(
            ui,
            center,
            r,
            start,
            start + 1.8,
            Color32::from_rgb(0, 140, 240),
        );
        ui.ctx().request_repaint();
    } else if progress > 0 {
        let frac = (progress as f32 / 100.0).clamp(0.0, 1.0);
        let start = -std::f32::consts::FRAC_PI_2;
        let end = start + frac * std::f32::consts::TAU;
        draw_arc(ui, center, r, start, end, Color32::from_rgb(0, 140, 240));
    }
}

fn draw_arc(ui: &mut Ui, center: Pos2, radius: f32, start: f32, end: f32, color: Color32) {
    let n = 24;
    let mut points = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = start + (end - start) * t;
        points.push(Pos2::new(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(2.5_f32, color)));
}

pub fn section_card(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    let fill = if ui.visuals().dark_mode {
        Color32::from_rgb(30, 30, 34)
    } else {
        Color32::from_rgb(255, 255, 255)
    };
    let stroke = Stroke::new(
        1.0_f32,
        if ui.visuals().dark_mode {
            Color32::from_rgb(55, 55, 62)
        } else {
            Color32::from_rgb(220, 222, 228)
        },
    );
    // Ne pas forcer la largeur intérieure = available_width() extérieur :
    // les marges du Frame s'ajoutent et le cadre déborde / est croppé à droite.
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            let w = ui.available_width();
            ui.set_width(w);
            ui.set_max_width(w);
            ui.label(egui::RichText::new(title).strong().size(15.0));
            ui.add_space(4.0);
            add_contents(ui);
        });
}

/// Bouton action compact aligné sur la hauteur des toggles (~22–28 px).
pub fn action_button(ui: &mut Ui, label: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(
        egui::Button::new(label)
            .min_size(Vec2::new(0.0, 28.0))
            .corner_radius(CornerRadius::same(6)),
    )
}

/// Bouton supprimer (✕ dessiné — lisible sans police emoji).
pub fn delete_button(ui: &mut Ui) -> Response {
    let size = Vec2::splat(28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let bg = if hovered {
            Color32::from_rgba_unmultiplied(200, 60, 60, 40)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), bg);
        let stroke = Stroke::new(
            1.8_f32,
            if hovered {
                Color32::from_rgb(220, 80, 80)
            } else if ui.visuals().dark_mode {
                Color32::from_rgb(180, 180, 188)
            } else {
                Color32::from_rgb(110, 112, 120)
            },
        );
        let pad = 8.0;
        let painter = ui.painter();
        painter.line_segment(
            [
                Pos2::new(rect.left() + pad, rect.top() + pad),
                Pos2::new(rect.right() - pad, rect.bottom() - pad),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(rect.right() - pad, rect.top() + pad),
                Pos2::new(rect.left() + pad, rect.bottom() - pad),
            ],
            stroke,
        );
    }
    response.on_hover_text("Retirer de la liste")
}

/// Barre de progression globale en bas.
pub fn bottom_progress_bar(ui: &mut Ui, fraction: f32, label: &str, busy: bool) {
    let fill = if ui.visuals().dark_mode {
        Color32::from_rgb(24, 24, 28)
    } else {
        Color32::from_rgb(245, 246, 248)
    };
    egui::Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(13.5)
                        .color(if busy {
                            Color32::from_rgb(0, 160, 100)
                        } else {
                            ui.visuals().weak_text_color()
                        }),
                );
                ui.add_space(8.0);
                let avail = ui.available_width().max(40.0);
                let height = 10.0;
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(avail, height), Sense::hover());
                let track = if ui.visuals().dark_mode {
                    Color32::from_rgb(50, 50, 56)
                } else {
                    Color32::from_rgb(210, 212, 218)
                };
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(5), track);
                let f = fraction.clamp(0.0, 1.0);
                if f > 0.001 {
                    let mut filled = rect;
                    filled.set_width(rect.width() * f);
                    ui.painter().rect_filled(
                        filled,
                        CornerRadius::same(5),
                        Color32::from_rgb(0, 140, 240),
                    );
                }
                if busy {
                    ui.ctx().request_repaint();
                }
            });
        });
}
