//! What this machine keeps about a project's sandbox, beside the project and never committed:
//! the answer to the one-time offer (ADR 0067 §1, ruling V21 1), how many new chats started
//! with the sandbox and without it — the opt-out rate (ADR 0067 §7, V12) — and the project's
//! hosts as this machine last told the person of them (#1341).
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
    /// The project's hosts as this machine last showed them to the person (#1341), each as
    /// [`super::hosts::Host`] spells it; absent before the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hosts_seen: Option<Vec<String>>,
    /// Your own hosts for this project that you confirmed in Settings on this machine (#1341),
    /// each as [`super::hosts::Host`] spells it: the only ones of `charter.local.toml` that
    /// grant anything ([`super::hosts::personal`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hosts_mine: Vec<String>,
    /// The folders you let every chat of this project on this machine write (#1342), each whole
    /// as [`super::grant::write`] took it: the only place a write grant of yours is kept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    writes_mine: Vec<String>,
    /// Each grant made on this machine from a block's Notice that lasts past one chat (#1342):
    /// what Settings' Granted list says of who granted it, when, and from which chat.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    granted: Vec<Made>,
    /// The folders you listed in Settings › Sandbox as ones chats may be granted (D-1342-10):
    /// added to the allowlist a write grant must be inside, on this machine only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    grantable: Vec<String>,
    /// The vaults you let a persona's chats use on this machine although the vault registry
    /// does not tag them for it (#1430): the only place such a grant is kept. Never the
    /// registry, whose shared half is committed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    vaults_mine: Vec<VaultGrant>,
    /// The dispatch grants you made for every chat of this project on this machine (#1437):
    /// the only place a dispatch grant of yours is kept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dispatch_mine: Vec<DispatchPair>,
    /// The project's dispatch grants as this machine last showed them to the person (#1437),
    /// each as [`crate::dispatchgrant::Pair`] is displayed; absent before the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dispatch_seen: Option<Vec<String>>,
    /// **Not this file's any more**: the nevers one dev build kept here, before they moved to
    /// `app/dispatch-never.json` ([`crate::dispatchnever`]). Carried as it was written until
    /// that module moves it ([`legacy_dispatch_nevers`], [`drop_legacy_dispatch_nevers`]), so
    /// no write of this file drops a never on the way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dispatch_never: Option<serde_json::Value>,
    /// The personas whose chats you let dispatch to **any** persona, on this machine (#1503).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dispatch_any: Vec<String>,
    /// The personas whose "any persona" grant in the project's file you accepted on this
    /// machine (#1503). Kept apart from `dispatch_seen`, so nothing that acknowledges a pair
    /// puts one in force.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dispatch_any_seen: Vec<String>,
    /// The grants of yours set aside because a name in them came to be another persona's
    /// (#1504): in force for no chat until you give them back or remove each in Settings
    /// ([`crate::dispatchdormant`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dispatch_dormant: Vec<Dormant>,
    /// What this machine accepted of the project's grants for a name whose grants were set
    /// aside: taken out of `dispatch_seen` and `dispatch_any_seen` then, and put back by Give
    /// back where the file still holds it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dispatch_accepted_aside: Vec<Accepted>,
    /// The project's grants you said "Not on my machine" to (#1504), each as
    /// [`crate::dispatchgrant::Pair`] is displayed, or `<asking> -> *` for any persona. None of
    /// them is in `dispatch_seen` or `dispatch_any_seen`, which is what keeps them out of
    /// force; this list only stops them being told as new.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dispatch_declined: Vec<String>,
    /// What this machine knows of each persona a grant names (#1504): its definition as it
    /// was when a dispatch was last judged with it there, and whether it has been seen gone
    /// since ([`crate::dispatchdormant`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dispatch_known: Vec<Known>,
    /// **What the acceptances above are bound to** (#1506): the commit of the project's
    /// history through which this machine has checked that everything in `dispatch_seen` and
    /// `dispatch_any_seen` stayed in the project's file ([`crate::dispatcharrival::settle`]).
    /// Absent where nothing is accepted, or the project is in no git repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dispatch_seen_at: Option<String>,
    /// The project's grants you accepted here that it then took away (#1506), each as
    /// `dispatch_declined` spells one: told once, then forgotten. One the file holds again
    /// waits for a new yes, and is said to have been accepted before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dispatch_gone: Vec<String>,
}

