//! 单周期管理器。
//!
//! 职责：
//! - 在单 timeframe 内串联 `SBar -> CBar -> Swing -> Trend -> KeyZone -> SD`；
//! - 维护每个 timeframe 的状态管理器与最新信号；
//! - 输出 append 过程中的回溯信息（`BacktrackIds`）。

use crate::keyzone::{KeyZoneManager, KeyZoneSignal};
use crate::sd::{SupplyDemand, SupplyDemandConfig, SupplyDemandResult};
use crate::swing::SwingManager;
use crate::trend::TrendManager;
use crate::constant::Timeframe;
use crate::bar::SBar;
use crate::cbar_manager::CBarManager;
use crate::sbar_manager::SBarManager;

pub(crate) struct TimeframeManager {
    pub(crate) timeframe: Timeframe,
    pub(crate) sbar_manager: SBarManager,
    pub(crate) cbar_manager: CBarManager,
    pub(crate) swing_manager: SwingManager,
    pub(crate) trend_manager: TrendManager,
    pub(crate) keyzone_manager: KeyZoneManager,
    pub(crate) latest_keyzone_signal: Option<KeyZoneSignal>,
    pub(crate) sd: SupplyDemand,
    pub(crate) latest_sd: Option<SupplyDemandResult>,
}

pub(crate) struct BacktrackIds {
    pub(crate) cbar_backtrack_id: Option<u64>,
    pub(crate) swing_backtrack_id: Option<u64>,
    pub(crate) trend_backtrack_id: Option<u64>,
}

impl TimeframeManager {
    pub(crate) fn new(timeframe: Timeframe) -> Self {
        Self {
            timeframe,
            sbar_manager: SBarManager::new(timeframe),
            cbar_manager: CBarManager::new(timeframe),
            swing_manager: SwingManager::new(),
            trend_manager: TrendManager::new(),
            keyzone_manager: KeyZoneManager::new(),
            latest_keyzone_signal: None,
            sd: SupplyDemand::default(),
            latest_sd: None,
        }
    }

    pub(crate) fn append(&mut self, sbar: SBar) -> BacktrackIds {
        let sbar = self.sbar_manager.append(sbar);
        let _cbar = self.cbar_manager.on_sbar(&sbar);
        let cbar_backtrack_id = self.cbar_manager.backtrack_id();
        let cbars = self.cbar_manager.all_rows();
        let swing_backtrack_id = self
            .swing_manager
            .rebuild_from_cbars_with_backtrack(&cbars, cbar_backtrack_id);
        let swings = self.swing_manager.all_rows();
        let trend_backtrack_id = self
            .trend_manager
            .rebuild_from_swings_with_backtrack(&swings, swing_backtrack_id);

        self.keyzone_manager.rebuild_from(
            self.timeframe,
            &self.swing_manager.last_n(20),
            &self.trend_manager.last_n(20),
            &self.sbar_manager.last_n(200),
        );

        let recent = self.sbar_manager.last_n(2);
        if let Some(last_bar) = recent.last() {
            self.latest_keyzone_signal = self
                .keyzone_manager
                .evaluate_latest_signal(last_bar, recent.first().filter(|_| recent.len() > 1));
        } else {
            self.latest_keyzone_signal = None;
        }

        let keyzone_bias = self
            .latest_keyzone_signal
            .as_ref()
            .map(|x| x.signed_strength())
            .unwrap_or(0.0);
        self.latest_sd = Some(
            self.sd
                .evaluate_window_with_bias(&self.sbar_manager.last_n(50), keyzone_bias),
        );
        BacktrackIds {
            cbar_backtrack_id,
            swing_backtrack_id,
            trend_backtrack_id,
        }
    }

    pub(crate) fn set_sd_config(&mut self, config: SupplyDemandConfig) {
        self.sd = SupplyDemand::with_config(config);
    }
}
