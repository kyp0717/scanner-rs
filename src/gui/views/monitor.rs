use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length};

use crate::gui::app::{App, Message};
use crate::gui::theme::{self, Colors};

impl App {
    pub fn monitor_view(&self) -> Element<Message> {
        let status = self.status_bar();

        let alert_table = self.alert_table_view();
        let detail = self.detail_panel_view();

        let main = row![alert_table, detail]
            .spacing(4)
            .height(Length::Fill);

        column![status, main]
            .spacing(4)
            .padding(4)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn status_bar(&self) -> Element<Message> {
        let port = self
            .engine
            .connected_port
            .or(self.engine.settings.port)
            .map(|p| p.to_string())
            .unwrap_or_else(|| "auto".to_string());
        let poll_str = if self.engine.polling { "on" } else { "off" };
        let seen = self.engine.alert_seen.len();

        let status_text = text(format!(
            "Scanner -- {}:{}  |  Polling: {}  |  Seen: {}",
            self.engine.settings.host, port, poll_str, seen
        ))
        .size(self.font_size + 1)
        .style(theme::text_dim);

        container(status_text)
            .width(Length::Fill)
            .padding([4, 8])
            .style(theme::status_bar)
            .into()
    }

    fn alert_table_view(&self) -> Element<Message> {
        let fs = self.font_size;
        let header = row![
            text("Time")
                .size(fs)
                .width(Length::FillPortion(3))
                .style(theme::text_color(Colors::YELLOW)),
            text("Symbol")
                .size(fs)
                .width(Length::FillPortion(2))
                .style(theme::text_color(Colors::YELLOW)),
            text("Last")
                .size(fs)
                .width(Length::FillPortion(2))
                .style(theme::text_color(Colors::YELLOW)),
            text("Chg%")
                .size(fs)
                .width(Length::FillPortion(2))
                .style(theme::text_color(Colors::YELLOW)),
            text("Hits")
                .size(fs)
                .width(Length::FillPortion(1))
                .style(theme::text_color(Colors::YELLOW)),
            text("Name")
                .size(fs)
                .width(Length::FillPortion(4))
                .style(theme::text_color(Colors::YELLOW)),
        ]
        .spacing(4)
        .padding([0, 4]);

        let mut rows_col = column![header].spacing(0);

        if self.engine.alert_rows.is_empty() {
            rows_col = rows_col.push(
                text("No alerts yet")
                    .size(fs + 1)
                    .style(theme::text_dim),
            );
        } else {
            for (i, r) in self.engine.alert_rows.iter().enumerate() {
                let price = r.last.map(|p| format!("{p:.2}")).unwrap_or("-".into());
                let chg_str = r
                    .change_pct
                    .map(|c| format!("{c:+.1}%"))
                    .unwrap_or("-".into());
                let hits = format!("{}/8", r.scanner_hits);
                let name = if r.enriched {
                    r.name.as_deref().unwrap_or("-")
                } else {
                    "..."
                };
                let name = if name.len() > 18 {
                    format!("{}..", &name[..16])
                } else {
                    name.to_string()
                };

                let chg_color = if r.change_pct.unwrap_or(0.0) >= 0.0 {
                    Colors::GREEN
                } else {
                    Colors::RED
                };

                let row_content = row![
                    text(&r.alert_time).size(fs).width(Length::FillPortion(3)),
                    text(&r.symbol)
                        .size(fs)
                        .width(Length::FillPortion(2))
                        .style(theme::text_color(Colors::CYAN)),
                    text(price).size(fs).width(Length::FillPortion(2)),
                    text(chg_str)
                        .size(fs)
                        .width(Length::FillPortion(2))
                        .style(theme::text_color(chg_color)),
                    text(hits).size(fs).width(Length::FillPortion(1)),
                    text(name).size(fs).width(Length::FillPortion(4)),
                ]
                .spacing(4)
                .padding([2, 4]);

                let is_selected = i == self.selected_alert_row;
                let row_btn = button(row_content)
                    .on_press(Message::SelectAlert(i))
                    .padding(0)
                    .width(Length::Fill)
                    .style(theme::alert_row_style(is_selected));

                rows_col = rows_col.push(row_btn);
            }
        }

        container(scrollable(rows_col).height(Length::Fill))
            .width(Length::FillPortion(1))
            .height(Length::Fill)
            .padding(4)
            .style(theme::card_container)
            .into()
    }

    fn detail_panel_view(&self) -> Element<Message> {
        let fs = self.font_size;
        let mut lines = column![].spacing(4).padding(8);

        if self.engine.alert_rows.is_empty()
            || self.selected_alert_row >= self.engine.alert_rows.len()
        {
            lines = lines.push(text("No stock selected").size(fs + 1).style(theme::text_dim));
            return container(lines)
                .width(Length::FillPortion(1))
                .height(Length::Fill)
                .style(theme::card_container)
                .into();
        }

        let r = &self.engine.alert_rows[self.selected_alert_row];

        lines = lines.push(
            text(&r.symbol)
                .size(fs + 6)
                .style(theme::text_color(Colors::CYAN)),
        );
        lines = lines.push(Space::with_height(4));

        macro_rules! label {
            ($s:expr) => {
                text(String::from($s))
                    .size(fs)
                    .width(Length::FillPortion(2))
                    .style(theme::text_color(Colors::YELLOW))
            };
        }
        macro_rules! val {
            ($s:expr) => {
                text($s).size(fs).width(Length::FillPortion(3))
            };
        }

        // Price
        let price_str = r.last.map(|p| format!("${p:.2}")).unwrap_or("-".into());
        lines = lines.push(row![label!("Price"), val!(price_str)]);

        // Change%
        let chg_str = r
            .change_pct
            .map(|c| format!("{c:+.1}%"))
            .unwrap_or("-".into());
        let chg_color = if r.change_pct.unwrap_or(0.0) >= 0.0 {
            Colors::GREEN
        } else {
            Colors::RED
        };
        lines = lines.push(row![
            label!("Change"),
            text(chg_str)
                .size(fs)
                .width(Length::FillPortion(3))
                .style(theme::text_color(chg_color))
        ]);

        // Volume
        let vol_str = r.volume.map(format_volume).unwrap_or("-".into());
        lines = lines.push(row![label!("Volume"), val!(vol_str)]);

        // RVol
        let rvol_str = fmt_or_dots(r.enriched, r.rvol.map(|v| format!("{v:.1}x")));
        lines = lines.push(row![label!("RVol"), val!(rvol_str)]);

        // Float
        let float_str = fmt_or_dots(
            r.enriched,
            r.float_shares.map(|v| {
                if v >= 1e9 {
                    format!("{:.1}B", v / 1e9)
                } else if v >= 1e6 {
                    format!("{:.1}M", v / 1e6)
                } else if v >= 1e3 {
                    format!("{:.0}K", v / 1e3)
                } else {
                    format!("{v:.0}")
                }
            }),
        );
        lines = lines.push(row![label!("Float"), val!(float_str)]);

        // Short%
        let short_str =
            fmt_or_dots(r.enriched, r.short_pct.map(|v| format!("{:.1}%", v * 100.0)));
        lines = lines.push(row![label!("Short%"), val!(short_str)]);

        lines = lines.push(Space::with_height(4));

        // Name, Sector, Industry
        let name_str = fmt_or_dots(r.enriched, r.name.clone());
        lines = lines.push(row![label!("Name"), val!(name_str)]);

        let sector_str = fmt_or_dots(r.enriched, r.sector.clone());
        lines = lines.push(row![label!("Sector"), val!(sector_str)]);

        let industry_str = fmt_or_dots(r.enriched, r.industry.clone());
        lines = lines.push(row![label!("Industry"), val!(industry_str)]);

        lines = lines.push(Space::with_height(4));

        // Scanner Hits
        lines = lines.push(row![
            label!("Scanners"),
            val!(format!("{}/8", r.scanner_hits))
        ]);

        // Catalyst
        let catalyst_display = r.catalyst.as_ref().map(|cat| {
            let ago = r.catalyst_time.map(|epoch| {
                let now = chrono::Utc::now().timestamp();
                let diff = now - epoch;
                if diff < 60 {
                    "now".to_string()
                } else if diff < 3600 {
                    format!("{}m ago", diff / 60)
                } else if diff < 86400 {
                    format!("{}h ago", diff / 3600)
                } else {
                    format!("{}d ago", diff / 86400)
                }
            });
            match ago {
                Some(a) => format!("{cat} ({a})"),
                None => cat.clone(),
            }
        });
        let cat_str = fmt_or_dots(r.enriched, catalyst_display);
        lines = lines.push(row![label!("Catalyst"), val!(cat_str)]);

        // News Headlines
        if !r.news_headlines.is_empty() {
            lines = lines.push(Space::with_height(4));
            lines = lines.push(
                text("News")
                    .size(fs)
                    .style(theme::text_color(Colors::YELLOW)),
            );
            let news_size = if fs > 9 { fs - 1 } else { fs };
            for (i, headline) in r.news_headlines.iter().take(5).enumerate() {
                let ago = headline
                    .published
                    .map(|epoch| {
                        let now = chrono::Utc::now().timestamp();
                        let diff = now - epoch;
                        if diff < 60 {
                            "now".to_string()
                        } else if diff < 3600 {
                            format!("{}m", diff / 60)
                        } else if diff < 86400 {
                            format!("{}h", diff / 3600)
                        } else {
                            format!("{}d", diff / 86400)
                        }
                    })
                    .unwrap_or_default();
                let prefix = if ago.is_empty() {
                    format!(" {}. ", i + 1)
                } else {
                    format!(" {}. {} ", i + 1, ago)
                };
                let title = &headline.title;
                let max_title = 50usize.saturating_sub(prefix.len());
                let truncated = if title.len() > max_title {
                    format!("{}...", &title[..max_title.saturating_sub(3)])
                } else {
                    title.clone()
                };
                lines = lines.push(text(format!("{prefix}{truncated}")).size(news_size));
            }
        } else if !r.enriched {
            lines = lines.push(row![
                label!("News"),
                text(String::from("..."))
                    .size(fs)
                    .width(Length::FillPortion(3))
                    .style(theme::text_dim)
            ]);
        }

        container(scrollable(lines).height(Length::Fill))
            .width(Length::FillPortion(1))
            .height(Length::Fill)
            .style(theme::card_container)
            .into()
    }
}

fn format_volume(vol: i64) -> String {
    if vol >= 1_000_000 {
        format!("{:.1}M", vol as f64 / 1_000_000.0)
    } else if vol >= 1_000 {
        format!("{:.0}K", vol as f64 / 1_000.0)
    } else {
        format!("{vol}")
    }
}

fn fmt_or_dots(enriched: bool, val: Option<String>) -> String {
    match val {
        Some(v) if !v.is_empty() => v,
        _ if enriched => "-".into(),
        _ => "...".into(),
    }
}
