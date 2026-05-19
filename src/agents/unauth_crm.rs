//! Unauthenticated CRM agent.
//!
//! Uses #[agent] macro — generates Agent trait impl (name, tools, config).
//! system_prompt is set via the description attribute.
//! Tools: CheckAuthenticationTool, HandoffToAuthenticatedCrmTool.

use agtrs::prelude::*;
use injectable::prelude::*;

use crate::tools::check_auth::CheckAuthenticationTool;
use crate::tools::handoff::HandoffToAuthenticatedCrmTool;
use crate::tools::knowledge_tool::SearchPublicInfoTool;

/// The unauthenticated CRM agent — handles public inquiries and authentication.
#[agent(
    name = "Banking CRM Agent (Public)",
    description = "You are Lauren, a friendly banking assistant for unauthenticated users. \
                   Your role is to help users with general banking questions and guide them \
                   through authentication. You can check if a user is authenticated and \
                   hand off to the authenticated CRM agent once they are verified. \
                   Always be helpful, professional, and security-conscious. \
                   Never provide account-specific information to unauthenticated users.",
    tools(
        CheckAuthenticationTool,
        HandoffToAuthenticatedCrmTool,
        SearchPublicInfoTool
    ),
    max_turns = 4,
    temperature = 0.7,
    scope = "singleton"
)]
#[injectable]
pub struct UnauthenticatedCrmAgent {
    #[injectable(inject)]
    pub llm: Inject<dyn LlmProvider>,
}
