//! Named on-disk presets for the full starting configuration of all machines.

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::machine::{
    default_machine_name, validate_map_size, wrap_pos, Machine, DEFAULT_MAP_HEIGHT,
    DEFAULT_MAP_WIDTH, MAX_MACHINE_SPEED, MAX_STATES, MAX_SYMBOLS, MIN_MACHINE_SPEED, MIN_STATES,
    MIN_SYMBOLS,
};
use crate::palette::{empty_canvas, Palette, PaletteSpec};
use crate::program::{Program, ScheduleMode, SpeedSnapshot};
use crate::settings::PresetPerformanceBinding;
use crate::storage;
use crate::tape::TapeInit;

const PRESET_VERSION: u32 = 7;
const MIN_PRESET_VERSION: u32 = 1;

/// One machine's starting configuration inside a preset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresetMachine {
    pub start_x: i32,
    pub start_y: i32,
    pub speed: f32,
    #[serde(default = "default_active")]
    pub active: bool,
    #[serde(default)]
    pub name: String,
    pub table: Vec<i32>,
    /// Per-machine state count. `0` or missing inherits [`Preset::num_states`].
    #[serde(default)]
    pub num_states: usize,
    /// Per-machine drawing palette. Missing in older files (Classic).
    #[serde(default)]
    pub palette: PaletteSpec,
}

fn default_active() -> bool {
    true
}

/// Resolve a machine's state count: per-machine value, or preset-level inherit.
pub fn machine_num_states(preset: &Preset, machine: &PresetMachine) -> usize {
    if machine.num_states == 0 {
        preset.num_states
    } else {
        machine.num_states
    }
}

/// Full program starting state saved as a named preset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub version: u32,
    pub name: String,
    /// Unix timestamp (seconds) of the last Store. Missing in older files.
    #[serde(default)]
    pub saved_at: u64,
    /// Legacy / summary state count. New saves use the first machine's count
    /// (or the shared count when all machines match). Older files store the
    /// uniform count that every machine inherited.
    pub num_states: usize,
    pub num_symbols: usize,
    #[serde(default = "default_map_width")]
    pub map_width: usize,
    #[serde(default = "default_map_height")]
    pub map_height: usize,
    pub machines: Vec<PresetMachine>,
    /// Per-machine performance shortcuts saved with this preset.
    #[serde(default)]
    pub performance_bindings: Vec<PresetPerformanceBinding>,
    /// How Restart fills the canvas. Missing in older files (Empty).
    #[serde(default)]
    pub tape_init: TapeInit,
    /// When true, Random / Mutate may pick diagonal move actions. Missing → false.
    #[serde(default)]
    pub allow_diagonals: bool,
    /// Named machine-speed snapshots. Missing in older files (empty).
    #[serde(default)]
    pub speed_snapshots: Vec<SpeedSnapshot>,
}

/// How the preset browser orders its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresetSort {
    Name,
    #[default]
    DateSaved,
}

impl PresetSort {
    pub const ALL: [PresetSort; 2] = [Self::Name, Self::DateSaved];
}

impl std::fmt::Display for PresetSort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Name => "Name",
            Self::DateSaved => "Date saved",
        })
    }
}

/// Summary row for the preset browser list.
#[derive(Debug, Clone)]
pub struct PresetInfo {
    pub name: String,
    pub num_machines: usize,
    /// Minimum state count among machines (after inherit).
    pub num_states_min: usize,
    /// Maximum state count among machines (after inherit).
    pub num_states_max: usize,
    pub num_symbols: usize,
    pub saved_at: u64,
}

impl PresetInfo {
    /// Human-readable state summary: `3` or `3–5` when machines differ.
    pub fn states_label(&self) -> String {
        if self.num_states_min == self.num_states_max {
            self.num_states_min.to_string()
        } else {
            format!("{}–{}", self.num_states_min, self.num_states_max)
        }
    }
}

