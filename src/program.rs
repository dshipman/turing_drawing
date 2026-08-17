//! Shared tape plus one or more machines that step on it in order.

use crate::machine::{Machine, MAP_LEN};

pub use crate::machine::{
    step_rate, MAP_HEIGHT, MAP_WIDTH, MAX_MACHINE_SPEED, MAX_STATES, MAX_SYMBOLS,
    MIN_MACHINE_SPEED, MIN_STATES, MIN_SYMBOLS,
};

/// The program: shared grid, shared alphabet size, and the machines that draw on it.
#[derive(Debug, Clone)]
pub struct Program {
    pub num_states: usize,
    pub num_symbols: usize,
    pub map: Vec<i32>,
    pub machines: Vec<Machine>,
    pub itr_count: u64,
}

impl Program {
    pub fn new_random(num_states: usize, num_symbols: usize) -> Self {
        assert!(num_states >= MIN_STATES && num_states <= MAX_STATES);
        assert!(num_symbols >= MIN_SYMBOLS && num_symbols <= MAX_SYMBOLS);

        let mut prog = Self {
            num_states,
            num_symbols,
            map: vec![0; MAP_LEN],
            machines: vec![Machine::new_random(num_states, num_symbols)],
            itr_count: 0,
        };
        prog.reset();
        prog
    }

    /// Parse a single-machine encoding into a one-machine program.
    pub fn from_string(s: &str) -> Result<Self, String> {
        let parsed = Machine::from_string(s)?;
        let mut prog = Self {
            num_states: parsed.num_states,
            num_symbols: parsed.num_symbols,
            map: vec![0; MAP_LEN],
            machines: vec![parsed.machine],
            itr_count: 0,
        };
        prog.reset();
        Ok(prog)
    }

    pub fn reset(&mut self) {
        self.itr_count = 0;
        self.map.fill(0);
        for machine in &mut self.machines {
            machine.reset();
        }
    }

    /// Replace every machine with a new random table and start, using `num_states` /
    /// `num_symbols`. Keeps the current machine count.
    pub fn randomize(&mut self, num_states: usize, num_symbols: usize) {
        assert!(num_states >= MIN_STATES && num_states <= MAX_STATES);
        assert!(num_symbols >= MIN_SYMBOLS && num_symbols <= MAX_SYMBOLS);

        self.num_states = num_states;
        self.num_symbols = num_symbols;
        let speeds: Vec<f32> = self.machines.iter().map(|m| m.speed).collect();
        let n = self.machines.len().max(1);
        self.machines = (0..n)
            .map(|_| Machine::new_random(num_states, num_symbols))
            .collect();
        for (machine, speed) in self.machines.iter_mut().zip(speeds) {
            machine.speed = speed;
        }
        self.reset();
    }

    /// Replace one machine with a new random table and start, then reset.
    /// Uses the program's current state/symbol counts so other machines stay valid.
    /// Keeps the slot's speed slider.
    pub fn randomize_machine(&mut self, index: usize) -> Result<(), String> {
        if index >= self.machines.len() {
            return Err("invalid machine index".into());
        }
        let speed = self.machines[index].speed;
        self.machines[index] = Machine::new_random(self.num_states, self.num_symbols);
        self.machines[index].speed = speed;
        self.reset();
        Ok(())
    }

    /// Set one machine's relative speed slider (`MIN_MACHINE_SPEED`..=`MAX_MACHINE_SPEED`).
    pub fn set_machine_speed(&mut self, index: usize, speed: f32) -> Result<(), String> {
        let Some(machine) = self.machines.get_mut(index) else {
            return Err("invalid machine index".into());
        };
        machine.set_speed(speed);
        Ok(())
    }

    /// Append a random machine with the current counts, then reset the program.
    pub fn add_machine(&mut self) {
        self.machines
            .push(Machine::new_random(self.num_states, self.num_symbols));
        self.reset();
    }

    /// Remove a machine (not the last one), then reset the program.
    pub fn remove_machine(&mut self, index: usize) -> Result<(), String> {
        if self.machines.len() <= 1 {
            return Err("cannot remove the last machine".into());
        }
        if index >= self.machines.len() {
            return Err("invalid machine index".into());
        }
        self.machines.remove(index);
        self.reset();
        Ok(())
    }

