use super::Palette;

pub const DEFAULT_PRESET_ID: &str = "pico-8";
const EMERGENCY_DEFAULT_COLORS: &[[u8; 4]] = &[[0, 0, 0, 255]];
const EMERGENCY_DEFAULT_PRESET: PalettePreset = PalettePreset {
    id: "emergency-default",
    name: "Safe Default",
    colors: EMERGENCY_DEFAULT_COLORS,
};

#[derive(Clone, Debug, PartialEq)]
pub struct PalettePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub colors: &'static [[u8; 4]],
}

pub const PICO_8_COLORS: &[[u8; 4]] = &[
    [26, 28, 44, 255],
    [93, 39, 93, 255],
    [177, 62, 83, 255],
    [239, 125, 87, 255],
    [255, 205, 117, 255],
    [167, 240, 112, 255],
    [56, 183, 100, 255],
    [37, 113, 121, 255],
    [41, 54, 111, 255],
    [59, 93, 201, 255],
    [65, 166, 246, 255],
    [115, 239, 247, 255],
    [244, 244, 244, 255],
    [148, 176, 194, 255],
    [86, 108, 134, 255],
    [51, 60, 87, 255],
];

pub const GAMEBOY_COLORS: &[[u8; 4]] = &[
    [15, 56, 15, 255],
    [48, 98, 48, 255],
    [139, 172, 15, 255],
    [155, 188, 15, 255],
];

pub const COMMODORE_64_COLORS: &[[u8; 4]] = &[
    [0, 0, 0, 255],
    [255, 255, 255, 255],
    [136, 0, 0, 255],
    [170, 255, 238, 255],
    [204, 68, 204, 255],
    [0, 204, 85, 255],
    [0, 0, 170, 255],
    [238, 238, 119, 255],
    [221, 136, 85, 255],
    [102, 68, 0, 255],
    [255, 119, 119, 255],
    [51, 51, 51, 255],
    [119, 119, 119, 255],
    [170, 255, 102, 255],
    [0, 136, 255, 255],
    [187, 187, 187, 255],
];

pub const PRESETS: &[PalettePreset] = &[
    PalettePreset {
        id: "pico-8",
        name: "PICO-8",
        colors: PICO_8_COLORS,
    },
    PalettePreset {
        id: "gameboy",
        name: "Gameboy",
        colors: GAMEBOY_COLORS,
    },
    PalettePreset {
        id: "c64",
        name: "Commodore 64",
        colors: COMMODORE_64_COLORS,
    },
];

pub fn default_preset() -> PalettePreset {
    get_preset(DEFAULT_PRESET_ID).unwrap_or_else(|| EMERGENCY_DEFAULT_PRESET.clone())
}

pub fn get_preset(id: &str) -> Option<PalettePreset> {
    PRESETS.iter().find(|p| p.id == id).cloned()
}

impl PalettePreset {
    /// Converts a built-in candidate only when it satisfies the persisted
    /// palette contract. Keeping this validation at the library boundary makes
    /// a malformed future preset fail closed instead of entering a project.
    pub fn to_palette(&self) -> Option<Palette> {
        let valid_metadata = !self.id.trim().is_empty()
            && !self.name.trim().is_empty()
            && !self.id.chars().any(char::is_control)
            && !self.name.chars().any(char::is_control);
        let valid_colors = !self.colors.is_empty()
            && self.colors.len() <= super::MAX_PALETTE_COLORS
            && self.colors.iter().all(|color| color[3] == 255);

        (valid_metadata && valid_colors).then(|| Palette {
            colors: self.colors.to_vec(),
            selected_index: 0,
        })
    }
}

/// Returns the explicit built-in default, with a minimal opaque fallback so a
/// code regression in the preset table can never create an empty palette.
pub fn default_palette() -> Palette {
    default_preset().to_palette().unwrap_or_else(|| Palette {
        colors: vec![[0, 0, 0, 255]],
        selected_index: 0,
    })
}

/// Resolves a named built-in palette, falling back deterministically when an
/// identifier was removed or its candidate no longer validates.
pub fn preset_palette_or_default(id: &str) -> Palette {
    get_preset(id)
        .and_then(|preset| preset.to_palette())
        .unwrap_or_else(default_palette)
}

#[cfg(test)]
mod tests {
    use super::{
        default_palette, default_preset, preset_palette_or_default, PalettePreset,
        DEFAULT_PRESET_ID, PRESETS,
    };
    use crate::document::MAX_PALETTE_COLORS;
    use std::collections::HashSet;

    #[test]
    fn shipped_presets_are_unique_valid_and_include_the_explicit_default() {
        let mut ids = HashSet::new();
        for preset in PRESETS {
            assert!(ids.insert(preset.id), "duplicate preset ID: {}", preset.id);
            assert!(
                preset.to_palette().is_some(),
                "invalid preset: {}",
                preset.id
            );
        }

        assert_eq!(default_preset().id, DEFAULT_PRESET_ID);
        assert_eq!(
            default_palette(),
            preset_palette_or_default(DEFAULT_PRESET_ID)
        );
    }

    #[test]
    fn preset_validation_accepts_the_limit_and_rejects_invalid_candidates() {
        let at_limit = Box::leak(vec![[1, 2, 3, 255]; MAX_PALETTE_COLORS].into_boxed_slice());
        let too_many = Box::leak(vec![[1, 2, 3, 255]; MAX_PALETTE_COLORS + 1].into_boxed_slice());
        let candidates = [
            (
                PalettePreset {
                    id: "limit",
                    name: "Limit",
                    colors: at_limit,
                },
                true,
            ),
            (
                PalettePreset {
                    id: "empty",
                    name: "Empty",
                    colors: &[],
                },
                false,
            ),
            (
                PalettePreset {
                    id: "oversized",
                    name: "Oversized",
                    colors: too_many,
                },
                false,
            ),
            (
                PalettePreset {
                    id: "transparent",
                    name: "Transparent",
                    colors: &[[1, 2, 3, 254]],
                },
                false,
            ),
            (
                PalettePreset {
                    id: "",
                    name: "Missing ID",
                    colors: &[[1, 2, 3, 255]],
                },
                false,
            ),
        ];

        for (candidate, expected_valid) in candidates {
            assert_eq!(candidate.to_palette().is_some(), expected_valid);
        }
    }

    #[test]
    fn missing_preset_identifier_falls_back_to_the_explicit_default() {
        assert_eq!(preset_palette_or_default("retired-id"), default_palette());
    }
}
