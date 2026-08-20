//! File-dialog orchestration and shared import/export types.
//!
//! Encoding happens in the format modules. This module deliberately keeps the
//! bytes together with their intended output format so a GIF can never be sent
//! through a PNG save dialog by accident.

pub mod gif;
pub mod png;
pub mod project;
pub mod spritesheet;
pub mod webp;

use std::fmt;

use crossbeam_channel::{unbounded, Receiver, Sender};
use rfd::AsyncFileDialog;

/// Largest edge length accepted for imported and generated raster documents.
///
/// The limit is intentionally shared by import and export code. It prevents a
/// malformed PNG header or an enormous sprite sheet from causing an
/// unexpectedly large allocation before the document model gets a chance to
/// validate it.
pub const MAX_CANVAS_DIMENSION: u32 = crate::document::canvas::MAX_DIMENSION;

/// Largest number of pixels accepted for one raster document or export.
pub const MAX_CANVAS_PIXELS: u64 = crate::document::canvas::MAX_PIXELS as u64;

/// The file type selected by an export request.
///
/// `SpriteSheetPng` remains distinct from `Png` even though both contain PNG
/// bytes: they have different user-facing labels and default names, and the
/// distinction is useful for completion/error feedback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Project,
    Png,
    Gif,
    SpriteSheetPng,
    WebP,
}

impl ExportFormat {
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Project => "PixelBuddy project",
            Self::Png => "PNG image",
            Self::Gif => "animated GIF",
            Self::SpriteSheetPng => "PNG sprite sheet",
            Self::WebP => "WebP image",
        }
    }

    pub const fn dialog_filter_name(self) -> &'static str {
        match self {
            Self::Project => "PixelBuddy Project",
            Self::Png => "PNG Image",
            Self::Gif => "GIF Animation",
            Self::SpriteSheetPng => "PNG Sprite Sheet",
            Self::WebP => "WebP Image",
        }
    }

    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Project => &[project::PROJECT_FILE_EXTENSION],
            Self::Png | Self::SpriteSheetPng => &["png"],
            Self::Gif => &["gif"],
            Self::WebP => &["webp"],
        }
    }

    pub const fn default_file_name(self) -> &'static str {
        match self {
            Self::Project => "untitled.pbud",
            Self::Png => "pixelbuddy-export.png",
            Self::Gif => "pixelbuddy-animation.gif",
            Self::SpriteSheetPng => "pixelbuddy-sprite-sheet.png",
            Self::WebP => "pixelbuddy-export.webp",
        }
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.display_name())
    }
}

/// Encoded export data paired with the format that must be used to save it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportRequest {
    pub format: ExportFormat,
    pub bytes: Vec<u8>,
    suggested_file_name: Option<String>,
    /// Revision of the project snapshot being written, when this is an
    /// editable-project save. Raster exports leave it unset.
    source_revision: Option<u64>,
}

impl ExportRequest {
    pub fn new(format: ExportFormat, bytes: Vec<u8>) -> Self {
        Self {
            format,
            bytes,
            suggested_file_name: None,
            source_revision: None,
        }
    }

    pub fn png(bytes: Vec<u8>) -> Self {
        Self::new(ExportFormat::Png, bytes)
    }

    pub fn project(bytes: Vec<u8>) -> Self {
        Self::new(ExportFormat::Project, bytes)
    }

    pub fn gif(bytes: Vec<u8>) -> Self {
        Self::new(ExportFormat::Gif, bytes)
    }

    pub fn sprite_sheet_png(bytes: Vec<u8>) -> Self {
        Self::new(ExportFormat::SpriteSheetPng, bytes)
    }

    pub fn webp(bytes: Vec<u8>) -> Self {
        Self::new(ExportFormat::WebP, bytes)
    }

    /// Overrides the default filename shown by the native/browser save dialog.
    pub fn with_suggested_file_name(mut self, file_name: impl Into<String>) -> Self {
        self.suggested_file_name = Some(file_name.into());
        self
    }