    /// Load an encoding into `index`. If this is the only machine, shared counts
    /// may change. With multiple machines, counts must match.
    pub fn load_machine(&mut self, index: usize, s: &str) -> Result<(), String> {
        if index >= self.machines.len() {
            return Err("invalid machine index".into());
        }

        let parsed = Machine::from_string(s)?;
        let speed = self.machines[index].speed;
        if self.machines.len() == 1 {
            self.num_states = parsed.num_states;
            self.num_symbols = parsed.num_symbols;
            self.machines[0] = parsed.machine;
            self.machines[0].speed = speed;
            self.reset();
            return Ok(());
        }

        if parsed.num_states != self.num_states || parsed.num_symbols != self.num_symbols {
            return Err(format!(
                "machine must have {} states and {} symbols",
                self.num_states, self.num_symbols
            ));
        }

        self.machines[index] = parsed.machine;
        self.machines[index].speed = speed;
        self.reset();
        Ok(())
    }

    pub fn machine_encoding(&self, index: usize) -> String {
        self.machines[index].to_string(self.num_states, self.num_symbols)
    }

    /// Run `num_itrs` interleaved rounds. Each machine accrues its floating-point
    /// step rate and takes any whole steps that are due (default: one per round).
    pub fn update(&mut self, num_itrs: usize) {
        let width = MAP_WIDTH as i32;
        let height = MAP_HEIGHT as i32;
        let num_states = self.num_states;

        for _ in 0..num_itrs {
            for machine in &mut self.machines {
                machine.take_scheduled_steps(&mut self.map, num_states, width, height);
            }
            self.itr_count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::{Machine, ACTION_DOWN, ACTION_LEFT, ACTION_RIGHT};

    fn fixed_machine(table: Vec<i32>, start_x: i32, start_y: i32) -> Machine {
        Machine {
            table,
            state: 0,
            x_pos: start_x,
            y_pos: start_y,
            start_x,
            start_y,
            speed: 0.0,
            rounds_at_speed: 0,
            steps_at_speed: 0,
        }
    }

    fn fixed_program(
        num_states: usize,
        num_symbols: usize,
        table: Vec<i32>,
        start_x: i32,
        start_y: i32,
    ) -> Program {
        Program {
            num_states,
            num_symbols,
            map: vec![0; MAP_LEN],
            machines: vec![fixed_machine(table, start_x, start_y)],
            itr_count: 0,
        }
    }

    #[test]
    fn roundtrip_encoding() {
        let p = Program::new_random(4, 3);
        let s = p.machine_encoding(0);
        let q = Program::from_string(&s).unwrap();
        assert_eq!(p.num_states, q.num_states);
        assert_eq!(p.num_symbols, q.num_symbols);
        assert_eq!(p.machines[0].table, q.machines[0].table);
        assert_eq!(p.machines[0].start_x, q.machines[0].start_x);
        assert_eq!(p.machines[0].start_y, q.machines[0].start_y);
    }

    #[test]
    fn from_string_strips_hash() {
        let p = Program::new_random(2, 2);
        let s = format!("#{}", p.machine_encoding(0));
        let q = Program::from_string(&s).unwrap();
        assert_eq!(p.machines[0].table, q.machines[0].table);
        assert_eq!(p.machines[0].start_x, q.machines[0].start_x);
        assert_eq!(p.machines[0].start_y, q.machines[0].start_y);
    }

    #[test]
    fn original_format_loads_at_origin() {
        // 1 state × 2 symbols × 3 = 6 table entries, no start fields.
        let enc = "1,2,0,1,0,0,1,0";
        let p = Program::from_string(enc).unwrap();
        assert_eq!(p.num_states, 1);
        assert_eq!(p.num_symbols, 2);
        assert_eq!(p.machines[0].start_x, 0);
        assert_eq!(p.machines[0].start_y, 0);
        assert_eq!(p.machines[0].x_pos, 0);
        assert_eq!(p.machines[0].y_pos, 0);
        assert_eq!(p.machines[0].table, vec![0, 1, 0, 0, 1, 0]);
    }

    #[test]
    fn wrap_left_increments_x() {
        // Fixed machine: state 0, symbol 0 -> stay state 0, write 1, ACTION_LEFT (x += 1)
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_LEFT], 0, 0);
        p.update(1);
        assert_eq!(p.machines[0].x_pos, 1);
        assert_eq!(p.machines[0].y_pos, 0);
        assert_eq!(p.map[0], 1);

        // Wrap at right edge
        p.machines[0].x_pos = (MAP_WIDTH - 1) as i32;
        p.map[MAP_WIDTH - 1] = 0;
        p.update(1);
        assert_eq!(p.machines[0].x_pos, 0);
    }

