use std::time::Duration;

use iced::keyboard::key;
use iced::widget::image::Handle;
use iced::widget::{
    center, column, container, mouse_area, opaque, pick_list, row, scrollable, slider, stack,
    toggler, Space,
};
use iced::{
    clipboard, event, keyboard, mouse, time, window, Alignment, Element, Event, Length, Point,
    Size, Subscription, Task, Theme,
};
use web_time::Instant;

use turing_drawing::chrome;
use turing_drawing::color_picker::{self, ColorPicker, ControlsMessage, GradientEndpoint};
use turing_drawing::dirty::DirtyRect;
use turing_drawing::drawing::{self, simulation_frame};
use turing_drawing::gpu_raster::{self, RasterMode};
use turing_drawing::palette::{parse_hex_rgb, rgb_to_hex, Palette, PaletteKind};
use turing_drawing::preset::{self, PresetInfo, PresetSort};
use turing_drawing::program::{
    default_machine_name, remap_index_after_reorder, step_rate, Program, ScheduleMode,
    DEFAULT_MUTATE_PERCENT, MACHINE_SPEED_STEP, MAX_MACHINE_SPEED, MAX_MAP_SIZE, MAX_MUTATE_PERCENT,
    MAX_STATES, MAX_SYMBOLS, MIN_MACHINE_SPEED, MIN_MAP_SIZE, MIN_MUTATE_PERCENT, MIN_STATES,
    MIN_SYMBOLS,
};
use turing_drawing::settings::{
    self, Action, AtelierTheme, BindTarget, PerformanceBindings, UserSettings, DEFAULT_MAX_ITRS,
    MAX_MAX_ITRS, MAX_REFRESH_HZ, MIN_MAX_ITRS, MIN_REFRESH_HZ,
};
use turing_drawing::tape::{
    TapeInit, TapeInitKind, DEFAULT_GAUSSIAN_MEAN, DEFAULT_GAUSSIAN_SIGMA, DEFAULT_PERLIN_OCTAVES,
    DEFAULT_PERLIN_SCALE, MAX_GAUSSIAN_MEAN, MAX_GAUSSIAN_SIGMA, MAX_PERLIN_OCTAVES,
    MAX_PERLIN_SCALE, MIN_GAUSSIAN_MEAN, MIN_GAUSSIAN_SIGMA, MIN_PERLIN_OCTAVES, MIN_PERLIN_SCALE,
};

const REFRESH_PRESETS: [RefreshPreset; 6] = [
    RefreshPreset(30),
    RefreshPreset(60),
    RefreshPreset(120),
    RefreshPreset(144),
    RefreshPreset(165),
    RefreshPreset(240),
];
const CHUNK: usize = 5_000;
/// Resume this long after the last resize during a fullscreen transition.
const MODE_SWITCH_SETTLE: Duration = Duration::from_millis(100);
/// Fallback cap if no resize events arrive during a mode switch.
const MODE_SWITCH_PAUSE_MAX: Duration = Duration::from_millis(800);
const WINDOW_DEFAULT: Size = Size::new(1100.0, 720.0);
const WINDOW_MIN: Size = Size::new(900.0, 560.0);
const SETTINGS_OVERLAY_DEFAULT: Size = Size::new(560.0, 490.0);
const SETTINGS_OVERLAY_MIN: Size = Size::new(480.0, 280.0);
const PRESET_OVERLAY_DEFAULT: Size = Size::new(520.0, 460.0);
const PRESET_OVERLAY_MIN: Size = Size::new(400.0, 240.0);
const OVERLAY_WINDOW_MARGIN: f32 = 40.0;
const SIM_SPEED_MIN: f32 = 0.0;
const SIM_SPEED_MAX: f32 = 1.0;
const SIM_SPEED_STEP: f32 = 0.001;
const RESOLUTION_PRESETS: [ResolutionPreset; 18] = [
    ResolutionPreset::square(512, "512 × 512"),
    ResolutionPreset::square(1024, "1024 × 1024"),
    ResolutionPreset::square(2048, "2048 × 2048"),
    ResolutionPreset::new(1280, 720, "1280 × 720 (16:9)"),
    ResolutionPreset::new(1920, 1080, "1920 × 1080 (16:9)"),
    ResolutionPreset::new(2560, 1440, "2560 × 1440 (16:9)"),
    ResolutionPreset::new(3840, 2160, "3840 × 2160 (16:9)"),
    ResolutionPreset::new(1280, 800, "1280 × 800 (16:10)"),
    ResolutionPreset::new(1440, 900, "1440 × 900 (16:10)"),
    ResolutionPreset::new(1920, 1200, "1920 × 1200 (16:10)"),
    ResolutionPreset::new(2560, 1600, "2560 × 1600 (16:10)"),
    ResolutionPreset::new(1470, 956, "1470 × 956 (13\" MacBook Air)"),
    ResolutionPreset::new(1512, 982, "1512 × 982 (14\" MacBook Pro)"),
    ResolutionPreset::new(1728, 1117, "1728 × 1117 (16\" MacBook Pro)"),
    ResolutionPreset::new(2560, 1664, "2560 × 1664 (13\" Air native)"),
    ResolutionPreset::new(2880, 1864, "2880 × 1864 (15\" Air native)"),
    ResolutionPreset::new(2560, 1080, "2560 × 1080 (21:9)"),
    ResolutionPreset::new(3440, 1440, "3440 × 1440 (21:9)"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RefreshPreset(u32);

impl std::fmt::Display for RefreshPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} Hz", self.0)
    }
}

fn frame_period(hz: u32) -> Duration {
    let hz = hz.clamp(MIN_REFRESH_HZ, MAX_REFRESH_HZ);
    Duration::from_secs_f64(1.0 / f64::from(hz))
}

fn clamp_overlay_size(size: Size, min: Size, window: Size) -> Size {
    Size::new(
        size.width.clamp(
            min.width,
            (window.width - OVERLAY_WINDOW_MARGIN).max(min.width),
        ),
        size.height.clamp(
            min.height,
            (window.height - OVERLAY_WINDOW_MARGIN).max(min.height),
        ),
    )
}

fn parse_refresh_hz(text: &str) -> Result<u32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Refresh rate is empty".into());
    }
    match trimmed.parse::<u32>() {
        Ok(hz) => Ok(hz.clamp(MIN_REFRESH_HZ, MAX_REFRESH_HZ)),
        Err(_) => Err(format!(
            "Refresh rate must be an integer {MIN_REFRESH_HZ}..={MAX_REFRESH_HZ}"
        )),
    }
}

fn parse_clamped_f32(text: &str, min: f32, max: f32) -> Result<f32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Value is empty".into());
    }
    match trimmed.parse::<f32>() {
        Ok(v) if v.is_finite() => Ok(v.clamp(min, max)),
        _ => Err("Value must be a finite number".into()),
    }
}

fn format_sim_speed(speed: f32) -> String {
    format!("{speed:.3}")
}

fn format_machine_speed(speed: f32) -> String {
    if speed.abs() < 1e-4 {
        "0.00".into()
    } else {
        format!("{speed:+.2}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ResolutionPreset {
    width: usize,
    height: usize,
    label: &'static str,
}

impl ResolutionPreset {
    const fn new(width: usize, height: usize, label: &'static str) -> Self {
        Self {
            width,
            height,
            label,
        }
    }

    const fn square(size: usize, label: &'static str) -> Self {
        Self::new(size, size, label)
    }
}

impl std::fmt::Display for ResolutionPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

fn parse_map_dim(text: &str, name: &str) -> Result<usize, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(format!("{name} is empty"));
    }
    match trimmed.parse::<usize>() {
        Ok(n) if (MIN_MAP_SIZE..=MAX_MAP_SIZE).contains(&n) => Ok(n),
        Ok(_) | Err(_) => Err(format!(
            "{name} must be an integer {MIN_MAP_SIZE}..={MAX_MAP_SIZE}"
        )),
    }
}

fn main() -> iced::Result {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    iced::application(App::new, App::update, App::view)
        .title("Turing Drawings")
        .theme(theme)
        .subscription(App::subscription)
        .window(window::Settings {
            size: WINDOW_DEFAULT,
            min_size: Some(WINDOW_MIN),
            ..Default::default()
        })
        .run()
}

#[cfg(target_arch = "wasm32")]
fn location_hash() -> Option<String> {
    let window = web_sys::window()?;
    let hash = window.location().hash().ok()?;
    let trimmed = hash.trim();
    if trimmed.is_empty() || trimmed == "#" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn theme(_app: &App) -> Theme {
    if atelier_ui::theme().name == "paper" {
        Theme::Light
    } else {
        Theme::Dark
    }
}

fn on_event(event: Event, status: event::Status, _id: window::Id) -> Option<Message> {
    if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
        return Some(Message::PointerReleased);
    }
    if status == event::Status::Captured {
        return None;
    }
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            repeat,
            ..
        }) => {
            if repeat {
                return None;
            }
            match &key {
                keyboard::Key::Named(key::Named::Escape) => Some(Message::Escape),
                _ => Some(Message::KeyPressed { key, modifiers }),
            }
        }
        _ => None,
    }
}

