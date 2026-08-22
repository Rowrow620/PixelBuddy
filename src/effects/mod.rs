use std::collections::HashMap;

use crate::document::Document;
use crate::editor::EditorState;

const EFFECT_PREVIEW_INTERVAL_SECONDS: f64 = 1.0 / 30.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EffectType {
    AdjustColor,
    Offset,
    Mirror,
    Rotate,
    InvertColors,
    Desaturation,
    Posterize,
}

pub struct ActiveEffectState {
    pub effect_type: EffectType,
    pub original_document: Box<Document>,
    pub preview_document: Option<Box<Document>>,

    // Effect-specific parameters
    pub hue_shift: f32,  // -180 to 180
    pub saturation: f32, // -100 to 100
    pub value: f32,      // -100 to 100

    pub offset_x: i32,
    pub offset_y: i32,

    pub mirror_horizontal: bool,
    pub mirror_vertical: bool,

    pub rotate_angle: f32,
    pub posterize_levels: u8,
    preview_dirty: bool,
    last_preview_refresh_at: f64,
}

impl ActiveEffectState {
    pub fn new(effect_type: EffectType, editor: &EditorState) -> Self {
        let original_document = Box::new(editor.document().clone());
        let preview_document = original_document.clone();
        Self {
            effect_type,
            original_document,
            preview_document: Some(preview_document),
            hue_shift: 0.0,
            saturation: 0.0,
            value: 0.0,
            offset_x: 0,
            offset_y: 0,
            mirror_horizontal: false,
            mirror_vertical: false,
            rotate_angle: 0.0,
            posterize_levels: 4,
            preview_dirty: false,
            last_preview_refresh_at: 0.0,
        }
    }

    pub fn refresh_preview(&mut self, selection: &crate::editor::selection::Selection) {
        let mut preview = self
            .preview_document
            .take()
            .unwrap_or_else(|| self.original_document.clone());

        let active_idx = self.original_document.active_layer_index;
        let reusable = preview.width == self.original_document.width
            && preview.height == self.original_document.height
            && preview.layers.len() == self.original_document.layers.len()
            && active_idx < preview.layers.len();
        if reusable {
            let source_pixels = self.original_document.layers[active_idx].canvas.pixels();
            let destination_pixels = preview.layers[active_idx].canvas.pixels_mut();
            if destination_pixels.len() == source_pixels.len() {
                destination_pixels.copy_from_slice(source_pixels);
            } else {
                preview.clone_from(&self.original_document);
            }
        } else {
            preview.clone_from(&self.original_document);
        }

        self.apply(&mut preview, selection);
        self.preview_document = Some(preview);
    }

    fn refresh_if_dirty(&mut self, selection: &crate::editor::selection::Selection) -> bool {
        if !self.preview_dirty {
            return false;
        }

        self.refresh_preview(selection);
        self.preview_dirty = false;
        true
    }

