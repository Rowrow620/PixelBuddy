//! Animated GIF export.

use std::io::Cursor;

use image::codecs::gif::GifEncoder;
use image::{Delay, Frame, RgbaImage};

use crate::{
    document::AnimationManager,
    io::{
        resize_rgba_nearest_neighbor, rgba_byte_len, scaled_canvas_dimensions,
        validate_animation_frames, validate_canvas_dimensions, validate_export_scale, ExportFormat,
        IoError,
    },
};

/// Exports each animation frame as a GIF frame.
///
/// GIF timing follows `AnimationFrame::duration_ms`, the same timing source
/// used by preview playback. The timeline FPS control synchronizes those
/// durations when the user selects a uniform animation speed.
pub fn export_animation_to_gif(animation: &AnimationManager) -> Result<Vec<u8>, IoError> {
    export_animation_to_gif_at_scale(animation, 1)
}

/// Exports each animation frame as a nearest-neighbor scaled GIF frame.
///
/// `AnimationFrame::duration_ms` is passed through unchanged, so scaling only
/// changes the resolution—not the animation's timing.
pub fn export_animation_to_gif_at_scale(
    animation: &AnimationManager,
    scale: u32,
) -> Result<Vec<u8>, IoError> {
    validate_export_scale(scale)?;
    let (frame_width, frame_height) = validate_animation_frames(animation)?;
    let (scaled_width, scaled_height) = scaled_canvas_dimensions(frame_width, frame_height, scale)?;
    export_animation_to_gif_at_dimensions(animation, scaled_width, scaled_height)
}

