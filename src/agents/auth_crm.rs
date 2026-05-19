//! Authenticated CRM agent.
//!
//! Uses #[agent] macro.
//! Tools: GetBalanceTool, GetTransactionHistoryTool, CheckAuthenticationTool,
//!        HandoffToTransferTool, HandoffToDisputesTool.

use agtrs::prelude::*;
use injectable::prelude::*;

use crate::tools::banking_tools::{GetBalanceTool, GetTransactionHistoryTool};
use crate::tools::check_auth::CheckAuthenticationTool;
use crate::tools::handoff::{HandoffToDisputesTool, HandoffToTransferTool};

/// The authenticated CRM agent — handles account inquiries for verified users.
#[agent(
    name = "Banking CRM Agent (Authenticated)",
    description = "You are Lauren, an authenticated banking assistant. You help verified users \
                   with account inquiries, balance checks, and transaction history. You can also \
                   hand off to specialized agents for transfers and disputes. \
                   Always verify the user's identity before providing account information. \
                   Be professional, accurate, and helpful.",
    tools(
        GetBalanceTool,
        GetTransactionHistoryTool,
        CheckAuthenticationTool,
        HandoffToTransferTool,
        HandoffToDisputesTool
    ),
    max_turns = 8,
    temperature = 0.7,
    scope = "singleton"
)]
#[injectable]
pub struct AuthenticatedCrmAgent {
    #[injectable(inject)]
    pub llm: Inject<dyn LlmProvider>,
}
