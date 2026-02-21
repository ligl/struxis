use chrono::{DateTime, Timelike, Utc};

use crate::bar::SBar;
use crate::constant::Timeframe;

#[derive(Debug, Clone)]
pub struct TickInput {
    pub symbol: String,
    pub exchange: String,
    pub datetime: DateTime<Utc>,
    pub last_price: f64,
    pub volume: f64,
    pub turnover: f64,
    pub open_interest: f64,
}

pub struct TickBarAggregator {
    current_bar: Option<SBar>,
    last_volume: Option<f64>,
    last_turnover: Option<f64>,
}

impl Default for TickBarAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl TickBarAggregator {
    pub fn new() -> Self {
        Self {
            current_bar: None,
            last_volume: None,
            last_turnover: None,
        }
    }

    pub fn update(&mut self, tick: TickInput) -> Option<SBar> {
        let minute_dt = tick
            .datetime
            .with_second(0)
            .and_then(|x| x.with_nanosecond(0))
            .unwrap_or(tick.datetime);

        let delta_volume = match self.last_volume {
            Some(prev) if tick.volume >= prev => tick.volume - prev,
            _ => 0.0,
        };
        let delta_turnover = match self.last_turnover {
            Some(prev) if tick.turnover >= prev => tick.turnover - prev,
            _ => 0.0,
        };

        self.last_volume = Some(tick.volume);
        self.last_turnover = Some(tick.turnover);

        match self.current_bar.as_mut() {
            None => {
                self.current_bar = Some(SBar {
                    id: None,
                    symbol: tick.symbol,
                    exchange: tick.exchange,
                    timeframe: Timeframe::M1,
                    datetime: minute_dt,
                    open_price: tick.last_price,
                    high_price: tick.last_price,
                    low_price: tick.last_price,
                    close_price: tick.last_price,
                    volume: delta_volume,
                    open_interest: tick.open_interest,
                    turnover: delta_turnover,
                });
                None
            }
            Some(bar) => {
                if bar.datetime == minute_dt {
                    bar.high_price = bar.high_price.max(tick.last_price);
                    bar.low_price = bar.low_price.min(tick.last_price);
                    bar.close_price = tick.last_price;
                    bar.volume += delta_volume;
                    bar.turnover += delta_turnover;
                    bar.open_interest = tick.open_interest;
                    None
                } else {
                    let finished = self.current_bar.take();
                    self.current_bar = Some(SBar {
                        id: None,
                        symbol: tick.symbol,
                        exchange: tick.exchange,
                        timeframe: Timeframe::M1,
                        datetime: minute_dt,
                        open_price: tick.last_price,
                        high_price: tick.last_price,
                        low_price: tick.last_price,
                        close_price: tick.last_price,
                        volume: delta_volume,
                        open_interest: tick.open_interest,
                        turnover: delta_turnover,
                    });
                    finished
                }
            }
        }
    }

    pub fn flush(&mut self) -> Option<SBar> {
        self.current_bar.take()
    }
}

pub struct BarWindowAggregator {
    timeframe: Timeframe,
    window: usize,
    buffer: Vec<SBar>,
}

impl BarWindowAggregator {
    pub fn new(timeframe: Timeframe) -> Option<Self> {
        let window = match timeframe {
            Timeframe::M1 => 1,
            Timeframe::M5 => 5,
            Timeframe::M15 => 15,
            Timeframe::H1 => 60,
            Timeframe::D1 => 1440,
        };
        if window <= 1 {
            return None;
        }
        Some(Self {
            timeframe,
            window,
            buffer: Vec::with_capacity(window),
        })
    }

    pub fn update(&mut self, m1_bar: SBar) -> Option<SBar> {
        self.buffer.push(m1_bar);
        if self.buffer.len() < self.window {
            return None;
        }

        let first = self.buffer.first()?.clone();
        let last = self.buffer.last()?.clone();

        let high = self
            .buffer
            .iter()
            .map(|x| x.high_price)
            .fold(f64::MIN, f64::max);
        let low = self
            .buffer
            .iter()
            .map(|x| x.low_price)
            .fold(f64::MAX, f64::min);
        let volume: f64 = self.buffer.iter().map(|x| x.volume).sum();
        let turnover: f64 = self.buffer.iter().map(|x| x.turnover).sum();

        self.buffer.clear();

        Some(SBar {
            id: None,
            symbol: first.symbol,
            exchange: first.exchange,
            timeframe: self.timeframe,
            datetime: last.datetime,
            open_price: first.open_price,
            high_price: high,
            low_price: low,
            close_price: last.close_price,
            volume,
            open_interest: last.open_interest,
            turnover,
        })
    }
}
