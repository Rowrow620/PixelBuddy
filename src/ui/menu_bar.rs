use crate::app::PixelBuddyApp;
use crate::io;

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(15, 15, 25))
        .inner_margin(egui::Margin::symmetric(8, 2));

    egui::TopBottomPanel::top("menu_bar")
        .frame(frame)
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        app.show_new_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button("Open Project (.pbud)").clicked() {
                        io::trigger_open_project(app.io_handler.sender.clone());
                        ui.close_menu();
                    }
                    if ui.button("Save Project (.pbud)").clicked() {
                        io::trigger_save_project(&app.editor, app.io_handler.sender.clone());
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Import Image (PNG/WebP)").clicked() {
                        io::trigger_open_file(app.io_handler.sender.clone());
                        ui.close_menu();
                    }
                    if ui.button("Export PNG...").clicked() {
                        app.open_png_export_dialog();
                        ui.close_menu();
                    }
                    if ui.button("Export WebP...").clicked() {
                        app.open_webp_export_dialog();
                        ui.close_menu();
                    }
                    if ui.button("Export Animated GIF...").clicked() {
                        app.open_gif_export_dialog();
                        ui.close_menu();
                    }
                    if ui.button("Export Sprite Sheet (PNG)...").clicked() {
                        app.open_sprite_sheet_export_dialog();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo (Ctrl+Z)").clicked() {
                        if app.editor.undo() {
                            app.texture_dirty = true;
                        }
                        ui.close_menu();
                    }
                    if ui.button("Redo (Ctrl+Y)").clicked() {
                        if app.editor.redo() {
                            app.texture_dirty = true;
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Cut (Ctrl+X)").clicked() {
                        let clipboard = crate::editor::clipboard::ClipboardBuffer::copy(
                            app.editor.document(),
                            &app.editor.selection,
                        );
                        app.editor.clipboard = clipboard;
                        if app.editor.clipboard.is_some() {
                            app.clear_selection();
                        }
                        ui.close_menu();
                    }
                    if ui.button("Copy (Ctrl+C)").clicked() {
                        let clipboard = crate::editor::clipboard::ClipboardBuffer::copy(
                            app.editor.document(),
                            &app.editor.selection,
                        );
                        app.editor.clipboard = clipboard;
                        ui.close_menu();
                    }
                    if ui.button("Paste (Ctrl+V)").clicked() {
                        if let Some(buf) = &app.editor.clipboard.clone() {
                            let (origin_x, origin_y) = app.paste_origin(buf.width, buf.height);
                            let mut changes = Vec::new();
                            for y in 0..buf.height {
                                for x in 0..buf.width {
                                    let idx = (y * buf.width + x) as usize;
                                    let color = buf.pixels[idx];
                                    if color[3] > 0 {
                                        changes.push((
                                            origin_x.saturating_add(x),
                                            origin_y.saturating_add(y),
                                            color,
                                        ));
                                    }
                                }
                            }
                            app.apply_tool_changes(changes);
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Select All (Ctrl+A)").clicked() {
                        app.editor.selection.set_rect(
                            0,
                            0,
                            (app.editor.document().width as i32) - 1,
                            (app.editor.document().height as i32) - 1,
                        );
                        ui.close_menu();
                    }
                    if ui.button("Deselect (Ctrl+D)").clicked() {
                        app.editor.selection.deselect();
                        ui.close_menu();
                    }
                    if ui.button("Clear Selection (Del)").clicked() {
                        app.clear_selection();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Flip Horizontal").clicked() {
                        app.flip_horizontal();
                        ui.close_menu();
                    }
                    if ui.button("Flip Vertical").clicked() {
                        app.flip_vertical();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Merge Down").clicked() {
                        app.merge_down();
                        ui.close_menu();
                    }
                    if ui.button("Flatten Visible").clicked() {
                        app.flatten_visible();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Swap Colors (X)").clicked() {
                        app.editor.swap_colors();
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut app.show_grid, "Toggle Grid").clicked() {
                        ui.close_menu();
                    }
                    if ui
                        .checkbox(&mut app.show_timeline, "Animation Timeline")
                        .clicked()
                    {
                        ui.close_menu();
                    }
                    if ui.button("Zoom In").clicked() {
                        app.zoom *= 2.0;
                        if app.zoom > 64.0 {
                            app.zoom = 64.0;
                        }
                        ui.close_menu();
                    }
                    if ui.button("Zoom Out").clicked() {
                        app.zoom *= 0.5;
                        if app.zoom < 0.5 {
                            app.zoom = 0.5;
                        }
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
                        for (label, dim) in [
                            ("16×16", 16),
                            ("32×32", 32),
                            ("64×64", 64),
                            ("128×128", 128),
                        ] {
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
                    ui.label(egui::RichText::new("Resize Existing Canvas").strong());
                    ui.horizontal(|ui| {
                        for (label, dim) in [
                            ("16×16", 16),
                            ("32×32", 32),
                            ("64×64", 64),
                            ("128×128", 128),
                        ] {
                            if ui.button(label).clicked() {
                                app.pending_resize = Some((dim, dim));
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

                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard Shortcuts").clicked() {
                        app.show_help_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button("About PixelBuddy").clicked() {
                        app.show_about_dialog = true;
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text_color = ui.visuals().text_color();
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(egui::include_image!(
                                "../../assets/icons/win-close.svg"
                            ))
                            .tint(text_color),
                        ))
                        .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(egui::include_image!(
                                "../../assets/icons/win-max.svg"
                            ))
                            .tint(text_color),
                        ))
                        .clicked()
                    {
                        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                    }
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(egui::include_image!(
                                "../../assets/icons/win-min.svg"
                            ))
                            .tint(text_color),
                        ))
                        .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }

                    // The remaining space is draggable
                    let response =
                        ui.allocate_response(ui.available_size(), egui::Sense::click_and_drag());
                    if response.dragged() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if response.double_clicked() {
                        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                    }
                });
            });
        });
}
