//! Horizontal PNG sprite-sheet export.

use std::io::Cursor;

use image::{ImageFormat, RgbaImage};

use crate::{
    document::AnimationManager,
    io::{
        resize_rgba_nearest_neighbor, rgba_byte_len, scaled_canvas_dimensions,
        validate_animation_frames, validate_canvas_dimensions, validate_export_scale, ExportFormat,
        IoError,
    },
};

/// Exports animation frames as a left-to-right PNG sprite sheet.
///
/// This is intentionally a simple one-row layout for now. It validates that
/// every frame has the same dimensions rather than silently clipping or
/// padding mismatched frames.
pub fn export_spritesheet_png(animation: &AnimationManager) -> Result<Vec<u8>, IoError> {
    export_spritesheet_png_at_scale(animation, 1)
}

/// Exports animation frames as a left-to-right PNG sprite sheet at an integer
/// nearest-neighbor scale.
///
/// The scale applies to the completed sheet, so each frame remains aligned to
/// the same grid and no filtering is introduced between adjacent frames.
pub fn export_spritesheet_png_at_scale(
    animation: &AnimationManager,
    scale: u32,
) -> Result<Vec<u8>, IoError> {
    validate_export_scale(scale)?;
    let (frame_width, frame_height) = validate_animation_frames(animation)?;
    let frame_count =
        u32::try_from(animation.frames.len()).map_err(|_| IoError::DimensionOverflow {
            operation: "converting sprite-sheet frame count",
        })?;
    let unscaled_sheet_width =
        frame_width
            .checked_mul(frame_count)
            .ok_or(IoError::DimensionOverflow {
                operation: "calculating sprite-sheet width",
            })?;
    let unscaled_sheet_height = frame_height;

    // The final horizontal sheet width includes every frame and the requested
    // scale. Validate it before allocating the intermediate or final image.
    let (sheet_width, sheet_height) =
        scaled_canvas_dimensions(unscaled_sheet_width, unscaled_sheet_height, scale)?;
    export_spritesheet_png_at_dimensions(animation, sheet_width, sheet_height)
}

