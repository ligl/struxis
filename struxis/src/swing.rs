use chrono::{DateTime, Utc};
use polars::df;
use polars::prelude::DataFrame;

use crate::bar::{CBar, Fractal};
use crate::constant::{Direction, FractalType};
use crate::utils::{approx_eq_f64, first_changed_id};

#[derive(Debug, Clone)]
pub struct Swing {
    pub id: Option<u64>,
    pub direction: Direction,
    pub cbar_start_id: u64,
    pub cbar_end_id: u64,
    pub sbar_start_id: u64,
    pub sbar_end_id: u64,
    pub high_price: f64,
    pub low_price: f64,
    pub span: usize,
    pub volume: f64,
    pub start_oi: f64,
    pub end_oi: f64,
    pub is_completed: bool,
    pub created_at: DateTime<Utc>,
}

impl Swing {
    pub fn distance(&self) -> f64 {
        self.high_price - self.low_price
    }

    pub fn overlap(&self, other: &Swing) -> bool {
        self.low_price.max(other.low_price) <= self.high_price.min(other.high_price)
    }
}

pub struct SwingManager {
    rows: Vec<Swing>,
    id_cursor: u64,
    df_cache: DataFrame,
    backtrack_id: Option<u64>,
}

impl Default for SwingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SwingManager {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            id_cursor: 0,
            df_cache: DataFrame::default(),
            backtrack_id: None,
        }
    }

    pub fn on_new_fractal(&mut self, fractal: &Fractal, right_cbar: &CBar) -> Option<Swing> {
        let fractal_type = Fractal::verify(&fractal.left, &fractal.middle, &fractal.right);
        if fractal_type == FractalType::None {
            return None;
        }

        if self.rows.is_empty() {
            let swing = self.append_from_fractal(fractal, right_cbar, false);
            self.rebuild_cache();
            return Some(swing);
        }

        if let Some(active) = self.rows.last_mut() {
            active.cbar_end_id = fractal.middle.id.unwrap_or(active.cbar_end_id);
            active.sbar_end_id = fractal.middle.sbar_end_id;
            active.high_price = active.high_price.max(fractal.middle.high_price);
            active.low_price = active.low_price.min(fractal.middle.low_price);
            active.is_completed = true;
        }

        let _ = self.append_from_fractal(fractal, right_cbar, false);
        self.rebuild_cache();
        self.rows.last().cloned()
    }

    pub fn rebuild_from_cbars(&mut self, cbars: &[CBar]) -> Option<u64> {
        self.rebuild_from_cbars_with_backtrack(cbars, None)
    }

    pub fn rebuild_from_cbars_with_backtrack(
        &mut self,
        cbars: &[CBar],
        cbar_backtrack_id: Option<u64>,
    ) -> Option<u64> {
        let previous_rows = self.rows.clone();
        self.rows.clear();
        self.id_cursor = 0;

        for pivot in 1..cbars.len().saturating_sub(1) {
            let left = cbars[pivot - 1].clone();
            let middle = cbars[pivot].clone();
            let right = cbars[pivot + 1].clone();
            let fractal = Fractal { left, middle, right };
            let fractal_type = Fractal::verify(&fractal.left, &fractal.middle, &fractal.right);
            if fractal_type == FractalType::None {
                continue;
            }

            if self.rows.is_empty() {
                let _ = self.append_from_fractal(&fractal, &fractal.right, false);
                continue;
            }

            let active_index = self.rows.len() - 1;
            let mut active = self.rows[active_index].clone();
            active.high_price = active.high_price.max(fractal.high_price());
            active.low_price = active.low_price.min(fractal.low_price());

            let prev_completed = self.rows.iter().rev().find(|x| x.is_completed).cloned();
            let start_fractal = find_fractal_by_middle_id(cbars, active.cbar_start_id);

            if determine_swing(
                start_fractal.as_ref(),
                &fractal,
                &active,
                prev_completed.as_ref(),
            ) {
                active.cbar_end_id = fractal.middle.id.unwrap_or(active.cbar_end_id);
                active.sbar_end_id = fractal.middle.sbar_end_id;
                active.is_completed = true;
                self.rows[active_index] = active.clone();

                self.id_cursor += 1;
                let new_active = Swing {
                    id: Some(self.id_cursor),
                    direction: active.direction.opposite(),
                    cbar_start_id: active.cbar_end_id,
                    cbar_end_id: fractal.right.id.unwrap_or_default(),
                    sbar_start_id: active.sbar_end_id,
                    sbar_end_id: fractal.right.sbar_end_id,
                    high_price: if active.direction == Direction::Up {
                        active.high_price
                    } else {
                        fractal.right.high_price
                    },
                    low_price: if active.direction == Direction::Down {
                        active.low_price
                    } else {
                        fractal.right.low_price
                    },
                    span: 1,
                    volume: 0.0,
                    start_oi: 0.0,
                    end_oi: 0.0,
                    is_completed: false,
                    created_at: Utc::now(),
                };
                self.rows.push(new_active);
            } else {
                active.cbar_end_id = fractal.right.id.unwrap_or(active.cbar_end_id);
                active.sbar_end_id = fractal.right.sbar_end_id;
                active.is_completed = false;
                self.rows[active_index] = active;
            }
        }

        self.backtrack_id = if cbar_backtrack_id.is_some() {
            first_changed_id(
                &previous_rows,
                &self.rows,
                |x| x.id,
                swing_eq_wo_created_at,
                true,
            )
            .or_else(|| self.rows.first().and_then(|x| x.id))
        } else {
            first_changed_id(
                &previous_rows,
                &self.rows,
                |x| x.id,
                swing_eq_wo_created_at,
                false,
            )
        };
        self.rebuild_cache();
        self.backtrack_id
    }

    fn append_from_fractal(&mut self, fractal: &Fractal, right_cbar: &CBar, completed: bool) -> Swing {
        self.id_cursor += 1;
        let direction = match Fractal::verify(&fractal.left, &fractal.middle, &fractal.right) {
            FractalType::Top => Direction::Down,
            FractalType::Bottom => Direction::Up,
            FractalType::None => Direction::None,
        };

        let swing = Swing {
            id: Some(self.id_cursor),
            direction,
            cbar_start_id: fractal.middle.id.unwrap_or_default(),
            cbar_end_id: right_cbar.id.unwrap_or_default(),
            sbar_start_id: fractal.middle.sbar_start_id,
            sbar_end_id: right_cbar.sbar_end_id,
            high_price: fractal.middle.high_price.max(right_cbar.high_price),
            low_price: fractal.middle.low_price.min(right_cbar.low_price),
            span: 1,
            volume: 0.0,
            start_oi: 0.0,
            end_oi: 0.0,
            is_completed: completed,
            created_at: Utc::now(),
        };
        self.rows.push(swing.clone());
        swing
    }

    pub fn last_n(&self, n: usize) -> Vec<Swing> {
        self.rows
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
    }

    pub fn dataframe(&self) -> DataFrame {
        self.df_cache.clone()
    }

    pub fn all_rows(&self) -> Vec<Swing> {
        self.rows.clone()
    }

    pub fn backtrack_id(&self) -> Option<u64> {
        self.backtrack_id
    }

    fn rebuild_cache(&mut self) {
        let ids: Vec<u64> = self
            .rows
            .iter()
            .map(|x| x.id.unwrap_or_default())
            .collect();
        let directions: Vec<i8> = self
            .rows
            .iter()
            .map(|x| match x.direction {
                Direction::Up => 1,
                Direction::Down => -1,
                Direction::Range => 2,
                Direction::None => 0,
            })
            .collect();
        let cbar_start_id: Vec<u64> = self.rows.iter().map(|x| x.cbar_start_id).collect();
        let cbar_end_id: Vec<u64> = self.rows.iter().map(|x| x.cbar_end_id).collect();
        let sbar_start_id: Vec<u64> = self.rows.iter().map(|x| x.sbar_start_id).collect();
        let sbar_end_id: Vec<u64> = self.rows.iter().map(|x| x.sbar_end_id).collect();
        let high_price: Vec<f64> = self.rows.iter().map(|x| x.high_price).collect();
        let low_price: Vec<f64> = self.rows.iter().map(|x| x.low_price).collect();
        let span: Vec<u32> = self.rows.iter().map(|x| x.span as u32).collect();
        let volume: Vec<f64> = self.rows.iter().map(|x| x.volume).collect();
        let start_oi: Vec<f64> = self.rows.iter().map(|x| x.start_oi).collect();
        let end_oi: Vec<f64> = self.rows.iter().map(|x| x.end_oi).collect();
        let is_completed: Vec<bool> = self.rows.iter().map(|x| x.is_completed).collect();
        let created_at: Vec<i64> = self
            .rows
            .iter()
            .map(|x| x.created_at.timestamp_millis())
            .collect();

        self.df_cache = df!(
            "id" => ids,
            "direction" => directions,
            "cbar_start_id" => cbar_start_id,
            "cbar_end_id" => cbar_end_id,
            "sbar_start_id" => sbar_start_id,
            "sbar_end_id" => sbar_end_id,
            "high_price" => high_price,
            "low_price" => low_price,
            "span" => span,
            "volume" => volume,
            "start_oi" => start_oi,
            "end_oi" => end_oi,
            "is_completed" => is_completed,
            "created_at" => created_at
        )
        .expect("failed to rebuild swing dataframe cache");
    }
}

