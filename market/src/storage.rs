//! 数据落地与存储模块。
//!
//! 提供两种持久化能力：
//! - `BarStore`：同步 append-only 写盘。
//! - `AsyncBarStore`：异步写盘（后台线程 + 有界队列，主线程非阻塞入队）。

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};

use chrono::{DateTime, Utc};
use crossbeam::channel::{self, Receiver, Sender, TrySendError};

use crate::{Bar, BrokerBar};

/// bar 存储组件（append-only，同步写盘）。
#[derive(Debug)]
pub struct BarStore {
	path: PathBuf,
	writer: BufWriter<File>,
	written_records: u64,
}

impl BarStore {
	/// 以 append 模式打开（不存在则创建）存储文件。
	pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
		let path = path.as_ref().to_path_buf();
		if let Some(parent) = path.parent() {
			if !parent.as_os_str().is_empty() {
				fs::create_dir_all(parent)?;
			}
		}

		let file = OpenOptions::new().create(true).append(true).open(&path)?;
		Ok(Self {
			path,
			writer: BufWriter::new(file),
			written_records: 0,
		})
	}

	/// 追加写入一条 `Bar`。
	pub fn append_bar(&mut self, bar: &Bar) -> std::io::Result<()> {
		write_record(&mut self.writer, bar)?;
		self.written_records = self.written_records.saturating_add(1);
		Ok(())
	}

	/// 追加写入一条 `BrokerBar`（内部转换为 `Bar`）。
	pub fn append_broker_bar(&mut self, broker_bar: &BrokerBar) -> std::io::Result<()> {
		self.append_bar(broker_bar)
	}

	/// 强制 flush 到磁盘。
	pub fn flush(&mut self) -> std::io::Result<()> {
		self.writer.flush()
	}

	/// 当前存储文件路径。
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// 当前实例已写入的记录数量。
	pub fn written_records(&self) -> u64 {
		self.written_records
	}

	/// 从存储文件顺序读取全部记录（用于回放/校验）。
	pub fn read_all(path: impl AsRef<Path>) -> std::io::Result<Vec<Bar>> {
		let file = File::open(path)?;
		let reader = BufReader::new(file);
		let mut out = Vec::new();
		for line in reader.lines() {
			let line = line?;
			if line.trim().is_empty() {
				continue;
			}
			if let Some(bar) = parse_record(&line) {
				out.push(bar);
			}
		}
		Ok(out)
	}
}

/// `AsyncBarStore` 初始化配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncBarStoreConfig {
	/// 后台写线程入队容量（有界队列）。
	pub queue_capacity: usize,
}

impl Default for AsyncBarStoreConfig {
	fn default() -> Self {
		Self { queue_capacity: 16_384 }
	}
}

enum AsyncStoreMessage {
	Bar(Bar),
	Flush(Sender<std::io::Result<()>>),
	Shutdown(Sender<std::io::Result<()>>),
}

/// 异步 bar 存储组件（非阻塞入队 + 后台线程写盘）。
#[derive(Debug)]
pub struct AsyncBarStore {
	path: PathBuf,
	sender: Sender<AsyncStoreMessage>,
	worker: Option<JoinHandle<std::io::Result<()>>>,
}

impl AsyncBarStore {
	/// 打开异步存储。
	pub fn open(path: impl AsRef<Path>, config: AsyncBarStoreConfig) -> std::io::Result<Self> {
		let path = path.as_ref().to_path_buf();
		let queue_capacity = config.queue_capacity.max(1);
		let (tx, rx) = channel::bounded(queue_capacity);

		let worker_path = path.clone();
		let worker = thread::Builder::new()
			.name("market-async-store".to_string())
			.spawn(move || run_async_store_worker(worker_path, rx))
			.map_err(|error| std::io::Error::other(error.to_string()))?;

		Ok(Self {
			path,
			sender: tx,
			worker: Some(worker),
		})
	}

	/// 非阻塞入队一条 `Bar`。
	///
	/// 当队列已满时返回 `WouldBlock`。
	pub fn enqueue_bar(&self, bar: &Bar) -> std::io::Result<()> {
		self.try_send(AsyncStoreMessage::Bar(bar.clone()))
	}

	/// 非阻塞入队一条 `BrokerBar`。
	pub fn enqueue_broker_bar(&self, broker_bar: &BrokerBar) -> std::io::Result<()> {
		self.try_send(AsyncStoreMessage::Bar(broker_bar.clone()))
	}

	/// 请求后台线程 flush，并等待结果。
	pub fn flush(&self) -> std::io::Result<()> {
		let (ack_tx, ack_rx) = channel::bounded(1);
		self.try_send(AsyncStoreMessage::Flush(ack_tx))?;
		ack_rx
			.recv()
			.map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "async store flush ack channel closed"))?
	}

	/// 关闭异步存储并等待后台线程退出。
	pub fn close(mut self) -> std::io::Result<()> {
		self.shutdown_and_join()
	}

	/// 当前存储文件路径。
	pub fn path(&self) -> &Path {
		&self.path
	}

	fn try_send(&self, message: AsyncStoreMessage) -> std::io::Result<()> {
		match self.sender.try_send(message) {
			Ok(()) => Ok(()),
			Err(TrySendError::Full(_)) => Err(std::io::Error::new(
				std::io::ErrorKind::WouldBlock,
				"async store queue is full",
			)),
			Err(TrySendError::Disconnected(_)) => Err(std::io::Error::new(
				std::io::ErrorKind::BrokenPipe,
				"async store worker is disconnected",
			)),
		}
	}

	fn shutdown_and_join(&mut self) -> std::io::Result<()> {
		if self.worker.is_none() {
			return Ok(());
		}

		let (ack_tx, ack_rx) = channel::bounded(1);
		let send_result = self.sender.try_send(AsyncStoreMessage::Shutdown(ack_tx));
		let ack_result = match send_result {
			Ok(()) => ack_rx
				.recv()
				.map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "async store shutdown ack channel closed"))?,
			Err(TrySendError::Full(_)) => Err(std::io::Error::new(
				std::io::ErrorKind::WouldBlock,
				"async store queue is full during shutdown",
			)),
			Err(TrySendError::Disconnected(_)) => Ok(()),
		};

		let join_result = self.worker.take().map(|handle| {
			handle
				.join()
				.map_err(|_| std::io::Error::other("async store worker panicked"))
		});

		if let Some(result) = join_result {
			let worker_result = result?;
			worker_result?;
		}

		ack_result
	}
}

