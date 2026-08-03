use crate::app::PixelBuddyApp;
use crate::editor::ToolType;

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::SidePanel::left("toolbar")
        .exact_width(52.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                
                let tools: &[(ToolType, &str, &str)] = &[
                    (ToolType::Pencil, "✏", "Pencil (B)"),
                    (ToolType::Eraser, "⌫", "Eraser (E)"),
                    (ToolType::Line, "╱", "Line (L)"),
                    (ToolType::Rectangle, "▢", "Rectangle (R)"),
                    (ToolType::Ellipse, "◯", "Ellipse (O)"),
                    (ToolType::Fill, "◉", "Fill (G)"),
                    (ToolType::Eyedropper, "⊙", "Eyedropper (I)"),
                ];
                
                for &(tool, icon, tooltip) in tools {
                    let is_active = app.editor.active_tool == tool;
                    let button = if is_active {
                        egui::Button::new(
                            egui::RichText::new(icon).size(18.0)
                        )
                        .min_size(egui::vec2(36.0, 36.0))
                        .fill(ui.visuals().selection.bg_fill)
                    } else {
                        egui::Button::new(
                            egui::RichText::new(icon).size(18.0)
                        )
                        .min_size(egui::vec2(36.0, 36.0))
                    };
                    if ui.add(button).on_hover_text(tooltip).clicked() {
                        app.editor.set_active_tool(tool);
                    }
                    ui.add_space(2.0);
                }
                
                ui.separator();
                
                // Foreground / background color swatches
                ui.add_space(8.0);
                
                let fg = app.editor.primary_color;
                let fg_color = egui::Color32::from_rgba_unmultiplied(fg[0], fg[1], fg[2], fg[3]);
                let (fg_rect, _fg_response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
                ui.painter().rect_filled(fg_rect, 4, fg_color);
                
                let bg = app.editor.secondary_color;
                let bg_color = egui::Color32::from_rgba_unmultiplied(bg[0], bg[1], bg[2], bg[3]);
                let (bg_rect, _bg_response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
                ui.painter().rect_filled(bg_rect, 4, bg_color);
                
                if ui.small_button("⇄ Swap").on_hover_text("Swap colors (X)").clicked() {
                    app.editor.swap_colors();
                }
                
                ui.separator();
                
                // Color picker for primary color
                let mut color32 = egui::Color32::from_rgba_unmultiplied(
                    app.editor.primary_color[0],
                    app.editor.primary_color[1],
                    app.editor.primary_color[2],
                    app.editor.primary_color[3],
                );
                if egui::color_picker::color_edit_button_srgba(
                    ui, &mut color32, egui::color_picker::Alpha::Opaque
                ).changed() {
                    let arr = color32.to_array();
                    app.editor.set_primary_color(arr);
                }
                
                ui.separator();
                
                // Tool-specific options
                match app.editor.active_tool {
                    ToolType::Fill => {
                        ui.label("Tolerance");
                        let mut tol = app.fill_tolerance as i32;
                        if ui.add(egui::Slider::new(&mut tol, 0..=255)).changed() {
                            app.fill_tolerance = tol as u8;
                        }
                        ui.checkbox(&mut app.fill_contiguous, "Contiguous");
                    }
                    ToolType::Rectangle | ToolType::Ellipse => {
                        ui.checkbox(&mut app.shape_filled, "Filled");
                    }
                    _ => {}
                }
            });
        });
}
