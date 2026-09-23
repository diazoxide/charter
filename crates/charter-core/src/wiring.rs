//! Whether a chat may start on a profile, and whether charter's guard runs in it.
//!
//! **There is nothing left to install.** This module used to be a port of `charter/wiring.py`
//! (ADR 0022): it asked `claude plugin list --json` under the profile's environment whether
//! the Python charter's `charter@charter` plugin was installed and enabled for the chat's
//! directory, installed it from its GitHub marketplace when it was not, and refused every
//! chat it could not wire — which, offline, was every Claude chat. Codex needed the same
//! marketplace's plugin, a policy line and a trusted guard hook in `~/.codex/config.toml`.
//!
//! The app now ships its own plugin and arms every chat it starts with it, for that session
//! alone ([`crate::plugin`], [`crate::harness::Harness::state_hooks`]): Claude Code loads the
//! bundled plugin with `--plugin-dir`, and Codex is armed with `-c` flags from the same
//! registry. So the guard runs in a chat because the app started it, whatever the profile's
//! config folder holds, and there is no folder to ask about and nothing to put in one.
//!
//! What is left is the gate a launch still needs — the kind is one this app starts, the file
//! the profile came from is not one git would carry, and the operator approved its command —
//! and, for `charter doctor`, the one fact about a profile that can still be wrong: whether
//! its harness can be found at all.

use std::path::{Path, PathBuf};

use crate::profiles::{self, Profile, Source};
use crate::shown;

/// What charter found when it asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// charter's guard runs in a chat started on this profile.
    Wired,
    /// charter could not look, or cannot start this profile. **Not a pass.**
    Unknown,
}

/// What was asked, what it answered, and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wiring {
    pub state: State,
    /// What was asked and what it answered — escaped, never clipped.
    pub detail: String,
    /// The command that would fix it; empty when wired.
    pub fix: String,
}

/// A value escaped for a terminal and not clipped.
fn whole(value: &str) -> String {
    shown::readable(value, usize::MAX)
}

/// Why this app will not start `p` at all — `None` when it may.
///
/// **Parsing is not launching.** [`crate::profiles`] reads all three kinds, `opencode`
/// included, because the Python charter accepts one and both implementations read one plane:
/// refusing it at the PARSE would give an operator two different answers about their own
/// file. What this app will not do is start one, and that is the spec's decision (decision 6,
/// "Harnesses in v1: Claude Code and Codex. opencode follows").
///
/// Decided from the DECLARED `kind`, which is a known value
/// ([`crate::harness::Harness::of_kind`]), never from the program name, which is not.
fn not_startable(p: &Profile) -> Option<String> {
    if crate::harness::Harness::of_kind(&p.kind).is_some() {
        return None;
    }
    Some(format!(
        "profile '{}' runs {}, which this app does not start — charter-app v1 starts Claude \
         Code and Codex, and opencode follows.",
        shown::short(&p.name),
        shown::short(&p.kind),
    ))
}

/// Why charter may not run `p`'s command — `None` when it may.
///
/// The ignore check first: an approved profile in a file git would commit is still a
/// declaration charter has refused, and a record saying the operator once approved it says
/// nothing about the file it now sits in. Built-ins skip both — their command is charter's
/// own, out of the registry.
fn not_asked(p: &Profile, root: &Path) -> Option<(String, String)> {
    if p.source == Source::BuiltIn {
        return None;
    }
    let name = shown::short(&p.name);
    let check = profiles::ignore_check(root);
    if !check.passes() {
        return Some((
            format!(
                "profile '{name}' is declared in a file charter has refused ({}) — nothing \
                 was started.",
                check.reason
            ),
            check.fix,
        ));
    }
    crate::profiletrust::approval_needed(root, p).map(|state| {
        (
            format!(
                "profile '{name}' is {}, and nobody has approved it — nothing was started. \
                 Approve it once so charter can run it.",
                state.as_str()
            ),
            format!(
                "start a chat on '{}' from the app's new-chat picker, which shows its command \
                 and asks once",
                whole(&p.name)
            ),
        )
    })
}

