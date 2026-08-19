use crate::document::Document;
use crate::editor::history::DrawCommand;
use crate::editor::{EditorState, ToolType};
use crate::io::{FileAction, IoHandler};
use crate::tools;
use egui::{ColorImage, TextureFilter, TextureHandle, TextureOptions};
use std::collections::BTreeMap;

const RECOVERY_STORAGE_KEY: &str = "pixelbuddy.recovery.v1";
const STATUS_TOAST_DURATION_SECONDS: f64 = 6.0;
/// Keep the notification visually attached to, but outside of, the canvas.
const STATUS_TOAST_CANVAS_GAP: f32 = 10.0;
/// Before the canvas has been laid out, leave room for the fixed-width Layers
/// panel so transient messages do not cover its controls.
const STATUS_TOAST_FALLBACK_RIGHT_INSET: f32 = 216.0;
const STATUS_TOAST_FALLBACK_TOP_INSET: f32 = 48.0;

/// Returns the fullscreen setting to request after a toggle. Unknown viewport
/// state is treated as windowed so the first toggle always enters fullscreen.
fn next_fullscreen_state(current: Option<bool>) -> bool {
    !current.unwrap_or(false)
}

/// Parses the user-facing integer scaling field for flattened raster exports.
/// Project files never use this value because their pixel dimensions are part
/// of the editable document data.
fn parse_raster_export_scale(scale_text: &str) -> Result<u32, String> {
    let scale = scale_text
        .trim()
        .parse::<u32>()
        .map_err(|_| "Enter a whole-number export scale.".to_owned())?;
    if scale == 0 {
        return Err("Export scale must be at least 1×.".to_owned());
    }
    Ok(scale)
}

fn parse_raster_export_dimension(value: &str, name: &str) -> Result<u32, String> {
    let dimension = value
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("Enter a whole-number {name}."))?;
    if dimension == 0 {
        return Err(format!("Export {name} must be at least 1 px."));
    }
    Ok(dimension)
}

/// A user-requested replacement held until unsaved work has been explicitly
/// discarded. Keeping decoded data here prevents an Open action from changing
/// the active project before the confirmation is accepted.
enum PendingReplacement {
    NewDocument {
        width: u32,
        height: u32,
    },
    ImportedImage {
        document: Document,
        file_name: String,
    },
    OpenedProject {
        editor: EditorState,
        file_name: String,
    },
}

/// The flattened raster formats that can be enlarged at export time.
///
/// This intentionally excludes `.pbud`: editable project files always store
/// their native canvas dimensions and never go through raster scaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RasterExportKind {
    Png,
    Gif,
    SpriteSheetPng,
}

impl RasterExportKind {
    const fn dialog_title(self) -> &'static str {
        match self {
            Self::Png => "Export PNG",
            Self::Gif => "Export Animated GIF",
            Self::SpriteSheetPng => "Export Sprite Sheet",
        }
    }

    const fn export_button_label(self) -> &'static str {
        match self {
            Self::Png => "Export PNG",
            Self::Gif => "Export GIF",
            Self::SpriteSheetPng => "Export Sprite Sheet",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Png => "Exports the active frame as a flattened PNG image.",
            Self::Gif => "Exports every animation frame while keeping its current timing.",
            Self::SpriteSheetPng => "Places every animation frame left to right in one PNG image.",
        }
    }

    /// Returns the raster dimensions after nearest-neighbor integer scaling.
    /// A sprite sheet is one horizontal row, so its unscaled width also
    /// includes every animation frame.
    fn output_dimensions(
        self,
        frame_width: u32,
        frame_height: u32,
        frame_count: usize,
        scale: u32,
    ) -> Option<(u32, u32)> {
        if scale == 0 {
            return None;
        }

        let width = match self {
            Self::Png | Self::Gif => frame_width,
            Self::SpriteSheetPng => frame_width.checked_mul(u32::try_from(frame_count).ok()?)?,
        };

        Some((width.checked_mul(scale)?, frame_height.checked_mul(scale)?))
    }
}

