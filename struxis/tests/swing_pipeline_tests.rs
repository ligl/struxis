use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Timelike, Utc};
use serde::Deserialize;

use struxis::{Direction, FractalType, MultiTimeframeContext, SBar, Timeframe};

#[test]
fn completed_swings_follow_bottom_top_and_top_bottom_contract() {
    let mut mtc = MultiTimeframeContext::new("I8888.XDCE");
    mtc.register(Timeframe::M15);

    for bar in sample_dataset_bars(300) {
        mtc.append(Timeframe::M15, bar);
    }

    let cbars = mtc.get_cbar_window(Timeframe::M15, usize::MAX);
    let swings = mtc.get_swing_window(Timeframe::M15, usize::MAX);

    let fractal_by_cbar_id = cbars
        .iter()
        .filter_map(|x| x.id.map(|id| (id, x.fractal_type)))
        .collect::<std::collections::HashMap<_, _>>();

    let completed = swings
        .iter()
        .filter(|x| x.state == struxis::SwingState::Confirmed)
        .collect::<Vec<_>>();
    assert!(
        !completed.is_empty(),
        "need completed swings to validate swing direction contract"
    );

    for swing in completed {
        let start_ft = fractal_by_cbar_id
            .get(&swing.cbar_start_id)
            .copied()
            .unwrap_or(FractalType::None);
        let end_ft = fractal_by_cbar_id
            .get(&swing.cbar_end_id)
            .copied()
            .unwrap_or(FractalType::None);

        match swing.direction {
            Direction::Up => {
                assert_eq!(
                    start_ft,
                    FractalType::Bottom,
                    "up swing must start from bottom fractal"
                );
                assert_eq!(
                    end_ft,
                    FractalType::Top,
                    "up swing must end at top fractal"
                );
            }
            Direction::Down => {
                assert_eq!(
                    start_ft,
                    FractalType::Top,
                    "down swing must start from top fractal"
                );
                assert_eq!(
                    end_ft,
                    FractalType::Bottom,
                    "down swing must end at bottom fractal"
                );
            }
            _ => panic!("completed swing should not be non-directional"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CsvBarRow {
    datetime: String,
    #[serde(alias = "open")]
    open_price: f64,
    #[serde(alias = "high")]
    high_price: f64,
    #[serde(alias = "low")]
    low_price: f64,
    #[serde(alias = "close")]
    close_price: f64,
    #[serde(default)]
    volume: f64,
    #[serde(default)]
    open_interest: f64,
    #[serde(default, alias = "money")]
    turnover: f64,
}

fn sample_dataset_bars(limit: usize) -> Vec<SBar> {
    let dataset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("dataset")
        .join("I8888.XDCE_15m.csv");

    let mut csv = csv::Reader::from_path(dataset).expect("dataset should load");
    csv.deserialize::<CsvBarRow>()
        .take(limit)
        .map(|row| {
            let row = row.expect("valid csv row");
            SBar {
                id: None,
                symbol: "I8888".to_string(),
                exchange: "XDCE".to_string(),
                timeframe: Timeframe::M15,
                datetime: parse_datetime(&row.datetime).expect("valid datetime"),
                open_price: row.open_price,
                high_price: row.high_price,
                low_price: row.low_price,
                close_price: row.close_price,
                volume: row.volume,
                open_interest: row.open_interest,
                turnover: row.turnover,
            }
        })
        .collect::<Vec<_>>()
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }

    let patterns = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y/%m/%d %H:%M:%S%.f",
        "%Y%m%d%H%M%S%.f",
    ];

    for pattern in patterns {
        if let Ok(dt) = NaiveDateTime::parse_from_str(value, pattern) {
            return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
        }
    }

    if let Ok(d) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        if let Some(dt) = d.and_hms_opt(0, 0, 0) {
            return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
        }
    }

    Err(format!("invalid datetime: {value}"))
}

#[test]
fn incremental_append_keeps_completed_swings_directional() {
    let mut mtc = MultiTimeframeContext::new("I8888.XDCE");
    mtc.register(Timeframe::M15);

    for bar in sample_bars(160) {
        mtc.append(Timeframe::M15, bar);
        let swings = mtc.get_swing_window(Timeframe::M15, usize::MAX);

        for swing in swings.into_iter().filter(|x| x.state == struxis::SwingState::Confirmed) {
            assert!(
                matches!(swing.direction, Direction::Up | Direction::Down),
                "completed swing must have directional state"
            );
        }
    }
}

#[test]
fn confirmed_swings_are_cbar_continuous() {
    let mut mtc = MultiTimeframeContext::new("I8888.XDCE");
    mtc.register(Timeframe::M15);

    for bar in sample_dataset_bars(300) {
        mtc.append(Timeframe::M15, bar);
    }

    let swings = mtc
        .get_swing_window(Timeframe::M15, usize::MAX)
        .into_iter()
        .filter(|x| x.state == struxis::SwingState::Confirmed)
        .collect::<Vec<_>>();

    assert!(swings.len() >= 2, "need multiple confirmed swings for continuity check");
    for pair in swings.windows(2) {
        assert_eq!(
            pair[0].cbar_end_id,
            pair[1].cbar_start_id,
            "adjacent confirmed swings must connect on cbar endpoint"
        );
    }
}

fn sample_bars(count: usize) -> Vec<SBar> {
    let mut bars = Vec::with_capacity(count);
    let base_dt = Utc::now()
        .with_second(0)
        .and_then(|x| x.with_nanosecond(0))
        .expect("valid dt");

    let mut price = 100.0_f64;
    let cycle = [0.0_f64, 3.3, -2.7, 4.1, -3.4, 2.2, -1.8, 3.6, -2.5, 1.5];
    for i in 0..count {
        let open = price;
        let drift = (i as f64) * 0.03;
        let close = (100.0 + drift + cycle[i % cycle.len()]).max(1.0);
        let high = open.max(close) + 0.9;
        let low = open.min(close) - 0.9;
        let volume = 100.0 + (i as f64 * 2.0);
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