    /// Associates an editable-project save with the editor revision that was
    /// encoded. The completion event can then avoid marking later edits saved.
    pub fn with_source_revision(mut self, source_revision: u64) -> Self {
        self.source_revision = Some(source_revision);
        self
    }

    pub fn suggested_file_name(&self) -> &str {
        self.suggested_file_name
            .as_deref()
            .unwrap_or_else(|| self.format.default_file_name())
    }

    pub const fn source_revision(&self) -> Option<u64> {
        self.source_revision
    }
}

/// Errors reported by the image encoders, importer, or asynchronous file save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IoError {
    Decode {
        format: &'static str,
        message: String,
    },
    Encode {
        format: ExportFormat,
        message: String,
    },
    InvalidCanvasDimensions {
        width: u32,
        height: u32,
    },
    /// Raster exports only support positive integer nearest-neighbor scales.
    InvalidExportScale {
        scale: u32,
    },
    InvalidRgbaBufferLength {
        width: u32,
        height: u32,
        actual: usize,
        expected: usize,
    },
    EmptyAnimation,
    MismatchedFrameDimensions {
        frame_index: usize,
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    DimensionOverflow {
        operation: &'static str,
    },
    FileWrite {
        format: ExportFormat,
        file_name: String,
        message: String,
    },
}

impl fmt::Display for IoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode { format, message } => {
                write!(formatter, "Could not decode {format}: {message}")
            }
            Self::Encode { format, message } => {
                write!(formatter, "Could not encode {format}: {message}")
            }
            Self::InvalidCanvasDimensions { width, height } => write!(
                formatter,
                "Image dimensions {width}x{height} are invalid or exceed PixelBuddy's {MAX_CANVAS_DIMENSION}x{MAX_CANVAS_DIMENSION} / {MAX_CANVAS_PIXELS}-pixel limit",
            ),
            Self::InvalidExportScale { scale } => {
                write!(formatter, "Export scale must be at least 1 (got {scale})")
            }
            Self::InvalidRgbaBufferLength {
                width,
                height,
                actual,
                expected,
            } => write!(
                formatter,
                "RGBA data for {width}x{height} image has {actual} bytes; expected {expected}",
            ),
            Self::EmptyAnimation => formatter.write_str("Cannot export an animation with no frames"),
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
                expected_height,
            ),
            Self::DimensionOverflow { operation } => {
                write!(formatter, "Image dimensions overflow while {operation}")
            }
            Self::FileWrite {
                format,
                file_name,
                message,
            } => write!(formatter, "Could not save {format} to {file_name}: {message}"),
        }
    }
}

impl std::error::Error for IoError {}

/// Lets project-file validation errors flow through the existing asynchronous
/// I/O status path without discarding their user-facing detail.
impl From<project::ProjectError> for IoError {
    fn from(error: project::ProjectError) -> Self {
        Self::Decode {
            format: "PixelBuddy project",
            message: error.to_string(),
        }
    }
}

/// Validates dimensions before allocating an RGBA canvas.
pub fn validate_canvas_dimensions(width: u32, height: u32) -> Result<(), IoError> {
    let pixel_count = u64::from(width).checked_mul(u64::from(height));
    let exceeds_pixel_limit = match pixel_count {
        Some(count) => count > MAX_CANVAS_PIXELS,
        None => true,
    };
    if width == 0
        || height == 0
        || width > MAX_CANVAS_DIMENSION
        || height > MAX_CANVAS_DIMENSION
        || exceeds_pixel_limit
    {
        return Err(IoError::InvalidCanvasDimensions { width, height });
    }

    Ok(())
}

/// Computes a checked RGBA byte length after applying the image-size limits.
pub(crate) fn rgba_byte_len(width: u32, height: u32) -> Result<usize, IoError> {
    validate_canvas_dimensions(width, height)?;

    let byte_count = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(IoError::DimensionOverflow {
            operation: "calculating RGBA buffer length",
        })?;

    usize::try_from(byte_count).map_err(|_| IoError::DimensionOverflow {
        operation: "converting RGBA buffer length for this platform",
    })
}

