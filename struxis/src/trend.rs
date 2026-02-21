use chrono::{DateTime, Utc};
use polars::df;
use polars::prelude::DataFrame;

use crate::constant::Direction;
use crate::swing::Swing;
use crate::utils::{approx_eq_f64, first_changed_id};

#[derive(Debug, Clone)]
pub struct Trend {
    pub id: Option<u64>,
    pub direction: Direction,
    pub swing_start_id: u64,
    pub swing_end_id: u64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SfFractalType {
    Top,
    Bottom,
    None,
}

#[derive(Debug, Clone, Default)]
struct TrendSfSeq {
    trend: Option<Trend>,
    sfs: Vec<Swing>,
}

impl TrendSfSeq {
    fn clear(&mut self) {
        self.trend = None;
        self.sfs.clear();
    }

    fn update_trend(&mut self, swing: &Swing) {
        if self.trend.is_none() {
            self.trend = Some(Trend {
                id: None,
                direction: swing.direction,
                swing_start_id: swing.id.unwrap_or_default(),
                swing_end_id: swing.id.unwrap_or_default(),
                sbar_start_id: swing.sbar_start_id,
                sbar_end_id: swing.sbar_end_id,
                high_price: swing.high_price,
                low_price: swing.low_price,
                span: swing.span,
                volume: swing.volume,
                start_oi: swing.start_oi,
                end_oi: swing.end_oi,
                is_completed: false,
                created_at: Utc::now(),
            });
        }

        if let Some(trend) = self.trend.as_mut() {
            trend.swing_end_id = swing.id.unwrap_or(trend.swing_end_id);
            trend.sbar_end_id = swing.sbar_end_id;
            trend.high_price = trend.high_price.max(swing.high_price);
            trend.low_price = trend.low_price.min(swing.low_price);
            trend.span += swing.span;
            trend.volume += swing.volume;
            trend.end_oi = swing.end_oi;
        }
    }

    fn agg_swing(&mut self, swing: &Swing) {
        let Some(trend) = self.trend.as_ref() else {
            return;
        };
        if swing.direction == trend.direction {
            return;
        }

        let mut tmp = swing.clone();
        loop {
            let Some(prev) = self.sfs.last().cloned() else {
                break;
            };
            let inclusive = (prev.high_price >= tmp.high_price && prev.low_price <= tmp.low_price)
                || (prev.high_price <= tmp.high_price && prev.low_price >= tmp.low_price);
            if !inclusive {
                break;
            }
            if trend.direction == Direction::Up {
                tmp.high_price = tmp.high_price.max(prev.high_price);
                tmp.low_price = tmp.low_price.max(prev.low_price);
            } else {
                tmp.high_price = tmp.high_price.min(prev.high_price);
                tmp.low_price = tmp.low_price.min(prev.low_price);
            }
            self.sfs.pop();
        }
        self.sfs.push(tmp);
    }

    fn fractal_type(&self) -> SfFractalType {
        if self.sfs.len() < 3 {
            return SfFractalType::None;
        }
        let right = &self.sfs[self.sfs.len() - 1];
        let mid = &self.sfs[self.sfs.len() - 2];
        let left = &self.sfs[self.sfs.len() - 3];

        let is_top = mid.high_price >= left.high_price
            && mid.high_price >= right.high_price
            && mid.low_price >= left.low_price
            && mid.low_price >= right.low_price;
        if is_top {
            return SfFractalType::Top;
        }

        let is_bottom = mid.high_price <= left.high_price
            && mid.high_price <= right.high_price
            && mid.low_price <= left.low_price
            && mid.low_price <= right.low_price;
        if is_bottom {
            return SfFractalType::Bottom;
        }
        SfFractalType::None
    }

