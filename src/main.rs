use std::time::{Duration, Instant};

use iced::keyboard::key;
use iced::widget::image::Handle;
use iced::widget::{
    center, column, container, mouse_area, opaque, pick_list, row, scrollable, slider, stack,
    toggler, Space,
};
use iced::{
    clipboard, event, keyboard, mouse, time, window, Alignment, Element, Event, Length, Size,
    Subscription, Task, Theme,
};

use turing_drawing::chrome;
use turing_drawing::color_picker::{self, ColorPicker, ControlsMessage, GradientEndpoint};
use turing_drawing::drawing::{self, simulation_frame};
use turing_drawing::gpu_raster::{self, RasterMode};
use turing_drawing::palette::{fill_rgba_from_map, rgba_from_map, Palette, PaletteKind};
use turing_drawing::preset::{self, PresetInfo, PresetSort};
use turing_drawing::program::{
    default_machine_name, remap_index_after_reorder, step_rate, Program, DEFAULT_MAP_HEIGHT,
    DEFAULT_MAP_WIDTH, DEFAULT_MUTATE_PERCENT, MAX_MACHINE_SPEED, MAX_MAP_SIZE, MAX_MUTATE_PERCENT,
    MAX_STATES, MAX_SYMBOLS, MIN_MACHINE_SPEED, MIN_MAP_SIZE, MIN_MUTATE_PERCENT, MIN_STATES,
    MIN_SYMBOLS,
};

const DEFAULT_REFRESH_HZ: u32 = 60;
const MIN_REFRESH_HZ: u32 = 1;
const MAX_REFRESH_HZ: u32 = 240;
const REFRESH_PRESETS: [RefreshPreset; 6] = [
    RefreshPreset(30),
    RefreshPreset(60),
    RefreshPreset(120),
    RefreshPreset(144),
    RefreshPreset(165),
    RefreshPreset(240),
];
const DEFAULT_MAX_ITRS: u64 = 350_000;
const MIN_MAX_ITRS: u64 = 1_000;
const MAX_MAX_ITRS: u64 = 2_000_000;
const CHUNK: usize = 5_000;
/// Resume this long after the last resize during a fullscreen transition.
const MODE_SWITCH_SETTLE: Duration = Duration::from_millis(100);
/// Fallback cap if no resize events arrive during a mode switch.
const MODE_SWITCH_PAUSE_MAX: Duration = Duration::from_millis(800);
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
    iced::application(App::new, App::update, App::view)
        .title("Turing Drawings")
        .theme(theme)
        .subscription(App::subscription)
        .window(window::Settings {
            size: Size::new(1100.0, 720.0),
            min_size: Some(Size::new(900.0, 560.0)),
            ..Default::default()
        })
        .run()
}

fn theme(_app: &App) -> Theme {
    Theme::Dark
}

fn on_event(event: Event, status: event::Status, _id: window::Id) -> Option<Message> {
    if let Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) = event {
        return Some(Message::MachineDragEnd);
    }
    if status == event::Status::Captured {
        return None;
    }
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named),
            ..
        }) => match named {
            key::Named::F11 => Some(Message::ToggleFullscreen),
            key::Named::Escape => Some(Message::Escape),
            _ => None,
        },
        _ => None,
    }
}

