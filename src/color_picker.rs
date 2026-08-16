//! Palette sidebar controls and the gradient colour-picker panel.

use iced::widget::{
    button, canvas, column, container, mouse_area, pick_list, row, scrollable, slider, text,
    text_input, Space,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};
use iced_color_wheel::{color_to_hsv, hsv_to_color, WheelProgram};

use crate::palette::{parse_hex_rgb, rgb_to_hex, Palette, PaletteKind, Rgb};

/// Which gradient colour is being edited in the colour wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientEndpoint {
    Start,
    End,
}

#[derive(Debug, Clone, Copy)]
pub enum RgbChannel {
    Red,
    Green,
    Blue,
}

#[derive(Debug, Clone, Copy)]
pub enum HsvChannel {
    Hue,
    Saturation,
    Value,
}

/// Messages produced by the colour-picker panel and its swatches.
#[derive(Debug, Clone)]
pub enum Message {
    Toggle(GradientEndpoint),
    Close,
    HueSatChanged(f32, f32),
    HsvChanged(HsvChannel, f32),
    RgbChanged(RgbChannel, f32),
    HexChanged(String),
}

/// Messages from the palette sidebar (kind, gradient hex fields, and picker).
#[derive(Debug, Clone)]
pub enum ControlsMessage {
    KindSelected(PaletteKind),
    GradientStartChanged(String),
    GradientEndChanged(String),
    Picker(Message),
}

/// HSV / hex state for the open colour picker.
#[derive(Debug, Clone)]
pub struct ColorPicker {
    open: Option<GradientEndpoint>,
    hue: f32,
    saturation: f32,
    value: f32,
    hex: String,
}

impl ColorPicker {
    pub fn new() -> Self {
        Self {
            open: None,
            hue: 0.0,
            saturation: 0.0,
            value: 1.0,
            hex: "#000000".into(),
        }
    }

