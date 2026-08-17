//! Drawing colour palettes: presets and a user-defined gradient.

use crate::machine::MAX_SYMBOLS;

pub type Rgb = [u8; 3];

/// Named palette modes selectable from the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    Classic,
    Grayscale,
    Sunset,
    Ocean,
    Neon,
    Gradient,
}

impl PaletteKind {
    pub const ALL: [PaletteKind; 6] = [
        Self::Classic,
        Self::Grayscale,
        Self::Sunset,
        Self::Ocean,
        Self::Neon,
        Self::Gradient,
    ];
}

impl std::fmt::Display for PaletteKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Classic => "Classic",
            Self::Grayscale => "Grayscale",
            Self::Sunset => "Sunset",
            Self::Ocean => "Ocean",
            Self::Neon => "Neon",
            Self::Gradient => "Gradient",
        })
    }
}

/// Original Turing Drawings `colorMap`.
const CLASSIC: [Rgb; MAX_SYMBOLS] = [
    [255, 0, 0],     // Initial symbol (untouched)
    [0, 0, 0],       // Black
    [255, 255, 255], // White
    [0, 255, 0],     // Green
    [0, 0, 255],     // Blue
    [255, 255, 0],   // Yellow
    [0, 255, 255],   // Cyan
    [255, 0, 255],   // Magenta
];

const GRAYSCALE: [Rgb; MAX_SYMBOLS] = [
    [0, 0, 0],
    [36, 36, 36],
    [73, 73, 73],
    [109, 109, 109],
    [146, 146, 146],
    [182, 182, 182],
    [219, 219, 219],
    [255, 255, 255],
];

const SUNSET: [Rgb; MAX_SYMBOLS] = [
    [45, 10, 30],
    [90, 20, 50],
    [160, 40, 60],
    [220, 80, 50],
    [255, 140, 60],
    [255, 190, 100],
    [255, 120, 140],
    [255, 200, 210],
];

const OCEAN: [Rgb; MAX_SYMBOLS] = [
    [5, 15, 40],
    [10, 40, 80],
    [15, 70, 120],
    [20, 110, 150],
    [30, 160, 170],
    [80, 200, 190],
    [160, 230, 220],
    [220, 245, 240],
];

const NEON: [Rgb; MAX_SYMBOLS] = [
    [10, 10, 20],
    [0, 255, 128],
    [0, 200, 255],
    [255, 0, 180],
    [255, 255, 0],
    [180, 0, 255],
    [255, 80, 0],
    [255, 255, 255],
];

const DEFAULT_GRADIENT_START: Rgb = [0x1a, 0x1a, 0x2e];
const DEFAULT_GRADIENT_END: Rgb = [0xe9, 0x45, 0x60];

/// Active drawing palette plus gradient editor state.
#[derive(Debug, Clone)]
pub struct Palette {
    pub kind: PaletteKind,
    pub colors: [Rgb; MAX_SYMBOLS],
    pub gradient_start: Rgb,
    pub gradient_end: Rgb,
    pub gradient_start_hex: String,
    pub gradient_end_hex: String,
}

impl Palette {
    pub fn classic() -> Self {
        Self {
            kind: PaletteKind::Classic,
            colors: CLASSIC,
            gradient_start: DEFAULT_GRADIENT_START,
            gradient_end: DEFAULT_GRADIENT_END,
            gradient_start_hex: rgb_to_hex(DEFAULT_GRADIENT_START),
            gradient_end_hex: rgb_to_hex(DEFAULT_GRADIENT_END),
        }
    }

    /// Rebuild `colors` from the current kind / gradient endpoints.
    pub fn resolve(&mut self, num_symbols: usize) {
        self.colors = match self.kind {
            PaletteKind::Classic => CLASSIC,
            PaletteKind::Grayscale => GRAYSCALE,
            PaletteKind::Sunset => SUNSET,
            PaletteKind::Ocean => OCEAN,
            PaletteKind::Neon => NEON,
            PaletteKind::Gradient => {
                gradient_colors(self.gradient_start, self.gradient_end, num_symbols)
            }
        };
    }

    pub fn set_kind(&mut self, kind: PaletteKind, num_symbols: usize) {
        self.kind = kind;
        self.resolve(num_symbols);
    }

    /// Set gradient start from an RGB triple (also refreshes the hex field).
    pub fn set_gradient_start_rgb(&mut self, rgb: Rgb, num_symbols: usize) {
        self.gradient_start = rgb;
        self.gradient_start_hex = rgb_to_hex(rgb);
        if self.kind == PaletteKind::Gradient {
            self.resolve(num_symbols);
        }
    }

    /// Set gradient end from an RGB triple (also refreshes the hex field).
    pub fn set_gradient_end_rgb(&mut self, rgb: Rgb, num_symbols: usize) {
        self.gradient_end = rgb;
        self.gradient_end_hex = rgb_to_hex(rgb);
        if self.kind == PaletteKind::Gradient {
            self.resolve(num_symbols);
        }
    }

