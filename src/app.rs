use crate::document::Document;
use crate::editor::history::DrawCommand;
use crate::editor::{EditorState, ToolType};
use crate::io::{FileAction, IoHandler};
use crate::shortcut_dispatcher::{
    shortcut_permissions, ShortcutCommand, ShortcutDispatcher, ShortcutPermissions,
};
use crate::tools;
use egui::{ColorImage, TextureFilter, TextureHandle, TextureOptions};
use std::collections::BTreeMap;

mod dialogs;
mod textures;

const RECOVERY_STORAGE_KEY: &str = "pixelbuddy.recovery.v1";
const VIEW_PREFERENCES_STORAGE_KEY: &str = "pixelbuddy.view_preferences.v1";
const STATUS_TOAST_DURATION_SECONDS: f64 = 6.0;
/// Keep the notification visually attached to, but outside of, the canvas.
const STATUS_TOAST_CANVAS_GAP: f32 = 10.0;
/// Before the canvas has been laid out, leave room for the fixed-width Layers
/// panel so transient messages do not cover its controls.
const STATUS_TOAST_FALLBACK_RIGHT_INSET: f32 = 216.0;
const STATUS_TOAST_FALLBACK_TOP_INSET: f32 = 48.0;

/// Returns the fullscreen setting to request after a toggle. Unknown viewport
/// state is treated as windowed so the first toggle always enters fullscreen.
#[cfg(not(target_arch = "wasm32"))]
fn next_fullscreen_state(current: Option<bool>) -> bool {
    !current.unwrap_or(false)
}

/// Native window presentation reported by egui for the root viewport.
///
/// Fullscreen takes precedence because it can visually cover an underlying
/// maximized window until fullscreen is explicitly exited.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowPresentation {
    Windowed,
    Maximized,
    Fullscreen,
}

#[cfg(not(target_arch = "wasm32"))]
impl WindowPresentation {
    fn from_viewport(maximized: Option<bool>, fullscreen: Option<bool>) -> Self {
        if fullscreen == Some(true) {
            Self::Fullscreen
        } else if maximized == Some(true) {
            Self::Maximized
        } else {
            Self::Windowed
        }
    }

    fn allows_resize_handles(self) -> bool {
        self == Self::Windowed
    }
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

#[derive(Clone, Debug, PartialEq)]
pub enum PalettePolicy {
    KeepCurrent,
    UseDefault,
    UsePreset(String),
}

/// A user-requested replacement held until unsaved work has been explicitly
/// discarded. Keeping decoded data here prevents an Open action from changing
/// the active project before the confirmation is accepted.
enum DocumentReplacement {
    NewDocument {
        width: u32,
        height: u32,
        palette_policy: PalettePolicy,
    },
    ImportedImage {
        document: Document,
        file_name: String,
        palette_policy: PalettePolicy,
    },
    OpenedProject {
        editor: EditorState,
        file_name: String,
    },
    RecoveredProject {
        editor: EditorState,
    },
    ImportedAnimation {
        animation: crate::document::AnimationManager,
        file_name: String,
        palette_policy: PalettePolicy,
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
    WebP,
}

impl RasterExportKind {
    const fn dialog_title(self) -> &'static str {
        match self {
            Self::Png => "Export PNG",
            Self::Gif => "Export Animated GIF",
            Self::SpriteSheetPng => "Export Sprite Sheet",
            Self::WebP => "Export WebP",
        }
    }

    const fn export_button_label(self) -> &'static str {
        match self {
            Self::Png => "Export PNG",
            Self::Gif => "Export GIF",
            Self::SpriteSheetPng => "Export Sprite Sheet",
            Self::WebP => "Export WebP",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Png => "Exports the active frame as a flattened PNG image.",
            Self::Gif => "Exports every animation frame while keeping its current timing.",
            Self::SpriteSheetPng => "Places every animation frame left to right in one PNG image.",
            Self::WebP => "Exports the active frame as a flattened WebP image.",
        }
    }