    pub fn close(&mut self) {
        self.open = None;
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn open_endpoint(&self) -> Option<GradientEndpoint> {
        self.open
    }

    pub fn sync_from_rgb(&mut self, rgb: Rgb) {
        let (h, s, v) = color_to_hsv(rgb_color(rgb));
        self.hue = h;
        self.saturation = s;
        self.value = v;
        self.hex = rgb_to_hex(rgb);
    }

    /// Apply a picker message. `Ok(true)` means the palette changed and the
    /// drawing frame should be rebuilt. `Err` is a parse error for the status line.
    pub fn update(
        &mut self,
        message: Message,
        palette: &mut Palette,
        num_symbols: usize,
    ) -> Result<bool, String> {
        match message {
            Message::Toggle(endpoint) => {
                if self.open == Some(endpoint) {
                    self.open = None;
                } else {
                    let rgb = match endpoint {
                        GradientEndpoint::Start => palette.gradient_start,
                        GradientEndpoint::End => palette.gradient_end,
                    };
                    self.sync_from_rgb(rgb);
                    self.open = Some(endpoint);
                }
                Ok(false)
            }
            Message::Close => {
                self.open = None;
                Ok(false)
            }
            Message::HueSatChanged(h, s) => {
                self.hue = h;
                self.saturation = s;
                self.apply_from_hsv(palette, num_symbols);
                Ok(true)
            }
            Message::HsvChanged(channel, value) => {
                match channel {
                    HsvChannel::Hue => self.hue = value.rem_euclid(360.0),
                    HsvChannel::Saturation => self.saturation = value.clamp(0.0, 1.0),
                    HsvChannel::Value => self.value = value.clamp(0.0, 1.0),
                }
                self.apply_from_hsv(palette, num_symbols);
                Ok(true)
            }
            Message::RgbChanged(channel, value) => {
                let mut rgb = self.rgb();
                let v = value.round().clamp(0.0, 255.0) as u8;
                match channel {
                    RgbChannel::Red => rgb[0] = v,
                    RgbChannel::Green => rgb[1] = v,
                    RgbChannel::Blue => rgb[2] = v,
                }
                self.sync_from_rgb(rgb);
                self.apply_rgb(rgb, palette, num_symbols);
                Ok(true)
            }
            Message::HexChanged(hex) => {
                self.hex = hex;
                match parse_hex_rgb(&self.hex) {
                    Ok(rgb) => {
                        self.sync_from_rgb(rgb);
                        self.apply_rgb(rgb, palette, num_symbols);
                        Ok(true)
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    fn rgb(&self) -> Rgb {
        let color = hsv_to_color(self.hue, self.saturation, self.value);
        [
            (color.r * 255.0).round() as u8,
            (color.g * 255.0).round() as u8,
            (color.b * 255.0).round() as u8,
        ]
    }

    fn apply_from_hsv(&mut self, palette: &mut Palette, num_symbols: usize) {
        let rgb = self.rgb();
        self.hex = rgb_to_hex(rgb);
        self.apply_rgb(rgb, palette, num_symbols);
    }

    fn apply_rgb(&self, rgb: Rgb, palette: &mut Palette, num_symbols: usize) {
        let Some(endpoint) = self.open else {
            return;
        };
        match endpoint {
            GradientEndpoint::Start => palette.set_gradient_start_rgb(rgb, num_symbols),
            GradientEndpoint::End => palette.set_gradient_end_rgb(rgb, num_symbols),
        }
    }

    fn panel(&self, endpoint: GradientEndpoint) -> Element<'_, Message> {
        let label = match endpoint {
            GradientEndpoint::Start => "Start colour",
            GradientEndpoint::End => "End colour",
        };
        let rgb = self.rgb();

        let panel = column![
            row![
                text(label).size(13),
                Space::new().width(Length::Fill),
                button("Close").on_press(Message::Close),
            ]
            .align_y(Alignment::Center),
            container(
                canvas(WheelProgram::new(
                    self.hue,
                    self.saturation,
                    self.value,
                    Message::HueSatChanged,
                ))
                .width(200)
                .height(200),
            )
            .center_x(Length::Fill),
            container(Space::new().height(20))
                .width(Length::Fill)
                .style(move |_theme: &Theme| container::Style {
                    background: Some(Background::Color(rgb_color(rgb))),
                    border: Border {
                        color: Color::from_rgb(0.4, 0.4, 0.4),
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    ..container::Style::default()
                }),
            column![
                text("HSV").size(13),
                channel_slider(
                    "H",
                    0.0..=360.0,
                    self.hue,
                    1.0,
                    format!("{:.0}°", self.hue),
                    |v| Message::HsvChanged(HsvChannel::Hue, v),
                ),
                channel_slider(
                    "S",
                    0.0..=1.0,
                    self.saturation,
                    0.01,
                    format!("{:.0}%", self.saturation * 100.0),
                    |v| Message::HsvChanged(HsvChannel::Saturation, v),
                ),
                channel_slider(
                    "V",
                    0.0..=1.0,
                    self.value,
                    0.01,
                    format!("{:.0}%", self.value * 100.0),
                    |v| Message::HsvChanged(HsvChannel::Value, v),
                ),
            ]
            .spacing(4),
            column![
                text("RGB").size(13),
                channel_slider(
                    "R",
                    0.0..=255.0,
                    f32::from(rgb[0]),
                    1.0,
                    rgb[0].to_string(),
                    |v| Message::RgbChanged(RgbChannel::Red, v),
                ),
                channel_slider(
                    "G",
                    0.0..=255.0,
                    f32::from(rgb[1]),
                    1.0,
                    rgb[1].to_string(),
                    |v| Message::RgbChanged(RgbChannel::Green, v),
                ),
                channel_slider(
                    "B",
                    0.0..=255.0,
                    f32::from(rgb[2]),
                    1.0,
                    rgb[2].to_string(),
                    |v| Message::RgbChanged(RgbChannel::Blue, v),
                ),
            ]
            .spacing(4),
            row![
                text("Hex:").width(36),
                text_input("#RRGGBB", &self.hex)
                    .on_input(Message::HexChanged)
                    .width(Length::Fill),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8);

        scrollable(
            container(panel)
                .padding(8)
                .style(|_theme: &Theme| container::Style {
                    background: Some(Background::Color(Color::from_rgb(0.12, 0.12, 0.12))),
                    border: Border {
                        color: Color::from_rgb(0.35, 0.35, 0.35),
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..container::Style::default()
                }),
        )
        .height(Length::Fixed(420.0))
        .into()
    }
}

/// Kind pick list, swatches, gradient editors, and optional picker panel.
pub fn palette_controls<'a>(
    palette: &'a Palette,
    picker: &'a ColorPicker,
) -> Element<'a, ControlsMessage> {
    let mut controls = column![
        row![
            text("Palette:").width(110),
            pick_list(
                PaletteKind::ALL,
                Some(palette.kind),
                ControlsMessage::KindSelected,
            )
            .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(6);

    let mut swatches = row![].spacing(4);
    for &c in &palette.colors {
        swatches = swatches.push(color_swatch(c, 28.0, 18.0));
    }
    controls = controls.push(swatches);

    if palette.kind == PaletteKind::Gradient {
        controls = controls
            .push(
                row![
                    text("Start:").width(110),
                    clickable_color_swatch(
                        palette.gradient_start,
                        28.0,
                        22.0,
                        ControlsMessage::Picker(Message::Toggle(GradientEndpoint::Start)),
                    ),
                    text_input("#RRGGBB", &palette.gradient_start_hex)
                        .on_input(ControlsMessage::GradientStartChanged)
                        .width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .push(
                row![
                    text("End:").width(110),
                    clickable_color_swatch(
                        palette.gradient_end,
                        28.0,
                        22.0,
                        ControlsMessage::Picker(Message::Toggle(GradientEndpoint::End)),
                    ),
                    text_input("#RRGGBB", &palette.gradient_end_hex)
                        .on_input(ControlsMessage::GradientEndChanged)
                        .width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );

        if let Some(endpoint) = picker.open_endpoint() {
            controls = controls.push(picker.panel(endpoint).map(ControlsMessage::Picker));
        }
    }

    controls.into()
}

fn rgb_color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb[0], rgb[1], rgb[2])
}

fn color_swatch<'a, Message: 'a>(rgb: Rgb, width: f32, height: f32) -> Element<'a, Message> {
    container(Space::new().width(width).height(height))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(rgb_color(rgb))),
            border: Border {
                color: Color::from_rgb(0.4, 0.4, 0.4),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn clickable_color_swatch<'a>(
    rgb: Rgb,
    width: f32,
    height: f32,
    on_press: ControlsMessage,
) -> Element<'a, ControlsMessage> {
    mouse_area(color_swatch(rgb, width, height))
        .on_press(on_press)
        .into()
}

fn channel_slider<'a>(
    label: &'a str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    step: f32,
    value_label: String,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    row![
        text(label).width(18),
        slider(range, value, on_change).step(step),
        text(value_label).width(48).align_x(Alignment::End),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}
