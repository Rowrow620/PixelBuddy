use serde::{Deserialize, Serialize};

use super::{canvas::Canvas, valid_layer_name};
use crate::document::canvas::CanvasError;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub canvas: Canvas,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerError {
    InvalidName,
    Canvas(CanvasError),
}

impl std::fmt::Display for LayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName => write!(f, "layer name is invalid"),
            Self::Canvas(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for LayerError {}

impl From<CanvasError> for LayerError {
    fn from(error: CanvasError) -> Self {
        Self::Canvas(error)
    }
}

impl Layer {
    pub fn try_new(name: impl Into<String>, width: u32, height: u32) -> Result<Self, LayerError> {
        let name = name.into();
        if !valid_layer_name(&name) {
            return Err(LayerError::InvalidName);
        }
        Ok(Self {
            name,
            canvas: Canvas::try_new(width, height)?,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            locked: false,
        })
    }

    pub(crate) fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self::try_new(name, width, height).expect("internal layer construction must be valid")
    }

    pub fn try_resized(&self, width: u32, height: u32) -> Result<Self, LayerError> {
        if !valid_layer_name(&self.name) {
            return Err(LayerError::InvalidName);
        }
        Ok(Self {
            name: self.name.clone(),
            canvas: self.canvas.try_resized(width, height)?,
            opacity: self.opacity,
            blend_mode: self.blend_mode,
            visible: self.visible,
            locked: self.locked,
        })
    }

    /// Returns opacity normalized to the supported 0.0..=1.0 range.
    ///
    /// The field remains public for backward compatibility, so compositing
    /// always uses this accessor rather than trusting direct writes.
    pub fn normalized_opacity(&self) -> f32 {
        Self::normalize_opacity(self.opacity)
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = Self::normalize_opacity(opacity);
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    /// Whether an editing command may write to this layer.
    pub fn is_editable(&self) -> bool {
        !self.locked
    }

    /// Composites `top` over `base` with the requested blend mode.
    ///
    /// RGB values are stored unpremultiplied. The calculation follows the
    /// W3C source-over blend model so blend modes behave correctly when either
    /// pixel is partially or fully transparent.
    pub fn blend_mode_apply(base: [u8; 4], top: [u8; 4], mode: BlendMode, opacity: f32) -> [u8; 4] {
        let source_alpha = (top[3] as f32 / 255.0) * Self::normalize_opacity(opacity);
        if source_alpha <= f32::EPSILON {
            return base;
        }

        let base_alpha = base[3] as f32 / 255.0;
        let output_alpha = source_alpha + base_alpha * (1.0 - source_alpha);
        if output_alpha <= f32::EPSILON {
            return [0, 0, 0, 0];
        }

        let composite_channel = |base: u8, top: u8| {
            let base = base as f32 / 255.0;
            let top = top as f32 / 255.0;
            let blended = Self::blend_channel(base, top, mode);
            let premultiplied = top * source_alpha * (1.0 - base_alpha)
                + base * base_alpha * (1.0 - source_alpha)
                + blended * source_alpha * base_alpha;

            (premultiplied / output_alpha).clamp(0.0, 1.0)
        };

        [
            (composite_channel(base[0], top[0]) * 255.0).round() as u8,
            (composite_channel(base[1], top[1]) * 255.0).round() as u8,
            (composite_channel(base[2], top[2]) * 255.0).round() as u8,
            (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
        ]
    }

    fn normalize_opacity(opacity: f32) -> f32 {
        if opacity.is_nan() {
            0.0
        } else {
            opacity.clamp(0.0, 1.0)
        }
    }

    fn blend_channel(base: f32, top: f32, mode: BlendMode) -> f32 {
        match mode {
            BlendMode::Normal => top,
            BlendMode::Multiply => base * top,
            BlendMode::Screen => 1.0 - (1.0 - base) * (1.0 - top),
            BlendMode::Overlay => {
                if base <= 0.5 {
                    2.0 * base * top
                } else {
                    1.0 - 2.0 * (1.0 - base) * (1.0 - top)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BlendMode, Layer, LayerError};
    use crate::document::{canvas::CanvasError, MAX_LAYER_NAME_BYTES};

    #[test]
    fn fallible_construction_rejects_invalid_names_and_dimensions() {
        assert_eq!(
            Layer::try_new("x".repeat(MAX_LAYER_NAME_BYTES + 1), 1, 1).unwrap_err(),
            LayerError::InvalidName
        );
        assert_eq!(
            Layer::try_new("Layer", 0, 1).unwrap_err(),
            LayerError::Canvas(CanvasError::InvalidDimensions {
                width: 0,
                height: 1,
            })
        );
    }

    #[test]
    fn transparent_destination_keeps_source_color_for_every_blend_mode() {
        let source = [231, 42, 17, 128];

        for mode in [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
        ] {
            assert_eq!(
                Layer::blend_mode_apply([0, 0, 0, 0], source, mode, 1.0),
                source,
                "{mode:?} should not use transparent RGB as a blend backdrop"
            );
        }
    }

    #[test]
    fn transparent_source_is_a_noop() {
        let base = [10, 20, 30, 40];

        assert_eq!(
            Layer::blend_mode_apply(base, [255, 255, 255, 0], BlendMode::Screen, 1.0),
            base
        );
    }

    #[test]
    fn normal_mode_uses_correct_source_over_alpha() {
        assert_eq!(
            Layer::blend_mode_apply([0, 255, 0, 128], [255, 0, 0, 128], BlendMode::Normal, 1.0,),
            [170, 85, 0, 192]
        );
    }

    #[test]
    fn layer_helpers_normalize_opacity_and_expose_lock_state() {
        let mut layer = Layer::new("Test", 1, 1);
        layer.set_opacity(2.0);
        assert_eq!(layer.normalized_opacity(), 1.0);

        layer.set_opacity(f32::NAN);
        assert_eq!(layer.normalized_opacity(), 0.0);

        assert!(layer.is_editable());
        layer.set_locked(true);
        assert!(layer.is_locked());
        assert!(!layer.is_editable());
    }
}
