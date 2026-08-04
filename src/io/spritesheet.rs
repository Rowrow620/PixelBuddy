use crate::document::AnimationManager;
use image::RgbaImage;
use std::io::Cursor;

pub fn export_spritesheet_png(animation: &AnimationManager) -> Option<Vec<u8>> {
    if animation.frames.is_empty() {
        return None;
    }

    let frame_count = animation.frames.len() as u32;
    let frame_w = animation.frames[0].document.width;
    let frame_h = animation.frames[0].document.height;

    let sheet_w = frame_w * frame_count;
    let sheet_h = frame_h;

    let mut sheet_img = RgbaImage::new(sheet_w, sheet_h);

    for (idx, anim_frame) in animation.frames.iter().enumerate() {
        let canvas = anim_frame.document.composite_preview();
        let offset_x = idx as u32 * frame_w;

        for y in 0..frame_h {
            for x in 0..frame_w {
                let pixel = canvas.get_pixel(x, y);
                sheet_img.put_pixel(offset_x + x, y, image::Rgba(pixel));
            }
        }
    }

    let mut buffer = Vec::new();
    sheet_img.write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png).ok()?;
    Some(buffer)
}
