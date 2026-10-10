//! **The network, as the window shows it** (#1662, spec #1661): Settings' Network page (Open
//! hosts, Allowed, Blocked lately) and each chat's Network view, both read from this machine's
//! network record (`purlis_core::sandboxblock::record`), and what writes that record from the
//! app: every block a chat's sandbox met, and every Allow and removal a person made.
//!
//! **Nothing here changes what a chat may reach.** Allow on Blocked lately is the same judged,
//! audited Allow a block's Notice makes (`sandboxing::judged`), kept for this project on this
//! machine or for everyone in it; a chat takes it from its next start, as every Allow made
//! outside its own Notice does.
//!
//! **What the record holds is data.** A chat's name is what the person called it, and a host a
//! block names came on a line the chat's hook sent: both are shown as text and never followed.

use std::path::Path;

use purlis_core::sandbox::{self, grant, hosts::Host};
use purlis_core::sandboxblock::record::{self, Entry, Record};
use purlis_core::sandboxblock::{Kind, Operation};

use crate::planes::{PlaneId, Planes};
use crate::sandboxing::{Allowed, Audit, GrantLevel, now_secs};

/// The most rows Blocked lately and a chat's refusals list: the newest.
const AT_MOST_LISTED: usize = 50;

/// The chat chat `session` is, as the record names it, and its persona.
fn chat_of(chats: &crate::chats::Chats, session: u32) -> (record::Chat, Option<String>) {
    let recorded = chats.recorded_chat(session);
    let chat = record::Chat {
        id: recorded.as_ref().and_then(|one| one.identity.id.clone()),
        name: chats.shown_name(session),
        session: Some(session),
    };
    (chat, recorded.and_then(|one| one.persona))
}

/// **Records a block the app took** for chat `blocked.chat` in the project at `root`, heard at
/// `at`: only the host a refused connection was to, of everything the line named.
pub(crate) fn record_block(
    record: &Record,
    root: &Path,
    chats: &crate::chats::Chats,
    blocked: &purlis_core::hookwire::SandboxBlocked,
    at: u64,
) {
    let (chat, persona) = chat_of(chats, blocked.chat);
    let entry = Entry::blocked(
        &blocked.sandbox_blocked,
        blocked.target.as_deref(),
        chat,
        persona.as_deref(),
        at,
    );
    if let Err(why) = record.write(root, &entry) {
        tracing::warn!(
            "purlis: a sandbox block of chat {} was not kept in the network record ({why})",
            blocked.chat
        );
    }
}

/// Connections purlis's own proxy carried for one chat, as its tally told them (#1664).
pub(crate) struct Connections<'a> {
    /// The chat's number: the one whose proxy it came in on.
    pub session: u32,
    pub who: &'a crate::chats::ReachedAs,
    /// The host and port, where the tally told one apart.
    pub target: Option<&'a str>,
    /// The layer that let them through.
    pub by: &'static str,
    pub times: u64,
}

/// **Records connections purlis's own proxy carried** for a chat in the project at `root`, told
/// at `at`. The chat is who its proxy started for; its name, while it is still open.
pub(crate) fn record_connections(
    record: &Record,
    root: &Path,
    chats: &crate::chats::Chats,
    connections: &Connections<'_>,
    at: u64,
) {
    let chat = record::Chat {
        id: connections.who.id.clone(),
        name: chats.shown_name(connections.session),
        session: Some(connections.session),
    };
    let entry = Entry::connected(
        connections.target,
        connections.by,
        connections.times,
        chat,
        connections.who.persona.as_deref(),
        at,
    );
    if let Err(why) = record.write(root, &entry) {
        tracing::warn!(
            "purlis: connections of chat {} were not kept in the network record ({why})",
            connections.session
        );
    }
}

