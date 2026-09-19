//! Persisted startup defaults and keybindings for the app.

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use atelier_ui::Appearance;
use iced::keyboard::{key, Key, Modifiers};
use serde::{Deserialize, Deserializer, Serialize};

use crate::gpu_raster::RasterMode;
use crate::machine::{
    DEFAULT_MAP_HEIGHT, DEFAULT_MAP_WIDTH, MAX_MAP_SIZE, MAX_STATES, MAX_SYMBOLS, MIN_MAP_SIZE,
    MIN_STATES, MIN_SYMBOLS,
};
use crate::palette::{
    parse_hex_rgb, rgb_to_hex, PaletteKind, DEFAULT_GRADIENT_END, DEFAULT_GRADIENT_START,
};
use crate::program::ScheduleMode;
use crate::storage;
use crate::tape::{
    TapeInitKind, DEFAULT_GAUSSIAN_MEAN, DEFAULT_GAUSSIAN_SIGMA, DEFAULT_PERLIN_OCTAVES,
    DEFAULT_PERLIN_SCALE, MAX_GAUSSIAN_MEAN, MAX_GAUSSIAN_SIGMA, MAX_PERLIN_OCTAVES,
    MAX_PERLIN_SCALE, MIN_GAUSSIAN_MEAN, MIN_GAUSSIAN_SIGMA, MIN_PERLIN_OCTAVES, MIN_PERLIN_SCALE,
};

const SETTINGS_VERSION: u32 = 1;

pub const DEFAULT_REFRESH_HZ: u32 = 60;
pub const MIN_REFRESH_HZ: u32 = 1;
pub const MAX_REFRESH_HZ: u32 = 240;
pub const DEFAULT_MAX_ITRS: u64 = 350_000;
pub const MIN_MAX_ITRS: u64 = 1_000;
pub const MAX_MAX_ITRS: u64 = 2_000_000;

/// App data folder (`…/turing_drawing`), shared with presets. Native only.
#[cfg(not(target_arch = "wasm32"))]
pub fn app_data_dir() -> Result<std::path::PathBuf, String> {
    storage::app_data_dir()
}

/// Path to `settings.json` under the app data folder. Native only.
#[cfg(not(target_arch = "wasm32"))]
pub fn settings_path() -> Result<std::path::PathBuf, String> {
    storage::settings_path()
}

/// Rebindable commands. Machine/preset actions need an id or name on the binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Random,
    Mutate,
    Restart,
    OpenPresets,
    OpenSettings,
    ToggleFullscreen,
    AddMachine,
    IncStates,
    DecStates,
    IncSymbols,
    DecSymbols,
    ToggleCanvas,
    TogglePalette,
    ToggleSimulation,
    CloseMachineDetails,
    StorePreset,
    ClosePresetBrowser,
    CloseSettings,
    CloseColorPicker,
    SettingsTabDefaults,
    SettingsTabKeybindings,
    RandomizeMachine,
    MutateMachine,
    TogglePickStart,
    CopyShare,
    LoadShare,
    RemoveMachine,
    LoadPreset,
    DeletePreset,
    ReseedTape,
}

impl Action {
    pub const GLOBAL: [Action; 22] = [
        Self::Random,
        Self::Mutate,
        Self::Restart,
        Self::ReseedTape,
        Self::OpenPresets,
        Self::OpenSettings,
        Self::ToggleFullscreen,
        Self::AddMachine,
        Self::IncStates,
        Self::DecStates,
        Self::IncSymbols,
        Self::DecSymbols,
        Self::ToggleCanvas,
        Self::TogglePalette,
        Self::ToggleSimulation,
        Self::CloseMachineDetails,
        Self::StorePreset,
        Self::ClosePresetBrowser,
        Self::CloseSettings,
        Self::CloseColorPicker,
        Self::SettingsTabDefaults,
        Self::SettingsTabKeybindings,
    ];

    pub const PERFORMANCE: [Action; 6] = [
        Self::RandomizeMachine,
        Self::MutateMachine,
        Self::TogglePickStart,
        Self::CopyShare,
        Self::LoadShare,
        Self::RemoveMachine,
    ];