/// One grant of yours set aside because a name in it came to be another persona's (#1504):
/// `was` is that name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Dormant {
    pub asking: String,
    /// The target persona's name; `*` where [`Dormant::any`].
    pub target: String,
    /// Whether it was "any persona". **Said by this key and never by `target`**: a `*` someone
    /// wrote as the target of a named grant granted nothing, and set aside it is still a pair
    /// nobody can be given.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub any: bool,
    /// Empty where a hand-edit dropped it: the entry is then nobody's to be given back.
    #[serde(default)]
    pub was: String,
}

/// One of the project's grants this machine had accepted, set aside with the grants of the
/// name `was` (#1504).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Accepted {
    /// The grant, as `dispatch_declined` spells one: a pair as it is displayed, or
    /// `<asking> -> *`.
    pub said: String,
    #[serde(default)]
    pub was: String,
}

/// What this machine knows of one persona a dispatch grant names (#1504).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Known {
    pub name: String,
    /// The SHA-256 of its definition, in hex, as it was when a dispatch was last judged with
    /// it there. Empty where none was, or where purlis itself removed the persona.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// Whether the name has been seen to be no persona of the project since: **the absence
    /// mark**. While it stands and the name is a persona again, its grants are not in force
    /// until the same definition is back or the person says so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub away: bool,
}

/// One dispatch grant of yours, as the file keeps it: chats running as `asking` may dispatch
/// to `target`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct DispatchPair {
    asking: String,
    target: String,
}

/// One vault you let one persona's chats use on this machine (#1430), beside whatever the vault
/// registry tags it for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VaultGrant {
    pub vault: String,
    pub persona: String,
}

impl VaultGrant {
    /// The grant as the Granted list and the audit name it: `<vault> for <persona>`.
    pub fn target(&self) -> String {
        format!("{} for {}", self.vault, self.persona)
    }
}

/// What a [`Made`] record and the audit call a vault grant.
pub const VAULT: &str = "vault";

/// One grant made on this machine that lasts past its chat (#1342), as `app/sandbox.json` keeps
/// it: what Settings' Granted list says of it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Made {
    /// `host`, `write` or `vault`.
    pub what: String,
    /// The host or the folder, as the sandbox writes it; a vault and the persona it was allowed
    /// for, as [`VaultGrant::target`] says them.
    pub target: String,
    /// `you` or `project`.
    pub level: String,
    /// When, in seconds since 1970.
    pub at: u64,
    /// The chat it was granted from, by the name its tab showed, where it came from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
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
            .unwrap_or_else(|| OnDisk {
                // What starts again never takes a never an earlier build left here with it.
                dispatch_never: now.and_then(legacy_nevers_in),
                ..OnDisk::default()
            });
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

/// Whether the project at `root` is offered the sandbox: it is a project, no sandbox is in
/// force in it (it has not turned the sandbox on, and no policy requires it here, D-1423-1:
/// an offer to turn on what is on already would ask nothing), and nobody on this machine has
/// answered the offer. A project charter made
/// turned it on already, so only one made before the sandbox existed is ever offered it.
///
/// A `charter.toml` charter cannot read is never offered anything: what it says about the
/// sandbox is unknown, and every chat in it is refused until it is fixed.
pub fn offer_due(root: &Path) -> bool {
    let plane = Plane::read(root);
    crate::names::has_manifest(root)
        && !plane.unreadable()
        && plane.in_force(&super::policy::Locks::of(root)).is_none()
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

// ---- the project's hosts, told once per change (#1341) --------------------------------------

/// The project's own hosts as they changed since this machine last told the person: what was
/// added, what was taken away, and the whole list now — what acknowledging it records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostsChange {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub now: Vec<String>,
}

