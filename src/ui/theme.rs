use egui::Color32;

pub fn setup_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    
    // Deep dark background — navy-charcoal
    visuals.window_fill = Color32::from_rgb(18, 18, 30);
    visuals.panel_fill = Color32::from_rgb(22, 22, 38);
    visuals.extreme_bg_color = Color32::from_rgb(12, 12, 22);
    visuals.faint_bg_color = Color32::from_rgb(30, 30, 48);
    
    // Accent color: vibrant indigo
    let accent = Color32::from_rgb(99, 102, 241); // #6366f1
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent);
    visuals.hyperlink_color = Color32::from_rgb(129, 140, 248);
    
    // Widget colors
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 38, 56);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(50, 50, 72);
    visuals.widgets.active.bg_fill = accent;
    
    // Subtle window shadow
    visuals.window_shadow.offset = [0, 2];
    visuals.window_shadow.blur = 8;
    visuals.window_shadow.spread = 0;
    visuals.window_shadow.color = Color32::from_black_alpha(80);
    visuals.popup_shadow = visuals.window_shadow;
    
    ctx.set_visuals(visuals);
}
