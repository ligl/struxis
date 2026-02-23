use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use broker::{BrokerError, ExchangeAdapter};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use market::{BrokerBar, Feed};
use struxis::{DataReceiver, MarketBarInput, MultiTimeframeContext, Timeframe};
use tokio::sync::broadcast::error::TryRecvError;

struct CsvReplayAdapter {
    bars: VecDeque<BrokerBar>,
    connected: bool,
    exchange: String,
}

impl CsvReplayAdapter {
    fn from_files(
        files: &[(PathBuf, Timeframe)],
        symbol: &str,
        exchange: &str,
        max_rows_per_file: usize,
    ) -> Result<Self, BrokerError> {
        let mut all = Vec::<BrokerBar>::new();
        for (file, timeframe) in files {
            let mut bars = load_csv_bars(file, symbol, exchange, *timeframe, max_rows_per_file)?;
            all.append(&mut bars);
        }

        all.sort_by_key(|bar| (bar.datetime, timeframe_minutes(bar.timeframe)));

        Ok(Self {
            bars: all.into(),
            connected: false,
            exchange: exchange.to_string(),
        })
    }
}

impl ExchangeAdapter for CsvReplayAdapter {
    fn venue(&self) -> &str {
        &self.exchange
    }

    fn connect(&mut self) -> Result<(), BrokerError> {
        self.connected = true;
        Ok(())
    }

    fn poll_bar(&mut self) -> Result<Option<BrokerBar>, BrokerError> {
        if !self.connected {
            return Err(BrokerError::NotConnected);
        }
        Ok(self.bars.pop_front())
    }
}

#[test]
fn broker_feed_receiver_pipeline_validates_i888_dataset() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dataset_dir = root.join("..").join("dataset");

    let files = vec![
        (dataset_dir.join("I8888.XDCE_15m.csv"), Timeframe::M15),
        (dataset_dir.join("I8888.XDCE_60m.csv"), Timeframe::H1),
        (dataset_dir.join("I8888.XDCE_1d.csv"), Timeframe::D1),
    ];

    let feed = Feed::new("I8888", "XDCE");
    let mut rx_15m = feed.subscribe("I8888", timeframe_secs(Timeframe::M15));
    let mut rx_1h = feed.subscribe("I8888", timeframe_secs(Timeframe::H1));
    let mut rx_1d = feed.subscribe("I8888", timeframe_secs(Timeframe::D1));

    let mut receiver = DataReceiver::new(MultiTimeframeContext::new("I8888.XDCE"));
    receiver.register_timeframe(Timeframe::M15);
    receiver.register_timeframe(Timeframe::H1);
    receiver.register_timeframe(Timeframe::D1);

    let mut adapter = CsvReplayAdapter::from_files(&files, "I8888", "XDCE", 300)
        .expect("csv replay adapter should load dataset files");
    adapter.connect().expect("adapter should connect");

    let mut sent: HashMap<Timeframe, usize> = HashMap::new();
    let mut received: HashMap<Timeframe, usize> = HashMap::new();

    loop {
        let Some(bar) = adapter.poll_bar().expect("poll should not fail") else {
            break;
        };

        let tf = bar.timeframe;
        *sent.entry(tf).or_insert(0) += 1;
        let _ = feed.ingest_broker_bar(bar, timeframe_secs(tf));

        drain_channel(&mut rx_15m, &mut receiver, &mut received);
        drain_channel(&mut rx_1h, &mut receiver, &mut received);
        drain_channel(&mut rx_1d, &mut receiver, &mut received);
    }

    drain_channel(&mut rx_15m, &mut receiver, &mut received);
    drain_channel(&mut rx_1h, &mut receiver, &mut received);
    drain_channel(&mut rx_1d, &mut receiver, &mut received);

    let mut total_swing_count = 0usize;
    let mut total_trend_count = 0usize;

    for tf in [Timeframe::M15, Timeframe::H1, Timeframe::D1] {
        let sent_count = *sent.get(&tf).unwrap_or(&0);
        let recv_count = *received.get(&tf).unwrap_or(&0);
        assert!(sent_count > 0, "sent count should be > 0 for {:?}", tf);
        assert_eq!(recv_count, sent_count, "recv count should match sent for {:?}", tf);
        assert_eq!(receiver.mtc().count(tf), recv_count, "MTC count should match recv for {:?}", tf);

        let cbars = receiver.mtc().get_cbar_window(tf, 20_000);
        assert!(!cbars.is_empty(), "cbars should not be empty for {:?}", tf);
        for cbar in &cbars {
            assert!(cbar.high_price >= cbar.low_price, "cbar high/low invalid for {:?}", tf);
            assert!(cbar.sbar_end_id >= cbar.sbar_start_id, "cbar span invalid for {:?}", tf);
        }

        let swings = receiver.mtc().get_swing_window(tf, 20_000);
        total_swing_count += swings.len();
        for swing in &swings {
            assert!(swing.high_price >= swing.low_price, "swing high/low invalid for {:?}", tf);
            assert!(swing.sbar_end_id >= swing.sbar_start_id, "swing span invalid for {:?}", tf);
        }

        let trends = receiver.mtc().get_trend_window(tf, 20_000);
        total_trend_count += trends.len();
        for trend in &trends {
            assert!(trend.high_price >= trend.low_price, "trend high/low invalid for {:?}", tf);
            assert!(trend.sbar_end_id >= trend.sbar_start_id, "trend span invalid for {:?}", tf);
        }
    }

    assert!(total_swing_count > 0, "at least one swing should be produced");
    assert!(total_trend_count > 0, "at least one trend should be produced");
}