/// **How the project's hosts changed since this machine last told the person** — its own
/// `[sandbox] hosts` and the `[[forge]]` hosts the `forge` preset lets every chat reach (ADR 0067 §1 as
/// amended: each teammate sees a one-time Notice naming what changed). `None` when nothing did,
/// and in a project whose chats are not sandboxed, where its hosts reach nothing. A project
/// first seen on this machine with hosts is a change: nothing widens unseen. Order is no change.
///
/// A persona's own hosts (#1362) are committed too, so they are told the same way, each named
/// with the persona whose chats reach it ([`Plane::granted_hosts`]).
///
/// **Held to an administrator's policy** (#1423): a host policy locks out reaches no chat, so
/// the Notice never names it as one that does. A policy that comes to lock a host out, or lets
/// one back in, is a change to what chats reach, and is told as one.
pub fn hosts_changed(root: &Path) -> Option<HostsChange> {
    let plane = Plane::read(root);
    // Only the hosts a chat reaches: none an administrator's policy locks out (#1423).
    let locks = super::policy::Locks::of(root);
    plane.in_force(&locks)?;
    let now = plane.granted_hosts(&locks);
    let seen = read(root).hosts_seen.unwrap_or_default();
    let added: Vec<String> = now
        .iter()
        .filter(|one| !seen.contains(one))
        .cloned()
        .collect();
    let removed: Vec<String> = seen
        .iter()
        .filter(|one| !now.contains(one))
        .cloned()
        .collect();
    (!added.is_empty() || !removed.is_empty()).then_some(HostsChange {
        added,
        removed,
        now,
    })
}

/// Records that the person was told of `shown`, the project's hosts as the Notice showed them:
/// a change after it was shown is told again ([`hosts_changed`]).
pub fn acknowledge_hosts(root: &Path, shown: &[String]) -> io::Result<()> {
    change(root, |held| held.hosts_seen = Some(shown.to_vec()))
}

// ---- your own hosts, as you confirmed them (#1341) -------------------------------------------

/// Your own hosts for the project at `root` that you confirmed in Settings on this machine.
pub fn confirmed_hosts(root: &Path) -> Vec<String> {
    read(root).hosts_mine
}

/// Records `host` as one you confirmed: Settings' Add and Confirm, never a chat (a sandboxed
/// chat cannot write this file).
pub fn confirm_host(root: &Path, host: &str) -> io::Result<()> {
    change(root, |held| {
        if !held.hosts_mine.iter().any(|one| one == host) {
            held.hosts_mine.push(host.to_owned());
        }
    })
}

/// Takes `host` off the record: Settings' Remove.
pub fn unconfirm_host(root: &Path, host: &str) -> io::Result<()> {
    change(root, |held| held.hosts_mine.retain(|one| one != host))
}

// ---- the folders you let every chat write (#1342) --------------------------------------------

/// The folders you let every chat of the project at `root` write on this machine, as granted.
/// [`super::Compiled::granted`] judges each again before a start compiles it in.
pub fn granted_writes(root: &Path) -> Vec<std::path::PathBuf> {
    read(root)
        .writes_mine
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect()
}

/// Lets every chat of the project at `root` write `folder` on this machine: a block's Notice,
/// never a chat (a sandboxed chat cannot write this file). `folder` is one
/// [`super::grant::write`] took.
pub fn grant_write(root: &Path, folder: &Path) -> io::Result<()> {
    let folder = folder.display().to_string();
    change(root, |held| {
        if !held.writes_mine.contains(&folder) {
            held.writes_mine.push(folder);
        }
    })
}

/// Takes `folder` off what you let every chat write: Settings' Revoke.
pub fn revoke_write(root: &Path, folder: &Path) -> io::Result<()> {
    let folder = folder.display().to_string();
    change(root, |held| {
        held.writes_mine.retain(|one| *one != folder);
        held.granted
            .retain(|made| !(made.what == "write" && made.target == folder));
    })
}

/// The folders you listed as ones chats in the project at `root` may be granted (D-1342-10).
pub fn grantable_folders(root: &Path) -> Vec<std::path::PathBuf> {
    read(root)
        .grantable
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect()
}

/// Lists `folder` as one chats may be granted: Settings, never a chat. `folder` is one
/// [`super::grant::grantable`] took.
pub fn list_grantable(root: &Path, folder: &Path) -> io::Result<()> {
    let folder = folder.display().to_string();
    change(root, |held| {
        if !held.grantable.contains(&folder) {
            held.grantable.push(folder);
        }
    })
}

/// Takes `folder` off that list. A grant already made inside it is judged again at the next
/// start, and no longer reaches a chat.
pub fn unlist_grantable(root: &Path, folder: &Path) -> io::Result<()> {
    let folder = folder.display().to_string();
    change(root, |held| held.grantable.retain(|one| *one != folder))
}

/// Records that `made` was granted here, replacing an earlier record of the same grant.
pub fn record_made(root: &Path, made: Made) -> io::Result<()> {
    change(root, |held| {
        held.granted.retain(|one| {
            !(one.what == made.what && one.target == made.target && one.level == made.level)
        });
        held.granted.push(made);
    })
}

