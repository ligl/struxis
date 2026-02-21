use chrono::{Timelike, Utc};

use struxis::{DataReceiver, MarketBarInput, MultiTimeframeContext, TickInput, Timeframe};

#[test]
fn ingest_single_bar_works() {
    let mut receiver = DataReceiver::new(MultiTimeframeContext::new("I2601.DCE"));
    receiver.register_timeframe(Timeframe::M5);

    receiver.ingest_bar(MarketBarInput {
        symbol: "I2601".to_string(),
        exchange: "DCE".to_string(),
        timeframe: Timeframe::M5,
        datetime: Utc::now(),
        open_price: 100.0,
        high_price: 101.0,
        low_price: 99.0,
        close_price: 100.5,
        volume: 10.0,
        open_interest: 12.0,
        turnover: 1000.0,
    });

    assert_eq!(receiver.mtc().count(Timeframe::M5), 1);
}

#[test]
fn ingest_tick_generates_m1() {
    let mut receiver = DataReceiver::new(MultiTimeframeContext::new("I2601.DCE"));
    receiver.register_timeframe(Timeframe::M1);

    let t1 = Utc::now()
        .with_second(10)
        .and_then(|x: chrono::DateTime<Utc>| x.with_nanosecond(0))
        .expect("valid dt");
    let t2 = t1 + chrono::Duration::minutes(1);

    let _ = receiver.ingest_tick(TickInput {
        symbol: "I2601".to_string(),
        exchange: "DCE".to_string(),
        datetime: t1,
        last_price: 100.0,
        volume: 10.0,
        turnover: 1000.0,
        open_interest: 200.0,
    });
    let emitted = receiver.ingest_tick(TickInput {
        symbol: "I2601".to_string(),
        exchange: "DCE".to_string(),
        datetime: t2,
        last_price: 101.0,
        volume: 20.0,
        turnover: 2100.0,
        open_interest: 210.0,
    });

    assert_eq!(emitted, 1);
    assert_eq!(receiver.mtc().count(Timeframe::M1), 1);
}

#[test]
fn e2e_pipeline_is_deterministic_across_independent_runs() {
    let inputs = sample_bars(60);

    let mut receiver_a = DataReceiver::new(MultiTimeframeContext::new("I2601.DCE"));
    receiver_a.register_timeframe(Timeframe::M5);
    receiver_a.ingest_batch(inputs.clone());

    let mut receiver_b = DataReceiver::new(MultiTimeframeContext::new("I2601.DCE"));
    receiver_b.register_timeframe(Timeframe::M5);
    receiver_b.ingest_batch(inputs);

    let a_swings = receiver_a.mtc().get_swing_window(Timeframe::M5, 1000);
    let b_swings = receiver_b.mtc().get_swing_window(Timeframe::M5, 1000);
    let a_trends = receiver_a.mtc().get_trend_window(Timeframe::M5, 1000);
    let b_trends = receiver_b.mtc().get_trend_window(Timeframe::M5, 1000);

    assert_eq!(a_swings.len(), b_swings.len());
    assert_eq!(a_trends.len(), b_trends.len());

    let a_swing_core = a_swings
        .iter()
        .map(|x| {
            (
                x.id,
                x.direction,
                x.cbar_start_id,
                x.cbar_end_id,
                x.sbar_start_id,
                x.sbar_end_id,
                x.is_completed,
            )
        })
        .collect::<Vec<_>>();
    let b_swing_core = b_swings
        .iter()
        .map(|x| {
            (
                x.id,
                x.direction,
                x.cbar_start_id,
                x.cbar_end_id,
                x.sbar_start_id,
                x.sbar_end_id,
                x.is_completed,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(a_swing_core, b_swing_core);

    let a_trend_core = a_trends
        .iter()
        .map(|x| {
            (
                x.id,
                x.direction,
                x.swing_start_id,
                x.swing_end_id,
                x.sbar_start_id,
                x.sbar_end_id,
                x.is_completed,
            )
        })
        .collect::<Vec<_>>();
    let b_trend_core = b_trends
        .iter()
        .map(|x| {
            (
                x.id,
                x.direction,
                x.swing_start_id,
                x.swing_end_id,
                x.sbar_start_id,
                x.sbar_end_id,
                x.is_completed,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(a_trend_core, b_trend_core);
}

#[test]
fn ingest_batch_matches_incremental_ingest() {
    let inputs = sample_bars(50);

    let mut batch_receiver = DataReceiver::new(MultiTimeframeContext::new("I2601.DCE"));
    batch_receiver.register_timeframe(Timeframe::M5);
    batch_receiver.ingest_batch(inputs.clone());

    let mut inc_receiver = DataReceiver::new(MultiTimeframeContext::new("I2601.DCE"));
    inc_receiver.register_timeframe(Timeframe::M5);
    for input in inputs {
        inc_receiver.ingest_bar(input);
    }

    let batch_cbar = batch_receiver.mtc().get_cbar_window(Timeframe::M5, 1000);
    let inc_cbar = inc_receiver.mtc().get_cbar_window(Timeframe::M5, 1000);
    let batch_swing = batch_receiver.mtc().get_swing_window(Timeframe::M5, 1000);
    let inc_swing = inc_receiver.mtc().get_swing_window(Timeframe::M5, 1000);

    assert_eq!(batch_cbar.len(), inc_cbar.len());
    assert_eq!(batch_swing.len(), inc_swing.len());

    let batch_cbar_core = batch_cbar
        .iter()
        .map(|x| (x.id, x.sbar_start_id, x.sbar_end_id, x.fractal_type))
        .collect::<Vec<_>>();
    let inc_cbar_core = inc_cbar
        .iter()
        .map(|x| (x.id, x.sbar_start_id, x.sbar_end_id, x.fractal_type))
        .collect::<Vec<_>>();
    assert_eq!(batch_cbar_core, inc_cbar_core);
}

fn sample_bars(count: usize) -> Vec<MarketBarInput> {
    let mut bars = Vec::with_capacity(count);
    let base_dt = Utc::now()
        .with_second(0)
        .and_then(|x| x.with_nanosecond(0))
        .expect("valid dt");

    let mut price = 100.0;
    for i in 0..count {
        let phase = (i % 10) as f64;
        let wave = if i % 2 == 0 {
            0.6 + phase * 0.03
        } else {
            -0.5 + phase * 0.02
        };
        let open = price;
        let close = (open + wave).max(1.0);
        let high = open.max(close) + 0.4 + (phase * 0.01);
        let low = open.min(close) - 0.35 - (phase * 0.01);
        let volume = 100.0 + (i as f64 * 3.0);
        let open_interest = 1000.0 + (i as f64 * 5.0);
        price = close;

        bars.push(MarketBarInput {
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
