pub mod builder;
pub mod factory;

pub use builder::{
    ChannelKeyZoneBuilder, KeyZoneBuilder, SwingKeyZoneBuilder, TrendKeyZoneBuilder,
};
pub use factory::KeyZoneFactory;

use crate::constant::{Direction, KeyZoneOrientation, KeyZoneOrigin, Timeframe};
use crate::bar::SBar;
use crate::swing::Swing;
use crate::trend::Trend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyZoneState {
    Approach,
    Touch,
    Accept,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyZoneBehavior {
    StrongAccept,
    WeakAccept,
    StrongReject,
    WeakReject,
    SecondPush,
    BreakoutFailure,
}

#[derive(Debug, Clone)]
pub struct KeyZoneSignal {
    pub zone_id: Option<u64>,
    pub behavior: KeyZoneBehavior,
    pub direction: Direction,
    pub strength: f64,
    pub sbar_id: u64,
}

impl KeyZoneSignal {
    pub fn signed_strength(&self) -> f64 {
        let dir_sign = match self.direction {
            Direction::Up => 1.0,
            Direction::Down => -1.0,
            _ => 0.0,
        };
        let behavior_sign = match self.behavior {
            KeyZoneBehavior::StrongAccept
            | KeyZoneBehavior::WeakAccept
            | KeyZoneBehavior::SecondPush => 1.0,
            KeyZoneBehavior::StrongReject
            | KeyZoneBehavior::WeakReject
            | KeyZoneBehavior::BreakoutFailure => -1.0,
        };
        (dir_sign * behavior_sign * self.strength).clamp(-1.0, 1.0)
    }
}

#[derive(Debug, Clone)]
pub struct KeyZoneReaction {
    pub sbar_id: u64,
    pub state: KeyZoneState,
    pub strength: f64,
}

#[derive(Debug, Clone)]
pub struct KeyZone {
    pub id: Option<u64>,
    pub timeframe: Timeframe,
    pub origin_type: KeyZoneOrigin,
    pub orientation: KeyZoneOrientation,
    pub upper: f64,
    pub lower: f64,
    pub touch_count: u32,
    pub last_touch_id: Option<u64>,
    pub sbar_start_id: u64,
    pub sbar_end_id: u64,
    pub direction_hint: Direction,
    pub reactions: Vec<KeyZoneReaction>,
}

impl KeyZone {
    pub fn contains(&self, price: f64) -> bool {
        self.lower <= price && price <= self.upper
    }
}

pub struct KeyZoneManager {
    rows: Vec<KeyZone>,
    id_cursor: u64,
    latest_signal: Option<KeyZoneSignal>,
}

impl Default for KeyZoneManager {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyZoneManager {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            id_cursor: 0,
            latest_signal: None,
        }
    }

    pub fn rebuild_from(
        &mut self,
        timeframe: Timeframe,
        swings: &[Swing],
        trends: &[Trend],
        sbars: &[SBar],
    ) {
        self.rows.clear();
        self.latest_signal = None;
        for swing in swings.iter().rev().take(5) {
            self.push_zone_from_swing(timeframe, swing, sbars);
        }
        for trend in trends.iter().rev().take(5) {
            self.push_zone_from_trend(timeframe, trend, sbars);
        }
    }

    fn push_zone_from_swing(&mut self, timeframe: Timeframe, swing: &Swing, sbars: &[SBar]) {
        self.id_cursor += 1;
        let (lower, upper, touch_count, last_touch_id) = refine_zone_bounds(
            sbars,
            swing.sbar_start_id,
            swing.sbar_end_id,
            swing.direction,
            swing.low_price,
            swing.high_price,
        );
        self.rows.push(KeyZone {
            id: Some(self.id_cursor),
            timeframe,
            origin_type: KeyZoneOrigin::Swing,
            orientation: KeyZoneOrientation::Horizontal,
            upper,
            lower,
            touch_count,
            last_touch_id,
            sbar_start_id: swing.sbar_start_id,
            sbar_end_id: swing.sbar_end_id,
            direction_hint: swing.direction,
            reactions: Vec::new(),
        });
    }

    fn push_zone_from_trend(&mut self, timeframe: Timeframe, trend: &Trend, sbars: &[SBar]) {
        self.id_cursor += 1;
        let (lower, upper, touch_count, last_touch_id) = refine_zone_bounds(
            sbars,
            trend.sbar_start_id,
            trend.sbar_end_id,
            trend.direction,
            trend.low_price,
            trend.high_price,
        );
        self.rows.push(KeyZone {
            id: Some(self.id_cursor),
            timeframe,
            origin_type: KeyZoneOrigin::Trend,
            orientation: KeyZoneOrientation::Horizontal,
            upper,
            lower,
            touch_count,
            last_touch_id,
            sbar_start_id: trend.sbar_start_id,
            sbar_end_id: trend.sbar_end_id,
            direction_hint: trend.direction,
            reactions: Vec::new(),
        });
    }

    pub fn rows(&self) -> &[KeyZone] {
        &self.rows
    }

    pub fn evaluate_latest_signal(
        &mut self,
        latest_bar: &SBar,
        prev_bar: Option<&SBar>,
    ) -> Option<KeyZoneSignal> {
        let mut best: Option<KeyZoneSignal> = None;
        for zone in &mut self.rows {
            if let Some(signal) = classify_zone_signal(zone, latest_bar, prev_bar) {
                let state = match signal.behavior {
                    KeyZoneBehavior::StrongAccept
                    | KeyZoneBehavior::WeakAccept
                    | KeyZoneBehavior::SecondPush => KeyZoneState::Accept,
                    KeyZoneBehavior::StrongReject
                    | KeyZoneBehavior::WeakReject
                    | KeyZoneBehavior::BreakoutFailure => KeyZoneState::Reject,
                };
                zone.reactions.push(KeyZoneReaction {
                    sbar_id: latest_bar.id.unwrap_or_default(),
                    state,
                    strength: signal.strength,
                });
                let should_replace = best
                    .as_ref()
                    .map(|x| signal.strength > x.strength)
                    .unwrap_or(true);
                if should_replace {
                    best = Some(signal);
                }
            }
        }
        self.latest_signal = best.clone();
        best
    }

    pub fn latest_signal(&self) -> Option<&KeyZoneSignal> {
        self.latest_signal.as_ref()
    }
}