fn drain_channel(
    rx: &mut tokio::sync::broadcast::Receiver<std::sync::Arc<BrokerBar>>,
    receiver: &mut DataReceiver,
    received: &mut HashMap<Timeframe, usize>,
) {
    loop {
        match rx.try_recv() {
            Ok(shared) => {
                let bar = shared.as_ref();
                receiver.ingest_bar(MarketBarInput {
                    symbol: bar.symbol.clone(),
                    exchange: bar.exchange.clone(),
                    timeframe: bar.timeframe,
                    datetime: bar.datetime,
                    open_price: bar.open_price,
                    high_price: bar.high_price,
                    low_price: bar.low_price,
                    close_price: bar.close_price,
                    volume: bar.volume,
                    open_interest: bar.open_interest,
                    turnover: bar.turnover,
                });
                *received.entry(bar.timeframe).or_insert(0) += 1;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }
}

fn load_csv_bars(
    file_path: &Path,
    symbol: &str,
    exchange: &str,
    timeframe: Timeframe,
    max_rows: usize,
) -> Result<Vec<BrokerBar>, BrokerError> {
    let file = File::open(file_path).map_err(|e| {
        BrokerError::AdapterError(format!("failed to open {}: {}", file_path.display(), e))
    })?;
    let reader = BufReader::new(file);

    let mut bars = Vec::new();
    for (line_no, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| {
            BrokerError::AdapterError(format!("failed to read {} line {}: {}", file_path.display(), line_no + 1, e))
        })?;

        if line_no == 0 {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }

        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 8 {
            return Err(BrokerError::AdapterError(format!(
                "invalid csv row in {} line {}",
                file_path.display(),
                line_no + 1
            )));
        }

        let datetime = parse_datetime(cols[0])?;
        let open = parse_f64(cols[1], file_path, line_no + 1, "open")?;
        let close = parse_f64(cols[2], file_path, line_no + 1, "close")?;
        let high = parse_f64(cols[3], file_path, line_no + 1, "high")?;
        let low = parse_f64(cols[4], file_path, line_no + 1, "low")?;
        let volume = parse_f64(cols[5], file_path, line_no + 1, "volume")?;
        let turnover = parse_f64(cols[6], file_path, line_no + 1, "money")?;
        let open_interest = parse_f64(cols[7], file_path, line_no + 1, "open_interest")?;

        bars.push(BrokerBar {
            id: None,
            symbol: symbol.to_string(),
            exchange: exchange.to_string(),
            timeframe,
            datetime,
            open_price: open,
            high_price: high,
            low_price: low,
            close_price: close,
            volume,
            open_interest,
            turnover,
        });

        if bars.len() >= max_rows {
            break;
        }
    }

    Ok(bars)
}

fn parse_datetime(raw: &str) -> Result<DateTime<Utc>, BrokerError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(dt) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }

    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let dt = d
            .and_hms_opt(0, 0, 0)
            .expect("valid date should build midnight datetime");
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }

    Err(BrokerError::AdapterError(format!(
        "invalid datetime value: {}",
        raw
    )))
}

fn parse_f64(raw: &str, file_path: &Path, line_no: usize, column: &str) -> Result<f64, BrokerError> {
    raw.parse::<f64>().map_err(|e| {
        BrokerError::AdapterError(format!(
            "invalid {} in {} line {}: {}",
            column,
            file_path.display(),
            line_no,
            e
        ))
    })
}

fn timeframe_secs(tf: Timeframe) -> u64 {
    match tf {
        Timeframe::M1 => 60,
        Timeframe::M5 => 5 * 60,
        Timeframe::M15 => 15 * 60,
        Timeframe::H1 => 60 * 60,
        Timeframe::D1 => 24 * 60 * 60,
    }
}

fn timeframe_minutes(tf: Timeframe) -> u64 {
    match tf {
        Timeframe::M1 => 1,
        Timeframe::M5 => 5,
        Timeframe::M15 => 15,
        Timeframe::H1 => 60,
        Timeframe::D1 => 1440,
    }
}
