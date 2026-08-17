//! Named on-disk presets for the full starting configuration of all machines.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::machine::{
    Machine, MAP_HEIGHT, MAP_WIDTH, MAX_MACHINE_SPEED, MAX_STATES, MAX_SYMBOLS, MIN_MACHINE_SPEED,
    MIN_STATES, MIN_SYMBOLS,
};
use crate::program::Program;

const PRESET_VERSION: u32 = 1;
const APP_DIR: &str = "turing_drawing";
const PRESETS_SUBDIR: &str = "presets";

/// One machine's starting configuration inside a preset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresetMachine {
    pub start_x: i32,
    pub start_y: i32,
    pub speed: f32,
    pub table: Vec<i32>,
}

/// Full program starting state saved as a named preset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Preset {
    pub version: u32,
    pub name: String,
    pub num_states: usize,
    pub num_symbols: usize,
    pub machines: Vec<PresetMachine>,
}

/// Summary row for the preset browser list.
#[derive(Debug, Clone)]
pub struct PresetInfo {
    pub name: String,
    pub num_machines: usize,
    pub num_states: usize,
    pub num_symbols: usize,
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
            num_states: self.num_states,
            num_symbols: self.num_symbols,
            machines: self
                .machines
                .iter()
                .map(|m| PresetMachine {
                    start_x: m.start_x,
                    start_y: m.start_y,
                    speed: m.speed,
                    table: m.table.clone(),
                })
                .collect(),
        })
    }

    /// Replace this program with a loaded preset and reset to starting state.
    pub fn from_preset(preset: &Preset) -> Result<Self, String> {
        validate_preset(preset)?;

        let machines: Vec<Machine> = preset
            .machines
            .iter()
            .map(|m| {
                let start_x = wrap_pos(m.start_x, MAP_WIDTH as i32);
                let start_y = wrap_pos(m.start_y, MAP_HEIGHT as i32);
                let speed = m.speed.clamp(MIN_MACHINE_SPEED, MAX_MACHINE_SPEED);
                Machine {
                    table: m.table.clone(),
                    state: 0,
                    x_pos: start_x,
                    y_pos: start_y,
                    start_x,
                    start_y,
                    speed,
                    rounds_at_speed: 0,
                    steps_at_speed: 0,
                }
            })
            .collect();

        let mut prog = Self {
            num_states: preset.num_states,
            num_symbols: preset.num_symbols,
            map: vec![0; crate::machine::MAP_LEN],
            machines,
            itr_count: 0,
        };
        prog.reset();
        Ok(prog)
    }
}

fn validate_preset(preset: &Preset) -> Result<(), String> {
    if preset.version != PRESET_VERSION {
        return Err(format!(
            "unsupported preset version {} (expected {PRESET_VERSION})",
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
    Ok(())
}

fn wrap_pos(v: i32, dim: i32) -> i32 {
    v.rem_euclid(dim)
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
    let base = dirs::data_dir().ok_or_else(|| "could not resolve app data directory".to_string())?;
    Ok(base.join(APP_DIR).join(PRESETS_SUBDIR))
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

/// Write (or overwrite) a preset to disk. Returns the display name stored.
pub fn save_preset(preset: &Preset) -> Result<String, String> {
    validate_preset(preset)?;
    let dir = ensure_presets_dir()?;
    let path = preset_path(&dir, &preset.name)?;
    let json = serde_json::to_string_pretty(preset)
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

/// List saved presets (sorted by name). Skips unreadable or invalid files.
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
        infos.push(PresetInfo {
            name: preset.name,
            num_machines: preset.machines.len(),
            num_states: preset.num_states,
            num_symbols: preset.num_symbols,
        });
    }
    infos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(infos)
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
            map: vec![0; crate::machine::MAP_LEN],
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
            num_states: 1,
            num_symbols: 2,
            machines: vec![PresetMachine {
                start_x: 0,
                start_y: 0,
                speed: 0.0,
                table: vec![0, 1, 0, 0, 1, 0],
            }],
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

        assert!(Program::to_preset(
            &Program::new_random(2, 2),
            "   "
        )
        .is_err());
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
}
