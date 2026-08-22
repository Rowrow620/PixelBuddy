#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GradientShape {
    Linear,
    Radial,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GradientInterpolation {
    Step,
    Linear,
    Smooth, // cubic-style
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GradientColorSpace {
    Srgb,
    LinearRgb,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GradientEdgeMode {
    Clamp,
    Repeat,
    Mirror,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GradientDithering {
    None,
    Bayer2x2,
    Bayer4x4,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GradientBlendMode {
    Replace,
    AlphaBlend,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GradientTarget {
    ActiveLayer,
    CurrentSelection,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStop {
    pub position: f32, // 0.0 to 1.0
    pub color: [u8; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct GradientState {
    pub stops: Vec<GradientStop>,
    pub selected_stop: Option<usize>,
    pub shape: GradientShape,
    pub interpolation: GradientInterpolation,
    pub color_space: GradientColorSpace,
    pub edge_mode: GradientEdgeMode,
    pub dithering: GradientDithering,
    pub blend_mode: GradientBlendMode,
    pub target: GradientTarget,

    // Linear specific
    pub linear_start: [f32; 2],
    pub linear_end: [f32; 2],

    // Radial specific
    pub radial_center: [f32; 2],
    pub radial_radius: [f32; 2],
    pub radial_linked: bool,
}

impl Default for GradientState {
    fn default() -> Self {
        Self {
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: [0, 0, 0, 255],
                },
                GradientStop {
                    position: 1.0,
                    color: [255, 255, 255, 255],
                },
            ],
            selected_stop: None,
            shape: GradientShape::Linear,
            interpolation: GradientInterpolation::Linear,
            color_space: GradientColorSpace::Srgb,
            edge_mode: GradientEdgeMode::Clamp,
            dithering: GradientDithering::None,
            blend_mode: GradientBlendMode::Replace,
            target: GradientTarget::ActiveLayer,
            linear_start: [0.0, 0.5],
            linear_end: [1.0, 0.5],
            radial_center: [0.5, 0.5],
            radial_radius: [0.5, 0.5],
            radial_linked: true,
        }
    }
}

impl GradientState {
    pub fn sample_color(&self, mut t: f32) -> [u8; 4] {
        if self.stops.is_empty() {
            return [0, 0, 0, 0];
        }
        if self.stops.len() == 1 {
            return self.stops[0].color;
        }

        match self.edge_mode {
            GradientEdgeMode::Clamp => {
                t = t.clamp(0.0, 1.0);
            }
            GradientEdgeMode::Repeat => {
                t = t.rem_euclid(1.0);
            }
            GradientEdgeMode::Mirror => {
                let wrapped = t.rem_euclid(2.0);
                if wrapped > 1.0 {
                    t = 2.0 - wrapped;
                } else {
                    t = wrapped;
                }
            }
        }

        let mut stops = self.stops.clone();
        stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());

        if t <= stops[0].position {
            return stops[0].color;
        }
        if t >= stops.last().unwrap().position {
            return stops.last().unwrap().color;
        }

        let mut lower = &stops[0];
        let mut upper = &stops[1];
        for i in 0..stops.len() - 1 {
            if t >= stops[i].position && t <= stops[i + 1].position {
                lower = &stops[i];
                upper = &stops[i + 1];
                break;
            }
        }

        let range = upper.position - lower.position;
        let mut local_t = if range > 0.0 {
            (t - lower.position) / range
        } else {
            0.0
        };

        match self.interpolation {
            GradientInterpolation::Step => {
                if local_t < 0.5 {
                    return lower.color;
                } else {
                    return upper.color;
                }
            }
            GradientInterpolation::Linear => {
                // already linear
            }
            GradientInterpolation::Smooth => {
                // simple cubic ease in-out
                local_t = local_t * local_t * (3.0 - 2.0 * local_t);
            }
        }

        let c1 = lower.color;
        let c2 = upper.color;

        match self.color_space {
            GradientColorSpace::Srgb => {
                let r = (c1[0] as f32 * (1.0 - local_t) + c2[0] as f32 * local_t) as u8;
                let g = (c1[1] as f32 * (1.0 - local_t) + c2[1] as f32 * local_t) as u8;
                let b = (c1[2] as f32 * (1.0 - local_t) + c2[2] as f32 * local_t) as u8;
                let a = (c1[3] as f32 * (1.0 - local_t) + c2[3] as f32 * local_t) as u8;
                [r, g, b, a]
            }
            GradientColorSpace::LinearRgb => {
                // Approximate sRGB -> Linear
                let to_linear = |c: u8| -> f32 {
                    let v = c as f32 / 255.0;
                    if v <= 0.04045 {
                        v / 12.92
                    } else {
                        ((v + 0.055) / 1.055).powf(2.4)
                    }
                };
                let to_srgb = |c: f32| -> u8 {
                    let v = if c <= 0.0031308 {
                        c * 12.92
                    } else {
                        1.055 * c.powf(1.0 / 2.4) - 0.055
                    };
                    (v.clamp(0.0, 1.0) * 255.0) as u8
                };

                let r1 = to_linear(c1[0]);
                let g1 = to_linear(c1[1]);
                let b1 = to_linear(c1[2]);

                let r2 = to_linear(c2[0]);
                let g2 = to_linear(c2[1]);
                let b2 = to_linear(c2[2]);

                let r = to_srgb(r1 * (1.0 - local_t) + r2 * local_t);
                let g = to_srgb(g1 * (1.0 - local_t) + g2 * local_t);
                let b = to_srgb(b1 * (1.0 - local_t) + b2 * local_t);
                let a = (c1[3] as f32 * (1.0 - local_t) + c2[3] as f32 * local_t) as u8;
                [r, g, b, a]
            }
        }
    }
}