    /// Returns the raster dimensions after nearest-neighbor integer scaling.
    /// A sprite sheet is one horizontal row, so its unscaled width also
    /// multiplies by the frame count.
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
            Self::Png | Self::Gif | Self::WebP => frame_width,
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

#[derive(PartialEq, Clone, Copy)]
pub enum SpriteSheetImportMode {
    NewProject,
    AppendFrames,
    ActiveLayer,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum FrameThumbnailInvalidation {
    #[default]
    None,
    Current,
    Frames(Vec<usize>),
    All,
    Structure,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EditEffects {
    pub changed: bool,
    pub current_texture_dirty: bool,
    pub frame_thumbnails: FrameThumbnailInvalidation,
    pub onion_skin_dirty: bool,
}

impl EditEffects {
    fn persisted_only(changed: bool) -> Self {
        Self {
            changed,
            ..Self::default()
        }
    }

    pub(crate) fn current_frame_artwork(changed: bool) -> Self {
        Self {
            changed,
            current_texture_dirty: changed,
            frame_thumbnails: if changed {
                FrameThumbnailInvalidation::Current
            } else {
                FrameThumbnailInvalidation::None
            },
            onion_skin_dirty: changed,
        }
    }

    fn all_frame_artwork(changed: bool, structure: bool) -> Self {
        Self {
            changed,
            current_texture_dirty: changed,
            frame_thumbnails: if !changed {
                FrameThumbnailInvalidation::None
            } else if structure {
                FrameThumbnailInvalidation::Structure
            } else {
                FrameThumbnailInvalidation::All
            },
            onion_skin_dirty: changed,
        }
    }
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
    /// Tile-local endpoint owned by the active canvas gesture.
    pub(crate) canvas_action_last_pixel: Option<(i32, i32)>,
    /// Tile offset where a non-wrapping gesture began.
    pub(crate) canvas_action_tile_offset: Option<(i32, i32)>,
    /// Signed repeated-space points used to make pencil and eraser strokes
    /// continuous when they cross a tile seam.
    pub(crate) canvas_action_virtual_points: Vec<(i32, i32)>,
    /// Monotonic identity used to invalidate cached gesture previews.
    pub(crate) canvas_action_generation: u64,
    /// Prevent raw canvas pointer handling while foreground dialogs or popups are active.
    pub(crate) canvas_input_blocked: bool,

    pub preview_changes: Vec<tools::PixelChange>,
    pub canvas_texture: Option<TextureHandle>,
    pub checkerboard_texture: Option<TextureHandle>,
    onion_previous_texture: Option<TextureHandle>,
    onion_next_texture: Option<TextureHandle>,
    onion_texture_pair: Option<(usize, usize)>,
    pub texture_dirty: bool,
    pub show_new_dialog: bool,
    pub show_help_dialog: bool,
    pub show_about_dialog: bool,
    pub active_effect: Option<crate::effects::ActiveEffectState>,
    pub pending_resize: Option<(u32, u32)>,
    pub show_custom_resize_dialog: bool,
    pub resize_width: String,
    pub resize_height: String,
    pub resize_error: Option<String>,
    pub new_width: String,
    pub new_height: String,
    pub new_project_palette_policy: PalettePolicy,
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
    pending_replacement: Option<DocumentReplacement>,
    /// Monotonically advancing identity for the active project. Revisions are
    /// local to an `EditorState`, so this prevents delayed async save results
    /// from an older project from matching a replacement project's revision.
    document_session_id: u64,
    /// Changes whenever the logical frame shown for editing changes. This is
    /// intentionally separate from the document session: switching away and
    /// back must still invalidate frame-bound UI drafts and async targets.
    active_frame_generation: u64,
    /// Save dialogs can complete out of order. Request IDs let the newest
    /// completed save win without allowing an older completion to roll back
    /// the active filename or saved-state bookkeeping.
    next_project_save_request_id: u64,
    last_applied_project_save_request_id: u64,
    /// A native-only persisted dirty snapshot. It is restored only after the
    /// user explicitly accepts the recovery prompt. Web builds rely on manual
    /// project downloads instead of size-constrained browser Local Storage.
    recovery_snapshot: Option<String>,
    show_close_confirmation: bool,
    allow_close: bool,
    pub show_spritesheet_import_dialog: bool,
    pub show_image_import_dialog: bool,
    pub image_import_document: Option<crate::document::Document>,
    pub image_import_file_name: Option<String>,
    pub spritesheet_import_mode: SpriteSheetImportMode,
    spritesheet_import_source_session_id: Option<u64>,
    spritesheet_import_source_revision: Option<u64>,
    spritesheet_import_source_frame_generation: Option<u64>,
    spritesheet_import_source_active_layer_index: Option<usize>,
    pub spritesheet_import_data: Option<(Vec<u8>, String)>,
    pub spritesheet_import_texture: Option<egui::TextureHandle>,
    pub spritesheet_import_columns: String,
    pub spritesheet_import_rows: String,
    pub spritesheet_import_error: Option<String>,
    pub show_rulers: bool,
    pub show_guides: bool,
    pub horizontal_guides: Vec<i32>,
    pub vertical_guides: Vec<i32>,
    pub dragging_guide: Option<(bool, usize)>,
    pub tile_mode: TileMode,
    pub tile_preview: TilePreviewSettings,
    pub fit_tile_preview_requested: bool,
    pub(crate) tile_preview_fit_active: bool,
    pub frame_thumbnails: Vec<Option<TextureHandle>>,
}

pub const MAX_TILE_PREVIEW_COUNT: u8 = 15;
pub const MIN_CANVAS_ZOOM: f32 = 0.001;
pub const MAX_CANVAS_ZOOM: f32 = 64.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum TileMode {
    #[default]
    None,
    Both,
    XAxis,
    YAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct TilePreviewSettings {
    columns: u8,
    rows: u8,
}

impl Default for TilePreviewSettings {
    fn default() -> Self {
        Self {
            columns: 3,
            rows: 3,
        }
    }
}

impl TilePreviewSettings {
    pub fn columns(self) -> u8 {
        self.columns
    }

    pub fn rows(self) -> u8 {
        self.rows
    }

    pub fn set_columns(&mut self, columns: u8) {
        self.columns = columns.clamp(1, MAX_TILE_PREVIEW_COUNT);
    }

    pub fn set_rows(&mut self, rows: u8) {
        self.rows = rows.clamp(1, MAX_TILE_PREVIEW_COUNT);
    }

    fn normalized(mut self) -> Self {
        self.set_columns(self.columns);
        self.set_rows(self.rows);
        self
    }

    pub fn effective_dimensions(self, mode: TileMode) -> (u8, u8) {
        let normalized = self.normalized();
        match mode {
            TileMode::None => (1, 1),
            TileMode::Both => (normalized.columns, normalized.rows),
            TileMode::XAxis => (normalized.columns, 1),
            TileMode::YAxis => (1, normalized.rows),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct ViewPreferences {
    tile_mode: TileMode,
    tile_preview: TilePreviewSettings,
    show_timeline: bool,
}

impl ViewPreferences {
    fn normalized(mut self) -> Self {
        self.tile_preview = self.tile_preview.normalized();
        self
    }
}

fn load_view_preferences(storage: Option<&dyn eframe::Storage>) -> ViewPreferences {
    storage
        .and_then(|storage| {
            eframe::get_value::<ViewPreferences>(storage, VIEW_PREFERENCES_STORAGE_KEY)
        })
        .unwrap_or_default()
        .normalized()
}

fn recovery_snapshot_within_budget(encoded_bytes: usize) -> bool {
    encoded_bytes <= crate::io::project::MAX_RECOVERY_SNAPSHOT_BYTES
}

fn load_recovery_snapshot(storage: Option<&dyn eframe::Storage>) -> Option<String> {
    storage
        .and_then(|storage| storage.get_string(RECOVERY_STORAGE_KEY))
        .filter(|snapshot| !snapshot.trim().is_empty())
        .filter(|snapshot| {
            let within_budget = recovery_snapshot_within_budget(snapshot.len());
            if !within_budget {
                log::error!(
                    "Ignoring local recovery snapshot larger than the {}-byte limit",
                    crate::io::project::MAX_RECOVERY_SNAPSHOT_BYTES
                );
            }
            within_budget
        })
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
            canvas_action_last_pixel: None,
            canvas_action_tile_offset: None,
            canvas_action_virtual_points: Vec::new(),
            canvas_action_generation: 0,
            canvas_input_blocked: false,
            preview_changes: Vec::new(),
            canvas_texture: None,
            checkerboard_texture: None,
            onion_previous_texture: None,
            onion_next_texture: None,
            onion_texture_pair: None,
            texture_dirty: true,
            show_new_dialog: false,
            show_help_dialog: false,
            show_about_dialog: false,
            active_effect: None,
            pending_resize: None,
            show_custom_resize_dialog: false,
            resize_width: width.to_string(),
            resize_height: height.to_string(),
            resize_error: None,
            new_width: "64".to_string(),
            new_height: "64".to_string(),
            new_project_palette_policy: PalettePolicy::UseDefault,
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
            document_session_id: 0,
            active_frame_generation: 0,
            next_project_save_request_id: 1,
            last_applied_project_save_request_id: 0,
            recovery_snapshot: None,
            show_close_confirmation: false,
            allow_close: false,
            show_spritesheet_import_dialog: false,
            show_image_import_dialog: false,
            image_import_document: None,
            image_import_file_name: None,
            spritesheet_import_mode: SpriteSheetImportMode::NewProject,
            spritesheet_import_source_session_id: None,
            spritesheet_import_source_revision: None,
            spritesheet_import_source_frame_generation: None,
            spritesheet_import_source_active_layer_index: None,
            spritesheet_import_data: None,
            spritesheet_import_texture: None,
            spritesheet_import_columns: "1".to_string(),
            spritesheet_import_rows: "1".to_string(),
            spritesheet_import_error: None,
            show_rulers: false,
            show_guides: true,
            horizontal_guides: Vec::new(),
            vertical_guides: Vec::new(),
            dragging_guide: None,
            tile_mode: TileMode::None,
            tile_preview: TilePreviewSettings::default(),
            fit_tile_preview_requested: false,
            tile_preview_fit_active: false,
            frame_thumbnails: vec![None],
        }
    }

    fn view_preferences(&self) -> ViewPreferences {
        ViewPreferences {
            tile_mode: self.tile_mode,
            tile_preview: self.tile_preview,
            show_timeline: self.show_timeline,
        }
        .normalized()
    }

    /// Constructs the app while retaining cross-platform view preferences and
    /// a native-only dirty recovery snapshot. Web projects use explicit
    /// downloads because browser Local Storage cannot reliably hold them.
    pub fn from_creation_context(
        cc: &eframe::CreationContext<'_>,
        width: u32,
        height: u32,
    ) -> Self {
        let mut app = Self::new(width, height);
        let preferences = load_view_preferences(cc.storage);
        app.tile_mode = preferences.tile_mode;
        app.tile_preview = preferences.tile_preview;
        app.show_timeline = preferences.show_timeline;
        #[cfg(not(target_arch = "wasm32"))]
        {
            app.recovery_snapshot = load_recovery_snapshot(cc.storage);
        }

        app
    }

    /// Queues a new document, asking before it would replace unsaved work.
    pub fn request_new_document(&mut self, width: u32, height: u32, palette_policy: PalettePolicy) {
        self.request_document_replacement(DocumentReplacement::NewDocument {
            width,
            height,
            palette_policy,
        });
    }

    /// Queues a flattened imported image, asking before it would replace
    /// unsaved editable project data.
    pub fn request_imported_image(
        &mut self,
        document: Document,
        file_name: String,
        palette_policy: PalettePolicy,
    ) {
        self.request_document_replacement(DocumentReplacement::ImportedImage {
            document,
            file_name,
            palette_policy,
        });
    }

    /// Queues a decoded project, asking before it replaces unsaved work.
    pub fn request_opened_project(&mut self, editor: EditorState, file_name: String) {
        self.request_document_replacement(DocumentReplacement::OpenedProject { editor, file_name });
    }

    fn request_imported_animation(
        &mut self,
        animation: crate::document::AnimationManager,
        file_name: String,
        palette_policy: PalettePolicy,
    ) {
        self.request_document_replacement(DocumentReplacement::ImportedAnimation {
            animation,
            file_name,
            palette_policy,
        });
    }

    fn request_recovered_project(&mut self, editor: EditorState) {
        self.request_document_replacement(DocumentReplacement::RecoveredProject { editor });
    }

    /// The only gateway for replacing the active project. Decoding can happen
    /// before this call, but no active editor or document-scoped app state may
    /// change until this guard either applies immediately or the user confirms.
    fn request_document_replacement(&mut self, replacement: DocumentReplacement) {
        if self.pending_replacement.is_some() {
            self.status_message = Some((
                "Finish the current project-replacement confirmation before starting another"
                    .to_owned(),
                true,
            ));
            return;
        }

        if self.editor.is_dirty() {
            self.pending_replacement = Some(replacement);
        } else {
            self.commit_document_replacement(replacement);
        }
    }

    fn apply_palette_policy(&mut self, policy: &PalettePolicy) {
        match policy {
            PalettePolicy::KeepCurrent => { /* do nothing */ }
            PalettePolicy::UseDefault => {
                self.editor.animation.current_doc_mut().palette =
                    crate::document::palette_library::default_preset().to_palette();
            }
            PalettePolicy::UsePreset(id) => {
                if let Some(preset) = crate::document::palette_library::get_preset(id) {
                    self.editor.animation.current_doc_mut().palette = preset.to_palette();
                } else {
                    self.editor.animation.current_doc_mut().palette =
                        crate::document::palette_library::default_preset().to_palette();
                }
            }
        }
    }

    fn confirm_pending_document_replacement(&mut self) {
        if let Some(replacement) = self.pending_replacement.take() {
            self.commit_document_replacement(replacement);
        }
    }

    fn cancel_pending_document_replacement(&mut self) {
        self.pending_replacement = None;
    }

    fn commit_document_replacement(&mut self, replacement: DocumentReplacement) {
        // Effect previews belong to one exact project/frame revision. A
        // replacement must never leave a preview capable of applying to the
        // incoming project.
        self.active_effect = None;
        let (status_message, should_be_dirty, show_timeline) = match replacement {
            DocumentReplacement::NewDocument {
                width,
                height,
                palette_policy,
            } => {
                self.editor = EditorState::new(width, height);
                self.apply_palette_policy(&palette_policy);
                ("Created a new project".to_owned(), false, false)
            }
            DocumentReplacement::ImportedImage {
                document,
                file_name,
                palette_policy,
            } => {
                self.editor = EditorState::from_imported_document(document);
                self.apply_palette_policy(&palette_policy);
                (
                    format!("Imported {file_name}; save as a PixelBuddy project to preserve edits"),
                    true,
                    false,
                )
            }
            DocumentReplacement::OpenedProject {
                mut editor,
                file_name,
            } => {
                editor.set_project_name(Some(file_name.clone()));
                let show_timeline = editor.animation.frames.len() > 1;
                self.editor = editor;
                (format!("Opened {file_name}"), false, show_timeline)
            }
            DocumentReplacement::RecoveredProject { mut editor } => {
                editor.set_project_name(None);
                let show_timeline = editor.animation.frames.len() > 1;
                self.editor = editor;
                (
                    "Recovered local draft — save it as a PixelBuddy project".to_owned(),
                    true,
                    show_timeline,
                )
            }
            DocumentReplacement::ImportedAnimation {
                animation,
                file_name,
                palette_policy,
            } => {
                self.editor = EditorState::from_imported_animation(animation);
                self.apply_palette_policy(&palette_policy);
                (format!("Imported sprite sheet {file_name}"), true, true)
            }
        };

        self.editor.reset_runtime_state_for_replacement();
        self.editor.mark_saved();
        if should_be_dirty {
            self.editor.mark_dirty();
        }

        self.document_session_id = self.document_session_id.wrapping_add(1);
        self.active_frame_generation = self.active_frame_generation.wrapping_add(1);
        self.last_applied_project_save_request_id = 0;
        self.pending_replacement = None;
        self.recovery_snapshot = None;
        self.show_close_confirmation = false;
        self.allow_close = false;

        self.cancel_canvas_action();
        self.last_canvas_pixel = None;
        self.canvas_rect = None;
        self.pan_offset = egui::Vec2::ZERO;
        self.auto_fit_requested = true;
        self.tile_preview_fit_active = false;

        self.canvas_texture = None;
        self.onion_previous_texture = None;
        self.onion_next_texture = None;
        self.onion_texture_pair = None;
        self.frame_thumbnails.clear();
        self.frame_thumbnails
            .resize_with(self.editor.animation.frames.len(), || None);
        self.texture_dirty = true;
        self.show_custom_resize_dialog = false;
        self.resize_error = None;

        self.show_new_dialog = false;
        self.new_document_error = None;
        self.pending_resize = None;
        self.export_resolution_dialog = None;
        self.show_spritesheet_import_dialog = false;
        self.spritesheet_import_mode = SpriteSheetImportMode::NewProject;
        self.spritesheet_import_source_session_id = None;
        self.spritesheet_import_source_revision = None;
        self.spritesheet_import_source_frame_generation = None;
        self.spritesheet_import_source_active_layer_index = None;
        self.spritesheet_import_data = None;
        self.spritesheet_import_texture = None;
        self.spritesheet_import_columns = "1".to_owned();
        self.spritesheet_import_rows = "1".to_owned();
        self.spritesheet_import_error = None;

        self.horizontal_guides.clear();
        self.vertical_guides.clear();
        self.dragging_guide = None;
        if show_timeline {
            self.show_timeline = true;
        }

        self.status_message = Some((status_message, false));
        self.status_message_shown_at = None;
        self.last_status_message = None;
    }

    pub fn start_effect(&mut self, effect_type: crate::effects::EffectType) {
        if self.active_effect.is_some() {
            return;
        }

        // Validate the visible frame before adopting it for editing. A failed
        // effect request must not pause playback or dirty the project.
        let Some(active_layer) = self
            .editor
            .document()
            .layers
            .get(self.editor.document().active_layer_index)
        else {
            self.status_message =
                Some(("The active effect layer no longer exists.".to_owned(), true));
            return;
        };
        if active_layer.locked {
            self.status_message = Some((
                "Unlock the active layer before applying an effect.".to_owned(),
                true,
            ));
            return;
        }

        // Playback's visible frame can differ from its persisted selection.
        // An effect is an edit, so first pause and adopt exactly what the user
        // can see, then capture provenance from that stable frame.
        if self.editor.animation.is_playing {
            self.prepare_active_frame_transition();
            let selection_changed = self.editor.pause_animation_for_editing();
            if selection_changed {
                self.finish_active_frame_transition(false);
            }
        }

        let mut effect = crate::effects::ActiveEffectState::new(
            effect_type,
            &self.editor,
            self.document_session_id,
            self.active_frame_generation,
        );
        let selection = self.editor.selection;
        effect.refresh_preview(&selection);
        self.active_effect = Some(effect);
        self.texture_dirty = true;
    }

    pub(crate) fn cancel_active_effect(&mut self) -> bool {
        if self.active_effect.take().is_none() {
            return false;
        }
        self.texture_dirty = true;
        true
    }

    pub(crate) const fn document_session_id(&self) -> u64 {
        self.document_session_id
    }

    pub(crate) const fn active_frame_generation(&self) -> u64 {
        self.active_frame_generation
    }

    fn current_project_import_is_current(
        &mut self,
        source_document_session_id: Option<u64>,
        source_revision: Option<u64>,
        source_active_frame_generation: Option<u64>,
        source_active_layer_index: Option<usize>,
        import_name: &str,
    ) -> bool {
        let document_matches = source_document_session_id == Some(self.document_session_id);
        let revision_matches =
            source_revision.is_none_or(|revision| revision == self.editor.revision());
        let frame_matches = source_active_frame_generation
            .is_none_or(|generation| generation == self.active_frame_generation);
        let layer_matches = source_active_layer_index
            .is_none_or(|index| index == self.editor.document().active_layer_index);
        if document_matches && revision_matches && frame_matches && layer_matches {
            return true;
        }

        self.status_message = Some((
            format!(
                "Skipped {import_name} import because the active project, frame, or target layer changed while the file picker was open"
            ),
            true,
        ));
        false
    }

    /// AppendFrames is document-bound, while ActiveLayer is additionally
    /// bound to the exact revision, frame generation, and layer slot.
    fn current_spritesheet_import_is_current(&mut self) -> bool {
        let active_layer_mode = self.spritesheet_import_mode == SpriteSheetImportMode::ActiveLayer;
        let source_revision = active_layer_mode
            .then_some(self.spritesheet_import_source_revision)
            .flatten();
        let source_frame_generation = active_layer_mode
            .then_some(self.spritesheet_import_source_frame_generation)
            .flatten();
        let source_active_layer_index = active_layer_mode
            .then_some(self.spritesheet_import_source_active_layer_index)
            .flatten();

        self.current_project_import_is_current(
            self.spritesheet_import_source_session_id,
            source_revision,
            source_frame_generation,
            source_active_layer_index,
            "sprite-sheet",
        )
    }

    fn handle_opened_image(
        &mut self,
        data: Vec<u8>,
        file_name: String,
        as_new_project: bool,
        source_document_session_id: u64,
        source_active_frame_generation: u64,
    ) {
        if !as_new_project
            && !self.current_project_import_is_current(
                Some(source_document_session_id),
                None,
                Some(source_active_frame_generation),
                None,
                "image",
            )
        {
            return;
        }

        let result = if file_name.to_lowercase().ends_with(".webp") {
            crate::io::webp::import_webp_to_document(&data)
        } else {
            crate::io::png::import_png_to_document(&data)
        };

        match result {
            Ok(document) if as_new_project => {
                self.image_import_document = Some(document);
                self.image_import_file_name = Some(file_name);
                self.show_image_import_dialog = true;
            }
            Ok(document) => {
                if self.editor.document().layers.len() >= crate::document::MAX_LAYERS_PER_FRAME {
                    self.status_message = Some((
                        format!(
                            "Frames are limited to {} layers",
                            crate::document::MAX_LAYERS_PER_FRAME
                        ),
                        true,
                    ));
                    return;
                }
                if !crate::document::valid_layer_name(&file_name) {
                    self.status_message = Some((
                        format!(
                            "Imported layer names must be at most {} UTF-8 bytes and contain no control characters",
                            crate::document::MAX_LAYER_NAME_BYTES
                        ),
                        true,
                    ));
                    return;
                }
                let document_width = self.editor.document().width;
                let document_height = self.editor.document().height;
                let Some(mut imported_layer) = document.layers.into_iter().next() else {
                    self.status_message = Some(("Imported image had no layers".to_owned(), true));
                    return;
                };
                imported_layer.name = file_name;

                let mut new_layer = crate::document::Layer::new(
                    imported_layer.name.clone(),
                    document_width,
                    document_height,
                );
                for y in 0..imported_layer.canvas.height() {
                    for x in 0..imported_layer.canvas.width() {
                        let pixel = imported_layer.canvas.get_pixel(x, y);
                        new_layer.canvas.set_pixel(x, y, pixel);
                    }
                }

                self.prepare_current_project_import();
                if self.mutate_current_frame("Import image as new layer", true, move |document| {
                    document.layers.push(new_layer);
                    document.active_layer_index = document.layers.len() - 1;
                    true
                }) {
                    self.status_message = Some(("Imported image as new layer".to_owned(), false));
                }
            }
            Err(error) => {
                log::error!("Unable to open image: {error}");
                self.status_message = Some((error.to_string(), true));
            }
        }
    }

    /// Stops preview/canvas activity before a current-project import mutates
    /// the frame collection or artwork.
    fn prepare_current_project_import(&mut self) {
        // A file chooser can complete asynchronously after an effect was
        // opened. Do not let that stale preview hide the successful import.
        self.cancel_active_effect();
        self.cancel_canvas_action();
        self.last_canvas_pixel = None;
        self.editor.pause_animation_for_editing();
    }

    /// Appends imported frames without changing the visible editing frame.
    fn append_imported_animation_frames(
        &mut self,
        animation: crate::document::AnimationManager,
    ) -> bool {
        let Some(total_frames) = self
            .editor
            .animation
            .frames
            .len()
            .checked_add(animation.frames.len())
        else {
            return false;
        };
        if total_frames > crate::document::animation::MAX_ANIMATION_FRAMES {
            return false;
        }
        self.prepare_current_project_import();
        self.editor.animation.frames.extend(animation.frames);
        self.editor.mark_dirty();
        self.consume_edit_effects(EditEffects::all_frame_artwork(true, true));
        true
    }

    /// Validates every frame that an ActiveLayer import would touch before
    /// any pixels are changed, keeping the multi-frame operation atomic.
    fn active_layer_import_target(
        &self,
        animation: &crate::document::AnimationManager,
    ) -> Result<(usize, usize), String> {
        if animation.frames.is_empty() {
            return Err("Sprite sheet contains no frames.".to_owned());
        }

        let active_idx = self.editor.document().active_layer_index;
        let expected_width = self.editor.document().width;
        let expected_height = self.editor.document().height;
        let affected_frames = animation
            .frames
            .len()
            .min(self.editor.animation.frames.len());

        for frame_index in 0..affected_frames {
            let target_frame = &self.editor.animation.frames[frame_index];
            let imported_frame = &animation.frames[frame_index];
            if imported_frame.document.width != expected_width
                || imported_frame.document.height != expected_height
            {
                return Err(format!(
                    "Sprite-sheet frame {} is {}x{}, but the project is {}x{}.",
                    frame_index + 1,
                    imported_frame.document.width,
                    imported_frame.document.height,
                    expected_width,
                    expected_height
                ));
            }
            if imported_frame.document.layers.is_empty() {
                return Err(format!(
                    "Sprite-sheet frame {} contains no image layer.",
                    frame_index + 1
                ));
            }
            let Some(target_layer) = target_frame.document.layers.get(active_idx) else {
                return Err(format!(
                    "Frame {} no longer has target layer {}.",
                    frame_index + 1,
                    active_idx + 1
                ));
            };
            if target_layer.locked {
                return Err(format!(
                    "Target layer {} is locked in frame {}.",
                    active_idx + 1,
                    frame_index + 1
                ));
            }
        }

        Ok((active_idx, affected_frames))
    }

    /// Blends an imported animation into the selected layer across matching
    /// frames after a complete preflight, so failures cannot leave half of
    /// the animation modified.
    fn import_animation_into_active_layer(
        &mut self,
        animation: crate::document::AnimationManager,
    ) -> Result<usize, String> {
        let (active_idx, affected_frames) = self.active_layer_import_target(&animation)?;
        let document_width = self.editor.document().width;
        let document_height = self.editor.document().height;
        self.prepare_current_project_import();
        let mut changed = false;

        for (target_frame, imported_frame) in self
            .editor
            .animation
            .frames
            .iter_mut()
            .zip(animation.frames)
            .take(affected_frames)
        {
            let source_layer = &imported_frame.document.layers[0];
            let target_layer = &mut target_frame.document.layers[active_idx];
            for y in 0..document_height {
                for x in 0..document_width {
                    let pixel = source_layer.canvas.get_pixel(x, y);
                    if pixel[3] > 0 {
                        let before = target_layer.canvas.get_pixel(x, y);
                        target_layer.canvas.blend_pixel(x, y, pixel);
                        changed |= target_layer.canvas.get_pixel(x, y) != before;
                    }
                }
            }
        }

        if changed {
            self.editor.history.clear();
            self.editor.mark_dirty();
            self.consume_edit_effects(EditEffects {
                changed: true,
                current_texture_dirty: true,
                frame_thumbnails: FrameThumbnailInvalidation::Frames(
                    (0..affected_frames).collect(),
                ),
                onion_skin_dirty: true,
            });
        }
        Ok(affected_frames)
    }

    /// Validates and opens the one Save As workflow for the complete editable
    /// project. Menu and keyboard callers share this command, and no request
    /// identity or clean-state metadata changes until encoding succeeds.
    pub fn command_save_project_as(&mut self) -> bool {
        let bytes = match crate::io::project::encode_editor_bytes(&self.editor) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.status_message = Some((error.to_string(), true));
                return false;
            }
        };
        let project_source = self.next_project_save_source();
        let suggested_name = self
            .editor
            .project_name
            .clone()
            .unwrap_or_else(|| "untitled.pbud".to_owned());
        crate::io::trigger_export(
            crate::io::ExportRequest::project(bytes)
                .with_suggested_file_name(suggested_name)
                .with_project_source(project_source),
            self.io_handler.sender.clone(),
        );
        true
    }
    fn next_project_save_source(&mut self) -> crate::io::ProjectSaveSource {
        let request_id = self.next_project_save_request_id;
        self.next_project_save_request_id = self.next_project_save_request_id.wrapping_add(1);
        if self.next_project_save_request_id == 0 {
            self.next_project_save_request_id = 1;
        }

        crate::io::ProjectSaveSource::new(
            self.document_session_id,
            self.editor.revision(),
            request_id,
        )
    }

    fn handle_export_completed(
        &mut self,
        format: crate::io::ExportFormat,
        file_name: String,
        project_source: Option<crate::io::ProjectSaveSource>,
    ) {
        let mut newer_edits_remain = false;
        let mut belongs_to_replaced_project = false;
        let mut superseded_save = false;

        if format == crate::io::ExportFormat::Project {
            if let Some(source) = project_source
                .filter(|source| source.document_session_id() == self.document_session_id)
            {
                if source.request_id() <= self.last_applied_project_save_request_id {
                    superseded_save = true;
                } else {
                    self.last_applied_project_save_request_id = source.request_id();
                    self.editor.set_project_name(Some(file_name.clone()));
                    newer_edits_remain = !self.editor.mark_saved_if_current(source.revision());
                }
            } else {
                // A save dialog can outlive the project that opened it. The
                // file was still written successfully, but its completion must
                // not rename or mark the replacement project as persisted.
                belongs_to_replaced_project = true;
            }
        }

        let message = if belongs_to_replaced_project {
            format!("Saved {format} as {file_name}; active project was not changed")
        } else if superseded_save {
            format!("Saved {format} as {file_name}; a newer save result remains active")
        } else if newer_edits_remain {
            format!("Saved {format} as {file_name}; newer edits remain unsaved")
        } else {
            format!("Saved {format} as {file_name}")
        };
        self.status_message = Some((message, false));
    }

    /// Opens the shared nearest-neighbor scale chooser for a flattened PNG.
    /// The project document remains untouched until the user confirms export.
    pub fn open_png_export_dialog(&mut self) {
        self.open_raster_export_dialog(RasterExportKind::Png);
    }

    pub fn open_webp_export_dialog(&mut self) {
        self.open_raster_export_dialog(RasterExportKind::WebP);
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
    #[cfg(not(target_arch = "wasm32"))]
    pub fn toggle_fullscreen(ctx: &egui::Context, app: &mut PixelBuddyApp) {
        let fullscreen = ctx.input(|input| next_fullscreen_state(input.viewport().fullscreen));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
        app.auto_fit_requested = true;
    }

    /// Returns the effective native presentation used by the custom title bar.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn window_presentation(ctx: &egui::Context) -> WindowPresentation {
        ctx.input(|input| {
            let viewport = input.viewport();
            WindowPresentation::from_viewport(viewport.maximized, viewport.fullscreen)
        })
    }

    /// Maximizes a windowed viewport or restores any screen-filling state.
    ///
    /// Exiting fullscreen and clearing maximization together is intentional:
    /// fullscreen can preserve an underlying maximized placement on Windows.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn toggle_maximize_or_restore(ctx: &egui::Context, app: &mut PixelBuddyApp) {
        match Self::window_presentation(ctx) {
            WindowPresentation::Windowed => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            }
            WindowPresentation::Maximized => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            }
            WindowPresentation::Fullscreen => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            }
        }
        app.auto_fit_requested = true;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn custom_window_borders(ctx: &egui::Context) {
        if !Self::window_presentation(ctx).allows_resize_handles() {
            return;
        }

        let rect = ctx.screen_rect();
        let edge = 6.0;

        let edges = [
            (
                egui::Rect::from_min_max(
                    egui::pos2(rect.min.x + edge, rect.min.y),
                    egui::pos2(rect.max.x - edge, rect.min.y + edge),
                ),
                egui::ResizeDirection::North,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                egui::Rect::from_min_max(
                    egui::pos2(rect.min.x + edge, rect.max.y - edge),
                    egui::pos2(rect.max.x - edge, rect.max.y),
                ),
                egui::ResizeDirection::South,
                egui::CursorIcon::ResizeVertical,
            ),
            (
                egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, rect.min.y + edge),
                    egui::pos2(rect.min.x + edge, rect.max.y - edge),
                ),
                egui::ResizeDirection::West,
                egui::CursorIcon::ResizeHorizontal,
            ),
            (
                egui::Rect::from_min_max(
                    egui::pos2(rect.max.x - edge, rect.min.y + edge),
                    egui::pos2(rect.max.x, rect.max.y - edge),
                ),
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
                egui::Rect::from_min_size(
                    egui::pos2(rect.max.x - edge, rect.min.y),
                    egui::vec2(edge, edge),
                ),
                egui::ResizeDirection::NorthEast,
                egui::CursorIcon::ResizeNeSw,
            ),
            (
                egui::Rect::from_min_size(
                    egui::pos2(rect.min.x, rect.max.y - edge),
                    egui::vec2(edge, edge),
                ),
                egui::ResizeDirection::SouthWest,
                egui::CursorIcon::ResizeNeSw,
            ),
            (
                egui::Rect::from_min_size(
                    egui::pos2(rect.max.x - edge, rect.max.y - edge),
                    egui::vec2(edge, edge),
                ),
                egui::ResizeDirection::SouthEast,
                egui::CursorIcon::ResizeNwSe,
            ),
        ];

        for (id_str, rect, dir, cursor) in edges
            .into_iter()
            .zip(["n", "s", "w", "e"])
            .map(|((r, d, c), id)| (id, r, d, c))
            .chain(
                corners
                    .into_iter()
                    .zip(["nw", "ne", "sw", "se"])
                    .map(|((r, d, c), id)| (id, r, d, c)),
            )
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

    pub(crate) fn consume_edit_effects(&mut self, effects: EditEffects) -> bool {
        if !effects.changed {
            return false;
        }

        if effects.current_texture_dirty {
            self.texture_dirty = true;
        }
        match effects.frame_thumbnails {
            FrameThumbnailInvalidation::None => {}
            FrameThumbnailInvalidation::Current => {
                if let Some(thumbnail) = self
                    .frame_thumbnails
                    .get_mut(self.editor.animation.current_frame_index)
                {
                    *thumbnail = None;
                }
            }
            FrameThumbnailInvalidation::Frames(indices) => {
                for index in indices {
                    if let Some(thumbnail) = self.frame_thumbnails.get_mut(index) {
                        *thumbnail = None;
                    }
                }
            }
            FrameThumbnailInvalidation::All => self
                .frame_thumbnails
                .iter_mut()
                .for_each(|thumbnail| *thumbnail = None),
            FrameThumbnailInvalidation::Structure => {
                self.frame_thumbnails.clear();
                self.frame_thumbnails
                    .resize_with(self.editor.animation.frames.len(), || None);
            }
        }
        if effects.onion_skin_dirty {
            self.invalidate_onion_skin_cache();
        }
        true
    }

    fn mutate_current_frame(
        &mut self,
        description: &'static str,
        artwork_changed: bool,
        mutation: impl FnOnce(&mut Document) -> bool,
    ) -> bool {
        let changed = self.editor.mutate_document(description, mutation);
        let effects = if artwork_changed {
            EditEffects::current_frame_artwork(changed)
        } else {
            EditEffects::persisted_only(changed)
        };
        self.consume_edit_effects(effects)
    }

    pub(crate) fn select_layer_current_frame(&mut self, index: usize) -> bool {
        let changed = self.editor.select_layer_current_frame(index);
        self.consume_edit_effects(EditEffects::persisted_only(changed))
    }

    pub(crate) fn set_layer_visibility_current_frame(
        &mut self,
        index: usize,
        visible: bool,
    ) -> bool {
        self.mutate_current_frame("Toggle layer visibility", true, |document| {
            let Some(layer) = document.layers.get_mut(index) else {
                return false;
            };
            if layer.visible == visible {
                return false;
            }
            layer.visible = visible;
            true
        })
    }

    pub(crate) fn rename_layer_current_frame(&mut self, index: usize, name: String) -> bool {
        if !crate::document::valid_layer_name(&name) {
            self.status_message = Some((
                format!(
                    "Layer names must be at most {} UTF-8 bytes and contain no control characters",
                    crate::document::MAX_LAYER_NAME_BYTES
                ),
                true,
            ));
            return false;
        }
        self.mutate_current_frame("Rename layer", false, move |document| {
            let Some(layer) = document.layers.get_mut(index) else {
                return false;
            };
            if layer.name == name {
                return false;
            }
            layer.name = name;
            true
        })
    }

    pub(crate) fn move_layer_current_frame(&mut self, from: usize, to: usize) -> bool {
        self.mutate_current_frame("Move layer", true, |document| {
            if from >= document.layers.len() || to >= document.layers.len() || from == to {
                return false;
            }
            document.move_layer(from, to);
            true
        })
    }

    pub(crate) fn set_layer_opacity_current_frame(&mut self, index: usize, opacity: f32) -> bool {
        self.mutate_current_frame("Set layer opacity", true, |document| {
            let Some(layer) = document.layers.get_mut(index) else {
                return false;
            };
            let opacity = opacity.clamp(0.0, 1.0);
            if layer.opacity == opacity {
                return false;
            }
            layer.opacity = opacity;
            true
        })
    }

    pub(crate) fn set_layer_locked_current_frame(&mut self, index: usize, locked: bool) -> bool {
        self.mutate_current_frame("Lock layer", false, |document| {
            let Some(layer) = document.layers.get_mut(index) else {
                return false;
            };
            if layer.locked == locked {
                return false;
            }
            layer.locked = locked;
            true
        })
    }

    pub(crate) fn set_layer_blend_mode_current_frame(
        &mut self,
        index: usize,
        mode: crate::document::BlendMode,
    ) -> bool {
        self.mutate_current_frame("Set layer blend mode", true, |document| {
            let Some(layer) = document.layers.get_mut(index) else {
                return false;
            };
            if layer.blend_mode == mode {
                return false;
            }
            layer.blend_mode = mode;
            true
        })
    }

    pub(crate) fn move_palette_color_current_frame(&mut self, from: usize, to: usize) -> bool {
        self.mutate_current_frame("Move palette color", false, |document| {
            document.palette.move_color(from, to)
        })
    }

    pub(crate) fn remove_palette_color_current_frame(&mut self, index: usize) -> bool {
        self.mutate_current_frame("Remove palette color", false, |document| {
            document.palette.remove_color(index)
        })
    }

    pub(crate) fn add_palette_color_current_frame(&mut self, color: [u8; 4]) -> bool {
        if self.editor.document().palette.colors.len() >= crate::document::MAX_PALETTE_COLORS {
            self.status_message = Some((
                format!(
                    "Palettes are limited to {} colors",
                    crate::document::MAX_PALETTE_COLORS
                ),
                true,
            ));
            return false;
        }
        self.mutate_current_frame("Add palette color", false, |document| {
            document.palette.add_color(color);
            true
        })
    }

    pub(crate) fn select_palette_color_current_frame(&mut self, index: usize) -> bool {
        let changed = self.editor.select_palette_color_current_frame(index);
        self.consume_edit_effects(EditEffects::persisted_only(changed))
    }

    pub(crate) fn undo_current_frame(&mut self) -> bool {
        let changed = self.editor.undo();
        self.consume_edit_effects(EditEffects::current_frame_artwork(changed))
    }

    pub(crate) fn redo_current_frame(&mut self) -> bool {
        let changed = self.editor.redo();
        self.consume_edit_effects(EditEffects::current_frame_artwork(changed))
    }

    pub(crate) fn jump_to_undo_index_current_frame(&mut self, index: usize) -> bool {
        let changed = self.editor.jump_to_undo_index(index);
        self.consume_edit_effects(EditEffects::current_frame_artwork(changed))
    }

    pub(crate) fn set_animation_fps(&mut self, fps: u32, current_time: f64) -> bool {
        if self.editor.animation.fps == fps {
            return false;
        }
        self.editor.set_animation_fps(fps);
        self.editor.animation.reset_playback_clock(current_time);
        self.consume_edit_effects(EditEffects::persisted_only(true))
    }

    pub(crate) fn set_onion_skin_enabled(&mut self, enabled: bool) -> bool {
        if self.editor.animation.onion_skin_enabled == enabled {
            return false;
        }
        self.editor.set_onion_skin_enabled(enabled);
        self.consume_edit_effects(EditEffects {
            changed: true,
            current_texture_dirty: true,
            ..EditEffects::default()
        })
    }

    pub(crate) fn set_onion_skin_opacity(&mut self, opacity: f32) -> bool {
        let opacity = opacity.clamp(0.0, 1.0);
        if self.editor.animation.onion_skin_opacity == opacity {
            return false;
        }
        self.editor.set_onion_skin_opacity(opacity);
        self.consume_edit_effects(EditEffects::persisted_only(true))
    }
    pub(crate) fn create_animation_tag(
        &mut self,
        tag: crate::document::animation::FrameTag,
    ) -> bool {
        let changed = self.editor.create_animation_tag(tag);
        self.consume_edit_effects(EditEffects::persisted_only(changed))
    }

    pub(crate) fn update_animation_tag(
        &mut self,
        index: usize,
        tag: crate::document::animation::FrameTag,
    ) -> bool {
        let changed = self.editor.update_animation_tag(index, tag);
        self.consume_edit_effects(EditEffects::persisted_only(changed))
    }

    pub(crate) fn remove_animation_tag(&mut self, index: usize) -> bool {
        let changed = self.editor.remove_animation_tag(index);
        self.consume_edit_effects(EditEffects::persisted_only(changed))
    }
    fn synchronize_all_frame_artwork_change(&mut self) {
        self.consume_edit_effects(EditEffects::all_frame_artwork(true, false));
    }

    pub(crate) fn resize_canvas(&mut self, width: u32, height: u32) -> bool {
        match self.editor.resize_animation(width, height) {
            Ok(true) => {}
            Ok(false) => return false,
            Err(error) => {
                self.status_message = Some((error.to_string(), true));
                return false;
            }
        }
        self.cancel_canvas_action();
        self.last_canvas_pixel = None;
        self.pan_offset = egui::Vec2::ZERO;
        self.auto_fit_requested = true;
        self.synchronize_all_frame_artwork_change();
        true
    }

    pub(crate) fn add_layer_all_frames(&mut self) -> bool {
        if !self.editor.add_layer_all_frames() {
            if self
                .editor
                .animation
                .frames
                .iter()
                .any(|frame| frame.document.layers.len() >= crate::document::MAX_LAYERS_PER_FRAME)
            {
                self.status_message = Some((
                    format!(
                        "Frames are limited to {} layers",
                        crate::document::MAX_LAYERS_PER_FRAME
                    ),
                    true,
                ));
            }
            return false;
        }
        self.synchronize_all_frame_artwork_change();
        true
    }

    pub(crate) fn duplicate_active_layer_all_frames(&mut self) -> bool {
        if !self.editor.duplicate_active_layer_all_frames() {
            if self
                .editor
                .animation
                .frames
                .iter()
                .any(|frame| frame.document.layers.len() >= crate::document::MAX_LAYERS_PER_FRAME)
            {
                self.status_message = Some((
                    format!(
                        "Frames are limited to {} layers",
                        crate::document::MAX_LAYERS_PER_FRAME
                    ),
                    true,
                ));
            }
            return false;
        }
        self.synchronize_all_frame_artwork_change();
        true
    }

    pub(crate) fn remove_active_layer_all_frames(&mut self) -> bool {
        if !self.editor.remove_active_layer_all_frames() {
            return false;
        }
        self.synchronize_all_frame_artwork_change();
        true
    }
    /// Clears app-owned state that is meaningful only for the frame that was
    /// active when an interaction began. Call this only for a transition that
    /// is known to succeed so same-frame and boundary requests remain no-ops.
    fn prepare_active_frame_transition(&mut self) {
        let outgoing_index = self.editor.animation.current_frame_index;
        self.prepare_active_frame_transition_from(outgoing_index);
    }

    /// Prepares transition effects for a captured outgoing frame.
    fn prepare_active_frame_transition_from(&mut self, outgoing_index: usize) {
        if self.texture_dirty {
            if let Some(thumbnail) = self.frame_thumbnails.get_mut(outgoing_index) {
                *thumbnail = None;
            }
        }
        self.cancel_canvas_action();
        self.last_canvas_pixel = None;
    }

    /// Synchronizes rendering and frame-bound UI identity after an active
    /// frame transition. Structural changes rebuild index-aligned thumbnails;
    /// a plain selection keeps them because no artwork changed.
    fn finish_active_frame_transition(&mut self, structure_changed: bool) {
        self.active_frame_generation = self.active_frame_generation.wrapping_add(1);
        self.consume_edit_effects(EditEffects {
            changed: true,
            current_texture_dirty: true,
            frame_thumbnails: if structure_changed {
                FrameThumbnailInvalidation::Structure
            } else {
                FrameThumbnailInvalidation::None
            },
            onion_skin_dirty: true,
        });
    }

    /// The sole UI/app command for selecting an existing frame.
    ///
    /// Model history and marquee state are handled by `EditorState`; this
    /// layer cancels unfinished canvas gestures and synchronizes caches. A
    /// same-frame or invalid request changes nothing.
    pub(crate) fn select_frame(&mut self, index: usize) -> bool {
        if index >= self.editor.animation.frames.len() {
            return false;
        }
        let displayed_index = self.editor.animation.current_frame_index;
        let selected_index = self.editor.animation.selected_frame_index();
        if index == displayed_index && index == selected_index {
            return false;
        }

        self.prepare_active_frame_transition();
        let changed = self.editor.select_frame(index);
        debug_assert!(changed, "the frame request was validated above");
        self.finish_active_frame_transition(false);
        true
    }

    pub(crate) fn select_previous_frame(&mut self) -> bool {
        let Some(previous) = self.editor.animation.current_frame_index.checked_sub(1) else {
            return false;
        };
        self.select_frame(previous)
    }

    pub(crate) fn select_next_frame(&mut self) -> bool {
        let next = self.editor.animation.current_frame_index.saturating_add(1);
        self.select_frame(next)
    }

    /// Advances preview playback through the same app-level synchronization
    /// effects as manual selection without marking every preview tick dirty.
    fn update_animation_playback(&mut self, current_time: f64) -> bool {
        if self.active_effect.is_some() {
            return false;
        }
        let outgoing_index = self.editor.animation.current_frame_index;
        if !self.editor.update_animation_playback(current_time) {
            return false;
        }

        self.prepare_active_frame_transition_from(outgoing_index);
        self.finish_active_frame_transition(false);
        true
    }

    pub(crate) fn toggle_animation_playback(&mut self, current_time: f64) {
        if self.active_effect.is_some() {
            return;
        }
        if self.editor.animation.frames.len() > 1 {
            // Editing and preview playback must never own the canvas pointer
            // lifecycle at the same time.
            self.cancel_canvas_action();
            self.last_canvas_pixel = None;
        }
        self.editor.toggle_animation_playback(current_time);
    }

    pub(crate) fn stop_animation(&mut self) -> bool {
        if self.editor.animation.current_frame_index == 0
            && self.editor.animation.selected_frame_index() == 0
        {
            self.editor.animation.stop();
            return false;
        }

        self.prepare_active_frame_transition();
        let changed = self.editor.stop_animation();
        debug_assert!(changed, "a nonzero active frame must return to frame zero");
        self.finish_active_frame_transition(false);
        true
    }

    pub(crate) fn add_frame(&mut self) -> bool {
        if self.editor.animation.frames.len() >= crate::document::animation::MAX_ANIMATION_FRAMES {
            self.status_message = Some((
                format!(
                    "Animations are limited to {} frames",
                    crate::document::animation::MAX_ANIMATION_FRAMES
                ),
                true,
            ));
            return false;
        }
        self.prepare_active_frame_transition();
        let changed = self.editor.add_frame();
        debug_assert!(changed, "the frame limit was checked above");
        self.finish_active_frame_transition(true);
        true
    }

    pub(crate) fn duplicate_frame(&mut self) -> bool {
        if self.editor.animation.frames.len() >= crate::document::animation::MAX_ANIMATION_FRAMES {
            self.status_message = Some((
                format!(
                    "Animations are limited to {} frames",
                    crate::document::animation::MAX_ANIMATION_FRAMES
                ),
                true,
            ));
            return false;
        }
        self.prepare_active_frame_transition();
        let changed = self.editor.duplicate_frame();
        debug_assert!(changed, "the frame limit was checked above");
        self.finish_active_frame_transition(true);
        true
    }

    pub(crate) fn remove_current_frame(&mut self) -> bool {
        if self.editor.animation.frames.len() <= 1 {
            return false;
        }

        self.prepare_active_frame_transition();
        self.editor.remove_frame();
        self.finish_active_frame_transition(true);
        true
    }

    /// Apply a set of pixel changes to the active layer, recording undo history.
    pub fn apply_tool_changes(&mut self, changes: Vec<tools::PixelChange>) {
        if changes.is_empty() {
            return;
        }

        self.editor.pause_animation_for_editing();

        let active_layer_index = self.editor.document().active_layer_index;
        let Some(layer) = self.editor.document().layers.get(active_layer_index) else {
            return;
        };
        if layer.locked {
            self.status_message = Some(("Layer is locked".to_owned(), true));

            return;
        }
        // A tool can emit multiple writes to the same pixel (notably Move).
        // Keep only the final value before capturing the original color for undo.
        let final_changes: BTreeMap<(u32, u32), [u8; 4]> = changes
            .into_iter()
            .map(|(x, y, color)| ((x, y), color))
            .collect();
        let (has_sel, sel_min_x, sel_max_x, sel_min_y, sel_max_y) = if self.editor.selection.active
        {
            (
                true,
                self.editor.selection.min_x(),
                self.editor.selection.max_x(),
                self.editor.selection.min_y(),
                self.editor.selection.max_y(),
            )
        } else {
            (false, 0, 0, 0, 0)
        };
        let mut history_changes = Vec::new();
        {
            let layer = &mut self.editor.document_mut().layers[active_layer_index];
            for ((x, y), new_color) in final_changes {
                if layer.canvas.in_bounds(x as i32, y as i32) {
                    if has_sel
                        && ((x as i32) < sel_min_x
                            || (x as i32) > sel_max_x
                            || (y as i32) < sel_min_y
                            || (y as i32) > sel_max_y)
                    {
                        continue;
                    }
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
            self.consume_edit_effects(EditEffects::current_frame_artwork(true));
        }
    }

    pub fn clear_selection(&mut self) {
        let active_layer_index = self.editor.document().active_layer_index;
        let Some(layer) = self.editor.document().layers.get(active_layer_index) else {
            return;
        };
        if layer.locked {
            self.status_message = Some(("Layer is locked".to_owned(), true));

            return;
        }

        let mut changes = Vec::new();
        let (min_x, max_x, min_y, max_y) = if self.editor.selection.active {
            (
                self.editor.selection.min_x(),
                self.editor.selection.max_x(),
                self.editor.selection.min_y(),
                self.editor.selection.max_y(),
            )
        } else {
            (
                0,
                self.editor.document().width.saturating_sub(1) as i32,
                0,
                self.editor.document().height.saturating_sub(1) as i32,
            )
        };

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                changes.push((x as u32, y as u32, [0, 0, 0, 0]));
            }
        }
        self.apply_tool_changes(changes);
    }

    pub fn flip_horizontal(&mut self) {
        self.mutate_current_frame("Flip Horizontal", true, |doc| {
            let active_layer_index = doc.active_layer_index;
            if active_layer_index >= doc.layers.len() || doc.layers[active_layer_index].locked {
                return false;
            }
            let width = doc.width;
            let height = doc.height;
            let layer = &mut doc.layers[active_layer_index];
            for y in 0..height {
                for x in 0..(width / 2) {
                    let opp_x = width - 1 - x;
                    let left = layer.canvas.get_pixel(x, y);
                    let right = layer.canvas.get_pixel(opp_x, y);
                    layer.canvas.set_pixel(x, y, right);
                    layer.canvas.set_pixel(opp_x, y, left);
                }
            }
            true
        });
    }

    pub fn flip_vertical(&mut self) {
        self.mutate_current_frame("Flip Vertical", true, |doc| {
            let active_layer_index = doc.active_layer_index;
            if active_layer_index >= doc.layers.len() || doc.layers[active_layer_index].locked {
                return false;
            }
            let width = doc.width;
            let height = doc.height;
            let layer = &mut doc.layers[active_layer_index];
            for y in 0..(height / 2) {
                let opp_y = height - 1 - y;
                for x in 0..width {
                    let top = layer.canvas.get_pixel(x, y);
                    let bottom = layer.canvas.get_pixel(x, opp_y);
                    layer.canvas.set_pixel(x, y, bottom);
                    layer.canvas.set_pixel(opp_y, y, top);
                }
            }
            true
        });
    }

    /// Explains why Merge Down is unavailable for the selected frame.
    ///
    /// Layer index zero is the bottom of the stack. Flattening non-Normal
    /// blend modes into one layer cannot generally preserve their interaction
    /// with layers farther below, and combining mixed visibility states would
    /// either reveal hidden pixels or discard visible ones.
    pub(crate) fn merge_down_unavailable_reason(&self) -> Option<&'static str> {
        let document = self.editor.document();
        let active = document.active_layer_index;
        if active == 0 || active >= document.layers.len() {
            return Some("The bottom layer has no layer below it");
        }

        let top = &document.layers[active];
        let bottom = &document.layers[active - 1];
        if top.locked || bottom.locked {
            return Some("Unlock both layers before merging");
        }
        if !top.visible || !bottom.visible {
            return Some("Both layers must be visible before merging");
        }
        if top.blend_mode != crate::document::BlendMode::Normal
            || bottom.blend_mode != crate::document::BlendMode::Normal
        {
            return Some("Merge Down currently supports Normal blend mode only");
        }
        None
    }

    pub fn merge_down(&mut self) -> bool {
        if self.merge_down_unavailable_reason().is_some() {
            return false;
        }

        let changed = self.editor.mutate_document("Merge Down", |document| {
            let active = document.active_layer_index;
            let destination = active - 1;
            let top = document.layers[active].clone();
            let bottom = document.layers[destination].clone();
            let mut merged = bottom.clone();

            for y in 0..document.height {
                for x in 0..document.width {
                    let bottom_pixel = crate::document::Layer::blend_mode_apply(
                        [0, 0, 0, 0],
                        bottom.canvas.get_pixel(x, y),
                        crate::document::BlendMode::Normal,
                        bottom.opacity,
                    );
                    let merged_pixel = crate::document::Layer::blend_mode_apply(
                        bottom_pixel,
                        top.canvas.get_pixel(x, y),
                        crate::document::BlendMode::Normal,
                        top.opacity,
                    );
                    merged.canvas.set_pixel(x, y, merged_pixel);
                }
            }

            // The lower layer's identity/name is retained. Its presentation
            // metadata is baked into the pixels, so the result is one visible,
            // editable Normal layer at full opacity.
            merged.opacity = 1.0;
            merged.blend_mode = crate::document::BlendMode::Normal;
            merged.visible = true;
            merged.locked = false;
            document.layers[destination] = merged;
            document.layers.remove(active);
            document.active_layer_index = destination;
            true
        });

        self.consume_edit_effects(EditEffects::current_frame_artwork(changed))
    }
    pub fn flatten_visible(&mut self) {
        self.mutate_current_frame("Flatten Visible", true, |doc| {
            let flattened = doc.flatten();
            let mut layer = crate::document::layer::Layer::new("Background", doc.width, doc.height);
            layer.canvas = flattened;
            doc.layers = vec![layer];
            doc.active_layer_index = 0;
            true
        });
    }

    /// Starts an interaction that owns its own press/release lifecycle.
    ///
    /// Canvas drags intentionally do not rely on `Response::hover_pos()` for
    /// cleanup: egui can still report a release after the cursor leaves the
    /// canvas, while `hover_pos()` is then `None`.
    pub fn begin_canvas_action(&mut self, x: i32, y: i32) {
        self.begin_canvas_action_on_tile((x, y), (0, 0), (x, y));
    }

    pub(crate) fn begin_canvas_action_on_tile(
        &mut self,
        pixel: (i32, i32),
        tile_offset: (i32, i32),
        virtual_pixel: (i32, i32),
    ) {
        debug_assert!(pixel.0 >= 0 && pixel.1 >= 0);
        // A pointer action adopts the currently displayed frame for editing;
        // playback must not advance underneath the gesture.
        self.editor.pause_animation_for_editing();
        self.is_drawing = true;
        self.canvas_action_generation = self.canvas_action_generation.wrapping_add(1);
        self.stroke_points.clear();
        self.stroke_points.push((pixel.0 as u32, pixel.1 as u32));
        self.shape_start = Some(pixel);
        self.last_canvas_pixel = Some(pixel);
        self.canvas_action_last_pixel = Some(pixel);
        self.canvas_action_tile_offset = Some(tile_offset);
        self.canvas_action_virtual_points.clear();
        self.canvas_action_virtual_points.push(virtual_pixel);
        self.preview_changes.clear();
    }

    /// Clears transient interaction state without mutating the document.
    pub fn cancel_canvas_action(&mut self) {
        let cancel_partial_marquee =
            self.is_drawing && self.editor.active_tool == ToolType::Marquee;
        self.is_drawing = false;
        self.stroke_points.clear();
        self.shape_start = None;
        self.canvas_action_last_pixel = None;
        self.canvas_action_tile_offset = None;
        self.canvas_action_virtual_points.clear();
        self.preview_changes.clear();
        if cancel_partial_marquee {
            self.editor.selection.deselect();
        }
    }

    /// Switching tools cannot reinterpret an unfinished drag as a different
    /// operation when the pointer is eventually released.
    pub fn set_active_tool(&mut self, tool: ToolType) {
        if self.editor.active_tool != tool {
            self.cancel_canvas_action();
        }
        self.editor.set_active_tool(tool);
    }

    pub fn paste_origin(&self, clipboard_width: u32, clipboard_height: u32) -> (u32, u32) {
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

    fn can_create_canvas(width: u32, height: u32) -> Result<(), String> {
        crate::io::validate_canvas_dimensions(width, height).map_err(|error| error.to_string())
    }

    /// Returns the source frame dimensions used by a raster export preview.
    /// PNG exports only the active frame, while GIF and sprite-sheet exports
    /// begin with the first animation frame and validate every other frame at
    /// the time of export.
    fn raster_export_source_dimensions(&self, kind: RasterExportKind) -> Option<(u32, u32, usize)> {
        match kind {
            RasterExportKind::Png | RasterExportKind::WebP => {
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
            RasterExportKind::WebP => crate::io::webp::export_document_to_webp_at_dimensions(
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
            RasterExportKind::WebP => crate::io::ExportRequest::webp(encoded),
            RasterExportKind::Gif => crate::io::ExportRequest::gif(encoded),
            RasterExportKind::SpriteSheetPng => crate::io::ExportRequest::sprite_sheet_png(encoded),
        };
        crate::io::trigger_export(request, self.io_handler.sender.clone());
        Ok(())
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
    fn foreground_dialog_open(&self) -> bool {
        self.show_new_dialog
            || self.show_help_dialog
            || self.show_about_dialog
            || self.pending_resize.is_some()
            || self.show_custom_resize_dialog
            || self.export_resolution_dialog.is_some()
            || self.pending_replacement.is_some()
            || self.recovery_snapshot.is_some()
            || self.show_close_confirmation
            || self.show_spritesheet_import_dialog
            || self.active_effect.is_some()
    }

    fn shortcut_permissions(&self, ctx: &egui::Context) -> ShortcutPermissions {
        shortcut_permissions(
            ctx.wants_keyboard_input(),
            ctx.memory(|memory| memory.top_modal_layer().is_some()),
            ctx.memory(|memory| memory.any_popup_open()),
            self.foreground_dialog_open(),
        )
    }
    fn handle_shortcuts(&mut self, ctx: &egui::Context, current_time: f64) {
        let permissions = self.shortcut_permissions(ctx);
        let commands = ctx.input(|input| ShortcutDispatcher::commands(input, permissions));
        for command in commands {
            match command {
                #[cfg(not(target_arch = "wasm32"))]
                ShortcutCommand::ToggleFullscreen => Self::toggle_fullscreen(ctx, self),
                ShortcutCommand::CancelCanvasAction => self.cancel_canvas_action(),
                ShortcutCommand::SaveProjectAs => {
                    self.command_save_project_as();
                }
                ShortcutCommand::TogglePlayback => {
                    self.toggle_animation_playback(current_time);
                }
                ShortcutCommand::Undo => {
                    self.undo_current_frame();
                }
                ShortcutCommand::Redo => {
                    self.redo_current_frame();
                }
                ShortcutCommand::NewProject => self.show_new_dialog = true,
                ShortcutCommand::OpenProject => {
                    crate::io::trigger_open_project(self.io_handler.sender.clone());
                }
                ShortcutCommand::SwapColors => self.editor.swap_colors(),
                ShortcutCommand::Deselect => self.editor.selection.deselect(),
                ShortcutCommand::SelectAll => self.editor.selection.set_rect(
                    0,
                    0,
                    self.editor.document().width as i32 - 1,
                    self.editor.document().height as i32 - 1,
                ),
                ShortcutCommand::Copy => {
                    self.editor.clipboard = crate::editor::clipboard::ClipboardBuffer::copy(
                        self.editor.document(),
                        &self.editor.selection,
                    );
                }
                ShortcutCommand::Cut => {
                    self.editor.clipboard = crate::editor::clipboard::ClipboardBuffer::copy(
                        self.editor.document(),
                        &self.editor.selection,
                    );
                    if self.editor.clipboard.is_some() {
                        self.clear_selection();
                    }
                }
                ShortcutCommand::ClearSelection => {
                    self.clear_selection();
                }
                ShortcutCommand::Paste => {
                    if let Some(buf) = self.editor.clipboard.clone() {
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
                ShortcutCommand::DecreaseBrushSize => {
                    if self.editor.brush_size > 1 {
                        self.editor.brush_size -= 1;
                    }
                }
                ShortcutCommand::IncreaseBrushSize => {
                    if self.editor.brush_size < 8 {
                        self.editor.brush_size += 1;
                    }
                }
                ShortcutCommand::PreviousFrame => {
                    self.select_previous_frame();
                }
                ShortcutCommand::NextFrame => {
                    self.select_next_frame();
                }
                ShortcutCommand::SelectTool(tool) => {
                    self.set_active_tool(tool);
                }
            }
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
                FileAction::OpenedImage {
                    data,
                    file_name,
                    as_new_project,
                    source_document_session_id,
                    source_active_frame_generation,
                } => self.handle_opened_image(
                    data,
                    file_name,
                    as_new_project,
                    source_document_session_id,
                    source_active_frame_generation,
                ),
                FileAction::OpenedSpriteSheet {
                    data,
                    file_name,
                    as_new_project,
                    source_document_session_id,
                    source_revision,
                    source_active_frame_generation,
                    source_active_layer_index,
                } => {
                    if !as_new_project
                        && !self.current_project_import_is_current(
                            Some(source_document_session_id),
                            None,
                            None,
                            None,
                            "sprite-sheet",
                        )
                    {
                        continue;
                    }

                    match crate::io::spritesheet::decode_spritesheet_preview(&data, &file_name) {
                        Ok((preview_width, preview_height, pixels)) => {
                            self.show_spritesheet_import_dialog = true;
                            self.spritesheet_import_mode = if as_new_project {
                                SpriteSheetImportMode::NewProject
                            } else {
                                SpriteSheetImportMode::AppendFrames
                            };
                            self.spritesheet_import_source_session_id =
                                Some(source_document_session_id);
                            self.spritesheet_import_source_revision = Some(source_revision);
                            self.spritesheet_import_source_frame_generation =
                                Some(source_active_frame_generation);
                            self.spritesheet_import_source_active_layer_index =
                                Some(source_active_layer_index);
                            self.spritesheet_import_columns = "1".to_owned();
                            self.spritesheet_import_rows = "1".to_owned();
                            self.spritesheet_import_error = None;
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                [preview_width as usize, preview_height as usize],
                                &pixels,
                            );
                            self.spritesheet_import_texture = Some(ctx.load_texture(
                                "spritesheet_preview",
                                color_image,
                                egui::TextureOptions::NEAREST,
                            ));
                            self.spritesheet_import_data = Some((data, file_name));
                        }
                        Err(error) => {
                            log::error!("Unable to preview sprite sheet: {error}");
                            self.show_spritesheet_import_dialog = false;
                            self.spritesheet_import_data = None;
                            self.spritesheet_import_texture = None;
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
                    project_source,
                } => self.handle_export_completed(format, file_name, project_source),
                FileAction::Failed(error) => {
                    log::error!("File operation failed: {error}");
                    self.status_message = Some((error.to_string(), true));
                }
            }
        }

        // Handle animation playback stepping
        let current_time = ctx.input(|i| i.time);
        if self.active_effect.is_none() {
            self.update_animation_playback(current_time);
        }
        if self.editor.animation.is_playing {
            ctx.request_repaint();
        }

        if self.active_effect.is_none() {
            self.handle_shortcuts(ctx, current_time);
        }

        self.canvas_input_blocked = false;
        crate::ui::menu_bar::show(ctx, self);
        crate::ui::toolbar::show(ctx, self);
        crate::ui::layers_panel::show(ctx, self);
        crate::ui::status_bar::show(ctx, self);
        if self.show_timeline {
            crate::ui::timeline_panel::show(ctx, self);
        }
        self.canvas_input_blocked |= ctx.memory(|memory| memory.any_popup_open());
        self.canvas_input_blocked |= self.active_effect.is_some();
        crate::ui::canvas_view::show(ctx, self);
        crate::effects::show_effect_modal(ctx, self);

        #[cfg(not(target_arch = "wasm32"))]
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
                    self.draw_palette_policy_selector(ui);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            if let (Ok(w), Ok(h)) = (
                                self.new_width.parse::<u32>(),
                                self.new_height.parse::<u32>(),
                            ) {
                                match Self::can_create_canvas(w, h) {
                                    Ok(()) => {
                                        self.request_new_document(
                                            w,
                                            h,
                                            self.new_project_palette_policy.clone(),
                                        );
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

        if self.show_help_dialog {
            let mut open = true;
            egui::Window::new("Keyboard Shortcuts")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    egui::Grid::new("shortcuts_grid")
                        .num_columns(2)
                        .spacing([40.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            let shortcuts = [
                                ("Ctrl+N", "New Document"),
                                ("Ctrl+O", "Open Project"),
                                ("Ctrl+S", "Save Project"),
                                ("Ctrl+Z", "Undo"),
                                ("Ctrl+Y / Ctrl+Shift+Z", "Redo"),
                                ("Ctrl+C", "Copy"),
                                ("Ctrl+V", "Paste"),
                                ("Ctrl+D", "Deselect"),
                                ("X", "Swap Colors"),
                                ("Space", "Play/Pause Animation"),
                                ("H", "Hand Tool (Pan)"),
                                ("Z", "Zoom Tool"),
                                ("M", "Marquee Select"),
                                ("V", "Move Tool"),
                                ("B", "Pencil Tool"),
                                ("E", "Eraser Tool"),
                                ("L", "Line Tool"),
                                ("R", "Rectangle Tool"),
                                ("O", "Ellipse Tool"),
                                ("G", "Fill Tool"),
                                ("I", "Eyedropper Tool"),
                            ];
                            for (keys, desc) in shortcuts {
                                ui.label(egui::RichText::new(keys).strong());
                                ui.label(desc);
                                ui.end_row();
                            }
                        });
                    ui.separator();
                    if ui.button("Close").clicked() {
                        self.show_help_dialog = false;
                    }
                });
            if !open {
                self.show_help_dialog = false;
            }
        }

        if self.show_about_dialog {
            let mut open = true;
            egui::Window::new("About PixelBuddy")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.heading("PixelBuddy");
                    ui.label("A pixel art editor built using Rust.");
                    ui.add_space(8.0);
                    ui.hyperlink_to("View on GitHub", "https://github.com/rowrow620/PixelBuddy");
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        self.show_about_dialog = false;
                    }
                });
            if !open {
                self.show_about_dialog = false;
            }
        }

        if let Some((w, h)) = self.pending_resize {
            if self.show_custom_resize_dialog {
                let mut open = true;
                egui::Window::new("Custom Canvas Size")
                    .open(&mut open)
                    .resizable(false)
                    .collapsible(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.label("Enter any dimensions within PixelBuddy's canvas limits.");
                        ui.label(
                            egui::RichText::new(format!(
                                "Maximum side: {} px · Maximum total: {} pixels",
                                crate::io::MAX_CANVAS_DIMENSION,
                                crate::io::MAX_CANVAS_PIXELS
                            ))
                            .small()
                            .color(egui::Color32::GRAY),
                        );
                        ui.add_space(6.0);

                        egui::Grid::new("custom_resize_dimensions")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("Width:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.resize_width)
                                        .desired_width(96.0),
                                );
                                ui.end_row();

                                ui.label("Height:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.resize_height)
                                        .desired_width(96.0),
                                );
                                ui.end_row();
                            });

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Continue").clicked() {
                                match (
                                    self.resize_width.trim().parse::<u32>(),
                                    self.resize_height.trim().parse::<u32>(),
                                ) {
                                    (Ok(width), Ok(height)) => {
                                        match Self::can_create_canvas(width, height) {
                                            Ok(()) => {
                                                self.pending_resize = Some((width, height));
                                                self.resize_error = None;
                                                self.show_custom_resize_dialog = false;
                                            }
                                            Err(error) => self.resize_error = Some(error),
                                        }
                                    }
                                    _ => {
                                        self.resize_error = Some(
                                            "Enter whole-number canvas dimensions.".to_owned(),
                                        );
                                    }
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                self.resize_error = None;
                                self.show_custom_resize_dialog = false;
                            }
                        });

                        if let Some(error) = &self.resize_error {
                            ui.colored_label(egui::Color32::from_rgb(248, 113, 113), error);
                        }
                    });
                if !open {
                    self.resize_error = None;
                    self.show_custom_resize_dialog = false;
                }
            }

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
                            .strong(),
                    );
                    ui.label(format!("Are you sure you want to resize to {w}x{h}?"));

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Resize").clicked() {
                            self.resize_canvas(w, h);
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
        eframe::set_value(
            storage,
            VIEW_PREFERENCES_STORAGE_KEY,
            &self.view_preferences(),
        );

        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.editor.is_dirty() {
                match crate::io::project::encode_editor(&self.editor) {
                    Ok(snapshot) if recovery_snapshot_within_budget(snapshot.len()) => {
                        // One key replacement is the atomic unit exposed by
                        // eframe's native persistence backend.
                        storage.set_string(RECOVERY_STORAGE_KEY, snapshot);
                    }
                    Ok(snapshot) => {
                        storage.set_string(RECOVERY_STORAGE_KEY, String::new());
                        let message = format!(
                            "Recovery snapshot is {} bytes, exceeding the {}-byte recovery limit",
                            snapshot.len(),
                            crate::io::project::MAX_RECOVERY_SNAPSHOT_BYTES
                        );
                        log::error!("{message}");
                        self.status_message = Some((message, true));
                    }
                    Err(error) => {
                        log::error!("Unable to create local project recovery snapshot: {error}")
                    }
                }
            } else {
                // An explicit project save or discard makes the previous native
                // recovery snapshot stale.
                storage.set_string(RECOVERY_STORAGE_KEY, String::new());
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // Recovery is intentionally native-only. Clear snapshots written
            // by older Web builds so they no longer consume browser storage.
            storage.set_string(RECOVERY_STORAGE_KEY, String::new());
        }
        storage.flush();
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(20)
    }
}

#[cfg(test)]
mod tests;
