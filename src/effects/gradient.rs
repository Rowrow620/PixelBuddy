pub const MAX_GRADIENT_STOPS: usize = 32;

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
pub enum GradientCoverage {
    /// Recolor only pixels that already contain layer content and retain their
    /// original alpha. This is the safe default for sprites with transparent
    /// surroundings.
    PaintedPixels,
    /// Generate gradient pixels everywhere in the selected target region,
    /// including pixels that were previously transparent.
    EntireTarget,
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
    pub coverage: GradientCoverage,
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
            coverage: GradientCoverage::PaintedPixels,
            linear_start: [0.0, 0.5],
            linear_end: [1.0, 0.5],
            radial_center: [0.5, 0.5],
            radial_radius: [0.5, 0.5],
            radial_linked: true,
        }
    }
}

impl GradientState {
    pub fn compose_fill_pixel(&self, original: [u8; 4], sampled: [u8; 4]) -> Option<[u8; 4]> {
        let preserve_alpha = self.coverage == GradientCoverage::PaintedPixels;
        if preserve_alpha && original[3] == 0 {
            return None;
        }

        let output = match self.blend_mode {
            GradientBlendMode::Replace if preserve_alpha => {
                [sampled[0], sampled[1], sampled[2], original[3]]
            }
            GradientBlendMode::Replace => sampled,
            GradientBlendMode::AlphaBlend if preserve_alpha => {
                let sampled_alpha = sampled[3] as f32 / 255.0;
                let mix_channel = |sampled_channel: u8, original_channel: u8| {
                    (sampled_channel as f32 * sampled_alpha
                        + original_channel as f32 * (1.0 - sampled_alpha))
                        .clamp(0.0, 255.0) as u8
                };
                [
                    mix_channel(sampled[0], original[0]),
                    mix_channel(sampled[1], original[1]),
                    mix_channel(sampled[2], original[2]),
                    original[3],
                ]
            }
            GradientBlendMode::AlphaBlend => {
                let sampled_alpha = sampled[3] as f32 / 255.0;
                let original_alpha = original[3] as f32 / 255.0;
                let output_alpha = sampled_alpha + original_alpha * (1.0 - sampled_alpha);
                if output_alpha <= 0.0 {
                    [0, 0, 0, 0]
                } else {
                    let blend_channel = |sampled_channel: u8, original_channel: u8| {
                        ((sampled_channel as f32 * sampled_alpha)
                            + (original_channel as f32 * original_alpha * (1.0 - sampled_alpha)))
                            / output_alpha
                    };
                    [
                        blend_channel(sampled[0], original[0]).clamp(0.0, 255.0) as u8,
                        blend_channel(sampled[1], original[1]).clamp(0.0, 255.0) as u8,
                        blend_channel(sampled[2], original[2]).clamp(0.0, 255.0) as u8,
                        (output_alpha * 255.0).clamp(0.0, 255.0) as u8,
                    ]
                }
            }
        };

        Some(output)
    }

    pub fn prepare_for_preview(&mut self) -> bool {
        let geometry_is_finite = self
            .linear_start
            .into_iter()
            .chain(self.linear_end)
            .chain(self.radial_center)
            .chain(self.radial_radius)
            .all(f32::is_finite);
        let normalized_geometry_is_bounded = self
            .linear_start
            .into_iter()
            .chain(self.linear_end)
            .chain(self.radial_center)
            .all(|value| (0.0..=1.0).contains(&value));
        if !geometry_is_finite
            || !normalized_geometry_is_bounded
            || self
                .radial_radius
                .iter()
                .any(|radius| !(0.01..=2.0).contains(radius))
            || !(2..=MAX_GRADIENT_STOPS).contains(&self.stops.len())
            || self
                .stops
                .iter()
                .any(|stop| !stop.position.is_finite() || !(0.0..=1.0).contains(&stop.position))
        {
            return false;
        }

        self.stops
            .sort_by(|left, right| left.position.total_cmp(&right.position));
        self.stops[0].position = 0.0;
        self.stops
            .last_mut()
            .expect("two gradient stops were validated above")
            .position = 1.0;
        self.selected_stop = self.selected_stop.filter(|index| *index < self.stops.len());
        true
    }