/// Exports the completed one-row sprite sheet at exact pixel dimensions.
pub fn export_spritesheet_png_at_dimensions(
    animation: &AnimationManager,
    sheet_width: u32,
    sheet_height: u32,
) -> Result<Vec<u8>, IoError> {
    validate_canvas_dimensions(sheet_width, sheet_height)?;
    let (frame_width, frame_height) = validate_animation_frames(animation)?;
    let frame_count =
        u32::try_from(animation.frames.len()).map_err(|_| IoError::DimensionOverflow {
            operation: "converting sprite-sheet frame count",
        })?;
    let unscaled_sheet_width =
        frame_width
            .checked_mul(frame_count)
            .ok_or(IoError::DimensionOverflow {
                operation: "calculating sprite-sheet width",
            })?;
    let unscaled_sheet_height = frame_height;

    let frame_len = rgba_byte_len(frame_width, frame_height)?;
    let unscaled_sheet_len = rgba_byte_len(unscaled_sheet_width, unscaled_sheet_height)?;
    let mut sheet_pixels = vec![0; unscaled_sheet_len];

    for (frame_index, animation_frame) in animation.frames.iter().enumerate() {
        let canvas = animation_frame.document.composite_preview();
        let source_pixels = canvas.pixels();
        if source_pixels.len() != frame_len {
            return Err(IoError::InvalidRgbaBufferLength {
                width: frame_width,
                height: frame_height,
                actual: source_pixels.len(),
                expected: frame_len,
            });
        }

        let row_len = frame_width as usize * 4;
        let frame_column_offset =
            frame_index
                .checked_mul(row_len)
                .ok_or(IoError::DimensionOverflow {
                    operation: "calculating sprite-sheet frame column offset",
                })?;
        for row in 0..frame_height as usize {
            let source_start = row * row_len;
            let destination_start = row
                .checked_mul(unscaled_sheet_width as usize * 4)
                .and_then(|offset| offset.checked_add(frame_column_offset))
                .ok_or(IoError::DimensionOverflow {
                    operation: "calculating sprite-sheet row offset",
                })?;
            sheet_pixels[destination_start..destination_start + row_len]
                .copy_from_slice(&source_pixels[source_start..source_start + row_len]);
        }
    }

    let (actual_sheet_width, actual_sheet_height, scaled_sheet_pixels) =
        resize_rgba_nearest_neighbor(
            &sheet_pixels,
            unscaled_sheet_width,
            unscaled_sheet_height,
            sheet_width,
            sheet_height,
        )?;
    debug_assert_eq!(
        (actual_sheet_width, actual_sheet_height),
        (sheet_width, sheet_height)
    );
    let actual_sheet_len = scaled_sheet_pixels.len();
    let sheet_len = rgba_byte_len(sheet_width, sheet_height)?;
    let sheet = RgbaImage::from_raw(sheet_width, sheet_height, scaled_sheet_pixels).ok_or(
        IoError::InvalidRgbaBufferLength {
            width: sheet_width,
            height: sheet_height,
            actual: actual_sheet_len,
            expected: sheet_len,
        },
    )?;
    let mut cursor = Cursor::new(Vec::new());
    sheet
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|error| IoError::Encode {
            format: ExportFormat::SpriteSheetPng,
            message: error.to_string(),
        })?;

    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::{
        export_spritesheet_png, export_spritesheet_png_at_dimensions,
        export_spritesheet_png_at_scale,
    };
    use crate::{
        document::{AnimationFrame, AnimationManager, Document},
        io::IoError,
    };

    #[test]
    fn sprite_sheet_places_each_frame_left_to_right() {
        let mut first_document = Document::new(2, 1);
        first_document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 255]);

        let mut second_document = Document::new(2, 1);
        second_document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [0, 0, 255, 255]);

        let mut animation = AnimationManager::new(first_document);
        animation.frames.push(AnimationFrame::new(second_document));

        let encoded = export_spritesheet_png(&animation).expect("sprite sheet should export");
        let image = image::load_from_memory_with_format(&encoded, image::ImageFormat::Png)
            .expect("sprite sheet should decode")
            .into_rgba8();

        assert_eq!(image.dimensions(), (4, 1));
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(1, 0).0, [0, 0, 0, 0]);
        assert_eq!(image.get_pixel(2, 0).0, [0, 0, 0, 0]);
        assert_eq!(image.get_pixel(3, 0).0, [0, 0, 255, 255]);
    }

    #[test]
    fn scaled_sprite_sheet_expands_each_frame_with_nearest_neighbor_pixels() {
        let mut first_document = Document::new(2, 1);
        first_document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 255]);
        first_document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [0, 255, 0, 255]);

        let mut second_document = Document::new(2, 1);
        second_document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [0, 0, 255, 255]);
        second_document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [255, 255, 255, 255]);

        let mut animation = AnimationManager::new(first_document);
        animation.frames.push(AnimationFrame::new(second_document));

        let encoded = export_spritesheet_png_at_scale(&animation, 2)
            .expect("scaled sprite sheet should export");
        let image = image::load_from_memory_with_format(&encoded, image::ImageFormat::Png)
            .expect("sprite sheet should decode")
            .into_rgba8();

        assert_eq!(image.dimensions(), (8, 2));
        for y in 0..2 {
            assert_eq!(image.get_pixel(0, y).0, [255, 0, 0, 255]);
            assert_eq!(image.get_pixel(1, y).0, [255, 0, 0, 255]);
            assert_eq!(image.get_pixel(2, y).0, [0, 255, 0, 255]);
            assert_eq!(image.get_pixel(3, y).0, [0, 255, 0, 255]);
            assert_eq!(image.get_pixel(4, y).0, [0, 0, 255, 255]);
            assert_eq!(image.get_pixel(5, y).0, [0, 0, 255, 255]);
            assert_eq!(image.get_pixel(6, y).0, [255, 255, 255, 255]);
            assert_eq!(image.get_pixel(7, y).0, [255, 255, 255, 255]);
        }
    }

    #[test]
    fn scaled_sprite_sheet_rejects_zero_and_final_width_overflow() {
        let animation = AnimationManager::new(Document::new(2, 1));
        assert_eq!(
            export_spritesheet_png_at_scale(&animation, 0),
            Err(IoError::InvalidExportScale { scale: 0 })
        );

        let mut wide_animation =
            AnimationManager::new(Document::new(crate::io::MAX_CANVAS_DIMENSION / 2, 1));
        wide_animation
            .frames
            .push(AnimationFrame::new(Document::new(
                crate::io::MAX_CANVAS_DIMENSION / 2,
                1,
            )));

        assert!(matches!(
            export_spritesheet_png_at_scale(&wide_animation, 3),
            Err(IoError::InvalidCanvasDimensions {
                width,
                height: 3,
            }) if width == crate::io::MAX_CANVAS_DIMENSION * 3
        ));
    }

    #[test]
    fn sprite_sheet_export_accepts_exact_final_dimensions() {
        let mut animation = AnimationManager::new(Document::new(2, 1));
        animation
            .frames
            .push(AnimationFrame::new(Document::new(2, 1)));

        let encoded = export_spritesheet_png_at_dimensions(&animation, 7, 3)
            .expect("exact-size sprite sheet should export");
        let image = image::load_from_memory_with_format(&encoded, image::ImageFormat::Png)
            .expect("sprite sheet should decode")
            .into_rgba8();

        assert_eq!(image.dimensions(), (7, 3));
    }

    #[test]
    fn sprite_sheet_rejects_mismatched_frame_dimensions() {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        animation
            .frames
            .push(AnimationFrame::new(Document::new(2, 3)));

        assert!(matches!(
            export_spritesheet_png(&animation),
            Err(IoError::MismatchedFrameDimensions { frame_index: 1, .. })
        ));
    }
}
