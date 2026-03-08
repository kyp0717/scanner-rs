use iced::widget::{column, container, table, text};
use iced::{Element, Length};

use crate::gui::app::{App, Message};
use crate::gui::theme::{self, Colors};

/// Sample row data for the test table.
#[derive(Debug, Clone)]
struct StockRow {
    symbol: String,
    price: f64,
    change_pct: f64,
    volume: i64,
    scanner_hits: u32,
    sector: String,
}

fn sample_data() -> Vec<StockRow> {
    vec![
        StockRow { symbol: "AAPL".into(), price: 178.50, change_pct: 2.3, volume: 45_000_000, scanner_hits: 5, sector: "Technology".into() },
        StockRow { symbol: "TSLA".into(), price: 245.80, change_pct: -1.5, volume: 62_000_000, scanner_hits: 7, sector: "Consumer Cyclical".into() },
        StockRow { symbol: "NVDA".into(), price: 890.25, change_pct: 4.1, volume: 38_000_000, scanner_hits: 8, sector: "Technology".into() },
        StockRow { symbol: "GME".into(), price: 22.40, change_pct: 15.8, volume: 120_000_000, scanner_hits: 6, sector: "Consumer Cyclical".into() },
        StockRow { symbol: "AMC".into(), price: 5.12, change_pct: 8.4, volume: 85_000_000, scanner_hits: 4, sector: "Communication".into() },
        StockRow { symbol: "PLTR".into(), price: 42.30, change_pct: 3.2, volume: 28_000_000, scanner_hits: 3, sector: "Technology".into() },
        StockRow { symbol: "SOFI".into(), price: 9.85, change_pct: -2.1, volume: 15_000_000, scanner_hits: 2, sector: "Financial".into() },
        StockRow { symbol: "MARA".into(), price: 18.70, change_pct: 12.5, volume: 42_000_000, scanner_hits: 5, sector: "Financial".into() },
        StockRow { symbol: "RIOT".into(), price: 11.30, change_pct: 9.7, volume: 31_000_000, scanner_hits: 4, sector: "Financial".into() },
        StockRow { symbol: "NIO".into(), price: 6.45, change_pct: -3.8, volume: 55_000_000, scanner_hits: 3, sector: "Consumer Cyclical".into() },
    ]
}

fn fmt_volume(vol: i64) -> String {
    if vol >= 1_000_000 {
        format!("{:.1}M", vol as f64 / 1_000_000.0)
    } else if vol >= 1_000 {
        format!("{:.0}K", vol as f64 / 1_000.0)
    } else {
        format!("{vol}")
    }
}

impl App {
    pub fn test_view(&self) -> Element<Message> {
        let fs = self.font_size;
        let data = sample_data();

        let sym_col = table::column(
            text("Symbol").size(fs).style(theme::text_color(Colors::YELLOW)),
            |row: StockRow| -> Element<Message> {
                text(row.symbol)
                    .size(fs)
                    .style(theme::text_color(Colors::CYAN))
                    .into()
            },
        )
        .width(Length::FillPortion(2));

        let price_col = table::column(
            text("Price").size(fs).style(theme::text_color(Colors::YELLOW)),
            move |row: StockRow| -> Element<Message> {
                text(format!("${:.2}", row.price)).size(fs).into()
            },
        )
        .width(Length::FillPortion(2));

        let chg_col = table::column(
            text("Chg%").size(fs).style(theme::text_color(Colors::YELLOW)),
            move |row: StockRow| -> Element<Message> {
                let color = if row.change_pct >= 0.0 {
                    Colors::GREEN
                } else {
                    Colors::RED
                };
                text(format!("{:+.1}%", row.change_pct))
                    .size(fs)
                    .style(theme::text_color(color))
                    .into()
            },
        )
        .width(Length::FillPortion(2));

        let vol_col = table::column(
            text("Volume").size(fs).style(theme::text_color(Colors::YELLOW)),
            move |row: StockRow| -> Element<Message> {
                text(fmt_volume(row.volume)).size(fs).into()
            },
        )
        .width(Length::FillPortion(2));

        let hits_col = table::column(
            text("Hits").size(fs).style(theme::text_color(Colors::YELLOW)),
            move |row: StockRow| -> Element<Message> {
                text(format!("{}/8", row.scanner_hits)).size(fs).into()
            },
        )
        .width(Length::FillPortion(1));

        let sector_col = table::column(
            text("Sector").size(fs).style(theme::text_color(Colors::YELLOW)),
            move |row: StockRow| -> Element<Message> {
                text(row.sector)
                    .size(fs)
                    .style(theme::text_dim)
                    .into()
            },
        )
        .width(Length::FillPortion(3));

        let tbl = table::table(
            [sym_col, price_col, chg_col, vol_col, hits_col, sector_col],
            data,
        )
        .padding(8)
        .separator(1);

        let content = column![
            text("Table Widget Test")
                .size(fs + 4)
                .style(theme::text_color(Colors::CYAN)),
            text("iced 0.14 native Table widget with sample stock data")
                .size(if fs > 3 { fs - 2 } else { fs })
                .style(theme::text_dim),
            tbl,
        ]
        .spacing(8)
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fill);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(4)
            .style(theme::card_container)
            .into()
    }
}