struct App {
    program: Program,
    num_symbols: usize,
    /// Fraction of `max_itrs` to run each tick (`0.0` = paused, `1.0` = max).
    speed: f32,
    /// Text field for the global speed slider.
    speed_text: String,
    /// Target simulation ticks per second.
    refresh_hz: u32,
    /// Text field for a custom refresh-rate value.
    refresh_hz_text: String,
    /// Text field for a custom canvas width.
    map_width_text: String,
    /// Text field for a custom canvas height.
    map_height_text: String,
    /// Max simulation rounds to run each tick at speed `1.0`.
    max_itrs: u64,
    share_texts: Vec<String>,
    /// Draft text for each machine's speed field (parallel to `share_texts`).
    machine_speed_texts: Vec<String>,
    status: String,
    /// Cached RGBA frame; rebuilt when the map changes.
    pixels: Vec<u8>,
    /// Pending region to upload on the GPU raster path.
    gpu_dirty: Option<DirtyRect>,
    /// Revision paired with `gpu_dirty`.
    gpu_dirty_revision: u64,
    frame: Handle,
    /// When true, only the drawing is shown (controls and share encodings hidden).
    drawing_only: bool,
    color_picker: ColorPicker,
    /// Gradient picker for the selected machine's palette.
    machine_color_picker: ColorPicker,
    /// Machine whose inspector section is open.
    selected_machine: Option<usize>,
    /// Machine currently being dragged in the left pool.
    dragging_machine: Option<usize>,
    /// Whether the preset browser overlay is open.
    preset_browser_open: bool,
    /// Session size of the settings overlay.
    settings_overlay_size: Size,
    /// Session size of the preset browser overlay.
    preset_overlay_size: Size,
    /// Latest OS window size, used to clamp overlay dialogs.
    window_size: Size,
    /// In-progress overlay resize drag, if any.
    overlay_resize: Option<OverlayResize>,
    /// Name field for storing a new/overwrite preset.
    preset_name: String,
    /// Cached list of presets on disk (refreshed when the browser opens or changes).
    preset_list: Vec<PresetInfo>,
    /// Order of the preset browser list.
    preset_sort: PresetSort,
    /// Whether the inline speed-snapshot panel is open.
    speed_snapshot_open: bool,
    /// Draft name for storing a speed snapshot.
    speed_snapshot_name: String,
    canvas_open: bool,
    palette_open: bool,
    simulation_open: bool,
    /// When set, simulation stays paused until this instant.
    mode_switch_resume_at: Option<Instant>,
    /// GPU colorize (default) or the CPU RGBA fallback.
    raster_mode: RasterMode,
    /// Shared mutate slider (`MIN_MUTATE_PERCENT`..=`MAX_MUTATE_PERCENT`).
    mutate_percent: f32,
    /// Machine whose start position will be set by the next canvas click.
    picking_start: Option<usize>,
    /// Swallow the double-click that follows a start-position pick.
    suppress_drawing_only: bool,
    settings: UserSettings,
    performance_bindings: PerformanceBindings,
    settings_open: bool,
    settings_tab: SettingsTab,
    capturing_action: Option<BindTarget>,
    button_controls: Option<BindTarget>,
    settings_width_text: String,
    settings_height_text: String,
    settings_refresh_text: String,
    settings_speed_text: String,
    settings_gradient_start: String,
    settings_gradient_end: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectorGroup {
    Canvas,
    Palette,
    Simulation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Defaults,
    Keybindings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayKind {
    Settings,
    Presets,
}

#[derive(Debug, Clone, Copy)]
struct OverlayResize {
    kind: OverlayKind,
    last_cursor: Option<Point>,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    IncStates,
    DecStates,
    IncSymbols,
    DecSymbols,
    SpeedChanged(f32),
    SpeedText(String),
    RefreshPreset(u32),
    RefreshHzText(String),
    ResolutionPreset(usize, usize),
    MapWidthText(String),
    MapHeightText(String),
    TapeKind(TapeInitKind),
    TapeGaussianMean(f32),
    TapeGaussianSigma(f32),
    TapePerlinScale(f32),
    TapePerlinOctaves(f32),
    ReseedTape,
    MaxItrsChanged(f32),
    RasterMode(RasterMode),
    Random,
    Mutate,
    MutateMachine(usize),
    MutatePercent(f32),
    Restart,
    AddMachine,
    RemoveMachine(usize),
    RandomizeMachine(usize),
    SelectMachine(usize),
    CloseMachineDetails,
    MachineNameChanged(usize, String),
    MachineDragStart(usize),
    MachineDragOver(usize),
    PointerReleased,
    OverlayResizeStart(OverlayKind),
    OverlayResizeMove(Point),
    OverlayResizeEnd,
    MachineSpeedChanged(usize, f32),
    MachineSpeedText(usize, String),
    ToggleMachineActive(usize, bool),
    ScheduleModeToggled(bool),
    AllowDiagonalsToggled(bool),
    TogglePickStart(usize),
    CanvasClicked {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    ShareChanged(usize, String),
    CopyShare(usize),
    LoadShare(usize),
    ToggleDrawingOnly,
    ToggleFullscreen,
    PauseForModeSwitch,
    WindowResized(Size),
    Escape,
    PaletteSelected(PaletteKind),
    GradientStartChanged(String),
    GradientEndChanged(String),
    ColorPicker(color_picker::Message),
    MachinePaletteSelected(usize, PaletteKind),
    MachineGradientStartChanged(usize, String),
    MachineGradientEndChanged(usize, String),
    MachineColorPicker(usize, color_picker::Message),
    OpenPresetBrowser,
    ClosePresetBrowser,
    PresetNameChanged(String),
    StorePreset,
    LoadPreset(String),
    DeletePreset(String),
    PresetSortChanged(PresetSort),
    ToggleInspectorGroup(InspectorGroup),
    KeyPressed {
        key: keyboard::Key,
        modifiers: keyboard::Modifiers,
    },
    OpenSettings,
    CloseSettings,
    SettingsTab(SettingsTab),
    CaptureBinding(BindTarget),
    OpenButtonControls(BindTarget),
    CloseButtonControls,
    ClearBinding,
    SettingsTheme(AtelierTheme),
    SettingsIncStates,
    SettingsDecStates,
    SettingsIncSymbols,
    SettingsDecSymbols,
    SettingsResolutionPreset(usize, usize),
    SettingsWidthText(String),
    SettingsHeightText(String),
    SettingsSpeed(f32),
    SettingsSpeedText(String),
    SettingsRefreshPreset(u32),
    SettingsRefreshText(String),
    SettingsMaxItrs(f32),
    SettingsRasterMode(RasterMode),
    SettingsPalette(PaletteKind),
    SettingsGradientStart(String),
    SettingsGradientEnd(String),
    SettingsTapeKind(TapeInitKind),
    SettingsGaussianMean(f32),
    SettingsGaussianSigma(f32),
    SettingsPerlinScale(f32),
    SettingsPerlinOctaves(f32),
    SettingsAllowDiagonals(bool),
    ToggleSpeedSnapshots,
    SpeedSnapshotNameChanged(String),
    StoreSpeedSnapshot,
    RecallSpeedSnapshot(String),
    DeleteSpeedSnapshot(String),
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let settings = UserSettings::load();
        settings.theme.activate();
        let defaults = settings.defaults.clone();
        let num_states = defaults.num_states;
        let num_symbols = defaults.num_symbols;
        let mut program = Program::new_random_sized_with_diagonals(
            num_states,
            num_symbols,
            defaults.map_width,
            defaults.map_height,
            defaults.allow_diagonals,
        );
        let mut palette = Palette::classic();
        let _ = palette.set_gradient_start_hex(defaults.gradient_start.clone(), num_symbols);
        let _ = palette.set_gradient_end_hex(defaults.gradient_end.clone(), num_symbols);
        palette.set_kind(defaults.palette_kind, num_symbols);
        program.canvas_palette = palette.clone();
        for machine in &mut program.machines {
            machine.palette = palette.clone();
        }
        program.set_schedule_mode(defaults.schedule_mode);
        program.set_tape_init(TapeInit::from_kind_and_params(
            defaults.tape_kind,
            defaults.gaussian_mean,
            defaults.gaussian_sigma,
            defaults.perlin_scale,
            defaults.perlin_octaves,
        ));
        #[cfg(target_arch = "wasm32")]
        let status = if let Some(hash) = location_hash() {
            match Program::from_string(&hash) {
                Ok(mut from_hash) => {
                    from_hash.canvas_palette = palette.clone();
                    for machine in &mut from_hash.machines {
                        machine.palette = palette.clone();
                    }
                    from_hash.set_schedule_mode(defaults.schedule_mode);
                    program = from_hash;
                    "Loaded encoding from URL hash".into()
                }
                Err(e) => format!("Could not load URL hash: {e}"),
            }
        } else {
            String::new()
        };
        #[cfg(not(target_arch = "wasm32"))]
        let status = String::new();
        let num_symbols = program.num_symbols;
        let map_width = program.width;
        let map_height = program.height;
        let share_texts = (0..program.machines.len())
            .map(|i| program.machine_encoding(i))
            .collect();
        let machine_speed_texts = program
            .machines
            .iter()
            .map(|m| format_machine_speed(m.speed))
            .collect();
        let pixels = program.canvas.clone();
        let frame = Handle::from_rgba(program.width as u32, program.height as u32, pixels.clone());
        let init_dirty = DirtyRect::full(program.width as u32, program.height as u32);
        let init_revision = program.canvas_revision;

        (
            Self {
                program,
                num_symbols,
                speed: defaults.speed,
                speed_text: format_sim_speed(defaults.speed),
                refresh_hz: defaults.refresh_hz,
                refresh_hz_text: defaults.refresh_hz.to_string(),
                map_width_text: map_width.to_string(),
                map_height_text: map_height.to_string(),
                max_itrs: defaults.max_itrs,
                share_texts,
                machine_speed_texts,
                status,
                pixels,
                gpu_dirty: Some(init_dirty),
                gpu_dirty_revision: init_revision,
                frame,
                drawing_only: false,
                color_picker: ColorPicker::new(),
                machine_color_picker: ColorPicker::new(),
                selected_machine: None,
                dragging_machine: None,
                preset_browser_open: false,
                settings_overlay_size: SETTINGS_OVERLAY_DEFAULT,
                preset_overlay_size: PRESET_OVERLAY_DEFAULT,
                window_size: WINDOW_DEFAULT,
                overlay_resize: None,
                preset_name: String::new(),
                preset_list: Vec::new(),
                preset_sort: PresetSort::DateSaved,
                speed_snapshot_open: false,
                speed_snapshot_name: String::new(),
                canvas_open: true,
                palette_open: true,
                simulation_open: true,
                mode_switch_resume_at: None,
                raster_mode: defaults.raster_mode,
                mutate_percent: f32::from(DEFAULT_MUTATE_PERCENT),
                picking_start: None,
                suppress_drawing_only: false,
                settings,
                performance_bindings: PerformanceBindings::default(),
                settings_open: false,
                settings_tab: SettingsTab::Defaults,
                capturing_action: None,
                button_controls: None,
                settings_width_text: defaults.map_width.to_string(),
                settings_height_text: defaults.map_height.to_string(),
                settings_refresh_text: defaults.refresh_hz.to_string(),
                settings_speed_text: format_sim_speed(defaults.speed),
                settings_gradient_start: defaults.gradient_start,
                settings_gradient_end: defaults.gradient_end,
            },
            Task::none(),
        )
    }

    fn pause_for_mode_switch(&mut self) {
        self.mode_switch_resume_at = Some(Instant::now() + MODE_SWITCH_PAUSE_MAX);
    }

    fn note_mode_switch_resize(&mut self) {
        if self.mode_switch_resume_at.is_some() {
            self.mode_switch_resume_at = Some(Instant::now() + MODE_SWITCH_SETTLE);
        }
    }

    fn mode_switch_paused(&self) -> bool {
        self.mode_switch_resume_at
            .is_some_and(|resume_at| Instant::now() < resume_at)
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = if self.mode_switch_paused() {
            time::every(MODE_SWITCH_SETTLE).map(|_| Message::Tick)
        } else {
            time::every(frame_period(self.refresh_hz)).map(|_| Message::Tick)
        };

        Subscription::batch([
            tick,
            event::listen_with(on_event),
            window::resize_events().map(|(_id, size)| Message::WindowResized(size)),
        ])
    }

    fn sync_share_texts(&mut self) {
        self.share_texts = (0..self.program.machines.len())
            .map(|i| self.program.machine_encoding(i))
            .collect();
        self.sync_machine_speed_texts();
    }

    fn sync_machine_speed_texts(&mut self) {
        self.machine_speed_texts = self
            .program
            .machines
            .iter()
            .map(|m| format_machine_speed(m.speed))
            .collect();
    }

    fn mutate_percent_u8(&self) -> u8 {
        self.mutate_percent
            .round()
            .clamp(f32::from(MIN_MUTATE_PERCENT), f32::from(MAX_MUTATE_PERCENT)) as u8
    }

    fn refresh_preset_list(&mut self) {
        match preset::list_presets() {
            Ok(mut list) => {
                preset::sort_preset_infos(&mut list, self.preset_sort);
                self.preset_list = list;
            }
            Err(e) => {
                self.preset_list.clear();
                self.status = format!("Could not list presets: {e}");
            }
        }
    }

    fn persist_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.status = format!("Could not save settings: {e}");
        }
    }

    fn overlay_size(&self, kind: OverlayKind) -> Size {
        match kind {
            OverlayKind::Settings => self.settings_overlay_size,
            OverlayKind::Presets => self.preset_overlay_size,
        }
    }

    fn set_overlay_size(&mut self, kind: OverlayKind, size: Size) {
        let min = match kind {
            OverlayKind::Settings => SETTINGS_OVERLAY_MIN,
            OverlayKind::Presets => PRESET_OVERLAY_MIN,
        };
        let clamped = clamp_overlay_size(size, min, self.window_size);
        match kind {
            OverlayKind::Settings => self.settings_overlay_size = clamped,
            OverlayKind::Presets => self.preset_overlay_size = clamped,
        }
    }

    fn end_overlay_resize(&mut self) {
        self.overlay_resize = None;
    }

    fn forget_machine_bindings(&mut self, id: u64) {
        self.performance_bindings.clear_machine(id);
        if self
            .button_controls
            .as_ref()
            .is_some_and(|t| t.machine_id == Some(id))
        {
            self.button_controls = None;
            self.capturing_action = None;
        }
    }

    fn prune_machine_bindings(&mut self) {
        self.performance_bindings
            .retain_machine_ids(&self.program.machine_ids());
        if let Some(target) = &self.button_controls {
            if let Some(id) = target.machine_id {
                if self.program.index_of_machine(id).is_none() {
                    self.button_controls = None;
                    self.capturing_action = None;
                }
            }
        }
    }

    fn binding_for(&self, target: &BindTarget) -> Option<&settings::Keybinding> {
        if target.is_performance() {
            self.performance_bindings.binding_for(target)
        } else {
            self.settings.binding_for(target)
        }
    }

    fn set_binding(&mut self, binding: settings::Keybinding) {
        if binding.machine_id.is_some() {
            self.performance_bindings.set_binding(binding);
        } else {
            self.settings.set_binding(binding);
            self.persist_settings();
        }
    }

    fn clear_binding(&mut self, target: &BindTarget) {
        if target.is_performance() {
            self.performance_bindings.clear_binding(target);
        } else {
            self.settings.clear_binding(target);
            self.persist_settings();
        }
    }

