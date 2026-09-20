//! Shared tape plus one or more machines that step on it in order.

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::dirty::DirtyRect;
use crate::machine::{table_has_diagonal_actions, validate_map_size, wrap_pos, Machine};
use crate::palette::{empty_canvas, fill_rgba_from_map, Palette};
use crate::tape::{self, TapeInit};

pub use crate::machine::{
    default_machine_name, mutation_count, step_rate, DEFAULT_MAP_HEIGHT, DEFAULT_MAP_WIDTH,
    DEFAULT_MUTATE_PERCENT, MACHINE_SPEED_STEP, MAX_MACHINE_SPEED, MAX_MAP_SIZE,
    MAX_MUTATE_PERCENT, MAX_STATES, MAX_SYMBOLS, MIN_MACHINE_SPEED, MIN_MAP_SIZE,
    MIN_MUTATE_PERCENT, MIN_STATES, MIN_SYMBOLS,
};

/// How per-machine speed sliders map to steps within each simulation round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleMode {
    /// Each machine steps at `step_rate(speed)` independently.
    Absolute,
    /// Active machines share a fixed budget of 1.0 steps/round, split by weight (default).
    #[default]
    Normalised,
}

impl ScheduleMode {
    pub const ALL: [ScheduleMode; 2] = [Self::Absolute, Self::Normalised];
}

impl std::fmt::Display for ScheduleMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Absolute => "Absolute",
            Self::Normalised => "Normalised",
        })
    }
}

/// Where `index` lands after `remove(from)` then `insert(to)`.
pub fn remap_index_after_reorder(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < to && index > from && index <= to {
        index - 1
    } else if to < from && index >= to && index < from {
        index + 1
    } else {
        index
    }
}

/// Named capture of every machine's speed slider, in list order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeedSnapshot {
    pub name: String,
    pub speeds: Vec<f32>,
}

/// The program: shared grid, shared alphabet size, and the machines that draw on it.
#[derive(Debug, Clone)]
pub struct Program {
    pub num_symbols: usize,
    pub width: usize,
    pub height: usize,
    pub map: Vec<i32>,
    /// Baked RGBA canvas (4 bytes per cell).
    pub canvas: Vec<u8>,
    /// Colours used when Restart / Reseed fills the canvas from the tape.
    pub canvas_palette: Palette,
    pub canvas_revision: u64,
    pub(crate) canvas_dirty: Option<DirtyRect>,
    pub machines: Vec<Machine>,
    pub itr_count: u64,
    /// How [`Self::reset`] fills the tape. Empty (all zeros) is the default.
    pub tape_init: TapeInit,
    /// Absolute vs normalised step-rate scheduling (session preference).
    pub schedule_mode: ScheduleMode,
    /// When true, Random / Mutate / Add machine may pick diagonal move actions.
    pub allow_diagonals: bool,
    /// Named speed snapshots for the current machine list (session + presets).
    pub speed_snapshots: Vec<SpeedSnapshot>,
    /// Machine layout key paired with [`Self::speed_snapshots`]; not persisted.
    pub(crate) speed_snapshot_key: u64,
}

impl Program {
    pub fn new_random(num_states: usize, num_symbols: usize) -> Self {
        Self::new_random_sized(
            num_states,
            num_symbols,
            DEFAULT_MAP_WIDTH,
            DEFAULT_MAP_HEIGHT,
        )
    }

    pub fn new_random_sized(
        num_states: usize,
        num_symbols: usize,
        width: usize,
        height: usize,
    ) -> Self {
        Self::new_random_sized_with_diagonals(num_states, num_symbols, width, height, false)
    }

    pub fn new_random_sized_with_diagonals(
        num_states: usize,
        num_symbols: usize,
        width: usize,
        height: usize,
        allow_diagonals: bool,
    ) -> Self {
        assert!(num_states >= MIN_STATES && num_states <= MAX_STATES);
        assert!(num_symbols >= MIN_SYMBOLS && num_symbols <= MAX_SYMBOLS);
        assert!(validate_map_size(width, height).is_ok());

        let mut machine =
            Machine::new_random(num_states, num_symbols, width, height, allow_diagonals);
        machine.name = default_machine_name(0);
        let canvas_palette = Palette::classic();
        machine.palette = canvas_palette.clone();
        let cells = width * height;
        let mut prog = Self {
            num_symbols,
            width,
            height,
            map: vec![0; cells],
            canvas: empty_canvas(cells),
            canvas_palette,
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![machine],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Normalised,
            allow_diagonals,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        };
        prog.ensure_machine_ids();
        prog.reset();
        prog.speed_snapshot_key = prog.machine_layout_key();
        prog
    }

