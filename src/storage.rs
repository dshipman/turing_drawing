//! Platform persistence for settings and presets.
//!
//! Native builds use the app data directory on disk. Wasm builds use
//! `localStorage` with keys that keep the same JSON payloads.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fs;
    use std::path::{Path, PathBuf};

    const APP_DIR: &str = "turing_drawing";
    const SETTINGS_FILE: &str = "settings.json";
    const PRESETS_SUBDIR: &str = "presets";

    pub fn app_data_dir() -> Result<PathBuf, String> {
        let base =
            dirs::data_dir().ok_or_else(|| "could not resolve app data directory".to_string())?;
        Ok(base.join(APP_DIR))
    }

    pub fn settings_path() -> Result<PathBuf, String> {
        Ok(app_data_dir()?.join(SETTINGS_FILE))
    }

    pub fn read_settings_text() -> Result<Option<String>, String> {
        let path = settings_path()?;
        if !path.exists() {
            return Ok(None);
        }
        fs::read_to_string(&path)
            .map(Some)
            .map_err(|e| format!("failed to read settings: {e}"))
    }

    pub fn write_settings_text(json: &str) -> Result<(), String> {
        let dir = app_data_dir()?;
        fs::create_dir_all(&dir)
            .map_err(|e| format!("could not create settings directory: {e}"))?;
        fs::write(dir.join(SETTINGS_FILE), json)
            .map_err(|e| format!("failed to write settings: {e}"))
    }

    pub fn write_settings_to(path: &Path, json: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("could not create settings directory: {e}"))?;
        }
        fs::write(path, json).map_err(|e| format!("failed to write settings: {e}"))
    }

    pub fn read_settings_from(path: &Path) -> Result<Option<String>, String> {
        if !path.exists() {
            return Ok(None);
        }
        fs::read_to_string(path)
            .map(Some)
            .map_err(|e| format!("failed to read settings: {e}"))
    }

    pub fn presets_dir() -> Result<PathBuf, String> {
        Ok(app_data_dir()?.join(PRESETS_SUBDIR))
    }

    pub fn ensure_presets_dir() -> Result<PathBuf, String> {
        let dir = presets_dir()?;
        fs::create_dir_all(&dir).map_err(|e| format!("could not create presets directory: {e}"))?;
        Ok(dir)
    }

    pub fn preset_file_path(name_stem: &str) -> Result<PathBuf, String> {
        Ok(presets_dir()?.join(format!("{name_stem}.json")))
    }

    pub fn write_preset_text(name_stem: &str, json: &str) -> Result<(), String> {
        let dir = ensure_presets_dir()?;
        let path = dir.join(format!("{name_stem}.json"));
        fs::write(&path, json).map_err(|e| format!("failed to write preset: {e}"))
    }

    pub fn read_preset_text(name_stem: &str) -> Result<String, String> {
        let path = preset_file_path(name_stem)?;
        fs::read_to_string(&path).map_err(|e| format!("failed to read preset: {e}"))
    }

    pub fn delete_preset_text(name_stem: &str) -> Result<(), String> {
        let path = preset_file_path(name_stem)?;
        if !path.exists() {
            return Err(format!("preset not found: {name_stem}"));
        }
        fs::remove_file(&path).map_err(|e| format!("failed to delete preset: {e}"))
    }

    pub fn list_preset_entries() -> Result<Vec<(String, Option<u64>)>, String> {
        let dir = match presets_dir() {
            Ok(d) => d,
            Err(e) => return Err(e),
        };
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let entries =
            fs::read_dir(&dir).map_err(|e| format!("failed to list presets directory: {e}"))?;

        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let mtime = file_mtime_secs(&path);
            out.push((text, mtime));
        }
        Ok(out)
    }

    fn file_mtime_secs(path: &Path) -> Option<u64> {
        use std::time::UNIX_EPOCH;
        let meta = fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?;
        Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs())
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    const SETTINGS_KEY: &str = "turing_drawing.settings";
    const PRESET_PREFIX: &str = "turing_drawing.preset.";

    fn local_storage() -> Result<web_sys::Storage, String> {
        let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
        window
            .local_storage()
            .map_err(|_| "localStorage unavailable".to_string())?
            .ok_or_else(|| "localStorage unavailable".to_string())
    }

    pub fn read_settings_text() -> Result<Option<String>, String> {
        let storage = local_storage()?;
        storage
            .get_item(SETTINGS_KEY)
            .map_err(|_| "failed to read settings from localStorage".to_string())
    }

    pub fn write_settings_text(json: &str) -> Result<(), String> {
        let storage = local_storage()?;
        storage
            .set_item(SETTINGS_KEY, json)
            .map_err(|_| "failed to write settings to localStorage".to_string())
    }

    pub fn write_preset_text(name_stem: &str, json: &str) -> Result<(), String> {
        let storage = local_storage()?;
        let key = format!("{PRESET_PREFIX}{name_stem}");
        storage
            .set_item(&key, json)
            .map_err(|_| "failed to write preset to localStorage".to_string())
    }

    pub fn read_preset_text(name_stem: &str) -> Result<String, String> {
        let storage = local_storage()?;
        let key = format!("{PRESET_PREFIX}{name_stem}");
        storage
            .get_item(&key)
            .map_err(|_| "failed to read preset from localStorage".to_string())?
            .ok_or_else(|| format!("preset not found: {name_stem}"))
    }

    pub fn delete_preset_text(name_stem: &str) -> Result<(), String> {
        let storage = local_storage()?;
        let key = format!("{PRESET_PREFIX}{name_stem}");
        if storage
            .get_item(&key)
            .map_err(|_| "failed to read preset from localStorage".to_string())?
            .is_none()
        {
            return Err(format!("preset not found: {name_stem}"));
        }
        storage
            .remove_item(&key)
            .map_err(|_| "failed to delete preset from localStorage".to_string())
    }

    pub fn list_preset_entries() -> Result<Vec<(String, Option<u64>)>, String> {
        let storage = local_storage()?;
        let len = storage
            .length()
            .map_err(|_| "failed to list localStorage keys".to_string())?;
        let mut out = Vec::new();
        for i in 0..len {
            let Ok(Some(key)) = storage.key(i) else {
                continue;
            };
            if !key.starts_with(PRESET_PREFIX) {
                continue;
            }
            let Ok(Some(text)) = storage.get_item(&key) else {
                continue;
            };
            out.push((text, None));
        }
        Ok(out)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

/// Unix timestamp in seconds, portable across native and wasm.
pub fn unix_now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
