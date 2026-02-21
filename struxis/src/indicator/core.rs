use crate::bar::SBar;

pub trait Indicator: Send {
    fn name(&self) -> &str;
    fn reset(&mut self);
    fn update(&mut self, bar: &SBar) -> Option<f64>;
    fn backfill(
        &mut self,
        highs: &[f64],
        lows: &[f64],
        closes: &[f64],
        start_index: usize,
    ) -> Vec<Option<f64>>;
}