    pub fn is_performance(self) -> bool {
        Self::PERFORMANCE.contains(&self)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Random => "Random",
            Self::Mutate => "Mutate",
            Self::Restart => "Restart",
            Self::ReseedTape => "Reseed tape",
            Self::OpenPresets => "Presets",
            Self::OpenSettings => "Settings",
            Self::ToggleFullscreen => "Toggle fullscreen",
            Self::AddMachine => "Add machine",
            Self::IncStates => "States +",
            Self::DecStates => "States −",
            Self::IncSymbols => "Symbols +",
            Self::DecSymbols => "Symbols −",
            Self::ToggleCanvas => "Canvas group",
            Self::TogglePalette => "Palette group",
            Self::ToggleSimulation => "Simulation group",
            Self::CloseMachineDetails => "Close machine details",
            Self::StorePreset => "Store preset",
            Self::ClosePresetBrowser => "Close presets",
            Self::CloseSettings => "Close settings",
            Self::CloseColorPicker => "Close colour picker",
            Self::SettingsTabDefaults => "Settings: Defaults",
            Self::SettingsTabKeybindings => "Settings: Keybindings",
            Self::RandomizeMachine => "Randomise",
            Self::MutateMachine => "Mutate machine",
            Self::TogglePickStart => "Set start",
            Self::CopyShare => "Copy encoding",
            Self::LoadShare => "Load encoding",
            Self::RemoveMachine => "Remove machine",
            Self::LoadPreset => "Load preset",
            Self::DeletePreset => "Delete preset",
        }
    }
}

/// Which control a keybinding belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindTarget {
    pub action: Action,
    pub machine_id: Option<u64>,
    pub preset_name: Option<String>,
}

impl BindTarget {
    pub fn global(action: Action) -> Self {
        Self {
            action,
            machine_id: None,
            preset_name: None,
        }
    }

    pub fn machine(action: Action, machine_id: u64) -> Self {
        Self {
            action,
            machine_id: Some(machine_id),
            preset_name: None,
        }
    }

    pub fn preset(action: Action, name: impl Into<String>) -> Self {
        Self {
            action,
            machine_id: None,
            preset_name: Some(name.into()),
        }
    }

    pub fn label(&self, machine_name: Option<&str>) -> String {
        let base = self.action.label();
        if let Some(name) = machine_name {
            format!("Performance binding — {base} ({name})")
        } else if let Some(preset) = &self.preset_name {
            format!("{base} — {preset}")
        } else {
            base.to_string()
        }
    }

    pub fn is_performance(&self) -> bool {
        self.machine_id.is_some()
    }
}

impl From<Action> for BindTarget {
    fn from(action: Action) -> Self {
        Self::global(action)
    }
}

