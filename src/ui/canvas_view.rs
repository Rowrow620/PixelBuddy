use crate::app::PixelBuddyApp;
use crate::editor::ToolType;
use crate::tools;
use egui::{Color32, Rect, Sense, Stroke, Vec2, Pos2};

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        
        let painter = ui.painter_at(rect);
        
        // Handle scroll for zoom
        if response.hovered() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_factor = if scroll_delta > 0.0 { 1.25 } else { 0.8 };
                app.zoom = (app.zoom * zoom_factor).clamp(0.5, 64.0);
            }
        }
        
        // Handle middle drag for panning
        if response.dragged_by(egui::PointerButton::Middle) {
            app.pan_offset += response.drag_delta();
        }

        let canvas_w = app.editor.document.width as f32;
        let canvas_h = app.editor.document.height as f32;
        let display_w = canvas_w * app.zoom;
        let display_h = canvas_h * app.zoom;
        
        let canvas_origin = Pos2::new(
            rect.center().x - display_w / 2.0 + app.pan_offset.x,
            rect.center().y - display_h / 2.0 + app.pan_offset.y,
        );
        let canvas_rect = Rect::from_min_size(canvas_origin, Vec2::new(display_w, display_h));
        
        // Draw checkerboard background for transparency
        let check_size = (8.0 * app.zoom).max(8.0).min(20.0);
        let cols = (display_w / check_size).ceil() as i32;
        let rows = (display_h / check_size).ceil() as i32;
        for row in 0..rows {
            for col in 0..cols {
                let color = if (col + row) % 2 == 0 {
                    Color32::from_gray(200)
                } else {
                    Color32::from_gray(160)
                };
                let check_min = Pos2::new(
                    canvas_origin.x + col as f32 * check_size,
                    canvas_origin.y + row as f32 * check_size,
                );
                let check_rect = Rect::from_min_size(check_min, Vec2::splat(check_size));
                let clipped = check_rect.intersect(canvas_rect);
                if clipped.width() > 0.0 && clipped.height() > 0.0 {
                    painter.rect_filled(clipped, 0, color);
                }
            }
        }
        
        // Draw composited canvas image
        if let Some(texture) = &app.canvas_texture {
            let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
            painter.image(texture.id(), canvas_rect, uv, Color32::WHITE);
        }
        
        // Draw pixel grid when zoomed in enough
        if app.show_grid && app.zoom >= 4.0 {
            let grid_color = Color32::from_rgba_unmultiplied(255, 255, 255, 30);
            let grid_stroke = Stroke::new(1.0_f32, grid_color);
            for x in 0..=app.editor.document.width {
                let px = canvas_origin.x + (x as f32) * app.zoom;
                painter.line_segment(
                    [Pos2::new(px, canvas_origin.y), Pos2::new(px, canvas_origin.y + display_h)],
                    grid_stroke,
                );
            }
            for y in 0..=app.editor.document.height {
                let py = canvas_origin.y + (y as f32) * app.zoom;
                painter.line_segment(
                    [Pos2::new(canvas_origin.x, py), Pos2::new(canvas_origin.x + display_w, py)],
                    grid_stroke,
                );
            }
        }

        // Mouse interaction with canvas
        if let Some(pointer_pos) = response.hover_pos() {
            if canvas_rect.contains(pointer_pos) {
                let cx = ((pointer_pos.x - canvas_origin.x) / app.zoom).floor() as i32;
                let cy = ((pointer_pos.y - canvas_origin.y) / app.zoom).floor() as i32;
                
                if cx >= 0 && cx < canvas_w as i32 && cy >= 0 && cy < canvas_h as i32 {
                    // Highlight current pixel
                    let highlight_rect = Rect::from_min_size(
                        Pos2::new(
                            canvas_origin.x + cx as f32 * app.zoom,
                            canvas_origin.y + cy as f32 * app.zoom,
                        ),
                        Vec2::splat(app.zoom),
                    );
                    painter.rect_filled(highlight_rect, 0, Color32::from_white_alpha(40));

                    let color = app.editor.primary_color;
                    let ux = cx as u32;
                    let uy = cy as u32;

                    // Handle drag start
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        app.is_drawing = true;
                        app.stroke_points.clear();
                        app.stroke_points.push((ux, uy));
                        app.shape_start = Some((cx, cy));
                    }
                    
                    // Accumulate stroke points during drag
                    if app.is_drawing && response.dragged_by(egui::PointerButton::Primary) {
                        match app.editor.active_tool {
                            ToolType::Pencil | ToolType::Eraser => {
                                let last = app.stroke_points.last().copied();
                                if last != Some((ux, uy)) {
                                    app.stroke_points.push((ux, uy));
                                }
                            }
                            _ => {}
                        }
                    }

                    // Handle drag release — apply the tool action
                    if response.drag_stopped_by(egui::PointerButton::Primary) && app.is_drawing {
                        if let Some((sx, sy)) = app.shape_start {
                            let changes = match app.editor.active_tool {
                                ToolType::Pencil => tools::pencil::draw_stroke(&app.stroke_points, color),
                                ToolType::Eraser => tools::eraser::erase_stroke(&app.stroke_points),
                                ToolType::Line => tools::line::draw_line(sx, sy, cx, cy, color),
                                ToolType::Rectangle => tools::shape::draw_rectangle(sx, sy, cx, cy, color, app.shape_filled),
                                ToolType::Ellipse => {
                                    let ecx = (sx + cx) / 2;
                                    let ecy = (sy + cy) / 2;
                                    let rx = (cx - sx).abs() / 2;
                                    let ry = (cy - sy).abs() / 2;
                                    tools::shape::draw_ellipse(ecx, ecy, rx, ry, color, app.shape_filled)
                                }
                                _ => vec![],
                            };
                            app.apply_tool_changes(changes);
                        }
                        app.is_drawing = false;
                        app.shape_start = None;
                        app.stroke_points.clear();
                    }

                    // Handle single click for fill, eyedropper, or single-pixel draw
                    if response.clicked_by(egui::PointerButton::Primary) && !app.is_drawing {
                        match app.editor.active_tool {
                            ToolType::Fill => {
                                let changes = tools::fill::flood_fill(
                                    &app.editor.document.active_layer().canvas,
                                    ux, uy, color,
                                    app.fill_tolerance,
                                    app.fill_contiguous,
                                );
                                app.apply_tool_changes(changes);
                            }
                            ToolType::Eyedropper => {
                                let picked = tools::eyedropper::pick_color(
                                    &app.editor.document.active_layer().canvas,
                                    ux, uy,
                                );
                                app.editor.set_primary_color(picked);
                            }
                            ToolType::Pencil => {
                                let changes = tools::pencil::draw_pixel(ux, uy, color);
                                app.apply_tool_changes(changes);
                            }
                            ToolType::Eraser => {
                                let changes = tools::eraser::erase_pixel(ux, uy);
                                app.apply_tool_changes(changes);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        
        // Status bar text in corner
        let status_text = format!(
            "{}×{} | Zoom: {:.0}x",
            app.editor.document.width, app.editor.document.height, app.zoom
        );
        painter.text(
            Pos2::new(rect.max.x - 10.0, rect.max.y - 10.0),
            egui::Align2::RIGHT_BOTTOM,
            status_text,
            egui::FontId::proportional(13.0),
            Color32::from_white_alpha(180),
        );
    });
}
