//! Tool hooks that leverage Axum's HTTP request extensions.
//!
//! Three hooks demonstrate the pattern:
//!
//! 1. [`AuthRequiredHook`] — stateless; rejects calls when no `UserIdExtension` is
//!    present in `ToolContext.extensions`. Populated by the Axum request handler.
//!
//! 2. [`IdorGuardHook`] — stateless; prevents Insecure Direct Object Reference
//!    attacks. It reads the authenticated user ID from `extensions` and compares it
//!    with the `user_id` field the LLM placed in the tool input, rejecting any
//!    mismatch before the tool body ever runs.
//!
//! 3. [`AuditLogHook`] — an `#[injectable]` unit struct. Emits `ToolCallStarted` /
//!    `ToolCallComplete` signals by resolving `AppSignalBus` at call time from
//!    `ctx.agent_context.resolve_context()`. Because it has no fields it is
//!    constructable in-place and can live in the `hooks(...)` macro attribute:
//!
//! ```ignore
//! #[tool(name = "get_balance", hooks(
//!     crate::tools::hooks::AuthRequiredHook,
//!     crate::tools::hooks::IdorGuardHook { user_id_field: "user_id" },
//!     crate::tools::hooks::AuditLogHook,
//! ))]
//! impl GetBalanceTool { … }
//! ```

use std::sync::Arc;
use std::time::Instant;

use agtrs::prelude::*;
use async_trait::async_trait;
use injectable::prelude::*;

use crate::error::UserIdExtension;
use crate::signals::bus::{AppSignal, AppSignalBus};

// ── helpers ───────────────────────────────────────────────────────────────────

fn auth_uid(ctx: &ToolCallContext) -> Option<String> {
    ctx.tool_context
        .extensions
        .get::<UserIdExtension>()
        .map(|e| e.0.clone())
        .filter(|s| !s.is_empty())
}

