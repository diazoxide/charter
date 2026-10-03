//! `charter mcp`: charter's MCP server over stdio (HP-7, Q15), the one action channel every
//! harness reaches the same way. Each harness adapter starts it for one chat
//! ([`charter_core::chattools`] has how, and what each tool may do); nobody types it.
//!
//! **stdout is the protocol.** Nothing here prints: a tool's answer is the text it returns,
//! and a line on stdout that is not JSON-RPC would end the connection.
//!
//! **The chat's place is read at every call**, by the same ladder a `charter` command run in the
//! chat reads ([`crate::Here`]), so a tool reaches exactly what `charter … ` with no `-w`
//! reaches, and a `charter workspace use` in the chat moves both together.

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ElicitRequestParams,
    ElicitResult, ElicitationAction, ElicitationSchema, Implementation, InputRequest,
    InputRequests, InputRequiredResult, ListToolsResult, PaginatedRequestParams, ProtocolVersion,
    ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData, ServerHandler, ServiceExt};

use charter_core::chattools;

/// The key an `ask_operator` answer comes back under, at protocol versions that ask by
/// returning `input_required` (SEP-2322).
const ANSWER: &str = "answer";

/// Serves the tools on stdin and stdout until the harness closes them.
pub fn serve() -> Result<u8, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("could not start the MCP server: {e}"))?;
    runtime.block_on(async {
        let running = Server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| format!("the MCP client did not start a session: {e}"))?;
        running
            .waiting()
            .await
            .map_err(|e| format!("the MCP session ended badly: {e}"))?;
        Ok(0)
    })
}

/// The server. It holds nothing: every call reads the chat's place afresh.
struct Server;

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                chattools::SERVER,
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "charter's tools for this chat: the todos, memory, session records and changes \
                 of the workspace it works in, and a question for the operator.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(
            chattools::TOOLS.iter().map(tool).collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.as_ref();
        if !chattools::TOOLS.iter().any(|t| t.name == name) {
            return Err(ErrorData::invalid_params(
                format!("charter has no tool {name:?}"),
                None,
            ));
        }
        let args = request.arguments.clone().unwrap_or_default();
        if name == chattools::ASK_OPERATOR {
            return ask_operator(&request, &args, &context).await;
        }
        let name = name.to_owned();
        Ok(done(
            blocking(move || {
                let here = crate::Here::read()?;
                let place = here.place(None);
                chattools::call(
                    here.plane.root(),
                    &place,
                    &name,
                    &args,
                    chrono::Local::now().naive_local(),
                )
            })
            .await,
        ))
    }
}

/// `body`, off the runtime's one thread, so its file I/O never stalls a question in flight,
/// and with a panic answered as a tool error rather than a call that never returns. (A release
/// build aborts on a panic, so there the server ends and the harness says so; it still never
/// hangs.)
async fn blocking(
    body: impl FnOnce() -> Result<String, String> + Send + 'static,
) -> Result<String, String> {
    tokio::task::spawn_blocking(body)
        .await
        .unwrap_or_else(|e| Err(format!("the tool failed inside charter: {e}")))
}

