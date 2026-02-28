use chrono::{Timelike, Utc};

use struxis::{
    Direction, KeyZone, KeyZoneManager, MultiTimeframeContext, SBar, Swing, SwingState, Timeframe,
    TrendManager,
};

#[test]
fn validates_intermediate_structures_invariants() {
    let mut mtc = MultiTimeframeContext::new("I2601.DCE");
    mtc.register(Timeframe::M5);

    for bar in sample_bars(120) {
        mtc.append(
            Timeframe::M5,
            bar,
        );
    }

    let cbars = mtc.get_cbar_window(Timeframe::M5, 300);
    let swings = mtc.get_swing_window(Timeframe::M5, 300);

    assert!(!cbars.is_empty(), "cbars should not be empty");
    assert!(!swings.is_empty(), "swings should not be empty");

    for (i, cbar) in cbars.iter().enumerate() {
        let id = cbar.id.expect("cbar id should be present");
        if i > 0 {
            let prev_id = cbars[i - 1].id.expect("prev cbar id should be present");
            assert!(id > prev_id, "cbar id should be strictly increasing");
        }
        assert!(
            cbar.high_price >= cbar.low_price,
            "cbar high should be >= low"
        );
        assert!(
            cbar.sbar_end_id >= cbar.sbar_start_id,
            "cbar sbar_end_id should be >= sbar_start_id"
        );
    }

    for (i, swing) in swings.iter().enumerate() {
        let id = swing.id.expect("swing id should be present");
        if i > 0 {
            let prev_id = swings[i - 1].id.expect("prev swing id should be present");
            assert!(id > prev_id, "swing id should be strictly increasing");
        }
        assert!(
            swing.high_price >= swing.low_price,
            "swing high should be >= low"
        );
        assert!(
            swing.cbar_end_id >= swing.cbar_start_id,
            "swing cbar_end_id should be >= cbar_start_id"
        );
        assert!(
            swing.sbar_end_id >= swing.sbar_start_id,
            "swing sbar_end_id should be >= sbar_start_id"
        );
        assert!(
            !matches!(swing.direction, Direction::None | Direction::Range),
            "swing direction should be directional"
        );
    }

}

#[test]
fn validates_trend_and_keyzone_invariants_on_controlled_data() {
    let swings = controlled_swings();

    let mut trend_manager = TrendManager::new();
    trend_manager.rebuild_from_swings(&swings);
    let trends = trend_manager.all_rows();
    assert!(!trends.is_empty(), "trends should not be empty");

    for (i, trend) in trends.iter().enumerate() {
        let id = trend.id.expect("trend id should be present");
        if i > 0 {
            let prev_id = trends[i - 1].id.expect("prev trend id should be present");
            assert!(id > prev_id, "trend id should be strictly increasing");
        }
        assert!(
            trend.high_price >= trend.low_price,
            "trend high should be >= low"
        );
        assert!(
            trend.swing_end_id >= trend.swing_start_id,
            "trend swing_end_id should be >= swing_start_id"
        );
        assert!(
            trend.sbar_end_id >= trend.sbar_start_id,
            "trend sbar_end_id should be >= sbar_start_id"
        );
        assert!(
            !matches!(trend.direction, Direction::None | Direction::Range),
            "trend direction should be directional"
        );
    }

    let sbars = controlled_sbars(200);
    let mut keyzone_manager = KeyZoneManager::new();
    keyzone_manager.rebuild_from(Timeframe::M5, &swings, &trends, &sbars);
    let keyzones = keyzone_manager.rows().to_vec();
    assert!(!keyzones.is_empty(), "keyzones should not be empty");

    assert_all_keyzones_valid(&keyzones);
}

fn assert_all_keyzones_valid(keyzones: &[KeyZone]) {
    for zone in keyzones {
        let id = zone.id.expect("keyzone id should be present");
        assert!(id > 0, "keyzone id should be positive");
        assert!(zone.upper >= zone.lower, "keyzone upper should be >= lower");
        assert!(
            zone.sbar_end_id >= zone.sbar_start_id,
            "keyzone sbar_end_id should be >= sbar_start_id"
        );
        if let Some(last_touch_id) = zone.last_touch_id {
            assert!(
                last_touch_id >= zone.sbar_start_id && last_touch_id <= zone.sbar_end_id,
                "keyzone last_touch_id should be in zone span"
            );
        }
    }
}

fn sample_bars(count: usize) -> Vec<SBar> {
    let mut bars = Vec::with_capacity(count);
    let base_dt = Utc::now()
        .with_second(0)
        .and_then(|x| x.with_nanosecond(0))
        .expect("valid dt");

    let mut price: f64 = 100.0;
    let cycle = [0.0_f64, 3.2, -2.8, 4.1, -3.6, 2.4, -1.9, 3.7];
    for i in 0..count {
        let open: f64 = price;
        let drift = (i as f64) * 0.06;
        let target = (100.0 + drift + cycle[i % cycle.len()]).max(1.0);
        let close: f64 = target;
        let high = open.max(close) + 0.9;
        let low = open.min(close) - 0.9;
        let volume = 120.0 + (i as f64 * 3.0);
        let open_interest = 1000.0 + (i as f64 * 5.0);
        price = close;

        bars.push(SBar {
            id: None,
            symbol: "I2601".to_string(),
            exchange: "DCE".to_string(),
            timeframe: Timeframe::M5,
            datetime: base_dt + chrono::Duration::minutes((i as i64) * 5),
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
    fn swing(id: u64, direction: Direction, high: f64, low: f64) -> Swing {
        Swing {
            id: Some(id),
            direction,
            cbar_start_id: id,
            cbar_end_id: id,
            sbar_start_id: id,
            sbar_end_id: id,
            high_price: high,
            low_price: low,
            span: 1,
            volume: 1.0,
            start_oi: 1.0,
            end_oi: 1.0,
            state: SwingState::Confirmed,
            created_at: Utc::now(),
        }
    }

    vec![
        swing(1, Direction::Up, 10.0, 5.0),
        swing(2, Direction::Down, 9.0, 4.0),
        swing(3, Direction::Up, 11.0, 6.0),
        swing(4, Direction::Down, 11.0, 9.0),
        swing(5, Direction::Up, 12.0, 10.0),
        swing(6, Direction::Down, 10.0, 8.0),
        swing(7, Direction::Up, 13.0, 11.0),
        swing(8, Direction::Down, 12.0, 7.0),
    ]
}

fn controlled_sbars(count: usize) -> Vec<SBar> {
    let mut bars = Vec::with_capacity(count);
    let base_dt = Utc::now()
        .with_second(0)
        .and_then(|x| x.with_nanosecond(0))
        .expect("valid dt");

    let mut price: f64 = 100.0;
    for i in 0..count {
        let open: f64 = price;
        let delta = if i % 2 == 0 { 1.0 } else { -0.8 };
        let close: f64 = (open + delta).max(1.0);
        let high = open.max(close) + 0.5;
        let low = open.min(close) - 0.5;
        let volume = 100.0 + i as f64;
        let open_interest = 1000.0 + (i as f64 * 2.0);
        price = close;

        bars.push(SBar {
            id: Some((i + 1) as u64),
            symbol: "I2601".to_string(),
            exchange: "DCE".to_string(),
            timeframe: Timeframe::M5,
            datetime: base_dt + chrono::Duration::minutes((i as i64) * 5),
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