/// Whether charter's guard runs in a chat the app would start on `p`.
///
/// It does wherever the app can start the harness at all, because the app arms the chat
/// itself. So the one question left is whether the harness can be FOUND — the same lookup a
/// launch makes ([`crate::programs::resolve_argv`], charter-app#134) — and a harness that
/// cannot be is an unknown, not a pass, with the directories that were searched.
///
/// Nothing is run: a profile charter may not run a command for is told why before anything
/// else, and a harness that is found is not asked anything.
pub fn detect(p: &Profile, root: &Path) -> Wiring {
    if let Some((why, fix)) = not_asked(p, root) {
        return Wiring {
            state: State::Unknown,
            detail: why,
            fix,
        };
    }
    if let Some(why) = not_startable(p) {
        return Wiring {
            state: State::Unknown,
            detail: why,
            fix: String::new(),
        };
    }
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    let argv = profiles::expanded_command(p, &home);
    match crate::programs::resolve_argv(&argv) {
        // A command the operator wrote as a path is handed to the spawn as written and never
        // searched for, so the one thing to ask of it is whether it is there.
        Ok(found)
            if found
                .first()
                .is_some_and(|program| names_a_place(program) && !Path::new(program).is_file()) =>
        {
            let program = whole(found.first().map(String::as_str).unwrap_or_default());
            Wiring {
                state: State::Unknown,
                detail: format!("{program} is not a program on this machine"),
                fix: format!(
                    "point profile {} at a program that exists, in charter.local.toml",
                    whole(&p.name)
                ),
            }
        }
        Ok(found) => Wiring {
            state: State::Wired,
            // What the app does first and the path after it: a doctor row is clipped, and the
            // sentence is the part a reader acts on.
            detail: format!(
                "{} — {}",
                match p.kind.as_str() {
                    "codex" =>
                        "the app arms each Codex chat with charter's hooks; Codex asks once to trust them",
                    _ => "the app arms each chat with its own plugin, charter-app",
                },
                whole(found.first().map(String::as_str).unwrap_or_default()),
            ),
            fix: String::new(),
        },
        Err(gone) => Wiring {
            state: State::Unknown,
            detail: gone.said(),
            fix: format!(
                "give profile {} an absolute command in charter.local.toml: \
                 command = [\"/full/path/to/{}\"]",
                whole(&p.name),
                whole(&gone.program)
            ),
        },
    }
}

/// Whether `program` names a place rather than a word to search for. Off unix a bare word is
/// handed back unresolved ([`crate::programs::resolve`]), and it is not a missing file.
fn names_a_place(program: &str) -> bool {
    program.contains('/') || program.contains(std::path::MAIN_SEPARATOR)
}

/// Why `p` may not start — `None` when it may.
///
/// **The one home for every launch path**: the picker's start, a reopen, a handoff. First the
/// kind, because a profile that can never start is not worth approving and the sentence an
/// operator wants is the one about v1 rather than one about consent; then the gate that says
/// whether charter may run the profile's command at all.
pub fn refusal(p: &Profile, root: &Path) -> Option<String> {
    if let Some(why) = not_startable(p) {
        return Some(why);
    }
    not_asked(p, root).map(|(why, _fix)| why)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built_in(kind: &str, command: &str) -> Profile {
        let mut p = profiles::builtins()
            .into_iter()
            .find(|p| p.kind == "claude")
            .expect("a built-in claude profile");
        p.kind = kind.to_owned();
        p.command = vec![command.to_owned()];
        p
    }

    #[test]
    fn a_built_in_claude_profile_may_start_with_nothing_installed() {
        // The whole point: no marketplace, no install record, no config folder asked.
        let root = tempfile::tempdir().expect("a plane");
        assert_eq!(refusal(&built_in("claude", "claude"), root.path()), None);
    }

    #[test]
    fn an_opencode_profile_is_refused_by_what_this_app_starts() {
        let root = tempfile::tempdir().expect("a plane");
        let why = refusal(&built_in("opencode", "opencode"), root.path())
            .expect("opencode is not started");
        assert!(why.contains("does not start"), "{why}");
    }

    #[cfg(unix)]
    #[test]
    fn a_harness_that_is_found_is_wired_and_says_the_app_arms_it() {
        let root = tempfile::tempdir().expect("a plane");
        let w = detect(&built_in("claude", "/bin/sh"), root.path());
        assert_eq!(w.state, State::Wired, "{w:?}");
        assert!(w.detail.contains("charter-app"), "{}", w.detail);
        assert!(!w.detail.contains("charter@charter"), "{}", w.detail);
    }

    #[test]
    fn a_harness_that_cannot_be_found_is_an_unknown_and_never_a_pass() {
        let root = tempfile::tempdir().expect("a plane");
        let w = detect(
            &built_in("claude", "charter-no-such-harness-anywhere"),
            root.path(),
        );
        assert_eq!(w.state, State::Unknown, "{w:?}");
        assert!(w.fix.contains("absolute command"), "{}", w.fix);
    }
}
