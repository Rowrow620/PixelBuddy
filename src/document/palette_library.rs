use super::Palette;

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
    PRESETS[0].clone()
}

pub fn get_preset(id: &str) -> Option<PalettePreset> {
    PRESETS.iter().find(|p| p.id == id).cloned()
}

impl PalettePreset {
    pub fn to_palette(&self) -> Palette {
        Palette {
            colors: self.colors.to_vec(),
            selected_index: 0,
        }
    }
}
