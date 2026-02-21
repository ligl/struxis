use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::constant::Timeframe;
use crate::bar::SBar;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct SupplyDemandConfig {
    pub layer1_weight: f64,
    pub layer2_weight: f64,
    pub layer3_weight: f64,

    pub f1_weight: f64,
    pub f2_weight: f64,
    pub f3_weight: f64,
    pub f4_weight: f64,
    pub f5_weight: f64,
    pub f6_weight: f64,
    pub f7_weight: f64,
    pub f8_weight: f64,
    pub f9_weight: f64,

    pub stable_threshold: f64,
    pub weakening_threshold: f64,
    pub critical_threshold: f64,

    pub keyzone_bias_scale: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SupplyDemandConfigPatch {
    pub layer1_weight: Option<f64>,
    pub layer2_weight: Option<f64>,
    pub layer3_weight: Option<f64>,

    pub f1_weight: Option<f64>,
    pub f2_weight: Option<f64>,
    pub f3_weight: Option<f64>,
    pub f4_weight: Option<f64>,
    pub f5_weight: Option<f64>,
    pub f6_weight: Option<f64>,
    pub f7_weight: Option<f64>,
    pub f8_weight: Option<f64>,
    pub f9_weight: Option<f64>,

    pub stable_threshold: Option<f64>,
    pub weakening_threshold: Option<f64>,
    pub critical_threshold: Option<f64>,

    pub keyzone_bias_scale: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SupplyDemandProfileConfig {
    #[serde(default)]
    pub default: SupplyDemandConfigPatch,
    #[serde(default)]
    pub timeframe: HashMap<String, SupplyDemandConfigPatch>,
    #[serde(default)]
    pub symbol: HashMap<String, SupplyDemandConfigPatch>,
    #[serde(default)]
    pub symbol_timeframe: HashMap<String, SupplyDemandConfigPatch>,
}

impl Default for SupplyDemandConfig {
    fn default() -> Self {
        Self {
            layer1_weight: 0.45,
            layer2_weight: 0.30,
            layer3_weight: 0.25,

            f1_weight: 0.40,
            f2_weight: 0.40,
            f3_weight: 0.20,
            f4_weight: 0.40,
            f5_weight: 0.20,
            f6_weight: 0.40,
            f7_weight: 0.50,
            f8_weight: 0.25,
            f9_weight: 0.25,

            stable_threshold: 0.70,
            weakening_threshold: 0.45,
            critical_threshold: 0.25,

            keyzone_bias_scale: 0.35,
        }
    }
}

impl SupplyDemandConfig {
    pub fn apply_patch(mut self, patch: SupplyDemandConfigPatch) -> Self {
        if let Some(v) = patch.layer1_weight {
            self.layer1_weight = v;
        }
        if let Some(v) = patch.layer2_weight {
            self.layer2_weight = v;
        }
        if let Some(v) = patch.layer3_weight {
            self.layer3_weight = v;
        }

        if let Some(v) = patch.f1_weight {
            self.f1_weight = v;
        }
        if let Some(v) = patch.f2_weight {
            self.f2_weight = v;
        }
        if let Some(v) = patch.f3_weight {
            self.f3_weight = v;
        }
        if let Some(v) = patch.f4_weight {
            self.f4_weight = v;
        }
        if let Some(v) = patch.f5_weight {
            self.f5_weight = v;
        }
        if let Some(v) = patch.f6_weight {
            self.f6_weight = v;
        }
        if let Some(v) = patch.f7_weight {
            self.f7_weight = v;
        }
        if let Some(v) = patch.f8_weight {
            self.f8_weight = v;
        }
        if let Some(v) = patch.f9_weight {
            self.f9_weight = v;
        }

        if let Some(v) = patch.stable_threshold {
            self.stable_threshold = v;
        }
        if let Some(v) = patch.weakening_threshold {
            self.weakening_threshold = v;
        }
        if let Some(v) = patch.critical_threshold {
            self.critical_threshold = v;
        }
        if let Some(v) = patch.keyzone_bias_scale {
            self.keyzone_bias_scale = v;
        }
        self
    }

    pub fn from_yaml_str(yaml: &str) -> Result<Self, serde_yaml::Error> {
        let patch: SupplyDemandConfigPatch = serde_yaml::from_str(yaml)?;
        Ok(Self::default().apply_patch(patch))
    }

    pub fn from_yaml_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let raw = fs::read_to_string(path)?;
        let config = Self::from_yaml_str(&raw)?;
        Ok(config)
    }
}

impl SupplyDemandProfileConfig {
    pub fn from_yaml_str(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn from_yaml_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let raw = fs::read_to_string(path)?;
        let profile = Self::from_yaml_str(&raw)?;
        Ok(profile)
    }

    pub fn resolve_for(&self, symbol: &str, timeframe: Timeframe) -> SupplyDemandConfig {
        let symbol_norm = normalize_key(symbol);
        let tf_norm = timeframe.as_str().to_string();
        let symbol_tf = format!("{symbol_norm}.{tf_norm}");
        let symbol_wild = format!("{symbol_norm}.*");
        let tf_wild = format!("*.{tf_norm}");

        let mut config = SupplyDemandConfig::default().apply_patch(self.default.clone());

        if let Some(patch) = find_patch(&self.timeframe, &tf_norm) {
            config = config.apply_patch(patch.clone());
        }
        if let Some(patch) = find_patch(&self.symbol, &symbol_norm) {
            config = config.apply_patch(patch.clone());
        }
        if let Some(patch) = find_patch(&self.symbol_timeframe, &tf_wild) {
            config = config.apply_patch(patch.clone());
        }
        if let Some(patch) = find_patch(&self.symbol_timeframe, &symbol_wild) {
            config = config.apply_patch(patch.clone());
        }
        if let Some(patch) = find_patch(&self.symbol_timeframe, &symbol_tf) {
            config = config.apply_patch(patch.clone());
        }

        config
    }
}

fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn find_patch<'a>(
    map: &'a HashMap<String, SupplyDemandConfigPatch>,
    key: &str,
) -> Option<&'a SupplyDemandConfigPatch> {
    let key_norm = normalize_key(key);
    map.iter()
        .find(|(k, _)| normalize_key(k) == key_norm)
        .map(|(_, v)| v)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyDemandStage {
    Stable,
    Weakening,
    Critical,
    Failed,
}

