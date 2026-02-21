//! `market` crate 入口。
//!
//! 职责：提供行情标准化、有界无锁 ingress、分片分发（fan-out）与基础指标。
//! 该文件只做模块装配与统一导出，具体实现位于各子模块。
//!
//! 模块分工：
//! - `bar`：`Bar` / `BrokerBar` 数据结构。
//! - `ingress`：有界无锁队列与过载策略。
//! - `distributor`：分片广播分发。
//! - `feed`：统一入口与主流程。
//! - `metrics`：运行指标快照。
//!
//! 快速示例：
//! ```rust
//! use market::{BrokerBar, Feed};
//! use chrono::Utc;
//! use struxis::Timeframe;
//!
//! let feed = Feed::new("I2601", "DCE");
//! let mut sub = feed.subscribe("I2601", 300);
//!
//! let _ = feed.ingest_broker_bar(BrokerBar {
//!     id: None,
//!     symbol: "I2601".to_string(),
//!     exchange: "DCE".to_string(),
//!     timeframe: Timeframe::M1,
//!     datetime: Utc::now(),
//!     open_price: 100.0,
//!     high_price: 101.0,
//!     low_price: 99.5,
//!     close_price: 100.5,
//!     volume: 1200.0,
//!     open_interest: 5000.0,
//!     turnover: 120600.0,
//! }, 300);
//!
//! let _ = sub.try_recv();
//! ```

mod bar;
mod distributor;
mod feed;
mod ingress;
mod metrics;
mod storage;

pub use bar::{Bar, BrokerBar, SharedBar};
pub use distributor::Distributor;
pub use feed::{Feed, FeedConfig};
pub use ingress::{IngressPushResult, OverloadPolicy, RingBuffer};
pub use metrics::MarketMetrics;
pub use storage::{AsyncBarStore, AsyncBarStoreConfig, BarStore};

#[cfg(test)]
mod tests {
	use super::{BrokerBar, Feed, IngressPushResult, OverloadPolicy, RingBuffer};
	use std::sync::Arc;

	#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
	async fn fanout_delivers_same_bar_to_multiple_subscribers() {
		let feed = Feed::new("I2601", "DCE");
		let mut sub_a = feed.subscribe("I2601", 300);
		let mut sub_b = feed.subscribe("I2601", 300);

		let bars = feed.bootstrap_bars(1);
		let bar = bars[0].clone();
		let _ = feed.ingest_broker_bar(
			BrokerBar {
				id: None,
				symbol: bar.symbol,
				exchange: bar.exchange,
				timeframe: bar.timeframe,
				datetime: bar.datetime,
				open_price: bar.open_price,
				high_price: bar.high_price,
				low_price: bar.low_price,
				close_price: bar.close_price,
				volume: bar.volume,
				open_interest: bar.open_interest,
				turnover: bar.turnover,
			},
			300,
		);

		let recv_a = sub_a.recv().await.expect("subscriber a receives bar");
		let recv_b = sub_b.recv().await.expect("subscriber b receives bar");

		assert_eq!(recv_a.symbol, "I2601");
		assert_eq!(recv_b.symbol, "I2601");
		assert_eq!(recv_a.close_price, recv_b.close_price);
	}

	#[test]
	fn ring_buffer_respects_capacity() {
		let queue = RingBuffer::with_policy(1, OverloadPolicy::DropNewest);
		let feed = Feed::new("I2601", "DCE");
		let bars = feed.bootstrap_bars(2);
		let first = Arc::new(bars[0].clone());
		let second = Arc::new(bars[1].clone());

		assert_eq!(queue.push(first), IngressPushResult::Enqueued);
		assert_eq!(queue.push(second), IngressPushResult::DroppedNewest);
		assert_eq!(queue.len(), 1);
	}

	#[test]
	fn ring_buffer_drop_oldest_keeps_latest() {
		let queue = RingBuffer::with_policy(1, OverloadPolicy::DropOldest);
		let feed = Feed::new("I2601", "DCE");
		let bars = feed.bootstrap_bars(2);
		let first = Arc::new(bars[0].clone());
		let second = Arc::new(bars[1].clone());

		assert_eq!(queue.push(first), IngressPushResult::Enqueued);
		assert_eq!(queue.push(second.clone()), IngressPushResult::DroppedOldest);
		let latest = queue.pop().expect("latest bar should be retained");
		assert_eq!(latest.datetime, second.datetime);
	}

	#[test]
	fn metrics_track_published_and_channels() {
		let feed = Feed::new("I2601", "DCE");
		let _sub = feed.subscribe("I2601", 300);
		let bars = feed.bootstrap_bars(1);
		let bar = bars[0].clone();
		let _ = feed.ingest_broker_bar(
			BrokerBar {
				id: None,
				symbol: bar.symbol,
				exchange: bar.exchange,
				timeframe: bar.timeframe,
				datetime: bar.datetime,
				open_price: bar.open_price,
				high_price: bar.high_price,
				low_price: bar.low_price,
				close_price: bar.close_price,
				volume: bar.volume,
				open_interest: bar.open_interest,
				turnover: bar.turnover,
			},
			300,
		);

		let metrics = feed.metrics();
		assert_eq!(metrics.published, 1);
		assert_eq!(metrics.backpressure_events, 0);
		assert_eq!(feed.subscriber_count("I2601", 300), 1);
		assert_eq!(feed.active_channels().len(), 1);
	}
}
