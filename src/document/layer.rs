use serde::{Deserialize, Serialize};
use super::canvas::Canvas;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub name: String,
    pub canvas: Canvas,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub visible: bool,
    pub locked: bool,
}

impl Layer {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            name: name.into(),
            canvas: Canvas::new(width, height),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            locked: false,
        }
    }

    pub fn blend_mode_apply(base: [u8; 4], top: [u8; 4], mode: BlendMode, opacity: f32) -> [u8; 4] {
        let alpha = (top[3] as f32 / 255.0) * opacity;
        if alpha <= 0.0 {
            return base;
        }
        
        let base_alpha = base[3] as f32 / 255.0;
        let out_alpha = alpha + base_alpha * (1.0 - alpha);
        
        if out_alpha <= 0.0 {
            return [0, 0, 0, 0];
        }

        let blend = |b: f32, t: f32, m: BlendMode| -> f32 {
            match m {
                BlendMode::Normal => t,
                BlendMode::Multiply => b * t,
                BlendMode::Screen => 1.0 - (1.0 - b) * (1.0 - t),
                BlendMode::Overlay => {
                    if b < 0.5 {
                        2.0 * b * t
                    } else {
                        1.0 - 2.0 * (1.0 - b) * (1.0 - t)
                    }
                }
            }
        };

        let b_r = base[0] as f32 / 255.0;
        let b_g = base[1] as f32 / 255.0;
        let b_b = base[2] as f32 / 255.0;
        
        let t_r = top[0] as f32 / 255.0;
        let t_g = top[1] as f32 / 255.0;
        let t_b = top[2] as f32 / 255.0;

        let out_r = (blend(b_r, t_r, mode) * alpha + b_r * base_alpha * (1.0 - alpha)) / out_alpha;
        let out_g = (blend(b_g, t_g, mode) * alpha + b_g * base_alpha * (1.0 - alpha)) / out_alpha;
        let out_b = (blend(b_b, t_b, mode) * alpha + b_b * base_alpha * (1.0 - alpha)) / out_alpha;

        [
            (out_r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (out_g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (out_b.clamp(0.0, 1.0) * 255.0).round() as u8,
            (out_alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }
}