/// Takes the record of a grant off, once it is revoked.
pub fn forget_made(root: &Path, what: &str, target: &str, level: &str) -> io::Result<()> {
    change(root, |held| {
        held.granted
            .retain(|one| !(one.what == what && one.target == target && one.level == level));
    })
}

/// Every grant made on this machine that lasts past its chat, as recorded.
pub fn made(root: &Path) -> Vec<Made> {
    read(root).granted
}

// ---- the vaults you let a persona's chats use (#1430) ---------------------------------------

/// The vaults you let a persona's chats use in the project at `root` on this machine, beside
/// what the vault registry tags. A file that cannot be read grants nothing.
pub fn granted_vaults(root: &Path) -> Vec<VaultGrant> {
    read(root).vaults_mine
}

/// Lets `persona`'s chats in the project at `root` use `vault` on this machine: a refused
/// vault's Notice, never a chat (a sandboxed chat cannot write this file, and no line on the
/// hook channel reaches this).
pub fn grant_vault(root: &Path, vault: &str, persona: &str) -> io::Result<()> {
    let grant = VaultGrant {
        vault: vault.to_owned(),
        persona: persona.to_owned(),
    };
    change(root, |held| {
        if !held.vaults_mine.contains(&grant) {
            held.vaults_mine.push(grant);
        }
    })
}

// ---- your dispatch grants, and the project's as you were told of them (#1437) ----------------

/// The dispatch grants you made for every chat of the project at `root` on this machine, each
/// as asking persona and target persona. [`crate::dispatchgrant::yours`] reads them as pairs.
pub fn granted_dispatch(root: &Path) -> Vec<(String, String)> {
    read(root)
        .dispatch_mine
        .into_iter()
        .map(|pair| (pair.asking, pair.target))
        .collect()
}

/// Lets every chat of the project at `root` that runs as `asking` dispatch to `target`, on
/// this machine: the grant Notice's Allow, never a chat (a sandboxed chat cannot write this
/// file).
pub fn grant_dispatch(root: &Path, asking: &str, target: &str) -> io::Result<()> {
    let pair = DispatchPair {
        asking: asking.to_owned(),
        target: target.to_owned(),
    };
    change(root, |held| {
        if !held.dispatch_mine.contains(&pair) {
            held.dispatch_mine.push(pair);
        }
    })
}

/// Takes `vault` off what `persona`'s chats may use here: Settings' Revoke. The next brokered
/// run reads it, so nothing restarts.
pub fn revoke_vault(root: &Path, vault: &str, persona: &str) -> io::Result<()> {
    let grant = VaultGrant {
        vault: vault.to_owned(),
        persona: persona.to_owned(),
    };
    let target = grant.target();
    change(root, |held| {
        held.vaults_mine.retain(|one| *one != grant);
        held.granted
            .retain(|made| !(made.what == VAULT && made.target == target));
    })
}

/// Takes that grant back: Settings' Revoke. Answers whether there was one.
pub fn revoke_dispatch(root: &Path, asking: &str, target: &str) -> io::Result<bool> {
    let mut was = false;
    change(root, |held| {
        let before = held.dispatch_mine.len();
        held.dispatch_mine
            .retain(|pair| !(pair.asking == asking && pair.target == target));
        was = held.dispatch_mine.len() != before;
    })?;
    Ok(was)
}

/// The project's dispatch grants as this machine last told the person of them; `None` before
/// the first.
pub fn dispatch_seen(root: &Path) -> Option<Vec<String>> {
    read(root).dispatch_seen
}

/// Records that the person was told of `shown`, the project's dispatch grants as the Notice
/// showed them.
pub fn acknowledge_dispatch(root: &Path, shown: &[String]) -> io::Result<()> {
    change(root, |held| {
        held.dispatch_seen = Some(shown.to_vec());
        // A yes is the newer answer: what is allowed here is no longer declined here, and is
        // no longer one that was taken away.
        held.dispatch_declined.retain(|one| !shown.contains(one));
        held.dispatch_gone.retain(|one| !shown.contains(one));
    })
}

// ---- any persona, and what an earlier build left of nevers (#1503) -----------------------

/// What `text`, this file, holds under the key one dev build kept nevers in, whatever else in
/// the file reads or does not.
fn legacy_nevers_in(text: &str) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()?
        .get("dispatch_never")
        .cloned()
}

