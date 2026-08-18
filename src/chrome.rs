//! Shared dark, dense chrome for the Lightroom / Resolve-style layout.

use iced::widget::text::IntoFragment;
use iced::widget::{
    button, container, overlay, pick_list, rule, scrollable, slider, text, text_input,
};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

pub const WINDOW: Color = Color::from_rgb8(0x14, 0x14, 0x14);
pub const PANEL: Color = Color::from_rgb8(0x1E, 0x1E, 0x1E);
pub const TOOLBAR: Color = Color::from_rgb8(0x2A, 0x2A, 0x2A);
pub const INPUT: Color = Color::from_rgb8(0x16, 0x16, 0x16);
pub const BUTTON: Color = Color::from_rgb8(0x33, 0x33, 0x33);
pub const BUTTON_HOVER: Color = Color::from_rgb8(0x3D, 0x3D, 0x3D);
pub const BUTTON_PRESS: Color = Color::from_rgb8(0x2C, 0x2C, 0x2C);
pub const RULE: Color = Color::from_rgb8(0x3A, 0x3A, 0x3A);
pub const TEXT: Color = Color::from_rgb8(0xE0, 0xE0, 0xE0);
pub const TEXT_DIM: Color = Color::from_rgb8(0x9A, 0x9A, 0x9A);
pub const ACCENT: Color = Color::from_rgb8(0xE8, 0x7A, 0x2E);
pub const DANGER: Color = Color::from_rgb8(0xC0, 0x4A, 0x3A);
pub const SCRIM: Color = Color {
    a: 0.65,
    ..Color::BLACK
};

pub const FONT_BODY: f32 = 12.0;
pub const FONT_SMALL: f32 = 11.0;
pub const LABEL_WIDTH: f32 = 72.0;
pub const LEFT_PANEL: f32 = 248.0;
pub const RIGHT_PANEL: f32 = 288.0;
pub const RADIUS: f32 = 2.0;

const BTN_PAD: [f32; 2] = [4.0, 8.0];
const FIELD_PAD: [f32; 2] = [3.0, 6.0];

pub fn window(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(WINDOW)),
        text_color: Some(TEXT),
        ..container::Style::default()
    }
}

pub fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        text_color: Some(TEXT),
        ..container::Style::default()
    }
}

pub fn toolbar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(TOOLBAR)),
        text_color: Some(TEXT),
        border: Border {
            color: RULE,
            width: 0.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn status_bar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(TOOLBAR)),
        text_color: Some(TEXT_DIM),
        ..container::Style::default()
    }
}

pub fn viewer(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::BLACK)),
        ..container::Style::default()
    }
}

pub fn overlay_panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        border: Border {
            color: RULE,
            width: 1.0,
            radius: RADIUS.into(),
        },
        text_color: Some(TEXT),
        ..container::Style::default()
    }
}

pub fn scrim(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SCRIM)),
        ..container::Style::default()
    }
}

pub fn picker_panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(WINDOW)),
        border: Border {
            color: RULE,
            width: 1.0,
            radius: RADIUS.into(),
        },
        ..container::Style::default()
    }
}

pub fn machine_card(
    selected: bool,
    active: bool,
    dragging: bool,
) -> impl Fn(&Theme) -> container::Style {
    move |_theme: &Theme| container::Style {
        background: Some(Background::Color(if dragging {
            Color::from_rgb8(0x32, 0x2A, 0x20)
        } else if !active {
            Color::from_rgb8(0x14, 0x14, 0x14)
        } else if selected {
            Color::from_rgb8(0x2A, 0x24, 0x1C)
        } else {
            Color::from_rgb8(0x18, 0x18, 0x18)
        })),
        border: Border {
            color: if dragging || selected {
                ACCENT
            } else if !active {
                Color::from_rgb8(0x2E, 0x2E, 0x2E)
            } else {
                RULE
            },
            width: if dragging { 2.0 } else { 1.0 },
            radius: RADIUS.into(),
        },
        ..container::Style::default()
    }
}

pub fn drag_handle<'a>() -> text::Text<'a> {
    text("::").size(FONT_BODY).color(TEXT_DIM)
}

pub fn swatch_border(_theme: &Theme) -> Border {
    Border {
        color: RULE,
        width: 1.0,
        radius: RADIUS.into(),
    }
}

pub fn label<'a>(content: impl IntoFragment<'a>) -> text::Text<'a> {
    text(content).size(FONT_BODY).color(TEXT_DIM)
}

pub fn value<'a>(content: impl IntoFragment<'a>) -> text::Text<'a> {
    text(content).size(FONT_BODY).color(TEXT)
}

pub fn dim<'a>(content: impl IntoFragment<'a>) -> text::Text<'a> {
    text(content).size(FONT_SMALL).color(TEXT_DIM)
}

pub fn compact_button<'a, Message: Clone + 'a>(
    label: impl IntoFragment<'a>,
) -> button::Button<'a, Message> {
    button(text(label).size(FONT_BODY))
        .padding(BTN_PAD)
        .style(button_style)
}

pub fn accent_button<'a, Message: Clone + 'a>(
    label: impl IntoFragment<'a>,
) -> button::Button<'a, Message> {
    button(text(label).size(FONT_BODY))
        .padding(BTN_PAD)
        .style(accent_button_style)
}

pub fn danger_button<'a, Message: Clone + 'a>(
    label: impl IntoFragment<'a>,
) -> button::Button<'a, Message> {
    button(text(label).size(FONT_BODY))
        .padding(BTN_PAD)
        .style(danger_button_style)
}

pub fn header_button<'a, Message: Clone + 'a>(
    content: impl Into<iced::Element<'a, Message>>,
) -> button::Button<'a, Message> {
    button(content)
        .padding([6, 8])
        .width(iced::Length::Fill)
        .style(header_button_style)
}

