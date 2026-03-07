use iced::widget::{column, container, scrollable, text, text_input};
use iced::{Element, Length};

use crate::gui::app::{App, Message};
use crate::gui::theme::{self, Colors};

impl App {
    pub fn scanner_view(&self) -> Element<Message> {
        let fs = self.font_size;

        let input = text_input("> type a command...", &self.input)
            .on_input(Message::InputChanged)
            .on_submit(Message::SubmitCommand)
            .size(fs + 2)
            .padding(8)
            .style(theme::command_input_style);

        let mut output = column![].spacing(1).padding(4);

        if self.output_lines.is_empty() {
            output = output.push(
                text("Type a command: scan, list, poll, history, help")
                    .size(fs)
                    .style(theme::text_dim),
            );
        } else {
            for line in &self.output_lines {
                let style = if line.starts_with('#') || line.starts_with('-') {
                    theme::text_color(Colors::TEXT_DIM)
                } else {
                    theme::text_color(Colors::TEXT)
                };
                output = output.push(text(line).size(fs).style(style));
            }
        }

        let output_panel = container(scrollable(output).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(4)
            .style(theme::card_container);

        column![input, output_panel]
            .spacing(4)
            .padding(4)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
