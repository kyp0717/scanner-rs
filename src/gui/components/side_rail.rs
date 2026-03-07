use iced::widget::{button, column, container, svg, tooltip, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use crate::gui::app::{Message, View};
use crate::gui::theme::Colors;

const ICON_MONITOR: &[u8] = include_bytes!("../../../assets/icons/monitor.svg");
const ICON_SEARCH: &[u8] = include_bytes!("../../../assets/icons/search.svg");
const ICON_HISTORY: &[u8] = include_bytes!("../../../assets/icons/history.svg");
const ICON_GEAR: &[u8] = include_bytes!("../../../assets/icons/gear.svg");

struct RailIcon {
    view: View,
    svg_bytes: &'static [u8],
    label: &'static str,
}

fn get_rail_icons() -> Vec<RailIcon> {
    vec![
        RailIcon {
            view: View::Monitor,
            svg_bytes: ICON_MONITOR,
            label: "Monitor",
        },
        RailIcon {
            view: View::Scanner,
            svg_bytes: ICON_SEARCH,
            label: "Scanner",
        },
        RailIcon {
            view: View::Log,
            svg_bytes: ICON_HISTORY,
            label: "Log",
        },
        RailIcon {
            view: View::Settings,
            svg_bytes: ICON_GEAR,
            label: "Settings",
        },
    ]
}

const RAIL_WIDTH: u16 = 48;
const ICON_SIZE: u16 = 36;
const SVG_SIZE: u16 = 20;

pub fn side_rail_view(current_view: View) -> Element<'static, Message> {
    let mut rail_buttons = column![]
        .spacing(8)
        .padding(6)
        .align_x(Alignment::Center);

    for rail_icon in get_rail_icons() {
        let is_active = rail_icon.view == current_view;

        let icon_handle = svg::Handle::from_memory(rail_icon.svg_bytes);
        let icon_color = if is_active {
            Colors::TEXT
        } else {
            Colors::TEXT_DIM
        };
        let icon_widget = svg(icon_handle)
            .width(SVG_SIZE)
            .height(SVG_SIZE)
            .style(move |_theme, _status| svg::Style {
                color: Some(icon_color),
            });

        let icon_btn = button(
            container(icon_widget)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .on_press(Message::NavigateTo(rail_icon.view))
        .padding(8)
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .style(rail_icon_style(is_active));

        let with_tooltip = tooltip(icon_btn, rail_icon.label, tooltip::Position::Right)
            .gap(8)
            .style(tooltip_style);

        rail_buttons = rail_buttons.push(with_tooltip);
    }

    container(column![rail_buttons, Space::with_height(Length::Fill)].width(RAIL_WIDTH))
        .height(Length::Fill)
        .style(rail_container_style)
        .into()
}

fn rail_icon_style(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (bg_color, text_color, border_color) = if active {
            (Colors::PRIMARY, Colors::TEXT, Colors::ACCENT)
        } else {
            match status {
                button::Status::Hovered => (
                    Color {
                        a: 0.3,
                        ..Colors::PRIMARY
                    },
                    Colors::TEXT,
                    Color::TRANSPARENT,
                ),
                _ => (Colors::SURFACE, Colors::TEXT_DIM, Color::TRANSPARENT),
            }
        };

        button::Style {
            background: Some(Background::Color(bg_color)),
            text_color,
            border: Border {
                color: border_color,
                width: if active { 2.0 } else { 0.0 },
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    }
}

fn rail_container_style(_theme: &Theme) -> container::Style {
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

fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Colors::PRIMARY)),
        border: Border {
            color: Colors::ACCENT,
            width: 1.0,
            radius: 4.0.into(),
        },
        text_color: Some(Colors::TEXT),
        ..Default::default()
    }
}