impl Program {
    /// Capture the current machines as a named preset (starting config only).
    pub fn to_preset(&self, name: &str) -> Result<Preset, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("preset name is empty".into());
        }
        if self.machines.is_empty() {
            return Err("program has no machines".into());
        }

        let summary_states = self.machines[0].num_states;
        Ok(Preset {
            version: PRESET_VERSION,
            name: name.to_string(),
            saved_at: 0,
            num_states: summary_states,
            num_symbols: self.num_symbols,
            map_width: self.width,
            map_height: self.height,
            machines: self
                .machines
                .iter()
                .map(|m| PresetMachine {
                    start_x: m.start_x,
                    start_y: m.start_y,
                    speed: m.speed,
                    active: m.active,
                    name: m.name.clone(),
                    table: m.table.clone(),
                    num_states: m.num_states,
                    palette: m.palette.to_spec(),
                })
                .collect(),
            performance_bindings: Vec::new(),
            tape_init: self.tape_init.clone(),
            allow_diagonals: self.allow_diagonals,
            speed_snapshots: self.speed_snapshots.clone(),
        })
    }

    /// Replace this program with a loaded preset and reset to starting state.
    pub fn from_preset(preset: &Preset) -> Result<Self, String> {
        validate_preset(preset)?;

        let width = preset.map_width;
        let height = preset.map_height;
        let machines: Vec<Machine> = preset
            .machines
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let start_x = wrap_pos(m.start_x, width as i32);
                let start_y = wrap_pos(m.start_y, height as i32);
                let speed = m.speed.clamp(MIN_MACHINE_SPEED, MAX_MACHINE_SPEED);
                let name = if m.name.trim().is_empty() {
                    default_machine_name(i)
                } else {
                    m.name.clone()
                };
                let num_states = machine_num_states(preset, m);
                Machine {
                    num_states,
                    table: m.table.clone(),
                    state: 0,
                    x_pos: start_x,
                    y_pos: start_y,
                    start_x,
                    start_y,
                    speed,
                    active: m.active,
                    name,
                    id: 0,
                    palette: Palette::from_spec(&m.palette, preset.num_symbols),
                    rounds_at_speed: 0,
                    steps_at_speed: 0,
                }
            })
            .collect();

        let mut tape_init = preset.tape_init.clone();
        tape_init.sanitize();
        let canvas_palette = machines
            .first()
            .map(|m| m.palette.clone())
            .unwrap_or_else(Palette::classic);
        let speed_snapshots = sanitize_speed_snapshots(&preset.speed_snapshots, machines.len());
        let mut prog = Self {
            num_symbols: preset.num_symbols,
            width,
            height,
            map: vec![0; width * height],
            canvas: empty_canvas(width * height),
            canvas_palette,
            canvas_revision: 0,
            canvas_dirty: None,
            machines,
            itr_count: 0,
            tape_init,
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: preset.allow_diagonals,
            speed_snapshots,
            speed_snapshot_key: 0,
        };
        prog.ensure_machine_ids();
        prog.reset();
        prog.speed_snapshot_key = prog.machine_layout_key();
        Ok(prog)
    }
}

fn validate_preset(preset: &Preset) -> Result<(), String> {
    if preset.version < MIN_PRESET_VERSION || preset.version > PRESET_VERSION {
        return Err(format!(
            "unsupported preset version {} (expected {MIN_PRESET_VERSION}..={PRESET_VERSION})",
            preset.version
        ));
    }
    if preset.name.trim().is_empty() {
        return Err("preset name is empty".into());
    }
    if preset.num_states < MIN_STATES || preset.num_states > MAX_STATES {
        return Err(format!(
            "num states must be {MIN_STATES}..={MAX_STATES}, got {}",
            preset.num_states
        ));
    }
    if preset.num_symbols < MIN_SYMBOLS || preset.num_symbols > MAX_SYMBOLS {
        return Err(format!(
            "num symbols must be {MIN_SYMBOLS}..={MAX_SYMBOLS}, got {}",
            preset.num_symbols
        ));
    }
    validate_map_size(preset.map_width, preset.map_height)?;
    if preset.machines.is_empty() {
        return Err("preset has no machines".into());
    }

    for (i, m) in preset.machines.iter().enumerate() {
        let num_states = machine_num_states(preset, m);
        if num_states < MIN_STATES || num_states > MAX_STATES {
            return Err(format!(
                "machine {}: num states must be {MIN_STATES}..={MAX_STATES}, got {num_states}",
                i + 1
            ));
        }
        let expected = num_states * preset.num_symbols * 3;
        if m.table.len() != expected {
            return Err(format!(
                "machine {}: table length {}, expected {expected}",
                i + 1,
                m.table.len()
            ));
        }
        for (j, chunk) in m.table.chunks_exact(3).enumerate() {
            let action = chunk[2];
            if !(0..=7).contains(&action) {
                return Err(format!(
                    "machine {}: rule {}: invalid action {action} (expected 0..=7)",
                    i + 1,
                    j + 1
                ));
            }
        }
        // Speed is clamped on load; only reject non-finite values.
        if !m.speed.is_finite() {
            return Err(format!("machine {}: speed must be finite", i + 1));
        }
    }
    for (i, binding) in preset.performance_bindings.iter().enumerate() {
        if binding.machine_index >= preset.machines.len() {
            return Err(format!(
                "performance binding {}: machine index {} out of range ({} machines)",
                i + 1,
                binding.machine_index,
                preset.machines.len()
            ));
        }
        if !binding.action.is_performance() {
            return Err(format!(
                "performance binding {}: action {:?} is not a machine action",
                i + 1,
                binding.action
            ));
        }
        if binding.key.is_none() && binding.named.is_none() {
            return Err(format!("performance binding {}: missing key", i + 1));
        }
    }
    Ok(())
}