/// Exports every animation frame at exact pixel dimensions.
pub fn export_animation_to_gif_at_dimensions(
    animation: &AnimationManager,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, IoError> {
    validate_canvas_dimensions(width, height)?;
    let (frame_width, frame_height) = validate_animation_frames(animation)?;
    let expected_len = rgba_byte_len(frame_width, frame_height)?;

    let mut buffer = Vec::new();
    {
        let mut encoder = GifEncoder::new(Cursor::new(&mut buffer));

        for animation_frame in &animation.frames {
            let canvas = animation_frame.document.composite_preview();
            let actual_len = canvas.pixels().len();
            if actual_len != expected_len {
                return Err(IoError::InvalidRgbaBufferLength {
                    width: frame_width,
                    height: frame_height,
                    actual: actual_len,
                    expected: expected_len,
                });
            }

            let (actual_width, actual_height, pixels) = resize_rgba_nearest_neighbor(
                canvas.pixels(),
                frame_width,
                frame_height,
                width,
                height,
            )?;
            debug_assert_eq!((actual_width, actual_height), (width, height));
            let scaled_len = pixels.len();
            let expected_scaled_len = rgba_byte_len(width, height)?;
            let image = RgbaImage::from_raw(width, height, pixels).ok_or(
                IoError::InvalidRgbaBufferLength {
                    width,
                    height,
                    actual: scaled_len,
                    expected: expected_scaled_len,
                },
            )?;
            let delay = Delay::from_numer_denom_ms(animation_frame.duration_ms, 1);
            let frame = Frame::from_parts(image, 0, 0, delay);

            encoder
                .encode_frame(frame)
                .map_err(|error| IoError::Encode {
                    format: ExportFormat::Gif,
                    message: error.to_string(),
                })?;
        }
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::codecs::gif::GifDecoder;
    use image::{AnimationDecoder, RgbaImage};

    use super::{
        export_animation_to_gif, export_animation_to_gif_at_dimensions,
        export_animation_to_gif_at_scale,
    };
    use crate::{
        document::{AnimationFrame, AnimationManager, Document},
        io::IoError,
    };

    #[test]
    fn gif_export_round_trips_frames_and_per_frame_durations() {
        let mut first_document = Document::new(2, 1);
        first_document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 255]);

        let mut animation = AnimationManager::new(first_document);
        animation.frames[0].duration_ms = 120;

        let mut second_document = Document::new(2, 1);
        second_document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [0, 0, 255, 255]);
        animation.frames.push(AnimationFrame {
            document: second_document,
            duration_ms: 250,
        });

        let encoded = export_animation_to_gif(&animation).expect("GIF export should succeed");
        let decoder = GifDecoder::new(Cursor::new(encoded)).expect("GIF should decode");
        let frames = decoder
            .into_frames()
            .collect_frames()
            .expect("GIF frames should decode");

        assert_eq!(frames.len(), 2);
        assert_eq!(
            frames[0].buffer(),
            &RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 0, 0]).unwrap()
        );
        assert_eq!(frames[0].delay().numer_denom_ms(), (120, 1));
        assert_eq!(frames[1].delay().numer_denom_ms(), (250, 1));
    }

    #[test]
    fn scaled_gif_export_expands_frames_without_changing_timing() {
        let mut first_document = Document::new(2, 1);
        first_document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 255]);
        first_document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [0, 255, 0, 255]);

        let mut animation = AnimationManager::new(first_document);
        animation.frames[0].duration_ms = 120;

        let mut second_document = Document::new(2, 1);
        second_document
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [0, 0, 255, 255]);
        second_document
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [255, 255, 255, 255]);
        animation.frames.push(AnimationFrame {
            document: second_document,
            duration_ms: 250,
        });

        let encoded =
            export_animation_to_gif_at_scale(&animation, 2).expect("scaled GIF should export");
        let decoder = GifDecoder::new(Cursor::new(encoded)).expect("GIF should decode");
        let frames = decoder
            .into_frames()
            .collect_frames()
            .expect("GIF frames should decode");

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].buffer().dimensions(), (4, 2));
        for y in 0..2 {
            assert_eq!(frames[0].buffer().get_pixel(0, y).0, [255, 0, 0, 255]);
            assert_eq!(frames[0].buffer().get_pixel(1, y).0, [255, 0, 0, 255]);
            assert_eq!(frames[0].buffer().get_pixel(2, y).0, [0, 255, 0, 255]);
            assert_eq!(frames[0].buffer().get_pixel(3, y).0, [0, 255, 0, 255]);
        }
        assert_eq!(frames[0].delay().numer_denom_ms(), (120, 1));
        assert_eq!(frames[1].delay().numer_denom_ms(), (250, 1));
    }

    #[test]
    fn scaled_gif_export_rejects_zero_and_oversized_scales() {
        let animation = AnimationManager::new(Document::new(2, 1));
        assert_eq!(
            export_animation_to_gif_at_scale(&animation, 0),
            Err(IoError::InvalidExportScale { scale: 0 })
        );

        let oversized_animation =
            AnimationManager::new(Document::new(crate::io::MAX_CANVAS_DIMENSION, 1));
        assert!(matches!(
            export_animation_to_gif_at_scale(&oversized_animation, 2),
            Err(IoError::InvalidCanvasDimensions { .. })
        ));
    }

    #[test]
    fn gif_export_accepts_exact_frame_dimensions() {
        let animation = AnimationManager::new(Document::new(2, 1));
        let encoded = export_animation_to_gif_at_dimensions(&animation, 3, 2)
            .expect("exact-size GIF should export");
        let decoder = GifDecoder::new(Cursor::new(encoded)).expect("GIF should decode");
        let frames = decoder
            .into_frames()
            .collect_frames()
            .expect("GIF frames should decode");

        assert_eq!(frames[0].buffer().dimensions(), (3, 2));
    }

    #[test]
    fn gif_export_rejects_mismatched_frame_dimensions() {
        let mut animation = AnimationManager::new(Document::new(2, 2));
        animation
            .frames
            .push(AnimationFrame::new(Document::new(3, 2)));

        assert!(matches!(
            export_animation_to_gif(&animation),
            Err(IoError::MismatchedFrameDimensions { frame_index: 1, .. })
        ));
    }
}
