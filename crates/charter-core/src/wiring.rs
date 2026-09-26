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
//! bundled plugin with `--plugin-dir`, Codex is armed with `-c` flags from the same
//! registry, and opencode loads the bundle's opencode shim through `OPENCODE_CONFIG_CONTENT`
//! ([`crate::opencode`]). So the guard runs in a chat because the app started it, whatever the profile's
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
/// **Parsing is not launching.** [`crate::profiles`] reads every kind the Python charter
/// accepted, so both implementations give one answer about one file; what this app starts is
/// asked here. Since #371 that is all three kinds.
///
/// Decided from the DECLARED `kind`, which is a known value
/// ([`crate::harness::Harness::of_kind`]), never from the program name, which is not.
///
/// **And a profile that would start its harness past charter's guard is not started.** An
/// opencode profile that passes `--pure`, or sets the variables charter arms opencode through,
/// would run a chat that looks guarded and loads no charter plugin at all (measured, opencode
/// 1.18.23 — [`crate::opencode::disarmed_by`]).
fn not_startable(p: &Profile) -> Option<String> {
    let Some(harness) = crate::harness::Harness::of_kind(&p.kind) else {
        return Some(format!(
            "profile '{}' runs {}, which this app does not start.",
            shown::short(&p.name),
            shown::short(&p.kind),
        ));
    };
    let disarmed = match harness {
        crate::harness::Harness::Opencode => crate::opencode::disarmed_by(&p.command, &p.env),
        crate::harness::Harness::ClaudeCode | crate::harness::Harness::Codex => None,
    };
    disarmed.map(|why| {
        format!(
            "profile '{}' would start opencode without charter's guard: {why}. Nothing was \
             started — take it out of the profile in charter.local.toml.",
            shown::short(&p.name),
        )
    })
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
                        "the app arms each Codex chat with charter's hooks; Codex asks once to trust them"
                            .to_owned(),
                    "opencode" =>
                        "the app arms each opencode chat with charter's opencode plugin, for that chat alone"
                            .to_owned(),
                    _ => format!(
                        "the app arms each chat with its own plugin, {}",
                        crate::plugin::LOADED_AS
                    ),
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
///
/// **A question only off unix**, and the mutation run excludes its two mutants on that
/// ground (`.cargo/mutants.toml`). On unix `MAIN_SEPARATOR` is `/`, so `||` and `&&` between
/// the two halves ask one question twice; and every program `resolve_argv` hands back on unix
/// is absolute, so `true` is what this already answers there.
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
    fn an_opencode_profile_may_start_with_nothing_installed() {
        // #371: the app arms an opencode chat with its own shim, as it does the other two.
        let root = tempfile::tempdir().expect("a plane");
        assert_eq!(
            refusal(&built_in("opencode", "opencode"), root.path()),
            None
        );
    }

    #[test]
    fn an_opencode_profile_that_would_load_no_plugin_is_not_started() {
        // Measured on 1.18.23: `--pure` and `OPENCODE_PURE=1` load no external plugin, and the
        // vault's content then reached the model.
        let root = tempfile::tempdir().expect("a plane");
        let mut pure = built_in("opencode", "opencode");
        pure.command.push("--pure".to_owned());
        let why = refusal(&pure, root.path()).expect("refused");
        assert!(
            why.contains("without charter's guard") && why.contains("--pure"),
            "{why}"
        );

        let mut env = built_in("opencode", "opencode");
        env.env = vec![("OPENCODE_CONFIG_CONTENT".to_owned(), "{}".to_owned())];
        assert!(refusal(&env, root.path()).is_some());

        // The same flag on another harness is that harness's business.
        let mut claude = built_in("claude", "claude");
        claude.command.push("--pure".to_owned());
        assert_eq!(refusal(&claude, root.path()), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_harness_that_is_found_is_wired_and_says_the_app_arms_it() {
        let root = tempfile::tempdir().expect("a plane");
        let w = detect(&built_in("claude", "/bin/sh"), root.path());
        assert_eq!(w.state, State::Wired, "{w:?}");
        assert!(
            w.detail
                .starts_with("the app arms each chat with its own plugin, charter@inline — "),
            "{}",
            w.detail
        );
        assert!(!w.detail.contains("charter-app"), "{}", w.detail);
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
