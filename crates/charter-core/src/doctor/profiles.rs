//! `harness profiles`, and one `profile <name>` row per profile a selector would list.

use std::path::Path;

use super::{DISPLAY_LIMIT, Doctor, NOT_CHECKED_HINT, Row};
use crate::profiles::{self, IgnoreCheck, Profile, Source};
use crate::shown;
use crate::wiring::{self, State};

/// `harness profiles`: this machine's profiles as `charter.local.toml` declares them now, and
/// whether git would carry that file.
///
/// **`--preflight` skips the git question** (re-review N8): every git call on a hook path is
/// paid at every session start. What is left costs one file read, so a refused profile and a
/// default naming nothing are still reported there.
pub(super) fn harness_profiles(d: &Doctor) -> Row {
    const NAME: &str = "harness profiles";
    // Read first, whatever git says, as `charter harness list` reads it.
    let set = profiles::current(&d.root);
    let check = if d.preflight {
        IgnoreCheck::default()
    } else {
        profiles::ignore_check(&d.root)
    };
    if !check.passes() {
        return Row::warn(NAME, check.reason, check.fix);
    }
    let names = set
        .profiles()
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if let Some(first) = set.refused.first() {
        let who: Vec<&str> = set
            .refused
            .iter()
            .map(|r| {
                if r.name.is_empty() {
                    r.source.as_str()
                } else {
                    r.name.as_str()
                }
            })
            .collect();
        return Row::warn(
            NAME,
            format!("{} refused: {}", set.refused.len(), who.join(", ")),
            first.reason.clone(),
        );
    }
    if let Some(value) = &set.default_refused {
        return Row::warn(
            NAME,
            format!(
                "[harness] default = \"{value}\" names no profile this machine has — one of: \
                 {names}."
            ),
            format!("set [harness] default to one of: {names}, or delete the key"),
        );
    }
    Row::ok(
        NAME,
        format!("{} profile(s): {names}", set.profiles().len()),
    )
}

/// `contain.counted`: `text`, already contained, within `limit` characters — and saying how
/// many it did not show. A row an operator acts on cannot use a bare ellipsis: a clipped
/// command and a whole one would read the same.
fn counted(text: &str, limit: usize) -> String {
    let n = text.chars().count();
    if n <= limit {
        return text.to_owned();
    }
    let kept: String = text.chars().take(limit).collect();
    format!("{kept}\u{2026} +{} not shown", n - limit)
}

/// A profile's row name. One spelling, because the column is sized from these strings.
fn row_name(p: &Profile) -> String {
    format!(
        "profile {}",
        counted(&shown::readable(&p.name, usize::MAX), DISPLAY_LIMIT)
    )
}

/// Every profile a selector would show, in `charter harness list`'s order — `wiring.listed`.
///
/// Declared profiles always: one whose program is not installed is still somebody's
/// declaration, and this is where they find out. A built-in only when its program is
/// installed: a row about a harness this machine does not have is one nobody can act on.
///
/// **"Installed" is [`crate::programs::on_path`], which is `shutil.which` WIDENED**, and the
/// one place this port deliberately answers a wider question than Python's. Python's
/// `wiring.listed` asks `shutil.which`, which reads `$PATH` and nothing else; this asks
/// `$PATH` first and then the fixed user-bin directories charter searches everywhere else
/// (charter-app#134). Keeping Python's answer here would split the app in two: a built-in
/// `claude` that starts a chat perfectly well, and a `charter doctor` that reports no such
/// profile because the harness is in `~/.local/bin`. One idea of which profiles exist, or the
/// report and the launch disagree about the operator's own machine.
///
/// The differential does not see the difference: it runs with `HOME` pointed at a scratch
/// directory, so every home-relative directory is empty, and no harness is installed
/// machine-wide on the runner.
fn listed(root: &Path) -> Vec<Profile> {
    let order: Vec<String> = profiles::builtins().into_iter().map(|p| p.name).collect();
    let home = profiles::home().unwrap_or_else(|| std::path::PathBuf::from("~"));
    let mut rows: Vec<Profile> = profiles::current(root)
        .profiles()
        .iter()
        .filter(|p| {
            p.source != Source::BuiltIn
                || profiles::expanded_command(p, &home)
                    .first()
                    .is_some_and(|program| crate::programs::on_path(program))
        })
        .cloned()
        .collect();
    rows.sort_by_key(|p| {
        let place = order.iter().position(|name| *name == p.name);
        (place.is_none(), place.unwrap_or(0), p.name.clone())
    });
    rows
}

/// Why charter may not run a profile's command to ask it anything, as `(detail, hint)` —
/// `doctor._not_probed`. Asking would mean running a command out of a file a chat can write.
fn not_probed(root: &Path, p: &Profile, ignored: &IgnoreCheck) -> Option<(String, String)> {
    if p.source == Source::BuiltIn {
        return None;
    }
    if !ignored.passes() {
        return Some((
            "not probed — git would carry charter.local.toml, so every profile in it is \
             refused (the harness profiles row says why)"
                .to_owned(),
            ignored.fix.clone(),
        ));
    }
    crate::profiletrust::approval_needed(root, p).map(|state| {
        (
            format!(
                "{}, and not approved yet — charter asks before it runs a command it has not \
                 been shown",
                state.as_str()
            ),
            format!(
                "start a chat on '{}' from the app's new-chat picker, which shows its command \
                 and asks once",
                shown::readable(&p.name, usize::MAX)
            ),
        )
    })
}

/// One row per listed profile: can the app start its harness, and so guard its chats?
///
/// **Never on a hook path** (ruling 11): only a `charter doctor` a person types asks. The
/// questions run side by side.
///
/// The answer is this binary's own ([`wiring::detect`]): the app arms every chat it starts
/// with its own plugin, so what is left to say about a profile is whether its harness can be
/// found. Nothing is run to find out.
pub(super) fn profile_rows(d: &Doctor) -> Vec<Row> {
    if d.preflight {
        return Vec::new();
    }
    let rows = listed(&d.root);
    // One git call however many profiles are declared, and none for a plane that declares
    // none.
    let ignored = if rows.iter().any(|p| p.source != Source::BuiltIn) {
        profiles::ignore_check(&d.root)
    } else {
        IgnoreCheck::default()
    };
    std::thread::scope(|scope| {
        let pending: Vec<_> = rows
            .iter()
            .map(|p| match not_probed(&d.root, p, &ignored) {
                Some(held) => Err(held),
                None => Ok(scope.spawn(move || wiring::detect(p, &d.root))),
            })
            .collect();
        rows.iter()
            .zip(pending)
            .map(|(p, pending)| {
                let name = row_name(p);
                let w = match pending {
                    Err((detail, hint)) => {
                        return Row::warn(
                            &name,
                            counted(&detail, DISPLAY_LIMIT),
                            counted(&hint, DISPLAY_LIMIT),
                        );
                    }
                    Ok(handle) => match handle.join() {
                        Ok(w) => w,
                        // A probe that panicked costs its own row, never the report.
                        Err(_) => {
                            return Row::warn(
                                &name,
                                "not checked (the probe for this profile stopped)",
                                NOT_CHECKED_HINT,
                            );
                        }
                    },
                };
                let detail = counted(&w.detail, DISPLAY_LIMIT);
                match w.state {
                    State::Wired => Row::ok(&name, detail),
                    State::Unknown if w.fix.is_empty() => {
                        Row::warn(&name, detail, NOT_CHECKED_HINT)
                    }
                    State::Unknown => Row::warn(&name, detail, counted(&w.fix, DISPLAY_LIMIT)),
                }
            })
            .collect()
    })
}
