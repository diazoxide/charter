//! `charter hook permissionrequest`: Claude Code's permission prompt, asked in charter's window
//! as well as in the pane (HP-6).
//!
//! The hook hands its payload to the app on the hook channel and waits for the operator's
//! answer there; when it comes, the hook prints Claude Code's own decision for the option they
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
    let asked = PermissionAsked {
        chat,
        permission_request: Source::ClaudeCode,
        payload,
    };
    match hookwire::ask_permission(
        std::path::Path::new(&socket),
        hookwire::ChatToken::from_env().as_ref(),
        &asked,
        WAITS_AT_MOST,
    ) {
        Ok(Some(chosen)) => {
            if let Some(decision) = hooked::decision(Source::ClaudeCode, &asked.payload, &chosen) {
                println!("{decision}");
            }
        }
        Ok(None) => {}
        // Said where a zero-exit hook's stderr goes, the harness's debug log, and the pane asks.
        Err(why) => eprintln!("purlis: the app did not take this permission ask ({why})"),
    }
    ExitCode::SUCCESS
}