    fn match_any_binding(
        &self,
        key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> Option<&settings::Keybinding> {
        self.performance_bindings
            .match_binding(key, modifiers)
            .or_else(|| self.settings.match_binding(key, modifiers))
    }

    fn target_label(&self, target: &BindTarget) -> String {
        let machine_name = target.machine_id.and_then(|id| {
            self.program
                .index_of_machine(id)
                .map(|i| self.program.machines[i].display_name(i))
        });
        target.label(machine_name.as_deref())
    }

    fn dispatch_binding(&mut self, binding: &settings::Keybinding) -> Task<Message> {
        if let Some(id) = binding.machine_id {
            let Some(index) = self.program.index_of_machine(id) else {
                return Task::none();
            };
            return match binding.action {
                Action::RandomizeMachine => self.update(Message::RandomizeMachine(index)),
                Action::MutateMachine => self.update(Message::MutateMachine(index)),
                Action::TogglePickStart => self.update(Message::TogglePickStart(index)),
                Action::CopyShare => self.update(Message::CopyShare(index)),
                Action::LoadShare => self.update(Message::LoadShare(index)),
                Action::RemoveMachine => self.update(Message::RemoveMachine(index)),
                _ => Task::none(),
            };
        }
        if let Some(name) = &binding.preset_name {
            return match binding.action {
                Action::LoadPreset => self.update(Message::LoadPreset(name.clone())),
                Action::DeletePreset => self.update(Message::DeletePreset(name.clone())),
                _ => Task::none(),
            };
        }
        match binding.action {
            Action::Random => self.update(Message::Random),
            Action::Mutate => self.update(Message::Mutate),
            Action::Restart => self.update(Message::Restart),
            Action::ReseedTape => self.update(Message::ReseedTape),
            Action::OpenPresets => self.update(Message::OpenPresetBrowser),
            Action::OpenSettings => self.update(Message::OpenSettings),
            Action::ToggleFullscreen => self.update(Message::ToggleFullscreen),
            Action::AddMachine => self.update(Message::AddMachine),
            Action::IncStates => self.update(Message::IncStates),
            Action::DecStates => self.update(Message::DecStates),
            Action::IncSymbols => self.update(Message::IncSymbols),
            Action::DecSymbols => self.update(Message::DecSymbols),
            Action::ToggleCanvas => {
                self.update(Message::ToggleInspectorGroup(InspectorGroup::Canvas))
            }
            Action::TogglePalette => {
                self.update(Message::ToggleInspectorGroup(InspectorGroup::Palette))
            }
            Action::ToggleSimulation => {
                self.update(Message::ToggleInspectorGroup(InspectorGroup::Simulation))
            }
            Action::CloseMachineDetails => self.update(Message::CloseMachineDetails),
            Action::StorePreset => self.update(Message::StorePreset),
            Action::ClosePresetBrowser => self.update(Message::ClosePresetBrowser),
            Action::CloseSettings => self.update(Message::CloseSettings),
            Action::CloseColorPicker => {
                self.color_picker.close();
                self.machine_color_picker.close();
                Task::none()
            }
            Action::SettingsTabDefaults => self.update(Message::SettingsTab(SettingsTab::Defaults)),
            Action::SettingsTabKeybindings => {
                self.update(Message::SettingsTab(SettingsTab::Keybindings))
            }
            _ => Task::none(),
        }
    }

    fn sync_settings_drafts(&mut self) {
        let d = &self.settings.defaults;
        self.settings_width_text = d.map_width.to_string();
        self.settings_height_text = d.map_height.to_string();
        self.settings_refresh_text = d.refresh_hz.to_string();
        self.settings_speed_text = format_sim_speed(d.speed);
        self.settings_gradient_start = d.gradient_start.clone();
        self.settings_gradient_end = d.gradient_end.clone();
    }

    fn try_apply_settings_resolution(&mut self) {
        match (
            parse_map_dim(&self.settings_width_text, "Width"),
            parse_map_dim(&self.settings_height_text, "Height"),
        ) {
            (Ok(width), Ok(height)) => {
                self.settings.defaults.map_width = width;
                self.settings.defaults.map_height = height;
                self.persist_settings();
                self.status.clear();
            }
            (Err(e), _) | (_, Err(e)) => {
                self.status = e;
            }
        }
    }

    fn apply_loaded_program(&mut self, mut program: Program, label: &str) {
        let schedule_mode = self.program.schedule_mode;
        program.canvas_palette = self.program.canvas_palette.clone();
        program.canvas_palette.resolve(program.num_symbols);
        program.schedule_mode = schedule_mode;
        program.reset();
        self.program = program;
        self.num_symbols = self.program.num_symbols;
        self.sync_resolution_text();
        self.selected_machine = None;
        self.dragging_machine = None;
        self.picking_start = None;
        self.machine_color_picker.close();
        self.sync_share_texts();
        self.status = label.to_string();
        self.prune_machine_bindings();
        self.refresh_frame();
    }

    fn toggle_fullscreen() -> Task<Message> {
        window::latest().and_then(|id| {
            window::mode(id).then(move |mode| {
                let next = if mode == window::Mode::Fullscreen {
                    window::Mode::Windowed
                } else {
                    window::Mode::Fullscreen
                };
                Task::batch([
                    Task::done(Message::PauseForModeSwitch),
                    window::set_mode(id, next),
                ])
            })
        })
    }

    fn exit_fullscreen_if_needed() -> Task<Message> {
        window::latest().and_then(|id| {
            window::mode(id).then(move |mode| {
                if mode == window::Mode::Fullscreen {
                    Task::batch([
                        Task::done(Message::PauseForModeSwitch),
                        window::set_mode(id, window::Mode::Windowed),
                    ])
                } else {
                    Task::none()
                }
            })
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle_message(message);
        if self.program.sync_speed_snapshots() {
            self.status = "Snapshots cleared".into();
        }
        task
    }

    fn handle_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.run_frame();
                Task::none()
            }
            Message::IncStates => {
                let index = self.selected_machine.unwrap_or(0);
                let Some(current) = self.program.machines.get(index).map(|m| m.num_states) else {
                    return Task::none();
                };
                let next = current.saturating_add(1).min(MAX_STATES);
                if next != current {
                    match self.program.set_machine_num_states(index, next) {
                        Ok(()) => {
                            self.sync_share_texts();
                            let name = self.program.machines[index].display_name(index);
                            self.status = format!("{name}: {next} states; program reset");
                            self.refresh_frame();
                        }
                        Err(e) => {
                            self.status = e;
                        }
                    }
                }
                Task::none()
            }
            Message::DecStates => {
                let index = self.selected_machine.unwrap_or(0);
                let Some(current) = self.program.machines.get(index).map(|m| m.num_states) else {
                    return Task::none();
                };
                let next = current.saturating_sub(1).max(MIN_STATES);
                if next != current {
                    match self.program.set_machine_num_states(index, next) {
                        Ok(()) => {
                            self.sync_share_texts();
                            let name = self.program.machines[index].display_name(index);
                            self.status = format!("{name}: {next} states; program reset");
                            self.refresh_frame();
                        }
                        Err(e) => {
                            self.status = e;
                        }
                    }
                }
                Task::none()
            }
            Message::IncSymbols => {
                if self.num_symbols < MAX_SYMBOLS {
                    self.num_symbols += 1;
                    self.program.canvas_palette.resolve(self.num_symbols);
                }
                Task::none()
            }
            Message::DecSymbols => {
                if self.num_symbols > MIN_SYMBOLS {
                    self.num_symbols -= 1;
                    self.program.canvas_palette.resolve(self.num_symbols);
                }
                Task::none()
            }
            Message::SpeedChanged(speed) => {
                self.speed = speed.clamp(SIM_SPEED_MIN, SIM_SPEED_MAX);
                self.speed_text = format_sim_speed(self.speed);
                Task::none()
            }
            Message::SpeedText(text) => {
                self.speed_text = text;
                if let Ok(speed) = parse_clamped_f32(&self.speed_text, SIM_SPEED_MIN, SIM_SPEED_MAX)
                {
                    self.speed = speed;
                }
                Task::none()
            }
            Message::RefreshPreset(hz) => {
                let hz = hz.clamp(MIN_REFRESH_HZ, MAX_REFRESH_HZ);
                self.refresh_hz = hz;
                self.refresh_hz_text = hz.to_string();
                self.status.clear();
                Task::none()
            }
            Message::RefreshHzText(text) => {
                self.refresh_hz_text = text;
                match parse_refresh_hz(&self.refresh_hz_text) {
                    Ok(hz) => {
                        self.refresh_hz = hz;
                        self.status.clear();
                    }
                    Err(e) => {
                        self.status = e;
                    }
                }
                Task::none()
            }
            Message::ResolutionPreset(width, height) => {
                self.apply_resolution(width, height);
                Task::none()
            }
            Message::MapWidthText(text) => {
                self.map_width_text = text;
                self.try_apply_custom_resolution();
                Task::none()
            }
            Message::MapHeightText(text) => {
                self.map_height_text = text;
                self.try_apply_custom_resolution();
                Task::none()
            }
            Message::TapeKind(kind) => {
                if self.program.tape_init.kind != kind {
                    self.program.tape_init.kind = kind;
                    self.program.reset();
                    self.status = format!("Tape: {kind}; drawing reset");
                    self.refresh_frame();
                }
                Task::none()
            }
            Message::TapeGaussianMean(value) => {
                self.program.tape_init.gaussian_mean = value;
                self.program.tape_init.sanitize();
                self.program.reset();
                self.refresh_frame();
                Task::none()
            }
            Message::TapeGaussianSigma(value) => {
                self.program.tape_init.gaussian_sigma = value;
                self.program.tape_init.sanitize();
                self.program.reset();
                self.refresh_frame();
                Task::none()
            }
            Message::TapePerlinScale(value) => {
                self.program.tape_init.perlin_scale = value;
                self.program.tape_init.sanitize();
                self.program.reset();
                self.refresh_frame();
                Task::none()
            }
            Message::TapePerlinOctaves(value) => {
                self.program.tape_init.perlin_octaves = value.round() as u8;
                self.program.tape_init.sanitize();
                self.program.reset();
                self.refresh_frame();
                Task::none()
            }
            Message::ReseedTape => {
                self.program.reseed_tape();
                self.status = "Tape reseeded; drawing reset".into();
                self.refresh_frame();
                Task::none()
            }
            Message::MaxItrsChanged(value) => {
                let value = value.round() as u64;
                self.max_itrs = value.clamp(MIN_MAX_ITRS, MAX_MAX_ITRS);
                Task::none()
            }
            Message::RasterMode(mode) => {
                self.set_raster_mode(mode);
                Task::none()
            }
            Message::Random => {
                self.program.randomize(self.num_symbols);
                self.sync_share_texts();
                self.status = format!(
                    "New machines: {} machine(s), {} symbols",
                    self.program.machines.len(),
                    self.num_symbols
                );
                self.refresh_frame();
                Task::none()
            }
            Message::Mutate => {
                let percent = self.mutate_percent_u8();
                let n = self.program.machines.len();
                self.program.mutate_all(percent);
                self.sync_share_texts();
                self.status = format!("Mutated {n} machine(s) at {percent}%");
                self.refresh_frame();
                Task::none()
            }
            Message::MutatePercent(value) => {
                self.mutate_percent =
                    value.clamp(f32::from(MIN_MUTATE_PERCENT), f32::from(MAX_MUTATE_PERCENT));
                Task::none()
            }
            Message::Restart => {
                self.program.reset();
                self.status = "Restarted".into();
                self.refresh_frame();
                Task::none()
            }
            Message::AddMachine => {
                let num_states = self.settings.defaults.num_states;
                self.program.add_machine(num_states);
                self.sync_share_texts();
                self.status = format!(
                    "Added machine with {num_states} states ({} total); program reset",
                    self.program.machines.len()
                );
                self.refresh_frame();
                Task::none()
            }
            Message::RemoveMachine(i) => {
                let id = self.program.machines.get(i).map(|m| m.id);
                match self.program.remove_machine(i) {
                    Ok(()) => {
                        if let Some(id) = id {
                            self.forget_machine_bindings(id);
                        }
                        self.adjust_selection_after_remove(i);
                        self.sync_share_texts();
                        self.status = format!(
                            "Removed machine ({} remaining); program reset",
                            self.program.machines.len()
                        );
                        self.refresh_frame();
                        Task::none()
                    }
                    Err(e) => {
                        self.status = e;
                        Task::none()
                    }
                }
            }
            Message::RandomizeMachine(i) => match self.program.randomize_machine(i) {
                Ok(()) => {
                    self.sync_share_texts();
                    let name = self.program.machines[i].display_name(i);
                    self.status = format!("Randomised {name}; program reset");
                    self.refresh_frame();
                    Task::none()
                }
                Err(e) => {
                    self.status = e;
                    Task::none()
                }
            },
            Message::MutateMachine(i) => {
                match self.program.mutate_machine(i, self.mutate_percent_u8()) {
                    Ok(()) => {
                        self.sync_share_texts();
                        let name = self.program.machines[i].display_name(i);
                        let percent = self.mutate_percent_u8();
                        self.status = format!("Mutated {name} at {percent}%");
                        self.refresh_frame();
                        Task::none()
                    }
                    Err(e) => {
                        self.status = e;
                        Task::none()
                    }
                }
            }
            Message::TogglePickStart(i) => {
                if i >= self.program.machines.len() {
                    self.status = "invalid machine index".into();
                    return Task::none();
                }
                self.picking_start = if self.picking_start == Some(i) {
                    None
                } else {
                    Some(i)
                };
                self.status = if let Some(index) = self.picking_start {
                    let name = self.program.machines[index].display_name(index);
                    format!("Click the canvas to set start for {name}")
                } else {
                    "Cancelled start placement".into()
                };
                Task::none()
            }
            Message::CanvasClicked {
                x,
                y,
                width,
                height,
            } => {
                let Some(index) = self.picking_start else {
                    return Task::none();
                };
                let Some((cx, cy)) = gpu_raster::map_cell_at(
                    self.program.width,
                    self.program.height,
                    width,
                    height,
                    x,
                    y,
                ) else {
                    return Task::none();
                };
                match self.program.set_machine_start(index, cx, cy) {
                    Ok(()) => {
                        let name = self.program.machines[index].display_name(index);
                        self.picking_start = None;
                        self.suppress_drawing_only = true;
                        self.sync_share_texts();
                        self.status =
                            format!("Start for {name} set to ({cx}, {cy}); program reset");
                        self.refresh_frame();
                    }
                    Err(e) => {
                        self.status = e;
                    }
                }
                Task::none()
            }
            Message::SelectMachine(i) => {
                if self.dragging_machine.is_some() {
                    return Task::none();
                }
                self.selected_machine = if self.selected_machine == Some(i) {
                    None
                } else {
                    Some(i)
                };
                self.machine_color_picker.close();
                Task::none()
            }
            Message::CloseMachineDetails => {
                self.selected_machine = None;
                self.machine_color_picker.close();
                Task::none()
            }
            Message::MachineNameChanged(i, name) => match self.program.set_machine_name(i, name) {
                Ok(()) => Task::none(),
                Err(e) => {
                    self.status = e;
                    Task::none()
                }
            },
            Message::MachineDragStart(i) => {
                if self.program.machines.len() < 2 || i >= self.program.machines.len() {
                    return Task::none();
                }
                self.dragging_machine = Some(i);
                self.selected_machine = Some(i);
                Task::none()
            }
            Message::MachineDragOver(to) => {
                let Some(from) = self.dragging_machine else {
                    return Task::none();
                };
                if from == to {
                    return Task::none();
                }
                match self.apply_machine_reorder(from, to) {
                    Ok(()) => Task::none(),
                    Err(e) => {
                        self.status = e;
                        Task::none()
                    }
                }
            }
            Message::PointerReleased => {
                self.dragging_machine = None;
                self.end_overlay_resize();
                Task::none()
            }
            Message::OverlayResizeStart(kind) => {
                self.overlay_resize = Some(OverlayResize {
                    kind,
                    last_cursor: None,
                });
                Task::none()
            }
            Message::OverlayResizeMove(point) => {
                let Some(resize) = self.overlay_resize else {
                    return Task::none();
                };
                let Some(last) = resize.last_cursor else {
                    self.overlay_resize = Some(OverlayResize {
                        kind: resize.kind,
                        last_cursor: Some(point),
                    });
                    return Task::none();
                };
                let kind = resize.kind;
                self.overlay_resize = Some(OverlayResize {
                    kind,
                    last_cursor: Some(point),
                });
                let current = self.overlay_size(kind);
                self.set_overlay_size(
                    kind,
                    Size::new(
                        current.width + (point.x - last.x) * 2.0,
                        current.height + (point.y - last.y) * 2.0,
                    ),
                );
                Task::none()
            }
            Message::OverlayResizeEnd => {
                self.end_overlay_resize();
                Task::none()
            }
            Message::MachineSpeedChanged(i, speed) => {
                match self.program.set_machine_speed(i, speed) {
                    Ok(()) => {
                        if let Some(slot) = self.machine_speed_texts.get_mut(i) {
                            *slot = format_machine_speed(self.program.machines[i].speed);
                        }
                        Task::none()
                    }
                    Err(e) => {
                        self.status = e;
                        Task::none()
                    }
                }
            }
            Message::MachineSpeedText(i, text) => {
                if let Some(slot) = self.machine_speed_texts.get_mut(i) {
                    *slot = text;
                }
                if let Some(slot) = self.machine_speed_texts.get(i) {
                    if let Ok(speed) = parse_clamped_f32(slot, MIN_MACHINE_SPEED, MAX_MACHINE_SPEED)
                    {
                        if let Err(e) = self.program.set_machine_speed(i, speed) {
                            self.status = e;
                        }
                    }
                }
                Task::none()
            }
            Message::ToggleMachineActive(i, active) => {
                match self.program.set_machine_active(i, active) {
                    Ok(()) => Task::none(),
                    Err(e) => {
                        self.status = e;
                        Task::none()
                    }
                }
            }
            Message::ScheduleModeToggled(normalised) => {
                let mode = if normalised {
                    ScheduleMode::Normalised
                } else {
                    ScheduleMode::Absolute
                };
                self.program.set_schedule_mode(mode);
                self.settings.defaults.schedule_mode = mode;
                self.persist_settings();
                Task::none()
            }
            Message::AllowDiagonalsToggled(allow) => {
                self.program.set_allow_diagonals(allow);
                Task::none()
            }
            Message::ShareChanged(i, s) => {
                if let Some(slot) = self.share_texts.get_mut(i) {
                    *slot = s;
                }
                Task::none()
            }
            Message::CopyShare(i) => {
                let Some(text) = self.share_texts.get(i).cloned() else {
                    return Task::none();
                };
                let name = self.program.machines.get(i).map(|m| m.display_name(i));
                self.status = match name {
                    Some(name) => format!("Copied {name} encoding to clipboard"),
                    None => "Copied encoding to clipboard".into(),
                };
                clipboard::write(text)
            }
            Message::LoadShare(i) => {
                let Some(text) = self.share_texts.get(i).cloned() else {
                    return Task::none();
                };
                match self.program.load_machine(i, &text) {
                    Ok(()) => {
                        self.num_symbols = self.program.num_symbols;
                        self.program.canvas_palette.resolve(self.num_symbols);
                        self.sync_share_texts();
                        let name = self.program.machines[i].display_name(i);
                        self.status = format!("Loaded encoding for {name}");
                        self.refresh_frame();
                        Task::none()
                    }
                    Err(e) => {
                        self.status = format!("Load failed: {e}");
                        Task::none()
                    }
                }
            }
            Message::ToggleDrawingOnly => {
                if self.suppress_drawing_only {
                    self.suppress_drawing_only = false;
                    return Task::none();
                }
                self.drawing_only = !self.drawing_only;
                Task::none()
            }
            Message::ToggleFullscreen => {
                self.pause_for_mode_switch();
                Self::toggle_fullscreen()
            }
            Message::PauseForModeSwitch => {
                self.pause_for_mode_switch();
                Task::none()
            }
            Message::WindowResized(size) => {
                self.window_size = size;
                self.set_overlay_size(OverlayKind::Settings, self.settings_overlay_size);
                self.set_overlay_size(OverlayKind::Presets, self.preset_overlay_size);
                self.note_mode_switch_resize();
                Task::none()
            }
            Message::Escape => {
                if self.button_controls.is_some() {
                    self.button_controls = None;
                    self.capturing_action = None;
                    return Task::none();
                }
                if self.capturing_action.is_some() {
                    self.capturing_action = None;
                    self.status = "Cancelled keybinding capture".into();
                    return Task::none();
                }
                if self.settings_open {
                    self.settings_open = false;
                    self.end_overlay_resize();
                    return Task::none();
                }
                if self.picking_start.is_some() {
                    self.picking_start = None;
                    self.status = "Cancelled start placement".into();
                    return Task::none();
                }
                if self.preset_browser_open {
                    self.preset_browser_open = false;
                    self.end_overlay_resize();
                    return Task::none();
                }
                if self.dragging_machine.is_some() {
                    self.dragging_machine = None;
                    return Task::none();
                }
                if self.selected_machine.is_some() {
                    self.selected_machine = None;
                    self.machine_color_picker.close();
                    return Task::none();
                }
                if self.color_picker.is_open() {
                    self.color_picker.close();
                    return Task::none();
                }
                if self.machine_color_picker.is_open() {
                    self.machine_color_picker.close();
                    return Task::none();
                }
                self.drawing_only = false;
                Self::exit_fullscreen_if_needed()
            }
            Message::PaletteSelected(kind) => {
                self.program.canvas_palette.set_kind(kind, self.num_symbols);
                if kind != PaletteKind::Gradient {
                    self.color_picker.close();
                }
                self.status = format!("Canvas palette: {kind}");
                Task::none()
            }
            Message::GradientStartChanged(hex) => {
                match self
                    .program
                    .canvas_palette
                    .set_gradient_start_hex(hex, self.num_symbols)
                {
                    Ok(()) => {
                        self.status.clear();
                        if self.color_picker.open_endpoint() == Some(GradientEndpoint::Start) {
                            self.color_picker
                                .sync_from_rgb(self.program.canvas_palette.gradient_start);
                        }
                    }
                    Err(e) => {
                        self.status = format!("Start colour: {e}");
                    }
                }
                Task::none()
            }
            Message::GradientEndChanged(hex) => {
                match self
                    .program
                    .canvas_palette
                    .set_gradient_end_hex(hex, self.num_symbols)
                {
                    Ok(()) => {
                        self.status.clear();
                        if self.color_picker.open_endpoint() == Some(GradientEndpoint::End) {
                            self.color_picker
                                .sync_from_rgb(self.program.canvas_palette.gradient_end);
                        }
                    }
                    Err(e) => {
                        self.status = format!("End colour: {e}");
                    }
                }
                Task::none()
            }
            Message::ColorPicker(msg) => {
                match self.color_picker.update(
                    msg,
                    &mut self.program.canvas_palette,
                    self.num_symbols,
                ) {
                    Ok(true) => {
                        self.status.clear();
                    }
                    Ok(false) => {}
                    Err(e) => {
                        self.status = format!("Picker colour: {e}");
                    }
                }
                Task::none()
            }
            Message::MachinePaletteSelected(i, kind) => {
                if let Some(machine) = self.program.machines.get_mut(i) {
                    machine.palette.set_kind(kind, self.program.num_symbols);
                    if kind != PaletteKind::Gradient {
                        self.machine_color_picker.close();
                    }
                    let name = machine.display_name(i);
                    self.status = format!("{name} palette: {kind}");
                }
                Task::none()
            }
            Message::MachineGradientStartChanged(i, hex) => {
                let n = self.program.num_symbols;
                match self.program.machines.get_mut(i) {
                    Some(machine) => match machine.palette.set_gradient_start_hex(hex, n) {
                        Ok(()) => {
                            self.status.clear();
                            if self.machine_color_picker.open_endpoint()
                                == Some(GradientEndpoint::Start)
                            {
                                self.machine_color_picker
                                    .sync_from_rgb(machine.palette.gradient_start);
                            }
                        }
                        Err(e) => {
                            self.status = format!("Start colour: {e}");
                        }
                    },
                    None => {}
                }
                Task::none()
            }
            Message::MachineGradientEndChanged(i, hex) => {
                let n = self.program.num_symbols;
                match self.program.machines.get_mut(i) {
                    Some(machine) => match machine.palette.set_gradient_end_hex(hex, n) {
                        Ok(()) => {
                            self.status.clear();
                            if self.machine_color_picker.open_endpoint()
                                == Some(GradientEndpoint::End)
                            {
                                self.machine_color_picker
                                    .sync_from_rgb(machine.palette.gradient_end);
                            }
                        }
                        Err(e) => {
                            self.status = format!("End colour: {e}");
                        }
                    },
                    None => {}
                }
                Task::none()
            }
            Message::MachineColorPicker(i, msg) => {
                let n = self.program.num_symbols;
                if let Some(machine) = self.program.machines.get_mut(i) {
                    match self
                        .machine_color_picker
                        .update(msg, &mut machine.palette, n)
                    {
                        Ok(true) => {
                            self.status.clear();
                        }
                        Ok(false) => {}
                        Err(e) => {
                            self.status = format!("Picker colour: {e}");
                        }
                    }
                }
                Task::none()
            }
            Message::OpenPresetBrowser => {
                self.selected_machine = None;
                self.machine_color_picker.close();
                self.settings_open = false;
                self.capturing_action = None;
                self.end_overlay_resize();
                self.preset_browser_open = true;
                self.refresh_preset_list();
                Task::none()
            }
            Message::ClosePresetBrowser => {
                self.preset_browser_open = false;
                self.end_overlay_resize();
                Task::none()
            }
            Message::PresetNameChanged(name) => {
                self.preset_name = name;
                Task::none()
            }
            Message::StorePreset => match self.program.to_preset(&self.preset_name) {
                Ok(mut preset) => {
                    preset.performance_bindings = self
                        .performance_bindings
                        .to_preset_bindings(&self.program.machine_ids());
                    match preset::save_preset(&preset) {
                        Ok(name) => {
                            self.status = format!("Stored preset \"{name}\"");
                            self.refresh_preset_list();
                        }
                        Err(e) => self.status = format!("Store failed: {e}"),
                    }
                    Task::none()
                }
                Err(e) => {
                    self.status = format!("Store failed: {e}");
                    Task::none()
                }
            },
            Message::LoadPreset(name) => match preset::load_preset(&name) {
                Ok(preset) => match Program::from_preset(&preset) {
                    Ok(program) => {
                        let n = program.machines.len();
                        let perf = PerformanceBindings::from_preset_bindings(
                            &preset.performance_bindings,
                            &program.machine_ids(),
                        );
                        self.apply_loaded_program(
                            program,
                            &format!("Loaded preset \"{name}\" ({n} machine(s))"),
                        );
                        self.performance_bindings = perf;
                        Task::none()
                    }
                    Err(e) => {
                        self.status = format!("Load failed: {e}");
                        Task::none()
                    }
                },
                Err(e) => {
                    self.status = format!("Load failed: {e}");
                    Task::none()
                }
            },
            Message::DeletePreset(name) => match preset::delete_preset(&name) {
                Ok(()) => {
                    self.settings.clear_preset(&name);
                    if self
                        .button_controls
                        .as_ref()
                        .is_some_and(|t| t.preset_name.as_deref() == Some(name.as_str()))
                    {
                        self.button_controls = None;
                        self.capturing_action = None;
                    }
                    self.persist_settings();
                    self.status = format!("Deleted preset \"{name}\"");
                    self.refresh_preset_list();
                    Task::none()
                }
                Err(e) => {
                    self.status = format!("Delete failed: {e}");
                    Task::none()
                }
            },
            Message::PresetSortChanged(sort) => {
                self.preset_sort = sort;
                preset::sort_preset_infos(&mut self.preset_list, sort);
                Task::none()
            }
            Message::ToggleInspectorGroup(group) => {
                match group {
                    InspectorGroup::Canvas => self.canvas_open = !self.canvas_open,
                    InspectorGroup::Palette => self.palette_open = !self.palette_open,
                    InspectorGroup::Simulation => self.simulation_open = !self.simulation_open,
                }
                Task::none()
            }
            Message::KeyPressed { key, modifiers } => {
                if let Some(target) = self.capturing_action.clone() {
                    if let Some(binding) =
                        settings::Keybinding::from_event(target.clone(), &key, modifiers)
                    {
                        let label = binding.display();
                        let name = self.target_label(&target);
                        self.set_binding(binding);
                        self.capturing_action = None;
                        if target.is_performance() {
                            self.status = format!(
                                "Performance binding (stored with presets): {name}: {label}"
                            );
                        } else {
                            self.status = format!("Bound {name}: {label}");
                        }
                    }
                    return Task::none();
                }
                if self.button_controls.is_some()
                    && matches!(key, keyboard::Key::Named(key::Named::Enter))
                {
                    self.button_controls = None;
                    return Task::none();
                }
                if let Some(binding) = self.match_any_binding(&key, modifiers).cloned() {
                    return self.dispatch_binding(&binding);
                }
                Task::none()
            }
            Message::OpenSettings => {
                self.preset_browser_open = false;
                self.end_overlay_resize();
                self.settings_open = true;
                self.capturing_action = None;
                self.sync_settings_drafts();
                Task::none()
            }
            Message::CloseSettings => {
                self.settings_open = false;
                self.capturing_action = None;
                self.end_overlay_resize();
                Task::none()
            }
            Message::SettingsTab(tab) => {
                self.settings_tab = tab;
                if tab != SettingsTab::Keybindings {
                    self.capturing_action = None;
                }
                Task::none()
            }
            Message::SettingsTheme(theme) => {
                self.settings.theme = theme;
                theme.activate();
                self.persist_settings();
                Task::none()
            }
            Message::CaptureBinding(target) => {
                self.capturing_action = Some(target.clone());
                self.status = format!("Press a key for {}", self.target_label(&target));
                Task::none()
            }
            Message::OpenButtonControls(target) => {
                self.button_controls = Some(target.clone());
                self.capturing_action = Some(target);
                self.status.clear();
                Task::none()
            }
            Message::CloseButtonControls => {
                self.button_controls = None;
                self.capturing_action = None;
                Task::none()
            }
            Message::ClearBinding => {
                if let Some(target) = self.button_controls.clone() {
                    self.clear_binding(&target);
                    self.capturing_action = None;
                    if target.is_performance() {
                        self.status = format!(
                            "Cleared performance binding for {}",
                            self.target_label(&target)
                        );
                    } else {
                        self.status = format!("Cleared binding for {}", self.target_label(&target));
                    }
                }
                Task::none()
            }
            Message::SettingsIncStates => {
                if self.settings.defaults.num_states < MAX_STATES {
                    self.settings.defaults.num_states += 1;
                    self.persist_settings();
                }
                Task::none()
            }
            Message::SettingsDecStates => {
                if self.settings.defaults.num_states > MIN_STATES {
                    self.settings.defaults.num_states -= 1;
                    self.persist_settings();
                }
                Task::none()
            }
            Message::SettingsIncSymbols => {
                if self.settings.defaults.num_symbols < MAX_SYMBOLS {
                    self.settings.defaults.num_symbols += 1;
                    self.persist_settings();
                }
                Task::none()
            }
            Message::SettingsDecSymbols => {
                if self.settings.defaults.num_symbols > MIN_SYMBOLS {
                    self.settings.defaults.num_symbols -= 1;
                    self.persist_settings();
                }
                Task::none()
            }
            Message::SettingsResolutionPreset(width, height) => {
                self.settings.defaults.map_width = width;
                self.settings.defaults.map_height = height;
                self.settings_width_text = width.to_string();
                self.settings_height_text = height.to_string();
                self.persist_settings();
                self.status.clear();
                Task::none()
            }
            Message::SettingsWidthText(text) => {
                self.settings_width_text = text;
                self.try_apply_settings_resolution();
                Task::none()
            }
            Message::SettingsHeightText(text) => {
                self.settings_height_text = text;
                self.try_apply_settings_resolution();
                Task::none()
            }
            Message::SettingsSpeed(speed) => {
                self.settings.defaults.speed = speed.clamp(SIM_SPEED_MIN, SIM_SPEED_MAX);
                self.settings_speed_text = format_sim_speed(self.settings.defaults.speed);
                self.persist_settings();
                Task::none()
            }
            Message::SettingsSpeedText(text) => {
                self.settings_speed_text = text;
                if let Ok(speed) =
                    parse_clamped_f32(&self.settings_speed_text, SIM_SPEED_MIN, SIM_SPEED_MAX)
                {
                    self.settings.defaults.speed = speed;
                    self.persist_settings();
                }
                Task::none()
            }
            Message::SettingsRefreshPreset(hz) => {
                let hz = hz.clamp(MIN_REFRESH_HZ, MAX_REFRESH_HZ);
                self.settings.defaults.refresh_hz = hz;
                self.settings_refresh_text = hz.to_string();
                self.persist_settings();
                self.status.clear();
                Task::none()
            }
            Message::SettingsRefreshText(text) => {
                self.settings_refresh_text = text;
                match parse_refresh_hz(&self.settings_refresh_text) {
                    Ok(hz) => {
                        self.settings.defaults.refresh_hz = hz;
                        self.persist_settings();
                        self.status.clear();
                    }
                    Err(e) => {
                        self.status = e;
                    }
                }
                Task::none()
            }
            Message::SettingsMaxItrs(value) => {
                let value = value.round() as u64;
                self.settings.defaults.max_itrs = value.clamp(MIN_MAX_ITRS, MAX_MAX_ITRS);
                self.persist_settings();
                Task::none()
            }
            Message::SettingsRasterMode(mode) => {
                self.settings.defaults.raster_mode = mode;
                self.persist_settings();
                Task::none()
            }
            Message::SettingsPalette(kind) => {
                self.settings.defaults.palette_kind = kind;
                self.persist_settings();
                Task::none()
            }
            Message::SettingsGradientStart(hex) => {
                self.settings_gradient_start = hex.clone();
                match parse_hex_rgb(&hex) {
                    Ok(rgb) => {
                        self.settings.defaults.gradient_start = rgb_to_hex(rgb);
                        self.persist_settings();
                        self.status.clear();
                    }
                    Err(e) => {
                        self.status = format!("Start colour: {e}");
                    }
                }
                Task::none()
            }
            Message::SettingsGradientEnd(hex) => {
                self.settings_gradient_end = hex.clone();
                match parse_hex_rgb(&hex) {
                    Ok(rgb) => {
                        self.settings.defaults.gradient_end = rgb_to_hex(rgb);
                        self.persist_settings();
                        self.status.clear();
                    }
                    Err(e) => {
                        self.status = format!("End colour: {e}");
                    }
                }
                Task::none()
            }
            Message::SettingsTapeKind(kind) => {
                self.settings.defaults.tape_kind = kind;
                self.persist_settings();
                self.status.clear();
                Task::none()
            }
            Message::SettingsGaussianMean(value) => {
                self.settings.defaults.gaussian_mean =
                    value.clamp(MIN_GAUSSIAN_MEAN, MAX_GAUSSIAN_MEAN);
                self.persist_settings();
                Task::none()
            }
            Message::SettingsGaussianSigma(value) => {
                self.settings.defaults.gaussian_sigma =
                    value.clamp(MIN_GAUSSIAN_SIGMA, MAX_GAUSSIAN_SIGMA);
                self.persist_settings();
                Task::none()
            }
            Message::SettingsPerlinScale(value) => {
                self.settings.defaults.perlin_scale =
                    value.clamp(MIN_PERLIN_SCALE, MAX_PERLIN_SCALE);
                self.persist_settings();
                Task::none()
            }
            Message::SettingsPerlinOctaves(value) => {
                self.settings.defaults.perlin_octaves =
                    (value.round() as u8).clamp(MIN_PERLIN_OCTAVES, MAX_PERLIN_OCTAVES);
                self.persist_settings();
                Task::none()
            }
            Message::SettingsAllowDiagonals(allow) => {
                self.settings.defaults.allow_diagonals = allow;
                self.persist_settings();
                Task::none()
            }
            Message::ToggleSpeedSnapshots => {
                self.speed_snapshot_open = !self.speed_snapshot_open;
                Task::none()
            }
            Message::SpeedSnapshotNameChanged(name) => {
                self.speed_snapshot_name = name;
                Task::none()
            }
            Message::StoreSpeedSnapshot => {
                match self
                    .program
                    .store_speed_snapshot(&self.speed_snapshot_name)
                {
                    Ok(name) => {
                        self.speed_snapshot_name.clear();
                        self.status = format!("Stored snapshot \"{name}\"");
                    }
                    Err(e) => self.status = format!("Store snapshot failed: {e}"),
                }
                Task::none()
            }
            Message::RecallSpeedSnapshot(name) => {
                match self.program.recall_speed_snapshot(&name) {
                    Ok(()) => {
                        self.sync_machine_speed_texts();
                        self.status = format!("Recalled snapshot \"{name}\"");
                    }
                    Err(e) => self.status = format!("Recall snapshot failed: {e}"),
                }
                Task::none()
            }
            Message::DeleteSpeedSnapshot(name) => {
                match self.program.delete_speed_snapshot(&name) {
                    Ok(()) => {
                        self.status = format!("Deleted snapshot \"{name}\"");
                    }
                    Err(e) => self.status = format!("Delete snapshot failed: {e}"),
                }
                Task::none()
            }
        }
    }

