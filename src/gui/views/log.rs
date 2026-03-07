use iced::widget::{column, container, scrollable, text};
use iced::{Element, Length};

use crate::gui::app::{App, Message};
use crate::gui::theme::{self, Colors};

impl App {
    pub fn log_view(&self) -> Element<Message> {
        let fs = self.font_size;

        let title = text(format!("Log ({} entries)", self.log_lines.len()))
            .size(fs + 2)
            .style(theme::text_color(Colors::CYAN));

        let mut lines = column![title].spacing(2).padding(8);

        if self.log_lines.is_empty() {
            lines = lines.push(
                text("No log entries yet. Enable polling to start tracking.")
                    .size(fs)
                    .style(theme::text_dim),
            );
        } else {
            for line in self.log_lines.iter().rev() {
                let style = if line.contains("[poll]") {
                    theme::text_color(Colors::TEXT)
                } else if line.contains("[enrich]") {
                    theme::text_color(Colors::GREEN)
                } else if line.contains("[tws]") {
                    theme::text_color(Colors::CYAN)
                } else if line.contains("[scan]") {
                    theme::text_color(Colors::YELLOW)
                } else {
                    theme::text_color(Colors::TEXT_DIM)
                };
                lines = lines.push(text(line).size(fs).style(style));
            }
        }

        container(scrollable(lines).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(4)
            .style(theme::card_container)
            .into()
    }
}