    fn has_gap(&self) -> bool {
        if self.sfs.len() < 3 {
            return false;
        }
        let right = &self.sfs[self.sfs.len() - 1];
        let mid = &self.sfs[self.sfs.len() - 2];
        let left = &self.sfs[self.sfs.len() - 3];
        match self.fractal_type() {
            SfFractalType::Top => left.high_price < mid.low_price && mid.low_price < right.high_price,
            SfFractalType::Bottom => left.low_price > mid.high_price && mid.high_price > right.low_price,
            SfFractalType::None => false,
        }
    }
}

pub struct TrendManager {
    rows: Vec<Trend>,
    id_cursor: u64,
    df_cache: DataFrame,
    backtrack_id: Option<u64>,
    active_sfs: TrendSfSeq,
    pullback_sfs: TrendSfSeq,
}

impl Default for TrendManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TrendManager {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            id_cursor: 0,
            df_cache: DataFrame::default(),
            backtrack_id: None,
            active_sfs: TrendSfSeq::default(),
            pullback_sfs: TrendSfSeq::default(),
        }
    }

    pub fn on_swing_changed(&mut self, swing: &Swing) -> Option<Trend> {
        self.rebuild_from_swings(&[swing.clone()]);
        self.rows.last().cloned()
    }

    pub fn rebuild_from_swings(&mut self, swings: &[Swing]) -> Option<u64> {
        self.rebuild_from_swings_with_backtrack(swings, None)
    }

    pub fn rebuild_from_swings_with_backtrack(
        &mut self,
        swings: &[Swing],
        swing_backtrack_id: Option<u64>,
    ) -> Option<u64> {
        let previous_rows = self.rows.clone();
        self.rows.clear();
        self.id_cursor = 0;
        self.active_sfs.clear();
        self.pullback_sfs.clear();

        let completed: Vec<Swing> = swings.iter().filter(|x| x.is_completed).cloned().collect();
        if completed.len() < 3 {
            self.rebuild_cache();
            self.backtrack_id = if swing_backtrack_id.is_some() {
                previous_rows.first().and_then(|x| x.id)
            } else {
                None
            };
            return self.backtrack_id;
        }

        let mut seed_end_index: Option<usize> = None;
        for i in 2..completed.len() {
            let a = &completed[i - 2];
            let b = &completed[i - 1];
            let c = &completed[i];
            if can_seed_trend(a, b, c) {
                let trend = Trend {
                    id: Some(self.next_id()),
                    direction: a.direction,
                    swing_start_id: a.id.unwrap_or_default(),
                    swing_end_id: c.id.unwrap_or_default(),
                    sbar_start_id: a.sbar_start_id,
                    sbar_end_id: c.sbar_end_id,
                    high_price: a.high_price.max(c.high_price),
                    low_price: a.low_price.min(c.low_price),
                    span: a.span + b.span + c.span,
                    volume: a.volume + b.volume + c.volume,
                    start_oi: a.start_oi,
                    end_oi: c.end_oi,
                    is_completed: false,
                    created_at: Utc::now(),
                };
                self.rows.push(trend.clone());
                self.active_sfs.trend = Some(trend);
                self.active_sfs.agg_swing(b);
                seed_end_index = Some(i);
                break;
            }
        }

        let Some(seed_end_index) = seed_end_index else {
            self.rebuild_cache();
            self.backtrack_id = if swing_backtrack_id.is_some() {
                previous_rows.first().and_then(|x| x.id)
            } else {
                None
            };
            return self.backtrack_id;
        };

        for swing in completed.iter().skip(seed_end_index + 1) {
            self.build_trend_step(swing, &completed);
        }

        self.backtrack_id = if swing_backtrack_id.is_some() {
            first_changed_id(
                &previous_rows,
                &self.rows,
                |x| x.id,
                trend_eq_wo_created_at,
                true,
            )
                .or_else(|| self.rows.first().and_then(|x| x.id))
        } else {
            first_changed_id(
                &previous_rows,
                &self.rows,
                |x| x.id,
                trend_eq_wo_created_at,
                false,
            )
        };
        self.rebuild_cache();
        self.backtrack_id
    }

    fn build_trend_step(&mut self, swing: &Swing, swings: &[Swing]) {
        if self.pullback_sfs.trend.is_some() {
            self.build_pullback_step(swing, swings);
            return;
        }

        self.active_sfs.update_trend(swing);
        self.active_sfs.agg_swing(swing);
        if let Some(active) = self.active_sfs.trend.clone() {
            self.update_active_trend(active);
        }

        let ft = self.active_sfs.fractal_type();
        if ft == SfFractalType::None {
            return;
        }

        if self.active_sfs.has_gap() {
            self.pullback_sfs = self.split_pullback_from_seq(&self.active_sfs, swings);
            return;
        }

        let completed_active = if let Some(active) = self.active_sfs.trend.as_mut() {
            active.is_completed = true;
            Some((active.clone(), active.direction.opposite()))
        } else {
            None
        };

        if let Some((completed_trend, new_direction)) = completed_active {
            let completed_trend = self.confirm_trend(completed_trend, swings);
            self.update_active_trend(completed_trend.clone());

            let Some(new_trend_start_swing) = next_swing_by_id(swings, completed_trend.swing_end_id) else {
                self.pullback_sfs.clear();
                self.active_sfs.clear();
                return;
            };

            let new_trend = Trend {
                id: Some(self.next_id()),
                direction: new_direction,
                swing_start_id: new_trend_start_swing.id.unwrap_or_default(),
                swing_end_id: swing.id.unwrap_or_default(),
                sbar_start_id: new_trend_start_swing.sbar_start_id,
                sbar_end_id: swing.sbar_end_id,
                high_price: new_trend_start_swing.high_price.max(swing.high_price),
                low_price: new_trend_start_swing.low_price.min(swing.low_price),
                span: new_trend_start_swing.span + swing.span,
                volume: new_trend_start_swing.volume + swing.volume,
                start_oi: new_trend_start_swing.start_oi,
                end_oi: swing.end_oi,
                is_completed: false,
                created_at: Utc::now(),
            };
            self.rows.push(new_trend.clone());
            self.active_sfs.clear();
            self.active_sfs.trend = Some(new_trend);
            rebuild_sfs_for_trend(&mut self.active_sfs, swings);
            self.pullback_sfs.clear();
        }
    }

    fn build_pullback_step(&mut self, swing: &Swing, swings: &[Swing]) {
        let Some(active) = self.active_sfs.trend.as_ref() else {
            return;
        };
        let is_new_limit = (active.direction == Direction::Up && swing.high_price > active.high_price)
            || (active.direction == Direction::Down && swing.low_price < active.low_price);

        if is_new_limit {
            self.active_sfs.update_trend(swing);
            self.active_sfs.agg_swing(swing);
            if let Some(active) = self.active_sfs.trend.clone() {
                self.update_active_trend(active);
            }
            self.pullback_sfs.clear();
            return;
        }

        self.pullback_sfs.update_trend(swing);
        self.pullback_sfs.agg_swing(swing);

        let f = self.pullback_sfs.fractal_type();
        if f == SfFractalType::None {
            self.active_sfs.update_trend(swing);
            self.active_sfs.agg_swing(swing);
            if let Some(active) = self.active_sfs.trend.clone() {
                self.update_active_trend(active);
            }
            return;
        }

        let pullback_dir = self
            .pullback_sfs
            .trend
            .as_ref()
            .map(|x| x.direction)
            .unwrap_or(Direction::None);
        let qualifies = (pullback_dir == Direction::Up && f == SfFractalType::Top)
            || (pullback_dir == Direction::Down && f == SfFractalType::Bottom);

        if !qualifies {
            self.active_sfs.agg_swing(swing);
            self.active_sfs.update_trend(swing);
            if let Some(active) = self.active_sfs.trend.clone() {
                self.update_active_trend(active);
            }

            self.pullback_sfs.agg_swing(swing);
            self.pullback_sfs.update_trend(swing);
            return;
        }

        let completed_active = if let Some(active) = self.active_sfs.trend.as_mut() {
            active.is_completed = true;
            Some((active.clone(), active.direction.opposite()))
        } else {
            None
        };

        if let Some((completed_trend, new_direction)) = completed_active {
            let completed_trend = self.confirm_trend(completed_trend, swings);
            self.update_active_trend(completed_trend.clone());

            if self.pullback_sfs.has_gap() {
                self.active_sfs = self.pullback_sfs.clone();
                if let Some(active_trend) = self.active_sfs.trend.clone() {
                    self.update_active_trend(active_trend);
                }
                self.pullback_sfs = self.split_pullback_from_seq(&self.active_sfs, swings);
                return;
            }

            let completed_pullback = self.pullback_sfs.trend.clone().map(|mut t| {
                t.is_completed = true;
                t
            });
            if let Some(pullback_trend) = completed_pullback {
                let pullback_trend = self.confirm_trend(pullback_trend, swings);
                self.update_active_trend(pullback_trend.clone());

                let Some(new_trend_start_swing) = next_swing_by_id(
                    swings,
                    pullback_trend.swing_end_id,
                ) else {
                    self.pullback_sfs.clear();
                    self.active_sfs.clear();
                    return;
                };

                let new_trend = Trend {
                    id: Some(self.next_id()),
                    direction: new_direction,
                    swing_start_id: new_trend_start_swing.id.unwrap_or_default(),
                    swing_end_id: swing.id.unwrap_or_default(),
                    sbar_start_id: new_trend_start_swing.sbar_start_id,
                    sbar_end_id: swing.sbar_end_id,
                    high_price: new_trend_start_swing.high_price.max(swing.high_price),
                    low_price: new_trend_start_swing.low_price.min(swing.low_price),
                    span: new_trend_start_swing.span + swing.span,
                    volume: new_trend_start_swing.volume + swing.volume,
                    start_oi: new_trend_start_swing.start_oi,
                    end_oi: swing.end_oi,
                    is_completed: false,
                    created_at: Utc::now(),
                };
                self.rows.push(new_trend.clone());
                self.pullback_sfs.clear();
                self.active_sfs.clear();
                self.active_sfs.trend = Some(new_trend);
                rebuild_sfs_for_trend(&mut self.active_sfs, swings);
                return;
            }

            let new_trend = Trend {
                id: Some(self.next_id()),
                direction: new_direction,
                swing_start_id: swing.id.unwrap_or_default(),
                swing_end_id: swing.id.unwrap_or_default(),
                sbar_start_id: swing.sbar_start_id,
                sbar_end_id: swing.sbar_end_id,
                high_price: swing.high_price,
                low_price: swing.low_price,
                span: swing.span,
                volume: swing.volume,
                start_oi: swing.start_oi,
                end_oi: swing.end_oi,
                is_completed: false,
                created_at: Utc::now(),
            };
            self.rows.push(new_trend.clone());
            self.active_sfs.clear();
            self.active_sfs.trend = Some(new_trend);
            self.pullback_sfs.clear();
        }
    }

    fn split_pullback_from_seq(&self, base_seq: &TrendSfSeq, swings: &[Swing]) -> TrendSfSeq {
        let mut pullback = TrendSfSeq::default();
        let Some(active_trend) = base_seq.trend.as_ref() else {
            return pullback;
        };
        if !base_seq.has_gap() {
            return pullback;
        }

        let opposite_direction = active_trend.direction.opposite();
        let limit_swing = find_limit_swing(
            swings,
            active_trend.swing_start_id,
            active_trend.swing_end_id,
            opposite_direction,
            if active_trend.direction == Direction::Up {
                LimitKind::Max
            } else {
                LimitKind::Min
            },
        );
        let Some(limit_swing) = limit_swing else {
            return pullback;
        };

        let swing_list = swings_in_range(swings, limit_swing.id.unwrap_or_default(), active_trend.swing_end_id);
        if swing_list.is_empty() {
            return pullback;
        }

        pullback.trend = Some(Trend {
            id: None,
            direction: opposite_direction,
            swing_start_id: limit_swing.id.unwrap_or_default(),
            swing_end_id: active_trend.swing_end_id,
            sbar_start_id: limit_swing.sbar_start_id,
            sbar_end_id: active_trend.sbar_end_id,
            high_price: limit_swing.high_price,
            low_price: limit_swing.low_price,
            span: 0,
            volume: 0.0,
            start_oi: limit_swing.start_oi,
            end_oi: active_trend.end_oi,
            is_completed: false,
            created_at: Utc::now(),
        });

        for item in swing_list {
            pullback.agg_swing(item);
            if let Some(trend) = pullback.trend.as_mut() {
                trend.swing_end_id = item.id.unwrap_or(trend.swing_end_id);
                trend.sbar_end_id = item.sbar_end_id;
                trend.high_price = trend.high_price.max(item.high_price);
                trend.low_price = trend.low_price.min(item.low_price);
                trend.span += item.span;
                trend.volume += item.volume;
                trend.end_oi = item.end_oi;
            }
        }

        pullback
    }

    fn update_active_trend(&mut self, trend: Trend) {
        if self.rows.is_empty() {
            self.rows.push(trend);
            return;
        }
        let idx = self.rows.len() - 1;
        self.rows[idx] = trend;
    }

    fn confirm_trend(&mut self, mut trend: Trend, swings: &[Swing]) -> Trend {
        trend.is_completed = true;

        let start_limit_kind = if trend.direction == Direction::Down {
            LimitKind::Max
        } else {
            LimitKind::Min
        };
        if let Some(start_swing) = find_limit_swing(
            swings,
            trend.swing_start_id,
            trend.swing_end_id,
            trend.direction,
            start_limit_kind,
        ) {
            if start_swing.id.unwrap_or_default() != trend.swing_start_id {
                trend.swing_start_id = start_swing.id.unwrap_or_default();
                trend.sbar_start_id = start_swing.sbar_start_id;
                trend.high_price = trend.high_price.max(start_swing.high_price);
                trend.low_price = trend.low_price.min(start_swing.low_price);

                if let Some(prev_trend_index) = self.rows.len().checked_sub(2) {
                    if self.rows[prev_trend_index].is_completed {
                        if let Some(prev_end_swing) = prev_swing_by_id(swings, trend.swing_start_id)
                        {
                            if self.rows[prev_trend_index].swing_end_id
                                != prev_end_swing.id.unwrap_or_default()
                            {
                                self.rows[prev_trend_index].swing_end_id =
                                    prev_end_swing.id.unwrap_or_default();
                                self.rows[prev_trend_index].sbar_end_id = prev_end_swing.sbar_end_id;
                                self.rows[prev_trend_index].high_price = self.rows[prev_trend_index]
                                    .high_price
                                    .max(prev_end_swing.high_price);
                                self.rows[prev_trend_index].low_price = self.rows[prev_trend_index]
                                    .low_price
                                    .min(prev_end_swing.low_price);
                            }
                        }
                    }
                }
            }
        }

        let end_limit_kind = if trend.direction == Direction::Down {
            LimitKind::Min
        } else {
            LimitKind::Max
        };
        if let Some(end_swing) = find_limit_swing(
            swings,
            trend.swing_start_id,
            trend.swing_end_id,
            trend.direction,
            end_limit_kind,
        ) {
            trend.swing_end_id = end_swing.id.unwrap_or(trend.swing_end_id);
            trend.sbar_end_id = end_swing.sbar_end_id;
            trend.high_price = trend.high_price.max(end_swing.high_price);
            trend.low_price = trend.low_price.min(end_swing.low_price);
            trend.end_oi = end_swing.end_oi;
        }

        if let Some((span, volume, start_oi, end_oi)) = stats_from_swings(
            swings,
            trend.swing_start_id,
            trend.swing_end_id,
        ) {
            trend.span = span;
            trend.volume = volume;
            trend.start_oi = start_oi;
            trend.end_oi = end_oi;
        }

        trend
    }

    fn next_id(&mut self) -> u64 {
        self.id_cursor += 1;
        self.id_cursor
    }

    pub fn last_n(&self, n: usize) -> Vec<Trend> {
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

    pub fn all_rows(&self) -> Vec<Trend> {
        self.rows.clone()
    }

    pub fn backtrack_id(&self) -> Option<u64> {
        self.backtrack_id
    }

    fn rebuild_cache(&mut self) {
        let ids: Vec<u64> = self.rows.iter().map(|x| x.id.unwrap_or_default()).collect();
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
        let swing_start_id: Vec<u64> = self.rows.iter().map(|x| x.swing_start_id).collect();
        let swing_end_id: Vec<u64> = self.rows.iter().map(|x| x.swing_end_id).collect();
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
            "swing_start_id" => swing_start_id,
            "swing_end_id" => swing_end_id,
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
        .expect("failed to rebuild trend dataframe cache");
    }
}