    pub fn sample_color(&self, mut t: f32) -> [u8; 4] {
        if self.stops.is_empty() {
            return [0, 0, 0, 0];
        }
        if self.stops.len() == 1 {
            return self.stops[0].color;
        }
        if !t.is_finite() {
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

        if t <= self.stops[0].position {
            return self.stops[0].color;
        }
        if t >= self
            .stops
            .last()
            .expect("non-empty stops checked above")
            .position
        {
            return self
                .stops
                .last()
                .expect("non-empty stops checked above")
                .color;
        }

        let (lower, upper) = self
            .stops
            .windows(2)
            .find_map(|pair| {
                (t >= pair[0].position && t <= pair[1].position).then_some((&pair[0], &pair[1]))
            })
            .unwrap_or_else(|| {
                let last = self.stops.len() - 1;
                (&self.stops[last - 1], &self.stops[last])
            });

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

#[cfg(test)]
mod tests {
    use super::{
        GradientBlendMode, GradientCoverage, GradientState, GradientStop, MAX_GRADIENT_STOPS,
    };

    #[test]
    fn painted_pixel_coverage_skips_clear_pixels_and_preserves_source_alpha() {
        let gradient = GradientState::default();

        assert_eq!(
            gradient.compose_fill_pixel([10, 20, 30, 0], [200, 150, 100, 255]),
            None
        );
        assert_eq!(
            gradient.compose_fill_pixel([10, 20, 30, 96], [200, 150, 100, 255]),
            Some([200, 150, 100, 96])
        );
    }

    #[test]
    fn entire_target_coverage_can_generate_previously_clear_pixels() {
        let gradient = GradientState {
            coverage: GradientCoverage::EntireTarget,
            ..GradientState::default()
        };

        assert_eq!(
            gradient.compose_fill_pixel([10, 20, 30, 0], [200, 150, 100, 192]),
            Some([200, 150, 100, 192])
        );
    }

    #[test]
    fn painted_pixel_alpha_blend_changes_rgb_without_expanding_alpha() {
        let gradient = GradientState {
            blend_mode: GradientBlendMode::AlphaBlend,
            ..GradientState::default()
        };

        assert_eq!(
            gradient.compose_fill_pixel([200, 0, 0, 96], [0, 0, 200, 128]),
            Some([99, 0, 100, 96])
        );
    }

    #[test]
    fn entire_target_alpha_blend_uses_source_over_compositing() {
        let gradient = GradientState {
            blend_mode: GradientBlendMode::AlphaBlend,
            coverage: GradientCoverage::EntireTarget,
            ..GradientState::default()
        };

        assert_eq!(
            gradient.compose_fill_pixel([255, 0, 0, 128], [0, 255, 0, 128]),
            Some([84, 170, 0, 191])
        );
        assert_eq!(
            gradient.compose_fill_pixel([9, 8, 7, 0], [1, 2, 3, 128]),
            Some([1, 2, 3, 128])
        );
    }

    #[test]
    fn preview_preparation_sorts_once_and_preserves_endpoints() {
        let mut gradient = GradientState {
            stops: vec![
                GradientStop {
                    position: 0.8,
                    color: [80, 0, 0, 255],
                },
                GradientStop {
                    position: 0.2,
                    color: [20, 0, 0, 255],
                },
                GradientStop {
                    position: 0.5,
                    color: [50, 0, 0, 255],
                },
            ],
            ..GradientState::default()
        };

        assert!(gradient.prepare_for_preview());
        assert_eq!(gradient.stops[0].position, 0.0);
        assert_eq!(gradient.stops[1].position, 0.5);
        assert_eq!(gradient.stops[2].position, 1.0);
        assert_eq!(gradient.sample_color(0.0), [20, 0, 0, 255]);
        assert_eq!(gradient.sample_color(1.0), [80, 0, 0, 255]);
    }

    #[test]
    fn preview_preparation_rejects_non_finite_and_excessive_inputs() {
        let mut non_finite = GradientState::default();
        non_finite.stops[0].position = f32::NAN;
        assert!(!non_finite.prepare_for_preview());

        let mut excessive = GradientState {
            stops: (0..=MAX_GRADIENT_STOPS)
                .map(|index| GradientStop {
                    position: index as f32 / MAX_GRADIENT_STOPS as f32,
                    color: [index as u8, 0, 0, 255],
                })
                .collect(),
            ..GradientState::default()
        };
        assert!(!excessive.prepare_for_preview());
    }
}
