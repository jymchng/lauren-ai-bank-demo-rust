use injectable::prelude::*;
use serde::Serialize;
use tokio::sync::broadcast;

/// Signal types for the application event bus.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AppSignal {
    /// Emitted when an LLM call completes.
    #[serde(rename = "token_usage")]
    ModelCallComplete {
        model: String,
        input_tokens: usize,
        output_tokens: usize,
        cost_usd: f64,
        duration_ms: u64,
        conversation_id: String,
    },
    /// Emitted when an agent turn completes (internal, not forwarded to frontend).
    AgentTurnComplete {
        agent_name: String,
        turn: usize,
        conversation_id: String,
    },
    /// Emitted when a tool call starts.
    #[serde(rename = "tool_started")]
    ToolCallStarted {
        tool_name: String,
        tool_use_id: String,
        conversation_id: String,
    },
    /// Emitted when a tool call completes.
    #[serde(rename = "tool_complete")]
    ToolCallComplete {
        tool_name: String,
        tool_use_id: String,
        duration_ms: u64,
        success: bool,
        error: Option<String>,
        conversation_id: String,
    },
    /// Emitted when a guardrail is triggered.
    #[serde(rename = "guardrail_triggered")]
    GuardrailTriggered {
        guardrail_name: String,
        agent_name: String,
        violation: String,
        passed: bool,
        conversation_id: String,
    },
    /// Emitted when a tool requires human approval (Python-compatible shape).
    #[serde(rename = "transfer_approval_request")]
    ToolPendingApproval {
        approval_id: String,
        from_user: String,
        to_user: String,
        amount_usd: f64,
        description: String,
        conversation_id: String,
        created_at_ms: u64,
    },
    /// Emitted when an agent run completes.
    #[serde(rename = "run_complete")]
    AgentRunComplete {
        agent_name: String,
        turns: usize,
        total_cost_usd: f64,
        conversation_id: String,
    },
    /// Emitted when an agent hands off to another agent.
    #[serde(rename = "agent_handoff")]
    AgentHandoff {
        from_agent: String,
        to_agent: String,
        summary: String,
        conversation_id: String,
    },
    /// Broadcast to all clients after a successful transfer.
    #[serde(rename = "balance_changed")]
    BalanceChanged {
        from_user: String,
        to_user: String,
        amount: f64,
        from_balance: f64,
        to_balance: f64,
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

    /// Clone the sender — lets callers register bridge closures that push into this bus.
    pub fn sender(&self) -> broadcast::Sender<AppSignal> {
        self.sender.clone()
    }
}

impl Default for AppSignalBus {
    fn default() -> Self {
        Self::new()
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_signal_bus_emit_and_receive() {
        let bus = AppSignalBus::new();
        let mut rx = bus.subscribe();

        bus.emit(AppSignal::AgentRunComplete {
            agent_name: "TestAgent".into(),
            turns: 1,
            total_cost_usd: 0.0,
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
            guardrail_name: "LlmScopeGuard".into(),
            agent_name: "Agent1".into(),
            violation: "".into(),
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
            input_tokens: 80,
            output_tokens: 20,
            cost_usd: 0.01,
            duration_ms: 500,
            conversation_id: "conv1".into(),
        };
        let json = serde_json::to_string(&signal).unwrap();
        assert!(json.contains("token_usage"));
    }

    #[test]
    fn test_all_signal_types() {
        let signals = vec![
            AppSignal::ModelCallComplete {
                model: "gpt-4".into(),
                input_tokens: 80,
                output_tokens: 20,
                cost_usd: 0.01,
                duration_ms: 500,
                conversation_id: "conv1".into(),
            },
            AppSignal::AgentTurnComplete {
                agent_name: "Agent1".into(),
                turn: 1,
                conversation_id: "conv1".into(),
            },
            AppSignal::ToolCallStarted {
                tool_name: "get_balance".into(),
                tool_use_id: "tu-1".into(),
                conversation_id: "conv1".into(),
            },
            AppSignal::ToolCallComplete {
                tool_name: "get_balance".into(),
                tool_use_id: "tu-1".into(),
                duration_ms: 50,
                success: true,
                error: None,
                conversation_id: "conv1".into(),
            },
            AppSignal::GuardrailTriggered {
                guardrail_name: "LlmScopeGuard".into(),
                agent_name: "Agent1".into(),
                violation: "".into(),
                passed: true,
                conversation_id: "conv1".into(),
            },
            AppSignal::ToolPendingApproval {
                approval_id: "conv1".into(),
                from_user: "alice".into(),
                to_user: "bob".into(),
                amount_usd: 100.0,
                description: "Transfer to bob".into(),
                conversation_id: "conv1".into(),
                created_at_ms: 0,
            },
            AppSignal::BalanceChanged {
                from_user: "alice".into(),
                to_user: "bob".into(),
                amount: 100.0,
                from_balance: 4900.0,
                to_balance: 3300.0,
            },
            AppSignal::AgentRunComplete {
                agent_name: "Agent1".into(),
                turns: 2,
                total_cost_usd: 0.0,
                conversation_id: "conv1".into(),
            },
            AppSignal::AgentHandoff {
                from_agent: "Banking CRM Agent (Authenticated)".into(),
                to_agent: "Banking Transfer Agent".into(),
                summary: "User wants to transfer $100".into(),
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
