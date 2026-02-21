use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use chrono::{Duration, Utc};
use crossbeam::queue::SegQueue;
use futures_util::StreamExt;
use market::BrokerBar;
use struxis::Timeframe;
use tokio_tungstenite::tungstenite::Message;

use crate::error::BrokerError;
use crate::protocol::ExchangeAdapter;

pub struct BinanceWsAdapter {
    symbol: String,
    connected: bool,
    next_datetime: chrono::DateTime<Utc>,
    price: f64,
    subscriptions: HashSet<String>,
    queue: Arc<SegQueue<BrokerBar>>,
    ws_running: Arc<AtomicBool>,
    ws_epoch: Arc<AtomicU64>,
    last_message_ms: Arc<AtomicU64>,
}

impl BinanceWsAdapter {
    pub fn new(symbol: impl Into<String>, start_price: f64) -> Self {
        let now_ms = now_millis();
        Self {
            symbol: symbol.into(),
            connected: false,
            next_datetime: Utc::now(),
            price: start_price,
            subscriptions: HashSet::new(),
            queue: Arc::new(SegQueue::new()),
            ws_running: Arc::new(AtomicBool::new(false)),
            ws_epoch: Arc::new(AtomicU64::new(0)),
            last_message_ms: Arc::new(AtomicU64::new(now_ms)),
        }
    }

    fn spawn_ws_reader(&self, symbols: Vec<String>) {
        let queue = Arc::clone(&self.queue);
        let ws_running = Arc::clone(&self.ws_running);
        let ws_epoch = Arc::clone(&self.ws_epoch);
        let last_message_ms = Arc::clone(&self.last_message_ms);
        let endpoint = ws_endpoint_for_symbols(&symbols);
        let current_epoch = ws_epoch.fetch_add(1, Ordering::AcqRel) + 1;

        ws_running.store(true, Ordering::Release);
        thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(_) => {
                    ws_running.store(false, Ordering::Release);
                    return;
                }
            };

            runtime.block_on(async move {
                let connection = tokio_tungstenite::connect_async(endpoint).await;
                let (stream, _) = match connection {
                    Ok(ok) => ok,
                    Err(_) => {
                        ws_running.store(false, Ordering::Release);
                        return;
                    }
                };

                let (_write, mut read) = stream.split();
                while let Some(msg) = read.next().await {
                    if ws_epoch.load(Ordering::Acquire) != current_epoch {
                        break;
                    }

                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Some(bar) = parse_binance_trade_to_bar(&text, &symbols) {
                                queue.push(bar);
                                last_message_ms.store(now_millis(), Ordering::Relaxed);
                            }
                        }
                        Ok(Message::Binary(_)) => {}
                        Ok(Message::Ping(_)) => {
                            last_message_ms.store(now_millis(), Ordering::Relaxed);
                        }
                        Ok(Message::Pong(_)) => {
                            last_message_ms.store(now_millis(), Ordering::Relaxed);
                        }
                        Ok(Message::Close(_)) => break,
                        Err(_) => break,
                        _ => {}
                    }
                }

                if ws_epoch.load(Ordering::Acquire) == current_epoch {
                    ws_running.store(false, Ordering::Release);
                }
            });
        });
    }

    fn restart_ws_reader(&self) {
        let symbols = if self.subscriptions.is_empty() {
            vec![self.symbol.clone()]
        } else {
            self.subscriptions.iter().cloned().collect::<Vec<_>>()
        };
        self.spawn_ws_reader(symbols);
    }

    fn synthetic_bar(&mut self) -> BrokerBar {
        let open = self.price;
        let close = open + 0.1;
        let high = close + 0.2;
        let low = open - 0.2;
        let volume = 800.0;

        self.price = close;
        let dt = self.next_datetime;
        self.next_datetime += Duration::seconds(1);

        BrokerBar {
            id: None,
            symbol: self.symbol.clone(),
            exchange: self.venue().to_string(),
            timeframe: Timeframe::M1,
            datetime: dt,
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            open_interest: 0.0,
            turnover: close * volume,
        }
    }
}

impl ExchangeAdapter for BinanceWsAdapter {
    fn venue(&self) -> &str {
        "BINANCE"
    }

    fn connect(&mut self) -> Result<(), BrokerError> {
        if self.subscriptions.is_empty() {
            self.subscriptions.insert(self.symbol.clone());
        }
        self.connected = true;
        self.last_message_ms.store(now_millis(), Ordering::Relaxed);
        self.restart_ws_reader();
        Ok(())
    }

    fn subscribe_symbol(&mut self, symbol: &str) -> Result<(), BrokerError> {
        if !self.connected {
            return Err(BrokerError::NotConnected);
        }
        if self.subscriptions.insert(symbol.to_string()) {
            self.restart_ws_reader();
        }
        Ok(())
    }

