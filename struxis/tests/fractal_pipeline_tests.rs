use chrono::{Timelike, Utc};

use struxis::{Fractal, FractalType, MultiTimeframeContext, SBar, Timeframe};

#[test]
fn fractal_labels_match_local_three_cbar_rule() {
    let mut mtc = MultiTimeframeContext::new("I8888.XDCE");
    mtc.register(Timeframe::M15);

    for bar in sample_bars(160) {
        mtc.append(Timeframe::M15, bar);
    }

    let cbars = mtc.get_cbar_window(Timeframe::M15, usize::MAX);
    assert!(cbars.len() > 20, "need enough cbar rows for fractal validation");

    assert_eq!(cbars.first().map(|x| x.fractal_type), Some(FractalType::None));
    assert_eq!(cbars.last().map(|x| x.fractal_type), Some(FractalType::None));

    for i in 1..(cbars.len() - 1) {
        let expected = Fractal::verify(&cbars[i - 1], &cbars[i], &cbars[i + 1]);
        assert_eq!(
            cbars[i].fractal_type,
            expected,
            "fractal mismatch at cbar id {:?}",
            cbars[i].id
        );
    }
}

#[test]
fn incremental_append_never_uses_future_information() {
    let input = sample_bars(120);
    let mut mtc = MultiTimeframeContext::new("I8888.XDCE");
    mtc.register(Timeframe::M15);

    for bar in input {
        mtc.append(Timeframe::M15, bar);
        let cbars = mtc.get_cbar_window(Timeframe::M15, usize::MAX);
        if cbars.len() < 3 {
            continue;
        }

        assert_eq!(cbars.first().map(|x| x.fractal_type), Some(FractalType::None));
        assert_eq!(cbars.last().map(|x| x.fractal_type), Some(FractalType::None));

        for i in 1..(cbars.len() - 1) {
            let expected = Fractal::verify(&cbars[i - 1], &cbars[i], &cbars[i + 1]);
            assert_eq!(
                cbars[i].fractal_type,
                expected,
                "incremental fractal mismatch at cbar id {:?}",
                cbars[i].id
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

    let mut price = 100.0_f64;
    let cycle = [0.0_f64, 3.1, -2.9, 4.2, -3.3, 2.0, -1.7, 3.5, -2.6, 1.4];
    for i in 0..count {
        let open = price;
        let drift = (i as f64) * 0.02;
        let close = (100.0 + drift + cycle[i % cycle.len()]).max(1.0);
        let high = open.max(close) + 0.9;
        let low = open.min(close) - 0.9;
        let volume = 100.0 + (i as f64 * 1.5);
        let open_interest = 1200.0 + (i as f64 * 2.5);
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
