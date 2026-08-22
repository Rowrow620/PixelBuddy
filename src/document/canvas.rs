/// The largest supported side length for a canvas.
///
/// This keeps both the pixel buffer and the per-pixel renderer within a
/// practical bound on native and WebAssembly builds.
pub const MAX_DIMENSION: u32 = 8_192;

/// The largest supported number of pixels in a canvas (64 MiB of RGBA data).
pub const MAX_PIXELS: usize = 16_777_216;

const CHANNELS_PER_PIXEL: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasError {
    InvalidDimensions {
        width: u32,
        height: u32,
    },
    DimensionTooLarge {
        width: u32,
        height: u32,
        max_dimension: u32,
    },
    TooManyPixels {
        width: u32,
        height: u32,
        max_pixels: usize,
    },
    SizeOverflow {
        width: u32,
        height: u32,
    },
    AllocationFailed {
        width: u32,
        height: u32,
    },
}

impl std::fmt::Display for CanvasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(f, "canvas dimensions must be at least 1x1 (got {width}x{height})")
            }
            Self::DimensionTooLarge {
                width,
                height,
                max_dimension,
            } => write!(
                f,
                "canvas dimensions must not exceed {max_dimension}x{max_dimension} (got {width}x{height})"
            ),
            Self::TooManyPixels {
                width,
                height,
                max_pixels,
            } => write!(
                f,
                "canvas may contain at most {max_pixels} pixels (got {width}x{height})"
            ),
            Self::SizeOverflow { width, height } => {
                write!(f, "canvas buffer size overflows for {width}x{height}")
            }
            Self::AllocationFailed { width, height } => {
                write!(f, "could not allocate a canvas buffer for {width}x{height}")
            }
        }
    }
}

impl std::error::Error for CanvasError {}