/// **`audit`, then the network record**: every Allow and removal a person makes is audited
/// first (an audit that cannot be written changes nothing), then written to `record` with the
/// chat it came from. A record that cannot be written is said in the log and changes nothing:
/// the Allow stands, and is audited.
pub(crate) fn recorded<'a>(
    audit: Audit<'a>,
    record: Option<&'a Record>,
    root: &'a Path,
    chats: &'a crate::chats::Chats,
) -> impl Fn(Option<u32>, &grant::Audited<'_>) -> Result<(), String> + 'a {
    move |number, audited| {
        audit(number, audited)?;
        if let Some(record) = record {
            let at = now_secs();
            let entry = if audited.granted {
                let (chat, persona) = number.map(|n| chat_of(chats, n)).unwrap_or_default();
                Entry::allowed(
                    audited.what,
                    audited.target,
                    audited.level.word(),
                    chat,
                    persona.as_deref(),
                    at,
                )
            } else {
                Entry::removed(audited.what, audited.target, audited.level.word(), at)
            };
            if let Err(why) = record.write(root, &entry) {
                tracing::warn!("purlis: an Allow was not kept in the network record ({why})");
            }
        }
        Ok(())
    }
}

/// **Records a host added, confirmed or removed in Settings** (#1341's Your hosts and the
/// project's hosts), once `written` says it was: an Allow or a removal at this project on this
/// machine (`local`) or everyone in it (`shared`), decided by the person at this machine.
pub(crate) fn record_settings_host(
    record: Option<&Record>,
    root: &Path,
    written: &crate::settings::EntryWritten,
    allowed: bool,
    which: crate::settings::SettingsWhich,
) {
    let Some(record) = record else { return };
    let crate::settings::EntryWritten::Saved { added, removed, .. } = written else {
        return;
    };
    let host = if allowed {
        added.clone()
    } else {
        removed
            .as_ref()
            .and_then(|values| values.iter().find(|one| one.field == "host"))
            .map(|one| one.value.clone())
    };
    let Some(host) = host else { return };
    let scope = match which {
        crate::settings::SettingsWhich::Shared => grant::Level::Project,
        crate::settings::SettingsWhich::Local => grant::Level::You,
    }
    .word();
    let at = now_secs();
    let entry = if allowed {
        Entry::allowed("host", &host, scope, record::Chat::default(), None, at)
    } else {
        Entry::removed("host", &host, scope, at)
    };
    if let Err(why) = record.write(root, &entry) {
        tracing::warn!("purlis: a host change was not kept in the network record ({why})");
    }
}

/// One group of **Open hosts**: a preset's, or the project's own.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct OpenHosts {
    /// The preset's name in the window ("AI providers"), or "This project's hosts".
    pub title: String,
    /// Each host, spelled out.
    pub hosts: Vec<String>,
}

/// One row of **Blocked lately**, or of a chat's refusals: what was refused, the newest time,
/// and how often.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct BlockedLately {
    /// The host and port, where the block named one.
    pub target: Option<String>,
    /// What was blocked, as the block's Notice says it.
    pub said: String,
    /// The chat it was, by the name it was shown under, where the record has it.
    pub chat: Option<String>,
    /// When it was last refused, in seconds since 1970.
    pub at: u32,
    /// How many times it was refused in the record.
    pub times: u32,
    /// Whether a chat reaches it now: it is an Open host, or allowed since.
    pub reached: bool,
    /// The scopes Allow may keep it at here: none where there is nothing to allow (no host
    /// named, reached already, not a host a proxy refuses) or policy forbids it.
    pub levels: Vec<GrantLevel>,
}

/// **Settings' Network page** (#1662): Open hosts and Blocked lately. Allowed is the list
/// `sandbox_grants` answers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SandboxNetwork {
    /// Whether chats here run sandboxed. Where they do not, nothing is open or blocked.
    pub on: bool,
    pub open: Vec<OpenHosts>,
    pub blocked: Vec<BlockedLately>,
}

/// Why a chat reaches a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ReachedBy {
    /// An Open host: a preset's, or the project's own.
    Open,
    /// One of its persona's hosts, allowed on this machine.
    Persona,
    /// An Allowed host: for this chat, this project on this machine, or everyone in it.
    Allowed,
}

/// One host a chat reaches, and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Reached {
    pub host: String,
    pub by: ReachedBy,
}

/// **A chat's Network view** (#1662): what it can reach now and what it was refused.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ChatNetwork {
    /// Whether the chat is open in this app. A closed one has nothing to say.
    pub open: bool,
    /// Whether this run of the chat is sandboxed. One that is not can reach anything, and
    /// purlis records nothing of it.
    pub sandboxed: bool,
    /// What its sandbox lets it reach, as it was started.
    pub reach: Vec<Reached>,
    /// What it was refused, newest first.
    pub refused: Vec<BlockedLately>,
}

/// `plane`'s Open hosts under `locks`: each preset in force, then its own hosts. None where
/// its chats are not sandboxed.
fn open_of(plane: &sandbox::Plane, locks: &sandbox::policy::Locks) -> Option<Vec<OpenHosts>> {
    let policy = plane.in_force(locks)?;
    let mut out: Vec<OpenHosts> = locks
        .presets(&policy.egress)
        .into_iter()
        .map(|preset| OpenHosts {
            title: preset.title().to_owned(),
            hosts: sandbox::hosts(&[preset], plane, locks),
        })
        .collect();
    let own: Vec<String> = plane.granted_hosts(locks);
    let presets: Vec<String> = out.iter().flat_map(|one| one.hosts.clone()).collect();
    let own: Vec<String> = own
        .into_iter()
        .filter(|host| !presets.contains(host))
        .collect();
    if !own.is_empty() {
        out.push(OpenHosts {
            title: "This project's hosts".to_owned(),
            hosts: own,
        });
    }
    Some(out)
}