    /// Parse a single-machine encoding into a one-machine program on the default canvas.
    pub fn from_string(s: &str) -> Result<Self, String> {
        let mut parsed = Machine::from_string(s, DEFAULT_MAP_WIDTH, DEFAULT_MAP_HEIGHT)?;
        parsed.machine.name = default_machine_name(0);
        let canvas_palette = Palette::classic();
        parsed.machine.palette = canvas_palette.clone();
        let allow_diagonals = table_has_diagonal_actions(&parsed.machine.table);
        let cells = DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT;
        let mut prog = Self {
            num_symbols: parsed.num_symbols,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; cells],
            canvas: empty_canvas(cells),
            canvas_palette,
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![parsed.machine],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Normalised,
            allow_diagonals,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        };
        prog.ensure_machine_ids();
        prog.reset();
        prog.speed_snapshot_key = prog.machine_layout_key();
        Ok(prog)
    }

    pub fn reset(&mut self) {
        self.itr_count = 0;
        tape::fill(
            &mut self.map,
            self.width,
            self.height,
            self.num_symbols,
            &self.tape_init,
        );
        if self.canvas.len() != self.map.len() * 4 {
            self.canvas.resize(self.map.len() * 4, 0);
        }
        fill_rgba_from_map(&self.map, &mut self.canvas, &self.canvas_palette.colors);
        self.mark_canvas_dirty(DirtyRect::full(self.width as u32, self.height as u32));
        for machine in &mut self.machines {
            machine.reset();
        }
    }

    /// Rebuild every palette's colour table for the current symbol count.
    pub fn resolve_palettes(&mut self) {
        self.canvas_palette.resolve(self.num_symbols);
        for machine in &mut self.machines {
            machine.palette.resolve(self.num_symbols);
        }
    }

    /// Pick a new tape seed and refill from the current generator.
    pub fn reseed_tape(&mut self) {
        self.tape_init.reseed();
        self.reset();
    }

    /// Replace the tape generator (keeps the seed unless `init` supplies another) and reset.
    pub fn set_tape_init(&mut self, mut init: TapeInit) {
        init.sanitize();
        self.tape_init = init;
        self.reset();
    }

    /// Resize the canvas, wrap start positions, and reset the drawing.
    pub fn set_size(&mut self, width: usize, height: usize) -> Result<(), String> {
        validate_map_size(width, height)?;
        if width == self.width && height == self.height {
            return Ok(());
        }
        self.width = width;
        self.height = height;
        self.map = vec![0; width * height];
        self.canvas = empty_canvas(width * height);
        let w = width as i32;
        let h = height as i32;
        for machine in &mut self.machines {
            machine.start_x = wrap_pos(machine.start_x, w);
            machine.start_y = wrap_pos(machine.start_y, h);
        }
        self.reset();
        Ok(())
    }

    /// Replace every machine with a new random table and start, using each
    /// machine's own state count and the shared `num_symbols`. Keeps the current
    /// machine count, names, ids, palettes, and speed sliders.
    pub fn randomize(&mut self, num_symbols: usize) {
        assert!(num_symbols >= MIN_SYMBOLS && num_symbols <= MAX_SYMBOLS);

        self.num_symbols = num_symbols;
        let speeds: Vec<f32> = self.machines.iter().map(|m| m.speed).collect();
        let names: Vec<String> = self.machines.iter().map(|m| m.name.clone()).collect();
        let ids: Vec<u64> = self.machines.iter().map(|m| m.id).collect();
        let palettes: Vec<Palette> = self.machines.iter().map(|m| m.palette.clone()).collect();
        let state_counts: Vec<usize> = self.machines.iter().map(|m| m.num_states).collect();
        let n = self.machines.len().max(1);
        let (width, height) = (self.width, self.height);
        let allow_diagonals = self.allow_diagonals;
        self.machines = (0..n)
            .map(|i| {
                let num_states = state_counts.get(i).copied().unwrap_or(MIN_STATES);
                Machine::new_random(num_states, num_symbols, width, height, allow_diagonals)
            })
            .collect();
        for (i, machine) in self.machines.iter_mut().enumerate() {
            if let Some(speed) = speeds.get(i) {
                machine.speed = *speed;
            }
            if let Some(id) = ids.get(i) {
                machine.id = *id;
            }
            if let Some(palette) = palettes.get(i) {
                machine.palette = palette.clone();
            }
            machine.name = names
                .get(i)
                .cloned()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| default_machine_name(i));
        }
        self.resolve_palettes();
        self.ensure_machine_ids();
        self.reset();
        for (i, machine) in self.machines.iter_mut().enumerate() {
            if let Some(palette) = palettes.get(i) {
                machine.palette = palette.clone();
            }
        }
    }

    /// Replace one machine with a new random table and start, then reset.
    /// Uses that machine's state count and the program's symbol count.
    /// Keeps the slot's speed slider, active flag, and name.
    pub fn randomize_machine(&mut self, index: usize) -> Result<(), String> {
        if index >= self.machines.len() {
            return Err("invalid machine index".into());
        }
        let speed = self.machines[index].speed;
        let active = self.machines[index].active;
        let name = self.machines[index].name.clone();
        let id = self.machines[index].id;
        let palette = self.machines[index].palette.clone();
        let num_states = self.machines[index].num_states;
        let allow_diagonals = self.allow_diagonals;
        self.machines[index] = Machine::new_random(
            num_states,
            self.num_symbols,
            self.width,
            self.height,
            allow_diagonals,
        );
        self.machines[index].speed = speed;
        self.machines[index].active = active;
        self.machines[index].name = name;
        self.machines[index].id = id;
        self.machines[index].palette = palette;
        self.reset();
        Ok(())
    }

    /// Re-randomize `percent` of one machine's transition rules. Does not reset.
    pub fn mutate_machine(&mut self, index: usize, percent: u8) -> Result<(), String> {
        let num_symbols = self.num_symbols;
        let allow_diagonals = self.allow_diagonals;
        let Some(machine) = self.machines.get_mut(index) else {
            return Err("invalid machine index".into());
        };
        machine.mutate_table(num_symbols, percent, allow_diagonals);
        Ok(())
    }

    /// Re-randomize `percent` of every machine's transition rules. Does not reset.
    pub fn mutate_all(&mut self, percent: u8) {
        let num_symbols = self.num_symbols;
        let allow_diagonals = self.allow_diagonals;
        for machine in &mut self.machines {
            machine.mutate_table(num_symbols, percent, allow_diagonals);
        }
    }

    /// Set one machine's persistent start cell, wrap to the canvas, and reset.
    pub fn set_machine_start(&mut self, index: usize, x: i32, y: i32) -> Result<(), String> {
        let Some(machine) = self.machines.get_mut(index) else {
            return Err("invalid machine index".into());
        };
        machine.start_x = wrap_pos(x, self.width as i32);
        machine.start_y = wrap_pos(y, self.height as i32);
        self.reset();
        Ok(())
    }

    /// Set one machine's relative speed slider (`MIN_MACHINE_SPEED`..=`MAX_MACHINE_SPEED`).
    pub fn set_machine_speed(&mut self, index: usize, speed: f32) -> Result<(), String> {
        let Some(machine) = self.machines.get_mut(index) else {
            return Err("invalid machine index".into());
        };
        machine.set_speed(speed);
        if self.schedule_mode == ScheduleMode::Normalised {
            self.reset_schedule_accumulators();
        }
        Ok(())
    }

    /// Enable or disable stepping for one machine.
    pub fn set_machine_active(&mut self, index: usize, active: bool) -> Result<(), String> {
        let Some(machine) = self.machines.get_mut(index) else {
            return Err("invalid machine index".into());
        };
        if machine.active == active {
            return Ok(());
        }
        machine.active = active;
        if self.schedule_mode == ScheduleMode::Normalised {
            self.reset_schedule_accumulators();
        }
        Ok(())
    }

    /// Absolute vs normalised step-rate scheduling.
    pub fn set_schedule_mode(&mut self, mode: ScheduleMode) {
        if self.schedule_mode != mode {
            self.schedule_mode = mode;
            self.reset_schedule_accumulators();
        }
    }

    /// Enable or disable diagonal actions for Random / Mutate / Add machine.
    /// Does not rewrite existing tables or reset the drawing.
    pub fn set_allow_diagonals(&mut self, allow: bool) {
        self.allow_diagonals = allow;
    }

    /// Clear fractional step accruals on every machine (does not reset the drawing).
    pub fn reset_schedule_accumulators(&mut self) {
        for machine in &mut self.machines {
            machine.reset_schedule_accumulators();
        }
    }

    /// Sum of `step_rate` weights for active machines.
    pub fn active_weight_sum(&self) -> f64 {
        self.machines
            .iter()
            .filter(|m| m.active)
            .map(|m| step_rate(m.speed))
            .sum()
    }

    /// Per-round step rate for machine `index` under the current schedule mode.
    pub fn scheduled_rate(&self, index: usize) -> f64 {
        let Some(machine) = self.machines.get(index) else {
            return 0.0;
        };
        if !machine.active {
            return 0.0;
        }
        let weight = step_rate(machine.speed);
        match self.schedule_mode {
            ScheduleMode::Absolute => weight,
            ScheduleMode::Normalised => {
                let sum = self.active_weight_sum();
                if sum <= 0.0 {
                    0.0
                } else {
                    weight / sum
                }
            }
        }
    }

    /// Set one machine's display name (may be empty; UI falls back to a default).
    pub fn set_machine_name(&mut self, index: usize, name: String) -> Result<(), String> {
        let Some(machine) = self.machines.get_mut(index) else {
            return Err("invalid machine index".into());
        };
        machine.name = name;
        Ok(())
    }

    /// Resize one machine's state count, rebuild its table, and reset.
    pub fn set_machine_num_states(&mut self, index: usize, num_states: usize) -> Result<(), String> {
        if !(MIN_STATES..=MAX_STATES).contains(&num_states) {
            return Err(format!(
                "num states must be {MIN_STATES}..={MAX_STATES}, got {num_states}"
            ));
        }
        let num_symbols = self.num_symbols;
        let allow_diagonals = self.allow_diagonals;
        let Some(machine) = self.machines.get_mut(index) else {
            return Err("invalid machine index".into());
        };
        machine.resize_states(num_states, num_symbols, allow_diagonals);
        self.reset();
        Ok(())
    }

    /// Append a random machine with `num_states` and the program's symbol count,
    /// then reset the program.
    pub fn add_machine(&mut self, num_states: usize) {
        assert!(num_states >= MIN_STATES && num_states <= MAX_STATES);
        let mut machine = Machine::new_random(
            num_states,
            self.num_symbols,
            self.width,
            self.height,
            self.allow_diagonals,
        );
        machine.name = default_machine_name(self.machines.len());
        machine.palette = self.canvas_palette.clone();
        self.machines.push(machine);
        self.ensure_machine_ids();
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

    /// Move a machine from `from` to `to` without resetting the drawing.
    pub fn reorder_machines(&mut self, from: usize, to: usize) -> Result<(), String> {
        let n = self.machines.len();
        if from >= n || to >= n {
            return Err("invalid machine index".into());
        }
        if from != to {
            let machine = self.machines.remove(from);
            self.machines.insert(to, machine);
        }
        Ok(())
    }

    /// Load an encoding into `index`. If this is the only machine, the shared
    /// symbol count may change. With multiple machines, symbols must match;
    /// state counts may differ per machine.
    pub fn load_machine(&mut self, index: usize, s: &str) -> Result<(), String> {
        if index >= self.machines.len() {
            return Err("invalid machine index".into());
        }

        let parsed = Machine::from_string(s, self.width, self.height)?;
        if table_has_diagonal_actions(&parsed.machine.table) {
            self.allow_diagonals = true;
        }
        let speed = self.machines[index].speed;
        let name = self.machines[index].name.clone();
        let id = self.machines[index].id;
        let palette = self.machines[index].palette.clone();
        if self.machines.len() == 1 {
            self.num_symbols = parsed.num_symbols;
            self.machines[0] = parsed.machine;
            self.machines[0].speed = speed;
            self.machines[0].name = name;
            self.machines[0].id = id;
            self.machines[0].palette = palette;
            self.resolve_palettes();
            self.reset();
            return Ok(());
        }

        if parsed.num_symbols != self.num_symbols {
            return Err(format!(
                "machine must have {} symbols",
                self.num_symbols
            ));
        }

        self.machines[index] = parsed.machine;
        self.machines[index].speed = speed;
        self.machines[index].name = name;
        self.machines[index].id = id;
        self.machines[index].palette = palette;
        self.reset();
        Ok(())
    }

    pub fn machine_encoding(&self, index: usize) -> String {
        self.machines[index].to_string(self.num_symbols)
    }

    /// Run `num_itrs` interleaved rounds. Each active machine accrues its
    /// scheduled step rate and takes any whole steps that are due.
    pub fn update(&mut self, num_itrs: usize) -> Option<DirtyRect> {
        let width = self.width as i32;
        let height = self.height as i32;
        let mut dirty: Option<DirtyRect> = None;
        let weight_sum = match self.schedule_mode {
            ScheduleMode::Absolute => 0.0,
            ScheduleMode::Normalised => self.active_weight_sum(),
        };

        for _ in 0..num_itrs {
            for machine in self.machines.iter_mut() {
                if !machine.active {
                    continue;
                }
                let rate = match self.schedule_mode {
                    ScheduleMode::Absolute => step_rate(machine.speed),
                    ScheduleMode::Normalised => {
                        if weight_sum <= 0.0 {
                            0.0
                        } else {
                            step_rate(machine.speed) / weight_sum
                        }
                    }
                };
                let wrote =
                    machine.take_scheduled_steps(&mut self.map, &mut self.canvas, width, height, rate);
                if let Some(rect) = wrote {
                    dirty = Some(match dirty {
                        Some(acc) => acc.union(rect),
                        None => rect,
                    });
                }
            }
            self.itr_count += 1;
        }
        if let Some(rect) = dirty {
            self.mark_canvas_dirty(rect);
        }
        dirty
    }

    fn mark_canvas_dirty(&mut self, rect: DirtyRect) {
        self.canvas_dirty = Some(match self.canvas_dirty {
            Some(acc) => acc.union(rect),
            None => rect,
        });
        self.canvas_revision = self.canvas_revision.wrapping_add(1);
    }

    pub fn take_canvas_dirty(&mut self) -> Option<DirtyRect> {
        self.canvas_dirty.take()
    }

    /// Assign ids to any machine that still has `id == 0`.
    pub fn ensure_machine_ids(&mut self) {
        let mut next = self.machines.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        for machine in &mut self.machines {
            if machine.id == 0 {
                machine.id = next;
                next += 1;
            }
        }
    }

    pub fn index_of_machine(&self, id: u64) -> Option<usize> {
        self.machines.iter().position(|m| m.id == id)
    }

    pub fn machine_ids(&self) -> Vec<u64> {
        self.machines.iter().map(|m| m.id).collect()
    }

    /// Key for speed-snapshot validity: machine count and list order (stable ids).
    /// Canvas size and other program edits do not affect this key.
    pub fn machine_layout_key(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.machines.len().hash(&mut hasher);
        for m in &self.machines {
            m.id.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn next_auto_snapshot_name(&self) -> String {
        let mut n = 1u32;
        loop {
            let candidate = format!("Snapshot {n}");
            if !self.speed_snapshots.iter().any(|s| s.name == candidate) {
                return candidate;
            }
            n = n.saturating_add(1);
            if n == 0 {
                return format!("Snapshot {}", self.speed_snapshots.len() + 1);
            }
        }
    }

    /// Capture current machine speeds under `name` (empty → auto `Snapshot N`).
    /// Returns the name that was stored. Overwrites an existing name.
    pub fn store_speed_snapshot(&mut self, name: &str) -> Result<String, String> {
        if self.machines.is_empty() {
            return Err("program has no machines".into());
        }
        let name = {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                self.next_auto_snapshot_name()
            } else {
                trimmed.to_string()
            }
        };
        let speeds: Vec<f32> = self.machines.iter().map(|m| m.speed).collect();
        if let Some(existing) = self.speed_snapshots.iter_mut().find(|s| s.name == name) {
            existing.speeds = speeds;
        } else {
            self.speed_snapshots.push(SpeedSnapshot {
                name: name.clone(),
                speeds,
            });
        }
        self.speed_snapshot_key = self.machine_layout_key();
        Ok(name)
    }

    /// Apply a named snapshot's speeds. Does not reset the drawing.
    pub fn recall_speed_snapshot(&mut self, name: &str) -> Result<(), String> {
        let speeds = self
            .speed_snapshots
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.speeds.clone())
            .ok_or_else(|| format!("snapshot not found: {name}"))?;
        if speeds.len() != self.machines.len() {
            return Err(format!(
                "snapshot \"{name}\" has {} speed(s), program has {}",
                speeds.len(),
                self.machines.len()
            ));
        }
        for (i, speed) in speeds.into_iter().enumerate() {
            self.set_machine_speed(i, speed)?;
        }
        Ok(())
    }

    /// Remove one named snapshot.
    pub fn delete_speed_snapshot(&mut self, name: &str) -> Result<(), String> {
        let before = self.speed_snapshots.len();
        self.speed_snapshots.retain(|s| s.name != name);
        if self.speed_snapshots.len() == before {
            return Err(format!("snapshot not found: {name}"));
        }
        Ok(())
    }

    /// Clear snapshots if machines were added, removed, or reordered.
    /// Returns `true` when a non-empty table was cleared.
    pub fn sync_speed_snapshots(&mut self) -> bool {
        if self.speed_snapshots.is_empty() {
            return false;
        }
        let key = self.machine_layout_key();
        if key != self.speed_snapshot_key {
            self.speed_snapshots.clear();
            self.speed_snapshot_key = key;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::{
        Machine, ACTION_DOWN, ACTION_DOWN_LEFT, ACTION_LEFT, ACTION_RIGHT, ACTION_UP_RIGHT,
    };
    use crate::palette::{empty_canvas, rgb_at, Palette, PaletteKind};
    use crate::tape::{TapeInit, TapeInitKind};

    fn fixed_machine(num_states: usize, table: Vec<i32>, start_x: i32, start_y: i32) -> Machine {
        Machine {
            num_states,
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

    fn fixed_program(
        num_states: usize,
        num_symbols: usize,
        table: Vec<i32>,
        start_x: i32,
        start_y: i32,
    ) -> Program {
        Program {
            num_symbols,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT],
            canvas: empty_canvas(DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT),
            canvas_palette: Palette::classic(),
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![fixed_machine(num_states, table, start_x, start_y)],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        }
    }

    #[test]
    fn roundtrip_encoding() {
        let p = Program::new_random(4, 3);
        let s = p.machine_encoding(0);
        let q = Program::from_string(&s).unwrap();
        assert_eq!(p.machines[0].num_states, q.machines[0].num_states);
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
        assert_eq!(p.machines[0].num_states, 1);
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
        let width = p.width;
        p.machines[0].x_pos = (width - 1) as i32;
        p.map[width - 1] = 0;
        p.update(1);
        assert_eq!(p.machines[0].x_pos, 0);
    }

    #[test]
    fn wrap_right_decrements_x() {
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_RIGHT], 0, 0);
        p.update(1);
        assert_eq!(p.machines[0].x_pos, (p.width - 1) as i32);
    }

    #[test]
    fn diagonal_step_and_corner_wrap() {
        // UP_RIGHT = (+1, -1): from (0, 0) wraps y to bottom.
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_UP_RIGHT], 0, 0);
        p.update(1);
        assert_eq!(p.machines[0].x_pos, 1);
        assert_eq!(p.machines[0].y_pos, (p.height - 1) as i32);

        // Corner wrap: (width-1, 0) + UP_RIGHT → (0, height-1).
        let width = p.width as i32;
        let height = p.height as i32;
        p.machines[0].x_pos = width - 1;
        p.machines[0].y_pos = 0;
        p.machines[0].state = 0;
        let idx = (width - 1) as usize;
        p.map[idx] = 0;
        p.update(1);
        assert_eq!(p.machines[0].x_pos, 0);
        assert_eq!(p.machines[0].y_pos, height - 1);

        // DOWN_LEFT = (−1, +1): from (0, height-1) → (width-1, 0).
        p.machines[0].table = vec![0, 1, ACTION_DOWN_LEFT];
        p.machines[0].x_pos = 0;
        p.machines[0].y_pos = height - 1;
        p.machines[0].state = 0;
        let corner = ((height - 1) * width) as usize;
        p.map[corner] = 0;
        p.update(1);
        assert_eq!(p.machines[0].x_pos, width - 1);
        assert_eq!(p.machines[0].y_pos, 0);
    }

    #[test]
    fn generated_actions_respect_diagonal_flag() {
        for _ in 0..10 {
            let p = Program::new_random(4, 3);
            assert!(!p.allow_diagonals);
            for chunk in p.machines[0].table.chunks_exact(3) {
                assert!((0..4).contains(&chunk[2]), "cardinal-only action {}", chunk[2]);
            }
        }

        let mut saw_diagonal = false;
        for _ in 0..20 {
            let p = Program::new_random_sized_with_diagonals(
                8,
                8,
                DEFAULT_MAP_WIDTH,
                DEFAULT_MAP_HEIGHT,
                true,
            );
            assert!(p.allow_diagonals);
            for chunk in p.machines[0].table.chunks_exact(3) {
                assert!((0..8).contains(&chunk[2]), "8-dir action {}", chunk[2]);
                if chunk[2] >= 4 {
                    saw_diagonal = true;
                }
            }
        }
        assert!(saw_diagonal, "expected at least one diagonal action when enabled");
    }

    #[test]
    fn encoding_with_diagonal_enables_flag() {
        // 1 state × 2 symbols: both transitions use ACTION_UP_RIGHT (4).
        let enc = format!("1,2,0,0,0,1,{ACTION_UP_RIGHT},0,1,{ACTION_UP_RIGHT}");
        let p = Program::from_string(&enc).unwrap();
        assert!(p.allow_diagonals);
        p.machines[0]
            .table
            .chunks_exact(3)
            .for_each(|c| assert_eq!(c[2], ACTION_UP_RIGHT));
    }

    #[test]
    fn random_never_writes_zero() {
        let p = Program::new_random(4, 3);
        let m = &p.machines[0];
        for sy0 in 0..p.num_symbols {
            for st0 in 0..m.num_states {
                let idx = (m.num_states * sy0 + st0) * 3;
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
        assert_eq!(p.machines[0].num_states, 4);
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

        p.add_machine(1);
        assert_eq!(p.machines.len(), 2);
        assert_eq!(p.itr_count, 0);
        assert!(p.map.iter().all(|&s| s == 0));
        assert_eq!(p.machines[0].x_pos, p.machines[0].start_x);
        assert_eq!(p.machines[0].y_pos, p.machines[0].start_y);
        assert_eq!(p.machines[1].x_pos, p.machines[1].start_x);
        assert_eq!(p.machines[1].y_pos, p.machines[1].start_y);
        assert_eq!(p.machines[1].num_states, 1);
    }

    #[test]
    fn randomize_and_add_preserve_distinct_machine_ids() {
        let mut p = Program::new_random(2, 2);
        let id0 = p.machines[0].id;
        assert_ne!(id0, 0);
        p.add_machine(2);
        let id1 = p.machines[1].id;
        assert_ne!(id0, id1);
        p.randomize(2);
        assert_eq!(p.machines[0].id, id0);
        assert_eq!(p.machines[1].id, id1);
        p.remove_machine(1).unwrap();
        assert_eq!(p.machines[0].id, id0);
        assert_eq!(p.index_of_machine(id1), None);
    }

    #[test]
    fn interleaved_second_machine_sees_first_write() {
        // Both start at (0,0). Machine 0 writes 1 and moves. Machine 1 writes 2
        // only when it reads symbol 1 — so map[0] == 2 means it saw the write.
        let m0 = fixed_machine(
            1,
            vec![0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT],
            0,
            0,
        );
        let m1 = fixed_machine(
            1,
            vec![0, 1, ACTION_LEFT, 0, 2, ACTION_RIGHT, 0, 2, ACTION_RIGHT],
            0,
            0,
        );
        let mut p = Program {
            num_symbols: 3,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT],
            canvas: empty_canvas(DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT),
            canvas_palette: Palette::classic(),
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![m0, m1],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        };

        p.update(1);
        assert_eq!(p.map[0], 2);
        assert_eq!(p.machines[0].x_pos, 1);
        assert_eq!(p.machines[1].x_pos, (p.width - 1) as i32);
        assert_eq!(p.itr_count, 1);
    }

    #[test]
    fn load_rejects_mismatched_symbols_when_multiple_machines() {
        let mut p = Program::new_random(4, 3);
        p.add_machine(4);
        assert_eq!(p.machines.len(), 2);

        let other = Machine::new_random(2, 2, p.width, p.height, false);
        let enc = other.to_string(2);
        let err = p.load_machine(0, &enc).unwrap_err();
        assert!(err.contains("3 symbols"));
        assert!(!err.contains("states"));
        assert_eq!(p.machines.len(), 2);
        assert_eq!(p.machines[0].num_states, 4);
        assert_eq!(p.num_symbols, 3);
    }

    #[test]
    fn load_allows_mismatched_states_when_multiple_machines() {
        let mut p = Program::new_random(4, 3);
        p.add_machine(4);
        let other = Machine::new_random(2, 3, p.width, p.height, false);
        let enc = other.to_string(3);
        p.load_machine(0, &enc).unwrap();
        assert_eq!(p.machines[0].num_states, 2);
        assert_eq!(p.machines[1].num_states, 4);
        assert_eq!(p.num_symbols, 3);
        assert_eq!(p.machines[0].table, other.table);
    }

    #[test]
    fn load_single_machine_may_change_counts() {
        let mut p = Program::new_random(4, 3);
        let other = Machine::new_random(2, 2, p.width, p.height, false);
        let enc = other.to_string(2);
        p.load_machine(0, &enc).unwrap();
        assert_eq!(p.machines[0].num_states, 2);
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
        p.add_machine(4);
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
            let mut m = fixed_machine(1, table.clone(), 0, 0);
            m.speed = 10.0;
            m
        };
        let slow = {
            let mut m = fixed_machine(1, table.clone(), 0, 1);
            m.speed = -10.0;
            m
        };
        let mid = {
            let mut m = fixed_machine(1, table.clone(), 0, 3);
            m.speed = 5.0;
            m
        };
        let normal = fixed_machine(1, table, 0, 2);
        let mut p = Program {
            num_symbols: 2,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT],
            canvas: empty_canvas(DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT),
            canvas_palette: Palette::classic(),
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![fast, slow, normal, mid],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        };

        p.update(10);
        assert_eq!(p.machines[0].x_pos, 100);
        assert_eq!(p.machines[1].x_pos, 1);
        assert_eq!(p.machines[2].x_pos, 10);
        assert_eq!(p.machines[3].x_pos, (step_rate(5.0) * 10.0).floor() as i32);
    }

    fn walkers(speeds: &[(f32, bool)]) -> Program {
        let table = vec![0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT];
        let machines = speeds
            .iter()
            .enumerate()
            .map(|(i, &(speed, active))| {
                let mut m = fixed_machine(1, table.clone(), 0, i as i32);
                m.speed = speed;
                m.active = active;
                m
            })
            .collect();
        Program {
            num_symbols: 2,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT],
            canvas: empty_canvas(DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT),
            canvas_palette: Palette::classic(),
            canvas_revision: 0,
            canvas_dirty: None,
            machines,
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Normalised,
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        }
    }

    #[test]
    fn normalised_equal_speeds_split_budget() {
        let mut p = walkers(&[(0.0, true), (0.0, true)]);
        p.update(100);
        assert_eq!(p.machines[0].x_pos, 50);
        assert_eq!(p.machines[1].x_pos, 50);
        assert_eq!(p.machines[0].x_pos + p.machines[1].x_pos, 100);
    }

    #[test]
    fn normalised_speeds_are_relative_shares() {
        let mut p = walkers(&[(10.0, true), (0.0, true)]);
        p.update(100);
        // Weights 10 : 1 → ~91 and ~9 steps; total still ~100.
        assert_eq!(p.machines[0].x_pos, 90);
        assert_eq!(p.machines[1].x_pos, 9);
        assert_eq!(p.machines[0].x_pos + p.machines[1].x_pos, 99);
        assert!((p.scheduled_rate(0) - 10.0 / 11.0).abs() < 1e-12);
        assert!((p.scheduled_rate(1) - 1.0 / 11.0).abs() < 1e-12);
    }

    #[test]
    fn normalised_inactive_excluded_from_denominator() {
        let mut p = walkers(&[(0.0, true), (10.0, false), (0.0, true)]);
        assert!((p.active_weight_sum() - 2.0).abs() < 1e-12);
        assert!((p.scheduled_rate(0) - 0.5).abs() < 1e-12);
        assert_eq!(p.scheduled_rate(1), 0.0);
        assert!((p.scheduled_rate(2) - 0.5).abs() < 1e-12);

        p.update(100);
        assert_eq!(p.machines[0].x_pos, 50);
        assert_eq!(p.machines[1].x_pos, 0);
        assert_eq!(p.machines[2].x_pos, 50);
    }

    #[test]
    fn absolute_totals_scale_with_sum_of_rates() {
        let mut p = walkers(&[(0.0, true), (0.0, true)]);
        p.set_schedule_mode(ScheduleMode::Absolute);
        p.update(10);
        // Two machines at 1× → 20 total steps.
        assert_eq!(p.machines[0].x_pos + p.machines[1].x_pos, 20);
    }

    #[test]
    fn inactive_machine_does_not_step() {
        let table = vec![0, 1, ACTION_RIGHT as i32];
        let active = fixed_machine(1, table.clone(), 0, 0);
        let mut inactive = fixed_machine(1, table, 0, 0);
        inactive.active = false;

        let mut p = Program {
            num_symbols: 2,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT],
            canvas: empty_canvas(DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT),
            canvas_palette: Palette::classic(),
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![active, inactive],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        };

        p.update(5);
        assert_ne!(p.machines[0].x_pos, 0);
        assert_eq!(p.machines[1].x_pos, 0);
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
        assert!(p.machines[0].active);
    }

    #[test]
    fn wrap_on_nonsquare_canvas() {
        let mut p = Program {
            num_symbols: 2,
            width: 10,
            height: 8,
            map: vec![0; 80],
            canvas: empty_canvas(80),
            canvas_palette: Palette::classic(),
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![fixed_machine(1, vec![0, 1, ACTION_LEFT], 9, 0)],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        };
        p.update(1);
        assert_eq!(p.machines[0].x_pos, 0);
        assert_eq!(p.machines[0].y_pos, 0);
        assert_eq!(p.map[9], 1);

        p.machines[0].x_pos = 0;
        p.machines[0].y_pos = 7;
        p.machines[0].table = vec![0, 1, ACTION_DOWN];
        p.map[7 * 10] = 0;
        p.update(1);
        assert_eq!(p.machines[0].y_pos, 0);
        assert_eq!(p.map[7 * 10], 1);
    }

    #[test]
    fn set_size_reallocates_and_wraps_starts() {
        let mut p = Program::new_random(2, 2);
        p.machines[0].start_x = 500;
        p.machines[0].start_y = 500;
        p.set_size(128, 64).unwrap();
        assert_eq!(p.width, 128);
        assert_eq!(p.height, 64);
        assert_eq!(p.map.len(), 128 * 64);
        assert_eq!(p.canvas.len(), 128 * 64 * 4);
        assert_eq!(p.machines[0].start_x, wrap_pos(500, 128));
        assert_eq!(p.machines[0].start_y, wrap_pos(500, 64));
        assert_eq!(p.machines[0].x_pos, p.machines[0].start_x);
        assert_eq!(p.itr_count, 0);
        assert!(p.map.iter().all(|&s| s == 0));
        assert!(p.set_size(10, 512).is_err());
        assert!(p.set_size(512, 5000).is_err());
        p.set_size(128, 64).unwrap();
        assert_eq!(p.width, 128);
    }

    #[test]
    fn new_and_added_machines_get_default_names() {
        let mut p = Program::new_random(2, 2);
        assert_eq!(p.machines[0].name, "Machine 1");
        p.add_machine(2);
        assert_eq!(p.machines[1].name, "Machine 2");
        let q = Program::from_string(&p.machine_encoding(0)).unwrap();
        assert_eq!(q.machines[0].name, "Machine 1");
    }

    #[test]
    fn randomize_and_load_keep_machine_name() {
        let mut p = Program::new_random(2, 2);
        p.add_machine(5);
        p.set_machine_name(0, "Walker".into()).unwrap();
        p.set_machine_name(1, "Hopper".into()).unwrap();
        let other = p.machines[1].clone();

        p.randomize_machine(0).unwrap();
        assert_eq!(p.machines[0].name, "Walker");
        assert_eq!(p.machines[1].name, "Hopper");
        assert_eq!(p.machines[1].table, other.table);
        assert_eq!(p.machines[0].num_states, 2);
        assert_eq!(p.machines[1].num_states, 5);

        p.randomize(3);
        assert_eq!(p.machines[0].name, "Walker");
        assert_eq!(p.machines[1].name, "Hopper");
        assert_eq!(p.machines[0].num_states, 2);
        assert_eq!(p.machines[1].num_states, 5);
        assert_eq!(p.num_symbols, 3);

        let enc = Machine::new_random(3, 3, p.width, p.height, false).to_string(3);
        p.load_machine(1, &enc).unwrap();
        assert_eq!(p.machines[1].name, "Hopper");
        assert_eq!(p.machines[1].num_states, 3);
        assert!(p.set_machine_name(9, "x".into()).is_err());
    }

    #[test]
    fn set_machine_num_states_resizes_and_resets() {
        let mut p = Program::new_random(2, 3);
        p.update(10);
        assert!(p.itr_count > 0);
        p.set_machine_num_states(0, 5).unwrap();
        assert_eq!(p.machines[0].num_states, 5);
        assert_eq!(p.machines[0].table.len(), 5 * 3 * 3);
        assert_eq!(p.itr_count, 0);
        assert!(p.map.iter().all(|&s| s == 0));
        p.set_machine_num_states(0, 1).unwrap();
        assert_eq!(p.machines[0].num_states, 1);
        assert_eq!(p.machines[0].table.len(), 1 * 3 * 3);
        assert!(p.set_machine_num_states(0, 0).is_err());
        assert!(p.set_machine_num_states(9, 2).is_err());
    }

    #[test]
    fn add_machine_uses_requested_state_count() {
        let mut p = Program::new_random(4, 3);
        p.add_machine(2);
        assert_eq!(p.machines[0].num_states, 4);
        assert_eq!(p.machines[1].num_states, 2);
        assert_eq!(p.num_symbols, 3);
        assert_eq!(p.machines[1].table.len(), 2 * 3 * 3);
    }

    #[test]
    fn display_name_falls_back_when_empty() {
        let mut p = Program::new_random(2, 2);
        p.machines[0].name.clear();
        assert_eq!(p.machines[0].display_name(0), "Machine 1");
        p.machines[0].name = "  ".into();
        assert_eq!(p.machines[0].display_name(0), "Machine 1");
        p.machines[0].name = "  custom  ".into();
        assert_eq!(p.machines[0].display_name(0), "custom");
    }

    #[test]
    fn remap_index_after_reorder_shifts_neighbors() {
        // 0 1 2 3, move 0 -> 2 => 1 2 0 3
        assert_eq!(remap_index_after_reorder(0, 0, 2), 2);
        assert_eq!(remap_index_after_reorder(1, 0, 2), 0);
        assert_eq!(remap_index_after_reorder(2, 0, 2), 1);
        assert_eq!(remap_index_after_reorder(3, 0, 2), 3);
        // move 3 -> 0 => 3 0 1 2
        assert_eq!(remap_index_after_reorder(3, 3, 0), 0);
        assert_eq!(remap_index_after_reorder(0, 3, 0), 1);
        assert_eq!(remap_index_after_reorder(2, 3, 0), 3);
        assert_eq!(remap_index_after_reorder(1, 1, 1), 1);
    }

    #[test]
    fn reorder_machines_moves_without_reset() {
        let mut p = Program::new_random(2, 2);
        p.add_machine(2);
        p.add_machine(2);
        p.machines[0].name = "A".into();
        p.machines[1].name = "B".into();
        p.machines[2].name = "C".into();
        p.update(5);
        let itrs = p.itr_count;
        assert!(p.map.iter().any(|&s| s != 0));
        let map_before = p.map.clone();

        p.reorder_machines(0, 2).unwrap();
        assert_eq!(p.machines[0].name, "B");
        assert_eq!(p.machines[1].name, "C");
        assert_eq!(p.machines[2].name, "A");
        assert_eq!(p.itr_count, itrs);
        assert_eq!(p.map, map_before);

        p.reorder_machines(2, 0).unwrap();
        assert_eq!(p.machines[0].name, "A");
        assert_eq!(p.machines[1].name, "B");
        assert_eq!(p.machines[2].name, "C");

        p.reorder_machines(1, 1).unwrap();
        assert_eq!(p.machines[1].name, "B");
        assert!(p.reorder_machines(0, 9).is_err());
        assert!(p.reorder_machines(9, 0).is_err());
    }

    #[test]
    fn mutate_machine_rewrites_table_without_reset() {
        let mut p = Program::new_random(4, 3);
        p.add_machine(4);
        p.update(50);
        let start = (p.machines[0].start_x, p.machines[0].start_y);
        let pos = (p.machines[0].x_pos, p.machines[0].y_pos);
        let state = p.machines[0].state;
        let other = p.machines[1].clone();
        let itrs = p.itr_count;
        let map = p.map.clone();

        p.mutate_machine(0, 10).unwrap();
        assert_eq!((p.machines[0].start_x, p.machines[0].start_y), start);
        assert_eq!((p.machines[0].x_pos, p.machines[0].y_pos), pos);
        assert_eq!(p.machines[0].state, state);
        assert_eq!(p.itr_count, itrs);
        assert_eq!(p.map, map);
        assert_eq!(p.machines[1].table, other.table);
        assert_eq!(p.machines[1].start_x, other.start_x);

        let num_states = p.machines[0].num_states;
        for sy0 in 0..p.num_symbols {
            for st0 in 0..num_states {
                let idx = (num_states * sy0 + st0) * 3;
                let write = p.machines[0].table[idx + 1];
                assert!(write >= 1 && write < p.num_symbols as i32);
            }
        }
        assert!(p.mutate_machine(9, 10).is_err());
    }

    #[test]
    fn mutate_all_rewrites_every_machine() {
        let mut p = Program::new_random(2, 2);
        p.add_machine(2);
        for machine in &mut p.machines {
            for chunk in machine.table.chunks_mut(3) {
                chunk[1] = 0;
            }
        }
        p.update(8);
        let itrs = p.itr_count;
        let starts: Vec<_> = p
            .machines
            .iter()
            .map(|m| (m.start_x, m.start_y, m.x_pos, m.y_pos))
            .collect();

        p.mutate_all(100);
        assert_eq!(p.itr_count, itrs);
        for (i, machine) in p.machines.iter().enumerate() {
            assert_eq!(
                (
                    machine.start_x,
                    machine.start_y,
                    machine.x_pos,
                    machine.y_pos
                ),
                starts[i]
            );
            for sy0 in 0..p.num_symbols {
                for st0 in 0..machine.num_states {
                    let idx = (machine.num_states * sy0 + st0) * 3;
                    let write = machine.table[idx + 1];
                    assert!(write >= 1 && write < p.num_symbols as i32);
                }
            }
        }
    }

    #[test]
    fn set_machine_start_wraps_and_resets() {
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_LEFT], 0, 0);
        p.update(10);
        assert!(p.itr_count > 0);
        assert!(p.map.iter().any(|&s| s != 0));

        p.set_machine_start(0, 3, 4).unwrap();
        assert_eq!(p.machines[0].start_x, 3);
        assert_eq!(p.machines[0].start_y, 4);
        assert_eq!(p.machines[0].x_pos, 3);
        assert_eq!(p.machines[0].y_pos, 4);
        assert_eq!(p.machines[0].state, 0);
        assert_eq!(p.itr_count, 0);
        assert!(p.map.iter().all(|&s| s == 0));

        p.set_machine_start(0, -1, p.height as i32 + 5).unwrap();
        assert_eq!(p.machines[0].start_x, wrap_pos(-1, p.width as i32));
        assert_eq!(
            p.machines[0].start_y,
            wrap_pos(p.height as i32 + 5, p.height as i32)
        );
        assert_eq!(p.machines[0].x_pos, p.machines[0].start_x);
        assert_eq!(p.machines[0].y_pos, p.machines[0].start_y);
        assert!(p.set_machine_start(3, 0, 0).is_err());
    }

    #[test]
    fn reset_refills_from_seeded_tape_init() {
        let mut p = Program::new_random(2, 4);
        p.tape_init.kind = TapeInitKind::Uniform;
        p.tape_init.seed = 123;
        p.reset();
        let first = p.map.clone();
        assert!(first.iter().any(|&s| s != 0));
        assert!(first.iter().all(|&s| s >= 0 && s < 4));

        p.update(20);
        assert_ne!(p.map, first);
        p.reset();
        assert_eq!(p.map, first);
        assert_eq!(p.itr_count, 0);

        p.reseed_tape();
        assert_ne!(p.tape_init.seed, 123);
        assert_ne!(p.map, first);
    }

    #[test]
    fn step_bakes_machine_palette_rgb() {
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_LEFT], 0, 0);
        p.machines[0].palette.colors[1] = [10, 20, 30];
        p.update(1);
        assert_eq!(p.map[0], 1);
        assert_eq!(rgb_at(&p.canvas, 0), [10, 20, 30]);
    }

    #[test]
    fn palette_edits_do_not_recolor_canvas() {
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_LEFT], 0, 0);
        p.machines[0].palette.colors[1] = [10, 20, 30];
        p.update(1);
        assert_eq!(rgb_at(&p.canvas, 0), [10, 20, 30]);

        p.machines[0].palette.colors[1] = [1, 2, 3];
        assert_eq!(rgb_at(&p.canvas, 0), [10, 20, 30]);
    }

    #[test]
    fn two_machines_use_distinct_colors_for_same_symbol() {
        let m0 = {
            let mut m = fixed_machine(1, vec![0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT], 0, 0);
            m.palette.colors[1] = [10, 20, 30];
            m
        };
        let m1 = {
            let mut m = fixed_machine(1, vec![0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT], 1, 0);
            m.palette.colors[1] = [40, 50, 60];
            m
        };
        let mut p = Program {
            num_symbols: 2,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT],
            canvas: empty_canvas(DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT),
            canvas_palette: Palette::classic(),
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![m0, m1],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        };
        p.update(1);
        assert_eq!(p.map[0], 1);
        assert_eq!(p.map[1], 1);
        assert_eq!(rgb_at(&p.canvas, 0), [10, 20, 30]);
        assert_eq!(rgb_at(&p.canvas, 1), [40, 50, 60]);
    }

    #[test]
    fn reset_fills_canvas_from_tape_using_canvas_palette() {
        let mut p = fixed_program(1, 2, vec![0, 1, ACTION_LEFT], 0, 0);
        p.canvas_palette.colors[0] = [9, 8, 7];
        p.reset();
        assert_eq!(rgb_at(&p.canvas, 0), [9, 8, 7]);

        p.machines[0].palette.colors[1] = [10, 20, 30];
        p.update(1);
        assert_eq!(rgb_at(&p.canvas, 0), [10, 20, 30]);

        p.canvas_palette.colors[0] = [1, 1, 1];
        assert_eq!(rgb_at(&p.canvas, 0), [10, 20, 30]);
        p.reset();
        assert_eq!(rgb_at(&p.canvas, 0), [1, 1, 1]);
    }

    #[test]
    fn add_machine_clones_canvas_palette() {
        let mut p = Program::new_random(2, 3);
        p.canvas_palette.set_kind(PaletteKind::Sunset, 3);
        p.add_machine(2);
        assert_eq!(p.machines[1].palette.kind, PaletteKind::Sunset);
        assert_eq!(p.machines[1].palette.colors, p.canvas_palette.colors);
    }

    #[test]
    fn randomize_preserves_machine_palette() {
        let mut p = Program::new_random(2, 3);
        p.machines[0].palette.set_kind(PaletteKind::Neon, 3);
        let colors = p.machines[0].palette.colors;
        p.randomize(3);
        assert_eq!(p.machines[0].palette.kind, PaletteKind::Neon);
        assert_eq!(p.machines[0].palette.colors, colors);
        assert_eq!(p.machines[0].num_states, 2);
        assert_eq!(p.num_symbols, 3);
    }

    #[test]
    fn speed_snapshot_store_recall_overwrite_and_auto_name() {
        let mut p = Program::new_random(2, 2);
        p.add_machine(2);
        p.set_machine_speed(0, 3.0).unwrap();
        p.set_machine_speed(1, -2.0).unwrap();

        let name = p.store_speed_snapshot("").unwrap();
        assert_eq!(name, "Snapshot 1");
        assert_eq!(p.speed_snapshots.len(), 1);

        p.set_machine_speed(0, 0.0).unwrap();
        p.set_machine_speed(1, 0.0).unwrap();
        p.recall_speed_snapshot("Snapshot 1").unwrap();
        assert_eq!(p.machines[0].speed, 3.0);
        assert_eq!(p.machines[1].speed, -2.0);

        p.set_machine_speed(0, 5.0).unwrap();
        p.set_machine_speed(1, 1.0).unwrap();
        let again = p.store_speed_snapshot("Snapshot 1").unwrap();
        assert_eq!(again, "Snapshot 1");
        assert_eq!(p.speed_snapshots.len(), 1);
        assert_eq!(p.speed_snapshots[0].speeds, vec![5.0, 1.0]);

        let second = p.store_speed_snapshot("").unwrap();
        assert_eq!(second, "Snapshot 2");
        assert_eq!(p.speed_snapshots.len(), 2);

        p.delete_speed_snapshot("Snapshot 1").unwrap();
        assert_eq!(p.speed_snapshots.len(), 1);
        assert_eq!(p.speed_snapshots[0].name, "Snapshot 2");
    }

    #[test]
    fn speed_snapshots_cleared_only_on_machine_count_or_order() {
        let mut p = Program::new_random(2, 2);
        p.add_machine(2);
        p.store_speed_snapshot("A").unwrap();
        assert!(!p.speed_snapshots.is_empty());

        p.set_machine_speed(0, 4.0).unwrap();
        assert!(!p.sync_speed_snapshots());
        assert_eq!(p.speed_snapshots.len(), 1);

        p.reset();
        assert!(!p.sync_speed_snapshots());

        p.reseed_tape();
        assert!(!p.sync_speed_snapshots());

        p.set_schedule_mode(ScheduleMode::Normalised);
        assert!(!p.sync_speed_snapshots());

        p.set_machine_active(0, false).unwrap();
        assert!(!p.sync_speed_snapshots());

        p.set_size(256, 256).unwrap();
        assert!(!p.sync_speed_snapshots());

        p.mutate_machine(0, 50).unwrap();
        assert!(!p.sync_speed_snapshots());

        p.set_machine_name(0, "Renamed".into()).unwrap();
        assert!(!p.sync_speed_snapshots());

        p.add_machine(2);
        assert!(p.sync_speed_snapshots());
        assert!(p.speed_snapshots.is_empty());

        p.store_speed_snapshot("B").unwrap();
        p.remove_machine(p.machines.len() - 1).unwrap();
        assert!(p.sync_speed_snapshots());
        assert!(p.speed_snapshots.is_empty());

        p.add_machine(2);
        p.store_speed_snapshot("C").unwrap();
        assert_eq!(p.machines.len(), 3);
        p.reorder_machines(0, 2).unwrap();
        assert!(p.sync_speed_snapshots());
        assert!(p.speed_snapshots.is_empty());
    }
}
