//! One Turing machine: transition table, head, and start position.
//!
//! Table layout, action mapping (including swapped left/right), and share-string
//! encoding match https://github.com/maximecb/Turing-Drawings, with an optional
//! start-position prefix for this port.

use rand::seq::SliceRandom;
use rand::Rng;

use crate::palette::{write_rgb, Palette};

pub const DEFAULT_MAP_WIDTH: usize = 512;
pub const DEFAULT_MAP_HEIGHT: usize = 512;
pub const MIN_MAP_SIZE: usize = 64;
pub const MAX_MAP_SIZE: usize = 4096;

pub const ACTION_LEFT: i32 = 0;
pub const ACTION_RIGHT: i32 = 1;
const ACTION_UP: i32 = 2;
pub const ACTION_DOWN: i32 = 3;
const NUM_ACTIONS: i32 = 4;

pub const MIN_STATES: usize = 1;
pub const MAX_STATES: usize = 32;
pub const MIN_SYMBOLS: usize = 2;
pub const MAX_SYMBOLS: usize = 8;

/// Per-machine speed slider: `0` is default (1×), `+10` is 10×, `-10` is 1/10×.
pub const MIN_MACHINE_SPEED: f32 = -10.0;
pub const MAX_MACHINE_SPEED: f32 = 10.0;

/// Shared mutate slider: fraction of transition-table rules to re-randomize.
pub const MIN_MUTATE_PERCENT: u8 = 1;
pub const MAX_MUTATE_PERCENT: u8 = 100;
pub const DEFAULT_MUTATE_PERCENT: u8 = 10;

/// A decoded share string: dimensions plus the machine they describe.
#[derive(Debug, Clone)]
pub struct ParsedMachine {
    pub num_states: usize,
    pub num_symbols: usize,
    pub machine: Machine,
}

/// Default label for a machine slot (`"Machine 1"` at index 0).
pub fn default_machine_name(index: usize) -> String {
    format!("Machine {}", index + 1)
}

/// Transition: (next_state, write_symbol, action)
#[derive(Debug, Clone)]
pub struct Machine {
    /// Flat table: for each (symbol, state), three i32s [next_state, write_symbol, action]
    pub table: Vec<i32>,
    pub state: i32,
    pub x_pos: i32,
    pub y_pos: i32,
    pub start_x: i32,
    pub start_y: i32,
    /// Relative step-rate slider (`MIN_MACHINE_SPEED`..=`MAX_MACHINE_SPEED`).
    pub speed: f32,
    /// When false, the machine does not step during simulation.
    pub active: bool,
    /// User-facing label. Empty falls back to [`default_machine_name`].
    pub name: String,
    /// Stable id for per-machine keybindings. `0` means unassigned.
    pub id: u64,
    /// Colours baked into the canvas when this machine writes a symbol.
    pub palette: Palette,
    /// Scheduling rounds spent at the current `speed` (for fractional rates).
    pub(crate) rounds_at_speed: u64,
    /// Whole steps already taken during `rounds_at_speed`.
    pub(crate) steps_at_speed: u64,
}

impl Machine {
    pub fn new_random(num_states: usize, num_symbols: usize, width: usize, height: usize) -> Self {
        assert!(num_states >= MIN_STATES && num_states <= MAX_STATES);
        assert!(num_symbols >= MIN_SYMBOLS && num_symbols <= MAX_SYMBOLS);
        assert!(validate_map_size(width, height).is_ok());

        let mut rng = rand::rng();
        let mut table = vec![0i32; num_states * num_symbols * 3];

        for st in 0..num_states {
            for sy in 0..num_symbols {
                let (next_st, write_sy, action) = random_trans(&mut rng, num_states, num_symbols);
                set_trans_raw(&mut table, num_states, st, sy, next_st, write_sy, action);
            }
        }

        let start_x = rng.random_range(0..width as i32);
        let start_y = rng.random_range(0..height as i32);

        Self {
            table,
            state: 0,
            x_pos: start_x,
            y_pos: start_y,
            start_x,
            start_y,
            speed: 0.0,
            active: true,
            name: String::new(),
            id: 0,
            palette: Palette::classic(),
            rounds_at_speed: 0,
            steps_at_speed: 0,
        }
    }

