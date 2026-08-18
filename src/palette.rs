//! Drawing colour palettes: presets and a user-defined gradient.

use serde::{Deserialize, Serialize};

use crate::machine::MAX_SYMBOLS;

pub type Rgb = [u8; 3];

/// Named palette modes selectable from the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaletteKind {
    #[default]
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

pub const DEFAULT_GRADIENT_START: Rgb = [0x1a, 0x1a, 0x2e];
pub const DEFAULT_GRADIENT_END: Rgb = [0xe9, 0x45, 0x60];

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

    /// Snapshot kind and gradient endpoints for presets.
    pub fn to_spec(&self) -> PaletteSpec {
        PaletteSpec {
            kind: self.kind,
            gradient_start: self.gradient_start_hex.clone(),
            gradient_end: self.gradient_end_hex.clone(),
        }
    }

    /// Rebuild a palette from a saved spec.
    pub fn from_spec(spec: &PaletteSpec, num_symbols: usize) -> Self {
        let mut palette = Self::classic();
        let _ = palette.set_gradient_start_hex(spec.gradient_start.clone(), num_symbols);
        let _ = palette.set_gradient_end_hex(spec.gradient_end.clone(), num_symbols);
        palette.set_kind(spec.kind, num_symbols);
        palette
    }

    /// Rebuild `colors` from the current kind / gradient endpoints.
    ///
    /// Named palettes (except Classic) pick evenly across the full source
    /// range so a small symbol count still reaches the bright end. Classic
    /// keeps the original first-N `colorMap` so red / black / white stay
    /// symbols 0–2.
    pub fn resolve(&mut self, num_symbols: usize) {
        self.colors = match self.kind {
            PaletteKind::Classic => CLASSIC,
            PaletteKind::Grayscale => sample_palette(&GRAYSCALE, num_symbols),
            PaletteKind::Sunset => sample_palette(&SUNSET, num_symbols),
            PaletteKind::Ocean => sample_palette(&OCEAN, num_symbols),
            PaletteKind::Neon => sample_palette(&NEON, num_symbols),
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

/// Serializable palette snapshot. Colour tables are rebuilt with `num_symbols`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaletteSpec {
    #[serde(default)]
    pub kind: PaletteKind,
    #[serde(default = "default_spec_gradient_start")]
    pub gradient_start: String,
    #[serde(default = "default_spec_gradient_end")]
    pub gradient_end: String,
}

fn default_spec_gradient_start() -> String {
    rgb_to_hex(DEFAULT_GRADIENT_START)
}

fn default_spec_gradient_end() -> String {
    rgb_to_hex(DEFAULT_GRADIENT_END)
}

impl Default for PaletteSpec {
    fn default() -> Self {
        Self {
            kind: PaletteKind::Classic,
            gradient_start: default_spec_gradient_start(),
            gradient_end: default_spec_gradient_end(),
        }
    }
}

/// Spread `num_symbols` slots across a fixed source palette.
/// Unused slots copy the last source colour.
fn sample_palette(source: &[Rgb; MAX_SYMBOLS], num_symbols: usize) -> [Rgb; MAX_SYMBOLS] {
    let n = num_symbols.clamp(2, MAX_SYMBOLS);
    let last = source[MAX_SYMBOLS - 1];
    let mut colors = [last; MAX_SYMBOLS];
    for (i, slot) in colors.iter_mut().enumerate().take(n) {
        let idx = i * (MAX_SYMBOLS - 1) / (n - 1);
        *slot = source[idx];
    }
    colors
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

/// Allocate an RGBA frame from the symbol map.
pub fn rgba_from_map(map: &[i32], colors: &[Rgb; MAX_SYMBOLS]) -> Vec<u8> {
    let mut pixels = vec![0u8; map.len() * 4];
    fill_rgba_from_map(map, &mut pixels, colors);
    pixels
}

/// Colorize `map` into `pixels` (4 bytes per cell, alpha 255).
pub fn fill_rgba_from_map(map: &[i32], pixels: &mut [u8], colors: &[Rgb; MAX_SYMBOLS]) {
    debug_assert_eq!(pixels.len(), map.len() * 4);
    for (i, &sy) in map.iter().enumerate() {
        write_rgb(pixels, i, colors[sy as usize]);
    }
}

/// Opaque RGBA length for a symbol map of `cells` entries.
pub fn rgba_len(cells: usize) -> usize {
    cells * 4
}

/// Allocate a zeroed RGBA canvas for `cells` tape cells.
pub fn empty_canvas(cells: usize) -> Vec<u8> {
    vec![0u8; rgba_len(cells)]
}

/// Write an opaque RGB triple into `pixels` at cell `idx`.
pub fn write_rgb(pixels: &mut [u8], idx: usize, rgb: Rgb) {
    let o = idx * 4;
    pixels[o] = rgb[0];
    pixels[o + 1] = rgb[1];
    pixels[o + 2] = rgb[2];
    pixels[o + 3] = 255;
}

/// Read the RGB triple stored at cell `idx`.
pub fn rgb_at(pixels: &[u8], idx: usize) -> Rgb {
    let o = idx * 4;
    [pixels[o], pixels[o + 1], pixels[o + 2]]
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
    fn sample_palette_spans_full_range() {
        // 2 symbols: first and last. 3: first, mid, last. 8: every source slot.
        let two = sample_palette(&GRAYSCALE, 2);
        assert_eq!(two[0], GRAYSCALE[0]);
        assert_eq!(two[1], GRAYSCALE[7]);
        for i in 2..MAX_SYMBOLS {
            assert_eq!(two[i], GRAYSCALE[7]);
        }

        let three = sample_palette(&SUNSET, 3);
        assert_eq!(three[0], SUNSET[0]);
        assert_eq!(three[1], SUNSET[3]);
        assert_eq!(three[2], SUNSET[7]);

        let full = sample_palette(&OCEAN, 8);
        assert_eq!(full, OCEAN);
    }

    #[test]
    fn palette_resolve_classic() {
        let mut p = Palette::classic();
        p.resolve(3);
        assert_eq!(p.colors, CLASSIC);
    }

    #[test]
    fn palette_resolve_named_uses_full_range() {
        let mut p = Palette::classic();
        p.set_kind(PaletteKind::Grayscale, 3);
        assert_eq!(p.colors[0], GRAYSCALE[0]);
        assert_eq!(p.colors[1], GRAYSCALE[3]);
        assert_eq!(p.colors[2], GRAYSCALE[7]);
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

    #[test]
    fn spec_roundtrip_restores_kind_and_gradient() {
        let mut p = Palette::classic();
        p.set_kind(PaletteKind::Gradient, 4);
        p.set_gradient_start_hex("#010203".into(), 4).unwrap();
        p.set_gradient_end_hex("#f0f1f2".into(), 4).unwrap();
        let spec = p.to_spec();
        let q = Palette::from_spec(&spec, 4);
        assert_eq!(q.kind, PaletteKind::Gradient);
        assert_eq!(q.colors, p.colors);
        assert_eq!(q.gradient_start, [1, 2, 3]);
        assert_eq!(q.gradient_end, [0xf0, 0xf1, 0xf2]);
    }

    #[test]
    fn fill_rgba_writes_palette_and_opaque_alpha() {
        let colors = CLASSIC;
        let map = [0, 2, 1];
        let mut pixels = vec![0u8; 12];
        fill_rgba_from_map(&map, &mut pixels, &colors);
        assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
        assert_eq!(&pixels[4..8], &[255, 255, 255, 255]);
        assert_eq!(&pixels[8..12], &[0, 0, 0, 255]);
        assert_eq!(rgba_from_map(&map, &colors), pixels);
        assert_eq!(rgb_at(&pixels, 1), [255, 255, 255]);
        write_rgb(&mut pixels, 1, [1, 2, 3]);
        assert_eq!(rgb_at(&pixels, 1), [1, 2, 3]);
        assert_eq!(rgba_len(3), 12);
        assert_eq!(empty_canvas(2).len(), 8);
    }
}