/// Keep snapshots only when every row is well-formed for `machine_count`.
/// If any row is invalid, drop the whole table.
fn sanitize_speed_snapshots(snapshots: &[SpeedSnapshot], machine_count: usize) -> Vec<SpeedSnapshot> {
    if snapshots.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(snapshots.len());
    for s in snapshots {
        let name = s.name.trim();
        if name.is_empty() {
            return Vec::new();
        }
        if s.speeds.len() != machine_count {
            return Vec::new();
        }
        if !s.speeds.iter().all(|v| {
            v.is_finite() && (MIN_MACHINE_SPEED..=MAX_MACHINE_SPEED).contains(v)
        }) {
            return Vec::new();
        }
        out.push(SpeedSnapshot {
            name: name.to_string(),
            speeds: s.speeds.clone(),
        });
    }
    out
}

fn default_map_width() -> usize {
    DEFAULT_MAP_WIDTH
}

fn default_map_height() -> usize {
    DEFAULT_MAP_HEIGHT
}

/// Sanitize a display name into a safe filename stem (`[A-Za-z0-9._-]+`).
pub fn sanitize_filename(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("preset name is empty".into());
    }
    let safe: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() || safe.chars().all(|c| c == '.' || c == '_') {
        return Err("preset name has no usable characters".into());
    }
    Ok(safe)
}

