use chrono::{Timelike, Utc};

use struxis::{DataReceiver, MarketBarInput, MultiTimeframeContext, Timeframe};

#[test]
fn receiver_to_sd_factors_pipeline() {
    let mut receiver = DataReceiver::new(MultiTimeframeContext::new("I2601.DCE"));
    receiver.register_timeframe(Timeframe::M5);

    receiver.ingest_batch(sample_bars(80));

    let sd = receiver
        .mtc()
        .get_sd(Timeframe::M5)
        .expect("sd should be produced after ingest");

    println!(
        "score={:.4} stage={:?} dominance={:.4} efficiency={:.4} sustainability={:.4} vol_adj={:.4}",
        sd.score,
        sd.stage,
        sd.factors.dominance,
        sd.factors.efficiency,
        sd.factors.sustainability,
        sd.factors.volatility_adjustment
    );
    println!(
        "f1={:.4} f2={:.4} f3={:.4} f4={:.4} f5={:.4} f6={:.4} f7={:.4} f8={:.4} f9={:.4}",
        sd.factors.f1_rejection_acceptance,
        sd.factors.f2_advancement_efficiency,
        sd.factors.f3_momentum_consistency,
        sd.factors.f4_volume_confirmation,
        sd.factors.f5_oi_nature,
        sd.factors.f6_vol_oi_alignment,
        sd.factors.f7_swing_relative_strength,
        sd.factors.f8_keyzone_reaction,
        sd.factors.f9_mtf_alignment
    );

    assert!(sd.score.is_finite());
    assert!(sd.factors.dominance.is_finite());
    assert!(sd.factors.efficiency.is_finite());
    assert!(sd.factors.sustainability.is_finite());

    let has_signal = receiver.mtc().get_keyzone_signal(Timeframe::M5).is_some();
    println!("keyzone_signal_present={}", has_signal);
}

fn sample_bars(count: usize) -> Vec<MarketBarInput> {
    let mut bars = Vec::with_capacity(count);
    let base_dt = Utc::now()
        .with_second(0)
        .and_then(|x| x.with_nanosecond(0))
        .expect("valid dt");

    let mut price = 100.0;
    for i in 0..count {
        let phase = (i % 10) as f64;
        let wave = if i % 2 == 0 {
            0.6 + phase * 0.03
        } else {
            -0.5 + phase * 0.02
        };
        let open = price;
        let close = (open + wave).max(1.0);
        let high = open.max(close) + 0.4 + (phase * 0.01);
        let low = open.min(close) - 0.35 - (phase * 0.01);
        let volume = 100.0 + (i as f64 * 3.0);
        let open_interest = 1000.0 + (i as f64 * 5.0);
        price = close;

        bars.push(MarketBarInput {
            symbol: "I2601".to_string(),
            exchange: "DCE".to_string(),
            timeframe: Timeframe::M5,
            datetime: base_dt + chrono::Duration::minutes((i as i64) * 5),
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
