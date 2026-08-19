//! Flattened PNG interchange import and export.

use std::io::Cursor;

use image::{ImageFormat, ImageReader, RgbaImage};

use crate::{
    document::Document,
    io::{
        resize_rgba_nearest_neighbor, rgba_byte_len, scaled_canvas_dimensions,
        validate_canvas_dimensions, ExportFormat, IoError,
    },
};

/// Exports the document's flattened composite as a PNG.
///
/// PNG is an interchange format here: it intentionally does not preserve
/// PixelBuddy layers, palette entries, animation frames, or editor state.
pub fn export_document_to_png(document: &Document) -> Result<Vec<u8>, IoError> {
    export_document_to_png_at_scale(document, 1)
}

/// Exports the document's flattened composite as a PNG at an integer scale.
///
/// Scaling uses nearest-neighbor sampling so each source pixel remains a
/// crisp square. PixelBuddy project files deliberately do not use this path:
/// they always preserve their native editable canvas dimensions.
pub fn export_document_to_png_at_scale(
    document: &Document,
    scale: u32,
) -> Result<Vec<u8>, IoError> {
    let (scaled_width, scaled_height) =
        scaled_canvas_dimensions(document.width, document.height, scale)?;
    export_document_to_png_at_dimensions(document, scaled_width, scaled_height)
}

/// Exports the flattened document at exact pixel dimensions.
pub fn export_document_to_png_at_dimensions(
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
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|error| IoError::Encode {
            format: ExportFormat::Png,
            message: error.to_string(),
        })?;

    Ok(cursor.into_inner())
}

/// Imports a PNG as a single-layer document.
///
/// Dimensions are read and validated before decoding pixel data, preventing a
/// malformed header from triggering a large image allocation.
pub fn import_png_to_document(data: &[u8]) -> Result<Document, IoError> {
    let (width, height) = ImageReader::with_format(Cursor::new(data), ImageFormat::Png)
        .into_dimensions()
        .map_err(|error| IoError::Decode {
            format: "PNG image",
            message: error.to_string(),
        })?;
    validate_canvas_dimensions(width, height)?;

    let image = image::load_from_memory_with_format(data, ImageFormat::Png)
        .map_err(|error| IoError::Decode {
            format: "PNG image",
            message: error.to_string(),
        })?
        .into_rgba8();

    let (decoded_width, decoded_height) = image.dimensions();
    if (decoded_width, decoded_height) != (width, height) {
        return Err(IoError::Decode {
            format: "PNG image",
            message: format!(
                "header dimensions {width}x{height} did not match decoded dimensions {decoded_width}x{decoded_height}"
            ),
        });
    }

    let expected_len = rgba_byte_len(width, height)?;
    let source_pixels = image.as_raw();
    if source_pixels.len() != expected_len {
        return Err(IoError::InvalidRgbaBufferLength {
            width,
            height,
            actual: source_pixels.len(),
            expected: expected_len,
        });
    }

    let mut document = Document::new(width, height);
    let destination_pixels = document.active_layer_mut().canvas.pixels_mut();
    if destination_pixels.len() != source_pixels.len() {
        return Err(IoError::InvalidRgbaBufferLength {
            width,
            height,
            actual: destination_pixels.len(),
            expected: source_pixels.len(),
        });
    }
    destination_pixels.copy_from_slice(source_pixels);

    Ok(document)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{ImageFormat, RgbaImage};

    use super::{
        export_document_to_png, export_document_to_png_at_dimensions,
        export_document_to_png_at_scale, import_png_to_document,
    };
    use crate::{
        document::Document,
        io::{IoError, MAX_CANVAS_DIMENSION},
    };

    #[test]
    fn png_round_trip_preserves_dimensions_and_alpha() {
        let mut document = Document::new(2, 2);
        let canvas = &mut document.active_layer_mut().canvas;
        canvas.set_pixel(0, 0, [255, 0, 0, 255]);
        canvas.set_pixel(1, 0, [1, 2, 3, 127]);
        canvas.set_pixel(0, 1, [0, 0, 0, 0]);
        canvas.set_pixel(1, 1, [10, 20, 30, 64]);

        let encoded = export_document_to_png(&document).expect("PNG export should succeed");
        let imported = import_png_to_document(&encoded).expect("PNG import should succeed");

        assert_eq!((imported.width, imported.height), (2, 2));
        assert_eq!(
            imported.active_layer().canvas.pixels(),
            document.composite_preview().pixels()
        );
    }

    #[test]
    fn scaled_png_export_uses_integer_nearest_neighbor_pixels() {
        let mut document = Document::new(2, 2);
        let canvas = &mut document.active_layer_mut().canvas;
        canvas.set_pixel(0, 0, [255, 0, 0, 255]);
        canvas.set_pixel(1, 0, [0, 255, 0, 255]);
        canvas.set_pixel(0, 1, [0, 0, 255, 255]);
        canvas.set_pixel(1, 1, [255, 255, 255, 96]);

        let encoded =
            export_document_to_png_at_scale(&document, 2).expect("scaled PNG export should work");
        let image = image::load_from_memory_with_format(&encoded, ImageFormat::Png)
            .expect("scaled PNG should decode")
            .into_rgba8();

        assert_eq!(image.dimensions(), (4, 4));
        for y in 0..2 {
            for x in 0..2 {
                assert_eq!(image.get_pixel(x, y).0, [255, 0, 0, 255]);
                assert_eq!(image.get_pixel(x + 2, y).0, [0, 255, 0, 255]);
                assert_eq!(image.get_pixel(x, y + 2).0, [0, 0, 255, 255]);
                assert_eq!(image.get_pixel(x + 2, y + 2).0, [255, 255, 255, 96]);
            }
        }
    }

    #[test]
    fn scaled_png_export_rejects_zero_and_oversized_scales() {
        let document = Document::new(2, 1);
        assert_eq!(
            export_document_to_png_at_scale(&document, 0),
            Err(IoError::InvalidExportScale { scale: 0 })
        );

        let largest_width_document = Document::new(MAX_CANVAS_DIMENSION, 1);
        assert!(matches!(
            export_document_to_png_at_scale(&largest_width_document, 2),
            Err(IoError::InvalidCanvasDimensions {
                width,
                height: 2,
            }) if width == MAX_CANVAS_DIMENSION * 2
        ));
    }

    #[test]
    fn png_export_accepts_exact_non_integer_dimensions() {
        let mut document = Document::new(2, 1);
        document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 255]);
        document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [0, 0, 255, 255]);

        let encoded = export_document_to_png_at_dimensions(&document, 3, 2)
            .expect("exact-size PNG export should work");
        let image = image::load_from_memory_with_format(&encoded, ImageFormat::Png)
            .expect("exact-size PNG should decode")
            .into_rgba8();

        assert_eq!(image.dimensions(), (3, 2));
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(1, 0).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(2, 0).0, [0, 0, 255, 255]);
    }

    #[test]
    fn malformed_png_returns_a_decode_error() {
        let error = import_png_to_document(b"not a png").expect_err("invalid data must fail");
        assert!(matches!(
            error,
            IoError::Decode {
                format: "PNG image",
                ..
            }
        ));
    }

    #[test]
    fn oversized_png_is_rejected_from_its_header() {
        let image = RgbaImage::new(MAX_CANVAS_DIMENSION + 1, 1);
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, ImageFormat::Png)
            .expect("fixture PNG should encode");

        let error = import_png_to_document(&encoded.into_inner())
            .expect_err("oversized dimensions must be rejected");
        assert!(matches!(error, IoError::InvalidCanvasDimensions { .. }));
    }
}
