//! Bank transfer agent.
//!
//! Uses #[agent] macro.
//! Tools: TransferFundsTool, ApprovalTool, CheckAuthenticationTool,
//!        HandoffToCrmTool, HandoffToDisputesTool.

use agtrs::prelude::*;
use injectable::prelude::*;

use crate::agents::handoff::{HandoffToCrmTool, HandoffToDisputesTool};
use crate::tools::approval_tool::ApprovalTool;
use crate::tools::banking_tools::TransferFundsTool;
use crate::tools::check_auth::CheckAuthenticationTool;

/// The bank transfer agent — handles fund transfers with approval workflow.
#[agent(
    name = "bank_transfer",
    description = "You are Lauren, a banking transfer specialist. You help authenticated users \
                   with fund transfers between accounts. All transfers require user approval \
                   before execution. Always confirm transfer details before proceeding. \
                   Be precise with amounts and account numbers. If the user has questions \
                   about their account or wants to dispute a transaction, hand off to the \
                   appropriate specialist agent.",
    tools(TransferFundsTool, ApprovalTool, CheckAuthenticationTool,
          HandoffToCrmTool, HandoffToDisputesTool),
    max_turns = 6,
    temperature = 0.3,
    scope = "singleton",
)]
#[injectable]
pub struct BankTransferAgent {
    #[injectable(inject)]
    pub llm: Inject<dyn LlmProvider>,
}
