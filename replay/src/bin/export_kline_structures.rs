use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use struxis::{
    DataReceiver, Direction, FractalType, MarketBarInput, MultiTimeframeContext, SwingState,
    Timeframe,
};

#[derive(Debug, Serialize)]
struct ExportPayload {
    symbol: String,
    exchange: String,
    timeframe: String,
    candles: Vec<CandlePoint>,
    cbar_candles: Vec<CBarCandlePoint>,
    cbar_fractals: Vec<FractalMarker>,
    swing_segments_completed: Vec<AnchoredSegment>,
    swing_segments_pending: Vec<AnchoredSegment>,
    swing_segments_forming: Vec<AnchoredSegment>,
    trend_segments_completed: Vec<AnchoredSegment>,
    keyzones: Vec<KeyZoneRange>,
}

#[derive(Debug, Serialize)]
struct CandlePoint {
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

#[derive(Debug, Serialize)]
struct CBarCandlePoint {
    id: u64,
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    fractal: String,
}

#[derive(Debug, Serialize)]
struct AnchoredSegment {
    id: u64,
    direction: String,
    start_kind: String,
    end_kind: String,
    t0: i64,
    t1: i64,
    v0: f64,
    v1: f64,
}

#[derive(Debug, Serialize)]
struct KeyZoneRange {
    t0: i64,
    t1: i64,
    upper: f64,
    lower: f64,
    origin: String,
}

#[derive(Debug, Serialize)]
struct FractalMarker {
    id: u64,
    time: i64,
    price: f64,
    kind: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "usage: cargo run -p replay --bin export_kline_structures -- <csv_path> <symbol> <exchange> <timeframe:15m|1h|1d|5m|1m> [output_json] [max_rows]"
        );
        std::process::exit(2);
    }

    let csv_path = PathBuf::from(&args[1]);
    let symbol = args[2].clone();
    let exchange = args[3].clone();
    let timeframe = parse_timeframe(&args[4])?;
    let output = if args.len() >= 6 {
        PathBuf::from(&args[5])
    } else {
        PathBuf::from(format!(
            "replay/web/kline-structures-data-{}.json",
            normalize_tf_label(&args[4])
        ))
    };
    let max_rows = if args.len() >= 7 {
        Some(args[6].parse::<usize>()?)
    } else {
        None
    };

    let mut receiver = DataReceiver::new(MultiTimeframeContext::new(format!("{}.{}", symbol, exchange)));
    receiver.register_timeframe(timeframe);
    let rows = load_market_bar_inputs(&csv_path, &symbol, &exchange, timeframe)?;
    let mut ingested = 0usize;
    for row in rows.into_iter().take(max_rows.unwrap_or(usize::MAX)) {
        receiver.ingest_bar(row);
        ingested += 1;
    }

    let mtc = receiver.mtc();
    let count = mtc.count(timeframe);
    let sbars = mtc.get_sbar_window(timeframe, count);
    let cbars = mtc.get_cbar_window(timeframe, count.saturating_mul(2));
    let swings = mtc.get_swing_window(timeframe, count.saturating_mul(2));
    let trends = mtc.get_trend_window(timeframe, count.saturating_mul(2));
    let keyzones = mtc.get_keyzone_window(timeframe, 2000);

    let mut sbar_time = HashMap::<u64, i64>::new();
    let mut sbar_rows = Vec::<(u64, i64)>::new();
    let candles = sbars
        .iter()
        .filter_map(|bar| {
            let id = bar.id?;
            let time = bar.datetime.timestamp();
            sbar_time.insert(id, time);
            sbar_rows.push((id, time));
            Some(CandlePoint {
                time,
                open: bar.open_price,
                high: bar.high_price,
                low: bar.low_price,
                close: bar.close_price,
            })
        })
        .collect::<Vec<_>>();

    let cbar_by_id: HashMap<u64, _> = cbars
        .iter()
        .filter_map(|cbar| cbar.id.map(|id| (id, cbar)))
        .collect();

    let mut cbar_candles = Vec::new();
    let mut cbar_fractals = Vec::new();
    let mut cbar_by_sbar_id = HashMap::<u64, &struxis::CBar>::new();
    for cbar in &cbars {
        for sid in cbar.sbar_start_id..=cbar.sbar_end_id {
            cbar_by_sbar_id.insert(sid, cbar);
        }
    }

    let mut prev_cbar_close: Option<f64> = None;
    for (sbar_id, sbar_ts) in &sbar_rows {
        let Some(cbar) = cbar_by_sbar_id.get(sbar_id).copied() else {
            continue;
        };
        let Some(mid_id) = cbar.id else {
            continue;
        };

        let midpoint = (cbar.high_price + cbar.low_price) / 2.0;
        let cbar_open = prev_cbar_close.unwrap_or(midpoint);
        cbar_candles.push(CBarCandlePoint {
            id: mid_id,
            time: *sbar_ts,
            open: cbar_open,
            high: cbar.high_price,
            low: cbar.low_price,
            close: midpoint,
            fractal: fractal_type_label(cbar.fractal_type).to_string(),
        });
        prev_cbar_close = Some(midpoint);
    }

    for cbar in &cbars {
        let Some(t1) = sbar_time.get(&cbar.sbar_end_id).copied() else {
            continue;
        };
        let Some(mid_id) = cbar.id else {
            continue;
        };

        match cbar.fractal_type {
            FractalType::Top => cbar_fractals.push(FractalMarker {
                id: mid_id,
                time: t1,
                price: cbar.high_price,
                kind: "Top".to_string(),
            }),
            FractalType::Bottom => cbar_fractals.push(FractalMarker {
                id: mid_id,
                time: t1,
                price: cbar.low_price,
                kind: "Bottom".to_string(),
            }),
            FractalType::None => {}
        }
    }

    let mut swing_segments_completed = Vec::new();
    let mut swing_segments_pending = Vec::new();
    let mut swing_segments_forming = Vec::new();
    for swing in &swings {
        let Some(start_cbar) = cbar_by_id.get(&swing.cbar_start_id) else {
            continue;
        };
        let Some(end_cbar) = cbar_by_id.get(&swing.cbar_end_id) else {
            continue;
        };
        let Some(id) = swing.id else {
            continue;
        };
        let Some(t0) = sbar_time.get(&start_cbar.sbar_end_id).copied() else {
            continue;
        };
        let Some(t1) = sbar_time.get(&end_cbar.sbar_end_id).copied() else {
            continue;
        };

        let seg = AnchoredSegment {
            id,
            direction: direction_label(swing.direction).to_string(),
            start_kind: fractal_type_label(start_cbar.fractal_type).to_string(),
            end_kind: fractal_type_label(end_cbar.fractal_type).to_string(),
            t0,
            t1,
            v0: anchor_price(start_cbar, swing.direction, true),
            v1: anchor_price(end_cbar, swing.direction, false),
        };

        match swing.state {
            SwingState::Confirmed => swing_segments_completed.push(seg),
            SwingState::PendingReverse => swing_segments_pending.push(seg),
            SwingState::Forming => swing_segments_forming.push(seg),
        }
    }

    let swing_by_id: HashMap<u64, _> = swings
        .iter()
        .filter_map(|swing| swing.id.map(|id| (id, swing)))
        .collect();

    let trend_segments_completed = trends
        .iter()
        .filter_map(|trend| {
            if !trend.is_completed {
                return None;
            }

            let trend_id = trend.id?;
            let start_swing = swing_by_id.get(&trend.swing_start_id)?;
            let end_swing = swing_by_id.get(&trend.swing_end_id)?;
            let start_cbar = cbar_by_id.get(&start_swing.cbar_start_id)?;
            let end_cbar = cbar_by_id.get(&end_swing.cbar_end_id)?;

            let t0 = sbar_time.get(&start_cbar.sbar_end_id).copied()?;
            let t1 = sbar_time.get(&end_cbar.sbar_end_id).copied()?;
            let v0 = anchor_price(start_cbar, trend.direction, true);
            let v1 = anchor_price(end_cbar, trend.direction, false);

            Some(AnchoredSegment {
                id: trend_id,
                direction: direction_label(trend.direction).to_string(),
                start_kind: fractal_type_label(start_cbar.fractal_type).to_string(),
                end_kind: fractal_type_label(end_cbar.fractal_type).to_string(),
                t0,
                t1,
                v0,
                v1,
            })
        })
        .collect::<Vec<_>>();

    let keyzones = keyzones
        .iter()
        .filter_map(|zone| {
            let t0 = sbar_time.get(&zone.sbar_start_id).copied()?;
            let t1 = sbar_time.get(&zone.sbar_end_id).copied()?;
            Some(KeyZoneRange {
                t0,
                t1,
                upper: zone.upper,
                lower: zone.lower,
                origin: format!("{:?}", zone.origin_type),
            })
        })
        .collect::<Vec<_>>();

    let payload = ExportPayload {
        symbol,
        exchange,
        timeframe: args[4].clone(),
        candles,
        cbar_candles,
        cbar_fractals,
        swing_segments_completed,
        swing_segments_pending,
        swing_segments_forming,
        trend_segments_completed,
        keyzones,
    };

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, serde_json::to_vec_pretty(&payload)?)?;

    println!(
        "exported {} bars to {} (ingested={}, cbar={}, swing={}, trend={}, keyzone={})",
        payload.candles.len(),
        output.display(),
        ingested,
        payload.cbar_candles.len(),
        payload.swing_segments_completed.len(),
        payload.trend_segments_completed.len(),
        payload.keyzones.len(),
    );

    Ok(())
}

