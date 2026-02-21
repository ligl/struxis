use crate::keyzone::KeyZoneSignal;
use crate::mtc::MultiTimeframeContext;
use crate::sd::SupplyDemandResult;
use crate::swing::Swing;
use crate::trend::Trend;
use crate::constant::Timeframe;
use crate::bar::{CBar, SBar};

#[derive(Debug, Clone)]
pub struct TimeframeAnalysis {
    pub timeframe: Timeframe,
    pub latest_cbar: Option<CBar>,
    pub latest_swing: Option<Swing>,
    pub latest_trend: Option<Trend>,
    pub keyzone_signal: Option<KeyZoneSignal>,
    pub sd: Option<SupplyDemandResult>,
}

#[derive(Debug, Clone)]
pub struct AnalysisSnapshot {
    pub symbol: String,
    pub higher: TimeframeAnalysis,
    pub trade: TimeframeAnalysis,
    pub entry: TimeframeAnalysis,
}

pub struct AnalysisEngine {
    mtc: MultiTimeframeContext,
    higher_tf: Timeframe,
    trade_tf: Timeframe,
    entry_tf: Timeframe,
}

impl AnalysisEngine {
    pub fn new(symbol: impl Into<String>) -> Self {
        let symbol = symbol.into();
        let mut mtc = MultiTimeframeContext::new(symbol.clone());
        mtc.register(Timeframe::M5);
        mtc.register(Timeframe::M15);
        mtc.register(Timeframe::H1);

        Self {
            mtc,
            higher_tf: Timeframe::H1,
            trade_tf: Timeframe::M15,
            entry_tf: Timeframe::M5,
        }
    }

    pub fn with_timeframes(
        symbol: impl Into<String>,
        higher_tf: Timeframe,
        trade_tf: Timeframe,
        entry_tf: Timeframe,
    ) -> Self {
        let symbol = symbol.into();
        let mut mtc = MultiTimeframeContext::new(symbol);
        mtc.register(entry_tf);
        mtc.register(trade_tf);
        mtc.register(higher_tf);

        Self {
            mtc,
            higher_tf,
            trade_tf,
            entry_tf,
        }
    }

    pub fn append(&mut self, timeframe: Timeframe, bar: SBar) {
        self.mtc.append(timeframe, bar);
    }

    pub fn snapshot(&self) -> AnalysisSnapshot {
        AnalysisSnapshot {
            symbol: self.mtc.symbol().to_string(),
            higher: self.timeframe_analysis(self.higher_tf),
            trade: self.timeframe_analysis(self.trade_tf),
            entry: self.timeframe_analysis(self.entry_tf),
        }
    }

    pub fn mtc(&self) -> &MultiTimeframeContext {
        &self.mtc
    }

    pub fn mtc_mut(&mut self) -> &mut MultiTimeframeContext {
        &mut self.mtc
    }

    fn timeframe_analysis(&self, timeframe: Timeframe) -> TimeframeAnalysis {
        TimeframeAnalysis {
            timeframe,
            latest_cbar: self.mtc.get_cbar_window(timeframe, 1).into_iter().next(),
            latest_swing: self.mtc.get_swing_window(timeframe, 1).into_iter().next(),
            latest_trend: self.mtc.get_trend_window(timeframe, 1).into_iter().next(),
            keyzone_signal: self.mtc.get_keyzone_signal(timeframe).cloned(),
            sd: self.mtc.get_sd(timeframe).cloned(),
        }
    }
}