/// Whether one of `listed` (as a sandbox lists hosts) lets a chat reach `target`.
fn covered(listed: &[String], target: &Host) -> bool {
    listed
        .iter()
        .filter_map(|one| Host::parse(one).ok())
        .any(|one| one.covers(target))
}

/// Every host every chat here reaches on this machine: the Open hosts and yours.
fn reached_here(root: &Path, open: &[OpenHosts]) -> Vec<String> {
    open.iter()
        .flat_map(|one| one.hosts.clone())
        .chain(
            sandbox::hosts::personal(root)
                .iter()
                .map(ToString::to_string),
        )
        .collect()
}

/// The scopes Allow on a row for `target` may keep it at, under `locks`.
fn levels_for(target: &Host, locks: &sandbox::policy::Locks) -> Vec<GrantLevel> {
    [grant::Level::You, grant::Level::Project]
        .into_iter()
        .filter(|level| {
            locks
                .refuses_grant(&grant::What::Host(target.clone()), *level)
                .is_none()
        })
        .map(GrantLevel::from)
        .collect()
}

/// The rows `entries` make, newest first: one per host refused (or per kind of refusal where
/// none was named), with how often, judged against `reached` and `locks`.
fn rows_of(
    entries: &[Entry],
    reached: &[String],
    locks: &sandbox::policy::Locks,
) -> Vec<BlockedLately> {
    let mut rows: Vec<BlockedLately> = Vec::new();
    for entry in entries.iter().rev().filter(|entry| entry.is_host_block()) {
        let Some(block) = entry.block else { continue };
        let said = block.said();
        let at = u32::try_from(entry.at).unwrap_or(u32::MAX);
        if let Some(row) = rows
            .iter_mut()
            .find(|row| row.target == entry.target && row.said == said)
        {
            row.times = row.times.saturating_add(1);
            continue;
        }
        let host = entry
            .target
            .as_deref()
            .filter(|_| (block.operation, block.kind) == (Operation::Connect, Kind::Host))
            .and_then(|target| Host::parse(target).ok());
        let is_reached = host.as_ref().is_some_and(|host| covered(reached, host));
        rows.push(BlockedLately {
            target: entry.target.clone(),
            said,
            chat: entry.chat.name.clone(),
            at,
            times: 1,
            reached: is_reached,
            levels: match &host {
                Some(host) if !is_reached => levels_for(host, locks),
                _ => Vec::new(),
            },
        });
    }
    rows.truncate(AT_MOST_LISTED);
    rows
}

/// [`SandboxNetwork`] for the project at `root`, whose committed file reads as `plane`, from
/// `record` at `now`.
fn network_of(
    root: &Path,
    plane: &sandbox::Plane,
    record: Option<&Record>,
    now: u64,
) -> SandboxNetwork {
    let locks = sandbox::policy::Locks::of(root);
    let Some(open) = open_of(plane, &locks) else {
        return SandboxNetwork {
            on: false,
            open: Vec::new(),
            blocked: Vec::new(),
        };
    };
    let entries = record.map(|one| one.read(root, now)).unwrap_or_default();
    let blocked = rows_of(&entries, &reached_here(root, &open), &locks);
    SandboxNetwork {
        on: true,
        open,
        blocked,
    }
}

