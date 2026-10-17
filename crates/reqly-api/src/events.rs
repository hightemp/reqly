//! Push events from the core to every connected UI.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Event {
    /// Dotted topic, e.g. `entity.changed`.
    pub topic: String,
    #[ts(type = "unknown")]
    pub payload: Value,
}

impl Event {
    pub fn new(topic: impl Into<String>, payload: impl Serialize) -> Self {
        Self {
            topic: topic.into(),
            payload: serde_json::to_value(payload).unwrap_or(Value::Null),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Publishes to all current subscribers; having none is not an error.
    pub fn publish(&self, event: Event) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_receive_published_events() {
        let bus = EventBus::default();
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        bus.publish(Event::new("app.ready", serde_json::json!({ "ok": true })));
        assert_eq!(a.recv().await.unwrap().topic, "app.ready");
        assert_eq!(b.recv().await.unwrap().payload["ok"], true);
    }

    #[test]
    fn publish_without_subscribers_is_fine() {
        EventBus::default().publish(Event::new("noop", ()));
    }
}