fn parse_timeframe(raw: &str) -> Result<Timeframe, Box<dyn std::error::Error>> {
    let tf = match raw.to_ascii_lowercase().as_str() {
        "1m" => Timeframe::M1,
        "5m" => Timeframe::M5,
        "15m" => Timeframe::M15,
        "1h" | "60m" => Timeframe::H1,
        "1d" => Timeframe::D1,
        _ => {
            return Err(format!("unsupported timeframe: {}", raw).into());
        }
    };
    Ok(tf)
}

fn normalize_tf_label(raw: &str) -> &'static str {
    match raw.to_ascii_lowercase().as_str() {
        "1m" => "1m",
        "5m" => "5m",
        "15m" => "15m",
        "1h" | "60m" => "1h",
        "1d" => "1d",
        _ => "15m",
    }
}

fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "Up",
        Direction::Down => "Down",
        Direction::Range => "Range",
        Direction::None => "None",
    }
}

fn fractal_type_label(fractal_type: FractalType) -> &'static str {
    match fractal_type {
        FractalType::Top => "Top",
        FractalType::Bottom => "Bottom",
        FractalType::None => "None",
    }
}

fn anchor_price(cbar: &struxis::CBar, direction: Direction, is_start: bool) -> f64 {
    match cbar.fractal_type {
        FractalType::Top => cbar.high_price,
        FractalType::Bottom => cbar.low_price,
        FractalType::None => match (direction, is_start) {
            (Direction::Up, true) => cbar.low_price,
            (Direction::Up, false) => cbar.high_price,
            (Direction::Down, true) => cbar.high_price,
            (Direction::Down, false) => cbar.low_price,
            _ => (cbar.high_price + cbar.low_price) / 2.0,
        },
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

fn load_market_bar_inputs(
    file_path: &PathBuf,
    symbol: &str,
    exchange: &str,
    timeframe: Timeframe,
) -> Result<Vec<MarketBarInput>, Box<dyn std::error::Error>> {
    let mut reader = csv::Reader::from_path(file_path)?;
    let mut out = Vec::new();

    for row in reader.deserialize::<CsvBarRow>() {
        let row = row?;
        let datetime = parse_datetime(&row.datetime)?;
        out.push(MarketBarInput {
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            timeframe,
            datetime,
            open_price: row.open_price,
            high_price: row.high_price,
            low_price: row.low_price,
            close_price: row.close_price,
            volume: row.volume,
            open_interest: row.open_interest,
            turnover: row.turnover,
        });
    }

    Ok(out)
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

    Err(format!("invalid datetime: {}", value).into())
}
