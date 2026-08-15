mod machine;
mod program;

use std::time::{Duration, Instant};

use iced::keyboard::key;
use iced::widget::{
    button, column, container, image, mouse_area, row, scrollable, slider, text, text_input, Space,
};
use iced::widget::image::{FilterMethod, Handle};
use iced::{
    clipboard, event, keyboard, time, window, Alignment, Background, Border, Color, ContentFit,
    Element, Event, Length, Size, Subscription, Task, Theme,
};

use program::{Program, MAP_HEIGHT, MAP_WIDTH, MAX_STATES, MAX_SYMBOLS, MIN_STATES, MIN_SYMBOLS};

/// RGB triples matching the original `colorMap` (symbol index → color).
const COLOR_MAP: [[u8; 3]; 8] = [
    [255, 0, 0],     // Initial symbol (untouched)
    [0, 0, 0],       // Black
    [255, 255, 255], // White
    [0, 255, 0],     // Green
    [0, 0, 255],     // Blue
    [255, 255, 0],   // Yellow
    [0, 255, 255],   // Cyan
    [255, 0, 255],   // Magenta
];

const UPDATE_TIME: Duration = Duration::from_millis(40);
const UPDATE_ITRS: u64 = 350_000;
const CHUNK: usize = 5_000;

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
    /// Fraction of `UPDATE_ITRS` to run each tick (`0.0` = paused, `1.0` = max).
    speed: f32,
    share_texts: Vec<String>,
    status: String,
    /// Cached RGBA frame; rebuilt when the map changes.
    pixels: Vec<u8>,
    frame: Handle,
    /// When true, only the drawing is shown (controls and share encodings hidden).
    drawing_only: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    IncStates,
    DecStates,
    IncSymbols,
    DecSymbols,
    SpeedChanged(f32),
    Random,
    Restart,
    AddMachine,
    RemoveMachine(usize),
    ShareChanged(usize, String),
    CopyShare(usize),
    LoadShare(usize),
    ToggleDrawingOnly,
    ToggleFullscreen,
    Escape,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let num_states = 4;
        let num_symbols = 3;
        let program = Program::new_random(num_states, num_symbols);
        let share_texts = vec![program.machine_encoding(0)];
        let pixels = rgba_from_map(&program.map);
        let frame = Handle::from_rgba(MAP_WIDTH as u32, MAP_HEIGHT as u32, pixels.clone());

        (
            Self {
                program,
                num_states,
                num_symbols,
                speed: 1.0,
                share_texts,
                status: String::new(),
                pixels,
                frame,
                drawing_only: false,
            },
            Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            time::every(UPDATE_TIME).map(|_| Message::Tick),
            event::listen_with(on_event),
        ])
    }

    fn sync_share_texts(&mut self) {
        self.share_texts = (0..self.program.machines.len())
            .map(|i| self.program.machine_encoding(i))
            .collect();
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
                }
                Task::none()
            }
            Message::DecSymbols => {
                if self.num_symbols > MIN_SYMBOLS {
                    self.num_symbols -= 1;
                }
                Task::none()
            }
            Message::SpeedChanged(speed) => {
                self.speed = speed.clamp(0.0, 1.0);
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
                self.drawing_only = false;
                Self::exit_fullscreen_if_needed()
            }
        }
    }

    fn run_frame(&mut self) {
        let max_itrs = (UPDATE_ITRS as f64 * f64::from(self.speed)) as u64;
        if max_itrs == 0 {
            return;
        }

        let start = Instant::now();
        let start_itr = self.program.itr_count;

        loop {
            let remaining = max_itrs.saturating_sub(self.program.itr_count - start_itr);
            if remaining == 0 || start.elapsed() >= UPDATE_TIME {
                break;
            }
            let chunk = CHUNK.min(remaining as usize);
            self.program.update(chunk);
        }

        self.refresh_frame();
    }

    fn refresh_frame(&mut self) {
        fill_rgba_from_map(&self.program.map, &mut self.pixels);
        self.frame = Handle::from_rgba(MAP_WIDTH as u32, MAP_HEIGHT as u32, self.pixels.clone());
    }

    fn drawing_canvas(&self) -> Element<'_, Message> {
        let framed = container(
            image(self.frame.clone())
                .expand(true)
                .content_fit(ContentFit::Contain)
                .filter_method(FilterMethod::Nearest),
        )
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

        let canvas = self.drawing_canvas();

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
                text("Speed:").width(110),
                slider(0.0..=1.0, self.speed, Message::SpeedChanged).step(0.01_f32),
                text(format!("{:.2}", self.speed))
                    .width(40)
                    .align_x(Alignment::Center),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            Space::new().height(8),
            row![
                button("Random").on_press(Message::Random),
                button("Restart").on_press(Message::Restart),
                button("Add machine").on_press(Message::AddMachine),
                button("Fullscreen").on_press(Message::ToggleFullscreen),
            ]
            .spacing(10),
            Space::new().height(12),
            scrollable(
                text(
                    "Turing Drawings uses randomly generated Turing machines \
                     to produce drawings on a canvas, as a form of generative art. \
                     Machines share one finite 2D grid; each cell holds a symbol \
                     (a color). They must use the same number of states and symbols, \
                     but each has its own rules and start position. Press Random to \
                     regenerate every machine, Add machine to add another (this \
                     resets the drawing), or Restart to clear the grid. Each machine \
                     has an encoding below; original website #hashes load with start (0,0). \
                     Double-click the drawing to hide controls; F11 or Fullscreen for \
                     OS fullscreen.",
                )
                .size(14),
            )
            .height(140),
            Space::new().height(8),
            text(format!(
                "Machines: {}   Iterations: {}",
                self.program.machines.len(),
                self.program.itr_count
            ))
            .size(13),
            text(&self.status).size(13),
        ]
        .spacing(6)
        .width(380)
        .padding(8);

        let mut machine_rows = column![text("Shareable encodings:").size(14)].spacing(8);

        for (i, share) in self.share_texts.iter().enumerate() {
            let mut remove = button("Remove");
            if can_remove {
                remove = remove.on_press(Message::RemoveMachine(i));
            }

            machine_rows = machine_rows.push(
                column![
                    text(format!("Machine {}", i + 1)).size(13),
                    row![
                        text_input("numStates,numSymbols,startX,startY,...", share)
                            .on_input(move |s| Message::ShareChanged(i, s))
                            .width(Length::Fill),
                        button("Copy").on_press(Message::CopyShare(i)),
                        button("Load").on_press(Message::LoadShare(i)),
                        remove,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(4),
            );
        }

        let share_list = scrollable(machine_rows)
            .height(Length::Fixed(160.0))
            .width(Length::Fill);

        let body = row![canvas, controls]
            .spacing(16)
            .align_y(Alignment::Start)
            .width(Length::Fill)
            .height(Length::Fill);

        container(
            column![body, Space::new().height(12), share_list]
                .spacing(4)
                .padding(16)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(Self::root_style())
        .into()
    }
}

fn rgba_from_map(map: &[i32]) -> Vec<u8> {
    let mut pixels = vec![0u8; map.len() * 4];
    fill_rgba_from_map(map, &mut pixels);
    pixels
}

fn fill_rgba_from_map(map: &[i32], pixels: &mut [u8]) {
    debug_assert_eq!(pixels.len(), map.len() * 4);
    for (i, &sy) in map.iter().enumerate() {
        let c = COLOR_MAP[sy as usize];
        let o = i * 4;
        pixels[o] = c[0];
        pixels[o + 1] = c[1];
        pixels[o + 2] = c[2];
        pixels[o + 3] = 255;
    }
}