/// Transient state for the export-size chooser. It is deliberately separate
/// from the editable document so choosing a raster resolution never dirties a
/// project or changes its `.pbud` contents.
struct RasterExportDialog {
    kind: RasterExportKind,
    sizing: RasterExportSizing,
    scale_text: String,
    width_text: String,
    height_text: String,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RasterExportSizing {
    Scale,
    Dimensions,
}

pub struct PixelBuddyApp {
    pub editor: EditorState,
    pub zoom: f32,
    pub pan_offset: egui::Vec2,
    /// The canvas bounds from the current UI frame. This lets transient UI
    /// elements follow the document as it is panned or zoomed.
    pub canvas_rect: Option<egui::Rect>,
    pub show_grid: bool,
    pub is_drawing: bool,
    pub stroke_points: Vec<(u32, u32)>,
    pub shape_start: Option<(i32, i32)>,
    /// The most recent in-bounds canvas pixel reached by the pointer.
    ///
    /// This doubles as a safe endpoint when a drag is released outside the
    /// canvas and as the default anchor for paste.
    pub last_canvas_pixel: Option<(i32, i32)>,
    pub preview_changes: Vec<tools::PixelChange>,
    pub canvas_texture: Option<TextureHandle>,
    pub checkerboard_texture: Option<TextureHandle>,
    onion_previous_texture: Option<TextureHandle>,
    onion_next_texture: Option<TextureHandle>,
    onion_texture_pair: Option<(usize, usize)>,
    pub texture_dirty: bool,
    pub show_new_dialog: bool,
    pub pending_resize: Option<(u32, u32)>,
    pub new_width: String,
    pub new_height: String,
    pub fill_tolerance: u8,
    pub fill_contiguous: bool,
    pub shape_filled: bool,
    pub io_handler: IoHandler,
    pub auto_fit_requested: bool,
    pub show_timeline: bool,
    pub new_document_error: Option<String>,
    /// The currently open raster export-size chooser, if any. Project saves
    /// intentionally bypass this state because `.pbud` files are not scaled.
    export_resolution_dialog: Option<RasterExportDialog>,
    pub status_message: Option<(String, bool)>,
    /// `status_message` remains public because file UI code can surface
    /// failures directly. These fields detect new messages and manage the
    /// toast lifetime without changing that call-site API.
    status_message_shown_at: Option<f64>,
    last_status_message: Option<(String, bool)>,
    pending_replacement: Option<PendingReplacement>,
    /// A locally persisted dirty snapshot. It is restored only after the user
    /// explicitly accepts the recovery prompt.
    recovery_snapshot: Option<String>,
    show_close_confirmation: bool,
    allow_close: bool,
}

impl PixelBuddyApp {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            editor: EditorState::new(width, height),
            zoom: 8.0,
            pan_offset: egui::Vec2::ZERO,
            canvas_rect: None,
            show_grid: true,
            is_drawing: false,
            stroke_points: Vec::new(),
            shape_start: None,
            last_canvas_pixel: None,
            preview_changes: Vec::new(),
            canvas_texture: None,
            checkerboard_texture: None,
            onion_previous_texture: None,
            onion_next_texture: None,
            onion_texture_pair: None,
            texture_dirty: true,
            show_new_dialog: false,
            pending_resize: None,
            new_width: "64".to_string(),
            new_height: "64".to_string(),
            fill_tolerance: 0,
            fill_contiguous: true,
            shape_filled: false,
            io_handler: IoHandler::new(),
            auto_fit_requested: true,
            show_timeline: false,
            new_document_error: None,
            export_resolution_dialog: None,
            status_message: None,
            status_message_shown_at: None,
            last_status_message: None,
            pending_replacement: None,
            recovery_snapshot: None,
            show_close_confirmation: false,
            allow_close: false,
        }
    }

    /// Constructs the app while retaining a dirty local snapshot for an
    /// explicit recovery prompt. eframe backs this with application storage on
    /// desktop and browser Local Storage on WebAssembly.
    pub fn from_creation_context(
        cc: &eframe::CreationContext<'_>,
        width: u32,
        height: u32,
    ) -> Self {
        let mut app = Self::new(width, height);
        app.recovery_snapshot = cc.storage.and_then(|storage| {
            storage
                .get_string(RECOVERY_STORAGE_KEY)
                .filter(|snapshot| !snapshot.trim().is_empty())
        });
        app
    }

    /// Queues a new document, asking before it would replace unsaved work.
    pub fn request_new_document(&mut self, width: u32, height: u32) {
        self.request_replacement(PendingReplacement::NewDocument { width, height });
    }

    /// Queues a flattened imported image, asking before it would replace
    /// unsaved editable project data.
    pub fn request_imported_image(&mut self, document: Document, file_name: String) {
        self.request_replacement(PendingReplacement::ImportedImage {
            document,
            file_name,
        });
    }

    /// Queues a decoded project, asking before it replaces unsaved work.
    pub fn request_opened_project(&mut self, editor: EditorState, file_name: String) {
        self.request_replacement(PendingReplacement::OpenedProject { editor, file_name });
    }

    fn request_replacement(&mut self, replacement: PendingReplacement) {
        if self.editor.is_dirty() {
            self.pending_replacement = Some(replacement);
        } else {
            self.apply_replacement(replacement);
        }
    }

    fn apply_replacement(&mut self, replacement: PendingReplacement) {
        self.cancel_canvas_action();
        self.last_canvas_pixel = None;
        self.pan_offset = egui::Vec2::ZERO;
        self.auto_fit_requested = true;
        self.texture_dirty = true;
        self.new_document_error = None;
        self.show_new_dialog = false;

        match replacement {
            PendingReplacement::NewDocument { width, height } => {
                self.editor = EditorState::new(width, height);
                self.status_message = Some(("Created a new project".to_owned(), false));
            }
            PendingReplacement::ImportedImage {
                document,
                file_name,
            } => {
                self.editor.replace_document(document);
                self.status_message = Some((
                    format!("Imported {file_name}; save as a PixelBuddy project to preserve edits"),
                    false,
                ));
            }
            PendingReplacement::OpenedProject {
                mut editor,
                file_name,
            } => {
                editor.set_project_name(Some(file_name.clone()));
                editor.mark_saved();
                self.editor = editor;
                self.status_message = Some((format!("Opened {file_name}"), false));
            }
        }
    }

    /// Opens a format-aware Save As dialog for the complete editable project.
    pub fn save_project_as(&mut self) {
        let source_revision = self.editor.revision();
        let suggested_name = self
            .editor
            .project_name
            .clone()
            .unwrap_or_else(|| "untitled.pbud".to_owned());
        match crate::io::project::encode_editor_bytes(&self.editor) {
            Ok(bytes) => crate::io::trigger_export(
                crate::io::ExportRequest::project(bytes)
                    .with_suggested_file_name(suggested_name)
                    .with_source_revision(source_revision),
                self.io_handler.sender.clone(),
            ),
            Err(error) => self.status_message = Some((error.to_string(), true)),
        }
    }

    /// Opens the shared nearest-neighbor scale chooser for a flattened PNG.
    /// The project document remains untouched until the user confirms export.
    pub fn open_png_export_dialog(&mut self) {
        self.open_raster_export_dialog(RasterExportKind::Png);
    }

    /// Opens the shared nearest-neighbor scale chooser for the animation GIF.
    pub fn open_gif_export_dialog(&mut self) {
        self.open_raster_export_dialog(RasterExportKind::Gif);
    }

    /// Opens the shared nearest-neighbor scale chooser for a one-row PNG
    /// sprite sheet.
    pub fn open_sprite_sheet_export_dialog(&mut self) {
        self.open_raster_export_dialog(RasterExportKind::SpriteSheetPng);
    }

    fn open_raster_export_dialog(&mut self, kind: RasterExportKind) {
        // Start at 1× so confirming this dialog preserves the old export
        // behavior. The text field remains editable for any positive integer
        // scale, beyond the convenience presets in the dialog.
        let (width_text, height_text) = self
            .raster_export_source_dimensions(kind)
            .and_then(|(width, height, count)| kind.output_dimensions(width, height, count, 1))
            .map(|(width, height)| (width.to_string(), height.to_string()))
            .unwrap_or_else(|| (String::new(), String::new()));
        self.export_resolution_dialog = Some(RasterExportDialog {
            kind,
            sizing: RasterExportSizing::Scale,
            scale_text: "1".to_owned(),
            width_text,
            height_text,
            error: None,
        });
    }

    /// Toggles the root viewport between its normal and borderless fullscreen
    /// presentation. The command is supported by native eframe backends and
    /// remains safe for WebAssembly backends that choose not to act on it.
    pub fn toggle_fullscreen(ctx: &egui::Context, app: &mut PixelBuddyApp) {
        let fullscreen = ctx.input(|input| next_fullscreen_state(input.viewport().fullscreen));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
        app.auto_fit_requested = true;
    }

    fn custom_window_borders(ctx: &egui::Context) {
        let rect = ctx.screen_rect();
        let edge = 6.0;

        let edges = [
            (
                egui::Rect::from_min_max(egui::pos2(rect.min.x + edge, rect.min.y), egui::pos2(rect.max.x - edge, rect.min.y + edge)),
                egui::ResizeDirection::North,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                egui::Rect::from_min_max(egui::pos2(rect.min.x + edge, rect.max.y - edge), egui::pos2(rect.max.x - edge, rect.max.y)),
                egui::ResizeDirection::South,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.min.y + edge), egui::pos2(rect.min.x + edge, rect.max.y - edge)),
                egui::ResizeDirection::West,
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                egui::Rect::from_min_max(egui::pos2(rect.max.x - edge, rect.min.y + edge), egui::pos2(rect.max.x, rect.max.y - edge)),
                egui::ResizeDirection::East,
                egui::CursorIcon::ResizeHorizontal,
            ),
        ];

        let corners = [
            (
                egui::Rect::from_min_size(rect.min, egui::vec2(edge, edge)),
                egui::ResizeDirection::NorthWest,
                egui::CursorIcon::ResizeNwSe,
            ),
            (
                egui::Rect::from_min_size(egui::pos2(rect.max.x - edge, rect.min.y), egui::vec2(edge, edge)),
                egui::ResizeDirection::NorthEast,
                egui::CursorIcon::ResizeNeSw,
            ),
            (
                egui::Rect::from_min_size(egui::pos2(rect.min.x, rect.max.y - edge), egui::vec2(edge, edge)),
                egui::ResizeDirection::SouthWest,
                egui::CursorIcon::ResizeNeSw,
            ),
            (
                egui::Rect::from_min_size(egui::pos2(rect.max.x - edge, rect.max.y - edge), egui::vec2(edge, edge)),
                egui::ResizeDirection::SouthEast,
                egui::CursorIcon::ResizeNwSe,
            ),
        ];

        for (id_str, rect, dir, cursor) in edges.into_iter().zip(["n", "s", "w", "e"]).map(|((r, d, c), id)| (id, r, d, c))
            .chain(corners.into_iter().zip(["nw", "ne", "sw", "se"]).map(|((r, d, c), id)| (id, r, d, c)))
        {
            let id = egui::Id::new("resize_edge").with(id_str);
            // We must use an Area to put the interact rect on top of everything
            egui::Area::new(id)
                .fixed_pos(rect.min)
                .order(egui::Order::Tooltip)
                .interactable(true)
                .show(ctx, |ui| {
                    let response = ui.allocate_response(rect.size(), egui::Sense::drag());
                    if response.hovered() || response.dragged() {
                        ctx.set_cursor_icon(cursor);
                    }
                    if response.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                    }
                });
        }
    }

    pub fn update_texture(&mut self, ctx: &egui::Context) {
        if self.texture_dirty || self.canvas_texture.is_none() {
            let canvas = self.editor.document().composite_preview();
            let size = [canvas.width() as usize, canvas.height() as usize];
            let image = ColorImage::from_rgba_unmultiplied(size, canvas.pixels());
            let options = TextureOptions {
                magnification: TextureFilter::Nearest,
                minification: TextureFilter::Nearest,
                ..Default::default()
            };
            if let Some(texture) = &mut self.canvas_texture {
                texture.set(image, options);
            } else {
                self.canvas_texture = Some(ctx.load_texture("canvas", image, options));
            }
            self.texture_dirty = false;
        }
    }

    /// Returns a repeating 2×2 checkerboard texture. Rendering this as one
    /// tiled image avoids issuing one paint primitive per canvas pixel.
    pub fn checkerboard_texture_id(&mut self, ctx: &egui::Context) -> egui::TextureId {
        if self.checkerboard_texture.is_none() {
            let mut image = ColorImage::new([2, 2], egui::Color32::from_gray(210));
            image.pixels = vec![
                egui::Color32::from_gray(210),
                egui::Color32::from_gray(170),
                egui::Color32::from_gray(170),
                egui::Color32::from_gray(210),
            ];
            self.checkerboard_texture = Some(ctx.load_texture(
                "pixelbuddy_checkerboard",
                image,
                TextureOptions::NEAREST_REPEAT,
            ));
        }

        self.checkerboard_texture
            .as_ref()
            .expect("checkerboard texture is initialized above")
            .id()
    }

    /// Returns cached texture IDs for the neighboring animation frames.
    ///
    /// The current document is never an onion-skin source, so edits made to it
    /// do not invalidate these textures. A frame switch changes the pair and
    /// refreshes exactly the two needed composites.
    pub fn onion_texture_ids(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<(egui::TextureId, egui::TextureId)> {
        if !self.editor.animation.onion_skin_enabled || self.editor.animation.frames.len() <= 1 {
            return None;
        }

        let current = self.editor.animation.current_frame_index;
        let frame_count = self.editor.animation.frames.len();
        let previous = if current == 0 {
            frame_count - 1
        } else {
            current - 1
        };
        let next = (current + 1) % frame_count;
        let pair = (previous, next);

        if self.onion_texture_pair != Some(pair)
            || self.onion_previous_texture.is_none()
            || self.onion_next_texture.is_none()
        {
            let previous_canvas = self.editor.animation.frames[previous]
                .document
                .composite_preview();
            let next_canvas = self.editor.animation.frames[next]
                .document
                .composite_preview();
            let previous_image = ColorImage::from_rgba_unmultiplied(
                [
                    previous_canvas.width() as usize,
                    previous_canvas.height() as usize,
                ],
                previous_canvas.pixels(),
            );
            let next_image = ColorImage::from_rgba_unmultiplied(
                [next_canvas.width() as usize, next_canvas.height() as usize],
                next_canvas.pixels(),
            );

            if let Some(texture) = &mut self.onion_previous_texture {
                texture.set(previous_image, TextureOptions::NEAREST);
            } else {
                self.onion_previous_texture = Some(ctx.load_texture(
                    "pixelbuddy_onion_previous",
                    previous_image,
                    TextureOptions::NEAREST,
                ));
            }
            if let Some(texture) = &mut self.onion_next_texture {
                texture.set(next_image, TextureOptions::NEAREST);
            } else {
                self.onion_next_texture = Some(ctx.load_texture(
                    "pixelbuddy_onion_next",
                    next_image,
                    TextureOptions::NEAREST,
                ));
            }
            self.onion_texture_pair = Some(pair);
        }

        Some((
            self.onion_previous_texture
                .as_ref()
                .expect("onion texture is initialized above")
                .id(),
            self.onion_next_texture
                .as_ref()
                .expect("onion texture is initialized above")
                .id(),
        ))
    }

    /// Makes the next onion-skin draw rebuild its neighboring-frame textures.
    ///
    /// Frame insertion, deletion, and reordering can change the artwork at
    /// the same pair of indices, so simply marking the main canvas texture
    /// dirty is not sufficient for onion skinning.
    pub fn invalidate_onion_skin_cache(&mut self) {
        self.onion_texture_pair = None;
    }

    /// Apply a set of pixel changes to the active layer, recording undo history.
    pub fn apply_tool_changes(&mut self, changes: Vec<tools::PixelChange>) {
        if changes.is_empty() {
            return;
        }

        if self.editor.animation.is_playing {
            self.editor.animation.stop();
        }

        let active_layer_index = self.editor.document().active_layer_index;
        let Some(layer) = self.editor.document().layers.get(active_layer_index) else {
            return;
        };
        if layer.locked {
            return;
        }

        // A tool can emit multiple writes to the same pixel (notably Move).
        // Keep only the final value before capturing the original color for undo.
        let final_changes: BTreeMap<(u32, u32), [u8; 4]> = changes
            .into_iter()
            .map(|(x, y, color)| ((x, y), color))
            .collect();
        let mut history_changes = Vec::new();
        {
            let layer = &self.editor.document().layers[active_layer_index];
            for ((x, y), new_color) in final_changes {
                if layer.canvas.in_bounds(x as i32, y as i32) {
                    let old_color = layer.canvas.get_pixel(x, y);
                    if old_color != new_color {
                        history_changes.push((x, y, old_color, new_color));
                    }
                }
            }
        }
        if !history_changes.is_empty() {
            let cmd = Box::new(DrawCommand::new(active_layer_index, history_changes));
            // Use the EditorState helper to avoid split-borrow issues
            self.editor.push_command(cmd);
            self.texture_dirty = true;
            self.invalidate_onion_skin_cache();
        }
    }

    /// Starts an interaction that owns its own press/release lifecycle.
    ///
    /// Canvas drags intentionally do not rely on `Response::hover_pos()` for
    /// cleanup: egui can still report a release after the cursor leaves the
    /// canvas, while `hover_pos()` is then `None`.
    pub fn begin_canvas_action(&mut self, x: i32, y: i32) {
        debug_assert!(x >= 0 && y >= 0);
        self.is_drawing = true;
        self.stroke_points.clear();
        self.stroke_points.push((x as u32, y as u32));
        self.shape_start = Some((x, y));
        self.last_canvas_pixel = Some((x, y));
        self.preview_changes.clear();
    }

    /// Clears transient interaction state without mutating the document.
    pub fn cancel_canvas_action(&mut self) {
        self.is_drawing = false;
        self.stroke_points.clear();
        self.shape_start = None;
        self.preview_changes.clear();
    }

    /// Switching tools cannot reinterpret an unfinished drag as a different
    /// operation when the pointer is eventually released.
    pub fn set_active_tool(&mut self, tool: ToolType) {
        if self.editor.active_tool != tool {
            self.cancel_canvas_action();
        }
        self.editor.set_active_tool(tool);
    }

    fn paste_origin(&self, clipboard_width: u32, clipboard_height: u32) -> (u32, u32) {
        let document = self.editor.document();
        let fallback = (
            document.width.saturating_sub(clipboard_width) / 2,
            document.height.saturating_sub(clipboard_height) / 2,
        );
        let anchor = if self.editor.selection.active {
            Some((
                self.editor.selection.min_x().max(0) as u32,
                self.editor.selection.min_y().max(0) as u32,
            ))
        } else {
            self.last_canvas_pixel
                .filter(|(x, y)| *x >= 0 && *y >= 0)
                .map(|(x, y)| (x as u32, y as u32))
        }
        .unwrap_or(fallback);

        (
            anchor.0.min(document.width.saturating_sub(1)),
            anchor.1.min(document.height.saturating_sub(1)),
        )
    }

    fn show_project_lifecycle_dialogs(&mut self, ctx: &egui::Context) {
        if self.show_close_confirmation {
            self.show_close_confirmation(ctx);
            return;
        }
        self.show_recovery_dialog(ctx);
        self.show_replace_confirmation(ctx);
    }

    fn intercept_dirty_close_request(&mut self, ctx: &egui::Context) {
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
            let snapshot = self
                .recovery_snapshot
                .take()
                .expect("recovery snapshot was checked before opening the dialog");
            match crate::io::project::decode_editor(&snapshot) {
                Ok(editor) => {
                    self.apply_replacement(PendingReplacement::OpenedProject {
                        editor,
                        file_name: "Recovered draft".to_owned(),
                    });
                    self.editor.mark_dirty();
                    self.status_message = Some((
                        "Recovered local draft — save it as a PixelBuddy project".to_owned(),
                        false,
                    ));
                }
                Err(error) => {
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
            if let Some(replacement) = self.pending_replacement.take() {
                self.apply_replacement(replacement);
            }
        } else if cancel {
            self.pending_replacement = None;
        }
    }

    fn can_create_canvas(width: u32, height: u32) -> Result<(), String> {
        crate::io::validate_canvas_dimensions(width, height).map_err(|error| error.to_string())
    }

    /// Returns the source frame dimensions used by a raster export preview.
    /// PNG exports only the active frame, while GIF and sprite-sheet exports
    /// begin with the first animation frame and validate every other frame at
    /// the time of export.
    fn raster_export_source_dimensions(&self, kind: RasterExportKind) -> Option<(u32, u32, usize)> {
        match kind {
            RasterExportKind::Png => {
                let document = self.editor.document();
                Some((document.width, document.height, 1))
            }
            RasterExportKind::Gif | RasterExportKind::SpriteSheetPng => {
                self.editor.animation.frames.first().map(|frame| {
                    (
                        frame.document.width,
                        frame.document.height,
                        self.editor.animation.frames.len(),
                    )
                })
            }
        }
    }

    /// Encodes the selected raster format at an integer nearest-neighbor
    /// scale, then hands its bytes to the existing format-aware save dialog.
    fn export_raster_at_dimensions(
        &mut self,
        kind: RasterExportKind,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let encoded = match kind {
            RasterExportKind::Png => crate::io::png::export_document_to_png_at_dimensions(
                self.editor.document(),
                width,
                height,
            ),
            RasterExportKind::Gif => crate::io::gif::export_animation_to_gif_at_dimensions(
                &self.editor.animation,
                width,
                height,
            ),
            RasterExportKind::SpriteSheetPng => {
                crate::io::spritesheet::export_spritesheet_png_at_dimensions(
                    &self.editor.animation,
                    width,
                    height,
                )
            }
        }
        .map_err(|error| error.to_string())?;

        let request = match kind {
            RasterExportKind::Png => crate::io::ExportRequest::png(encoded),
            RasterExportKind::Gif => crate::io::ExportRequest::gif(encoded),
            RasterExportKind::SpriteSheetPng => crate::io::ExportRequest::sprite_sheet_png(encoded),
        };
        crate::io::trigger_export(request, self.io_handler.sender.clone());
        Ok(())
    }

    fn show_export_resolution_dialog(&mut self, ctx: &egui::Context) {
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
                            RasterExportKind::Png => {
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

    fn status_toast_expired(shown_at: f64, now: f64) -> bool {
        (now - shown_at).max(0.0) >= STATUS_TOAST_DURATION_SECONDS
    }

    /// Returns the point and pivot for a status toast. Once the canvas has
    /// been laid out, the toast sits immediately above its upper-right corner
    /// instead of being tied to the application window. This makes it follow
    /// panning, zooming, and viewport layout changes.
    fn status_toast_anchor(
        canvas_rect: Option<egui::Rect>,
        screen_rect: egui::Rect,
    ) -> (egui::Pos2, egui::Align2) {
        if let Some(canvas_rect) = canvas_rect.filter(|rect| rect.is_finite() && rect.is_positive())
        {
            return (
                egui::pos2(
                    canvas_rect.right(),
                    canvas_rect.top() - STATUS_TOAST_CANVAS_GAP,
                ),
                egui::Align2::RIGHT_BOTTOM,
            );
        }

        (
            egui::pos2(
                screen_rect.right() - STATUS_TOAST_FALLBACK_RIGHT_INSET,
                screen_rect.top() + STATUS_TOAST_FALLBACK_TOP_INSET,
            ),
            egui::Align2::RIGHT_TOP,
        )
    }

    fn show_status_toast(&mut self, ctx: &egui::Context) {
        let Some((message, is_error)) = self.status_message.clone() else {
            self.status_message_shown_at = None;
            self.last_status_message = None;
            return;
        };

        let now = ctx.input(|input| input.time);
        let current_message = (message.clone(), is_error);
        if self.last_status_message.as_ref() != Some(&current_message) {
            self.last_status_message = Some(current_message);
            self.status_message_shown_at = Some(now);
        }

        let shown_at = self.status_message_shown_at.unwrap_or(now);
        if Self::status_toast_expired(shown_at, now) {
            self.status_message = None;
            self.status_message_shown_at = None;
            self.last_status_message = None;
            return;
        }

        // Wake the app up when the message is due to disappear, even when the
        // user is otherwise idle.
        let remaining = (STATUS_TOAST_DURATION_SECONDS - (now - shown_at).max(0.0)).max(0.0);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining));

        let color = if is_error {
            egui::Color32::from_rgb(248, 113, 113)
        } else {
            egui::Color32::from_rgb(134, 239, 172)
        };
        let (toast_position, toast_pivot) =
            Self::status_toast_anchor(self.canvas_rect, ctx.screen_rect());
        let mut dismiss = false;
        egui::Area::new(egui::Id::new("file_status_message"))
            .order(egui::Order::Foreground)
            .fixed_pos(toast_position)
            .pivot(toast_pivot)
            .constrain_to(ctx.screen_rect())
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&message).color(color));
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("×")
                                        .color(ui.visuals().weak_text_color())
                                        .size(16.0),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Dismiss notification")
                            .clicked()
                        {
                            dismiss = true;
                        }
                    });
                });
            });

        if dismiss {
            self.status_message = None;
            self.status_message_shown_at = None;
            self.last_status_message = None;
        }
    }
}

