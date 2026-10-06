//! What this machine keeps about a project's sandbox, beside the project and never committed:
//! the answer to the one-time offer (ADR 0067 §1, ruling V21 1), and how many new chats started
//! with the sandbox and without it — the opt-out rate (ADR 0067 §7, V12).
//!
//! **Local, and never sent** (ruling V78 d). The count is shown by `charter doctor` and the
//! Settings tab, and nothing reads it for anything else.
//!
//! **Where:** `.charter/app/sandbox.json`, which the integrity class denies every sandboxed chat
//! ([`super::Class::Integrity`]), so a sandboxed chat can neither answer the offer nor change the
//! count. An unsandboxed chat runs as the person and can write it, as it can write anything of
//! theirs: the offer is only shown in a project whose chats are all unsandboxed, so a chat there
//! could answer it. Nothing here is a boundary against that.

use std::io;
use std::path::Path;

use super::{Plane, TABLE};

/// The file, relative to the project's state folder ([`path`]).
pub const IN_STATE: &str = "app/sandbox.json";

/// The file in the project at `root`.
pub fn path(root: &Path) -> std::path::PathBuf {
    crate::names::state(root).join(IN_STATE)
}

/// What the file holds.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct OnDisk {
    /// `"taken"` or `"kept-off"` once the offer is answered; absent before.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    offer: String,
    #[serde(default)]
    chats: Tally,
}

fn read(root: &Path) -> OnDisk {
    std::fs::read_to_string(path(root))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Reads the file, changes it and writes it back, under charter's lock on its directory.
fn change(root: &Path, how: impl FnOnce(&mut OnDisk)) -> io::Result<()> {
    let path = path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::rewrite::update(root, &path, |now| {
        // A file that is not this shape reads as empty, as [`read`] reads it: the count
        // starts again rather than refusing every chat start that would add to it.
        let mut held: OnDisk = now
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default();
        how(&mut held);
        serde_json::to_string_pretty(&held)
            .map(|text| Some(format!("{text}\n")))
            .map_err(io::Error::other)
    })
    .map(|_| ())
}

// ---- the one-time offer ----------------------------------------------------------------------

/// The operator's answer to the offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Turn the sandbox on for this project: `[sandbox] mode = "on"` in `charter.toml`.
    TurnOn,
    /// Keep running chats as before. Nothing in the project changes.
    KeepItOff,
}

/// Whether the project at `root` is offered the sandbox: it is a project, it has not turned
/// the sandbox on, and nobody on this machine has answered the offer. A project charter made
/// turned it on already, so only one made before the sandbox existed is ever offered it.
///
/// A `charter.toml` charter cannot read is never offered anything: what it says about the
/// sandbox is unknown, and every chat in it is refused until it is fixed.
pub fn offer_due(root: &Path) -> bool {
    let plane = Plane::read(root);
    crate::names::has_manifest(root)
        && !plane.unreadable()
        && plane.said().policy.is_none()
        && read(root).offer.is_empty()
}

/// Answers the offer for the project at `root`, once: turning the sandbox on writes it into
/// `charter.toml` first, so an answer is never recorded for a file that did not change.
pub fn answer(root: &Path, answer: Answer) -> io::Result<()> {
    if answer == Answer::TurnOn {
        // A `charter.toml` the sandbox cannot read — a link, a device, over the cap, not TOML —
        // already refuses every chat. It is never edited into a file that reads as on.
        if Plane::read(root).unreadable() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "charter.toml is not a file purlis reads (a link, a special file, too large, or \
                 not TOML), so nothing was changed. Fix it, and every chat in this project starts \
                 again.",
            ));
        }
        turn_on(root)?;
    }
    change(root, |held| {
        held.offer = match answer {
            Answer::TurnOn => "taken",
            Answer::KeepItOff => "kept-off",
        }
        .to_owned();
    })
}

