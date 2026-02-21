//! 多周期上下文（MTC）对外入口。
//!
//! 该模块负责：
//! - 对外提供 `MultiTimeframeContext` API；
//! - 协调各子模块完成追加、回溯、快照导出与事件通知；
//! - 组合内部管理器完成单周期处理链路。

use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::path::Path;

use polars::prelude::DataFrame;
use polars::prelude::ParquetWriter;

use crate::bar::{CBar, Fractal, SBar};
use crate::constant::{DataError, EventType, Timeframe};
use crate::events::{EventPayload, Observable, Subscriber};
use crate::keyzone::KeyZoneSignal;
use crate::sd::{SupplyDemandConfig, SupplyDemandResult};
use crate::swing::Swing;
use crate::trend::Trend;
use crate::timeframe_manager::TimeframeManager;

pub struct MultiTimeframeContext {
    symbol: String,
    managers: HashMap<Timeframe, TimeframeManager>,
    observable: Observable,
}

impl MultiTimeframeContext {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            symbol: symbol.into(),
            managers: HashMap::new(),
            observable: Observable::default(),
        }
    }

    pub fn register(&mut self, timeframe: Timeframe) {
        self.managers
            .entry(timeframe)
            .or_insert_with(|| TimeframeManager::new(timeframe));
    }

    pub fn subscribe(&mut self, event_type: Option<EventType>, subscriber: Subscriber) {
        self.observable.subscribe(event_type, subscriber);
    }

    pub fn append(&mut self, timeframe: Timeframe, sbar: SBar) {
        if let Some(manager) = self.managers.get_mut(&timeframe) {
            let evt = manager.append(sbar);
            self.observable.notify(
                timeframe,
                EventType::SBarCreated,
                EventPayload {
                    backtrack_id: None,
                    note: Some("sbar appended".to_string()),
                },
            );
            self.observable.notify(
                timeframe,
                EventType::CBarChanged,
                EventPayload {
                    backtrack_id: evt.cbar_backtrack_id,
                    note: Some("cbar changed".to_string()),
                },
            );
            self.observable.notify(
                timeframe,
                EventType::SwingChanged,
                EventPayload {
                    backtrack_id: evt.swing_backtrack_id,
                    note: Some("swing changed".to_string()),
                },
            );
            self.observable.notify(
                timeframe,
                EventType::TrendChanged,
                EventPayload {
                    backtrack_id: evt.trend_backtrack_id,
                    note: Some("trend changed".to_string()),
                },
            );
            self.observable.notify(
                timeframe,
                EventType::TimeframeEnd,
                EventPayload {
                    backtrack_id: evt.cbar_backtrack_id,
                    note: Some("timeframe pipeline done".to_string()),
                },
            );
            self.observable.notify(
                timeframe,
                EventType::MtcNewBar,
                EventPayload {
                    backtrack_id: evt.cbar_backtrack_id,
                    note: Some("timeframe pipeline completed".to_string()),
                },
            );
        }
    }

    pub fn set_sd_config(&mut self, timeframe: Timeframe, config: SupplyDemandConfig) {
        if let Some(manager) = self.managers.get_mut(&timeframe) {
            manager.set_sd_config(config);
        }
    }

    pub fn get_sbar_window(&self, timeframe: Timeframe, length: usize) -> Vec<SBar> {
        self.managers
            .get(&timeframe)
            .map(|m| m.sbar_manager.last_n(length))
            .unwrap_or_default()
    }

    pub fn get_cbar_window(&self, timeframe: Timeframe, length: usize) -> Vec<CBar> {
        self.managers
            .get(&timeframe)
            .map(|m| m.cbar_manager.last_n(length))
            .unwrap_or_default()
    }

    pub fn get_sd(&self, timeframe: Timeframe) -> Option<&SupplyDemandResult> {
        self.managers
            .get(&timeframe)
            .and_then(|m| m.latest_sd.as_ref())
    }

    pub fn get_keyzone_signal(&self, timeframe: Timeframe) -> Option<&KeyZoneSignal> {
        self.managers
            .get(&timeframe)
            .and_then(|m| m.latest_keyzone_signal.as_ref())
    }

    pub fn get_swing_window(&self, timeframe: Timeframe, length: usize) -> Vec<Swing> {
        self.managers
            .get(&timeframe)
            .map(|m| m.swing_manager.last_n(length))
            .unwrap_or_default()
    }

    pub fn get_trend_window(&self, timeframe: Timeframe, length: usize) -> Vec<Trend> {
        self.managers
            .get(&timeframe)
            .map(|m| m.trend_manager.last_n(length))
            .unwrap_or_default()
    }

    pub fn get_latest_fractal(&self, timeframe: Timeframe) -> Option<Fractal> {
        self.managers
            .get(&timeframe)
            .and_then(|m| m.cbar_manager.last_fractal())
    }

    pub fn get_fractal_at_cbar_id(&self, timeframe: Timeframe, cbar_id: u64) -> Option<Fractal> {
        self.managers
            .get(&timeframe)
            .and_then(|m| m.cbar_manager.fractal_at_id(cbar_id))
    }

    pub fn get_prev_fractal(&self, timeframe: Timeframe, cbar_id: u64) -> Option<Fractal> {
        self.managers
            .get(&timeframe)
            .and_then(|m| m.cbar_manager.prev_fractal(cbar_id))
    }

    pub fn get_next_fractal(&self, timeframe: Timeframe, cbar_id: u64) -> Option<Fractal> {
        self.managers
            .get(&timeframe)
            .and_then(|m| m.cbar_manager.next_fractal(cbar_id))
    }

    pub fn count(&self, timeframe: Timeframe) -> usize {
        self.managers
            .get(&timeframe)
            .map(|m| m.sbar_manager.row_count())
            .unwrap_or(0)
    }

    pub fn get_sbar_dataframe(&self, timeframe: Timeframe) -> Option<DataFrame> {
        self.managers
            .get(&timeframe)
            .map(|m| m.sbar_manager.dataframe())
    }

    pub fn get_cbar_dataframe(&self, timeframe: Timeframe) -> Option<DataFrame> {
        self.managers
            .get(&timeframe)
            .map(|m| m.cbar_manager.dataframe())
    }

    pub fn get_swing_dataframe(&self, timeframe: Timeframe) -> Option<DataFrame> {
        self.managers
            .get(&timeframe)
            .map(|m| m.swing_manager.dataframe())
    }

    pub fn get_trend_dataframe(&self, timeframe: Timeframe) -> Option<DataFrame> {
        self.managers
            .get(&timeframe)
            .map(|m| m.trend_manager.dataframe())
    }

    pub fn write_parquet_snapshot(
        &self,
        timeframe: Timeframe,
        output_dir: impl AsRef<Path>,
    ) -> Result<(), DataError> {
        if let Some(manager) = self.managers.get(&timeframe) {
            write_parquet_snapshot_for_timeframe(timeframe, manager, output_dir)?;
        }

        Ok(())
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }
}