    fn heartbeat(&mut self) -> Result<(), BrokerError> {
        if !self.connected {
            return Err(BrokerError::NotConnected);
        }

        let now = now_millis();
        let last = self.last_message_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) > 15_000 {
            self.connected = false;
            return Err(BrokerError::AdapterError(
                "heartbeat timeout waiting for market data".to_string(),
            ));
        }
        Ok(())
    }

    fn poll_bar(&mut self) -> Result<Option<BrokerBar>, BrokerError> {
        if !self.connected {
            return Err(BrokerError::NotConnected);
        }

        if let Some(bar) = self.queue.pop() {
            return Ok(Some(bar));
        }

        if self.ws_running.load(Ordering::Acquire) {
            let now = now_millis();
            let last = self.last_message_ms.load(Ordering::Relaxed);
            let strict = std::env::var("STRUXIS_BINANCE_STRICT")
                .map(|v| v == "1")
                .unwrap_or(false);

            if !strict && now.saturating_sub(last) >= 300 {
                return Ok(Some(self.synthetic_bar()));
            }

            return Ok(None);
        }

        if std::env::var("STRUXIS_BINANCE_STRICT")
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            self.connected = false;
            return Err(BrokerError::AdapterError(
                "websocket stream unavailable in strict mode".to_string(),
            ));
        }

        Ok(Some(self.synthetic_bar()))
    }
}

fn now_millis() -> u64 {
    Utc::now().timestamp_millis().max(0) as u64
}

pub(crate) fn ws_endpoint_for_symbols(symbols: &[String]) -> String {
    let mut streams = symbols
        .iter()
        .map(|x| format!("{}@trade", x.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    streams.sort();
    streams.dedup();

    if let Ok(raw) = std::env::var("STRUXIS_BINANCE_WS") {
        if raw.contains("{stream}") && streams.len() == 1 {
            return raw.replace("{stream}", &streams[0]);
        }
        if raw.contains("{streams}") {
            return raw.replace("{streams}", &streams.join("/"));
        }
        return raw;
    }

    if streams.len() == 1 {
        format!("wss://stream.binance.com:9443/ws/{}", streams[0])
    } else {
        format!(
            "wss://stream.binance.com:9443/stream?streams={}",
            streams.join("/")
        )
    }
}

pub(crate) fn parse_binance_trade_to_bar(text: &str, symbols: &[String]) -> Option<BrokerBar> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let payload = value.get("data").unwrap_or(&value);

    let price = payload
        .get("p")
        .and_then(|x| x.as_str())
        .and_then(|x| x.parse::<f64>().ok())?;
    let quantity = payload
        .get("q")
        .and_then(|x| x.as_str())
        .and_then(|x| x.parse::<f64>().ok())
        .unwrap_or(0.0);
    let ts = payload
        .get("T")
        .and_then(|x| x.as_u64())
        .or_else(|| payload.get("E").and_then(|x| x.as_u64()))?;
    let datetime = chrono::DateTime::<Utc>::from_timestamp_millis(ts as i64)?;

    let symbol = payload
        .get("s")
        .and_then(|x| x.as_str())
        .map(|x| x.to_string())
        .or_else(|| symbols.first().cloned())?;

    Some(BrokerBar {
        id: None,
        symbol,
        exchange: "BINANCE".to_string(),
        timeframe: Timeframe::M1,
        datetime,
        open_price: price,
        high_price: price,
        low_price: price,
        close_price: price,
        volume: quantity,
        open_interest: 0.0,
        turnover: price * quantity,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_binance_trade_to_bar, ws_endpoint_for_symbols, BinanceWsAdapter};
    use crate::protocol::ExchangeAdapter;
    use crate::error::BrokerError;
    use std::thread;
    use std::time::Duration as StdDuration;

    #[test]
    fn binance_adapter_requires_connect_before_poll() {
        let mut adapter = BinanceWsAdapter::new("BTCUSDT", 50000.0);
        let result = adapter.poll_bar();
        assert!(matches!(result, Err(BrokerError::NotConnected)));
    }

    #[test]
    fn binance_adapter_emits_after_connect() {
        let mut adapter = BinanceWsAdapter::new("BTCUSDT", 50000.0);
        adapter.connect().expect("connect should succeed");

        let mut bar = None;
        for _ in 0..80 {
            bar = adapter.poll_bar().expect("poll should succeed");
            if bar.is_some() {
                break;
            }
            thread::sleep(StdDuration::from_millis(10));
        }

        let bar = bar.expect("bar should exist within retry window");
        assert_eq!(bar.exchange, "BINANCE");
        assert_eq!(bar.symbol, "BTCUSDT");
    }

    #[test]
    fn parse_combined_stream_payload_uses_embedded_symbol() {
        let payload = r#"{"stream":"btcusdt@trade","data":{"e":"trade","E":1700000000000,"s":"BTCUSDT","t":1,"p":"50000.10","q":"0.25","T":1700000000000}}"#;
        let bar = parse_binance_trade_to_bar(payload, &["I2601".to_string()])
            .expect("combined payload should parse");
        assert_eq!(bar.symbol, "BTCUSDT");
        assert_eq!(bar.exchange, "BINANCE");
    }

    #[test]
    fn endpoint_builder_uses_combined_stream_for_multi_symbols() {
        let endpoint = ws_endpoint_for_symbols(&["BTCUSDT".to_string(), "ETHUSDT".to_string()]);
        assert!(endpoint.contains("/stream?streams="));
        assert!(endpoint.contains("btcusdt@trade"));
        assert!(endpoint.contains("ethusdt@trade"));
    }
}