/// **The nevers an earlier build left in this file**, for [`crate::dispatchnever`] to move:
/// each as asking persona and target persona. Empty where there are none; `None` where the
/// key is there and is not a list of pairs, so what it refused is unknown.
pub fn legacy_dispatch_nevers(root: &Path) -> Option<Vec<(String, String)>> {
    let Some(left) = std::fs::read_to_string(path(root))
        .ok()
        .as_deref()
        .and_then(legacy_nevers_in)
    else {
        return Some(Vec::new());
    };
    crate::dispatchnever::pairs_of(&left)
}

/// Takes that key out of this file, once what it held is kept where it belongs. Every other
/// key stays as it is written, read by this build or not.
pub fn drop_legacy_dispatch_nevers(root: &Path) -> io::Result<()> {
    crate::rewrite::update(root, &path(root), |now| {
        let Some(serde_json::Value::Object(mut whole)) =
            now.and_then(|text| serde_json::from_str(text).ok())
        else {
            return Ok(None);
        };
        if whole.remove("dispatch_never").is_none() {
            return Ok(None);
        }
        serde_json::to_string_pretty(&whole)
            .map(|text| Some(format!("{text}\n")))
            .map_err(io::Error::other)
    })
    .map(|_| ())
}

/// The personas whose chats you let dispatch to any persona in the project at `root`, on this
/// machine, as the file spells them. [`crate::dispatchgrant::any_yours`] keeps the names.
pub fn granted_dispatch_any(root: &Path) -> Vec<String> {
    read(root).dispatch_any
}

/// Lets every chat of the project at `root` that runs as `asking` dispatch to any persona, on
/// this machine: Settings, never a Notice's answer and never a chat.
pub fn grant_dispatch_any(root: &Path, asking: &str) -> io::Result<()> {
    change(root, |held| {
        if !held.dispatch_any.iter().any(|one| one == asking) {
            held.dispatch_any.push(asking.to_owned());
        }
    })
}

/// Takes that back: Settings' Revoke. Answers whether there was one.
pub fn revoke_dispatch_any(root: &Path, asking: &str) -> io::Result<bool> {
    let mut was = false;
    change(root, |held| {
        let before = held.dispatch_any.len();
        held.dispatch_any.retain(|one| one != asking);
        was = held.dispatch_any.len() != before;
    })?;
    Ok(was)
}

/// The personas whose "any persona" grant in the project's file you accepted on this machine.
pub fn dispatch_any_seen(root: &Path) -> Vec<String> {
    read(root).dispatch_any_seen
}

/// Records that you accepted the project's "any persona" grant for `asking` on this machine.
pub fn acknowledge_dispatch_any(root: &Path, asking: &str) -> io::Result<()> {
    change(root, |held| {
        if !held.dispatch_any_seen.iter().any(|one| one == asking) {
            held.dispatch_any_seen.push(asking.to_owned());
        }
        let said = format!("{asking} -> {}", crate::dispatchgrant::ANY);
        held.dispatch_declined.retain(|one| *one != said);
        held.dispatch_gone.retain(|one| *one != said);
    })
}

/// Takes `asking` off what you accepted of the project's "any persona" grants.
pub fn forget_dispatch_any(root: &Path, asking: &str) -> io::Result<()> {
    change(root, |held| {
        held.dispatch_any_seen.retain(|one| one != asking)
    })
}

// ---- a persona that goes away, what was set aside, and what was declined (#1504) -------------

/// The asking and target names of a pair as [`crate::dispatchgrant::Pair`] is displayed.
fn sides(said: &str) -> Option<(&str, &str)> {
    said.split_once(" -> ")
}

/// Every name the records in force hold: the personas of your grants, of "any persona", and
/// of what this machine accepted of the project's.
fn granted_names(held: &OnDisk) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        if name != crate::dispatchgrant::ANY && !names.iter().any(|one| one == name) {
            names.push(name.to_owned());
        }
    };
    for pair in &held.dispatch_mine {
        add(&pair.asking);
        add(&pair.target);
    }
    for name in held.dispatch_any.iter().chain(&held.dispatch_any_seen) {
        add(name);
    }
    for said in held.dispatch_seen.iter().flatten() {
        if let Some((asking, target)) = sides(said) {
            add(asking);
            add(target);
        }
    }
    names
}

