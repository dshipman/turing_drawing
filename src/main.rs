mod program;

use std::time::{Duration, Instant};

use iced::widget::{
    button, column, container, image, row, scrollable, text, text_input, Space,
};
use iced::widget::image::{FilterMethod, Handle};
use iced::{
    clipboard, time, Alignment, Background, Border, Color, Element, Length, Subscription, Task,
    Theme,
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
        .window_size((920.0, 640.0))
        .run()
}

fn theme(_app: &App) -> Theme {
    Theme::Dark
}

struct App {
    program: Program,
    num_states: usize,
    num_symbols: usize,
    share_text: String,
    status: String,
    /// Cached RGBA frame; rebuilt when the map changes.
    pixels: Vec<u8>,
    frame: Handle,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    IncStates,
    DecStates,
    IncSymbols,
    DecSymbols,
    Random,
    Restart,
    ShareChanged(String),
    CopyShare,
    LoadShare,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let num_states = 4;
        let num_symbols = 3;
        let program = Program::new_random(num_states, num_symbols);
        let share_text = program.to_string();
        let pixels = rgba_from_map(&program.map);
        let frame = Handle::from_rgba(MAP_WIDTH as u32, MAP_HEIGHT as u32, pixels.clone());

        (
            Self {
                program,
                num_states,
                num_symbols,
                share_text,
                status: String::new(),
                pixels,
                frame,
            },
            Task::none(),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        time::every(UPDATE_TIME).map(|_| Message::Tick)
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
            Message::Random => {
                self.program = Program::new_random(self.num_states, self.num_symbols);
                self.share_text = self.program.to_string();
                self.status = format!(
                    "New machine: {} states, {} symbols",
                    self.num_states, self.num_symbols
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
            Message::ShareChanged(s) => {
                self.share_text = s;
                Task::none()
            }
            Message::CopyShare => {
                self.status = "Copied encoding to clipboard".into();
                clipboard::write(self.share_text.clone())
            }
            Message::LoadShare => match Program::from_string(&self.share_text) {
                Ok(p) => {
                    self.num_states = p.num_states;
                    self.num_symbols = p.num_symbols;
                    self.program = p;
                    self.share_text = self.program.to_string();
                    self.status = "Loaded encoding".into();
                    self.refresh_frame();
                    Task::none()
                }
                Err(e) => {
                    self.status = format!("Load failed: {e}");
                    Task::none()
                }
            },
        }
    }

    fn run_frame(&mut self) {
        let start = Instant::now();
        let start_itr = self.program.itr_count;

        loop {
            self.program.update(CHUNK);
            if self.program.itr_count - start_itr >= UPDATE_ITRS || start.elapsed() >= UPDATE_TIME {
                break;
            }
        }

        self.refresh_frame();
    }

    fn refresh_frame(&mut self) {
        fill_rgba_from_map(&self.program.map, &mut self.pixels);
        self.frame = Handle::from_rgba(MAP_WIDTH as u32, MAP_HEIGHT as u32, self.pixels.clone());
    }

    fn view(&self) -> Element<'_, Message> {
        let canvas = container(
            image(self.frame.clone())
                .width(MAP_WIDTH as f32)
                .height(MAP_HEIGHT as f32)
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
            Space::new().height(8),
            row![
                button("Random").on_press(Message::Random),
                button("Restart").on_press(Message::Restart),
            ]
            .spacing(10),
            Space::new().height(12),
            scrollable(
                text(
                    "Turing Drawings uses randomly generated Turing machines \
                     to produce drawings on a canvas, as a form of generative art. \
                     Machines operate on a finite 2D grid; each cell holds a symbol \
                     (a color). Press Random for a new machine, Restart to clear the \
                     grid. Copy the encoding below to share a machine, or paste one \
                     (including original website #hashes) and press Load.",
                )
                .size(14),
            )
            .height(160),
            Space::new().height(8),
            text(format!("Iterations: {}", self.program.itr_count)).size(13),
            text(&self.status).size(13),
        ]
        .spacing(6)
        .width(340)
        .padding(8);

        let share_row = column![
            text("Shareable encoding for this drawing:").size(14),
            row![
                text_input("numStates,numSymbols,...", &self.share_text)
                    .on_input(Message::ShareChanged)
                    .width(Length::Fill),
                button("Copy").on_press(Message::CopyShare),
                button("Load").on_press(Message::LoadShare),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(6)
        .width(Length::Fill);

        let body = row![canvas, controls].spacing(16).align_y(Alignment::Start);

        container(
            column![body, Space::new().height(12), share_row]
                .spacing(4)
                .padding(16),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::BLACK)),
            text_color: Some(Color::WHITE),
            ..container::Style::default()
        })
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