    fn apply(&self, target: &mut Document, selection: &crate::editor::selection::Selection) {
        let width = target.width;
        let height = target.height;
        let active_idx = target.active_layer_index;

        if active_idx >= target.layers.len() {
            return;
        }

        // For simplicity, we apply to the active layer.
        let src_layer = &self.original_document.layers[active_idx];
        let dst_layer = &mut target.layers[active_idx];

        match self.effect_type {
            EffectType::AdjustColor => {
                const MAX_CACHED_COLORS: usize = 4_096;
                let mut adjusted_colors = HashMap::<[u8; 3], [u8; 3]>::with_capacity(256);

                for y in 0..height {
                    for x in 0..width {
                        if selection.active && !selection.contains(x as i32, y as i32) {
                            continue;
                        }
                        let p = src_layer.canvas.get_pixel(x, y);
                        if p[3] == 0 {
                            continue;
                        }

                        let source_rgb = [p[0], p[1], p[2]];
                        let adjusted_rgb = if let Some(adjusted) = adjusted_colors.get(&source_rgb)
                        {
                            *adjusted
                        } else {
                            let rgb = palette::Srgb::new(
                                p[0] as f32 / 255.0,
                                p[1] as f32 / 255.0,
                                p[2] as f32 / 255.0,
                            );
                            use palette::IntoColor;
                            let hsv: palette::Hsv = rgb.into_color();

                            let new_h = (hsv.hue.into_positive_degrees() + self.hue_shift)
                                .rem_euclid(360.0);
                            let new_s =
                                (hsv.saturation + (self.saturation / 100.0)).clamp(0.0, 1.0);
                            let new_v = (hsv.value + (self.value / 100.0)).clamp(0.0, 1.0);

                            let new_hsv = palette::Hsv::new(new_h, new_s, new_v);
                            let new_rgb: palette::Srgb = new_hsv.into_color();
                            let adjusted = [
                                (new_rgb.red * 255.0).round() as u8,
                                (new_rgb.green * 255.0).round() as u8,
                                (new_rgb.blue * 255.0).round() as u8,
                            ];
                            if adjusted_colors.len() < MAX_CACHED_COLORS {
                                adjusted_colors.insert(source_rgb, adjusted);
                            }
                            adjusted
                        };

                        dst_layer.canvas.set_pixel(
                            x,
                            y,
                            [adjusted_rgb[0], adjusted_rgb[1], adjusted_rgb[2], p[3]],
                        );
                    }
                }
            }
            EffectType::Offset => {
                dst_layer.canvas.clear([0, 0, 0, 0]);
                if !selection.active {
                    offset_canvas_wrapped(
                        &src_layer.canvas,
                        &mut dst_layer.canvas,
                        self.offset_x,
                        self.offset_y,
                    );
                    return;
                }
                for y in 0..height {
                    for x in 0..width {
                        if selection.active && !selection.contains(x as i32, y as i32) {
                            if selection.active {
                                dst_layer
                                    .canvas
                                    .set_pixel(x, y, src_layer.canvas.get_pixel(x, y));
                            }
                            continue;
                        }

                        let mut src_x = x as i32 - self.offset_x;
                        let mut src_y = y as i32 - self.offset_y;

                        src_x = src_x.rem_euclid(width as i32);
                        src_y = src_y.rem_euclid(height as i32);

                        dst_layer.canvas.set_pixel(
                            x,
                            y,
                            src_layer.canvas.get_pixel(src_x as u32, src_y as u32),
                        );
                    }
                }
            }
            EffectType::Mirror => {
                dst_layer.canvas.clear([0, 0, 0, 0]);
                for y in 0..height {
                    for x in 0..width {
                        if selection.active && !selection.contains(x as i32, y as i32) {
                            dst_layer
                                .canvas
                                .set_pixel(x, y, src_layer.canvas.get_pixel(x, y));
                            continue;
                        }

                        let mut src_x = x;
                        let mut src_y = y;

                        if self.mirror_horizontal {
                            src_x = width - 1 - x;
                        }
                        if self.mirror_vertical {
                            src_y = height - 1 - y;
                        }

                        dst_layer
                            .canvas
                            .set_pixel(x, y, src_layer.canvas.get_pixel(src_x, src_y));
                    }
                }
            }
            EffectType::Rotate => {
                dst_layer.canvas.clear([0, 0, 0, 0]);
                for y in 0..height {
                    for x in 0..width {
                        if selection.active && !selection.contains(x as i32, y as i32) {
                            dst_layer
                                .canvas
                                .set_pixel(x, y, src_layer.canvas.get_pixel(x, y));
                            continue;
                        }

                        if let Some((src_x, src_y)) =
                            rotated_source_coordinate(x, y, width, height, self.rotate_angle)
                        {
                            dst_layer.canvas.set_pixel(
                                x,
                                y,
                                src_layer.canvas.get_pixel(src_x, src_y),
                            );
                        }
                    }
                }
            }
            EffectType::InvertColors => {
                for y in 0..height {
                    for x in 0..width {
                        if selection.active && !selection.contains(x as i32, y as i32) {
                            continue;
                        }
                        let p = src_layer.canvas.get_pixel(x, y);
                        if p[3] > 0 {
                            dst_layer.canvas.set_pixel(
                                x,
                                y,
                                [255 - p[0], 255 - p[1], 255 - p[2], p[3]],
                            );
                        }
                    }
                }
            }
            EffectType::Desaturation => {
                for y in 0..height {
                    for x in 0..width {
                        if selection.active && !selection.contains(x as i32, y as i32) {
                            continue;
                        }
                        let p = src_layer.canvas.get_pixel(x, y);
                        if p[3] > 0 {
                            let l = (0.299 * p[0] as f32
                                + 0.587 * p[1] as f32
                                + 0.114 * p[2] as f32) as u8;
                            dst_layer.canvas.set_pixel(x, y, [l, l, l, p[3]]);
                        }
                    }
                }
            }
            EffectType::Posterize => {
                let levels = self.posterize_levels.max(2) as f32;
                let step = 255.0 / (levels - 1.0);
                for y in 0..height {
                    for x in 0..width {
                        if selection.active && !selection.contains(x as i32, y as i32) {
                            continue;
                        }
                        let p = src_layer.canvas.get_pixel(x, y);
                        if p[3] > 0 {
                            let r = ((p[0] as f32 / step).round() * step) as u8;
                            let g = ((p[1] as f32 / step).round() * step) as u8;
                            let b = ((p[2] as f32 / step).round() * step) as u8;
                            dst_layer.canvas.set_pixel(x, y, [r, g, b, p[3]]);
                        }
                    }
                }
            }
        }
    }
}