/// **Open hosts and Blocked lately** for Settings' Network page (#1662).
#[tauri::command]
#[specta::specta]
pub fn sandbox_network(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<SandboxNetwork, String> {
    let held = planes.held(&plane)?;
    let root = held.root();
    Ok(network_of(
        root,
        &sandbox::Plane::read(root),
        planes.network(),
        now_secs(),
    ))
}

/// [`ChatNetwork`] for chat `session` of the project at `root`.
fn chat_network_of(
    root: &Path,
    chats: &crate::chats::Chats,
    session: u32,
    record: Option<&Record>,
    now: u64,
) -> ChatNetwork {
    let Some(recorded) = chats.recorded_chat(session) else {
        return ChatNetwork {
            open: false,
            sandboxed: false,
            reach: Vec::new(),
            refused: Vec::new(),
        };
    };
    let confines = chats
        .confines_of(session)
        .filter(|_| !chats.unsandboxed(session));
    let Some(confines) = confines else {
        return ChatNetwork {
            open: true,
            sandboxed: false,
            reach: Vec::new(),
            refused: Vec::new(),
        };
    };
    let locks = sandbox::policy::Locks::of(root);
    let plane = sandbox::Plane::read(root);
    let open: Vec<String> = open_of(&plane, &locks)
        .unwrap_or_default()
        .into_iter()
        .flat_map(|one| one.hosts)
        .collect();
    let persona: Vec<String> = recorded
        .persona
        .as_deref()
        .map(|persona| {
            sandbox::persona::shown_in(root, &plane, &locks)
                .into_iter()
                .filter(|one| one.persona == persona && one.allowed())
                .flat_map(|one| one.hosts)
                .map(|host| host.to_string())
                .collect()
        })
        .unwrap_or_default();
    let entries = record.map(|one| one.read(root, now)).unwrap_or_default();
    ChatNetwork {
        open: true,
        sandboxed: true,
        reach: reach_of(&confines.hosts, &open, &persona),
        refused: refused_of(
            &entries,
            (recorded.identity.id.as_deref(), session),
            &confines.hosts,
            &locks,
        ),
    }
}

/// Each of `hosts` a chat's sandbox reaches, by why: one of the `open` hosts, one of its
/// `persona`'s, or else one allowed.
fn reach_of(hosts: &[String], open: &[String], persona: &[String]) -> Vec<Reached> {
    hosts
        .iter()
        .map(|host| Reached {
            host: host.clone(),
            by: if open.contains(host) {
                ReachedBy::Open
            } else if persona.contains(host) {
                ReachedBy::Persona
            } else {
                ReachedBy::Allowed
            },
        })
        .collect()
}

/// What the chat whose id is `id` (or, with none, whose number is `session`) was refused, as
/// rows judged against what it `reached` and `locks`: by its id, which outlives a restart and a
/// relaunch, so another chat that has its number since is never shown as it.
fn refused_of(
    entries: &[Entry],
    (id, session): (Option<&str>, u32),
    reached: &[String],
    locks: &sandbox::policy::Locks,
) -> Vec<BlockedLately> {
    let mine: Vec<Entry> = entries
        .iter()
        .filter(|entry| match (id, entry.chat.id.as_deref()) {
            (Some(id), Some(theirs)) => id == theirs,
            (None, None) => entry.chat.session == Some(session),
            _ => false,
        })
        .cloned()
        .collect();
    rows_of(&mine, reached, locks)
        .into_iter()
        .map(|row| BlockedLately { chat: None, ..row })
        .collect()
}

/// **A chat's Network view** (#1662): what chat `session` reaches now, and what it was refused.
#[tauri::command]
#[specta::specta]
pub fn chat_network(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<ChatNetwork, String> {
    let held = planes.held(&plane)?;
    Ok(chat_network_of(
        held.root(),
        held.chats(),
        session,
        planes.network(),
        now_secs(),
    ))
}

/// **Allow on Blocked lately** in the project at `root` (#1662): `host`, judged by the core as a
/// block's Allow is (never the window's word, never past a policy lock), then audited and kept
/// by the same path a block's Notice keeps it by (`sandboxing::kept_for`), for this project on
/// this machine or for everyone in it. No chat is named, so none is owed a restart: each takes
/// it from its next start.
fn allow_blocked(
    root: &Path,
    chats: &crate::chats::Chats,
    host: &str,
    level: GrantLevel,
    audit: Audit<'_>,
    at: u64,
) -> Result<Allowed, String> {
    let (what, level) = crate::sandboxing::judged(
        root,
        &sandbox::Machine::this(),
        None,
        0,
        (crate::sandboxing::GrantWhat::Host, host, level),
    )?;
    crate::sandboxing::kept_for(root, chats, None, (&what, level), audit, at)?;
    Ok(Allowed {
        said: format!(
            "Allowed {} {}. A chat that is running reaches it from its next start.",
            what.target(),
            level.said()
        ),
    })
}

/// **Allow** on a row of Blocked lately (#1662): the window's alone (`WINDOW_ONLY`).
#[tauri::command]
#[specta::specta]
pub fn allow_blocked_host(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    host: String,
    level: GrantLevel,
) -> Result<Allowed, String> {
    let held = planes.held(&plane)?;
    let root = held.root().to_path_buf();
    let audit = |number: Option<u32>, audited: &grant::Audited<'_>| {
        held.hooks().record_grant(&root, number, audited)
    };
    let audit = recorded(&audit, planes.network(), &root, held.chats());
    allow_blocked(&root, held.chats(), &host, level, &audit, now_secs())
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
