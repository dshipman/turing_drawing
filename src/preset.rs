//! Named on-disk presets for the full starting configuration of all machines.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::machine::{
    default_machine_name, validate_map_size, wrap_pos, Machine, DEFAULT_MAP_HEIGHT,
    DEFAULT_MAP_WIDTH, MAX_MACHINE_SPEED, MAX_STATES, MAX_SYMBOLS, MIN_MACHINE_SPEED, MIN_STATES,
    MIN_SYMBOLS,
};
use crate::program::Program;
use crate::settings::PresetPerformanceBinding;

const PRESET_VERSION: u32 = 2;
const MIN_PRESET_VERSION: u32 = 1;
const PRESETS_SUBDIR: &str = "presets";

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
}

fn default_active() -> bool {
    true
}

/// Full program starting state saved as a named preset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub version: u32,
    pub name: String,
    /// Unix timestamp (seconds) of the last Store. Missing in older files.
    #[serde(default)]
    pub saved_at: u64,
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
    pub num_states: usize,
    pub num_symbols: usize,
    pub saved_at: u64,
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

        Ok(Preset {
            version: PRESET_VERSION,
            name: name.to_string(),
            saved_at: 0,
            num_states: self.num_states,
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
                })
                .collect(),
            performance_bindings: Vec::new(),
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
                Machine {
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
                    rounds_at_speed: 0,
                    steps_at_speed: 0,
                }
            })
            .collect();

        let mut prog = Self {
            num_states: preset.num_states,
            num_symbols: preset.num_symbols,
            width,
            height,
            map: vec![0; width * height],
            machines,
            itr_count: 0,
        };
        prog.ensure_machine_ids();
        prog.reset();
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

    let expected = preset.num_states * preset.num_symbols * 3;
    for (i, m) in preset.machines.iter().enumerate() {
        if m.table.len() != expected {
            return Err(format!(
                "machine {}: table length {}, expected {expected}",
                i + 1,
                m.table.len()
            ));
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

/// Directory that holds preset JSON files.
pub fn presets_dir() -> Result<PathBuf, String> {
    Ok(crate::settings::app_data_dir()?.join(PRESETS_SUBDIR))
}

fn ensure_presets_dir() -> Result<PathBuf, String> {
    let dir = presets_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("could not create presets directory: {e}"))?;
    Ok(dir)
}

fn preset_path(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let stem = sanitize_filename(name)?;
    Ok(dir.join(format!("{stem}.json")))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn file_mtime_secs(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs())
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

/// Write (or overwrite) a preset to disk. Returns the display name stored.
/// Stamps `saved_at` with the current time so date-saved sort stays accurate.
pub fn save_preset(preset: &Preset) -> Result<String, String> {
    validate_preset(preset)?;
    let dir = ensure_presets_dir()?;
    let path = preset_path(&dir, &preset.name)?;
    let mut to_write = preset.clone();
    to_write.saved_at = unix_now();
    let json = serde_json::to_string_pretty(&to_write)
        .map_err(|e| format!("failed to serialize preset: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("failed to write preset: {e}"))?;
    Ok(preset.name.clone())
}

/// Load a preset by display name (filename stem from sanitized name).
pub fn load_preset(name: &str) -> Result<Preset, String> {
    let dir = presets_dir()?;
    let path = preset_path(&dir, name)?;
    let text = fs::read_to_string(&path).map_err(|e| format!("failed to read preset: {e}"))?;
    let preset: Preset =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse preset: {e}"))?;
    validate_preset(&preset)?;
    Ok(preset)
}

/// Delete a preset file by display name.
pub fn delete_preset(name: &str) -> Result<(), String> {
    let dir = presets_dir()?;
    let path = preset_path(&dir, name)?;
    if !path.exists() {
        return Err(format!("preset not found: {name}"));
    }
    fs::remove_file(&path).map_err(|e| format!("failed to delete preset: {e}"))?;
    Ok(())
}

/// List saved presets (unsorted). Skips unreadable or invalid files.
/// Older files without `saved_at` fall back to the file's modification time.
pub fn list_presets() -> Result<Vec<PresetInfo>, String> {
    let dir = match presets_dir() {
        Ok(d) => d,
        Err(e) => return Err(e),
    };
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries =
        fs::read_dir(&dir).map_err(|e| format!("failed to list presets directory: {e}"))?;

    let mut infos = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(preset) = serde_json::from_str::<Preset>(&text) else {
            continue;
        };
        if validate_preset(&preset).is_err() {
            continue;
        }
        let saved_at = if preset.saved_at > 0 {
            preset.saved_at
        } else {
            file_mtime_secs(&path).unwrap_or(0)
        };
        infos.push(PresetInfo {
            name: preset.name,
            num_machines: preset.machines.len(),
            num_states: preset.num_states,
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
    use std::sync::Mutex;

    // Serialize filesystem tests that touch a shared temp-style path via env override
    // is awkward with dirs; use an isolated temp dir by exercising save/load via
    // path helpers and validate/roundtrip without requiring the real app-data dir
    // for pure logic tests. Disk tests use a mutex + unique subdirectory under
    // the real presets dir when available, or skip if data_dir is unavailable.

    static FS_LOCK: Mutex<()> = Mutex::new(());

    fn fixed_machine(table: Vec<i32>, start_x: i32, start_y: i32, speed: f32) -> Machine {
        Machine {
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
        let m0 = fixed_machine(vec![0, 1, ACTION_LEFT, 0, 1, ACTION_LEFT], 10, 20, 2.5);
        let m1 = fixed_machine(vec![0, 1, ACTION_DOWN, 0, 1, ACTION_DOWN], 30, 40, -1.0);
        let mut p = Program {
            num_states: 1,
            num_symbols: 2,
            width: DEFAULT_MAP_WIDTH,
            height: DEFAULT_MAP_HEIGHT,
            map: vec![0; DEFAULT_MAP_WIDTH * DEFAULT_MAP_HEIGHT],
            machines: vec![m0.clone(), m1.clone()],
            itr_count: 0,
        };
        p.update(5);
        assert!(p.itr_count > 0);

        let preset = p.to_preset("two walkers").unwrap();
        assert_eq!(preset.version, PRESET_VERSION);
        assert_eq!(preset.machines.len(), 2);
        assert_eq!(preset.machines[0].speed, 2.5);
        assert_eq!(preset.machines[0].start_x, 10);

        let q = Program::from_preset(&preset).unwrap();
        assert_eq!(q.num_states, 1);
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
            }],
            performance_bindings: Vec::new(),
        };
        assert!(validate_preset(&good).is_ok());

        let mut bad = good.clone();
        bad.version = 99;
        assert!(validate_preset(&bad).is_err());

        bad = good.clone();
        bad.num_states = 0;
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
        assert_eq!(loaded.num_symbols, 3);

        delete_preset(&name_under).unwrap();
    }

    fn info(name: &str, saved_at: u64) -> PresetInfo {
        PresetInfo {
            name: name.into(),
            num_machines: 1,
            num_states: 2,
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
    }

    #[test]
    fn machine_name_roundtrips_in_preset() {
        let mut p = Program::new_random(2, 2);
        p.add_machine();
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
            }],
            performance_bindings: Vec::new(),
        };
        assert!(validate_preset(&bad).is_err());
        bad.map_width = 512;
        bad.map_height = 9000;
        assert!(validate_preset(&bad).is_err());
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
}