struct App {
    program: Program,
    num_states: usize,
    num_symbols: usize,
    /// Fraction of `max_itrs` to run each tick (`0.0` = paused, `1.0` = max).
    speed: f32,
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
    status: String,
    /// Cached RGBA frame; rebuilt when the map changes.
    pixels: Vec<u8>,
    frame: Handle,
    /// When true, only the drawing is shown (controls and share encodings hidden).
    drawing_only: bool,
    palette: Palette,
    color_picker: ColorPicker,
    /// Machine whose inspector section is open.
    selected_machine: Option<usize>,
    /// Machine currently being dragged in the left pool.
    dragging_machine: Option<usize>,
    /// Whether the preset browser overlay is open.
    preset_browser_open: bool,
    /// Name field for storing a new/overwrite preset.
    preset_name: String,
    /// Cached list of presets on disk (refreshed when the browser opens or changes).
    preset_list: Vec<PresetInfo>,
    /// Order of the preset browser list.
    preset_sort: PresetSort,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectorGroup {
    Canvas,
    Palette,
    Simulation,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    IncStates,
    DecStates,
    IncSymbols,
    DecSymbols,
    SpeedChanged(f32),
    RefreshPreset(u32),
    RefreshHzText(String),
    ResolutionPreset(usize, usize),
    MapWidthText(String),
    MapHeightText(String),
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
    MachineDragEnd,
    MachineSpeedChanged(usize, f32),
    ToggleMachineActive(usize, bool),
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
    WindowResized,
    Escape,
    PaletteSelected(PaletteKind),
    GradientStartChanged(String),
    GradientEndChanged(String),
    ColorPicker(color_picker::Message),
    OpenPresetBrowser,
    ClosePresetBrowser,
    PresetNameChanged(String),
    StorePreset,
    LoadPreset(String),
    DeletePreset(String),
    PresetSortChanged(PresetSort),
    ToggleInspectorGroup(InspectorGroup),
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let num_states = 4;
        let num_symbols = 3;
        let program = Program::new_random(num_states, num_symbols);
        let share_texts = vec![program.machine_encoding(0)];
        let palette = Palette::classic();
        let pixels = rgba_from_map(&program.map, &palette.colors);
        let frame = Handle::from_rgba(program.width as u32, program.height as u32, pixels.clone());

        (
            Self {
                program,
                num_states,
                num_symbols,
                speed: 1.0,
                refresh_hz: DEFAULT_REFRESH_HZ,
                refresh_hz_text: DEFAULT_REFRESH_HZ.to_string(),
                map_width_text: DEFAULT_MAP_WIDTH.to_string(),
                map_height_text: DEFAULT_MAP_HEIGHT.to_string(),
                max_itrs: DEFAULT_MAX_ITRS,
                share_texts,
                status: String::new(),
                pixels,
                frame,
                drawing_only: false,
                palette,
                color_picker: ColorPicker::new(),
                selected_machine: None,
                dragging_machine: None,
                preset_browser_open: false,
                preset_name: String::new(),
                preset_list: Vec::new(),
                preset_sort: PresetSort::DateSaved,
                canvas_open: true,
                palette_open: true,
                simulation_open: true,
                mode_switch_resume_at: None,
                raster_mode: RasterMode::Gpu,
                mutate_percent: f32::from(DEFAULT_MUTATE_PERCENT),
                picking_start: None,
                suppress_drawing_only: false,
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
            window::resize_events().map(|(_id, _size)| Message::WindowResized),
        ])
    }