impl eframe::App for PixelBuddyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.intercept_dirty_close_request(ctx);

        // Ensure texture is updated before rendering panels
        self.update_texture(ctx);
        if self.texture_dirty {
            ctx.request_repaint();
        }

        // Handle I/O events
        while let Ok(action) = self.io_handler.receiver.try_recv() {
            match action {
                FileAction::OpenedImage { data, file_name } => {
                    match crate::io::png::import_png_to_document(&data) {
                        Ok(doc) => {
                            self.request_imported_image(doc, file_name);
                        }
                        Err(error) => {
                            log::error!("Unable to open PNG: {error}");
                            self.status_message = Some((error.to_string(), true));
                        }
                    }
                }
                FileAction::OpenedProject { data, file_name } => {
                    match crate::io::project::decode_editor_bytes(&data) {
                        Ok(editor) => self.request_opened_project(editor, file_name),
                        Err(error) => {
                            log::error!("Unable to open PixelBuddy project: {error}");
                            self.status_message = Some((error.to_string(), true));
                        }
                    }
                }
                FileAction::Exported {
                    format,
                    file_name,
                    source_revision,
                } => {
                    let mut newer_edits_remain = false;
                    if format == crate::io::ExportFormat::Project {
                        self.editor.set_project_name(Some(file_name.clone()));
                        newer_edits_remain = source_revision
                            .map(|revision| !self.editor.mark_saved_if_current(revision))
                            .unwrap_or(false);
                    }
                    let message = if newer_edits_remain {
                        format!("Saved {format} as {file_name}; newer edits remain unsaved")
                    } else {
                        format!("Saved {format} as {file_name}")
                    };
                    self.status_message = Some((message, false));
                }
                FileAction::Failed(error) => {
                    log::error!("File operation failed: {error}");
                    self.status_message = Some((error.to_string(), true));
                }
            }
        }

        // Handle animation playback stepping
        let current_time = ctx.input(|i| i.time);
        if self.editor.update_animation_playback(current_time) {
            self.texture_dirty = true;
        }
        if self.editor.animation.is_playing {
            ctx.request_repaint();
        }

        // Handle shortcuts
        if ctx.input(|input| input.key_pressed(egui::Key::F11)) {
            Self::toggle_fullscreen(ctx, self);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel_canvas_action();
        }
        if ctx.input(|i| !i.modifiers.ctrl && i.key_pressed(egui::Key::Space)) {
            self.editor.animation.toggle_play(current_time);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.save_project_as();
        }
        if ctx.input(|i| i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::Z))
            && self.editor.undo()
        {
            self.texture_dirty = true;
        }
        if ctx.input(|i| {
            i.modifiers.ctrl
                && (i.key_pressed(egui::Key::Y)
                    || (i.modifiers.shift && i.key_pressed(egui::Key::Z)))
        }) && self.editor.redo()
        {
            self.texture_dirty = true;
        }
        if ctx.input(|i| !i.modifiers.ctrl && i.key_pressed(egui::Key::X)) {
            self.editor.swap_colors();
        }
        // Deselect Marquee (Ctrl+D or Right-Click in canvas)
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::D)) {
            self.editor.selection.deselect();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::C)) {
            let clipboard = crate::editor::clipboard::ClipboardBuffer::copy(
                self.editor.document(),
                &self.editor.selection,
            );
            self.editor.clipboard = clipboard;
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
            if let Some(buf) = &self.editor.clipboard.clone() {
                let (origin_x, origin_y) = self.paste_origin(buf.width, buf.height);
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
                self.apply_tool_changes(changes);
            }
        }

        let tools = [
            (egui::Key::H, ToolType::Hand),
            (egui::Key::M, ToolType::Marquee),
            (egui::Key::V, ToolType::Move),
            (egui::Key::B, ToolType::Pencil),
            (egui::Key::E, ToolType::Eraser),
            (egui::Key::L, ToolType::Line),
            (egui::Key::R, ToolType::Rectangle),
            (egui::Key::O, ToolType::Ellipse),
            (egui::Key::G, ToolType::Fill),
            (egui::Key::I, ToolType::Eyedropper),
        ];
        for (key, tool) in tools {
            if ctx.input(|i| !i.modifiers.ctrl && i.key_pressed(key)) {
                self.set_active_tool(tool);
            }
        }
        if ctx.input(|i| !i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::Z)) {
            self.set_active_tool(ToolType::Zoom);
        }

                crate::ui::menu_bar::show(ctx, self);
        crate::ui::toolbar::show(ctx, self);
        crate::ui::layers_panel::show(ctx, self);
        if self.show_timeline {
            crate::ui::timeline_panel::show(ctx, self);
        }
        crate::ui::status_bar::show(ctx, self);
        crate::ui::canvas_view::show(ctx, self);

        Self::custom_window_borders(ctx);

        if self.show_new_dialog {
            let mut open = true;
            egui::Window::new("New Document")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("Presets").strong());
                    ui.horizontal(|ui| {
                        for (label, w, h) in [
                            ("16×16", "16", "16"),
                            ("32×32", "32", "32"),
                            ("64×64", "64", "64"),
                            ("128×128", "128", "128"),
                        ] {
                            if ui.button(label).clicked() {
                                self.new_width = w.to_string();
                                self.new_height = h.to_string();
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Width:");
                        ui.text_edit_singleline(&mut self.new_width);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Height:");
                        ui.text_edit_singleline(&mut self.new_height);
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            if let (Ok(w), Ok(h)) = (
                                self.new_width.parse::<u32>(),
                                self.new_height.parse::<u32>(),
                            ) {
                                match Self::can_create_canvas(w, h) {
                                    Ok(()) => {
                                        self.request_new_document(w, h);
                                        self.new_document_error = None;
                                        self.show_new_dialog = false;
                                    }
                                    Err(error) => self.new_document_error = Some(error),
                                }
                            } else {
                                self.new_document_error =
                                    Some("Enter whole-number canvas dimensions.".to_owned());
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_new_dialog = false;
                        }
                    });
                    if let Some(error) = &self.new_document_error {
                        ui.colored_label(egui::Color32::from_rgb(248, 113, 113), error);
                    }
                });
            if !open {
                self.show_new_dialog = false;
            }
        }

        if let Some((w, h)) = self.pending_resize {
            let mut open = true;
            egui::Window::new("Resize Canvas")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Warning: Resizing will clear your undo history!")
                            .color(egui::Color32::from_rgb(248, 113, 113))
                            .strong()
                    );
                    ui.label(format!("Are you sure you want to resize to {}x{}?", w, h));
                    
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Resize").clicked() {
                            self.editor.animation.resize(w, h);
                            self.editor.history.clear();
                            self.pan_offset = egui::Vec2::ZERO;
                            self.auto_fit_requested = true;
                            self.texture_dirty = true;
                            self.pending_resize = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_resize = None;
                        }
                    });
                });
            if !open {
                self.pending_resize = None;
            }
        }
        self.show_export_resolution_dialog(ctx);

        self.show_project_lifecycle_dialogs(ctx);

        self.show_status_toast(ctx);

        self.update_texture(ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.editor.is_dirty() {
            match crate::io::project::encode_editor(&self.editor) {
                Ok(snapshot) => storage.set_string(RECOVERY_STORAGE_KEY, snapshot),
                Err(error) => {
                    log::error!("Unable to create local project recovery snapshot: {error}")
                }
            }
        } else {
            // An explicit project save or discard makes the previous recovery
            // snapshot stale, so remove it from both native storage and Web
            // Local Storage on the next persistence pass.
            storage.set_string(RECOVERY_STORAGE_KEY, String::new());
        }
        storage.flush();
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(20)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_raster_export_dimension, parse_raster_export_scale, PendingReplacement,
        PixelBuddyApp, RasterExportKind, RasterExportSizing,
    };

    #[test]
    fn cancelling_a_canvas_action_discards_only_transient_state() {
        let mut app = PixelBuddyApp::new(8, 8);
        app.begin_canvas_action(3, 4);
        app.preview_changes.push((3, 4, [1, 2, 3, 255]));

        app.cancel_canvas_action();

        assert!(!app.is_drawing);
        assert!(app.shape_start.is_none());
        assert!(app.stroke_points.is_empty());
        assert!(app.preview_changes.is_empty());
        assert_eq!(app.last_canvas_pixel, Some((3, 4)));
    }

    #[test]
    fn paste_prefers_selection_then_last_canvas_pixel() {
        let mut app = PixelBuddyApp::new(8, 8);
        app.last_canvas_pixel = Some((6, 5));
        assert_eq!(app.paste_origin(2, 2), (6, 5));

        app.editor.selection.set_rect(2, 1, 5, 4);
        assert_eq!(app.paste_origin(2, 2), (2, 1));
    }

    #[test]
    fn dirty_project_replacement_waits_for_explicit_discard() {
        let mut app = PixelBuddyApp::new(8, 8);
        app.editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [1, 2, 3, 255]);

        app.request_new_document(4, 4);

        assert_eq!(
            (app.editor.document().width, app.editor.document().height),
            (8, 8)
        );
        assert!(matches!(
            app.pending_replacement,
            Some(PendingReplacement::NewDocument {
                width: 4,
                height: 4
            })
        ));

        let replacement = app
            .pending_replacement
            .take()
            .expect("dirty replacement should remain queued");
        app.apply_replacement(replacement);

        assert_eq!(
            (app.editor.document().width, app.editor.document().height),
            (4, 4)
        );
        assert!(!app.editor.is_dirty());
    }

    #[test]
    fn status_toast_expires_after_six_seconds() {
        assert!(!PixelBuddyApp::status_toast_expired(10.0, 15.999));
        assert!(PixelBuddyApp::status_toast_expired(10.0, 16.0));
    }

    #[test]
    fn fullscreen_toggle_inverts_known_state_and_enters_from_unknown_state() {
        assert!(super::next_fullscreen_state(None));
        assert!(super::next_fullscreen_state(Some(false)));
        assert!(!super::next_fullscreen_state(Some(true)));
    }

    #[test]
    fn fullscreen_toggle_sends_a_root_viewport_command() {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput::default());

        let mut app = PixelBuddyApp::new(16, 16);
        PixelBuddyApp::toggle_fullscreen(&ctx, &mut app);

        let output = ctx.end_pass();
        let root_viewport = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("the root viewport has output after a UI pass");
        assert!(root_viewport
            .commands
            .contains(&egui::ViewportCommand::Fullscreen(true)));
    }

    #[test]
    fn status_toast_is_anchored_just_above_the_canvas_upper_right_corner() {
        let canvas = egui::Rect::from_min_max(egui::pos2(100.0, 120.0), egui::pos2(500.0, 420.0));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_920.0, 1_080.0));

        let (position, pivot) = PixelBuddyApp::status_toast_anchor(Some(canvas), screen);

        assert_eq!(position, egui::pos2(500.0, 110.0));
        assert_eq!(pivot, egui::Align2::RIGHT_BOTTOM);
    }

    #[test]
    fn status_toast_uses_workspace_fallback_before_canvas_layout() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_920.0, 1_080.0));

        let (position, pivot) = PixelBuddyApp::status_toast_anchor(None, screen);

        assert_eq!(position, egui::pos2(1_704.0, 48.0));
        assert_eq!(pivot, egui::Align2::RIGHT_TOP);
    }

    #[test]
    fn raster_export_scales_report_the_actual_output_dimensions() {
        assert_eq!(
            RasterExportKind::Png.output_dimensions(16, 8, 1, 4),
            Some((64, 32))
        );
        assert_eq!(
            RasterExportKind::Gif.output_dimensions(16, 8, 3, 2),
            Some((32, 16))
        );
        assert_eq!(
            RasterExportKind::SpriteSheetPng.output_dimensions(16, 8, 3, 2),
            Some((96, 16))
        );
    }

    #[test]
    fn raster_export_scale_requires_a_positive_whole_number() {
        assert_eq!(parse_raster_export_scale(" 8 "), Ok(8));
        assert!(parse_raster_export_scale("0").is_err());
        assert!(parse_raster_export_scale("1.5").is_err());
        assert!(parse_raster_export_scale("pixels").is_err());
    }

    #[test]
    fn raster_export_dimensions_require_positive_whole_pixels() {
        assert_eq!(parse_raster_export_dimension(" 1024 ", "width"), Ok(1024));
        assert!(parse_raster_export_dimension("0", "height").is_err());
        assert!(parse_raster_export_dimension("10.5", "width").is_err());
    }

    #[test]
    fn opening_a_raster_export_chooser_defaults_to_one_x() {
        let mut app = PixelBuddyApp::new(16, 16);

        app.open_sprite_sheet_export_dialog();

        let dialog = app
            .export_resolution_dialog
            .as_ref()
            .expect("sprite-sheet export should open the scale chooser");
        assert_eq!(dialog.kind, RasterExportKind::SpriteSheetPng);
        assert_eq!(dialog.sizing, RasterExportSizing::Scale);
        assert_eq!(dialog.scale_text, "1");
        assert_eq!(dialog.width_text, "16");
        assert_eq!(dialog.height_text, "16");
        assert!(dialog.error.is_none());
    }
}