    fn run_frame(&mut self) {
        if let Some(resume_at) = self.mode_switch_resume_at {
            if Instant::now() < resume_at {
                return;
            }
            self.mode_switch_resume_at = None;
        }

        let max_itrs = (self.max_itrs as f64 * f64::from(self.speed)) as u64;
        if max_itrs == 0 {
            return;
        }

        let start = Instant::now();
        let start_itr = self.program.itr_count;
        let budget = frame_period(self.refresh_hz);

        loop {
            let remaining = max_itrs.saturating_sub(self.program.itr_count - start_itr);
            if remaining == 0 || start.elapsed() >= budget {
                break;
            }
            let chunk = CHUNK.min(remaining as usize);
            let _ = self.program.update(chunk);
        }

        self.refresh_frame();
    }

    fn sync_gpu_dirty(&mut self) {
        if let Some(rect) = self.program.take_canvas_dirty() {
            self.gpu_dirty = Some(match self.gpu_dirty {
                Some(acc) => acc.union(rect),
                None => rect,
            });
            self.gpu_dirty_revision = self.program.canvas_revision;
        }
    }

    fn sync_resolution_text(&mut self) {
        self.map_width_text = self.program.width.to_string();
        self.map_height_text = self.program.height.to_string();
    }

    fn apply_resolution(&mut self, width: usize, height: usize) {
        if width == self.program.width && height == self.program.height {
            self.sync_resolution_text();
            self.status.clear();
            return;
        }
        match self.program.set_size(width, height) {
            Ok(()) => {
                self.sync_resolution_text();
                self.sync_share_texts();
                self.status = format!("Resolution: {width} × {height}; drawing reset");
                self.refresh_frame();
            }
            Err(e) => {
                self.status = e;
            }
        }
    }

