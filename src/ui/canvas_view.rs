use crate::app::PixelBuddyApp;
use crate::editor::ToolType;
use crate::tools;
use egui::{Color32, Rect, Sense, Stroke, Vec2, Pos2};

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        
        let painter = ui.painter_at(rect);
        
        // Auto-fit canvas to viewport on open/new/request
        if app.auto_fit_requested && rect.width() > 0.0 && rect.height() > 0.0 {
            let canvas_w = app.editor.document.width as f32;
            let canvas_h = app.editor.document.height as f32;
            let fit_x = (rect.width() * 0.75) / canvas_w;
            let fit_y = (rect.height() * 0.75) / canvas_h;
            app.zoom = fit_x.min(fit_y).clamp(1.0, 64.0);
            app.pan_offset = Vec2::ZERO;
            app.auto_fit_requested = false;
        }
        
        // Handle scroll for zoom (smoother multiplier)
        if response.hovered() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_factor = if scroll_delta > 0.0 { 1.08 } else { 0.925 };
                app.zoom = (app.zoom * zoom_factor).clamp(0.5, 64.0);
            }
        }
        
        // Handle middle drag for panning OR Hand tool primary drag
        if response.dragged_by(egui::PointerButton::Middle)
            || (app.editor.active_tool == ToolType::Hand && response.dragged_by(egui::PointerButton::Primary))
        {
            app.pan_offset += response.drag_delta();
        }

        // Handle Zoom tool click zooming
        if app.editor.active_tool == ToolType::Zoom && response.hovered() {
            if response.clicked_by(egui::PointerButton::Primary) {
                app.zoom = (app.zoom * 1.5).clamp(0.5, 64.0);
            } else if response.clicked_by(egui::PointerButton::Secondary) {
                app.zoom = (app.zoom / 1.5).clamp(0.5, 64.0);
            }
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
        
        // Draw checkerboard background matching exact pixel grid
        let check_size = app.zoom;
        for row in 0..app.editor.document.height {
            for col in 0..app.editor.document.width {
                let color = if (col + row) % 2 == 0 {
                    Color32::from_gray(210)
                } else {
                    Color32::from_gray(170)
                };
                let check_min = Pos2::new(
                    canvas_origin.x + col as f32 * check_size,
                    canvas_origin.y + row as f32 * check_size,
                );
                let check_rect = Rect::from_min_size(check_min, Vec2::splat(check_size));
                painter.rect_filled(check_rect, 0, color);
            }
        }
        
        // Draw Onion Skin ghost overlay (previous/next frame previews)
        if app.editor.animation.onion_skin_enabled && app.editor.animation.frames.len() > 1 {
            let current_idx = app.editor.animation.current_frame_index;
            let frame_count = app.editor.animation.frames.len();
            
            // Previous frame (ghost red tint)
            let prev_idx = if current_idx == 0 { frame_count - 1 } else { current_idx - 1 };
            let prev_canvas = app.editor.animation.frames[prev_idx].document.composite_preview();
            let size = [prev_canvas.width() as usize, prev_canvas.height() as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, prev_canvas.pixels());
            let prev_tex = ctx.load_texture("onion_prev", image, egui::TextureOptions::NEAREST);
            let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
            painter.image(prev_tex.id(), canvas_rect, uv, Color32::from_rgba_unmultiplied(255, 120, 120, 90));

            // Next frame (ghost blue tint)
            let next_idx = (current_idx + 1) % frame_count;
            let next_canvas = app.editor.animation.frames[next_idx].document.composite_preview();
            let next_size = [next_canvas.width() as usize, next_canvas.height() as usize];
            let next_image = egui::ColorImage::from_rgba_unmultiplied(next_size, next_canvas.pixels());
            let next_tex = ctx.load_texture("onion_next", next_image, egui::TextureOptions::NEAREST);
            painter.image(next_tex.id(), canvas_rect, uv, Color32::from_rgba_unmultiplied(120, 180, 255, 90));
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

        // Draw selection marquee overlay box
        if app.editor.selection.active {
            let sel = &app.editor.selection;
            let sel_min = Pos2::new(canvas_origin.x + sel.min_x() as f32 * app.zoom, canvas_origin.y + sel.min_y() as f32 * app.zoom);
            let sel_max = Pos2::new(canvas_origin.x + (sel.max_x() + 1) as f32 * app.zoom, canvas_origin.y + (sel.max_y() + 1) as f32 * app.zoom);
            let sel_rect = Rect::from_min_max(sel_min, sel_max);
            painter.rect_stroke(sel_rect, 0.0, Stroke::new(2.0_f32, Color32::from_rgb(99, 102, 241)), egui::StrokeKind::Outside);
            painter.rect_stroke(sel_rect, 0.0, Stroke::new(1.0_f32, Color32::WHITE), egui::StrokeKind::Inside);
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

                        if app.editor.active_tool == ToolType::Marquee {
                            tools::marquee::update_selection(&mut app.editor.selection, cx, cy, cx, cy);
                        }
                    }
                    
                    // Accumulate stroke / selection drag
                    if app.is_drawing && response.dragged_by(egui::PointerButton::Primary) {
                        match app.editor.active_tool {
                            ToolType::Pencil | ToolType::Eraser => {
                                let last = app.stroke_points.last().copied();
                                if last != Some((ux, uy)) {
                                    app.stroke_points.push((ux, uy));
                                }
                            }
                            ToolType::Marquee => {
                                if let Some((sx, sy)) = app.shape_start {
                                    tools::marquee::update_selection(&mut app.editor.selection, sx, sy, cx, cy);
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
                                ToolType::Move => {
                                    tools::move_tool::move_pixels(&app.editor.document, &app.editor.selection, cx - sx, cy - sy)
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
        
        // Interactive status bar in corner with clickable zoom controls
        egui::Area::new(egui::Id::new("canvas_zoom_overlay"))
            .fixed_pos(Pos2::new(rect.max.x - 220.0, rect.max.y - 32.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{}×{}", app.editor.document.width, app.editor.document.height))
                            .color(Color32::from_white_alpha(180))
                            .size(12.0)
                    );
                    ui.label(egui::RichText::new("|").color(Color32::from_white_alpha(100)));
                    ui.label(egui::RichText::new("Zoom:").color(Color32::from_white_alpha(180)).size(12.0));
                    if ui.small_button("➖").on_hover_text("Zoom Out (-)").clicked() {
                        app.zoom = (app.zoom * 0.85).clamp(0.5, 64.0);
                    }
                    ui.label(
                        egui::RichText::new(format!("{:.0}x", app.zoom))
                            .color(Color32::WHITE)
                            .size(12.0)
                    );
                    if ui.small_button("➕").on_hover_text("Zoom In (+)").clicked() {
                        app.zoom = (app.zoom * 1.18).clamp(0.5, 64.0);
                    }
                });
            });
    });
}