fn can_seed_trend(a: &Swing, b: &Swing, c: &Swing) -> bool {
    let overlap = a.low_price.max(b.low_price).max(c.low_price)
        <= a.high_price.min(b.high_price).min(c.high_price);
    if !overlap {
        return false;
    }

    if !(a.direction == b.direction.opposite() && b.direction == c.direction.opposite()) {
        return false;
    }

    match a.direction {
        Direction::Down => c.high_price < a.high_price && c.low_price < a.low_price,
        Direction::Up => c.high_price > a.high_price && c.low_price > a.low_price,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy)]
enum LimitKind {
    Max,
    Min,
}

fn next_swing_by_id(swings: &[Swing], curr_id: u64) -> Option<&Swing> {
    for (idx, swing) in swings.iter().enumerate() {
        if swing.id.unwrap_or_default() == curr_id {
            return swings.get(idx + 1);
        }
    }
    None
}

fn prev_swing_by_id(swings: &[Swing], curr_id: u64) -> Option<&Swing> {
    for (idx, swing) in swings.iter().enumerate() {
        if swing.id.unwrap_or_default() == curr_id {
            if idx == 0 {
                return None;
            }
            return swings.get(idx - 1);
        }
    }
    None
}

fn swings_in_range(swings: &[Swing], start_id: u64, end_id: u64) -> Vec<&Swing> {
    swings
        .iter()
        .filter(|x| {
            let id = x.id.unwrap_or_default();
            start_id <= id && id <= end_id
        })
        .collect()
}

