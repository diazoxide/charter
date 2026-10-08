//! **A grant** (#1342, spec #1330): what a person allows a sandboxed chat past its project's
//! sandbox, from the Notice a block raised: a host to reach, or a folder to write.
//!
//! # What can be granted, and what never can
//!
//! - **A host** ([`Host`]): the project's own hosts' rules ([`super::hosts`]), so a grant never
//!   reaches this machine, a link-local or metadata address, or a name ending in a number.
//! - **A folder to write** ([`write`]), **only from an allowlist**, judged on the folder the
//!   kernel would write (D-1342-12: resolved first, so a link the chat made judges as where it
//!   points, and only that resolved folder is kept and compiled):
//!   - inside the project tree;
//!   - a folder you listed in Settings › Sandbox as one chats may be granted
//!     ([`super::local::grantable_folders`], this machine only), as it was resolved when you
//!     listed it: one that has since come to resolve elsewhere is no longer on it (R8).
//!
//!   No temp folder is on it (D-1342-14): no per-chat temp folder is known to grant, and a
//!   harness's sandboxed temp folder is shared by every session of its user. No cache folder is
//!   on it by default (D-1342-11): a cache is shared by every project, and many hold code
//!   another program later runs. On top of the allowlist, never `/`, the home
//!   folder, the project or a folder above it; never a folder on `PATH`, a harness's own home or
//!   temp folder ([`Ground::of`]); and never anything the sandbox denies by class (ADR 0067 §5).
//!   A class is refused with the way that does work ([`Refused::Never`]); anything else with why,
//!   and the Notice offers the person "Start without the sandbox" for that chat
//!   ([`Refused::Outside`]), so a block never dead-ends. On macOS every comparison is
//!   case-folded, as its volumes are. A granted folder that *holds* a denied path keeps the
//!   denial: every compiler writes its denials after its grants, and Claude Code's later-code
//!   names are denied again under each granted folder by absolute path.
//!
//! **No read is granted.** A chat's sandbox denies reads only for the classes above, so a read
//! block is always one of those and is answered with the brokered route.
//!
//! # Levels
//!
//! - **This chat** ([`Level::Chat`]): held by the app for that one chat, never on disk, and handed
//!   to its starts alone ([`Grants`]); a chat's restart takes it with the conversation.
//! - **You** ([`Level::You`]): this machine only. A host is one of your own hosts (#1341); a
//!   folder is kept in `app/sandbox.json` ([`super::local::grant_write`]), which no sandboxed
//!   chat writes.
//! - **The project** ([`Level::Project`]): a host only, as one of the project's own hosts in the
//!   committed file. A folder is a path on one machine, so it is never the project's.
//!
//! Every grant is audited (`trust.sandbox.grant`), and so is every revoke.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use super::hosts::Host;
use super::{Class, Denial};

/// What a grant lets a chat do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum What {
    /// Reach a host.
    Host(Host),
    /// Write a folder and everything in it.
    Write(PathBuf),
}

impl What {
    /// The word it is told and audited by: `host` or `write`.
    pub fn word(&self) -> &'static str {
        match self {
            Self::Host(_) => "host",
            Self::Write(_) => "write",
        }
    }

    /// What it names, as the sandbox writes it: the host, or the folder.
    pub fn target(&self) -> String {
        match self {
            Self::Host(host) => host.to_string(),
            Self::Write(path) => path.display().to_string(),
        }
    }
}

/// What a grant says, as a sentence ends: "reaching api.example.com", "writing /opt/cache".
impl fmt::Display for What {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(host) => write!(f, "reaching {host}"),
            Self::Write(path) => write!(f, "writing {} and everything in it", path.display()),
        }
    }
}

/// Who a grant is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The one chat the block was in, for as long as the app holds it.
    Chat,
    /// Every chat of this project on this machine.
    You,
    /// Every chat of everyone who opens the project: committed.
    Project,
}