pub fn field<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .padding(FIELD_PAD)
        .size(FONT_BODY)
        .style(text_input_style)
}

pub fn hrule<'a>() -> rule::Rule<'a> {
    rule::horizontal(1).style(rule_style)
}

pub fn vrule<'a>() -> rule::Rule<'a> {
    rule::vertical(1).style(rule_style)
}

pub fn button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Active => (BUTTON, TEXT),
        button::Status::Hovered => (BUTTON_HOVER, TEXT),
        button::Status::Pressed => (BUTTON_PRESS, TEXT),
        button::Status::Disabled => (
            Color::from_rgb8(0x28, 0x28, 0x28),
            Color::from_rgb8(0x66, 0x66, 0x66),
        ),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: RULE,
            width: 1.0,
            radius: RADIUS.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn accent_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Active => Color::from_rgb8(0x3A, 0x2A, 0x18),
        button::Status::Hovered => Color::from_rgb8(0x4A, 0x34, 0x1C),
        button::Status::Pressed => Color::from_rgb8(0x32, 0x24, 0x14),
        button::Status::Disabled => Color::from_rgb8(0x28, 0x28, 0x28),
    };
    let text_color = match status {
        button::Status::Disabled => Color::from_rgb8(0x66, 0x66, 0x66),
        _ => TEXT,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: ACCENT,
            width: 1.0,
            radius: RADIUS.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn danger_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Active => Color::from_rgb8(0x3A, 0x22, 0x1E),
        button::Status::Hovered => Color::from_rgb8(0x4A, 0x2A, 0x24),
        button::Status::Pressed => Color::from_rgb8(0x32, 0x1C, 0x18),
        button::Status::Disabled => Color::from_rgb8(0x28, 0x28, 0x28),
    };
    let text_color = match status {
        button::Status::Disabled => Color::from_rgb8(0x66, 0x66, 0x66),
        _ => Color::from_rgb8(0xE8, 0xC0, 0xB8),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: DANGER,
            width: 1.0,
            radius: RADIUS.into(),
        },
        shadow: Shadow::default(),
        snap: true,
    }
}

fn header_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => {
            Some(Background::Color(Color::from_rgb8(0x26, 0x26, 0x26)))
        }
        _ => None,
    };
    button::Style {
        background,
        text_color: TEXT_DIM,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    }
}

pub fn text_input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => ACCENT,
        text_input::Status::Hovered => Color::from_rgb8(0x55, 0x55, 0x55),
        _ => RULE,
    };
    text_input::Style {
        background: Background::Color(INPUT),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: RADIUS.into(),
        },
        icon: TEXT_DIM,
        placeholder: TEXT_DIM,
        value: TEXT,
        selection: Color { a: 0.35, ..ACCENT },
    }
}

pub fn pick_list_style(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let border_color = match status {
        pick_list::Status::Hovered | pick_list::Status::Opened { .. } => {
            Color::from_rgb8(0x55, 0x55, 0x55)
        }
        pick_list::Status::Active => RULE,
    };
    pick_list::Style {
        text_color: TEXT,
        placeholder_color: TEXT_DIM,
        handle_color: TEXT_DIM,
        background: Background::Color(INPUT),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: RADIUS.into(),
        },
    }
}

pub fn menu_style(_theme: &Theme) -> overlay::menu::Style {
    overlay::menu::Style {
        background: Background::Color(PANEL),
        border: Border {
            color: RULE,
            width: 1.0,
            radius: RADIUS.into(),
        },
        text_color: TEXT,
        selected_text_color: TEXT,
        selected_background: Background::Color(Color::from_rgb8(0x3A, 0x2E, 0x22)),
        shadow: Shadow::default(),
    }
}

pub fn slider_style(_theme: &Theme, status: slider::Status) -> slider::Style {
    let handle = match status {
        slider::Status::Active => Color::from_rgb8(0xC0, 0xC0, 0xC0),
        slider::Status::Hovered | slider::Status::Dragged => ACCENT,
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (ACCENT.into(), Color::from_rgb8(0x3A, 0x3A, 0x3A).into()),
            width: 3.0,
            border: Border {
                radius: 1.5.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 5.0 },
            background: handle.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

pub fn scrollable_style(_theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let scroller = match status {
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered: true,
            ..
        }
        | scrollable::Status::Dragged {
            is_vertical_scrollbar_dragged: true,
            ..
        } => Color::from_rgb8(0x66, 0x66, 0x66),
        _ => Color::from_rgb8(0x4A, 0x4A, 0x4A),
    };
    let rail = scrollable::Rail {
        background: Some(Background::Color(WINDOW)),
        border: Border {
            radius: RADIUS.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        scroller: scrollable::Scroller {
            background: Background::Color(scroller),
            border: Border {
                radius: RADIUS.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(Color { a: 0.9, ..PANEL }),
            border: Border {
                color: RULE,
                width: 1.0,
                radius: 16.0.into(),
            },
            shadow: Shadow {
                color: Color {
                    a: 0.7,
                    ..Color::BLACK
                },
                offset: Vector::ZERO,
                blur_radius: 2.0,
            },
            icon: TEXT,
        },
    }
}

pub fn rule_style(_theme: &Theme) -> rule::Style {
    rule::Style {
        color: RULE,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn decorate_pick_list<'a, T, L, V, Message>(
    pick: pick_list::PickList<'a, T, L, V, Message>,
) -> pick_list::PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
    V: std::borrow::Borrow<T> + 'a,
    Message: Clone + 'a,
{
    pick.padding(FIELD_PAD)
        .text_size(FONT_BODY)
        .style(pick_list_style)
        .menu_style(menu_style)
}
