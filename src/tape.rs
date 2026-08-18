//! Seeded generators for the tape's initial symbol grid.

use std::f32::consts::PI;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

/// How Restart (and every other reset) fills the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapeInitKind {
    #[default]
    Empty,
    Uniform,
    Gaussian,
    Perlin,
}

impl TapeInitKind {
    pub const ALL: [TapeInitKind; 4] = [Self::Empty, Self::Uniform, Self::Gaussian, Self::Perlin];
}

impl std::fmt::Display for TapeInitKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "Empty",
            Self::Uniform => "Uniform",
            Self::Gaussian => "Gaussian",
            Self::Perlin => "Perlin",
        })
    }
}

pub const MIN_GAUSSIAN_MEAN: f32 = 0.0;
pub const MAX_GAUSSIAN_MEAN: f32 = 1.0;
pub const DEFAULT_GAUSSIAN_MEAN: f32 = 0.5;

pub const MIN_GAUSSIAN_SIGMA: f32 = 0.01;
pub const MAX_GAUSSIAN_SIGMA: f32 = 1.0;
pub const DEFAULT_GAUSSIAN_SIGMA: f32 = 0.25;

pub const MIN_PERLIN_SCALE: f32 = 8.0;
pub const MAX_PERLIN_SCALE: f32 = 256.0;
pub const DEFAULT_PERLIN_SCALE: f32 = 32.0;

pub const MIN_PERLIN_OCTAVES: u8 = 1;
pub const MAX_PERLIN_OCTAVES: u8 = 6;
pub const DEFAULT_PERLIN_OCTAVES: u8 = 4;

/// Kind, seed, and generator parameters for the initial tape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TapeInit {
    #[serde(default)]
    pub kind: TapeInitKind,
    #[serde(default)]
    pub seed: u64,
    #[serde(default = "default_gaussian_mean")]
    pub gaussian_mean: f32,
    #[serde(default = "default_gaussian_sigma")]
    pub gaussian_sigma: f32,
    #[serde(default = "default_perlin_scale")]
    pub perlin_scale: f32,
    #[serde(default = "default_perlin_octaves")]
    pub perlin_octaves: u8,
}

fn default_gaussian_mean() -> f32 {
    DEFAULT_GAUSSIAN_MEAN
}
fn default_gaussian_sigma() -> f32 {
    DEFAULT_GAUSSIAN_SIGMA
}
fn default_perlin_scale() -> f32 {
    DEFAULT_PERLIN_SCALE
}
fn default_perlin_octaves() -> u8 {
    DEFAULT_PERLIN_OCTAVES
}

impl Default for TapeInit {
    fn default() -> Self {
        Self {
            kind: TapeInitKind::Empty,
            seed: 0,
            gaussian_mean: DEFAULT_GAUSSIAN_MEAN,
            gaussian_sigma: DEFAULT_GAUSSIAN_SIGMA,
            perlin_scale: DEFAULT_PERLIN_SCALE,
            perlin_octaves: DEFAULT_PERLIN_OCTAVES,
        }
    }
}

impl TapeInit {
    /// Kind and params from settings; a fresh seed is chosen each time.
    pub fn from_kind_and_params(
        kind: TapeInitKind,
        gaussian_mean: f32,
        gaussian_sigma: f32,
        perlin_scale: f32,
        perlin_octaves: u8,
    ) -> Self {
        let mut init = Self {
            kind,
            seed: random_seed(),
            gaussian_mean,
            gaussian_sigma,
            perlin_scale,
            perlin_octaves,
        };
        init.sanitize();
        init
    }

    pub fn reseed(&mut self) {
        self.seed = random_seed();
    }

