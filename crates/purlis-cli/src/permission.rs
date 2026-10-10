//! `purlis hook permissionrequest`: a harness's permission prompt, asked in purlis's window
//! as well as in the pane (HP-6): Claude Code's and Codex's `PermissionRequest`, and opencode's
//! `permission.asked` as purlis's shim hands it on (#1691). Which one it is comes from the
//! harness the app put into the chat's environment ([`Source::of_harness`]).
//!
//! The hook hands its payload to the app on the hook channel and waits for the operator's
//! answer there; when it comes, the hook prints its harness's own decision for the option they
//! chose ([`purlis_core::harness::hooked::decision`]), and the harness carries it out without
//! the pane ever having focus.
//!
//! **Every other way out prints nothing**, and then the harness's own prompt decides in the
//! pane: no app listening, a payload that will not read, an app that will not hold the ask,
//! the deadline passing. This hook never allows or denies anything by itself.

use std::process::ExitCode;
use std::time::Duration;

use purlis_core::harness::hooked::{self, Source};
use purlis_core::hookwire::{self, PermissionAsked};

/// How long the hook waits for the app's reply: under the harness's timeout for it, so the
/// hook prints before the harness stops waiting. The app answers "none" before this, at the
/// ask's deadline.
const WAITS_AT_MOST: Duration = hooked::HOOK_TIMEOUT.saturating_sub(Duration::from_secs(1));

/// Asks the app, and prints Claude Code's decision for what the operator chose.
pub fn permissionrequest(payload: &str) -> ExitCode {
    let Some(socket) = purlis_core::envvar::var_os(hookwire::SOCKET_ENV) else {
        return ExitCode::SUCCESS;
    };
    let Some(chat) =
        purlis_core::envvar::var(hookwire::CHAT_ENV).and_then(|chat| chat.parse().ok())
    else {
        return ExitCode::SUCCESS;
    };
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(payload) else {
        return ExitCode::SUCCESS;
    };
    // A harness this does not know decides nothing, and its own prompt asks in the pane.
    let Some(source) =
        Source::of_harness(purlis_core::envvar::var(hookwire::HARNESS_ENV).as_deref())
    else {
        return ExitCode::SUCCESS;
    };
    let asked = PermissionAsked {
        chat,
        permission_request: source,
        payload,
    };
    match hookwire::ask_permission(
        std::path::Path::new(&socket),
        hookwire::ChatToken::from_env().as_ref(),
        &asked,
        WAITS_AT_MOST,
    ) {
        Ok(Some(chosen)) => {
            if let Some(decision) = hooked::decision(source, &asked.payload, &chosen) {
                println!("{decision}");
            }
        }
        Ok(None) => {
            if let Some(report) = left_to_its_terminal(source, &asked.payload, &|name| {
                purlis_core::envvar::var(name)
            }) {
                // Best effort, as every report is: the prompt asks in the pane either way.
                let _ = hookwire::deliver_report(
                    std::path::Path::new(&socket),
                    hookwire::ChatToken::from_env().as_ref(),
                    &report,
                );
            }
        }
        // Said where a zero-exit hook's stderr goes, the harness's debug log, and the pane asks.
        Err(why) => eprintln!("purlis: the app did not take this permission ask ({why})"),
    }
    ExitCode::SUCCESS
}

/// **The report that the prompt now waits in the harness's own terminal** (#1691), for a
/// harness that has no notice of its own to say so: Codex, which has no `Notification`. The
/// app took the ask and nobody answered it in time, or would not hold it, so the hook decides
/// nothing and the harness asks in its pane; the chat is then listed as waiting there, so it
/// does not wait unseen. Claude Code says it itself (`permission_prompt`), and purlis's opencode
/// shim says it as opencode asks.
pub fn left_to_its_terminal(
    source: Source,
    payload: &serde_json::Value,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<hookwire::Report> {
    if source != Source::Codex {
        return None;
    }
    let mut said = serde_json::json!({
        "hook_event_name": "Notification",
        "notification_type": "permission_prompt",
    });
    for field in ["session_id", "agent_id", "agent_type"] {
        if let Some(value) = payload.get(field).filter(|value| value.is_string()) {
            said[field] = value.clone();
        }
    }
    hookwire::Report::read(
        purlis_core::state::Event::Notification,
        &said.to_string(),
        env,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use purlis_core::harness::model::Prompt;
    use purlis_core::state::{Event, Notified};

    fn env(name: &str) -> Option<String> {
        match name {
            hookwire::CHAT_ENV => Some("7".to_owned()),
            hookwire::SOCKET_ENV => Some("/tmp/s.sock".to_owned()),
            _ => None,
        }
    }

    #[test]
    fn a_codex_prompt_left_to_its_pane_is_reported_as_a_permission_waiting_there() {
        let payload = serde_json::json!({
            "session_id": "11111111-2222-4333-8444-555555555555",
            "tool_name": "Bash", "tool_input": {"command": "npm test"}
        });
        let report = left_to_its_terminal(Source::Codex, &payload, &env).expect("a report");
        assert_eq!(report.chat, 7);
        assert_eq!(report.event, Event::Notification);
        assert_eq!(report.detail.notified, Notified::Asks(Prompt::Permission));
        assert!(!report.detail.idle);
    }

    #[test]
    fn a_harness_with_a_notice_of_its_own_is_left_to_say_it() {
        let payload = serde_json::json!({"session_id": "s"});
        for source in [Source::ClaudeCode, Source::Opencode] {
            assert!(
                left_to_its_terminal(source, &payload, &env).is_none(),
                "{source:?}"
            );
        }
    }
}
