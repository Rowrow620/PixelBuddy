use egui::Color32;

pub const SEPARATOR_COLOR: Color32 = Color32::from_rgb(42, 42, 68);

pub fn setup_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "PressStart2P".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../../assets/fonts/PressStart2P-Regular.ttf"))),
    );
    fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "PressStart2P".to_owned());
    fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "PressStart2P".to_owned());
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();

    // Deep dark background — navy-charcoal
    visuals.window_fill = Color32::from_rgb(26, 26, 46);
    visuals.panel_fill = Color32::from_rgb(20, 20, 34); // left toolbar
    visuals.extreme_bg_color = Color32::from_rgb(10, 10, 20); // canvas viewport
    visuals.faint_bg_color = Color32::from_rgb(36, 36, 64); // inputs

    // Accent color: vibrant indigo
    let accent = Color32::from_rgb(99, 102, 241); // #6366f1
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.hyperlink_color = Color32::from_rgb(129, 140, 248);

    // Widget colors
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(42, 42, 68);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(36, 36, 60);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(50, 50, 75));
    
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(56, 56, 84);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(70, 70, 100));
    
    visuals.widgets.active.bg_fill = accent;

    // Subtle window shadow & corners
    visuals.window_shadow.offset = [0, 4];
    visuals.window_shadow.blur = 12;
    visuals.window_shadow.spread = 0;
    visuals.window_shadow.color = Color32::from_black_alpha(100);
    visuals.popup_shadow = visuals.window_shadow;
    visuals.window_corner_radius = 8.into();

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    use egui::{TextStyle, FontFamily};
    style.text_styles = [
        (TextStyle::Heading, egui::FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Name("Heading2".into()), egui::FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Name("Context".into()), egui::FontId::new(10.0, FontFamily::Proportional)),
        (TextStyle::Body, egui::FontId::new(10.0, FontFamily::Proportional)),
        (TextStyle::Monospace, egui::FontId::new(10.0, FontFamily::Monospace)),
        (TextStyle::Button, egui::FontId::new(10.0, FontFamily::Proportional)),
        (TextStyle::Small, egui::FontId::new(8.0, FontFamily::Proportional)),
    ].into();
    ctx.set_style(style);
}