/// One shortcut: a character or named key plus modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keybinding {
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ctrl: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub logo: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl Keybinding {
    pub fn restart_shift_r() -> Self {
        Self {
            action: Action::Restart,
            machine_id: None,
            preset_name: None,
            key: Some("r".into()),
            named: None,
            shift: true,
            ctrl: false,
            alt: false,
            logo: false,
        }
    }

    pub fn fullscreen_f11() -> Self {
        Self {
            action: Action::ToggleFullscreen,
            machine_id: None,
            preset_name: None,
            key: None,
            named: Some("f11".into()),
            shift: false,
            ctrl: false,
            alt: false,
            logo: false,
        }
    }

    pub fn from_event(
        target: impl Into<BindTarget>,
        key: &Key,
        modifiers: Modifiers,
    ) -> Option<Self> {
        let target = target.into();
        match key {
            Key::Named(named) => {
                if is_unusable_named(*named) {
                    return None;
                }
                Some(Self {
                    action: target.action,
                    machine_id: target.machine_id,
                    preset_name: target.preset_name,
                    key: None,
                    named: Some(named_id(*named)),
                    shift: modifiers.shift(),
                    ctrl: modifiers.control(),
                    alt: modifiers.alt(),
                    logo: modifiers.logo(),
                })
            }
            Key::Character(c) => {
                let s = c.to_ascii_lowercase();
                if s.trim().is_empty() {
                    return None;
                }
                Some(Self {
                    action: target.action,
                    machine_id: target.machine_id,
                    preset_name: target.preset_name,
                    key: Some(s),
                    named: None,
                    shift: modifiers.shift(),
                    ctrl: modifiers.control(),
                    alt: modifiers.alt(),
                    logo: modifiers.logo(),
                })
            }
            Key::Unidentified => None,
        }
    }

    pub fn target(&self) -> BindTarget {
        BindTarget {
            action: self.action,
            machine_id: self.machine_id,
            preset_name: self.preset_name.clone(),
        }
    }

    pub fn same_target(&self, other: &Self) -> bool {
        self.action == other.action
            && self.machine_id == other.machine_id
            && self.preset_name == other.preset_name
    }

    pub fn matches(&self, key: &Key, modifiers: Modifiers) -> bool {
        if self.shift != modifiers.shift()
            || self.ctrl != modifiers.control()
            || self.alt != modifiers.alt()
            || self.logo != modifiers.logo()
        {
            return false;
        }
        match key {
            Key::Character(c) => {
                self.named.is_none()
                    && self
                        .key
                        .as_deref()
                        .is_some_and(|stored| stored.eq_ignore_ascii_case(c))
            }
            Key::Named(named) => {
                self.key.is_none()
                    && self
                        .named
                        .as_deref()
                        .is_some_and(|stored| stored == named_id(*named))
            }
            Key::Unidentified => false,
        }
    }

    pub fn same_combo(&self, other: &Self) -> bool {
        self.key == other.key
            && self.named == other.named
            && self.shift == other.shift
            && self.ctrl == other.ctrl
            && self.alt == other.alt
            && self.logo == other.logo
    }

    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.logo {
            parts.push(if cfg!(target_os = "macos") {
                "Cmd".to_string()
            } else {
                "Super".to_string()
            });
        }
        parts.push(self.key_label());
        parts.join("+")
    }

    fn key_label(&self) -> String {
        if let Some(named) = &self.named {
            return display_named(named);
        }
        if let Some(key) = &self.key {
            if key.len() == 1 {
                return key.to_ascii_uppercase();
            }
            return key.clone();
        }
        "?".into()
    }
}

/// One performance binding inside a preset file (keyed by machine list index).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetPerformanceBinding {
    pub machine_index: usize,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ctrl: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub logo: bool,
}

impl PresetPerformanceBinding {
    pub fn from_keybinding(binding: &Keybinding, machine_index: usize) -> Option<Self> {
        if binding.machine_id.is_none() || !binding.action.is_performance() {
            return None;
        }
        Some(Self {
            machine_index,
            action: binding.action,
            key: binding.key.clone(),
            named: binding.named.clone(),
            shift: binding.shift,
            ctrl: binding.ctrl,
            alt: binding.alt,
            logo: binding.logo,
        })
    }

    pub fn to_keybinding(&self, machine_id: u64) -> Keybinding {
        Keybinding {
            action: self.action,
            machine_id: Some(machine_id),
            preset_name: None,
            key: self.key.clone(),
            named: self.named.clone(),
            shift: self.shift,
            ctrl: self.ctrl,
            alt: self.alt,
            logo: self.logo,
        }
    }

    pub fn display(&self) -> String {
        Keybinding {
            action: self.action,
            machine_id: None,
            preset_name: None,
            key: self.key.clone(),
            named: self.named.clone(),
            shift: self.shift,
            ctrl: self.ctrl,
            alt: self.alt,
            logo: self.logo,
        }
        .display()
    }
}

/// Per-machine shortcuts for the current session; persisted inside presets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PerformanceBindings {
    bindings: Vec<Keybinding>,
}

impl PerformanceBindings {
    pub fn binding_for(&self, target: &BindTarget) -> Option<&Keybinding> {
        let id = target.machine_id?;
        self.bindings
            .iter()
            .find(|b| b.action == target.action && b.machine_id == Some(id))
    }