fn find_limit_swing(
    swings: &[Swing],
    start_id: u64,
    end_id: u64,
    direction: Direction,
    limit_kind: LimitKind,
) -> Option<&Swing> {
    let mut candidates = swings_in_range(swings, start_id, end_id)
        .into_iter()
        .filter(|x| x.direction == direction)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| {
        let va = match limit_kind {
            LimitKind::Max => a.high_price,
            LimitKind::Min => a.low_price,
        };
        let vb = match limit_kind {
            LimitKind::Max => b.high_price,
            LimitKind::Min => b.low_price,
        };
        va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
    });

    match limit_kind {
        LimitKind::Max => candidates.last().copied(),
        LimitKind::Min => candidates.first().copied(),
    }
}

fn rebuild_sfs_for_trend(seq: &mut TrendSfSeq, swings: &[Swing]) {
    let Some(trend) = seq.trend.clone() else {
        return;
    };
    seq.sfs.clear();
    for swing in swings_in_range(swings, trend.swing_start_id, trend.swing_end_id) {
        seq.agg_swing(swing);
    }
}

fn stats_from_swings(
    swings: &[Swing],
    start_id: u64,
    end_id: u64,
) -> Option<(usize, f64, f64, f64)> {
    let in_range = swings_in_range(swings, start_id, end_id);
    let first = in_range.first()?;
    let last = in_range.last()?;
    let span = in_range.iter().map(|x| x.span).sum::<usize>();
    let volume = in_range.iter().map(|x| x.volume).sum::<f64>();
    Some((span, volume, first.start_oi, last.end_oi))
}

fn trend_eq_wo_created_at(a: &Trend, b: &Trend) -> bool {
    a.id == b.id
        && a.direction == b.direction
        && a.swing_start_id == b.swing_start_id
        && a.swing_end_id == b.swing_end_id
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

