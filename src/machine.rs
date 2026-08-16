//! One Turing machine: transition table, head, and start position.
//!
//! Table layout, action mapping (including swapped left/right), and share-string
//! encoding match https://github.com/maximecb/Turing-Drawings, with an optional
//! start-position prefix for this port.

use rand::Rng;

pub const MAP_WIDTH: usize = 512;
pub const MAP_HEIGHT: usize = 512;
pub const MAP_LEN: usize = MAP_WIDTH * MAP_HEIGHT;

pub const ACTION_LEFT: i32 = 0;
pub const ACTION_RIGHT: i32 = 1;
const ACTION_UP: i32 = 2;
pub const ACTION_DOWN: i32 = 3;
const NUM_ACTIONS: i32 = 4;

pub const MIN_STATES: usize = 1;
pub const MAX_STATES: usize = 32;
pub const MIN_SYMBOLS: usize = 2;
pub const MAX_SYMBOLS: usize = 8;

/// A decoded share string: dimensions plus the machine they describe.
#[derive(Debug, Clone)]
pub struct ParsedMachine {
    pub num_states: usize,
    pub num_symbols: usize,
    pub machine: Machine,
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
}

impl Machine {
    pub fn new_random(num_states: usize, num_symbols: usize) -> Self {
        assert!(num_states >= MIN_STATES && num_states <= MAX_STATES);
        assert!(num_symbols >= MIN_SYMBOLS && num_symbols <= MAX_SYMBOLS);

        let mut rng = rand::rng();
        let mut table = vec![0i32; num_states * num_symbols * 3];

        for st in 0..num_states {
            for sy in 0..num_symbols {
                let next_st = rng.random_range(0..num_states) as i32;
                // Never write symbol 0 (red = untouched), matching the original
                let write_sy = rng.random_range(1..num_symbols) as i32;
                let action = rng.random_range(0..NUM_ACTIONS);
                set_trans_raw(&mut table, num_states, st, sy, next_st, write_sy, action);
            }
        }

        let start_x = rng.random_range(0..MAP_WIDTH as i32);
        let start_y = rng.random_range(0..MAP_HEIGHT as i32);

        Self {
            table,
            state: 0,
            x_pos: start_x,
            y_pos: start_y,
            start_x,
            start_y,
        }
    }

    pub fn reset(&mut self) {
        self.state = 0;
        self.x_pos = self.start_x;
        self.y_pos = self.start_y;
    }

    /// One read / write / move on the shared tape.
    pub fn step(&mut self, map: &mut [i32], num_states: usize, width: i32, height: i32) {
        let idx_map = (width * self.y_pos + self.x_pos) as usize;
        let sy = map[idx_map] as usize;
        let st = self.state as usize;

        let t = trans_index(num_states, st, sy);
        let next_st = self.table[t];
        let write_sy = self.table[t + 1];
        let ac = self.table[t + 2];

        self.state = next_st;
        map[idx_map] = write_sy;

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
    pub fn from_string(s: &str) -> Result<ParsedMachine, String> {
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
                wrap_pos(nums[2], MAP_WIDTH as i32),
                wrap_pos(nums[3], MAP_HEIGHT as i32),
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
        };

        Ok(ParsedMachine {
            num_states,
            num_symbols,
            machine,
        })
    }
}

#[inline]
fn trans_index(num_states: usize, state: usize, symbol: usize) -> usize {
    (num_states * symbol + state) * 3
}

fn wrap_pos(v: i32, dim: i32) -> i32 {
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
