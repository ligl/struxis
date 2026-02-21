mod decision;

use market::{Bar, SharedBar};
use struxis::{
	AnalysisEngine, Direction, KeyZoneBehavior, SupplyDemandFactors, SupplyDemandResult,
	SupplyDemandStage, Timeframe,
};

pub use decision::{DecisionAction, DecisionContext, DecisionEngine, DecisionResult, PositionSide};

pub struct Strategy {
	analysis: AnalysisEngine,
	decision_engine: DecisionEngine,
	timeframe: Timeframe,
}

impl Strategy {
	pub fn new(symbol: impl Into<String>, exchange: impl Into<String>) -> Self {
		let _ = exchange.into();
		Self {
			analysis: AnalysisEngine::new(symbol),
			decision_engine: DecisionEngine,
			timeframe: Timeframe::M5,
		}
	}

	pub fn on_bar(&mut self, mut bar: Bar) -> DecisionResult {
		bar.id = None;
		bar.timeframe = self.timeframe;
		self.analysis.append(self.timeframe, bar);
		let snapshot = self.analysis.snapshot();

		let higher_sd = snapshot.higher.sd.unwrap_or_else(default_sd);
		let trade_sd = snapshot.trade.sd.unwrap_or_else(default_sd);
		let entry_sd = snapshot.entry.sd.unwrap_or_else(default_sd);
		let entry_keyzone = snapshot.entry.keyzone_signal;

		let ctx = DecisionContext {
			higher_tf_direction: to_direction(&higher_sd),
			trade_tf_direction: to_direction(&trade_sd),
			entry_tf_direction: to_direction(&entry_sd),
			has_keyzone_conflict: matches!(
				entry_keyzone.as_ref().map(|x| x.behavior),
				Some(KeyZoneBehavior::StrongReject | KeyZoneBehavior::WeakReject)
			),
			is_accept_state: matches!(
				entry_keyzone.as_ref().map(|x| x.behavior),
				Some(
					KeyZoneBehavior::StrongAccept
						| KeyZoneBehavior::WeakAccept
						| KeyZoneBehavior::SecondPush
				)
			),
			keyzone_behavior: entry_keyzone.as_ref().map(|x| x.behavior),
			keyzone_strength: entry_keyzone.as_ref().map(|x| x.strength).unwrap_or(0.0),
			gate_consistency: 7,
			gate_conflicts: 0,
			second_push_ready: matches!(
				entry_keyzone.as_ref().map(|x| x.behavior),
				Some(
					KeyZoneBehavior::SecondPush
						| KeyZoneBehavior::StrongAccept
						| KeyZoneBehavior::WeakAccept
				)
			),
			breakout_failure: matches!(
				entry_keyzone.as_ref().map(|x| x.behavior),
				Some(KeyZoneBehavior::BreakoutFailure)
			),
			cooldown_active: false,
			prefer_close_over_open: true,
			position: PositionSide::Flat,
			sd: entry_sd,
		};

		self.decision_engine.evaluate(&ctx)
	}

	pub fn on_shared_bar(&mut self, shared_bar: SharedBar) -> DecisionResult {
		let bar = match std::sync::Arc::try_unwrap(shared_bar) {
			Ok(bar) => bar,
			Err(shared) => (*shared).clone(),
		};
		self.on_bar(bar)
	}
}

fn to_direction(x: &SupplyDemandResult) -> Direction {
	if x.score > 0.2 {
		Direction::Up
	} else if x.score < -0.2 {
		Direction::Down
	} else {
		Direction::None
	}
}

fn default_sd() -> SupplyDemandResult {
	SupplyDemandResult {
		score: 0.0,
		stage: SupplyDemandStage::Failed,
		factors: SupplyDemandFactors::default(),
		explanation: "no sd".to_string(),
	}
}
