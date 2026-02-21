use std::collections::HashMap;

use crate::bar::SBar;

use super::core::Indicator;

#[derive(Default)]
pub struct IndicatorManager {
    indicators: Vec<Box<dyn Indicator>>,
    outputs: HashMap<String, Vec<Option<f64>>>,
    dirty_index: usize,
}

impl IndicatorManager {
    pub fn register(&mut self, indicator: Box<dyn Indicator>) {
        let name = indicator.name().to_string();
        if self.outputs.contains_key(&name) {
            return;
        }
        self.outputs.insert(name, Vec::new());
        self.indicators.push(indicator);
    }

    pub fn update(&mut self, bar: &SBar) -> HashMap<String, Option<f64>> {
        let mut row = HashMap::new();
        for indicator in &mut self.indicators {
            let name = indicator.name().to_string();
            let value = indicator.update(bar);
            self.outputs.entry(name.clone()).or_default().push(value);
            row.insert(name, value);
        }
        self.dirty_index = self.outputs.values().next().map_or(0, Vec::len);
        row
    }

    pub fn mark_dirty(&mut self, idx: usize) {
        self.dirty_index = self.dirty_index.min(idx);
    }
}
