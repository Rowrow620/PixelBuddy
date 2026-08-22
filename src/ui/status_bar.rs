use crate::app::{PixelBuddyApp, MAX_CANVAS_ZOOM, MIN_CANVAS_ZOOM};
use egui::{Color32, RichText, Widget};

fn zoom_label(zoom: f32) -> String {
    if zoom >= 1.0 {
        format!("{zoom:.1}x")
    } else if zoom >= 0.01 {
        format!("{:.1}%", zoom * 100.0)
    } else {
        format!("{:.2}%", zoom * 100.0)
    }
}

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

            ui.horizontal_wrapped(|ui| {
                let (cx, cy) = if let Some(pos) = app.last_canvas_pixel {
                    (pos.0, pos.1)
                } else {
                    (0, 0)
                };

                let doc = app.editor.document();

                ui.label(
                    RichText::new(format!("Cursor: ({cx}, {cy})"))
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

                // Frame info if playing animation
                let frame_index = app.editor.animation.current_frame_index;
                let frame_count = app.editor.animation.frames.len();
                if frame_count > 1 {
                    ui.label(RichText::new("|").color(Color32::from_white_alpha(100)));
                    ui.label(
                        RichText::new(format!("Frame {}/{}", frame_index + 1, frame_count))
                            .color(Color32::from_white_alpha(180))
                            .size(12.0),
                    );
                }

                ui.label(RichText::new("|").color(Color32::from_white_alpha(100)));

                ui.label(
                    RichText::new("Zoom:")
                        .color(Color32::from_white_alpha(180))
                        .size(12.0),
                );

                if egui::Button::new("➖")
                    .frame(false)
                    .ui(ui)
                    .on_hover_text("Zoom Out")
                    .clicked()
                {
                    app.zoom = (app.zoom / 1.18).clamp(MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM);
                }

                ui.label(
                    RichText::new(zoom_label(app.zoom))
                        .color(Color32::from_white_alpha(180))
                        .size(12.0),
                );

                if egui::Button::new("➕")
                    .frame(false)
                    .ui(ui)
                    .on_hover_text("Zoom In")
                    .clicked()
                {
                    app.zoom = (app.zoom * 1.18).clamp(MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM);
                }
            });
        });
}

fn ui_bg_color(ctx: &egui::Context) -> Color32 {
    ctx.style().visuals.window_fill
}

#[cfg(test)]
mod tests {
    use super::zoom_label;

    #[test]
    fn zoom_label_keeps_low_zoom_steps_visible() {
        assert_eq!(zoom_label(2.0), "2.0x");
        assert_eq!(zoom_label(0.5), "50.0%");
        assert_eq!(zoom_label(0.00464), "0.46%");
        assert_eq!(zoom_label(0.00548), "0.55%");
        assert_eq!(zoom_label(0.001), "0.10%");
    }
}
