//! ingress（入站）模块。
//!
//! 提供有界无锁队列与过载策略，用于在高并发场景下控制内存占用并保持可预测行为。

use std::sync::Arc;

use crossbeam::queue::ArrayQueue;

use crate::SharedBar;

/// ingress 满载时的处理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverloadPolicy {
	/// 丢弃当前新入队数据，保留既有队列内容。
	DropNewest,
	/// 丢弃队列最旧数据，再尝试写入当前新数据。
	DropOldest,
}

impl Default for OverloadPolicy {
	fn default() -> Self {
		Self::DropOldest
	}
}

/// 单次 `push` 的结果，用于上层统计背压与丢弃行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngressPushResult {
	/// 成功入队。
	Enqueued,
	/// 因策略或竞争导致新数据被丢弃。
	DroppedNewest,
	/// 为写入新数据而丢弃了旧数据。
	DroppedOldest,
}

/// 有界无锁 ring buffer 抽象。
#[derive(Debug)]
pub struct RingBuffer {
	queue: Arc<ArrayQueue<SharedBar>>,
	capacity: usize,
	overload_policy: OverloadPolicy,
}

impl RingBuffer {
	/// 使用默认过载策略创建 ring buffer。
	pub fn new(capacity: usize) -> Self {
		Self::with_policy(capacity, OverloadPolicy::default())
	}

	/// 使用指定过载策略创建 ring buffer。
	pub fn with_policy(capacity: usize, overload_policy: OverloadPolicy) -> Self {
		let bounded_capacity = capacity.max(1);
		Self {
			queue: Arc::new(ArrayQueue::new(bounded_capacity)),
			capacity: bounded_capacity,
			overload_policy,
		}
	}

	/// 尝试写入一个 bar，并返回入队结果。
	pub fn push(&self, bar: SharedBar) -> IngressPushResult {
		match self.queue.push(bar) {
			Ok(()) => IngressPushResult::Enqueued,
			Err(returned) => match self.overload_policy {
				OverloadPolicy::DropNewest => IngressPushResult::DroppedNewest,
				OverloadPolicy::DropOldest => {
					let _ = self.queue.pop();
					if self.queue.push(returned).is_ok() {
						IngressPushResult::DroppedOldest
					} else {
						IngressPushResult::DroppedNewest
					}
				}
			},
		}
	}

	/// 弹出一个 bar。
	pub fn pop(&self) -> Option<SharedBar> {
		self.queue.pop()
	}

	/// 当前队列长度。
	pub fn len(&self) -> usize {
		self.queue.len()
	}

	/// 队列容量上限。
	pub fn capacity(&self) -> usize {
		self.capacity
	}
}