fn offset_canvas_wrapped(
    source: &crate::document::Canvas,
    destination: &mut crate::document::Canvas,
    offset_x: i32,
    offset_y: i32,
) {
    debug_assert_eq!(source.width(), destination.width());
    debug_assert_eq!(source.height(), destination.height());

    let width = source.width() as usize;
    let height = source.height() as usize;
    let row_bytes = width * 4;
    let horizontal_shift = offset_x.rem_euclid(width as i32) as usize;
    let leading_bytes = horizontal_shift * 4;
    let source_split = (width - horizontal_shift) * 4;

    for destination_y in 0..height {
        let source_y = (destination_y as i32 - offset_y).rem_euclid(height as i32) as usize;
        let source_start = source_y * row_bytes;
        let source_row = &source.pixels()[source_start..source_start + row_bytes];
        let destination_start = destination_y * row_bytes;
        let destination_row =
            &mut destination.pixels_mut()[destination_start..destination_start + row_bytes];

        if horizontal_shift == 0 {
            destination_row.copy_from_slice(source_row);
        } else {
            destination_row[..leading_bytes].copy_from_slice(&source_row[source_split..]);
            destination_row[leading_bytes..].copy_from_slice(&source_row[..source_split]);
        }
    }
}

fn rotated_source_coordinate(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    clockwise_degrees: f32,
) -> Option<(u32, u32)> {
    if width == 0 || height == 0 || !clockwise_degrees.is_finite() {
        return None;
    }

    let radians = clockwise_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let center_x = (width.saturating_sub(1)) as f32 / 2.0;
    let center_y = (height.saturating_sub(1)) as f32 / 2.0;
    let destination_x = x as f32 - center_x;
    let destination_y = y as f32 - center_y;

    // Inverse-map the destination pixel into the source so the rotation has
    // no holes. Nearest-neighbor sampling keeps pixel-art edges crisp.
    let source_x = cos * destination_x + sin * destination_y + center_x;
    let source_y = -sin * destination_x + cos * destination_y + center_y;
    let source_x = source_x.round() as i64;
    let source_y = source_y.round() as i64;

    (source_x >= 0 && source_x < i64::from(width) && source_y >= 0 && source_y < i64::from(height))
        .then_some((source_x as u32, source_y as u32))
}

fn effect_preview_size(document_size: egui::Vec2, maximum_size: egui::Vec2) -> egui::Vec2 {
    if document_size.x <= 0.0
        || document_size.y <= 0.0
        || maximum_size.x <= 0.0
        || maximum_size.y <= 0.0
    {
        return egui::Vec2::ZERO;
    }

    let scale = (maximum_size.x / document_size.x)
        .min(maximum_size.y / document_size.y)
        .max(0.001);
    document_size * scale
}