    /// Label shown in the machine list and inspector. Empty names fall back to
    /// [`default_machine_name`].
    pub fn display_name(&self, index: usize) -> String {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            default_machine_name(index)
        } else {
            trimmed.to_string()
        }
    }

    pub fn set_speed(&mut self, speed: f32) {
        let speed = speed.clamp(MIN_MACHINE_SPEED, MAX_MACHINE_SPEED);
        if speed != self.speed {
            self.speed = speed;
            self.rounds_at_speed = 0;
            self.steps_at_speed = 0;
        }
    }

    /// Accrue this round's floating-point rate and take any whole steps due.
    pub fn take_scheduled_steps(
        &mut self,
        map: &mut [i32],
        canvas: &mut [u8],
        num_states: usize,
        width: i32,
        height: i32,
    ) {
        self.rounds_at_speed += 1;
        let due = (step_rate(self.speed) * self.rounds_at_speed as f64).floor() as u64;
        let steps = due.saturating_sub(self.steps_at_speed);
        self.steps_at_speed = due;
        for _ in 0..steps {
            self.step(map, canvas, num_states, width, height);
        }
    }

    pub fn reset(&mut self) {
        self.state = 0;
        self.x_pos = self.start_x;
        self.y_pos = self.start_y;
        self.rounds_at_speed = 0;
        self.steps_at_speed = 0;
    }

    /// Re-randomize `percent` of `(state, symbol)` rules. Leaves start position,
    /// head, state, speed, name, and active unchanged.
    pub fn mutate_table(&mut self, num_states: usize, num_symbols: usize, percent: u8) {
        let n_rules = num_states * num_symbols;
        debug_assert_eq!(self.table.len(), n_rules * 3);
        let count = mutation_count(n_rules, percent);
        if count == 0 {
            return;
        }

        let mut rng = rand::rng();
        let mut indices: Vec<usize> = (0..n_rules).collect();
        indices.shuffle(&mut rng);
        for &idx in indices.iter().take(count) {
            let st = idx % num_states;
            let sy = idx / num_states;
            let (next_st, write_sy, action) = random_trans(&mut rng, num_states, num_symbols);
            set_trans_raw(
                &mut self.table,
                num_states,
                st,
                sy,
                next_st,
                write_sy,
                action,
            );
        }
    }

    /// One read / write / move on the shared tape. The written symbol is stored
    /// on the tape; the canvas stores this machine's current RGB for that symbol.
    pub fn step(
        &mut self,
        map: &mut [i32],
        canvas: &mut [u8],
        num_states: usize,
        width: i32,
        height: i32,
    ) {
        let idx_map = (width * self.y_pos + self.x_pos) as usize;
        let sy = map[idx_map] as usize;
        let st = self.state as usize;

        let t = trans_index(num_states, st, sy);
        let next_st = self.table[t];
        let write_sy = self.table[t + 1];
        let ac = self.table[t + 2];

        self.state = next_st;
        map[idx_map] = write_sy;
        write_rgb(
            canvas,
            idx_map,
            self.palette.colors[write_sy as usize],
        );

        // Keep original action mapping (LEFT/RIGHT names are swapped vs motion)
        match ac {
            ACTION_LEFT => {
                self.x_pos += 1;
                if self.x_pos >= width {
                    self.x_pos -= width;
                }
            }
            ACTION_RIGHT => {
                self.x_pos -= 1;
                if self.x_pos < 0 {
                    self.x_pos += width;
                }
            }
            ACTION_UP => {
                self.y_pos -= 1;
                if self.y_pos < 0 {
                    self.y_pos += height;
                }
            }
            ACTION_DOWN => {
                self.y_pos += 1;
                if self.y_pos >= height {
                    self.y_pos -= height;
                }
            }
            _ => panic!("invalid action: {ac}"),
        }
    }

    /// Share string: `numStates,numSymbols,startX,startY,` then flat table values.
    pub fn to_string(&self, num_states: usize, num_symbols: usize) -> String {
        let mut parts = Vec::with_capacity(4 + self.table.len());
        parts.push(num_states.to_string());
        parts.push(num_symbols.to_string());
        parts.push(self.start_x.to_string());
        parts.push(self.start_y.to_string());
        for v in &self.table {
            parts.push(v.to_string());
        }
        parts.join(",")
    }

    /// Parse a share string (optional leading `#` from original URLs).
    ///
    /// Accepts the original `numStates,numSymbols,table...` form (start `(0,0)`)
    /// and the extended `numStates,numSymbols,startX,startY,table...` form.
    /// Start coordinates wrap to `width` × `height`.
    pub fn from_string(s: &str, width: usize, height: usize) -> Result<ParsedMachine, String> {
        validate_map_size(width, height)?;
        let s = s.trim().trim_start_matches('#');
        if s.is_empty() {
            return Err("empty encoding".into());
        }

        let nums: Result<Vec<i32>, _> = s.split(',').map(|p| p.trim().parse::<i32>()).collect();
        let nums = nums.map_err(|e| format!("invalid number in encoding: {e}"))?;

        if nums.len() < 2 {
            return Err("encoding too short".into());
        }

        let num_states = nums[0] as usize;
        let num_symbols = nums[1] as usize;

        if num_states < MIN_STATES || num_states > MAX_STATES {
            return Err(format!(
                "num states must be {MIN_STATES}..={MAX_STATES}, got {num_states}"
            ));
        }
        if num_symbols < MIN_SYMBOLS || num_symbols > MAX_SYMBOLS {
            return Err(format!(
                "num symbols must be {MIN_SYMBOLS}..={MAX_SYMBOLS}, got {num_symbols}"
            ));
        }

        let expected = num_states * num_symbols * 3;
        let (start_x, start_y, table) = if nums.len() == 2 + expected {
            (0, 0, nums[2..].to_vec())
        } else if nums.len() == 4 + expected {
            (
                wrap_pos(nums[2], width as i32),
                wrap_pos(nums[3], height as i32),
                nums[4..].to_vec(),
            )
        } else {
            return Err(format!(
                "invalid transition table length: expected {expected} or {} with start position, got {}",
                expected + 2,
                nums.len() - 2
            ));
        };

        let machine = Self {
            table,
            state: 0,
            x_pos: start_x,
            y_pos: start_y,
            start_x,
            start_y,
            speed: 0.0,
            active: true,
            name: String::new(),
            id: 0,
            palette: Palette::classic(),
            rounds_at_speed: 0,
            steps_at_speed: 0,
        };

        Ok(ParsedMachine {
            num_states,
            num_symbols,
            machine,
        })
    }
}