    pub fn set_binding(&mut self, binding: Keybinding) {
        debug_assert!(binding.machine_id.is_some());
        self.bindings
            .retain(|b| !b.same_target(&binding) && !b.same_combo(&binding));
        self.bindings.push(binding);
    }

    pub fn clear_binding(&mut self, target: &BindTarget) {
        let Some(id) = target.machine_id else {
            return;
        };
        self.bindings
            .retain(|b| b.action != target.action || b.machine_id != Some(id));
    }

    pub fn clear_machine(&mut self, machine_id: u64) {
        self.bindings.retain(|b| b.machine_id != Some(machine_id));
    }

    pub fn retain_machine_ids(&mut self, ids: &[u64]) {
        self.bindings.retain(|b| match b.machine_id {
            Some(id) => ids.contains(&id),
            None => false,
        });
    }

    pub fn match_binding(&self, key: &Key, modifiers: Modifiers) -> Option<&Keybinding> {
        self.bindings.iter().find(|b| b.matches(key, modifiers))
    }

    pub fn to_preset_bindings(&self, machine_ids: &[u64]) -> Vec<PresetPerformanceBinding> {
        self.bindings
            .iter()
            .filter_map(|b| {
                let id = b.machine_id?;
                let index = machine_ids.iter().position(|&m| m == id)?;
                PresetPerformanceBinding::from_keybinding(b, index)
            })
            .collect()
    }

    pub fn from_preset_bindings(
        preset_bindings: &[PresetPerformanceBinding],
        machine_ids: &[u64],
    ) -> Self {
        let bindings = preset_bindings
            .iter()
            .filter_map(|pb| {
                let id = *machine_ids.get(pb.machine_index)?;
                Some(pb.to_keybinding(id))
            })
            .collect();
        Self { bindings }
    }
}

fn named_id(named: key::Named) -> String {
    format!("{named:?}").to_ascii_lowercase()
}

fn display_named(id: &str) -> String {
    if id.starts_with('f') && id.len() <= 3 && id[1..].chars().all(|c| c.is_ascii_digit()) {
        return id.to_ascii_uppercase();
    }
    let mut chars = id.chars();
    match chars.next() {
        Some(c) => format!("{}{}", c.to_ascii_uppercase(), chars.as_str()),
        None => id.to_string(),
    }
}

fn is_unusable_named(named: key::Named) -> bool {
    matches!(
        named,
        key::Named::Escape
            | key::Named::Shift
            | key::Named::Control
            | key::Named::Alt
            | key::Named::AltGraph
            | key::Named::Super
            | key::Named::Meta
            | key::Named::Hyper
            | key::Named::Symbol
            | key::Named::CapsLock
            | key::Named::NumLock
            | key::Named::ScrollLock
            | key::Named::Fn
            | key::Named::FnLock
            | key::Named::SymbolLock
    )
}

/// Startup values for the right-hand inspector.
///
/// `num_states` is the default state count for the first machine at launch and
/// for machines added with Add machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelDefaults {
    #[serde(default = "default_num_states")]
    pub num_states: usize,
    #[serde(default = "default_num_symbols")]
    pub num_symbols: usize,
    #[serde(default = "default_map_width")]
    pub map_width: usize,
    #[serde(default = "default_map_height")]
    pub map_height: usize,
    #[serde(default)]
    pub tape_kind: TapeInitKind,
    #[serde(default = "default_gaussian_mean")]
    pub gaussian_mean: f32,
    #[serde(default = "default_gaussian_sigma")]
    pub gaussian_sigma: f32,
    #[serde(default = "default_perlin_scale")]
    pub perlin_scale: f32,
    #[serde(default = "default_perlin_octaves")]
    pub perlin_octaves: u8,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default = "default_refresh_hz")]
    pub refresh_hz: u32,
    #[serde(default = "default_max_itrs")]
    pub max_itrs: u64,
    #[serde(default)]
    pub schedule_mode: ScheduleMode,
    #[serde(default)]
    pub allow_diagonals: bool,
    #[serde(default)]
    pub raster_mode: RasterMode,
    #[serde(default)]
    pub palette_kind: PaletteKind,
    #[serde(default = "default_gradient_start")]
    pub gradient_start: String,
    #[serde(default = "default_gradient_end")]
    pub gradient_end: String,
}

