use chrono::{DateTime, Utc};
use polars::df;
use polars::prelude::DataFrame;

use crate::bar::{CBar, Fractal};
use crate::constant::{Direction, FractalType};
use crate::utils::{approx_eq_f64, first_changed_id};
use crate::IdGenerator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwingState {
    Forming,
    PendingReverse,
    Confirmed,
}

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
    pub state: SwingState,
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
    id_generator: &'static IdGenerator,
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
            id_generator: crate::id_generator::swing_id_generator(),
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
            active.state = SwingState::Confirmed;
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

        // 新增：候选终点与后验延伸机制
        let mut pending_candidate: Option<(usize, Fractal)> = None;
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

            // --- 修正版：仅重叠时禁止分段，否则允许分段并完成前一 swing ---
            let start_fractal = find_fractal_by_middle_id(cbars, active.cbar_start_id);
            let end_fractal = find_fractal_by_middle_id(cbars, active.cbar_end_id);
            let overlap_with_start = start_fractal.as_ref().map_or(false, |sf| fractal_overlap(sf, &fractal, true));
            let overlap_with_end = end_fractal.as_ref().map_or(false, |ef| fractal_overlap(ef, &fractal, true));
            let fractal_type = Fractal::verify(&fractal.left, &fractal.middle, &fractal.right);
            let direction_match = match active.direction {
                Direction::Up => fractal_type == FractalType::Top,
                Direction::Down => fractal_type == FractalType::Bottom,
                _ => false,
            };
            if overlap_with_start || overlap_with_end || !direction_match {
                // 只延长 active swing 区间，不生成新 swing
                // 保证终点 fractal id 指向当前 fractal 的 middle（分型点）
                active.cbar_end_id = fractal.middle.id.unwrap_or(active.cbar_end_id);
                active.sbar_end_id = fractal.middle.sbar_end_id;
                active.high_price = active.high_price.max(fractal.high_price());
                active.low_price = active.low_price.min(fractal.low_price());
                active.state = SwingState::Forming;
                self.rows[active_index] = active;
                continue;
            } else {
                // 非重叠且 fractal 类型与方向配对，允许分段，前一 swing 标记为 completed/confirmed
                let mut completed = active.clone();
                completed.cbar_end_id = fractal.middle.id.unwrap_or(completed.cbar_end_id);
                completed.sbar_end_id = fractal.middle.sbar_end_id;
                completed.state = SwingState::Confirmed;
                self.rows[active_index] = completed;
            }

            let prev_reference_swing = self
                .rows
                .iter()
                .rev()
                .find(|x| x.state != SwingState::Forming)
                .cloned();
            let in_bootstrap_phase = self.rows.len() == 1;
            let bootstrap_reference_break_ok = if in_bootstrap_phase {
                start_fractal
                    .as_ref()
                    .map(|start| {
                        end_breaks_start_reference(cbars, start, &fractal, active.direction)
                    })
                    .unwrap_or(false)
            } else {
                true
            };

            let pending_prev_index = if self.rows.len() >= 2 {
                let idx = self.rows.len() - 2;
                (self.rows[idx].state == SwingState::PendingReverse).then_some(idx)
            } else {
                None
            };

            // 后验延伸：如果有候选终点，且当前 fractal 突破了候选终点，则延长 swing
            if let Some((pending_idx, candidate_fractal)) = &pending_candidate {
                let candidate_ft = candidate_fractal.fractal_type();
                let candidate_price = match candidate_ft {
                    FractalType::Top => candidate_fractal.high_price(),
                    FractalType::Bottom => candidate_fractal.low_price(),
                    _ => 0.0,
                };
                let extend = match candidate_ft {
                    FractalType::Top => fractal.high_price() > candidate_price,
                    FractalType::Bottom => fractal.low_price() < candidate_price,
                    _ => false,
                };
                if extend {
                    // 恢复 swing 为 forming，延长终点
                    let mut resumed = self.rows[*pending_idx].clone();
                    resumed.state = SwingState::Forming;
                    resumed.cbar_end_id = fractal.right.id.unwrap_or(resumed.cbar_end_id);
                    resumed.sbar_end_id = fractal.right.sbar_end_id;
                    resumed.high_price = resumed.high_price.max(fractal.high_price());
                    resumed.low_price = resumed.low_price.min(fractal.low_price());
                    self.rows[*pending_idx] = resumed;
                    // 移除候选
                    self.rows.truncate(*pending_idx + 1);
                    pending_candidate = None;
                    continue;
                }
            }

            if let Some(prev_index) = pending_prev_index {
                let prev_pending = self.rows[prev_index].clone();
                if should_resume_previous_swing(&prev_pending, &active, &fractal) {
                    self.rows.pop();
                    let mut resumed = self.rows[prev_index].clone();
                    resumed.state = SwingState::Forming;
                    resumed.cbar_end_id = fractal.right.id.unwrap_or(resumed.cbar_end_id);
                    resumed.sbar_end_id = fractal.right.sbar_end_id;
                    resumed.high_price = resumed.high_price.max(fractal.high_price());
                    resumed.low_price = resumed.low_price.min(fractal.low_price());
                    self.rows[prev_index] = resumed;
                    pending_candidate = None;
                    continue;
                }
            }

            if determine_swing(
                start_fractal.as_ref(),
                &fractal,
                &active,
                prev_reference_swing.as_ref(),
                bootstrap_reference_break_ok,
            ) {
                let provisional_end_id = fractal.middle.id.unwrap_or(active.cbar_end_id);

                if let Some(end_id) = find_swing_extreme_cbar_id(
                    cbars,
                    active.cbar_start_id,
                    provisional_end_id,
                    active.direction,
                ) {
                    active.cbar_end_id = end_id;
                } else {
                    active.cbar_end_id = provisional_end_id;
                }

                apply_cbar_range_stats(&mut active, cbars);
                active.state = SwingState::PendingReverse;
                self.rows[active_index] = active.clone();

                // 记录候选终点
                pending_candidate = Some((active_index, fractal.clone()));

                let new_active = Swing {
                    id: Some(self.id_generator.get_id()),
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
                    state: SwingState::Forming,
                    created_at: Utc::now(),
                };
                self.rows.push(new_active);
            } else {
                if in_bootstrap_phase
                    && should_reanchor_start(start_fractal.as_ref(), &fractal, active.direction)
                {
                    active.cbar_start_id = fractal.middle.id.unwrap_or(active.cbar_start_id);
                    active.sbar_start_id = fractal.middle.sbar_start_id;
                }
                active.cbar_end_id = fractal.right.id.unwrap_or(active.cbar_end_id);
                apply_cbar_range_stats(&mut active, cbars);
                active.state = SwingState::Forming;
                self.rows[active_index] = active;
            }
        }

        // --- 新增：循环结束后将最后一段 forming swing 标记为 confirmed ---
        if let Some(last) = self.rows.last_mut() {
            if last.state == SwingState::Forming {
                last.state = SwingState::Confirmed;
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
        let new_id = self.id_generator.get_id();
        let direction = match Fractal::verify(&fractal.left, &fractal.middle, &fractal.right) {
            FractalType::Top => Direction::Down,
            FractalType::Bottom => Direction::Up,
            FractalType::None => Direction::None,
        };

        let swing = Swing {
            id: Some(new_id),
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
            state: if completed {
                SwingState::Confirmed
            } else {
                SwingState::Forming
            },
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
        let is_completed: Vec<bool> = self.rows.iter().map(|x| x.state == SwingState::Confirmed).collect();
        let state: Vec<i8> = self
            .rows
            .iter()
            .map(|x| match x.state {
                SwingState::Forming => 0,
                SwingState::PendingReverse => 1,
                SwingState::Confirmed => 2,
            })
            .collect();
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
            "state" => state,
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
        self.left
            .high_price
            .max(self.middle.high_price)
            .max(self.right.high_price)
    }

    fn low_price(&self) -> f64 {
        self.left
            .low_price
            .min(self.middle.low_price)
            .min(self.right.low_price)
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
    bootstrap_reference_break_ok: bool,
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

    if !bootstrap_reference_break_ok {
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

    if !fractal_overlap(start_fractal, end_fractal, false) {
        return true;
    }

    let Some(prev_swing) = prev_swing else {
        return true;
    };

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

fn should_reanchor_start(
    start_fractal: Option<&Fractal>,
    current_fractal: &Fractal,
    direction: Direction,
) -> bool {
    let Some(start) = start_fractal else {
        return false;
    };

    let start_ft = start.fractal_type();
    let current_ft = current_fractal.fractal_type();
    if start_ft == FractalType::None || current_ft == FractalType::None || start_ft != current_ft {
        return false;
    }

    match direction {
        Direction::Down => {
            current_ft == FractalType::Top && current_fractal.high_price() >= start.high_price()
        }
        Direction::Up => {
            current_ft == FractalType::Bottom && current_fractal.low_price() <= start.low_price()
        }
        _ => false,
    }
}

fn latest_fractal_before(cbars: &[CBar], before_middle_id: u64, kind: FractalType) -> Option<Fractal> {
    if cbars.len() < 3 {
        return None;
    }

    let mut prev: Option<Fractal> = None;
    for pivot in 1..cbars.len().saturating_sub(1) {
        let mid = &cbars[pivot];
        let Some(mid_id) = mid.id else {
            continue;
        };
        if mid_id >= before_middle_id {
            break;
        }

        let fractal = Fractal {
            left: cbars[pivot - 1].clone(),
            middle: mid.clone(),
            right: cbars[pivot + 1].clone(),
        };
        if fractal.fractal_type() == kind {
            prev = Some(fractal);
        }
    }
    prev
}

fn end_breaks_start_reference(
    cbars: &[CBar],
    start_fractal: &Fractal,
    end_fractal: &Fractal,
    direction: Direction,
) -> bool {
    let end_kind = end_fractal.fractal_type();
    if end_kind == FractalType::None {
        return false;
    }

    let reference_kind = match direction {
        Direction::Down => FractalType::Bottom,
        Direction::Up => FractalType::Top,
        _ => return false,
    };

    if end_kind != reference_kind {
        return false;
    }

    let start_id = start_fractal.middle.id.unwrap_or_default();
    let Some(reference) = latest_fractal_before(cbars, start_id, reference_kind) else {
        return false;
    };

    match direction {
        Direction::Down => end_fractal.low_price() < reference.low_price(),
        Direction::Up => end_fractal.high_price() > reference.high_price(),
        _ => false,
    }
}

fn should_resume_previous_swing(prev_pending: &Swing, active: &Swing, fractal: &Fractal) -> bool {
    if prev_pending.state != SwingState::PendingReverse {
        return false;
    }
    if prev_pending.direction == active.direction {
        return false;
    }

    let fractal_type = fractal.fractal_type();
    match prev_pending.direction {
        Direction::Down => {
            fractal_type == FractalType::Bottom
                && fractal.low_price() < active.low_price
        }
        Direction::Up => {
            fractal_type == FractalType::Top
                && fractal.high_price() > active.high_price
        }
        _ => false,
    }
}

fn cbar_by_id(cbars: &[CBar], id: u64) -> Option<&CBar> {
    cbars.iter().find(|x| x.id == Some(id))
}

fn cbar_ids_in_range(cbars: &[CBar], start_id: u64, end_id: u64) -> Vec<u64> {
    let lo = start_id.min(end_id);
    let hi = start_id.max(end_id);
    cbars
        .iter()
        .filter_map(|x| {
            let id = x.id?;
            (lo <= id && id <= hi).then_some(id)
        })
        .collect::<Vec<_>>()
}

fn find_swing_extreme_cbar_id(
    cbars: &[CBar],
    start_id: u64,
    end_id: u64,
    direction: Direction,
) -> Option<u64> {
    let ids = cbar_ids_in_range(cbars, start_id, end_id);
    if ids.is_empty() {
        return None;
    }

    let mut best: Option<(&CBar, u64)> = None;
    for id in ids {
        let cbar = cbar_by_id(cbars, id)?;
        best = match best {
            None => Some((cbar, id)),
            Some((prev, prev_id)) => {
                let better = match direction {
                    Direction::Up => {
                        cbar.high_price > prev.high_price
                            || (approx_eq_f64(cbar.high_price, prev.high_price) && id > prev_id)
                    }
                    Direction::Down => {
                        cbar.low_price < prev.low_price
                            || (approx_eq_f64(cbar.low_price, prev.low_price) && id > prev_id)
                    }
                    _ => false,
                };
                if better {
                    Some((cbar, id))
                } else {
                    Some((prev, prev_id))
                }
            }
        };
    }
    best.map(|(_, id)| id)
}

fn apply_cbar_range_stats(swing: &mut Swing, cbars: &[CBar]) {
    let ids = cbar_ids_in_range(cbars, swing.cbar_start_id, swing.cbar_end_id);
    if ids.is_empty() {
        return;
    }

    let mut high = f64::MIN;
    let mut low = f64::MAX;
    let mut sbar_start = u64::MAX;
    let mut sbar_end = 0_u64;
    for id in ids {
        if let Some(cbar) = cbar_by_id(cbars, id) {
            high = high.max(cbar.high_price);
            low = low.min(cbar.low_price);
            sbar_start = sbar_start.min(cbar.sbar_start_id);
            sbar_end = sbar_end.max(cbar.sbar_end_id);
        }
    }
    if sbar_start != u64::MAX {
        swing.sbar_start_id = sbar_start;
    }
    if sbar_end != 0 {
        swing.sbar_end_id = sbar_end;
    }
    if high != f64::MIN {
        swing.high_price = high;
    }
    if low != f64::MAX {
        swing.low_price = low;
    }
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
        && a.state == b.state
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{fractal_overlap, CBar, Fractal};
    use crate::constant::FractalType;

    fn cbar(id: u64, high: f64, low: f64) -> CBar {
        CBar {
            id: Some(id),
            sbar_start_id: id,
            sbar_end_id: id,
            high_price: high,
            low_price: low,
            fractal_type: FractalType::None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn fractal_overlap_uses_full_three_cbar_envelope() {
        let start = Fractal {
            left: cbar(1, 120.0, 110.0),
            middle: cbar(2, 100.0, 90.0),
            right: cbar(3, 101.0, 91.0),
        };
        let end = Fractal {
            left: cbar(4, 95.0, 85.0),
            middle: cbar(5, 84.0, 80.0),
            right: cbar(6, 83.0, 79.0),
        };

        assert!(
            fractal_overlap(&start, &end, true),
            "fractal overlap should use full 3-cbar envelope, not middle bar only"
        );
        assert!(
            fractal_overlap(&start, &end, false),
            "envelope intersection has positive width in this fixture"
        );
    }

    #[test]
    fn fractal_overlap_distinguishes_touching_from_intersection() {
        let start = Fractal {
            left: cbar(1, 120.0, 110.0),
            middle: cbar(2, 100.0, 90.0),
            right: cbar(3, 102.0, 92.0),
        };
        let end = Fractal {
            left: cbar(4, 90.0, 80.0),
            middle: cbar(5, 89.0, 79.0),
            right: cbar(6, 88.0, 78.0),
        };

        assert!(
            fractal_overlap(&start, &end, true),
            "strict mode should treat boundary touch as overlap"
        );
        assert!(
            !fractal_overlap(&start, &end, false),
            "non-strict mode should require positive overlap width"
        );
    }
}