pub fn validate_map_size(width: usize, height: usize) -> Result<(), String> {
    if !(MIN_MAP_SIZE..=MAX_MAP_SIZE).contains(&width) {
        return Err(format!(
            "map width must be {MIN_MAP_SIZE}..={MAX_MAP_SIZE}, got {width}"
        ));
    }
    if !(MIN_MAP_SIZE..=MAX_MAP_SIZE).contains(&height) {
        return Err(format!(
            "map height must be {MIN_MAP_SIZE}..={MAX_MAP_SIZE}, got {height}"
        ));
    }
    Ok(())
}

/// How many of `n_rules` to rewrite for `percent` (1–100).
///
/// Uses round-half-up, then at least one rule when `n_rules > 0`.
pub fn mutation_count(n_rules: usize, percent: u8) -> usize {
    if n_rules == 0 {
        return 0;
    }
    let percent = u32::from(percent.clamp(MIN_MUTATE_PERCENT, MAX_MUTATE_PERCENT));
    let n = n_rules as u32;
    let count = (n * percent + 50) / 100;
    (count as usize).clamp(1, n_rules)
}

fn random_trans<R: Rng>(rng: &mut R, num_states: usize, num_symbols: usize) -> (i32, i32, i32) {
    let next_st = rng.random_range(0..num_states) as i32;
    // Never write symbol 0 (red = untouched), matching the original
    let write_sy = rng.random_range(1..num_symbols) as i32;
    let action = rng.random_range(0..NUM_ACTIONS);
    (next_st, write_sy, action)
}

/// Relative step rate for a speed slider value.
///
/// `0` → 1×, `+10` → 10×, `−10` → 1/10×. Intermediate values use
/// `10^(speed / 10)` so frequency scales continuously through the default.
pub fn step_rate(speed: f32) -> f64 {
    let speed = speed.clamp(MIN_MACHINE_SPEED, MAX_MACHINE_SPEED);
    10f64.powf(f64::from(speed) / 10.0)
}

#[inline]
fn trans_index(num_states: usize, state: usize, symbol: usize) -> usize {
    (num_states * symbol + state) * 3
}

pub(crate) fn wrap_pos(v: i32, dim: i32) -> i32 {
    v.rem_euclid(dim)
}

fn set_trans_raw(
    table: &mut [i32],
    num_states: usize,
    st0: usize,
    sy0: usize,
    st1: i32,
    sy1: i32,
    ac1: i32,
) {
    let idx = trans_index(num_states, st0, sy0);
    table[idx] = st1;
    table[idx + 1] = sy1;
    table[idx + 2] = ac1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_count_rounds_and_has_floor() {
        assert_eq!(mutation_count(12, 10), 1);
        assert_eq!(mutation_count(12, 100), 12);
        assert_eq!(mutation_count(2, 10), 1);
        assert_eq!(mutation_count(0, 10), 0);
        assert_eq!(mutation_count(10, 50), 5);
        assert_eq!(mutation_count(15, 10), 2);
    }
}
