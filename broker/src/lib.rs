pub mod adapters;
pub mod error;
pub mod lifecycle;
pub mod pump;
pub mod protocol;

pub use error::BrokerError;
pub use lifecycle::{
	BrokerLifecycleConfig, BrokerLifecycleStats, ReconnectPolicy, ResilientAdapter,
};
pub use pump::{pump_from_adapter, pump_from_feed, pump_from_resilient_adapter};
pub use protocol::{ExchangeAdapter, ExchangeFeed};
pub use adapters::{BinanceWsAdapter, CtpFeed, MockAdapter};

#[cfg(test)]
mod tests {
	use super::{
		pump_from_resilient_adapter, BrokerError, BrokerLifecycleConfig, ExchangeAdapter,
		ReconnectPolicy, ResilientAdapter,
	};
	use chrono::Utc;
	use market::{BrokerBar, Feed};
	use struxis::Timeframe;

	struct FlakyAdapter {
		connected: bool,
		connect_calls: u32,
		subscribe_calls: u32,
		poll_calls: u32,
		fail_first_connect: bool,
		fail_first_poll: bool,
	}

	impl FlakyAdapter {
		fn new() -> Self {
			Self {
				connected: false,
				connect_calls: 0,
				subscribe_calls: 0,
				poll_calls: 0,
				fail_first_connect: true,
				fail_first_poll: true,
			}
		}
	}

	impl ExchangeAdapter for FlakyAdapter {
		fn venue(&self) -> &str {
			"TEST"
		}

		fn connect(&mut self) -> Result<(), BrokerError> {
			self.connect_calls += 1;
			if self.fail_first_connect {
				self.fail_first_connect = false;
				return Err(BrokerError::ConnectionFailed("first connect fails".to_string()));
			}
			self.connected = true;
			Ok(())
		}

		fn subscribe_symbol(&mut self, _symbol: &str) -> Result<(), BrokerError> {
			if !self.connected {
				return Err(BrokerError::NotConnected);
			}
			self.subscribe_calls += 1;
			Ok(())
		}

		fn heartbeat(&mut self) -> Result<(), BrokerError> {
			if self.connected {
				Ok(())
			} else {
				Err(BrokerError::NotConnected)
			}
		}

		fn poll_bar(&mut self) -> Result<Option<BrokerBar>, BrokerError> {
			if !self.connected {
				return Err(BrokerError::NotConnected);
			}

			self.poll_calls += 1;
			if self.fail_first_poll {
				self.fail_first_poll = false;
				self.connected = false;
				return Err(BrokerError::NotConnected);
			}

			Ok(Some(BrokerBar {
				id: None,
				symbol: "I2601".to_string(),
				exchange: "TEST".to_string(),
				timeframe: Timeframe::M1,
				datetime: Utc::now(),
				open_price: 100.0,
				high_price: 100.5,
				low_price: 99.8,
				close_price: 100.2,
				volume: 1000.0,
				open_interest: 100.0,
				turnover: 100200.0,
			}))
		}
	}

	#[test]
	fn resilient_adapter_reconnects_and_recovers_subscription() {
		let feed = Feed::new("I2601", "DCE");
		let mut resilient = ResilientAdapter::new(
			FlakyAdapter::new(),
			BrokerLifecycleConfig {
				heartbeat_interval_ms: 5,
				heartbeat_timeout_ms: 50,
				reconnect: ReconnectPolicy {
					initial_delay_ms: 0,
					max_delay_ms: 0,
					max_retries: 3,
				},
			},
		);

		resilient
			.subscribe_symbol("I2601")
			.expect("pre-connection subscription must be accepted");

		let published = pump_from_resilient_adapter(&feed, 300, 2, &mut resilient)
			.expect("resilient pumping should recover and publish");
		assert_eq!(published, 2);

		let stats = resilient.stats();
		assert!(stats.reconnect_total >= 2);
		assert!(stats.subscription_replays >= 1);
	}
}
