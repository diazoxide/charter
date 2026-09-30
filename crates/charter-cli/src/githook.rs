//! `charter git-hook <name>`: charter's check in a chat's git hooks (SQ-16).
//!
//! For `pre-commit` and `pre-merge-commit`, the staged diff is scanned ([`charter_core::diffscan`]); a finding refuses
//! the commit on standard error, masked, and tells the app, so the chat joins the needs-you
//! queue. Every other hook has no check and answers success. The repository's own hook of the
//! same name is the shim's to run, after this ([`charter_core::githooks`]).

use std::path::Path;
use std::process::ExitCode;

use charter_core::githooks::CHECKED;
use charter_core::{diffscan, hookwire};

/// Runs charter's check for hook `name` in the repository git ran it in: this process's
/// working directory, which git sets to the top of the work tree.
pub fn run(name: &str) -> ExitCode {
    if !CHECKED.contains(&name) {
        return ExitCode::SUCCESS;
    }
    let Ok(repo) = std::env::current_dir() else {
        eprintln!("charter: this commit could not be scanned for secrets (no working directory)");
        return ExitCode::FAILURE;
    };
    match diffscan::staged(&repo) {
        Ok(found) if found.is_empty() => ExitCode::SUCCESS,
        Ok(found) => {
            eprint!("{}", diffscan::refusal(&found));
            tell_the_app(&repo, &found);
            ExitCode::FAILURE
        }
        Err(why) => {
            eprintln!("charter: this commit could not be scanned for secrets ({why})");
            ExitCode::FAILURE
        }
    }
}

/// The app's needs-you item for this refusal. Dropped whatever it answers: the commit is
/// refused whether or not an app hears about it, and a chat outside the app has none. ADR 0068
/// §6 has a refused line spooled instead; no hook spools until FD-30 (charter#667), and this
/// one spools with the rest when it does.
fn tell_the_app(repo: &Path, found: &[diffscan::Finding]) {
    let env = |name: &str| std::env::var(name).ok();
    let (Some(socket), Some(chat)) = (
        env(hookwire::SOCKET_ENV),
        env(hookwire::CHAT_ENV).and_then(|chat| chat.parse().ok()),
    ) else {
        return;
    };
    let _ = hookwire::tell_refused(
        Path::new(&socket),
        hookwire::ChatToken::from_env().as_ref(),
        &hookwire::CommitRefused {
            chat,
            commit_refused: diffscan::summary(repo, found),
        },
    );
}