fn default_num_states() -> usize {
    3
}
fn default_num_symbols() -> usize {
    3
}
fn default_map_width() -> usize {
    DEFAULT_MAP_WIDTH
}
fn default_map_height() -> usize {
    DEFAULT_MAP_HEIGHT
}
fn default_speed() -> f32 {
    1.0
}
fn default_refresh_hz() -> u32 {
    DEFAULT_REFRESH_HZ
}
fn default_max_itrs() -> u64 {
    DEFAULT_MAX_ITRS
}
fn default_gradient_start() -> String {
    rgb_to_hex(DEFAULT_GRADIENT_START)
}
fn default_gradient_end() -> String {
    rgb_to_hex(DEFAULT_GRADIENT_END)
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

impl Default for PanelDefaults {
    fn default() -> Self {
        Self {
            num_states: default_num_states(),
            num_symbols: default_num_symbols(),
            map_width: default_map_width(),
            map_height: default_map_height(),
            tape_kind: TapeInitKind::Empty,
            gaussian_mean: default_gaussian_mean(),
            gaussian_sigma: default_gaussian_sigma(),
            perlin_scale: default_perlin_scale(),
            perlin_octaves: default_perlin_octaves(),
            speed: default_speed(),
            refresh_hz: default_refresh_hz(),
            max_itrs: default_max_itrs(),
            schedule_mode: ScheduleMode::Absolute,
            allow_diagonals: false,
            raster_mode: RasterMode::Gpu,
            palette_kind: PaletteKind::Classic,
            gradient_start: default_gradient_start(),
            gradient_end: default_gradient_end(),
        }
    }
}

impl PanelDefaults {
    pub fn sanitize(&mut self) {
        self.num_states = self.num_states.clamp(MIN_STATES, MAX_STATES);
        self.num_symbols = self.num_symbols.clamp(MIN_SYMBOLS, MAX_SYMBOLS);
        self.map_width = self.map_width.clamp(MIN_MAP_SIZE, MAX_MAP_SIZE);
        self.map_height = self.map_height.clamp(MIN_MAP_SIZE, MAX_MAP_SIZE);
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
        if !self.speed.is_finite() {
            self.speed = default_speed();
        } else {
            self.speed = self.speed.clamp(0.0, 1.0);
        }
        self.refresh_hz = self.refresh_hz.clamp(MIN_REFRESH_HZ, MAX_REFRESH_HZ);
        self.max_itrs = self.max_itrs.clamp(MIN_MAX_ITRS, MAX_MAX_ITRS);
        match parse_hex_rgb(&self.gradient_start) {
            Ok(rgb) => self.gradient_start = rgb_to_hex(rgb),
            Err(_) => self.gradient_start = default_gradient_start(),
        }
        match parse_hex_rgb(&self.gradient_end) {
            Ok(rgb) => self.gradient_end = rgb_to_hex(rgb),
            Err(_) => self.gradient_end = default_gradient_end(),
        }
    }
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

/// On-disk settings file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UserSettings {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub defaults: PanelDefaults,
    #[serde(default = "default_keybindings")]
    pub keybindings: Vec<Keybinding>,
}

/// Legacy built-in theme enum persisted before appearance panes.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyTheme {
    Graphite,
    Slate,
    Paper,
}

impl LegacyTheme {
    fn name(self) -> &'static str {
        match self {
            Self::Graphite => "graphite",
            Self::Slate => "slate",
            Self::Paper => "paper",
        }
    }
}

impl<'de> Deserialize<'de> for UserSettings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default = "default_version")]
            version: u32,
            #[serde(default)]
            theme: Option<LegacyTheme>,
            #[serde(default)]
            appearance: Option<Appearance>,
            #[serde(default)]
            defaults: PanelDefaults,
            #[serde(default = "default_keybindings")]
            keybindings: Vec<Keybinding>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let appearance = raw.appearance.unwrap_or_else(|| {
            appearance_from_legacy_theme(raw.theme.unwrap_or(LegacyTheme::Graphite))
        });

        Ok(Self {
            version: raw.version,
            appearance,
            defaults: raw.defaults,
            keybindings: raw.keybindings,
        })
    }
}

