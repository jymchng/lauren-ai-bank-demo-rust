//! Disputes agent.
//!
//! Uses #[agent] macro.
//! Tools: GetBalanceTool, GetTransactionHistoryTool, CheckAuthenticationTool,
//!        HandoffToTransferTool, HandoffToCrmTool.

use agtrs::prelude::*;
use injectable::prelude::*;

use crate::agents::handoff::{HandoffToCrmTool, HandoffToTransferTool};
use crate::tools::banking_tools::{GetBalanceTool, GetTransactionHistoryTool};
use crate::tools::check_auth::CheckAuthenticationTool;

/// The disputes agent — handles transaction disputes and chargebacks.
#[agent(
    name = "disputes",
    description = "You are Lauren, a banking disputes specialist. You help authenticated users \
                   with transaction disputes and chargebacks. You can check account balances \
                   and transaction history to investigate disputed charges. Always gather \
                   all relevant information before initiating a dispute. If the user wants \
                   to make a transfer or has general account questions, hand off to the \
                   appropriate specialist agent.",
    tools(
        GetBalanceTool,
        GetTransactionHistoryTool,
        CheckAuthenticationTool,
        HandoffToCrmTool,
        HandoffToTransferTool
    ),
    max_turns = 6,
    temperature = 0.3,
    scope = "singleton"
)]
#[injectable]
pub struct DisputesAgent {
    #[injectable(inject)]
    pub llm: Inject<dyn LlmProvider>,
}
