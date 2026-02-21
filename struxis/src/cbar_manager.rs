//! CBar 管理器实现。
//!
//! 负责：
//! - 基于 SBar 增量构建 CBar；
//! - 处理包含关系与回溯链；
//! - 分型检测与 dataframe cache 维护。

use chrono::Utc;
use polars::df;
use polars::prelude::DataFrame;

use crate::constant::{Direction, FractalType, Timeframe};
use crate::bar::{CBar, Fractal, SBar};
use crate::utils::{approx_eq_f64, first_changed_id};

pub(crate) struct CBarManager {
    rows: Vec<CBar>,
    id_cursor: u64,
    df_cache: DataFrame,
    backtrack_id: Option<u64>,
}

impl CBarManager {
    pub(crate) fn new(timeframe: Timeframe) -> Self {
        let _ = timeframe;
        Self {
            rows: Vec::new(),
            id_cursor: 0,
            df_cache: DataFrame::default(),
            backtrack_id: None,
        }
    }

    pub(crate) fn on_sbar(&mut self, sbar: &SBar) -> CBar {
        let previous_rows = self.rows.clone();
        self.backtrack_id = None;

        let curr_id = sbar.id.unwrap_or_default();
        let curr_high = sbar.high_price;
        let curr_low = sbar.low_price;

        if self.rows.is_empty() {
            let cbar = self.append_cbar(curr_id, curr_id, curr_high, curr_low);
            self.detect_last_fractal();
            self.backtrack_id = first_backtrack_cbar_id(&previous_rows, &self.rows);
            self.rebuild_cache();
            return cbar;
        }

        let last = self.rows.last().cloned().expect("cbar last exists");
        let (start_id, merged_high, merged_low) =
            self.merge_with_last_if_needed(&last, curr_id, curr_high, curr_low);

        let cbar = self.append_cbar(start_id, curr_id, merged_high, merged_low);
        self.detect_last_fractal();
        self.backtrack_id = first_backtrack_cbar_id(&previous_rows, &self.rows);
        self.rebuild_cache();
        cbar
    }

    fn merge_with_last_if_needed(
        &mut self,
        last: &CBar,
        curr_id: u64,
        curr_high: f64,
        curr_low: f64,
    ) -> (u64, f64, f64) {
        let mut start_id = curr_id;
        let mut merged_high = curr_high;
        let mut merged_low = curr_low;

        let direction = self.resolve_direction(last.high_price, curr_high);
        if is_inclusive(last.high_price, last.low_price, curr_high, curr_low) {
            start_id = last.sbar_start_id;
            match direction {
                Direction::Up => {
                    merged_high = last.high_price.max(curr_high);
                    merged_low = last.low_price.max(curr_low);
                }
                _ => {
                    merged_high = last.high_price.min(curr_high);
                    merged_low = last.low_price.min(curr_low);
                }
            }

            self.rows.pop();
            self.merge_backward_inclusive(
                direction,
                &mut start_id,
                &mut merged_high,
                &mut merged_low,
            );
        }

        (start_id, merged_high, merged_low)
    }

    fn resolve_direction(&self, last_high: f64, curr_high: f64) -> Direction {
        let hint = self.direction_hint();
        if hint != Direction::None {
            return hint;
        }
        if curr_high >= last_high {
            Direction::Up
        } else {
            Direction::Down
        }
    }

    fn merge_backward_inclusive(
        &mut self,
        direction: Direction,
        start_id: &mut u64,
        merged_high: &mut f64,
        merged_low: &mut f64,
    ) {
        while self.rows.len() >= 2 {
            let new_last = self.rows[self.rows.len() - 1].clone();
            let prev = self.rows[self.rows.len() - 2].clone();

            if direction == Direction::Up && new_last.high_price <= prev.high_price {
                break;
            }
            if direction == Direction::Down && new_last.low_price >= prev.low_price {
                break;
            }

            if !is_inclusive(
                prev.high_price,
                prev.low_price,
                new_last.high_price,
                new_last.low_price,
            ) {
                break;
            }

            *start_id = prev.sbar_start_id;
            if direction == Direction::Up {
                *merged_high = (*merged_high).max(prev.high_price);
                *merged_low = (*merged_low).max(prev.low_price);
            } else {
                *merged_high = (*merged_high).min(prev.high_price);
                *merged_low = (*merged_low).min(prev.low_price);
            }

            self.rows.pop();
        }
    }

    pub(crate) fn backtrack_id(&self) -> Option<u64> {
        self.backtrack_id
    }

    fn append_cbar(&mut self, start_id: u64, end_id: u64, high_price: f64, low_price: f64) -> CBar {
        self.id_cursor += 1;
        let cbar = CBar {
            id: Some(self.id_cursor),
            sbar_start_id: start_id,
            sbar_end_id: end_id,
            high_price,
            low_price,
            fractal_type: FractalType::None,
            created_at: Utc::now(),
        };
        self.rows.push(cbar.clone());
        cbar
    }