#[derive(Clone, Debug)]
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    /// Creates a canvas when its dimensions are valid and its buffer can be
    /// allocated without overflowing.
    pub fn try_new(width: u32, height: u32) -> Result<Self, CanvasError> {
        let byte_len = Self::buffer_len(width, height)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(byte_len)
            .map_err(|_| CanvasError::AllocationFailed { width, height })?;
        pixels.resize(byte_len, 0);

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Internal convenience for dimensions already validated by a higher-level
    /// model constructor. External input paths must use [`Canvas::try_new`].
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self::try_new(width, height).expect("internal canvas dimensions must be valid")
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let Some(idx) = self.pixel_index(x, y) else {
            return [0, 0, 0, 0];
        };

        [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        let Some(idx) = self.pixel_index(x, y) else {
            return;
        };

        self.pixels[idx..idx + CHANNELS_PER_PIXEL].copy_from_slice(&color);
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn retained_pixel_bytes(&self) -> usize {
        self.pixels.capacity()
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Builds a resized copy without changing the source canvas on validation
    /// or allocation failure.
    pub fn try_resized(&self, new_width: u32, new_height: u32) -> Result<Self, CanvasError> {
        let mut resized = Canvas::try_new(new_width, new_height)?;
        for y in 0..self.height.min(new_height) {
            for x in 0..self.width.min(new_width) {
                resized.set_pixel(x, y, self.get_pixel(x, y));
            }
        }
        Ok(resized)
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn clear(&mut self, color: [u8; 4]) {
        for chunk in self.pixels.chunks_exact_mut(CHANNELS_PER_PIXEL) {
            chunk.copy_from_slice(&color);
        }
    }

    pub fn blend_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        let Some(idx) = self.pixel_index(x, y) else {
            return;
        };

        let base = [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ];
        let source_alpha = color[3] as f32 / 255.0;
        let base_alpha = base[3] as f32 / 255.0;
        let output_alpha = source_alpha + base_alpha * (1.0 - source_alpha);

        if output_alpha <= f32::EPSILON {
            self.pixels[idx..idx + CHANNELS_PER_PIXEL].fill(0);
            return;
        }

        let composite_channel = |source: u8, destination: u8| {
            let value = (source as f32 * source_alpha
                + destination as f32 * base_alpha * (1.0 - source_alpha))
                / output_alpha;
            value.round().clamp(0.0, 255.0) as u8
        };

        self.pixels[idx..idx + CHANNELS_PER_PIXEL].copy_from_slice(&[
            composite_channel(color[0], base[0]),
            composite_channel(color[1], base[1]),
            composite_channel(color[2], base[2]),
            (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
        ]);
    }

    fn buffer_len(width: u32, height: u32) -> Result<usize, CanvasError> {
        if width == 0 || height == 0 {
            return Err(CanvasError::InvalidDimensions { width, height });
        }

        let width_usize =
            usize::try_from(width).map_err(|_| CanvasError::SizeOverflow { width, height })?;
        let height_usize =
            usize::try_from(height).map_err(|_| CanvasError::SizeOverflow { width, height })?;
        let pixel_count = width_usize
            .checked_mul(height_usize)
            .ok_or(CanvasError::SizeOverflow { width, height })?;
        let byte_len = pixel_count
            .checked_mul(CHANNELS_PER_PIXEL)
            .ok_or(CanvasError::SizeOverflow { width, height })?;

        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(CanvasError::DimensionTooLarge {
                width,
                height,
                max_dimension: MAX_DIMENSION,
            });
        }
        if pixel_count > MAX_PIXELS {
            return Err(CanvasError::TooManyPixels {
                width,
                height,
                max_pixels: MAX_PIXELS,
            });
        }

        Ok(byte_len)
    }

    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let width = usize::try_from(self.width).ok()?;
        let x = usize::try_from(x).ok()?;
        let y = usize::try_from(y).ok()?;
        let pixel_index = y.checked_mul(width)?.checked_add(x)?;
        let byte_index = pixel_index.checked_mul(CHANNELS_PER_PIXEL)?;
        let end = byte_index.checked_add(CHANNELS_PER_PIXEL)?;

        (end <= self.pixels.len()).then_some(byte_index)
    }
}

#[cfg(test)]
mod tests {
    use super::{Canvas, CanvasError, MAX_DIMENSION, MAX_PIXELS};

    #[test]
    fn checked_construction_rejects_invalid_and_oversized_dimensions() {
        assert_eq!(
            Canvas::try_new(0, 1).unwrap_err(),
            CanvasError::InvalidDimensions {
                width: 0,
                height: 1,
            }
        );
        assert_eq!(
            Canvas::try_new(MAX_DIMENSION + 1, 1).unwrap_err(),
            CanvasError::DimensionTooLarge {
                width: MAX_DIMENSION + 1,
                height: 1,
                max_dimension: MAX_DIMENSION,
            }
        );
        assert_eq!(
            Canvas::try_new(MAX_DIMENSION, MAX_DIMENSION).unwrap_err(),
            CanvasError::TooManyPixels {
                width: MAX_DIMENSION,
                height: MAX_DIMENSION,
                max_pixels: MAX_PIXELS,
            }
        );
    }

    #[test]
    fn checked_construction_detects_size_overflow() {
        assert_eq!(
            Canvas::try_new(u32::MAX, u32::MAX).unwrap_err(),
            CanvasError::SizeOverflow {
                width: u32::MAX,
                height: u32::MAX,
            }
        );
    }

    #[test]
    fn failed_resize_keeps_the_original_canvas_unchanged() {
        let mut canvas = Canvas::new(2, 2);
        canvas.set_pixel(1, 1, [1, 2, 3, 4]);

        assert!(canvas.try_resized(0, 1).is_err());
        assert_eq!((canvas.width(), canvas.height()), (2, 2));
        assert_eq!(canvas.get_pixel(1, 1), [1, 2, 3, 4]);
    }

    #[test]
    fn indexed_access_is_bounds_safe() {
        let mut canvas = Canvas::new(2, 2);
        canvas.set_pixel(1, 1, [1, 2, 3, 4]);
        canvas.set_pixel(2, 1, [9, 9, 9, 9]);

        assert_eq!(canvas.get_pixel(1, 1), [1, 2, 3, 4]);
        assert_eq!(canvas.get_pixel(2, 1), [0, 0, 0, 0]);
        assert!(!canvas.in_bounds(-1, 0));
    }

    #[test]
    fn normal_alpha_blending_uses_source_over_compositing() {
        let mut canvas = Canvas::new(1, 1);
        canvas.set_pixel(0, 0, [0, 255, 0, 128]);

        canvas.blend_pixel(0, 0, [255, 0, 0, 128]);

        assert_eq!(canvas.get_pixel(0, 0), [170, 85, 0, 192]);
    }
}
