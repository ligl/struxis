use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::Deserialize;
use struxis::{
    DataReceiver, Fractal, FractalType, MarketBarInput, MultiTimeframeContext, Timeframe,
};

fn load_i888_receiver() -> DataReceiver {
    let mut receiver = DataReceiver::new(MultiTimeframeContext::new("I8888.XDCE"));
    receiver.register_timeframe(Timeframe::M15);

    let dataset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("dataset")
        .join("I8888.XDCE_15m.csv");

    let mut csv = csv::Reader::from_path(dataset).expect("dataset should load");
    for row in csv.deserialize::<CsvBarRow>().take(160) {
        let row = row.expect("valid csv row");
        receiver.ingest_bar(MarketBarInput {
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
        });
    }

    receiver
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
fn cbar_sequence_respects_non_inclusive_structure() {
    let receiver = load_i888_receiver();
    let cbars = receiver.mtc().get_cbar_window(Timeframe::M15, usize::MAX);

    assert!(cbars.len() > 20, "need enough cbars for semantics check");

    for pair in cbars.windows(2) {
        let left = &pair[0];
        let right = &pair[1];

        let inclusive = (left.high_price >= right.high_price && left.low_price <= right.low_price)
            || (left.high_price <= right.high_price && left.low_price >= right.low_price);

        assert!(
            !inclusive,
            "consecutive cbars should not be inclusive: left#{:?} right#{:?}",
            left.id,
            right.id
        );
    }
}

#[test]
fn cbar_fractal_labels_match_three_bar_rule() {
    let receiver = load_i888_receiver();
    let cbars = receiver.mtc().get_cbar_window(Timeframe::M15, usize::MAX);

    assert!(cbars.len() > 20, "need enough cbars for fractal check");

    for i in 1..(cbars.len() - 1) {
        let expected = Fractal::verify(&cbars[i - 1], &cbars[i], &cbars[i + 1]);
        assert_eq!(
            cbars[i].fractal_type,
            expected,
            "fractal label mismatch at cbar id {:?}",
            cbars[i].id
        );
    }
}

#[test]
fn completed_swings_start_end_on_opposite_fractals() {
    let receiver = load_i888_receiver();
    let cbars = receiver.mtc().get_cbar_window(Timeframe::M15, usize::MAX);
    let swings = receiver.mtc().get_swing_window(Timeframe::M15, usize::MAX);

    let mut fractal_by_cbar_id = std::collections::HashMap::new();
    for c in &cbars {
        if let Some(id) = c.id {
            fractal_by_cbar_id.insert(id, c.fractal_type);
        }
    }

    let completed = swings
        .into_iter()
        .filter(|x| x.state == struxis::SwingState::Confirmed)
        .collect::<Vec<_>>();
    assert!(
        !completed.is_empty(),
        "need completed swings to validate start/end fractal semantics"
    );

    for s in completed {
        let start_ft = fractal_by_cbar_id
            .get(&s.cbar_start_id)
            .copied()
            .unwrap_or(FractalType::None);
        let end_ft = fractal_by_cbar_id
            .get(&s.cbar_end_id)
            .copied()
            .unwrap_or(FractalType::None);

        match s.direction {
            struxis::Direction::Up => {
                assert_eq!(start_ft, FractalType::Bottom, "up swing must start from bottom fractal");
                assert_eq!(end_ft, FractalType::Top, "up swing must end at top fractal");
            }
            struxis::Direction::Down => {
                assert_eq!(start_ft, FractalType::Top, "down swing must start from top fractal");
                assert_eq!(end_ft, FractalType::Bottom, "down swing must end at bottom fractal");
            }
            _ => panic!("completed swing should not have non directional type"),
        }
    }
}
