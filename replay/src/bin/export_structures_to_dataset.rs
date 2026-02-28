use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;
use struxis::{DataReceiver, MarketBarInput, MultiTimeframeContext, Timeframe};

#[derive(Debug, Serialize)]
struct Payload {
    sbars: Vec<CandlePoint>,
    cbars: Vec<CBarPoint>,
    cbar_fractals: Vec<FractalMarker>,
}

#[derive(Debug, Serialize)]
struct CandlePoint {
    id: String,
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

#[derive(Debug, Serialize)]
struct CBarPoint {
    id: String,
    time: i64,
    sbar_start_id: String,
    sbar_end_id: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    fractal: i8,
}

#[derive(Debug, Serialize)]
struct FractalMarker {
    id: String,
    time: i64,
    price: f64,
    kind: String,
}

fn parse_timeframe(s: &str) -> Result<Timeframe, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "15m" => Ok(Timeframe::M15),
        "1h" => Ok(Timeframe::H1),
        "1d" => Ok(Timeframe::D1),
        "5m" => Ok(Timeframe::M5),
        "1m" => Ok(Timeframe::M1),
        _ => Err(format!("unsupported timeframe: {}", s)),
    }
}

fn fractal_label(ft: struxis::FractalType) -> i8 {
    match ft {
        struxis::FractalType::Top => 1,
        struxis::FractalType::Bottom => -1,
        struxis::FractalType::None => 0,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: cargo run -p replay --bin export_structures_to_dataset -- <csv_path> <symbol> <exchange> <timeframe> [output_path] [max_rows]");
        std::process::exit(2);
    }

    let csv_path = PathBuf::from(&args[1]);
    let symbol = args[2].clone();
    let exchange = args[3].clone();
    let timeframe = if args.len() >= 5 { parse_timeframe(&args[4])? } else { Timeframe::M15 };
    let output = if args.len() >= 6 {
        PathBuf::from(&args[5])
    } else {
        let name = format!("{}_{}_{}_structures.json", symbol, exchange, format!("{:?}", timeframe).to_lowercase());
        PathBuf::from("dataset").join(name)
    };
    let max_rows = if args.len() >= 7 { Some(args[6].parse::<usize>()?) } else { None };

    let mut receiver = DataReceiver::new(MultiTimeframeContext::new(format!("{}.{}", symbol, exchange)));
    receiver.register_timeframe(timeframe);

    // load csv
    let rows = {
        let mut v = Vec::new();
        let mut rdr = csv::Reader::from_path(&csv_path)?;
        for row in rdr.deserialize::<HashMap<String, String>>() {
            let map = row?;
            // minimal parse, rely on export_kline_structures for full fidelity
            // here we just construct MarketBarInput by reading required keys
            let datetime = map.get("datetime").or_else(|| map.get("time")).ok_or("missing datetime")?.to_string();
            let dt = if let Ok(t) = chrono::DateTime::parse_from_rfc3339(&datetime) { t.with_timezone(&Utc) } else if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&datetime, "%Y-%m-%d %H:%M:%S%.f") { chrono::DateTime::<Utc>::from_utc(naive, Utc) } else { return Err("invalid datetime in csv".into()); };
            let open = map.get("open").or_else(|| map.get("open_price")).ok_or("missing open")?.parse::<f64>()?;
            let high = map.get("high").or_else(|| map.get("high_price")).ok_or("missing high")?.parse::<f64>()?;
            let low = map.get("low").or_else(|| map.get("low_price")).ok_or("missing low")?.parse::<f64>()?;
            let close = map.get("close").or_else(|| map.get("close_price")).ok_or("missing close")?.parse::<f64>()?;
            v.push(MarketBarInput{ symbol: symbol.clone(), exchange: exchange.clone(), timeframe, datetime: dt, open_price: open, high_price: high, low_price: low, close_price: close, volume: 0.0, open_interest: 0.0, turnover: 0.0 });
        }
        v
    };

    for row in rows.into_iter().take(max_rows.unwrap_or(usize::MAX)) { receiver.ingest_bar(row); }

    let mtc = receiver.mtc();
    let count = mtc.count(timeframe);
    let sbars = mtc.get_sbar_window(timeframe, count);
    let cbars = mtc.get_cbar_window(timeframe, count.saturating_mul(2));

    // map sbar times
    let mut sbar_time: HashMap<u64, i64> = HashMap::new();
    let mut sbar_rows: Vec<(u64, i64)> = Vec::new();
    for bar in &sbars { if let Some(id) = bar.id { let ts = bar.datetime.timestamp(); sbar_time.insert(id, ts); sbar_rows.push((id, ts)); } }

    let mut candles = Vec::new();
    for bar in &sbars {
        if let Some(id) = bar.id {
            candles.push(CandlePoint { id: id.to_string(), time: bar.datetime.timestamp(), open: bar.open_price, high: bar.high_price, low: bar.low_price, close: bar.close_price });
        }
    }

    // Build one CBar entry per CBar (avoid duplicating per SBar)
    let mut cbar_candles = Vec::new();
    let mut prev_cbar_close: Option<f64> = None;
    // compute base time after last sbar to place cbars on an independent time axis
    let max_sbar_time = sbar_rows.iter().map(|(_,ts)| *ts).max().unwrap_or(0);
    let mut idx: i64 = 0;
    for cbar in &cbars {
        if let Some(mid_id) = cbar.id {
            // compute a new time for this cbar so it does not overlap with sbars
            // step by 60 seconds per cbar
            let cbar_time = max_sbar_time + 60 * (idx + 1);
            if let Some(_t) = sbar_time.get(&cbar.sbar_end_id).copied() {
                let midpoint = (cbar.high_price + cbar.low_price) / 2.0;
                let cbar_open = prev_cbar_close.unwrap_or(midpoint);
                cbar_candles.push(CBarPoint { id: mid_id.to_string(), time: cbar_time, sbar_start_id: cbar.sbar_start_id.to_string(), sbar_end_id: cbar.sbar_end_id.to_string(), open: cbar_open, high: cbar.high_price, low: cbar.low_price, close: midpoint, fractal: fractal_label(cbar.fractal_type) });
                prev_cbar_close = Some(midpoint);
                idx += 1;
            }
        }
    }

    let mut cbar_fractals = Vec::new();
    for c in &cbars {
        if let Some(t1) = sbar_time.get(&c.sbar_end_id).copied() {
            if let Some(mid_id) = c.id {
                match c.fractal_type {
                    struxis::FractalType::Top => cbar_fractals.push(FractalMarker { id: mid_id.to_string(), time: t1, price: c.high_price, kind: "Top".to_string() }),
                    struxis::FractalType::Bottom => cbar_fractals.push(FractalMarker { id: mid_id.to_string(), time: t1, price: c.low_price, kind: "Bottom".to_string() }),
                    _ => {}
                }
            }
        }
    }

    let payload = Payload { sbars: candles, cbars: cbar_candles, cbar_fractals };

    if let Some(parent) = output.parent() { fs::create_dir_all(parent)?; }
    fs::write(&output, serde_json::to_vec_pretty(&payload)?)?;

    println!("wrote structures to {}", output.display());
    Ok(())
}