fn appearance_from_legacy_theme(legacy: LegacyTheme) -> Appearance {
    let name = legacy.name();
    let mut appearance = Appearance::default();
    if let Some(theme) = atelier_ui::Theme::resolve(name) {
        appearance.theme = theme;
        appearance.save_as = name.into();
    }
    appearance
}

fn default_version() -> u32 {
    SETTINGS_VERSION
}

fn default_keybindings() -> Vec<Keybinding> {
    vec![Keybinding::restart_shift_r(), Keybinding::fullscreen_f11()]
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            appearance: Appearance::default(),
            defaults: PanelDefaults::default(),
            keybindings: default_keybindings(),
        }
    }
}

impl UserSettings {
    /// Push the persisted appearance into the process-global Atelier theme.
    pub fn apply_appearance(&self) {
        self.appearance.apply();
    }

    pub fn load() -> Self {
        match storage::read_settings_text() {
            Ok(Some(text)) => Self::from_json(&text),
            Ok(None) | Err(_) => Self::default(),
        }
    }

    fn from_json(text: &str) -> Self {
        match serde_json::from_str::<Self>(text) {
            Ok(mut settings) => {
                settings.sanitize();
                settings
            }
            Err(_) => Self::default(),
        }
    }

    /// Load settings from a filesystem path. Native only (used by tests).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from(path: &Path) -> Self {
        match storage::read_settings_from(path) {
            Ok(Some(text)) => Self::from_json(&text),
            Ok(None) | Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize settings: {e}"))?;
        storage::write_settings_text(&json)
    }

