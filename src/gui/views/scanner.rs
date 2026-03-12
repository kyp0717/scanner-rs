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

        // If we have scan results and we're on the results view, show split panel
        let right_panel = if selected_key == "__results__" && !self.scan_results.is_empty() {
            self.scan_results_split_panel(fs)
        } else if selected_key == "__results__" {
            // Results view but no results (or cleared)
            self.category_output_panel(fs)
        } else {
            match selected_key {
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
            }
        };

        row![sidebar_panel, right_panel]
            .spacing(4)
            .padding(4)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Split panel showing scan results table (left) + detail panel (right).
    fn scan_results_split_panel(&self, fs: u32) -> Element<Message> {
        let left_pct = self.alert_split as u16;
        let right_pct = (100 - self.alert_split) as u16;

        let results_table = self.scan_results_table_view(fs, left_pct);
        let detail = self.scan_result_detail_view(fs, right_pct);

        let status = {
            let count = self.scan_results.len();
            let status_text = text(format!("{} — {} results", self.scan_results_code, count))
                .size(fs + 1)
                .style(theme::text_color(Colors::CYAN));

            let bar = row![
                status_text,
                Space::new().width(Length::Fill),
            ]
            .padding([4, 8]);

            container(bar)
                .width(Length::Fill)
                .style(theme::status_bar)
        };

        let main = row![results_table, detail]
            .spacing(4)
            .height(Length::Fill);

        column![status, main]
            .spacing(4)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Table of scan results (left side of split).
    fn scan_results_table_view(&self, fs: u32, pct: u16) -> Element<Message> {
        let header = row![
            text("#")
                .size(fs)
                .width(Length::FillPortion(1))
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
            text("Volume")
                .size(fs)
                .width(Length::FillPortion(3))
                .style(theme::text_color(Colors::YELLOW)),
            text("Name")
                .size(fs)
                .width(Length::FillPortion(4))
                .style(theme::text_color(Colors::YELLOW)),
        ]
        .spacing(4)
        .padding([0, 4]);

        let mut rows_col = column![header].spacing(0);

        for (i, r) in self.scan_results.iter().enumerate() {
            let price = r.last.map(|p| format!("{p:.2}")).unwrap_or("-".into());
            let chg_str = r
                .change_pct
                .map(|c| format!("{c:+.1}%"))
                .unwrap_or("-".into());
            let vol_str = r.volume.map(format_volume).unwrap_or("-".into());
            let name = r.name.as_deref().unwrap_or("-");
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
                text(format!("{}", r.rank))
                    .size(fs)
                    .width(Length::FillPortion(1))
                    .style(theme::text_dim),
                text(&r.symbol)
                    .size(fs)
                    .width(Length::FillPortion(2))
                    .style(theme::text_color(Colors::CYAN)),
                text(price).size(fs).width(Length::FillPortion(2)),
                text(chg_str)
                    .size(fs)
                    .width(Length::FillPortion(2))
                    .style(theme::text_color(chg_color)),
                text(vol_str).size(fs).width(Length::FillPortion(3)),
                text(name).size(fs).width(Length::FillPortion(4)),
            ]
            .spacing(4)
            .padding([2, 4]);

            let is_selected = i == self.selected_scan_row;
            let row_btn = button(row_content)
                .on_press(Message::SelectScanResult(i))
                .padding(0)
                .width(Length::Fill)
                .style(theme::alert_row_style(is_selected));

            rows_col = rows_col.push(row_btn);
        }

        container(scrollable(rows_col).height(Length::Fill))
            .width(Length::FillPortion(pct))
            .height(Length::Fill)
            .padding(4)
            .style(theme::card_container)
            .into()
    }

    /// Detail panel for a selected scan result (right side of split).
    fn scan_result_detail_view(&self, fs: u32, pct: u16) -> Element<Message> {
        let mut lines = column![].spacing(4).padding(8);

        if self.scan_results.is_empty()
            || self.selected_scan_row >= self.scan_results.len()
        {
            lines = lines.push(text("No stock selected").size(fs + 1).style(theme::text_dim));
            return container(lines)
                .width(Length::FillPortion(pct))
                .height(Length::Fill)
                .style(theme::card_container)
                .into();
        }

        let r = &self.scan_results[self.selected_scan_row];

        lines = lines.push(
            text(&r.symbol)
                .size(fs + 6)
                .style(theme::text_color(Colors::CYAN)),
        );
        lines = lines.push(Space::new().height(4));

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

        // Rank
        lines = lines.push(row![label!("Rank"), val!(format!("#{}", r.rank))]);

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

        // Avg Volume (10d and 3mo)
        let avg_vol_10d_str = fmt_or_dots(r.enriched, r.avg_volume_10d.map(format_raw_shares));
        let avg_vol_3mo_str = fmt_or_dots(r.enriched, r.avg_volume.map(format_raw_shares));
        lines = lines.push(row![label!("Avg Vol 10d"), val!(avg_vol_10d_str)]);
        lines = lines.push(row![label!("Avg Vol 3mo"), val!(avg_vol_3mo_str)]);

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

        lines = lines.push(Space::new().height(4));

        // Name, Sector, Industry, Country
        let name_str = fmt_or_dots(r.enriched, r.name.clone());
        lines = lines.push(row![label!("Name"), val!(name_str)]);

        let sector_str = fmt_or_dots(r.enriched, r.sector.clone());
        lines = lines.push(row![label!("Sector"), val!(sector_str)]);

        let industry_str = fmt_or_dots(r.enriched, r.industry.clone());
        lines = lines.push(row![label!("Industry"), val!(industry_str)]);

        let country_str = fmt_or_dots(r.enriched, r.country.clone());
        lines = lines.push(row![label!("Country"), val!(country_str)]);

        // Catalyst
        let catalyst_str = fmt_or_dots(r.enriched, r.catalyst.clone());
        lines = lines.push(row![label!("Catalyst"), val!(catalyst_str)]);

        // Bid/Ask
        if r.bid.is_some() || r.ask.is_some() {
            lines = lines.push(Space::new().height(4));
            let bid_str = r.bid.map(|p| format!("${p:.2}")).unwrap_or("-".into());
            let ask_str = r.ask.map(|p| format!("${p:.2}")).unwrap_or("-".into());
            lines = lines.push(row![label!("Bid"), val!(bid_str)]);
            lines = lines.push(row![label!("Ask"), val!(ask_str)]);
        }

        // Close
        if let Some(close) = r.close {
            let close_str = format!("${close:.2}");
            lines = lines.push(row![label!("Prev Close"), val!(close_str)]);
        }

        // News Headlines
        if !r.news_headlines.is_empty() {
            lines = lines.push(Space::new().height(4));
            lines = lines.push(
                text("News")
                    .size(fs)
                    .style(theme::text_color(Colors::YELLOW)),
            );
            let news_size = if fs > 9 { fs - 1 } else { fs };
            let now_ts = chrono::Utc::now().timestamp();
            let five_days = 5 * 86400;
            for headline in r.news_headlines.iter()
                .filter(|h| h.published.map_or(true, |ep| now_ts - ep < five_days))
                .take(5)
            {
                if let Some(epoch) = headline.published {
                    let dt = chrono::DateTime::from_timestamp(epoch, 0)
                        .unwrap_or_default()
                        .with_timezone(&chrono::Local);
                    let diff = now_ts - epoch;
                    let ago = if diff < 60 {
                        "now".to_string()
                    } else if diff < 3600 {
                        format!("{}m ago", diff / 60)
                    } else if diff < 86400 {
                        format!("{}h ago", diff / 3600)
                    } else {
                        format!("{}d ago", diff / 86400)
                    };
                    lines = lines.push(
                        text(format!("  {} ({})", dt.format("%b %d %H:%M"), ago))
                            .size(if news_size > 2 { news_size - 2 } else { news_size })
                            .style(theme::text_dim),
                    );
                }
                lines = lines.push(
                    text(format!("  {}", headline.title))
                        .size(news_size),
                );
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
            .width(Length::FillPortion(pct))
            .height(Length::Fill)
            .style(theme::card_container)
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

/// IB TWS reports volume in round lots (100 shares). Convert to shares for display.
fn format_volume(vol: i64) -> String {
    // IB volume is in round lots (×100 to get shares)
    let shares = vol as f64 * 100.0;
    format_shares_f64(shares)
}

/// Format a value already in raw shares (e.g. Yahoo Finance avg volume).
fn format_raw_shares(vol: i64) -> String {
    format_shares_f64(vol as f64)
}

fn format_shares_f64(shares: f64) -> String {
    if shares >= 1_000_000.0 {
        format!("{:.1}M", shares / 1_000_000.0)
    } else if shares >= 1_000.0 {
        format!("{:.1}K", shares / 1_000.0)
    } else {
        format!("{:.0}", shares)
    }
}

fn fmt_or_dots(enriched: bool, val: Option<String>) -> String {
    match val {
        Some(v) if !v.is_empty() => v,
        _ if enriched => "-".into(),
        _ => "...".into(),
    }
}
