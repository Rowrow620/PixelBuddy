use super::*;

impl PixelBuddyApp {
    pub fn update_texture(&mut self, ctx: &egui::Context) {
        if self.texture_dirty || self.canvas_texture.is_none() {
            let document = self
                .active_effect
                .as_ref()
                .and_then(|effect| effect.preview_document.as_deref())
                .unwrap_or_else(|| self.editor.document());
            let canvas = document.composite_preview();
            let size = [canvas.width() as usize, canvas.height() as usize];
            let image = ColorImage::from_rgba_unmultiplied(size, canvas.pixels());
            let options = TextureOptions {
                magnification: TextureFilter::Nearest,
                minification: TextureFilter::Nearest,
                ..Default::default()
            };
            if let Some(texture) = &mut self.canvas_texture {
                texture.set(image, options);
            } else {
                self.canvas_texture = Some(ctx.load_texture("canvas", image, options));
            }

            // Timeline previews are generated lazily at a fixed 24×24 size.
            // Do not mirror the full canvas into one GPU texture per frame.
            if self.frame_thumbnails.len() != self.editor.animation.frames.len() {
                self.frame_thumbnails
                    .resize(self.editor.animation.frames.len(), None);
            }

            self.texture_dirty = false;
        }
    }

    /// Returns a repeating 2×2 checkerboard texture. Rendering this as one
    /// tiled image avoids issuing one paint primitive per canvas pixel.
    pub fn checkerboard_texture_id(&mut self, ctx: &egui::Context) -> egui::TextureId {
        if self.checkerboard_texture.is_none() {
            let mut image = ColorImage::new([2, 2], egui::Color32::from_gray(210));
            image.pixels = vec![
                egui::Color32::from_gray(210),
                egui::Color32::from_gray(170),
                egui::Color32::from_gray(170),
                egui::Color32::from_gray(210),
            ];
            self.checkerboard_texture = Some(ctx.load_texture(
                "pixelbuddy_checkerboard",
                image,
                TextureOptions::NEAREST_REPEAT,
            ));
        }

        self.checkerboard_texture
            .as_ref()
            .expect("checkerboard texture is initialized above")
            .id()
    }

    /// Returns cached texture IDs for the neighboring animation frames.
    ///
    /// The current document is never an onion-skin source, so edits made to it
    /// do not invalidate these textures. A frame switch changes the pair and
    /// refreshes exactly the two needed composites.
    pub fn onion_texture_ids(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<(egui::TextureId, egui::TextureId)> {
        if !self.editor.animation.onion_skin_enabled || self.editor.animation.frames.len() <= 1 {
            return None;
        }

        let current = self.editor.animation.current_frame_index;
        let frame_count = self.editor.animation.frames.len();
        let previous = if current == 0 {
            frame_count - 1
        } else {
            current - 1
        };
        let next = (current + 1) % frame_count;
        let pair = (previous, next);

        if self.onion_texture_pair != Some(pair)
            || self.onion_previous_texture.is_none()
            || self.onion_next_texture.is_none()
        {
            let previous_canvas = self.editor.animation.frames[previous]
                .document
                .composite_preview();
            let next_canvas = self.editor.animation.frames[next]
                .document
                .composite_preview();
            let previous_image = ColorImage::from_rgba_unmultiplied(
                [
                    previous_canvas.width() as usize,
                    previous_canvas.height() as usize,
                ],
                previous_canvas.pixels(),
            );
            let next_image = ColorImage::from_rgba_unmultiplied(
                [next_canvas.width() as usize, next_canvas.height() as usize],
                next_canvas.pixels(),
            );

            if let Some(texture) = &mut self.onion_previous_texture {
                texture.set(previous_image, TextureOptions::NEAREST);
            } else {
                self.onion_previous_texture = Some(ctx.load_texture(
                    "pixelbuddy_onion_previous",
                    previous_image,
                    TextureOptions::NEAREST,
                ));
            }
            if let Some(texture) = &mut self.onion_next_texture {
                texture.set(next_image, TextureOptions::NEAREST);
            } else {
                self.onion_next_texture = Some(ctx.load_texture(
                    "pixelbuddy_onion_next",
                    next_image,
                    TextureOptions::NEAREST,
                ));
            }
            self.onion_texture_pair = Some(pair);
        }

        Some((
            self.onion_previous_texture
                .as_ref()
                .expect("onion texture is initialized above")
                .id(),
            self.onion_next_texture
                .as_ref()
                .expect("onion texture is initialized above")
                .id(),
        ))
    }

    /// Makes the next onion-skin draw rebuild its neighboring-frame textures.
    ///
    /// Frame insertion, deletion, and reordering can change the artwork at
    /// the same pair of indices, so simply marking the main canvas texture
    /// dirty is not sufficient for onion skinning.
    pub fn invalidate_onion_skin_cache(&mut self) {
        self.onion_texture_pair = None;
    }
}
