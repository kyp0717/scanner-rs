use iced::widget::{column, container, row, text, Space};
use iced::{Element, Length};

use crate::gui::app::{App, Message};
use crate::gui::theme::{self, Colors};

impl App {
    pub fn settings_view(&self) -> Element<Message> {
        let fs = self.font_size;
        let s = &self.engine.settings;

        macro_rules! label {
            ($s:expr) => {
                text(String::from($s)).size(fs + 1).width(120).style(theme::text_color(Colors::YELLOW))
            };
        }
        macro_rules! val {
            ($s:expr) => {
                text($s).size(fs + 1)
            };
        }

        let port_str = self
            .engine
            .connected_port
            .or(s.port)
            .map(|p| p.to_string())
            .unwrap_or("auto".to_string());
        let port_type = match self.engine.connected_port {
            Some(7500) => " (paper)",
            Some(7497) => " (live)",
            _ => "",
        };

        let poll_str = if self.engine.polling { "on" } else { "off" };

        let mut lines = column![].spacing(8).padding(16);

        lines = lines.push(
            text(String::from("Settings"))
                .size(fs + 4)
                .style(theme::text_color(Colors::CYAN)),
        );
        lines = lines.push(Space::with_height(8));

        lines = lines.push(row![
            label!("Connection"),
            val!(format!("{}:{}{}", s.host, port_str, port_type))
        ]);
        lines = lines.push(row![
            label!("Polling"),
            val!(format!("{} (15s cycle)", poll_str))
        ]);
        lines = lines.push(row![
            label!("Seen"),
            val!(format!("{} stocks", self.engine.alert_seen.len()))
        ]);
        lines = lines.push(row![label!("Rows"), val!(format!("{}", s.rows))]);
        lines = lines.push(row![
            label!("Min Price"),
            val!(s.min_price
                .map(|p| format!("${p}"))
                .unwrap_or("none".into()))
        ]);
        lines = lines.push(row![
            label!("Max Price"),
            val!(s.max_price
                .map(|p| format!("${p}"))
                .unwrap_or("none".into()))
        ]);
        lines = lines.push(row![
            label!("Supabase"),
            val!(if self.engine.db.is_some() {
                "connected".into()
            } else {
                String::from("not configured")
            })
        ]);
        lines = lines.push(row![
            label!("Font Size"),
            val!(format!("{} (Ctrl+/- to adjust)", self.font_size))
        ]);

        container(lines)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(4)
            .style(theme::card_container)
            .into()
    }
}
