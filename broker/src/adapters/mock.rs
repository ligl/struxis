use chrono::{Duration, Utc};
use market::BrokerBar;
use struxis::Timeframe;

use crate::{BrokerError, ExchangeAdapter};

pub struct MockAdapter {
    symbol: String,
    exchange: String,
    next_datetime: chrono::DateTime<Utc>,
    price: f64,
    connected: bool,
}

impl MockAdapter {
    pub fn new(symbol: impl Into<String>, exchange: impl Into<String>, start_price: f64) -> Self {
        Self {
            symbol: symbol.into(),
            exchange: exchange.into(),
            next_datetime: Utc::now(),
            price: start_price,
            connected: false,
        }
    }
}

impl ExchangeAdapter for MockAdapter {
    fn venue(&self) -> &str {
        &self.exchange
    }

    fn connect(&mut self) -> Result<(), BrokerError> {
        self.connected = true;
        Ok(())
    }

    fn poll_bar(&mut self) -> Result<Option<BrokerBar>, BrokerError> {
        if !self.connected {
            return Err(BrokerError::NotConnected);
        }

        let open = self.price;
        let close = open + 0.2;
        let high = close + 0.4;
        let low = open - 0.3;
        let volume = 1000.0;
        let open_interest = 5000.0;

        self.price = close;
        let dt = self.next_datetime;
        self.next_datetime += Duration::minutes(1);

        Ok(Some(BrokerBar {
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
        }))
    }

    fn subscribe_symbol(&mut self, symbol: &str) -> Result<(), BrokerError> {
        self.symbol = symbol.to_string();
        Ok(())
    }

    fn heartbeat(&mut self) -> Result<(), BrokerError> {
        if self.connected {
            Ok(())
        } else {
            Err(BrokerError::NotConnected)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MockAdapter;
    use crate::ExchangeAdapter;

    #[test]
    fn mock_adapter_requires_connect_and_emits_bar() {
        let mut adapter = MockAdapter::new("I2601", "MOCK", 100.0);

        assert!(adapter.poll_bar().is_err());

        adapter.connect().expect("mock adapter should connect");
        let bar = adapter
            .poll_bar()
            .expect("poll should succeed")
            .expect("bar should exist");

        assert_eq!(bar.symbol, "I2601");
        assert_eq!(bar.exchange, "MOCK");
		assert!(bar.close_price > bar.open_price);
    }
}
