use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;

#[derive(Debug, Deserialize)]
struct CBarCandle {
    id: u64,
    time: u64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    fractal: String,
}

fn is_inclusive(a: &CBarCandle, b: &CBarCandle) -> bool {
    (a.high >= b.high && a.low <= b.low) || (a.high <= b.high && a.low >= b.low)
}

fn main() {
    let file = File::open("kline-structures-data-15m.json").expect("open json");
    let reader = BufReader::new(file);
    let data: serde_json::Value = serde_json::from_reader(reader).expect("parse json");
    let cbar_candles: Vec<CBarCandle> = serde_json::from_value(data["cbar_candles"].clone()).expect("parse cbar_candles");

    let mut found = false;
    for pair in cbar_candles.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        if is_inclusive(a, b) {
            found = true;
            println!(
                "包含关系: #{} ({}-{}) [{}] 和 #{} ({}-{}) [{}]",
                a.id, a.high, a.low, a.time, b.id, b.high, b.low, b.time
            );
        }
    }
    if !found {
        println!("未发现包含关系，归约正确");
    }
}