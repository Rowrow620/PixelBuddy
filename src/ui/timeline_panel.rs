use crate::app::PixelBuddyApp;

/// Payload carried while a frame button is dragged within the timeline.
///
/// Keeping this distinct from a bare `usize` prevents an unrelated drag-and-
/// drop interaction from ever being interpreted as a timeline frame move.
#[derive(Clone, Copy, Debug)]
struct FrameDragPayload {
    source_index: usize,
}

/// Returns the final index for a drag insertion on either side of a frame.
///
/// `AnimationManager::move_frame` accepts a final index, rather than an
/// insertion slot. Removing the source shifts targets to its right, so a
/// before/after drop needs to account for which side of the target the source
/// started on.
fn frame_drop_destination(
    source_index: usize,
    target_index: usize,
    insert_after_target: bool,
    frame_count: usize,
) -> Option<usize> {
    if source_index >= frame_count || target_index >= frame_count || source_index == target_index {
        return None;
    }

    let destination = match (source_index < target_index, insert_after_target) {
        // The source is removed from before the target, so the target shifts
        // one position left in the resulting vector.
        (true, false) => target_index - 1,
        (true, true) => target_index,
        (false, false) => target_index,
        (false, true) => (target_index + 1).min(frame_count - 1),
    };

    (destination != source_index).then_some(destination)
}

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    let mut tag_to_remove = None;
    let mut frame_selection = None;
    let mut layer_visibility_changes = Vec::new();
    let mut layer_rename_change = None;
    let mut new_active_layer = app.editor.document().active_layer_index;

    egui::TopBottomPanel::bottom("timeline_panel")
        .max_height(350.0)
        .show(ctx, |ui| {
            // TOP CONTROLS BAR (Play, FPS, Onion, etc.)
            ui.horizontal_wrapped(|ui| {
                ui.add_space(4.0);
                let play_icon = if app.editor.animation.is_playing { "⏸" } else { "▶" };
                if ui.button(play_icon).on_hover_text("Play/Pause Animation (Space)").clicked() {
                    let current_time = ctx.input(|input| input.time);
                    app.editor.animation.toggle_play(current_time);
                }
                if ui.button("◼").on_hover_text("Stop Animation").clicked() {
                    app.editor.stop_animation();
                    app.texture_dirty = true;
                }

                ui.separator();

                ui.label(egui::RichText::new("FPS:").size(11.0));
                let mut fps = app.editor.animation.fps as i32;
                if ui.add(egui::Slider::new(&mut fps, 1..=30).suffix(" fps")).changed() {
                    app.editor.set_animation_fps(fps as u32);
                    app.editor.animation.reset_playback_clock(ctx.input(|input| input.time));
                }

                ui.separator();

                let mut onion_skin_enabled = app.editor.animation.onion_skin_enabled;
                if ui.checkbox(&mut onion_skin_enabled, "Onion Skin").clicked() {
                    app.editor.set_onion_skin_enabled(onion_skin_enabled);
                    app.texture_dirty = true;
                }
                if onion_skin_enabled {
                    let mut onion_skin_opacity = app.editor.animation.onion_skin_opacity;
                    if ui.add(egui::Slider::new(&mut onion_skin_opacity, 0.0..=1.0).text("Opacity").show_value(false)).changed() {
                        app.editor.set_onion_skin_opacity(onion_skin_opacity);
                        ctx.request_repaint();
                    }
                }

                ui.separator();

                let icon_size = egui::vec2(16.0, 16.0);
                let button_size = egui::vec2(24.0, 24.0);
                let text_color = ui.visuals().text_color();

                let add_img = egui::Image::new(egui::include_image!("../../assets/icons/plus.svg")).tint(text_color).fit_to_exact_size(icon_size);
                if ui.add(egui::Button::image(add_img).min_size(button_size)).on_hover_text("Add new blank frame").clicked() {
                    app.editor.add_frame();
                    let current = app.editor.animation.current_frame_index;
                    app.frame_thumbnails.insert(current, None);
                    app.texture_dirty = true;
                    app.invalidate_onion_skin_cache();
                }

                let dup_img = egui::Image::new(egui::include_image!("../../assets/icons/copy.svg")).tint(text_color).fit_to_exact_size(icon_size);
                if ui.add(egui::Button::image(dup_img).min_size(button_size)).on_hover_text("Duplicate current frame").clicked() {
                    app.editor.duplicate_frame();
                    let current = app.editor.animation.current_frame_index;
                    app.frame_thumbnails.insert(current, None);
                    app.texture_dirty = true;
                    app.invalidate_onion_skin_cache();
                }

                let del_img = egui::Image::new(egui::include_image!("../../assets/icons/trash.svg")).tint(text_color).fit_to_exact_size(icon_size);
                if ui.add(egui::Button::image(del_img).min_size(button_size)).on_hover_text("Delete current frame").clicked() {
                    let current = app.editor.animation.current_frame_index;
                    app.editor.remove_frame();
                    if app.frame_thumbnails.len() > current {
                        app.frame_thumbnails.remove(current);
                    }
                    app.texture_dirty = true;
                    app.invalidate_onion_skin_cache();
                }
            });
            ui.separator();

            // GRID LAYOUT
            let header_height = 42.0;
            let row_height = 32.0;
            let frame_count = app.editor.animation.frames.len();
            let current_frame = app.editor.animation.current_frame_index;
            let layers_count = app.editor.document().layers.len();
            let active_idx = app.editor.document().active_layer_index;

            egui::ScrollArea::vertical().id_salt("timeline_vscroll").show(ui, |ui| {
                ui.horizontal(|ui| {
                    // LEFT COLUMN (Layers)
                    ui.vertical(|ui| {
                        ui.set_width(220.0);
                        // Header space
                        ui.allocate_ui(egui::vec2(220.0, header_height), |ui| {
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                                ui.label(egui::RichText::new("LAYERS").strong());
                            });
                        });
                        
                        // Layer rows
                        for i in (0..layers_count).rev() {
                            ui.allocate_ui(egui::vec2(220.0, row_height), |ui| {
                                crate::ui::layers_panel::draw_layer_row_ui(
                                    ctx, app, ui, i, active_idx, current_frame,
                                    &mut layer_visibility_changes,
                                    &mut layer_rename_change,
                                    &mut new_active_layer
                                );
                            });
                        }
                    });

                    ui.separator();

                    // RIGHT COLUMN (Frames)
                    egui::ScrollArea::horizontal().id_salt("timeline_hscroll").show(ui, |ui| {
                        ui.vertical(|ui| {
                            // Frame headers (Thumbnail + Number)
                            let tags_rect_start = ui.cursor().min;
                            ui.add_space(18.0); // Space for tags
                            
                            let mut frame_rects = Vec::new();

                            ui.horizontal(|ui| {
                                for f in 0..frame_count {
                                    let is_active = f == current_frame;
                                    let label = format!("Frame {}", f + 1);

                                    let (frame_response, rect) = ui.vertical(|ui| {
                                        ui.set_width(32.0);
                                        let frame_response = if let Some(Some(thumb)) = app.frame_thumbnails.get(f) {
                                            let btn = egui::ImageButton::new(
                                                egui::Image::new(thumb)
                                                    .fit_to_exact_size(egui::vec2(32.0, 32.0))
                                                    .maintain_aspect_ratio(true)
                                            );
                                            let r = ui.add(btn.sense(egui::Sense::click_and_drag())).on_hover_text(&label);
                                            if is_active {
                                                ui.painter().rect_stroke(r.rect, 2.0, egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill), egui::StrokeKind::Inside);
                                            }
                                            r
                                        } else {
                                            let mut btn = egui::Button::new(egui::RichText::new(&label).size(11.0)).min_size(egui::vec2(32.0, 32.0));
                                            if is_active {
                                                btn = btn.stroke(egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill));
                                            }
                                            ui.add(btn.sense(egui::Sense::click_and_drag())).on_hover_text(&label)
                                        };

                                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                                            ui.label(egui::RichText::new(format!("{}", f + 1)).size(10.0).color(egui::Color32::from_gray(140)));
                                        });

                                        (frame_response.clone(), frame_response.rect)
                                    }).inner;
                                    
                                    frame_rects.push(rect);

                                    // Tags context menu and DND logic would go here
                                    frame_response.context_menu(|ui| {
                                        if ui.button("Create Tag").clicked() {
                                            app.editor.animation.tags.push(crate::document::animation::FrameTag {
                                                name: "New Tag".to_owned(),
                                                color: [0.8, 0.2, 0.2],
                                                from_frame: f,
                                                to_frame: f,
                                            });
                                            ui.close_menu();
                                        }
                                    });

                                    // Just select for now if clicked
                                    if frame_response.clicked() {
                                        frame_selection = Some(f);
                                    }
                                }
                            });

                            // Render Tags
                            let painter = ui.painter();
                            for (tag_idx, tag) in app.editor.animation.tags.iter_mut().enumerate() {
                                if tag.from_frame < frame_rects.len() && tag.to_frame < frame_rects.len() {
                                    let start_rect = frame_rects[tag.from_frame];
                                    let end_rect = frame_rects[tag.to_frame];
                                    let tag_rect = egui::Rect::from_min_max(
                                        egui::pos2(start_rect.left(), tags_rect_start.y),
                                        egui::pos2(end_rect.right(), tags_rect_start.y + 16.0),
                                    );
                                    let color = egui::Color32::from_rgb((tag.color[0] * 255.0) as u8, (tag.color[1] * 255.0) as u8, (tag.color[2] * 255.0) as u8);
                                    painter.rect_filled(tag_rect, 4.0, color);
                                    painter.text(tag_rect.min + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, &tag.name, egui::FontId::proportional(10.0), egui::Color32::WHITE);
                                    
                                    let tag_id = ui.id().with("tag").with(tag_idx);
                                    let tag_response = ui.interact(tag_rect, tag_id, egui::Sense::click_and_drag());
                                    tag_response.context_menu(|ui| {
                                        ui.text_edit_singleline(&mut tag.name);
                                        ui.color_edit_button_rgb(&mut tag.color);
                                        if ui.button("Delete Tag").clicked() {
                                            tag_to_remove = Some(tag_idx);
                                            ui.close_menu();
                                        }
                                    });
                                }
                            }

                            // Layer rows (Grid)
                            for i in (0..layers_count).rev() {
                                ui.horizontal(|ui| {
                                    for f in 0..frame_count {
                                        ui.allocate_ui(egui::vec2(32.0, row_height), |ui| {
                                            ui.centered_and_justified(|ui| {
                                                let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                                                let is_current_frame = f == current_frame;
                                                let has_layer = app.editor.animation.frames[f].document.layers.len() > i;
                                                
                                                if has_layer {
                                                    let bg = if is_current_frame && i == active_idx {
                                                        ui.visuals().selection.bg_fill
                                                    } else {
                                                        egui::Color32::from_gray(60)
                                                    };
                                                    ui.painter().rect_filled(rect, 2.0, bg);
                                                    ui.painter().circle_filled(rect.center(), 2.0, egui::Color32::from_gray(160));
                                                } else {
                                                    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0_f32, egui::Color32::from_gray(40)), egui::StrokeKind::Inside);
                                                }
                                                
                                                if response.clicked() {
                                                    frame_selection = Some(f);
                                                    new_active_layer = i;
                                                }
                                            });
                                        });
                                    }
                                });
                            }
                        });
                    });
                });
            });
        });

    if let Some(f) = frame_selection {
        app.editor.select_frame(f);
        app.texture_dirty = true;
    }

    if new_active_layer != app.editor.document().active_layer_index {
        app.editor.document_mut().active_layer_index = new_active_layer;
    }

    for (idx, visible) in &layer_visibility_changes {
        if app.editor.mutate_document("Toggle layer visibility", |document| {
            if let Some(layer) = document.layers.get_mut(*idx) {
                if layer.visible != *visible {
                    layer.visible = *visible;
                    return true;
                }
            }
            false
        }) {
            app.texture_dirty = true;
        }
    }

    if let Some((idx, name)) = layer_rename_change {
        let _ = app.editor.mutate_document("Rename layer", move |document| {
            if let Some(layer) = document.layers.get_mut(idx) {
                if layer.name != name {
                    layer.name = name;
                    return true;
                }
            }
            false
        });
    }
    
    if let Some(idx) = tag_to_remove {
        app.editor.animation.tags.remove(idx);
    }
}

#[cfg(test)]
mod tests {
    use super::frame_drop_destination;

    #[test]
    fn drag_destination_preserves_before_and_after_drop_positions() {
        // Moving the first frame around the third frame: it belongs at index
        // one before the target and index two after it.
        assert_eq!(frame_drop_destination(0, 2, false, 4), Some(1));
        assert_eq!(frame_drop_destination(0, 2, true, 4), Some(2));

        // Moving the last frame around the second frame does not need a
        // left-shift correction, because its source is after the target.
        assert_eq!(frame_drop_destination(3, 1, false, 4), Some(1));
        assert_eq!(frame_drop_destination(3, 1, true, 4), Some(2));
    }

    #[test]
    fn drag_destination_rejects_noop_and_invalid_moves() {
        // Dragging immediately before/after an adjacent frame can be a true
        // no-op. Avoid marking the project dirty for those drops.
        assert_eq!(frame_drop_destination(0, 1, false, 4), None);
        assert_eq!(frame_drop_destination(1, 0, true, 4), None);
        assert_eq!(frame_drop_destination(2, 2, false, 4), None);
        assert_eq!(frame_drop_destination(4, 0, false, 4), None);
        assert_eq!(frame_drop_destination(0, 4, true, 4), None);
    }
}