/// Writes `mode = "on"` into the project's `charter.toml`, keeping every line already there:
/// the whole block with the default egress where the file has no `sandbox` at all, the mode
/// alone where it has a `[sandbox]` table already.
///
/// **What was written is read back before it is kept.** The mode is set by a text edit, and a
/// `[sandbox]` or `mode =` line can sit inside a multi-line string, where the edit would land
/// in the string and leave the table as it was. So the result is parsed again, and kept only
/// if the sandbox now reads as on and nothing more is refused than before. Any other form is
/// refused, and nothing is written: an answer is never recorded for a sandbox still off.
///
/// **Only a regular `charter.toml` at the project root**, never a link's target: the text is
/// read with the sandbox's own reader ([`super::read_plane_file`]), which refuses anything else,
/// and must be the text the write is made against.
fn turn_on(root: &Path) -> io::Result<()> {
    let target = crate::names::manifest(root);
    let dir = root.to_path_buf();
    let unedited = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "charter.toml's {TABLE} is written in a form purlis does not edit, so nothing \
                 was changed. Add mode = \"on\" to its [{TABLE}] table by hand to turn the \
                 sandbox on."
            ),
        )
    };
    crate::rewrite::update(&dir, &target, |now| {
        let Some(now) = now else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is not there", target.display()),
            ));
        };
        // Under the write's lock: still a regular file holding exactly this text.
        if super::read_plane_file(&target).ok().flatten().as_deref() != Some(now) {
            return Err(unedited());
        }
        let Ok(top) = now.parse::<toml::Table>() else {
            return Err(unedited());
        };
        let header = format!("[{TABLE}]");
        let next = match top.get(TABLE) {
            None => {
                let gap = if now.is_empty() || now.ends_with("\n\n") {
                    ""
                } else if now.ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                format!("{now}{gap}{}", super::on_block())
            }
            Some(toml::Value::Table(_)) if now.lines().any(|line| line.trim() == header) => {
                crate::scaffold::planefile::edited(now, TABLE, "mode", "on")
            }
            Some(_) => return Err(unedited()),
        };
        // Never a file that would refuse every chat: what is kept must read, and read as on.
        let before = Plane::of(Some(now)).said();
        let written = Plane::of(Some(&next));
        let after = written.said();
        if written.unreadable() || after.policy.is_none() || after.refused != before.refused {
            return Err(unedited());
        }
        Ok(Some(next))
    })
    .map(|_| ())
}

// ---- the opt-out count -----------------------------------------------------------------------

/// How a new chat in a sandboxed project started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// Under the sandbox.
    Sandboxed,
    /// Without it, because a person turned it off for that chat.
    OptedOut,
    /// Without it, because this machine has no sandbox backend (Windows, ruling V21 3).
    NoBackend,
}

/// How many new chats in a sandboxed project started each way on this machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tally {
    #[serde(default)]
    pub sandboxed: u64,
    #[serde(default)]
    pub opted_out: u64,
    #[serde(default)]
    pub no_backend: u64,
}

impl Tally {
    /// The opt-outs among the chats a person could have run sandboxed, as a whole percent
    /// rounded up, so a rate is never shown under the bar when it is not; `None` before the
    /// first such chat. A chat on a machine with no backend was never a choice, so it is not
    /// in the rate.
    pub fn rate_percent(&self) -> Option<u64> {
        let chose = self.sandboxed + self.opted_out;
        (chose > 0).then(|| (self.opted_out * 100).div_ceil(chose))
    }

    /// One sentence: the rate, and the bar it is held to (V12).
    pub fn said(&self) -> String {
        let chose = self.sandboxed + self.opted_out;
        let mut parts = Vec::new();
        if let Some(rate) = self.rate_percent() {
            parts.push(format!(
                "{} of {chose} {} started without the sandbox on this machine ({rate}%); the \
                 bar is under 10%",
                self.opted_out,
                chats(chose)
            ));
        }
        if self.no_backend > 0 {
            parts.push(format!(
                "{} {} started without the sandbox because this machine has no sandbox backend",
                self.no_backend,
                chats(self.no_backend)
            ));
        }
        if parts.is_empty() {
            return "no chat has started under this project's sandbox on this machine yet"
                .to_owned();
        }
        parts.join("; ")
    }
}

fn chats(n: u64) -> &'static str {
    if n == 1 { "chat" } else { "chats" }
}

/// What this machine counted for the project at `root`. A file that cannot be read counts
/// nothing.
pub fn tally(root: &Path) -> Tally {
    read(root).chats
}

/// Counts one new chat in the project at `root`.
pub fn count(root: &Path, started: Started) -> io::Result<()> {
    change(root, |held| {
        let n = match started {
            Started::Sandboxed => &mut held.chats.sandboxed,
            Started::OptedOut => &mut held.chats.opted_out,
            Started::NoBackend => &mut held.chats.no_backend,
        };
        *n = n.saturating_add(1);
    })
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