fn tool(spec: &chattools::Tool) -> Tool {
    let schema = match (spec.schema)() {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    let mut annotations = ToolAnnotations::new();
    annotations.read_only_hint = Some(spec.read_only);
    Tool::new(spec.name, spec.description, schema).with_annotations(annotations)
}

/// A tool's answer, or its refusal as an error the model reads.
fn done(answered: Result<String, String>) -> CallToolResponse {
    match answered {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(why) => CallToolResult::error(vec![ContentBlock::text(why)]),
    }
    .into()
}

/// The question, put to the person through the harness's own prompt: an elicitation, which the
/// harness shows to the person and the model never answers. From protocol 2026-07-28 it is asked
/// by returning `input_required` and read from the retried call (SEP-2322); before it, by a
/// request sent while the call is in flight.
async fn ask_operator(
    request: &CallToolRequestParams,
    args: &serde_json::Map<String, serde_json::Value>,
    context: &RequestContext<RoleServer>,
) -> Result<CallToolResponse, ErrorData> {
    let question = match chattools::question(args) {
        Ok(question) => question,
        Err(why) => return Ok(done(Err(why))),
    };
    let state = chattools::question_state(&question);
    if let Some(answer) = request
        .input_responses
        .as_ref()
        .and_then(|responses| responses.get(ANSWER))
    {
        // An answer is read back only for the question it answers.
        if request.request_state.as_deref() != Some(state.as_str()) {
            return Ok(done(Err(
                "this answer was given to another question, so it was not read; ask again"
                    .to_owned(),
            )));
        }
        let answered = serde_json::from_value::<ElicitResult>(answer.clone())
            .map_err(|e| format!("the harness answered in a shape charter cannot read: {e}"))
            .and_then(said);
        return Ok(done(answered));
    }
    let params = ElicitRequestParams::FormElicitationParams {
        meta: None,
        message: chattools::asked(&question),
        requested_schema: answer_schema(),
    };
    let asks_by_returning = context
        .protocol_version()
        .is_some_and(|v| v.as_str() >= ProtocolVersion::V_2026_07_28.as_str());
    // A 2026-07-28 harness says what it can do on each request; an older one said it once, at
    // `initialize`.
    let can_ask = if asks_by_returning {
        context
            .meta
            .client_capabilities()
            .and_then(|capabilities| capabilities.elicitation)
            .is_some_and(|elicitation| elicitation.form.is_some() || elicitation.url.is_none())
    } else {
        context
            .peer
            .supported_elicitation_modes()
            .contains(&rmcp::service::ElicitationMode::Form)
    };
    if !can_ask {
        return Ok(done(Err(
            "this harness cannot ask the operator through MCP; ask in your reply instead"
                .to_owned(),
        )));
    }
    if asks_by_returning {
        let mut requests = InputRequests::new();
        requests.insert(
            ANSWER.to_owned(),
            InputRequest::Elicitation(rmcp::model::ElicitRequest::new(params)),
        );
        return Ok(InputRequiredResult::new(Some(requests), Some(state)).into());
    }
    let answered = context
        .peer
        .create_elicitation_with_timeout(params, None)
        .await
        .map_err(|e| format!("the operator could not be asked: {e}"))
        .and_then(said);
    Ok(done(answered))
}

/// What the operator's answer says, as the chat reads it.
fn said(result: ElicitResult) -> Result<String, String> {
    match result.action {
        ElicitationAction::Accept => result
            .content
            .as_ref()
            .and_then(|content| content.get(ANSWER))
            .and_then(serde_json::Value::as_str)
            .map(|answer| format!("The operator answered: {answer}"))
            .ok_or_else(|| "the operator accepted but gave no answer".to_owned()),
        ElicitationAction::Decline => Ok("The operator declined to answer.".to_owned()),
        ElicitationAction::Cancel => Ok("The operator dismissed the question.".to_owned()),
        _ => Err("the harness answered in a way charter does not know".to_owned()),
    }
}

/// One line of text, the operator's answer.
fn answer_schema() -> ElicitationSchema {
    serde_json::from_value(serde_json::json!({
        "type": "object",
        "properties": { ANSWER: {
            "type": "string",
            "title": "Your answer",
            "description": chattools::NO_SECRET_HERE,
        } },
        "required": [ANSWER],
    }))
    .expect("a fixed schema")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(future)
    }

    #[test]
    fn a_tool_that_panics_answers_an_error_and_never_hangs() {
        let answered = run(blocking(|| panic!("a tool went wrong")));
        let why = answered.expect_err("an error");
        assert!(why.contains("failed inside charter"), "{why}");
    }

    #[test]
    fn a_tool_that_returns_is_answered_as_it_returned() {
        assert_eq!(
            run(blocking(|| Ok("done".to_owned()))),
            Ok("done".to_owned())
        );
    }
}
