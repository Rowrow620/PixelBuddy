//! Versioned persistence for editable PixelBuddy projects.
//!
//! PNG and GIF are intentionally flattened interchange formats. A
//! `.pbud` file preserves editable canvas layers, palettes, animation
//! frames, timing, and the small set of editor settings that affect the
//! project. The file is UTF-8 RON so it can also be stored by eframe's native
//! persistence or browser local storage without a platform-specific codec.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    document::{
        animation::{
            FrameTag, FrameTagValidationError, MAX_ANIMATION_FRAMES, MAX_ANIMATION_TAGS, MAX_FPS,
            MIN_FPS,
        },
        AnimationFrame, AnimationManager, Canvas, Document, Layer, Palette,
    },
    editor::{EditorState, ToolType},
};

/// Extension written for editable PixelBuddy projects.
pub const PROJECT_FILE_EXTENSION: &str = "pbud";

/// Extension used by the first unreleased project-file implementation.
///
/// Opening remains backward compatible so early local `.pixelbuddy` files do
/// not become inaccessible after the shorter `.pbud` name is adopted.
pub const LEGACY_PROJECT_FILE_EXTENSION: &str = "pixelbuddy";

/// The only project-file schema currently understood by PixelBuddy.
pub const PROJECT_FORMAT_VERSION: u32 = 1;

/// Maximum textual project size accepted before RON parsing begins.
///
/// This is distinct from the canvas limit: RON represents pixel bytes as
/// text, so accepting arbitrarily large files would allow a small-looking
/// import action to exhaust memory while parsing. The decoded layer data is
/// capped separately below.
pub const MAX_PROJECT_FILE_BYTES: usize = 256 * 1024 * 1024;

/// Maximum combined raw RGBA bytes held by all layers in a decoded project.
///
/// Individual canvases are already limited by `Canvas::MAX_PIXELS`; this
/// project-level cap prevents a valid-but-hostile file from multiplying that
/// allocation across thousands of layers or frames.
pub const MAX_PROJECT_CANVAS_BYTES: usize = 256 * 1024 * 1024;

/// Recovery uses browser/native key-value storage and therefore has a lower
/// limit than an explicitly saved project file.
pub const MAX_RECOVERY_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
/// Metadata collection limits prevent tiny canvases from multiplying model/UI
/// allocations through huge layer or palette vectors.
pub const MAX_LAYERS_PER_FRAME: usize = crate::document::MAX_LAYERS_PER_FRAME;
pub const MAX_PALETTE_COLORS: usize = crate::document::MAX_PALETTE_COLORS;
pub const MAX_LAYER_NAME_BYTES: usize = crate::document::MAX_LAYER_NAME_BYTES;