/// Directory that holds preset JSON files. Native only.
#[cfg(not(target_arch = "wasm32"))]
pub fn presets_dir() -> Result<PathBuf, String> {
    storage::presets_dir()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
fn ensure_presets_dir() -> Result<PathBuf, String> {
    storage::ensure_presets_dir()
}

fn unix_now() -> u64 {
    storage::unix_now()
}

/// Format a unix timestamp as `YYYY-MM-DD HH:MM UTC` for the browser list.
pub fn format_saved_at(secs: u64) -> String {
    if secs == 0 {
        return "unknown date".into();
    }
    let (year, month, day, hour, minute) = utc_parts(secs);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// Civil UTC date/time from unix seconds (Howard Hinnant's algorithm).
fn utc_parts(secs: u64) -> (i32, u32, u32, u32, u32) {
    let secs_of_day = (secs % 86_400) as u32;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let z = (secs / 86_400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m, d, hour, minute)
}

/// Write (or overwrite) a preset. Returns the display name stored.
/// Stamps `saved_at` with the current time so date-saved sort stays accurate.
pub fn save_preset(preset: &Preset) -> Result<String, String> {
    validate_preset(preset)?;
    let stem = sanitize_filename(&preset.name)?;
    let mut to_write = preset.clone();
    to_write.saved_at = unix_now();
    let json = serde_json::to_string_pretty(&to_write)
        .map_err(|e| format!("failed to serialize preset: {e}"))?;
    storage::write_preset_text(&stem, &json)?;
    Ok(preset.name.clone())
}

/// Load a preset by display name (filename stem from sanitized name).
pub fn load_preset(name: &str) -> Result<Preset, String> {
    let stem = sanitize_filename(name)?;
    let text = storage::read_preset_text(&stem)?;
    let preset: Preset =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse preset: {e}"))?;
    validate_preset(&preset)?;
    Ok(preset)
}

/// Delete a preset by display name.
pub fn delete_preset(name: &str) -> Result<(), String> {
    let stem = sanitize_filename(name)?;
    storage::delete_preset_text(&stem)
}

/// List saved presets (unsorted). Skips unreadable or invalid files.
/// Older files without `saved_at` fall back to the file's modification time (native).
pub fn list_presets() -> Result<Vec<PresetInfo>, String> {
    let entries = storage::list_preset_entries()?;
    let mut infos = Vec::new();
    for (text, mtime) in entries {
        let Ok(preset) = serde_json::from_str::<Preset>(&text) else {
            continue;
        };
        if validate_preset(&preset).is_err() {
            continue;
        }
        let saved_at = if preset.saved_at > 0 {
            preset.saved_at
        } else {
            mtime.unwrap_or(0)
        };
        let mut states_min = usize::MAX;
        let mut states_max = 0usize;
        for m in &preset.machines {
            let n = machine_num_states(&preset, m);
            states_min = states_min.min(n);
            states_max = states_max.max(n);
        }
        if states_min == usize::MAX {
            states_min = preset.num_states;
            states_max = preset.num_states;
        }
        infos.push(PresetInfo {
            name: preset.name,
            num_machines: preset.machines.len(),
            num_states_min: states_min,
            num_states_max: states_max,
            num_symbols: preset.num_symbols,
            saved_at,
        });
    }
    Ok(infos)
}

/// Sort a preset list in place: name A–Z, or date saved newest first.
pub fn sort_preset_infos(infos: &mut [PresetInfo], sort: PresetSort) {
    match sort {
        PresetSort::Name => {
            infos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
        PresetSort::DateSaved => {
            infos.sort_by(|a, b| {
                b.saved_at
                    .cmp(&a.saved_at)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::{ACTION_DOWN, ACTION_LEFT};
    use crate::tape::{TapeInit, TapeInitKind};
    use std::fs;
    use std::sync::Mutex;

    // Serialize filesystem tests that touch a shared temp-style path via env override
    // is awkward with dirs; use an isolated temp dir by exercising save/load via
    // path helpers and validate/roundtrip without requiring the real app-data dir
    // for pure logic tests. Disk tests use a mutex + unique subdirectory under
    // the real presets dir when available, or skip if data_dir is unavailable.

    static FS_LOCK: Mutex<()> = Mutex::new(());

    fn fixed_machine(num_states: usize, table: Vec<i32>, start_x: i32, start_y: i32, speed: f32) -> Machine {
        Machine {
            num_states,
            table,
            state: 0,
            x_pos: start_x,
            y_pos: start_y,
            start_x,
            start_y,
            speed,
            active: true,
            name: String::new(),
            id: 0,
            palette: Palette::classic(),
            rounds_at_speed: 0,
            steps_at_speed: 0,
        }
    }

    #[test]
    fn sanitize_replaces_unsafe_chars() {
        assert_eq!(sanitize_filename("two walkers").unwrap(), "two_walkers");
        assert_eq!(sanitize_filename("A-B.c_1").unwrap(), "A-B.c_1");
        assert!(sanitize_filename("   ").is_err());
        assert!(sanitize_filename("@@@").is_err());
    }

    #[test]
    fn roundtrip_program_preset() {
        let m0 = fixed_machine(1, vec![0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT], 10, 20, 2.5);
        let m1 = fixed_machine(1, vec![0, 1, ACTION_DOWN, 0, 1, ACTION_DOWN], 30, 40, -1.0);
        let mut p = Program {
            num_symbols: 2,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT],
            canvas: empty_canvas(DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT),
            canvas_palette: Palette::classic(),
            canvas_revision: 0,
            canvas_dirty: None,
            machines: vec![m0.clone(), m1.clone()],
            itr_count: 0,
            tape_init: TapeInit::default(),
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
            speed_snapshot_key: 0,
        };
        p.update(5);
        assert!(p.itr_count > 0);

        let preset = p.to_preset("two walkers").unwrap();
        assert_eq!(preset.version, PRESET_VERSION);
        assert_eq!(preset.machines.len(), 2);
        assert_eq!(preset.machines[0].speed, 2.5);
        assert_eq!(preset.machines[0].start_x, 10);
        assert_eq!(preset.machines[0].num_states, 1);
        assert_eq!(preset.machines[1].num_states, 1);

        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(q.machines[0].num_states, 1);
        assert_eq!(q.num_symbols, 2);
        assert_eq!(q.width, DEFAULT_MAP_WIDTH);
        assert_eq!(q.height, DEFAULT_MAP_HEIGHT);
        assert_eq!(q.machines.len(), 2);
        assert_eq!(q.machines[0].table, m0.table);
        assert_eq!(q.machines[1].table, m1.table);
        assert_eq!(q.machines[0].start_x, 10);
        assert_eq!(q.machines[0].start_y, 20);
        assert_eq!(q.machines[0].speed, 2.5);
        assert_eq!(q.machines[1].speed, -1.0);
        assert_eq!(q.itr_count, 0);
        assert!(q.map.iter().all(|&s| s == 0));
        assert_eq!(q.machines[0].x_pos, 10);
        assert_eq!(q.machines[0].state, 0);
    }

    #[test]
    fn mixed_state_counts_roundtrip() {
        let mut p = Program::new_random(2, 3);
        p.add_machine(5);
        assert_eq!(p.machines[0].num_states, 2);
        assert_eq!(p.machines[1].num_states, 5);
        let preset = p.to_preset("mixed").unwrap();
        assert_eq!(preset.machines[0].num_states, 2);
        assert_eq!(preset.machines[1].num_states, 5);
        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(q.machines[0].num_states, 2);
        assert_eq!(q.machines[1].num_states, 5);
        assert_eq!(q.machines[0].table.len(), 2 * 3 * 3);
        assert_eq!(q.machines[1].table.len(), 5 * 3 * 3);
    }

    #[test]
    fn reject_bad_preset() {
        let good = Preset {
            version: PRESET_VERSION,
            name: "ok".into(),
            saved_at: 0,
            num_states: 1,
            num_symbols: 2,
            map_width: DEFAULT_MAP_WIDTH,
            map_height: DEFAULT_MAP_HEIGHT,
            machines: vec![PresetMachine {
                start_x: 0,
                start_y: 0,
                speed: 0.0,
                active: true,
                name: String::new(),
                table: vec![0, 1, 0, 0, 1, 0],
                num_states: 1,
                palette: PaletteSpec::default(),
            }],
            performance_bindings: Vec::new(),
            tape_init: TapeInit::default(),
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
        };
        assert!(validate_preset(&good).is_ok());

        let mut bad = good.clone();
        bad.version = 99;
        assert!(validate_preset(&bad).is_err());

        bad = good.clone();
        bad.num_states = 0;
        bad.machines[0].num_states = 0;
        assert!(validate_preset(&bad).is_err());

        bad = good.clone();
        bad.machines[0].table = vec![1, 2];
        assert!(validate_preset(&bad).is_err());

        bad = good.clone();
        bad.machines.clear();
        assert!(validate_preset(&bad).is_err());

        assert!(Program::to_preset(&Program::new_random(2, 2), "   ").is_err());
    }

    #[test]
    fn json_roundtrip() {
        let p = Program::new_random(4, 3);
        let preset = p.to_preset("json test").unwrap();
        let text = serde_json::to_string(&preset).unwrap();
        let back: Preset = serde_json::from_str(&text).unwrap();
        assert_eq!(preset, back);
        let q = Program::from_preset(&back).unwrap();
        assert_eq!(q.machines[0].table, p.machines[0].table);
    }

    #[test]
    fn save_load_overwrite_and_delete() {
        let _guard = FS_LOCK.lock().unwrap();
        let Ok(dir) = ensure_presets_dir() else {
            return;
        };

        let unique = format!("test_preset_{}", std::process::id());
        let path = dir.join(format!("{unique}.json"));
        let _ = fs::remove_file(&path);

        let p = Program::new_random(2, 2);
        let mut preset = p.to_preset(&unique).unwrap();
        save_preset(&preset).unwrap();

        let loaded = load_preset(&unique).unwrap();
        assert_eq!(loaded.machines[0].table, preset.machines[0].table);
        assert!(loaded.saved_at > 0);

        // Overwrite with a different table length would fail validation; change speed.
        preset.machines[0].speed = 5.0;
        save_preset(&preset).unwrap();
        let loaded2 = load_preset(&unique).unwrap();
        assert_eq!(loaded2.machines[0].speed, 5.0);

        let listed = list_presets().unwrap();
        assert!(listed.iter().any(|i| i.name == unique));

        delete_preset(&unique).unwrap();
        assert!(load_preset(&unique).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn same_sanitized_name_overwrites() {
        let _guard = FS_LOCK.lock().unwrap();
        let Ok(_) = ensure_presets_dir() else {
            return;
        };

        let base = format!("overwrite_me_{}", std::process::id());
        // Space and underscore sanitize to the same filename stem.
        let name_space = format!("{base} x");
        let name_under = format!("{base}_x");
        assert_eq!(
            sanitize_filename(&name_space).unwrap(),
            sanitize_filename(&name_under).unwrap()
        );

        let _ = delete_preset(&name_space);
        let _ = delete_preset(&name_under);

        let p1 = Program::new_random(2, 2);
        let mut preset = p1.to_preset(&name_space).unwrap();
        save_preset(&preset).unwrap();

        let p2 = Program::new_random(3, 3);
        preset = p2.to_preset(&name_under).unwrap();
        save_preset(&preset).unwrap();

        let loaded = load_preset(&name_space).unwrap();
        assert_eq!(loaded.num_states, 3);
        assert_eq!(loaded.machines[0].num_states, 3);
        assert_eq!(loaded.num_symbols, 3);

        delete_preset(&name_under).unwrap();
    }

    fn info(name: &str, saved_at: u64) -> PresetInfo {
        PresetInfo {
            name: name.into(),
            num_machines: 1,
            num_states_min: 2,
            num_states_max: 2,
            num_symbols: 2,
            saved_at,
        }
    }

    #[test]
    fn sort_by_name_and_date() {
        let mut infos = vec![info("zeta", 10), info("Alpha", 30), info("beta", 20)];
        sort_preset_infos(&mut infos, PresetSort::Name);
        let names: Vec<_> = infos.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", "zeta"]);

        sort_preset_infos(&mut infos, PresetSort::DateSaved);
        let names: Vec<_> = infos.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", "zeta"]);

        let mut ties = vec![info("b", 5), info("a", 5), info("c", 9)];
        sort_preset_infos(&mut ties, PresetSort::DateSaved);
        let names: Vec<_> = ties.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["c", "a", "b"]);
    }

    #[test]
    fn old_json_without_saved_at_loads() {
        let json = r#"{
            "version": 1,
            "name": "legacy",
            "num_states": 1,
            "num_symbols": 2,
            "machines": [{"start_x": 0, "start_y": 0, "speed": 0.0, "table": [0, 1, 0, 0, 1, 0]}]
        }"#;
        let preset: Preset = serde_json::from_str(json).unwrap();
        assert_eq!(preset.saved_at, 0);
        assert_eq!(preset.map_width, DEFAULT_MAP_WIDTH);
        assert_eq!(preset.map_height, DEFAULT_MAP_HEIGHT);
        assert!(preset.machines[0].name.is_empty());
        assert!(validate_preset(&preset).is_ok());
        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(q.machines[0].name, "Machine 1");
        assert_eq!(q.machines[0].num_states, 1);
        assert_eq!(preset.machines[0].num_states, 0);
    }

    #[test]
    fn machine_name_roundtrips_in_preset() {
        let mut p = Program::new_random(2, 2);
        p.add_machine(2);
        p.machines[0].name = "Walker".into();
        p.machines[1].name = "Hopper".into();
        let preset = p.to_preset("named").unwrap();
        assert_eq!(preset.machines[0].name, "Walker");
        assert_eq!(preset.machines[1].name, "Hopper");
        let text = serde_json::to_string(&preset).unwrap();
        let back: Preset = serde_json::from_str(&text).unwrap();
        let q = Program::from_preset(&back).unwrap();
        assert_eq!(q.machines[0].name, "Walker");
        assert_eq!(q.machines[1].name, "Hopper");
    }

    #[test]
    fn custom_canvas_size_roundtrips() {
        let mut p = Program::new_random(2, 2);
        p.set_size(1920, 1080).unwrap();
        let preset = p.to_preset("wide").unwrap();
        assert_eq!(preset.map_width, 1920);
        assert_eq!(preset.map_height, 1080);
        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(q.width, 1920);
        assert_eq!(q.height, 1080);
        assert_eq!(q.map.len(), 1920 * 1080);
    }

    #[test]
    fn reject_invalid_map_size() {
        let mut bad = Preset {
            version: PRESET_VERSION,
            name: "ok".into(),
            saved_at: 0,
            num_states: 1,
            num_symbols: 2,
            map_width: 10,
            map_height: 512,
            machines: vec![PresetMachine {
                start_x: 0,
                start_y: 0,
                speed: 0.0,
                active: true,
                name: String::new(),
                table: vec![0, 1, 0, 0, 1, 0],
                num_states: 1,
                palette: PaletteSpec::default(),
            }],
            performance_bindings: Vec::new(),
            tape_init: TapeInit::default(),
            allow_diagonals: false,
            speed_snapshots: Vec::new(),
        };
        assert!(validate_preset(&bad).is_err());
        bad.map_width = 512;
        bad.map_height = 9000;
        assert!(validate_preset(&bad).is_err());
    }

    #[test]
    fn states_label_uniform_and_mixed() {
        let uniform = PresetInfo {
            name: "a".into(),
            num_machines: 2,
            num_states_min: 3,
            num_states_max: 3,
            num_symbols: 3,
            saved_at: 0,
        };
        assert_eq!(uniform.states_label(), "3");
        let mixed = PresetInfo {
            name: "b".into(),
            num_machines: 2,
            num_states_min: 3,
            num_states_max: 5,
            num_symbols: 3,
            saved_at: 0,
        };
        assert_eq!(mixed.states_label(), "3–5");
    }

    #[test]
    fn format_saved_at_is_utc() {
        assert_eq!(format_saved_at(0), "unknown date");
        // 2026-08-17 05:55:00 UTC
        assert_eq!(format_saved_at(1_786_946_100), "2026-08-17 05:55 UTC");
    }

    #[test]
    fn save_stamps_saved_at() {
        let _guard = FS_LOCK.lock().unwrap();
        let Ok(_) = ensure_presets_dir() else {
            return;
        };
        let unique = format!("stamp_preset_{}", std::process::id());
        let _ = delete_preset(&unique);
        let p = Program::new_random(2, 2);
        let preset = p.to_preset(&unique).unwrap();
        assert_eq!(preset.saved_at, 0);
        save_preset(&preset).unwrap();
        let loaded = load_preset(&unique).unwrap();
        assert!(loaded.saved_at > 0);
        delete_preset(&unique).unwrap();
    }

    #[test]
    fn old_json_without_tape_init_loads_empty() {
        let json = r#"{
            "version": 2,
            "name": "legacy tape",
            "num_states": 1,
            "num_symbols": 2,
            "map_width": 512,
            "map_height": 512,
            "machines": [{"start_x": 0, "start_y": 0, "speed": 0.0, "table": [0, 1, 0, 0, 1, 0]}]
        }"#;
        let preset: Preset = serde_json::from_str(json).unwrap();
        assert_eq!(preset.tape_init, TapeInit::default());
        assert_eq!(preset.tape_init.kind, TapeInitKind::Empty);
        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(q.tape_init.kind, TapeInitKind::Empty);
        assert!(q.map.iter().all(|&s| s == 0));
        assert_eq!(
            q.machines[0].palette.kind,
            crate::palette::PaletteKind::Classic
        );
    }

    #[test]
    fn machine_palette_roundtrips_in_preset() {
        let mut p = Program::new_random(2, 4);
        p.machines[0]
            .palette
            .set_kind(crate::palette::PaletteKind::Ocean, 4);
        p.machines[0]
            .palette
            .set_gradient_start_hex("#112233".into(), 4)
            .unwrap();
        p.machines[0]
            .palette
            .set_gradient_end_hex("#aabbcc".into(), 4)
            .unwrap();

        let preset = p.to_preset("ocean walker").unwrap();
        assert_eq!(preset.version, PRESET_VERSION);
        assert_eq!(
            preset.machines[0].palette.kind,
            crate::palette::PaletteKind::Ocean
        );
        assert_eq!(preset.machines[0].palette.gradient_start, "#112233");
        assert_eq!(preset.machines[0].palette.gradient_end, "#aabbcc");

        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(
            q.machines[0].palette.kind,
            crate::palette::PaletteKind::Ocean
        );
        assert_eq!(q.machines[0].palette.colors, p.machines[0].palette.colors);
        assert_eq!(q.machines[0].palette.gradient_start, [0x11, 0x22, 0x33]);
    }

    #[test]
    fn old_json_without_machine_palette_loads_classic() {
        let json = r#"{
            "version": 3,
            "name": "legacy palette",
            "num_states": 1,
            "num_symbols": 2,
            "machines": [{"start_x": 0, "start_y": 0, "speed": 0.0, "table": [0, 1, 0, 0, 1, 0]}]
        }"#;
        let preset: Preset = serde_json::from_str(json).unwrap();
        assert_eq!(preset.machines[0].palette, PaletteSpec::default());
        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(
            q.machines[0].palette.kind,
            crate::palette::PaletteKind::Classic
        );
    }

    #[test]
    fn tape_init_roundtrips_in_preset() {
        let mut p = Program::new_random(2, 4);
        p.tape_init = TapeInit {
            kind: TapeInitKind::Perlin,
            seed: 77,
            gaussian_mean: 0.3,
            gaussian_sigma: 0.2,
            perlin_scale: 16.0,
            perlin_octaves: 3,
        };
        p.reset();
        let first = p.map.clone();
        assert!(first.iter().any(|&s| s != 0));

        let preset = p.to_preset("noisy").unwrap();
        assert_eq!(preset.version, PRESET_VERSION);
        assert_eq!(preset.tape_init.kind, TapeInitKind::Perlin);
        assert_eq!(preset.tape_init.seed, 77);

        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(q.tape_init.kind, TapeInitKind::Perlin);
        assert_eq!(q.tape_init.seed, 77);
        assert_eq!(q.tape_init.perlin_scale, 16.0);
        assert_eq!(q.map, first);
    }

    #[test]
    fn allow_diagonals_roundtrips_in_preset() {
        let mut p = Program::new_random(2, 2);
        p.set_allow_diagonals(true);
        let preset = p.to_preset("diag").unwrap();
        assert!(preset.allow_diagonals);
        assert_eq!(preset.version, PRESET_VERSION);
        let q = Program::from_preset(&preset).unwrap();
        assert!(q.allow_diagonals);
    }

    #[test]
    fn old_json_without_allow_diagonals_loads_false() {
        let json = r#"{
            "version": 5,
            "name": "legacy diag",
            "num_states": 1,
            "num_symbols": 2,
            "map_width": 512,
            "map_height": 512,
            "machines": [{"start_x": 0, "start_y": 0, "speed": 0.0, "table": [0, 1, 0, 0, 1, 0]}]
        }"#;
        let preset: Preset = serde_json::from_str(json).unwrap();
        assert!(!preset.allow_diagonals);
        let q = Program::from_preset(&preset).unwrap();
        assert!(!q.allow_diagonals);
    }

    #[test]
    fn reject_invalid_action_in_preset() {
        let mut bad = Preset {
            version: PRESET_VERSION,
            name: "bad action".into(),
            saved_at: 0,
            num_states: 1,
            num_symbols: 2,
            map_width: DEFAULT_MAP_WIDTH,
            map_height: DEFAULT_MAP_HEIGHT,
            machines: vec![PresetMachine {
                start_x: 0,
                start_y: 0,
                speed: 0.0,
                active: true,
                name: String::new(),
                table: vec![0, 1, 8, 0, 1, 0],
                num_states: 1,
                palette: PaletteSpec::default(),
            }],
            performance_bindings: Vec::new(),
            tape_init: TapeInit::default(),
            allow_diagonals: true,
            speed_snapshots: Vec::new(),
        };
        assert!(validate_preset(&bad).is_err());
        bad.machines[0].table = vec![0, 1, 7, 0, 1, 0];
        assert!(validate_preset(&bad).is_ok());
    }

    #[test]
    fn speed_snapshots_roundtrip_in_preset() {
        let mut p = Program::new_random(2, 2);
        p.add_machine(2);
        p.set_machine_speed(0, 2.5).unwrap();
        p.set_machine_speed(1, -1.5).unwrap();
        p.store_speed_snapshot("Fast").unwrap();
        p.set_machine_speed(0, -3.0).unwrap();
        p.set_machine_speed(1, 4.0).unwrap();
        p.store_speed_snapshot("Slow").unwrap();

        let preset = p.to_preset("with snaps").unwrap();
        assert_eq!(preset.version, PRESET_VERSION);
        assert_eq!(preset.speed_snapshots.len(), 2);
        assert_eq!(preset.speed_snapshots[0].name, "Fast");
        assert_eq!(preset.speed_snapshots[0].speeds, vec![2.5, -1.5]);
        assert_eq!(preset.speed_snapshots[1].name, "Slow");
        assert_eq!(preset.speed_snapshots[1].speeds, vec![-3.0, 4.0]);

        let mut q = Program::from_preset(&preset).unwrap();
        assert_eq!(q.speed_snapshots, preset.speed_snapshots);
        q.recall_speed_snapshot("Fast").unwrap();
        assert_eq!(q.machines[0].speed, 2.5);
        assert_eq!(q.machines[1].speed, -1.5);
    }

    #[test]
    fn old_json_without_speed_snapshots_loads_empty() {
        let json = r#"{
            "version": 6,
            "name": "legacy snaps",
            "num_states": 1,
            "num_symbols": 2,
            "map_width": 512,
            "map_height": 512,
            "machines": [{"start_x": 0, "start_y": 0, "speed": 0.0, "table": [0, 1, 0, 0, 1, 0]}]
        }"#;
        let preset: Preset = serde_json::from_str(json).unwrap();
        assert!(preset.speed_snapshots.is_empty());
        let q = Program::from_preset(&preset).unwrap();
        assert!(q.speed_snapshots.is_empty());
    }

    #[test]
    fn malformed_speed_snapshots_are_dropped_on_load() {
        let mut p = Program::new_random(2, 2);
        p.add_machine(2);
        let mut preset = p.to_preset("bad snaps").unwrap();
        preset.speed_snapshots = vec![SpeedSnapshot {
            name: "oops".into(),
            speeds: vec![1.0], // wrong length
        }];
        let q = Program::from_preset(&preset).unwrap();
        assert!(q.speed_snapshots.is_empty());
    }
}
