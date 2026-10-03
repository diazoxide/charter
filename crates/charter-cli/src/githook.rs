//! `charter git-hook <name>`: charter's check in a chat's git hooks (SQ-16).
//!
//! For `pre-commit` and `pre-merge-commit`, the staged diff is scanned ([`charter_core::diffscan`]); a finding refuses
//! the commit on standard error, masked, and tells the app, so the chat joins the needs-you
//! queue. For `commit-msg`, the message is stamped with the chat's provenance trailers
//! ([`charter_core::provenance::stamp`]) and the hook always succeeds. Every other hook has no
//! check and answers success. The repository's own hook of the
//! same name is the shim's to run, after this ([`charter_core::githooks`]).

use std::path::Path;
use std::process::ExitCode;

use charter_core::githooks::{CHECKED, COMMIT_MSG, PRE_COMMIT};
use charter_core::{diffscan, hookwire};

/// Runs charter's check for hook `name` in the repository git ran it in: this process's
/// working directory, which git sets to the top of the work tree.
pub fn run(name: &str, args: &[String]) -> ExitCode {
    if name == COMMIT_MSG {
        if let (Ok(top), Some(message)) = (std::env::current_dir(), args.first()) {
            let message = top.join(message);
            charter_core::provenance::stamp(&message, &top, &|n| std::env::var(n).ok());
        }
        return ExitCode::SUCCESS;
    }
    if !CHECKED.contains(&name) {
        return ExitCode::SUCCESS;
    }
    let Ok(repo) = std::env::current_dir() else {
        eprintln!("charter: this commit could not be scanned for secrets (no working directory)");
        return ExitCode::FAILURE;
    };
    let scan = match diffscan::checked(&repo) {
        Ok(scan) => scan,
        Err(why) => {
            eprintln!("charter: this commit could not be scanned for secrets ({why})");
            return ExitCode::FAILURE;
        }
    };
    // A merge brings the allowlist as it was committed on the other side, by whoever committed
    // it there; only a commit a chat makes itself may not change it (SQ-17).
    if name == PRE_COMMIT && scan.changes_the_allowlist {
        eprint!("{}", diffscan::ALLOWLIST_REFUSAL);
        tell_the_app(format!(
            "commit refused in {}: it changes the scan's allowlist",
            repo.file_name().map_or_else(
                || repo.display().to_string(),
                |n| n.to_string_lossy().into_owned()
            )
        ));
        return ExitCode::FAILURE;
    }
    if scan.refused.is_empty() {
        return ExitCode::SUCCESS;
    }
    eprint!("{}", diffscan::refusal(&scan.refused));
    for problem in &scan.problems {
        eprintln!("  (and {problem})");
    }
    tell_the_app(diffscan::summary(&repo, &scan.refused));
    ExitCode::FAILURE
}

/// The app's needs-you item for this refusal. The commit is refused whatever this answers, and
/// a chat outside the app has none. A line the app does not take is spooled for the next host
/// to record (ADR 0068 §6, FD-30).
fn tell_the_app(why: String) {
    let env = |name: &str| std::env::var(name).ok();
    let (Some(socket), Some(chat)) = (
        env(hookwire::SOCKET_ENV),
        env(hookwire::CHAT_ENV).and_then(|chat| chat.parse().ok()),
    ) else {
        return;
    };
    let _ = hookwire::deliver_refused(
        Path::new(&socket),
        hookwire::ChatToken::from_env().as_ref(),
        &hookwire::CommitRefused {
            chat,
            commit_refused: why,
        },
    );
}
