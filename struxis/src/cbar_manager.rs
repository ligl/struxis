//! CBar 管理器实现。
//!
//! 负责：
//! - 基于 SBar 增量构建 CBar；
//! - 处理包含关系与回溯链；
//! - 分型检测与 dataframe cache 维护。

use chrono::Utc;
use polars::df;
use polars::prelude::DataFrame;

use crate::IdGenerator;
use crate::bar::{CBar, Fractal, SBar};
use crate::constant::{Direction, FractalType, Timeframe};
use crate::utils::{approx_eq_f64, first_changed_id};

pub(crate) struct CBarManager {
    rows: Vec<CBar>,
    id_generator: &'static IdGenerator,
    df_cache: DataFrame,
    backtrack_id: Option<u64>,
}

 
impl CBarManager {
    pub(crate) fn new(timeframe: Timeframe) -> Self {
        let _ = timeframe;
        Self {
            rows: Vec::new(),
            id_generator: crate::id_generator::cbar_id_generator(),
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

        // 1. 先插入新cbar
        let new_cbar = CBar {
            id: None, // id稍后分配
            sbar_start_id: curr_id,
            sbar_end_id: curr_id,
            high_price: curr_high,
            low_price: curr_low,
            fractal_type: crate::constant::FractalType::None,
            created_at: chrono::Utc::now(),
        };
        self.rows.push(new_cbar.clone());

        // 2. 使用 helper 进行持续向前合并，直到无包含关系
        // merge_with_last_if_needed 会弹出被包含的项并返回合并后的 start_id/high/low/created_at
        let last = self.rows.last().cloned().expect("just pushed");
        let (start_id, merged_high, merged_low, created_at) =
            self.merge_with_last_if_needed(&last, curr_id, curr_high, curr_low);

        // push 合并结果（无论是否真的合并，helper 会弹出原来的条目并返回合并范围）
        let merged = CBar {
            id: None,
            sbar_start_id: start_id,
            sbar_end_id: curr_id,
            high_price: merged_high,
            low_price: merged_low,
            fractal_type: crate::constant::FractalType::None,
            created_at,
        };
        self.rows.push(merged);

        // 3. 分配id，保证唯一递增
        for cbar in &mut self.rows {
            if cbar.id.is_none() {
                cbar.id = Some(self.id_generator.get_id());
            }
        }

        self.recompute_fractals();
        self.backtrack_id = first_backtrack_cbar_id(&previous_rows, &self.rows);
        self.rebuild_cache();
        self.rows.last().cloned().expect("cbar must exist")
    }

    fn merge_with_last_if_needed(
        &mut self,
        _last: &CBar,
        curr_id: u64,
        curr_high: f64,
        curr_low: f64,
    ) -> (u64, f64, f64, chrono::DateTime<Utc>) {
        let mut start_id = curr_id;
        let mut merged_high = curr_high;
        let mut merged_low = curr_low;
        let mut created_at = chrono::Utc::now();

        // 持续向前合并，直到与前一CBar无包含关系为止
        loop {
            if self.rows.is_empty() {
                break;
            }
            let last = self.rows.last().unwrap().clone();
            if !is_inclusive(last.high_price, last.low_price, merged_high, merged_low) {
                break;
            }
            start_id = last.sbar_start_id;
            // 使用被包含项的 created_at（逐步向前覆盖，最终为最早被合并项的 created_at）
            created_at = last.created_at;
            // 合并高低点
            merged_high = merged_high.max(last.high_price);
            merged_low = merged_low.min(last.low_price);
            self.rows.pop();
        }
        (start_id, merged_high, merged_low, created_at)
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
            // 统一使用 high = max(...), low = min(...) 保持合并语义一致
            *merged_high = (*merged_high).max(prev.high_price);
            *merged_low = (*merged_low).min(prev.low_price);
            self.rows.pop();
        }
    }

    pub(crate) fn backtrack_id(&self) -> Option<u64> {
        self.backtrack_id
    }

    #[allow(dead_code)]
    fn append_cbar(&mut self, start_id: u64, end_id: u64, high_price: f64, low_price: f64) -> CBar {
        let cbar = CBar {
            id: Some(self.id_generator.get_id()),
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

    #[allow(dead_code)]
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

    fn recompute_fractals(&mut self) {
        for row in &mut self.rows {
            row.fractal_type = FractalType::None;
        }

        if self.rows.len() < 3 {
            return;
        }

        for pivot in 1..(self.rows.len() - 1) {
            let left = self.rows[pivot - 1].clone();
            let middle = self.rows[pivot].clone();
            let right = self.rows[pivot + 1].clone();
            let fractal = Fractal::verify(&left, &middle, &right);
            self.rows[pivot].fractal_type = fractal;
        }
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
        Some(Fractal {
            left,
            middle,
            right,
        })
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
                return Some(Fractal {
                    left,
                    middle,
                    right,
                });
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
                return Some(Fractal {
                    left,
                    middle,
                    right,
                });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constant::Timeframe;
    use chrono::Utc;

    fn mk_sbar(id: u64, high: f64, low: f64) -> SBar {
        SBar {
            id: Some(id),
            symbol: "T".to_string(),
            exchange: "X".to_string(),
            timeframe: Timeframe::M15,
            datetime: Utc::now(),
            open_price: (high + low) / 2.0,
            high_price: high,
            low_price: low,
            close_price: (high + low) / 2.0,
            volume: 0.0,
            open_interest: 0.0,
            turnover: 0.0,
        }
    }

    #[test]
    fn test_multiple_inclusive_merge() {
        let mut mgr = CBarManager::new(Timeframe::M15);

        // s1 包含 s2，随后 s3 包含合并后的 cbar -> 最终只有一条 cbar
        let s1 = mk_sbar(1, 10.0, 1.0);
        let s2 = mk_sbar(2, 9.0, 2.0);
        let s3 = mk_sbar(3, 12.0, 0.0);

        mgr.on_sbar(&s1);
        // after first, one cbar exists
        assert_eq!(mgr.all_rows().len(), 1);

        mgr.on_sbar(&s2);
        // s1 contains s2 -> merged into one cbar
        assert_eq!(mgr.all_rows().len(), 1);
        let c = mgr.all_rows()[0].clone();
        assert_eq!(c.sbar_start_id, 1);
        assert_eq!(c.sbar_end_id, 2);
        assert_eq!(c.high_price, 10.0);
        assert_eq!(c.low_price, 1.0);

        mgr.on_sbar(&s3);
        // s3 contains previous merged -> result should be single cbar covering 1..3
        assert_eq!(mgr.all_rows().len(), 1);
        let c2 = mgr.all_rows()[0].clone();
        assert_eq!(c2.sbar_start_id, 1);
        assert_eq!(c2.sbar_end_id, 3);
        assert_eq!(c2.high_price, 12.0);
        assert_eq!(c2.low_price, 0.0);
    }
}
