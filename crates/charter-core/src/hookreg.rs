//! Every Claude Code hook the Rust `charter` answers: its word, the event it is wired to, and
//! the tool matcher on that event — one list, so that whatever wires the hooks (a plugin's
//! `hooks.json`, the app's per-session settings) is generated from the binary that answers
//! them rather than kept in step with it by hand.
//!
//! `charter hook --list --json` prints it. **The shape is a contract** — another build step
//! reads it — so it only ever GROWS: a field is never renamed or removed, and a new field is
//! one a reader that ignores it loses nothing by ignoring.
//!
//! ```json
//! {"schema": 1, "handlers": [
//!   {"name": "sessionstart", "event": "SessionStart", "matcher": null, "timeout": 5,
//!    "args": ["hook", "sessionstart"]},
//!   {"name": "pretooluse", "event": "PreToolUse", "matcher": "Bash", "timeout": 10,
//!    "args": ["hook", "pretooluse"]}
//! ]}
//! ```
//!
//! - `matcher` is the harness's tool matcher — a `|`-separated alternation of tool names — or
//!   `null` for an event that has no tool.
//! - `timeout` is seconds, what the Python charter's own plugin gave the same word.
//! - `args` is what to run after the `charter` binary's own path.
//!
//! # What is answered and NOT listed
//!
//! [`NO_OPS`] are words an older plugin wires that this binary answers with exit 0 and nothing
//! else, so a plugin that still names them can never block or slow a tool call on them. They
//! are left off the list because a generated plugin wiring them would start a process per
//! tool call for nothing. The three internal subcommands the Python plugin also wires —
//! `workspace _reconcile`, `persona _gc`, `workspace _autosave` — are answered the same way and
//! are not hooks of this binary at all.

/// One hook this binary answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handler {
    /// The word after `charter hook`.
    pub name: &'static str,
    /// The harness event it is wired to, as Claude Code spells it.
    pub event: &'static str,
    /// The tool matcher on that event, or `None` for an event with no tool.
    pub matcher: Option<&'static str>,
    /// Seconds.
    pub timeout: u32,
}

/// Every hook the Rust `charter` answers, in the order a session meets them.
pub const HANDLERS: [Handler; 14] = [
    Handler {
        name: "sessionstart",
        event: "SessionStart",
        matcher: None,
        timeout: 5,
    },
    Handler {
        name: "userpromptsubmit",
        event: "UserPromptSubmit",
        matcher: None,
        timeout: 5,
    },
    Handler {
        name: "pretooluse",
        event: "PreToolUse",
        matcher: Some("Bash"),
        timeout: 10,
    },
    Handler {
        name: "pretooluse-read",
        event: "PreToolUse",
        matcher: Some("Read|Grep"),
        timeout: 5,
    },
    Handler {
        name: "pretooluse-edit",
        event: "PreToolUse",
        matcher: Some("Write|Edit|MultiEdit"),
        timeout: 5,
    },
    Handler {
        name: "pretooluse-dispatch",
        event: "PreToolUse",
        matcher: Some("Task|Agent"),
        timeout: 5,
    },
    Handler {
        name: "posttooluse",
        event: "PostToolUse",
        matcher: Some("Write|Edit|MultiEdit"),
        timeout: 5,
    },
    Handler {
        name: "posttooluse-skill",
        event: "PostToolUse",
        matcher: Some("Skill"),
        timeout: 5,
    },
    Handler {
        name: "posttooluse-dispatch",
        event: "PostToolUse",
        matcher: Some("Task|Agent"),
        timeout: 5,
    },
    Handler {
        name: "posttooluse-message",
        event: "PostToolUse",
        matcher: Some("SendMessage"),
        timeout: 5,
    },
    Handler {
        name: "notification",
        event: "Notification",
        matcher: None,
        timeout: 5,
    },
    Handler {
        name: "stop",
        event: "Stop",
        matcher: None,
        timeout: 5,
    },
    Handler {
        name: "subagentstop",
        event: "SubagentStop",
        matcher: None,
        timeout: 5,
    },
    Handler {
        name: "sessionend",
        event: "SessionEnd",
        matcher: None,
        timeout: 5,
    },
];

/// Words answered with exit 0 and nothing else, and never listed — see the module header.
///
/// `posttooluse-bash` is the Python charter's tally of a Bash call that came back: the turn
/// marker for the tmux frame's spinner, and the trace row for an ask that was approved. Neither
/// is something charter-app keeps.
pub const NO_OPS: [&str; 1] = ["posttooluse-bash"];

/// The handler for `name`, if this binary answers it.
pub fn find(name: &str) -> Option<&'static Handler> {
    HANDLERS.iter().find(|h| h.name == name)
}

/// The registry as `charter hook --list --json` prints it: one line, keys in a fixed order.
pub fn json() -> String {
    let handlers: Vec<serde_json::Value> = HANDLERS
        .iter()
        .map(|h| {
            serde_json::json!({
                "name": h.name,
                "event": h.event,
                "matcher": h.matcher,
                "timeout": h.timeout,
                "args": ["hook", h.name],
            })
        })
        .collect();
    serde_json::json!({"schema": 1, "handlers": handlers}).to_string()
}

/// The registry as a person reads it: one row per hook.
pub fn table() -> String {
    let width = HANDLERS.iter().map(|h| h.name.len()).max().unwrap_or(0);
    let event_width = HANDLERS.iter().map(|h| h.event.len()).max().unwrap_or(0);
    HANDLERS
        .iter()
        .map(|h| {
            format!(
                "{:width$}  {:event_width$}  {}",
                h.name,
                h.event,
                h.matcher.unwrap_or("-")
            )
            .trim_end()
            .to_string()
                + "\n"
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_json_is_the_documented_shape() {
        let doc: serde_json::Value = serde_json::from_str(&json()).unwrap();
        assert_eq!(doc["schema"], 1);
        let first = &doc["handlers"][0];
        assert_eq!(
            first,
            &serde_json::json!({"name": "sessionstart", "event": "SessionStart",
                "matcher": null, "timeout": 5, "args": ["hook", "sessionstart"]})
        );
        assert_eq!(doc["handlers"].as_array().unwrap().len(), HANDLERS.len());
    }

    #[test]
    fn every_word_is_listed_once_and_no_no_op_is_listed() {
        let mut names: Vec<&str> = HANDLERS.iter().map(|h| h.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), HANDLERS.len());
        for word in NO_OPS {
            assert!(
                find(word).is_none(),
                "{word} is a no-op and must not be wired"
            );
        }
    }

    #[test]
    fn a_tool_hook_names_its_tool_and_an_event_hook_names_none() {
        for h in HANDLERS {
            let tool_event = matches!(h.event, "PreToolUse" | "PostToolUse");
            assert_eq!(h.matcher.is_some(), tool_event, "{}", h.name);
        }
    }

    #[test]
    fn every_event_word_the_app_reports_is_answered() {
        for event in [
            "sessionstart",
            "userpromptsubmit",
            "notification",
            "subagentstop",
            "stop",
            "sessionend",
        ] {
            assert!(
                crate::state::Event::parse(event).is_some(),
                "{event} is not an event word"
            );
            assert!(find(event).is_some(), "{event} is not in the registry");
        }
    }
}
