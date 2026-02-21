use chrono::{DateTime, Utc};

use crate::constant::{FractalType, Timeframe};

#[derive(Debug, Clone)]
pub struct SBar {
    pub id: Option<u64>,
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

impl SBar {
    pub fn body(&self) -> f64 {
        (self.close_price - self.open_price).abs()
    }

    pub fn upper_shadow(&self) -> f64 {
        self.high_price - self.close_price.max(self.open_price)
    }

    pub fn lower_shadow(&self) -> f64 {
        self.close_price.min(self.open_price) - self.low_price
    }

    pub fn total_range(&self) -> f64 {
        self.high_price - self.low_price
    }
}

#[derive(Debug, Clone)]
pub struct CBar {
    pub id: Option<u64>,
    pub sbar_start_id: u64,
    pub sbar_end_id: u64,
    pub high_price: f64,
    pub low_price: f64,
    pub fractal_type: FractalType,
    pub created_at: DateTime<Utc>,
}

impl CBar {
    pub fn is_inclusive(&self, other: &Self) -> bool {
        (self.high_price >= other.high_price && self.low_price <= other.low_price)
            || (self.high_price <= other.high_price && self.low_price >= other.low_price)
    }
}

#[derive(Debug, Clone)]
pub struct Fractal {
    pub left: CBar,
    pub middle: CBar,
    pub right: CBar,
}

impl Fractal {
    pub fn verify(left: &CBar, middle: &CBar, right: &CBar) -> FractalType {
        let is_top = middle.high_price >= left.high_price
            && middle.high_price >= right.high_price
            && middle.low_price >= left.low_price
            && middle.low_price >= right.low_price;
        if is_top {
            return FractalType::Top;
        }

        let is_bottom = middle.high_price <= left.high_price
            && middle.high_price <= right.high_price
            && middle.low_price <= left.low_price
            && middle.low_price <= right.low_price;
        if is_bottom {
            return FractalType::Bottom;
        }
        FractalType::None
    }
}