    #[test]
    fn wrap_right_decrements_x() {
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_RIGHT], 0, 0);
        p.update(1);
        assert_eq!(p.machines[0].x_pos, (MAP_WIDTH - 1) as i32);
    }

    #[test]
    fn random_never_writes_zero() {
        let p = Program::new_random(4, 3);
        let m = &p.machines[0];
        for sy0 in 0..p.num_symbols {
            for st0 in 0..p.num_states {
                let idx = (p.num_states * sy0 + st0) * 3;
                let write = m.table[idx + 1];
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
        assert_eq!(p.machines[0].table.len(), 36);
        assert_eq!(p.machines[0].start_x, 0);
        assert_eq!(p.machines[0].start_y, 0);

        p.update(100);
        assert_eq!(p.itr_count, 100);
        assert!(p.map.iter().any(|&s| s != 0));

        p.reset();
        assert_eq!(p.itr_count, 0);
        assert_eq!(p.machines[0].x_pos, 0);
        assert_eq!(p.machines[0].y_pos, 0);
        assert_eq!(p.machines[0].state, 0);
        assert!(p.map.iter().all(|&s| s == 0));
    }

    #[test]
    fn reset_restores_start_position() {
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_LEFT], 10, 20);
        p.update(3);
        assert_ne!(p.machines[0].x_pos, 10);
        p.reset();
        assert_eq!(p.machines[0].x_pos, 10);
        assert_eq!(p.machines[0].y_pos, 20);
        assert_eq!(p.itr_count, 0);
    }

    #[test]
    fn add_machine_resets_program() {
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_LEFT], 0, 0);
        p.update(50);
        assert_eq!(p.itr_count, 50);
        assert!(p.map.iter().any(|&s| s != 0));

        p.add_machine();
        assert_eq!(p.machines.len(), 2);
        assert_eq!(p.itr_count, 0);
        assert!(p.map.iter().all(|&s| s == 0));
        assert_eq!(p.machines[0].x_pos, p.machines[0].start_x);
        assert_eq!(p.machines[0].y_pos, p.machines[0].start_y);
        assert_eq!(p.machines[1].x_pos, p.machines[1].start_x);
        assert_eq!(p.machines[1].y_pos, p.machines[1].start_y);
    }

    #[test]
    fn interleaved_second_machine_sees_first_write() {
        // Both start at (0,0). Machine 0 writes 1 and moves. Machine 1 writes 2
        // only when it reads symbol 1 — so map[0] == 2 means it saw the write.
        let m0 = fixed_machine(vec![0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT], 0, 0);
        let m1 = fixed_machine(vec![0, 1, ACTION_LEFT, 0, 2, ACTION_RIGHT, 0, 2, ACTION_RIGHT], 0, 0);
        let mut p = Program {
            num_states: 1,
            num_symbols: 3,
            map: vec![0; MAP_LEN],
            machines: vec![m0, m1],
            itr_count: 0,
        };

        p.update(1);
        assert_eq!(p.map[0], 2);
        assert_eq!(p.machines[0].x_pos, 1);
        assert_eq!(p.machines[1].x_pos, (MAP_WIDTH - 1) as i32);
        assert_eq!(p.itr_count, 1);
    }

    #[test]
    fn load_rejects_mismatched_counts_when_multiple_machines() {
        let mut p = Program::new_random(4, 3);
        p.add_machine();
        assert_eq!(p.machines.len(), 2);

        let other = Machine::new_random(2, 2);
        let enc = other.to_string(2, 2);
        let err = p.load_machine(0, &enc).unwrap_err();
        assert!(err.contains("4 states") && err.contains("3 symbols"));
        assert_eq!(p.machines.len(), 2);
        assert_eq!(p.num_states, 4);
        assert_eq!(p.num_symbols, 3);
    }

    #[test]
    fn load_single_machine_may_change_counts() {
        let mut p = Program::new_random(4, 3);
        let other = Machine::new_random(2, 2);
        let enc = other.to_string(2, 2);
        p.load_machine(0, &enc).unwrap();
        assert_eq!(p.num_states, 2);
        assert_eq!(p.num_symbols, 2);
        assert_eq!(p.machines[0].table, other.table);
        assert_eq!(p.itr_count, 0);
    }

    #[test]
    fn cannot_remove_last_machine() {
        let mut p = Program::new_random(2, 2);
        assert!(p.remove_machine(0).is_err());
        assert_eq!(p.machines.len(), 1);
    }

    #[test]
    fn randomize_machine_replaces_only_that_machine() {
        let mut p = Program::new_random(4, 3);
        p.add_machine();
        let other = p.machines[1].clone();
        p.update(10);
        assert!(p.itr_count > 0);

        p.randomize_machine(0).unwrap();
        assert_eq!(p.machines.len(), 2);
        assert_eq!(p.machines[1].table, other.table);
        assert_eq!(p.machines[1].start_x, other.start_x);
        assert_eq!(p.machines[1].start_y, other.start_y);
        assert_eq!(p.itr_count, 0);
        assert!(p.map.iter().all(|&s| s == 0));
        assert_eq!(p.machines[0].x_pos, p.machines[0].start_x);
        assert_eq!(p.machines[1].x_pos, p.machines[1].start_x);
        assert!(p.randomize_machine(5).is_err());
    }

    #[test]
    fn machine_speed_scales_steps() {
        // Always move +x, independent of the symbol read.
        let table = vec![0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT];
        let fast = {
            let mut m = fixed_machine(table.clone(), 0, 0);
            m.speed = 10.0;
            m
        };
        let slow = {
            let mut m = fixed_machine(table.clone(), 0, 1);
            m.speed = -10.0;
            m
        };
        let mid = {
            let mut m = fixed_machine(table.clone(), 0, 3);
            m.speed = 5.0;
            m
        };
        let normal = fixed_machine(table, 0, 2);
        let mut p = Program {
            num_states: 1,
            num_symbols: 2,
            map: vec![0; MAP_LEN],
            machines: vec![fast, slow, normal, mid],
            itr_count: 0,
        };

        p.update(10);
        assert_eq!(p.machines[0].x_pos, 100);
        assert_eq!(p.machines[1].x_pos, 1);
        assert_eq!(p.machines[2].x_pos, 10);
        assert_eq!(
            p.machines[3].x_pos,
            (step_rate(5.0) * 10.0).floor() as i32
        );
    }

    #[test]
    fn step_rate_is_continuous_through_default() {
        assert!((step_rate(0.0) - 1.0).abs() < 1e-12);
        assert!((step_rate(10.0) - 10.0).abs() < 1e-12);
        assert!((step_rate(-10.0) - 0.1).abs() < 1e-12);
        assert!(step_rate(2.5) > 1.0 && step_rate(2.5) < step_rate(5.0));
        assert!(step_rate(-2.5) < 1.0 && step_rate(-2.5) > step_rate(-5.0));
    }

    #[test]
    fn set_machine_speed_clamps_and_is_kept_on_randomize() {
        let mut p = Program::new_random(2, 2);
        p.set_machine_speed(0, 7.25).unwrap();
        assert_eq!(p.machines[0].speed, 7.25);
        p.set_machine_speed(0, 99.0).unwrap();
        assert_eq!(p.machines[0].speed, MAX_MACHINE_SPEED);
        p.set_machine_speed(0, -99.0).unwrap();
        assert_eq!(p.machines[0].speed, MIN_MACHINE_SPEED);
        assert!(p.set_machine_speed(3, 0.0).is_err());

        p.set_machine_speed(0, 4.5).unwrap();
        p.randomize_machine(0).unwrap();
        assert_eq!(p.machines[0].speed, 4.5);
    }
}
