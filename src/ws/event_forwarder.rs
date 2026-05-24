use std::sync::Arc;

use crate::signals::bus::AppSignalBus;
use injectable::prelude::*;

/// Event forwarder that bridges signal bus events to WebSocket connections.
#[derive(Debug, Clone)]
pub struct EventForwarder {
    signal_bus: Arc<AppSignalBus>,
}

#[injectable]
impl EventForwarder {
    /// Create a new EventForwarder.
    #[injectable(ctor)]
    pub fn new(#[injectable(inject)] signal_bus: Arc<AppSignalBus>) -> Self {
        Self { signal_bus }
    }

    /// Subscribe to signals for a user.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<crate::signals::bus::AppSignal> {
        self.signal_bus.subscribe()
    }

    /// Get a reference to the signal bus.
    pub fn signal_bus(&self) -> &Arc<AppSignalBus> {
        &self.signal_bus
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_event_forwarder_subscribe() {
        let bus = Arc::new(AppSignalBus::new());
        let forwarder = EventForwarder::new(bus.clone());
        let _rx = forwarder.subscribe();
        assert_eq!(bus.receiver_count(), 1);
    }

    #[test]
    fn test_event_forwarder_signal_bus() {
        let bus = Arc::new(AppSignalBus::new());
        let forwarder = EventForwarder::new(bus.clone());
        assert!(Arc::ptr_eq(forwarder.signal_bus(), &bus));
    }
}
