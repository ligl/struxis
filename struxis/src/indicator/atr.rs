use crate::bar::SBar;

use super::core::Indicator;

#[derive(Debug, Clone)]
pub struct Atr {
    name: String,
    period: usize,
    value: Option<f64>,
    prev_close: Option<f64>,
}

impl Atr {
    pub fn new(period: usize) -> Self {
        assert!(period > 0, "period must be > 0");
        Self {
            name: format!("atr_{period}"),
            period,
            value: None,
            prev_close: None,
        }
    }

    fn true_range(&self, high: f64, low: f64, prev_close: Option<f64>) -> f64 {
        if let Some(prev_close) = prev_close {
            (high - low)
                .max((high - prev_close).abs())
                .max((low - prev_close).abs())
        } else {
            high - low
        }
    }
}

impl Indicator for Atr {
    fn name(&self) -> &str {
        &self.name
    }

    fn reset(&mut self) {
        self.value = None;
        self.prev_close = None;
    }

    fn update(&mut self, bar: &SBar) -> Option<f64> {
        let tr = self.true_range(bar.high_price, bar.low_price, self.prev_close);
        self.prev_close = Some(bar.close_price);
        self.value = Some(match self.value {
            None => tr,
            Some(prev) => (prev * (self.period as f64 - 1.0) + tr) / self.period as f64,
        });
        self.value
    }

    fn backfill(
        &mut self,
        highs: &[f64],
        lows: &[f64],
        closes: &[f64],
        start_index: usize,
    ) -> Vec<Option<f64>> {
        if start_index == 0 {
            self.reset();
        }
        let mut vals = Vec::new();
        for i in start_index..closes.len() {
            let tr = self.true_range(highs[i], lows[i], self.prev_close);
            self.prev_close = Some(closes[i]);
            self.value = Some(match self.value {
                None => tr,
                Some(prev) => (prev * (self.period as f64 - 1.0) + tr) / self.period as f64,
            });
            vals.push(self.value);
        }
        vals
    }
}
