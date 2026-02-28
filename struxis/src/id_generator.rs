use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use std::sync::OnceLock;

const WORKER_ID_BITS: u64 = 10;
const SEQUENCE_BITS: u64 = 12;
const MAX_SEQUENCE: u64 = (1 << SEQUENCE_BITS) - 1;
const WORKER_ID_SHIFT: u64 = SEQUENCE_BITS;
const TIMESTAMP_SHIFT: u64 = SEQUENCE_BITS + WORKER_ID_BITS;

const EPOCH_MS: u64 = 1735689600000;

#[derive(Debug)]
struct Inner {
    sequence: u64,
    last_timestamp: u64,
}

#[derive(Debug)]
pub struct IdGenerator {
    worker_id: u64,
    inner: Mutex<Inner>,
}

static GLOBAL_ID_GENERATOR: OnceLock<IdGenerator> = OnceLock::new();

pub fn global_id_generator() -> &'static IdGenerator {
    GLOBAL_ID_GENERATOR.get_or_init(|| IdGenerator::new(0))
}

// Per-structure worker IDs
const WORKER_SBAR: u64 = 1;
const WORKER_CBAR: u64 = 2;
const WORKER_SWING: u64 = 3;
const WORKER_TREND: u64 = 4;
const WORKER_KEYZONE: u64 = 5;

static SBAR_ID_GENERATOR: OnceLock<IdGenerator> = OnceLock::new();
static CBAR_ID_GENERATOR: OnceLock<IdGenerator> = OnceLock::new();
static SWING_ID_GENERATOR: OnceLock<IdGenerator> = OnceLock::new();
static TREND_ID_GENERATOR: OnceLock<IdGenerator> = OnceLock::new();
static KEYZONE_ID_GENERATOR: OnceLock<IdGenerator> = OnceLock::new();

pub fn sbar_id_generator() -> &'static IdGenerator {
    SBAR_ID_GENERATOR.get_or_init(|| IdGenerator::new(WORKER_SBAR))
}

pub fn cbar_id_generator() -> &'static IdGenerator {
    CBAR_ID_GENERATOR.get_or_init(|| IdGenerator::new(WORKER_CBAR))
}

pub fn swing_id_generator() -> &'static IdGenerator {
    SWING_ID_GENERATOR.get_or_init(|| IdGenerator::new(WORKER_SWING))
}

pub fn trend_id_generator() -> &'static IdGenerator {
    TREND_ID_GENERATOR.get_or_init(|| IdGenerator::new(WORKER_TREND))
}

pub fn keyzone_id_generator() -> &'static IdGenerator {
    KEYZONE_ID_GENERATOR.get_or_init(|| IdGenerator::new(WORKER_KEYZONE))
}

impl IdGenerator {
    pub fn new(worker_id: u64) -> Self {
        assert!(worker_id <= 1023, "worker_id must be <= 1023");
        Self {
            worker_id,
            inner: Mutex::new(Inner {
                sequence: 0,
                last_timestamp: 0,
            }),
        }
    }

    pub fn get_id(&self) -> u64 {
        let mut guard = self.inner.lock().expect("id generator mutex poisoned");
        let mut ts = current_timestamp_ms();

        if ts < guard.last_timestamp {
            ts = guard.last_timestamp;
        }

        if ts == guard.last_timestamp {
            guard.sequence = (guard.sequence + 1) & MAX_SEQUENCE;
            if guard.sequence == 0 {
                while ts <= guard.last_timestamp {
                    ts = current_timestamp_ms();
                }
            }
        } else {
            guard.sequence = 0;
        }

        guard.last_timestamp = ts;

        ((ts - EPOCH_MS) << TIMESTAMP_SHIFT)
            | (self.worker_id << WORKER_ID_SHIFT)
            | guard.sequence
    }
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as u64
}
