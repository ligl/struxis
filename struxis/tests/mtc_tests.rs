use chrono::Utc;
use std::sync::{Arc, Mutex};

use struxis::{EventType, MultiTimeframeContext, SBar, Timeframe};

#[test]
fn timeframe_manager_appends_sbar() {
    let mut mtc = MultiTimeframeContext::new("I2601.DCE");
    mtc.register(Timeframe::M5);

    mtc.append(
        Timeframe::M5,
        SBar {
            id: None,
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
        },
    );

    assert_eq!(mtc.count(Timeframe::M5), 1);
    assert!(mtc.get_sd(Timeframe::M5).is_some());
}

#[test]
fn mtc_emits_new_bar_event() {
    let mut mtc = MultiTimeframeContext::new("I2601.DCE");
    mtc.register(Timeframe::M5);

    let counter = Arc::new(Mutex::new(0usize));
    let counter_clone = Arc::clone(&counter);
    mtc.subscribe(
        Some(EventType::MtcNewBar),
        Arc::new(move |_tf, _evt, _payload| {
            let mut guard = counter_clone.lock().expect("lock");
            *guard += 1;
        }),
    );

    mtc.append(
        Timeframe::M5,
        SBar {
            id: None,
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
        },
    );

    let guard = counter.lock().expect("lock");
    assert_eq!(*guard, 1);
}

