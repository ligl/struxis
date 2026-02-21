use broker::{
	pump_from_feed, pump_from_resilient_adapter, BinanceWsAdapter, BrokerLifecycleConfig,
	CtpFeed, ReconnectPolicy, ResilientAdapter,
};
use market::{Feed, FeedConfig, OverloadPolicy};
use strategy::Strategy;

pub fn init() {
	println!("runtime initialized");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
	Ctp,
    Binance,
}

pub fn run_live_bootstrap() {
	run_live_with_mode(RuntimeMode::Ctp);
}

pub fn run_live_with_mode(mode: RuntimeMode) {
	let feed_config = runtime_feed_config();
	println!(
		"runtime mode={:?} market channel_capacity={} ingress_capacity={} overload={:?}",
		mode,
		feed_config.channel_capacity,
		feed_config.ingress_capacity,
		feed_config.overload_policy
	);
	let feed = Feed::with_config("I2601", "DCE", feed_config);
	let symbols = runtime_symbols();
	let primary_symbol = symbols
		.first()
		.cloned()
		.unwrap_or_else(|| "I2601".to_string());

	let mut strategy = Strategy::new(primary_symbol.clone(), feed.exchange.clone());
	let mut strategy_sub = feed.subscribe(&primary_symbol, 300);
	let mut monitor_subs = symbols
		.iter()
		.filter(|symbol| *symbol != &primary_symbol)
		.cloned()
		.map(|symbol| (symbol.clone(), feed.subscribe(&symbol, 300)))
		.collect::<Vec<_>>();

	let published = match mode {
		RuntimeMode::Ctp => {
			let mut ctp_feed = CtpFeed::new("I2601", "CTP", 100.0);
			pump_from_feed(&feed, 300, 8, &mut ctp_feed)
		}
		RuntimeMode::Binance => {
			let adapter = BinanceWsAdapter::new(primary_symbol.clone(), 100.0);
			let mut resilient = ResilientAdapter::new(
				adapter,
				BrokerLifecycleConfig {
					heartbeat_interval_ms: 1000,
					heartbeat_timeout_ms: 5000,
					reconnect: ReconnectPolicy {
						initial_delay_ms: 10,
						max_delay_ms: 1000,
						max_retries: 5,
					},
				},
			);

			for symbol in &symbols {
				if let Err(error) = resilient.subscribe_symbol(symbol.clone()) {
					println!("broker subscribe error symbol={} err={}", symbol, error);
					return;
				}
			}

			match pump_from_resilient_adapter(&feed, 300, 8, &mut resilient) {
				Ok(count) => count,
				Err(error) => {
					println!("broker resilient poll error: {}", error);
					return;
				}
			}
		}
	};

	for _ in 0..published {
		let bar = match strategy_sub.try_recv() {
			Ok(bar) => bar,
			Err(_) => {
				for (symbol, monitor_sub) in &mut monitor_subs {
					if let Ok(monitor_bar) = monitor_sub.try_recv() {
						println!(
							"monitor symbol={} close={} volume={}",
							symbol,
							monitor_bar.close_price,
							monitor_bar.volume
						);
					}
				}
				let _ = feed.pop_ingress();
				continue;
			}
		};

		let _ = feed.pop_ingress();

		let decision = strategy.on_shared_bar(bar);
		println!("decision={:?} reason={}", decision.action, decision.reason);

		for (symbol, monitor_sub) in &mut monitor_subs {
			if let Ok(monitor_bar) = monitor_sub.try_recv() {
				println!(
					"monitor symbol={} close={} volume={}",
					symbol,
					monitor_bar.close_price,
					monitor_bar.volume
				);
			}
		}

	}

	let metrics = feed.metrics();
	for symbol in &symbols {
		println!(
			"market subscribers symbol={} count={}",
			symbol,
			feed.subscriber_count(symbol, 300)
		);
	}
	println!(
		"market metrics published={} dropped={} ingress={}/{}",
		metrics.published,
		metrics.dropped,
		metrics.ingress_len,
		metrics.ingress_capacity
	);
}

fn runtime_feed_config() -> FeedConfig {
	let defaults = FeedConfig::default();
	FeedConfig {
		channel_capacity: env_usize("STRUXIS_MARKET_CHANNEL_CAPACITY")
			.unwrap_or(defaults.channel_capacity),
		ingress_capacity: env_usize("STRUXIS_MARKET_INGRESS_CAPACITY")
			.unwrap_or(defaults.ingress_capacity),
		overload_policy: env_overload_policy("STRUXIS_MARKET_OVERLOAD")
			.unwrap_or(defaults.overload_policy),
	}
}

fn env_usize(key: &str) -> Option<usize> {
	std::env::var(key)
		.ok()
		.and_then(|value| value.parse::<usize>().ok())
		.filter(|value| *value > 0)
}

fn env_overload_policy(key: &str) -> Option<OverloadPolicy> {
	let value = std::env::var(key).ok()?.to_ascii_lowercase();
	match value.as_str() {
		"drop_newest" => Some(OverloadPolicy::DropNewest),
		"drop_oldest" => Some(OverloadPolicy::DropOldest),
		_ => None,
	}
}

fn runtime_symbols() -> Vec<String> {
	let raw = std::env::var("STRUXIS_SYMBOLS").unwrap_or_else(|_| "I2601".to_string());
	let mut symbols = raw
		.split(',')
		.map(|x| x.trim())
		.filter(|x| !x.is_empty())
		.map(|x| x.to_string())
		.collect::<Vec<_>>();

	if symbols.is_empty() {
		symbols.push("I2601".to_string());
	}

	let mut deduped = Vec::new();
	for symbol in symbols {
		if !deduped.iter().any(|x| x == &symbol) {
			deduped.push(symbol);
		}
	}

	deduped
}
