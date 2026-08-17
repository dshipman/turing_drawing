mod color_picker;
mod drawing;
mod machine;
mod palette;
mod preset;
mod program;

use std::time::{Duration, Instant};

use iced::keyboard::key;
use iced::widget::image::Handle;
use iced::widget::{
    button, center, column, container, mouse_area, opaque, pick_list, row, scrollable,
    slider, stack, text, text_input, Space,
};
use iced::{
    clipboard, event, keyboard, time, window, Alignment, Background, Border, Color,
    Element, Event, Length, Size, Subscription, Task, Theme,
};

use color_picker::{ColorPicker, ControlsMessage, GradientEndpoint};
use drawing::simulation_frame;
use palette::{Palette, PaletteKind, Rgb};
use preset::{PresetInfo, PresetSort};
use program::{
    step_rate, Program, DEFAULT_MAP_HEIGHT, DEFAULT_MAP_WIDTH, MAX_MACHINE_SPEED, MAX_MAP_SIZE,
    MAX_STATES, MAX_SYMBOLS, MIN_MACHINE_SPEED, MIN_MAP_SIZE, MIN_STATES, MIN_SYMBOLS,
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
            size: Size::new(960.0, 720.0),
            min_size: Some(Size::new(640.0, 480.0)),
            ..Default::default()
        })
        .run()
}

fn theme(_app: &App) -> Theme {
    Theme::Dark
}

