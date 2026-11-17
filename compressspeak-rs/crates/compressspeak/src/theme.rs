use cs_core::ThemePref;
use egui::{Color32, Context, FontFamily, FontId, TextStyle, ThemePreference, Visuals};

pub fn apply_theme(ctx: &Context, pref: ThemePref) {
    match pref {
        ThemePref::Dark => {
            ctx.set_theme(ThemePreference::Dark);
            ctx.set_visuals(dark_visuals());
        }
        ThemePref::Light => {
            ctx.set_theme(ThemePreference::Light);
            ctx.set_visuals(light_visuals());
        }
        ThemePref::System => {
            ctx.set_theme(ThemePreference::System);
        }
    }
    apply_style(ctx);
}

fn apply_style(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    // Compact mais lisible
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.indent = 12.0;
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(20.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(15.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(14.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(13.0, FontFamily::Proportional),
    );
    ctx.set_style(style);
}

fn dark_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = Color32::from_rgb(22, 22, 26);
    v.window_fill = Color32::from_rgb(30, 30, 34);
    v.extreme_bg_color = Color32::from_rgb(18, 18, 22);
    v.faint_bg_color = Color32::from_rgb(38, 38, 44);
    v.widgets.inactive.bg_fill = Color32::from_rgb(48, 48, 54);
    v.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0_f32, Color32::from_rgb(230, 230, 235));
    v.widgets.hovered.bg_fill = Color32::from_rgb(58, 58, 66);
    v.widgets.active.bg_fill = Color32::from_rgb(0, 110, 200);
    v.selection.bg_fill = Color32::from_rgb(0, 120, 215);
    v.override_text_color = Some(Color32::from_rgb(235, 235, 240));
    v
}

fn light_visuals() -> Visuals {
    let mut v = Visuals::light();
    v.panel_fill = Color32::from_rgb(244, 245, 248);
    v.window_fill = Color32::from_rgb(255, 255, 255);
    v.extreme_bg_color = Color32::from_rgb(255, 255, 255);
    v.faint_bg_color = Color32::from_rgb(236, 238, 242);
    v.widgets.inactive.bg_fill = Color32::from_rgb(236, 238, 242);
    v.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0_f32, Color32::from_rgb(30, 30, 35));
    v.selection.bg_fill = Color32::from_rgb(0, 120, 215);
    v.override_text_color = Some(Color32::from_rgb(25, 25, 30));
    v
}