fn classify_zone_signal(
    zone: &KeyZone,
    latest_bar: &SBar,
    prev_bar: Option<&SBar>,
) -> Option<KeyZoneSignal> {
    let overlap_low = latest_bar.low_price.max(zone.lower);
    let overlap_high = latest_bar.high_price.min(zone.upper);
    if overlap_high <= overlap_low {
        return None;
    }

    let zone_span = (zone.upper - zone.lower).max(1e-6);
    let overlap_ratio = ((overlap_high - overlap_low) / zone_span).clamp(0.0, 1.0);
    let body_ratio = (latest_bar.body() / latest_bar.total_range().max(1e-6)).clamp(0.0, 1.0);
    let closes_inside = zone.contains(latest_bar.close_price);
    let closes_opposite_side = match zone.direction_hint {
        Direction::Up => latest_bar.close_price < zone.lower,
        Direction::Down => latest_bar.close_price > zone.upper,
        _ => !closes_inside,
    };
    let directional_body = match zone.direction_hint {
        Direction::Up => latest_bar.close_price >= latest_bar.open_price,
        Direction::Down => latest_bar.close_price <= latest_bar.open_price,
        _ => true,
    };

    let prev_broke_with_hint = prev_bar
        .map(|bar| match zone.direction_hint {
            Direction::Up => bar.close_price > zone.upper,
            Direction::Down => bar.close_price < zone.lower,
            _ => false,
        })
        .unwrap_or(false);
    if prev_broke_with_hint && closes_inside {
        return Some(KeyZoneSignal {
            zone_id: zone.id,
            behavior: KeyZoneBehavior::BreakoutFailure,
            direction: zone.direction_hint,
            strength: (0.65 + 0.35 * overlap_ratio).clamp(0.0, 1.0),
            sbar_id: latest_bar.id.unwrap_or_default(),
        });
    }

    let prev_touched = prev_bar
        .map(|bar| bar.low_price <= zone.upper && bar.high_price >= zone.lower)
        .unwrap_or(false);
    if prev_touched && zone.touch_count >= 2 && directional_body {
        return Some(KeyZoneSignal {
            zone_id: zone.id,
            behavior: KeyZoneBehavior::SecondPush,
            direction: zone.direction_hint,
            strength: (0.55 + 0.45 * body_ratio).clamp(0.0, 1.0),
            sbar_id: latest_bar.id.unwrap_or_default(),
        });
    }

    if closes_inside && directional_body {
        let behavior = if overlap_ratio >= 0.55 && body_ratio >= 0.45 {
            KeyZoneBehavior::StrongAccept
        } else {
            KeyZoneBehavior::WeakAccept
        };
        return Some(KeyZoneSignal {
            zone_id: zone.id,
            behavior,
            direction: zone.direction_hint,
            strength: (0.4 * overlap_ratio + 0.6 * body_ratio).clamp(0.0, 1.0),
            sbar_id: latest_bar.id.unwrap_or_default(),
        });
    }

    if closes_opposite_side || !directional_body {
        let behavior = if closes_opposite_side || body_ratio >= 0.45 {
            KeyZoneBehavior::StrongReject
        } else {
            KeyZoneBehavior::WeakReject
        };
        return Some(KeyZoneSignal {
            zone_id: zone.id,
            behavior,
            direction: zone.direction_hint,
            strength: (0.5 * overlap_ratio + 0.5 * body_ratio).clamp(0.0, 1.0),
            sbar_id: latest_bar.id.unwrap_or_default(),
        });
    }

    None
}

