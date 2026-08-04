use crate::app::PixelBuddyApp;
use crate::document::layer::BlendMode;

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::SidePanel::right("layers_panel")
        .exact_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Layers");
            ui.separator();
            
            let layers_count = app.editor.document.layers.len();
            let active_idx = app.editor.document.active_layer_index;
            let mut new_active = active_idx;
            let mut visibility_changes: Vec<(usize, bool)> = Vec::new();

            egui::ScrollArea::vertical().id_salt("layers_scroll").max_height(250.0).show(ui, |ui| {
                // Iterate in reverse for Photoshop-like display (top layer listed first)
                for i in (0..layers_count).rev() {
                    let is_active = i == active_idx;
                    let layer_name = app.editor.document.layers[i].name.clone();
                    let layer_visible = app.editor.document.layers[i].visible;
                    
                    let mut frame = egui::Frame::NONE
                        .inner_margin(egui::Margin::same(4))
                        .corner_radius(4);
                        
                    if is_active {
                        frame = frame.fill(ui.visuals().selection.bg_fill);
                    }
                    
                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Visibility toggle
                            let vis = if layer_visible { "v" } else { "-" };
                            if ui.small_button(vis).on_hover_text("Toggle visibility").clicked() {
                                visibility_changes.push((i, !layer_visible));
                            }
                            
                            // Layer name — click to select
                            if ui.selectable_label(is_active, &layer_name).clicked() {
                                new_active = i;
                            }
                        });
                    });
                }
            });
            
            // Apply visibility changes
            for (idx, visible) in &visibility_changes {
                app.editor.document.layers[*idx].visible = *visible;
                app.texture_dirty = true;
            }
            
            // Apply active layer selection
            if new_active != active_idx {
                app.editor.document.active_layer_index = new_active;
            }
            
            ui.separator();
            
            // Layer action buttons
            ui.horizontal_wrapped(|ui| {
                if ui.button("+").on_hover_text("Add Layer").clicked() {
                    app.editor.document.add_layer();
                    app.texture_dirty = true;
                }
                if ui.button("Del").on_hover_text("Delete Layer").clicked() {
                    if layers_count > 1 {
                        app.editor.document.remove_layer(app.editor.document.active_layer_index);
                        app.texture_dirty = true;
                    }
                }
                if ui.button("Dup").on_hover_text("Duplicate Layer").clicked() {
                    app.editor.document.duplicate_layer(app.editor.document.active_layer_index);
                    app.texture_dirty = true;
                }
                if ui.button("Up").on_hover_text("Move Up").clicked() {
                    let idx = app.editor.document.active_layer_index;
                    if idx + 1 < layers_count {
                        app.editor.document.move_layer(idx, idx + 1);
                        app.texture_dirty = true;
                    }
                }
                if ui.button("Dn").on_hover_text("Move Down").clicked() {
                    let idx = app.editor.document.active_layer_index;
                    if idx > 0 {
                        app.editor.document.move_layer(idx, idx - 1);
                        app.texture_dirty = true;
                    }
                }
            });
            
            ui.separator();
            
            // Active layer properties
            if layers_count > 0 {
                let active = app.editor.document.active_layer_index;
                
                ui.label("Opacity");
                let mut opacity = app.editor.document.layers[active].opacity;
                if ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).fixed_decimals(2)).changed() {
                    app.editor.document.layers[active].opacity = opacity;
                    app.texture_dirty = true;
                }
                
                ui.label("Blend Mode");
                let current_mode = app.editor.document.layers[active].blend_mode;
                let mode_label = match current_mode {
                    BlendMode::Normal => "Normal",
                    BlendMode::Multiply => "Multiply",
                    BlendMode::Screen => "Screen",
                    BlendMode::Overlay => "Overlay",
                };
                egui::ComboBox::from_id_salt("blend_mode")
                    .selected_text(mode_label)
                    .show_ui(ui, |ui| {
                        for (mode, label) in [
                            (BlendMode::Normal, "Normal"),
                            (BlendMode::Multiply, "Multiply"),
                            (BlendMode::Screen, "Screen"),
                            (BlendMode::Overlay, "Overlay"),
                        ] {
                            if ui.selectable_label(current_mode == mode, label).clicked() {
                                app.editor.document.layers[active].blend_mode = mode;
                                app.texture_dirty = true;
                            }
                        }
                    });
            }

            ui.add_space(12.0);
            ui.heading("Palette");
            ui.separator();

            let selected = app.editor.document.palette.selected_index;
            let palette_len = app.editor.document.palette.colors.len();

            egui::ScrollArea::vertical().id_salt("palette_scroll").max_height(220.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for i in 0..palette_len {
                        let color = app.editor.document.palette.colors[i];
                        let egui_color = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                        
                        let stroke = if i == selected {
                            egui::Stroke::new(2.0_f32, ui.visuals().selection.bg_fill)
                        } else {
                            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60))
                        };
                        
                        let button = egui::Button::new("  ")
                            .fill(egui_color)
                            .stroke(stroke)
                            .min_size(egui::vec2(22.0, 22.0));
                        
                        let response = ui.add(button);
                        if response.clicked() {
                            app.editor.document.palette.set_selected(i);
                            app.editor.set_primary_color(color);
                        }
                        if response.secondary_clicked() {
                            app.editor.secondary_color = color;
                        }
                    }
                });
            });

            ui.add_space(6.0);
            if ui.button("+ Add Color").on_hover_text("Add active primary color to palette").clicked() {
                app.editor.document.palette.add_color(app.editor.primary_color);
            }

            ui.separator();
            ui.collapsing("Undo History", |ui| {
                let undo_descs = app.editor.history.undo_descriptions();
                let redo_descs = app.editor.history.redo_descriptions();
                
                egui::ScrollArea::vertical().id_salt("history_scroll").max_height(140.0).show(ui, |ui| {
                    if undo_descs.is_empty() && redo_descs.is_empty() {
                        ui.label(egui::RichText::new("No actions yet").small().color(egui::Color32::GRAY));
                    } else {
                        for (idx, desc) in undo_descs.iter().enumerate() {
                            let is_latest = idx + 1 == undo_descs.len();
                            let text = format!("{}. {}", idx + 1, desc);
                            let label = if is_latest {
                                egui::RichText::new(&text).strong().color(egui::Color32::WHITE)
                            } else {
                                egui::RichText::new(&text).color(egui::Color32::LIGHT_GRAY)
                            };
                            if ui.selectable_label(is_latest, label).on_hover_text("Click to jump to this point in history").clicked() {
                                app.editor.history.jump_to_undo_index(idx, &mut app.editor.document);
                                app.texture_dirty = true;
                            }
                        }
                        for desc in redo_descs.iter().rev() {
                            let text = format!("↷ {}", desc);
                            ui.label(egui::RichText::new(&text).italics().color(egui::Color32::DARK_GRAY));
                        }
                    }
                });
            });
        });
}