/// Ensures a raster export uses a positive integer nearest-neighbor scale.
pub(crate) fn validate_export_scale(scale: u32) -> Result<(), IoError> {
    if scale == 0 {
        return Err(IoError::InvalidExportScale { scale });
    }

    Ok(())
}

/// Returns the output dimensions for a positive integer raster-export scale.
///
/// This validates both the scaled dimensions and their RGBA byte length before
/// an encoder allocates an output image. Keeping the check here makes PNG,
/// GIF, and sprite-sheet exports enforce the same practical size limit.
pub(crate) fn scaled_canvas_dimensions(
    width: u32,
    height: u32,
    scale: u32,
) -> Result<(u32, u32), IoError> {
    validate_export_scale(scale)?;

    let scaled_width = width.checked_mul(scale).ok_or(IoError::DimensionOverflow {
        operation: "scaling export width",
    })?;
    let scaled_height = height
        .checked_mul(scale)
        .ok_or(IoError::DimensionOverflow {
            operation: "scaling export height",
        })?;

    // `rgba_byte_len` also checks the dimension/pixel limits and ensures the
    // byte length is representable on the current platform.
    rgba_byte_len(scaled_width, scaled_height)?;
    Ok((scaled_width, scaled_height))
}

/// Resizes RGBA pixels to exact output dimensions using nearest-neighbor
/// sampling. The horizontal and vertical ratios do not need to be equal or
/// whole numbers.
pub(crate) fn resize_rgba_nearest_neighbor(
    source_pixels: &[u8],
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> Result<(u32, u32, Vec<u8>), IoError> {
    let source_len = rgba_byte_len(source_width, source_height)?;
    if source_pixels.len() != source_len {
        return Err(IoError::InvalidRgbaBufferLength {
            width: source_width,
            height: source_height,
            actual: source_pixels.len(),
            expected: source_len,
        });
    }

    let target_len = rgba_byte_len(target_width, target_height)?;

    let source_width = usize::try_from(source_width).map_err(|_| IoError::DimensionOverflow {
        operation: "converting source width for nearest-neighbor scaling",
    })?;
    let source_height = usize::try_from(source_height).map_err(|_| IoError::DimensionOverflow {
        operation: "converting source height for nearest-neighbor scaling",
    })?;
    let target_width = usize::try_from(target_width).map_err(|_| IoError::DimensionOverflow {
        operation: "converting target width for nearest-neighbor resizing",
    })?;
    let target_height = usize::try_from(target_height).map_err(|_| IoError::DimensionOverflow {
        operation: "converting target height for nearest-neighbor resizing",
    })?;

    let source_row_len = source_width * 4;
    let target_row_len = target_width * 4;
    let mut target_pixels = vec![0; target_len];

    for target_y in 0..target_height {
        let source_y = target_y * source_height / target_height;
        let source_row_start = source_y * source_row_len;
        let target_row_start = target_y * target_row_len;

        for target_x in 0..target_width {
            let source_x = target_x * source_width / target_width;
            let source_start = source_row_start + source_x * 4;
            let target_start = target_row_start + target_x * 4;
            target_pixels[target_start..target_start + 4]
                .copy_from_slice(&source_pixels[source_start..source_start + 4]);
        }
    }

    Ok((
        u32::try_from(target_width).map_err(|_| IoError::DimensionOverflow {
            operation: "converting target width after nearest-neighbor resizing",
        })?,
        u32::try_from(target_height).map_err(|_| IoError::DimensionOverflow {
            operation: "converting target height after nearest-neighbor resizing",
        })?,
        target_pixels,
    ))
}

/// Ensures every animation frame can be combined into one GIF or sprite sheet.
pub(crate) fn validate_animation_frames(
    animation: &crate::document::AnimationManager,
) -> Result<(u32, u32), IoError> {
    let first_frame = animation.frames.first().ok_or(IoError::EmptyAnimation)?;
    let expected_width = first_frame.document.width;
    let expected_height = first_frame.document.height;
    validate_canvas_dimensions(expected_width, expected_height)?;

    for (frame_index, frame) in animation.frames.iter().enumerate() {
        let actual_width = frame.document.width;
        let actual_height = frame.document.height;
        if (actual_width, actual_height) != (expected_width, expected_height) {
            return Err(IoError::MismatchedFrameDimensions {
                frame_index,
                expected_width,
                expected_height,
                actual_width,
                actual_height,
            });
        }
        validate_canvas_dimensions(actual_width, actual_height)?;
    }

    Ok((expected_width, expected_height))
}

#[derive(Debug)]
pub enum FileAction {
    OpenedImage {
        data: Vec<u8>,
        file_name: String,
    },
    OpenedSpriteSheet {
        data: Vec<u8>,
        file_name: String,
    },
    /// Raw UTF-8 project bytes selected by the user. The app decodes these
    /// only after confirming that replacing dirty work is intentional.
    OpenedProject {
        data: Vec<u8>,
        file_name: String,
    },
    Exported {
        format: ExportFormat,
        file_name: String,
        source_revision: Option<u64>,
    },
    Failed(IoError),
}

pub struct IoHandler {
    pub sender: Sender<FileAction>,
    pub receiver: Receiver<FileAction>,
}

impl Default for IoHandler {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }
}