/// Every name a dispatch grant in force on this machine holds, in the project at `root`.
pub fn dispatch_granted_names(root: &Path) -> Vec<String> {
    granted_names(&read(root))
}

/// What this machine knows of the personas its dispatch grants name.
pub fn dispatch_known(root: &Path) -> Vec<Known> {
    read(root).dispatch_known
}

/// **Marks each of `names` as seen gone**, in one write: the absence mark. With `forget`, what
/// was known of its definition is dropped too, so nothing that comes back under the name is
/// taken for it (purlis's own removal). Nothing is written where each is marked already.
pub fn mark_dispatch_away(root: &Path, names: &[String], forget: bool) -> io::Result<()> {
    let known = read(root).dispatch_known;
    let marked = |name: &String| {
        known
            .iter()
            .any(|one| one.name == *name && one.away && !(forget && !one.hash.is_empty()))
    };
    if names.iter().all(marked) {
        return Ok(());
    }
    change(root, |held| {
        for name in names {
            match held.dispatch_known.iter_mut().find(|one| one.name == *name) {
                Some(one) => {
                    one.away = true;
                    if forget {
                        one.hash.clear();
                    }
                }
                None => held.dispatch_known.push(Known {
                    name: name.clone(),
                    hash: String::new(),
                    away: true,
                }),
            }
        }
    })
}

/// **Records each name of `seen` as there, with its definition's hash**, in one write: the
/// absence mark of each is lifted. Nothing is written where each is known so already.
pub fn know_dispatch_personas(root: &Path, seen: &[(String, String)]) -> io::Result<()> {
    let known = read(root).dispatch_known;
    let same = |(name, hash): &(String, String)| {
        known
            .iter()
            .any(|one| one.name == *name && one.hash == *hash && !one.away)
    };
    if seen.iter().all(same) {
        return Ok(());
    }
    change(root, |held| {
        for (name, hash) in seen {
            match held.dispatch_known.iter_mut().find(|one| one.name == *name) {
                Some(one) => {
                    one.hash.clone_from(hash);
                    one.away = false;
                }
                None => held.dispatch_known.push(Known {
                    name: name.clone(),
                    hash: hash.clone(),
                    away: false,
                }),
            }
        }
    })
}

/// The grants of yours set aside in the project at `root`, oldest first.
pub fn dormant_dispatch(root: &Path) -> Vec<Dormant> {
    read(root).dispatch_dormant
}

/// What this machine accepted of the project's grants and set aside, in the project at `root`.
pub fn accepted_aside_dispatch(root: &Path) -> Vec<Accepted> {
    read(root).dispatch_accepted_aside
}

/// What [`set_aside_dispatch`] took out of force.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetAside {
    /// Your grants, as they are now kept.
    pub grants: Vec<Dormant>,
    /// What this machine had accepted of the project's grants.
    pub accepted: Vec<Accepted>,
}

impl SetAside {
    /// Whether nothing was.
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty() && self.accepted.is_empty()
    }
}

/// **Sets aside every grant of yours that names a name `gone` says changed hands**, in one
/// write: each pair of `dispatch_mine` and each name of `dispatch_any` goes to
/// `dispatch_dormant`, and what this machine accepted of the project's grants for the name
/// goes from `dispatch_seen` and `dispatch_any_seen` to `dispatch_accepted_aside`, so a pair
/// the project's file still holds waits for a yes again. **Nothing is lost**: what the
/// Granted record says of each (when, and from which chat) stays, for Give back. Answers what
/// was set aside. Nothing is written where nothing names such a name.
pub fn set_aside_dispatch(root: &Path, gone: &dyn Fn(&str) -> bool) -> io::Result<SetAside> {
    if !granted_names(&read(root)).iter().any(|name| gone(name)) {
        return Ok(SetAside::default());
    }
    let mut out = SetAside::default();
    change(root, |held| {
        out = SetAside::default();
        let mut keep = Vec::new();
        for pair in std::mem::take(&mut held.dispatch_mine) {
            let was = if gone(&pair.asking) {
                Some(pair.asking.clone())
            } else if gone(&pair.target) {
                Some(pair.target.clone())
            } else {
                None
            };
            match was {
                Some(was) => out.grants.push(Dormant {
                    asking: pair.asking,
                    target: pair.target,
                    any: false,
                    was,
                }),
                None => keep.push(pair),
            }
        }
        held.dispatch_mine = keep;
        for asking in std::mem::take(&mut held.dispatch_any) {
            if gone(&asking) {
                out.grants.push(Dormant {
                    was: asking.clone(),
                    asking,
                    target: crate::dispatchgrant::ANY.to_owned(),
                    any: true,
                });
            } else {
                held.dispatch_any.push(asking);
            }
        }
        for asking in std::mem::take(&mut held.dispatch_any_seen) {
            if gone(&asking) {
                out.accepted.push(Accepted {
                    said: format!("{asking} -> {}", crate::dispatchgrant::ANY),
                    was: asking,
                });
            } else {
                held.dispatch_any_seen.push(asking);
            }
        }
        if let Some(seen) = held.dispatch_seen.as_mut() {
            for said in std::mem::take(seen) {
                let was = sides(&said).and_then(|(asking, target)| {
                    [asking, target]
                        .into_iter()
                        .find(|side| gone(side))
                        .map(str::to_owned)
                });
                match was {
                    Some(was) => out.accepted.push(Accepted { said, was }),
                    None => seen.push(said),
                }
            }
        }
        for one in &out.grants {
            if !held.dispatch_dormant.contains(one) {
                held.dispatch_dormant.push(one.clone());
            }
        }
        for said in &out.accepted {
            if !held.dispatch_accepted_aside.contains(said) {
                held.dispatch_accepted_aside.push(said.clone());
            }
        }
    })?;
    Ok(out)
}

