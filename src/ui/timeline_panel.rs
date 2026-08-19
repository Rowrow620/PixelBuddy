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
    egui::TopBottomPanel::bottom("timeline_panel")
        .exact_height(58.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Playback Controls
                ui.add_space(4.0);
                let play_icon = if app.editor.animation.is_playing {
                    "⏸"
                } else {
                    "▶"
                };
                if ui
                    .button(play_icon)
                    .on_hover_text("Play/Pause Animation (Space)")
                    .clicked()
                {
                    let current_time = ctx.input(|input| input.time);
                    app.editor.animation.toggle_play(current_time);
                }
                if ui.button("◼").on_hover_text("Stop Animation").clicked() {
                    app.editor.stop_animation();
                    app.texture_dirty = true;
                }

                ui.separator();

                // FPS Control. Changing it updates the stored duration of
                // every frame, so preview playback and GIF export agree.
                ui.label(egui::RichText::new("FPS:").size(11.0));
                let mut fps = app.editor.animation.fps as i32;
                if ui
                    .add(egui::Slider::new(&mut fps, 1..=30).suffix(" fps"))
                    .on_hover_text("Sets one speed for preview playback and GIF export")
                    .changed()
                {
                    app.editor.set_animation_fps(fps as u32);
                    app.editor
                        .animation
                        .reset_playback_clock(ctx.input(|input| input.time));
                }

                ui.separator();

                // Onion Skin Toggle
                let mut onion_skin_enabled = app.editor.animation.onion_skin_enabled;
                if ui.checkbox(&mut onion_skin_enabled, "Onion Skin").on_hover_text("Show ghost images of neighboring frames").clicked() {
                    app.editor.set_onion_skin_enabled(onion_skin_enabled);
                    app.texture_dirty = true;
                }
                if onion_skin_enabled {
                    let mut onion_skin_opacity = app.editor.animation.onion_skin_opacity;
                    if ui
                        .add(
                            egui::Slider::new(&mut onion_skin_opacity, 0.0..=1.0)
                                .text("Onion opacity")
                                .show_value(false),
                        )
                        .on_hover_text("Opacity for the previous and next frame overlays")
                        .changed()
                    {
                        app.editor.set_onion_skin_opacity(onion_skin_opacity);
                        ctx.request_repaint();
                    }
                }

                ui.separator();

                // Frame Actions
                if ui
                    .button("+ Frame")
                    .on_hover_text("Add new blank frame")
                    .clicked()
                {
                    app.editor.add_frame();
                    app.texture_dirty = true;
                    app.invalidate_onion_skin_cache();
                }
                if ui
                    .button("Dup Frame")
                    .on_hover_text("Duplicate current frame")
                    .clicked()
                {
                    app.editor.duplicate_frame();
                    app.texture_dirty = true;
                    app.invalidate_onion_skin_cache();
                }
                if ui
                    .button("Del Frame")
                    .on_hover_text("Delete current frame")
                    .clicked()
                {
                    let frame_count_before = app.editor.animation.frames.len();
                    app.editor.remove_frame();
                    if app.editor.animation.frames.len() != frame_count_before {
                        app.texture_dirty = true;
                        app.invalidate_onion_skin_cache();
                    }
                }
                if ui
                    .button("Copy Frame")
                    .on_hover_text("Copy the current frame")
                    .clicked()
                {
                    app.editor.copy_current_frame();
                }

                let has_copied_frame = app.editor.has_copied_frame();
                let paste_tooltip = if has_copied_frame {
                    "Paste the copied frame after the current frame"
                } else {
                    "Copy a frame before pasting"
                };
                if ui
                    .add_enabled(has_copied_frame, egui::Button::new("Paste Frame"))
                    .on_hover_text(paste_tooltip)
                    .clicked()
                    && app.editor.paste_frame_after_current()
                {
                    app.texture_dirty = true;
                    app.invalidate_onion_skin_cache();
                }

                ui.separator();

                // Frame Selection Track
                let current_frame = app.editor.animation.current_frame_index;
                let frame_count = app.editor.animation.frames.len();

                egui::ScrollArea::horizontal()
                    .id_salt("timeline_frames_scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for i in 0..frame_count {
                                let is_active = i == current_frame;
                                let label = format!("Frame {}", i + 1);

                                let mut button =
                                    egui::Button::new(egui::RichText::new(&label).size(11.0))
                                        .min_size(egui::vec2(54.0, 26.0));

                                if is_active {
                                    button = button.stroke(egui::Stroke::new(
                                        2.0_f32,
                                        ui.visuals().selection.bg_fill,
                                    ));
                                }

                                // One response owns both click and drag. The
                                // previous drag-source wrapper overlaid a
                                // drag-only response on the button, which
                                // swallowed ordinary clicks before selection
                                // could see them.
                                let frame_response = ui.add(button.sense(egui::Sense::click_and_drag()));
                                frame_response.dnd_set_drag_payload(FrameDragPayload {
                                    source_index: i,
                                });
                                let insert_after_target = ctx
                                    .pointer_interact_pos()
                                    .is_some_and(|pointer| {
                                        pointer.x >= frame_response.rect.center().x
                                    });

                                // Give the user a precise insertion marker. The left half of a
                                // target inserts before it, while the right half inserts after
                                // it. The helper translates that visual position into the final
                                // index expected by `EditorState::move_frame`.
                                if let Some(payload) =
                                    frame_response.dnd_hover_payload::<FrameDragPayload>()
                                {
                                    if frame_drop_destination(
                                        payload.source_index,
                                        i,
                                        insert_after_target,
                                        frame_count,
                                    )
                                    .is_some()
                                    {
                                        let insertion_x = if insert_after_target {
                                            frame_response.rect.right()
                                        } else {
                                            frame_response.rect.left()
                                        };
                                        ui.painter().vline(
                                            insertion_x,
                                            frame_response.rect.y_range(),
                                            egui::Stroke::new(
                                                2.0_f32,
                                                ui.visuals().selection.bg_fill,
                                            ),
                                        );
                                    }
                                }

                                let received_frame_drop = if let Some(payload) =
                                    frame_response.dnd_release_payload::<FrameDragPayload>()
                                {
                                    if let Some(destination) = frame_drop_destination(
                                        payload.source_index,
                                        i,
                                        insert_after_target,
                                        frame_count,
                                    )
                                    {
                                        if app.editor.move_frame(payload.source_index, destination)
                                        {
                                            app.texture_dirty = true;
                                            app.invalidate_onion_skin_cache();
                                        }
                                    }
                                    true
                                } else {
                                    false
                                };

                                // A drop must not also be treated as a click
                                // on its target. `move_frame` deliberately
                                // preserves the logical active frame, while a
                                // click would incorrectly select the target.
                                if !received_frame_drop
                                    && frame_response
                                    .on_hover_text(
                                        "Select frame. Drag before or after another frame to reorder.",
                                    )
                                    .clicked()
                                {
                                    app.editor.select_frame(i);
                                    app.texture_dirty = true;
                                }
                            }
                        });
                    });
            });
        });
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