/// Errors that describe an invalid or unsupported editable project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectError {
    EmptyInput,
    NotUtf8,
    FileTooLarge {
        actual: usize,
        maximum: usize,
    },
    Encode {
        message: String,
    },
    Decode {
        message: String,
    },
    UnsupportedVersion {
        found: u32,
        supported: u32,
    },
    EmptyAnimation,
    InvalidCurrentFrameIndex {
        index: usize,
        frame_count: usize,
    },
    InvalidFps {
        fps: u32,
    },
    InvalidFrameDuration {
        frame_index: usize,
        duration_ms: u32,
    },
    MismatchedFrameDimensions {
        frame_index: usize,
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    InvalidDocumentDimensions {
        width: u32,
        height: u32,
    },
    EmptyLayers {
        frame_index: usize,
    },
    InvalidActiveLayerIndex {
        frame_index: usize,
        index: usize,
        layer_count: usize,
    },
    EmptyPalette {
        frame_index: usize,
    },
    InvalidPaletteIndex {
        frame_index: usize,
        index: usize,
        color_count: usize,
    },
    InvalidLayerCanvasDimensions {
        frame_index: usize,
        layer_index: usize,
        document_width: u32,
        document_height: u32,
        canvas_width: u32,
        canvas_height: u32,
    },
    InvalidCanvasBufferLength {
        frame_index: usize,
        layer_index: usize,
        width: u32,
        height: u32,
        actual: usize,
        expected: usize,
    },
    InvalidLayerOpacity {
        frame_index: usize,
        layer_index: usize,
    },
    InvalidOnionSkinOpacity,
    CanvasAllocationFailed {
        width: u32,
        height: u32,
    },
    ProjectCanvasLimitExceeded {
        maximum: usize,
    },
    ResourceLimitExceeded {
        resource: &'static str,
        actual: usize,
        maximum: usize,
    },
    InvalidMetadata {
        message: String,
    },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("The project file is empty"),
            Self::NotUtf8 => formatter.write_str("PixelBuddy project files must be valid UTF-8"),
            Self::FileTooLarge { actual, maximum } => write!(
                formatter,
                "The project file is {actual} bytes, exceeding the {maximum}-byte safety limit"
            ),
            Self::Encode { message } => write!(formatter, "Could not encode PixelBuddy project: {message}"),
            Self::Decode { message } => write!(formatter, "Could not decode PixelBuddy project: {message}"),
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "This project uses format version {found}; PixelBuddy supports version {supported}"
            ),
            Self::EmptyAnimation => formatter.write_str("A project must contain at least one frame"),
            Self::InvalidCurrentFrameIndex { index, frame_count } => write!(
                formatter,
                "Current frame index {index} is outside the {frame_count} saved frames"
            ),
            Self::InvalidFps { fps } => write!(
                formatter,
                "Animation FPS {fps} is outside PixelBuddy's {MIN_FPS}..={MAX_FPS} range"
            ),
            Self::InvalidFrameDuration {
                frame_index,
                duration_ms,
            } => write!(
                formatter,
                "Frame {} has invalid duration {duration_ms} ms",
                frame_index + 1
            ),
            Self::MismatchedFrameDimensions {
                frame_index,
                expected_width,
                expected_height,
                actual_width,
                actual_height,
            } => write!(
                formatter,
                "Frame {} is {}x{}, but the first frame is {}x{}",
                frame_index + 1,
                actual_width,
                actual_height,
                expected_width,
                expected_height
            ),
            Self::InvalidDocumentDimensions { width, height } => write!(
                formatter,
                "Document dimensions {width}x{height} are invalid or exceed PixelBuddy's canvas limit"
            ),
            Self::EmptyLayers { frame_index } => {
                write!(formatter, "Frame {} has no layers", frame_index + 1)
            }
            Self::InvalidActiveLayerIndex {
                frame_index,
                index,
                layer_count,
            } => write!(
                formatter,
                "Frame {} selects layer {index}, but it contains {layer_count} layers",
                frame_index + 1
            ),
            Self::EmptyPalette { frame_index } => {
                write!(formatter, "Frame {} has an empty palette", frame_index + 1)
            }
            Self::InvalidPaletteIndex {
                frame_index,
                index,
                color_count,
            } => write!(
                formatter,
                "Frame {} selects palette color {index}, but it contains {color_count} colors",
                frame_index + 1
            ),
            Self::InvalidLayerCanvasDimensions {
                frame_index,
                layer_index,
                document_width,
                document_height,
                canvas_width,
                canvas_height,
            } => write!(
                formatter,
                "Frame {}, layer {} is {}x{} but its document is {}x{}",
                frame_index + 1,
                layer_index + 1,
                canvas_width,
                canvas_height,
                document_width,
                document_height
            ),
            Self::InvalidCanvasBufferLength {
                frame_index,
                layer_index,
                width,
                height,
                actual,
                expected,
            } => write!(
                formatter,
                "Frame {}, layer {} has {actual} RGBA bytes for a {}x{} canvas; expected {expected}",
                frame_index + 1,
                layer_index + 1,
                width,
                height
            ),
            Self::InvalidLayerOpacity {
                frame_index,
                layer_index,
            } => write!(
                formatter,
                "Frame {}, layer {} has an invalid opacity",
                frame_index + 1,
                layer_index + 1
            ),
            Self::InvalidOnionSkinOpacity => {
                formatter.write_str("The project has an invalid onion-skin opacity")
            }
            Self::CanvasAllocationFailed { width, height } => {
                write!(formatter, "Could not allocate the {width}x{height} canvas in this project")
            }
            Self::ProjectCanvasLimitExceeded { maximum } => write!(
                formatter,
                "The project contains more than {maximum} bytes of editable canvas data"
            ),
            Self::ResourceLimitExceeded {
                resource,
                actual,
                maximum,
            } => write!(
                formatter,
                "The project contains {actual} {resource}, exceeding the limit of {maximum}"
            ),
            Self::InvalidMetadata { message } => {
                write!(formatter, "The project contains invalid metadata: {message}")
            }
        }
    }
}

impl std::error::Error for ProjectError {}

/// Serializes an editor into a versioned UTF-8 RON project string.
///
/// The snapshot includes editable artwork and project-scoped settings. Undo
/// history, selection, clipboard, file name, and playback clock are runtime
/// concerns and are intentionally not persisted.
pub fn encode_editor(editor: &EditorState) -> Result<String, ProjectError> {
    let file = ProjectFile::from_editor(editor)?;
    let encoded =
        ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::new()).map_err(|error| {
            ProjectError::Encode {
                message: error.to_string(),
            }
        })?;

    if encoded.len() > MAX_PROJECT_FILE_BYTES {
        return Err(ProjectError::FileTooLarge {
            actual: encoded.len(),
            maximum: MAX_PROJECT_FILE_BYTES,
        });
    }

    Ok(encoded)
}

/// Serializes an editor into bytes suitable for a native file-save API.
pub fn encode_editor_bytes(editor: &EditorState) -> Result<Vec<u8>, ProjectError> {
    Ok(encode_editor(editor)?.into_bytes())
}