impl Level {
    /// The word it is told, audited and listed by.
    pub fn word(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::You => "you",
            Self::Project => "project",
        }
    }

    /// The level `word` names.
    pub fn of_word(word: &str) -> Option<Self> {
        [Self::Chat, Self::You, Self::Project]
            .into_iter()
            .find(|level| level.word() == word)
    }

    /// The level of a host at this level, as the project's hosts' locks ask it
    /// ([`super::hosts::Locks`]): this chat's and yours are this machine's.
    pub fn hosts_level(self) -> super::hosts::Level {
        match self {
            Self::Chat | Self::You => super::hosts::Level::You,
            Self::Project => super::hosts::Level::Project,
        }
    }

    /// The level as the Notice says it, in the person's own words.
    pub fn said(self) -> &'static str {
        match self {
            Self::Chat => "for this chat",
            Self::You => "for me on this machine",
            Self::Project => "for everyone in this project",
        }
    }
}

/// **One chat's grants**, as a start compiles them in beside the project's and this machine's
/// ([`super::Compiled::granted`]). Empty for every start but a chat the person allowed something
/// for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    pub hosts: Vec<Host>,
    pub writes: Vec<PathBuf>,
}

impl Grants {
    /// Whether it grants nothing.
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty() && self.writes.is_empty()
    }

    /// `what` added, once.
    pub fn add(&mut self, what: &What) {
        match what {
            What::Host(host) if !self.hosts.contains(host) => self.hosts.push(host.clone()),
            What::Write(path) if !self.writes.contains(path) => self.writes.push(path.clone()),
            What::Host(_) | What::Write(_) => {}
        }
    }
}

/// Where a write grant is judged: the project, the chat's own folder, the home folder, what the
/// chat is denied ([`super::Denied::of`], with the keychain files), the allowlist, the folders
/// refused on top of it, and whether names compare case-folded. [`Ground::of`] makes one.
#[derive(Debug, Clone, Copy)]
pub struct Place<'a> {
    pub root: &'a Path,
    pub chat: &'a Path,
    pub home: Option<&'a Path>,
    pub denied: &'a [Denial],
    /// Folders a grant may be in or be: the ones you listed in Settings, each as it was
    /// resolved when listed, and compared as that spelling, never resolved again.
    pub allowed: &'a [PathBuf],
    /// Folders a grant may neither be in nor hold: those on `PATH`, and each harness's home and
    /// temp folders.
    pub refused: &'a [PathBuf],
    /// The names a harness's own temp root starts with in a system temp folder (`claude-`):
    /// refused too. [`Ground::of`] gives [`HARNESS_TEMP`]; a test of the rest names none.
    pub harness_temp: &'a [&'a str],
    /// Whether names compare case-folded: macOS's volumes do.
    pub folded: bool,
}

/// [`Place`]'s lists, owned: what this machine says for a project and a chat.
#[derive(Debug, Clone, Default)]
pub struct Ground {
    pub root: PathBuf,
    pub chat: PathBuf,
    pub home: Option<PathBuf>,
    pub denied: Vec<Denial>,
    pub allowed: Vec<PathBuf>,
    pub refused: Vec<PathBuf>,
    pub harness_temp: &'static [&'static str],
    pub folded: bool,
}

/// The system temp folders, as the kernel names them: a harness's own temp roots in them are
/// refused, and nothing in them is on the allowlist (D-1342-14).
const SYSTEM_TEMP: [&str; 2] = ["/private/tmp", "/private/var/folders"];

/// The prefix of a harness's own temp root in a system temp folder (`claude-<uid>`).
pub const HARNESS_TEMP: [&str; 1] = ["claude-"];

/// The harness temp roots [`Ground::of`] refuses: [`HARNESS_TEMP`], always, outside this crate's
/// own tests.
#[cfg(not(test))]
fn harness_temp() -> &'static [&'static str] {
    &HARNESS_TEMP
}

