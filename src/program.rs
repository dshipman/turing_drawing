//! 2D Turing-machine engine compatible with the original Turing Drawings JS demo.
//!
//! Table layout, action mapping (including swapped left/right), and share-string
//! encoding match https://github.com/maximecb/Turing-Drawings

use rand::Rng;

pub const MAP_WIDTH: usize = 512;
pub const MAP_HEIGHT: usize = 512;
pub const MAP_LEN: usize = MAP_WIDTH * MAP_HEIGHT;

pub const ACTION_LEFT: i32 = 0;
pub const ACTION_RIGHT: i32 = 1;
pub const ACTION_UP: i32 = 2;
pub const ACTION_DOWN: i32 = 3;
pub const NUM_ACTIONS: i32 = 4;

pub const MIN_STATES: usize = 1;
pub const MAX_STATES: usize = 32;
pub const MIN_SYMBOLS: usize = 2;
pub const MAX_SYMBOLS: usize = 8;

/// Transition: (next_state, write_symbol, action)
#[derive(Debug, Clone)]
pub struct Program {
    pub num_states: usize,
    pub num_symbols: usize,
    /// Flat table: for each (symbol, state), three i32s [next_state, write_symbol, action]
    pub table: Vec<i32>,
    pub map: Vec<i32>,
    pub state: i32,
    pub x_pos: i32,
    pub y_pos: i32,
    pub itr_count: u64,
}

impl Program {
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

