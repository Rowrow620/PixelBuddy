use crate::app::{PixelBuddyApp, TileMode, MAX_CANVAS_ZOOM, MIN_CANVAS_ZOOM};
use crate::editor::ToolType;
use crate::tools;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

const WHEEL_ZOOM_FACTOR: f32 = 1.06;

mod tile_layout;
use tile_layout::*;

fn prepare_canvas_input(app: &mut PixelBuddyApp) -> bool {
    if app.canvas_input_blocked {
        if app.is_drawing {
            app.cancel_canvas_action();
        }
        false
    } else {
        true
    }
}

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let mut available_rect = ui.available_rect_before_wrap();
        let ruler_size = 20.0;

        let mut top_ruler_rect = None;
        let mut left_ruler_rect = None;
        let mut top_ruler_resp = None;
        let mut left_ruler_resp = None;

        if app.show_rulers {
            top_ruler_rect = Some(Rect::from_min_size(
                Pos2::new(available_rect.min.x + ruler_size, available_rect.min.y),
                Vec2::new(available_rect.width() - ruler_size, ruler_size),
            ));
            left_ruler_rect = Some(Rect::from_min_size(
                Pos2::new(available_rect.min.x, available_rect.min.y + ruler_size),
                Vec2::new(ruler_size, available_rect.height() - ruler_size),
            ));

            top_ruler_resp = Some(ui.interact(
                top_ruler_rect.unwrap(),
                ui.id().with("top_ruler"),
                Sense::click_and_drag(),
            ));
            left_ruler_resp = Some(ui.interact(
                left_ruler_rect.unwrap(),
                ui.id().with("left_ruler"),
                Sense::click_and_drag(),
            ));

            available_rect.min.x += ruler_size;
            available_rect.min.y += ruler_size;
        }

        let rect = available_rect;
        let response = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
        let canvas_input_enabled = prepare_canvas_input(app);
        ui.allocate_rect(ui.available_rect_before_wrap(), Sense::hover());

        let painter = ui.painter().with_clip_rect(rect);
        let canvas_width = app.editor.document().width;
        let canvas_height = app.editor.document().height;
        let canvas_w = canvas_width as f32;
        let canvas_h = canvas_height as f32;
        let tile_layout = TileLayout::new(app.tile_mode, app.tile_preview);

        if app.fit_tile_preview_requested {
            if let Some(zoom) =
                tile_preview_fit_zoom(rect, canvas_width, canvas_height, tile_layout)
            {
                app.zoom = zoom;
                app.pan_offset = Vec2::ZERO;
                app.tile_preview_fit_active = true;
            }
            app.fit_tile_preview_requested = false;
            app.auto_fit_requested = false;
        } else if app.auto_fit_requested && rect.width() > 0.0 && rect.height() > 0.0 {
            // Ordinary document lifecycle fitting remains source-canvas based.
            let source_layout = TileLayout::new(TileMode::None, app.tile_preview);
            if let Some(zoom) =
                tile_preview_fit_zoom(rect, canvas_width, canvas_height, source_layout)
            {
                app.zoom = zoom;
            }
            app.pan_offset = Vec2::ZERO;
            app.tile_preview_fit_active = false;
            app.auto_fit_requested = false;
        }

        // A notched wheel's smooth delta is intentionally spread over several
        // repaint frames by egui. Treating every one of those frames as a zoom
        // step made a single wheel tick jump through many zoom levels. Use the
        // raw input instead, with one deliberately small step per event.
        if canvas_input_enabled && response.hovered() {
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
        if canvas_input_enabled
            && (response.dragged_by(egui::PointerButton::Middle)
                || (app.editor.active_tool == ToolType::Hand
                    && response.dragged_by(egui::PointerButton::Primary)))
        {
            app.pan_offset += response.drag_delta();
        }

        // Handle Zoom tool click zooming
        if canvas_input_enabled && app.editor.active_tool == ToolType::Zoom && response.hovered() {
            if response.clicked_by(egui::PointerButton::Primary) {
                app.zoom = (app.zoom * 1.5).clamp(MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM);
            } else if response.clicked_by(egui::PointerButton::Secondary) {
                app.zoom = (app.zoom / 1.5).clamp(MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM);
            }
        }

        let display_w = canvas_w * app.zoom;
        let display_h = canvas_h * app.zoom;
        let tile_size = Vec2::new(display_w, display_h);
        let canvas_origin = canvas_origin_for_layout(
            tile_layout,
            rect,
            tile_size,
            app.pan_offset,
            app.tile_preview_fit_active,
        );
        let canvas_rect = Rect::from_min_size(canvas_origin, Vec2::new(display_w, display_h));
        // The app uses the exact painted bounds to position transient UI,
        // such as status notifications, just outside the canvas. Recording
        // this after pan/zoom calculations keeps those elements in sync.
        app.canvas_rect = Some(canvas_rect);

        let checkerboard_tex = app.checkerboard_texture_id(ctx);
        let onion_tex = app.onion_texture_ids(ctx);
        let canvas_tex = app.canvas_texture.as_ref().map(|t| t.id());

        for (ox, oy) in tile_layout.offsets() {
            let offset_x = ox as f32 * display_w;
            let offset_y = oy as f32 * display_h;
            let current_rect = canvas_rect.translate(Vec2::new(offset_x, offset_y));
            if !current_rect.intersects(rect) {
                continue;
            }

            let checkerboard_uv =
                Rect::from_min_max(Pos2::ZERO, Pos2::new(canvas_w / 2.0, canvas_h / 2.0));
            painter.image(
                checkerboard_tex,
                current_rect,
                checkerboard_uv,
                Color32::WHITE,
            );

            if let Some((previous_texture, next_texture)) = onion_tex {
                let onion_alpha =
                    (app.editor.animation.onion_skin_opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
                let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
                painter.image(
                    previous_texture,
                    current_rect,
                    uv,
                    Color32::from_rgba_unmultiplied(255, 120, 120, onion_alpha),
                );
                painter.image(
                    next_texture,
                    current_rect,
                    uv,
                    Color32::from_rgba_unmultiplied(120, 180, 255, onion_alpha),
                );
            }

            if let Some(texture_id) = canvas_tex {
                let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
                painter.image(texture_id, current_rect, uv, Color32::WHITE);
            }

            if (ox, oy) != (0, 0) {
                painter.rect_stroke(
                    current_rect,
                    0.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(180, 0, 255).linear_multiply(0.5)),
                    egui::StrokeKind::Middle,
                );
            }
            if app.show_grid && app.zoom >= 4.0 {
                draw_pixel_grid(
                    &painter,
                    current_rect,
                    rect,
                    app.zoom,
                    canvas_width,
                    canvas_height,
                );
            }
        }

        // Track input independently from hover state. `Response::hover_pos()`
        // becomes `None` once a captured drag leaves the canvas, but the
        // interaction must still receive its eventual release.
        let pointer_hit = canvas_input_enabled
            .then(|| ctx.input(|input| input.pointer.interact_pos()))
            .flatten()
            .filter(|position| rect.contains(*position))
            .and_then(|position| {
                canvas_hit_at(
                    position,
                    canvas_origin,
                    app.zoom,
                    canvas_width,
                    canvas_height,
                    tile_layout,
                )
            });
        if let Some(hit) = pointer_hit {
            app.last_canvas_pixel = Some(hit.pixel);
        }

        let (primary_pressed, primary_released, primary_down) = ctx.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
            )
        });
        if primary_pressed && is_canvas_drag_tool(app.editor.active_tool) {
            if let Some(hit) = pointer_hit {
                app.begin_canvas_action_on_tile(hit.pixel, hit.tile_offset, hit.virtual_pixel);
                if app.editor.active_tool == ToolType::Marquee {
                    let (x, y) = hit.pixel;
                    tools::marquee::update_selection(&mut app.editor.selection, x, y, x, y);
                }
            }
        }
        if app.is_drawing && primary_down {
            if let Some(hit) = pointer_hit {
                update_canvas_action(app, hit);
            }
        }

        // Every configured tile is an editable mirror of the source.
        if let Some(hit) = pointer_hit {
            let (cx, cy) = hit.pixel;
            let highlight_rect = Rect::from_min_size(
                Pos2::new(
                    hit.tile_origin.x + cx as f32 * app.zoom,
                    hit.tile_origin.y + cy as f32 * app.zoom,
                ),
                Vec2::splat(app.zoom),
            );
            painter.rect_filled(highlight_rect, 0, Color32::from_white_alpha(40));

            let color = app.editor.primary_color;
            let ux = cx as u32;
            let uy = cy as u32;

            if response.clicked_by(egui::PointerButton::Secondary)
                && app.editor.active_tool == ToolType::Marquee
            {
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

        // Finish a press that did not become an egui drag (for example a
        // one-pixel pencil click), or a drag whose release happened outside
        // the canvas. The last in-bounds pixel is the safe fallback endpoint.
        if app.is_drawing {
            if primary_released {
                let endpoint = active_canvas_action_endpoint(app, pointer_hit);
                if let Some(endpoint) = endpoint {
                    finish_canvas_action(
                        app,
                        endpoint,
                        ctx.input(|i| i.modifiers.shift),
                        canvas_width,
                        canvas_height,
                        app.tile_mode,
                    );
                } else {
                    app.cancel_canvas_action();
                }
            } else if !primary_down {
                // A lost focus or cancelled native pointer event must not
                // leave a stale operation waiting for another release.
                app.cancel_canvas_action();
            }
        }

        // Draw selection after interaction handling so a marquee updates in
        // the same frame rather than one frame later.
        if app.editor.selection.active {
            for (ox, oy) in tile_layout.offsets() {
                let offset = Vec2::new(ox as f32 * display_w, oy as f32 * display_h);
                let current_rect = canvas_rect.translate(offset);
                if current_rect.intersects(rect) {
                    draw_selection_outline(&painter, app, canvas_origin + offset);
                }
            }
        }

        if app.is_drawing {
            let is_stroke = matches!(app.editor.active_tool, ToolType::Pencil | ToolType::Eraser);
            if is_stroke {
                let cache_key = stroke_preview_cache_key(app, canvas_width, canvas_height);
                let source_preview_rects = ctx
                    .data(|data| data.get_temp::<StrokePreviewCache>(stroke_preview_cache_id()))
                    .filter(|cache| cache.key == cache_key)
                    .map(|cache| cache.rects)
                    .unwrap_or_else(|| {
                        let mut pixels = tiled_stroke_pixels(
                            &app.canvas_action_virtual_points,
                            app.editor.brush_size,
                            app.tile_mode,
                            canvas_width,
                            canvas_height,
                        );
                        retain_preview_pixels_in_selection(&mut pixels, cache_key.selection);
                        let rects = std::sync::Arc::new(pixel_preview_rects(&pixels));
                        ctx.data_mut(|data| {
                            data.insert_temp(
                                stroke_preview_cache_id(),
                                StrokePreviewCache {
                                    key: cache_key,
                                    rects: rects.clone(),
                                },
                            );
                        });
                        rects
                    });
                let projected_preview_rects;
                let (preview_rects, preview_zoom): (&[PixelPreviewRect], f32) = if app.zoom < 1.0 {
                    projected_preview_rects =
                        screen_space_preview_rects(source_preview_rects.as_ref(), app.zoom);
                    (&projected_preview_rects, 1.0)
                } else {
                    (source_preview_rects.as_ref(), app.zoom)
                };
                let primary = app.editor.primary_color;
                let preview_color = if app.editor.active_tool == ToolType::Eraser {
                    Color32::from_rgba_unmultiplied(244, 63, 94, 160)
                } else {
                    Color32::from_rgba_unmultiplied(
                        primary[0],
                        primary[1],
                        primary[2],
                        primary[3].clamp(96, 180),
                    )
                };

                for (ox, oy) in tile_layout.offsets() {
                    let tile_offset = Vec2::new(ox as f32 * display_w, oy as f32 * display_h);
                    let current_rect = canvas_rect.translate(tile_offset);
                    if current_rect.intersects(rect) {
                        draw_pixel_preview_rects(
                            &painter.with_clip_rect(current_rect),
                            preview_rects,
                            current_rect.min,
                            preview_zoom,
                            preview_color,
                        );
                    }
                }
            } else if let (Some(start), Some(end)) = (app.shape_start, app.canvas_action_last_pixel)
            {
                let shift = ctx.input(|input| input.modifiers.shift);
                for (ox, oy) in tile_layout.offsets() {
                    let tile_offset = Vec2::new(ox as f32 * display_w, oy as f32 * display_h);
                    let current_rect = canvas_rect.translate(tile_offset);
                    if !current_rect.intersects(rect) {
                        continue;
                    }
                    let preview_origin =
                        mirrored_preview_origin(canvas_origin, tile_size, (ox, oy), (0, 0), false);
                    draw_canvas_preview(
                        &painter.with_clip_rect(current_rect),
                        app,
                        preview_origin,
                        current_rect,
                        start,
                        end,
                        shift,
                    );
                }
            }
        } else {
            ctx.data_mut(|data| data.remove::<StrokePreviewCache>(stroke_preview_cache_id()));
        }
        if app.show_guides {
            let guide_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 255, 255));
            let mut guide_to_remove = None;

            // Check if we are currently dragging a guide
            if let Some((is_horizontal, idx)) = app.dragging_guide {
                if let Some(pos) = ctx.pointer_interact_pos() {
                    if is_horizontal {
                        let y = ((pos.y - canvas_origin.y) / app.zoom).round() as i32;
                        app.horizontal_guides[idx] = y;
                        if !rect.contains(pos) && pos.y < rect.top() {
                            guide_to_remove = Some((true, idx));
                        }
                    } else {
                        let x = ((pos.x - canvas_origin.x) / app.zoom).round() as i32;
                        app.vertical_guides[idx] = x;
                        if !rect.contains(pos) && pos.x < rect.left() {
                            guide_to_remove = Some((false, idx));
                        }
                    }
                }

                if ctx.input(|i| i.pointer.any_released()) {
                    if let Some((is_horiz, r_idx)) = guide_to_remove {
                        if is_horiz {
                            app.horizontal_guides.remove(r_idx);
                        } else {
                            app.vertical_guides.remove(r_idx);
                        }
                    }
                    app.dragging_guide = None;
                }
            } else if response.drag_started() {
                // Check if user clicked on an existing guide
                if let Some(pos) = response.interact_pointer_pos() {
                    let click_y = ((pos.y - canvas_origin.y) / app.zoom).round() as i32;
                    let click_x = ((pos.x - canvas_origin.x) / app.zoom).round() as i32;

                    let mut found = false;
                    for (i, &g_y) in app.horizontal_guides.iter().enumerate() {
                        if (g_y - click_y).abs() <= 1 {
                            app.dragging_guide = Some((true, i));
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        for (i, &g_x) in app.vertical_guides.iter().enumerate() {
                            if (g_x - click_x).abs() <= 1 {
                                app.dragging_guide = Some((false, i));
                                break;
                            }
                        }
                    }
                }
            }

            for &g_y in &app.horizontal_guides {
                let y = canvas_origin.y + (g_y as f32) * app.zoom;
                painter.line_segment(
                    [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                    guide_stroke,
                );
            }
            for &g_x in &app.vertical_guides {
                let x = canvas_origin.x + (g_x as f32) * app.zoom;
                painter.line_segment(
                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                    guide_stroke,
                );
            }
        }

        if app.show_rulers {
            let (label_step, tick_step) = ruler_steps(app.zoom).unwrap_or((1, 1));

            if let Some(top_rect) = top_ruler_rect {
                let r_painter = ui.painter().with_clip_rect(top_rect);
                r_painter.rect_filled(top_rect, 0.0, Color32::from_rgb(30, 30, 40));

                let start_col = ((top_rect.left() - canvas_origin.x) / app.zoom)
                    .floor()
                    .max(0.0) as i32;
                let end_col = ((top_rect.right() - canvas_origin.x) / app.zoom)
                    .ceil()
                    .max(0.0) as i32;

                let first_col = aligned_ruler_start(start_col, tick_step);
                for col in (first_col..=end_col).step_by(tick_step as usize) {
                    let x = canvas_origin.x + (col as f32) * app.zoom;
                    let is_major = col % label_step == 0;
                    let tick_h = if is_major {
                        top_rect.height() * 0.8
                    } else {
                        top_rect.height() * 0.3
                    };
                    r_painter.line_segment(
                        [
                            Pos2::new(x, top_rect.bottom() - tick_h),
                            Pos2::new(x, top_rect.bottom()),
                        ],
                        Stroke::new(1.0_f32, Color32::from_gray(100)),
                    );
                    if is_major {
                        r_painter.text(
                            Pos2::new(x + 2.0, top_rect.top() + 2.0),
                            egui::Align2::LEFT_TOP,
                            col.to_string(),
                            egui::FontId::proportional(10.0),
                            Color32::from_gray(150),
                        );
                    }
                }

                if let Some(resp) = &top_ruler_resp {
                    if resp.drag_started() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let y = ((pos.y - canvas_origin.y) / app.zoom).round() as i32;
                            app.horizontal_guides.push(y);
                            app.dragging_guide = Some((true, app.horizontal_guides.len() - 1));
                            // force show guides
                            app.show_guides = true;
                        }
                    }
                }

                if let Some(pos) = ctx.pointer_latest_pos() {
                    r_painter.line_segment(
                        [
                            Pos2::new(pos.x, top_rect.top()),
                            Pos2::new(pos.x, top_rect.bottom()),
                        ],
                        Stroke::new(1.0_f32, Color32::from_rgb(255, 100, 100)),
                    );
                }
            }

            if let Some(left_rect) = left_ruler_rect {
                let r_painter = ui.painter().with_clip_rect(left_rect);
                r_painter.rect_filled(left_rect, 0.0, Color32::from_rgb(30, 30, 40));

                let start_row = ((left_rect.top() - canvas_origin.y) / app.zoom)
                    .floor()
                    .max(0.0) as i32;
                let end_row = ((left_rect.bottom() - canvas_origin.y) / app.zoom)
                    .ceil()
                    .max(0.0) as i32;

                let first_row = aligned_ruler_start(start_row, tick_step);
                for row in (first_row..=end_row).step_by(tick_step as usize) {
                    let y = canvas_origin.y + (row as f32) * app.zoom;
                    let is_major = row % label_step == 0;
                    let tick_w = if is_major {
                        left_rect.width() * 0.8
                    } else {
                        left_rect.width() * 0.3
                    };
                    r_painter.line_segment(
                        [
                            Pos2::new(left_rect.right() - tick_w, y),
                            Pos2::new(left_rect.right(), y),
                        ],
                        Stroke::new(1.0_f32, Color32::from_gray(100)),
                    );
                    if is_major {
                        r_painter.text(
                            Pos2::new(left_rect.left() + 2.0, y + 2.0),
                            egui::Align2::LEFT_TOP,
                            row.to_string(),
                            egui::FontId::proportional(10.0),
                            Color32::from_gray(150),
                        );
                    }
                }

                if let Some(resp) = &left_ruler_resp {
                    if resp.drag_started() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let x = ((pos.x - canvas_origin.x) / app.zoom).round() as i32;
                            app.vertical_guides.push(x);
                            app.dragging_guide = Some((false, app.vertical_guides.len() - 1));
                            // force show guides
                            app.show_guides = true;
                        }
                    }
                }

                if let Some(pos) = ctx.pointer_latest_pos() {
                    r_painter.line_segment(
                        [
                            Pos2::new(left_rect.left(), pos.y),
                            Pos2::new(left_rect.right(), pos.y),
                        ],
                        Stroke::new(1.0_f32, Color32::from_rgb(255, 100, 100)),
                    );
                }
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