fn show_effect_preview(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    document_size: egui::Vec2,
) {
    ui.label(egui::RichText::new("Preview").strong());
    let available_width = ui.available_width().clamp(180.0, 320.0);
    let preview_size = effect_preview_size(document_size, egui::vec2(available_width, 200.0));

    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        ui.set_min_size(preview_size);
        if let Some(texture) = texture {
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(preview_size)
                    .texture_options(egui::TextureOptions::NEAREST),
            );
        } else {
            ui.centered_and_justified(|ui| {
                ui.spinner();
            });
        }
    });
    ui.add_space(6.0);
}

fn rotate_preset_button(ui: &mut egui::Ui, angle: &mut f32, preset: f32, label: &str) -> bool {
    let selected = (*angle - preset).abs() < 0.05;
    let visuals = ui.visuals();
    let text_color = if selected {
        egui::Color32::WHITE
    } else {
        visuals.text_color()
    };
    let mut button = egui::Button::new(egui::RichText::new(label).color(text_color));
    if selected {
        button = button
            .fill(visuals.selection.bg_fill)
            .stroke(visuals.selection.stroke);
    }

    if ui.add(button).clicked() {
        *angle = preset;
        true
    } else {
        false
    }
}

fn effect_preview_refresh_due(last_refresh_at: f64, now: f64, pointer_active: bool) -> bool {
    !pointer_active
        || !last_refresh_at.is_finite()
        || !now.is_finite()
        || now < last_refresh_at
        || now - last_refresh_at >= EFFECT_PREVIEW_INTERVAL_SECONDS
}

