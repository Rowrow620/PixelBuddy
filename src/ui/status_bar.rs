use crate::app::PixelBuddyApp;
use egui::{Color32, RichText, Widget};

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::TopBottomPanel::bottom("status_bar")
        .frame(egui::Frame::NONE.fill(ui_bg_color(ctx)).inner_margin(4.0))
        .show_separator_line(false)
        .show(ctx, |ui| {
            // Draw top separator manually for precise color control
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.min.x..=rect.max.x,
                rect.min.y,
                egui::Stroke::new(1.0_f32, crate::ui::theme::SEPARATOR_COLOR),
            );

            ui.horizontal(|ui| {
                // Left: Cursor and Canvas Info
                let (cx, cy) = if let Some(pos) = app.last_canvas_pixel {
                    (pos.0, pos.1)
                } else {
                    (0, 0)
                };

                let doc = app.editor.document();

                ui.label(
                    RichText::new(format!("Cursor: ({}, {})", cx, cy))
                        .color(Color32::from_white_alpha(180))
                        .size(12.0),
                );
                ui.label(RichText::new("|").color(Color32::from_white_alpha(100)));
                ui.label(
                    RichText::new(format!("{}×{}", doc.width, doc.height))
                        .color(Color32::from_white_alpha(180))
                        .size(12.0),
                );

                ui.label(RichText::new("|").color(Color32::from_white_alpha(100)));

                // Active tool name
                let tool_name = match app.editor.active_tool {
                    crate::editor::ToolType::Hand => "Hand (H)",
                    crate::editor::ToolType::Zoom => "Zoom (Z)",
                    crate::editor::ToolType::Marquee => "Marquee (M)",
                    crate::editor::ToolType::Move => "Move (V)",
                    crate::editor::ToolType::Pencil => "Pencil (B)",
                    crate::editor::ToolType::Eraser => "Eraser (E)",
                    crate::editor::ToolType::Line => "Line (L)",
                    crate::editor::ToolType::Rectangle => "Rectangle (R)",
                    crate::editor::ToolType::Ellipse => "Ellipse (O)",
                    crate::editor::ToolType::Fill => "Fill (G)",
                    crate::editor::ToolType::Eyedropper => "Eyedropper (I)",
                };
                ui.label(
                    RichText::new(tool_name)
                        .color(Color32::from_white_alpha(180))
                        .size(12.0),
                );

                ui.label(RichText::new("|").color(Color32::from_white_alpha(100)));

                // Active layer
                let active_layer = doc.active_layer_index;
                let layer_name = &doc.layers[active_layer].name;
                ui.label(
                    RichText::new(format!(
                        "Layer: \"{}\" ({}/{})",
                        layer_name,
                        active_layer + 1,
                        doc.layers.len()
                    ))
                    .color(Color32::from_white_alpha(180))
                    .size(12.0),
                );

                if app.editor.animation.frames.len() > 1 {
                    ui.label(RichText::new("|").color(Color32::from_white_alpha(100)));
                    ui.label(
                        RichText::new(format!(
                            "Frame {}/{}",
                            app.editor.animation.current_frame_index + 1,
                            app.editor.animation.frames.len()
                        ))
                        .color(Color32::from_white_alpha(180))
                        .size(12.0),
                    );
                }

                // Right side: Zoom controls
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if egui::Button::new("➕")
                        .frame(false)
                        .ui(ui)
                        .on_hover_text("Zoom In (+)")
                        .clicked()
                    {
                        app.zoom = (app.zoom * 1.18).clamp(0.5, 64.0);
                    }
                    ui.label(
                        RichText::new(format!("{:.0}x", app.zoom))
                            .color(Color32::WHITE)
                            .size(12.0),
                    );
                    if egui::Button::new("➖")
                        .frame(false)
                        .ui(ui)
                        .on_hover_text("Zoom Out (-)")
                        .clicked()
                    {
                        app.zoom = (app.zoom * 0.85).clamp(0.5, 64.0);
                    }
                    ui.label(
                        RichText::new("Zoom:")
                            .color(Color32::from_white_alpha(180))
                            .size(12.0),
                    );
                });
            });
        });
}

fn ui_bg_color(ctx: &egui::Context) -> Color32 {
    ctx.style().visuals.window_fill
}
