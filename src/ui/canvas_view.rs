use crate::app::PixelBuddyApp;
use crate::editor::ToolType;
use crate::tools;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

const WHEEL_ZOOM_FACTOR: f32 = 1.06;
const MIN_CANVAS_ZOOM: f32 = 0.5;
const MAX_CANVAS_ZOOM: f32 = 64.0;

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());

        let painter = ui.painter_at(rect);
        let canvas_width = app.editor.document().width;
        let canvas_height = app.editor.document().height;

        // Auto-fit canvas to viewport on open/new/request
        if app.auto_fit_requested && rect.width() > 0.0 && rect.height() > 0.0 {
            let canvas_w = canvas_width as f32;
            let canvas_h = canvas_height as f32;
            let fit_x = (rect.width() * 0.95) / canvas_w;
            let fit_y = (rect.height() * 0.95) / canvas_h;
            app.zoom = fit_x.min(fit_y).clamp(1.0, 64.0);
            app.pan_offset = Vec2::ZERO;
            app.auto_fit_requested = false;
        }

        // A notched wheel's smooth delta is intentionally spread over several
        // repaint frames by egui. Treating every one of those frames as a zoom
        // step made a single wheel tick jump through many zoom levels. Use the
        // raw input instead, with one deliberately small step per event.
        if response.hovered() {
            let wheel_delta = ui.input(|input| {
                // Keep Ctrl/Command scroll available for egui's own zoom
                // gesture instead of treating it as ordinary canvas zoom.
                if input.modifiers.ctrl || input.modifiers.command || input.modifiers.mac_cmd {
                    0.0
                } else {
                    input.raw_scroll_delta.y
                }
            });
            if let Some(zoom) = wheel_zoom(app.zoom, wheel_delta) {
                app.zoom = zoom;
            }
        }

        // Handle middle drag for panning OR Hand tool primary drag
        if response.dragged_by(egui::PointerButton::Middle)
            || (app.editor.active_tool == ToolType::Hand
                && response.dragged_by(egui::PointerButton::Primary))
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

        let canvas_w = canvas_width as f32;
        let canvas_h = canvas_height as f32;
        let display_w = canvas_w * app.zoom;
        let display_h = canvas_h * app.zoom;

        let canvas_origin = Pos2::new(
            rect.center().x - display_w / 2.0 + app.pan_offset.x,
            rect.center().y - display_h / 2.0 + app.pan_offset.y,
        );
        let canvas_rect = Rect::from_min_size(canvas_origin, Vec2::new(display_w, display_h));
        // The app uses the exact painted bounds to position transient UI,
        // such as status notifications, just outside the canvas. Recording
        // this after pan/zoom calculations keeps those elements in sync.
        app.canvas_rect = Some(canvas_rect);

        // Only emit background/grid primitives for pixels visible in the
        // viewport. A large canvas can otherwise create millions of shapes
        // every frame even when most of it is panned off screen.
        let visible_min_col = ((rect.left() - canvas_origin.x) / app.zoom)
            .floor()
            .max(0.0) as u32;
        let visible_max_col = ((rect.right() - canvas_origin.x) / app.zoom)
            .ceil()
            .clamp(0.0, canvas_w) as u32;
        let visible_min_row = ((rect.top() - canvas_origin.y) / app.zoom).floor().max(0.0) as u32;
        let visible_max_row = ((rect.bottom() - canvas_origin.y) / app.zoom)
            .ceil()
            .clamp(0.0, canvas_h) as u32;

        // A single repeating texture replaces the old per-pixel checkerboard
        // draw loop. The UV span is expressed in two-pixel tiles, so each
        // checker cell remains exactly one canvas pixel at every zoom level.
        let checkerboard_uv =
            Rect::from_min_max(Pos2::ZERO, Pos2::new(canvas_w / 2.0, canvas_h / 2.0));
        painter.image(
            app.checkerboard_texture_id(ctx),
            canvas_rect,
            checkerboard_uv,
            Color32::WHITE,
        );

        // Onion skins are composited/uploaded only when their neighboring
        // frame pair changes rather than once per paint.
        if let Some((previous_texture, next_texture)) = app.onion_texture_ids(ctx) {
            let onion_alpha =
                (app.editor.animation.onion_skin_opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
            let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
            painter.image(
                previous_texture,
                canvas_rect,
                uv,
                Color32::from_rgba_unmultiplied(255, 120, 120, onion_alpha),
            );
            painter.image(
                next_texture,
                canvas_rect,
                uv,
                Color32::from_rgba_unmultiplied(120, 180, 255, onion_alpha),
            );
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
            let visible_left = canvas_origin.x + visible_min_col as f32 * app.zoom;
            let visible_right = canvas_origin.x + visible_max_col as f32 * app.zoom;
            let visible_top = canvas_origin.y + visible_min_row as f32 * app.zoom;
            let visible_bottom = canvas_origin.y + visible_max_row as f32 * app.zoom;

            for x in visible_min_col..=visible_max_col {
                let px = canvas_origin.x + (x as f32) * app.zoom;
                painter.line_segment(
                    [Pos2::new(px, visible_top), Pos2::new(px, visible_bottom)],
                    grid_stroke,
                );
            }
            for y in visible_min_row..=visible_max_row {
                let py = canvas_origin.y + (y as f32) * app.zoom;
                painter.line_segment(
                    [Pos2::new(visible_left, py), Pos2::new(visible_right, py)],
                    grid_stroke,
                );
            }
        }

        // Track input independently from hover state. `Response::hover_pos()`
        // becomes `None` once a captured drag leaves the canvas, but the
        // interaction must still receive its eventual release.
        let pointer_pixel = ctx
            .input(|input| input.pointer.interact_pos())
            .filter(|position| rect.contains(*position))
            .and_then(|position| {
                canvas_pixel_at(
                    position,
                    canvas_origin,
                    app.zoom,
                    canvas_width,
                    canvas_height,
                )
            });
        if let Some(pixel) = pointer_pixel {
            app.last_canvas_pixel = Some(pixel);
        }

        let (primary_pressed, primary_released, primary_down) = ctx.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
            )
        });
        if primary_pressed && is_canvas_drag_tool(app.editor.active_tool) {
            if let Some((x, y)) = pointer_pixel {
                app.begin_canvas_action(x, y);
                if app.editor.active_tool == ToolType::Marquee {
                    tools::marquee::update_selection(&mut app.editor.selection, x, y, x, y);
                }
            }
        }
        if app.is_drawing && primary_down {
            if let Some((x, y)) = pointer_pixel {
                update_canvas_action(app, x, y);
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
                    if response.drag_started_by(egui::PointerButton::Primary)
                        && is_canvas_drag_tool(app.editor.active_tool)
                        && !app.is_drawing
                    {
                        app.begin_canvas_action(cx, cy);

                        if app.editor.active_tool == ToolType::Marquee {
                            tools::marquee::update_selection(
                                &mut app.editor.selection,
                                cx,
                                cy,
                                cx,
                                cy,
                            );
                        }
                    }

                    // `pointer_pixel` above handles captured drags as well;
                    // retain this response path for integrations that report
                    // drag position only through the widget response.
                    if app.is_drawing && response.dragged_by(egui::PointerButton::Primary) {
                        update_canvas_action(app, cx, cy);
                    }

                    // Handle drag release — apply the tool action
                    if response.drag_stopped_by(egui::PointerButton::Primary) && app.is_drawing {
                        finish_canvas_action(app, cx, cy, ctx.input(|i| i.modifiers.shift));
                    }

                    if response.clicked_by(egui::PointerButton::Secondary)
                        && app.editor.active_tool == ToolType::Marquee {
                            app.editor.selection.deselect();
                        }

                    // Handle single click for fill, eyedropper, or single-pixel draw
                    if response.clicked_by(egui::PointerButton::Primary) && !app.is_drawing {
                        match app.editor.active_tool {
                            ToolType::Fill => {
                                let changes = {
                                    let canvas = &app.editor.document().active_layer().canvas;
                                    tools::fill::flood_fill(
                                        canvas,
                                        ux,
                                        uy,
                                        color,
                                        app.fill_tolerance,
                                        app.fill_contiguous,
                                    )
                                };
                                app.apply_tool_changes(changes);
                            }
                            ToolType::Eyedropper => {
                                let picked = tools::eyedropper::pick_color(
                                    &app.editor.document().active_layer().canvas,
                                    ux,
                                    uy,
                                );
                                app.editor.set_primary_color(picked);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Finish a press that did not become an egui drag (for example a
        // one-pixel pencil click), or a drag whose release happened outside
        // the canvas. The last in-bounds pixel is the safe fallback endpoint.
        if app.is_drawing {
            if primary_down {
                ctx.request_repaint();
            } else if primary_released {
                let endpoint = drag_endpoint(pointer_pixel, app.last_canvas_pixel, app.shape_start);
                if let Some((x, y)) = endpoint {
                    finish_canvas_action(app, x, y, ctx.input(|i| i.modifiers.shift));
                } else {
                    app.cancel_canvas_action();
                }
            } else {
                // A lost focus or cancelled native pointer event must not
                // leave a stale operation waiting for another release.
                app.cancel_canvas_action();
            }
        }

        // Draw selection after interaction handling so a marquee updates in
        // the same frame rather than one frame later.
        if app.editor.selection.active {
            draw_selection_outline(&painter, app, canvas_origin);
        }

        if app.is_drawing {
            if let (Some(start), Some(end)) = (app.shape_start, app.last_canvas_pixel) {
                draw_canvas_preview(&painter, app, canvas_origin, canvas_rect, start, end, ctx.input(|i| i.modifiers.shift));
            }
        }
    });
}

fn is_canvas_drag_tool(tool: ToolType) -> bool {
    matches!(
        tool,
        ToolType::Pencil
            | ToolType::Eraser
            | ToolType::Marquee
            | ToolType::Move
            | ToolType::Line
            | ToolType::Rectangle
            | ToolType::Ellipse
    )
}

fn wheel_zoom(zoom: f32, raw_wheel_delta: f32) -> Option<f32> {
    let factor = if raw_wheel_delta > 0.0 {
        WHEEL_ZOOM_FACTOR
    } else if raw_wheel_delta < 0.0 {
        1.0 / WHEEL_ZOOM_FACTOR
    } else {
        return None;
    };

    Some((zoom * factor).clamp(MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM))
}

fn canvas_pixel_at(
    position: Pos2,
    canvas_origin: Pos2,
    zoom: f32,
    canvas_width: u32,
    canvas_height: u32,
) -> Option<(i32, i32)> {
    if !zoom.is_finite() || zoom <= 0.0 || canvas_width == 0 || canvas_height == 0 {
        return None;
    }

    let x = ((position.x - canvas_origin.x) / zoom).floor();
    let y = ((position.y - canvas_origin.y) / zoom).floor();
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x >= canvas_width as f32
        || y >= canvas_height as f32
    {
        return None;
    }

    Some((x as i32, y as i32))
}

fn drag_endpoint(
    current: Option<(i32, i32)>,
    last_canvas_pixel: Option<(i32, i32)>,
    start: Option<(i32, i32)>,
) -> Option<(i32, i32)> {
    current.or(last_canvas_pixel).or(start)
}

fn constrain_end_point(start_x: i32, start_y: i32, end_x: i32, end_y: i32, tool: ToolType, shift: bool) -> (i32, i32) {
    if !shift {
        return (end_x, end_y);
    }
    match tool {
        ToolType::Rectangle | ToolType::Ellipse | ToolType::Marquee => {
            let dx = end_x - start_x;
            let dy = end_y - start_y;
            let size = dx.abs().max(dy.abs());
            (start_x + size * dx.signum(), start_y + size * dy.signum())
        }
        ToolType::Line => {
            let dx = end_x - start_x;
            let dy = end_y - start_y;
            if dx.abs() > dy.abs() * 2 {
                (end_x, start_y) // Horizontal
            } else if dy.abs() > dx.abs() * 2 {
                (start_x, end_y) // Vertical
            } else {
                let size = dx.abs().max(dy.abs());
                (start_x + size * dx.signum(), start_y + size * dy.signum()) // 45 deg
            }
        }
        _ => (end_x, end_y),
    }
}

fn update_canvas_action(app: &mut PixelBuddyApp, x: i32, y: i32) {
    app.last_canvas_pixel = Some((x, y));
    match app.editor.active_tool {
        ToolType::Pencil | ToolType::Eraser => {
            let point = (x as u32, y as u32);
            if app.stroke_points.last().copied() != Some(point) {
                app.stroke_points.push(point);
            }
        }
        ToolType::Marquee => {
            if let Some((start_x, start_y)) = app.shape_start {
                tools::marquee::update_selection(&mut app.editor.selection, start_x, start_y, x, y);
            }
        }
        _ => {}
    }
}

fn finish_canvas_action(app: &mut PixelBuddyApp, end_x: i32, end_y: i32, shift: bool) {
    let Some((start_x, start_y)) = app.shape_start else {
        app.cancel_canvas_action();
        return;
    };

    let tool = app.editor.active_tool;
    let (end_x, end_y) = constrain_end_point(start_x, start_y, end_x, end_y, tool, shift);

    if matches!(tool, ToolType::Pencil | ToolType::Eraser) {
        let end_point = (end_x as u32, end_y as u32);
        if app.stroke_points.last().copied() != Some(end_point) {
            app.stroke_points.push(end_point);
        }
    }
    if tool == ToolType::Marquee {
        tools::marquee::update_selection(&mut app.editor.selection, start_x, start_y, end_x, end_y);
    }

    let color = app.editor.primary_color;
    let mut changes = match tool {
        ToolType::Pencil => tools::pencil::draw_stroke(&app.stroke_points, color),
        ToolType::Eraser => tools::eraser::erase_stroke(&app.stroke_points),
        ToolType::Line => tools::line::draw_line(start_x, start_y, end_x, end_y, color),
        ToolType::Rectangle => {
            tools::shape::draw_rectangle(start_x, start_y, end_x, end_y, color, app.shape_filled)
        }
        ToolType::Ellipse => {
            let center_x = (start_x + end_x) / 2;
            let center_y = (start_y + end_y) / 2;
            let radius_x = (end_x - start_x).abs() / 2;
            let radius_y = (end_y - start_y).abs() / 2;
            tools::shape::draw_ellipse(
                center_x,
                center_y,
                radius_x,
                radius_y,
                color,
                app.shape_filled,
            )
        }
        ToolType::Move => tools::move_tool::move_pixels(
            app.editor.document(),
            &app.editor.selection,
            end_x - start_x,
            end_y - start_y,
        ),
        _ => Vec::new(),
    };

    if matches!(tool, ToolType::Pencil | ToolType::Eraser | ToolType::Line | ToolType::Rectangle | ToolType::Ellipse) {
        let size = app.editor.brush_size as u32;
        if size > 1 {
            let mut expanded = Vec::new();
            for (x, y, color) in changes {
                for dy in 0..size {
                    for dx in 0..size {
                        expanded.push((x.saturating_add(dx), y.saturating_add(dy), color));
                    }
                }
            }
            changes = expanded;
        }
    }

    app.apply_tool_changes(changes);
    app.cancel_canvas_action();
}

fn pixel_rect(canvas_origin: Pos2, zoom: f32, x0: i32, y0: i32, x1: i32, y1: i32) -> Rect {
    let min_x = x0.min(x1);
    let max_x = x0.max(x1) + 1;
    let min_y = y0.min(y1);
    let max_y = y0.max(y1) + 1;
    Rect::from_min_max(
        Pos2::new(
            canvas_origin.x + min_x as f32 * zoom,
            canvas_origin.y + min_y as f32 * zoom,
        ),
        Pos2::new(
            canvas_origin.x + max_x as f32 * zoom,
            canvas_origin.y + max_y as f32 * zoom,
        ),
    )
}

fn pixel_center(canvas_origin: Pos2, zoom: f32, x: i32, y: i32) -> Pos2 {
    Pos2::new(
        canvas_origin.x + (x as f32 + 0.5) * zoom,
        canvas_origin.y + (y as f32 + 0.5) * zoom,
    )
}

fn draw_selection_outline(painter: &egui::Painter, app: &PixelBuddyApp, canvas_origin: Pos2) {
    let selection = &app.editor.selection;
    let selection_rect = pixel_rect(
        canvas_origin,
        app.zoom,
        selection.min_x(),
        selection.min_y(),
        selection.max_x(),
        selection.max_y(),
    );
    painter.rect_stroke(
        selection_rect,
        0.0,
        Stroke::new(2.0_f32, Color32::from_rgb(99, 102, 241)),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        selection_rect,
        0.0,
        Stroke::new(1.0_f32, Color32::WHITE),
        egui::StrokeKind::Inside,
    );
}

fn draw_canvas_preview(
    painter: &egui::Painter,
    app: &PixelBuddyApp,
    canvas_origin: Pos2,
    canvas_rect: Rect,
    start: (i32, i32),
    end: (i32, i32),
    shift: bool,
) {
    let (end_x, end_y) = constrain_end_point(start.0, start.1, end.0, end.1, app.editor.active_tool, shift);
    let end = (end_x, end_y);
    let primary = app.editor.primary_color;
    let preview_color = Color32::from_rgba_unmultiplied(
        primary[0],
        primary[1],
        primary[2],
        primary[3].clamp(96, 180),
    );
    let eraser_color = Color32::from_rgba_unmultiplied(244, 63, 94, 160);
    let stroke = Stroke::new((app.zoom * 0.8).max(1.0), preview_color);

    match app.editor.active_tool {
        ToolType::Pencil | ToolType::Eraser => {
            let color = if app.editor.active_tool == ToolType::Eraser {
                eraser_color
            } else {
                preview_color
            };
            let stroke = Stroke::new((app.zoom * 0.8).max(1.0), color);
            let size = app.editor.brush_size as i32;
            
            if app.stroke_points.len() == 1 {
                let (x, y) = app.stroke_points[0];
                painter.rect_filled(
                    pixel_rect(
                        canvas_origin,
                        app.zoom,
                        x as i32,
                        y as i32,
                        (x as i32) + size - 1,
                        (y as i32) + size - 1,
                    ),
                    0.0,
                    color,
                );
            }
            for points in app.stroke_points.windows(2) {
                let (x0, y0) = points[0];
                let (x1, y1) = points[1];
                let (cx0, cy0) = (x0 as i32, y0 as i32);
                let (cx1, cy1) = (x1 as i32, y1 as i32);
                
                // For thickness > 1 in preview, just draw a filled polygon or multiple lines.
                // An approximation for preview is to just draw lines with a thicker stroke.
                let thick_stroke = Stroke::new(stroke.width.max(size as f32 * app.zoom), color);
                painter.line_segment(
                    [
                        pixel_center(canvas_origin, app.zoom, cx0 + size / 2, cy0 + size / 2),
                        pixel_center(canvas_origin, app.zoom, cx1 + size / 2, cy1 + size / 2),
                    ],
                    thick_stroke,
                );
            }
        }
        ToolType::Line => {
            painter.line_segment(
                [
                    pixel_center(canvas_origin, app.zoom, start.0, start.1),
                    pixel_center(canvas_origin, app.zoom, end.0, end.1),
                ],
                stroke,
            );
        }
        ToolType::Rectangle => {
            let preview_rect = pixel_rect(canvas_origin, app.zoom, start.0, start.1, end.0, end.1);
            if app.shape_filled {
                painter.rect_filled(
                    preview_rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(
                        primary[0],
                        primary[1],
                        primary[2],
                        primary[3].clamp(48, 96),
                    ),
                );
            }
            painter.rect_stroke(preview_rect, 0.0, stroke, egui::StrokeKind::Inside);
        }
        ToolType::Ellipse => {
            let center_x = (start.0 + end.0) / 2;
            let center_y = (start.1 + end.1) / 2;
            let radius_x = (end.0 - start.0).abs() / 2;
            let radius_y = (end.1 - start.1).abs() / 2;
            let center = pixel_center(canvas_origin, app.zoom, center_x, center_y);
            let radius = Vec2::new(
                (radius_x as f32 + 0.5) * app.zoom,
                (radius_y as f32 + 0.5) * app.zoom,
            );
            if app.shape_filled {
                painter.add(egui::Shape::ellipse_filled(
                    center,
                    radius,
                    Color32::from_rgba_unmultiplied(
                        primary[0],
                        primary[1],
                        primary[2],
                        primary[3].clamp(48, 96),
                    ),
                ));
            }
            painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
        }
        ToolType::Move => {
            let source = if app.editor.selection.active {
                let selection = &app.editor.selection;
                pixel_rect(
                    canvas_origin,
                    app.zoom,
                    selection.min_x(),
                    selection.min_y(),
                    selection.max_x(),
                    selection.max_y(),
                )
            } else {
                canvas_rect
            };
            let delta = Vec2::new(
                (end.0 - start.0) as f32 * app.zoom,
                (end.1 - start.1) as f32 * app.zoom,
            );
            let destination = source.translate(delta);
            painter.rect_filled(
                destination,
                0.0,
                Color32::from_rgba_unmultiplied(99, 102, 241, 45),
            );
            painter.rect_stroke(
                destination,
                0.0,
                Stroke::new(2.0_f32, Color32::from_rgb(99, 102, 241)),
                egui::StrokeKind::Outside,
            );
            painter.line_segment(
                [source.center(), destination.center()],
                Stroke::new(1.0_f32, Color32::from_white_alpha(150)),
            );
        }
        ToolType::Marquee
        | ToolType::Hand
        | ToolType::Zoom
        | ToolType::Fill
        | ToolType::Eyedropper => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{canvas_pixel_at, drag_endpoint, wheel_zoom};
    use egui::Pos2;

    #[test]
    fn canvas_pixel_mapping_rejects_positions_outside_the_canvas() {
        let origin = Pos2::new(10.0, 20.0);
        assert_eq!(
            canvas_pixel_at(Pos2::new(14.1, 24.1), origin, 4.0, 3, 2),
            Some((1, 1))
        );
        assert_eq!(
            canvas_pixel_at(Pos2::new(22.0, 20.0), origin, 4.0, 3, 2),
            None
        );
        assert_eq!(
            canvas_pixel_at(Pos2::new(10.0, 20.0), origin, 0.0, 3, 2),
            None
        );
    }

    #[test]
    fn release_outside_canvas_uses_the_last_valid_drag_pixel() {
        assert_eq!(
            drag_endpoint(None, Some((7, 3)), Some((1, 1))),
            Some((7, 3))
        );
        assert_eq!(drag_endpoint(None, None, Some((1, 1))), Some((1, 1)));
    }

    #[test]
    fn wheel_zoom_is_a_single_gentle_step_even_for_large_raw_deltas() {
        let single_line = wheel_zoom(10.0, 1.0).expect("a non-zero wheel delta zooms");
        let large_wheel_notch = wheel_zoom(10.0, 120.0).expect("a wheel notch zooms");

        assert!((single_line - large_wheel_notch).abs() < f32::EPSILON);
        assert!(single_line > 10.0);
        assert!(single_line < 11.0);

        let restored = wheel_zoom(single_line, -120.0).expect("reverse scroll zooms out");
        assert!((restored - 10.0).abs() < 0.000_1);
    }

    #[test]
    fn wheel_zoom_clamps_and_ignores_zero_delta() {
        assert_eq!(wheel_zoom(64.0, 1.0), Some(64.0));
        assert_eq!(wheel_zoom(0.5, -1.0), Some(0.5));
        assert_eq!(wheel_zoom(10.0, 0.0), None);
    }
}