        let mut prog = Self {
            num_states,
            num_symbols,
            table,
            map: vec![0; MAP_LEN],
            state: 0,
            x_pos: 0,
            y_pos: 0,
            itr_count: 0,
        };
        prog.reset();
        prog
    }

    pub fn reset(&mut self) {
        self.state = 0;
        self.x_pos = 0;
        self.y_pos = 0;
        self.itr_count = 0;
        self.map.fill(0);
    }

    #[inline]
    fn trans_index(num_states: usize, state: usize, symbol: usize) -> usize {
        (num_states * symbol + state) * 3
    }

    #[allow(dead_code)]
    pub fn set_trans(&mut self, st0: usize, sy0: usize, st1: i32, sy1: i32, ac1: i32) {
        set_trans_raw(&mut self.table, self.num_states, st0, sy0, st1, sy1, ac1);
    }

    /// Run `num_itrs` machine steps.
    pub fn update(&mut self, num_itrs: usize) {
        let width = self.map_width() as i32;
        let height = self.map_height() as i32;
        let num_states = self.num_states;

        for _ in 0..num_itrs {
            let idx_map = (width * self.y_pos + self.x_pos) as usize;
            let sy = self.map[idx_map] as usize;
            let st = self.state as usize;

            let t = Self::trans_index(num_states, st, sy);
            let next_st = self.table[t];
            let write_sy = self.table[t + 1];
            let ac = self.table[t + 2];

            self.state = next_st;
            self.map[idx_map] = write_sy;

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

            self.itr_count += 1;
        }
    }

    pub fn map_width(&self) -> usize {
        MAP_WIDTH
    }

    pub fn map_height(&self) -> usize {
        MAP_HEIGHT
    }

    /// Share string: `numStates,numSymbols,` then flat table values.
    pub fn to_string(&self) -> String {
        let mut parts = Vec::with_capacity(2 + self.table.len());
        parts.push(self.num_states.to_string());
        parts.push(self.num_symbols.to_string());
        for v in &self.table {
            parts.push(v.to_string());
        }
        parts.join(",")
    }

    /// Parse a share string (optional leading `#` from original URLs).
    pub fn from_string(s: &str) -> Result<Self, String> {
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
        if nums.len() - 2 != expected {
            return Err(format!(
                "invalid transition table length: expected {expected}, got {}",
                nums.len() - 2
            ));
        }

        let table = nums[2..].to_vec();

        let mut prog = Self {
            num_states,
            num_symbols,
            table,
            map: vec![0; MAP_LEN],
            state: 0,
            x_pos: 0,
            y_pos: 0,
            itr_count: 0,
        };
        prog.reset();
        Ok(prog)
    }
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
    let idx = (num_states * sy0 + st0) * 3;
    table[idx] = st1;
    table[idx + 1] = sy1;
    table[idx + 2] = ac1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encoding() {
        let p = Program::new_random(4, 3);
        let s = p.to_string();
        let q = Program::from_string(&s).unwrap();
        assert_eq!(p.num_states, q.num_states);
        assert_eq!(p.num_symbols, q.num_symbols);
        assert_eq!(p.table, q.table);
    }

    #[test]
    fn from_string_strips_hash() {
        let p = Program::new_random(2, 2);
        let s = format!("#{}", p.to_string());
        let q = Program::from_string(&s).unwrap();
        assert_eq!(p.table, q.table);
    }

    #[test]
    fn wrap_left_increments_x() {
        // Fixed machine: state 0, symbol 0 -> stay state 0, write 1, ACTION_LEFT (x += 1)
        let mut p = Program {
            num_states: 1,
            num_symbols: 2,
            table: vec![0, 1, ACTION_LEFT],
            map: vec![0; MAP_LEN],
            state: 0,
            x_pos: 0,
            y_pos: 0,
            itr_count: 0,
        };
        p.update(1);
        assert_eq!(p.x_pos, 1);
        assert_eq!(p.y_pos, 0);
        assert_eq!(p.map[0], 1);

        // Wrap at right edge
        p.x_pos = (MAP_WIDTH - 1) as i32;
        p.map[MAP_WIDTH - 1] = 0;
        p.update(1);
        assert_eq!(p.x_pos, 0);
    }

    #[test]
    fn wrap_right_decrements_x() {
        let mut p = Program {
            num_states: 1,
            num_symbols: 2,
            table: vec![0, 1, ACTION_RIGHT],
            map: vec![0; MAP_LEN],
            state: 0,
            x_pos: 0,
            y_pos: 0,
            itr_count: 0,
        };
        p.update(1);
        assert_eq!(p.x_pos, (MAP_WIDTH - 1) as i32);
    }

    #[test]
    fn random_never_writes_zero() {
        let p = Program::new_random(4, 3);
        for sy0 in 0..p.num_symbols {
            for st0 in 0..p.num_states {
                let idx = (p.num_states * sy0 + st0) * 3;
                let write = p.table[idx + 1];
                assert!(write >= 1 && write < p.num_symbols as i32);
            }
        }
    }

    #[test]
    fn reject_bad_encoding() {
        assert!(Program::from_string("").is_err());
        assert!(Program::from_string("4,3,1").is_err());
        assert!(Program::from_string("0,3").is_err());
    }

    #[test]
    fn original_share_string_and_restart() {
        // Hand-built encoding in the original `numStates,numSymbols,table...` format.
        // 4 states × 3 symbols × 3 = 36 table entries.
        let mut table = Vec::new();
        for _sy in 0..3 {
            for _st in 0..4 {
                table.push(1); // next state
                table.push(1); // write symbol (never 0)
                table.push(ACTION_DOWN);
            }
        }
        let parts: Vec<String> = std::iter::once("4".into())
            .chain(std::iter::once("3".into()))
            .chain(table.into_iter().map(|v| v.to_string()))
            .collect();
        let enc = format!("#{}", parts.join(","));

        let mut p = Program::from_string(&enc).unwrap();
        assert_eq!(p.num_states, 4);
        assert_eq!(p.num_symbols, 3);
        assert_eq!(p.table.len(), 36);

        p.update(100);
        assert_eq!(p.itr_count, 100);
        assert!(p.map.iter().any(|&s| s != 0));

        p.reset();
        assert_eq!(p.itr_count, 0);
        assert_eq!(p.x_pos, 0);
        assert_eq!(p.y_pos, 0);
        assert_eq!(p.state, 0);
        assert!(p.map.iter().all(|&s| s == 0));
    }
}