#[derive(Debug, Clone, Default)]
pub struct SupplyDemandFactors {
    pub f1_rejection_acceptance: f64,
    pub f2_advancement_efficiency: f64,
    pub f3_momentum_consistency: f64,
    pub f4_volume_confirmation: f64,
    pub f5_oi_nature: f64,
    pub f6_vol_oi_alignment: f64,
    pub f7_swing_relative_strength: f64,
    pub f8_keyzone_reaction: f64,
    pub f9_mtf_alignment: f64,

    pub a_initiative: f64,
    pub b_direction_consistency: f64,
    pub c_pullback_role: f64,
    pub d_time_efficiency: f64,
    pub e_body_wick_efficiency: f64,
    pub f_vol_oi_cost_effectiveness: f64,
    pub g_marginal_deterioration: f64,
    pub h_key_behavior_mismatch: f64,
    pub i_opponent_response_quality: f64,

    pub dominance: f64,
    pub efficiency: f64,
    pub sustainability: f64,
    pub volatility_adjustment: f64,
}

#[derive(Debug, Clone)]
pub struct SupplyDemandResult {
    pub score: f64,
    pub stage: SupplyDemandStage,
    pub factors: SupplyDemandFactors,
    pub explanation: String,
}

#[derive(Debug, Clone, Default)]
pub struct SupplyDemand {
    config: SupplyDemandConfig,
}

impl SupplyDemand {
    pub fn evaluate_window(&self, bars: &[SBar]) -> SupplyDemandResult {
        self.evaluate_window_with_bias(bars, 0.0)
    }

