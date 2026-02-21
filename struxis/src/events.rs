use std::collections::HashMap;
use std::sync::Arc;

use crate::constant::{EventType, Timeframe};

#[derive(Debug, Clone, Default)]
pub struct EventPayload {
    pub backtrack_id: Option<u64>,
    pub note: Option<String>,
}

pub type Subscriber = Arc<dyn Fn(Timeframe, EventType, &EventPayload) + Send + Sync>;

#[derive(Default)]
pub struct Observable {
    subscribers: HashMap<EventType, Vec<Subscriber>>,
    all_subscribers: Vec<Subscriber>,
}

impl Observable {
    pub fn subscribe(&mut self, event_type: Option<EventType>, subscriber: Subscriber) {
        if let Some(event_type) = event_type {
            self.subscribers
                .entry(event_type)
                .or_default()
                .push(subscriber);
        } else {
            self.all_subscribers.push(subscriber);
        }
    }

    pub fn notify(&self, timeframe: Timeframe, event_type: EventType, payload: EventPayload) {
        if let Some(subscribers) = self.subscribers.get(&event_type) {
            for subscriber in subscribers {
                subscriber(timeframe, event_type, &payload);
            }
        }

        for subscriber in &self.all_subscribers {
            subscriber(timeframe, event_type, &payload);
        }
    }
}