/// Decodes a versioned UTF-8 RON project into a clean editor session.
pub fn decode_editor(input: &str) -> Result<EditorState, ProjectError> {
    if input.is_empty() {
        return Err(ProjectError::EmptyInput);
    }
    if input.len() > MAX_PROJECT_FILE_BYTES {
        return Err(ProjectError::FileTooLarge {
            actual: input.len(),
            maximum: MAX_PROJECT_FILE_BYTES,
        });
    }

    let file: ProjectFile = ron::de::from_str(input).map_err(|error| ProjectError::Decode {
        message: error.to_string(),
    })?;
    file.into_editor()
}

/// Decodes bytes from a native file-open API or eframe persistence storage.
pub fn decode_editor_bytes(input: &[u8]) -> Result<EditorState, ProjectError> {
    let input = std::str::from_utf8(input).map_err(|_| ProjectError::NotUtf8)?;
    decode_editor(input)
}

/// Serializes the editable animation alone using default editor preferences.
///
/// This is useful for callers that are not managing a full `EditorState` but
/// still need a safe, versioned project snapshot.
pub fn encode_animation(animation: &AnimationManager) -> Result<String, ProjectError> {
    let file = ProjectFile::from_animation(animation)?;
    let encoded =
        ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::new()).map_err(|error| {
            ProjectError::Encode {
                message: error.to_string(),
            }
        })?;

    if encoded.len() > MAX_PROJECT_FILE_BYTES {
        return Err(ProjectError::FileTooLarge {
            actual: encoded.len(),
            maximum: MAX_PROJECT_FILE_BYTES,
        });
    }

    Ok(encoded)
}

/// Decodes only the editable animation from a project file.
pub fn decode_animation(input: &str) -> Result<AnimationManager, ProjectError> {
    Ok(decode_project_file(input)?.animation)
}

fn decode_project_file(input: &str) -> Result<DecodedProject, ProjectError> {
    if input.is_empty() {
        return Err(ProjectError::EmptyInput);
    }
    if input.len() > MAX_PROJECT_FILE_BYTES {
        return Err(ProjectError::FileTooLarge {
            actual: input.len(),
            maximum: MAX_PROJECT_FILE_BYTES,
        });
    }

    let file: ProjectFile = ron::de::from_str(input).map_err(|error| ProjectError::Decode {
        message: error.to_string(),
    })?;
    file.into_decoded()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    format_version: u32,
    animation: StoredAnimation,
    editor: StoredEditorPreferences,
}

impl ProjectFile {
    fn from_editor(editor: &EditorState) -> Result<Self, ProjectError> {
        Ok(Self {
            format_version: PROJECT_FORMAT_VERSION,
            animation: StoredAnimation::from_runtime(&editor.animation)?,
            editor: StoredEditorPreferences {
                primary_color: editor.primary_color,
                secondary_color: editor.secondary_color,
                active_tool: editor.active_tool,
            },
        })
    }

    fn from_animation(animation: &AnimationManager) -> Result<Self, ProjectError> {
        Ok(Self {
            format_version: PROJECT_FORMAT_VERSION,
            animation: StoredAnimation::from_runtime(animation)?,
            editor: StoredEditorPreferences::default(),
        })
    }

    fn into_editor(self) -> Result<EditorState, ProjectError> {
        let decoded = self.into_decoded()?;
        let first_document = decoded.animation.current_doc();
        let mut editor = EditorState::new(first_document.width, first_document.height);
        editor.replace_project(decoded.animation, None);
        editor.primary_color = decoded.editor.primary_color;
        editor.secondary_color = decoded.editor.secondary_color;
        editor.active_tool = decoded.editor.active_tool;
        Ok(editor)
    }

    fn into_decoded(self) -> Result<DecodedProject, ProjectError> {
        if self.format_version != PROJECT_FORMAT_VERSION {
            return Err(ProjectError::UnsupportedVersion {
                found: self.format_version,
                supported: PROJECT_FORMAT_VERSION,
            });
        }

        Ok(DecodedProject {
            animation: self.animation.into_runtime()?,
            editor: self.editor,
        })
    }
}

