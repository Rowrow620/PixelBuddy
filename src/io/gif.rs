use crate::document::AnimationManager;
use image::codecs::gif::GifEncoder;
use image::{Delay, Frame, RgbaImage};
use std::io::Cursor;

pub fn export_animation_to_gif(animation: &AnimationManager) -> Option<Vec<u8>> {
    if animation.frames.is_empty() {
        return None;
    }

    let mut buffer = Vec::new();
    {
        let mut encoder = GifEncoder::new(Cursor::new(&mut buffer));
        let fps = animation.fps.max(1);
        let delay_ms = 1000 / fps;
        let delay = Delay::from_numer_denom_ms(delay_ms, 1);

        for anim_frame in &animation.frames {
            let canvas = anim_frame.document.composite_preview();
            let img = RgbaImage::from_raw(canvas.width(), canvas.height(), canvas.pixels().to_vec())?;
            let frame = Frame::from_parts(img, 0, 0, delay);
            encoder.encode_frame(frame).ok()?;
        }
    }

    Some(buffer)
}
