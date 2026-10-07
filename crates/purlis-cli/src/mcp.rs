//! `charter mcp`: charter's MCP server over stdio (HP-7, Q15), the one action channel every
//! harness reaches the same way. Each harness adapter starts it for one chat
//! ([`purlis_core::chattools`] has how, and what each tool may do); nobody types it.
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

use purlis_core::chattools;

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
                "purlis's tools for this chat: the todos, memory, session records and changes \
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
                format!("purlis has no tool {name:?}"),
                None,
            ));
        }
        let args = request.arguments.clone().unwrap_or_default();
        if name == chattools::ASK_OPERATOR {
            return ask_operator(&request, &args, &context).await;
        }
        if name == chattools::SESSION_RECORD {
            return Ok(done(blocking(move || session_record(&args)).await));
        }
        if name == chattools::PERSONA_REMEMBER {
            return Ok(done(blocking(move || persona_remember(&args)).await));
        }
        if name == chattools::PERSONA_WHERE {
            return Ok(done(blocking(crate::whereworking::tool).await));
        }
        if name == chattools::DISPATCH {
            return Ok(done(blocking(move || dispatch(&args)).await));
        }
        if name == chattools::DISPATCH_REPORT {
            return Ok(done(blocking(move || dispatch_report(&args)).await));
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
        .unwrap_or_else(|e| Err(format!("the tool failed inside purlis: {e}")))
}

/// `session_record`: the one operation `purlis session record` run in the chat performs
/// ([`crate::session::write`]), so the app that started the chat writes it over the chat's hook
/// socket where it can, and this server writes it where it cannot (#1332). Answered with what
/// the command prints: the record, then what becomes of the tab.
fn session_record(args: &serde_json::Map<String, serde_json::Value>) -> Result<String, String> {
    let (title, body, pieces) = chattools::record_args(args)?;
    let here = crate::Here::read()?;
    // Outside the sandbox: in a chat the app started, never written here (#1408).
    purlis_core::sessionrecord::check(&title, &body)
        .map_err(|why| format!("nothing was written: {why}"))?;
    match crate::session::write(&here, &title, &body, &pieces, None, None, true) {
        Ok(saved) => {
            let mut said = vec![format!("Session record → {}", saved.shown)];
            said.extend(saved.warnings);
            said.push(saved.tab);
            Ok(said.join("\n"))
        }
        Err(crate::session::NotWritten::Refused(why) | crate::session::NotWritten::Failed(why)) => {
            Err(format!("nothing was written: {why}"))
        }
    }
}

/// `persona_remember`: the one operation `purlis persona remember` run in the chat performs, so
/// the app that started the chat writes it (a brokered write, #1333). Answered with what the
/// command says.
///
/// **This server runs outside the chat's sandbox, so in a chat the app started it never writes
/// itself.** It writes only where there is no app that could have: no chat, or an app that has
/// gone. A harness that hands it no connection to the app (Codex), an app that dropped the ask,
/// or one that did not answer, is refused in a sentence: a write made here would be one no
/// sandbox bounds, no rate holds and the trace does not credit.
fn persona_remember(args: &serde_json::Map<String, serde_json::Value>) -> Result<String, String> {
    use crate::brokered::Forwarded;
    let (text, title, shared) = chattools::persona_remember_args(args)?;
    let write = purlis_core::brokered::Write::PersonaRemember {
        text: text.clone(),
        title: title.clone(),
        shared,
    };
    write.check()?;
    let said = |path: &str| {
        format!(
            "Remembered ({}persistent) → {path}",
            if shared { "shared " } else { "" }
        )
    };
    match crate::brokered::forwarded(write) {
        Forwarded::Written { path, .. } => Ok(said(&path)),
        Forwarded::Refused(why) | Forwarded::Unsure(why) => {
            Err(format!("nothing was written: {why}"))
        }
        Forwarded::NotTaken(not) if !not.may_write_outside() => Err(format!(
            "nothing was written: {}",
            not.refusal("purlis persona remember")
        )),
        Forwarded::NotTaken(_) => {
            let here = crate::Here::read()?;
            let root = here.plane.root();
            if let purlis_core::compat::Compat::ReadOnly(why) = purlis_core::compat::read(root) {
                return Err(format!("nothing was written: {why}"));
            }
            let owner = if shared {
                purlis_core::contain::SHARED_PERSONA.to_owned()
            } else {
                here.active_persona(None).ok_or_else(|| {
                    "this chat runs as no persona, so it has no persona memory of its own: \
                     remember it with shared, or in its workspace with memory_add"
                        .to_owned()
                })?
            };
            let path = purlis_core::brokered::remember_persona(
                root,
                &owner,
                &text,
                title.as_deref(),
                chrono::Local::now().naive_local(),
            )?;
            Ok(said(&crate::voice::rel(root, &path)))
        }
    }
}

/// `dispatch`: the one operation `purlis dispatch` run in the chat performs
/// ([`crate::dispatch::send`]), so the app that started the chat decides and starts the persona
/// chat (#1436). Answered with what the command prints; a refusal is a tool error.
///
/// **Only the app starts a chat**, so where this server has no connection to it (under Codex,
/// which hands it only [`chattools::SCOPE_ENV`]) nothing is started, and the answer says to run
/// the command in the chat.
fn dispatch(args: &serde_json::Map<String, serde_json::Value>) -> Result<String, String> {
    let (to, name, brief) = chattools::dispatch_args(args)?;
    let here = crate::Here::read()?;
    crate::dispatch::send(&here, to.as_deref(), &name, &brief).map_err(|why| in_the_chat(&why))
}

/// `dispatch_report`: the one operation `purlis dispatch report` performs
/// ([`crate::dispatch::report`]).
fn dispatch_report(args: &serde_json::Map<String, serde_json::Value>) -> Result<String, String> {
    let (outcome, text, changed) = chattools::dispatch_report_args(args)?;
    crate::dispatch::report(&outcome, &text, changed.as_deref()).map_err(|why| in_the_chat(&why))
}

/// `why`, with where to go when this server was handed no connection to the app.
fn dispatch_tools_have_no_connection() -> bool {
    purlis_core::envvar::var_os(purlis_core::hookwire::SOCKET_ENV)
        .filter(|socket| !socket.is_empty())
        .is_none()
}

fn in_the_chat(why: &str) -> String {
    if dispatch_tools_have_no_connection() {
        format!(
            "{why} This harness hands purlis's tools no connection to the app: run `purlis \
             dispatch` in the chat instead."
        )
    } else {
        why.to_owned()
    }
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
            .map_err(|e| format!("the harness answered in a shape purlis cannot read: {e}"))
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
        _ => Err("the harness answered in a way purlis does not know".to_owned()),
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
        assert!(why.contains("failed inside purlis"), "{why}");
    }

    #[test]
    fn a_tool_that_returns_is_answered_as_it_returned() {
        assert_eq!(
            run(blocking(|| Ok("done".to_owned()))),
            Ok("done".to_owned())
        );
    }
}
