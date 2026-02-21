use std::thread;
use std::time::{Duration as StdDuration, Instant};

use market::BrokerBar;

use crate::error::BrokerError;
use crate::protocol::ExchangeAdapter;

#[derive(Debug, Clone, Copy)]
pub struct ReconnectPolicy {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub max_retries: u32,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial_delay_ms: 50,
            max_delay_ms: 2000,
            max_retries: 8,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BrokerLifecycleConfig {
    pub heartbeat_interval_ms: u64,
    pub heartbeat_timeout_ms: u64,
    pub reconnect: ReconnectPolicy,
}

impl Default for BrokerLifecycleConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 2000,
            heartbeat_timeout_ms: 10_000,
            reconnect: ReconnectPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BrokerLifecycleStats {
    pub reconnect_total: u64,
    pub connect_failures: u64,
    pub heartbeat_failures: u64,
    pub subscription_replays: u64,
}

pub struct ResilientAdapter<A: ExchangeAdapter> {
    adapter: A,
    config: BrokerLifecycleConfig,
    connected: bool,
    last_seen: Option<Instant>,
    subscriptions: Vec<String>,
    stats: BrokerLifecycleStats,
}

impl<A: ExchangeAdapter> ResilientAdapter<A> {
    pub fn new(adapter: A, config: BrokerLifecycleConfig) -> Self {
        Self {
            adapter,
            config,
            connected: false,
            last_seen: None,
            subscriptions: Vec::new(),
            stats: BrokerLifecycleStats::default(),
        }
    }

    pub fn subscribe_symbol(&mut self, symbol: impl Into<String>) -> Result<(), BrokerError> {
        let symbol = symbol.into();
        if !self.subscriptions.iter().any(|x| x == &symbol) {
            self.subscriptions.push(symbol.clone());
        }

        if self.connected {
            self.adapter.subscribe_symbol(&symbol)?;
        }

        Ok(())
    }

    pub fn connect(&mut self) -> Result<(), BrokerError> {
        self.reconnect_with_backoff()
    }

    pub fn poll_bar(&mut self) -> Result<Option<BrokerBar>, BrokerError> {
        self.ensure_live()?;

        match self.adapter.poll_bar() {
            Ok(result) => {
                self.last_seen = Some(Instant::now());
                Ok(result)
            }
            Err(_) => {
                self.connected = false;
                self.stats.connect_failures += 1;
                self.reconnect_with_backoff()?;
                let retry = self.adapter.poll_bar()?;
                self.last_seen = Some(Instant::now());
                Ok(retry)
            }
        }
    }

    pub fn stats(&self) -> BrokerLifecycleStats {
        self.stats.clone()
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    fn ensure_live(&mut self) -> Result<(), BrokerError> {
        if !self.connected {
            return self.reconnect_with_backoff();
        }

        if let Some(last) = self.last_seen {
            let elapsed_ms = last.elapsed().as_millis() as u64;
            if elapsed_ms >= self.config.heartbeat_timeout_ms {
                self.connected = false;
                self.stats.heartbeat_failures += 1;
                return self.reconnect_with_backoff();
            }

            if elapsed_ms >= self.config.heartbeat_interval_ms {
                if self.adapter.heartbeat().is_err() {
                    self.connected = false;
                    self.stats.heartbeat_failures += 1;
                    return self.reconnect_with_backoff();
                }
                self.last_seen = Some(Instant::now());
            }
        }

        Ok(())
    }

    fn reconnect_with_backoff(&mut self) -> Result<(), BrokerError> {
        let retries = self.config.reconnect.max_retries.max(1);
        for attempt in 0..retries {
            if attempt > 0 {
                let backoff = compute_backoff_ms(self.config.reconnect, attempt);
                if backoff > 0 {
                    thread::sleep(StdDuration::from_millis(backoff));
                }
            }

            match self.adapter.connect() {
                Ok(_) => {
                    self.connected = true;
                    self.last_seen = Some(Instant::now());
                    self.stats.reconnect_total += 1;
                    self.replay_subscriptions()?;
                    return Ok(());
                }
                Err(_) => {
                    self.stats.connect_failures += 1;
                }
            }
        }

        Err(BrokerError::ConnectionFailed(
            "reconnect retries exhausted".to_string(),
        ))
    }

    fn replay_subscriptions(&mut self) -> Result<(), BrokerError> {
        for symbol in self.subscriptions.clone() {
            self.adapter.subscribe_symbol(&symbol)?;
            self.stats.subscription_replays += 1;
        }
        Ok(())
    }
}

fn compute_backoff_ms(policy: ReconnectPolicy, attempt: u32) -> u64 {
    let shift = attempt.saturating_sub(1).min(10);
    let scaled = policy.initial_delay_ms.saturating_mul(1u64 << shift);
    scaled.min(policy.max_delay_ms.max(policy.initial_delay_ms))
}
