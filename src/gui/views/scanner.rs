use iced::widget::{button, column, container, row, scrollable, table, text, Space};
use iced::{Element, Length};

use crate::gui::app::{App, Message};
use crate::gui::theme::{self, Colors};

/// Scanner categories for the sidebar.
const SCANNER_CATEGORIES: &[(&str, &str, &[(&str, &str)])] = &[
    (
        "__alert__",
        "Alerts (8)",
        &[],
    ),
    (
        "__momentum__",
        "Momentum (7)",
        &[],
    ),
    (
        "__premarket_gaps__",
        "Premarket Gaps (2)",
        &[],
    ),
    (
        "__extended_hours__",
        "Extended Hours (3)",
        &[],
    ),
    (
        "__highs__",
        "Highs & Lows (1)",
        &[],
    ),
    (
        "Short Interest",
        "Short Interest",
        &[],
    ),
    (
        "Technical Indicators",
        "Technical Indicators",
        &[],
    ),
    (
        "Social Sentiment",
        "Social Sentiment",
        &[],
    ),
    (
        "Dividends",
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

/// Momentum scanners (combined Momentum & Gainers + Volume & Activity).
const MOMENTUM_SCANNERS_INFO: &[(&str, &str)] = &[
    (
        "TOP_PERC_GAIN",
        "Biggest percentage gainers on the day. The primary scanner for finding \
         momentum plays — stocks up 10%, 20%, or more on earnings, FDA, contracts, \
         or short squeezes.",
    ),
    (
        "TOP_PERC_LOSE",
        "Biggest percentage losers on the day. Mirror of TOP_PERC_GAIN for the \
         downside — useful for short setups, fade plays, or tracking sector weakness.",
    ),
    (
        "HOT_BY_VOLUME",
        "Highest volume relative to recent average. Flags unusual trading activity \
         that precedes large moves — institutional buying, news reaction, or momentum \
         traders piling in.",
    ),
    (
        "MOST_ACTIVE",
        "Highest absolute share volume traded. Shows raw liquidity — millions of \
         shares changing hands. Tight spreads, easy fills, heavy market attention. \
         Confirms a move has real participation.",
    ),
    (
        "TOP_TRADE_COUNT",
        "Most individual trades (not volume). Broad retail and institutional \
         participation. Many small trades indicate retail interest and social media \
         buzz — catches meme stocks and crowd-driven momentum.",
    ),
    (
        "HOT_BY_PRICE",
        "Most rapid price movement in a short window. Detects fast, explosive moves \
         happening in seconds to minutes — breaking news or technical breakouts. \
         Ideal for scalpers and intraday momentum traders.",
    ),
    (
        "TOP_VOLUME_RATE",
        "Fastest volume acceleration compared to recent bars. Measures the speed of \
         the current surge — a stock going from 10K to 500K shares/min ranks high. \
         Catches the earliest stage of a volume breakout.",
    ),
];

/// Premarket gap scanners.
const PREMARKET_GAPS_INFO: &[(&str, &str)] = &[
    (
        "HIGH_OPEN_GAP",
        "Stocks gapping up significantly from the previous close at the open. \
         Gap-ups signal overnight news, pre-market buying pressure, or earnings \
         surprises. Watch for gap-and-go continuation or gap-fade reversals.",
    ),
    (
        "LOW_OPEN_GAP",
        "Stocks gapping down significantly from the previous close at the open. \
         Gap-downs signal negative news, after-hours selling, or missed expectations. \
         Watch for bounce plays or continued breakdown.",
    ),
];

/// Extended hours (after-hours / pre-market) scanners.
const EXTENDED_HOURS_INFO: &[(&str, &str)] = &[
    (
        "AFTER_HOURS_PERC_GAIN",
        "Top percentage gainers in after-hours / pre-market trading. Catches \
         stocks reacting to earnings releases, FDA decisions, or other news \
         announced outside regular trading hours.",
    ),
    (
        "AFTER_HOURS_PERC_LOSE",
        "Top percentage losers in after-hours / pre-market trading. Flags \
         stocks dropping on missed earnings, downgrades, or negative news \
         released outside regular hours.",
    ),
    (
        "AFTER_HOURS_VOLUME",
        "Highest volume in after-hours / pre-market sessions. Shows which \
         stocks have the most trading activity outside RTH — often the \
         earliest signal of a catalyst before the regular session opens.",
    ),
];

/// Highs & Lows scanners.
const HIGHS_SCANNERS_INFO: &[(&str, &str)] = &[
    (
        "HIGH_VS_52W_HL",
        "Stocks trading at or near their 52-week high. No overhead resistance — \
         every holder is profitable. Momentum traders buy breakouts to new highs \
         expecting continuation. Combined with volume, one of the strongest \
         technical signals for sustained upward movement.",
    ),
];


impl App {
    pub fn scanner_view(&self) -> Element<Message> {
        let fs = self.font_size;

        // Sidebar
        let mut sidebar = column![
            text("Categories")
                .size(fs + 2)
                .style(theme::text_color(Colors::CYAN)),
            Space::new().height(4),
        ]
        .spacing(2)
        .padding(8);

        for &(key, label, _) in SCANNER_CATEGORIES {
            let has_scanners = matches!(
                key,
                "__alert__" | "__momentum__" | "__premarket_gaps__" | "__extended_hours__" | "__highs__"
            );
            let color = if key == "__alert__" {
                Colors::GREEN
            } else if has_scanners {
                Colors::TEXT
            } else {
                Colors::TEXT_DIM
            };
            let btn = button(
                text(label).size(fs).style(theme::text_color(color)),
            )
            .on_press(Message::ScanCategory(key.to_string()))
            .padding([4, 8])
            .width(Length::Fill)
            .style(theme::category_btn_style);

            sidebar = sidebar.push(btn);
        }

        let sidebar_panel = container(scrollable(sidebar).height(Length::Fill))
            .width(Length::Fixed(220.0))
            .height(Length::Fill)
            .style(theme::card_container);

        // Right panel
        let selected_key = self.scanner_selected.as_deref().unwrap_or("__alert__");
        let right_panel = match selected_key {
            "__alert__" => self.scanner_table_panel(
                fs,
                "Alerts",
                "These 8 scanners run every poll cycle to detect momentum stocks.",
                ALERT_SCANNERS_INFO,
            ),
            "__momentum__" => self.scanner_table_panel(
                fs,
                "Momentum",
                "Gainers, losers, volume, and price action scanners.",
                MOMENTUM_SCANNERS_INFO,
            ),
            "__premarket_gaps__" => self.scanner_table_panel(
                fs,
                "Premarket Gaps",
                "Stocks gapping up or down from previous close at the open.",
                PREMARKET_GAPS_INFO,
            ),
            "__extended_hours__" => self.scanner_table_panel(
                fs,
                "Extended Hours",
                "After-hours and pre-market scanners for outside RTH activity.",
                EXTENDED_HOURS_INFO,
            ),
            "__highs__" => self.scanner_table_panel(
                fs,
                "Highs & Lows",
                "52-week high and low breakout scanners.",
                HIGHS_SCANNERS_INFO,
            ),
            _ => self.category_output_panel(fs),
        };

        row![sidebar_panel, right_panel]
            .spacing(4)
            .padding(4)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn scanner_table_panel(
        &self,
        fs: u32,
        title: &'static str,
        subtitle: &'static str,
        scanners: &'static [(&'static str, &'static str)],
    ) -> Element<Message> {
        let desc_size = if fs > 3 { fs - 3 } else { fs };

        let code_col = table::column(
            text("Scanner Code")
                .size(fs)
                .style(theme::text_color(Colors::YELLOW)),
            move |r: (&str, &str)| -> Element<Message> {
                button(
                    text(r.0)
                        .size(fs)
                        .style(theme::text_color(Colors::GREEN)),
                )
                .on_press(Message::RunScan(r.0.to_string()))
                .padding([2, 4])
                .style(theme::category_btn_style)
                .into()
            },
        )
        .width(Length::FillPortion(2));

        let desc_col = table::column(
            text("Description")
                .size(fs)
                .style(theme::text_color(Colors::YELLOW)),
            move |r: (&str, &str)| -> Element<Message> {
                text(r.1)
                    .size(desc_size)
                    .style(theme::text_dim)
                    .into()
            },
        )
        .width(Length::FillPortion(5));

        let tbl = table::table([code_col, desc_col], scanners.to_vec())
            .padding(8)
            .separator(1);

        let content = column![
            text(title)
                .size(fs + 2)
                .style(theme::text_color(Colors::CYAN)),
            text(subtitle)
                .size(if fs > 3 { fs - 2 } else { fs })
                .style(theme::text_dim),
            Space::new().height(8),
            tbl,
        ]
        .spacing(4)
        .padding(8)
        .width(Length::Fill);

        container(content)
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
                text("Select a category")
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
