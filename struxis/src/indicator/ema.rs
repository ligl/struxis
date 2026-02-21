use crate::bar::SBar;

use super::core::Indicator;

#[derive(Debug, Clone)]
pub struct EmaIndicator {
    name: String,
    alpha: f64,
    value: Option<f64>,
}

impl EmaIndicator {
    pub fn new(period: usize) -> Self {
        assert!(period > 0, "period must be > 0");
        Self {
            name: format!("ema_{period}"),
            alpha: 2.0 / (period as f64 + 1.0),
            value: None,
        }
    }
}

impl Indicator for EmaIndicator {
    fn name(&self) -> &str {
        &self.name
    }

    fn reset(&mut self) {
        self.value = None;
    }

    fn update(&mut self, bar: &SBar) -> Option<f64> {
        let price = bar.close_price;
        self.value = Some(match self.value {
            None => price,
            Some(prev) => prev + self.alpha * (price - prev),
        });
        self.value
    }

    fn backfill(
        &mut self,
        _highs: &[f64],
        _lows: &[f64],
        closes: &[f64],
        start_index: usize,
    ) -> Vec<Option<f64>> {
        if start_index == 0 {
            self.reset();
        }
        let mut vals = Vec::new();
        for price in closes.iter().skip(start_index) {
            self.value = Some(match self.value {
                None => *price,
                Some(prev) => prev + self.alpha * (*price - prev),
            });
            vals.push(self.value);
        }
        vals
    }
}