pub fn show_effect_modal(ctx: &egui::Context, app: &mut crate::app::PixelBuddyApp) {
    let preview_texture = app.canvas_texture.clone();
    let preview_document_size = egui::vec2(
        app.editor.document().width as f32,
        app.editor.document().height as f32,
    );
    let mut effect_to_apply = false;
    let mut effect_to_cancel = false;
    let mut effect_changed = false;

    if let Some(effect) = &mut app.active_effect {
        let mut open = true;
        let title = match effect.effect_type {
            EffectType::AdjustColor => "Adjust Color",
            EffectType::Offset => "Offset Image",
            EffectType::Mirror => "Mirror Image",
            EffectType::Rotate => "Rotate Image",
            EffectType::InvertColors => "Invert Colors",
            EffectType::Desaturation => "Desaturation",
            EffectType::Posterize => "Posterize",
        };

        egui::Window::new(title)
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                show_effect_preview(ui, preview_texture.as_ref(), preview_document_size);

                match effect.effect_type {
                    EffectType::AdjustColor => {
                        if ui
                            .add(
                                egui::Slider::new(&mut effect.hue_shift, -180.0..=180.0)
                                    .text("Hue Shift"),
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut effect.saturation, -100.0..=100.0)
                                    .text("Saturation"),
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut effect.value, -100.0..=100.0)
                                    .text("Brightness/Value"),
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui.button("Reset").clicked() {
                            effect.hue_shift = 0.0;
                            effect.saturation = 0.0;
                            effect.value = 0.0;
                            effect_changed = true;
                        }
                    }
                    EffectType::Offset => {
                        if ui
                            .add(
                                egui::Slider::new(&mut effect.offset_x, -100..=100)
                                    .text("Offset X"),
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut effect.offset_y, -100..=100)
                                    .text("Offset Y"),
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui.button("Reset").clicked() {
                            effect.offset_x = 0;
                            effect.offset_y = 0;
                            effect_changed = true;
                        }
                    }
                    EffectType::Mirror => {
                        if ui
                            .checkbox(&mut effect.mirror_horizontal, "Horizontal")
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .checkbox(&mut effect.mirror_vertical, "Vertical")
                            .changed()
                        {
                            effect_changed = true;
                        }
                    }
                    EffectType::Rotate => {
                        if ui
                            .add(
                                egui::Slider::new(&mut effect.rotate_angle, -180.0..=180.0)
                                    .text("Angle")
                                    .suffix("°")
                                    .fixed_decimals(1),
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }

                        ui.label("Presets");
                        ui.horizontal(|ui| {
                            for (preset, label) in
                                [(0.0, "0°"), (90.0, "90°"), (180.0, "180°"), (-90.0, "270°")]
                            {
                                effect_changed |= rotate_preset_button(
                                    ui,
                                    &mut effect.rotate_angle,
                                    preset,
                                    label,
                                );
                            }
                        });
                    }
                    EffectType::InvertColors => {
                        ui.label("Invert Colors: Previews immediately.");
                    }
                    EffectType::Desaturation => {
                        ui.label("Desaturate: Converts to grayscale based on luminance.");
                    }
                    EffectType::Posterize => {
                        if ui
                            .add(
                                egui::Slider::new(&mut effect.posterize_levels, 2..=32)
                                    .text("Levels"),
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        effect_to_apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        effect_to_cancel = true;
                    }
                });
            });

        if !open {
            effect_to_cancel = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            effect_to_cancel = true;
        }
    }

    if effect_changed {
        if let Some(effect) = &mut app.active_effect {
            effect.preview_dirty = true;
        }
    }

    if !effect_to_apply && !effect_to_cancel {
        let now = ctx.input(|input| input.time);
        let pointer_active = ctx.input(|input| input.pointer.any_down());
        if let Some(effect) = &mut app.active_effect {
            if effect.preview_dirty
                && effect_preview_refresh_due(effect.last_preview_refresh_at, now, pointer_active)
            {
                let selection = app.editor.selection.clone();
                effect.refresh_if_dirty(&selection);
                effect.last_preview_refresh_at = now;
                app.texture_dirty = true;
            } else if effect.preview_dirty {
                let remaining = (EFFECT_PREVIEW_INTERVAL_SECONDS
                    - (now - effect.last_preview_refresh_at))
                    .max(0.001);
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining));
            }
        }
    }

    if effect_to_apply {
        let selection = app.editor.selection.clone();
        if let Some(effect) = &mut app.active_effect {
            if effect.refresh_if_dirty(&selection) {
                app.texture_dirty = true;
            }
        }

        if let Some(effect) = app.active_effect.take() {
            let title = match effect.effect_type {
                EffectType::AdjustColor => "Adjust Color",
                EffectType::Offset => "Offset",
                EffectType::Mirror => "Mirror",
                EffectType::Rotate => "Rotate",
                EffectType::InvertColors => "Invert Colors",
                EffectType::Desaturation => "Desaturation",
                EffectType::Posterize => "Posterize",
            };

            if let Some(preview) = effect.preview_document {
                app.editor.mutate_document(title, move |document| {
                    document.clone_from(&preview);
                    true
                });
            } else {
                debug_assert!(false, "an active effect must retain its preview document");
                return;
            }

            app.texture_dirty = true;
            use crate::app::EditEffects;
            app.consume_edit_effects(EditEffects::current_frame_artwork(true));
        }
    }

    if effect_to_cancel {
        if app.active_effect.take().is_some() {
            app.texture_dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        effect_preview_refresh_due, effect_preview_size, offset_canvas_wrapped,
        rotated_source_coordinate, ActiveEffectState, EffectType,
    };

    #[test]
    fn rotation_sampling_supports_presets_and_arbitrary_angles() {
        assert_eq!(rotated_source_coordinate(0, 0, 4, 4, 0.0), Some((0, 0)));
        assert_eq!(rotated_source_coordinate(0, 0, 4, 4, 90.0), Some((0, 3)));
        assert_eq!(rotated_source_coordinate(0, 0, 4, 4, -90.0), Some((3, 0)));

        // The center remains stable for a non-preset angle, while pixels that
        // inverse-map outside the fixed canvas are left transparent.
        assert_eq!(rotated_source_coordinate(2, 1, 5, 3, 37.0), Some((2, 1)));
        assert_eq!(rotated_source_coordinate(4, 0, 5, 3, 45.0), None);
    }

    #[test]
    fn effect_preview_preserves_aspect_ratio_within_its_bounds() {
        let landscape = effect_preview_size(egui::vec2(64.0, 32.0), egui::vec2(320.0, 200.0));
        assert_eq!(landscape, egui::vec2(320.0, 160.0));

        let portrait = effect_preview_size(egui::vec2(32.0, 64.0), egui::vec2(320.0, 200.0));
        assert_eq!(portrait, egui::vec2(100.0, 200.0));

        assert_eq!(
            effect_preview_size(egui::Vec2::ZERO, egui::vec2(320.0, 200.0)),
            egui::Vec2::ZERO
        );
    }

    #[test]
    fn wrapped_offset_fast_path_matches_pixel_mapping() {
        let mut source = crate::document::Canvas::new(3, 2);
        for (index, value) in [1, 2, 3, 4, 5, 6].into_iter().enumerate() {
            source.set_pixel((index % 3) as u32, (index / 3) as u32, [value, 0, 0, 255]);
        }
        let mut destination = crate::document::Canvas::new(3, 2);

        offset_canvas_wrapped(&source, &mut destination, 1, 1);

        let red_channels: Vec<u8> = destination
            .pixels()
            .chunks_exact(4)
            .map(|pixel| pixel[0])
            .collect();
        assert_eq!(red_channels, vec![6, 4, 5, 3, 1, 2]);

        offset_canvas_wrapped(&source, &mut destination, -1, -1);
        let red_channels: Vec<u8> = destination
            .pixels()
            .chunks_exact(4)
            .map(|pixel| pixel[0])
            .collect();
        assert_eq!(red_channels, vec![5, 6, 4, 2, 3, 1]);
    }

    #[test]
    fn preview_refresh_reuses_the_active_layer_pixel_buffer() {
        let mut editor = crate::editor::EditorState::new(16, 16);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [120, 80, 40, 255]);
        let selection = crate::editor::selection::Selection::new();
        let mut effect = ActiveEffectState::new(EffectType::AdjustColor, &editor);
        let pointer_before = effect
            .preview_document
            .as_ref()
            .expect("new effects have a preview")
            .active_layer()
            .canvas
            .pixels()
            .as_ptr();

        effect.hue_shift = 45.0;
        effect.refresh_preview(&selection);

        let preview = effect
            .preview_document
            .as_ref()
            .expect("refresh restores the preview");
        assert_eq!(
            preview.active_layer().canvas.pixels().as_ptr(),
            pointer_before,
            "parameter changes should reuse the canvas allocation"
        );
        assert_ne!(
            preview.active_layer().canvas.get_pixel(0, 0),
            [120, 80, 40, 255]
        );
    }

    #[test]
    fn repeated_adjust_color_pixels_produce_identical_cached_results() {
        let mut editor = crate::editor::EditorState::new(2, 1);
        editor.document_mut().active_layer_mut().canvas.pixels_mut()[..8]
            .copy_from_slice(&[80, 120, 160, 255, 80, 120, 160, 128]);
        let mut effect = ActiveEffectState::new(EffectType::AdjustColor, &editor);
        effect.saturation = 20.0;
        effect.value = -10.0;
        effect.refresh_preview(&crate::editor::selection::Selection::new());
        let pixels = effect
            .preview_document
            .as_ref()
            .expect("refresh creates a preview")
            .active_layer()
            .canvas
            .pixels();

        assert_eq!(&pixels[..3], &pixels[4..7]);
        assert_eq!(pixels[3], 255);
        assert_eq!(pixels[7], 128);
    }

    #[test]
    fn dirty_preview_is_refreshed_exactly_once_before_commit() {
        let mut editor = crate::editor::EditorState::new(1, 1);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [80, 120, 160, 255]);
        let selection = editor.selection.clone();
        let mut effect = ActiveEffectState::new(EffectType::AdjustColor, &editor);
        effect.value = 20.0;
        effect.preview_dirty = true;

        assert!(effect.refresh_if_dirty(&selection));
        assert!(!effect.preview_dirty);
        assert_ne!(
            effect
                .preview_document
                .as_ref()
                .expect("dirty refresh creates the final preview")
                .active_layer()
                .canvas
                .get_pixel(0, 0),
            [80, 120, 160, 255]
        );
        assert!(!effect.refresh_if_dirty(&selection));
    }

    #[test]
    fn active_pointer_previews_are_capped_but_final_changes_refresh_immediately() {
        assert!(!effect_preview_refresh_due(1.0, 1.02, true));
        assert!(effect_preview_refresh_due(1.0, 1.04, true));
        assert!(effect_preview_refresh_due(1.0, 1.001, false));
        assert!(effect_preview_refresh_due(2.0, 1.0, true));
    }
}
