//! Flattened WebP interchange import and export.

use std::io::Cursor;

use image::{ImageFormat, ImageReader, RgbaImage};

use crate::{
    document::Document,
    io::{
        resize_rgba_nearest_neighbor, rgba_byte_len, scaled_canvas_dimensions,
        validate_canvas_dimensions, ExportFormat, IoError,
    },
};

/// Exports the document's flattened composite as a WebP.
pub fn export_document_to_webp(document: &Document) -> Result<Vec<u8>, IoError> {
    export_document_to_webp_at_scale(document, 1)
}

/// Exports the document's flattened composite as a WebP at an integer scale.
pub fn export_document_to_webp_at_scale(
    document: &Document,
    scale: u32,
) -> Result<Vec<u8>, IoError> {
    let (scaled_width, scaled_height) =
        scaled_canvas_dimensions(document.width, document.height, scale)?;
    export_document_to_webp_at_dimensions(document, scaled_width, scaled_height)
}

/// Exports the flattened document at exact pixel dimensions.
pub fn export_document_to_webp_at_dimensions(
    document: &Document,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, IoError> {
    validate_canvas_dimensions(width, height)?;

    let canvas = document.composite_preview();
    let (actual_width, actual_height, pixels) = resize_rgba_nearest_neighbor(
        canvas.pixels(),
        canvas.width(),
        canvas.height(),
        width,
        height,
    )?;
    debug_assert_eq!((actual_width, actual_height), (width, height));
    let actual_len = pixels.len();
    let expected_len = rgba_byte_len(width, height)?;
    let image =
        RgbaImage::from_raw(width, height, pixels).ok_or(IoError::InvalidRgbaBufferLength {
            width,
            height,
            actual: actual_len,
            expected: expected_len,
        })?;

    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::WebP)
        .map_err(|error| IoError::Encode {
            format: ExportFormat::WebP,
            message: error.to_string(),
        })?;

    Ok(cursor.into_inner())
}

/// Imports a WebP as a single-layer document.
pub fn import_webp_to_document(data: &[u8]) -> Result<Document, IoError> {
    let (width, height) = ImageReader::with_format(Cursor::new(data), ImageFormat::WebP)
        .into_dimensions()
        .map_err(|error| IoError::Decode {
            format: "WebP image",
            message: error.to_string(),
        })?;
    validate_canvas_dimensions(width, height)?;

    let image = image::load_from_memory_with_format(data, ImageFormat::WebP)
        .map_err(|error| IoError::Decode {
            format: "WebP image",
            message: error.to_string(),
        })?;

    let rgba8 = image.to_rgba8();
    let pixels = rgba8.into_raw();
    let mut document = Document::new(width, height);

    let layer = document.active_layer_mut();
    layer.canvas.pixels_mut().copy_from_slice(&pixels);

    Ok(document)
}
