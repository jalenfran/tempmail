use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    NewMessage { id: String, local_part: String },
    MessageDeleted { id: String },
    AddressAdded { local_part: String },
    AddressRemoved { local_part: String },
}

#[derive(Clone)]
pub struct EventBus(broadcast::Sender<Event>);

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self(tx)
    }
    pub fn send(&self, evt: Event) {
        let _ = self.0.send(evt);
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.0.subscribe()
    }
}