fn refine_zone_bounds(
    sbars: &[SBar],
    start_id: u64,
    end_id: u64,
    direction: Direction,
    fallback_lower: f64,
    fallback_upper: f64,
) -> (f64, f64, u32, Option<u64>) {
    let scope = sbars
        .iter()
        .filter(|x| {
            let id = x.id.unwrap_or_default();
            start_id <= id && id <= end_id
        })
        .collect::<Vec<_>>();
    if scope.is_empty() {
        return (fallback_lower, fallback_upper, 0, None);
    }

    let tick = estimate_tick_size(&scope).max(1e-6);
    let highs = scope.iter().map(|x| x.high_price).collect::<Vec<_>>();
    let lows = scope.iter().map(|x| x.low_price).collect::<Vec<_>>();
    let opens = scope.iter().map(|x| x.open_price).collect::<Vec<_>>();
    let closes = scope.iter().map(|x| x.close_price).collect::<Vec<_>>();

    let (mut lower, mut upper, start_price, end_price) = match direction {
        Direction::Up => {
            let upper = highs.iter().copied().fold(f64::MIN, f64::max);
            let min_body_top = opens
                .iter()
                .zip(closes.iter())
                .map(|(o, c)| o.max(*c))
                .fold(f64::MAX, f64::min);
            (fallback_lower, upper, min_body_top, upper)
        }
        Direction::Down => {
            let lower = lows.iter().copied().fold(f64::MAX, f64::min);
            let max_body_bottom = opens
                .iter()
                .zip(closes.iter())
                .map(|(o, c)| o.min(*c))
                .fold(f64::MIN, f64::max);
            (lower, fallback_upper, lower, max_body_bottom)
        }
        _ => (fallback_lower, fallback_upper, fallback_lower, fallback_upper),
    };

    if end_price <= start_price {
        return (
            lower.min(upper),
            upper.max(lower),
            scope.len() as u32,
            scope.last().and_then(|x| x.id),
        );
    }

    let mut best_price = start_price;
    let mut best_count = 0u32;
    let mut price = start_price;
    while price <= end_price + tick * 0.5 {
        let mut count = 0u32;
        for bar in &scope {
            if bar.low_price <= price && price <= bar.high_price {
                count += 1;
            }
        }
        if count > best_count {
            best_count = count;
            best_price = price;
        }
        price += tick;
    }

    match direction {
        Direction::Up => {
            lower = best_price;
        }
        Direction::Down => {
            upper = best_price;
        }
        _ => {}
    }

    (
        lower.min(upper),
        upper.max(lower),
        best_count.max(1),
        scope.last().and_then(|x| x.id),
    )
}

fn estimate_tick_size(scope: &[&SBar]) -> f64 {
    let mut min_step = f64::MAX;
    for bar in scope {
        let candidates = [
            (bar.high_price - bar.low_price).abs(),
            (bar.close_price - bar.open_price).abs(),
            (bar.high_price - bar.open_price).abs(),
            (bar.low_price - bar.open_price).abs(),
        ];
        for step in candidates {
            if step > 1e-9 && step < min_step {
                min_step = step;
            }
        }
    }
    if min_step == f64::MAX {
        0.2
    } else {
        (min_step / 10.0).max(1e-4)
    }
}

