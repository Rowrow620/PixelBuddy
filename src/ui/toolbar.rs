use egui::{Color32, Rect, Stroke, Vec2};
use crate::app::PixelBuddyApp;
use crate::editor::ToolType;

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::SidePanel::left("toolbar")
        .exact_width(52.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                
                let tools: &[(ToolType, &str)] = &[
                    (ToolType::Hand, "Hand / Pan (H)"),
                    (ToolType::Zoom, "Zoom In/Out (Z)"),
                    (ToolType::Marquee, "Marquee Selection (M)\nRight-click or Ctrl+D to deselect"),
                    (ToolType::Move, "Move Tool (V)"),
                    (ToolType::Pencil, "Pencil / Brush (B)"),
                    (ToolType::Eraser, "Eraser (E)"),
                    (ToolType::Line, "Line (L)"),
                    (ToolType::Rectangle, "Rectangle (R)"),
                    (ToolType::Ellipse, "Ellipse / Circle (O)"),
                    (ToolType::Fill, "Flood Fill (G)"),
                    (ToolType::Eyedropper, "Eyedropper (I)"),
                ];
                
                for &(tool, tooltip) in tools {
                    let is_active = app.editor.active_tool == tool;
                    let stroke = if is_active {
                        Stroke::new(2.0_f32, ui.visuals().selection.bg_fill)
                    } else {
                        Stroke::NONE
                    };
                    
                    let (rect, response) = ui.allocate_exact_size(Vec2::new(36.0, 32.0), egui::Sense::click());
                    
                    let bg_color = if response.hovered() {
                        Color32::from_rgb(50, 50, 72)
                    } else {
                        Color32::from_rgb(38, 38, 56)
                    };
                    
                    ui.painter().rect_filled(rect, 4, bg_color);
                    if is_active {
                        ui.painter().rect_stroke(rect, 4, stroke, egui::StrokeKind::Inside);
                    }
                    
                    let icon_color = if is_active {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(210)
                    };
                    
                    draw_monochrome_icon(ui, rect, tool, icon_color);

                    if response.on_hover_text(tooltip).clicked() {
                        app.editor.set_active_tool(tool);
                    }
                    ui.add_space(2.0);
                }
            });
        });
}

fn draw_monochrome_icon(ui: &mut egui::Ui, rect: Rect, tool: ToolType, color: Color32) {
    let img = match tool {
        ToolType::Hand => egui::include_image!("../../assets/icons/hand.svg"),
        ToolType::Zoom => egui::include_image!("../../assets/icons/zoom.svg"),
        ToolType::Marquee => egui::include_image!("../../assets/icons/marquee.svg"),
        ToolType::Move => egui::include_image!("../../assets/icons/move.svg"),
        ToolType::Pencil => egui::include_image!("../../assets/icons/pencil.svg"),
        ToolType::Eraser => egui::include_image!("../../assets/icons/eraser.svg"),
        ToolType::Line => egui::include_image!("../../assets/icons/line.svg"),
        ToolType::Rectangle => egui::include_image!("../../assets/icons/rectangle.svg"),
        ToolType::Ellipse => egui::include_image!("../../assets/icons/ellipse.svg"),
        ToolType::Fill => egui::include_image!("../../assets/icons/fill.svg"),
        ToolType::Eyedropper => egui::include_image!("../../assets/icons/eyedropper.svg"),
    };

    let icon_rect = rect.shrink(6.0);
    let image = egui::Image::new(img).tint(color);
    image.paint_at(ui, icon_rect);
}