fn write_parquet_snapshot_for_timeframe(
    timeframe: Timeframe,
    manager: &TimeframeManager,
    output_dir: impl AsRef<Path>,
) -> Result<(), DataError> {
    let output_dir = output_dir.as_ref();
    create_dir_all(output_dir)?;

    let tf = format!("{:?}", timeframe).to_lowercase();

    let mut sbar_file = File::create(output_dir.join(format!("sbar_{tf}.parquet")))?;
    let mut sbar_df = manager.sbar_manager.dataframe();
    ParquetWriter::new(&mut sbar_file).finish(&mut sbar_df)?;

    let mut cbar_file = File::create(output_dir.join(format!("cbar_{tf}.parquet")))?;
    let mut cbar_df = manager.cbar_manager.dataframe();
    ParquetWriter::new(&mut cbar_file).finish(&mut cbar_df)?;

    let mut swing_file = File::create(output_dir.join(format!("swing_{tf}.parquet")))?;
    let mut swing_df = manager.swing_manager.dataframe();
    ParquetWriter::new(&mut swing_file).finish(&mut swing_df)?;

    let mut trend_file = File::create(output_dir.join(format!("trend_{tf}.parquet")))?;
    let mut trend_df = manager.trend_manager.dataframe();
    ParquetWriter::new(&mut trend_file).finish(&mut trend_df)?;

    Ok(())
}

