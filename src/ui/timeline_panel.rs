use crate::app::PixelBuddyApp;

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::TopBottomPanel::bottom("timeline_panel")
        .exact_height(58.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Playback Controls
                ui.add_space(4.0);
                let play_icon = if app.editor.animation.is_playing { "⏸" } else { "▶" };
                if ui.button(play_icon).on_hover_text("Play/Pause Animation (Space)").clicked() {
                    app.editor.save_current_frame();
                    app.editor.animation.toggle_play();
                }
                if ui.button("◼").on_hover_text("Stop Animation").clicked() {
                    app.editor.save_current_frame();
                    app.editor.animation.stop();
                    app.editor.select_frame(0);
                    app.texture_dirty = true;
                }

                ui.separator();

                // FPS Control
                ui.label(egui::RichText::new("FPS:").size(11.0));
                let mut fps = app.editor.animation.fps as i32;
                if ui.add(egui::Slider::new(&mut fps, 1..=30)).changed() {
                    app.editor.animation.fps = fps as u32;
                }

                ui.separator();

                // Onion Skin Toggle
                if ui.checkbox(&mut app.editor.animation.onion_skin_enabled, "Onion Skin").clicked() {
                    app.texture_dirty = true;
                }

                ui.separator();

                // Frame Actions
                if ui.button("+ Frame").on_hover_text("Add new blank frame").clicked() {
                    app.editor.save_current_frame();
                    app.editor.animation.add_frame();
                    app.editor.document = app.editor.animation.current_doc().clone();
                    app.texture_dirty = true;
                }
                if ui.button("Dup Frame").on_hover_text("Duplicate current frame").clicked() {
                    app.editor.save_current_frame();
                    app.editor.animation.duplicate_frame();
                    app.editor.document = app.editor.animation.current_doc().clone();
                    app.texture_dirty = true;
                }
                if ui.button("Del Frame").on_hover_text("Delete current frame").clicked() {
                    app.editor.animation.remove_frame();
                    app.editor.document = app.editor.animation.current_doc().clone();
                    app.texture_dirty = true;
                }

                ui.separator();

                // Frame Selection Track
                let current_frame = app.editor.animation.current_frame_index;
                let frame_count = app.editor.animation.frames.len();

                egui::ScrollArea::horizontal().id_salt("timeline_frames_scroll").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for i in 0..frame_count {
                            let is_active = i == current_frame;
                            let label = format!("Frame {}", i + 1);
                            
                            let mut button = egui::Button::new(
                                egui::RichText::new(&label).size(11.0)
                            ).min_size(egui::vec2(54.0, 26.0));

                            if is_active {
                                button = button.stroke(egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill));
                            }

                            if ui.add(button).clicked() {
                                app.editor.select_frame(i);
                                app.texture_dirty = true;
                            }
                        }
                    });
                });
            });
        });
}
