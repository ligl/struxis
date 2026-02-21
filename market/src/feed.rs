//! `Feed` 主模块。
//!
//! 聚合 ingress、distributor 与 metrics，提供上游 ingest 与下游订阅的统一入口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{Duration, Utc};
use struxis::Timeframe;
use tokio::sync::broadcast;

use crate::{
	Bar, BrokerBar, Distributor, IngressPushResult, MarketMetrics, OverloadPolicy, RingBuffer,
	AsyncBarStore, BarStore, SharedBar,
};

/// `Feed` 初始化配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedConfig {
	/// 每个分发通道的广播缓冲容量。
	pub channel_capacity: usize,
	/// ingress 有界队列容量。
	pub ingress_capacity: usize,
	/// ingress 满载时的过载策略。
	pub overload_policy: OverloadPolicy,
}

impl Default for FeedConfig {
	fn default() -> Self {
		Self {
			channel_capacity: 8192,
			ingress_capacity: 16384,
			overload_policy: OverloadPolicy::default(),
		}
	}
}

/// 行情处理与分发入口。
#[derive(Debug)]
pub struct Feed {
	/// 默认 symbol（用于 bootstrap 与默认场景）。
	pub symbol: String,
	/// 默认交易所标识。
	pub exchange: String,
	distributor: Distributor,
	ingress: RingBuffer,
	published: AtomicU64,
	dropped: AtomicU64,
	dropped_newest: AtomicU64,
	dropped_oldest: AtomicU64,
}

impl Feed {
	/// 使用默认配置创建 `Feed`。
	pub fn new(symbol: impl Into<String>, exchange: impl Into<String>) -> Self {
		Self::with_config(symbol, exchange, FeedConfig::default())
	}

	/// 使用完整配置创建 `Feed`。
	pub fn with_config(
		symbol: impl Into<String>,
		exchange: impl Into<String>,
		config: FeedConfig,
	) -> Self {
		Self {
			symbol: symbol.into(),
			exchange: exchange.into(),
			distributor: Distributor::new(config.channel_capacity),
			ingress: RingBuffer::with_policy(config.ingress_capacity, config.overload_policy),
			published: AtomicU64::new(0),
			dropped: AtomicU64::new(0),
			dropped_newest: AtomicU64::new(0),
			dropped_oldest: AtomicU64::new(0),
		}
	}

	/// 使用自定义容量创建 `Feed`（过载策略为默认值）。
	pub fn with_capacity(
		symbol: impl Into<String>,
		exchange: impl Into<String>,
		channel_capacity: usize,
		ingress_capacity: usize,
	) -> Self {
		Self::with_config(
			symbol,
			exchange,
			FeedConfig {
				channel_capacity,
				ingress_capacity,
				overload_policy: OverloadPolicy::default(),
			},
		)
	}

	/// 使用自定义容量与过载策略创建 `Feed`。
	pub fn with_policy(
		symbol: impl Into<String>,
		exchange: impl Into<String>,
		channel_capacity: usize,
		ingress_capacity: usize,
		overload_policy: OverloadPolicy,
	) -> Self {
		Self::with_config(
			symbol,
			exchange,
			FeedConfig {
				channel_capacity,
				ingress_capacity,
				overload_policy,
			},
		)
	}

	/// 订阅指定 symbol + interval 的行情。
	pub fn subscribe(&self, symbol: &str, interval_secs: u64) -> broadcast::Receiver<SharedBar> {
		self.distributor.subscribe(symbol, interval_secs)
	}

	/// 摄入 broker bar 并广播到对应频道。
///
/// 返回该次广播的接收者数量。
	pub fn ingest_broker_bar(&self, bar: BrokerBar, interval_secs: u64) -> usize {
		let symbol = bar.symbol.clone();
		let shared = Arc::new(bar);

		match self.ingress.push(shared.clone()) {
			IngressPushResult::Enqueued => {}
			IngressPushResult::DroppedNewest => {
				self.dropped.fetch_add(1, Ordering::Relaxed);
				self.dropped_newest.fetch_add(1, Ordering::Relaxed);
			}
			IngressPushResult::DroppedOldest => {
				self.dropped.fetch_add(1, Ordering::Relaxed);
				self.dropped_oldest.fetch_add(1, Ordering::Relaxed);
			}
		}

		let receivers = self.distributor.broadcast(&symbol, interval_secs, shared);
		self.published.fetch_add(1, Ordering::Relaxed);
		receivers
	}

	/// 摄入 broker bar 并在成功处理后同步落盘。
	///
	/// 返回该次广播接收者数量；落盘失败时返回 I/O 错误。
	pub fn ingest_broker_bar_with_store(
		&self,
		bar: BrokerBar,
		interval_secs: u64,
		store: &mut BarStore,
	) -> std::io::Result<usize> {
		let receivers = self.ingest_broker_bar(bar.clone(), interval_secs);
		store.append_broker_bar(&bar)?;
		Ok(receivers)
	}

	/// 摄入 broker bar 并异步落盘（非阻塞入队）。
	///
	/// 返回该次广播接收者数量；当异步队列满时返回 `WouldBlock`。
	pub fn ingest_broker_bar_with_async_store(
		&self,
		bar: BrokerBar,
		interval_secs: u64,
		store: &AsyncBarStore,
	) -> std::io::Result<usize> {
		let receivers = self.ingest_broker_bar(bar.clone(), interval_secs);
		store.enqueue_broker_bar(&bar)?;
		Ok(receivers)
	}

	/// 从 ingress 弹出一条 bar（通常用于消费完成后的清理/回放逻辑）。
	pub fn pop_ingress(&self) -> Option<SharedBar> {
		self.ingress.pop()
	}

	/// 返回当前指标快照。
	pub fn metrics(&self) -> MarketMetrics {
		let dropped_newest = self.dropped_newest.load(Ordering::Relaxed);
		let dropped_oldest = self.dropped_oldest.load(Ordering::Relaxed);
		MarketMetrics {
			published: self.published.load(Ordering::Relaxed),
			dropped: self.dropped.load(Ordering::Relaxed),
			dropped_newest,
			dropped_oldest,
			backpressure_events: dropped_newest + dropped_oldest,
			ingress_len: self.ingress.len(),
			ingress_capacity: self.ingress.capacity(),
		}
	}

	/// 查询指定频道的订阅者数量。
	pub fn subscriber_count(&self, symbol: &str, interval_secs: u64) -> usize {
		self.distributor.subscriber_count(symbol, interval_secs)
	}

	/// 列出当前活跃频道。
	pub fn active_channels(&self) -> Vec<String> {
		self.distributor.active_channels()
	}

	/// 生成用于测试/演示的 bootstrap bars。
	pub fn bootstrap_bars(&self, count: usize) -> Vec<Bar> {
		let start = Utc::now() - Duration::minutes(count as i64);
		(0..count)
			.map(|index| {
				let base = 100.0 + index as f64 * 0.2;
				Bar {
					id: None,
					symbol: self.symbol.clone(),
					exchange: self.exchange.clone(),
					timeframe: Timeframe::M1,
					datetime: start + Duration::minutes(index as i64),
					open_price: base,
					high_price: base + 0.6,
					low_price: base - 0.4,
					close_price: base + 0.2,
					volume: 1000.0 + index as f64 * 10.0,
					open_interest: 5000.0 + index as f64 * 8.0,
					turnover: (1000.0 + index as f64 * 10.0) * (base + 0.2),
				}
			})
			.collect()
	}
}