    fn try_apply_custom_resolution(&mut self) {
        match (
            parse_map_dim(&self.map_width_text, "Width"),
            parse_map_dim(&self.map_height_text, "Height"),
        ) {
            (Ok(width), Ok(height)) => {
                if width == self.program.width && height == self.program.height {
                    self.status.clear();
                    return;
                }
                self.apply_resolution(width, height);
            }
            (Err(e), _) | (_, Err(e)) => {
                self.status = e;
            }
        }
    }

    fn set_raster_mode(&mut self, mode: RasterMode) {
        if mode == self.raster_mode {
            return;
        }
        self.raster_mode = mode;
        match mode {
            RasterMode::Gpu => {
                self.status = "Raster: GPU".into();
            }
            RasterMode::Cpu => {
                self.status = "Raster: CPU".into();
                self.refresh_frame();
            }
        }
    }

    fn refresh_frame(&mut self) {
        self.sync_gpu_dirty();
        if self.raster_mode != RasterMode::Cpu {
            return;
        }
        self.pixels.clone_from(&self.program.canvas);
        self.frame = Handle::from_rgba(
            self.program.width as u32,
            self.program.height as u32,
            self.pixels.clone(),
        );
    }

    fn adjust_selection_after_remove(&mut self, removed: usize) {
        if let Some(selected) = self.selected_machine {
            if selected == removed {
                self.selected_machine = None;
            } else if selected > removed {
                self.selected_machine = Some(selected - 1);
            }
        }
        if let Some(dragging) = self.dragging_machine {
            if dragging == removed {
                self.dragging_machine = None;
            } else if dragging > removed {
                self.dragging_machine = Some(dragging - 1);
            }
        }
        if let Some(picking) = self.picking_start {
            if picking == removed {
                self.picking_start = None;
            } else if picking > removed {
                self.picking_start = Some(picking - 1);
            }
        }
    }

    fn apply_machine_reorder(&mut self, from: usize, to: usize) -> Result<(), String> {
        self.program.reorder_machines(from, to)?;
        if from != to && from < self.share_texts.len() && to < self.share_texts.len() {
            let text = self.share_texts.remove(from);
            self.share_texts.insert(to, text);
        }
        if from != to
            && from < self.machine_speed_texts.len()
            && to < self.machine_speed_texts.len()
        {
            let text = self.machine_speed_texts.remove(from);
            self.machine_speed_texts.insert(to, text);
        }
        if let Some(selected) = self.selected_machine {
            self.selected_machine = Some(remap_index_after_reorder(selected, from, to));
        }
        if let Some(dragging) = self.dragging_machine {
            self.dragging_machine = Some(remap_index_after_reorder(dragging, from, to));
        }
        if let Some(picking) = self.picking_start {
            self.picking_start = Some(remap_index_after_reorder(picking, from, to));
        }
        Ok(())
    }