    /// Write settings to a filesystem path. Native only (used by tests).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize settings: {e}"))?;
        storage::write_settings_to(path, &json)
    }

    pub fn sanitize(&mut self) {
        self.version = SETTINGS_VERSION;
        self.defaults.sanitize();
        self.keybindings.retain(|b| {
            (b.key.is_some() || b.named.is_some())
                && b.machine_id.is_none()
                && b.action.is_performance() == false
        });
    }

    pub fn match_binding(&self, key: &Key, modifiers: Modifiers) -> Option<&Keybinding> {
        self.keybindings.iter().find(|b| b.matches(key, modifiers))
    }

    pub fn action_for(&self, key: &Key, modifiers: Modifiers) -> Option<Action> {
        self.match_binding(key, modifiers).map(|b| b.action)
    }

    pub fn binding_for(&self, target: &BindTarget) -> Option<&Keybinding> {
        if target.machine_id.is_some() {
            return None;
        }
        self.keybindings.iter().find(|b| {
            b.action == target.action
                && b.machine_id.is_none()
                && b.preset_name == target.preset_name
        })
    }

    pub fn set_binding(&mut self, binding: Keybinding) {
        debug_assert!(binding.machine_id.is_none());
        self.keybindings
            .retain(|b| !b.same_target(&binding) && !b.same_combo(&binding));
        self.keybindings.push(binding);
    }

    pub fn clear_binding(&mut self, target: &BindTarget) {
        if target.machine_id.is_some() {
            return;
        }
        self.keybindings
            .retain(|b| b.action != target.action || b.preset_name != target.preset_name);
    }

    pub fn clear_preset(&mut self, name: &str) {
        self.keybindings
            .retain(|b| b.preset_name.as_deref() != Some(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "turing_drawing_settings_{name}_{}.json",
            std::process::id()
        ))
    }

    #[test]
    fn legacy_theme_field_migrates_to_appearance() {
        let loaded: UserSettings =
            serde_json::from_str(r#"{"version": 1, "theme": "slate"}"#).unwrap();
        assert_eq!(loaded.appearance.theme.name, "slate");
    }

    #[test]
    fn default_json_roundtrip() {
        let original = UserSettings::default();
        let json = serde_json::to_string_pretty(&original).unwrap();
        let loaded: UserSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.version, original.version);
        assert_eq!(loaded.appearance.theme.name, original.appearance.theme.name);
        assert_eq!(loaded.appearance.save_as, original.appearance.save_as);
        assert_eq!(loaded.defaults, original.defaults);
        assert_eq!(loaded.keybindings, original.keybindings);
        assert!(json.contains("\"action\": \"restart\""));
        assert!(json.contains("\"key\": \"r\""));
        assert!(json.contains("\"shift\": true"));
        assert!(json.contains("\"named\": \"f11\""));
    }

    #[test]
    fn missing_fields_use_defaults() {
        let loaded: UserSettings = serde_json::from_str(r#"{"version": 1}"#).unwrap();
        let mut loaded = loaded;
        loaded.sanitize();
        assert_eq!(loaded.appearance.theme.name, "graphite");
        assert_eq!(loaded.defaults, PanelDefaults::default());
        assert_eq!(loaded.keybindings, default_keybindings());
    }

    #[test]
    fn clamp_out_of_range_defaults() {
        let json = r##"{
            "version": 1,
            "defaults": {
                "num_states": 0,
                "num_symbols": 99,
                "map_width": 1,
                "map_height": 99999,
                "speed": 4.0,
                "refresh_hz": 0,
                "max_itrs": 10,
                "gradient_start": "nope",
                "gradient_end": "#gg0000"
            }
        }"##;
        let mut loaded: UserSettings = serde_json::from_str(json).unwrap();
        loaded.sanitize();
        assert_eq!(loaded.defaults.num_states, MIN_STATES);
        assert_eq!(loaded.defaults.num_symbols, MAX_SYMBOLS);
        assert_eq!(loaded.defaults.map_width, MIN_MAP_SIZE);
        assert_eq!(loaded.defaults.map_height, MAX_MAP_SIZE);
        assert_eq!(loaded.defaults.speed, 1.0);
        assert_eq!(loaded.defaults.refresh_hz, MIN_REFRESH_HZ);
        assert_eq!(loaded.defaults.max_itrs, MIN_MAX_ITRS);
        assert_eq!(loaded.defaults.gradient_start, default_gradient_start());
        assert_eq!(loaded.defaults.gradient_end, default_gradient_end());
    }

    #[test]
    fn shift_r_matches_restart() {
        let settings = UserSettings::default();
        let key = Key::Character("r".into());
        let shifted = Modifiers::SHIFT;
        assert_eq!(settings.action_for(&key, shifted), Some(Action::Restart));
        assert_eq!(settings.action_for(&key, Modifiers::empty()), None);
        assert_eq!(
            settings.action_for(&Key::Character("R".into()), shifted),
            Some(Action::Restart)
        );
    }

    #[test]
    fn f11_matches_toggle_fullscreen() {
        let settings = UserSettings::default();
        let key = Key::Named(key::Named::F11);
        assert_eq!(
            settings.action_for(&key, Modifiers::empty()),
            Some(Action::ToggleFullscreen)
        );
        assert_eq!(settings.action_for(&key, Modifiers::SHIFT), None);
    }

    #[test]
    fn capture_shift_r_and_f11() {
        let restart = Keybinding::from_event(
            Action::Restart,
            &Key::Character("R".into()),
            Modifiers::SHIFT,
        )
        .unwrap();
        assert_eq!(restart, Keybinding::restart_shift_r());
        assert_eq!(restart.display(), "Shift+R");

        let fullscreen = Keybinding::from_event(
            Action::ToggleFullscreen,
            &Key::Named(key::Named::F11),
            Modifiers::empty(),
        )
        .unwrap();
        assert_eq!(fullscreen, Keybinding::fullscreen_f11());
        assert_eq!(fullscreen.display(), "F11");
    }

    #[test]
    fn set_binding_moves_combo() {
        let mut settings = UserSettings::default();
        settings.set_binding(
            Keybinding::from_event(
                Action::Restart,
                &Key::Named(key::Named::F11),
                Modifiers::empty(),
            )
            .unwrap(),
        );
        assert_eq!(
            settings.binding_for(&BindTarget::global(Action::ToggleFullscreen)),
            None
        );
        assert_eq!(
            settings.action_for(&Key::Named(key::Named::F11), Modifiers::empty()),
            Some(Action::Restart)
        );
    }

    #[test]
    fn load_missing_file_uses_defaults() {
        let path = temp_path("missing");
        let _ = fs::remove_file(&path);
        assert_eq!(UserSettings::load_from(&path), UserSettings::default());
    }

    #[test]
    fn save_and_load_roundtrip_file() {
        let path = temp_path("roundtrip");
        let _ = fs::remove_file(&path);
        let mut settings = UserSettings::default();
        settings.appearance.theme = atelier_ui::themes::slate();
        settings.appearance.save_as = "slate".into();
        settings.defaults.num_states = 8;
        settings.defaults.palette_kind = PaletteKind::Ocean;
        settings.save_to(&path).unwrap();
        let loaded = UserSettings::load_from(&path);
        let _ = fs::remove_file(&path);
        assert_eq!(loaded.appearance.theme.name, "slate");
        assert_eq!(loaded.defaults.num_states, 8);
        assert_eq!(loaded.defaults.palette_kind, PaletteKind::Ocean);
        assert_eq!(
            loaded.action_for(&Key::Character("r".into()), Modifiers::SHIFT),
            Some(Action::Restart)
        );
    }

    #[test]
    fn invalid_json_uses_defaults() {
        let path = temp_path("invalid");
        fs::write(&path, "{not json").unwrap();
        let loaded = UserSettings::load_from(&path);
        let _ = fs::remove_file(&path);
        assert_eq!(loaded, UserSettings::default());
    }

    #[test]
    fn machine_bindings_are_per_id_and_clear_on_delete() {
        let mut perf = PerformanceBindings::default();
        let a = Keybinding::from_event(
            BindTarget::machine(Action::RandomizeMachine, 1),
            &Key::Character("m".into()),
            Modifiers::empty(),
        )
        .unwrap();
        let b = Keybinding::from_event(
            BindTarget::machine(Action::RandomizeMachine, 2),
            &Key::Character("n".into()),
            Modifiers::empty(),
        )
        .unwrap();
        perf.set_binding(a);
        perf.set_binding(b);
        assert!(perf
            .binding_for(&BindTarget::machine(Action::RandomizeMachine, 1))
            .is_some());
        assert!(perf
            .binding_for(&BindTarget::machine(Action::RandomizeMachine, 2))
            .is_some());
        perf.clear_machine(1);
        assert!(perf
            .binding_for(&BindTarget::machine(Action::RandomizeMachine, 1))
            .is_none());
        assert!(perf
            .binding_for(&BindTarget::machine(Action::RandomizeMachine, 2))
            .is_some());
        perf.retain_machine_ids(&[2]);
        assert!(perf
            .binding_for(&BindTarget::machine(Action::RandomizeMachine, 2))
            .is_some());
    }

    #[test]
    fn settings_sanitize_drops_machine_bindings() {
        let mut settings = UserSettings::default();
        settings.keybindings.push(Keybinding {
            action: Action::RandomizeMachine,
            machine_id: Some(1),
            preset_name: None,
            key: Some("m".into()),
            named: None,
            shift: false,
            ctrl: false,
            alt: false,
            logo: false,
        });
        settings.sanitize();
        assert_eq!(settings.keybindings.len(), 2);
        assert!(settings
            .binding_for(&BindTarget::global(Action::Restart))
            .is_some());
    }

    #[test]
    fn performance_bindings_roundtrip_preset_index() {
        let perf = PerformanceBindings::from_preset_bindings(
            &[PresetPerformanceBinding {
                machine_index: 1,
                action: Action::MutateMachine,
                key: Some("m".into()),
                named: None,
                shift: false,
                ctrl: false,
                alt: false,
                logo: false,
            }],
            &[10, 20],
        );
        assert_eq!(
            perf.binding_for(&BindTarget::machine(Action::MutateMachine, 20))
                .map(Keybinding::display),
            Some("M".into())
        );
        let preset = perf.to_preset_bindings(&[10, 20]);
        assert_eq!(preset.len(), 1);
        assert_eq!(preset[0].machine_index, 1);
    }
}