impl IoHandler {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn trigger_open_file(sender: Sender<FileAction>) {
    let task = async move {
        if let Some(file) = AsyncFileDialog::new()
            .add_filter("Images", &["png", "webp"])
            .pick_file()
            .await
        {
            let file_name = file.file_name();
            let data = file.read().await;
            let _ = sender.send(FileAction::OpenedImage { data, file_name });
        }
    };

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(task);

    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        pollster::block_on(task);
    });
}

pub fn trigger_open_spritesheet(sender: Sender<FileAction>) {
    let task = async move {
        if let Some(file) = AsyncFileDialog::new()
            .add_filter("Images", &["png", "webp"])
            .pick_file()
            .await
        {
            let file_name = file.file_name();
            let data = file.read().await;
            let _ = sender.send(FileAction::OpenedSpriteSheet { data, file_name });
        }
    };

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(task);

    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        pollster::block_on(task);
    });
}

/// Encodes the current editor state as a PixelBuddy project and prompts the
/// user to save it.
pub fn trigger_save_project(editor: &crate::editor::EditorState, sender: Sender<FileAction>) {
    if let Ok(project_string) = project::encode_editor(editor) {
        let request = ExportRequest::project(project_string.into_bytes())
            .with_source_revision(editor.revision());
        trigger_export(request, sender);
    }
}

/// Opens a versioned, editable PixelBuddy project file.
///
/// Project decoding is deliberately deferred to the app event loop: an open
/// request may need to wait for the user to confirm discarding unsaved work.
pub fn trigger_open_project(sender: Sender<FileAction>) {
    let task = async move {
        if let Some(file) = AsyncFileDialog::new()
            .add_filter(
                "PixelBuddy Project",
                &[
                    project::PROJECT_FILE_EXTENSION,
                    project::LEGACY_PROJECT_FILE_EXTENSION,
                ],
            )
            .pick_file()
            .await
        {
            let file_name = file.file_name();
            let data = file.read().await;
            let _ = sender.send(FileAction::OpenedProject { data, file_name });
        }
    };

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(task);

    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        pollster::block_on(task);
    });
}

