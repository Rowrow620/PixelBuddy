use crate::app::PixelBuddyApp;

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::TopBottomPanel::bottom("palette_panel")
        .exact_height(64.0)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Palette:");
                
                let selected = app.editor.document.palette.selected_index;
                let palette_len = app.editor.document.palette.colors.len();
                
                for i in 0..palette_len {
                    let color = app.editor.document.palette.colors[i];
                    let egui_color = egui::Color32::from_rgba_unmultiplied(
                        color[0], color[1], color[2], color[3],
                    );
                    
                    let stroke = if i == selected {
                        egui::Stroke::new(2.0_f32, egui::Color32::WHITE)
                    } else {
                        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60))
                    };
                    
                    let button = egui::Button::new("  ")
                        .fill(egui_color)
                        .stroke(stroke)
                        .min_size(egui::vec2(20.0, 20.0));
                    
                    let response = ui.add(button);
                    if response.clicked() {
                        app.editor.document.palette.set_selected(i);
                        app.editor.set_primary_color(color);
                    }
                    if response.secondary_clicked() {
                        app.editor.secondary_color = color;
                    }
                }
                
                ui.separator();
                
                if ui.small_button("+ Add").on_hover_text("Add current color to palette").clicked() {
                    app.editor.document.palette.add_color(app.editor.primary_color);
                }
            });
        });
}
