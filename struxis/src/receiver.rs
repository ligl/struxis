use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

use crate::bar::SBar;
use crate::constant::{DataError, Timeframe};
use crate::mtc::MultiTimeframeContext;
use crate::tick::{BarWindowAggregator, TickBarAggregator, TickInput};

/// 标准化的市场 bar 输入。
#[derive(Debug, Clone)]
pub struct MarketBarInput {
    pub symbol: String,
    pub exchange: String,
    pub timeframe: Timeframe,
    pub datetime: DateTime<Utc>,
    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub close_price: f64,
    pub volume: f64,
    pub open_interest: f64,
    pub turnover: f64,
}

impl MarketBarInput {
    pub fn into_sbar(self) -> SBar {
        SBar {
            id: None,
            symbol: self.symbol,
            exchange: self.exchange,
            timeframe: self.timeframe,
            datetime: self.datetime,
            open_price: self.open_price,
            high_price: self.high_price,
            low_price: self.low_price,
            close_price: self.close_price,
            volume: self.volume,
            open_interest: self.open_interest,
            turnover: self.turnover,
        }
    }
}

pub struct DataReceiver {
    mtc: MultiTimeframeContext,
    registered_timeframes: HashSet<Timeframe>,
    tick_aggregators: HashMap<String, TickBarAggregator>,
    window_aggregators: HashMap<Timeframe, BarWindowAggregator>,
}

impl DataReceiver {
    pub fn new(mtc: MultiTimeframeContext) -> Self {
        Self {
            mtc,
            registered_timeframes: HashSet::new(),
            tick_aggregators: HashMap::new(),
            window_aggregators: HashMap::new(),
        }
    }

    pub fn register_timeframe(&mut self, timeframe: Timeframe) {
        self.mtc.register(timeframe);
        self.registered_timeframes.insert(timeframe);
        if let Some(agg) = BarWindowAggregator::new(timeframe) {
            self.window_aggregators.insert(timeframe, agg);
        }
    }

    pub fn ingest_bar(&mut self, input: MarketBarInput) {
        self.mtc.append(input.timeframe, input.into_sbar());
    }

    pub fn ingest_batch(&mut self, inputs: Vec<MarketBarInput>) {
        for input in inputs {
            self.ingest_bar(input);
        }
    }

    pub fn ingest_tick(&mut self, tick: TickInput) -> usize {
        let key = format!("{}::{}", tick.symbol, tick.exchange);
        let aggregator = self
            .tick_aggregators
            .entry(key)
            .or_insert_with(TickBarAggregator::new);

        let mut emitted = 0usize;
        if let Some(m1_bar) = aggregator.update(tick) {
            emitted += self.forward_m1_bar(m1_bar);
        }
        emitted
    }

    pub fn flush_ticks(&mut self) -> usize {
        let mut emitted = 0usize;
        let mut bars = Vec::new();
        for agg in self.tick_aggregators.values_mut() {
            if let Some(bar) = agg.flush() {
                bars.push(bar);
            }
        }
        for bar in bars {
            emitted += self.forward_m1_bar(bar);
        }
        emitted
    }

    fn forward_m1_bar(&mut self, m1_bar: SBar) -> usize {
        let mut emitted = 0usize;
        if self.registered_timeframes.contains(&Timeframe::M1) {
            self.mtc.append(Timeframe::M1, m1_bar.clone());
            emitted += 1;
        }

        for (tf, agg) in &mut self.window_aggregators {
            if let Some(tf_bar) = agg.update(m1_bar.clone()) {
                self.mtc.append(*tf, tf_bar);
                emitted += 1;
            }
        }

        emitted
    }

    pub fn ingest_csv(
        &mut self,
        file_path: impl AsRef<std::path::Path>,
        symbol: impl Into<String>,
        exchange: impl Into<String>,
        timeframe: Timeframe,
    ) -> Result<usize, DataError> {
        let inputs = load_market_bar_inputs(file_path, symbol, exchange, timeframe)?;
        let count = inputs.len();
        self.ingest_batch(inputs);
        Ok(count)
    }

    pub fn mtc(&self) -> &MultiTimeframeContext {
        &self.mtc
    }

    pub fn mtc_mut(&mut self) -> &mut MultiTimeframeContext {
        &mut self.mtc
    }
}


#[derive(Debug, Deserialize)]
struct CsvBarRow {
    datetime: String,
    #[serde(alias = "open")]
    open_price: f64,
    #[serde(alias = "high")]
    high_price: f64,
    #[serde(alias = "low")]
    low_price: f64,
    #[serde(alias = "close")]
    close_price: f64,
    #[serde(default)]
    volume: f64,
    #[serde(default)]
    open_interest: f64,
    #[serde(default, alias = "money")]
    turnover: f64,
}

fn load_market_bar_inputs(
    file_path: impl AsRef<Path>,
    symbol: impl Into<String>,
    exchange: impl Into<String>,
    timeframe: Timeframe,
) -> Result<Vec<MarketBarInput>, DataError> {
    let symbol = symbol.into();
    let exchange = exchange.into();

    let mut reader = csv::Reader::from_path(file_path)?;
    let mut out = Vec::new();

    for row in reader.deserialize::<CsvBarRow>() {
        let row = row?;
        let datetime = parse_datetime(&row.datetime)?;
        out.push(MarketBarInput {
            symbol: symbol.clone(),
            exchange: exchange.clone(),
            timeframe,
            datetime,
            open_price: row.open_price,
            high_price: row.high_price,
            low_price: row.low_price,
            close_price: row.close_price,
            volume: row.volume,
            open_interest: row.open_interest,
            turnover: row.turnover,
        });
    }

    Ok(out)
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, DataError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }

    let patterns = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y/%m/%d %H:%M:%S%.f",
        "%Y%m%d%H%M%S%.f",
    ];

    for pattern in patterns {
        if let Ok(dt) = NaiveDateTime::parse_from_str(value, pattern) {
            return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
        }
    }

    Err(DataError::InvalidDatetime(value.to_string()))
}