trait FractalExt {
    fn fractal_type(&self) -> FractalType;
    fn high_price(&self) -> f64;
    fn low_price(&self) -> f64;
}

impl FractalExt for Fractal {
    fn fractal_type(&self) -> FractalType {
        Fractal::verify(&self.left, &self.middle, &self.right)
    }

    fn high_price(&self) -> f64 {
        self.middle.high_price
    }

    fn low_price(&self) -> f64 {
        self.middle.low_price
    }
}

fn find_fractal_by_middle_id(cbars: &[CBar], middle_id: u64) -> Option<Fractal> {
    if cbars.len() < 3 {
        return None;
    }
    for pivot in 1..cbars.len() - 1 {
        if cbars[pivot].id == Some(middle_id) {
            let fractal = Fractal {
                left: cbars[pivot - 1].clone(),
                middle: cbars[pivot].clone(),
                right: cbars[pivot + 1].clone(),
            };
            if fractal.fractal_type() != FractalType::None {
                return Some(fractal);
            }
        }
    }
    None
}

fn fractal_overlap(start: &Fractal, end: &Fractal, strict: bool) -> bool {
    let lo = start.low_price().max(end.low_price());
    let hi = start.high_price().min(end.high_price());
    if strict {
        lo <= hi
    } else {
        lo < hi
    }
}

