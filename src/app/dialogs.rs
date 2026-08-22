use super::*;

impl PixelBuddyApp {

    pub(super) fn draw_palette_policy_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Palette:");
            egui::ComboBox::from_id_salt("palette_policy")
                .selected_text(match &self.new_project_palette_policy {
                    PalettePolicy::KeepCurrent => "Keep current palette".to_owned(),
                    PalettePolicy::UseDefault => "Use default palette".to_owned(),
                    PalettePolicy::UsePreset(id) => crate::document::palette_library::get_preset(id)
                        .map(|p| p.name.to_owned())
                        .unwrap_or_else(|| "Unknown preset".to_owned()),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.new_project_palette_policy,
                        PalettePolicy::KeepCurrent,
                        "Keep current palette",
                    );
                    ui.selectable_value(
                        &mut self.new_project_palette_policy,
                        PalettePolicy::UseDefault,
                        "Use default palette",
                    );
                    for preset in crate::document::palette_library::PRESETS {
                        ui.selectable_value(
                            &mut self.new_project_palette_policy,
                            PalettePolicy::UsePreset(preset.id.to_string()),
                            preset.name,
                        );
                    }
                });
        });
    }

    pub(super) fn show_project_lifecycle_dialogs(&mut self, ctx: &egui::Context) {
        if self.show_close_confirmation {
            self.show_close_confirmation(ctx);
            return;
        }
        self.show_recovery_dialog(ctx);
        self.show_replace_confirmation(ctx);
        if self.show_spritesheet_import_dialog {
            self.show_spritesheet_import_dialog_ui(ctx);
        }
        if self.show_image_import_dialog {
            self.show_image_import_dialog_ui(ctx);
        }
    }

    pub(super) fn intercept_dirty_close_request(&mut self, ctx: &egui::Context) {
        if self.allow_close || !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.editor.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_close_confirmation = true;
        }
    }

    fn show_close_confirmation(&mut self, ctx: &egui::Context) {
        let mut discard_and_close = false;
        let mut keep_editing = false;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("You have unsaved project changes.");
                ui.label("Save the project first if you want to keep them.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Keep editing").clicked() {
                        keep_editing = true;
                    }
                    if ui
                        .button(
                            egui::RichText::new("Discard and close")
                                .color(egui::Color32::from_rgb(248, 113, 113)),
                        )
                        .clicked()
                    {
                        discard_and_close = true;
                    }
                });
            });

        if discard_and_close {
            self.recovery_snapshot = None;
            self.editor.mark_saved();
            self.allow_close = true;
            self.show_close_confirmation = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if keep_editing {
            self.show_close_confirmation = false;
        }
    }

    fn show_recovery_dialog(&mut self, ctx: &egui::Context) {
        if self.recovery_snapshot.is_none() || self.pending_replacement.is_some() {
            return;
        }

        let mut restore = false;
        let mut discard = false;
        egui::Window::new("Recover unsaved work?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("PixelBuddy found a local snapshot from an unsaved editing session.");
                ui.label("Restore it to continue working, or discard it and start fresh.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Restore draft").clicked() {
                        restore = true;
                    }
                    if ui.button("Discard snapshot").clicked() {
                        discard = true;
                    }
                });
            });

        if restore {
            let decoded = crate::io::project::decode_editor(
                self.recovery_snapshot
                    .as_deref()
                    .expect("recovery snapshot was checked before opening the dialog"),
            );
            match decoded {
                Ok(editor) => self.request_recovered_project(editor),
                Err(error) => {
                    self.recovery_snapshot = None;
                    log::error!("Unable to recover local PixelBuddy draft: {error}");
                    self.status_message =
                        Some((format!("Could not recover the local draft: {error}"), true));
                }
            }
        } else if discard {
            self.recovery_snapshot = None;
        }
    }

    fn show_replace_confirmation(&mut self, ctx: &egui::Context) {
        if self.pending_replacement.is_none() {
            return;
        }

        let mut discard_changes = false;
        let mut cancel = false;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("This action will replace your current project.");
                ui.label("Save the project first if you want to keep its latest changes.");
                ui.add_space(8.0);
                
                let palette_policy = match self.pending_replacement.as_ref().unwrap() {
                    DocumentReplacement::NewDocument { palette_policy, .. } => Some(palette_policy),
                    DocumentReplacement::ImportedImage { palette_policy, .. } => Some(palette_policy),
                    DocumentReplacement::ImportedAnimation { palette_policy, .. } => Some(palette_policy),
                    _ => None,
                };
                
                if let Some(policy) = palette_policy {
                    let (name, colors) = match policy {
                        PalettePolicy::KeepCurrent => {
                            ("Keeping current palette".to_owned(), self.editor.document().palette.colors.clone())
                        },
                        PalettePolicy::UseDefault => {
                            let p = crate::document::palette_library::default_preset();
                            (p.name.to_owned(), p.colors.iter().copied().collect())
                        },
                        PalettePolicy::UsePreset(id) => {
                            if let Some(p) = crate::document::palette_library::get_preset(id) {
                                (p.name.to_owned(), p.colors.iter().copied().collect())
                            } else {
                                let p = crate::document::palette_library::default_preset();
                                (p.name.to_owned(), p.colors.iter().copied().collect())
                            }
                        }
                    };
                    
                    ui.label(egui::RichText::new(format!("Applying Palette: {}", name)).strong());
                    ui.horizontal(|ui| {
                        let swatch_size = egui::Vec2::splat(12.0);
                        for &c in colors.iter().take(16) {
                            let color = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                            let (rect, _response) = ui.allocate_exact_size(swatch_size, egui::Sense::hover());
                            ui.painter().rect_filled(rect, 0.0, color);
                        }
                        if colors.len() > 16 {
                            ui.label("...");
                        }
                    });
                    ui.add_space(8.0);
                }

                ui.horizontal(|ui| {
                    if ui.button("Keep editing").clicked() {
                        cancel = true;
                    }
                    if ui
                        .button(
                            egui::RichText::new("Discard changes")
                                .color(egui::Color32::from_rgb(248, 113, 113)),
                        )
                        .clicked()
                    {
                        discard_changes = true;
                    }
                });
            });

        if discard_changes {
            self.confirm_pending_document_replacement();
        } else if cancel {
            self.cancel_pending_document_replacement();
        }
    }


    fn show_image_import_dialog_ui(&mut self, ctx: &egui::Context) {
        if !self.show_image_import_dialog {
            return;
        }

        let mut close = false;
        let mut perform_import = false;

        egui::Window::new("Import Image as New Project")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "File: {}",
                    self.image_import_file_name
                        .as_deref()
                        .unwrap_or("Unknown")
                ));
                ui.add_space(8.0);

                self.draw_palette_policy_selector(ui);

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Import").clicked() {
                        perform_import = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if perform_import {
            if let (Some(doc), Some(name)) = (
                self.image_import_document.take(),
                self.image_import_file_name.take(),
            ) {
                self.request_imported_image(doc, name, self.new_project_palette_policy.clone());
            }
            close = true;
        }

        if close {
            self.show_image_import_dialog = false;
            self.image_import_document = None;
            self.image_import_file_name = None;
        }
    }

    fn show_spritesheet_import_dialog_ui(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut perform_import = false;

        egui::Window::new("Import Sprite Sheet")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                if let Some(texture) = &self.spritesheet_import_texture {
                    ui.vertical_centered(|ui| {
                        let mut size = texture.size_vec2();
                        let max_size = 256.0;
                        if size.x > max_size || size.y > max_size {
                            let scale = (max_size / size.x).min(max_size / size.y);
                            size *= scale;
                        }

                        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        ui.painter().image(
                            texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    });
                    ui.add_space(8.0);
                }

                ui.label(format!(
                    "File: {}",
                    self.spritesheet_import_data
                        .as_ref()
                        .map(|d| d.1.as_str())
                        .unwrap_or("Unknown")
                ));
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Columns (Horizontal Frames):");
                    ui.text_edit_singleline(&mut self.spritesheet_import_columns);
                });
                ui.horizontal(|ui| {
                    ui.label("Rows (Vertical Frames):");
                    ui.text_edit_singleline(&mut self.spritesheet_import_rows);
                });

                ui.add_space(8.0);
                ui.label("Import Mode:");
                ui.radio_value(
                    &mut self.spritesheet_import_mode,
                    SpriteSheetImportMode::NewProject,
                    "New Project",
                );
                if self.spritesheet_import_mode == SpriteSheetImportMode::NewProject {
                    self.draw_palette_policy_selector(ui);
                }
                ui.radio_value(
                    &mut self.spritesheet_import_mode,
                    SpriteSheetImportMode::AppendFrames,
                    "Append as New Frames",
                );
                ui.radio_value(
                    &mut self.spritesheet_import_mode,
                    SpriteSheetImportMode::ActiveLayer,
                    "Import into Active Layer",
                );

                if let Some(err) = &self.spritesheet_import_error {
                    ui.add_space(8.0);
                    ui.colored_label(egui::Color32::from_rgb(248, 113, 113), err);
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                    if ui.button("Import").clicked() {
                        perform_import = true;
                    }
                });
            });

        if close {
            self.show_spritesheet_import_dialog = false;
            self.spritesheet_import_source_session_id = None;
            self.spritesheet_import_source_revision = None;
            self.spritesheet_import_source_frame_generation = None;
            self.spritesheet_import_source_active_layer_index = None;
            self.spritesheet_import_data = None;
            self.spritesheet_import_texture = None;
            self.spritesheet_import_error = None;
        } else if perform_import {
            if self.spritesheet_import_mode != SpriteSheetImportMode::NewProject
                && !self.current_spritesheet_import_is_current()
            {
                self.spritesheet_import_error = Some(
                    "The active project changed. Choose New Project or cancel this import."
                        .to_owned(),
                );
                return;
            }

            let cols = self.spritesheet_import_columns.parse::<u32>();
            let rows = self.spritesheet_import_rows.parse::<u32>();

            if cols.is_err()
                || rows.is_err()
                || *cols.as_ref().unwrap_or(&0) == 0
                || *rows.as_ref().unwrap_or(&0) == 0
            {
                self.spritesheet_import_error =
                    Some("Columns and Rows must be positive integers.".to_string());
                return;
            }

            let (cols, rows) = (cols.unwrap(), rows.unwrap());

            if let Some((data, file_name)) = self.spritesheet_import_data.take() {
                match crate::io::spritesheet::import_spritesheet(&data, &file_name, cols, rows) {
                    Ok(animation) => {
                        self.show_spritesheet_import_dialog = false;
                        self.spritesheet_import_error = None;

                        match self.spritesheet_import_mode {
                            SpriteSheetImportMode::NewProject => {
                                self.spritesheet_import_source_session_id = None;
                                self.spritesheet_import_source_revision = None;
                                self.spritesheet_import_source_frame_generation = None;
                                self.spritesheet_import_source_active_layer_index = None;
                                self.spritesheet_import_texture = None;
                                self.request_imported_animation(animation, file_name, self.new_project_palette_policy.clone());
                            }
                            SpriteSheetImportMode::AppendFrames => {
                                let doc_width = self.editor.document().width;
                                let doc_height = self.editor.document().height;

                                let first_frame = &animation.frames[0];
                                if first_frame.document.width != doc_width
                                    || first_frame.document.height != doc_height
                                {
                                    self.spritesheet_import_error = Some(format!(
                                        "Spritesheet frames are {}x{}, but current document is {}x{}. Dimensions must match exactly to import as frames.",
                                        first_frame.document.width, first_frame.document.height, doc_width, doc_height
                                    ));
                                    self.spritesheet_import_data = Some((data, file_name));
                                    self.show_spritesheet_import_dialog = true;
                                    return;
                                }

                                if !self.append_imported_animation_frames(animation) {
                                    self.spritesheet_import_error = Some(format!(
                                        "This import would exceed the {}-frame animation limit.",
                                        crate::document::animation::MAX_ANIMATION_FRAMES
                                    ));
                                    self.spritesheet_import_data = Some((data, file_name));
                                    self.show_spritesheet_import_dialog = true;
                                    return;
                                }
                                self.spritesheet_import_source_session_id = None;
                                self.spritesheet_import_source_revision = None;
                                self.spritesheet_import_source_frame_generation = None;
                                self.spritesheet_import_source_active_layer_index = None;
                                self.spritesheet_import_texture = None;
                                self.status_message =
                                    Some(("Appended sprite sheet frames".to_string(), false));
                            }
                            SpriteSheetImportMode::ActiveLayer => {
                                let doc_width = self.editor.document().width;
                                let doc_height = self.editor.document().height;

                                let first_frame = &animation.frames[0];
                                if first_frame.document.width != doc_width
                                    || first_frame.document.height != doc_height
                                {
                                    self.spritesheet_import_error = Some(format!(
                                        "Spritesheet frames are {}x{}, but current document is {}x{}. Dimensions must match exactly.",
                                        first_frame.document.width, first_frame.document.height, doc_width, doc_height
                                    ));
                                    self.spritesheet_import_data = Some((data, file_name));
                                    self.show_spritesheet_import_dialog = true;
                                    return;
                                }

                                let affected_frames =
                                    match self.import_animation_into_active_layer(animation) {
                                        Ok(affected_frames) => affected_frames,
                                        Err(error) => {
                                            self.spritesheet_import_error = Some(error);
                                            self.spritesheet_import_data = Some((data, file_name));
                                            self.show_spritesheet_import_dialog = true;
                                            return;
                                        }
                                    };
                                self.spritesheet_import_source_session_id = None;
                                self.spritesheet_import_source_revision = None;
                                self.spritesheet_import_source_frame_generation = None;
                                self.spritesheet_import_source_active_layer_index = None;
                                self.spritesheet_import_texture = None;
                                self.status_message = Some((
                                    format!(
                                        "Imported sprite sheet into active layer across {affected_frames} frame(s)"
                                    ),
                                    false,
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        self.spritesheet_import_error = Some(e.to_string());
                        self.spritesheet_import_data = Some((data, file_name));
                    }
                }
            }
        }
    }

    pub(super) fn show_export_resolution_dialog(&mut self, ctx: &egui::Context) {
        let Some(kind) = self
            .export_resolution_dialog
            .as_ref()
            .map(|dialog| dialog.kind)
        else {
            return;
        };
        let source_dimensions = self.raster_export_source_dimensions(kind);
        let mut open = true;
        let mut cancel = false;
        let mut export_requested = false;
        let mut selected_dimensions = None;

        {
            let dialog = self
                .export_resolution_dialog
                .as_mut()
                .expect("export dialog was checked before rendering");
            egui::Window::new(kind.dialog_title())
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_min_width(330.0);
                    ui.label(kind.description());
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Nearest-neighbor scaling keeps every pixel crisp.",
                        )
                        .small()
                        .color(ui.visuals().weak_text_color()),
                    );
                    ui.separator();

                    if let Some((frame_width, frame_height, frame_count)) = source_dimensions {
                        match kind {
                            RasterExportKind::Png | RasterExportKind::WebP => {
                                ui.label(format!("Source: {frame_width} × {frame_height} px"));
                            }
                            RasterExportKind::Gif => {
                                ui.label(format!(
                                    "Source: {frame_width} × {frame_height} px • {frame_count} frame{}",
                                    if frame_count == 1 { "" } else { "s" }
                                ));
                            }
                            RasterExportKind::SpriteSheetPng => {
                                ui.label(format!(
                                    "Frames: {frame_width} × {frame_height} px • {frame_count} frame{}",
                                    if frame_count == 1 { "" } else { "s" }
                                ));
                            }
                        }
                    } else {
                        ui.colored_label(
                            egui::Color32::from_rgb(248, 113, 113),
                            "There are no animation frames available to export.",
                        );
                    }

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Export size").strong());
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut dialog.sizing,
                            RasterExportSizing::Scale,
                            "Scale",
                        );
                        ui.selectable_value(
                            &mut dialog.sizing,
                            RasterExportSizing::Dimensions,
                            "Exact dimensions",
                        );
                    });

                    ui.add_enabled_ui(dialog.sizing == RasterExportSizing::Scale, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for scale in [1_u32, 2, 4, 8, 16] {
                            let selected = dialog.scale_text.trim() == scale.to_string();
                            if ui
                                .selectable_label(selected, format!("{scale}×"))
                                .on_hover_text(format!("Export at {scale}× the source dimensions"))
                                .clicked()
                            {
                                dialog.scale_text = scale.to_string();
                                if let Some((frame_width, frame_height, frame_count)) =
                                    source_dimensions
                                {
                                    if let Some((width, height)) = kind.output_dimensions(
                                        frame_width,
                                        frame_height,
                                        frame_count,
                                        scale,
                                    ) {
                                        dialog.width_text = width.to_string();
                                        dialog.height_text = height.to_string();
                                    }
                                }
                                dialog.error = None;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Custom:");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut dialog.scale_text)
                                .desired_width(56.0)
                                .hint_text("1"),
                        );
                        ui.label("×");
                        if response.changed() {
                            if let (Ok(scale), Some((frame_width, frame_height, frame_count))) = (
                                parse_raster_export_scale(&dialog.scale_text),
                                source_dimensions,
                            ) {
                                if let Some((width, height)) = kind.output_dimensions(
                                    frame_width,
                                    frame_height,
                                    frame_count,
                                    scale,
                                ) {
                                    dialog.width_text = width.to_string();
                                    dialog.height_text = height.to_string();
                                }
                            }
                            dialog.error = None;
                        }
                    });
                    });

                    ui.add_enabled_ui(dialog.sizing == RasterExportSizing::Dimensions, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Width:");
                            let width_response = ui.add(
                                egui::TextEdit::singleline(&mut dialog.width_text)
                                    .desired_width(72.0)
                                    .hint_text("1024"),
                            );
                            ui.label("×");
                            ui.label("Height:");
                            let height_response = ui.add(
                                egui::TextEdit::singleline(&mut dialog.height_text)
                                    .desired_width(72.0)
                                    .hint_text("1024"),
                            );
                            ui.label("px");
                            if width_response.changed() || height_response.changed() {
                                dialog.error = None;
                            }
                        });
                    });

                    ui.add_space(4.0);
                    let dimensions = match dialog.sizing {
                        RasterExportSizing::Scale => match (
                            parse_raster_export_scale(&dialog.scale_text),
                            source_dimensions,
                        ) {
                            (Ok(scale), Some((frame_width, frame_height, frame_count))) => kind
                                .output_dimensions(
                                frame_width,
                                frame_height,
                                frame_count,
                                scale,
                            )
                            .ok_or_else(|| {
                                "The selected scale overflows the output dimensions.".to_owned()
                            }),
                            (Err(error), _) => Err(error),
                            (_, None) => Err("There are no frames available to export.".to_owned()),
                        },
                        RasterExportSizing::Dimensions => {
                            let width = parse_raster_export_dimension(&dialog.width_text, "width");
                            let height =
                                parse_raster_export_dimension(&dialog.height_text, "height");
                            match (width, height) {
                                (Ok(width), Ok(height)) => Ok((width, height)),
                                (Err(error), _) | (_, Err(error)) => Err(error),
                            }
                        }
                    };

                    match dimensions {
                        Ok((output_width, output_height)) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Output: {output_width} × {output_height} px"
                                ))
                                .strong(),
                            );
                            match crate::io::validate_canvas_dimensions(output_width, output_height)
                            {
                                Ok(()) => selected_dimensions = Some((output_width, output_height)),
                                Err(error) => {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(248, 113, 113),
                                        error.to_string(),
                                    );
                                }
                            }
                        }
                        Err(error) if source_dimensions.is_some() => {
                            ui.colored_label(egui::Color32::from_rgb(248, 113, 113), error);
                        }
                        Err(_) => {}
                    }

                    if let Some(error) = &dialog.error {
                        ui.add_space(4.0);
                        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), error);
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        let can_export = source_dimensions.is_some()
                            && selected_dimensions.is_some();
                        if ui
                            .add_enabled(
                                can_export,
                                egui::Button::new(kind.export_button_label()),
                            )
                            .clicked()
                        {
                            export_requested = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
        }

        if !open || cancel {
            self.export_resolution_dialog = None;
            return;
        }

        if export_requested {
            if let Some((width, height)) = selected_dimensions {
                match self.export_raster_at_dimensions(kind, width, height) {
                    Ok(()) => self.export_resolution_dialog = None,
                    Err(error) => {
                        if let Some(dialog) = &mut self.export_resolution_dialog {
                            dialog.error = Some(error);
                        }
                    }
                }
            }
        }
    }
}