    pub fn sanitize(&mut self) {
        self.gaussian_mean = clamp_finite(
            self.gaussian_mean,
            MIN_GAUSSIAN_MEAN,
            MAX_GAUSSIAN_MEAN,
            DEFAULT_GAUSSIAN_MEAN,
        );
        self.gaussian_sigma = clamp_finite(
            self.gaussian_sigma,
            MIN_GAUSSIAN_SIGMA,
            MAX_GAUSSIAN_SIGMA,
            DEFAULT_GAUSSIAN_SIGMA,
        );
        self.perlin_scale = clamp_finite(
            self.perlin_scale,
            MIN_PERLIN_SCALE,
            MAX_PERLIN_SCALE,
            DEFAULT_PERLIN_SCALE,
        );
        self.perlin_octaves = self
            .perlin_octaves
            .clamp(MIN_PERLIN_OCTAVES, MAX_PERLIN_OCTAVES);
    }
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn random_seed() -> u64 {
    rand::rng().random()
}

/// Fill `map` (`width * height`) with symbols in `0..num_symbols` from `init`.
pub fn fill(map: &mut [i32], width: usize, height: usize, num_symbols: usize, init: &TapeInit) {
    debug_assert_eq!(map.len(), width.saturating_mul(height));
    if map.is_empty() || num_symbols == 0 {
        return;
    }
    let mut init = init.clone();
    init.sanitize();
    match init.kind {
        TapeInitKind::Empty => map.fill(0),
        TapeInitKind::Uniform => fill_uniform(map, num_symbols, init.seed),
        TapeInitKind::Gaussian => fill_gaussian(
            map,
            num_symbols,
            init.seed,
            init.gaussian_mean,
            init.gaussian_sigma,
        ),
        TapeInitKind::Perlin => fill_perlin(
            map,
            width,
            height,
            num_symbols,
            init.seed,
            init.perlin_scale,
            init.perlin_octaves,
        ),
    }
}

fn quantize(t: f32, num_symbols: usize) -> i32 {
    let n = num_symbols as f32;
    let v = (t.clamp(0.0, 1.0) * n).floor() as i32;
    v.min(num_symbols as i32 - 1).max(0)
}

fn fill_uniform(map: &mut [i32], num_symbols: usize, seed: u64) {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = num_symbols as i32;
    for cell in map.iter_mut() {
        *cell = rng.random_range(0..n);
    }
}

fn fill_gaussian(map: &mut [i32], num_symbols: usize, seed: u64, mean: f32, sigma: f32) {
    let mut rng = StdRng::seed_from_u64(seed);
    for cell in map.iter_mut() {
        *cell = quantize(mean + sigma * box_muller(&mut rng), num_symbols);
    }
}

fn box_muller<R: Rng>(rng: &mut R) -> f32 {
    let u1 = rng.random::<f32>().max(f32::EPSILON);
    let u2 = rng.random::<f32>();
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

fn fill_perlin(
    map: &mut [i32],
    width: usize,
    height: usize,
    num_symbols: usize,
    seed: u64,
    scale: f32,
    octaves: u8,
) {
    let period_x = ((width as f32 / scale).round() as i32).max(1);
    let period_y = ((height as f32 / scale).round() as i32).max(1);
    let seed = seed as u32;

    let mut amp_sum = 0.0;
    let mut amp = 1.0;
    for _ in 0..octaves {
        amp_sum += amp;
        amp *= 0.5;
    }

    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            let mut octave_amp = 1.0;
            let mut freq = 1i32;
            for oct in 0..octaves {
                let px = period_x.saturating_mul(freq).max(1);
                let py = period_y.saturating_mul(freq).max(1);
                let nx = (x as f32 / width as f32) * px as f32;
                let ny = (y as f32 / height as f32) * py as f32;
                sum +=
                    octave_amp * perlin2(nx, ny, px, py, seed.wrapping_add(u32::from(oct) * 1013));
                octave_amp *= 0.5;
                freq = freq.saturating_mul(2);
            }
            let t = (sum / amp_sum) * 0.5 + 0.5;
            map[y * width + x] = quantize(t, num_symbols);
        }
    }
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

fn hash2(ix: i32, iy: i32, seed: u32) -> u32 {
    let mut n = (ix as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((iy as u32).wrapping_mul(668_265_263))
        .wrapping_add(seed.wrapping_mul(3_626_077_663));
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    n ^ (n >> 16)
}

fn grad(hash: u32, x: f32, y: f32) -> f32 {
    match hash & 3 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        _ => -x - y,
    }
}

fn perlin2(x: f32, y: f32, period_x: i32, period_y: i32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let u = fade(fx);
    let v = fade(fy);

    let x0w = x0.rem_euclid(period_x);
    let y0w = y0.rem_euclid(period_y);
    let x1w = (x0 + 1).rem_euclid(period_x);
    let y1w = (y0 + 1).rem_euclid(period_y);

    let n00 = grad(hash2(x0w, y0w, seed), fx, fy);
    let n10 = grad(hash2(x1w, y0w, seed), fx - 1.0, fy);
    let n01 = grad(hash2(x0w, y1w, seed), fx, fy - 1.0);
    let n11 = grad(hash2(x1w, y1w, seed), fx - 1.0, fy - 1.0);

    lerp(lerp(n00, n10, u), lerp(n01, n11, u), v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 64;
    const H: usize = 48;
    const N: usize = 8;

    fn filled(kind: TapeInitKind, seed: u64) -> Vec<i32> {
        let mut map = vec![0; W * H];
        let init = TapeInit {
            kind,
            seed,
            ..TapeInit::default()
        };
        fill(&mut map, W, H, N, &init);
        map
    }

    fn assert_in_range(map: &[i32]) {
        assert!(map.iter().all(|&s| s >= 0 && s < N as i32));
    }

    fn unique_count(map: &[i32]) -> usize {
        let mut seen = [false; 16];
        for &s in map {
            seen[s as usize] = true;
        }
        seen.iter().filter(|&&b| b).count()
    }

    fn adjacent_mad(map: &[i32], width: usize, height: usize) -> f32 {
        let mut sum = 0.0;
        let mut n = 0;
        for y in 0..height {
            for x in 0..width {
                let here = map[y * width + x];
                let right = map[y * width + (x + 1) % width];
                let down = map[((y + 1) % height) * width + x];
                sum += (here - right).abs() as f32;
                sum += (here - down).abs() as f32;
                n += 2;
            }
        }
        sum / n as f32
    }

    #[test]
    fn empty_is_all_zeros() {
        let map = filled(TapeInitKind::Empty, 99);
        assert!(map.iter().all(|&s| s == 0));
    }

    #[test]
    fn same_seed_is_identical() {
        for kind in TapeInitKind::ALL {
            if kind == TapeInitKind::Empty {
                continue;
            }
            let a = filled(kind, 42);
            let b = filled(kind, 42);
            assert_eq!(a, b, "{kind} should be deterministic");
            assert_in_range(&a);
        }
    }

    #[test]
    fn different_seeds_differ() {
        for kind in [
            TapeInitKind::Uniform,
            TapeInitKind::Gaussian,
            TapeInitKind::Perlin,
        ] {
            let a = filled(kind, 1);
            let b = filled(kind, 2);
            assert_ne!(a, b, "{kind} should change with seed");
        }
    }

    #[test]
    fn uniform_uses_several_symbols() {
        let map = filled(TapeInitKind::Uniform, 7);
        assert_in_range(&map);
        assert!(unique_count(&map) > 1);
    }

    #[test]
    fn gaussian_stays_in_range() {
        let map = filled(TapeInitKind::Gaussian, 11);
        assert_in_range(&map);
        assert!(unique_count(&map) > 1);
    }

    #[test]
    fn perlin_is_more_correlated_than_uniform() {
        let uniform = filled(TapeInitKind::Uniform, 3);
        let perlin = filled(TapeInitKind::Perlin, 3);
        assert_in_range(&perlin);
        assert!(unique_count(&perlin) > 1);
        assert!(
            adjacent_mad(&perlin, W, H) < adjacent_mad(&uniform, W, H),
            "Perlin neighbors should be closer than uniform speckle"
        );
    }

    #[test]
    fn sanitize_clamps_and_replaces_nan() {
        let mut init = TapeInit {
            gaussian_mean: 4.0,
            gaussian_sigma: f32::NAN,
            perlin_scale: 1.0,
            perlin_octaves: 99,
            ..TapeInit::default()
        };
        init.sanitize();
        assert_eq!(init.gaussian_mean, MAX_GAUSSIAN_MEAN);
        assert_eq!(init.gaussian_sigma, DEFAULT_GAUSSIAN_SIGMA);
        assert_eq!(init.perlin_scale, MIN_PERLIN_SCALE);
        assert_eq!(init.perlin_octaves, MAX_PERLIN_OCTAVES);
    }

    #[test]
    fn json_roundtrip_and_legacy_default() {
        let init = TapeInit {
            kind: TapeInitKind::Perlin,
            seed: 99,
            gaussian_mean: 0.2,
            gaussian_sigma: 0.4,
            perlin_scale: 16.0,
            perlin_octaves: 2,
        };
        let text = serde_json::to_string(&init).unwrap();
        let back: TapeInit = serde_json::from_str(&text).unwrap();
        assert_eq!(init, back);

        let legacy: TapeInit = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy, TapeInit::default());
    }
}
