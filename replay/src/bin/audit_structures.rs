use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::Deserialize;
use struxis::{
    DataReceiver, Direction, Fractal, FractalType, MarketBarInput, MultiTimeframeContext, Timeframe,
};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "usage: cargo run -q -p replay --bin audit_structures -- <csv_path> <symbol> <exchange> <timeframe:15m|1h|1d|5m|1m> [max_rows]"
        );
        std::process::exit(2);
    }

    let csv_path = PathBuf::from(&args[1]);
    let symbol = args[2].clone();
    let exchange = args[3].clone();
    let timeframe = parse_timeframe(&args[4])?;
    let max_rows = if args.len() >= 6 {
        args[5].parse::<usize>()?
    } else {
        300
    };

    let mut receiver = DataReceiver::new(MultiTimeframeContext::new(format!("{}.{}", symbol, exchange)));
    receiver.register_timeframe(timeframe);

    let mut csv = csv::Reader::from_path(csv_path)?;
    for row in csv.deserialize::<CsvBarRow>().take(max_rows) {
        let row = row?;
        receiver.ingest_bar(MarketBarInput {
            symbol: symbol.clone(),
            exchange: exchange.clone(),
            timeframe,
            datetime: parse_datetime(&row.datetime)?,
            open_price: row.open_price,
            high_price: row.high_price,
            low_price: row.low_price,
            close_price: row.close_price,
            volume: row.volume,
            open_interest: row.open_interest,
            turnover: row.turnover,
        });
    }

    let mtc = receiver.mtc();
    let count = mtc.count(timeframe);
    let cbars = mtc.get_cbar_window(timeframe, count.saturating_mul(3));
    let swings = mtc.get_swing_window(timeframe, count.saturating_mul(3));
    let trends = mtc.get_trend_window(timeframe, count.saturating_mul(3));

    let mut violations = Vec::<String>::new();

    for pair in cbars.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        let inclusive = (left.high_price >= right.high_price && left.low_price <= right.low_price)
            || (left.high_price <= right.high_price && left.low_price >= right.low_price);
        if inclusive {
            violations.push(format!(
                "CBAR inclusive violation: left#{:?} right#{:?}",
                left.id, right.id
            ));
        }
    }

    if cbars.len() >= 3 {
        for i in 1..(cbars.len() - 1) {
            let expected = Fractal::verify(&cbars[i - 1], &cbars[i], &cbars[i + 1]);
            if cbars[i].fractal_type != expected {
                violations.push(format!(
                    "CBAR fractal mismatch at {:?}: got {:?}, expected {:?}",
                    cbars[i].id, cbars[i].fractal_type, expected
                ));
            }
        }
    }

    let mut fractal_by_cbar_id = HashMap::new();
    for c in &cbars {
        if let Some(id) = c.id {
            fractal_by_cbar_id.insert(id, c.fractal_type);
        }
    }

    for s in swings.iter().filter(|x| x.is_completed) {
        let start_ft = fractal_by_cbar_id
            .get(&s.cbar_start_id)
            .copied()
            .unwrap_or(FractalType::None);
        let end_ft = fractal_by_cbar_id
            .get(&s.cbar_end_id)
            .copied()
            .unwrap_or(FractalType::None);

        match s.direction {
            Direction::Up => {
                if start_ft != FractalType::Bottom || end_ft != FractalType::Top {
                    violations.push(format!(
                        "SWING semantic mismatch id={:?} dir=Up start_ft={:?} end_ft={:?}",
                        s.id, start_ft, end_ft
                    ));
                }
            }
            Direction::Down => {
                if start_ft != FractalType::Top || end_ft != FractalType::Bottom {
                    violations.push(format!(
                        "SWING semantic mismatch id={:?} dir=Down start_ft={:?} end_ft={:?}",
                        s.id, start_ft, end_ft
                    ));
                }
            }
            _ => {
                violations.push(format!("SWING non directional completed id={:?}", s.id));
            }
        }
    }

    for t in trends.iter().filter(|x| x.is_completed) {
        let dir_swings = swings
            .iter()
            .filter(|s| {
                let sid = s.id.unwrap_or_default();
                t.swing_start_id <= sid && sid <= t.swing_end_id && s.direction == t.direction
            })
            .count();
        if dir_swings == 0 {
            violations.push(format!(
                "TREND semantic mismatch id={:?} no same-direction swings in range",
                t.id
            ));
        }
    }

    println!(
        "AUDIT summary: bars={} cbars={} swings={} trends={} completed_swings={} completed_trends={}",
        count,
        cbars.len(),
        swings.len(),
        trends.len(),
        swings.iter().filter(|x| x.is_completed).count(),
        trends.iter().filter(|x| x.is_completed).count(),
    );

    if violations.is_empty() {
        println!("AUDIT result: PASS (no semantic violations found)");
    } else {
        println!("AUDIT result: FAIL violations={}", violations.len());
        for item in violations.iter().take(30) {
            println!("- {item}");
        }
        if violations.len() > 30 {
            println!("- ... {} more", violations.len() - 30);
        }
    }

    Ok(())
}

fn parse_timeframe(raw: &str) -> Result<Timeframe, Box<dyn std::error::Error>> {
    let tf = match raw.to_ascii_lowercase().as_str() {
        "1m" => Timeframe::M1,
        "5m" => Timeframe::M5,
        "15m" => Timeframe::M15,
        "1h" | "60m" => Timeframe::H1,
        "1d" => Timeframe::D1,
        _ => return Err(format!("unsupported timeframe: {raw}").into()),
    };
    Ok(tf)
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
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

    Err(format!("invalid datetime: {value}").into())
}