/// Takes one grant off what was set aside: Settings' Remove. `any` says which is meant, the
/// one set aside as "any persona" or the pair, so one press takes one entry. Answers whether
/// it was there.
pub fn forget_dormant_dispatch(
    root: &Path,
    asking: &str,
    target: &str,
    any: bool,
) -> io::Result<bool> {
    let mut was = false;
    change(root, |held| {
        let before = held.dispatch_dormant.len();
        held.dispatch_dormant
            .retain(|one| !(one.asking == asking && one.target == target && one.any == any));
        was = held.dispatch_dormant.len() != before;
    })?;
    Ok(was)
}

/// **Gives `name` back what was set aside for it**, in one write: Settings' own action, on
/// the person's press. Each grant set aside for the name that `ok` passes is in force again,
/// each acceptance set aside for it that `holds` says the project's file still has is
/// accepted again, and the name's absence mark is lifted with `hash` as what is known of it.
/// What `ok` does not pass stays set aside; an acceptance the file no longer has is dropped.
/// Answers what came back.
pub fn give_back_dispatch(
    root: &Path,
    name: &str,
    hash: &str,
    ok: &dyn Fn(&Dormant) -> bool,
    holds: &dyn Fn(&str) -> bool,
) -> io::Result<SetAside> {
    let mut back = SetAside::default();
    change(root, |held| {
        back = SetAside::default();
        for one in std::mem::take(&mut held.dispatch_dormant) {
            if one.was != name || !ok(&one) {
                held.dispatch_dormant.push(one);
                continue;
            }
            if one.any {
                if !held.dispatch_any.contains(&one.asking) {
                    held.dispatch_any.push(one.asking.clone());
                }
            } else {
                let pair = DispatchPair {
                    asking: one.asking.clone(),
                    target: one.target.clone(),
                };
                if !held.dispatch_mine.contains(&pair) {
                    held.dispatch_mine.push(pair);
                }
            }
            back.grants.push(one);
        }
        for one in std::mem::take(&mut held.dispatch_accepted_aside) {
            if one.was != name {
                held.dispatch_accepted_aside.push(one);
                continue;
            }
            if !holds(&one.said) || held.dispatch_declined.contains(&one.said) {
                continue;
            }
            let said = one.said.clone();
            match sides(&said) {
                Some((asking, target)) if target == crate::dispatchgrant::ANY => {
                    if !held.dispatch_any_seen.iter().any(|one| one == asking) {
                        held.dispatch_any_seen.push(asking.to_owned());
                    }
                }
                _ => {
                    let seen = held.dispatch_seen.get_or_insert_with(Vec::new);
                    if !seen.contains(&said) {
                        seen.push(said.clone());
                    }
                }
            }
            back.accepted.push(one);
        }
        match held.dispatch_known.iter_mut().find(|one| one.name == name) {
            Some(one) => {
                hash.clone_into(&mut one.hash);
                one.away = false;
            }
            None => held.dispatch_known.push(Known {
                name: name.to_owned(),
                hash: hash.to_owned(),
                away: false,
            }),
        }
    })?;
    Ok(back)
}

