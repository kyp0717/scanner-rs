use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length};

use crate::gui::app::{App, Message};
use crate::gui::theme::{self, Colors};

/// Scanner categories for the sidebar.
const SCANNER_CATEGORIES: &[(&str, &[(&str, &str)])] = &[
    (
        "Momentum & Gainers",
        &[
            ("TOP_PERC_GAIN", "Top % Gainers"),
            ("TOP_PERC_LOSE", "Top % Losers"),
        ],
    ),
    (
        "Volume & Activity",
        &[
            ("HOT_BY_VOLUME", "Hot by Volume"),
            ("MOST_ACTIVE", "Most Active"),
            ("TOP_TRADE_COUNT", "Top Trade Count"),
            ("HOT_BY_PRICE", "Hot by Price"),
            ("TOP_VOLUME_RATE", "Top Volume Rate"),
        ],
    ),
    (
        "Gaps & Extended Hours",
        &[
            ("HIGH_OPEN_GAP", "Gap Up"),
            ("LOW_OPEN_GAP", "Gap Down"),
        ],
    ),
    (
        "Highs & Lows",
        &[("HIGH_VS_52W_HL", "52-Week High/Low")],
    ),
    (
        "Short Interest",
        &[],
    ),
    (
        "Technical Indicators",
        &[],
    ),
    (
        "Social Sentiment",
        &[],
    ),
    (
        "Dividends",
        &[],
    ),
];

/// The 8 alert scanners with detailed descriptions.
const ALERT_SCANNERS_INFO: &[(&str, &str)] = &[
    (
        "HOT_BY_VOLUME",
        "Stocks with the highest volume relative to their recent average. \
         Flags unusual trading activity that often precedes large price moves. \
         A volume spike means institutional buying, news reaction, or momentum \
         traders piling in. Essential for catching moves early before they trend.",
    ),
    (
        "TOP_PERC_GAIN",
        "Stocks with the largest percentage gain on the day. \
         These are the biggest movers right now — up 10%, 20%, or more. \
         Often triggered by earnings beats, FDA approvals, contract wins, \
         or short squeezes. The primary scanner for finding momentum plays.",
    ),
    (
        "MOST_ACTIVE",
        "Stocks with the highest absolute share volume traded. \
         Unlike relative volume, this shows raw liquidity — millions of shares \
         changing hands. High activity means tight spreads, easy fills, and \
         heavy market attention. Useful for confirming that a move has real participation.",
    ),
    (
        "HIGH_OPEN_GAP",
        "Stocks gapping up significantly from the previous close at the open. \
         Gap-ups signal overnight news, pre-market buying pressure, or earnings \
         surprises. Traders watch for gap-and-go setups where the stock continues \
         higher after the opening bell, or gap-fade reversals back to prior close.",
    ),
    (
        "TOP_TRADE_COUNT",
        "Stocks with the most individual trades (not volume). \
         A high trade count means broad retail and institutional participation. \
         Stocks with many small trades indicate retail interest and social media \
         buzz. Useful for catching meme stocks and momentum plays driven by crowd behavior.",
    ),
    (
        "HOT_BY_PRICE",
        "Stocks with the most rapid price movement in a short window. \
         Detects fast, explosive moves — the kind that happen in seconds to minutes. \
         These are the stocks spiking right now, often on breaking news or a \
         technical breakout. Ideal for scalpers and intraday momentum traders.",
    ),
    (
        "TOP_VOLUME_RATE",
        "Stocks where volume is accelerating fastest compared to recent bars. \
         While HOT_BY_VOLUME compares to a 30-day average, volume rate measures \
         the speed of the current surge. A stock going from 10K to 500K shares/min \
         ranks high here. Catches the earliest stage of a volume breakout.",
    ),
    (
        "HIGH_VS_52W_HL",
        "Stocks trading at or near their 52-week high. \
         New highs mean no overhead resistance — every holder is profitable. \
         Momentum traders buy breakouts to new highs expecting continuation. \
         Combined with volume, a 52-week high breakout is one of the strongest \
         technical signals for sustained upward movement.",
    ),
];

impl App {
    pub fn scanner_view(&self) -> Element<Message> {
        let fs = self.font_size;

        // Sidebar: alert scanners button + scanner categories
        let mut sidebar = column![
            text("Categories")
                .size(fs + 2)
                .style(theme::text_color(Colors::CYAN)),
            Space::new().height(4),
        ]
        .spacing(2)
        .padding(8);

        // Alert Scanners button at the top
        let alert_btn = button(
            text("Alert Scanners (8)")
                .size(fs)
                .style(theme::text_color(Colors::GREEN)),
        )
        .on_press(Message::ScanCategory("__alert__".to_string()))
        .padding([4, 8])
        .width(Length::Fill)
        .style(theme::category_btn_style);

        sidebar = sidebar.push(alert_btn);
        sidebar = sidebar.push(Space::new().height(4));

        for &(category, scanners) in SCANNER_CATEGORIES {
            let count = scanners.len();
            let label = if count > 0 {
                format!("{category} ({count})")
            } else {
                category.to_string()
            };
            let btn = button(
                text(label)
                    .size(fs)
                    .style(theme::text_color(if count > 0 {
                        Colors::TEXT
                    } else {
                        Colors::TEXT_DIM
                    })),
            )
            .on_press(Message::ScanCategory(category.to_string()))
            .padding([4, 8])
            .width(Length::Fill)
            .style(theme::category_btn_style);

            sidebar = sidebar.push(btn);
        }

        let sidebar_panel = container(scrollable(sidebar).height(Length::Fill))
            .width(Length::Fixed(220.0))
            .height(Length::Fill)
            .style(theme::card_container);

        // Right panel: scanner details or category output
        let right_panel = if self.scanner_show_alerts {
            self.alert_scanners_panel(fs)
        } else {
            self.category_output_panel(fs)
        };

        row![sidebar_panel, right_panel]
            .spacing(4)
            .padding(4)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn alert_scanners_panel(&self, fs: u32) -> Element<Message> {
        let mut content = column![
            text("Alert Scanners")
                .size(fs + 2)
                .style(theme::text_color(Colors::CYAN)),
            text("These 8 scanners run every poll cycle to detect momentum stocks.")
                .size(if fs > 3 { fs - 2 } else { fs })
                .style(theme::text_dim),
            Space::new().height(8),
        ]
        .spacing(4)
        .padding(8);

        let desc_size = if fs > 3 { fs - 3 } else { fs };

        for &(code, description) in ALERT_SCANNERS_INFO {
            content = content.push(
                text(code)
                    .size(fs)
                    .style(theme::text_color(Colors::GREEN)),
            );
            content = content.push(
                text(description)
                    .size(desc_size)
                    .style(theme::text_dim),
            );
            content = content.push(Space::new().height(6));
        }

        container(scrollable(content.width(Length::Fill)).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(4)
            .style(theme::card_container)
            .into()
    }

    fn category_output_panel(&self, fs: u32) -> Element<Message> {
        let mut output = column![].spacing(1).padding(8);

        if self.output_lines.is_empty() {
            output = output.push(
                text("Select a category or click Alert Scanners")
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

        container(scrollable(output.width(Length::Fill)).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(4)
            .style(theme::card_container)
            .into()
    }
}
