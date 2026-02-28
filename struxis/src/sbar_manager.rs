//! SBar 管理器实现。
//!
//! 负责 SBar 序列追加、ID 分配、窗口读取与 dataframe cache 维护。

use polars::df;
use polars::prelude::DataFrame;

use crate::constant::Timeframe;
use crate::bar::SBar;
use crate::IdGenerator;

pub(crate) struct SBarManager {
    timeframe: Timeframe,
    rows: Vec<SBar>,
    id_generator: &'static IdGenerator,
    df_cache: DataFrame,
}

impl SBarManager {
    pub(crate) fn new(timeframe: Timeframe) -> Self {
        Self {
            timeframe,
            rows: Vec::new(),
            id_generator: crate::id_generator::sbar_id_generator(),
            df_cache: DataFrame::default(),
        }
    }

    pub(crate) fn append(&mut self, mut sbar: SBar) -> SBar {
        sbar.id = Some(self.id_generator.get_id());
        sbar.timeframe = self.timeframe;
        self.rows.push(sbar.clone());
        let row = df!(
            "id" => vec![sbar.id.unwrap_or_default()],
            "datetime" => vec![sbar.datetime.timestamp_millis()],
            "symbol" => vec![sbar.symbol.clone()],
            "exchange" => vec![sbar.exchange.clone()],
            "timeframe" => vec![format!("{:?}", sbar.timeframe).to_lowercase()],
            "open_price" => vec![sbar.open_price],
            "high_price" => vec![sbar.high_price],
            "low_price" => vec![sbar.low_price],
            "close_price" => vec![sbar.close_price],
            "volume" => vec![sbar.volume],
            "open_interest" => vec![sbar.open_interest],
            "turnover" => vec![sbar.turnover]
        )
        .expect("failed to build sbar dataframe row");
        if self.df_cache.height() == 0 {
            self.df_cache = row;
        } else {
            self.df_cache
                .vstack_mut(&row)
                .expect("failed to append sbar dataframe row");
        }
        sbar
    }

    pub(crate) fn last_n(&self, n: usize) -> Vec<SBar> {
        self.rows
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
    }

    pub(crate) fn dataframe(&self) -> DataFrame {
        self.df_cache.clone()
    }

    pub(crate) fn row_count(&self) -> usize {
        self.rows.len()
    }
}