fn conv_id(ctx: &ToolCallContext) -> String {
    ctx.tool_context
        .state
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// ── AuthRequiredHook ──────────────────────────────────────────────────────────

/// Rejects any tool call that has no authenticated user in the Axum extensions.
///
/// Install this hook on every tool that requires a logged-in user so that the
/// check happens once, at the hook layer, rather than being duplicated inside
/// every tool body.
///
/// ```ignore
/// hooks(crate::tools::hooks::AuthRequiredHook)
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct AuthRequiredHook;

#[async_trait]
impl ToolHook for AuthRequiredHook {
    async fn before(&self, ctx: &ToolCallContext) -> Result<BeforeHookDecision, AgtrsError> {
        if auth_uid(ctx).is_none() {
            return Ok(BeforeHookDecision::ReturnEarly(ToolResult::error(
                "User not authenticated",
                &ctx.tool_context.tool_use_id,
            )));
        }
        Ok(BeforeHookDecision::Proceed {
            modified_input: None,
        })
    }

    async fn after(&self, _ctx: &ToolAfterContext) -> Result<AfterHookDecision, AgtrsError> {
        Ok(AfterHookDecision::Return {
            modified_result: None,
        })
    }
}

// ── IdorGuardHook ─────────────────────────────────────────────────────────────

/// IDOR protection: rejects a tool call when the `user_id` field in the LLM's
/// input does not match the authenticated user in Axum extensions.
///
/// The LLM may be tricked (prompt injection) into passing a different user's ID
/// as an argument. This hook catches that mismatch at the hook layer, before the
/// tool body runs and before any database query is issued.
///
/// `user_id_field` is the JSON input field name that carries the claimed user ID
/// (typically `"user_id"`).
///
/// ```ignore
/// hooks(crate::tools::hooks::IdorGuardHook { user_id_field: "user_id" })
/// ```
#[derive(Debug, Clone, Copy)]
pub struct IdorGuardHook {
    pub user_id_field: &'static str,
}

#[async_trait]
impl ToolHook for IdorGuardHook {
    async fn before(&self, ctx: &ToolCallContext) -> Result<BeforeHookDecision, AgtrsError> {
        let Some(auth) = auth_uid(ctx) else {
            return Ok(BeforeHookDecision::ReturnEarly(ToolResult::error(
                "User not authenticated",
                &ctx.tool_context.tool_use_id,
            )));
        };

        let input_uid = ctx
            .input
            .get(self.user_id_field)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !input_uid.is_empty() && input_uid != auth {
            return Ok(BeforeHookDecision::ReturnEarly(ToolResult::error(
                format!("Unauthorized: authenticated as '{auth}', cannot access '{input_uid}'"),
                &ctx.tool_context.tool_use_id,
            )));
        }

        Ok(BeforeHookDecision::Proceed {
            modified_input: None,
        })
    }

    async fn after(&self, _ctx: &ToolAfterContext) -> Result<AfterHookDecision, AgtrsError> {
        Ok(AfterHookDecision::Return {
            modified_result: None,
        })
    }
}

// ── AuditLogHook ──────────────────────────────────────────────────────────────

/// Emits `AppSignal::ToolCallStarted` / `AppSignal::ToolCallComplete` signals.
///
/// `AppSignalBus` is resolved at call time from `ctx.agent_context.resolve_context()`
/// via `extract::<Inject<AppSignalBus>>()`.  In production the agent context carries
/// the injectable container's `ResolveContext`, which has `AppSignalBus` cached as
/// a singleton — the same instance the WebSocket gateway subscribes to.  In tests
/// with `EmptySingletonStore`, a fresh isolated bus is created (signals are emitted
/// but not observed), keeping the hook side-effect-free in unit tests.
///
/// Because this is a unit struct it can be placed directly in the `#[tool]` macro's
/// `hooks(...)` attribute alongside the stateless hooks:
///
/// ```ignore
/// #[tool(name = "get_balance", hooks(
///     crate::tools::hooks::AuthRequiredHook,
///     crate::tools::hooks::IdorGuardHook { user_id_field: "user_id" },
///     crate::tools::hooks::AuditLogHook,
/// ))]
/// impl GetBalanceTool { … }
/// ```
#[injectable]
#[derive(Debug, Clone, Copy, Default)]
pub struct AuditLogHook;

#[async_trait]
impl ToolHook for AuditLogHook {
    async fn before(&self, ctx: &ToolCallContext) -> Result<BeforeHookDecision, AgtrsError> {
        let conversation_id = conv_id(ctx);
        if let Ok(bus) = ctx
            .agent_context
            .resolve_context()
            .extract::<Inject<AppSignalBus>>()
            .await
        {
            bus.emit(AppSignal::ToolCallStarted {
                tool_name: ctx.tool_name.clone(),
                tool_use_id: ctx.call_id.to_string(),
                conversation_id,
            });
        }
        Ok(BeforeHookDecision::Proceed {
            modified_input: None,
        })
    }

    async fn after(&self, ctx: &ToolAfterContext) -> Result<AfterHookDecision, AgtrsError> {
        let conversation_id = ctx
            .call
            .tool_context
            .state
            .get("conversation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let duration_ms = ctx.duration.as_millis() as u64;
        let (success, error) = match &ctx.result {
            ToolExecutionStatus::Success(_) => (true, None),
            ToolExecutionStatus::Error(e) => (false, Some(e.clone())),
        };
        if let Ok(bus) = ctx
            .call
            .agent_context
            .resolve_context()
            .extract::<Inject<AppSignalBus>>()
            .await
        {
            bus.emit(AppSignal::ToolCallComplete {
                tool_name: ctx.call.tool_name.clone(),
                tool_use_id: ctx.call.call_id.to_string(),
                duration_ms,
                success,
                error,
                conversation_id,
            });
        }
        Ok(AfterHookDecision::Return {
            modified_result: None,
        })
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agtrs_runtime::agent::{AgentConfig, AgentContext};
    use injectable_runtime::{EmptySingletonStore, ResolveContext};
    use serde_json::json;
    use std::sync::Arc;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn make_ctx_with_user(user_id: &str) -> ToolContext {
        let mut ctx = ToolContext::new("test-id");
        if !user_id.is_empty() {
            ctx.extensions.insert(UserIdExtension(user_id.to_string()));
        }
        ctx.state
            .insert("conversation_id".into(), json!("conv-test"));
        ctx
    }

    fn make_agent_ctx() -> AgentContext {
        let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
        AgentContext::new(
            "test-agent",
            AgentConfig::default(),
            Arc::new(agtrs_runtime::testing::MockLlmProvider::new(Arc::new(
                agtrs_runtime::testing::MockTransport::new(),
            ))),
            resolve_ctx,
        )
    }

    fn tool_call_ctx(
        tool_name: &str,
        input: serde_json::Value,
        tool_ctx: ToolContext,
        agent_ctx: AgentContext,
    ) -> ToolCallContext {
        ToolCallContext {
            tool_name: tool_name.to_string(),
            input,
            agent_context: agent_ctx,
            tool_context: tool_ctx,
            started_at: Instant::now(),
            call_id: uuid::Uuid::new_v4(),
        }
    }

    // ── AuthRequiredHook ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn auth_hook_passes_when_user_authenticated() {
        let hook = AuthRequiredHook;
        let ctx = tool_call_ctx(
            "test",
            json!({}),
            make_ctx_with_user("alice"),
            make_agent_ctx(),
        );
        let decision = hook.before(&ctx).await.unwrap();
        assert!(
            matches!(decision, BeforeHookDecision::Proceed { .. }),
            "should proceed when user authenticated"
        );
    }

    #[tokio::test]
    async fn auth_hook_blocks_when_no_user_extension() {
        let hook = AuthRequiredHook;
        let ctx = tool_call_ctx("test", json!({}), make_ctx_with_user(""), make_agent_ctx());
        let decision = hook.before(&ctx).await.unwrap();
        match decision {
            BeforeHookDecision::ReturnEarly(result) => {
                assert!(result.is_error);
                assert!(result.content.contains("not authenticated"));
            }
            other => panic!("expected ReturnEarly, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn auth_hook_after_always_passes() {
        let hook = AuthRequiredHook;
        let tool_ctx = make_ctx_with_user("alice");
        let call_ctx = tool_call_ctx("test", json!({}), tool_ctx, make_agent_ctx());
        let after_ctx = ToolAfterContext {
            call: call_ctx,
            result: ToolExecutionStatus::Success(ToolResult::ok("ok", "id")),
            duration: std::time::Duration::from_millis(5),
        };
        let decision = hook.after(&after_ctx).await.unwrap();
        assert!(matches!(
            decision,
            AfterHookDecision::Return {
                modified_result: None
            }
        ));
    }

    // ── IdorGuardHook ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn idor_hook_passes_when_user_ids_match() {
        let hook = IdorGuardHook {
            user_id_field: "user_id",
        };
        let ctx = tool_call_ctx(
            "get_balance",
            json!({"user_id": "alice"}),
            make_ctx_with_user("alice"),
            make_agent_ctx(),
        );
        let decision = hook.before(&ctx).await.unwrap();
        assert!(
            matches!(decision, BeforeHookDecision::Proceed { .. }),
            "authenticated user accessing own data should proceed"
        );
    }

    #[tokio::test]
    async fn idor_hook_blocks_when_user_ids_mismatch() {
        let hook = IdorGuardHook {
            user_id_field: "user_id",
        };
        let ctx = tool_call_ctx(
            "get_balance",
            json!({"user_id": "bob"}),   // LLM claims bob
            make_ctx_with_user("alice"), // Axum says alice
            make_agent_ctx(),
        );
        let decision = hook.before(&ctx).await.unwrap();
        match decision {
            BeforeHookDecision::ReturnEarly(result) => {
                assert!(result.is_error);
                assert!(
                    result.content.contains("Unauthorized"),
                    "should say Unauthorized: {}",
                    result.content
                );
                assert!(result.content.contains("alice"));
                assert!(result.content.contains("bob"));
            }
            other => panic!("expected ReturnEarly, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn idor_hook_blocks_when_no_extension() {
        let hook = IdorGuardHook {
            user_id_field: "user_id",
        };
        let ctx = tool_call_ctx(
            "get_balance",
            json!({"user_id": "alice"}),
            make_ctx_with_user(""), // no extension
            make_agent_ctx(),
        );
        let decision = hook.before(&ctx).await.unwrap();
        match decision {
            BeforeHookDecision::ReturnEarly(result) => {
                assert!(result.is_error);
                assert!(result.content.contains("not authenticated"));
            }
            other => panic!("expected ReturnEarly, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn idor_hook_passes_when_input_user_id_absent() {
        // Tool doesn't pass user_id (no field to compare) — hook lets it through.
        let hook = IdorGuardHook {
            user_id_field: "user_id",
        };
        let ctx = tool_call_ctx(
            "some_tool",
            json!({"other_field": "value"}), // no user_id field
            make_ctx_with_user("alice"),
            make_agent_ctx(),
        );
        let decision = hook.before(&ctx).await.unwrap();
        assert!(matches!(decision, BeforeHookDecision::Proceed { .. }));
    }

    // ── AuditLogHook ─────────────────────────────────────────────────────────
    //
    // AuditLogHook is a unit struct that resolves AppSignalBus at call time via
    // ctx.agent_context.resolve_context().extract::<Inject<AppSignalBus>>().
    //
    // To observe emitted signals in tests, pre-extract AppSignalBus on the shared
    // ResolveContext before running the hook.  The singleton cache is Arc-shared
    // between the test setup and the AgentContext, so both see the same instance.

    fn make_agent_ctx_with_resolve(resolve_ctx: Arc<ResolveContext>) -> AgentContext {
        AgentContext::new(
            "test-agent",
            AgentConfig::default(),
            Arc::new(agtrs_runtime::testing::MockLlmProvider::new(Arc::new(
                agtrs_runtime::testing::MockTransport::new(),
            ))),
            resolve_ctx,
        )
    }

    #[tokio::test]
    async fn audit_hook_emits_tool_call_started_with_user_identity() {
        let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
        // Pre-extract to populate singleton cache; subscribe to observe signals.
        let bus = resolve_ctx.extract::<Inject<AppSignalBus>>().await.unwrap();
        let mut rx = bus.subscribe();

        let hook = AuditLogHook;
        let ctx = tool_call_ctx(
            "get_balance",
            json!({"user_id": "alice"}),
            make_ctx_with_user("alice"),
            make_agent_ctx_with_resolve(Arc::clone(&resolve_ctx)),
        );
        hook.before(&ctx).await.unwrap();

        let signal = rx.try_recv().expect("should have emitted ToolCallStarted");
        match signal {
            AppSignal::ToolCallStarted {
                tool_name,
                conversation_id,
                ..
            } => {
                assert_eq!(tool_name, "get_balance");
                assert_eq!(conversation_id, "conv-test");
            }
            other => panic!("expected ToolCallStarted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn audit_hook_emits_tool_call_complete_after_success() {
        let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
        let bus = resolve_ctx.extract::<Inject<AppSignalBus>>().await.unwrap();
        let mut rx = bus.subscribe();

        let hook = AuditLogHook;
        let call_ctx = tool_call_ctx(
            "get_balance",
            json!({"user_id": "alice"}),
            make_ctx_with_user("alice"),
            make_agent_ctx_with_resolve(Arc::clone(&resolve_ctx)),
        );
        let after_ctx = ToolAfterContext {
            call: call_ctx,
            result: ToolExecutionStatus::Success(ToolResult::ok("$5000.00", "id")),
            duration: std::time::Duration::from_millis(42),
        };
        hook.after(&after_ctx).await.unwrap();

        let signal = rx.try_recv().expect("should have emitted ToolCallComplete");
        match signal {
            AppSignal::ToolCallComplete {
                tool_name,
                duration_ms,
                success,
                error,
                conversation_id,
                ..
            } => {
                assert_eq!(tool_name, "get_balance");
                assert_eq!(duration_ms, 42);
                assert!(success);
                assert!(error.is_none());
                assert_eq!(conversation_id, "conv-test");
            }
            other => panic!("expected ToolCallComplete, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn audit_hook_emits_failure_signal_on_tool_error() {
        let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
        let bus = resolve_ctx.extract::<Inject<AppSignalBus>>().await.unwrap();
        let mut rx = bus.subscribe();

        let hook = AuditLogHook;
        let call_ctx = tool_call_ctx(
            "transfer_funds",
            json!({}),
            make_ctx_with_user("alice"),
            make_agent_ctx_with_resolve(Arc::clone(&resolve_ctx)),
        );
        let after_ctx = ToolAfterContext {
            call: call_ctx,
            result: ToolExecutionStatus::Error("transfer failed".into()),
            duration: std::time::Duration::from_millis(10),
        };
        hook.after(&after_ctx).await.unwrap();

        let signal = rx.try_recv().unwrap();
        match signal {
            AppSignal::ToolCallComplete { success, error, .. } => {
                assert!(!success);
                assert_eq!(error.as_deref(), Some("transfer failed"));
            }
            other => panic!("expected ToolCallComplete, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn audit_hook_records_correct_conversation_id_regardless_of_user() {
        let resolve_ctx = Arc::new(ResolveContext::from_store(Arc::new(EmptySingletonStore)));
        let bus = resolve_ctx.extract::<Inject<AppSignalBus>>().await.unwrap();
        let mut rx = bus.subscribe();

        let hook = AuditLogHook;
        let ctx = tool_call_ctx(
            "search",
            json!({}),
            make_ctx_with_user(""), // no Axum extension
            make_agent_ctx_with_resolve(Arc::clone(&resolve_ctx)),
        );
        hook.before(&ctx).await.unwrap();

        let signal = rx.try_recv().unwrap();
        match signal {
            AppSignal::ToolCallStarted {
                conversation_id, ..
            } => {
                assert_eq!(conversation_id, "conv-test");
            }
            other => panic!("expected ToolCallStarted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn audit_hook_proceeds_silently_in_empty_context() {
        // With a fresh context that has no pre-cached bus, the hook creates an
        // isolated AppSignalBus (signals go nowhere) and still proceeds.
        let hook = AuditLogHook;
        let ctx = tool_call_ctx(
            "get_balance",
            json!({}),
            make_ctx_with_user("alice"),
            make_agent_ctx(), // fresh context, no pre-cached bus
        );
        let result = hook.before(&ctx).await;
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            BeforeHookDecision::Proceed { .. }
        ));
    }

    // ── Hook composition: AuthRequired + IdorGuard run in sequence ────────────

    #[tokio::test]
    async fn composed_hooks_both_pass_for_correct_user() {
        let agent_ctx = make_agent_ctx();
        let tool_ctx = make_ctx_with_user("alice");

        // Simulate execute_with_hooks hook pipeline manually
        let auth = AuthRequiredHook;
        let idor = IdorGuardHook {
            user_id_field: "user_id",
        };
        let input = json!({"user_id": "alice"});

        let call_ctx = tool_call_ctx(
            "get_balance",
            input.clone(),
            tool_ctx.clone(),
            agent_ctx.clone(),
        );
        let d1 = auth.before(&call_ctx).await.unwrap();
        assert!(matches!(d1, BeforeHookDecision::Proceed { .. }));

        let call_ctx2 = tool_call_ctx("get_balance", input, tool_ctx, agent_ctx);
        let d2 = idor.before(&call_ctx2).await.unwrap();
        assert!(matches!(d2, BeforeHookDecision::Proceed { .. }));
    }

    #[tokio::test]
    async fn auth_hook_short_circuits_before_idor_check() {
        // AuthRequired fires first; IDOR would never run for unauthenticated requests.
        let hook = AuthRequiredHook;
        let ctx = tool_call_ctx(
            "get_balance",
            json!({"user_id": "alice"}),
            make_ctx_with_user(""),
            make_agent_ctx(),
        );
        match hook.before(&ctx).await.unwrap() {
            BeforeHookDecision::ReturnEarly(r) => {
                assert!(r.content.contains("not authenticated"));
            }
            other => panic!("auth hook should short-circuit: {other:?}"),
        }
    }
}
