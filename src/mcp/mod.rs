use std::collections::BTreeMap;

use clap::{Args, Subcommand};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ClientJsonRpcMessage, ClientNotification, ClientRequest, ContentBlock,
        ErrorCode, ServerCapabilities, ServerInfo, ServerJsonRpcMessage,
    },
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    tool, tool_handler, tool_router,
    transport::{Transport, async_rw::AsyncRwTransport, stdio},
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::client::EvaluationClient;
use crate::error::AppError;
use crate::protocol::{EvaluationRequest, OutputDetail, Question};
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

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Detail {
    #[default]
    Compact,
    Full,
}

impl From<Detail> for OutputDetail {
    fn from(value: Detail) -> Self {
        match value {
            Detail::Compact => Self::Compact,
            Detail::Full => Self::Full,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EvaluateRequest {
    /// Concise evidence shared by all questions. Strings, objects, arrays, and scalar JSON are accepted.
    context: Value,
    /// Independent judgments to run together. Use boolean for yes-probability, select for one bounded option, or scale for ordered degree.
    questions: BTreeMap<String, Question>,
    /// Compact returns scalar answers; full adds normalized probability distributions.
    #[serde(default)]
    detail: Detail,
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
        Parameters(input): Parameters<EvaluateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let request = EvaluationRequest {
            context: input.context,
            questions: input.questions,
        };
        validate_request(&request)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;

        match self.client.evaluate(&request, input.detail.into()).await {
            Ok(output) => {
                let text = serde_json::to_string(&output.compact())
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

struct PreInitTransport<T> {
    inner: T,
    initialized: bool,
}

impl<T> PreInitTransport<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            initialized: false,
        }
    }
}

impl<T> Transport<RoleServer> for PreInitTransport<T>
where
    T: Transport<RoleServer>,
{
    type Error = T::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner.send(item)
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        loop {
            let message = self.inner.receive().await?;
            if self.initialized {
                return Some(message);
            }
            match message {
                ClientJsonRpcMessage::Request(request)
                    if matches!(&request.request, ClientRequest::InitializeRequest(_)) =>
                {
                    self.initialized = true;
                    return Some(ClientJsonRpcMessage::Request(request));
                }
                ClientJsonRpcMessage::Request(request)
                    if matches!(&request.request, ClientRequest::CustomRequest(_)) =>
                {
                    let error =
                        ErrorData::new(ErrorCode::METHOD_NOT_FOUND, "Method not found", None);
                    if self
                        .inner
                        .send(ServerJsonRpcMessage::error(error, Some(request.id)))
                        .await
                        .is_err()
                    {
                        return None;
                    }
                }
                ClientJsonRpcMessage::Notification(notification)
                    if matches!(
                        notification.notification,
                        ClientNotification::CustomNotification(_)
                    ) => {}
                other => return Some(other),
            }
        }
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}