    fn direction_hint(&self) -> Direction {
        if self.rows.len() < 2 {
            return Direction::None;
        }
        let prev = &self.rows[self.rows.len() - 2];
        let last = &self.rows[self.rows.len() - 1];
        if last.high_price > prev.high_price {
            Direction::Up
        } else if last.low_price < prev.low_price {
            Direction::Down
        } else {
            Direction::None
        }
    }

    fn detect_last_fractal(&mut self) {
        if self.rows.len() < 3 {
            return;
        }
        let length = self.rows.len();
        let left = self.rows[length - 3].clone();
        let middle = self.rows[length - 2].clone();
        let right = self.rows[length - 1].clone();
        let fractal = Fractal::verify(&left, &middle, &right);
        self.rows[length - 2].fractal_type = fractal;
    }

    pub(crate) fn last_fractal(&self) -> Option<Fractal> {
        if self.rows.len() < 3 {
            return None;
        }
        let length = self.rows.len();
        let left = self.rows[length - 3].clone();
        let middle = self.rows[length - 2].clone();
        let right = self.rows[length - 1].clone();
        if Fractal::verify(&left, &middle, &right) == FractalType::None {
            return None;
        }
        Some(Fractal {
            left,
            middle,
            right,
        })
    }

    pub(crate) fn fractal_at_id(&self, id: u64) -> Option<Fractal> {
        let index = self.get_index(id)?;
        if index == 0 || index + 1 >= self.rows.len() {
            return None;
        }
        let left = self.rows[index - 1].clone();
        let middle = self.rows[index].clone();
        let right = self.rows[index + 1].clone();
        if Fractal::verify(&left, &middle, &right) == FractalType::None {
            return None;
        }
        Some(Fractal { left, middle, right })
    }

    pub(crate) fn prev_fractal(&self, id: u64) -> Option<Fractal> {
        let index = self.get_index(id)?;
        if index < 2 {
            return None;
        }
        for pivot in (1..index).rev() {
            let left = self.rows[pivot - 1].clone();
            let middle = self.rows[pivot].clone();
            let right = self.rows[pivot + 1].clone();
            if Fractal::verify(&left, &middle, &right) != FractalType::None {
                return Some(Fractal { left, middle, right });
            }
        }
        None
    }

    pub(crate) fn next_fractal(&self, id: u64) -> Option<Fractal> {
        let index = self.get_index(id)?;
        if index + 2 >= self.rows.len() {
            return None;
        }
        for pivot in (index + 1)..(self.rows.len() - 1) {
            let left = self.rows[pivot - 1].clone();
            let middle = self.rows[pivot].clone();
            let right = self.rows[pivot + 1].clone();
            if Fractal::verify(&left, &middle, &right) != FractalType::None {
                return Some(Fractal { left, middle, right });
            }
        }
        None
    }

    fn get_index(&self, id: u64) -> Option<usize> {
        self.rows.iter().position(|x| x.id == Some(id))
    }

    pub(crate) fn last_n(&self, n: usize) -> Vec<CBar> {
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

    pub(crate) fn all_rows(&self) -> Vec<CBar> {
        self.rows.clone()
    }

    pub(crate) fn dataframe(&self) -> DataFrame {
        self.df_cache.clone()
    }

    fn rebuild_cache(&mut self) {
        let ids: Vec<u64> = self.rows.iter().map(|x| x.id.unwrap_or_default()).collect();
        let sbar_start_id: Vec<u64> = self.rows.iter().map(|x| x.sbar_start_id).collect();
        let sbar_end_id: Vec<u64> = self.rows.iter().map(|x| x.sbar_end_id).collect();
        let high_price: Vec<f64> = self.rows.iter().map(|x| x.high_price).collect();
        let low_price: Vec<f64> = self.rows.iter().map(|x| x.low_price).collect();
        let fractal_type: Vec<i8> = self
            .rows
            .iter()
            .map(|x| match x.fractal_type {
                FractalType::Top => 1,
                FractalType::Bottom => -1,
                FractalType::None => 0,
            })
            .collect();
        let created_at: Vec<i64> = self
            .rows
            .iter()
            .map(|x| x.created_at.timestamp_millis())
            .collect();

        self.df_cache = df!(
            "id" => ids,
            "sbar_start_id" => sbar_start_id,
            "sbar_end_id" => sbar_end_id,
            "high_price" => high_price,
            "low_price" => low_price,
            "fractal_type" => fractal_type,
            "created_at" => created_at
        )
        .expect("failed to rebuild cbar dataframe cache");
    }
}

fn first_backtrack_cbar_id(previous: &[CBar], current: &[CBar]) -> Option<u64> {
    first_changed_id(previous, current, |x| x.id, cbar_eq_wo_created_at, false)
}

fn cbar_eq_wo_created_at(a: &CBar, b: &CBar) -> bool {
    a.id == b.id
        && a.sbar_start_id == b.sbar_start_id
        && a.sbar_end_id == b.sbar_end_id
        && approx_eq_f64(a.high_price, b.high_price)
        && approx_eq_f64(a.low_price, b.low_price)
        && a.fractal_type == b.fractal_type
}

fn is_inclusive(a_high: f64, a_low: f64, b_high: f64, b_low: f64) -> bool {
    (a_high >= b_high && a_low <= b_low) || (a_high <= b_high && a_low >= b_low)
}