/// Opens a format-aware save dialog and writes the encoded export bytes.
pub fn trigger_export(request: ExportRequest, sender: Sender<FileAction>) {
    let task = async move {
        let format = request.format;
        let suggested_file_name = request.suggested_file_name().to_owned();
        let source_revision = request.source_revision();
        let dialog = AsyncFileDialog::new()
            .add_filter(format.dialog_filter_name(), format.extensions())
            .set_file_name(&suggested_file_name);

        if let Some(file) = dialog.save_file().await {
            let file_name = file.file_name();
            match file.write(&request.bytes).await {
                Ok(()) => {
                    let _ = sender.send(FileAction::Exported {
                        format,
                        file_name,
                        source_revision,
                    });
                }
                Err(error) => {
                    let _ = sender.send(FileAction::Failed(IoError::FileWrite {
                        format,
                        file_name,
                        message: error.to_string(),
                    }));
                }
            }
        }
    };

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(task);

    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        pollster::block_on(task);
    });
}

/// Compatibility shim for callers that have not yet migrated to [`ExportRequest`].
#[deprecated(note = "use trigger_export(ExportRequest::png(data), sender)")]
pub fn trigger_export_png(data: Vec<u8>, sender: Sender<FileAction>) {
    trigger_export(ExportRequest::png(data), sender);
}

#[cfg(test)]
mod tests {
    use super::{
        resize_rgba_nearest_neighbor, scaled_canvas_dimensions, validate_canvas_dimensions,
        ExportFormat, ExportRequest, IoError, MAX_CANVAS_DIMENSION,
    };

    #[test]
    fn export_formats_have_distinct_dialog_defaults() {
        assert_eq!(ExportFormat::Png.extensions(), &["png"]);
        assert_eq!(ExportFormat::Gif.extensions(), &["gif"]);
        assert_eq!(ExportFormat::SpriteSheetPng.extensions(), &["png"]);
        assert_eq!(ExportFormat::Project.extensions(), &["pbud"]);

        assert_ne!(
            ExportFormat::Png.default_file_name(),
            ExportFormat::Gif.default_file_name()
        );
        assert_ne!(
            ExportFormat::Png.dialog_filter_name(),
            ExportFormat::SpriteSheetPng.dialog_filter_name()
        );

        let request = ExportRequest::gif(vec![1, 2, 3])
            .with_suggested_file_name("walk.gif")
            .with_source_revision(7);
        assert_eq!(request.format, ExportFormat::Gif);
        assert_eq!(request.suggested_file_name(), "walk.gif");
        assert_eq!(request.source_revision(), Some(7));
    }

    #[test]
    fn canvas_dimension_validation_rejects_invalid_and_oversized_images() {
        assert!(validate_canvas_dimensions(1, 1).is_ok());
        assert_eq!(
            validate_canvas_dimensions(0, 10),
            Err(IoError::InvalidCanvasDimensions {
                width: 0,
                height: 10,
            })
        );
        assert!(matches!(
            validate_canvas_dimensions(MAX_CANVAS_DIMENSION + 1, 1),
            Err(IoError::InvalidCanvasDimensions { .. })
        ));
    }

    #[test]
    fn nearest_neighbor_resizing_expands_each_source_pixel() {
        let source_pixels = vec![255, 0, 0, 255, 0, 0, 255, 127];

        let (width, height, scaled) = resize_rgba_nearest_neighbor(&source_pixels, 2, 1, 4, 2)
            .expect("resize should succeed");

        assert_eq!((width, height), (4, 2));
        assert_eq!(
            scaled,
            vec![
                255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 127, 0, 0, 255, 127, 255, 0, 0, 255,
                255, 0, 0, 255, 0, 0, 255, 127, 0, 0, 255, 127,
            ]
        );
    }

    #[test]
    fn scaling_rejects_zero_and_oversized_output_dimensions() {
        assert_eq!(
            scaled_canvas_dimensions(1, 1, 0),
            Err(IoError::InvalidExportScale { scale: 0 })
        );
        assert_eq!(
            scaled_canvas_dimensions(MAX_CANVAS_DIMENSION, 1, 2),
            Err(IoError::InvalidCanvasDimensions {
                width: MAX_CANVAS_DIMENSION * 2,
                height: 2,
            })
        );
    }
}