struct DecodedProject {
    animation: AnimationManager,
    editor: StoredEditorPreferences,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEditorPreferences {
    primary_color: [u8; 4],
    secondary_color: [u8; 4],
    active_tool: ToolType,
}

impl Default for StoredEditorPreferences {
    fn default() -> Self {
        Self {
            primary_color: [0, 0, 0, 255],
            secondary_color: [255, 255, 255, 255],
            active_tool: ToolType::Pencil,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAnimation {
    frames: Vec<StoredAnimationFrame>,
    #[serde(default)]
    tags: Vec<crate::document::animation::FrameTag>,
    current_frame_index: usize,
    fps: u32,
    onion_skin_enabled: bool,
    onion_skin_opacity: f32,
}

impl StoredAnimation {
    fn from_runtime(animation: &AnimationManager) -> Result<Self, ProjectError> {
        validate_runtime_animation(animation)?;
        Ok(Self {
            frames: animation
                .frames
                .iter()
                .map(StoredAnimationFrame::from_runtime)
                .collect(),
            tags: animation.tags.clone(),
            // Playback uses `current_frame_index` as a transient preview
            // cursor. Persist the stable editing selection it started from so
            // merely previewing cannot make clean in-memory state diverge
            // from an asynchronous save snapshot.
            current_frame_index: animation.selected_frame_index(),
            fps: animation.fps,
            onion_skin_enabled: animation.onion_skin_enabled,
            onion_skin_opacity: animation.onion_skin_opacity,
        })
    }

    fn into_runtime(self) -> Result<AnimationManager, ProjectError> {
        validate_animation_metadata(&self.tags, self.frames.len())?;
        if self.frames.is_empty() {
            return Err(ProjectError::EmptyAnimation);
        }
        if self.current_frame_index >= self.frames.len() {
            return Err(ProjectError::InvalidCurrentFrameIndex {
                index: self.current_frame_index,
                frame_count: self.frames.len(),
            });
        }
        if !(MIN_FPS..=MAX_FPS).contains(&self.fps) {
            return Err(ProjectError::InvalidFps { fps: self.fps });
        }
        if !is_normalized_finite(self.onion_skin_opacity) {
            return Err(ProjectError::InvalidOnionSkinOpacity);
        }

        let mut total_canvas_bytes = 0usize;
        let mut frames = Vec::with_capacity(self.frames.len());
        let mut expected_dimensions = None;

        for (frame_index, frame) in self.frames.into_iter().enumerate() {
            if frame.duration_ms == 0 {
                return Err(ProjectError::InvalidFrameDuration {
                    frame_index,
                    duration_ms: frame.duration_ms,
                });
            }

            let document = frame
                .document
                .into_runtime(frame_index, &mut total_canvas_bytes)?;
            if let Some((expected_width, expected_height)) = expected_dimensions {
                if (document.width, document.height) != (expected_width, expected_height) {
                    return Err(ProjectError::MismatchedFrameDimensions {
                        frame_index,
                        expected_width,
                        expected_height,
                        actual_width: document.width,
                        actual_height: document.height,
                    });
                }
            } else {
                expected_dimensions = Some((document.width, document.height));
            }

            frames.push(AnimationFrame::with_duration(document, frame.duration_ms));
        }

        Ok(AnimationManager {
            frames,
            tags: self.tags,
            current_frame_index: self.current_frame_index,
            fps: self.fps,
            is_playing: false,
            last_frame_time: 0.0,
            playback_origin_frame_index: None,
            onion_skin_enabled: self.onion_skin_enabled,
            onion_skin_opacity: self.onion_skin_opacity,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredAnimationFrame {
    document: StoredDocument,
    duration_ms: u32,
}

impl StoredAnimationFrame {
    fn from_runtime(frame: &AnimationFrame) -> Self {
        Self {
            document: StoredDocument::from_runtime(&frame.document),
            duration_ms: frame.duration_ms,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredDocument {
    layers: Vec<StoredLayer>,
    active_layer_index: usize,
    palette: StoredPalette,
    width: u32,
    height: u32,
}

impl StoredDocument {
    fn from_runtime(document: &Document) -> Self {
        Self {
            layers: document
                .layers
                .iter()
                .map(StoredLayer::from_runtime)
                .collect(),
            active_layer_index: document.active_layer_index,
            palette: StoredPalette::from_runtime(&document.palette),
            width: document.width,
            height: document.height,
        }
    }

    fn into_runtime(
        self,
        frame_index: usize,
        total_canvas_bytes: &mut usize,
    ) -> Result<Document, ProjectError> {
        expected_rgba_byte_len(self.width, self.height)?;
        if self.layers.is_empty() {
            return Err(ProjectError::EmptyLayers { frame_index });
        }
        enforce_resource_limit(
            "layers in one frame",
            self.layers.len(),
            MAX_LAYERS_PER_FRAME,
        )?;
        if self.active_layer_index >= self.layers.len() {
            return Err(ProjectError::InvalidActiveLayerIndex {
                frame_index,
                index: self.active_layer_index,
                layer_count: self.layers.len(),
            });
        }

        let palette = self.palette.into_runtime(frame_index)?;
        let mut layers = Vec::with_capacity(self.layers.len());
        for (layer_index, layer) in self.layers.into_iter().enumerate() {
            layers.push(layer.into_runtime(
                frame_index,
                layer_index,
                self.width,
                self.height,
                total_canvas_bytes,
            )?);
        }

        Ok(Document {
            layers,
            active_layer_index: self.active_layer_index,
            palette,
            width: self.width,
            height: self.height,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredLayer {
    name: String,
    canvas: StoredCanvas,
    opacity: f32,
    blend_mode: crate::document::BlendMode,
    visible: bool,
    locked: bool,
}

impl StoredLayer {
    fn from_runtime(layer: &Layer) -> Self {
        Self {
            name: layer.name.clone(),
            canvas: StoredCanvas::from_runtime(&layer.canvas),
            opacity: layer.opacity,
            blend_mode: layer.blend_mode,
            visible: layer.visible,
            locked: layer.locked,
        }
    }

    fn into_runtime(
        self,
        frame_index: usize,
        layer_index: usize,
        document_width: u32,
        document_height: u32,
        total_canvas_bytes: &mut usize,
    ) -> Result<Layer, ProjectError> {
        validate_layer_name(&self.name, frame_index, layer_index)?;
        if !is_normalized_finite(self.opacity) {
            return Err(ProjectError::InvalidLayerOpacity {
                frame_index,
                layer_index,
            });
        }

        let canvas = self.canvas.into_runtime(
            frame_index,
            layer_index,
            document_width,
            document_height,
            total_canvas_bytes,
        )?;
        Ok(Layer {
            name: self.name,
            canvas,
            opacity: self.opacity,
            blend_mode: self.blend_mode,
            visible: self.visible,
            locked: self.locked,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCanvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl StoredCanvas {
    fn from_runtime(canvas: &Canvas) -> Self {
        Self {
            width: canvas.width(),
            height: canvas.height(),
            pixels: canvas.pixels().to_vec(),
        }
    }

    fn into_runtime(
        self,
        frame_index: usize,
        layer_index: usize,
        document_width: u32,
        document_height: u32,
        total_canvas_bytes: &mut usize,
    ) -> Result<Canvas, ProjectError> {
        if (self.width, self.height) != (document_width, document_height) {
            return Err(ProjectError::InvalidLayerCanvasDimensions {
                frame_index,
                layer_index,
                document_width,
                document_height,
                canvas_width: self.width,
                canvas_height: self.height,
            });
        }

        let expected = expected_rgba_byte_len(self.width, self.height)?;
        if self.pixels.len() != expected {
            return Err(ProjectError::InvalidCanvasBufferLength {
                frame_index,
                layer_index,
                width: self.width,
                height: self.height,
                actual: self.pixels.len(),
                expected,
            });
        }

        *total_canvas_bytes = total_canvas_bytes
            .checked_add(expected)
            .filter(|total| *total <= MAX_PROJECT_CANVAS_BYTES)
            .ok_or(ProjectError::ProjectCanvasLimitExceeded {
                maximum: MAX_PROJECT_CANVAS_BYTES,
            })?;

        let mut canvas = Canvas::try_new(self.width, self.height).map_err(|_| {
            ProjectError::CanvasAllocationFailed {
                width: self.width,
                height: self.height,
            }
        })?;
        canvas.pixels_mut().copy_from_slice(&self.pixels);
        Ok(canvas)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPalette {
    colors: Vec<[u8; 4]>,
    selected_index: usize,
}

impl StoredPalette {
    fn from_runtime(palette: &Palette) -> Self {
        Self {
            colors: palette.colors.clone(),
            selected_index: palette.selected_index,
        }
    }

    fn into_runtime(self, frame_index: usize) -> Result<Palette, ProjectError> {
        if self.colors.is_empty() {
            return Err(ProjectError::EmptyPalette { frame_index });
        }
        enforce_resource_limit("palette colors", self.colors.len(), MAX_PALETTE_COLORS)?;
        if self.selected_index >= self.colors.len() {
            return Err(ProjectError::InvalidPaletteIndex {
                frame_index,
                index: self.selected_index,
                color_count: self.colors.len(),
            });
        }

        Ok(Palette {
            colors: self.colors,
            selected_index: self.selected_index,
        })
    }
}

fn enforce_resource_limit(
    resource: &'static str,
    actual: usize,
    maximum: usize,
) -> Result<(), ProjectError> {
    if actual > maximum {
        return Err(ProjectError::ResourceLimitExceeded {
            resource,
            actual,
            maximum,
        });
    }
    Ok(())
}

fn validate_animation_metadata(tags: &[FrameTag], frame_count: usize) -> Result<(), ProjectError> {
    enforce_resource_limit("animation frames", frame_count, MAX_ANIMATION_FRAMES)?;
    enforce_resource_limit("animation tags", tags.len(), MAX_ANIMATION_TAGS)?;
    for (index, tag) in tags.iter().enumerate() {
        if let Err(error) = tag.validate(frame_count) {
            let reason = match error {
                FrameTagValidationError::EmptyName => "has an empty name",
                FrameTagValidationError::NameTooLong => "has a name that exceeds the text limit",
                FrameTagValidationError::ControlCharacter => {
                    "has a name containing control characters"
                }
                FrameTagValidationError::InvalidColor => "has an invalid color",
                FrameTagValidationError::InvalidRange => "targets an invalid frame range",
            };
            return Err(ProjectError::InvalidMetadata {
                message: format!("animation tag {} {reason}", index + 1),
            });
        }
    }
    Ok(())
}

fn validate_layer_name(
    name: &str,
    frame_index: usize,
    layer_index: usize,
) -> Result<(), ProjectError> {
    if !crate::document::valid_layer_name(name) {
        return Err(ProjectError::InvalidMetadata {
            message: format!(
                "frame {}, layer {} has an invalid or overlong name",
                frame_index + 1,
                layer_index + 1
            ),
        });
    }
    Ok(())
}
fn validate_runtime_animation(animation: &AnimationManager) -> Result<(), ProjectError> {
    validate_animation_metadata(&animation.tags, animation.frames.len())?;
    if animation.frames.is_empty() {
        return Err(ProjectError::EmptyAnimation);
    }
    let selected_frame_index = animation.selected_frame_index();
    if selected_frame_index >= animation.frames.len() {
        return Err(ProjectError::InvalidCurrentFrameIndex {
            index: selected_frame_index,
            frame_count: animation.frames.len(),
        });
    }
    if !(MIN_FPS..=MAX_FPS).contains(&animation.fps) {
        return Err(ProjectError::InvalidFps { fps: animation.fps });
    }
    if !is_normalized_finite(animation.onion_skin_opacity) {
        return Err(ProjectError::InvalidOnionSkinOpacity);
    }

    let mut total_canvas_bytes = 0usize;
    let mut expected_dimensions = None;
    for (frame_index, frame) in animation.frames.iter().enumerate() {
        if frame.duration_ms == 0 {
            return Err(ProjectError::InvalidFrameDuration {
                frame_index,
                duration_ms: frame.duration_ms,
            });
        }
        validate_runtime_document(&frame.document, frame_index, &mut total_canvas_bytes)?;

        if let Some((expected_width, expected_height)) = expected_dimensions {
            if (frame.document.width, frame.document.height) != (expected_width, expected_height) {
                return Err(ProjectError::MismatchedFrameDimensions {
                    frame_index,
                    expected_width,
                    expected_height,
                    actual_width: frame.document.width,
                    actual_height: frame.document.height,
                });
            }
        } else {
            expected_dimensions = Some((frame.document.width, frame.document.height));
        }
    }

    Ok(())
}

fn validate_runtime_document(
    document: &Document,
    frame_index: usize,
    total_canvas_bytes: &mut usize,
) -> Result<(), ProjectError> {
    expected_rgba_byte_len(document.width, document.height)?;
    if document.layers.is_empty() {
        return Err(ProjectError::EmptyLayers { frame_index });
    }
    enforce_resource_limit(
        "layers in one frame",
        document.layers.len(),
        MAX_LAYERS_PER_FRAME,
    )?;
    if document.active_layer_index >= document.layers.len() {
        return Err(ProjectError::InvalidActiveLayerIndex {
            frame_index,
            index: document.active_layer_index,
            layer_count: document.layers.len(),
        });
    }
    if document.palette.colors.is_empty() {
        return Err(ProjectError::EmptyPalette { frame_index });
    }
    enforce_resource_limit(
        "palette colors",
        document.palette.colors.len(),
        MAX_PALETTE_COLORS,
    )?;
    if document.palette.selected_index >= document.palette.colors.len() {
        return Err(ProjectError::InvalidPaletteIndex {
            frame_index,
            index: document.palette.selected_index,
            color_count: document.palette.colors.len(),
        });
    }

    for (layer_index, layer) in document.layers.iter().enumerate() {
        validate_layer_name(&layer.name, frame_index, layer_index)?;
        if !is_normalized_finite(layer.opacity) {
            return Err(ProjectError::InvalidLayerOpacity {
                frame_index,
                layer_index,
            });
        }
        if (layer.canvas.width(), layer.canvas.height()) != (document.width, document.height) {
            return Err(ProjectError::InvalidLayerCanvasDimensions {
                frame_index,
                layer_index,
                document_width: document.width,
                document_height: document.height,
                canvas_width: layer.canvas.width(),
                canvas_height: layer.canvas.height(),
            });
        }

        let expected = expected_rgba_byte_len(document.width, document.height)?;
        if layer.canvas.pixels().len() != expected {
            return Err(ProjectError::InvalidCanvasBufferLength {
                frame_index,
                layer_index,
                width: document.width,
                height: document.height,
                actual: layer.canvas.pixels().len(),
                expected,
            });
        }
        *total_canvas_bytes = total_canvas_bytes
            .checked_add(expected)
            .filter(|total| *total <= MAX_PROJECT_CANVAS_BYTES)
            .ok_or(ProjectError::ProjectCanvasLimitExceeded {
                maximum: MAX_PROJECT_CANVAS_BYTES,
            })?;
    }

    Ok(())
}

fn expected_rgba_byte_len(width: u32, height: u32) -> Result<usize, ProjectError> {
    if width == 0
        || height == 0
        || width > crate::document::canvas::MAX_DIMENSION
        || height > crate::document::canvas::MAX_DIMENSION
    {
        return Err(ProjectError::InvalidDocumentDimensions { width, height });
    }

    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(ProjectError::InvalidDocumentDimensions { width, height })?;
    if pixels > crate::document::canvas::MAX_PIXELS as u64 {
        return Err(ProjectError::InvalidDocumentDimensions { width, height });
    }

    usize::try_from(
        pixels
            .checked_mul(4)
            .ok_or(ProjectError::InvalidDocumentDimensions { width, height })?,
    )
    .map_err(|_| ProjectError::InvalidDocumentDimensions { width, height })
}

fn is_normalized_finite(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_editor, decode_editor_bytes, encode_editor, ProjectError, ProjectFile,
        StoredAnimation, StoredAnimationFrame, StoredCanvas, StoredDocument,
        StoredEditorPreferences, StoredLayer, StoredPalette, PROJECT_FORMAT_VERSION,
    };
    use crate::{
        document::AnimationFrame,
        editor::{EditorState, ToolType},
    };

    #[test]
    fn project_round_trip_preserves_editable_art_animation_and_settings() {
        let mut editor = EditorState::new(2, 2);
        {
            let document = editor.document_mut();
            document
                .active_layer_mut()
                .canvas
                .set_pixel(0, 0, [10, 20, 30, 255]);
            document.add_layer();
            let layer = document.active_layer_mut();
            layer.name = "Highlights".to_owned();
            layer.opacity = 0.5;
            layer.visible = false;
            layer.locked = true;
            layer.canvas.set_pixel(1, 1, [40, 50, 60, 128]);
            document.palette.add_color([1, 2, 3, 255]);
        }
        editor.animation.frames[0].duration_ms = 120;
        let mut second_document = editor.document().clone();
        second_document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [70, 80, 90, 255]);
        editor
            .animation
            .frames
            .push(AnimationFrame::with_duration(second_document, 240));
        editor.animation.current_frame_index = 1;
        editor.animation.fps = 12;
        editor.animation.onion_skin_enabled = true;
        editor.animation.onion_skin_opacity = 0.6;
        editor.primary_color = [4, 5, 6, 255];
        editor.secondary_color = [7, 8, 9, 255];
        editor.active_tool = ToolType::Ellipse;
        editor.project_name = Some("unsaved-name.pbud".to_owned());

        let encoded = encode_editor(&editor).expect("project should encode");
        let loaded = decode_editor(&encoded).expect("project should decode");

        assert!(!loaded.is_dirty());
        assert_eq!(loaded.project_name, None);
        assert_eq!(loaded.primary_color, [4, 5, 6, 255]);
        assert_eq!(loaded.secondary_color, [7, 8, 9, 255]);
        assert_eq!(loaded.active_tool, ToolType::Ellipse);
        assert_eq!(loaded.animation.frames.len(), 2);
        assert_eq!(loaded.animation.current_frame_index, 1);
        assert_eq!(loaded.animation.fps, 12);
        assert!(loaded.animation.onion_skin_enabled);
        assert_eq!(loaded.animation.onion_skin_opacity, 0.6);
        assert_eq!(loaded.animation.frames[0].duration_ms, 120);
        assert_eq!(loaded.animation.frames[1].duration_ms, 240);

        let first = &loaded.animation.frames[0].document;
        assert_eq!(first.layers.len(), 2);
        assert_eq!(first.layers[1].name, "Highlights");
        assert_eq!(first.layers[1].opacity, 0.5);
        assert!(!first.layers[1].visible);
        assert!(first.layers[1].locked);
        assert_eq!(first.palette.selected_color(), [1, 2, 3, 255]);
        assert_eq!(first.layers[0].canvas.get_pixel(0, 0), [10, 20, 30, 255]);
        assert_eq!(
            loaded.animation.frames[1]
                .document
                .active_layer()
                .canvas
                .get_pixel(1, 0),
            [70, 80, 90, 255]
        );
    }

    #[test]
    fn invalid_project_syntax_and_unsupported_versions_are_rejected() {
        assert!(matches!(
            decode_editor("this is not RON"),
            Err(ProjectError::Decode { .. })
        ));

        let invalid_version = r#"(
            format_version: 99,
            animation: (
                frames: [],
                current_frame_index: 0,
                fps: 8,
                onion_skin_enabled: false,
                onion_skin_opacity: 0.35,
            ),
            editor: (
                primary_color: (0, 0, 0, 255),
                secondary_color: (255, 255, 255, 255),
                active_tool: Pencil,
            ),
        )"#;
        assert!(matches!(
            decode_editor(invalid_version),
            Err(ProjectError::UnsupportedVersion {
                found: 99,
                supported: PROJECT_FORMAT_VERSION,
            })
        ));

        assert!(matches!(
            decode_editor_bytes(&[0xff]),
            Err(ProjectError::NotUtf8)
        ));
    }

    #[test]
    fn malformed_canvas_buffer_is_rejected_before_it_can_reach_the_editor() {
        let file = ProjectFile {
            format_version: PROJECT_FORMAT_VERSION,
            animation: StoredAnimation {
                frames: vec![StoredAnimationFrame {
                    duration_ms: 100,
                    document: StoredDocument {
                        width: 1,
                        height: 1,
                        active_layer_index: 0,
                        palette: StoredPalette {
                            colors: vec![[0, 0, 0, 255]],
                            selected_index: 0,
                        },
                        layers: vec![StoredLayer {
                            name: "Layer 1".to_owned(),
                            canvas: StoredCanvas {
                                width: 1,
                                height: 1,
                                pixels: vec![0, 0, 0],
                            },
                            opacity: 1.0,
                            blend_mode: crate::document::BlendMode::Normal,
                            visible: true,
                            locked: false,
                        }],
                    },
                }],
                tags: Vec::new(),
                current_frame_index: 0,
                fps: 8,
                onion_skin_enabled: false,
                onion_skin_opacity: 0.35,
            },
            editor: StoredEditorPreferences::default(),
        };
        let encoded = ron::ser::to_string(&file).expect("test project should encode");

        assert!(matches!(
            decode_editor(&encoded),
            Err(ProjectError::InvalidCanvasBufferLength {
                frame_index: 0,
                layer_index: 0,
                actual: 3,
                expected: 4,
                ..
            })
        ));
    }

    #[test]
    fn zero_duration_frame_is_rejected_instead_of_silently_changing_timing() {
        let mut editor = EditorState::new(1, 1);
        editor.animation.frames[0].duration_ms = 0;

        assert_eq!(
            encode_editor(&editor),
            Err(ProjectError::InvalidFrameDuration {
                frame_index: 0,
                duration_ms: 0,
            })
        );
    }

    #[test]
    fn project_metadata_limits_reject_excessive_frames_layers_palettes_and_tags() {
        let mut frames = EditorState::new(1, 1);
        let frame = frames.animation.frames[0].clone();
        frames.animation.frames = vec![frame; crate::document::animation::MAX_ANIMATION_FRAMES + 1];
        assert!(matches!(
            encode_editor(&frames),
            Err(ProjectError::ResourceLimitExceeded {
                resource: "animation frames",
                ..
            })
        ));

        let mut layers = EditorState::new(1, 1);
        while layers.document().layers.len() <= super::MAX_LAYERS_PER_FRAME {
            layers.document_mut().add_layer();
        }
        assert!(matches!(
            encode_editor(&layers),
            Err(ProjectError::ResourceLimitExceeded {
                resource: "layers in one frame",
                ..
            })
        ));

        let mut palette = EditorState::new(1, 1);
        while palette.document().palette.colors.len() <= super::MAX_PALETTE_COLORS {
            palette.document_mut().palette.add_color([1, 2, 3, 255]);
        }
        assert!(matches!(
            encode_editor(&palette),
            Err(ProjectError::ResourceLimitExceeded {
                resource: "palette colors",
                ..
            })
        ));

        let mut tags = EditorState::new(1, 1);
        tags.animation.tags = (0..=crate::document::animation::MAX_ANIMATION_TAGS)
            .map(|index| crate::document::animation::FrameTag {
                name: format!("Tag {index}"),
                color: [0.5, 0.5, 0.5],
                from_frame: 0,
                to_frame: 0,
            })
            .collect();
        assert!(matches!(
            encode_editor(&tags),
            Err(ProjectError::ResourceLimitExceeded {
                resource: "animation tags",
                ..
            })
        ));
    }

    #[test]
    fn invalid_tag_text_is_rejected_on_encode_and_decode() {
        let mut editor = EditorState::new(1, 1);
        editor
            .animation
            .tags
            .push(crate::document::animation::FrameTag {
                name: "bad\nname".to_owned(),
                color: [0.5, 0.5, 0.5],
                from_frame: 0,
                to_frame: 0,
            });
        assert!(matches!(
            encode_editor(&editor),
            Err(ProjectError::InvalidMetadata { message })
                if message.contains("control characters")
        ));

        editor.animation.tags[0].name =
            "x".repeat(crate::document::animation::MAX_TAG_NAME_BYTES + 1);
        let mut file = ProjectFile::from_editor(&EditorState::new(1, 1))
            .expect("the base project should be valid");
        file.animation.tags = editor.animation.tags;
        assert!(matches!(
            file.into_editor(),
            Err(ProjectError::InvalidMetadata { message })
                if message.contains("text limit")
        ));
    }

    #[test]
    fn hostile_project_fixtures_are_rejected_at_the_expected_boundary() {
        let corrupt = include_str!("../../tests/fixtures/corrupt_project.pbud");
        assert!(matches!(
            decode_editor(corrupt),
            Err(ProjectError::Decode { .. })
        ));

        let oversized = include_str!("../../tests/fixtures/oversized_metadata.pbud");
        let error = match decode_editor(oversized) {
            Ok(_) => panic!("oversized metadata must be rejected"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error,
                ProjectError::InvalidMetadata { ref message }
                    if message.contains("overlong name")
            ),
            "unexpected oversized-metadata error: {error:?}"
        );
    }

    #[test]
    fn default_editor_project_is_valid() {
        let editor = EditorState::new(1, 1);
        let encoded = encode_editor(&editor).expect("new editor should encode");
        let loaded = decode_editor(&encoded).expect("new editor should decode");

        assert_eq!(loaded.document().width, 1);
        assert_eq!(loaded.document().height, 1);
        assert_eq!(loaded.active_tool, ToolType::Pencil);
    }
}
