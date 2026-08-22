#[cfg(not(target_arch = "wasm32"))]
use crate::app::WindowPresentation;
use crate::app::{
    PixelBuddyApp, TileMode, MAX_CANVAS_ZOOM, MAX_TILE_PREVIEW_COUNT, MIN_CANVAS_ZOOM,
};
use crate::io;

pub fn show(ctx: &egui::Context, app: &mut PixelBuddyApp) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(15, 15, 25))
        // Keep title-bar controls below the six-pixel native resize strip.
        .inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 6,
            bottom: 2,
        });

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
                        app.command_save_project_as();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Open Image (PNG/WebP)...").clicked() {
                        io::trigger_open_file(
                            app.io_handler.sender.clone(),
                            true,
                            app.document_session_id(),
                            app.active_frame_generation(),
                        );
                        ui.close_menu();
                    }
                    if ui.button("Open Sprite Sheet...").clicked() {
                        io::trigger_open_spritesheet(
                            app.io_handler.sender.clone(),
                            true,
                            app.document_session_id(),
                            app.editor.revision(),
                            app.active_frame_generation(),
                            app.editor.document().active_layer_index,
                        );
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Import Image to Current Frame...").clicked() {
                        io::trigger_open_file(
                            app.io_handler.sender.clone(),
                            false,
                            app.document_session_id(),
                            app.active_frame_generation(),
                        );
                        ui.close_menu();
                    }
                    if ui.button("Import Sprite Sheet as New Frames...").clicked() {
                        io::trigger_open_spritesheet(
                            app.io_handler.sender.clone(),
                            false,
                            app.document_session_id(),
                            app.editor.revision(),
                            app.active_frame_generation(),
                            app.editor.document().active_layer_index,
                        );
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
                        app.undo_current_frame();
                        ui.close_menu();
                    }
                    if ui.button("Redo (Ctrl+Y)").clicked() {
                        app.redo_current_frame();
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
                    let merge_reason = app.merge_down_unavailable_reason();
                    let merge_response = ui
                        .add_enabled(merge_reason.is_none(), egui::Button::new("Merge Down"))
                        .on_hover_text(merge_reason.unwrap_or(
                            "Merge the selected layer into the layer directly below it",
                        ));
                    if merge_response.clicked() {
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
                    ui.menu_button("Tile Preview", |ui| {
                        ui.set_min_width(220.0);
                        ui.radio_value(&mut app.tile_mode, TileMode::None, "Off");
                        ui.radio_value(&mut app.tile_mode, TileMode::XAxis, "Horizontal only");
                        ui.radio_value(&mut app.tile_mode, TileMode::YAxis, "Vertical only");
                        ui.radio_value(&mut app.tile_mode, TileMode::Both, "Both axes");

                        ui.separator();
                        let columns_enabled =
                            matches!(app.tile_mode, TileMode::XAxis | TileMode::Both);
                        let rows_enabled =
                            matches!(app.tile_mode, TileMode::YAxis | TileMode::Both);

                        let mut columns = app.tile_preview.columns();
                        if ui
                            .add_enabled(
                                columns_enabled,
                                egui::Slider::new(&mut columns, 1..=MAX_TILE_PREVIEW_COUNT)
                                    .text("Columns"),
                            )
                            .changed()
                        {
                            app.tile_preview.set_columns(columns);
                        }

                        let mut rows = app.tile_preview.rows();
                        if ui
                            .add_enabled(
                                rows_enabled,
                                egui::Slider::new(&mut rows, 1..=MAX_TILE_PREVIEW_COUNT)
                                    .text("Rows"),
                            )
                            .changed()
                        {
                            app.tile_preview.set_rows(rows);
                        }

                        let (effective_columns, effective_rows) =
                            app.tile_preview.effective_dimensions(app.tile_mode);
                        ui.label(format!(
                            "Preview: {effective_columns} x {effective_rows} tiles"
                        ));
                        ui.label(
                            egui::RichText::new("Even counts place the extra tile right or down.")
                                .small()
                                .weak(),
                        );

                        if ui
                            .add_enabled(
                                app.tile_mode != TileMode::None,
                                egui::Button::new("Fit Tile Preview"),
                            )
                            .clicked()
                        {
                            app.fit_tile_preview_requested = true;
                            ui.close_menu();
                        }
                    });
                    if ui.checkbox(&mut app.show_grid, "Toggle Grid").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut app.show_rulers, "Toggle Rulers").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut app.show_guides, "Toggle Guides").clicked() {
                        ui.close_menu();
                    }
                    if ui
                        .checkbox(&mut app.show_timeline, "Animation Timeline")
                        .clicked()
                    {
                        ui.close_menu();
                    }
                    if ui.button("Zoom In").clicked() {
                        app.zoom = (app.zoom * 2.0).clamp(MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM);
                        ui.close_menu();
                    }
                    if ui.button("Zoom Out").clicked() {
                        app.zoom = (app.zoom * 0.5).clamp(MIN_CANVAS_ZOOM, MAX_CANVAS_ZOOM);
                        ui.close_menu();
                    }
                    if ui.button("Fit to Window").clicked() {
                        app.auto_fit_requested = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Effects", |ui| {
                    if ui.button("Adjust Color...").clicked() {
                        app.start_effect(crate::effects::EffectType::AdjustColor);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Offset...").clicked() {
                        app.start_effect(crate::effects::EffectType::Offset);
                        ui.close_menu();
                    }
                    if ui.button("Mirror...").clicked() {
                        app.start_effect(crate::effects::EffectType::Mirror);
                        ui.close_menu();
                    }
                    if ui.button("Rotate...").clicked() {
                        app.start_effect(crate::effects::EffectType::Rotate);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Invert Colors...").clicked() {
                        app.start_effect(crate::effects::EffectType::InvertColors);
                        ui.close_menu();
                    }
                    if ui.button("Desaturation...").clicked() {
                        app.start_effect(crate::effects::EffectType::Desaturation);
                        ui.close_menu();
                    }
                    if ui.button("Posterize...").clicked() {
                        app.start_effect(crate::effects::EffectType::Posterize);
                        ui.close_menu();
                    }
                });

                ui.menu_button("Settings", |ui| {
                    ui.label(egui::RichText::new("New Canvas").strong());
                    ui.horizontal(|ui| {
                        for (label, dim) in [
                            ("16×16", 16),
                            ("32×32", 32),
                            ("64×64", 64),
                            ("128×128", 128),
                        ] {
                            if ui.button(label).clicked() {
                                app.request_new_document(dim, dim, crate::app::PalettePolicy::UseDefault);
                                ui.close_menu();
                            }
                        }
                    });

                    if ui.button("Custom Size…").clicked() {
                        app.new_document_error = None;
                        app.show_new_dialog = true;
                        ui.close_menu();
                    }
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

                    if ui.button("Custom Size…").clicked() {
                        let (width, height) = {
                            let document = app.editor.document();
                            (document.width, document.height)
                        };
                        app.resize_width = width.to_string();
                        app.resize_height = height.to_string();
                        app.resize_error = None;
                        app.show_custom_resize_dialog = true;
                        ui.close_menu();
                    }
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

                #[cfg(not(target_arch = "wasm32"))]
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
                    let window_presentation = PixelBuddyApp::window_presentation(ctx);
                    let maximize_or_restore = match window_presentation {
                        WindowPresentation::Windowed => {
                            egui::Image::new(egui::include_image!("../../assets/icons/win-max.svg"))
                        }
                        WindowPresentation::Maximized | WindowPresentation::Fullscreen => {
                            egui::Image::new(egui::include_image!(
                                "../../assets/icons/win-restore.svg"
                            ))
                        }
                    }
                    .tint(text_color);
                    let maximize_or_restore_response = ui
                        .add(egui::Button::image(maximize_or_restore))
                        .on_hover_text(match window_presentation {
                            WindowPresentation::Windowed => "Maximize",
                            WindowPresentation::Maximized | WindowPresentation::Fullscreen => {
                                "Restore"
                            }
                        });
                    if maximize_or_restore_response.clicked() {
                        PixelBuddyApp::toggle_maximize_or_restore(ctx, app);
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
                        PixelBuddyApp::toggle_maximize_or_restore(ctx, app);
                    }
                });
            });
        });
}
