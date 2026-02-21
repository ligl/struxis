use chrono::{Duration, Utc};
use market::BrokerBar;
use struxis::Timeframe;

use crate::protocol::ExchangeFeed;

pub struct CtpFeed {
    symbol: String,
    exchange: String,
    next_datetime: chrono::DateTime<Utc>,
    price: f64,
}

impl CtpFeed {
    pub fn new(symbol: impl Into<String>, exchange: impl Into<String>, start_price: f64) -> Self {
        Self {
            symbol: symbol.into(),
            exchange: exchange.into(),
            next_datetime: Utc::now(),
            price: start_price,
        }
    }
}

impl ExchangeFeed for CtpFeed {
    fn next_bar(&mut self) -> Option<BrokerBar> {
        let open = self.price;
        let close = open + 0.2;
        let high = close + 0.4;
        let low = open - 0.3;
        let volume = 1000.0;
        let open_interest = 5000.0;

        self.price = close;
        let dt = self.next_datetime;
        self.next_datetime += Duration::minutes(1);

        Some(BrokerBar {
            id: None,
            symbol: self.symbol.clone(),
            exchange: self.exchange.clone(),
            timeframe: Timeframe::M1,
            datetime: dt,
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            open_interest,
            turnover: close * volume,
        })
    }
}
