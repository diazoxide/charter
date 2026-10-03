//! `fake-harness --acp`: a scripted ACP agent on stdio, for the tests of charter's ACP client
//! (HP-2). It speaks just enough of ACP v1 to run one session the same way every time.
//!
//! What it does with a prompt is chosen by the prompt's text:
//!
//! - `ask`: asks permission to run `rm -rf build`, then says which option it was given, and
//!   whether it was given it after a `session/cancel`;
//! - `withdraw`: asks permission, then withdraws the request with `$/cancel_request`;
//! - `foreign`: asks permission and reports text for another session's id;
//! - `die`: exits in the middle of the turn;
//! - `flood <n>`: sends `n` notifications charter has no handler for;
//! - `chunks <n>`: sends `n` pieces of reply;
//! - `big <n> <bytes>`: sends `n` pieces of reply, each `bytes` long;
//! - `plans <n> <bytes>`: sends `n` plans, each of 100 steps `bytes` long;
//! - `unread <shape> <n> <bytes>`: sends `n` lines of a shape charter answers, then never
//!   reads again;
//! - `bigask <bytes>`: asks permission for a command `bytes` long;
//! - `asks <n>`: asks `n` permissions at once, and says each answer that comes early;
//! - `env <name>`: says the variable's value in its environment, or `unset`;
//! - `long <n>`: writes one line of `n` bytes;
//! - `fs`: calls the client's `fs/read_text_file`, which charter does not offer, and says what
//!   it was answered;
//! - `wait`: works until the turn is cancelled;
//! - anything else: reports a plan, its usage and a tool call, then echoes the text.
//!
//! With `--acp-silent` it answers nothing at all, and with `--acp-escape` it first leaves a
//! process outside its group holding its stdout. Every message it is sent is appended to
//! `--acp-record`, one JSON object a line, so a test can read what the client said.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde_json::{Value, json};

/// How the agent is scripted.
pub struct Script {
    /// Where each message the client sends is appended.
    pub record: Option<PathBuf>,
    /// The protocol version it answers `initialize` with.
    pub version: u64,
    /// Whether `session/new` answers that a login is needed.
    pub login: bool,
    /// Whether it reads everything and answers nothing.
    pub silent: bool,
    /// Whether it first starts a process outside its own group that keeps its stdout open,
    /// and records that process's pid as `{"escaped": pid}`.
    pub escape: bool,
}

const SESSION: &str = "fake-1";

pub fn serve(script: &Script) -> Result<(), String> {
    if script.escape {
        escape(script)?;
    }
    let mut lines = io::stdin().lock().lines();
    let mut out = io::stdout().lock();
    while let Some(message) = next(&mut lines, script)? {
        if script.silent {
            continue;
        }
        let id = message["id"].clone();
        match message["method"].as_str().unwrap_or_default() {
            "initialize" => reply(
                &mut out,
                &id,
                json!({
                    "protocolVersion": script.version,
                    "agentCapabilities": {"loadSession": true, "promptCapabilities": {"image": true}},
                    "authMethods": [{"id": "fake-login", "name": "Log in to the fake"}],
                    "agentInfo": {"name": "fake-harness", "version": "0"},
                }),
            )?,
            "session/new" if script.login => send(
                &mut out,
                &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "Authentication required"}}),
            )?,
            "session/new" => reply(&mut out, &id, json!({"sessionId": SESSION}))?,
            "session/prompt" => {
                let text = message["params"]["prompt"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let stop = prompt(&mut lines, &mut out, script, &text)?;
                reply(&mut out, &id, json!({"stopReason": stop}))?;
            }
            _ => {}
        }
    }
    Ok(())
}

type Lines<'a> = io::Lines<io::StdinLock<'a>>;

