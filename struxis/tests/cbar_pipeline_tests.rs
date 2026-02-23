use chrono::{Timelike, Utc};

use struxis::{Fractal, SBar, Timeframe, MultiTimeframeContext};

#[test]
fn cbar_ranges_cover_all_sbars_without_gap_or_overlap() {
    let mut mtc = MultiTimeframeContext::new("I8888.XDCE");
    mtc.register(Timeframe::M15);

    for bar in sample_bars(120) {
        mtc.append(Timeframe::M15, bar);
    }

    let cbar_rows = mtc.get_cbar_window(Timeframe::M15, usize::MAX);
    let sbar_count = mtc.count(Timeframe::M15) as u64;

    assert!(!cbar_rows.is_empty(), "cbar sequence should not be empty");

    let mut expected_start = 1_u64;
    for row in &cbar_rows {
        assert_eq!(
            row.sbar_start_id, expected_start,
            "cbar range should start from expected sbar id"
        );
        assert!(
            row.sbar_end_id >= row.sbar_start_id,
            "cbar range end should be >= start"
        );
        expected_start = row.sbar_end_id + 1;
    }

    assert_eq!(
        expected_start,
        sbar_count + 1,
        "cbar ranges should cover all sbars exactly once"
    );
}

#[test]
fn cbar_adjacent_rows_are_non_inclusive() {
    let mut mtc = MultiTimeframeContext::new("I8888.XDCE");
    mtc.register(Timeframe::M15);

    for bar in sample_bars(120) {
        mtc.append(Timeframe::M15, bar);
    }

    let cbar_rows = mtc.get_cbar_window(Timeframe::M15, usize::MAX);
    assert!(cbar_rows.len() > 10, "need enough cbar rows for inclusion check");

    for pair in cbar_rows.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        let inclusive = (left.high_price >= right.high_price && left.low_price <= right.low_price)
            || (left.high_price <= right.high_price && left.low_price >= right.low_price);

        assert!(
            !inclusive,
            "adjacent cbar rows should not be inclusive: left#{:?} right#{:?}",
            left.id,
            right.id
        );
    }
}

#[test]
fn cbar_fractal_labels_match_three_bar_rule() {
    let mut mtc = MultiTimeframeContext::new("I8888.XDCE");
    mtc.register(Timeframe::M15);

    for bar in sample_bars(120) {
        mtc.append(Timeframe::M15, bar);
    }

    let cbar_rows = mtc.get_cbar_window(Timeframe::M15, usize::MAX);
    assert!(cbar_rows.len() > 10, "need enough cbar rows for fractal check");

    for i in 1..(cbar_rows.len() - 1) {
        let expected = Fractal::verify(&cbar_rows[i - 1], &cbar_rows[i], &cbar_rows[i + 1]);
        assert_eq!(
            cbar_rows[i].fractal_type,
            expected,
            "fractal label mismatch at cbar id {:?}",
            cbar_rows[i].id
        );
    }
}

fn sample_bars(count: usize) -> Vec<SBar> {
    let mut bars = Vec::with_capacity(count);
    let base_dt = Utc::now()
        .with_second(0)
        .and_then(|x| x.with_nanosecond(0))
        .expect("valid dt");

    let mut price: f64 = 100.0;
    let cycle = [0.0_f64, 2.8, -2.4, 3.7, -3.1, 2.2, -1.6, 3.4, -2.9, 1.8];
    for i in 0..count {
        let open = price;
        let drift = (i as f64) * 0.03;
        let target = (100.0 + drift + cycle[i % cycle.len()]).max(1.0);
        let close = target;
        let high = open.max(close) + 0.8;
        let low = open.min(close) - 0.8;
        let volume = 120.0 + (i as f64 * 2.0);
        let open_interest = 1000.0 + (i as f64 * 3.0);
        price = close;

        bars.push(SBar {
            id: None,
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
