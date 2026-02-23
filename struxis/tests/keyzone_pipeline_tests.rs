use std::collections::{HashMap, HashSet};

use chrono::{Timelike, Utc};

use struxis::{
    Direction, KeyZoneManager, KeyZoneOrientation, KeyZoneOrigin, SBar, Swing, SwingState, Timeframe, Trend,
};

#[test]
fn keyzones_are_discrete_traceable_and_wick_refined() {
    let sbars = controlled_sbars(120);
    let swings = controlled_swings();
    let trends = controlled_trends();

    let mut manager = KeyZoneManager::new();
    manager.rebuild_from(Timeframe::M15, &swings, &trends, &sbars);
    let zones = manager.rows().to_vec();

    let expected_len = swings.len().min(5) + trends.len().min(5);
    assert_eq!(zones.len(), expected_len, "keyzone count should match source windows");

    let mut unique_ids = HashSet::new();
    for zone in &zones {
        let id = zone.id.expect("keyzone id should exist");
        assert!(unique_ids.insert(id), "keyzone id should be unique");
        assert_eq!(
            zone.orientation,
            KeyZoneOrientation::Horizontal,
            "keyzone should be a discrete horizontal zone object"
        );
        assert!(zone.upper >= zone.lower, "keyzone upper should be >= lower");
        assert!(zone.touch_count >= 1, "refined keyzone should have at least one touch");

        let last_touch = zone.last_touch_id.expect("refined keyzone should carry last_touch_id");
        assert!(
            zone.sbar_start_id <= last_touch && last_touch <= zone.sbar_end_id,
            "keyzone last_touch_id should be within its sbar range"
        );

        let scope = sbars_in_range(&sbars, zone.sbar_start_id, zone.sbar_end_id);
        assert!(!scope.is_empty(), "keyzone source sbar scope must exist");
        let min_low = scope.iter().map(|x| x.low_price).fold(f64::MAX, f64::min);
        let max_high = scope.iter().map(|x| x.high_price).fold(f64::MIN, f64::max);
        assert!(
            zone.lower >= min_low - 1e-9,
            "keyzone lower should be reproducible from sbar wick scope"
        );
        assert!(
            zone.upper <= max_high + 1e-9,
            "keyzone upper should be reproducible from sbar wick scope"
        );
    }

    let swing_by_span = swings
        .iter()
        .map(|x| ((x.sbar_start_id, x.sbar_end_id), x))
        .collect::<HashMap<_, _>>();
    let trend_by_span = trends
        .iter()
        .map(|x| ((x.sbar_start_id, x.sbar_end_id), x))
        .collect::<HashMap<_, _>>();

    for zone in &zones {
        match zone.origin_type {
            KeyZoneOrigin::Swing => {
                let src = swing_by_span
                    .get(&(zone.sbar_start_id, zone.sbar_end_id))
                    .expect("swing-origin zone should map back to source swing");
                assert_eq!(
                    zone.direction_hint, src.direction,
                    "swing-origin zone direction should trace to source"
                );
            }
            KeyZoneOrigin::Trend => {
                let src = trend_by_span
                    .get(&(zone.sbar_start_id, zone.sbar_end_id))
                    .expect("trend-origin zone should map back to source trend");
                assert_eq!(
                    zone.direction_hint, src.direction,
                    "trend-origin zone direction should trace to source"
                );
            }
            other => panic!("unexpected keyzone origin in pipeline test: {:?}", other),
        }
    }
}