    pub fn with_config(config: SupplyDemandConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SupplyDemandConfig {
        &self.config
    }

    pub fn evaluate_window_with_bias(&self, bars: &[SBar], keyzone_bias: f64) -> SupplyDemandResult {
        if bars.is_empty() {
            return SupplyDemandResult {
                score: 0.0,
                stage: SupplyDemandStage::Failed,
                factors: SupplyDemandFactors::default(),
                explanation: "empty window".to_string(),
            };
        }

        let first = &bars[0];
        let last = &bars[bars.len() - 1];

        let displacement = last.close_price - first.open_price;
        let total_range: f64 = bars.iter().map(SBar::total_range).sum();
        let price_direction = displacement.signum();
        let directional_efficiency = if total_range.abs() < f64::EPSILON {
            0.0
        } else {
            (displacement / total_range).clamp(-1.0, 1.0)
        };

        let up_count = bars
            .iter()
            .filter(|bar| bar.close_price > bar.open_price)
            .count() as f64;
        let down_count = bars
            .iter()
            .filter(|bar| bar.close_price < bar.open_price)
            .count() as f64;
        let signed_count = if up_count + down_count <= f64::EPSILON {
            0.0
        } else {
            (up_count - down_count) / (up_count + down_count)
        };

        let mut wick_sum = 0.0;
        let mut body_ratio_sum = 0.0;
        let mut direction_flip = 0u32;
        let mut prev_bar_dir: f64 = 0.0;
        for bar in bars {
            let range = bar.total_range();
            let body = bar.body();
            let wick = bar.upper_shadow().max(0.0) + bar.lower_shadow().max(0.0);
            wick_sum += wick;
            if range.abs() >= f64::EPSILON {
                body_ratio_sum += (body / range).clamp(0.0, 1.0);
            }

            let bar_dir = (bar.close_price - bar.open_price).signum();
            if prev_bar_dir.abs() > f64::EPSILON
                && bar_dir.abs() > f64::EPSILON
                && (prev_bar_dir - bar_dir).abs() > f64::EPSILON
            {
                direction_flip += 1;
            }
            if bar_dir.abs() > f64::EPSILON {
                prev_bar_dir = bar_dir;
            }
        }

        let avg_body_ratio = if bars.is_empty() {
            0.0
        } else {
            body_ratio_sum / bars.len() as f64
        };

        let volume_mean = bars.iter().map(|bar| bar.volume).sum::<f64>() / bars.len() as f64;
        let volume_last = last.volume;
        let volume_confirmation = if volume_mean.abs() < f64::EPSILON {
            0.0
        } else {
            ((volume_last / volume_mean) - 1.0).clamp(-1.0, 1.0)
        };

        let oi_delta = last.open_interest - first.open_interest;
        let oi_nature = if oi_delta.abs() < f64::EPSILON {
            0.0
        } else {
            oi_delta.signum() * price_direction
        };

        let vol_oi_alignment = (volume_confirmation * oi_nature).clamp(-1.0, 1.0);

        let swing_relative_strength = directional_efficiency;
        let keyzone_reaction = ((avg_body_ratio - 0.5).clamp(-1.0, 1.0) * price_direction
            + keyzone_bias.clamp(-1.0, 1.0) * self.config.keyzone_bias_scale)
            .clamp(-1.0, 1.0);
        let mtf_alignment = signed_count.signum() * price_direction;

        let rejection_acceptance = (avg_body_ratio - (wick_sum / (total_range + 1e-9))).clamp(-1.0, 1.0);
        let advancement_efficiency = directional_efficiency;
        let momentum_consistency = (1.0 - (direction_flip as f64 / bars.len().max(1) as f64)).clamp(0.0, 1.0)
            * price_direction;

        let a_initiative = (advancement_efficiency.abs() * volume_confirmation.abs()).clamp(0.0, 1.0)
            * price_direction;
        let b_direction_consistency = momentum_consistency;
        let c_pullback_role = (avg_body_ratio - 0.4).clamp(-1.0, 1.0) * price_direction;
        let d_time_efficiency = advancement_efficiency;
        let e_body_wick_efficiency = rejection_acceptance;
        let f_vol_oi_cost_effectiveness = vol_oi_alignment;
        let g_marginal_deterioration = (-advancement_efficiency.abs() + 0.5).clamp(-1.0, 1.0);
        let h_key_behavior_mismatch: f64 = if keyzone_reaction * price_direction < 0.0 {
            1.0
        } else {
            -1.0
        };
        let i_opponent_response_quality = (-momentum_consistency).clamp(-1.0, 1.0);

        let layer1 = (self.config.f1_weight * rejection_acceptance)
            + (self.config.f2_weight * advancement_efficiency)
            + (self.config.f3_weight * momentum_consistency);
        let layer2 = (self.config.f4_weight * volume_confirmation)
            + (self.config.f5_weight * oi_nature)
            + (self.config.f6_weight * vol_oi_alignment);
        let layer3 = (self.config.f7_weight * swing_relative_strength)
            + (self.config.f8_weight * keyzone_reaction)
            + (self.config.f9_weight * mtf_alignment);

        let score = (self.config.layer1_weight * layer1
            + self.config.layer2_weight * layer2
            + self.config.layer3_weight * layer3)
            .clamp(-1.0, 1.0);

        let dominance = ((a_initiative + b_direction_consistency + c_pullback_role) / 3.0).clamp(-1.0, 1.0);
        let efficiency = ((d_time_efficiency + e_body_wick_efficiency + f_vol_oi_cost_effectiveness) / 3.0)
            .clamp(-1.0, 1.0);
        let sustainability = (1.0
            - ((g_marginal_deterioration.max(0.0)
                + h_key_behavior_mismatch.max(0.0)
                + i_opponent_response_quality.max(0.0))
                / 3.0))
            .clamp(0.0, 1.0);
        let volatility_adjustment = (1.0 - (total_range / (bars.len() as f64 + 1.0)).tanh()).clamp(0.0, 1.0);

        let stage = if score.abs() >= self.config.stable_threshold {
            SupplyDemandStage::Stable
        } else if score.abs() >= self.config.weakening_threshold {
            SupplyDemandStage::Weakening
        } else if score.abs() >= self.config.critical_threshold {
            SupplyDemandStage::Critical
        } else {
            SupplyDemandStage::Failed
        };

        SupplyDemandResult {
            score,
            stage,
            factors: SupplyDemandFactors {
                f1_rejection_acceptance: rejection_acceptance,
                f2_advancement_efficiency: advancement_efficiency,
                f3_momentum_consistency: momentum_consistency,
                f4_volume_confirmation: volume_confirmation,
                f5_oi_nature: oi_nature,
                f6_vol_oi_alignment: vol_oi_alignment,
                f7_swing_relative_strength: swing_relative_strength,
                f8_keyzone_reaction: keyzone_reaction,
                f9_mtf_alignment: mtf_alignment,

                a_initiative,
                b_direction_consistency,
                c_pullback_role,
                d_time_efficiency,
                e_body_wick_efficiency,
                f_vol_oi_cost_effectiveness,
                g_marginal_deterioration,
                h_key_behavior_mismatch,
                i_opponent_response_quality,

                dominance,
                efficiency,
                sustainability,
                volatility_adjustment,
            },
            explanation: "sd scored by 3-layer 9-factor model with A-I atoms".to_string(),
        }
    }
}

