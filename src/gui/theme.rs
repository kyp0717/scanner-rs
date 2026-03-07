use iced::widget::{button, container, text, text_input};
use iced::{color, Background, Border, Color, Theme};

pub struct Colors;

impl Colors {
    pub const BG: Color = color!(0x0a0a0a);
    pub const SURFACE: Color = color!(0x1a1a1a);
    pub const PRIMARY: Color = color!(0x0f3460);
    pub const ACCENT: Color = color!(0xe94560);

    pub const TEXT: Color = color!(0xeaeaea);
    pub const TEXT_DIM: Color = color!(0x888888);

    pub const CYAN: Color = color!(0x00cccc);
    pub const GREEN: Color = color!(0x00c853);
    pub const RED: Color = color!(0xff5252);
    pub const YELLOW: Color = color!(0xffd600);
}

pub fn scanner_theme() -> Theme {
    Theme::custom(
        "Scanner Dark".to_string(),
        iced::theme::Palette {
            background: Colors::BG,
            text: Colors::TEXT,
            primary: Colors::PRIMARY,
            success: Colors::GREEN,
            danger: Colors::RED,
        },
    )
}

pub fn card_container(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Colors::SURFACE)),
        border: Border {
            color: Colors::PRIMARY,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

pub fn command_input_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Colors::SURFACE),
        border: Border {
            color: Colors::ACCENT,
            width: 1.0,
            radius: 4.0.into(),
        },
        icon: Colors::TEXT_DIM,
        placeholder: Colors::TEXT_DIM,
        value: Colors::TEXT,
        selection: Colors::PRIMARY,
    }
}

pub fn status_bar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Colors::SURFACE)),
        border: Border {
            color: Colors::PRIMARY,
            width: 0.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn alert_row_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let bg = if selected {
            Colors::PRIMARY
        } else {
            match status {
                button::Status::Hovered => Color {
                    a: 0.3,
                    ..Colors::PRIMARY
                },
                _ => Color::TRANSPARENT,
            }
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: Colors::TEXT,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 2.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn text_color(color: Color) -> impl Fn(&Theme) -> text::Style {
    move |_theme| text::Style { color: Some(color) }
}

pub fn text_dim(_theme: &Theme) -> text::Style {
    text::Style {
        color: Some(Colors::TEXT_DIM),
    }
}