impl Drop for AsyncBarStore {
	fn drop(&mut self) {
		let _ = self.shutdown_and_join();
	}
}

fn run_async_store_worker(path: PathBuf, rx: Receiver<AsyncStoreMessage>) -> std::io::Result<()> {
	let mut store = BarStore::open(path)?;
	while let Ok(message) = rx.recv() {
		match message {
			AsyncStoreMessage::Bar(bar) => {
				store.append_bar(&bar)?;
			}
			AsyncStoreMessage::Flush(ack_tx) => {
				let _ = ack_tx.send(store.flush());
			}
			AsyncStoreMessage::Shutdown(ack_tx) => {
				let result = store.flush();
				let _ = ack_tx.send(result);
				break;
			}
		}
	}
	Ok(())
}

fn write_record(writer: &mut BufWriter<File>, bar: &Bar) -> std::io::Result<()> {
	writeln!(
		writer,
		"{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
		bar.datetime.timestamp_millis(),
		bar.symbol,
		bar.exchange,
		bar.open_price,
		bar.high_price,
		bar.low_price,
		bar.close_price,
		bar.volume,
		bar.open_interest,
		bar.turnover
	)
}

fn parse_record(line: &str) -> Option<Bar> {
	let mut parts = line.split('|');
	let ts = parts.next()?.parse::<i64>().ok()?;
	let symbol = parts.next()?.to_string();
	let exchange = parts.next()?.to_string();
	let open = parts.next()?.parse::<f64>().ok()?;
	let high = parts.next()?.parse::<f64>().ok()?;
	let low = parts.next()?.parse::<f64>().ok()?;
	let close = parts.next()?.parse::<f64>().ok()?;
	let volume = parts.next()?.parse::<f64>().ok()?;
	let open_interest = parts.next()?.parse::<f64>().ok()?;
	let turnover = parts.next()?.parse::<f64>().ok()?;

	let datetime = DateTime::<Utc>::from_timestamp_millis(ts)?;

	Some(Bar {
		id: None,
		symbol,
		exchange,
		timeframe: struxis::Timeframe::M1,
		datetime,
		open_price: open,
		high_price: high,
		low_price: low,
		close_price: close,
		volume,
		open_interest,
		turnover,
	})
}

#[cfg(test)]
mod tests {
	use chrono::Utc;

	use super::{AsyncBarStore, AsyncBarStoreConfig, BarStore};
	use crate::Bar;

	#[test]
	fn store_append_and_read_back() {
		let path = std::env::temp_dir().join(format!(
			"market_store_test_{}_{}.log",
			std::process::id(),
			Utc::now().timestamp_nanos_opt().unwrap_or_default()
		));

		let mut store = BarStore::open(&path).expect("store should open");
		let now = Utc::now();
		let bar = Bar {
			id: None,
			symbol: "I2601".to_string(),
			exchange: "DCE".to_string(),
			timeframe: struxis::Timeframe::M1,
			datetime: now,
			open_price: 100.0,
			high_price: 100.5,
			low_price: 99.8,
			close_price: 100.2,
			volume: 1000.0,
			open_interest: 5000.0,
			turnover: 100200.0,
		};

		store.append_bar(&bar).expect("append should succeed");
		store.flush().expect("flush should succeed");

		let restored = BarStore::read_all(&path).expect("read back should succeed");
		assert_eq!(restored.len(), 1);
		assert_eq!(restored[0].symbol, "I2601");
		assert_eq!(restored[0].exchange, "DCE");

		let _ = std::fs::remove_file(path);
	}

	#[test]
	fn async_store_enqueue_flush_and_read_back() {
		let path = std::env::temp_dir().join(format!(
			"market_async_store_test_{}_{}.log",
			std::process::id(),
			Utc::now().timestamp_nanos_opt().unwrap_or_default()
		));

		let store = AsyncBarStore::open(
			&path,
			AsyncBarStoreConfig {
				queue_capacity: 128,
			},
		)
		.expect("async store should open");

		let bar = Bar {
			id: None,
			symbol: "I2601".to_string(),
			exchange: "DCE".to_string(),
			timeframe: struxis::Timeframe::M1,
			datetime: Utc::now(),
			open_price: 100.0,
			high_price: 100.5,
			low_price: 99.8,
			close_price: 100.2,
			volume: 1000.0,
			open_interest: 5000.0,
			turnover: 100200.0,
		};

		store.enqueue_bar(&bar).expect("enqueue should succeed");
		store.flush().expect("flush should succeed");
		store.close().expect("close should succeed");

		let restored = BarStore::read_all(&path).expect("read back should succeed");
		assert_eq!(restored.len(), 1);
		assert_eq!(restored[0].symbol, "I2601");

		let _ = std::fs::remove_file(path);
	}
}