#[cfg(test)]
thread_local! {
    /// A test's seam (round 4): a fixture project made in a temp folder may itself sit in a
    /// harness's temp root (`/private/tmp/claude-<uid>` under an agent), which the rule refuses
    /// as it must. A test of the rest names no root here; the rule has a test of its own.
    pub(crate) static TEST_HARNESS_TEMP: std::cell::Cell<&'static [&'static str]> =
        const { std::cell::Cell::new(&HARNESS_TEMP) };
}

#[cfg(test)]
fn harness_temp() -> &'static [&'static str] {
    TEST_HARNESS_TEMP.with(std::cell::Cell::get)
}

/// Every harness's own home and temp folders on `machine`, which a chat of that harness may
/// write: Codex's home and `homes.codex_project`, opencode's and Claude Code's folders under
/// each base directory and under the home, and `$CLAUDE_CODE_TMPDIR`. One list, asked by
/// [`Ground::of`] (no such folder is granted to a chat) and by the lookup of a provider's
/// program (no program is run from one, [`crate::secrets::program`]).
pub fn harness_homes(machine: &super::Machine, homes: &super::Homes) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = homes.codex.iter().cloned().collect();
    out.extend(homes.codex_project.iter().cloned());
    for base in [&homes.config, &homes.data, &homes.state, &homes.cache]
        .into_iter()
        .flatten()
    {
        out.push(base.join("opencode"));
        out.push(base.join("claude"));
    }
    if let Some(home) = &machine.home {
        out.push(home.join(".claude"));
        out.push(home.join("Library/Caches/opencode"));
        out.push(home.join("Library/Caches/claude"));
    }
    out.extend(
        machine
            .env
            .get("CLAUDE_CODE_TMPDIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.parent().is_some()),
    );
    out
}

impl Ground {
    /// The ground for a chat in `chat` of the project at `root` on `machine`: its denials with
    /// the keychain files; the folders you listed for this project that still resolve to
    /// themselves as the allowlist; and the folders on `PATH`, every harness's own home and temp
    /// folders refused.
    pub fn of(root: &Path, chat: &Path, machine: &super::Machine) -> Self {
        let mut denied = super::Denied::of(root, machine).paths;
        denied.extend(super::seatbelt::keychains(machine.home.as_deref()));
        let homes = super::Homes::of(machine);
        let mut refused: Vec<PathBuf> = machine
            .env
            .get("PATH")
            .map(|path| {
                std::env::split_paths(&path)
                    .filter(|dir| dir.is_absolute() && dir.parent().is_some())
                    .collect()
            })
            .unwrap_or_default();
        refused.extend(harness_homes(machine, &homes));
        Self {
            root: root.to_path_buf(),
            chat: chat.to_path_buf(),
            home: machine.home.clone(),
            denied,
            allowed: super::local::grantable_folders(root)
                .into_iter()
                .filter(|listed| still_itself(listed))
                .collect(),
            refused,
            harness_temp: harness_temp(),
            folded: machine.os == super::Os::MacOs,
        }
    }

    /// As a [`Place`].
    pub fn place(&self) -> Place<'_> {
        Place {
            root: &self.root,
            chat: &self.chat,
            home: self.home.as_deref(),
            denied: &self.denied,
            allowed: &self.allowed,
            refused: &self.refused,
            harness_temp: self.harness_temp,
            folded: self.folded,
        }
    }
}

/// **Why a grant is refused.** None of the sentences names the folder: it is a path the chat
/// chose, and the window shows it apart from purlis's words.
/// - [`Refused::Never`]: a denial class, which no person can grant; it says the way that works.
/// - [`Refused::Outside`]: off the allowlist, or a folder on `PATH` or a harness's own; it says
///   why, and the Notice offers the person "Start without the sandbox" for that chat.
/// - [`Refused::Not`]: not a folder or host a grant can name at all, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    Never(String),
    Outside(String),
    Not(String),
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Never(why) | Self::Outside(why) | Self::Not(why) => write!(f, "{why}"),
        }
    }
}

