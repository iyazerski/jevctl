mod transport;

use clap::{Args, Subcommand};
use rmcp::{
    ErrorData, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::{async_rw::AsyncRwTransport, stdio},
};

use crate::client::EvaluationClient;
use crate::error::AppError;
use crate::mcp::transport::PreInitTransport;
use crate::protocol::EvaluationRequest;
use crate::validation::validate_request;

const SERVER_INSTRUCTIONS: &str = "Use evaluate for fast semantic judgments that exact code cannot reliably make: intent, relevance, support, similarity, preference, or severity. Put concise shared evidence in context and batch independent questions. Use boolean for a yes-probability, select for one bounded option, and scale for an ordered degree. Compact output is the default; request full only when distributions matter. Do not use this for arithmetic, parsing, lookup, deterministic checks, execution, authorization, or prose generation. Results are advisory evidence and never replace tests or policy.";

#[derive(Debug, Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub command: McpCommand,
}

#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Serve MCP over stdin and stdout.
    Serve,
}

#[derive(Clone)]
struct JevctlMcp {
    client: EvaluationClient,
    #[allow(
        dead_code,
        reason = "rmcp reads this field from generated tool router code"
    )]
    tool_router: ToolRouter<Self>,
}

impl JevctlMcp {
    fn new(client: EvaluationClient) -> Self {
        Self {
            client,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for JevctlMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(SERVER_INSTRUCTIONS)
    }
}

#[tool_router]
impl JevctlMcp {
    #[tool(
        description = "Make fast semantic judgments over shared context when exact code cannot determine meaning. Batch narrow independent questions: boolean returns a yes-probability, select chooses a bounded option, and scale rates ordered levels. Use compact by default; full adds distributions. Do not use for deterministic checks, execution, authorization, or prose generation."
    )]
    async fn evaluate(
        &self,
        Parameters(request): Parameters<EvaluationRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        validate_request(&request)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;

        let detail = request.detail.unwrap_or_default();
        match self.client.evaluate(&request, detail).await {
            Ok(output) => {
                let text = output
                    .to_compact_json_string()
                    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                let structured = serde_json::to_value(&output)
                    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
                let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
                result.structured_content = Some(structured);
                Ok(result)
            }
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(
                error.to_string(),
            )])),
        }
    }
}

/// Run the requested MCP transport until the client closes it.
pub async fn run(args: McpArgs) -> Result<(), AppError> {
    match args.command {
        McpCommand::Serve => serve().await,
    }
}

async fn serve() -> Result<(), AppError> {
    let client = EvaluationClient::from_env()?;
    let (stdin, stdout) = stdio();
    let transport = PreInitTransport::new(AsyncRwTransport::new_server(stdin, stdout));
    let service = JevctlMcp::new(client)
        .serve(transport)
        .await
        .map_err(|error| AppError::Mcp(error.to_string()))?;
    service
        .waiting()
        .await
        .map_err(|error| AppError::Mcp(error.to_string()))?;
    Ok(())
}
