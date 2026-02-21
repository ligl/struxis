use market::Feed;

use crate::error::BrokerError;
use crate::lifecycle::ResilientAdapter;
use crate::protocol::{ExchangeAdapter, ExchangeFeed};

pub fn pump_from_feed(
    feed: &Feed,
    interval_secs: u64,
    count: usize,
    adapter_feed: &mut impl ExchangeFeed,
) -> usize {
    let mut published = 0usize;
    for _ in 0..count {
        if let Some(bar) = adapter_feed.next_bar() {
            let _ = feed.ingest_broker_bar(bar, interval_secs);
            published += 1;
        }
    }
    published
}

pub fn pump_from_adapter(
    feed: &Feed,
    interval_secs: u64,
    count: usize,
    adapter: &mut impl ExchangeAdapter,
) -> Result<usize, BrokerError> {
    let mut published = 0usize;
    for _ in 0..count {
        if let Some(bar) = adapter.poll_bar()? {
            let _ = feed.ingest_broker_bar(bar, interval_secs);
            published += 1;
        }
    }
    Ok(published)
}

pub fn pump_from_resilient_adapter<A: ExchangeAdapter>(
    feed: &Feed,
    interval_secs: u64,
    count: usize,
    adapter: &mut ResilientAdapter<A>,
) -> Result<usize, BrokerError> {
    let mut published = 0usize;
    let mut attempts = 0usize;
    let max_attempts = count.saturating_mul(100).max(1);

    while published < count && attempts < max_attempts {
        attempts += 1;
        if let Some(bar) = adapter.poll_bar()? {
            let _ = feed.ingest_broker_bar(bar, interval_secs);
            published += 1;
        } else {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    Ok(published)
}