/// The way that works for something of `class`, which is never granted (ADR 0067 §5).
pub fn brokered_route(class: Class) -> &'static str {
    match class {
        Class::Vaults => {
            "A chat never reads a vault. Run the command through `purlis secret exec <vault> -- \
             <command>`, which hands it the secret without the chat seeing it."
        }
        Class::Integrity => {
            "purlis keeps its own state, and a chat never writes it. Use purlis's own way for what \
             lives there: `purlis session record` for a session record, and purlis's memory and \
             todo tools for the rest."
        }
        Class::HumanPowers | Class::RunnerInternals => {
            "That holds what only a person approves, so it is never a chat's to write. Make the \
             change yourself, in the window or your own terminal."
        }
        Class::LaterCode => {
            "A later program loads code or settings from that, outside any sandbox, so no chat \
             ever writes it. Make the change yourself, in your own editor or terminal."
        }
    }
}

/// `typed` as a host a grant names, or why not.
pub fn host(typed: &str) -> Result<Host, Refused> {
    Host::parse(typed).map_err(Refused::Not)
}

/// `path` with `.` and `..` taken out, without asking the file system.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `path` as it is compared at `place`: case-folded where names are.
fn key(path: &Path, place: &Place<'_>) -> PathBuf {
    if place.folded {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

/// Whether `resolved` (a kernel spelling) is `outer` or below it, in `outer`'s real spelling.
/// The project tree's test: only where the kernel would write counts.
fn inside_real(resolved: &Path, outer: &Path, place: &Place<'_>) -> bool {
    key(resolved, place).starts_with(key(&super::real(outer), place))
}

/// Whether `resolved` is `listed` or below it, as `listed` is spelled: a folder you listed is
/// kept resolved, and never resolved again, so a link put in its place later widens nothing
/// (R8).
fn inside_as_listed(resolved: &Path, listed: &Path, place: &Place<'_>) -> bool {
    key(resolved, place).starts_with(key(listed, place))
}

/// **Whether `listed` still resolves to itself**: a folder you listed, or a grant kept, that it
/// or a folder above it has not since been swapped for a link (D-1342-12, R8).
pub fn still_itself(listed: &Path) -> bool {
    super::real(listed) == listed
}

/// Whether `resolved` is `outer` or below it under either of `outer`'s spellings, as written or
/// as the kernel names it. A refusal's test: either spelling refuses.
fn inside_any(resolved: &Path, outer: &Path, place: &Place<'_>) -> bool {
    let resolved = key(resolved, place);
    resolved.starts_with(key(outer, place)) || resolved.starts_with(key(&super::real(outer), place))
}

/// Whether `resolved` holds `inner`, under either of `inner`'s spellings.
fn holds_any(resolved: &Path, inner: &Path, place: &Place<'_>) -> bool {
    let resolved = key(resolved, place);
    key(inner, place).starts_with(&resolved)
        || key(&super::real(inner), place).starts_with(&resolved)
}

/// **`typed` as a folder a grant lets a chat write**, judged at `place`, or why not: the folder
/// the kernel would write ([`super::real`]), which is what is answered, kept and compiled
/// (D-1342-12).
pub fn write(typed: &str, place: &Place<'_>) -> Result<PathBuf, Refused> {
    let resolved = judged(typed, place)?;
    let allowed = inside_real(&resolved, place.root, place)
        || place
            .allowed
            .iter()
            .any(|dir| inside_as_listed(&resolved, dir, place));
    if !allowed {
        return Err(Refused::Outside(
            "purlis will not let a chat write that folder: a chat may be granted a folder in \
             this project, or in one you listed in Settings › Sandbox, because the rest is \
             shared with other programs, and much of it is loaded later outside any sandbox."
                .to_owned(),
        ));
    }
    Ok(resolved)
}

/// **`typed` as a folder you list in Settings as one chats may be granted** (D-1342-10): every
/// refusal [`write`] makes but the allowlist itself, which this adds to. Resolved, as a grant
/// is.
pub fn grantable(typed: &str, place: &Place<'_>) -> Result<PathBuf, Refused> {
    judged(typed, place)
}

/// What a folder you are about to list holds that later code is loaded from, outside any
/// sandbox, each named: what Settings warns of before it is listed. A folder that holds one is
/// still listed (the denials still hold inside it), but the person should know.
pub fn holds_later_code(folder: &Path, place: &Place<'_>) -> Vec<String> {
    let mut held: Vec<PathBuf> = place.refused.to_vec();
    if let Some(home) = place.home {
        for name in [
            "Library/LaunchAgents",
            ".ssh",
            ".config",
            "Library/Application Support",
        ] {
            held.push(home.join(name));
        }
    }
    held.into_iter()
        .filter(|inner| holds_any(folder, inner, place))
        .map(|inner| inner.display().to_string())
        .collect()
}

/// Every refusal of a write grant but the allowlist's, judged on the folder the kernel would
/// write; answers that folder.
fn judged(typed: &str, place: &Place<'_>) -> Result<PathBuf, Refused> {
    let typed = typed.trim();
    if typed.chars().any(char::is_control) || typed.contains('`') {
        return Err(Refused::Not(
            "purlis will not grant a folder with a control character or a backtick in its name."
                .to_owned(),
        ));
    }
    let asked = Path::new(typed);
    if !asked.is_absolute() {
        return Err(Refused::Not(
            "purlis grants a folder by its whole path, from /.".to_owned(),
        ));
    }
    let resolved = super::real(&lexical(asked));
    if resolved.parent().is_none() {
        return Err(Refused::Not(
            "purlis will not grant the whole machine. Name the one folder the chat needs."
                .to_owned(),
        ));
    }
    if place
        .home
        .is_some_and(|home| holds_any(&resolved, home, place))
    {
        return Err(Refused::Not(
            "purlis will not grant a folder that holds your whole home folder. Name the one \
             folder the chat needs."
                .to_owned(),
        ));
    }
    if holds_any(&resolved, place.root, place) || holds_any(&resolved, place.chat, place) {
        return Err(Refused::Not(
            "purlis will not grant a folder that holds this project: the chat's sandbox already \
             decides it."
                .to_owned(),
        ));
    }
    // A harness's own temp root in a system temp folder (`/private/tmp/claude-<uid>`).
    let harness_temp_root = |path: &Path| -> Option<PathBuf> {
        SYSTEM_TEMP.iter().find_map(|temp| {
            let temp = key(Path::new(temp), place);
            let first = key(path, place)
                .strip_prefix(&temp)
                .ok()?
                .components()
                .next()?
                .as_os_str()
                .to_string_lossy()
                .into_owned();
            place
                .harness_temp
                .iter()
                .any(|prefix| first.starts_with(prefix))
                .then(|| temp.join(first))
        })
    };
    // Never weakened, wherever the project is: a project under a refused folder has its own
    // folders refused with it, beyond the chat's own, which it writes already.
    if harness_temp_root(&resolved).is_some()
        || place
            .refused
            .iter()
            .any(|dir| inside_any(&resolved, dir, place) || holds_any(&resolved, dir, place))
    {
        return Err(Refused::Outside(
            "purlis will not let a chat write that folder: programs are run from it, or it is a \
             harness's own, outside any sandbox."
                .to_owned(),
        ));
    }
    // purlis's state folder, under either spelling, whether or not this project has one yet.
    if crate::names::STATE_DIR
        .spellings()
        .any(|name| inside_any(&resolved, &place.root.join(name), place))
    {
        return Err(Refused::Never(brokered_route(Class::Integrity).to_owned()));
    }
    if let Some(denial) = place
        .denied
        .iter()
        .find(|denial| inside_any(&resolved, &denial.path, place))
    {
        return Err(Refused::Never(brokered_route(denial.class).to_owned()));
    }
    // A project manifest too, wherever it is: a granted folder is written past what the chat's
    // own start compiled, so the manifests are held in it by name as the later-code names are
    // (D-T55-4), not only in the chat's own chain of folders (#1336).
    let planted = |path: &Path| {
        crate::sandboxblock::is_planted(path)
            || names_a_manifest(path)
            || (place.folded
                && (crate::sandboxblock::is_planted(&key(path, place))
                    || names_a_manifest(&key(path, place))))
    };
    if planted(&resolved) || planted(&lexical(asked)) {
        return Err(Refused::Never(brokered_route(Class::LaterCode).to_owned()));
    }
    Ok(resolved)
}

/// Whether any part of `path` is a project manifest's name ([`super::MANIFESTS`]).
fn names_a_manifest(path: &Path) -> bool {
    path.components().any(|part| match part {
        std::path::Component::Normal(name) => name
            .to_str()
            .is_some_and(|name| super::MANIFESTS.contains(&name)),
        _ => false,
    })
}

/// `writes`, less each folder that is no longer one a grant may name at `place`, or that no
/// longer resolves to itself (it, or a folder above it, was swapped for a link): what a start
/// compiles in, judged again then (D-1342-12).
pub fn still_grantable(writes: &[PathBuf], place: &Place<'_>) -> Vec<PathBuf> {
    // `write` resolves; one that resolves elsewhere than it was kept is dropped below.
    writes
        .iter()
        .filter(|path| {
            write(&path.display().to_string(), place).is_ok_and(|resolved| resolved == **path)
        })
        .fold(Vec::new(), |mut out, path| {
            if !out.contains(path) {
                out.push(path.clone());
            }
            out
        })
}

/// **The folder a write block proposes**: the folder the refused `path` is in, or the path itself
/// where it is a folder already. What the Notice shows, whole, before anyone allows it.
pub fn proposed_folder(path: &Path) -> PathBuf {
    let path = lexical(path);
    if path.is_dir() {
        return path;
    }
    path.parent().map_or(path.clone(), Path::to_path_buf)
}

/// **A grant or a revoke, as the audit records it** (`trust.sandbox.grant`,
/// `trust.sandbox.revoke`; ADR 0075 §4): who (the person, by scope, never a login), the level,
/// and what it named. Which chat it came from, when and on which machine are the envelope's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Audited<'a> {
    /// A grant made, or one taken back.
    pub granted: bool,
    /// `host` or `write`.
    pub what: &'a str,
    /// The host or the folder.
    pub target: &'a str,
    pub level: Level,
}

impl Audited<'_> {
    /// The event's kind.
    pub fn kind(&self) -> &'static str {
        if self.granted {
            "trust.sandbox.grant"
        } else {
            "trust.sandbox.revoke"
        }
    }

    /// The event's body.
    pub fn body(&self) -> serde_json::Value {
        serde_json::json!({
            "actor_kind": "human",
            "actor": "operator",
            "scope": "local-ui",
            "level": self.level.word(),
            "what": self.what,
            "target": self.target,
        })
    }
}

/// **What the chat is told once a grant reaches it**: its first message when it restarts on the
/// same conversation, so it retries with nothing typed.
///
/// The target is in backticks: it is a host or a path the block named, and the sentence around
/// it is purlis's own.
pub fn told(what: &What, level: Level) -> String {
    // A host never holds a backtick, and a folder with one is never granted ([`write`]); one is
    // dropped here all the same, so nothing ever closes the span early.
    let quoted = |text: String| format!("`{}`", text.replace('`', ""));
    let allowed = match what {
        What::Host(host) => format!("reaching {}", quoted(host.to_string())),
        What::Write(path) => format!(
            "writing {} and everything in it",
            quoted(path.display().to_string())
        ),
    };
    let whom = match level {
        Level::Chat => "for this chat",
        Level::You => "for every chat of this project on their machine",
        Level::Project => "for everyone in the project",
    };
    format!(
        "purlis: the person allowed {allowed} {whom}, and this chat restarted on the same \
         conversation to take it. Retry the command the sandbox blocked."
    )
}

#[cfg(test)]
#[path = "grant_tests.rs"]
mod tests;