/// The next message, recorded, or `None` at the end of input.
fn next(lines: &mut Lines<'_>, script: &Script) -> Result<Option<Value>, String> {
    let Some(line) = lines.next() else {
        return Ok(None);
    };
    let line = line.map_err(|err| format!("cannot read input: {err}"))?;
    let message: Value =
        serde_json::from_str(&line).map_err(|err| format!("not JSON: {line:?}: {err}"))?;
    if let Some(path) = &script.record {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| format!("cannot record to {}: {err}", path.display()))?;
        writeln!(file, "{message}").map_err(|err| format!("cannot record: {err}"))?;
    }
    Ok(Some(message))
}

/// One turn. Its stop reason.
fn prompt(
    lines: &mut Lines<'_>,
    out: &mut impl Write,
    script: &Script,
    text: &str,
) -> Result<&'static str, String> {
    match text {
        "ask" => {
            send(out, &permission(900, SESSION))?;
            let mut cancelled = false;
            let answer = loop {
                let Some(message) = next(lines, script)? else {
                    return Ok("cancelled");
                };
                if message["method"] == "session/cancel" {
                    cancelled = true;
                } else if message["id"] == 900 && message.get("method").is_none() {
                    break message;
                }
            };
            let outcome = &answer["result"]["outcome"];
            let said = match outcome["outcome"].as_str() {
                // ACP: once the client has sent `session/cancel`, every request it still owes
                // is answered `cancelled`. A selection after it is a client bug.
                Some("selected") if cancelled => format!(
                    "chose {} after cancel",
                    outcome["optionId"].as_str().unwrap_or("?")
                ),
                Some("selected") => {
                    format!("chose {}", outcome["optionId"].as_str().unwrap_or("?"))
                }
                Some(other) => other.to_owned(),
                None => format!("answered {answer}"),
            };
            chunk(out, &said)?;
            Ok(if cancelled { "cancelled" } else { "end_turn" })
        }
        "fs" => {
            send(
                out,
                &json!({"jsonrpc": "2.0", "id": 901, "method": "fs/read_text_file",
                        "params": {"sessionId": SESSION, "path": "/etc/hosts"}}),
            )?;
            let answer = loop {
                let Some(message) = next(lines, script)? else {
                    return Ok("cancelled");
                };
                if message["id"] == 901 && message.get("method").is_none() {
                    break message;
                }
            };
            chunk(out, &format!("refused {}", answer["error"]["code"]))?;
            Ok("end_turn")
        }
        "withdraw" => {
            send(out, &permission(902, SESSION))?;
            send(
                out,
                &json!({"jsonrpc": "2.0", "method": "$/cancel_request", "params": {"requestId": 902}}),
            )?;
            let answer = response_to(lines, script, 902)?;
            chunk(
                out,
                &format!("withdrew, answered {}", answer["error"]["code"]),
            )?;
            Ok("end_turn")
        }
        "foreign" => {
            send(out, &permission(903, "OTHER"))?;
            let answer = response_to(lines, script, 903)?;
            send(
                out,
                &json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "OTHER",
                        "update": {"sessionUpdate": "agent_message_chunk",
                                   "content": {"type": "text", "text": "spoofed"}}}}),
            )?;
            chunk(
                out,
                &format!("foreign answered {}", answer["error"]["code"]),
            )?;
            Ok("end_turn")
        }
        "die" => std::process::exit(0),
        _ if text.starts_with("flood ") => {
            // Notifications charter has no handler for, each naming a session: the kind the
            // SDK would keep for a handler that never comes.
            let count: usize = text["flood ".len()..].parse().unwrap_or(0);
            let padding = "x".repeat(10_000);
            for _ in 0..count {
                send(
                    out,
                    &json!({"jsonrpc": "2.0", "method": "x/unheard",
                            "params": {"sessionId": SESSION, "padding": padding}}),
                )?;
            }
            chunk(out, "flooded")?;
            Ok("end_turn")
        }
        _ if text.starts_with("big ") => {
            // `big <n> <bytes>`: `n` pieces of reply, each `bytes` long.
            let mut words = text["big ".len()..].split(' ');
            let count: usize = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let bytes: usize = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let piece = "z".repeat(bytes);
            for _ in 0..count {
                chunk(out, &piece)?;
            }
            Ok("end_turn")
        }
        _ if text.starts_with("plans ") => {
            // `plans <n> <bytes>`: `n` plans, each of 100 steps `bytes` long.
            let mut words = text["plans ".len()..].split(' ');
            let count: usize = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let bytes: usize = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let entries: Vec<Value> = (0..100)
                .map(|_| json!({"content": "p".repeat(bytes), "priority": "low", "status": "pending"}))
                .collect();
            let plan = json!({"sessionUpdate": "plan", "entries": entries});
            for _ in 0..count {
                update(out, plan.clone())?;
            }
            Ok("end_turn")
        }
        _ if text.starts_with("unread ") => {
            // `unread <shape> <n> <bytes>`: `n` lines of the shape, and never another read, so
            // whatever charter answers backs up behind a pipe nobody empties.
            let mut words = text["unread ".len()..].split(' ');
            let shape = words.next().unwrap_or_default().to_owned();
            let count: u64 = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let bytes: usize = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let big = "b".repeat(bytes);
            for at in 0..count {
                let line = match shape.as_str() {
                    // Not JSON: the SDK answers a parse error that echoes the line.
                    "garbage" => format!("{at}{big}"),
                    // Valid JSON, but no message: an answer with a null id.
                    "tiny" => "17".to_owned(),
                    // The same key twice: one reader keeps the first, another the last.
                    "dupkey" => {
                        format!(
                            r#"{{"jsonrpc":"2.0","id":"{at}-{big}","method":"x/y","id":"{at}-{big}"}}"#
                        )
                    }
                    "nullid" => {
                        format!(r#"{{"jsonrpc":"2.0","id":null,"method":"x/{big}","params":{{}}}}"#)
                    }
                    "badbatch" => {
                        format!(r#"[{{"jsonrpc":"2.0","id":"{at}-{big}","method":"x/y"}},5]"#)
                    }
                    "trailing" => {
                        format!(r#"{{"jsonrpc":"2.0","id":"{at}-{big}","method":"x/y"}} x"#)
                    }
                    // A request whose id is most of the line, echoed in its answer.
                    "hugeid" => format!(r#"{{"jsonrpc":"2.0","id":"{big}{at}","method":"x/y"}}"#),
                    other => return Err(format!("no shape {other:?}")),
                };
                writeln!(out, "{line}")
                    .and_then(|()| out.flush())
                    .map_err(|err| format!("cannot write output: {err}"))?;
            }
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        }
        _ if text.starts_with("bigask ") => {
            // `bigask <bytes>`: one permission request whose command is `bytes` long, and what
            // it was answered.
            let bytes: usize = text["bigask ".len()..].parse().unwrap_or(0);
            let mut ask = permission(904, SESSION);
            ask["params"]["toolCall"]["rawInput"]["command"] = json!("r".repeat(bytes));
            send(out, &ask)?;
            let answer = response_to(lines, script, 904)?;
            let outcome = answer["result"]["outcome"]["outcome"]
                .as_str()
                .unwrap_or("?")
                .to_owned();
            chunk(out, &format!("answered {outcome}"))?;
            Ok("end_turn")
        }
        _ if text.starts_with("asks ") => {
            // `asks <n>`: `n` permission requests at once, each for a call of its own. Each
            // answer that comes before a `session/cancel` is said as `early <outcome>`.
            let count: u64 = text["asks ".len()..].parse().unwrap_or(0);
            for at in 0..count {
                let mut ask = permission(2000 + at, SESSION);
                ask["params"]["toolCall"]["toolCallId"] = json!(format!("call-{at}"));
                send(out, &ask)?;
            }
            let mut answered = 0;
            let mut cancelled = false;
            while answered < count {
                let Some(message) = next(lines, script)? else {
                    return Ok("cancelled");
                };
                if message["method"] == "session/cancel" {
                    cancelled = true;
                } else if message.get("method").is_none()
                    && message["id"].as_u64().is_some_and(|id| id >= 2000)
                {
                    answered += 1;
                    if !cancelled {
                        let outcome = message["result"]["outcome"]["outcome"]
                            .as_str()
                            .unwrap_or("?")
                            .to_owned();
                        chunk(out, &format!("early {outcome}"))?;
                    }
                }
            }
            Ok("cancelled")
        }
        _ if text.starts_with("env ") => {
            let name = &text["env ".len()..];
            let value = std::env::var(name).unwrap_or_else(|_| "unset".to_owned());
            chunk(out, &format!("{name}={value}"))?;
            Ok("end_turn")
        }
        _ if text.starts_with("chunks ") => {
            let count: usize = text["chunks ".len()..].parse().unwrap_or(0);
            for at in 0..count {
                chunk(out, &format!("{at} "))?;
            }
            Ok("end_turn")
        }
        _ if text.starts_with("long ") => {
            // One line of this many bytes, with no newline until its end.
            let bytes: usize = text["long ".len()..].parse().unwrap_or(0);
            writeln!(out, "{}", "y".repeat(bytes))
                .and_then(|()| out.flush())
                .map_err(|err| format!("cannot write output: {err}"))?;
            Ok("end_turn")
        }
        "wait" => loop {
            let Some(message) = next(lines, script)? else {
                return Ok("cancelled");
            };
            if message["method"] == "session/cancel" {
                return Ok("cancelled");
            }
        },
        _ => {
            update(
                out,
                json!({"sessionUpdate": "plan", "entries": [
                    {"content": "Read the text", "priority": "high", "status": "completed"},
                    {"content": "Say it back", "priority": "medium", "status": "pending"},
                ]}),
            )?;
            update(
                out,
                json!({"sessionUpdate": "usage_update", "used": 50, "size": 200}),
            )?;
            update(
                out,
                json!({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Read a file",
                       "kind": "read", "status": "pending"}),
            )?;
            update(
                out,
                json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed"}),
            )?;
            chunk(out, &format!("you said: {text}"))?;
            Ok("end_turn")
        }
    }
}

/// `--acp-escape`: a `sleep` in a process group of its own, holding this agent's stdout, so
/// that ending the agent's group leaves the pipe open.
fn escape(script: &Script) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let escaped = std::process::Command::new("sleep")
        .arg("30")
        .stdin(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|err| format!("cannot start the escaping process: {err}"))?;
    if let Some(path) = &script.record {
        std::fs::write(path, format!("{{\"escaped\": {}}}\n", escaped.id()))
            .map_err(|err| format!("cannot record to {}: {err}", path.display()))?;
    }
    Ok(())
}

/// A permission request for `call-1`, as `id`, naming `session`.
fn permission(id: u64, session: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": {
        "sessionId": session,
        "toolCall": {"toolCallId": "call-1", "title": "Run rm -rf build", "kind": "execute",
                     "rawInput": {"command": "rm -rf build"}},
        "options": [
            {"optionId": "once", "name": "Allow once", "kind": "allow_once"},
            {"optionId": "no", "name": "Reject", "kind": "reject_once"},
        ],
    }})
}

/// The client's response to request `id`, skipping anything else it sends meanwhile.
fn response_to(lines: &mut Lines<'_>, script: &Script, id: u64) -> Result<Value, String> {
    loop {
        let Some(message) = next(lines, script)? else {
            return Ok(Value::Null);
        };
        if message["id"] == id && message.get("method").is_none() {
            return Ok(message);
        }
    }
}

fn chunk(out: &mut impl Write, text: &str) -> Result<(), String> {
    update(
        out,
        json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}),
    )
}

fn update(out: &mut impl Write, update: Value) -> Result<(), String> {
    send(
        out,
        &json!({"jsonrpc": "2.0", "method": "session/update",
                "params": {"sessionId": SESSION, "update": update}}),
    )
}

fn reply(out: &mut impl Write, id: &Value, result: Value) -> Result<(), String> {
    send(out, &json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn send(out: &mut impl Write, message: &Value) -> Result<(), String> {
    writeln!(out, "{message}")
        .and_then(|()| out.flush())
        .map_err(|err| format!("cannot write output: {err}"))
}
