use crate::document::Document;
use image::{ImageBuffer, RgbaImage};

pub fn export_document_to_png(document: &Document) -> Option<Vec<u8>> {
    let canvas = document.composite_preview();
    let img: RgbaImage = ImageBuffer::from_raw(canvas.width(), canvas.height(), canvas.pixels().to_vec())?;
    
    let mut cursor = std::io::Cursor::new(Vec::new());
    img.write_to(&mut cursor, image::ImageFormat::Png).ok()?;
    Some(cursor.into_inner())
}

pub fn import_png_to_document(data: &[u8]) -> Option<Document> {
    let img = image::load_from_memory_with_format(data, image::ImageFormat::Png).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    
    let mut doc = Document::new(width, height);
    // Replace the first layer's canvas pixels
    let active = doc.active_layer_mut();
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            active.canvas.set_pixel(x, y, [pixel[0], pixel[1], pixel[2], pixel[3]]);
        }
    }
    
    Some(doc)
}
