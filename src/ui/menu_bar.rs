use crate::app::PixelBuddyApp;
use crate::io;


pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    app.show_new_dialog = true;
                    ui.close_menu();
                }
                if ui.button("Open").clicked() {
                    io::trigger_open_file(app.io_handler.sender.clone());
                    ui.close_menu();
                }
                if ui.button("Save as PNG").clicked() {
                    // Export is effectively Save as PNG for now
                    if let Some(png_data) = io::png::export_document_to_png(&app.editor.document) {
                        io::trigger_export_png(png_data, app.io_handler.sender.clone());
                    }
                    ui.close_menu();
                }
                if ui.button("Export PNG").clicked() {
                    if let Some(png_data) = io::png::export_document_to_png(&app.editor.document) {
                        io::trigger_export_png(png_data, app.io_handler.sender.clone());
                    }
                    ui.close_menu();
                }
                if ui.button("Export Animated GIF").clicked() {
                    app.editor.save_current_frame();
                    if let Some(gif_data) = io::gif::export_animation_to_gif(&app.editor.animation) {
                        io::trigger_export_png(gif_data, app.io_handler.sender.clone());
                    }
                    ui.close_menu();
                }
                if ui.button("Export Sprite Sheet (PNG)").clicked() {
                    app.editor.save_current_frame();
                    if let Some(sheet_data) = io::spritesheet::export_spritesheet_png(&app.editor.animation) {
                        io::trigger_export_png(sheet_data, app.io_handler.sender.clone());
                    }
                    ui.close_menu();
                }
            });
            
            ui.menu_button("Edit", |ui| {
                if ui.button("Undo (Ctrl+Z)").clicked() {
                    app.editor.history.undo(&mut app.editor.document);
                    app.texture_dirty = true;
                    ui.close_menu();
                }
                if ui.button("Redo (Ctrl+Y)").clicked() {
                    app.editor.history.redo(&mut app.editor.document);
                    app.texture_dirty = true;
                    ui.close_menu();
                }
                if ui.button("Swap Colors (X)").clicked() {
                    app.editor.swap_colors();
                    ui.close_menu();
                }
            });
            
            ui.menu_button("View", |ui| {
                if ui.checkbox(&mut app.show_grid, "Toggle Grid").clicked() {
                    ui.close_menu();
                }
                if ui.checkbox(&mut app.show_timeline, "Animation Timeline").clicked() {
                    ui.close_menu();
                }
                if ui.button("Zoom In").clicked() {
                    app.zoom *= 2.0;
                    if app.zoom > 64.0 { app.zoom = 64.0; }
                    ui.close_menu();
                }
                if ui.button("Zoom Out").clicked() {
                    app.zoom *= 0.5;
                    if app.zoom < 0.5 { app.zoom = 0.5; }
                    ui.close_menu();
                }
                if ui.button("Fit to Window").clicked() {
                    app.auto_fit_requested = true;
                    ui.close_menu();
                }
            });

            ui.menu_button("Settings", |ui| {
                ui.label(egui::RichText::new("New Canvas Presets").strong());
                ui.horizontal(|ui| {
                    for (label, dim) in [("16×16", 16), ("32×32", 32), ("64×64", 64), ("128×128", 128)] {
                        if ui.button(label).clicked() {
                            app.editor = crate::editor::EditorState::new(dim, dim);
                            app.pan_offset = egui::Vec2::ZERO;
                            app.auto_fit_requested = true;
                            app.texture_dirty = true;
                            ui.close_menu();
                        }
                    }
                });

                ui.separator();
                ui.label(egui::RichText::new("Canvas & Viewport").strong());
                ui.checkbox(&mut app.show_grid, "Show Pixel Grid");
                ui.checkbox(&mut app.show_timeline, "Show Animation Timeline");
                if ui.button("Fit Canvas to Viewport").clicked() {
                    app.auto_fit_requested = true;
                    ui.close_menu();
                }

                ui.separator();
                ui.label(egui::RichText::new("Tool Defaults").strong());
                ui.horizontal(|ui| {
                    ui.label("Fill Tolerance:");
                    let mut tol = app.fill_tolerance as i32;
                    if ui.add(egui::Slider::new(&mut tol, 0..=255)).changed() {
                        app.fill_tolerance = tol as u8;
                    }
                });
                ui.checkbox(&mut app.fill_contiguous, "Contiguous Fill");
                ui.checkbox(&mut app.shape_filled, "Fill Shapes (Rect/Ellipse)");

                ui.separator();
                if ui.button("Reset Viewport Pan").clicked() {
                    app.pan_offset = egui::Vec2::ZERO;
                    app.auto_fit_requested = true;
                    ui.close_menu();
                }
            });
        });

        // Top Contextual Tool Options Bar
        ui.separator();
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            match app.editor.active_tool {
                crate::editor::ToolType::Fill => {
                    ui.label(egui::RichText::new("Flood Fill").strong().size(11.0));
                    ui.separator();
                    ui.label(egui::RichText::new("Tolerance:").size(11.0));
                    let mut tol = app.fill_tolerance as i32;
                    if ui.add(egui::Slider::new(&mut tol, 0..=255)).changed() {
                        app.fill_tolerance = tol as u8;
                    }
                    ui.checkbox(&mut app.fill_contiguous, "Contiguous Fill");
                }
                crate::editor::ToolType::Rectangle | crate::editor::ToolType::Ellipse => {
                    ui.label(egui::RichText::new("Shape Tool").strong().size(11.0));
                    ui.separator();
                    ui.checkbox(&mut app.shape_filled, "Fill Interior");
                }
                _ => {}
            }
        });
    });
}