fn on_event(event: Event, status: event::Status, _id: window::Id) -> Option<Message> {
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
    /// Machine whose details overlay is open.
    selected_machine: Option<usize>,
    /// Whether the preset browser overlay is open.
    preset_browser_open: bool,
    /// Name field for storing a new/overwrite preset.
    preset_name: String,
    /// Cached list of presets on disk (refreshed when the browser opens or changes).
    preset_list: Vec<PresetInfo>,
    /// Order of the preset browser list.
    preset_sort: PresetSort,
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
    Random,
    Restart,
    AddMachine,
    RemoveMachine(usize),
    RandomizeMachine(usize),
    SelectMachine(usize),
    CloseMachineDetails,
    MachineSpeedChanged(usize, f32),
    ShareChanged(usize, String),
    CopyShare(usize),
    LoadShare(usize),
    ToggleDrawingOnly,
    ToggleFullscreen,
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
                preset_browser_open: false,
                preset_name: String::new(),
                preset_list: Vec::new(),
                preset_sort: PresetSort::DateSaved,
            },
            Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            time::every(frame_period(self.refresh_hz)).map(|_| Message::Tick),
            event::listen_with(on_event),
        ])
    }

    fn sync_share_texts(&mut self) {
        self.share_texts = (0..self.program.machines.len())
            .map(|i| self.program.machine_encoding(i))
            .collect();
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
                window::set_mode(id, next)
            })
        })
    }

    fn exit_fullscreen_if_needed() -> Task<Message> {
        window::latest().and_then(|id| {
            window::mode(id).then(move |mode| {
                if mode == window::Mode::Fullscreen {
                    window::set_mode(id, window::Mode::Windowed)
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
                    self.status = format!("Randomised machine {}; program reset", i + 1);
                    self.refresh_frame();
                    Task::none()
                }
                Err(e) => {
                    self.status = e;
                    Task::none()
                }
            },
            Message::SelectMachine(i) => {
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
            Message::MachineSpeedChanged(i, speed) => {
                match self.program.set_machine_speed(i, speed) {
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
                self.status = format!("Copied machine {} encoding to clipboard", i + 1);
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
                        self.status = format!("Loaded encoding for machine {}", i + 1);
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
                self.drawing_only = !self.drawing_only;
                Task::none()
            }
            Message::ToggleFullscreen => Self::toggle_fullscreen(),
            Message::Escape => {
                if self.preset_browser_open {
                    self.preset_browser_open = false;
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
        }
    }

    fn run_frame(&mut self) {
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

    fn refresh_frame(&mut self) {
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
    }

    fn drawing_canvas(&self) -> Element<'_, Message> {
        let framed = container(simulation_frame(self.frame.clone()))
        .padding(0)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::BLACK)),
            border: Border {
                color: Color::WHITE,
                width: 2.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });

        container(mouse_area(framed).on_double_click(Message::ToggleDrawingOnly))
            .center(Length::Fill)
            .into()
    }

    fn root_style() -> impl Fn(&Theme) -> container::Style {
        |_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::BLACK)),
            text_color: Some(Color::WHITE),
            ..container::Style::default()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        if self.drawing_only {
            return container(self.drawing_canvas())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(Self::root_style())
                .into();
        }

        let drawing = self.drawing_canvas();

        let can_remove = self.program.machines.len() > 1;

        let controls = column![
            text("Turing Drawings").size(28),
            Space::new().height(8),
            row![
                text("Num states:").width(110),
                button("−").on_press(Message::DecStates),
                text(self.num_states.to_string())
                    .width(36)
                    .align_x(Alignment::Center),
                button("+").on_press(Message::IncStates),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text("Num symbols:").width(110),
                button("−").on_press(Message::DecSymbols),
                text(self.num_symbols.to_string())
                    .width(36)
                    .align_x(Alignment::Center),
                button("+").on_press(Message::IncSymbols),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text("Resolution:").width(110),
                pick_list(
                    RESOLUTION_PRESETS,
                    RESOLUTION_PRESETS.iter().copied().find(|preset| {
                        preset.width == self.program.width && preset.height == self.program.height
                    }),
                    |preset: ResolutionPreset| {
                        Message::ResolutionPreset(preset.width, preset.height)
                    },
                )
                .placeholder("Custom")
                .width(Length::Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                Space::new().width(110),
                text_input("W", &self.map_width_text)
                    .on_input(Message::MapWidthText)
                    .width(Length::Fill),
                text("×").width(16),
                text_input("H", &self.map_height_text)
                    .on_input(Message::MapHeightText)
                    .width(Length::Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text("Speed:").width(110),
                slider(0.0..=1.0, self.speed, Message::SpeedChanged).step(0.01_f32),
                text(format!("{:.2}", self.speed))
                    .width(40)
                    .align_x(Alignment::Center),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text("Refresh rate:").width(110),
                pick_list(
                    REFRESH_PRESETS,
                    REFRESH_PRESETS
                        .iter()
                        .copied()
                        .find(|preset| preset.0 == self.refresh_hz),
                    |preset: RefreshPreset| Message::RefreshPreset(preset.0),
                )
                .placeholder("Custom")
                .width(100),
                text_input("Hz", &self.refresh_hz_text)
                    .on_input(Message::RefreshHzText)
                    .width(Length::Fill),
                text("Hz").width(24),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text("Max itrs/frame:").width(110),
                slider(
                    MIN_MAX_ITRS as f32..=MAX_MAX_ITRS as f32,
                    self.max_itrs as f32,
                    Message::MaxItrsChanged,
                )
                .step(1_000.0_f32),
                text(self.max_itrs.to_string())
                    .width(72)
                    .align_x(Alignment::Center),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            color_picker::palette_controls(&self.palette, &self.color_picker).map(
                |msg| match msg {
                    ControlsMessage::KindSelected(kind) => Message::PaletteSelected(kind),
                    ControlsMessage::GradientStartChanged(hex) =>
                        Message::GradientStartChanged(hex),
                    ControlsMessage::GradientEndChanged(hex) => Message::GradientEndChanged(hex),
                    ControlsMessage::Picker(m) => Message::ColorPicker(m),
                }
            ),
        ]
        .spacing(6)
        .width(380)
        .padding(8);

        let controls = controls
            .push(Space::new().height(8))
            .push(
                row![
                    button("Random").on_press(Message::Random),
                    button("Restart").on_press(Message::Restart),
                    button("Add machine").on_press(Message::AddMachine),
                    button("Presets").on_press(Message::OpenPresetBrowser),
                    button("Fullscreen").on_press(Message::ToggleFullscreen),
                ]
                .spacing(10),
            )
            .push(Space::new().height(12))
            .push(
                scrollable(
                    text(
                        "Turing Drawings uses randomly generated Turing machines \
                         to produce drawings on a canvas, as a form of generative art. \
                         Machines share one finite 2D grid; each cell holds a symbol \
                         (a color). They must use the same number of states and symbols, \
                         but each has its own rules and start position. Press Random to \
                         regenerate every machine, or Randomise on a machine to regenerate \
                         only that one. Add machine to add another (this resets the \
                         drawing), or Restart to clear the grid. Use Presets to store or \
                         load the starting setup of all machines. Resolution sets the \
                         drawing grid (default 512 × 512); choose a preset or type a \
                         custom width and height (64–4096). Changing resolution clears \
                         the drawing. Each machine has its own \
                         Speed slider (0 is the default rate; frequency is 10^(speed/10), \
                         so +10 is ten times more often and −10 ten times less). \
                         Refresh rate sets how often the simulation ticks (default 60 Hz); \
                         choose a preset or type a custom Hz. Max itrs/frame is the round \
                         cap each tick at Speed 1 (default 350000); Speed is a fraction of \
                         that cap, and work also yields when the frame's time budget is \
                         spent. Click \
                         a machine for its details \
                         and shareable encoding; original website #hashes load \
                         with start (0,0). \
                         Double-click the drawing to hide controls; F11 or Fullscreen for \
                         OS fullscreen. Choose a palette to recolor the drawing; Gradient \
                         lets you pick start and end colours (click a swatch for the \
                         full colour picker: wheel, HSV, RGB, hex).",
                    )
                    .size(14),
                )
                .height(140),
            )
            .push(Space::new().height(8))
            .push(
                text(format!(
                    "Machines: {}   Iterations: {}",
                    self.program.machines.len(),
                    self.program.itr_count
                ))
                .size(13),
            )
            .push(text(&self.status).size(13));

        let mut machine_rows = column![text("Machines:").size(14)].spacing(6);

        for i in 0..self.program.machines.len() {
            let selected = self.selected_machine == Some(i);
            let speed = self.program.machines[i].speed;
            machine_rows = machine_rows.push(
                row![
                    mouse_area(
                        container(text(format!("Machine {}", i + 1)).size(14))
                            .padding([6, 10])
                            .width(120)
                            .style(machine_name_style(selected)),
                    )
                    .on_press(Message::SelectMachine(i)),
                    machine_speed_slider(i, speed),
                    button("Randomise").on_press(Message::RandomizeMachine(i)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        let machine_list = scrollable(machine_rows)
            .height(Length::Fixed(120.0))
            .width(Length::Fill);

        let body = row![drawing, controls]
            .spacing(16)
            .align_y(Alignment::Start)
            .width(Length::Fill)
            .height(Length::Fill);

        let content = container(
            column![body, Space::new().height(12), machine_list]
                .spacing(4)
                .padding(16)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(Self::root_style());

        if self.preset_browser_open {
            return stack([content.into(), self.preset_browser()])
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        if let Some(index) = self.selected_machine {
            if index < self.program.machines.len() {
                return stack([content.into(), self.machine_details(index, can_remove)])
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
            }
        }

        content.into()
    }

    fn preset_browser(&self) -> Element<'_, Message> {
        let mut list = column![].spacing(6);
        if self.preset_list.is_empty() {
            list = list.push(text("No saved presets yet.").size(13));
        } else {
            for info in &self.preset_list {
                let name_load = info.name.clone();
                let name_delete = info.name.clone();
                list = list.push(
                    row![
                        column![
                            text(&info.name).size(14),
                            text(format!(
                                "{} machine(s), {} states × {} symbols · {}",
                                info.num_machines,
                                info.num_states,
                                info.num_symbols,
                                preset::format_saved_at(info.saved_at)
                            ))
                            .size(12),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        button("Load").on_press(Message::LoadPreset(name_load)),
                        button("Delete").on_press(Message::DeletePreset(name_delete)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }

        let panel = container(
            column![
                row![
                    text("Presets").size(20),
                    Space::new().width(Length::Fill),
                    button("Close").on_press(Message::ClosePresetBrowser),
                ]
                .align_y(Alignment::Center),
                text(
                    "Store the starting setup of all machines (rules, starts, speeds, \
                     canvas size). Loading replaces the current machines and clears the drawing."
                )
                .size(13),
                row![
                    text_input("Preset name", &self.preset_name)
                        .on_input(Message::PresetNameChanged)
                        .width(Length::Fill),
                    button("Store").on_press(Message::StorePreset),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text("Saved presets:").size(14).width(Length::Fill),
                    text("Sort:").size(13),
                    pick_list(
                        PresetSort::ALL,
                        Some(self.preset_sort),
                        Message::PresetSortChanged,
                    )
                    .width(140),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                scrollable(list)
                    .height(Length::Fixed(280.0))
                    .width(Length::Fill),
            ]
            .spacing(10),
        )
        .padding(16)
        .width(560)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.12, 0.12, 0.12))),
            border: Border {
                color: Color::from_rgb(0.45, 0.45, 0.45),
                width: 1.0,
                radius: 6.0.into(),
            },
            text_color: Some(Color::WHITE),
            ..container::Style::default()
        });

        opaque(
            mouse_area(
                center(opaque(panel)).style(|_theme: &Theme| container::Style {
                    background: Some(Background::Color(Color {
                        a: 0.65,
                        ..Color::BLACK
                    })),
                    ..container::Style::default()
                }),
            )
            .on_press(Message::ClosePresetBrowser),
        )
    }

    fn machine_details(&self, index: usize, can_remove: bool) -> Element<'_, Message> {
        let machine = &self.program.machines[index];
        let share = self
            .share_texts
            .get(index)
            .map(String::as_str)
            .unwrap_or("");

        let mut remove = button("Remove");
        if can_remove {
            remove = remove.on_press(Message::RemoveMachine(index));
        }

        let panel = container(
            column![
                row![
                    text(format!("Machine {} details", index + 1)).size(20),
                    Space::new().width(Length::Fill),
                    button("Close").on_press(Message::CloseMachineDetails),
                ]
                .align_y(Alignment::Center),
                text(format!(
                    "State: {}    Position: ({}, {})    Start: ({}, {})",
                    machine.state, machine.x_pos, machine.y_pos, machine.start_x, machine.start_y
                ))
                .size(13),
                row![
                    text("Speed:").width(60),
                    machine_speed_slider(index, machine.speed),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("Shareable encoding:").size(14),
                text_input("numStates,numSymbols,startX,startY,...", share)
                    .on_input(move |s| Message::ShareChanged(index, s))
                    .width(Length::Fill),
                row![
                    button("Copy").on_press(Message::CopyShare(index)),
                    button("Load").on_press(Message::LoadShare(index)),
                    button("Randomise").on_press(Message::RandomizeMachine(index)),
                    remove,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(10),
        )
        .padding(16)
        .width(560)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.12, 0.12, 0.12))),
            border: Border {
                color: Color::from_rgb(0.45, 0.45, 0.45),
                width: 1.0,
                radius: 6.0.into(),
            },
            text_color: Some(Color::WHITE),
            ..container::Style::default()
        });

        opaque(
            mouse_area(
                center(opaque(panel)).style(|_theme: &Theme| container::Style {
                    background: Some(Background::Color(Color {
                        a: 0.65,
                        ..Color::BLACK
                    })),
                    ..container::Style::default()
                }),
            )
            .on_press(Message::CloseMachineDetails),
        )
    }
}

fn machine_speed_slider(index: usize, speed: f32) -> Element<'static, Message> {
    row![
        slider(MIN_MACHINE_SPEED..=MAX_MACHINE_SPEED, speed, move |v| {
            Message::MachineSpeedChanged(index, v)
        },)
        .step(0.1_f32),
        text(machine_speed_label(speed))
            .width(88)
            .align_x(Alignment::Center),
    ]
    .spacing(8)
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

fn machine_name_style(selected: bool) -> impl Fn(&Theme) -> container::Style {
    move |_theme: &Theme| container::Style {
        background: Some(Background::Color(if selected {
            Color::from_rgb(0.22, 0.22, 0.28)
        } else {
            Color::from_rgb(0.10, 0.10, 0.10)
        })),
        border: Border {
            color: if selected {
                Color::from_rgb(0.55, 0.55, 0.65)
            } else {
                Color::from_rgb(0.28, 0.28, 0.28)
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

fn rgba_from_map(map: &[i32], colors: &[Rgb; MAX_SYMBOLS]) -> Vec<u8> {
    let mut pixels = vec![0u8; map.len() * 4];
    fill_rgba_from_map(map, &mut pixels, colors);
    pixels
}

fn fill_rgba_from_map(map: &[i32], pixels: &mut [u8], colors: &[Rgb; MAX_SYMBOLS]) {
    debug_assert_eq!(pixels.len(), map.len() * 4);
    for (i, &sy) in map.iter().enumerate() {
        let c = colors[sy as usize];
        let o = i * 4;
        pixels[o] = c[0];
        pixels[o + 1] = c[1];
        pixels[o + 2] = c[2];
        pixels[o + 3] = 255;
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
