use std::collections::HashMap;

use crate::document::Document;
use crate::editor::EditorState;

pub mod gradient;

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
    Palettize,
    Outline,
    DropShadow,
    Pixelize,
    GradientFill,
    GradientMap,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectTarget {
    ActiveLayer,
    Selection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EffectRegion {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

impl EffectRegion {
    fn for_target(
        width: u32,
        height: u32,
        target: EffectTarget,
        selection: &crate::editor::selection::Selection,
    ) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        if target == EffectTarget::ActiveLayer {
            return Some(Self {
                min_x: 0,
                min_y: 0,
                max_x: width,
                max_y: height,
            });
        }
        if !selection.active {
            return None;
        }

        let min_x = selection.min_x().max(0) as u32;
        let min_y = selection.min_y().max(0) as u32;
        let max_x = (selection.max_x().saturating_add(1)).max(0) as u32;
        let max_y = (selection.max_y().saturating_add(1)).max(0) as u32;
        let region = Self {
            min_x: min_x.min(width),
            min_y: min_y.min(height),
            max_x: max_x.min(width),
            max_y: max_y.min(height),
        };
        (region.min_x < region.max_x && region.min_y < region.max_y).then_some(region)
    }

    fn contains(self, x: u32, y: u32) -> bool {
        x >= self.min_x && x < self.max_x && y >= self.min_y && y < self.max_y
    }

    fn width(self) -> u32 {
        self.max_x - self.min_x
    }

    fn height(self) -> u32 {
        self.max_y - self.min_y
    }
}

pub struct ActiveEffectState {
    pub effect_type: EffectType,
    pub original_document: Box<Document>,
    pub target: EffectTarget,
    pub preview_document: Option<Box<Document>>,
    source_document_session_id: u64,
    source_active_frame_generation: u64,
    source_revision: u64,
    source_frame_index: usize,
    source_active_layer_index: usize,
    source_selection: crate::editor::selection::Selection,

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
    pub palettize_policy: crate::app::PalettePolicy,
    pub outline_color: [u8; 4],
    pub outline_thickness: u32,
    pub drop_shadow_color: [u8; 4],
    pub drop_shadow_offset_x: i32,
    pub drop_shadow_offset_y: i32,
    pub drop_shadow_opacity: f32,
    pub pixelize_size: u32,
    pub gradient: crate::effects::gradient::GradientState,
    preview_dirty: bool,
    last_preview_refresh_at: f64,
}

impl ActiveEffectState {
    pub fn new(
        effect_type: EffectType,
        editor: &EditorState,
        document_session_id: u64,
        active_frame_generation: u64,
    ) -> Self {
        let selection_available = EffectRegion::for_target(
            editor.document().width,
            editor.document().height,
            EffectTarget::Selection,
            &editor.selection,
        )
        .is_some();
        let target = if selection_available {
            EffectTarget::Selection
        } else {
            EffectTarget::ActiveLayer
        };
        let original_document = Box::new(editor.document().clone());
        let preview_document = original_document.clone();
        Self {
            target,
            effect_type,
            original_document,
            preview_document: Some(preview_document),
            source_document_session_id: document_session_id,
            source_active_frame_generation: active_frame_generation,
            source_revision: editor.revision(),
            source_frame_index: editor.animation.current_frame_index,
            source_active_layer_index: editor.document().active_layer_index,
            source_selection: editor.selection,
            hue_shift: 0.0,
            saturation: 0.0,
            value: 0.0,
            offset_x: 0,
            offset_y: 0,
            mirror_horizontal: false,
            mirror_vertical: false,
            rotate_angle: 0.0,
            posterize_levels: 4,
            palettize_policy: crate::app::PalettePolicy::KeepCurrent,
            outline_color: [0, 0, 0, 255],
            outline_thickness: 1,
            drop_shadow_color: [0, 0, 0, 255],
            drop_shadow_offset_x: 2,
            drop_shadow_offset_y: 2,
            drop_shadow_opacity: 0.5,
            pixelize_size: 4,
            gradient: {
                let mut g = crate::effects::gradient::GradientState::default();
                g.stops.clear();
                g.stops.push(crate::effects::gradient::GradientStop {
                    position: 0.0,
                    color: editor.primary_color,
                });
                g.stops.push(crate::effects::gradient::GradientStop {
                    position: 1.0,
                    color: editor.secondary_color,
                });
                g
            },
            preview_dirty: false,
            last_preview_refresh_at: 0.0,
        }
    }

    fn provenance_matches(&self, app: &crate::app::PixelBuddyApp) -> bool {
        self.source_document_session_id == app.document_session_id()
            && self.source_active_frame_generation == app.active_frame_generation()
            && self.source_revision == app.editor.revision()
            && self.source_frame_index == app.editor.animation.current_frame_index
            && self.source_active_layer_index == app.editor.document().active_layer_index
            && self.source_selection == app.editor.selection
    }

    fn target_layer_is_editable(&self, app: &crate::app::PixelBuddyApp) -> bool {
        app.editor
            .document()
            .layers
            .get(self.source_active_layer_index)
            .is_some_and(|layer| !layer.locked)
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

        let gradient_valid = !matches!(
            self.effect_type,
            EffectType::GradientFill | EffectType::GradientMap
        ) || self.gradient.prepare_for_preview();
        if gradient_valid {
            self.apply(&mut preview, selection);
        }
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
        let Some(region) = EffectRegion::for_target(width, height, self.target, selection) else {
            return;
        };

        // For simplicity, we apply to the active layer.
        let src_layer = &self.original_document.layers[active_idx];
        let dst_layer = &mut target.layers[active_idx];

        match self.effect_type {
            EffectType::AdjustColor => {
                const MAX_CACHED_COLORS: usize = 4_096;
                let mut adjusted_colors = HashMap::<[u8; 3], [u8; 3]>::with_capacity(256);

                for y in 0..height {
                    for x in 0..width {
                        if !region.contains(x, y) {
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
                if self.target == EffectTarget::ActiveLayer {
                    offset_canvas_wrapped(
                        &src_layer.canvas,
                        &mut dst_layer.canvas,
                        self.offset_x,
                        self.offset_y,
                    );
                    return;
                }
                for y in region.min_y..region.max_y {
                    for x in region.min_x..region.max_x {
                        let local_x = x - region.min_x;
                        let local_y = y - region.min_y;
                        let src_x = region.min_x
                            + (local_x as i32 - self.offset_x).rem_euclid(region.width() as i32)
                                as u32;
                        let src_y = region.min_y
                            + (local_y as i32 - self.offset_y).rem_euclid(region.height() as i32)
                                as u32;

                        dst_layer
                            .canvas
                            .set_pixel(x, y, src_layer.canvas.get_pixel(src_x, src_y));
                    }
                }
            }
            EffectType::Mirror => {
                for y in region.min_y..region.max_y {
                    for x in region.min_x..region.max_x {
                        let mut src_x = x;
                        let mut src_y = y;

                        if self.mirror_horizontal {
                            src_x = region.min_x + region.max_x - 1 - x;
                        }
                        if self.mirror_vertical {
                            src_y = region.min_y + region.max_y - 1 - y;
                        }

                        dst_layer
                            .canvas
                            .set_pixel(x, y, src_layer.canvas.get_pixel(src_x, src_y));
                    }
                }
            }
            EffectType::Rotate => {
                for y in region.min_y..region.max_y {
                    for x in region.min_x..region.max_x {
                        let source = rotated_source_coordinate(
                            x - region.min_x,
                            y - region.min_y,
                            region.width(),
                            region.height(),
                            self.rotate_angle,
                        );
                        if let Some((src_x, src_y)) = source {
                            dst_layer.canvas.set_pixel(
                                x,
                                y,
                                src_layer
                                    .canvas
                                    .get_pixel(region.min_x + src_x, region.min_y + src_y),
                            );
                        } else {
                            dst_layer.canvas.set_pixel(x, y, [0, 0, 0, 0]);
                        }
                    }
                }
            }
            EffectType::InvertColors => {
                for y in 0..height {
                    for x in 0..width {
                        if !region.contains(x, y) {
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
                        if !region.contains(x, y) {
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
                        if !region.contains(x, y) {
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
            EffectType::Palettize => {
                let target_palette = match &self.palettize_policy {
                    crate::app::PalettePolicy::KeepCurrent => {
                        self.original_document.palette.colors.clone()
                    }
                    crate::app::PalettePolicy::UseDefault => {
                        crate::document::palette_library::default_preset()
                            .to_palette()
                            .colors
                    }
                    crate::app::PalettePolicy::UsePreset(id) => {
                        crate::document::palette_library::get_preset(id)
                            .unwrap_or_else(crate::document::palette_library::default_preset)
                            .to_palette()
                            .colors
                    }
                };

                let target_palette = if target_palette.is_empty() {
                    crate::document::palette_library::default_preset()
                        .to_palette()
                        .colors
                } else {
                    target_palette
                };
                target.palette.colors = target_palette.clone();
                target.palette.selected_index = target
                    .palette
                    .selected_index
                    .min(target.palette.colors.len() - 1);

                for y in 0..height {
                    for x in 0..width {
                        if !region.contains(x, y) {
                            continue;
                        }
                        let p = src_layer.canvas.get_pixel(x, y);
                        if p[3] > 0 {
                            let mut best_color = p;
                            let mut best_dist = f32::MAX;
                            for &c in &target_palette {
                                let dr = p[0] as f32 - c[0] as f32;
                                let dg = p[1] as f32 - c[1] as f32;
                                let db = p[2] as f32 - c[2] as f32;
                                let dist = dr * dr + dg * dg + db * db;
                                if dist < best_dist {
                                    best_dist = dist;
                                    best_color = [c[0], c[1], c[2], p[3]];
                                }
                            }
                            dst_layer.canvas.set_pixel(x, y, best_color);
                        }
                    }
                }
            }
            EffectType::Outline => {
                let color = self.outline_color;
                let thickness = self.outline_thickness as i32;

                // Copy original pixels first
                for y in 0..height {
                    for x in 0..width {
                        if !region.contains(x, y) {
                            continue;
                        }
                        let p = src_layer.canvas.get_pixel(x, y);
                        dst_layer.canvas.set_pixel(x, y, p);
                    }
                }

                // Add outline
                if color[3] > 0 && thickness > 0 {
                    for y in region.min_y..region.max_y {
                        for x in region.min_x..region.max_x {
                            let p = src_layer.canvas.get_pixel(x, y);
                            if p[3] == 0 {
                                // Transparent pixel. Check if it's within 'thickness' distance of an opaque pixel.
                                // For an outline, we typically use a diamond pattern (Manhattan distance) or square pattern (Chebyshev).
                                // Let's use Chebyshev (square) for simple thick outlines, or Manhattan for pixel art outlines.
                                // A common pixel art outline is just 1px (Manhattan).
                                let mut is_edge = false;
                                'search: for dy in -thickness..=thickness {
                                    for dx in -thickness..=thickness {
                                        if dx.abs() + dy.abs() > thickness {
                                            continue; // Manhattan distance
                                        }
                                        let nx = x as i32 + dx;
                                        let ny = y as i32 + dy;
                                        if nx >= 0
                                            && ny >= 0
                                            && region.contains(nx as u32, ny as u32)
                                            && src_layer.canvas.get_pixel(nx as u32, ny as u32)[3]
                                                > 0
                                        {
                                            is_edge = true;
                                            break 'search;
                                        }
                                    }
                                }

                                if is_edge {
                                    dst_layer.canvas.set_pixel(x, y, color);
                                }
                            }
                        }
                    }
                }
            }
            EffectType::DropShadow => {
                let color = self.drop_shadow_color;
                let ox = self.drop_shadow_offset_x;
                let oy = self.drop_shadow_offset_y;
                let opacity = self.drop_shadow_opacity.clamp(0.0, 1.0);

                // We will composite the original on top of the shadow.
                for y in region.min_y..region.max_y {
                    for x in region.min_x..region.max_x {
                        let original = src_layer.canvas.get_pixel(x, y);

                        // Check if shadow falls here
                        let sx = x as i32 - ox;
                        let sy = y as i32 - oy;

                        let shadow_p =
                            if sx >= 0 && sy >= 0 && region.contains(sx as u32, sy as u32) {
                                src_layer.canvas.get_pixel(sx as u32, sy as u32)
                            } else {
                                [0, 0, 0, 0]
                            };

                        let shadow_alpha = ((shadow_p[3] as f32 / 255.0)
                            * (color[3] as f32 / 255.0)
                            * opacity
                            * 255.0)
                            .round() as u8;
                        let shadow = [color[0], color[1], color[2], shadow_alpha];
                        let composed = crate::document::Layer::blend_mode_apply(
                            shadow,
                            original,
                            crate::document::BlendMode::Normal,
                            1.0,
                        );
                        dst_layer.canvas.set_pixel(x, y, composed);
                    }
                }
            }
            EffectType::Pixelize => {
                let size = self.pixelize_size.max(1);
                if size == 1 {
                    for y in 0..height {
                        for x in 0..width {
                            if !region.contains(x, y) {
                                continue;
                            }
                            dst_layer
                                .canvas
                                .set_pixel(x, y, src_layer.canvas.get_pixel(x, y));
                        }
                    }
                } else {
                    for by in (region.min_y..region.max_y).step_by(size as usize) {
                        for bx in (region.min_x..region.max_x).step_by(size as usize) {
                            // Find the center pixel of this block to use as the color (or could average)
                            // We will just sample the top-left or center pixel that is opaque, or average.
                            // Average is better:
                            let mut premultiplied_r = 0u64;
                            let mut premultiplied_g = 0u64;
                            let mut premultiplied_b = 0u64;
                            let mut alpha_sum = 0u64;
                            let mut count = 0u64;

                            for y in by..std::cmp::min(by + size, region.max_y) {
                                for x in bx..std::cmp::min(bx + size, region.max_x) {
                                    let p = src_layer.canvas.get_pixel(x, y);
                                    let alpha = u64::from(p[3]);
                                    premultiplied_r += u64::from(p[0]) * alpha;
                                    premultiplied_g += u64::from(p[1]) * alpha;
                                    premultiplied_b += u64::from(p[2]) * alpha;
                                    alpha_sum += alpha;
                                    count += 1;
                                }
                            }

                            let avg = if alpha_sum > 0 && count > 0 {
                                [
                                    (premultiplied_r / alpha_sum) as u8,
                                    (premultiplied_g / alpha_sum) as u8,
                                    (premultiplied_b / alpha_sum) as u8,
                                    (alpha_sum / count) as u8,
                                ]
                            } else {
                                [0, 0, 0, 0]
                            };

                            for y in by..std::cmp::min(by + size, region.max_y) {
                                for x in bx..std::cmp::min(bx + size, region.max_x) {
                                    dst_layer.canvas.set_pixel(x, y, avg);
                                }
                            }
                        }
                    }
                }
            }
            EffectType::GradientFill | EffectType::GradientMap => {
                let extent_x = region.width().saturating_sub(1) as f32;
                let extent_y = region.height().saturating_sub(1) as f32;
                let radius_width = region.width() as f32;
                let radius_height = region.height() as f32;

                let is_fill = self.effect_type == EffectType::GradientFill;
                let shape = self.gradient.shape;
                let cx = self.gradient.radial_center[0] * extent_x;
                let cy = self.gradient.radial_center[1] * extent_y;
                let rx = self.gradient.radial_radius[0] * radius_width;
                let ry = self.gradient.radial_radius[1] * radius_height;

                let sx = self.gradient.linear_start[0] * extent_x;
                let sy = self.gradient.linear_start[1] * extent_y;
                let ex = self.gradient.linear_end[0] * extent_x;
                let ey = self.gradient.linear_end[1] * extent_y;
                let dx = ex - sx;
                let dy = ey - sy;
                let len_sq = dx * dx + dy * dy;

                let blend = self.gradient.blend_mode;

                for y in 0..height {
                    for x in 0..width {
                        if !region.contains(x, y) {
                            continue;
                        }

                        let px = (x - region.min_x) as f32;
                        let py = (y - region.min_y) as f32;
                        let original_pixel = src_layer.canvas.get_pixel(x, y);

                        // Map preserves alpha, skip fully transparent
                        if !is_fill && original_pixel[3] == 0 {
                            continue;
                        }

                        let mut t = 0.0;
                        if is_fill {
                            match shape {
                                crate::effects::gradient::GradientShape::Linear => {
                                    if len_sq > 0.0001 {
                                        let vx = px - sx;
                                        let vy = py - sy;
                                        t = (vx * dx + vy * dy) / len_sq;
                                    }
                                }
                                crate::effects::gradient::GradientShape::Radial => {
                                    if rx > 0.0001 && ry > 0.0001 {
                                        let nx = (px - cx) / rx;
                                        let ny = (py - cy) / ry;
                                        t = (nx * nx + ny * ny).sqrt();
                                    }
                                }
                            }
                        } else {
                            let lum = (0.299 * original_pixel[0] as f32
                                + 0.587 * original_pixel[1] as f32
                                + 0.114 * original_pixel[2] as f32)
                                / 255.0;
                            t = lum;
                        }

                        let mut dither_offset = 0.0;
                        match self.gradient.dithering {
                            crate::effects::gradient::GradientDithering::None => {}
                            crate::effects::gradient::GradientDithering::Bayer2x2 => {
                                let bayer = [[0.0, 0.5], [0.75, 0.25]];
                                dither_offset = bayer[(y % 2) as usize][(x % 2) as usize] - 0.375;
                            }
                            crate::effects::gradient::GradientDithering::Bayer4x4 => {
                                let bayer = [
                                    [0.0 / 16.0, 8.0 / 16.0, 2.0 / 16.0, 10.0 / 16.0],
                                    [12.0 / 16.0, 4.0 / 16.0, 14.0 / 16.0, 6.0 / 16.0],
                                    [3.0 / 16.0, 11.0 / 16.0, 1.0 / 16.0, 9.0 / 16.0],
                                    [15.0 / 16.0, 7.0 / 16.0, 13.0 / 16.0, 5.0 / 16.0],
                                ];
                                dither_offset = bayer[(y % 4) as usize][(x % 4) as usize] - 0.46875;
                            }
                        }
                        t += dither_offset * 0.05;

                        let sampled = self.gradient.sample_color(t);

                        let out_color = if is_fill {
                            match blend {
                                crate::effects::gradient::GradientBlendMode::Replace => sampled,
                                crate::effects::gradient::GradientBlendMode::AlphaBlend => {
                                    let sa = sampled[3] as f32 / 255.0;
                                    let oa = original_pixel[3] as f32 / 255.0;
                                    let a_out = sa + oa * (1.0 - sa);
                                    if a_out > 0.0 {
                                        let r = ((sampled[0] as f32 * sa)
                                            + (original_pixel[0] as f32 * oa * (1.0 - sa)))
                                            / a_out;
                                        let g = ((sampled[1] as f32 * sa)
                                            + (original_pixel[1] as f32 * oa * (1.0 - sa)))
                                            / a_out;
                                        let b = ((sampled[2] as f32 * sa)
                                            + (original_pixel[2] as f32 * oa * (1.0 - sa)))
                                            / a_out;
                                        [r as u8, g as u8, b as u8, (a_out * 255.0) as u8]
                                    } else {
                                        [0, 0, 0, 0]
                                    }
                                }
                            }
                        } else {
                            [sampled[0], sampled[1], sampled[2], original_pixel[3]]
                        };

                        dst_layer.canvas.set_pixel(x, y, out_color);
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
        let title = match effect.effect_type {
            EffectType::AdjustColor => "Adjust Color",
            EffectType::Offset => "Offset Image",
            EffectType::Mirror => "Mirror Image",
            EffectType::Rotate => "Rotate Image",
            EffectType::InvertColors => "Invert Colors",
            EffectType::Desaturation => "Desaturation",
            EffectType::Posterize => "Posterize",
            EffectType::Palettize => "Palettize",
            EffectType::Outline => "Outline",
            EffectType::DropShadow => "Drop Shadow",
            EffectType::Pixelize => "Pixelize",
            EffectType::GradientFill => "Gradient Fill",
            EffectType::GradientMap => "Gradient Map",
        };

        let modal = egui::Modal::new(egui::Id::new("pixelbuddy.effect_modal")).show(ctx, |ui| {
            ui.set_max_width(560.0);
            ui.heading(title);
            ui.add_space(8.0);
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
                        .add(egui::Slider::new(&mut effect.offset_x, -100..=100).text("Offset X"))
                        .changed()
                    {
                        effect_changed = true;
                    }
                    if ui
                        .add(egui::Slider::new(&mut effect.offset_y, -100..=100).text("Offset Y"))
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
                            effect_changed |=
                                rotate_preset_button(ui, &mut effect.rotate_angle, preset, label);
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
                        .add(egui::Slider::new(&mut effect.posterize_levels, 2..=32).text("Levels"))
                        .changed()
                    {
                        effect_changed = true;
                    }
                }
                EffectType::Palettize => {
                    ui.horizontal(|ui| {
                        ui.label("Palette:");
                        let previous_policy = effect.palettize_policy.clone();
                        egui::ComboBox::from_id_salt("palettize_policy")
                            .selected_text(match &effect.palettize_policy {
                                crate::app::PalettePolicy::KeepCurrent => {
                                    "Keep current palette".to_owned()
                                }
                                crate::app::PalettePolicy::UseDefault => {
                                    "Use default palette".to_owned()
                                }
                                crate::app::PalettePolicy::UsePreset(id) => {
                                    crate::document::palette_library::get_preset(id)
                                        .map(|p| p.name.to_owned())
                                        .unwrap_or_else(|| "Unknown preset".to_owned())
                                }
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut effect.palettize_policy,
                                    crate::app::PalettePolicy::KeepCurrent,
                                    "Keep current palette",
                                );
                                ui.selectable_value(
                                    &mut effect.palettize_policy,
                                    crate::app::PalettePolicy::UseDefault,
                                    "Use default palette",
                                );
                                for preset in crate::document::palette_library::PRESETS {
                                    ui.selectable_value(
                                        &mut effect.palettize_policy,
                                        crate::app::PalettePolicy::UsePreset(preset.id.to_string()),
                                        preset.name,
                                    );
                                }
                            });
                        if previous_policy != effect.palettize_policy {
                            effect_changed = true;
                        }
                    });
                }
                EffectType::Outline => {
                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        let mut c = [
                            effect.outline_color[0],
                            effect.outline_color[1],
                            effect.outline_color[2],
                        ];
                        if ui.color_edit_button_srgb(&mut c).changed() {
                            effect.outline_color = [c[0], c[1], c[2], 255];
                            effect_changed = true;
                        }
                    });
                    if ui
                        .add(
                            egui::Slider::new(&mut effect.outline_thickness, 1..=10)
                                .text("Thickness"),
                        )
                        .changed()
                    {
                        effect_changed = true;
                    }
                }
                EffectType::DropShadow => {
                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        let mut c = [
                            effect.drop_shadow_color[0],
                            effect.drop_shadow_color[1],
                            effect.drop_shadow_color[2],
                        ];
                        if ui.color_edit_button_srgb(&mut c).changed() {
                            effect.drop_shadow_color = [c[0], c[1], c[2], 255];
                            effect_changed = true;
                        }
                    });
                    if ui
                        .add(
                            egui::Slider::new(&mut effect.drop_shadow_offset_x, -50..=50)
                                .text("Offset X"),
                        )
                        .changed()
                    {
                        effect_changed = true;
                    }
                    if ui
                        .add(
                            egui::Slider::new(&mut effect.drop_shadow_offset_y, -50..=50)
                                .text("Offset Y"),
                        )
                        .changed()
                    {
                        effect_changed = true;
                    }
                    if ui
                        .add(
                            egui::Slider::new(&mut effect.drop_shadow_opacity, 0.0..=1.0)
                                .text("Opacity"),
                        )
                        .changed()
                    {
                        effect_changed = true;
                    }
                }
                EffectType::Pixelize => {
                    if ui
                        .add(
                            egui::Slider::new(&mut effect.pixelize_size, 1..=64).text("Block Size"),
                        )
                        .changed()
                    {
                        effect_changed = true;
                    }
                }
                EffectType::GradientFill | EffectType::GradientMap => {
                    let is_fill = effect.effect_type == EffectType::GradientFill;
                    ui.label("Color Ramp");

                    let mut to_remove = None;
                    let stops_len = effect.gradient.stops.len();

                    for (i, stop) in effect.gradient.stops.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let mut c = egui::Color32::from_rgba_unmultiplied(
                                stop.color[0],
                                stop.color[1],
                                stop.color[2],
                                stop.color[3],
                            );
                            if crate::ui::layers_panel::compact_color_picker_popup(
                                ui,
                                &format!("grad_stop_{i}"),
                                &mut c,
                            )
                            .changed()
                            {
                                stop.color = [c.r(), c.g(), c.b(), c.a()];
                                effect_changed = true;
                            }

                            if i == 0 || i + 1 == stops_len {
                                ui.label(format!("Pos {:.2}", stop.position));
                            } else if ui
                                .add(egui::Slider::new(&mut stop.position, 0.0..=1.0).text("Pos"))
                                .changed()
                            {
                                effect_changed = true;
                            }

                            if stops_len > 2
                                && i > 0
                                && i + 1 < stops_len
                                && ui.button("X").clicked()
                            {
                                to_remove = Some(i);
                            }
                        });
                    }

                    if let Some(i) = to_remove {
                        effect.gradient.stops.remove(i);
                        effect_changed = true;
                    }

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                effect.gradient.stops.len()
                                    < crate::effects::gradient::MAX_GRADIENT_STOPS,
                                egui::Button::new("Add Stop"),
                            )
                            .clicked()
                        {
                            effect
                                .gradient
                                .stops
                                .push(crate::effects::gradient::GradientStop {
                                    position: 0.5,
                                    color: [128, 128, 128, 255],
                                });
                            effect_changed = true;
                        }
                        if ui.button("Distribute Stops").clicked() {
                            let n = effect.gradient.stops.len();
                            if n > 1 {
                                effect
                                    .gradient
                                    .stops
                                    .sort_by(|a, b| a.position.total_cmp(&b.position));
                                for (i, stop) in effect.gradient.stops.iter_mut().enumerate() {
                                    stop.position = i as f32 / (n - 1) as f32;
                                }
                                effect_changed = true;
                            }
                        }
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Interpolation:");
                        if ui
                            .radio_value(
                                &mut effect.gradient.interpolation,
                                crate::effects::gradient::GradientInterpolation::Step,
                                "Step",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .radio_value(
                                &mut effect.gradient.interpolation,
                                crate::effects::gradient::GradientInterpolation::Linear,
                                "Linear",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .radio_value(
                                &mut effect.gradient.interpolation,
                                crate::effects::gradient::GradientInterpolation::Smooth,
                                "Smooth",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Color Processing:");
                        if ui
                            .radio_value(
                                &mut effect.gradient.color_space,
                                crate::effects::gradient::GradientColorSpace::Srgb,
                                "sRGB",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .radio_value(
                                &mut effect.gradient.color_space,
                                crate::effects::gradient::GradientColorSpace::LinearRgb,
                                "Linear RGB",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Edge Mode:");
                        if ui
                            .radio_value(
                                &mut effect.gradient.edge_mode,
                                crate::effects::gradient::GradientEdgeMode::Clamp,
                                "Clamp",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .radio_value(
                                &mut effect.gradient.edge_mode,
                                crate::effects::gradient::GradientEdgeMode::Repeat,
                                "Repeat",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .radio_value(
                                &mut effect.gradient.edge_mode,
                                crate::effects::gradient::GradientEdgeMode::Mirror,
                                "Mirror",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Dithering:");
                        if ui
                            .radio_value(
                                &mut effect.gradient.dithering,
                                crate::effects::gradient::GradientDithering::None,
                                "None",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .radio_value(
                                &mut effect.gradient.dithering,
                                crate::effects::gradient::GradientDithering::Bayer2x2,
                                "Bayer 2x2",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                        if ui
                            .radio_value(
                                &mut effect.gradient.dithering,
                                crate::effects::gradient::GradientDithering::Bayer4x4,
                                "Bayer 4x4",
                            )
                            .changed()
                        {
                            effect_changed = true;
                        }
                    });

                    if is_fill {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Shape:");
                            if ui
                                .radio_value(
                                    &mut effect.gradient.shape,
                                    crate::effects::gradient::GradientShape::Linear,
                                    "Linear",
                                )
                                .changed()
                            {
                                effect_changed = true;
                            }
                            if ui
                                .radio_value(
                                    &mut effect.gradient.shape,
                                    crate::effects::gradient::GradientShape::Radial,
                                    "Radial",
                                )
                                .changed()
                            {
                                effect_changed = true;
                            }
                        });

                        match effect.gradient.shape {
                            crate::effects::gradient::GradientShape::Linear => {
                                ui.horizontal(|ui| {
                                    ui.label("Start X");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut effect.gradient.linear_start[0],
                                            0.0..=1.0,
                                        ))
                                        .changed()
                                    {
                                        effect_changed = true;
                                    }
                                    ui.label("Y");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut effect.gradient.linear_start[1],
                                            0.0..=1.0,
                                        ))
                                        .changed()
                                    {
                                        effect_changed = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("End X");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut effect.gradient.linear_end[0],
                                            0.0..=1.0,
                                        ))
                                        .changed()
                                    {
                                        effect_changed = true;
                                    }
                                    ui.label("Y");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut effect.gradient.linear_end[1],
                                            0.0..=1.0,
                                        ))
                                        .changed()
                                    {
                                        effect_changed = true;
                                    }
                                });
                            }
                            crate::effects::gradient::GradientShape::Radial => {
                                ui.horizontal(|ui| {
                                    ui.label("Center X");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut effect.gradient.radial_center[0],
                                            0.0..=1.0,
                                        ))
                                        .changed()
                                    {
                                        effect_changed = true;
                                    }
                                    ui.label("Y");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut effect.gradient.radial_center[1],
                                            0.0..=1.0,
                                        ))
                                        .changed()
                                    {
                                        effect_changed = true;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.label("Radius X");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut effect.gradient.radial_radius[0],
                                            0.01..=2.0,
                                        ))
                                        .changed()
                                    {
                                        if effect.gradient.radial_linked {
                                            effect.gradient.radial_radius[1] =
                                                effect.gradient.radial_radius[0];
                                        }
                                        effect_changed = true;
                                    }
                                    ui.label("Y");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut effect.gradient.radial_radius[1],
                                            0.01..=2.0,
                                        ))
                                        .changed()
                                    {
                                        if effect.gradient.radial_linked {
                                            effect.gradient.radial_radius[0] =
                                                effect.gradient.radial_radius[1];
                                        }
                                        effect_changed = true;
                                    }
                                });
                                if ui
                                    .checkbox(&mut effect.gradient.radial_linked, "Link X/Y")
                                    .changed()
                                {
                                    if effect.gradient.radial_linked {
                                        effect.gradient.radial_radius[1] =
                                            effect.gradient.radial_radius[0];
                                    }
                                    effect_changed = true;
                                }
                            }
                        }

                        ui.horizontal(|ui| {
                            ui.label("Blend:");
                            if ui
                                .radio_value(
                                    &mut effect.gradient.blend_mode,
                                    crate::effects::gradient::GradientBlendMode::Replace,
                                    "Replace",
                                )
                                .changed()
                            {
                                effect_changed = true;
                            }
                            if ui
                                .radio_value(
                                    &mut effect.gradient.blend_mode,
                                    crate::effects::gradient::GradientBlendMode::AlphaBlend,
                                    "Alpha Blend",
                                )
                                .changed()
                            {
                                effect_changed = true;
                            }
                        });
                    }
                }
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Target:");
                if ui
                    .radio_value(
                        &mut effect.target,
                        EffectTarget::ActiveLayer,
                        "Active Layer",
                    )
                    .changed()
                {
                    effect_changed = true;
                }
                let selection_available = EffectRegion::for_target(
                    effect.original_document.width,
                    effect.original_document.height,
                    EffectTarget::Selection,
                    &effect.source_selection,
                )
                .is_some();
                ui.add_enabled_ui(selection_available, |ui| {
                    if ui
                        .radio_value(
                            &mut effect.target,
                            EffectTarget::Selection,
                            "Current Selection",
                        )
                        .changed()
                    {
                        effect_changed = true;
                    }
                });
            });

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

        if modal.should_close() {
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
                let selection = app.editor.selection;
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
        let transaction_is_current = app.active_effect.as_ref().is_some_and(|effect| {
            effect.provenance_matches(app) && effect.target_layer_is_editable(app)
        });
        if !transaction_is_current {
            app.active_effect = None;
            app.texture_dirty = true;
            app.status_message = Some((
                "The project changed while the effect was open. The effect was cancelled."
                    .to_owned(),
                true,
            ));
            return;
        }

        let selection = app.editor.selection;
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
                EffectType::Palettize => "Palettize",
                EffectType::Outline => "Outline",
                EffectType::DropShadow => "Drop Shadow",
                EffectType::Pixelize => "Pixelize",
                EffectType::GradientFill => "Gradient Fill",
                EffectType::GradientMap => "Gradient Map",
            };

            if let Some(preview) = effect.preview_document {
                let changed = app.editor.mutate_document(title, move |document| {
                    if *document == *preview {
                        return false;
                    }
                    document.clone_from(&preview);
                    true
                });
                if changed {
                    app.texture_dirty = true;
                    use crate::app::EditEffects;
                    app.consume_edit_effects(EditEffects::current_frame_artwork(true));
                } else {
                    // The visible preview is byte-identical to the source, so
                    // there is no cache work to perform after closing it.
                    app.texture_dirty = false;
                }
            } else {
                debug_assert!(false, "an active effect must retain its preview document");
                return;
            }
        }
    }

    if effect_to_cancel && app.active_effect.take().is_some() {
        app.texture_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        effect_preview_refresh_due, effect_preview_size, offset_canvas_wrapped,
        rotated_source_coordinate, ActiveEffectState, EffectTarget, EffectType,
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
        let mut effect = ActiveEffectState::new(EffectType::AdjustColor, &editor, 0, 0);
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
        let mut effect = ActiveEffectState::new(EffectType::AdjustColor, &editor, 0, 0);
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
        let selection = editor.selection;
        let mut effect = ActiveEffectState::new(EffectType::AdjustColor, &editor, 0, 0);
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

    fn commit_test_effect(app: &mut crate::app::PixelBuddyApp) {
        let selection = app.editor.selection;
        if let Some(effect) = &mut app.active_effect {
            effect.refresh_if_dirty(&selection);
        }
        if let Some(effect) = app.active_effect.take() {
            if let Some(preview) = effect.preview_document {
                app.editor.mutate_document("Test", move |doc| {
                    if *doc == *preview {
                        return false;
                    }
                    doc.clone_from(&preview);
                    true
                });
            }
        }
    }

    #[test]
    fn adjust_color_handles_negative_values_and_preserves_alpha() {
        let mut app = crate::app::PixelBuddyApp::new(2, 1);
        app.editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [200, 100, 50, 128]);
        app.editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [0, 0, 0, 0]);

        app.start_effect(crate::effects::EffectType::AdjustColor);
        if let Some(effect) = &mut app.active_effect {
            effect.hue_shift = -90.0;
            effect.saturation = -50.0;
            effect.value = -20.0;
            effect.preview_dirty = true;
        }
        commit_test_effect(&mut app);

        let canvas = &app.editor.document().active_layer().canvas;
        let p = canvas.get_pixel(0, 0);
        assert_eq!(p[3], 128, "Alpha must be perfectly preserved");
        assert_eq!(
            canvas.get_pixel(1, 0),
            [0, 0, 0, 0],
            "Transparent pixels remain transparent"
        );
    }

    #[test]
    fn pixelize_zero_noop_and_selection_clipping() {
        let mut app = crate::app::PixelBuddyApp::new(2, 1);
        app.editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 255]);
        app.editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [0, 255, 0, 255]);

        app.editor.selection.active = true;
        app.editor.selection.x0 = 0;
        app.editor.selection.y0 = 0;
        app.editor.selection.x1 = 0;
        app.editor.selection.y1 = 0;

        app.start_effect(crate::effects::EffectType::Pixelize);
        if let Some(effect) = &mut app.active_effect {
            effect.target = crate::effects::EffectTarget::Selection;
            effect.pixelize_size = 4;
            effect.preview_dirty = true;
        }
        commit_test_effect(&mut app);

        let canvas = &app.editor.document().active_layer().canvas;
        assert_eq!(
            canvas.get_pixel(0, 0),
            [255, 0, 0, 255],
            "pixels outside the selection must not influence a selected block"
        );
        assert_eq!(
            canvas.get_pixel(1, 0),
            [0, 255, 0, 255],
            "Pixels outside selection remain perfectly untouched"
        );
    }

    #[test]
    fn gradient_fill_replace_and_alpha_blend() {
        let mut app = crate::app::PixelBuddyApp::new(1, 1);
        app.editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 128]);

        app.start_effect(crate::effects::EffectType::GradientFill);
        if let Some(effect) = &mut app.active_effect {
            effect.gradient.blend_mode = crate::effects::gradient::GradientBlendMode::AlphaBlend;
            effect.gradient.shape = crate::effects::gradient::GradientShape::Linear;
            effect.gradient.stops.clear();
            effect
                .gradient
                .stops
                .push(crate::effects::gradient::GradientStop {
                    position: 0.0,
                    color: [0, 255, 0, 255],
                });
            effect
                .gradient
                .stops
                .push(crate::effects::gradient::GradientStop {
                    position: 1.0,
                    color: [0, 255, 0, 255],
                });
            effect.preview_dirty = true;
        }
        commit_test_effect(&mut app);

        let canvas = &app.editor.document().active_layer().canvas;
        let p = canvas.get_pixel(0, 0);
        assert_eq!(
            p[1], 255,
            "Green gradient was alpha blended onto the canvas"
        );
    }

    #[test]
    fn effect_cancel_restores_document_perfectly() {
        let mut app = crate::app::PixelBuddyApp::new(1, 1);
        app.editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 128]);
        let original = app
            .editor
            .document()
            .active_layer()
            .canvas
            .pixels()
            .to_vec();

        app.start_effect(crate::effects::EffectType::GradientFill);
        if let Some(effect) = &mut app.active_effect {
            effect.preview_dirty = true;
            let selection = app.editor.selection;
            effect.refresh_if_dirty(&selection);
        }
        // simulate cancel
        app.active_effect = None;

        let restored = app
            .editor
            .document()
            .active_layer()
            .canvas
            .pixels()
            .to_vec();
        assert_eq!(
            original, restored,
            "Cancellation must result in a byte-for-byte identical document"
        );
    }

    #[test]
    fn starting_effect_pauses_playback_on_the_visible_frame() {
        let mut app = crate::app::PixelBuddyApp::new(1, 1);
        assert!(app.editor.add_frame());
        assert!(app.select_frame(0));
        app.toggle_animation_playback(0.0);
        assert!(app.editor.animation.is_playing);

        // Model one playback tick without depending on wall-clock timing.
        app.editor.animation.current_frame_index = 1;
        app.start_effect(EffectType::InvertColors);

        assert!(!app.editor.animation.is_playing);
        assert_eq!(app.editor.animation.current_frame_index, 1);
        assert_eq!(app.editor.animation.selected_frame_index(), 1);
        assert!(app.active_effect.is_some());
    }

    #[test]
    fn effect_provenance_rejects_intervening_edits_and_frame_changes() {
        let mut edited = crate::app::PixelBuddyApp::new(1, 1);
        edited.start_effect(EffectType::InvertColors);
        assert!(edited
            .active_effect
            .as_ref()
            .is_some_and(|effect| effect.provenance_matches(&edited)));
        edited.editor.mark_dirty();
        assert!(!edited
            .active_effect
            .as_ref()
            .is_some_and(|effect| effect.provenance_matches(&edited)));

        let mut switched = crate::app::PixelBuddyApp::new(1, 1);
        assert!(switched.editor.add_frame());
        switched.start_effect(EffectType::InvertColors);
        assert!(switched.select_frame(0));
        assert!(!switched
            .active_effect
            .as_ref()
            .is_some_and(|effect| effect.provenance_matches(&switched)));
    }

    #[test]
    fn locked_layers_reject_effects_before_preview_allocation() {
        let mut app = crate::app::PixelBuddyApp::new(1, 1);
        app.editor.document_mut().active_layer_mut().locked = true;

        app.start_effect(EffectType::InvertColors);

        assert!(app.active_effect.is_none());
        assert!(app
            .status_message
            .as_ref()
            .is_some_and(|(message, is_error)| *is_error && message.contains("Unlock")));
    }

    #[test]
    fn neutral_effect_commit_is_a_clean_history_free_noop() {
        let mut app = crate::app::PixelBuddyApp::new(1, 1);
        let revision_before = app.editor.revision();
        let pixels_before = app
            .editor
            .document()
            .active_layer()
            .canvas
            .pixels()
            .to_vec();

        app.start_effect(EffectType::AdjustColor);
        commit_test_effect(&mut app);

        assert_eq!(app.editor.revision(), revision_before);
        assert!(!app.editor.history.can_undo());
        assert_eq!(
            app.editor.document().active_layer().canvas.pixels(),
            pixels_before
        );
    }

    #[test]
    fn selection_offset_wraps_inside_selection_without_touching_neighbors() {
        let mut editor = crate::editor::EditorState::new(4, 1);
        let colors = [
            [10, 0, 0, 255],
            [20, 0, 0, 255],
            [30, 0, 0, 255],
            [40, 0, 0, 255],
        ];
        for (x, color) in colors.into_iter().enumerate() {
            editor
                .document_mut()
                .active_layer_mut()
                .canvas
                .set_pixel(x as u32, 0, color);
        }
        editor.selection.set_rect(1, 0, 2, 0);
        let mut effect = ActiveEffectState::new(EffectType::Offset, &editor, 0, 0);
        effect.offset_x = 1;
        effect.refresh_preview(&editor.selection);

        let canvas = &effect
            .preview_document
            .as_ref()
            .expect("preview exists")
            .active_layer()
            .canvas;
        assert_eq!(canvas.get_pixel(0, 0), colors[0]);
        assert_eq!(canvas.get_pixel(1, 0), colors[2]);
        assert_eq!(canvas.get_pixel(2, 0), colors[1]);
        assert_eq!(canvas.get_pixel(3, 0), colors[3]);
    }

    #[test]
    fn selection_mirror_uses_selection_bounds_instead_of_canvas_bounds() {
        let mut editor = crate::editor::EditorState::new(4, 1);
        let colors = [
            [10, 0, 0, 255],
            [20, 0, 0, 255],
            [30, 0, 0, 255],
            [40, 0, 0, 255],
        ];
        for (x, color) in colors.into_iter().enumerate() {
            editor
                .document_mut()
                .active_layer_mut()
                .canvas
                .set_pixel(x as u32, 0, color);
        }
        editor.selection.set_rect(1, 0, 2, 0);
        let mut effect = ActiveEffectState::new(EffectType::Mirror, &editor, 0, 0);
        effect.mirror_horizontal = true;
        effect.refresh_preview(&editor.selection);

        let canvas = &effect
            .preview_document
            .as_ref()
            .expect("preview exists")
            .active_layer()
            .canvas;
        assert_eq!(canvas.get_pixel(0, 0), colors[0]);
        assert_eq!(canvas.get_pixel(1, 0), colors[2]);
        assert_eq!(canvas.get_pixel(2, 0), colors[1]);
        assert_eq!(canvas.get_pixel(3, 0), colors[3]);
    }

    #[test]
    fn active_layer_target_ignores_marquee_while_selection_target_is_isolated() {
        let mut editor = crate::editor::EditorState::new(3, 1);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 255]);
        editor.selection.set_rect(1, 0, 1, 0);
        let mut effect = ActiveEffectState::new(EffectType::Outline, &editor, 0, 0);
        effect.outline_color = [0, 255, 0, 255];

        effect.refresh_preview(&editor.selection);
        assert_eq!(
            effect
                .preview_document
                .as_ref()
                .expect("selection preview exists")
                .active_layer()
                .canvas
                .get_pixel(1, 0),
            [0, 0, 0, 0],
            "an opaque neighbor outside the selection must not create an outline inside it"
        );

        effect.target = EffectTarget::ActiveLayer;
        effect.refresh_preview(&editor.selection);
        assert_eq!(
            effect
                .preview_document
                .as_ref()
                .expect("active-layer preview exists")
                .active_layer()
                .canvas
                .get_pixel(1, 0),
            [0, 255, 0, 255],
            "Active Layer must not be silently clipped by an existing marquee"
        );
    }

    #[test]
    fn unavailable_selection_defaults_to_active_layer_target() {
        let editor = crate::editor::EditorState::new(2, 2);
        let effect = ActiveEffectState::new(EffectType::InvertColors, &editor, 0, 0);
        assert_eq!(effect.target, EffectTarget::ActiveLayer);
    }

    #[test]
    fn palettize_replacement_keeps_selected_palette_index_valid() {
        let mut editor = crate::editor::EditorState::new(1, 1);
        for value in 0..32 {
            editor
                .document_mut()
                .palette
                .add_color([value, value, value, 255]);
        }
        let last = editor.document().palette.colors.len() - 1;
        editor.document_mut().palette.selected_index = last;

        let mut effect = ActiveEffectState::new(EffectType::Palettize, &editor, 0, 0);
        effect.palettize_policy = crate::app::PalettePolicy::UseDefault;
        effect.refresh_preview(&editor.selection);

        let palette = &effect
            .preview_document
            .as_ref()
            .expect("preview exists")
            .palette;
        assert!(!palette.colors.is_empty());
        assert!(palette.selected_index < palette.colors.len());
    }

    #[test]
    fn drop_shadow_multiplies_source_color_and_opacity_alpha() {
        let mut editor = crate::editor::EditorState::new(2, 1);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 255, 255, 255]);
        let mut effect = ActiveEffectState::new(EffectType::DropShadow, &editor, 0, 0);
        effect.drop_shadow_color = [20, 40, 60, 128];
        effect.drop_shadow_opacity = 0.5;
        effect.drop_shadow_offset_x = 1;
        effect.drop_shadow_offset_y = 0;
        effect.refresh_preview(&editor.selection);

        assert_eq!(
            effect
                .preview_document
                .as_ref()
                .expect("preview exists")
                .active_layer()
                .canvas
                .get_pixel(1, 0),
            [20, 40, 60, 64]
        );
    }

    #[test]
    fn pixelize_uses_alpha_weighted_rgb() {
        let mut editor = crate::editor::EditorState::new(2, 1);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [255, 0, 0, 255]);
        editor
            .document_mut()
            .active_layer_mut()
            .canvas
            .set_pixel(1, 0, [0, 0, 255, 0]);
        let mut effect = ActiveEffectState::new(EffectType::Pixelize, &editor, 0, 0);
        effect.pixelize_size = 2;
        effect.refresh_preview(&editor.selection);

        let canvas = &effect
            .preview_document
            .as_ref()
            .expect("preview exists")
            .active_layer()
            .canvas;
        assert_eq!(canvas.get_pixel(0, 0), [255, 0, 0, 127]);
        assert_eq!(canvas.get_pixel(1, 0), [255, 0, 0, 127]);
    }

    #[test]
    fn selection_gradient_maps_endpoints_to_first_and_last_visible_pixels() {
        let mut editor = crate::editor::EditorState::new(5, 1);
        for x in 0..5 {
            editor
                .document_mut()
                .active_layer_mut()
                .canvas
                .set_pixel(x, 0, [0, 255, 0, 255]);
        }
        editor.selection.set_rect(1, 0, 3, 0);
        let mut effect = ActiveEffectState::new(EffectType::GradientFill, &editor, 0, 0);
        effect.gradient.stops = vec![
            crate::effects::gradient::GradientStop {
                position: 0.0,
                color: [255, 0, 0, 255],
            },
            crate::effects::gradient::GradientStop {
                position: 1.0,
                color: [0, 0, 255, 255],
            },
        ];
        effect.refresh_preview(&editor.selection);

        let canvas = &effect
            .preview_document
            .as_ref()
            .expect("preview exists")
            .active_layer()
            .canvas;
        assert_eq!(canvas.get_pixel(0, 0), [0, 255, 0, 255]);
        assert_eq!(canvas.get_pixel(1, 0), [255, 0, 0, 255]);
        assert_eq!(canvas.get_pixel(3, 0), [0, 0, 255, 255]);
        assert_eq!(canvas.get_pixel(4, 0), [0, 255, 0, 255]);
    }
}
