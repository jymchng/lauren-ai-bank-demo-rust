use injectable::prelude::*;
use serde::Serialize;
use tokio::sync::broadcast;

/// Signal types for the application event bus.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AppSignal {
    /// Emitted when an LLM call completes.
    ModelCallComplete {
        model: String,
        usage_tokens: usize,
        cost_usd: f64,
        conversation_id: String,
    },
    /// Emitted when an agent turn completes.
    AgentTurnComplete {
        agent_name: String,
        turn: usize,
        conversation_id: String,
    },
    /// Emitted when a tool call starts.
    ToolCallStarted {
        tool_name: String,
        conversation_id: String,
    },
    /// Emitted when a tool call completes.
    ToolCallComplete {
        tool_name: String,
        duration_ms: u64,
        conversation_id: String,
    },
    /// Emitted when a guardrail is triggered.
    GuardrailTriggered {
        agent_name: String,
        passed: bool,
        conversation_id: String,
    },
    /// Emitted when a tool requires human approval.
    ToolPendingApproval {
        conversation_id: String,
        action_type: String,
        details: serde_json::Value,
    },
    /// Emitted when an agent run completes.
    AgentRunComplete {
        agent_name: String,
        conversation_id: String,
    },
}

/// Application signal bus using tokio broadcast channels.
#[derive(Debug, Clone)]
pub struct AppSignalBus {
    sender: broadcast::Sender<AppSignal>,
}

#[injectable]
impl AppSignalBus {
    /// Create a new signal bus.
    #[injectable(ctor)]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    /// Emit a signal to all subscribers.
    pub fn emit(&self, signal: AppSignal) {
        let _ = self.sender.send(signal);
    }

    /// Subscribe to the signal bus.
    pub fn subscribe(&self) -> broadcast::Receiver<AppSignal> {
        self.sender.subscribe()
    }

    /// Get the number of receivers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for AppSignalBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_bus_emit_and_receive() {
        let bus = AppSignalBus::new();
        let mut rx = bus.subscribe();

        bus.emit(AppSignal::AgentRunComplete {
            agent_name: "TestAgent".into(),
            conversation_id: "conv1".into(),
        });

        let signal = rx.try_recv().unwrap();
        match signal {
            AppSignal::AgentRunComplete { agent_name, .. } => {
                assert_eq!(agent_name, "TestAgent");
            }
            _ => panic!("Wrong signal type"),
        }
    }

    #[test]
    fn test_signal_bus_multiple_subscribers() {
        let bus = AppSignalBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(AppSignal::AgentTurnComplete {
            agent_name: "Agent1".into(),
            turn: 1,
            conversation_id: "conv1".into(),
        });

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_signal_bus_no_receivers() {
        let bus = AppSignalBus::new();
        // Emitting with no receivers should not panic
        bus.emit(AppSignal::GuardrailTriggered {
            agent_name: "Agent1".into(),
            passed: true,
            conversation_id: "conv1".into(),
        });
    }

    #[test]
    fn test_signal_bus_default() {
        let bus = AppSignalBus::default();
        assert_eq!(bus.receiver_count(), 0);
    }

    #[test]
    fn test_signal_bus_receiver_count() {
        let bus = AppSignalBus::new();
        assert_eq!(bus.receiver_count(), 0);
        let _rx1 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);
        let _rx2 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 2);
    }

    #[test]
    fn test_signal_serialization() {
        let signal = AppSignal::ModelCallComplete {
            model: "gpt-4".into(),
            usage_tokens: 100,
            cost_usd: 0.01,
            conversation_id: "conv1".into(),
        };
        let json = serde_json::to_string(&signal).unwrap();
        assert!(json.contains("ModelCallComplete"));
    }

    #[test]
    fn test_all_signal_types() {
        let signals = vec![
            AppSignal::ModelCallComplete {
                model: "gpt-4".into(),
                usage_tokens: 100,
                cost_usd: 0.01,
                conversation_id: "conv1".into(),
            },
            AppSignal::AgentTurnComplete {
                agent_name: "Agent1".into(),
                turn: 1,
                conversation_id: "conv1".into(),
            },
            AppSignal::ToolCallStarted {
                tool_name: "get_balance".into(),
                conversation_id: "conv1".into(),
            },
            AppSignal::ToolCallComplete {
                tool_name: "get_balance".into(),
                duration_ms: 50,
                conversation_id: "conv1".into(),
            },
            AppSignal::GuardrailTriggered {
                agent_name: "Agent1".into(),
                passed: true,
                conversation_id: "conv1".into(),
            },
            AppSignal::ToolPendingApproval {
                conversation_id: "conv1".into(),
                action_type: "transfer".into(),
                details: serde_json::json!({"amount": 100}),
            },
            AppSignal::AgentRunComplete {
                agent_name: "Agent1".into(),
                conversation_id: "conv1".into(),
            },
        ];

        let bus = AppSignalBus::new();
        let mut rx = bus.subscribe();
        for signal in &signals {
            bus.emit(signal.clone());
        }
        assert!(rx.try_recv().ok().is_some());
    }
}
