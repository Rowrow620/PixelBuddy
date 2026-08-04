use egui::{Color32, Pos2, Rect, Stroke, Vec2};
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
                    (ToolType::Marquee, "Marquee Selection (M)"),
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
                    
                    draw_monochrome_icon(ui.painter(), rect, tool, icon_color);

                    if response.on_hover_text(tooltip).clicked() {
                        app.editor.set_active_tool(tool);
                    }
                    ui.add_space(2.0);
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

fn draw_monochrome_icon(painter: &egui::Painter, rect: Rect, tool: ToolType, color: Color32) {
    let center = rect.center();
    let stroke = Stroke::new(1.5_f32, color);

    match tool {
        ToolType::Hand => {
            // 4-way directional arrow compass (matching Move icon style)
            let r = 7.0_f32;
            painter.line_segment([Pos2::new(center.x, center.y - r), Pos2::new(center.x, center.y + r)], stroke);
            painter.line_segment([Pos2::new(center.x - r, center.y), Pos2::new(center.x + r, center.y)], stroke);
            // Up arrow tip
            painter.line_segment([Pos2::new(center.x - 2.5, center.y - r + 2.5), Pos2::new(center.x, center.y - r)], stroke);
            painter.line_segment([Pos2::new(center.x + 2.5, center.y - r + 2.5), Pos2::new(center.x, center.y - r)], stroke);
            // Down arrow tip
            painter.line_segment([Pos2::new(center.x - 2.5, center.y + r - 2.5), Pos2::new(center.x, center.y + r)], stroke);
            painter.line_segment([Pos2::new(center.x + 2.5, center.y + r - 2.5), Pos2::new(center.x, center.y + r)], stroke);
            // Left arrow tip
            painter.line_segment([Pos2::new(center.x - r + 2.5, center.y - 2.5), Pos2::new(center.x - r, center.y)], stroke);
            painter.line_segment([Pos2::new(center.x - r + 2.5, center.y + 2.5), Pos2::new(center.x - r, center.y)], stroke);
            // Right arrow tip
            painter.line_segment([Pos2::new(center.x + r - 2.5, center.y - 2.5), Pos2::new(center.x + r, center.y)], stroke);
            painter.line_segment([Pos2::new(center.x + r - 2.5, center.y + 2.5), Pos2::new(center.x + r, center.y)], stroke);
        }
        ToolType::Zoom => {
            // Magnifying Glass
            let lens_center = Pos2::new(center.x - 2.0, center.y - 2.0);
            painter.circle_stroke(lens_center, 5.0, stroke);
            painter.line_segment([Pos2::new(center.x + 1.5, center.y + 1.5), Pos2::new(center.x + 6.5, center.y + 6.5)], stroke);
        }
        ToolType::Marquee => {
            // Dashed Marquee Rectangle
            let box_rect = Rect::from_center_size(center, Vec2::new(12.0, 12.0));
            painter.rect_stroke(box_rect, 0, Stroke::new(1.0_f32, color), egui::StrokeKind::Middle);
            // Inner corner accents
            painter.circle_filled(Pos2::new(center.x - 4.0, center.y - 4.0), 1.0, color);
            painter.circle_filled(Pos2::new(center.x + 4.0, center.y + 4.0), 1.0, color);
        }
        ToolType::Move => {
            // Solid Move Arrow Cross
            let r = 6.0_f32;
            painter.line_segment([Pos2::new(center.x, center.y - r), Pos2::new(center.x, center.y + r)], stroke);
            painter.line_segment([Pos2::new(center.x - r, center.y), Pos2::new(center.x + r, center.y)], stroke);
            // Arrow heads
            painter.line_segment([Pos2::new(center.x - 2.0, center.y - r + 2.0), Pos2::new(center.x, center.y - r)], stroke);
            painter.line_segment([Pos2::new(center.x + 2.0, center.y - r + 2.0), Pos2::new(center.x, center.y - r)], stroke);
            painter.line_segment([Pos2::new(center.x - 2.0, center.y + r - 2.0), Pos2::new(center.x, center.y + r)], stroke);
            painter.line_segment([Pos2::new(center.x + 2.0, center.y + r - 2.0), Pos2::new(center.x, center.y + r)], stroke);
        }
        ToolType::Pencil => {
            // Slanted Pencil
            painter.line_segment([Pos2::new(center.x - 5.0, center.y + 5.0), Pos2::new(center.x + 4.0, center.y - 4.0)], stroke);
            painter.line_segment([Pos2::new(center.x - 5.0, center.y + 5.0), Pos2::new(center.x - 7.0, center.y + 7.0)], stroke);
            painter.line_segment([Pos2::new(center.x + 2.0, center.y - 6.0), Pos2::new(center.x + 6.0, center.y - 2.0)], stroke);
        }
        ToolType::Eraser => {
            // Block Eraser
            let eraser_rect = Rect::from_center_size(center, Vec2::new(12.0, 10.0));
            painter.rect_stroke(eraser_rect, 2, stroke, egui::StrokeKind::Middle);
            painter.line_segment([Pos2::new(center.x - 1.0, center.y - 5.0), Pos2::new(center.x - 1.0, center.y + 5.0)], stroke);
        }
        ToolType::Line => {
            // Diagonal Line
            painter.line_segment([Pos2::new(center.x - 6.0, center.y + 6.0), Pos2::new(center.x + 6.0, center.y - 6.0)], stroke);
        }
        ToolType::Rectangle => {
            // Box
            let box_rect = Rect::from_center_size(center, Vec2::new(12.0, 12.0));
            painter.rect_stroke(box_rect, 0, stroke, egui::StrokeKind::Middle);
        }
        ToolType::Ellipse => {
            // Circle
            painter.circle_stroke(center, 6.0, stroke);
        }
        ToolType::Fill => {
            // Paint Bucket / Fill target
            let bucket_rect = Rect::from_center_size(Pos2::new(center.x - 1.0, center.y - 1.0), Vec2::new(10.0, 10.0));
            painter.rect_stroke(bucket_rect, 1, stroke, egui::StrokeKind::Middle);
            painter.circle_filled(Pos2::new(center.x + 5.0, center.y + 5.0), 2.5, color);
        }
        ToolType::Eyedropper => {
            // Pipette sampler
            painter.circle_stroke(center, 4.0, stroke);
            painter.line_segment([Pos2::new(center.x + 3.0, center.y + 3.0), Pos2::new(center.x + 7.0, center.y + 7.0)], stroke);
        }
    }
}