    /// Update gradient start from a hex string. Returns an error message on invalid input.
    pub fn set_gradient_start_hex(
        &mut self,
        hex: String,
        num_symbols: usize,
    ) -> Result<(), String> {
        self.gradient_start_hex = hex;
        match parse_hex_rgb(&self.gradient_start_hex) {
            Ok(rgb) => {
                self.gradient_start = rgb;
                if self.kind == PaletteKind::Gradient {
                    self.resolve(num_symbols);
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Update gradient end from a hex string. Returns an error message on invalid input.
    pub fn set_gradient_end_hex(&mut self, hex: String, num_symbols: usize) -> Result<(), String> {
        self.gradient_end_hex = hex;
        match parse_hex_rgb(&self.gradient_end_hex) {
            Ok(rgb) => {
                self.gradient_end = rgb;
                if self.kind == PaletteKind::Gradient {
                    self.resolve(num_symbols);
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

/// Linear RGB gradient across `num_symbols` slots; unused slots copy the end colour.
fn gradient_colors(start: Rgb, end: Rgb, num_symbols: usize) -> [Rgb; MAX_SYMBOLS] {
    let n = num_symbols.clamp(2, MAX_SYMBOLS);
    let mut colors = [end; MAX_SYMBOLS];
    for (i, slot) in colors.iter_mut().enumerate().take(n) {
        let t = i as f32 / (n - 1) as f32;
        *slot = lerp_rgb(start, end, t);
    }
    colors
}

fn lerp_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        lerp_u8(a[0], b[0], t),
        lerp_u8(a[1], b[1], t),
        lerp_u8(a[2], b[2], t),
    ]
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
}

/// Parse `#RRGGBB` or `RRGGBB` into an RGB triple.
pub fn parse_hex_rgb(s: &str) -> Result<Rgb, String> {
    let s = s.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return Err(format!("expected #RRGGBB, got {s:?}"));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| format!("invalid hex colour {s:?}"))?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| format!("invalid hex colour {s:?}"))?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| format!("invalid hex colour {s:?}"))?;
    Ok([r, g, b])
}

pub fn rgb_to_hex(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_matches_original_colormap() {
        assert_eq!(CLASSIC[0], [255, 0, 0]);
        assert_eq!(CLASSIC[1], [0, 0, 0]);
        assert_eq!(CLASSIC[2], [255, 255, 255]);
        assert_eq!(CLASSIC[7], [255, 0, 255]);
    }

    #[test]
    fn parse_hex_with_and_without_hash() {
        assert_eq!(parse_hex_rgb("#1a1a2e").unwrap(), [0x1a, 0x1a, 0x2e]);
        assert_eq!(parse_hex_rgb("e94560").unwrap(), [0xe9, 0x45, 0x60]);
        assert_eq!(parse_hex_rgb("  #AbCdEf  ").unwrap(), [0xab, 0xcd, 0xef]);
    }

    #[test]
    fn parse_hex_rejects_invalid() {
        assert!(parse_hex_rgb("").is_err());
        assert!(parse_hex_rgb("#fff").is_err());
        assert!(parse_hex_rgb("#gg0000").is_err());
        assert!(parse_hex_rgb("12345").is_err());
    }

    #[test]
    fn gradient_endpoints_and_unused_slots() {
        let start = [0, 0, 0];
        let end = [255, 0, 0];
        let colors = gradient_colors(start, end, 3);
        assert_eq!(colors[0], start);
        assert_eq!(colors[2], end);
        assert_eq!(colors[1], [128, 0, 0]);
        // Unused slots copy end.
        for i in 3..MAX_SYMBOLS {
            assert_eq!(colors[i], end);
        }
    }

    #[test]
    fn gradient_two_symbols() {
        let colors = gradient_colors([10, 20, 30], [40, 50, 60], 2);
        assert_eq!(colors[0], [10, 20, 30]);
        assert_eq!(colors[1], [40, 50, 60]);
    }

    #[test]
    fn palette_resolve_classic() {
        let mut p = Palette::classic();
        p.resolve(3);
        assert_eq!(p.colors, CLASSIC);
    }

    #[test]
    fn set_gradient_hex_updates_when_gradient_kind() {
        let mut p = Palette::classic();
        p.set_kind(PaletteKind::Gradient, 4);
        p.set_gradient_start_hex("#000000".into(), 4).unwrap();
        p.set_gradient_end_hex("#ffffff".into(), 4).unwrap();
        assert_eq!(p.colors[0], [0, 0, 0]);
        assert_eq!(p.colors[3], [255, 255, 255]);
        assert_eq!(p.colors[7], [255, 255, 255]);
    }

    #[test]
    fn invalid_hex_keeps_last_good_colour() {
        let mut p = Palette::classic();
        p.set_kind(PaletteKind::Gradient, 3);
        let before = p.gradient_start;
        let err = p
            .set_gradient_start_hex("not-a-colour".into(), 3)
            .unwrap_err();
        assert!(!err.is_empty());
        assert_eq!(p.gradient_start, before);
        assert_eq!(p.gradient_start_hex, "not-a-colour");
    }
}
