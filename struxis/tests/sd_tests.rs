use chrono::Utc;

use struxis::{
    SBar, SupplyDemand, SupplyDemandConfig, SupplyDemandProfileConfig, SupplyDemandStage,
    Timeframe,
};

fn bar(
    id: u64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    oi: f64,
) -> SBar {
    SBar {
        id: Some(id),
        symbol: "I2601".to_string(),
        exchange: "DCE".to_string(),
        timeframe: Timeframe::M5,
        datetime: Utc::now(),
        open_price: open,
        high_price: high,
        low_price: low,
        close_price: close,
        volume,
        open_interest: oi,
        turnover: volume * close,
    }
}

#[test]
fn sd_returns_failed_on_empty_window() {
    let model = SupplyDemand::default();
    let result = model.evaluate_window(&[]);
    assert_eq!(result.stage, SupplyDemandStage::Failed);
    assert_eq!(result.score, 0.0);
}

#[test]
fn sd_score_positive_in_bullish_window() {
    let model = SupplyDemand::default();
    let bars = vec![
        bar(1, 100.0, 102.0, 99.0, 101.8, 10.0, 1000.0),
        bar(2, 101.8, 103.0, 101.0, 102.7, 12.0, 1015.0),
        bar(3, 102.7, 104.0, 102.2, 103.8, 14.0, 1030.0),
        bar(4, 103.8, 105.0, 103.2, 104.7, 16.0, 1045.0),
    ];

    let result = model.evaluate_window(&bars);
    assert!(result.score > 0.0);
    assert_ne!(result.stage, SupplyDemandStage::Failed);
}

#[test]
fn sd_score_negative_in_bearish_window() {
    let model = SupplyDemand::default();
    let bars = vec![
        bar(1, 104.0, 104.5, 102.5, 103.0, 10.0, 1000.0),
        bar(2, 103.0, 103.3, 101.8, 102.0, 11.0, 1008.0),
        bar(3, 102.0, 102.2, 100.7, 101.0, 12.0, 1016.0),
        bar(4, 101.0, 101.2, 99.5, 100.0, 13.0, 1024.0),
    ];

    let result = model.evaluate_window(&bars);
    assert!(result.score < 0.0);
}

#[test]
fn sd_config_can_shift_stage_threshold() {
    let config = SupplyDemandConfig {
        stable_threshold: 0.5,
        ..SupplyDemandConfig::default()
    };
    let model = SupplyDemand::with_config(config);
    let bars = vec![
        bar(1, 100.0, 102.0, 99.0, 101.8, 10.0, 1000.0),
        bar(2, 101.8, 103.0, 101.0, 102.7, 12.0, 1015.0),
        bar(3, 102.7, 104.0, 102.2, 103.8, 14.0, 1030.0),
        bar(4, 103.8, 105.0, 103.2, 104.7, 16.0, 1045.0),
    ];
    let result = model.evaluate_window(&bars);
    assert_ne!(result.stage, SupplyDemandStage::Failed);
}

#[test]
fn sd_config_from_yaml_applies_partial_override() {
    let yaml = r#"
stable_threshold: 0.6
f8_weight: 0.4
keyzone_bias_scale: 0.5
"#;
    let config = SupplyDemandConfig::from_yaml_str(yaml).expect("yaml parse should succeed");
    assert_eq!(config.stable_threshold, 0.6);
    assert_eq!(config.f8_weight, 0.4);
    assert_eq!(config.keyzone_bias_scale, 0.5);
    assert_eq!(config.layer1_weight, SupplyDemandConfig::default().layer1_weight);
}

#[test]
fn sd_profile_resolve_supports_symbol_timeframe_override() {
    let yaml = r#"
default:
    stable_threshold: 0.7
timeframe:
    5m:
        f8_weight: 0.30
symbol:
    I2601:
        keyzone_bias_scale: 0.45
symbol_timeframe:
    "*.5m":
        layer3_weight: 0.3
    "i2601.*":
        f1_weight: 0.35
    "I2601.5m":
        stable_threshold: 0.55
"#;
    let profile = SupplyDemandProfileConfig::from_yaml_str(yaml).expect("yaml should parse");
    let config = profile.resolve_for("I2601", Timeframe::M5);
    assert_eq!(config.stable_threshold, 0.55);
    assert_eq!(config.f8_weight, 0.30);
    assert_eq!(config.keyzone_bias_scale, 0.45);
    assert_eq!(config.f1_weight, 0.35);
    assert_eq!(config.layer3_weight, 0.3);
}
