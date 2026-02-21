//! 分发模块（fan-out）。
//!
//! 通过按 channel key 分片的方式降低锁竞争，支持多 symbol/interval 下游并发订阅。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;

use crate::SharedBar;

/// 行情分发器。
///
/// 内部维护多个分片，每个分片持有自己的 channel map，
/// 以便将不同 key 的读写竞争隔离在不同锁上。
#[derive(Debug)]
pub struct Distributor {
	shards: Vec<RwLock<HashMap<String, Arc<broadcast::Sender<SharedBar>>>>>,
	shard_count: usize,
	channel_capacity: usize,
}

impl Distributor {
	/// 创建分发器。
	pub fn new(channel_capacity: usize) -> Self {
		let shard_count = default_distributor_shards();
		let mut shards = Vec::with_capacity(shard_count);
		for _ in 0..shard_count {
			shards.push(RwLock::new(HashMap::new()));
		}

		Self {
			shards,
			shard_count,
			channel_capacity: channel_capacity.max(1),
		}
	}

	/// 订阅指定 symbol + interval 的数据流。
	pub fn subscribe(&self, symbol: &str, interval_secs: u64) -> broadcast::Receiver<SharedBar> {
		let key = channel_key(symbol, interval_secs);
		let mut guard = self.shards[self.shard_index(&key)]
			.write()
			.expect("distributor shard lock poisoned");
		let sender = guard
			.entry(key)
			.or_insert_with(|| {
				let (tx, _) = broadcast::channel(self.channel_capacity);
				Arc::new(tx)
			})
			.clone();
		sender.subscribe()
	}

	/// 向指定频道广播一条 bar，返回当前接收者数量。
	pub fn broadcast(&self, symbol: &str, interval_secs: u64, bar: SharedBar) -> usize {
		let key = channel_key(symbol, interval_secs);
		let guard = self.shards[self.shard_index(&key)]
			.read()
			.expect("distributor shard lock poisoned");
		if let Some(sender) = guard.get(&key) {
			let _ = sender.send(bar);
			sender.receiver_count()
		} else {
			0
		}
	}

	/// 查询指定频道接收者数量。
	pub fn subscriber_count(&self, symbol: &str, interval_secs: u64) -> usize {
		let key = channel_key(symbol, interval_secs);
		let guard = self.shards[self.shard_index(&key)]
			.read()
			.expect("distributor shard lock poisoned");
		guard
			.get(&key)
			.map(|sender| sender.receiver_count())
			.unwrap_or(0)
	}

	/// 返回当前活跃频道（receiver_count > 0）。
	pub fn active_channels(&self) -> Vec<String> {
		let mut active = Vec::new();
		for shard in &self.shards {
			let guard = shard.read().expect("distributor shard lock poisoned");
			active.extend(
				guard
					.iter()
					.filter(|(_, sender)| sender.receiver_count() > 0)
					.map(|(key, _)| key.clone()),
			);
		}
		active
	}

	fn shard_index(&self, key: &str) -> usize {
		hash_key(key) % self.shard_count
	}
}

fn channel_key(symbol: &str, interval_secs: u64) -> String {
	format!("{}:{}", symbol.to_ascii_lowercase(), interval_secs)
}

fn hash_key(text: &str) -> usize {
	let mut hasher = std::collections::hash_map::DefaultHasher::new();
	text.hash(&mut hasher);
	hasher.finish() as usize
}

fn default_distributor_shards() -> usize {
	let cpu = std::thread::available_parallelism()
		.map(|x| x.get())
		.unwrap_or(8);
	(cpu.saturating_mul(2)).clamp(8, 128)
}