#[test]
fn keyzone_rebuild_is_stable_for_same_input() {
    let sbars = controlled_sbars(120);
    let swings = controlled_swings();
    let trends = controlled_trends();

    let mut manager = KeyZoneManager::new();
    manager.rebuild_from(Timeframe::M15, &swings, &trends, &sbars);
    let first = manager
        .rows()
        .iter()
        .map(|x| {
            (
                x.origin_type,
                x.sbar_start_id,
                x.sbar_end_id,
                x.lower,
                x.upper,
                x.touch_count,
                x.last_touch_id,
                x.direction_hint,
            )
        })
        .collect::<Vec<_>>();

    manager.rebuild_from(Timeframe::M15, &swings, &trends, &sbars);
    let second = manager
        .rows()
        .iter()
        .map(|x| {
            (
                x.origin_type,
                x.sbar_start_id,
                x.sbar_end_id,
                x.lower,
                x.upper,
                x.touch_count,
                x.last_touch_id,
                x.direction_hint,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        first, second,
        "same input should produce stable keyzone set and bounds"
    );
}

fn sbars_in_range(sbars: &[SBar], start_id: u64, end_id: u64) -> Vec<&SBar> {
    sbars
        .iter()
        .filter(|x| {
            let id = x.id.unwrap_or_default();
            start_id <= id && id <= end_id
        })
        .collect::<Vec<_>>()
}

fn controlled_sbars(count: usize) -> Vec<SBar> {
    let mut bars = Vec::with_capacity(count);
    let base_dt = Utc::now()
        .with_second(0)
        .and_then(|x| x.with_nanosecond(0))
        .expect("valid dt");

    let mut price = 100.0_f64;
    let cycle = [0.0_f64, 1.6, -1.2, 2.1, -1.7, 1.0, -0.8, 1.9, -1.4, 0.9];
    for i in 0..count {
        let open = price;
        let drift = (i as f64) * 0.01;
        let close = (100.0 + drift + cycle[i % cycle.len()]).max(1.0);
        let high = open.max(close) + 0.5;
        let low = open.min(close) - 0.5;
        let volume = 120.0 + (i as f64 * 2.0);
        let open_interest = 1000.0 + (i as f64 * 1.5);
        price = close;

        bars.push(SBar {
            id: Some((i + 1) as u64),
            symbol: "I8888".to_string(),
            exchange: "XDCE".to_string(),
            timeframe: Timeframe::M15,
            datetime: base_dt + chrono::Duration::minutes((i as i64) * 15),
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            open_interest,
            turnover: volume * close,
        });
    }

    bars
}

fn controlled_swings() -> Vec<Swing> {
    fn swing(
        id: u64,
        direction: Direction,
        sbar_start_id: u64,
        sbar_end_id: u64,
        high: f64,
        low: f64,
    ) -> Swing {
        Swing {
            id: Some(id),
            direction,
            cbar_start_id: id,
            cbar_end_id: id,
            sbar_start_id,
            sbar_end_id,
            high_price: high,
            low_price: low,
            span: (sbar_end_id - sbar_start_id + 1) as usize,
            volume: 10.0,
            start_oi: 1000.0,
            end_oi: 1001.0,
            is_completed: true,
            state: SwingState::Confirmed,
            created_at: Utc::now(),
        }
    }

    vec![
        swing(1, Direction::Up, 10, 18, 103.8, 99.2),
        swing(2, Direction::Down, 19, 27, 104.1, 98.9),
        swing(3, Direction::Up, 28, 36, 105.0, 99.5),
        swing(4, Direction::Down, 37, 45, 104.6, 98.2),
        swing(5, Direction::Up, 46, 54, 105.4, 99.8),
        swing(6, Direction::Down, 55, 63, 104.9, 98.4),
    ]
}

fn controlled_trends() -> Vec<Trend> {
    fn trend(
        id: u64,
        direction: Direction,
        swing_start_id: u64,
        swing_end_id: u64,
        sbar_start_id: u64,
        sbar_end_id: u64,
        high: f64,
        low: f64,
    ) -> Trend {
        Trend {
            id: Some(id),
            direction,
            swing_start_id,
            swing_end_id,
            sbar_start_id,
            sbar_end_id,
            high_price: high,
            low_price: low,
            span: (sbar_end_id - sbar_start_id + 1) as usize,
            volume: 30.0,
            start_oi: 1000.0,
            end_oi: 1002.0,
            is_completed: true,
            created_at: Utc::now(),
        }
    }

    vec![
        trend(1, Direction::Up, 1, 3, 10, 36, 105.0, 98.9),
        trend(2, Direction::Down, 2, 4, 19, 45, 105.0, 98.2),
        trend(3, Direction::Up, 3, 5, 28, 54, 105.4, 98.2),
    ]
}