fn ruler_steps(zoom: f32) -> Option<(i32, i32)> {
    if !zoom.is_finite() || zoom <= 0.0 {
        return None;
    }

    let required = (35.0 / zoom).ceil();
    if !required.is_finite() || required > i32::MAX as f32 {
        return None;
    }
    let required = required.max(1.0) as i32;
    let mut magnitude = 1_i32;
    while magnitude.checked_mul(5)? < required {
        magnitude = magnitude.checked_mul(10)?;
    }
    let label_step = [1_i32, 2, 5]
        .into_iter()
        .filter_map(|multiplier| magnitude.checked_mul(multiplier))
        .find(|candidate| *candidate >= required)?;
    Some((label_step, (label_step / 5).max(1)))
}

fn aligned_ruler_start(value: i32, step: i32) -> i32 {
    let remainder = value.rem_euclid(step);
    if remainder == 0 {
        value
    } else {
        value.saturating_add(step - remainder)
    }
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

fn draw_pixel_grid(
    painter: &egui::Painter,
    tile_rect: Rect,
    viewport: Rect,
    zoom: f32,
    canvas_width: u32,
    canvas_height: u32,
) {
    if !tile_rect.intersects(viewport) || !zoom.is_finite() || zoom <= 0.0 {
        return;
    }

    let visible_left = viewport.left().max(tile_rect.left());
    let visible_right = viewport.right().min(tile_rect.right());
    let visible_top = viewport.top().max(tile_rect.top());
    let visible_bottom = viewport.bottom().min(tile_rect.bottom());
    let min_col = ((visible_left - tile_rect.left()) / zoom)
        .floor()
        .clamp(0.0, canvas_width as f32) as u32;
    let max_col = ((visible_right - tile_rect.left()) / zoom)
        .ceil()
        .clamp(0.0, canvas_width as f32) as u32;
    let min_row = ((visible_top - tile_rect.top()) / zoom)
        .floor()
        .clamp(0.0, canvas_height as f32) as u32;
    let max_row = ((visible_bottom - tile_rect.top()) / zoom)
        .ceil()
        .clamp(0.0, canvas_height as f32) as u32;
    let stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 30));

    for x in min_col..=max_col {
        let px = tile_rect.left() + x as f32 * zoom;
        painter.line_segment(
            [Pos2::new(px, visible_top), Pos2::new(px, visible_bottom)],
            stroke,
        );
    }
    for y in min_row..=max_row {
        let py = tile_rect.top() + y as f32 * zoom;
        painter.line_segment(
            [Pos2::new(visible_left, py), Pos2::new(visible_right, py)],
            stroke,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CanvasActionEndpoint {
    pixel: (i32, i32),
    virtual_pixel: (i32, i32),
}

fn canvas_gesture_accepts_hit(
    tool: ToolType,
    start_tile: Option<(i32, i32)>,
    hit: CanvasHit,
) -> bool {
    matches!(tool, ToolType::Pencil | ToolType::Eraser) || start_tile == Some(hit.tile_offset)
}

fn active_canvas_action_endpoint(
    app: &PixelBuddyApp,
    current: Option<CanvasHit>,
) -> Option<CanvasActionEndpoint> {
    if let Some(hit) = current.filter(|hit| {
        canvas_gesture_accepts_hit(app.editor.active_tool, app.canvas_action_tile_offset, *hit)
    }) {
        return Some(CanvasActionEndpoint {
            pixel: hit.pixel,
            virtual_pixel: hit.virtual_pixel,
        });
    }

    let pixel = app.canvas_action_last_pixel.or(app.shape_start)?;
    let virtual_pixel = if matches!(app.editor.active_tool, ToolType::Pencil | ToolType::Eraser) {
        app.canvas_action_virtual_points
            .last()
            .copied()
            .unwrap_or(pixel)
    } else {
        pixel
    };
    Some(CanvasActionEndpoint {
        pixel,
        virtual_pixel,
    })
}

fn constrain_end_point(
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    tool: ToolType,
    shift: bool,
) -> (i32, i32) {
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

fn update_canvas_action(app: &mut PixelBuddyApp, hit: CanvasHit) {
    if !canvas_gesture_accepts_hit(app.editor.active_tool, app.canvas_action_tile_offset, hit) {
        return;
    }

    let (x, y) = hit.pixel;
    app.last_canvas_pixel = Some(hit.pixel);
    app.canvas_action_last_pixel = Some(hit.pixel);
    match app.editor.active_tool {
        ToolType::Pencil | ToolType::Eraser => {
            let point = (x as u32, y as u32);
            if app.stroke_points.last().copied() != Some(point) {
                app.stroke_points.push(point);
            }
            if app.canvas_action_virtual_points.last().copied() != Some(hit.virtual_pixel) {
                app.canvas_action_virtual_points.push(hit.virtual_pixel);
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

fn push_pixel_perfect_raw<F>(
    raw: (i32, i32),
    accepted_previous: &mut Option<(i32, i32)>,
    pending: &mut Option<(i32, i32)>,
    visit: &mut F,
) -> bool
where
    F: FnMut(i32, i32) -> bool,
{
    let Some(previous) = *accepted_previous else {
        *accepted_previous = Some(raw);
        return visit(raw.0, raw.1);
    };
    let Some(current) = pending.replace(raw) else {
        return true;
    };

    let is_corner = previous.0 != raw.0
        && previous.1 != raw.1
        && ((current.0 == previous.0 && current.1 == raw.1)
            || (current.0 == raw.0 && current.1 == previous.1));
    if !is_corner && current != previous {
        *accepted_previous = Some(current);
        return visit(current.0, current.1);
    }
    true
}

fn for_each_signed_stroke_point<F>(points: &[(i32, i32)], mut visit: F)
where
    F: FnMut(i32, i32) -> bool,
{
    let Some(&last_point) = points.last() else {
        return;
    };
    let mut accepted_previous = None;
    let mut pending = None;

    for segment in points.windows(2) {
        let (mut x0, mut y0) = segment[0];
        let (x1, y1) = segment[1];
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        while x0 != x1 || y0 != y1 {
            if !push_pixel_perfect_raw((x0, y0), &mut accepted_previous, &mut pending, &mut visit) {
                return;
            }
            let doubled_error = 2 * error;
            if doubled_error >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled_error <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    if !push_pixel_perfect_raw(last_point, &mut accepted_previous, &mut pending, &mut visit) {
        return;
    }
    if let (Some(previous), Some(current)) = (accepted_previous, pending) {
        if current != previous {
            visit(current.0, current.1);
        }
    }
}

fn mapped_tile_axis(value: i32, dimension: u32, wraps: bool) -> Option<u32> {
    let dimension = i32::try_from(dimension).ok()?;
    if dimension <= 0 {
        return None;
    }
    if wraps {
        Some(value.rem_euclid(dimension) as u32)
    } else if (0..dimension).contains(&value) {
        Some(value as u32)
    } else {
        None
    }
}

const DENSE_PIXEL_MASK_LIMIT: usize = 1_048_576;

enum PixelMask {
    Dense {
        bits: Vec<bool>,
    },
    Sparse {
        indices: std::collections::HashSet<usize>,
        pixel_count: usize,
    },
}

impl PixelMask {
    fn new(pixel_count: usize) -> Self {
        if pixel_count <= DENSE_PIXEL_MASK_LIMIT {
            Self::Dense {
                bits: vec![false; pixel_count],
            }
        } else {
            Self::Sparse {
                indices: std::collections::HashSet::new(),
                pixel_count,
            }
        }
    }

    fn insert(&mut self, index: usize) -> bool {
        match self {
            Self::Dense { bits } => {
                if bits[index] {
                    false
                } else {
                    bits[index] = true;
                    true
                }
            }
            Self::Sparse {
                indices,
                pixel_count,
            } => {
                if !indices.insert(index) {
                    return false;
                }
                let promote_at = (*pixel_count / 256).max(4_096);
                if indices.len() >= promote_at {
                    let mut bits = vec![false; *pixel_count];
                    for &seen in indices.iter() {
                        bits[seen] = true;
                    }
                    *self = Self::Dense { bits };
                }
                true
            }
        }
    }
}

fn tiled_stroke_pixels(
    points: &[(i32, i32)],
    brush_size: u8,
    tile_mode: TileMode,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<(u32, u32)> {
    let wraps_x = matches!(tile_mode, TileMode::XAxis | TileMode::Both);
    let wraps_y = matches!(tile_mode, TileMode::YAxis | TileMode::Both);
    let width = match usize::try_from(canvas_width) {
        Ok(width) if width > 0 => width,
        _ => return Vec::new(),
    };
    let height = match usize::try_from(canvas_height) {
        Ok(height) if height > 0 => height,
        _ => return Vec::new(),
    };
    let Some(pixel_count) = width.checked_mul(height) else {
        return Vec::new();
    };

    // Deduplicate the wrapped centerline before expanding the brush. A stroke
    // can traverse the same source pixels through many configured copies; this
    // keeps temporary memory bounded by the source canvas, not copy count.
    let mut centerline_seen = PixelMask::new(pixel_count);
    let mut centerline = Vec::new();
    for_each_signed_stroke_point(points, |x, y| {
        let Some(x) = mapped_tile_axis(x, canvas_width, wraps_x) else {
            return true;
        };
        let Some(y) = mapped_tile_axis(y, canvas_height, wraps_y) else {
            return true;
        };
        let index = y as usize * width + x as usize;
        if centerline_seen.insert(index) {
            centerline.push((x, y));
        }
        centerline.len() < pixel_count
    });

    let size = i32::from(brush_size.max(1));
    let mut changed_seen = PixelMask::new(pixel_count);
    let mut mapped = Vec::new();
    for (x, y) in centerline {
        let (x, y) = (x as i32, y as i32);
        for dy in 0..size {
            for dx in 0..size {
                let Some(x) = mapped_tile_axis(x + dx, canvas_width, wraps_x) else {
                    continue;
                };
                let Some(y) = mapped_tile_axis(y + dy, canvas_height, wraps_y) else {
                    continue;
                };
                let index = y as usize * width + x as usize;
                if changed_seen.insert(index) {
                    mapped.push((x, y));
                }
            }
        }
    }

    mapped.sort_unstable_by_key(|(x, y)| (*y, *x));
    mapped
}

fn tiled_stroke_changes(
    points: &[(i32, i32)],
    color: [u8; 4],
    brush_size: u8,
    tile_mode: TileMode,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<tools::PixelChange> {
    tiled_stroke_pixels(points, brush_size, tile_mode, canvas_width, canvas_height)
        .into_iter()
        .map(|(x, y)| (x, y, color))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelPreviewRect {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StrokePreviewCacheKey {
    gesture_generation: u64,
    point_count: usize,
    last_point: Option<(i32, i32)>,
    brush_size: u8,
    tile_mode: TileMode,
    canvas_width: u32,
    canvas_height: u32,
    selection: Option<(i32, i32, i32, i32)>,
}

#[derive(Clone)]
struct StrokePreviewCache {
    key: StrokePreviewCacheKey,
    rects: std::sync::Arc<Vec<PixelPreviewRect>>,
}

fn stroke_preview_cache_key(
    app: &PixelBuddyApp,
    canvas_width: u32,
    canvas_height: u32,
) -> StrokePreviewCacheKey {
    let selection = app.editor.selection.active.then(|| {
        (
            app.editor.selection.min_x(),
            app.editor.selection.max_x(),
            app.editor.selection.min_y(),
            app.editor.selection.max_y(),
        )
    });
    StrokePreviewCacheKey {
        gesture_generation: app.canvas_action_generation,
        point_count: app.canvas_action_virtual_points.len(),
        last_point: app.canvas_action_virtual_points.last().copied(),
        brush_size: app.editor.brush_size,
        tile_mode: app.tile_mode,
        canvas_width,
        canvas_height,
        selection,
    }
}

fn stroke_preview_cache_id() -> egui::Id {
    egui::Id::new("canvas_stroke_preview_cache")
}

fn retain_preview_pixels_in_selection(
    pixels: &mut Vec<(u32, u32)>,
    selection: Option<(i32, i32, i32, i32)>,
) {
    if let Some((min_x, max_x, min_y, max_y)) = selection {
        pixels.retain(|&(x, y)| {
            (x as i32) >= min_x && (x as i32) <= max_x && (y as i32) >= min_y && (y as i32) <= max_y
        });
    }
}

fn pixel_preview_rects(pixels: &[(u32, u32)]) -> Vec<PixelPreviewRect> {
    // The coalescer relies on unique row-major input. Stroke rasterization
    // guarantees that contract; keep it explicit for future callers.
    debug_assert!(pixels
        .windows(2)
        .all(|pair| { (pair[0].1, pair[0].0) < (pair[1].1, pair[1].0) }));
    let mut horizontal_runs: Vec<PixelPreviewRect> = Vec::new();
    for &(x, y) in pixels {
        if let Some(run) = horizontal_runs.last_mut() {
            if run.min_y == y && run.max_y == y + 1 && run.max_x == x {
                run.max_x = x + 1;
                continue;
            }
        }
        horizontal_runs.push(PixelPreviewRect {
            min_x: x,
            min_y: y,
            max_x: x + 1,
            max_y: y + 1,
        });
    }

    merge_vertical_preview_runs(horizontal_runs)
}

fn merge_vertical_preview_runs(horizontal_runs: Vec<PixelPreviewRect>) -> Vec<PixelPreviewRect> {
    let mut merged: Vec<PixelPreviewRect> = Vec::new();
    let mut active = std::collections::HashMap::<(u32, u32), usize>::new();
    let mut cursor = 0;
    while cursor < horizontal_runs.len() {
        let row = horizontal_runs[cursor].min_y;
        let mut next = std::collections::HashMap::new();
        while cursor < horizontal_runs.len() && horizontal_runs[cursor].min_y == row {
            let run = horizontal_runs[cursor];
            let key = (run.min_x, run.max_x);
            let merged_index = active
                .get(&key)
                .copied()
                .filter(|&index| merged[index].max_y == row)
                .unwrap_or_else(|| {
                    merged.push(run);
                    merged.len() - 1
                });
            merged[merged_index].max_y = row + 1;
            next.insert(key, merged_index);
            cursor += 1;
        }
        active = next;
    }
    merged
}

fn screen_space_preview_rects(rects: &[PixelPreviewRect], zoom: f32) -> Vec<PixelPreviewRect> {
    debug_assert!(zoom.is_finite() && zoom > 0.0 && zoom < 1.0);
    let mut rows = std::collections::BTreeMap::<u32, Vec<(u32, u32)>>::new();
    for rect in rects {
        let min_x = (rect.min_x as f32 * zoom).floor() as u32;
        let max_x = (rect.max_x as f32 * zoom).ceil() as u32;
        let min_y = (rect.min_y as f32 * zoom).floor() as u32;
        let max_y = (rect.max_y as f32 * zoom).ceil() as u32;
        if min_x >= max_x || min_y >= max_y {
            continue;
        }
        for row in min_y..max_y {
            rows.entry(row).or_default().push((min_x, max_x));
        }
    }

    let mut horizontal_runs = Vec::new();
    for (row, mut spans) in rows {
        spans.sort_unstable();
        let mut current: Option<(u32, u32)> = None;
        for (min_x, max_x) in spans {
            match current {
                Some((start, end)) if min_x <= end => {
                    current = Some((start, end.max(max_x)));
                }
                Some((start, end)) => {
                    horizontal_runs.push(PixelPreviewRect {
                        min_x: start,
                        min_y: row,
                        max_x: end,
                        max_y: row + 1,
                    });
                    current = Some((min_x, max_x));
                }
                None => current = Some((min_x, max_x)),
            }
        }
        if let Some((start, end)) = current {
            horizontal_runs.push(PixelPreviewRect {
                min_x: start,
                min_y: row,
                max_x: end,
                max_y: row + 1,
            });
        }
    }
    merge_vertical_preview_runs(horizontal_runs)
}

fn pixel_preview_screen_rect(rect: PixelPreviewRect, canvas_origin: Pos2, zoom: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(
            canvas_origin.x + rect.min_x as f32 * zoom,
            canvas_origin.y + rect.min_y as f32 * zoom,
        ),
        Pos2::new(
            canvas_origin.x + rect.max_x as f32 * zoom,
            canvas_origin.y + rect.max_y as f32 * zoom,
        ),
    )
}

fn draw_pixel_preview_rects(
    painter: &egui::Painter,
    rects: &[PixelPreviewRect],
    canvas_origin: Pos2,
    zoom: f32,
    color: Color32,
) {
    let clip_rect = painter.clip_rect();
    for rect in rects {
        let screen_rect = pixel_preview_screen_rect(*rect, canvas_origin, zoom);
        if screen_rect.intersects(clip_rect) {
            painter.rect_filled(screen_rect, 0.0, color);
        }
    }
}

fn finish_canvas_action(
    app: &mut PixelBuddyApp,
    endpoint: CanvasActionEndpoint,
    shift: bool,
    canvas_width: u32,
    canvas_height: u32,
    tile_mode: TileMode,
) {
    let Some((start_x, start_y)) = app.shape_start else {
        app.cancel_canvas_action();
        return;
    };

    let tool = app.editor.active_tool;
    let (end_x, end_y) = constrain_end_point(
        start_x,
        start_y,
        endpoint.pixel.0,
        endpoint.pixel.1,
        tool,
        shift,
    );

    if matches!(tool, ToolType::Pencil | ToolType::Eraser)
        && app.canvas_action_virtual_points.last().copied() != Some(endpoint.virtual_pixel)
    {
        app.canvas_action_virtual_points
            .push(endpoint.virtual_pixel);
    }
    if tool == ToolType::Marquee {
        tools::marquee::update_selection(&mut app.editor.selection, start_x, start_y, end_x, end_y);
    }

    let color = app.editor.primary_color;
    let mut changes = match tool {
        ToolType::Pencil => tiled_stroke_changes(
            &app.canvas_action_virtual_points,
            color,
            app.editor.brush_size,
            tile_mode,
            canvas_width,
            canvas_height,
        ),
        ToolType::Eraser => tiled_stroke_changes(
            &app.canvas_action_virtual_points,
            [0, 0, 0, 0],
            app.editor.brush_size,
            tile_mode,
            canvas_width,
            canvas_height,
        ),
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

    if matches!(
        tool,
        ToolType::Line | ToolType::Rectangle | ToolType::Ellipse
    ) {
        let size = u32::from(app.editor.brush_size);
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
    // Successful cleanup must not invoke the partial-marquee rollback.
    app.is_drawing = false;
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
    let (end_x, end_y) = constrain_end_point(
        start.0,
        start.1,
        end.0,
        end.1,
        app.editor.active_tool,
        shift,
    );
    let end = (end_x, end_y);
    let primary = app.editor.primary_color;
    let preview_color = Color32::from_rgba_unmultiplied(
        primary[0],
        primary[1],
        primary[2],
        primary[3].clamp(96, 180),
    );
    let stroke = Stroke::new((app.zoom * 0.8).max(1.0), preview_color);

    match app.editor.active_tool {
        ToolType::Pencil | ToolType::Eraser => {}
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
mod tests;
