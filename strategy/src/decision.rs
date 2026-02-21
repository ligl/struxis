use struxis::constant::Direction;
use struxis::{KeyZoneBehavior, SupplyDemandResult, SupplyDemandStage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecisionAction {
    Buy,
    Sell,
    Short,
    Cover,
    Wait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSide {
    Flat,
    Long,
    Short,
}

#[derive(Debug, Clone)]
pub struct DecisionContext {
    pub higher_tf_direction: Direction,
    pub trade_tf_direction: Direction,
    pub entry_tf_direction: Direction,
    pub has_keyzone_conflict: bool,
    pub is_accept_state: bool,
    pub keyzone_behavior: Option<KeyZoneBehavior>,
    pub keyzone_strength: f64,
    pub gate_consistency: u8,
    pub gate_conflicts: u8,
    pub second_push_ready: bool,
    pub breakout_failure: bool,
    pub cooldown_active: bool,
    pub prefer_close_over_open: bool,
    pub position: PositionSide,
    pub sd: SupplyDemandResult,
}

#[derive(Debug, Clone)]
pub struct DecisionResult {
    pub action: DecisionAction,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct DecisionEngine;

impl DecisionEngine {
    pub fn evaluate(&self, ctx: &DecisionContext) -> DecisionResult {
        if ctx.cooldown_active {
            return DecisionResult {
                action: DecisionAction::Wait,
                reason: "cooldown active".to_string(),
            };
        }

        let keyzone_breakout_failure = matches!(
            ctx.keyzone_behavior,
            Some(KeyZoneBehavior::BreakoutFailure)
        );

        if ctx.breakout_failure || keyzone_breakout_failure {
            return match ctx.position {
                PositionSide::Long => DecisionResult {
                    action: DecisionAction::Sell,
                    reason: "breakout failure: close long first".to_string(),
                },
                PositionSide::Short => DecisionResult {
                    action: DecisionAction::Cover,
                    reason: "breakout failure: close short first".to_string(),
                },
                PositionSide::Flat => DecisionResult {
                    action: DecisionAction::Wait,
                    reason: "breakout failure: no open position".to_string(),
                },
            };
        }

        if ctx.has_keyzone_conflict {
            return DecisionResult {
                action: DecisionAction::Wait,
                reason: "keyzone conflict".to_string(),
            };
        }

        if ctx.sd.stage == SupplyDemandStage::Failed {
            return DecisionResult {
                action: DecisionAction::Wait,
                reason: "sd not reliable".to_string(),
            };
        }

        let aligned = ctx.higher_tf_direction == ctx.trade_tf_direction
            && ctx.trade_tf_direction == ctx.entry_tf_direction
            && ctx.higher_tf_direction != Direction::None;

        let mut open_threshold: f64 = match ctx.sd.stage {
            SupplyDemandStage::Stable => 0.35,
            SupplyDemandStage::Weakening => 0.45,
            SupplyDemandStage::Critical => 0.55,
            SupplyDemandStage::Failed => 1.0,
        };

        let keyzone_behavior = ctx.keyzone_behavior;
        if let Some(behavior) = keyzone_behavior {
            match behavior {
                KeyZoneBehavior::StrongAccept => open_threshold -= 0.10,
                KeyZoneBehavior::WeakAccept => open_threshold -= 0.05,
                KeyZoneBehavior::SecondPush => open_threshold -= 0.08,
                KeyZoneBehavior::WeakReject => open_threshold += 0.08,
                KeyZoneBehavior::StrongReject => open_threshold += 0.12,
                KeyZoneBehavior::BreakoutFailure => open_threshold += 0.15,
            }
        }
        open_threshold = open_threshold.clamp(0.2, 0.8);

        let sd_quality = (0.4 * ctx.sd.factors.dominance
            + 0.35 * ctx.sd.factors.efficiency
            + 0.25 * ctx.sd.factors.sustainability)
            .clamp(-1.0, 1.0);
        let sd_quality_pass = sd_quality >= -0.1;

        let bullish_signal = aligned
            && ctx.higher_tf_direction == Direction::Up
            && ctx.sd.score >= open_threshold
            && sd_quality_pass
            && ctx.is_accept_state;
        let bearish_signal = aligned
            && ctx.higher_tf_direction == Direction::Down
            && ctx.sd.score <= -open_threshold
            && sd_quality_pass
            && ctx.is_accept_state;

        let risk_blocked = ctx.gate_consistency <= 4
            || ctx.gate_conflicts >= 3
            || (ctx.keyzone_strength > 0.7
                && matches!(
                    keyzone_behavior,
                    Some(KeyZoneBehavior::StrongReject | KeyZoneBehavior::BreakoutFailure)
                ));
        let gate_passed = ctx.gate_consistency >= 7 && !risk_blocked;

        if ctx.prefer_close_over_open {
            if ctx.position == PositionSide::Long
                && (!bullish_signal || ctx.sd.score < 0.0 || !ctx.is_accept_state)
            {
                return DecisionResult {
                    action: DecisionAction::Sell,
                    reason: "close-long priority on signal degradation".to_string(),
                };
            }
            if ctx.position == PositionSide::Short
                && (!bearish_signal || ctx.sd.score > 0.0 || !ctx.is_accept_state)
            {
                return DecisionResult {
                    action: DecisionAction::Cover,
                    reason: "close-short priority on signal degradation".to_string(),
                };
            }
        }

        if !aligned {
            return DecisionResult {
                action: DecisionAction::Wait,
                reason: "multi timeframe conflict".to_string(),
            };
        }

        if !gate_passed {
            return DecisionResult {
                action: DecisionAction::Wait,
                reason: "execution gate not passed".to_string(),
            };
        }

        if !ctx.second_push_ready {
            return DecisionResult {
                action: DecisionAction::Wait,
                reason: "waiting second push confirmation".to_string(),
            };
        }

        if bullish_signal && ctx.position == PositionSide::Flat {
            DecisionResult {
                action: DecisionAction::Buy,
                reason: "bullish dominance confirmed".to_string(),
            }
        } else if bearish_signal && ctx.position == PositionSide::Flat {
            DecisionResult {
                action: DecisionAction::Short,
                reason: "bearish dominance confirmed".to_string(),
            }
        } else if bullish_signal && ctx.position == PositionSide::Short {
            DecisionResult {
                action: DecisionAction::Cover,
                reason: "bullish reversal: close short".to_string(),
            }
        } else if bearish_signal && ctx.position == PositionSide::Long {
            DecisionResult {
                action: DecisionAction::Sell,
                reason: "bearish reversal: close long".to_string(),
            }
        } else {
            DecisionResult {
                action: DecisionAction::Wait,
                reason: "edge too weak".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use struxis::constant::Direction;
    use struxis::{KeyZoneBehavior, SupplyDemandFactors, SupplyDemandResult, SupplyDemandStage};

    use super::{DecisionAction, DecisionContext, DecisionEngine, PositionSide};

    fn sd(score: f64) -> SupplyDemandResult {
        SupplyDemandResult {
            score,
            stage: if score.abs() >= 0.2 {
                SupplyDemandStage::Stable
            } else {
                SupplyDemandStage::Failed
            },
            factors: SupplyDemandFactors::default(),
            explanation: "test".to_string(),
        }
    }

    fn ctx() -> DecisionContext {
        DecisionContext {
            higher_tf_direction: Direction::Up,
            trade_tf_direction: Direction::Up,
            entry_tf_direction: Direction::Up,
            has_keyzone_conflict: false,
            is_accept_state: true,
            keyzone_behavior: Some(KeyZoneBehavior::StrongAccept),
            keyzone_strength: 0.65,
            gate_consistency: 8,
            gate_conflicts: 0,
            second_push_ready: true,
            breakout_failure: false,
            cooldown_active: false,
            prefer_close_over_open: true,
            position: PositionSide::Flat,
            sd: sd(0.6),
        }
    }

    #[test]
    fn opens_long_when_all_gates_pass() {
        let engine = DecisionEngine;
        let result = engine.evaluate(&ctx());
        assert_eq!(result.action, DecisionAction::Buy);
    }

    #[test]
    fn waits_when_second_push_not_ready() {
        let engine = DecisionEngine;
        let mut c = ctx();
        c.second_push_ready = false;
        let result = engine.evaluate(&c);
        assert_eq!(result.action, DecisionAction::Wait);
    }

    #[test]
    fn closes_long_on_breakout_failure() {
        let engine = DecisionEngine;
        let mut c = ctx();
        c.position = PositionSide::Long;
        c.keyzone_behavior = Some(KeyZoneBehavior::BreakoutFailure);
        let result = engine.evaluate(&c);
        assert_eq!(result.action, DecisionAction::Sell);
    }

    #[test]
    fn blocks_on_gate_conflict() {
        let engine = DecisionEngine;
        let mut c = ctx();
        c.gate_consistency = 5;
        c.gate_conflicts = 3;
        let result = engine.evaluate(&c);
        assert_eq!(result.action, DecisionAction::Wait);
    }
}