fn determine_swing(
    start_fractal: Option<&Fractal>,
    end_fractal: &Fractal,
    active_swing: &Swing,
    prev_swing: Option<&Swing>,
) -> bool {
    let Some(start_fractal) = start_fractal else {
        return false;
    };

    let start_ft = start_fractal.fractal_type();
    let end_ft = end_fractal.fractal_type();

    if start_ft == FractalType::None || end_ft == FractalType::None {
        return false;
    }
    if start_ft == end_ft {
        return false;
    }

    if active_swing.direction == Direction::Up {
        if end_fractal.high_price() < start_fractal.low_price() {
            return false;
        }
    } else if end_fractal.low_price() > start_fractal.high_price() {
        return false;
    }

    if !fractal_overlap(start_fractal, end_fractal, true) {
        return true;
    }

    let Some(prev_swing) = prev_swing else {
        return false;
    };

    if fractal_overlap(start_fractal, end_fractal, false) {
        return false;
    }

    let distance = start_fractal
        .high_price()
        .max(end_fractal.high_price())
        - start_fractal.low_price().min(end_fractal.low_price());

    if prev_swing.distance() <= f64::EPSILON {
        return false;
    }
    if distance / prev_swing.distance() < 0.6 {
        return false;
    }

    let start_id = start_fractal.middle.id.unwrap_or_default();
    let end_id = end_fractal.middle.id.unwrap_or_default();
    let count_between = end_id.abs_diff(start_id).saturating_sub(1);
    count_between >= 5
}

fn swing_eq_wo_created_at(a: &Swing, b: &Swing) -> bool {
    a.id == b.id
        && a.direction == b.direction
        && a.cbar_start_id == b.cbar_start_id
        && a.cbar_end_id == b.cbar_end_id
        && a.sbar_start_id == b.sbar_start_id
        && a.sbar_end_id == b.sbar_end_id
        && approx_eq_f64(a.high_price, b.high_price)
        && approx_eq_f64(a.low_price, b.low_price)
        && a.span == b.span
        && approx_eq_f64(a.volume, b.volume)
        && approx_eq_f64(a.start_oi, b.start_oi)
        && approx_eq_f64(a.end_oi, b.end_oi)
        && a.is_completed == b.is_completed
}