    fn drawing_canvas(&self) -> Element<'_, Message> {
        let drawing: Element<'_, Message> = match self.raster_mode {
            RasterMode::Gpu => gpu_raster::canvas_shader(
                &self.program.canvas,
                self.program.width as u32,
                self.program.height as u32,
                self.gpu_dirty,
                self.gpu_dirty_revision,
            )
            .into(),
            RasterMode::Cpu => simulation_frame(self.frame.clone()).into(),
        };
        let picking = self.picking_start.is_some();
        let drawing = if picking {
            drawing::capture_click(drawing, |point, size| Message::CanvasClicked {
                x: point.x,
                y: point.y,
                width: size.width,
                height: size.height,
            })
        } else {
            drawing
        };
        let mut area = mouse_area(drawing);
        if picking {
            area = area.interaction(mouse::Interaction::Crosshair);
        } else {
            area = area.on_double_click(Message::ToggleDrawingOnly);
        }
        container(area)
            .center(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(chrome::viewer)
            .into()
    }

    fn view(&self) -> Element<'_, Message> {
        if self.drawing_only {
            return container(self.drawing_canvas())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(chrome::window)
                .into();
        }

        let content = container(
            column![
                self.toolbar(),
                chrome::hrule(),
                row![
                    self.machine_panel(),
                    chrome::vrule(),
                    self.drawing_canvas(),
                    chrome::vrule(),
                    self.inspector(),
                ]
                .height(Length::Fill),
                chrome::hrule(),
                self.status_bar(),
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(chrome::window);

        let mut layers = vec![content.into()];
        if self.settings_open {
            layers.push(self.settings_overlay());
        } else if self.preset_browser_open {
            layers.push(self.preset_browser());
        }
        if self.button_controls.is_some() {
            layers.push(self.button_controls_overlay());
        }
        if self.overlay_resize.is_some() {
            layers.push(
                mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_move(Message::OverlayResizeMove)
                    .on_release(Message::OverlayResizeEnd)
                    .interaction(mouse::Interaction::ResizingDiagonallyDown)
                    .into(),
            );
        }
        if layers.len() == 1 {
            layers.pop().unwrap()
        } else {
            stack(layers)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    fn toolbar(&self) -> Element<'_, Message> {
        container(
            row![
                bindable(
                    chrome::compact_button("Random").on_press(Message::Random),
                    BindTarget::global(Action::Random),
                ),
                bindable(
                    chrome::compact_button("Mutate").on_press(Message::Mutate),
                    BindTarget::global(Action::Mutate),
                ),
                chrome::dim("Mutate %"),
                chrome::resettable_slider(
                    slider(
                        f32::from(MIN_MUTATE_PERCENT)..=f32::from(MAX_MUTATE_PERCENT),
                        self.mutate_percent,
                        Message::MutatePercent,
                    )
                    .step(1.0_f32)
                    .width(100)
                    .style(chrome::slider_style),
                    Message::MutatePercent(f32::from(DEFAULT_MUTATE_PERCENT)),
                ),
                chrome::dim(format!("{:.0}%", self.mutate_percent)).width(36),
                bindable(
                    chrome::compact_button("Restart").on_press(Message::Restart),
                    BindTarget::global(Action::Restart),
                ),
                bindable(
                    chrome::compact_button("Presets").on_press(Message::OpenPresetBrowser),
                    BindTarget::global(Action::OpenPresets),
                ),
                bindable(
                    chrome::compact_button("Settings").on_press(Message::OpenSettings),
                    BindTarget::global(Action::OpenSettings),
                ),
                Space::new().width(Length::Fill),
                bindable(
                    chrome::compact_button("Fullscreen").on_press(Message::ToggleFullscreen),
                    BindTarget::global(Action::ToggleFullscreen),
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([6, 10])
        .width(Length::Fill)
        .style(chrome::toolbar)
        .into()
    }

    fn machine_panel(&self) -> Element<'_, Message> {
        let can_reorder = self.program.machines.len() > 1;
        let mut list = column![].spacing(6);
        for i in 0..self.program.machines.len() {
            let selected = self.selected_machine == Some(i);
            let dragging = self.dragging_machine == Some(i);
            let speed = self.program.machines[i].speed;
            let active = self.program.machines[i].active;
            let name = self.program.machines[i].display_name(i);

            let mut title = row![].spacing(6).align_y(Alignment::Center);
            if can_reorder {
                title = title.push(
                    mouse_area(chrome::drag_handle())
                        .on_press(Message::MachineDragStart(i))
                        .interaction(if dragging {
                            mouse::Interaction::Grabbing
                        } else {
                            mouse::Interaction::Grab
                        }),
                );
            }
            title = title
                .push(chrome::value(name))
                .push(Space::new().width(Length::Fill))
                .push(chrome::dim("Active"))
                .push(machine_active_toggler(i, active));

            let id = self.program.machines[i].id;
            let picking = self.picking_start == Some(i);
            let set_start = if picking {
                chrome::accent_button("Set start")
            } else {
                chrome::compact_button("Set start")
            }
            .on_press(Message::TogglePickStart(i));
            let actions = row![
                bindable(
                    chrome::compact_button("Randomise").on_press(Message::RandomizeMachine(i)),
                    BindTarget::machine(Action::RandomizeMachine, id),
                ),
                bindable(
                    chrome::compact_button("Mutate").on_press(Message::MutateMachine(i)),
                    BindTarget::machine(Action::MutateMachine, id),
                ),
                bindable(set_start, BindTarget::machine(Action::TogglePickStart, id)),
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            list = list.push(
                mouse_area(
                    container(
                        column![
                            title,
                            actions,
                            machine_speed_slider(
                                i,
                                speed,
                                self.machine_speed_texts
                                    .get(i)
                                    .map(String::as_str)
                                    .unwrap_or(""),
                                self.program.schedule_mode,
                                self.program.scheduled_rate(i),
                            ),
                        ]
                        .spacing(4),
                    )
                    .padding(8)
                    .width(Length::Fill)
                    .style(chrome::machine_card(selected, active, dragging)),
                )
                .on_press(Message::SelectMachine(i))
                .on_enter(Message::MachineDragOver(i)),
            );
        }

        let mut panel = column![
            row![
                container(chrome::dim("MACHINES")).padding(iced::Padding {
                    top: 8.0,
                    right: 0.0,
                    bottom: 4.0,
                    left: 10.0,
                }),
                Space::new().width(Length::Fill),
                container(bindable(
                    chrome::compact_button("Add machine").on_press(Message::AddMachine),
                    BindTarget::global(Action::AddMachine),
                ),)
                .padding(iced::Padding {
                    top: 4.0,
                    right: 8.0,
                    bottom: 4.0,
                    left: 0.0,
                }),
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill),
            row![
                container(chrome::dim("Normalised")).padding(iced::Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 4.0,
                    left: 10.0,
                }),
                Space::new().width(Length::Fill),
                container(
                    toggler(self.program.schedule_mode == ScheduleMode::Normalised)
                        .on_toggle(Message::ScheduleModeToggled),
                )
                .padding(iced::Padding {
                    top: 0.0,
                    right: 8.0,
                    bottom: 4.0,
                    left: 0.0,
                }),
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill),
            self.speed_snapshot_section(),
            chrome::hrule(),
            scrollable(list.padding(8))
                .style(chrome::scrollable_style)
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .height(Length::Fill);

        if let Some(index) = self.selected_machine {
            if index < self.program.machines.len() {
                panel = panel
                    .push(chrome::hrule())
                    .push(self.machine_inspector(index));
            }
        }

        container(panel)
            .width(chrome::LEFT_PANEL)
            .height(Length::Fill)
            .style(chrome::panel)
            .into()
    }

    fn speed_snapshot_section(&self) -> Element<'_, Message> {
        let count = self.program.speed_snapshots.len();
        let heading = if count == 0 {
            "Snapshots".to_string()
        } else {
            format!("Snapshots ({count})")
        };
        let toggle_label = if self.speed_snapshot_open {
            "Hide"
        } else {
            "Show"
        };
        let toggle = if self.speed_snapshot_open {
            chrome::accent_button(toggle_label)
        } else {
            chrome::compact_button(toggle_label)
        }
        .on_press(Message::ToggleSpeedSnapshots);

        let mut section = column![row![
            container(chrome::dim(heading)).padding(iced::Padding {
                top: 0.0,
                right: 0.0,
                bottom: 4.0,
                left: 10.0,
            }),
            Space::new().width(Length::Fill),
            container(toggle).padding(iced::Padding {
                top: 0.0,
                right: 8.0,
                bottom: 4.0,
                left: 0.0,
            }),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill)]
        .spacing(4);

        if self.speed_snapshot_open {
            let store_row = row![
                chrome::field("Name", &self.speed_snapshot_name)
                    .on_input(Message::SpeedSnapshotNameChanged)
                    .width(Length::Fill),
                chrome::compact_button("Store").on_press(Message::StoreSpeedSnapshot),
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            section = section.push(
                container(store_row).padding(iced::Padding {
                    top: 0.0,
                    right: 8.0,
                    bottom: 0.0,
                    left: 8.0,
                }),
            );

            if self.program.speed_snapshots.is_empty() {
                section = section.push(
                    container(chrome::dim("No snapshots yet")).padding(iced::Padding {
                        top: 0.0,
                        right: 8.0,
                        bottom: 4.0,
                        left: 10.0,
                    }),
                );
            } else {
                let mut rows = column![].spacing(2);
                for snap in &self.program.speed_snapshots {
                    let name = snap.name.clone();
                    rows = rows.push(
                        row![
                            chrome::compact_button(snap.name.as_str())
                                .on_press(Message::RecallSpeedSnapshot(name.clone()))
                                .width(Length::Fill),
                            chrome::danger_button("×")
                                .on_press(Message::DeleteSpeedSnapshot(name)),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    );
                }
                section = section.push(container(rows).padding(iced::Padding {
                    top: 0.0,
                    right: 8.0,
                    bottom: 4.0,
                    left: 8.0,
                }));
            }
        }

        section.into()
    }

    fn inspector(&self) -> Element<'_, Message> {
        let groups = column![]
            .push(collapsible(
                "CANVAS",
                self.canvas_open,
                Message::ToggleInspectorGroup(InspectorGroup::Canvas),
                BindTarget::global(Action::ToggleCanvas),
                self.canvas_group(),
            ))
            .push(collapsible(
                "PALETTE",
                self.palette_open,
                Message::ToggleInspectorGroup(InspectorGroup::Palette),
                BindTarget::global(Action::TogglePalette),
                self.palette_group(),
            ))
            .push(collapsible(
                "SIMULATION",
                self.simulation_open,
                Message::ToggleInspectorGroup(InspectorGroup::Simulation),
                BindTarget::global(Action::ToggleSimulation),
                self.simulation_group(),
            ));

        container(
            scrollable(groups.padding(8))
                .style(chrome::scrollable_style)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(chrome::RIGHT_PANEL)
        .height(Length::Fill)
        .style(chrome::panel)
        .into()
    }

    fn canvas_group(&self) -> Element<'_, Message> {
        let init = &self.program.tape_init;
        let mut col = column![
            inspector_row(
                "Symbols",
                stepper(
                    self.num_symbols,
                    Message::DecSymbols,
                    Message::IncSymbols,
                    Some((
                        BindTarget::global(Action::DecSymbols),
                        BindTarget::global(Action::IncSymbols),
                    )),
                ),
            ),
            inspector_row(
                "Diagonals",
                toggler(self.program.allow_diagonals).on_toggle(Message::AllowDiagonalsToggled),
            ),
            inspector_row(
                "Size",
                chrome::decorate_pick_list(
                    pick_list(
                        RESOLUTION_PRESETS,
                        RESOLUTION_PRESETS.iter().copied().find(|preset| {
                            preset.width == self.program.width
                                && preset.height == self.program.height
                        }),
                        |preset: ResolutionPreset| {
                            Message::ResolutionPreset(preset.width, preset.height)
                        },
                    )
                    .placeholder("Custom")
                    .width(Length::Fill),
                ),
            ),
            inspector_row(
                "",
                row![
                    chrome::field("W", &self.map_width_text)
                        .on_input(Message::MapWidthText)
                        .width(Length::Fill),
                    chrome::dim("×").width(14).align_x(Alignment::Center),
                    chrome::field("H", &self.map_height_text)
                        .on_input(Message::MapHeightText)
                        .width(Length::Fill),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            inspector_row(
                "Init",
                chrome::decorate_pick_list(
                    pick_list(TapeInitKind::ALL, Some(init.kind), Message::TapeKind)
                        .width(Length::Fill),
                ),
            ),
        ]
        .spacing(6);

        match init.kind {
            TapeInitKind::Gaussian => {
                col = col
                    .push(inspector_row(
                        "Mean",
                        row![
                            chrome::param_slider(
                                MIN_GAUSSIAN_MEAN..=MAX_GAUSSIAN_MEAN,
                                init.gaussian_mean,
                                0.01_f32,
                                Message::TapeGaussianMean,
                                Message::TapeGaussianMean(DEFAULT_GAUSSIAN_MEAN),
                            ),
                            chrome::dim(format!("{:.2}", init.gaussian_mean))
                                .width(36)
                                .align_x(Alignment::End),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ))
                    .push(inspector_row(
                        "Sigma",
                        row![
                            chrome::param_slider(
                                MIN_GAUSSIAN_SIGMA..=MAX_GAUSSIAN_SIGMA,
                                init.gaussian_sigma,
                                0.01_f32,
                                Message::TapeGaussianSigma,
                                Message::TapeGaussianSigma(DEFAULT_GAUSSIAN_SIGMA),
                            ),
                            chrome::dim(format!("{:.2}", init.gaussian_sigma))
                                .width(36)
                                .align_x(Alignment::End),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ));
            }
            TapeInitKind::Perlin => {
                col = col
                    .push(inspector_row(
                        "Scale",
                        row![
                            chrome::param_slider(
                                MIN_PERLIN_SCALE..=MAX_PERLIN_SCALE,
                                init.perlin_scale,
                                1.0_f32,
                                Message::TapePerlinScale,
                                Message::TapePerlinScale(DEFAULT_PERLIN_SCALE),
                            ),
                            chrome::dim(format!("{:.0}", init.perlin_scale))
                                .width(36)
                                .align_x(Alignment::End),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ))
                    .push(inspector_row(
                        "Octaves",
                        row![
                            chrome::param_slider(
                                f32::from(MIN_PERLIN_OCTAVES)..=f32::from(MAX_PERLIN_OCTAVES),
                                f32::from(init.perlin_octaves),
                                1.0_f32,
                                Message::TapePerlinOctaves,
                                Message::TapePerlinOctaves(f32::from(DEFAULT_PERLIN_OCTAVES)),
                            ),
                            chrome::dim(init.perlin_octaves.to_string())
                                .width(36)
                                .align_x(Alignment::End),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ));
            }
            TapeInitKind::Empty | TapeInitKind::Uniform => {}
        }

        if init.kind != TapeInitKind::Empty {
            col = col.push(inspector_row(
                "",
                bindable(
                    chrome::compact_button("Reseed").on_press(Message::ReseedTape),
                    BindTarget::global(Action::ReseedTape),
                ),
            ));
        }

        col.into()
    }

    fn palette_group(&self) -> Element<'_, Message> {
        column![
            chrome::dim("Applies on Restart / Reseed (machine 0 band)."),
            color_picker::palette_controls(
                &self.program.canvas_palette,
                &self.color_picker,
                self.num_symbols,
            )
            .map(|msg| match msg {
                ControlsMessage::KindSelected(kind) => Message::PaletteSelected(kind),
                ControlsMessage::GradientStartChanged(hex) => Message::GradientStartChanged(hex),
                ControlsMessage::GradientEndChanged(hex) => Message::GradientEndChanged(hex),
                ControlsMessage::Picker(color_picker::Message::BindClose) => {
                    Message::OpenButtonControls(BindTarget::global(Action::CloseColorPicker))
                }
                ControlsMessage::Picker(m) => Message::ColorPicker(m),
            }),
        ]
        .spacing(6)
        .into()
    }

    fn simulation_group(&self) -> Element<'_, Message> {
        column![
            inspector_row(
                "Speed",
                row![
                    chrome::param_slider(
                        SIM_SPEED_MIN..=SIM_SPEED_MAX,
                        self.speed,
                        SIM_SPEED_STEP,
                        Message::SpeedChanged,
                        Message::SpeedChanged(1.0),
                    ),
                    chrome::compact_field("", &self.speed_text)
                        .on_input(Message::SpeedText)
                        .width(48),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            inspector_row(
                "Refresh",
                chrome::decorate_pick_list(
                    pick_list(
                        REFRESH_PRESETS,
                        REFRESH_PRESETS
                            .iter()
                            .copied()
                            .find(|preset| preset.0 == self.refresh_hz),
                        |preset: RefreshPreset| Message::RefreshPreset(preset.0),
                    )
                    .placeholder("Custom")
                    .width(Length::Fill),
                ),
            ),
            inspector_row(
                "",
                row![
                    chrome::field("Hz", &self.refresh_hz_text)
                        .on_input(Message::RefreshHzText)
                        .width(Length::Fill),
                    chrome::dim("Hz").width(22),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            inspector_row(
                "Max itrs",
                row![
                    chrome::param_slider(
                        MIN_MAX_ITRS as f32..=MAX_MAX_ITRS as f32,
                        self.max_itrs as f32,
                        1_000.0_f32,
                        Message::MaxItrsChanged,
                        Message::MaxItrsChanged(DEFAULT_MAX_ITRS as f32),
                    ),
                    chrome::dim(self.max_itrs.to_string())
                        .width(64)
                        .align_x(Alignment::End),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            inspector_row(
                "Raster",
                chrome::decorate_pick_list(
                    pick_list(RasterMode::ALL, Some(self.raster_mode), Message::RasterMode,)
                        .width(Length::Fill),
                ),
            ),
        ]
        .spacing(6)
        .into()
    }

    fn machine_inspector(&self, index: usize) -> Element<'_, Message> {
        let machine = &self.program.machines[index];
        let name_placeholder = default_machine_name(index);
        let share = self
            .share_texts
            .get(index)
            .map(String::as_str)
            .unwrap_or("");
        let can_remove = self.program.machines.len() > 1;

        let id = machine.id;
        let mut remove = chrome::danger_button("Remove");
        if can_remove {
            remove = remove.on_press(Message::RemoveMachine(index));
        }

        let header = row![
            chrome::dim(machine.display_name(index)),
            Space::new().width(Length::Fill),
            bindable(
                chrome::compact_button("Close").on_press(Message::CloseMachineDetails),
                BindTarget::global(Action::CloseMachineDetails),
            ),
        ]
        .padding([6, 8])
        .align_y(Alignment::Center);

        column![
            header,
            container(
                column![
                    chrome::label("Name"),
                    chrome::field(&name_placeholder, &machine.name)
                        .on_input(move |s| Message::MachineNameChanged(index, s))
                        .width(Length::Fill),
                    inspector_row(
                        "States",
                        stepper(
                            machine.num_states,
                            Message::DecStates,
                            Message::IncStates,
                            Some((
                                BindTarget::global(Action::DecStates),
                                BindTarget::global(Action::IncStates),
                            )),
                        ),
                    ),
                    chrome::dim(format!(
                        "State {}  ·  ({}, {})  ·  start ({}, {})",
                        machine.state,
                        machine.x_pos,
                        machine.y_pos,
                        machine.start_x,
                        machine.start_y
                    )),
                    chrome::label("Palette"),
                    color_picker::palette_controls(
                        &machine.palette,
                        &self.machine_color_picker,
                        self.program.num_symbols,
                    )
                    .map(move |msg| machine_palette_message(index, msg)),
                    chrome::label("Encoding"),
                    chrome::field("numStates,numSymbols,startX,startY,...", share)
                        .on_input(move |s| Message::ShareChanged(index, s))
                        .width(Length::Fill),
                    row![
                        bindable(
                            chrome::compact_button("Copy").on_press(Message::CopyShare(index)),
                            BindTarget::machine(Action::CopyShare, id),
                        ),
                        bindable(
                            chrome::compact_button("Load").on_press(Message::LoadShare(index)),
                            BindTarget::machine(Action::LoadShare, id),
                        ),
                        bindable(remove, BindTarget::machine(Action::RemoveMachine, id)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            )
            .padding(iced::Padding {
                top: 2.0,
                right: 8.0,
                bottom: 10.0,
                left: 8.0,
            }),
        ]
        .into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let n = self.program.machines.len();
        let stats = format!(
            "{n} machine{}  ·  {} itrs  ·  {}×{}  ·  {} Hz  ·  {}",
            if n == 1 { "" } else { "s" },
            self.program.itr_count,
            self.program.width,
            self.program.height,
            self.refresh_hz,
            self.raster_mode,
        );
        container(
            row![
                chrome::dim(stats),
                Space::new().width(Length::Fill),
                chrome::dim(self.status.as_str()),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([4, 10])
        .width(Length::Fill)
        .style(chrome::status_bar)
        .into()
    }

    fn preset_browser(&self) -> Element<'_, Message> {
        let mut list = column![].spacing(6);
        if self.preset_list.is_empty() {
            list = list.push(chrome::dim("No saved presets yet."));
        } else {
            for info in &self.preset_list {
                let name_load = info.name.clone();
                let name_delete = info.name.clone();
                list = list.push(
                    row![
                        column![
                            chrome::value(&info.name),
                            chrome::dim(format!(
                                "{} machine(s), {} states × {} symbols · {}",
                                info.num_machines,
                                info.states_label(),
                                info.num_symbols,
                                preset::format_saved_at(info.saved_at)
                            )),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        bindable(
                            chrome::compact_button("Load")
                                .on_press(Message::LoadPreset(name_load.clone())),
                            BindTarget::preset(Action::LoadPreset, name_load),
                        ),
                        bindable(
                            chrome::danger_button("Delete")
                                .on_press(Message::DeletePreset(name_delete.clone())),
                            BindTarget::preset(Action::DeletePreset, name_delete),
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }

        let panel = container(
            column![
                row![
                    chrome::value("Presets").size(16),
                    Space::new().width(Length::Fill),
                    bindable(
                        chrome::compact_button("Close").on_press(Message::ClosePresetBrowser),
                        BindTarget::global(Action::ClosePresetBrowser),
                    ),
                ]
                .align_y(Alignment::Center),
                chrome::dim("Store rules, starts, speeds, canvas size, and tape init. Load replaces the current machines."),
                row![
                    chrome::field("Preset name", &self.preset_name)
                        .on_input(Message::PresetNameChanged)
                        .width(Length::Fill),
                    bindable(
                        chrome::compact_button("Store").on_press(Message::StorePreset),
                        BindTarget::global(Action::StorePreset),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    chrome::label("Saved").width(Length::Fill),
                    chrome::dim("Sort"),
                    chrome::decorate_pick_list(
                        pick_list(
                            PresetSort::ALL,
                            Some(self.preset_sort),
                            Message::PresetSortChanged,
                        )
                        .width(140),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                overlay_scrollable(list),
                overlay_resize_grip(OverlayKind::Presets),
            ]
            .spacing(10)
            .height(Length::Fill),
        )
        .padding(16)
        .width(self.preset_overlay_size.width)
        .height(self.preset_overlay_size.height)
        .style(chrome::overlay_panel);

        opaque(
            mouse_area(center(opaque(panel)).style(chrome::scrim))
                .on_press(Message::ClosePresetBrowser),
        )
    }

    fn settings_overlay(&self) -> Element<'_, Message> {
        let defaults_tab = bindable(
            if self.settings_tab == SettingsTab::Defaults {
                chrome::accent_button("Defaults")
            } else {
                chrome::compact_button("Defaults")
            }
            .on_press(Message::SettingsTab(SettingsTab::Defaults)),
            BindTarget::global(Action::SettingsTabDefaults),
        );
        let keys_tab = bindable(
            if self.settings_tab == SettingsTab::Keybindings {
                chrome::accent_button("Keybindings")
            } else {
                chrome::compact_button("Keybindings")
            }
            .on_press(Message::SettingsTab(SettingsTab::Keybindings)),
            BindTarget::global(Action::SettingsTabKeybindings),
        );

        let body: Element<'_, Message> = match self.settings_tab {
            SettingsTab::Defaults => self.settings_defaults_tab(),
            SettingsTab::Keybindings => self.settings_keybindings_tab(),
        };

        let panel = container(
            column![
                row![
                    chrome::value("Settings").size(16),
                    Space::new().width(Length::Fill),
                    bindable(
                        chrome::compact_button("Close").on_press(Message::CloseSettings),
                        BindTarget::global(Action::CloseSettings),
                    ),
                ]
                .align_y(Alignment::Center),
                row![defaults_tab, keys_tab]
                    .spacing(6)
                    .align_y(Alignment::Center),
                overlay_scrollable(body),
                overlay_resize_grip(OverlayKind::Settings),
            ]
            .spacing(10)
            .height(Length::Fill),
        )
        .padding(16)
        .width(self.settings_overlay_size.width)
        .height(self.settings_overlay_size.height)
        .style(chrome::overlay_panel);

        opaque(
            mouse_area(center(opaque(panel)).style(chrome::scrim)).on_press(Message::CloseSettings),
        )
    }

    fn settings_defaults_tab(&self) -> Element<'_, Message> {
        let d = &self.settings.defaults;
        let mut palette_block = column![inspector_row(
            "Palette",
            chrome::decorate_pick_list(
                pick_list(
                    PaletteKind::ALL,
                    Some(d.palette_kind),
                    Message::SettingsPalette,
                )
                .width(Length::Fill),
            ),
        )]
        .spacing(6);
        if d.palette_kind == PaletteKind::Gradient {
            palette_block = palette_block
                .push(inspector_row(
                    "Start",
                    chrome::field("#RRGGBB", &self.settings_gradient_start)
                        .on_input(Message::SettingsGradientStart)
                        .width(Length::Fill),
                ))
                .push(inspector_row(
                    "End",
                    chrome::field("#RRGGBB", &self.settings_gradient_end)
                        .on_input(Message::SettingsGradientEnd)
                        .width(Length::Fill),
                ));
        }

        let mut canvas_init = column![inspector_row(
            "Init",
            chrome::decorate_pick_list(
                pick_list(
                    TapeInitKind::ALL,
                    Some(d.tape_kind),
                    Message::SettingsTapeKind,
                )
                .width(Length::Fill),
            ),
        )]
        .spacing(6);
        match d.tape_kind {
            TapeInitKind::Gaussian => {
                canvas_init = canvas_init
                    .push(inspector_row(
                        "Mean",
                        row![
                            chrome::param_slider(
                                MIN_GAUSSIAN_MEAN..=MAX_GAUSSIAN_MEAN,
                                d.gaussian_mean,
                                0.01_f32,
                                Message::SettingsGaussianMean,
                                Message::SettingsGaussianMean(DEFAULT_GAUSSIAN_MEAN),
                            ),
                            chrome::dim(format!("{:.2}", d.gaussian_mean))
                                .width(36)
                                .align_x(Alignment::End),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ))
                    .push(inspector_row(
                        "Sigma",
                        row![
                            chrome::param_slider(
                                MIN_GAUSSIAN_SIGMA..=MAX_GAUSSIAN_SIGMA,
                                d.gaussian_sigma,
                                0.01_f32,
                                Message::SettingsGaussianSigma,
                                Message::SettingsGaussianSigma(DEFAULT_GAUSSIAN_SIGMA),
                            ),
                            chrome::dim(format!("{:.2}", d.gaussian_sigma))
                                .width(36)
                                .align_x(Alignment::End),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ));
            }
            TapeInitKind::Perlin => {
                canvas_init = canvas_init
                    .push(inspector_row(
                        "Scale",
                        row![
                            chrome::param_slider(
                                MIN_PERLIN_SCALE..=MAX_PERLIN_SCALE,
                                d.perlin_scale,
                                1.0_f32,
                                Message::SettingsPerlinScale,
                                Message::SettingsPerlinScale(DEFAULT_PERLIN_SCALE),
                            ),
                            chrome::dim(format!("{:.0}", d.perlin_scale))
                                .width(36)
                                .align_x(Alignment::End),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ))
                    .push(inspector_row(
                        "Octaves",
                        row![
                            chrome::param_slider(
                                f32::from(MIN_PERLIN_OCTAVES)..=f32::from(MAX_PERLIN_OCTAVES),
                                f32::from(d.perlin_octaves),
                                1.0_f32,
                                Message::SettingsPerlinOctaves,
                                Message::SettingsPerlinOctaves(f32::from(DEFAULT_PERLIN_OCTAVES)),
                            ),
                            chrome::dim(d.perlin_octaves.to_string())
                                .width(36)
                                .align_x(Alignment::End),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    ));
            }
            TapeInitKind::Empty | TapeInitKind::Uniform => {}
        }

        column![
            chrome::dim("APPEARANCE"),
            inspector_row(
                "Theme",
                chrome::decorate_pick_list(
                    pick_list(
                        AtelierTheme::ALL,
                        Some(self.settings.theme),
                        Message::SettingsTheme,
                    )
                    .width(Length::Fill),
                ),
            ),
            chrome::hrule(),
            chrome::dim("CANVAS"),
            inspector_row(
                "Default states",
                stepper(
                    d.num_states,
                    Message::SettingsDecStates,
                    Message::SettingsIncStates,
                    None,
                ),
            ),
            inspector_row(
                "Symbols",
                stepper(
                    d.num_symbols,
                    Message::SettingsDecSymbols,
                    Message::SettingsIncSymbols,
                    None,
                ),
            ),
            inspector_row(
                "Diagonals",
                toggler(d.allow_diagonals).on_toggle(Message::SettingsAllowDiagonals),
            ),
            inspector_row(
                "Size",
                chrome::decorate_pick_list(
                    pick_list(
                        RESOLUTION_PRESETS,
                        RESOLUTION_PRESETS.iter().copied().find(|preset| {
                            preset.width == d.map_width && preset.height == d.map_height
                        }),
                        |preset: ResolutionPreset| {
                            Message::SettingsResolutionPreset(preset.width, preset.height)
                        },
                    )
                    .placeholder("Custom")
                    .width(Length::Fill),
                ),
            ),
            inspector_row(
                "",
                row![
                    chrome::field("W", &self.settings_width_text)
                        .on_input(Message::SettingsWidthText)
                        .width(Length::Fill),
                    chrome::dim("×").width(14).align_x(Alignment::Center),
                    chrome::field("H", &self.settings_height_text)
                        .on_input(Message::SettingsHeightText)
                        .width(Length::Fill),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            canvas_init,
            chrome::hrule(),
            chrome::dim("PALETTE"),
            palette_block,
            chrome::hrule(),
            chrome::dim("SIMULATION"),
            inspector_row(
                "Speed",
                row![
                    chrome::param_slider(
                        SIM_SPEED_MIN..=SIM_SPEED_MAX,
                        d.speed,
                        SIM_SPEED_STEP,
                        Message::SettingsSpeed,
                        Message::SettingsSpeed(1.0),
                    ),
                    chrome::compact_field("", &self.settings_speed_text)
                        .on_input(Message::SettingsSpeedText)
                        .width(48),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            inspector_row(
                "Refresh",
                chrome::decorate_pick_list(
                    pick_list(
                        REFRESH_PRESETS,
                        REFRESH_PRESETS
                            .iter()
                            .copied()
                            .find(|preset| preset.0 == d.refresh_hz),
                        |preset: RefreshPreset| Message::SettingsRefreshPreset(preset.0),
                    )
                    .placeholder("Custom")
                    .width(Length::Fill),
                ),
            ),
            inspector_row(
                "",
                row![
                    chrome::field("Hz", &self.settings_refresh_text)
                        .on_input(Message::SettingsRefreshText)
                        .width(Length::Fill),
                    chrome::dim("Hz").width(22),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            inspector_row(
                "Max itrs",
                row![
                    chrome::param_slider(
                        MIN_MAX_ITRS as f32..=MAX_MAX_ITRS as f32,
                        d.max_itrs as f32,
                        1_000.0_f32,
                        Message::SettingsMaxItrs,
                        Message::SettingsMaxItrs(DEFAULT_MAX_ITRS as f32),
                    ),
                    chrome::dim(d.max_itrs.to_string())
                        .width(64)
                        .align_x(Alignment::End),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            inspector_row(
                "Raster",
                chrome::decorate_pick_list(
                    pick_list(
                        RasterMode::ALL,
                        Some(d.raster_mode),
                        Message::SettingsRasterMode,
                    )
                    .width(Length::Fill),
                ),
            ),
        ]
        .spacing(6)
        .into()
    }

    fn settings_keybindings_tab(&self) -> Element<'_, Message> {
        let mut list = column![].spacing(8);
        for action in Action::GLOBAL {
            let target = BindTarget::global(action);
            let combo = self
                .binding_for(&target)
                .map(settings::Keybinding::display)
                .unwrap_or_else(|| "Not set".into());
            let capturing = self.capturing_action.as_ref() == Some(&target);
            let set_btn = if capturing {
                chrome::accent_button("Press a key…")
            } else {
                chrome::compact_button("Set")
            }
            .on_press(Message::CaptureBinding(target));
            list = list.push(
                row![
                    chrome::value(action.label()).width(Length::Fill),
                    chrome::dim(combo).width(140).align_x(Alignment::End),
                    set_btn,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        column![
            chrome::dim(
                "Global shortcuts only. Per-machine performance bindings are set from machine buttons and saved with presets."
            ),
            list,
        ]
        .spacing(10)
        .into()
    }

    fn button_controls_overlay(&self) -> Element<'_, Message> {
        let Some(target) = &self.button_controls else {
            return Space::new().into();
        };
        let title = self.target_label(target);
        let combo = self
            .binding_for(target)
            .map(settings::Keybinding::display)
            .unwrap_or_else(|| "Not set".into());
        let capturing = self.capturing_action.as_ref() == Some(target);
        let set_btn = if capturing {
            chrome::accent_button("Press a key…")
        } else {
            chrome::compact_button("Set")
        }
        .on_press(Message::CaptureBinding(target.clone()));
        let bound = self.binding_for(target).is_some();
        let hint = if target.is_performance() {
            "Performance bindings are stored when you save a preset."
        } else {
            "Press a key to bind. Return closes after binding; Escape cancels."
        };
        let mut clear = chrome::danger_button("Clear");
        if bound {
            clear = clear.on_press(Message::ClearBinding);
        }

        let panel = container(
            column![
                row![
                    chrome::value(title).size(16),
                    Space::new().width(Length::Fill),
                    chrome::compact_button("Close").on_press(Message::CloseButtonControls),
                ]
                .align_y(Alignment::Center),
                chrome::dim(if target.is_performance() {
                    "Performance binding"
                } else {
                    "Current shortcut"
                }),
                chrome::value(combo),
                chrome::dim(hint),
                row![set_btn, clear].spacing(8).align_y(Alignment::Center),
            ]
            .spacing(10),
        )
        .padding(16)
        .width(360)
        .style(chrome::overlay_panel);

        opaque(
            mouse_area(center(opaque(panel)).style(chrome::scrim))
                .on_press(Message::CloseButtonControls),
        )
    }
}

fn bindable<'a>(
    button: iced::widget::button::Button<'a, Message>,
    target: BindTarget,
) -> Element<'a, Message> {
    mouse_area(button)
        .on_right_press(Message::OpenButtonControls(target))
        .into()
}

fn overlay_scrollable<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    atelier_ui::layout::themed_scrollable(content)
}

fn overlay_resize_grip(kind: OverlayKind) -> Element<'static, Message> {
    row![
        Space::new().width(Length::Fill),
        mouse_area(chrome::resize_grip())
            .on_press(Message::OverlayResizeStart(kind))
            .interaction(mouse::Interaction::ResizingDiagonallyDown),
    ]
    .into()
}

fn collapsible<'a>(
    title: &'a str,
    open: bool,
    toggle: Message,
    bind: BindTarget,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let chevron = if open { "▾" } else { "▸" };
    let header = bindable(
        chrome::header_button(
            row![chrome::dim(chevron), chrome::dim(title)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .on_press(toggle),
        bind,
    );

    let mut col = column![header];
    if open {
        col = col.push(container(body).padding(iced::Padding {
            top: 2.0,
            right: 8.0,
            bottom: 10.0,
            left: 8.0,
        }));
    }
    col.push(chrome::hrule()).into()
}

fn inspector_row<'a>(
    label: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    atelier_ui::layout::inspector_row(label, content)
}

fn stepper(
    value: usize,
    dec: Message,
    inc: Message,
    binds: Option<(BindTarget, BindTarget)>,
) -> Element<'static, Message> {
    let minus = chrome::compact_button("−").on_press(dec);
    let plus = chrome::compact_button("+").on_press(inc);
    let (minus, plus): (Element<'static, Message>, Element<'static, Message>) = match binds {
        Some((dec_bind, inc_bind)) => (bindable(minus, dec_bind), bindable(plus, inc_bind)),
        None => (minus.into(), plus.into()),
    };
    row![
        minus,
        chrome::value(value.to_string())
            .width(28)
            .align_x(Alignment::Center),
        plus,
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn machine_palette_message(index: usize, msg: ControlsMessage) -> Message {
    match msg {
        ControlsMessage::KindSelected(kind) => Message::MachinePaletteSelected(index, kind),
        ControlsMessage::GradientStartChanged(hex) => {
            Message::MachineGradientStartChanged(index, hex)
        }
        ControlsMessage::GradientEndChanged(hex) => Message::MachineGradientEndChanged(index, hex),
        ControlsMessage::Picker(color_picker::Message::BindClose) => {
            Message::OpenButtonControls(BindTarget::global(Action::CloseColorPicker))
        }
        ControlsMessage::Picker(m) => Message::MachineColorPicker(index, m),
    }
}

fn machine_active_toggler(index: usize, active: bool) -> Element<'static, Message> {
    toggler(active)
        .on_toggle(move |on| Message::ToggleMachineActive(index, on))
        .into()
}

fn machine_speed_slider<'a>(
    index: usize,
    speed: f32,
    speed_text: &'a str,
    schedule_mode: ScheduleMode,
    scheduled_rate: f64,
) -> Element<'a, Message> {
    let readout = match schedule_mode {
        ScheduleMode::Absolute => format!("×{:.2}", step_rate(speed)),
        ScheduleMode::Normalised => {
            if scheduled_rate <= 0.0 {
                "—".into()
            } else {
                format!("{:.0}%", scheduled_rate * 100.0)
            }
        }
    };
    row![
        chrome::param_slider(
            MIN_MACHINE_SPEED..=MAX_MACHINE_SPEED,
            speed,
            MACHINE_SPEED_STEP,
            move |v| Message::MachineSpeedChanged(index, v),
            Message::MachineSpeedChanged(index, 0.0),
        ),
        chrome::compact_field("", speed_text)
            .on_input(move |s| Message::MachineSpeedText(index, s))
            .width(48),
        chrome::dim(readout).width(40).align_x(Alignment::End),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_period_is_inverse_of_hz() {
        assert_eq!(frame_period(60), Duration::from_secs_f64(1.0 / 60.0));
        assert_eq!(frame_period(0), frame_period(MIN_REFRESH_HZ));
        assert_eq!(frame_period(1_000), frame_period(MAX_REFRESH_HZ));
    }

    #[test]
    fn parse_refresh_hz_clamps_and_rejects_invalid() {
        assert_eq!(parse_refresh_hz("60").unwrap(), 60);
        assert_eq!(parse_refresh_hz(" 75 ").unwrap(), 75);
        assert_eq!(parse_refresh_hz("0").unwrap(), MIN_REFRESH_HZ);
        assert_eq!(parse_refresh_hz("300").unwrap(), MAX_REFRESH_HZ);
        assert!(parse_refresh_hz("").is_err());
        assert!(parse_refresh_hz("abc").is_err());
    }

    #[test]
    fn parse_map_dim_accepts_in_range_and_rejects_invalid() {
        assert_eq!(parse_map_dim("512", "Width").unwrap(), 512);
        assert_eq!(parse_map_dim(" 1920 ", "Width").unwrap(), 1920);
        assert_eq!(parse_map_dim("64", "Height").unwrap(), MIN_MAP_SIZE);
        assert_eq!(parse_map_dim("4096", "Height").unwrap(), MAX_MAP_SIZE);
        assert!(parse_map_dim("", "Width").is_err());
        assert!(parse_map_dim("abc", "Width").is_err());
        assert!(parse_map_dim("63", "Width").is_err());
        assert!(parse_map_dim("4097", "Height").is_err());
        assert!(parse_map_dim("0", "Width").is_err());
    }

    #[test]
    fn parse_clamped_f32_clamps_and_rejects_invalid() {
        assert_eq!(parse_clamped_f32("0.5", 0.0, 1.0).unwrap(), 0.5);
        assert_eq!(parse_clamped_f32(" 1.25 ", 0.0, 1.0).unwrap(), 1.0);
        assert_eq!(parse_clamped_f32("-3", -10.0, 10.0).unwrap(), -3.0);
        assert_eq!(parse_clamped_f32("15", -10.0, 10.0).unwrap(), 10.0);
        assert_eq!(parse_clamped_f32("+2.50", -10.0, 10.0).unwrap(), 2.5);
        assert!(parse_clamped_f32("", 0.0, 1.0).is_err());
        assert!(parse_clamped_f32("abc", 0.0, 1.0).is_err());
        assert!(parse_clamped_f32("inf", 0.0, 1.0).is_err());
        assert!(parse_clamped_f32("nan", 0.0, 1.0).is_err());
    }
}
