//! Horizontal PNG sprite-sheet export.

use std::io::Cursor;

use image::{ImageFormat, RgbaImage};

use crate::{
    document::AnimationManager,
    io::{
        resize_rgba_nearest_neighbor, rgba_byte_len, scaled_canvas_dimensions,
        validate_animation_frames, validate_canvas_dimensions, validate_export_scale,
        validate_raster_input_size, ExportFormat, IoError, MAX_CANVAS_PIXELS,
    },
};

/// Maximum number of frames accepted from one sprite-sheet grid.
pub const MAX_SPRITESHEET_FRAMES: u64 = 1_024;

/// Maximum combined frame pixels allocated by one sprite-sheet import.
pub const MAX_SPRITESHEET_AGGREGATE_PIXELS: u64 = MAX_CANVAS_PIXELS / 2;

/// Maximum edge uploaded for the import dialog preview.
pub const SPRITESHEET_PREVIEW_MAX_EDGE: u32 = 256;
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

        let row_len = usize::try_from(frame_width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(IoError::DimensionOverflow {
                operation: "calculating sprite-sheet frame row bytes",
            })?;
        let sheet_stride = usize::try_from(unscaled_sheet_width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(IoError::DimensionOverflow {
                operation: "calculating sprite-sheet row stride",
            })?;
        let frame_column_offset =
            frame_index
                .checked_mul(row_len)
                .ok_or(IoError::DimensionOverflow {
                    operation: "calculating sprite-sheet frame column offset",
                })?;
        let frame_height =
            usize::try_from(frame_height).map_err(|_| IoError::DimensionOverflow {
                operation: "converting sprite-sheet frame height for this platform",
            })?;
        for row in 0..frame_height {
            let source_start = row.checked_mul(row_len).ok_or(IoError::DimensionOverflow {
                operation: "calculating sprite-sheet source row offset",
            })?;
            let source_end =
                source_start
                    .checked_add(row_len)
                    .ok_or(IoError::DimensionOverflow {
                        operation: "calculating sprite-sheet source row end",
                    })?;
            let destination_start = row
                .checked_mul(sheet_stride)
                .and_then(|offset| offset.checked_add(frame_column_offset))
                .ok_or(IoError::DimensionOverflow {
                    operation: "calculating sprite-sheet row offset",
                })?;
            let destination_end =
                destination_start
                    .checked_add(row_len)
                    .ok_or(IoError::DimensionOverflow {
                        operation: "calculating sprite-sheet destination row end",
                    })?;
            let source = source_pixels.get(source_start..source_end).ok_or(
                IoError::InvalidRgbaBufferLength {
                    width: frame_width,
                    height: frame_height as u32,
                    actual: source_pixels.len(),
                    expected: frame_len,
                },
            )?;
            let destination = sheet_pixels
                .get_mut(destination_start..destination_end)
                .ok_or(IoError::InvalidRgbaBufferLength {
                    width: unscaled_sheet_width,
                    height: unscaled_sheet_height,
                    actual: unscaled_sheet_len,
                    expected: unscaled_sheet_len,
                })?;
            destination.copy_from_slice(source);
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

/// Reads and validates an encoded sprite-sheet header before pixel decoding.
fn validated_source_dimensions(
    data: &[u8],
    file_name: &str,
) -> Result<(ImageFormat, u32, u32), IoError> {
    use image::ImageReader;

    validate_raster_input_size(data, "sprite sheet")?;
    let format = if file_name.to_lowercase().ends_with(".webp") {
        ImageFormat::WebP
    } else {
        ImageFormat::Png
    };
    let (width, height) = ImageReader::with_format(Cursor::new(data), format)
        .into_dimensions()
        .map_err(|error| IoError::Decode {
            format: "sprite sheet",
            message: error.to_string(),
        })?;
    validate_canvas_dimensions(width, height)?;
    Ok((format, width, height))
}

/// Decodes a source sheet into a bounded RGBA preview for the import dialog.
pub fn decode_spritesheet_preview(
    data: &[u8],
    file_name: &str,
) -> Result<(u32, u32, Vec<u8>), IoError> {
    let (format, width, height) = validated_source_dimensions(data, file_name)?;
    let image =
        image::load_from_memory_with_format(data, format).map_err(|error| IoError::Decode {
            format: "sprite sheet",
            message: error.to_string(),
        })?;
    if (image.width(), image.height()) != (width, height) {
        return Err(IoError::Decode {
            format: "sprite sheet",
            message: format!(
                "header dimensions {width}x{height} did not match decoded dimensions {}x{}",
                image.width(),
                image.height()
            ),
        });
    }

    let preview = image
        .thumbnail(SPRITESHEET_PREVIEW_MAX_EDGE, SPRITESHEET_PREVIEW_MAX_EDGE)
        .into_rgba8();
    let (preview_width, preview_height) = preview.dimensions();
    let expected = rgba_byte_len(preview_width, preview_height)?;
    let pixels = preview.into_raw();
    if pixels.len() != expected {
        return Err(IoError::InvalidRgbaBufferLength {
            width: preview_width,
            height: preview_height,
            actual: pixels.len(),
            expected,
        });
    }
    Ok((preview_width, preview_height, pixels))
}

fn validate_grid_dimensions(
    width: u32,
    height: u32,
    columns: u32,
    rows: u32,
) -> Result<(u32, u32, u64), IoError> {
    if columns == 0 || rows == 0 {
        return Err(IoError::Decode {
            format: "sprite sheet",
            message: "Columns and rows must be greater than zero.".to_owned(),
        });
    }
    let frame_count =
        u64::from(columns)
            .checked_mul(u64::from(rows))
            .ok_or(IoError::DimensionOverflow {
                operation: "calculating sprite-sheet frame count",
            })?;
    if frame_count > MAX_SPRITESHEET_FRAMES {
        return Err(IoError::SpriteSheetFrameLimitExceeded {
            requested: frame_count,
            maximum: MAX_SPRITESHEET_FRAMES,
        });
    }
    if width.checked_rem(columns) != Some(0) || height.checked_rem(rows) != Some(0) {
        return Err(IoError::Decode {
            format: "sprite sheet",
            message: format!(
                "Image dimensions {width}x{height} cannot be evenly divided by {columns} columns and {rows} rows."
            ),
        });
    }
    let frame_width = width / columns;
    let frame_height = height / rows;
    validate_canvas_dimensions(frame_width, frame_height)?;
    let frame_pixels = u64::from(frame_width)
        .checked_mul(u64::from(frame_height))
        .ok_or(IoError::DimensionOverflow {
            operation: "calculating sprite-sheet frame pixels",
        })?;
    let aggregate_pixels =
        frame_pixels
            .checked_mul(frame_count)
            .ok_or(IoError::DimensionOverflow {
                operation: "calculating aggregate sprite-sheet frame pixels",
            })?;
    if aggregate_pixels > MAX_SPRITESHEET_AGGREGATE_PIXELS {
        return Err(IoError::SpriteSheetPixelLimitExceeded {
            requested: aggregate_pixels,
            maximum: MAX_SPRITESHEET_AGGREGATE_PIXELS,
        });
    }
    Ok((frame_width, frame_height, frame_count))
}

/// Imports a sprite sheet grid and slices it into animation frames.
pub fn import_spritesheet(
    data: &[u8],
    file_name: &str,
    columns: u32,
    rows: u32,
) -> Result<AnimationManager, IoError> {
    let (format, width, height) = validated_source_dimensions(data, file_name)?;
    let (frame_width, frame_height, frame_count) =
        validate_grid_dimensions(width, height, columns, rows)?;

    let image = image::load_from_memory_with_format(data, format)
        .map_err(|error| IoError::Decode {
            format: "sprite sheet",
            message: error.to_string(),
        })?
        .into_rgba8();
    if image.dimensions() != (width, height) {
        return Err(IoError::Decode {
            format: "sprite sheet",
            message: format!(
                "header dimensions {width}x{height} did not match decoded dimensions {}x{}",
                image.width(),
                image.height()
            ),
        });
    }
    let src_pixels = image.as_raw();
    let source_stride = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or(IoError::DimensionOverflow {
            operation: "calculating sprite-sheet source row bytes",
        })?;
    let frame_row_bytes = usize::try_from(frame_width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or(IoError::DimensionOverflow {
            operation: "calculating sprite-sheet frame row bytes",
        })?;
    let capacity = usize::try_from(frame_count).map_err(|_| IoError::DimensionOverflow {
        operation: "converting sprite-sheet frame count for this platform",
    })?;
    let mut frames = Vec::with_capacity(capacity);
    for row in 0..rows {
        for column in 0..columns {
            let mut document = crate::document::Document::new(frame_width, frame_height);
            let pixels = document.active_layer_mut().canvas.pixels_mut();
            for y in 0..frame_height {
                let source_y = row
                    .checked_mul(frame_height)
                    .and_then(|value| value.checked_add(y))
                    .ok_or(IoError::DimensionOverflow {
                        operation: "calculating sprite-sheet source row",
                    })?;
                let source_x =
                    column
                        .checked_mul(frame_width)
                        .ok_or(IoError::DimensionOverflow {
                            operation: "calculating sprite-sheet source column",
                        })?;
                let src_start = usize::try_from(source_y)
                    .ok()
                    .and_then(|value| value.checked_mul(source_stride))
                    .and_then(|value| {
                        usize::try_from(source_x)
                            .ok()
                            .and_then(|x| x.checked_mul(4))
                            .and_then(|x| value.checked_add(x))
                    })
                    .ok_or(IoError::DimensionOverflow {
                        operation: "calculating sprite-sheet source byte offset",
                    })?;
                let src_end =
                    src_start
                        .checked_add(frame_row_bytes)
                        .ok_or(IoError::DimensionOverflow {
                            operation: "calculating sprite-sheet source row end",
                        })?;
                let dst_start = usize::try_from(y)
                    .ok()
                    .and_then(|value| value.checked_mul(frame_row_bytes))
                    .ok_or(IoError::DimensionOverflow {
                        operation: "calculating sprite-sheet destination byte offset",
                    })?;
                let dst_end =
                    dst_start
                        .checked_add(frame_row_bytes)
                        .ok_or(IoError::DimensionOverflow {
                            operation: "calculating sprite-sheet destination row end",
                        })?;
                let source = src_pixels.get(src_start..src_end).ok_or(IoError::Decode {
                    format: "sprite sheet",
                    message: "Decoded pixel data ended before the requested grid cell.".to_owned(),
                })?;
                let destination_len = pixels.len();
                let destination =
                    pixels
                        .get_mut(dst_start..dst_end)
                        .ok_or(IoError::InvalidRgbaBufferLength {
                            width: frame_width,
                            height: frame_height,
                            actual: destination_len,
                            expected: rgba_byte_len(frame_width, frame_height)?,
                        })?;
                destination.copy_from_slice(source);
            }
            frames.push(crate::document::AnimationFrame::new(document));
        }
    }

    let mut frames = frames.into_iter();
    let first = frames.next().ok_or(IoError::EmptyAnimation)?;
    let mut animation = AnimationManager::new(first.document);
    animation.frames[0].duration_ms = first.duration_ms;
    animation.frames.extend(frames);
    Ok(animation)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        decode_spritesheet_preview, export_spritesheet_png, export_spritesheet_png_at_dimensions,
        export_spritesheet_png_at_scale, import_spritesheet, validate_grid_dimensions,
        MAX_SPRITESHEET_AGGREGATE_PIXELS, MAX_SPRITESHEET_FRAMES, SPRITESHEET_PREVIEW_MAX_EDGE,
    };
    use crate::{
        document::{AnimationFrame, AnimationManager, Document},
        io::IoError,
    };

    fn encoded_png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::new(width, height);
        let mut cursor = Cursor::new(Vec::new());
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .expect("test PNG should encode");
        cursor.into_inner()
    }

    #[test]
    fn sprite_sheet_import_accepts_a_normal_grid_and_extreme_aspect_ratio() {
        let normal = encoded_png(4, 2);
        let animation = import_spritesheet(&normal, "normal.png", 2, 1)
            .expect("normal sprite sheet should import");
        assert_eq!(animation.frames.len(), 2);
        assert_eq!(animation.frames[0].document.width, 2);
        assert_eq!(animation.frames[0].document.height, 2);

        let wide = encoded_png(crate::io::MAX_CANVAS_DIMENSION, 1);
        let wide_animation = import_spritesheet(&wide, "wide.png", 1, 1)
            .expect("a bounded extreme aspect ratio should import");
        assert_eq!(
            wide_animation.frames[0].document.width,
            crate::io::MAX_CANVAS_DIMENSION
        );
    }

    #[test]
    fn sprite_sheet_import_rejects_zero_overflowing_and_excessive_grids_predecode() {
        let tiny = encoded_png(1, 1);
        assert!(matches!(
            import_spritesheet(&tiny, "tiny.png", 0, 1),
            Err(IoError::Decode { message, .. }) if message.contains("greater than zero")
        ));
        assert!(matches!(
            import_spritesheet(
                &tiny,
                "tiny.png",
                (MAX_SPRITESHEET_FRAMES + 1) as u32,
                1
            ),
            Err(IoError::SpriteSheetFrameLimitExceeded { requested, maximum })
                if requested == MAX_SPRITESHEET_FRAMES + 1
                    && maximum == MAX_SPRITESHEET_FRAMES
        ));
        assert!(matches!(
            validate_grid_dimensions(4_096, 4_096, 1, 1),
            Err(IoError::SpriteSheetPixelLimitExceeded { requested, maximum })
                if requested == 4_096 * 4_096
                    && maximum == MAX_SPRITESHEET_AGGREGATE_PIXELS
        ));
        assert!(matches!(
            validate_grid_dimensions(u32::MAX, 1, u32::MAX, u32::MAX),
            Err(IoError::SpriteSheetFrameLimitExceeded { .. })
        ));
    }

    #[test]
    fn sprite_sheet_preview_is_bounded_and_truncated_data_is_rejected() {
        let wide = encoded_png(1_024, 64);
        let (width, height, pixels) =
            decode_spritesheet_preview(&wide, "preview.png").expect("preview should decode");
        assert!(width <= SPRITESHEET_PREVIEW_MAX_EDGE);
        assert!(height <= SPRITESHEET_PREVIEW_MAX_EDGE);
        assert_eq!(pixels.len(), width as usize * height as usize * 4);

        assert!(matches!(
            decode_spritesheet_preview(&wide[..wide.len() / 2], "truncated.png"),
            Err(IoError::Decode { .. })
        ));
    }
    #[test]
    fn hostile_sprite_sheet_fixture_is_rejected_before_frame_allocation() {
        let hostile = include_bytes!("../../tests/fixtures/hostile_spritesheet.png");
        assert!(decode_spritesheet_preview(hostile, "hostile_spritesheet.png").is_err());
        assert!(import_spritesheet(hostile, "hostile_spritesheet.png", 1, 1).is_err());
    }

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
