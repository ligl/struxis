//! 指标快照模块。
//!
//! 提供 market 关键运行指标的只读快照结构。

/// market 运行指标快照。
#[derive(Debug, Clone)]
pub struct MarketMetrics {
	/// 已发布到分发器的 bar 数量。
	pub published: u64,
	/// 丢弃总量（`dropped_newest + dropped_oldest`）。
	pub dropped: u64,
	/// 新数据被丢弃数量。
	pub dropped_newest: u64,
	/// 旧数据被丢弃数量。
	pub dropped_oldest: u64,
	/// 背压事件总量。
	pub backpressure_events: u64,
	/// ingress 当前长度。
	pub ingress_len: usize,
	/// ingress 容量上限。
	pub ingress_capacity: usize,
}