/// The project's grants you said "Not on my machine" to in the project at `root`.
pub fn dispatch_declined(root: &Path) -> Vec<String> {
    read(root).dispatch_declined
}

/// **Not on my machine**, for the project's grant `said` (a pair as it is displayed, or
/// `<asking> -> *`): this machine's acceptance of it goes, and it is remembered as declined,
/// in one write.
pub fn decline_dispatch(root: &Path, said: &str) -> io::Result<()> {
    change(root, |held| {
        if let Some(seen) = held.dispatch_seen.as_mut() {
            seen.retain(|one| one != said);
        }
        if let Some((asking, target)) = sides(said)
            && target == crate::dispatchgrant::ANY
        {
            held.dispatch_any_seen.retain(|one| one != asking);
        }
        held.dispatch_accepted_aside.retain(|one| one.said != said);
        if !held.dispatch_declined.iter().any(|one| one == said) {
            held.dispatch_declined.push(said.to_owned());
        }
        // Answered: it is no longer one to say was accepted before.
        held.dispatch_gone.retain(|one| one != said);
    })
}

// ---- what an acceptance is bound to (#1506) --------------------------------------------------

/// What this machine keeps of the project's grants, as [`crate::dispatcharrival`] reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchBound {
    /// The pairs accepted here, each as [`crate::dispatchgrant::Pair`] is displayed.
    pub seen: Vec<String>,
    /// The asking personas whose "any persona" grant was accepted here.
    pub any_seen: Vec<String>,
    /// The grants declined here, a pair as it is displayed or `<asking> -> *`.
    pub declined: Vec<String>,
    /// The commit the acceptances were last checked through.
    pub at: Option<String>,
    /// The grants accepted here that the project then took away, not yet told.
    pub gone: Vec<String>,
}

/// What this machine keeps of the project's grants in the project at `root`.
pub fn dispatch_bound(root: &Path) -> DispatchBound {
    let held = read(root);
    DispatchBound {
        seen: held.dispatch_seen.unwrap_or_default(),
        any_seen: held.dispatch_any_seen,
        declined: held.dispatch_declined,
        at: held.dispatch_seen_at,
        gone: held.dispatch_gone,
    }
}

/// One settling of what this machine keeps of the project's grants
/// ([`crate::dispatcharrival::settle`]), written at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchSettled {
    /// The acceptances that no longer hold, as `dispatch_declined` spells a grant: each
    /// leaves `dispatch_seen` or `dispatch_any_seen` and is remembered in `dispatch_gone`.
    pub dropped: Vec<String>,
    /// The declines whose grant the project's file no longer holds: forgotten.
    pub undeclined: Vec<String>,
    /// The commit the acceptances are checked through now; none where nothing is accepted
    /// or no commit could be read.
    pub at: Option<String>,
}

/// Writes `settled`, in one write. Only ever narrows what is in force.
pub fn settle_dispatch(root: &Path, settled: &DispatchSettled) -> io::Result<()> {
    change(root, |held| {
        let any = |said: &str| {
            sides(said)
                .filter(|(_, target)| *target == crate::dispatchgrant::ANY)
                .map(|(asking, _)| asking.to_owned())
        };
        for said in &settled.dropped {
            let mut was = false;
            if let Some(asking) = any(said) {
                let before = held.dispatch_any_seen.len();
                held.dispatch_any_seen.retain(|one| *one != asking);
                was = held.dispatch_any_seen.len() != before;
            } else if let Some(seen) = held.dispatch_seen.as_mut() {
                let before = seen.len();
                seen.retain(|one| one != said);
                was = seen.len() != before;
            }
            if was && !held.dispatch_gone.contains(said) {
                held.dispatch_gone.push(said.clone());
            }
        }
        held.dispatch_declined
            .retain(|one| !settled.undeclined.contains(one));
        let accepted = held
            .dispatch_seen
            .as_ref()
            .is_some_and(|seen| !seen.is_empty())
            || !held.dispatch_any_seen.is_empty();
        held.dispatch_seen_at = settled.at.clone().filter(|_| accepted);
    })
}

/// Forgets that `said`, grants accepted here, were taken away: the person was told.
pub fn forget_dispatch_gone(root: &Path, said: &[String]) -> io::Result<()> {
    change(root, |held| {
        held.dispatch_gone.retain(|one| !said.contains(one))
    })
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