    fn sync_share_texts(&mut self) {
        self.share_texts = (0..self.program.machines.len())
            .map(|i| self.program.machine_encoding(i))
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

    fn apply_loaded_program(&mut self, program: Program, label: &str) {
        self.program = program;
        self.num_states = self.program.num_states;
        self.num_symbols = self.program.num_symbols;
        self.sync_resolution_text();
        self.palette.resolve(self.num_symbols);
        self.selected_machine = None;
        self.dragging_machine = None;
        self.picking_start = None;
        self.sync_share_texts();
        self.status = label.to_string();
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
        match message {
            Message::Tick => {
                self.run_frame();
                Task::none()
            }
            Message::IncStates => {
                if self.num_states < MAX_STATES {
                    self.num_states += 1;
                }
                Task::none()
            }
            Message::DecStates => {
                if self.num_states > MIN_STATES {
                    self.num_states -= 1;
                }
                Task::none()
            }
            Message::IncSymbols => {
                if self.num_symbols < MAX_SYMBOLS {
                    self.num_symbols += 1;
                    self.palette.resolve(self.num_symbols);
                    self.refresh_frame();
                }
                Task::none()
            }
            Message::DecSymbols => {
                if self.num_symbols > MIN_SYMBOLS {
                    self.num_symbols -= 1;
                    self.palette.resolve(self.num_symbols);
                    self.refresh_frame();
                }
                Task::none()
            }
            Message::SpeedChanged(speed) => {
                self.speed = speed.clamp(0.0, 1.0);
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
                self.program.randomize(self.num_states, self.num_symbols);
                self.sync_share_texts();
                self.status = format!(
                    "New machines: {} machine(s), {} states, {} symbols",
                    self.program.machines.len(),
                    self.num_states,
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
                self.program.add_machine();
                self.sync_share_texts();
                self.status = format!(
                    "Added machine ({} total); program reset",
                    self.program.machines.len()
                );
                self.refresh_frame();
                Task::none()
            }
            Message::RemoveMachine(i) => match self.program.remove_machine(i) {
                Ok(()) => {
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
            },
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
                Task::none()
            }
            Message::CloseMachineDetails => {
                self.selected_machine = None;
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
            Message::MachineDragEnd => {
                self.dragging_machine = None;
                Task::none()
            }
            Message::MachineSpeedChanged(i, speed) => {
                match self.program.set_machine_speed(i, speed) {
                    Ok(()) => Task::none(),
                    Err(e) => {
                        self.status = e;
                        Task::none()
                    }
                }
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
                        self.num_states = self.program.num_states;
                        self.num_symbols = self.program.num_symbols;
                        self.palette.resolve(self.num_symbols);
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
            Message::WindowResized => {
                self.note_mode_switch_resize();
                Task::none()
            }
            Message::Escape => {
                if self.picking_start.is_some() {
                    self.picking_start = None;
                    self.status = "Cancelled start placement".into();
                    return Task::none();
                }
                if self.preset_browser_open {
                    self.preset_browser_open = false;
                    return Task::none();
                }
                if self.dragging_machine.is_some() {
                    self.dragging_machine = None;
                    return Task::none();
                }
                if self.selected_machine.is_some() {
                    self.selected_machine = None;
                    return Task::none();
                }
                if self.color_picker.is_open() {
                    self.color_picker.close();
                    return Task::none();
                }
                self.drawing_only = false;
                Self::exit_fullscreen_if_needed()
            }
            Message::PaletteSelected(kind) => {
                self.palette.set_kind(kind, self.num_symbols);
                if kind != PaletteKind::Gradient {
                    self.color_picker.close();
                }
                self.status = format!("Palette: {kind}");
                self.refresh_frame();
                Task::none()
            }
            Message::GradientStartChanged(hex) => {
                match self.palette.set_gradient_start_hex(hex, self.num_symbols) {
                    Ok(()) => {
                        self.status.clear();
                        if self.color_picker.open_endpoint() == Some(GradientEndpoint::Start) {
                            self.color_picker.sync_from_rgb(self.palette.gradient_start);
                        }
                        self.refresh_frame();
                    }
                    Err(e) => {
                        self.status = format!("Start colour: {e}");
                    }
                }
                Task::none()
            }
            Message::GradientEndChanged(hex) => {
                match self.palette.set_gradient_end_hex(hex, self.num_symbols) {
                    Ok(()) => {
                        self.status.clear();
                        if self.color_picker.open_endpoint() == Some(GradientEndpoint::End) {
                            self.color_picker.sync_from_rgb(self.palette.gradient_end);
                        }
                        self.refresh_frame();
                    }
                    Err(e) => {
                        self.status = format!("End colour: {e}");
                    }
                }
                Task::none()
            }
            Message::ColorPicker(msg) => {
                match self
                    .color_picker
                    .update(msg, &mut self.palette, self.num_symbols)
                {
                    Ok(true) => {
                        self.status.clear();
                        self.refresh_frame();
                    }
                    Ok(false) => {}
                    Err(e) => {
                        self.status = format!("Picker colour: {e}");
                    }
                }
                Task::none()
            }
            Message::OpenPresetBrowser => {
                self.selected_machine = None;
                self.preset_browser_open = true;
                self.refresh_preset_list();
                Task::none()
            }
            Message::ClosePresetBrowser => {
                self.preset_browser_open = false;
                Task::none()
            }
            Message::PresetNameChanged(name) => {
                self.preset_name = name;
                Task::none()
            }
            Message::StorePreset => {
                match self.program.to_preset(&self.preset_name) {
                    Ok(preset) => match preset::save_preset(&preset) {
                        Ok(name) => {
                            self.status = format!("Stored preset \"{name}\"");
                            self.refresh_preset_list();
                        }
                        Err(e) => self.status = format!("Store failed: {e}"),
                    },
                    Err(e) => self.status = format!("Store failed: {e}"),
                }
                Task::none()
            }
            Message::LoadPreset(name) => match preset::load_preset(&name) {
                Ok(preset) => match Program::from_preset(&preset) {
                    Ok(program) => {
                        let n = program.machines.len();
                        self.apply_loaded_program(
                            program,
                            &format!("Loaded preset \"{name}\" ({n} machine(s))"),
                        );
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
            self.program.update(chunk);
        }

        self.refresh_frame();
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
        if self.raster_mode != RasterMode::Cpu {
            return;
        }
        let needed = self.program.map.len() * 4;
        if self.pixels.len() != needed {
            self.pixels.resize(needed, 0);
        }
        fill_rgba_from_map(&self.program.map, &mut self.pixels, &self.palette.colors);
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
            RasterMode::Gpu => gpu_raster::map_shader(
                &self.program.map,
                self.program.width as u32,
                self.program.height as u32,
                &self.palette.colors,
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

        if self.preset_browser_open {
            return stack([content.into(), self.preset_browser()])
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        content.into()
    }

    fn toolbar(&self) -> Element<'_, Message> {
        container(
            row![
                chrome::compact_button("Random").on_press(Message::Random),
                chrome::compact_button("Mutate").on_press(Message::Mutate),
                chrome::dim("Mutate %"),
                slider(
                    f32::from(MIN_MUTATE_PERCENT)..=f32::from(MAX_MUTATE_PERCENT),
                    self.mutate_percent,
                    Message::MutatePercent,
                )
                .step(1.0_f32)
                .width(100)
                .style(chrome::slider_style),
                chrome::dim(format!("{:.0}%", self.mutate_percent)).width(36),
                chrome::compact_button("Restart").on_press(Message::Restart),
                chrome::compact_button("Presets").on_press(Message::OpenPresetBrowser),
                Space::new().width(Length::Fill),
                chrome::compact_button("Fullscreen").on_press(Message::ToggleFullscreen),
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

            let picking = self.picking_start == Some(i);
            let set_start = if picking {
                chrome::accent_button("Set start")
            } else {
                chrome::compact_button("Set start")
            }
            .on_press(Message::TogglePickStart(i));
            let actions = row![
                chrome::compact_button("Randomise").on_press(Message::RandomizeMachine(i)),
                chrome::compact_button("Mutate").on_press(Message::MutateMachine(i)),
                set_start,
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            list = list.push(
                mouse_area(
                    container(column![title, actions, machine_speed_slider(i, speed)].spacing(4))
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
                container(chrome::compact_button("Add machine").on_press(Message::AddMachine),)
                    .padding(iced::Padding {
                        top: 4.0,
                        right: 8.0,
                        bottom: 4.0,
                        left: 0.0,
                    }),
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill),
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

    fn inspector(&self) -> Element<'_, Message> {
        let groups = column![]
            .push(collapsible(
                "CANVAS",
                self.canvas_open,
                Message::ToggleInspectorGroup(InspectorGroup::Canvas),
                self.canvas_group(),
            ))
            .push(collapsible(
                "PALETTE",
                self.palette_open,
                Message::ToggleInspectorGroup(InspectorGroup::Palette),
                self.palette_group(),
            ))
            .push(collapsible(
                "SIMULATION",
                self.simulation_open,
                Message::ToggleInspectorGroup(InspectorGroup::Simulation),
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
        column![
            inspector_row(
                "States",
                stepper(self.num_states, Message::DecStates, Message::IncStates),
            ),
            inspector_row(
                "Symbols",
                stepper(self.num_symbols, Message::DecSymbols, Message::IncSymbols,),
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
        ]
        .spacing(6)
        .into()
    }

    fn palette_group(&self) -> Element<'_, Message> {
        color_picker::palette_controls(&self.palette, &self.color_picker, self.num_symbols).map(
            |msg| match msg {
                ControlsMessage::KindSelected(kind) => Message::PaletteSelected(kind),
                ControlsMessage::GradientStartChanged(hex) => Message::GradientStartChanged(hex),
                ControlsMessage::GradientEndChanged(hex) => Message::GradientEndChanged(hex),
                ControlsMessage::Picker(m) => Message::ColorPicker(m),
            },
        )
    }

    fn simulation_group(&self) -> Element<'_, Message> {
        column![
            inspector_row(
                "Speed",
                row![
                    slider(0.0..=1.0, self.speed, Message::SpeedChanged)
                        .step(0.01_f32)
                        .style(chrome::slider_style),
                    chrome::dim(format!("{:.2}", self.speed))
                        .width(36)
                        .align_x(Alignment::End),
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
                    slider(
                        MIN_MAX_ITRS as f32..=MAX_MAX_ITRS as f32,
                        self.max_itrs as f32,
                        Message::MaxItrsChanged,
                    )
                    .step(1_000.0_f32)
                    .style(chrome::slider_style),
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

        let mut remove = chrome::danger_button("Remove");
        if can_remove {
            remove = remove.on_press(Message::RemoveMachine(index));
        }

        let header = row![
            chrome::dim(machine.display_name(index)),
            Space::new().width(Length::Fill),
            chrome::compact_button("Close").on_press(Message::CloseMachineDetails),
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
                    chrome::dim(format!(
                        "State {}  ·  ({}, {})  ·  start ({}, {})",
                        machine.state,
                        machine.x_pos,
                        machine.y_pos,
                        machine.start_x,
                        machine.start_y
                    )),
                    chrome::label("Encoding"),
                    chrome::field("numStates,numSymbols,startX,startY,...", share)
                        .on_input(move |s| Message::ShareChanged(index, s))
                        .width(Length::Fill),
                    row![
                        chrome::compact_button("Copy").on_press(Message::CopyShare(index)),
                        chrome::compact_button("Load").on_press(Message::LoadShare(index)),
                        remove,
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
                                info.num_states,
                                info.num_symbols,
                                preset::format_saved_at(info.saved_at)
                            )),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        chrome::compact_button("Load").on_press(Message::LoadPreset(name_load)),
                        chrome::danger_button("Delete")
                            .on_press(Message::DeletePreset(name_delete)),
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
                    chrome::compact_button("Close").on_press(Message::ClosePresetBrowser),
                ]
                .align_y(Alignment::Center),
                chrome::dim("Store rules, starts, speeds, and canvas size. Load replaces the current machines."),
                row![
                    chrome::field("Preset name", &self.preset_name)
                        .on_input(Message::PresetNameChanged)
                        .width(Length::Fill),
                    chrome::compact_button("Store").on_press(Message::StorePreset),
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
                scrollable(list)
                    .style(chrome::scrollable_style)
                    .height(Length::Fixed(280.0))
                    .width(Length::Fill),
            ]
            .spacing(10),
        )
        .padding(16)
        .width(520)
        .style(chrome::overlay_panel);

        opaque(
            mouse_area(center(opaque(panel)).style(chrome::scrim))
                .on_press(Message::ClosePresetBrowser),
        )
    }
}

fn collapsible<'a>(
    title: &'a str,
    open: bool,
    toggle: Message,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let chevron = if open { "▾" } else { "▸" };
    let header = chrome::header_button(
        row![chrome::dim(chevron), chrome::dim(title)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .on_press(toggle);

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
    row![
        chrome::label(label).width(chrome::LABEL_WIDTH),
        content.into(),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn stepper(value: usize, dec: Message, inc: Message) -> Element<'static, Message> {
    row![
        chrome::compact_button("−").on_press(dec),
        chrome::value(value.to_string())
            .width(28)
            .align_x(Alignment::Center),
        chrome::compact_button("+").on_press(inc),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn machine_active_toggler(index: usize, active: bool) -> Element<'static, Message> {
    toggler(active)
        .on_toggle(move |on| Message::ToggleMachineActive(index, on))
        .into()
}

fn machine_speed_slider(index: usize, speed: f32) -> Element<'static, Message> {
    row![
        slider(MIN_MACHINE_SPEED..=MAX_MACHINE_SPEED, speed, move |v| {
            Message::MachineSpeedChanged(index, v)
        })
        .step(0.1_f32)
        .style(chrome::slider_style),
        chrome::dim(machine_speed_label(speed))
            .width(72)
            .align_x(Alignment::End),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn machine_speed_label(speed: f32) -> String {
    let rate = step_rate(speed);
    if speed.abs() < 1e-4 {
        "0.0  ×1.00".into()
    } else {
        format!("{speed:+.1}  ×{rate:.2}")
    }
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
}
