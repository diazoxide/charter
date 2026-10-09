//! What was open when the app last quit, so the next launch can put it back.
//!
//! This is the app's own file, at `.charter/app/reopen.json` in the plane. It is not the
//! tmux frame's `.charter/frame/reopen.json` — that one belongs to the frame this app
//! replaces, and the app never touches it (`docs/plane-format.md`).
//!
//! The file is written by a process that may have been killed halfway, edited by hand, or
//! left behind by an older version, so reading is a conversion: a record of another
//! version, a file that is not JSON, and a `resume` that is not a session id all read as
//! "nothing to put back" rather than as an error a launch would have to handle.
//!
//! **What that does and does not cover.** `resume` is held to a shape because it is the one
//! field charter itself puts on a command line, where a leading `-` would be a flag the
//! operator never typed. `program`, `args` and `cwd` are **not** checked: they say what to
//! run, and there is no shape that separates a harness the operator installed from anything
//! else they might run. So this file is, for whoever can write it, a way to have a command
//! run at every later launch — behind one question that names how many chats and in which
//! projects (charter-app#250), and never what they run.
//!
//! **It is not true that a gitignored `.charter/` keeps this out of a clone.** An ignore rule
//! does not apply to a tracked path: `git add -f` commits a symlink — or a file far larger
//! than charter will read — and a fresh clone materialises it. (A FIFO is the one poison git
//! cannot carry; it needs a local writer.) That assumption is what left the record reachable through a link, so the
//! path is guarded now (`no_link_on_the_way`) rather than argued about. What remains true is
//! that anyone who can *write* this file could already write a shell profile — it is not a
//! way in, it is a way to *survive*: one write buys every launch after it, in another
//! process, with none of charter's guards in the path.

use std::path::{Path, PathBuf};

use crate::harness::{Harness, SessionId};

mod arrival;
pub use arrival::{Arrival, CloneSeat, arrive};

/// The one version of this file this app writes and reads. A record of any other version is
/// ignored whole, the way the frame's own manifest is: a format that changed means the
/// chats in it cannot be trusted to mean what they say.
pub const VERSION: u32 = 1;

/// Where the record lives, relative to the plane's state folder ([`path`]).
pub const IN_STATE: &str = "app/reopen.json";

/// The largest record charter will read, matching `contain.MAX_BYTES` on the Python side.
/// A real record is a few hundred bytes per chat; anything approaching this is not one.
pub const MAX_BYTES: u64 = 1_048_576;

/// One chat as it was: what it was running, where, and the conversation to bring back.
///
/// `Default` is a chat with no program, which is never one a record holds: it is for spelling a
/// chat by the fields that matter (`Chat { program, name, ..Default::default() }`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chat {
    /// The program, as it was launched. A path or a bare name.
    pub program: String,
    /// Its arguments, without any charter added — those are decided again at the reopen,
    /// because a resume spells them differently from a start.
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// What the operator calls this chat, and what a harness that takes a name is given.
    pub name: String,
    /// The conversation this chat is in NOW, where the app knows one — and so the one a
    /// relaunch brings back.
    ///
    /// **Current, not the one it started under** (Q10): the id charter chose at the start, then
    /// whatever the chat's own harness moved it onto, as the board adopted or followed it —
    /// the first id a Codex or opencode chat names through its hook, and the new one a Claude
    /// Code chat is in after `/clear`. A nested harness's report never reaches it (ADR 0024,
    /// C5). [`conversation_of`] reads it for a chat by number.
    pub resume: Option<SessionId>,
    /// Whether this was the chat in front.
    pub active: bool,
    /// The harness profile this chat started on, where it started on one.
    ///
    /// The NAME, and never the command or the environment it resolves to. The reopen reads
    /// `charter.local.toml` again, which is ADR 0022's rule and is load-bearing twice: an
    /// edit to the profile takes effect instead of a stale copy running, and a profile that
    /// is GONE means this chat is skipped by name rather than started on another account's.
    /// It also keeps that account out of a file that outlives the app.
    pub profile: Option<String>,
    /// The persona this chat adopted.
    pub persona: Option<String>,
    /// Whether this chat draws charter's footer in its pane (ADR 0029).
    ///
    /// Recorded for the same reason `persona` is: it is a choice the operator made about
    /// THIS chat in the picker, and a relaunch that dropped it would silently blank a footer
    /// they had turned on. A record written before ADR 0029 has no such key, and `false` is
    /// both serde's default and the behaviour every such record was written under.
    pub show_footer: bool,
    /// Whether the operator pinned this chat (ADR 0039, stored per ADR 0040).
    ///
    /// **A chat pin is an app record and not machine state**, which is the one of the three
    /// levels that does not go in `machine.rs`: ADR 0034 forbids a chat name outside a plane
    /// in as many words, and a chat is numbered per plane, so its number means nothing
    /// anywhere else. This file is already the only record that a chat exists at all, which
    /// is what lets a pin disappear with the chat it pins rather than needing a second list
    /// to keep in step. It is out of git, so a pin still travels with nobody.
    ///
    /// **And this is not a format change, although ADR 0039 expected one.** That record
    /// reasoned from this file's rule that a record of another version is ignored whole —
    /// which is exactly why a bump is expensive: every operator's open chats would be dropped
    /// at the first launch after it. A bump is owed when a field's absence cannot be read
    /// honestly, and this one's reads as `false`, which is what was true of every record
    /// written before pins existed. `show_footer` above is the same move under ADR 0029, and
    /// is the precedent rather than an analogy.
    pub pinned: bool,
    /// The number this chat answers to in its plane, where the record knows one.
    ///
    /// **This is the chat's identity on disk, and that is charter-app#90.** The number is
    /// what `$CHARTER_SESSION_ID` carries and what `.charter/sessions/<sid>.workspace` and
    /// `<sid>.lock` are keyed on (charter-app#63). Before this field the number was dealt
    /// again at every launch, in the order [`Record::chats`] happened to be in — so closing
    /// one chat shifted every later one down by one at the next relaunch, and each surviving
    /// chat read the workspace pointer and held the session lock of the chat that used to sit
    /// above it. Recording it makes the number outlive the process that dealt it, which is
    /// the only thing that makes the pointer mean this chat.
    ///
    /// `None` is a record written before this field, where nothing says which chat was
    /// which; those are dealt in order at the next launch, exactly as they always were, and
    /// carry numbers from then on. It is not a format change for the same reason `pinned`
    /// was not: a bump drops every operator's open chats, and this field's absence reads as
    /// the behaviour every record without it was written under.
    pub number: Option<u32>,
    /// The name the operator gave this chat, where they gave one (charter-app#254) — what its
    /// tab says instead of the default `<persona> <N>`.
    ///
    /// **Charter's label and nothing else.** It is not [`Self::name`], which is what the
    /// harness was started with (`--name`) and is told again at a resume: renaming a chat
    /// never reaches a harness that is running, and a split still starts its chat under the
    /// tab's chat name. Held to [`label`] wherever it comes from. `None` is "no name given",
    /// which is what every record written before this field says — not a format change, for
    /// [`Self::pinned`]'s reason.
    pub label: Option<String>,
    /// The chat a handoff opened this one from, where one did (charter-app#258, #259).
    ///
    /// What the tab's tooltip and the chat's header say (`↳ from steward 3 · ops`), and,
    /// for a handoff that asked for an answer, the pairing a report is checked against:
    /// **recorded by the app when it opened the chat, and never passed by the chat that
    /// reports**, so a chat cannot send its report anywhere but back to the chat that asked.
    /// Riding the record is what lets the pairing outlive a relaunch. `None` is every chat the
    /// operator opened, and every record written before this field.
    pub from: Option<HandedFrom>,
    /// Whose persona grants it holds instead of its own, where it holds another's (#1362):
    /// see [`HeldGrants`]. Its own home, apart from [`Self::from`], so a handoff note that does
    /// not read never drops it, and a Resume, which has no note, can hold too.
    pub held: Option<HeldGrants>,
    /// Whether the person opened this chat's tab from the Chats section (#1447).
    ///
    /// **Only a task chat's answer is read**: a chat a dispatch started in [`Mode::Task`] is
    /// listed with no tab until the person clicks its row, and this is that click, kept so a
    /// reload and a relaunch bring the tab back. Every other chat has a tab from its start;
    /// [`Self::has_tab`] is the one answer. `false` is every record written before this field
    /// — not a format change, for [`Self::pinned`]'s reason.
    pub tab_opened: bool,
    /// The chat this chat's tab shows in place of it, by that chat's [`Self::number`], where the
    /// person switched the tab to one (#1486): a task below this chat, at any depth.
    ///
    /// **The window's state, kept for it**: the core holds who asked whom, and which of those
    /// chats a session's tab shows is the person's arrangement, like a pin. Kept on the
    /// session's own entry so it goes with the session, and by number, as [`HandedFrom::chat`]
    /// names a chat: the number outlives a relaunch, and a chat started again under a new one
    /// is followed where its number is.
    ///
    /// **Never trusted to name a task of this chat.** The window shows it only where the chat
    /// it names is open and below this one, and shows the session's own chat otherwise. `None`
    /// is every record written before this field, and every tab that shows its own chat — not
    /// a format change, for [`Self::pinned`]'s reason.
    pub shows: Option<u32>,
    /// The chat whose tab this chat has a pane in, by that chat's [`Self::number`], where it is
    /// not its tab's own chat (#1489): a task the person opened beside the session that asked
    /// for it, or any chat started in a split.
    ///
    /// **The window's state, kept for it**, as [`Self::shows`] is. Two things read it. A
    /// launch puts a task back beside its session, where that session came back with a tab,
    /// and in the Chats list otherwise; every other chat comes back as a tab of its own, as it
    /// always has. And it says which chats are on screen with the chat in front: every pane of
    /// the tab in front is looked at, not only its first. The core starts, ends and allows
    /// nothing by it. `None` is every record written before this field, and every chat that is
    /// its tab's own or has no pane — not a format change, for [`Self::pinned`]'s reason.
    pub beside: Option<u32>,
    /// The workspace this chat's directory was renamed away from, where a rename left it with
    /// no conversation its harness can find (charter#367, D10).
    ///
    /// Claude Code finds a conversation by the directory it ran in
    /// ([`Harness::keeps_conversations_by_directory`]), so `charter workspace rename` drops
    /// such a chat's [`Self::resume`] and sets this instead. The next start is a fresh
    /// conversation and says why ([`Fresh::WorkspaceRenamed`]), once: the chat is recorded
    /// again without it. `None` is every other chat, and every record written before this
    /// field — not a format change, for [`Self::pinned`]'s reason.
    pub renamed_from: Option<String>,
    /// Whether the chat's last run started without the sandbox in a project that has it on: a
    /// person's opt-out, or a system with no backend (ADR 0067 §7).
    ///
    /// **A fact about the run that was, never a choice about the next one.** No start reads it
    /// as an opt-out — an opt-out is never inherited by a relaunch or a resume — so a chat
    /// that ran unsandboxed starts sandboxed next, or not at all where the sandbox cannot be
    /// applied. This is how a sandboxed start knows to say the sandbox came back on
    /// (`trust.sandbox.on`), and how the chat's session record says what it ran under. `false` is every other chat, and every record written before this field.
    pub unsandboxed: bool,
    /// Who this chat is beyond this clone: its id, its origin device, its current run and the
    /// chat it was resumed from (ADR 0066). See [`Identity`].
    pub identity: Identity,
    /// The process the app started this chat's program as, while it runs (V82, #1018).
    ///
    /// **Only what a commit is checked against.** `commit-msg` stamps provenance trailers on a
    /// commit only when the committing process descends from this one
    /// ([`crate::provenance::Provenance::in_chat`]): a program that merely inherited the chat's
    /// environment — an editor the chat opened, which the operator then commits from — is not
    /// the agent. It is the program the app started, which may be a wrapper profile's and not
    /// the harness itself; the harness runs below it, so the check holds either way.
    ///
    /// Taken from the session at every write of the record, and only while its program runs:
    /// the app writes the record again when a program ends, and the write at quit names no pid
    /// at all. A pid that may be handed to another process is on disk only between a program's
    /// reap and that rewrite, and after an app that crashed or was killed, until the next launch
    /// writes the record again ([`crate::process::descends_from`] names both). `None` is a
    /// chat whose program is not running, and every record written before this field — not a
    /// format change, for [`Self::pinned`]'s reason. A chat with none stamps nothing.
    pub pid: Option<u32>,
}

impl Chat {
    /// Whether the window draws this chat as a tab (#1447): every chat but a task chat the
    /// person has not opened, which is listed in the Chats section and nowhere on the strip.
    pub fn has_tab(&self) -> bool {
        self.tab_opened
            || self
                .from
                .as_ref()
                .is_none_or(|from| from.mode != Mode::Task)
    }
}

/// A chat's ids, as the record keeps them across a relaunch (ADR 0066, "What changes where").
///
/// **The number never leaves the clone, and the id is what leaves.** [`Chat::number`] is what
/// the operator reads and what this machine's files are keyed on; these are what an event, an
/// audit entry or a session record names the chat by, so they have to outlive the process that
/// minted them, which is what riding the record does.
///
/// Every field is a ULID or `None`, and **`None` is honest for every record written before
/// this**: an absent `id` reads as "mint one at this launch", which is what every chat before
/// it was, and the record stays `version: 1` for [`Chat::pinned`]'s reason. A value off disk
/// that is not a ULID reads as `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    /// The chat's id, minted once per clone and device when the host first started it there,
    /// and never changed in that clone (V43: a copy's launch mints it again, see [`arrive`]).
    pub id: Option<String>,
    /// The device id of the machine that minted [`Self::id`], where it had one: a fact about
    /// the chat, never part of its key. `None` reads as `unknown`.
    pub device: Option<String>,
    /// The chat's current run: the one the host began when it last started the chat, or the
    /// one it moved to since (a `/clear`).
    pub run: Option<String>,
    /// The chat a **Resume** from a session record started this one from (ADR 0064), by its
    /// id. `None` is `unknown` or none: a record that names no chat id leaves it so.
    pub resumed_from: Option<String>,
}

/// The chat a handoff came from, as the chat it opened keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandedFrom {
    /// The app's number for that chat — the key its reports are left under.
    pub chat: u32,
    /// Its name as the operator saw it when it handed off: the name it was given, or its
    /// default. A copy, so the note still reads after that chat is closed.
    pub name: String,
    /// Where it handed off from — a workspace, or the plane root (SI-1b) — which is where a
    /// report goes when it is gone.
    pub workspace: crate::active::Place,
    /// Whether it asked for a report, and whether one has been sent.
    pub report: Owed,
    /// Whether the work moved or a report is expected back: what decides whether the chat
    /// opens as a tab (#1447), and a task is listed under the chat that asked (#1436).
    pub mode: Mode,
    /// How many dispatches stand between this chat and the chat the person started: 1 for a
    /// chat that one dispatched. 0 is a record written before this field.
    pub depth: u32,
    /// **The lineage this chat is in, by the stable id of the chat the person started**
    /// ([`Identity::id`]): copied down each dispatch, so every chat of one lineage names the
    /// same root whichever chats between them have closed, and a chat started again under a
    /// new number is still in it. `None` is a record written before this field, or a lineage
    /// whose first chat had no id yet.
    ///
    /// **It says which lineage, and nothing else.** [`Self::chat`] is where this chat's report
    /// goes; neither field says who may steer the chat (tell it, cancel it, wait on it), which
    /// is its own question and not answered by this record.
    pub root: Option<String>,
    /// **The personas of the chats above this one, nearest first** (#1521): the asking chat's,
    /// then the ones its own record keeps, each `None` for a chat on no persona. Written by the
    /// app when it dispatches this chat, from its record of the asking chat and never from
    /// anything a chat sent, so the loop rule and the person's never for a chat above still
    /// hold when a chat in the middle has closed, finished or been cleared. One per dispatch
    /// above it: as long as [`Self::depth`] says.
    ///
    /// `None` is a record written before this field: its chain is read from the chats still
    /// open, and where that walk meets one that has closed, purlis cannot say who was there
    /// ([`crate::dispatchlimits::Lineage::chain_unread`]).
    pub above: Option<Vec<Option<String>>>,
    /// Whether the person started it, from that chat's tab (#1438), and not that chat itself.
    /// Its report still goes to that chat, and says so.
    pub by_person: bool,
}

/// How one chat started another (the spec's two modes of a dispatch, #1434).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// The work moved: the chat opens as a tab. Every record written before modes existed.
    #[default]
    Handoff,
    /// The asking chat expects a report: the chat is listed in the Chats section, and has no
    /// tab until the person opens it ([`Chat::has_tab`]).
    Task,
}

impl Mode {
    /// The word the record and the window's lineage carry.
    pub fn word(self) -> &'static str {
        match self {
            Self::Handoff => "handoff",
            Self::Task => "task",
        }
    }

    /// Anything but `task` reads as a handoff, which is what a record without the key says.
    pub fn of(word: &str) -> Self {
        if word == "task" {
            Self::Task
        } else {
            Self::Handoff
        }
    }
}

/// **The persona grants a chat holds instead of its own persona's** (#1362, D-1362-5 and
/// D-1362-6): set where a chat was started as a persona nobody the person trusts chose — a
/// handoff to a persona whose hosts reach past the asking chat's, or a Resume of a record whose
/// persona reaches past the default's — until the person allows its own on its tab. `persona` is
/// the grants it runs with meanwhile: the asking chat's as that chat ran, from the app's own
/// record and never from a request, or the default persona's. `None` is no persona's grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldGrants {
    pub persona: Option<String>,
}

/// What a handed-off chat owes the chat that opened it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owed {
    /// Nothing: the handoff was fire-and-forget.
    Nothing,
    /// One report, not yet sent.
    Due,
    /// The report was sent, and that is the handoff's one report: nothing re-arms it. Another
    /// needs another `--report` handoff (the operator's ruling, charter-app#259).
    Sent,
    /// The chat's program ended, or the chat was closed, before it reported, and the app told
    /// the asking chat so in its place (#1443): `failed: ended without a report`. That is the
    /// task's one report, as [`Self::Sent`] is.
    Failed,
}

impl Owed {
    fn word(self) -> &'static str {
        match self {
            Self::Nothing => "",
            Self::Due => "owed",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }

    fn of(word: &str) -> Self {
        match word {
            "owed" => Self::Due,
            "sent" => Self::Sent,
            "failed" => Self::Failed,
            _ => Self::Nothing,
        }
    }
}

/// The name a chat is shown under: the one the operator gave it, or its default — the persona
/// it adopted, else the harness it runs, then its own name (`steward 3`, `claude 4`); a chat on
/// neither is its own name alone (charter-app#254).
///
/// The window's `whoOf` and `openTab` draw the same rule, and this is the core's copy of it,
/// for the one sentence the core has to write with a chat's name in it: where a handed-off
/// chat came from.
pub fn shown_name(chat: &Chat, harness: Option<&str>) -> String {
    if let Some(label) = &chat.label {
        return label.clone();
    }
    match chat.persona.as_deref().or(harness) {
        Some(who) => format!("{who} {}", chat.name),
        None => chat.name.clone(),
    }
}

/// The most view tabs one record puts back. A window opens one per persona and per extension
/// view at most, and a plane with more personas than this is not one anybody reads tab by tab;
/// the bound exists so a hand-written record cannot make a launch draw ten thousand tabs.
pub const MOST_VIEWS: usize = 64;

/// The most a view tab's recorded title may be, in characters. A tab draws a word or two.
pub const MOST_TITLE: usize = 120;

/// One tab that held a **view** rather than a chat, as it was when the app last wrote this.
///
/// **A tab is a layout of panes, and a pane holds a session or a view** (ADR 0043, as
/// amended for view tabs). A chat comes back because its program is started again; a view has
/// no program of charter's to start, so what comes back is only the fact that the tab was
/// there — which view, on which strip, where among the tabs, and whether it was in front.
///
/// **Identified by data, and the same data for charter's own views and a stranger's.** `from`
/// is `None` for a view charter draws itself (the persona view) and an extension's id for one
/// an approved extension offers (persona statistics), `view` is which of theirs, and `key` is
/// what it is about inside that — a persona's name, or empty for the whole plane. Nothing here
/// knows what a persona view is.
///
/// **Putting one back runs nothing.** A view an extension offers is asked when the operator
/// opens it, and a tab that came back from this file waits for a press before its program is
/// asked anything (`app/src/Views.tsx`): this file is writable by whoever can write the plane's
/// state directory, and a line in it must not become a program run at every launch. That is
/// also why views are not part of `machine::Contribution` — they start nothing, so there is
/// nothing for the trust fingerprint to cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Whose view: `None` for charter's own, else the extension's id.
    pub from: Option<String>,
    /// Which view of theirs.
    pub view: String,
    /// What it is about inside that: a persona's name, or empty for the whole plane — or, for
    /// one of charter's own views keyed by a path of names, that view's shape ([`own_key_ok`]).
    pub key: String,
    /// What its tab said, so a tab whose source has gone still comes back under its name.
    pub title: String,
    /// The workspace whose strip it was on, or `None` for the strip of chats outside every
    /// workspace. A tab with no chat cannot be filed by where its chat works, so it says.
    pub workspace: Option<String>,
    /// Where it was on the strip, counted over chats and views together from the left. A
    /// place and not a promise: a chat that did not come back moves it by one.
    pub at: u32,
    /// Whether it was the tab in front.
    pub active: bool,
    /// Whether the operator pinned it (ADR 0039), for [`Chat::pinned`]'s reasons.
    pub pinned: bool,
    /// Where the view's divider was, as its first side's share of the tab in percent: the file
    /// tab's tree beside its preview (FM-2). `None` for a view with no divider, or one never
    /// moved.
    pub split: Option<u8>,
}

/// A branch's file tab (`app/src/pieceViews.ts`), keyed `workspace/repo/piece`, or
/// `workspace/repo/` for the repo's own folder: one of charter's views whose key is a path of
/// names rather than one name ([`own_key_ok`]).
const FILES_VIEW: &str = "piece-files";

/// The longest key a view of charter's own is held to: a file's path inside a branch is the
/// longest one the window writes, and no filesystem charter runs on takes a longer one.
const MOST_KEY: usize = 4096;

/// Every chat that was open, and the numbers this plane has already spent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// Every chat that was open, **in the order the chat strip drew them** — which is the
    /// order a launch puts them back in (ADR 0039, amended when a tab could be dragged).
    ///
    /// Not the order they were numbered in: a chat keeps its number across a launch
    /// ([`Chat::number`]), and the operator can drag it anywhere on the strip. Not a format
    /// change either, for [`Chat::pinned`]'s reason: a record written before tabs could be
    /// dragged lists them in number order, which was the strip's order when it was written.
    pub chats: Vec<Chat>,
    /// The view tabs that were open — see [`View`]. Empty in every record written before a tab
    /// could hold one, which is what was true of those.
    pub views: Vec<View>,
    /// The highest chat number this plane has ever dealt, open or closed — charter-app#90.
    ///
    /// **A number is never dealt twice, and this is what remembers that across a quit.**
    /// [`Chat::number`] alone would keep a chat that came back on its own pointer, but it
    /// could not stop a NEW chat inheriting a closed one's: close chat 3 and the record holds
    /// 1 and 2, so the highest number it names is 2 and the next chat is 3 again — reading
    /// the workspace that a stranger selected and taking the lock they held. The closed
    /// chat's pointer is still on disk; `wscmd::select`'s prune only drops it at 30 days.
    ///
    /// So the counter is carried rather than derived from the chats. It never goes backwards
    /// while the record is readable: the conversion both ways holds it at or above every
    /// number the record names, so a hand-edited file cannot re-deal one it still lists.
    pub dealt: u32,
    /// Whether this record was written by a quit that restarts charter to install an update
    /// (charter-app#251), rather than by the operator quitting.
    ///
    /// **Read by the launch after that quit, and by nothing else.** It adds one sentence to the
    /// launch's question saying why it is being asked; [`Choice::ReopenAll`] is the answer in
    /// front either way. Every later write of the record is an ordinary one and carries
    /// `false`, so it lasts until this plane's record is next written.
    ///
    /// **Restart to update is the one writer of `true`**, on each plane the app held. A plane
    /// the launch after it does not open (`--no-restore`, a declined trust ask) keeps the flag
    /// on disk, so it counts only at the launch that took [`RESTARTED_TO_UPDATE`] — see there.
    pub relaunch_after_update: bool,
    /// The clone and the device that wrote this record (V43), which is how a launch tells a
    /// copied project from a moved one — see [`arrive`]. `None` in every record written before
    /// V43, and in one built in memory: the app's writer stamps the clone it holds onto every
    /// record it writes.
    pub clone_seat: Option<CloneSeat>,
    /// The branch the window's sidebar was focused on — its cockpit (FM-5, #1108) — or `None`
    /// for the whole workspace. Kept with [`Record::views`] because it is the same kind of
    /// thing: how the window was arranged, which starts nothing.
    pub focus: Option<Focus>,
}

/// A branch the window focused its sidebar on: its workspace, its repo, and its folder's name,
/// or no folder for the repo's own (FM-5). Names only, each one charter would mint: the core
/// finds the folder from them, as it does for the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Focus {
    pub workspace: String,
    pub repo: String,
    pub piece: Option<String>,
}

impl Focus {
    /// The focus these names make, or `None` when one is not a name charter would mint: the one
    /// check for a focus, whether the window said it or a record holds it. These words reach a
    /// command the window sends back to the core.
    pub fn named(workspace: &str, repo: &str, piece: Option<&str>) -> Option<Self> {
        let ok = |word: &str| word.chars().count() <= 64 && crate::contain::workspace_name_ok(word);
        let repo_ok = repo.chars().count() <= 64 && crate::contain::repo_name_ok(repo);
        if !ok(workspace) || !repo_ok || !piece.is_none_or(ok) {
            return None;
        }
        Some(Self {
            workspace: workspace.to_owned(),
            repo: repo.to_owned(),
            piece: piece.map(str::to_owned),
        })
    }
}

/// What the operator answered when a launch found something to put back (charter-app#250).
///
/// **Asked once per launch, before any chat starts**, and never when the record holds
/// nothing ([`Record::holds_anything`]). A choice that is lost — the dialog closed, Esc, a
/// window that never answered — is [`Choice::ReopenAll`], because the other answer is the only
/// one that throws anything away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Every chat back in its tab and pane, resuming its conversation, and every view tab
    /// back where it was: what a launch did before it asked.
    ReopenAll,
    /// Nothing is put back, and the record is cleared of what it held.
    StartFresh,
}

impl Record {
    /// Whether a launch has anything to put back — a chat or a view tab. The counter of
    /// numbers dealt is bookkeeping and is not something that was open.
    pub fn holds_anything(&self) -> bool {
        !self.chats.is_empty() || !self.views.is_empty()
    }

    /// The record a launch puts back once the operator has answered.
    ///
    /// **A fresh start keeps [`Record::dealt`]**, and it is the one thing it keeps: the chats
    /// it drops still have workspace pointers and locks on disk keyed on their numbers, so a
    /// new chat dealt one of those would read a stranger's workspace (charter-app#90). The
    /// counter is taken at or above every number the record names, as a write would take it.
    pub fn chosen(self, choice: Choice) -> Record {
        match choice {
            Choice::ReopenAll => self,
            Choice::StartFresh => Record {
                dealt: highest_dealt(&self),
                ..Record::default()
            },
        }
    }
}

/// The file a restart to update leaves in charter's own config directory, beside
/// `machine.json` ([`crate::machine::dir`]) — charter-app#251.
///
/// **It is what scopes [`Record::relaunch_after_update`] to the restart.** The flag is written
/// into each plane the app held, and a plane the launch after the restart does not open
/// (`--no-restore`, a trust ask declined) keeps it on disk until its record is next written. A
/// launch days later from that plane's directory would then say "charter restarted to install
/// an update", which is not true of that launch. So the restart also leaves this, the next
/// launch takes it ([`take_restart_to_update`]) whatever it opens, and a record's flag counts
/// only at the launch that took it.
///
/// **A file of its own and not a field of the machine store**, the choice `theme.json` and
/// `layout.json` made for the reason [`crate::machine`] gives: that store keeps five things and
/// says the count is load-bearing, and this is not a fact about any plane it names. It also
/// works where the store refuses (ADR 0031's Windows): it holds nothing, so there is no
/// `0600` for it to need. Its existence is the whole of it, and all it can do is add one
/// sentence to a question charter was going to ask anyway.
pub const RESTARTED_TO_UPDATE: &str = "restarted-to-update";

/// Leaves [`RESTARTED_TO_UPDATE`] for the launch after this one.
///
/// Created in charter's own `0700` directory and never through a link at its name: the
/// directory is shared with the machine store, and a planted link is the one way a write of
/// nothing could land somewhere that matters.
pub fn mark_restart_to_update(config_root: &Path) -> std::io::Result<()> {
    let dir = crate::machine::private_dir(config_root)?;
    crate::contain::create_no_link(&dir, &dir.join(RESTARTED_TO_UPDATE)).map(drop)
}

/// Whether this launch follows a restart to update, answered once: the marker is removed as
/// it is read, so no later launch reads it too.
///
/// Every failure is "no", because the one thing "yes" adds is a sentence saying why a
/// question is being asked. A marker that cannot be removed is also "no" — one that stayed
/// would say "restarted" at every launch after this one.
pub fn take_restart_to_update(config_root: &Path) -> bool {
    std::fs::remove_file(crate::machine::dir(config_root).join(RESTARTED_TO_UPDATE)).is_ok()
}

/// How a chat came back, which is what the pane showing it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reopened {
    /// The harness was given the conversation to bring back.
    Resumed(SessionId),
    /// It started as a new chat, for this reason.
    Fresh(Fresh),
}

/// Why a chat came back as a new one rather than the conversation it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fresh {
    /// Nothing recorded a conversation for it — the app never learnt this harness's session
    /// id. Codex and opencode report theirs only through a hook, inside the first turn, so a
    /// chat quit before one comes back like this.
    NoConversationRecorded,
    /// Its program is not a harness charter has measured a resume for — a shell, say.
    NoResumeForThisProgram,
    /// Its own arguments already name a session, so charter added none of its own. What
    /// happens then is between the operator and the harness.
    SessionNamedByTheOperator,
    /// Its workspace was renamed, and its harness finds a conversation by the directory it ran
    /// in, so the one it had is under the old name ([`Chat::renamed_from`]).
    WorkspaceRenamed,
}

/// What starts a chat, and what the app has to remember about having started it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// The conversation this chat is now under — the one resumed, or the one the app just
    /// chose for it. This is what the next quit records, so a chat started fresh today can
    /// be resumed tomorrow. None where the harness chooses its own id, or is not one.
    pub session: Option<SessionId>,
    pub how: Reopened,
}

impl Chat {
    /// How this chat came back, told what a start that knew no chat said: "nothing recorded
    /// a conversation" is, for a chat a workspace rename left without its conversation, that
    /// its workspace was renamed. `start::ready` knows no chat, so its answer passes here.
    pub fn told(&self, how: Reopened) -> Reopened {
        match how {
            Reopened::Fresh(Fresh::NoConversationRecorded) if self.renamed_from.is_some() => {
                Reopened::Fresh(Fresh::WorkspaceRenamed)
            }
            how => how,
        }
    }

    /// The harness this chat runs, or none for a program that is not one.
    pub fn harness(&self) -> Option<Harness> {
        Harness::of_command(&self.program)
    }

    /// The program and arguments that bring this chat back, and which of the two happened.
    ///
    /// Charter's own words go FIRST and the chat's recorded arguments after them, which is
    /// the order `charter/frame/launcher.py:707` uses (`[*cmd, *words, *rest]`). It is not
    /// cosmetic: Codex resumes through a subcommand (`codex resume <id>`), and a subcommand
    /// after a positional prompt is not the same command line at all.
    pub fn launch(&self) -> Launch {
        let (added, session, how) = match (self.harness(), self.resume.as_ref()) {
            // The operator already named a session in the chat's own arguments, so charter
            // adds none of its own: two `--resume` on one command line is not a harness
            // anyone has measured, and the one the operator typed is the one they meant.
            (Some(harness), _) if harness.session_named_in(&self.args) => (
                Vec::new(),
                None,
                Reopened::Fresh(Fresh::SessionNamedByTheOperator),
            ),
            // Not a harness charter has measured — a shell. It comes back as itself, and
            // there is nothing to resume it by.
            (None, _) => (
                Vec::new(),
                None,
                Reopened::Fresh(Fresh::NoResumeForThisProgram),
            ),
            (Some(harness), Some(id)) => match harness.resume_argv(id, &self.name) {
                Some(argv) => (argv, Some(id.clone()), Reopened::Resumed(id.clone())),
                None => (
                    Vec::new(),
                    None,
                    Reopened::Fresh(Fresh::NoResumeForThisProgram),
                ),
            },
            // No conversation to bring back, so this is a new chat. Where the harness takes
            // an id charter chose, it is given one and that id is kept — otherwise the next
            // quit would have nothing to record and the chat could never be resumed at all.
            (Some(harness), None) => {
                let chosen = harness.chooses_session_id().then(SessionId::fresh);
                let argv = chosen
                    .as_ref()
                    .map(|id| harness.new_session_argv(id, &self.name))
                    .unwrap_or_default();
                (
                    argv,
                    chosen,
                    self.told(Reopened::Fresh(Fresh::NoConversationRecorded)),
                )
            }
        };
        let mut args = added;
        args.extend(self.args.iter().cloned());
        Launch {
            program: self.program.clone(),
            args,
            session,
            how,
        }
    }
}

/// The record's path inside `plane_root`.
pub fn path(plane_root: &Path) -> PathBuf {
    crate::names::state(plane_root).join(IN_STATE)
}

/// Refuses a record path a symlink could take outside the plane, or that is not a plain file.
///
/// `.charter/app/reopen.json` is charter's own, created by charter, and nothing legitimate
/// makes any part of it a link. A committed `.charter/app -> somewhere else` would otherwise
/// have the app **write** its record outside the plane and, worse, **read** the command line
/// it launches at startup from out there — with no consent step in the way. So every
/// component from the plane root down is checked with `symlink_metadata`, which does not
/// follow links, and one link anywhere in the chain refuses the whole operation.
///
/// This is deliberately blunter than resolving the path: a link here has no honest use, and
/// charter's Python side words the same rule the same way — "is a symlink, and charter writes
/// nothing through one". It checks every component BELOW `plane_root`, not the root itself:
/// a plane reached through a symlinked parent (`/tmp`, a symlinked `$HOME`) is an ordinary,
/// honest setup, and the app's own root comes from `getcwd()`, which is already resolved.
///
/// It does not stop a **hard** link, which no symlink check can see and git cannot carry;
/// whoever can make one can already write this file.
///
/// The last component is also held to being a plain file no bigger than [`MAX_BYTES`], and
/// **those two questions are now asked of the open descriptor** rather than of the name
/// (ADR 0028). A `symlink_metadata` on the path and a `read_to_string` of the path
/// are two different objects with a window between them: a FIFO swapped in after the check
/// blocked the launch for ever, which is the very failure the check exists to stop.
/// `contain::open_no_link` returns the descriptor the read will use, and [`refuse_unusable`]
/// asks *it*.
///
/// A FIFO is not a link, so a link check waves it through, and reading one **blocks for
/// ever** — at launch, before the window and the tray exist, leaving an app that can only be
/// killed. The size bound is the half that a clone can actually deliver: git cannot store a
/// FIFO, but a sparse multi-gigabyte file packs small and arrives full size, and reading it
/// whole at launch is the same failure by another road.
fn no_link_on_the_way(plane_root: &Path, file: &Path) -> std::io::Result<()> {
    // The walk itself is shared with every other path charter owns (`contain`), because two
    // copies of a containment gate drift. What is left here is what is specific to a RECORD.
    crate::contain::no_link_on_the_way(plane_root, file)?;
    match std::fs::symlink_metadata(file) {
        Ok(found) => refuse_unusable(file, &found),
        // Not there yet is fine: the app creates `.charter/app/` and the file itself.
        Err(_) => Ok(()),
    }
}

/// What a record may be, asked of whatever `found` describes.
///
/// Taken as `Metadata` rather than a path so the caller chooses the object: the read side
/// hands it an `fstat` of the descriptor it is about to read, which no swap can get between,
/// and the write side hands it an `lstat` of a file that is not open yet.
///
/// `pub(crate)` for `planegit`'s push record, which is the same kind of object — a small JSON
/// file charter keeps under the state directory and reads back — and meets the same two
/// hazards. A FIFO there blocks `charter save`, which an operator runs all day.
pub(crate) fn refuse_unusable(file: &Path, found: &std::fs::Metadata) -> std::io::Result<()> {
    // A record that is not a plain file: a FIFO would block the read for ever, a device
    // never ends. Directories above it are fine, the record itself is not.
    if !found.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{} is not a plain file, and purlis reads its record from nothing else",
                file.display()
            ),
        ));
    }
    // Read whole at launch, so a planted giant is a hang with nothing to click on.
    if found.len() > MAX_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, and purlis's record is never larger than {MAX_BYTES}",
                file.display(),
                found.len()
            ),
        ));
    }
    Ok(())
}

/// Writes the record, creating `.charter/app/` if it is not there.
///
/// The file is written beside itself and renamed over, so a launch that reads it never sees
/// half of one — the app can be killed at any moment, and quitting is exactly when it is.
///
/// That is [`crate::rewrite::replace`] with [`crate::rewrite::Mode::Private`] (#434): the
/// record names programs to run, so it is charter's own state at 0600. The walk from the
/// plane's root gates the temp file the bytes actually land on as well as the record — guarding
/// only the renamed-onto name once left a committed `reopen.json.writing -> outside` writing
/// the whole record out of the plane — and the temp is opened `O_NOFOLLOW`, so a link planted
/// after the walk answered is refused by the kernel (ADR 0028). A record that is itself a link
/// is refused.
pub fn write(plane_root: &Path, record: &Record) -> std::io::Result<()> {
    // Belt as well as braces. A root gets here from [`crate::plane::resolve`], which a
    // fenced build has already held — but also from a caller that was handed one, and this
    // file is the exact thing charter-app#129 damaged. The place where the record is written
    // is worth guarding on its own account, whatever route the root took to reach it.
    crate::fence::hold(crate::fence::Act::Write, plane_root);
    let file = path(plane_root);
    no_link_on_the_way(plane_root, &file)?;
    let dir = file.parent().expect("the record's path has a directory");
    crate::rewrite::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(&OnDisk::from(record))
        .expect("the record is plain data serde can always write");
    crate::rewrite::replace(
        plane_root,
        &file,
        (text + "\n").as_bytes(),
        crate::rewrite::Mode::Private,
    )
}

/// What was open, or nothing, with every refusal swallowed.
///
/// Nothing in the app calls this — [`read_or_refusal`] is what a launch uses, because a
/// refusal an operator never sees is the bug this module already had once. It is kept for
/// tests, and deliberately not public.
///
/// This never fails. No file is a first launch; a file of another version, or one that is
/// not the record at all, is nothing open — there is no repair that would be honest, and
/// refusing to start would be worse than starting empty.
#[cfg(test)]
fn read(plane_root: &Path) -> Record {
    read_or_refusal(plane_root).unwrap_or_default()
}

/// What was open, the refusal that stopped it being read, or nothing.
///
/// A refusal is returned rather than swallowed: a poisoned record and an empty one would
/// otherwise render identically, and the operator would read "nothing to reopen" and conclude
/// their chats were never recorded rather than that a committed file is defective.
pub fn read_or_refusal(plane_root: &Path) -> Result<Record, std::io::Error> {
    Ok(match read_held(plane_root)? {
        Held::Record(record) => record,
        Held::Absent | Held::Garbled => Record::default(),
    })
}

/// What was open, with "no record" told apart from "a record this charter cannot use".
///
/// `Ok(None)` is no record at all — a plane no app has quit in. A record that is there and does
/// not parse, or is of another [`VERSION`], is an `InvalidData` error here, where
/// [`read_or_refusal`] reads it as nothing open: a launch may start empty, but a caller that
/// deletes what the record's chats need (`retention::on_open`) may not read "garbled" as
/// "no chats".
pub fn read_strictly(plane_root: &Path) -> Result<Option<Record>, std::io::Error> {
    match read_held(plane_root)? {
        Held::Record(record) => Ok(Some(record)),
        Held::Absent => Ok(None),
        Held::Garbled => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{} is not a version {VERSION} record",
                path(plane_root).display()
            ),
        )),
    }
}

/// What reading the record found.
enum Held {
    Absent,
    Garbled,
    Record(Record),
}

fn read_held(plane_root: &Path) -> Result<Held, std::io::Error> {
    // Reading is guarded as well as writing, and the reason is the whole of ADR 0035: this
    // file says what to RUN. A test that reads a real plane's record starts the operator's
    // programs — which is what the reproduction for charter-app#129 did, from a checkout
    // one directory inside a plane: "1 of 1 chats back", in a plane the run never made.
    crate::fence::hold(crate::fence::Act::Read, plane_root);
    let file = path(plane_root);
    // A record reached through a link is not this plane's record, and what it holds is a
    // command line this launch would run — so the walk AND the open both answer, and the
    // open's answer is the kernel's at the instant it happens (ADR 0028). Measured
    // before that flag: 1881 launches per 20,000 took a planted command line from outside
    // the plane.
    let mut open = match crate::contain::open_no_link(plane_root, &file) {
        Ok(open) => open,
        // No record is a first launch. A record that exists and cannot be read — no
        // permission, a failing disk — is a defect with a repair, and saying "nothing to
        // reopen" would send the operator looking in the wrong place.
        Err(gone) if gone.kind() == std::io::ErrorKind::NotFound => return Ok(Held::Absent),
        Err(unreadable) => return Err(unreadable),
    };
    // Asked of the descriptor the read will use, not of the name: `fstat` and the read
    // cannot be handed two different files.
    refuse_unusable(&file, &open.metadata()?)?;
    let text = {
        use std::io::Read;
        let mut text = String::new();
        open.read_to_string(&mut text)?;
        text
    };
    let Ok(on_disk) = serde_json::from_str::<OnDisk>(&text) else {
        return Ok(Held::Garbled);
    };
    if on_disk.version != VERSION {
        return Ok(Held::Garbled);
    }
    let record = Record {
        chats: on_disk.chats.into_iter().map(Chat::from).collect(),
        views: on_disk
            .views
            .into_iter()
            .filter_map(ViewOnDisk::held)
            .take(MOST_VIEWS)
            .collect(),
        dealt: on_disk.dealt,
        relaunch_after_update: on_disk.relaunch_after_update,
        clone_seat: on_disk.clone.and_then(arrival::CloneSeatOnDisk::held),
        focus: on_disk.focus.and_then(FocusOnDisk::held),
    };
    // Held to the same invariant on the way in as on the way out: a file whose counter sits
    // below a number it still names — hand-edited, or written by a charter that did not know
    // about numbers — would otherwise deal that number to a second chat, which is the defect
    // (charter-app#90). Raising it here costs a gap in the counting and nothing else.
    Ok(Held::Record(Record {
        dealt: highest_dealt(&record),
        ..record
    }))
}

/// The conversation chat `number` is in now, as the app last recorded it in `plane_root` — or
/// none, where its harness has not named one yet or the record does not hold the chat.
///
/// **The one way to ask, from inside a chat, which conversation it is.** A chat knows its own
/// number: the app sets `$CHARTER_SESSION_ID` (and, where hooks report, `$CHARTER_CHAT`) to it
/// in every chat's environment, and [`Chat::number`] is that number on disk. The app keeps
/// [`Chat::resume`] current — the id charter chose at the start, the first one a Codex or
/// opencode chat names through its hook, the one a Claude Code chat moves to on `/clear` —
/// and writes it the moment the board takes the report, so this is never behind the chat's
/// own harness by more than the one write.
///
/// Read through [`read_or_refusal`], so a record that is a link, a FIFO or too large is
/// refused here as it is at a launch, and a record of another version holds no chat.
pub fn conversation_of(plane_root: &Path, number: u32) -> std::io::Result<Option<SessionId>> {
    Ok(read_or_refusal(plane_root)?
        .chats
        .into_iter()
        .find(|chat| chat.number == Some(number))
        .and_then(|chat| chat.resume))
}

/// Who chat `number` is, as the app last recorded it in `plane_root`: its id and current run
/// (ADR 0066), the way [`conversation_of`] answers its conversation. `None` where the record
/// does not hold the chat; an [`Identity`] with no id is a chat recorded before ids, which is
/// given one at the next launch.
pub fn identity_of(plane_root: &Path, number: u32) -> std::io::Result<Option<Identity>> {
    Ok(read_or_refusal(plane_root)?
        .chats
        .into_iter()
        .find(|chat| chat.number == Some(number))
        .map(|chat| chat.identity))
}

/// The longest name, in characters, an operator can give a chat.
pub const MOST_LABEL: usize = 64;

/// A name an operator gave a chat, as charter will hold it — or why it will not.
///
/// **One rule for every way a name arrives**: the picker's Name field, a rename on the tab, and
/// a record read off disk. Trimmed, because a space at either end is never what was meant and
/// draws as nothing. **Blank is `None`**, which is "no name given" — the tab says its default,
/// so clearing a name is how the default comes back. Bounded, because it is drawn on a tab.
/// And **refused, never stripped**, when it holds a control character or an invisible
/// formatting one ([`crate::panel::undrawable`]): what those do is make two different names look
/// like one on the strip, or turn the words around them backwards, and a name quietly changed
/// into another is not the one the operator typed.
pub fn label(raw: &str) -> Result<Option<String>, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let length = text.chars().count();
    if length > MOST_LABEL {
        return Err(format!(
            "That name is {length} characters long, and a chat's name is at most {MOST_LABEL}."
        ));
    }
    if text.contains(crate::panel::undrawable) {
        return Err(
            "That name holds a control character or an invisible formatting one, which \
             purlis will not draw."
                .to_owned(),
        );
    }
    Ok(Some(text.to_owned()))
}

/// The record as JSON, and the only place this file's field names are written down.
///
/// It is deliberately a shape apart from `Record`: every field is the plainest type JSON
/// has, so reading cannot fail on a value, and the conversion into `Chat` is the one place
/// a value is held to what it has to be. A `SessionId` that derived `Deserialize` would put
/// that guard where a later edit could quietly drop it.
#[derive(serde::Serialize, serde::Deserialize)]
struct OnDisk {
    version: u32,
    /// When it was recorded, in seconds since the epoch. Nothing reads it; it is here
    /// because a record nobody can date is one nobody can debug.
    at: u64,
    chats: Vec<ChatOnDisk>,
    /// The highest chat number this plane has dealt — see [`Record::dealt`]. Absent in every
    /// record written before chats kept their numbers, and `0` reads as "nothing dealt that
    /// this file does not already name", which is what was true of those.
    #[serde(default)]
    dealt: u32,
    /// The view tabs — see [`View`]. **Absent in every record written before tabs could hold
    /// one, and absent whenever none is open**, so a plane that never opened a view writes the
    /// record it always wrote. It is not a version bump for the reason [`Chat::pinned`] gives:
    /// a bump drops every operator's open chats at the first launch after it, and a missing
    /// list reads honestly as no view tabs. An older charter reading a newer record ignores
    /// the key, because nothing here refuses one it does not know.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    views: Vec<ViewOnDisk>,
    /// See [`Record::relaunch_after_update`]. **Written only when true**, so an ordinary
    /// quit writes the record it always wrote, and absent reads as the ordinary quit every
    /// record before it was. Not a version bump, for [`Chat::pinned`]'s reason.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    relaunch_after_update: bool,
    /// See [`Record::clone`]. Absent in every record written before V43, which reads as a
    /// clone that adopts the one it is opened in. Not a version bump, for [`Chat::pinned`]'s
    /// reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clone: Option<arrival::CloneSeatOnDisk>,
    /// See [`Record::focus`]. **Absent whenever the window is on the whole workspace**, so a
    /// window that never focused a branch writes the record it always wrote; any value that
    /// does not read as one is absent too. Not a version bump, for [`Chat::pinned`]'s reason.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    focus: Option<FocusOnDisk>,
}

/// A value that does not read as `T` reads as absent, never as a record that does not read.
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FocusOnDisk {
    #[serde(default)]
    workspace: String,
    #[serde(default)]
    repo: String,
    /// The branch's folder, or empty for the repo's own.
    #[serde(default)]
    piece: String,
}

impl FocusOnDisk {
    /// The focus this names, or nothing when a name in it is not one charter would mint
    /// ([`Focus::named`]).
    fn held(self) -> Option<Focus> {
        let piece = (!self.piece.is_empty()).then_some(self.piece.as_str());
        Focus::named(&self.workspace, &self.repo, piece)
    }
}

impl From<&Focus> for FocusOnDisk {
    fn from(focus: &Focus) -> Self {
        Self {
            workspace: focus.workspace.clone(),
            repo: focus.repo.clone(),
            piece: focus.piece.clone().unwrap_or_default(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ViewOnDisk {
    /// The extension's id, or empty for charter's own.
    #[serde(default)]
    from: String,
    #[serde(default)]
    view: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    title: String,
    /// The workspace's name, or empty for the strip outside every workspace.
    #[serde(default)]
    workspace: String,
    #[serde(default)]
    at: u32,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    pinned: bool,
    /// See [`View::split`]. Absent for a view never split, and any number reads: one outside a
    /// share of the tab is forgotten, never a reason to drop the tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    split: Option<f64>,
}

impl ViewOnDisk {
    /// The view this line names, or nothing when it is not one charter would have written.
    ///
    /// **Every field is held to a shape**, for `ChatOnDisk`'s reason: this file is writable by
    /// whoever can write the plane's state directory, and these words reach a tab, a command the
    /// window sends back to the core, and — for an extension's view — the id the executor is
    /// asked about. A line that fails is dropped whole rather than repaired: a view with a
    /// mangled id is not a view the operator had open.
    fn held(self) -> Option<View> {
        let id_ok =
            |word: &str| word.chars().count() <= 64 && crate::contain::workspace_name_ok(word);
        if !id_ok(&self.view) {
            return None;
        }
        let from = if self.from.is_empty() {
            None
        } else if id_ok(&self.from) {
            Some(self.from)
        } else {
            return None;
        };
        // A key is empty — the whole plane — or one word of the alphabet charter mints names
        // in. A persona's name passes; a path does not. charter's own views keyed by a path of
        // names are the exception, and each is held to the one shape its writer spells and
        // nothing else ([`own_key_ok`], #1297).
        let keyed = match from.is_none().then(|| own_key_ok(&self.view, &self.key)) {
            Some(Some(shaped)) => shaped,
            _ => self.key.is_empty() || id_ok(&self.key),
        };
        if !keyed {
            return None;
        }
        // The title is only ever drawn, as a text node — but it is drawn on a tab, so it is
        // one line of a bounded length, and a title that is not one is the view's own id.
        let title = if self.title.trim().is_empty()
            || self.title.chars().count() > MOST_TITLE
            || self.title.chars().any(char::is_control)
        {
            self.view.clone()
        } else {
            self.title
        };
        Some(View {
            from,
            view: self.view,
            key: self.key,
            title,
            // A name that is not a workspace name files the tab outside every workspace,
            // which is where the window puts a tab whose workspace has gone anyway.
            workspace: Some(self.workspace).filter(|name| crate::contain::workspace_name_ok(name)),
            at: self.at,
            active: self.active,
            pinned: self.pinned,
            split: self
                .split
                .filter(|share| share.is_finite() && (1.0..=99.0).contains(share))
                .map(|share| share.round() as u8),
        })
    }
}

/// The slug of a new memory's tab, not written yet: the window's `memories.DRAFT`. A slug
/// [`crate::memstore::slug_ok`] refuses, so it names no file in any store.
const MEMORY_DRAFT: &str = "\\";

/// Whether `key` is the one shape charter's own view `view` is keyed by, or `None` for a view
/// whose key is not a path of names (#1297).
///
/// **A trust boundary**, for [`ViewOnDisk::held`]'s reason: each key reaches a command the
/// window sends back to the core. So every view has an exact shape, each segment passes the
/// rule its writer mints it by, and nothing else is held — no `..`, no `.`, no empty segment,
/// no root, no backslash but a memory draft's own, no control character. Never repaired: a key
/// that fails drops its tab.
///
/// - `todo`: `<ws>/<slug>` (`app/src/todos.ts`), the slug a todo's file stem.
/// - `memory`: `workspace/<ws>/<slug>`, `persona/<name>/<slug>` or `shared/<slug>`
///   (`app/src/memories.ts` `memoryKey`), the slug [`MEMORY_DRAFT`] for a memory not written yet.
/// - `memory-archive`: `workspace/<ws>`, `persona/<name>` or `shared` (`scopeKey`).
/// - `piece-files`: `<ws>/<repo>/<piece>`, or `<ws>/<repo>/` for the repo's own folder
///   (`app/src/pieceViews.ts`).
/// - `piece-file`, `piece-diff`: that branch, then a file's plain relative path in it.
/// - `session`: a session record's plane-relative path, read only as
///   [`crate::sessionrecord::locate`] reads it.
fn own_key_ok(view: &str, key: &str) -> Option<bool> {
    let name = |word: &str| word.chars().count() <= 64 && crate::contain::workspace_name_ok(word);
    let repo = |word: &str| word.chars().count() <= 64 && crate::contain::repo_name_ok(word);
    let persona = |word: &str| word.chars().count() <= 64 && crate::contain::persona_name_ok(word);
    let slug = |word: &str| crate::memstore::slug_ok(word);
    let memory_slug = |word: &str| word == MEMORY_DRAFT || slug(word);
    let branch =
        |ws: &str, r: &str, piece: &str| name(ws) && repo(r) && (piece.is_empty() || name(piece));
    let parts: Vec<&str> = key.split('/').collect();
    let shaped = match view {
        FILES_VIEW => matches!(parts.as_slice(), [ws, r, piece] if branch(ws, r, piece)),
        "piece-file" | "piece-diff" => matches!(parts.as_slice(),
            [ws, r, piece, path @ ..] if branch(ws, r, piece)
                && !path.is_empty()
                && path.iter().all(|step| crate::contain::segment_ok(step))),
        "todo" => matches!(parts.as_slice(), [ws, s] if name(ws) && slug(s)),
        "memory" => match parts.as_slice() {
            ["workspace", ws, s] => name(ws) && memory_slug(s),
            ["persona", who, s] => persona(who) && memory_slug(s),
            ["shared", s] => memory_slug(s),
            _ => false,
        },
        "memory-archive" => match parts.as_slice() {
            ["workspace", ws] => name(ws),
            ["persona", who] => persona(who),
            ["shared"] => true,
            _ => false,
        },
        "session" => crate::sessionrecord::locate(key).is_ok(),
        _ => return None,
    };
    Some(shaped && key.chars().count() <= MOST_KEY && !key.chars().any(char::is_control))
}

impl From<&View> for ViewOnDisk {
    fn from(view: &View) -> Self {
        Self {
            from: view.from.clone().unwrap_or_default(),
            view: view.view.clone(),
            key: view.key.clone(),
            title: view.title.clone(),
            workspace: view.workspace.clone().unwrap_or_default(),
            at: view.at,
            active: view.active,
            pinned: view.pinned,
            split: view.split.map(f64::from),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ChatOnDisk {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    name: String,
    /// The conversation's id, or empty. A value that is not an id reads as empty: the chat
    /// comes back as a new one rather than putting an unknown word on a command line.
    #[serde(default)]
    resume: String,
    #[serde(default)]
    active: bool,
    /// The profile's name, or empty. A value that is not a name charter would mint reads as
    /// empty — it reaches a sidebar and a refusal sentence, and a name off disk is a name
    /// somebody else may have written.
    #[serde(default)]
    profile: String,
    /// The persona's name, or empty, under the same rule.
    #[serde(default)]
    persona: String,
    /// `"show"` where this chat draws charter's footer in its pane, empty otherwise.
    ///
    /// The same word the chat's environment carries
    /// ([`crate::start::FOOTER_SHOW`]), so the record and the launch cannot come to mean
    /// different things by it. **Anything else reads as empty** — the default, and the only
    /// answer that is safe for a word off a file somebody else may have written.
    #[serde(default)]
    footer: String,
    /// Whether the operator pinned this chat. Absent in every record written before pins
    /// existed, and `false` is what was true of those — see [`Chat::pinned`].
    #[serde(default)]
    pinned: bool,
    /// The chat's number in its plane, or `0` where the record does not know one — see
    /// [`Chat::number`]. Zero rather than a missing key because that is the value serde
    /// defaults to, and because zero is not a number any chat is ever dealt: [`Chat::number`]
    /// counts from one.
    #[serde(default)]
    number: u32,
    /// The name the operator gave the chat, or absent — see [`Chat::label`]. Absent in every
    /// record written before a chat could have one, and whenever none was given, so a plane
    /// that never renamed a chat writes the record it always wrote. Held to [`label`] on the
    /// way in, and a value it refuses reads as absent: the chat comes back under its default.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    label: String,
    /// The chat a handoff opened this one from, or absent — see [`Chat::from`]. Absent for
    /// every chat the operator opened, so a plane that never handed off writes the record it
    /// always wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<FromOnDisk>,
    /// [`Chat::held`]: the persona whose grants the chat holds, `""` for none; absent where it
    /// holds its own. **Read failing closed**: any value present that is not a persona's name
    /// holds no persona's grants, never its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    held: Option<serde_json::Value>,
    /// Whether the person opened this task chat's tab — see [`Chat::tab_opened`]. Absent until
    /// they do, so a project with no task chat writes the record it always wrote.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    tab_opened: bool,
    /// The chat this chat's tab shows in place of it, by number — see [`Chat::shows`]. Absent
    /// (zero) while the tab shows its own chat, so a project where no tab was switched to a
    /// task writes the record it always wrote.
    #[serde(default, skip_serializing_if = "is_zero")]
    shows: u32,
    /// The chat whose tab this chat has a pane in, by number — see [`Chat::beside`]. Absent
    /// (zero) for a chat that is its tab's own or has no pane, so a project with no split
    /// writes the record it always wrote.
    #[serde(default, skip_serializing_if = "is_zero")]
    beside: u32,
    /// The workspace a rename moved this chat away from, or absent — see
    /// [`Chat::renamed_from`]. A value that is not a workspace name reads as absent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    renamed_from: String,
    /// `"off"` where the chat's last run was unsandboxed — see [`Chat::unsandboxed`] — and
    /// absent otherwise. Any other word reads as absent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    sandbox: String,
    /// The chat's ids — see [`Identity`]. Each is a ULID, absent when not known: every record
    /// written before ids existed, and a chat no host has started yet. A value that is not a
    /// ULID reads as absent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    device: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    run: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    resumed_from: String,
    /// The process the chat's program runs as, or absent — see [`Chat::pid`]. Zero, the
    /// whole process group to `kill(2)`, is never one and reads as absent.
    #[serde(default, skip_serializing_if = "is_zero")]
    pid: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// `word` where it is a ULID, else nothing: an id off a file somebody else may have written is
/// held to the one shape charter mints, in its canonical spelling.
pub fn a_ulid(word: &str) -> Option<String> {
    ulid::Ulid::from_string(word).ok().map(|id| id.to_string())
}

/// The one word a record's `sandbox` holds.
const SANDBOX_OFF: &str = "off";

/// A new ULID: how the host mints a chat's id and each of its runs' (ADR 0066).
pub fn mint() -> String {
    ulid::Ulid::generate().to_string()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FromOnDisk {
    #[serde(default)]
    chat: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    workspace: String,
    /// `"owed"`, `"sent"`, or absent for a handoff that asked for nothing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    report: String,
    /// `"task"`, or absent for a handoff (#1436).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    mode: String,
    /// How deep in its chain of dispatches, or absent for 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    depth: u32,
    /// The id of the chat the person started, which the lineage descends from, or absent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    root: String,
    /// The personas of the chats above, nearest first, `""` for a chat on no persona; absent
    /// in a record written before it was kept (#1521).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    above: Option<Vec<String>>,
    /// `"person"` for a chat the person started from that chat's tab (#1438), or absent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    by: String,
}

/// The personas above a chat as its record keeps them, held to what the app writes: one per
/// dispatch above it (`depth` of them), each a persona's name or `""` for a chat on no
/// persona. Anything else reads as a chain not kept (`None`), which the loop rule reads as one
/// it cannot read whole: never as a shorter chain (#1521).
fn chain_above(above: Vec<String>, depth: u32) -> Option<Vec<Option<String>>> {
    if usize::try_from(depth).ok() != Some(above.len()) {
        return None;
    }
    above
        .into_iter()
        .map(|persona| match persona.as_str() {
            "" => Some(None),
            name if crate::contain::persona_name_ok(name) => Some(Some(persona)),
            _ => None,
        })
        .collect()
}

/// What a record's `by` holds for a chat the person started from another chat's tab.
const BY_PERSON: &str = "person";

impl From<&HandedFrom> for FromOnDisk {
    fn from(from: &HandedFrom) -> Self {
        Self {
            chat: from.chat,
            name: from.name.clone(),
            workspace: from.workspace.word().to_owned(),
            report: from.report.word().to_owned(),
            mode: match from.mode {
                Mode::Handoff => String::new(),
                Mode::Task => from.mode.word().to_owned(),
            },
            depth: from.depth,
            root: from.root.clone().unwrap_or_default(),
            above: from.above.as_ref().map(|above| {
                above
                    .iter()
                    .map(|persona| persona.clone().unwrap_or_default())
                    .collect()
            }),
            by: if from.by_person {
                BY_PERSON.to_owned()
            } else {
                String::new()
            },
        }
    }
}

impl FromOnDisk {
    /// Held to what the app would have written: a number a chat can have, a name [`label`]
    /// takes, a workspace name that can be one. Anything else reads as no handoff at all — the
    /// note is drawn, and the pairing is what a report is checked against.
    fn sound(self) -> Option<HandedFrom> {
        let depth = self.depth.min(crate::dispatchdecision::DEEPEST);
        Some(HandedFrom {
            chat: (self.chat > 0).then_some(self.chat)?,
            name: label(&self.name).ok().flatten()?,
            workspace: crate::active::Place::read(&self.workspace)?,
            report: Owed::of(&self.report),
            mode: Mode::of(&self.mode),
            // Held to the ceiling: a depth no chain can have never reads as a shallower one.
            depth,
            // Held to the one shape an id is minted in: anything else names no lineage.
            root: a_ulid(&self.root),
            above: self.above.and_then(|above| chain_above(above, depth)),
            by_person: self.by == BY_PERSON,
        })
    }
}

impl From<&Record> for OnDisk {
    fn from(record: &Record) -> Self {
        Self {
            version: VERSION,
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_secs())
                .unwrap_or_default(),
            chats: record
                .chats
                .iter()
                .map(|chat| ChatOnDisk {
                    program: chat.program.clone(),
                    args: chat.args.clone(),
                    cwd: chat
                        .cwd
                        .as_ref()
                        .map(|cwd| cwd.display().to_string())
                        .unwrap_or_default(),
                    name: chat.name.clone(),
                    resume: chat
                        .resume
                        .as_ref()
                        .map(SessionId::to_string)
                        .unwrap_or_default(),
                    active: chat.active,
                    profile: chat.profile.clone().unwrap_or_default(),
                    persona: chat.persona.clone().unwrap_or_default(),
                    footer: if chat.show_footer {
                        crate::start::FOOTER_SHOW.to_owned()
                    } else {
                        String::new()
                    },
                    pinned: chat.pinned,
                    number: chat.number.unwrap_or_default(),
                    label: chat.label.clone().unwrap_or_default(),
                    from: chat.from.as_ref().map(FromOnDisk::from),
                    held: chat.held.as_ref().map(|held| {
                        serde_json::Value::String(held.persona.clone().unwrap_or_default())
                    }),
                    tab_opened: chat.tab_opened,
                    shows: chat.shows.unwrap_or_default(),
                    beside: chat.beside.unwrap_or_default(),
                    renamed_from: chat.renamed_from.clone().unwrap_or_default(),
                    sandbox: if chat.unsandboxed {
                        SANDBOX_OFF.to_owned()
                    } else {
                        String::new()
                    },
                    id: chat.identity.id.clone().unwrap_or_default(),
                    device: chat.identity.device.clone().unwrap_or_default(),
                    run: chat.identity.run.clone().unwrap_or_default(),
                    resumed_from: chat.identity.resumed_from.clone().unwrap_or_default(),
                    pid: chat.pid.unwrap_or_default(),
                })
                .collect(),
            dealt: highest_dealt(record),
            views: record.views.iter().map(ViewOnDisk::from).collect(),
            relaunch_after_update: record.relaunch_after_update,
            clone: record
                .clone_seat
                .as_ref()
                .and_then(arrival::CloneSeatOnDisk::of),
            focus: record.focus.as_ref().map(FocusOnDisk::from),
        }
    }
}

/// The counter a record goes to disk with: what it carries, never below a number it names.
///
/// The two could only disagree through a caller that built a [`Record`] by hand — but the
/// one thing this counter must never do is come back smaller than a number still in the
/// file, because that is the whole of charter-app#90 handed back. It is cheaper to hold the
/// invariant here, where every write passes, than to trust each caller with it.
fn highest_dealt(record: &Record) -> u32 {
    record
        .chats
        .iter()
        .filter_map(|chat| chat.number)
        .fold(record.dealt, u32::max)
}

impl From<ChatOnDisk> for Chat {
    fn from(chat: ChatOnDisk) -> Self {
        Self {
            program: chat.program,
            args: chat.args,
            cwd: (!chat.cwd.is_empty()).then(|| PathBuf::from(chat.cwd)),
            name: chat.name,
            resume: SessionId::new(chat.resume).ok(),
            active: chat.active,
            // Held to the shape charter mints, for the same reason `resume` is: these come
            // off a file anyone who can write the plane's state directory can write.
            profile: Some(chat.profile).filter(|name| crate::contain::workspace_name_ok(name)),
            persona: Some(chat.persona).filter(|name| crate::contain::persona_name_ok(name)),
            // One word means "show" and every other word means the default, which is what
            // the app did before ADR 0029 and what a record written before it says.
            show_footer: chat.footer == crate::start::FOOTER_SHOW,
            pinned: chat.pinned,
            // Zero is "this record does not say", and so is any value a chat could not have
            // been dealt. Nothing else is held against it: the number keys a path component
            // that `contain::segment_ok` would pass for any integer, and a record that names
            // a number no chat here holds costs at most a gap in the counting.
            number: (chat.number > 0).then_some(chat.number),
            label: label(&chat.label).ok().flatten(),
            from: chat.from.and_then(FromOnDisk::sound),
            held: chat.held.map(|value| HeldGrants {
                persona: value
                    .as_str()
                    .filter(|persona| crate::personas::valid_name(persona))
                    .map(str::to_owned),
            }),
            tab_opened: chat.tab_opened,
            // Zero is "its own chat", as it is "this record does not say" for `number`.
            shows: (chat.shows > 0).then_some(chat.shows),
            beside: (chat.beside > 0).then_some(chat.beside),
            renamed_from: Some(chat.renamed_from)
                .filter(|name| crate::contain::workspace_name_ok(name)),
            unsandboxed: chat.sandbox == SANDBOX_OFF,
            identity: Identity {
                id: a_ulid(&chat.id),
                device: a_ulid(&chat.device),
                run: a_ulid(&chat.run),
                resumed_from: a_ulid(&chat.resumed_from),
            },
            pid: (chat.pid > 0).then_some(chat.pid),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;

    #[cfg(target_os = "macos")]
    #[test]
    fn a_record_folder_the_filesystem_refuses_to_make_is_named() {
        // #1421: an EPERM here printed only "Operation not permitted (os error 1)".
        let plane = tempfile::tempdir().unwrap();
        let dir = path(plane.path()).parent().unwrap().to_path_buf();
        let state = dir.parent().unwrap();
        fs::create_dir_all(state).unwrap();
        let _frozen = crate::rewrite::frozen::Frozen::at(state);

        let refused = write(plane.path(), &Record::default()).unwrap_err();

        crate::rewrite::frozen::names(&refused, &dir);
    }

    /// The ULIDs the core's tests spell a chat's ids with, one set for every module.
    pub(crate) const CHAT: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7F";
    pub(crate) const DEVICE: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7G";
    pub(crate) const RUN: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7H";
    pub(crate) const RESUMED: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7J";

    #[test]
    fn a_minted_id_is_a_ulid_and_a_ulid_reads_back_as_itself() {
        let id = mint();
        assert_eq!(a_ulid(&id), Some(id.clone()));
        assert_ne!(mint(), id, "a new one each time");
        assert_eq!(a_ulid("-rf"), None);
    }

    fn claude(name: &str, resume: Option<&str>) -> Chat {
        Chat {
            program: "claude".to_owned(),
            args: vec!["--model".to_owned(), "opus".to_owned()],
            cwd: Some(PathBuf::from("/Users/aharon/IdeaProjects/charter")),
            name: name.to_owned(),
            resume: resume.map(|id| SessionId::new(id).expect("a valid id in a test")),
            active: false,
            profile: None,
            persona: None,
            show_footer: false,
            pinned: false,
            number: None,
            label: None,
            from: None,
            renamed_from: None,
            ..Default::default()
        }
    }

    const ID: &str = "11111111-2222-4333-8444-555555555555";

    #[test]
    fn what_was_written_is_what_the_next_launch_reads() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![claude("ide.7", Some(ID)), claude("ide.8", None)],
            ..Default::default()
        };

        write(plane.path(), &record).expect("the record is written");

        assert_eq!(read(plane.path()), record);
    }

    #[test]
    fn the_record_is_written_where_the_plane_format_says() {
        let plane = tempfile::tempdir().unwrap();

        write(plane.path(), &Record::default()).expect("the record is written");

        assert!(plane.path().join(".charter/app/reopen.json").is_file());
    }

    #[test]
    fn a_plane_with_no_record_has_nothing_to_reopen() {
        let plane = tempfile::tempdir().unwrap();

        assert_eq!(read(plane.path()), Record::default());
    }

    #[test]
    fn a_record_of_another_version_is_ignored_whole() {
        let plane = tempfile::tempdir().unwrap();
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", Some(ID))],
                ..Default::default()
            },
        )
        .unwrap();
        let file = path(plane.path());
        let text = fs::read_to_string(&file).unwrap();
        fs::write(&file, text.replace("\"version\": 1", "\"version\": 2")).unwrap();

        assert_eq!(read(plane.path()), Record::default());
    }

    #[test]
    fn a_file_that_is_not_the_record_at_all_is_nothing_to_reopen() {
        let plane = tempfile::tempdir().unwrap();
        fs::create_dir_all(path(plane.path()).parent().unwrap()).unwrap();
        fs::write(path(plane.path()), "{ this is not json").unwrap();

        assert_eq!(read(plane.path()), Record::default());
    }

    #[test]
    fn a_chat_whose_recorded_conversation_could_be_read_as_a_flag_comes_back_without_one() {
        // The file is on disk and every value in it reaches a command line. A chat with a
        // hostile id is still reopened — it is a chat the operator had — but as a new one.
        let plane = tempfile::tempdir().unwrap();
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", Some(ID))],
                ..Default::default()
            },
        )
        .unwrap();
        let file = path(plane.path());
        let text = fs::read_to_string(&file).unwrap();
        fs::write(&file, text.replace(ID, "--dangerously-skip-permissions")).unwrap();

        let back = read(plane.path());

        assert_eq!(back.chats.len(), 1, "the chat itself was dropped: {back:?}");
        assert_eq!(back.chats[0].resume, None);
    }

    #[test]
    fn a_chat_with_a_recorded_conversation_is_resumed() {
        let chat = claude("ide.7", Some(ID));

        let launch = chat.launch();

        assert_eq!(launch.program, "claude");
        assert_eq!(
            launch.args,
            vec!["--resume", ID, "--name", "ide.7", "--model", "opus"]
        );
        assert_eq!(launch.how, Reopened::Resumed(SessionId::new(ID).unwrap()));
    }

    #[test]
    fn a_chat_with_no_recorded_conversation_comes_back_as_a_new_one() {
        // A Codex chat is always this: Codex reports its id through a hook inside its first
        // turn, so until hooks land the app has none to record.
        let chat = Chat {
            program: "codex".to_owned(),
            ..claude("ide.7", None)
        };

        let launch = chat.launch();

        assert_eq!(launch.program, "codex");
        assert_eq!(launch.args, vec!["--model", "opus"]);
        assert_eq!(launch.how, Reopened::Fresh(Fresh::NoConversationRecorded));
    }

    #[test]
    fn a_new_claude_chat_is_started_under_an_id_the_app_chose_so_it_can_be_resumed_next_time() {
        let chat = claude("ide.7", None);

        let launch = chat.launch();

        assert_eq!(launch.how, Reopened::Fresh(Fresh::NoConversationRecorded));
        let chosen = launch
            .args
            .iter()
            .position(|word| word == "--session-id")
            .map(|at| launch.args[at + 1].clone())
            .expect("a new Claude Code chat is given an id");
        assert!(SessionId::new(&chosen).is_ok(), "{chosen:?} is not an id");
        assert_eq!(
            launch.args[launch.args.len() - 2..],
            ["--model", "opus"],
            "the chat's own arguments no longer come last: {:?}",
            launch.args
        );
    }

    #[test]
    fn a_chat_a_workspace_rename_left_without_its_conversation_starts_fresh_and_says_why() {
        // charter#367, D10: Claude Code cannot find the conversation under the new directory,
        // so the rename dropped it. The chat starts a new one under an id charter chose, which
        // is what the next quit records, and says why rather than "nothing recorded".
        let chat = Chat {
            renamed_from: Some("alpha".to_owned()),
            ..claude("ide.7", None)
        };

        let launch = chat.launch();

        assert_eq!(launch.how, Reopened::Fresh(Fresh::WorkspaceRenamed));
        assert!(
            launch.args.contains(&"--session-id".to_owned()),
            "{:?}",
            launch.args
        );
        assert!(launch.session.is_some());
    }

    #[test]
    fn a_profile_start_that_knew_no_chat_is_told_the_rename_and_nothing_else_is_changed() {
        let renamed = Chat {
            renamed_from: Some("alpha".to_owned()),
            ..claude("ide.7", None)
        };
        let nothing = Reopened::Fresh(Fresh::NoConversationRecorded);

        assert_eq!(
            renamed.told(nothing.clone()),
            Reopened::Fresh(Fresh::WorkspaceRenamed)
        );
        assert_eq!(claude("ide.7", None).told(nothing.clone()), nothing);
        let resumed = Reopened::Resumed(SessionId::new(ID).unwrap());
        assert_eq!(renamed.told(resumed.clone()), resumed);
    }

    #[test]
    fn the_workspace_a_rename_moved_a_chat_from_survives_a_quit_and_a_bad_one_reads_as_none() {
        let plane = tempfile::tempdir().unwrap();
        let renamed = Chat {
            renamed_from: Some("alpha".to_owned()),
            ..claude("ide.7", None)
        };
        write(
            plane.path(),
            &Record {
                chats: vec![renamed.clone(), claude("ide.8", Some(ID))],
                ..Default::default()
            },
        )
        .unwrap();
        let text = fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches("renamed_from").count(), 1, "{text}");

        assert_eq!(read(plane.path()).chats[0], renamed);

        fs::write(path(plane.path()), text.replace("\"alpha\"", "\"../x\"")).unwrap();
        assert_eq!(read(plane.path()).chats[0].renamed_from, None);
    }

    /// ADR 0067 §7: the record says a chat's last run was unsandboxed, so the next run can say
    /// the sandbox came back on — and it is never read as an opt-out: the launch it builds is
    /// the same either way.
    #[test]
    fn a_chat_that_ran_unsandboxed_is_recorded_so_and_its_relaunch_is_built_the_same() {
        let plane = tempfile::tempdir().unwrap();
        let unsandboxed = Chat {
            unsandboxed: true,
            ..claude("ide.7", Some(ID))
        };
        write(
            plane.path(),
            &Record {
                chats: vec![unsandboxed.clone(), claude("ide.8", Some(ID))],
                ..Default::default()
            },
        )
        .unwrap();
        let text = fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches("\"sandbox\": \"off\"").count(), 1, "{text}");

        let back = read(plane.path());
        assert_eq!(back.chats[0], unsandboxed);
        assert!(!back.chats[1].unsandboxed);
        assert_eq!(
            back.chats[0].launch(),
            claude("ide.7", Some(ID)).launch(),
            "nothing about the next start reads it"
        );

        fs::write(path(plane.path()), text.replace("\"off\"", "\"none\"")).unwrap();
        assert!(
            !read(plane.path()).chats[0].unsandboxed,
            "only the one word"
        );
    }

    #[test]
    fn the_process_a_chat_runs_as_is_recorded_and_a_chat_with_none_writes_no_key() {
        let plane = tempfile::tempdir().unwrap();
        let running = Chat {
            pid: Some(4242),
            ..claude("ide.7", None)
        };
        write(
            plane.path(),
            &Record {
                chats: vec![running.clone(), claude("ide.8", Some(ID))],
                ..Default::default()
            },
        )
        .unwrap();
        let text = fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches("\"pid\"").count(), 1, "{text}");

        assert_eq!(read(plane.path()).chats[0], running);
        assert_eq!(read(plane.path()).chats[1].pid, None);

        fs::write(path(plane.path()), text.replace("4242", "0")).unwrap();
        assert_eq!(read(plane.path()).chats[0].pid, None, "zero is no process");
    }

    #[test]
    fn the_id_a_new_chat_was_given_is_the_one_the_next_quit_records() {
        // Without this a chat could never be resumed a second time: the app would choose an
        // id, hand it to the harness, and forget it before the quit that has to write it down.
        let chat = claude("ide.7", None);

        let launch = chat.launch();

        let chosen = launch.session.as_ref().expect("the chat is under an id");
        assert!(
            launch.args.contains(&chosen.to_string()),
            "the recorded id {chosen} is not the one the harness was given: {:?}",
            launch.args
        );
    }

    #[test]
    fn codex_resumes_through_its_subcommand_before_the_chat_s_own_arguments() {
        // `codex resume <id>` is a subcommand. `codex --model opus resume <id>` is not the
        // same command line, and with a prompt among the arguments it is not one at all.
        let chat = Chat {
            program: "codex".to_owned(),
            ..claude("ide.7", Some(ID))
        };

        let launch = chat.launch();

        assert_eq!(launch.args, vec!["resume", ID, "--model", "opus"]);
    }

    #[test]
    fn opencode_resumes_by_naming_its_session() {
        let chat = Chat {
            program: "opencode".to_owned(),
            args: Vec::new(),
            ..claude("ide.7", Some("ses_3a1b2c3d4e5f"))
        };

        let launch = chat.launch();

        assert_eq!(launch.args, vec!["-s", "ses_3a1b2c3d4e5f"]);
        assert_eq!(
            launch.how,
            Reopened::Resumed(SessionId::new("ses_3a1b2c3d4e5f").unwrap())
        );
    }

    #[test]
    fn a_chat_s_current_conversation_is_read_by_the_number_it_answers_to() {
        // The chat asks by `$CHARTER_SESSION_ID`, which is the number the record keeps.
        let plane = tempfile::tempdir().unwrap();
        let other = "99999999-2222-4333-8444-555555555555";
        write(
            plane.path(),
            &Record {
                chats: vec![
                    Chat {
                        number: Some(3),
                        ..claude("ide.3", Some(other))
                    },
                    Chat {
                        number: Some(7),
                        ..claude("ide.7", Some(ID))
                    },
                    Chat {
                        number: Some(8),
                        ..claude("ide.8", None)
                    },
                ],
                ..Default::default()
            },
        )
        .expect("the record is written");

        let conversation = |chat| conversation_of(plane.path(), chat).expect("the record reads");

        assert_eq!(conversation(7), Some(SessionId::new(ID).unwrap()));
        assert_eq!(
            conversation(8),
            None,
            "a chat whose harness has named none yet"
        );
        assert_eq!(conversation(9), None, "a chat the record does not hold");
    }

    #[test]
    fn a_chat_s_id_and_current_run_survive_a_quit_and_are_read_by_its_number() {
        let plane = tempfile::tempdir().unwrap();
        let identity = Identity {
            id: Some(CHAT.to_owned()),
            device: Some(DEVICE.to_owned()),
            run: Some(RUN.to_owned()),
            resumed_from: Some(RESUMED.to_owned()),
        };
        write(
            plane.path(),
            &Record {
                chats: vec![
                    Chat {
                        number: Some(7),
                        identity: identity.clone(),
                        ..claude("ide.7", Some(ID))
                    },
                    Chat {
                        number: Some(8),
                        ..claude("ide.8", None)
                    },
                ],
                ..Default::default()
            },
        )
        .expect("the record is written");

        assert_eq!(read(plane.path()).chats[0].identity, identity);
        let of = |chat| identity_of(plane.path(), chat).expect("the record reads");
        assert_eq!(of(7), Some(identity));
        assert_eq!(
            of(8),
            Some(Identity::default()),
            "a chat recorded before ids: one is minted at this launch"
        );
        assert_eq!(of(9), None, "a chat the record does not hold");
    }

    #[test]
    fn a_record_written_before_chats_had_ids_writes_none_and_a_bad_id_reads_as_none() {
        let plane = tempfile::tempdir().unwrap();
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", None)],
                ..Default::default()
            },
        )
        .expect("the record is written");
        let text = fs::read_to_string(path(plane.path())).unwrap();
        for key in ["\"id\"", "\"device\"", "\"run\"", "\"resumed_from\""] {
            assert!(!text.contains(key), "{key} is written only when known");
        }

        let written = text.replace(
            "\"program\": \"claude\"",
            &format!(
                "\"program\": \"claude\", \"id\": \"-rf\", \"device\": \"\", \"run\": \"{RUN}\""
            ),
        );
        fs::write(path(plane.path()), written).unwrap();
        assert_eq!(
            read(plane.path()).chats[0].identity,
            Identity {
                run: Some(RUN.to_owned()),
                ..Identity::default()
            },
            "a value that is not a ULID is not one"
        );
    }

    #[test]
    fn a_chat_in_a_plane_with_no_record_is_in_no_conversation_charter_knows() {
        let plane = tempfile::tempdir().unwrap();

        assert_eq!(
            conversation_of(plane.path(), 7).expect("no record reads"),
            None
        );
    }

    #[test]
    fn a_resumed_chat_stays_under_the_id_it_was_resumed_by() {
        let launch = claude("ide.7", Some(ID)).launch();

        assert_eq!(launch.session, Some(SessionId::new(ID).unwrap()));
    }

    #[test]
    fn a_codex_chat_is_under_no_id_the_app_knows_because_codex_chose_it() {
        let launch = Chat {
            program: "codex".to_owned(),
            ..claude("ide.7", None)
        }
        .launch();

        assert_eq!(launch.session, None);
    }

    #[test]
    fn a_chat_whose_own_arguments_name_a_session_is_not_given_a_second_one() {
        // `charter/harness/base.py:308` — the operator's flag wins. Charter adding
        // `--resume` beside one the operator typed gives a command line no harness has been
        // measured against.
        let chat = Chat {
            args: vec!["--continue".to_owned()],
            ..claude("ide.7", Some(ID))
        };

        let launch = chat.launch();

        assert_eq!(launch.args, vec!["--continue"]);
        assert_eq!(launch.session, None);
        assert_eq!(
            launch.how,
            Reopened::Fresh(Fresh::SessionNamedByTheOperator)
        );
    }

    #[test]
    fn a_pane_running_a_shell_comes_back_as_a_shell_and_says_why_it_is_not_resumed() {
        let chat = Chat {
            program: "/bin/zsh".to_owned(),
            args: vec![],
            resume: None,
            ..claude("ide.9", None)
        };

        let launch = chat.launch();

        assert_eq!(launch.program, "/bin/zsh");
        assert_eq!(launch.args, Vec::<String>::new());
        assert_eq!(launch.how, Reopened::Fresh(Fresh::NoResumeForThisProgram));
        assert_eq!(launch.session, None);
    }

    #[test]
    fn writing_the_record_leaves_nothing_beside_it() {
        // The record is written beside itself and renamed over, so a launch never reads
        // half of one. This is the guard on that: a write that stopped at the file beside
        // it would leave two here, and the record itself would never appear.
        let plane = tempfile::tempdir().unwrap();

        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", Some(ID))],
                ..Default::default()
            },
        )
        .unwrap();

        let left: Vec<String> = fs::read_dir(plane.path().join(".charter/app"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["reopen.json"]);
    }

    #[test]
    fn a_record_written_over_an_older_one_replaces_it() {
        let plane = tempfile::tempdir().unwrap();
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", Some(ID))],
                ..Default::default()
            },
        )
        .unwrap();

        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.8", None)],
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(read(plane.path()).chats.len(), 1);
        assert_eq!(read(plane.path()).chats[0].name, "ide.8");
    }

    #[test]
    fn the_chat_that_was_in_front_is_the_one_marked_active() {
        let plane = tempfile::tempdir().unwrap();
        let front = Chat {
            active: true,
            profile: None,
            persona: None,
            ..claude("ide.8", None)
        };
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", None), front],
                ..Default::default()
            },
        )
        .unwrap();

        let back = read(plane.path());

        assert_eq!(
            back.chats
                .iter()
                .filter(|chat| chat.active)
                .map(|chat| chat.name.as_str())
                .collect::<Vec<_>>(),
            vec!["ide.8"]
        );
    }

    /// A plane whose `.charter/app` is a symlink pointing out of it, and the outside
    /// directory it points at.
    fn a_plane_whose_record_directory_is_a_link() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().join("plane");
        let outside = held.path().join("outside");
        std::fs::create_dir_all(plane.join(".charter")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, plane.join(".charter/app")).unwrap();
        (held, plane, outside)
    }

    fn one_chat() -> Record {
        Record {
            views: Vec::new(),
            dealt: 0,
            relaunch_after_update: false,
            clone_seat: None,
            focus: None,
            chats: vec![Chat {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), "touch /tmp/pwned".into()],
                cwd: Some(PathBuf::from("/tmp")),
                name: "planted".into(),
                resume: None,
                active: true,
                profile: None,
                persona: None,
                show_footer: false,
                pinned: false,
                number: None,
                label: None,
                from: None,
                renamed_from: None,
                ..Default::default()
            }],
        }
    }

    #[test]
    fn a_record_directory_that_is_a_link_out_of_the_plane_is_not_written_through() {
        let (_held, plane, outside) = a_plane_whose_record_directory_is_a_link();

        let refused = write(&plane, &one_chat());

        assert!(refused.is_err(), "writing through the link was allowed");
        assert!(
            !outside.join("reopen.json").exists(),
            "the record was written outside the plane"
        );
    }

    #[test]
    fn a_record_reached_through_a_link_is_nothing_to_put_back() {
        let (_held, plane, outside) = a_plane_whose_record_directory_is_a_link();
        // Whoever planted the link also planted what the app would launch.
        std::fs::write(
            outside.join("reopen.json"),
            r#"{"version":1,"at":1789000000,"chats":[{"program":"/bin/sh","args":["-c","touch /tmp/pwned"],"cwd":"/tmp","name":"planted","resume":"","active":true}]}"#,
        )
        .unwrap();

        let read_back = read(&plane);

        assert_eq!(
            read_back,
            Record::default(),
            "the app took its launch from outside the plane"
        );
    }

    #[test]
    fn the_record_file_itself_being_a_link_is_refused_too() {
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().join("plane");
        let outside = held.path().join("outside");
        std::fs::create_dir_all(plane.join(".charter/app")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(outside.join("planted.json"), path(&plane)).unwrap();

        assert!(write(&plane, &one_chat()).is_err());
        assert!(!outside.join("planted.json").exists());
    }

    #[test]
    fn an_ordinary_plane_still_writes_and_reads_its_record() {
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().to_path_buf();

        write(&plane, &one_chat()).expect("an ordinary plane writes its record");

        assert_eq!(read(&plane), one_chat());
    }

    #[test]
    fn a_record_that_is_a_link_is_refused_and_what_it_points_at_is_untouched() {
        // #434. The temp file the bytes land on is gated by `rewrite`'s walk, and its own
        // tests plant links there; this is the record's own name.
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().join("plane");
        let outside = held.path().join("outside");
        std::fs::create_dir_all(plane.join(".charter/app")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let theirs = outside.join("theirs.json");
        std::fs::write(&theirs, "THEIRS\n").unwrap();
        std::os::unix::fs::symlink(&theirs, path(&plane)).unwrap();

        assert!(
            write(&plane, &one_chat()).is_err(),
            "a linked record was written"
        );
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "THEIRS\n");
        assert!(path(&plane).is_symlink());
    }

    #[test]
    fn a_record_write_that_dies_before_its_rename_leaves_the_old_record_whole() {
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().to_path_buf();
        write(&plane, &one_chat()).unwrap();
        let before = std::fs::read(path(&plane)).unwrap();
        let _killed = crate::rewrite::hook::set(|_, _| Err(std::io::Error::other("killed")));

        assert!(write(&plane, &Record::default()).is_err());

        assert_eq!(std::fs::read(path(&plane)).unwrap(), before);
        assert_eq!(read(&plane), one_chat());
    }

    #[test]
    fn the_record_is_private() {
        // #434: it names programs to run, so it is charter's own state at 0600.
        use std::os::unix::fs::PermissionsExt;
        let held = tempfile::tempdir().unwrap();
        write(held.path(), &one_chat()).unwrap();
        let mode = std::fs::metadata(path(held.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_record_that_is_not_a_plain_file_is_refused_instead_of_read_for_ever() {
        // A FIFO is not a link, so a link check waves it through, and read_to_string on one
        // never returns — at launch, before there is a window to close.
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().to_path_buf();
        std::fs::create_dir_all(plane.join(".charter/app")).unwrap();
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(path(&plane)))
            .expect("mkfifo runs");
        assert!(made.success(), "the test needs a fifo to plant");

        // In a thread, because the whole point is that the unguarded version never returns.
        let (say, heard) = std::sync::mpsc::channel();
        let asked = plane.clone();
        std::thread::spawn(move || say.send(read_or_refusal(&asked).is_err()));
        let answered = heard
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("reading a fifo record must not block");

        assert!(answered, "a fifo record was accepted");
    }

    #[test]
    fn a_refusal_is_returned_rather_than_read_as_an_empty_record() {
        let (_held, plane, _outside) = a_plane_whose_record_directory_is_a_link();

        let refusal = read_or_refusal(&plane).expect_err("a linked record is a refusal");

        assert!(
            refusal.to_string().contains(".charter/app"),
            "the refusal must name the path an operator has to repair: {refusal}"
        );
        // And the forgiving door still answers, for callers that only want what to reopen.
        assert_eq!(read(&plane), Record::default());
    }

    #[test]
    fn a_record_too_large_to_be_one_is_refused_rather_than_read_whole() {
        // git cannot carry a fifo, but it carries a sparse giant that arrives full size,
        // and reading it at launch is the same hang by another road.
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().to_path_buf();
        std::fs::create_dir_all(plane.join(".charter/app")).unwrap();
        let record = std::fs::File::create(path(&plane)).unwrap();
        record.set_len(MAX_BYTES + 1).unwrap();

        let refusal = read_or_refusal(&plane).expect_err("an oversized record is a refusal");

        assert!(
            refusal.to_string().contains("never larger than"),
            "{refusal}"
        );
    }

    #[test]
    fn a_record_of_an_honest_size_is_still_read() {
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().to_path_buf();
        write(&plane, &one_chat()).unwrap();

        assert_eq!(read_or_refusal(&plane).unwrap(), one_chat());
    }

    #[test]
    fn a_record_that_exists_but_cannot_be_read_is_a_refusal_not_an_empty_plane() {
        use std::os::unix::fs::PermissionsExt;
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().to_path_buf();
        write(&plane, &one_chat()).unwrap();
        std::fs::set_permissions(path(&plane), std::fs::Permissions::from_mode(0o000)).unwrap();

        let read_back = read_or_refusal(&plane);

        // Running as root reads it anyway; the point is only that it is never a silent empty.
        if let Ok(record) = read_back {
            assert_eq!(
                record,
                one_chat(),
                "an unreadable record read as an empty one"
            );
        }
    }

    #[test]
    fn a_plane_with_no_record_at_all_is_simply_nothing_to_reopen() {
        let held = tempfile::tempdir().unwrap();

        assert_eq!(read_or_refusal(held.path()).unwrap(), Record::default());
    }

    // ----- a pinned chat (ADR 0039, stored per ADR 0040) -----

    #[test]
    fn a_pinned_chat_comes_back_pinned() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            views: Vec::new(),
            dealt: 0,
            relaunch_after_update: false,
            clone_seat: None,
            focus: None,
            chats: vec![
                Chat {
                    pinned: true,
                    ..claude("ide.7", Some(ID))
                },
                claude("ide.8", None),
            ],
        };

        write(plane.path(), &record).expect("the record is written");

        assert_eq!(read(plane.path()), record);
    }

    #[test]
    fn a_record_written_before_pins_existed_reads_as_nothing_pinned() {
        // **This is why there is no version bump**, although ADR 0039 expected one:
        // a bump would read every such record as "nothing to put back" and take the
        // operator's open chats with it at the first launch after the upgrade. A field is
        // owed a bump when its absence cannot be read honestly, and this one's reads as
        // `false` — which is what was true of every record written before pins existed.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"chats":[{"program":"claude","name":"ide.7"}]}"#,
        )
        .unwrap();

        let back = read(plane.path());

        assert_eq!(back.chats.len(), 1);
        assert!(!back.chats[0].pinned);
    }

    #[test]
    fn a_pin_that_is_not_a_boolean_is_read_as_no_record_at_all() {
        // Every other field off this file is held to what it has to be, and a value of the
        // wrong type fails the parse — which is this file's oldest rule for a record it
        // cannot understand, and the one direction that never invents an arrangement.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"chats":[{"program":"claude","name":"i","pinned":"yes"}]}"#,
        )
        .unwrap();

        assert_eq!(read(plane.path()), Record::default());
    }

    // ----- the number a chat keeps, and the numbers the plane has spent (charter-app#90) --

    #[test]
    fn a_chat_comes_back_under_the_number_it_was_recorded_with() {
        // Without this the number is the chat's POSITION in the record, which changes the
        // moment another chat is closed — and the number is the key
        // `.charter/sessions/<n>.workspace` and `<n>.lock` are written at.
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            views: Vec::new(),
            chats: vec![
                Chat {
                    number: Some(2),
                    ..claude("ide.7", Some(ID))
                },
                Chat {
                    number: Some(5),
                    ..claude("ide.8", None)
                },
            ],
            dealt: 5,
            relaunch_after_update: false,
            clone_seat: None,
            focus: None,
        };

        write(plane.path(), &record).expect("the record is written");

        assert_eq!(read(plane.path()), record);
    }

    #[test]
    fn a_record_written_before_chats_kept_their_numbers_says_nothing_about_them() {
        // Every operator has one of these at the first launch after this change. Reading a
        // missing key as "no number" is what lets the launch deal them in order, which is
        // what it always did; inventing one would file a chat under a stranger's pointer on
        // purpose.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"chats":[{"program":"claude","name":"ide.7"}]}"#,
        )
        .unwrap();

        let back = read(plane.path());

        assert_eq!(back.chats[0].number, None);
        assert_eq!(back.dealt, 0);
    }

    #[test]
    fn a_number_of_zero_is_read_as_no_number_at_all() {
        // Zero is what serde defaults the key to and is no chat's number: `Sessions` counts
        // from one. A record that says zero is a record that says nothing.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"chats":[{"program":"claude","name":"i","number":0}]}"#,
        )
        .unwrap();

        assert_eq!(read(plane.path()).chats[0].number, None);
    }

    #[test]
    fn the_counter_written_down_is_never_below_a_number_the_record_names() {
        // A caller that built a record by hand, or one field updated and not the other,
        // would otherwise write a counter that hands the next launch a number two chats
        // answer to.
        let plane = tempfile::tempdir().unwrap();

        write(
            plane.path(),
            &Record {
                views: Vec::new(),
                chats: vec![Chat {
                    number: Some(6),
                    ..claude("ide.7", None)
                }],
                dealt: 1,
                relaunch_after_update: false,
                clone_seat: None,
                focus: None,
            },
        )
        .expect("the record is written");

        assert_eq!(read(plane.path()).dealt, 6);
    }

    #[test]
    fn a_record_whose_counter_was_edited_below_its_chats_is_read_with_it_raised() {
        // The same invariant on the way in. This file is one anybody who can write the
        // plane's state directory can write, and the one thing a counter must never do is
        // come back smaller than a number still in the file.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"dealt":1,"chats":[{"program":"claude","name":"i","number":4}]}"#,
        )
        .unwrap();

        assert_eq!(read(plane.path()).dealt, 4);
    }

    // ----- view tabs (ADR 0043, as amended: a pane holds a session or a view) -----

    fn persona_view(name: &str) -> View {
        View {
            from: None,
            view: "persona".into(),
            key: name.into(),
            title: name.into(),
            workspace: Some("ide".into()),
            at: 1,
            active: true,
            pinned: false,
            split: None,
        }
    }

    #[test]
    fn a_vault_s_tab_comes_back_under_any_name_charter_gives_a_vault() {
        // charter-app#235: a vault's tab is `{ view: "vault", key: <vault> }`, and every name
        // `registry::name_ok` accepts has to survive the record, or the tab silently stays shut.
        let plane = tempfile::tempdir().unwrap();
        let vault = |key: &str| View {
            view: "vault".into(),
            title: key.into(),
            active: false,
            ..persona_view(key)
        };
        let record = Record {
            chats: Vec::new(),
            views: vec![vault("ops"), vault("e2e-vault"), vault("team.prod_2")],
            dealt: 0,
            relaunch_after_update: false,
            clone_seat: None,
            focus: None,
        };

        write(plane.path(), &record).expect("the record is written");

        assert_eq!(read(plane.path()), record);
    }

    #[test]
    fn a_view_tab_comes_back_as_it_was_recorded() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![claude("ide.7", Some(ID))],
            views: vec![
                persona_view("steward"),
                View {
                    from: Some("persona-statistics".into()),
                    view: "statistics".into(),
                    key: String::new(),
                    title: "Statistics".into(),
                    workspace: None,
                    at: 2,
                    active: false,
                    pinned: true,
                    split: None,
                },
            ],
            dealt: 0,
            relaunch_after_update: false,
            clone_seat: None,
            focus: None,
        };

        write(plane.path(), &record).expect("the record is written");

        assert_eq!(read(plane.path()), record);
    }

    #[test]
    fn a_record_written_before_tabs_held_views_reads_as_no_view_tabs() {
        // Every operator's record at the first launch after this change. The chats in it must
        // still come back: a version bump here would have dropped them all.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"chats":[{"program":"claude","name":"ide.7"}]}"#,
        )
        .unwrap();

        let back = read(plane.path());

        assert_eq!(back.chats.len(), 1);
        assert!(back.views.is_empty());
    }

    #[test]
    fn a_plane_with_no_view_tabs_writes_no_views_key() {
        // So the record of a plane that never opened a view is byte for byte the record an
        // older charter wrote and reads.
        let plane = tempfile::tempdir().unwrap();
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", None)],
                ..Default::default()
            },
        )
        .unwrap();

        let text = fs::read_to_string(path(plane.path())).unwrap();

        assert!(!text.contains("\"views\""), "{text}");
    }

    #[test]
    fn a_view_line_that_is_not_one_charter_would_write_is_dropped_and_the_rest_kept() {
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"chats":[],"views":[
                {"view":"persona","key":"../../etc"},
                {"from":"a/b","view":"statistics"},
                {"view":""},
                {"view":"persona","key":"steward","workspace":"no/such"}
            ]}"#,
        )
        .unwrap();

        let back = read(plane.path());

        assert_eq!(back.views.len(), 1, "{:?}", back.views);
        assert_eq!(back.views[0].key, "steward");
        assert_eq!(
            back.views[0].workspace, None,
            "a name that is not a workspace files the tab outside every workspace"
        );
    }

    #[test]
    fn a_branch_s_file_tab_comes_back_with_its_split() {
        // FM-2 (#1105): the file tab is keyed by its branch, `workspace/repo/piece`, or
        // `workspace/repo/` for the repo's own folder, and remembers where its divider was.
        let plane = tempfile::tempdir().unwrap();
        let files = |key: &str, split: Option<u8>| View {
            view: "piece-files".into(),
            title: "Files · fix-login".into(),
            active: false,
            split,
            ..persona_view(key)
        };
        let record = Record {
            views: vec![
                files("alpha/svc/fix-login", Some(30)),
                files("alpha/svc/", None),
            ],
            ..Default::default()
        };

        write(plane.path(), &record).expect("the record is written");

        assert_eq!(read(plane.path()), record);
    }

    #[test]
    fn the_branch_a_window_focused_comes_back_with_its_views() {
        // FM-5 (#1108): the cockpit's branch is remembered with the window's saved views, so
        // one window can stay on one branch across a relaunch.
        let plane = tempfile::tempdir().unwrap();
        for piece in [Some("fix-login".to_string()), None] {
            let record = Record {
                views: vec![persona_view("steward")],
                focus: Some(Focus {
                    workspace: "alpha".into(),
                    repo: "svc".into(),
                    piece: piece.clone(),
                }),
                ..Default::default()
            };

            write(plane.path(), &record).expect("the record is written");

            assert_eq!(read(plane.path()), record, "{piece:?}");
        }
    }

    #[test]
    fn a_focus_that_names_no_branch_is_forgotten_and_the_rest_comes_back() {
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        for focus in [
            r#"{"workspace":"alpha","repo":"../etc","piece":"fix"}"#,
            r#"{"workspace":"","repo":"svc","piece":"fix"}"#,
            r#"{"workspace":"alpha","repo":"svc","piece":"a/b"}"#,
            format!(
                r#"{{"workspace":"{}","repo":"svc","piece":"fix"}}"#,
                "w".repeat(65)
            )
            .as_str(),
            r#"{"workspace":"alpha"}"#,
            r#""alpha/svc/fix""#,
        ] {
            std::fs::write(
                path(plane.path()),
                format!(
                    r#"{{"version":1,"at":0,"chats":[],"views":[{{"view":"persona","key":"steward"}}],"focus":{focus}}}"#
                ),
            )
            .unwrap();

            let back = read(plane.path());

            assert_eq!(back.focus, None, "{focus}");
            assert_eq!(back.views.len(), 1, "{focus}");
        }
    }

    #[test]
    fn a_focus_alone_is_nothing_a_launch_asks_about() {
        let record = Record {
            focus: Some(Focus {
                workspace: "alpha".into(),
                repo: "svc".into(),
                piece: None,
            }),
            ..Default::default()
        };
        assert!(!record.holds_anything());
    }

    #[test]
    fn a_file_tab_line_that_names_no_branch_is_dropped_and_a_wild_split_is_forgotten() {
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"chats":[],"views":[
                {"view":"piece-files","key":"alpha/../etc"},
                {"view":"piece-files","key":"alpha/svc/fix/extra"},
                {"view":"piece-files","key":"/svc/fix"},
                {"view":"persona","key":"alpha/svc/fix"},
                {"view":"piece-files","key":"alpha/svc/fix","split":250},
                {"view":"piece-files","key":"alpha/svc/","split":0},
                {"view":"piece-files","key":"alpha/svc/","split":-3.5}
            ]}"#,
        )
        .unwrap();

        let back = read(plane.path());

        let kept: Vec<_> = back
            .views
            .iter()
            .map(|v| (v.key.as_str(), v.split))
            .collect();
        assert_eq!(
            kept,
            [
                ("alpha/svc/fix", None),
                ("alpha/svc/", None),
                ("alpha/svc/", None)
            ]
        );
    }

    /// A view tab of charter's own, keyed `key`, as the window would have recorded it.
    fn own_view(view: &str, key: &str) -> View {
        View {
            view: view.into(),
            title: "A tab".into(),
            active: false,
            ..persona_view(key)
        }
    }

    /// Every key shape the window writes for a view keyed by a path of names (#1297), one per
    /// line, each as its writer spells it.
    const HELD_SHAPES: &[(&str, &str)] = &[
        ("todo", "alpha/20261006-091500-ship-the-fix"),
        ("todo", "alpha/ship-the-fix.md"),
        ("memory", "workspace/alpha/20261006-091500-a-lesson"),
        ("memory", "persona/steward/a-lesson"),
        ("memory", "persona/_shared/a-lesson"),
        ("memory", "shared/a-lesson"),
        ("memory", "workspace/alpha/\\"),
        ("memory", "persona/steward/\\"),
        ("memory", "shared/\\"),
        ("memory-archive", "workspace/alpha"),
        ("memory-archive", "persona/steward"),
        ("memory-archive", "shared"),
        ("piece-files", "alpha/svc/fix-login"),
        ("piece-files", "alpha/svc/"),
        ("piece-file", "alpha/svc/fix-login/src/main.rs"),
        ("piece-file", "alpha/svc//README.md"),
        ("piece-file", "alpha/svc/fix-login/.github/workflows/ci.yml"),
        ("piece-diff", "alpha/svc/fix-login/src/main.rs"),
        ("piece-diff", "alpha/svc//Cargo.toml"),
        (
            "session",
            "workspaces/alpha/sessions/20260928-140312-ship-it.md",
        ),
        ("session", "sessions/20260928-140312-ship-it.md"),
    ];

    #[test]
    fn todo_memory_session_and_file_tabs_come_back_after_a_relaunch() {
        // #1297: each of these was dropped from the record at every launch.
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            views: HELD_SHAPES
                .iter()
                .map(|(view, key)| own_view(view, key))
                .collect(),
            ..Default::default()
        };

        write(plane.path(), &record).expect("the record is written");

        let back: Vec<_> = read(plane.path())
            .views
            .iter()
            .map(|v| (v.view.clone(), v.key.clone()))
            .collect();
        let wrote: Vec<_> = HELD_SHAPES
            .iter()
            .map(|(view, key)| (view.to_string(), key.to_string()))
            .collect();
        assert_eq!(back, wrote);
    }

    #[test]
    fn a_path_shaped_key_that_is_not_its_view_s_own_shape_is_dropped() {
        // #1297: `reopen.json` is writable by whoever can write the state folder, so each view
        // holds its key to the one shape its writer spells, and nothing else comes back.
        let refused: &[(&str, &str)] = &[
            // todo: `<ws>/<slug>`
            ("todo", ""),
            ("todo", "alpha"),
            ("todo", "alpha/"),
            ("todo", "/ship"),
            ("todo", "alpha/../ship"),
            ("todo", "alpha/.."),
            ("todo", "../alpha/ship"),
            ("todo", "/etc/passwd"),
            ("todo", "alpha/ship/more"),
            ("todo", "alpha//ship"),
            ("todo", ".hidden/ship"),
            ("todo", "alpha/a\\b"),
            ("todo", "alpha/\\"),
            ("todo", "alpha/MEMORY"),
            ("todo", "alpha/sh\nip"),
            ("todo", "alpha/sh\0ip"),
            ("todo", "C:/ship"),
            // memory: `workspace|persona/<name>/<slug>`, `shared/<slug>`
            ("memory", ""),
            ("memory", "steward"),
            ("memory", "workspace/alpha"),
            ("memory", "workspace/alpha/"),
            ("memory", "workspace//lesson"),
            ("memory", "workspace/../lesson"),
            ("memory", "workspace/alpha/.."),
            ("memory", "workspace/alpha/lesson/more"),
            ("memory", "persona/Steward/lesson"),
            ("memory", "persona/../lesson"),
            ("memory", "persona/steward/a\\b"),
            ("memory", "persona/steward/\\\\"),
            ("memory", "shared/.."),
            ("memory", "shared/"),
            ("memory", "shared/a/b"),
            ("memory", "workspaces/alpha/lesson"),
            ("memory", "/workspace/alpha/lesson"),
            ("memory", "workspace/alpha/MEMORY.md"),
            // memory-archive: `workspace|persona/<name>`, `shared`
            ("memory-archive", ""),
            ("memory-archive", "alpha"),
            ("memory-archive", "workspace/"),
            ("memory-archive", "workspace/.."),
            ("memory-archive", "workspace/alpha/more"),
            ("memory-archive", "persona/Steward"),
            ("memory-archive", "shared/x"),
            ("memory-archive", "/shared"),
            // piece-files: `<ws>/<repo>/<piece>` or `<ws>/<repo>/`
            ("piece-files", "alpha"),
            ("piece-files", "alpha/../etc"),
            ("piece-files", "alpha/svc/fix/extra"),
            ("piece-files", "/svc/fix"),
            // piece-file and piece-diff: a branch, then a plain relative path
            ("piece-file", "alpha/svc/fix"),
            ("piece-file", "alpha/svc/fix/"),
            ("piece-file", "alpha/svc/fix/src//main.rs"),
            ("piece-file", "alpha/svc/fix/../../../etc/passwd"),
            ("piece-file", "alpha/svc/fix/src/../../x"),
            ("piece-file", "alpha/svc/fix/./src"),
            ("piece-file", "alpha/svc/fix//etc/passwd"),
            ("piece-file", "alpha/svc/fix/src\\..\\x"),
            ("piece-file", "alpha/svc/fix/C:x"),
            ("piece-file", "alpha/svc/fix/a\nb"),
            ("piece-file", "../svc/fix/src/main.rs"),
            ("piece-file", "alpha/../fix/src/main.rs"),
            ("piece-file", "alpha/svc/../src/main.rs"),
            ("piece-file", "alpha"),
            ("piece-diff", "alpha/svc/fix"),
            ("piece-diff", "alpha/svc/fix/../../x"),
            ("piece-diff", "/alpha/svc/fix/src/main.rs"),
            ("piece-diff", "alpha/svc/fix/src/"),
            // session: `workspaces/<ws>/sessions/<file>` or `sessions/<file>`
            ("session", ""),
            ("session", "20260928-140312-ship-it.md"),
            ("session", "/etc/passwd"),
            ("session", "sessions/index.md"),
            ("session", "sessions/../charter.toml"),
            (
                "session",
                "workspaces/../sessions/20260928-140312-ship-it.md",
            ),
            (
                "session",
                "workspaces/alpha/sessions/../../../20260928-140312-ship-it.md",
            ),
            (
                "session",
                "workspaces/alpha/sessions/sub/20260928-140312-ship-it.md",
            ),
            (
                "session",
                "workspaces/alpha/memory/20260928-140312-ship-it.md",
            ),
            (
                "session",
                "workspaces/.hidden/sessions/20260928-140312-ship-it.md",
            ),
            ("session", "sessions\\..\\20260928-140312-ship-it.md"),
            ("session", "/sessions/20260928-140312-ship-it.md"),
            ("session", "./sessions/20260928-140312-ship-it.md"),
            ("session", "alpha"),
        ];
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        for (view, key) in refused {
            let line = serde_json::json!({
                "version": 1, "at": 0, "chats": [],
                "views": [{"view": view, "key": key}, {"view": "persona", "key": "steward"}],
            });
            std::fs::write(path(plane.path()), line.to_string()).unwrap();

            let back = read(plane.path());

            let kept: Vec<_> = back.views.iter().map(|v| v.view.as_str()).collect();
            assert_eq!(kept, ["persona"], "{view} {key:?} was taken");
        }
    }

    #[test]
    fn a_key_longer_than_any_charter_writes_is_dropped() {
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        let deep = format!("alpha/svc/fix/{}", "d/".repeat(MOST_KEY / 2) + "f");
        let line = serde_json::json!({
            "version": 1, "at": 0, "chats": [],
            "views": [{"view": "piece-file", "key": deep}],
        });
        std::fs::write(path(plane.path()), line.to_string()).unwrap();

        assert!(read(plane.path()).views.is_empty());
    }

    #[test]
    fn an_extension_s_view_is_not_held_to_charter_s_shape_for_a_view_of_the_same_name() {
        // An extension's `todo` is its own view: its key is one word, as every extension's is,
        // and a path is still not one.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        let line = serde_json::json!({
            "version": 1, "at": 0, "chats": [],
            "views": [
                {"from": "acme", "view": "todo", "key": "steward"},
                {"from": "acme", "view": "todo", "key": "alpha/ship"},
            ],
        });
        std::fs::write(path(plane.path()), line.to_string()).unwrap();

        let back = read(plane.path());

        let kept: Vec<_> = back.views.iter().map(|v| v.key.as_str()).collect();
        assert_eq!(kept, ["steward"]);
    }

    #[test]
    fn a_view_title_that_is_not_one_line_of_words_is_the_view_s_own_id() {
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        let long = "x".repeat(MOST_TITLE + 1);
        std::fs::write(
            path(plane.path()),
            format!(
                r#"{{"version":1,"at":0,"chats":[],"views":[
                    {{"view":"one","title":"two\nlines"}},
                    {{"view":"two","title":"{long}"}},
                    {{"view":"three","title":"  "}},
                    {{"view":"four","title":"Statistics"}}
                ]}}"#
            ),
        )
        .unwrap();

        let titles: Vec<String> = read(plane.path())
            .views
            .into_iter()
            .map(|view| view.title)
            .collect();

        assert_eq!(titles, vec!["one", "two", "three", "Statistics"]);
    }

    #[test]
    fn a_record_puts_back_no_more_view_tabs_than_the_bound() {
        let plane = tempfile::tempdir().unwrap();
        let many: Vec<View> = (0..MOST_VIEWS + 5)
            .map(|at| View {
                key: format!("p{at}"),
                active: false,
                ..persona_view("x")
            })
            .collect();
        write(
            plane.path(),
            &Record {
                views: many,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(read(plane.path()).views.len(), MOST_VIEWS);
    }

    #[test]
    fn a_record_of_exactly_the_largest_size_is_still_read() {
        // The bound is "never LARGER than", as `charter/contain.py`'s `st.st_size > MAX_BYTES`
        // draws it: a record of exactly the cap is an honest one and is read.
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().to_path_buf();
        write(&plane, &one_chat()).unwrap();
        let mut text = std::fs::read(path(&plane)).unwrap();
        text.resize(usize::try_from(MAX_BYTES).unwrap(), b' ');
        std::fs::write(path(&plane), &text).unwrap();

        assert_eq!(read_or_refusal(&plane).unwrap(), one_chat());
    }

    // ----- the choice at a relaunch (charter-app#250) -----

    fn a_record_with_a_chat_and_a_view() -> Record {
        Record {
            chats: vec![Chat {
                number: Some(3),
                ..claude("ide.7", Some(ID))
            }],
            views: vec![persona_view("steward")],
            dealt: 5,
            ..Default::default()
        }
    }

    #[test]
    fn a_record_with_no_chat_and_no_view_tab_has_nothing_to_ask_about() {
        // A first launch, and a plane whose last quit had nothing open: no question.
        assert!(!Record::default().holds_anything());
        assert!(
            !Record {
                dealt: 9,
                ..Default::default()
            }
            .holds_anything(),
            "a counter is bookkeeping, not something that was open"
        );
    }

    #[test]
    fn a_chat_or_a_view_tab_alone_is_something_to_ask_about() {
        let chat = Record {
            chats: vec![claude("ide.7", None)],
            ..Default::default()
        };
        let view = Record {
            views: vec![persona_view("steward")],
            ..Default::default()
        };

        assert!(chat.holds_anything());
        assert!(view.holds_anything());
    }

    #[test]
    fn reopen_all_puts_back_the_record_exactly_as_it_was() {
        let record = a_record_with_a_chat_and_a_view();

        assert_eq!(record.clone().chosen(Choice::ReopenAll), record);
    }

    #[test]
    fn start_fresh_puts_back_no_chat_and_no_view_tab() {
        let fresh = a_record_with_a_chat_and_a_view().chosen(Choice::StartFresh);

        assert!(fresh.chats.is_empty());
        assert!(fresh.views.is_empty());
        assert!(!fresh.holds_anything());
    }

    #[test]
    fn start_fresh_keeps_every_number_already_dealt_so_none_is_dealt_twice() {
        // charter-app#90: a number is never dealt twice. Chat 3's workspace pointer and lock
        // are still on disk, so the first chat after a fresh start must not be 3 again —
        // nor 4 or 5, which the counter says were dealt to chats closed before the quit.
        let fresh = a_record_with_a_chat_and_a_view().chosen(Choice::StartFresh);

        assert_eq!(fresh.dealt, 5);

        let only_the_chat_knew = Record {
            chats: vec![Chat {
                number: Some(7),
                ..claude("ide.7", None)
            }],
            dealt: 2,
            ..Default::default()
        }
        .chosen(Choice::StartFresh);
        assert_eq!(only_the_chat_knew.dealt, 7);
    }

    #[test]
    fn a_fresh_start_written_and_read_back_is_still_nothing_to_reopen() {
        let plane = tempfile::tempdir().unwrap();

        write(
            plane.path(),
            &a_record_with_a_chat_and_a_view().chosen(Choice::StartFresh),
        )
        .unwrap();
        let back = read(plane.path());

        assert!(!back.holds_anything());
        assert_eq!(back.dealt, 5);
    }

    #[test]
    fn a_relaunch_after_an_update_is_said_in_the_record_and_read_back() {
        // charter-app#251's way in: the quit that installs an update writes this, and the
        // launch after it knows why it is being asked.
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            relaunch_after_update: true,
            ..a_record_with_a_chat_and_a_view()
        };

        write(plane.path(), &record).unwrap();

        assert_eq!(read(plane.path()), record);
    }

    #[test]
    fn an_ordinary_quit_writes_no_update_key_and_an_absent_one_reads_as_an_ordinary_quit() {
        let plane = tempfile::tempdir().unwrap();
        write(plane.path(), &a_record_with_a_chat_and_a_view()).unwrap();

        let text = fs::read_to_string(path(plane.path())).unwrap();

        assert!(!text.contains("relaunch_after_update"), "{text}");
        assert!(!read(plane.path()).relaunch_after_update);
    }

    #[test]
    fn a_fresh_start_is_not_a_relaunch_after_an_update_any_more() {
        // The flag is about the launch that reads it, and no later one.
        let fresh = Record {
            relaunch_after_update: true,
            ..a_record_with_a_chat_and_a_view()
        }
        .chosen(Choice::StartFresh);

        assert!(!fresh.relaunch_after_update);
    }

    // ----- the name the operator gave a chat (charter-app#254) ----------------------------

    #[test]
    fn a_chat_comes_back_under_the_name_the_operator_gave_it() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![Chat {
                label: Some("billing bug".into()),
                ..claude("3", None)
            }],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        assert_eq!(
            read(plane.path()).chats[0].label.as_deref(),
            Some("billing bug")
        );
    }

    #[test]
    fn a_record_written_before_chats_had_names_reads_as_the_default_one() {
        // Not a format change, for `Chat::pinned`'s reason: a missing key reads as "no name
        // given", which is what was true of every record written before a chat could have one.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            br#"{"version":1,"at":0,"chats":[{"program":"claude","name":"3"}]}"#,
        )
        .unwrap();

        let back = read(plane.path());

        assert_eq!(back.chats.len(), 1);
        assert_eq!(back.chats[0].label, None);
    }

    #[test]
    fn a_chat_with_no_name_given_writes_no_label_key() {
        // So the record a plane that never renamed a chat writes is the one it always wrote.
        let plane = tempfile::tempdir().unwrap();
        write(plane.path(), &one_chat()).unwrap();

        let text = std::fs::read_to_string(path(plane.path())).unwrap();

        assert!(!text.contains("\"label\""), "{text}");
    }

    #[test]
    fn a_label_off_disk_charter_would_refuse_reads_as_no_name_given() {
        // The file is writable by whoever can write the plane's state directory, and the label
        // is drawn on a tab: a line that would draw as something else keeps its chat and loses
        // only the name.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        std::fs::write(
            path(plane.path()),
            "{\"version\":1,\"at\":0,\"chats\":[{\"program\":\"claude\",\"name\":\"3\",\
             \"label\":\"pay\u{202e}lanigiro\"}]}",
        )
        .unwrap();

        let back = read(plane.path());

        assert_eq!(back.chats.len(), 1);
        assert_eq!(back.chats[0].label, None);
    }

    #[test]
    fn a_label_is_trimmed() {
        assert_eq!(label("  billing bug \t"), Ok(Some("billing bug".into())));
    }

    #[test]
    fn a_blank_label_is_no_name_given() {
        assert_eq!(label(""), Ok(None));
        assert_eq!(label("   "), Ok(None));
    }

    #[test]
    fn a_label_of_the_longest_length_is_kept_and_one_longer_is_refused() {
        let longest = "é".repeat(MOST_LABEL);
        assert_eq!(label(&longest), Ok(Some(longest.clone())));

        let refused = label(&format!("{longest}x")).unwrap_err();
        assert!(refused.contains(&MOST_LABEL.to_string()), "{refused}");
    }

    #[test]
    fn a_label_with_a_control_character_is_refused() {
        assert!(label("two\nlines").is_err());
        assert!(label("bell\u{7}").is_err());
    }

    #[test]
    fn a_label_with_an_invisible_character_is_refused() {
        // Each makes two different names look like one on the strip, or turns the words
        // around it backwards.
        for sneaky in ["pay\u{202e}lanigiro", "a\u{200b}b", "\u{feff}steward"] {
            assert!(label(sneaky).is_err(), "{sneaky:?} was let through");
        }
    }

    #[test]
    fn a_space_inside_a_label_is_content() {
        assert_eq!(label("steward 1"), Ok(Some("steward 1".into())));
    }

    // ----- where a handed-off chat came from (charter-app#258, #259) -----------------------

    fn handed() -> HandedFrom {
        HandedFrom {
            chat: 16,
            name: "steward 3".into(),
            workspace: crate::active::Place::Workspace("platform-next".into()),
            report: Owed::Due,
            mode: Mode::Handoff,
            depth: 0,
            root: None,
            above: None,
            by_person: false,
        }
    }

    // ----- a task chat is listed without a tab until the person opens it (#1447) ----------

    #[test]
    fn a_task_chat_comes_back_a_task_with_no_tab_until_the_person_opened_it() {
        let plane = tempfile::tempdir().unwrap();
        let task = HandedFrom {
            mode: Mode::Task,
            ..handed()
        };
        let listed = Chat {
            from: Some(task.clone()),
            ..claude("3", None)
        };
        let opened = Chat {
            tab_opened: true,
            ..listed.clone()
        };
        let record = Record {
            chats: vec![listed, opened],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let back = read(plane.path()).chats;
        assert_eq!(back[0].from, Some(task));
        assert!(!back[0].has_tab(), "listed, and on no strip");
        assert!(back[1].has_tab(), "the tab the person opened comes back");
    }

    #[test]
    fn what_a_sessions_tab_shows_comes_back_and_a_tab_on_its_own_chat_writes_nothing_of_it() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![
                Chat {
                    number: Some(4),
                    shows: Some(9),
                    ..claude("4", None)
                },
                Chat {
                    number: Some(5),
                    ..claude("5", None)
                },
            ],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches(r#""shows""#).count(), 1, "{text}");
        let back = read(plane.path()).chats;
        assert_eq!(back[0].shows, Some(9));
        assert_eq!(back[1].shows, None);
    }

    #[test]
    fn the_tab_a_chat_has_a_pane_in_comes_back_and_a_chat_in_no_split_writes_nothing_of_it() {
        // #1489: a task opened beside its session is put back beside it, and the core reads
        // the same number for which chats are on screen with the one in front.
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![
                Chat {
                    number: Some(4),
                    ..claude("4", None)
                },
                Chat {
                    number: Some(9),
                    beside: Some(4),
                    ..claude("9", None)
                },
            ],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches(r#""beside""#).count(), 1, "{text}");
        let back = read(plane.path()).chats;
        assert_eq!(back[0].beside, None);
        assert_eq!(back[1].beside, Some(4));
    }

    #[test]
    fn a_handoff_and_a_chat_the_person_opened_have_a_tab_and_write_the_record_they_always_wrote() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![
                Chat {
                    from: Some(handed()),
                    ..claude("3", None)
                },
                claude("4", None),
            ],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert!(
            !text.contains(r#""mode""#) && !text.contains("tab_opened"),
            "{text}"
        );
        let back = read(plane.path()).chats;
        assert_eq!(
            back[0].from.as_ref().map(|from| from.mode),
            Some(Mode::Handoff)
        );
        assert!(back[0].has_tab() && back[1].has_tab());
    }

    #[test]
    fn a_mode_nobody_wrote_reads_as_a_handoff() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![Chat {
                from: Some(HandedFrom {
                    mode: Mode::Task,
                    ..handed()
                }),
                ..claude("3", None)
            }],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();
        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert!(text.contains(r#""mode": "task""#), "{text}");
        std::fs::write(
            path(plane.path()),
            text.replace(r#""mode": "task""#, r#""mode": "hidden""#),
        )
        .unwrap();

        // Fails open to a tab: a chat is never hidden by a word purlis did not write.
        assert!(read(plane.path()).chats[0].has_tab());
    }

    #[test]
    fn a_task_comes_back_a_task_at_its_depth_and_a_handoff_writes_neither_key() {
        // #1436: the lineage a dispatch recorded outlives a relaunch, which is what keeps a
        // task listed under the chat that asked and its depth counted.
        let plane = tempfile::tempdir().unwrap();
        let task = HandedFrom {
            mode: crate::dispatchdecision::Mode::Task,
            depth: 2,
            above: None,
            by_person: false,
            ..handed()
        };
        let record = Record {
            chats: vec![
                Chat {
                    from: Some(task.clone()),
                    ..claude("3", None)
                },
                Chat {
                    from: Some(handed()),
                    ..claude("4", None)
                },
            ],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let back = read(plane.path());
        assert_eq!(back.chats[0].from, Some(task));
        assert_eq!(back.chats[1].from, Some(handed()));
        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches("\"mode\"").count(), 1, "{text}");
        assert_eq!(text.matches("\"depth\"").count(), 1, "{text}");
    }

    #[test]
    fn a_lineage_s_root_comes_back_with_the_chat_and_a_record_without_one_writes_no_key() {
        // The root is what keeps a lineage one lineage when a chat in the middle closes: it
        // has to outlive a relaunch, as the depth does.
        let plane = tempfile::tempdir().unwrap();
        let root = "01J9ZQ3V5N8X4T2K7M6P0R1S2A";
        let below = HandedFrom {
            mode: Mode::Task,
            depth: 1,
            root: Some(root.to_owned()),
            ..handed()
        };
        let record = Record {
            chats: vec![
                Chat {
                    from: Some(below.clone()),
                    ..claude("3", None)
                },
                Chat {
                    from: Some(handed()),
                    ..claude("4", None)
                },
            ],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let back = read(plane.path());
        assert_eq!(back.chats[0].from, Some(below));
        assert_eq!(back.chats[1].from, Some(handed()));
        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches("\"root\"").count(), 1, "{text}");
    }

    #[test]
    fn a_root_that_is_not_an_id_purlis_mints_names_no_lineage() {
        // The file is writable by anything running as the person. A root is compared, never
        // drawn or run, and one in no shape an id has is dropped: the chat is then counted by
        // who dispatched whom, as a record written before roots were kept is.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(path(plane.path()).parent().unwrap()).unwrap();
        for (written, read_as) in [
            ("../../etc", None),
            ("", None),
            (
                "01j9zq3v5n8x4t2k7m6p0r1s2a",
                Some("01J9ZQ3V5N8X4T2K7M6P0R1S2A"),
            ),
        ] {
            let from = format!(
                r#"{{"chat":16,"name":"steward 3","workspace":"ops","mode":"task","root":"{written}"}}"#
            );
            std::fs::write(
                path(plane.path()),
                format!(
                    r#"{{"version":1,"at":0,"chats":[{{"program":"claude","name":"3","from":{from}}}]}}"#
                ),
            )
            .unwrap();
            let back = read(plane.path()).chats[0].from.clone().expect("it reads");
            assert_eq!(back.root.as_deref(), read_as, "{written}");
        }
    }

    #[test]
    fn the_personas_above_a_task_come_back_with_it_and_an_older_record_writes_no_key() {
        // #1521: the chain the loop rule reads outlives a relaunch, as the depth does, and a
        // chat on no persona above it comes back as one.
        let plane = tempfile::tempdir().unwrap();
        let below = HandedFrom {
            mode: Mode::Task,
            depth: 2,
            above: Some(vec![Some("devops".to_owned()), None]),
            ..handed()
        };
        let record = Record {
            chats: vec![
                Chat {
                    from: Some(below.clone()),
                    ..claude("3", None)
                },
                Chat {
                    from: Some(handed()),
                    ..claude("4", None)
                },
            ],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let back = read(plane.path());
        assert_eq!(back.chats[0].from, Some(below));
        assert_eq!(back.chats[1].from, Some(handed()));
        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches("\"above\"").count(), 1, "{text}");
        let squeezed: String = text.split_whitespace().collect();
        assert!(squeezed.contains(r#""above":["devops",""]"#), "{text}");
    }

    #[test]
    fn personas_above_that_the_app_would_not_have_written_read_as_a_chain_not_kept() {
        // The record is the app's. One whose chain is not one per dispatch above, or names
        // what cannot be a persona, is read as kept by no one: the loop rule then reads it as a
        // chain it cannot read whole, never as a shorter one.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(path(plane.path()).parent().unwrap()).unwrap();
        let steward = || Some(vec![Some("steward".to_owned())]);
        for (depth, above, read_as) in [
            (1, r#"["steward"]"#, steward()),
            (1, r#"[""]"#, Some(vec![None])),
            (0, "[]", Some(Vec::new())),
            // Shorter or longer than the depth says.
            (2, r#"["steward"]"#, None),
            (1, "[]", None),
            (1, r#"["steward","devops"]"#, None),
            // Not a persona's name.
            (1, r#"["../steward"]"#, None),
            (1, r#"["Steward"]"#, None),
        ] {
            let from = format!(
                r#"{{"chat":16,"name":"steward 3","workspace":"ops","mode":"task","depth":{depth},"above":{above}}}"#
            );
            std::fs::write(
                path(plane.path()),
                format!(
                    r#"{{"version":1,"at":0,"chats":[{{"program":"claude","name":"3","from":{from}}}]}}"#
                ),
            )
            .unwrap();
            let back = read(plane.path()).chats[0].from.clone().expect("it reads");
            assert_eq!(back.above, read_as, "{depth} {above}");
        }
        // A record written before the key: no chain kept.
        std::fs::write(
            path(plane.path()),
            r#"{"version":1,"at":0,"chats":[{"program":"claude","name":"3","from":{"chat":16,"name":"steward 3","workspace":"ops","mode":"task","depth":1}}]}"#,
        )
        .unwrap();
        let back = read(plane.path()).chats[0].from.clone().expect("it reads");
        assert_eq!(back.above, None);
    }

    #[test]
    fn a_chat_the_person_started_from_a_tab_comes_back_saying_so_and_no_other_writes_the_key() {
        // #1438: who a report is marked as started by outlives a relaunch.
        let plane = tempfile::tempdir().unwrap();
        let asked = HandedFrom {
            mode: crate::dispatchdecision::Mode::Task,
            depth: 1,
            above: None,
            by_person: true,
            ..handed()
        };
        let record = Record {
            chats: vec![
                Chat {
                    from: Some(asked.clone()),
                    ..claude("3", None)
                },
                Chat {
                    from: Some(handed()),
                    ..claude("4", None)
                },
            ],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let back = read(plane.path());
        assert_eq!(back.chats[0].from, Some(asked));
        assert_eq!(back.chats[1].from, Some(handed()));
        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert_eq!(text.matches("\"by\"").count(), 1, "{text}");
    }

    #[test]
    fn a_depth_no_chain_can_have_reads_as_the_deepest_one_can() {
        // The file is writable by anything running as the person: a depth past the ceiling
        // is held to it, so a record never says a chain is shallower than it could be.
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        let from =
            r#"{"chat":16,"name":"steward 3","workspace":"ops","mode":"task","depth":4000000000}"#;
        std::fs::write(
            path(plane.path()),
            format!(
                r#"{{"version":1,"at":0,"chats":[{{"program":"claude","name":"3","from":{from}}}]}}"#
            ),
        )
        .unwrap();

        let back = read(plane.path()).chats[0].from.clone().expect("it reads");
        assert_eq!(back.depth, crate::dispatchdecision::DEEPEST);
        assert_eq!(back.mode, crate::dispatchdecision::Mode::Task);
    }

    #[test]
    fn a_handed_off_chat_comes_back_knowing_where_it_came_from_and_what_it_owes() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![Chat {
                from: Some(handed()),
                ..claude("3", None)
            }],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        assert_eq!(read(plane.path()).chats[0].from, Some(handed()));
    }

    #[test]
    fn a_task_the_app_reported_failed_for_comes_back_so_and_is_never_owed_again() {
        // #1443: a relaunch does not make a chat that ended without a report owe one again.
        let plane = tempfile::tempdir().unwrap();
        let failed = HandedFrom {
            report: Owed::Failed,
            mode: crate::dispatchdecision::Mode::Task,
            depth: 1,
            ..handed()
        };
        let record = Record {
            chats: vec![Chat {
                from: Some(failed.clone()),
                ..claude("3", None)
            }],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        assert_eq!(read(plane.path()).chats[0].from, Some(failed));
    }

    #[test]
    fn a_report_already_sent_comes_back_sent() {
        let plane = tempfile::tempdir().unwrap();
        let sent = HandedFrom {
            report: Owed::Sent,
            ..handed()
        };
        let record = Record {
            chats: vec![Chat {
                from: Some(sent.clone()),
                ..claude("3", None)
            }],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        assert_eq!(read(plane.path()).chats[0].from, Some(sent));
    }

    /// #1362, D-1362-5/6: a chat holding another persona's grants keeps holding them across a
    /// relaunch, and the hold is read failing closed: a value that is not a persona's name holds
    /// no persona's grants, and a handoff note that does not read never drops it.
    #[test]
    fn a_chat_holding_another_persona_s_grants_comes_back_holding_them() {
        let plane = tempfile::tempdir().unwrap();
        let holding = |persona: Option<&str>| Chat {
            from: Some(handed()),
            held: Some(HeldGrants {
                persona: persona.map(str::to_owned),
            }),
            ..claude("3", None)
        };
        for persona in [Some("qa"), None] {
            let record = Record {
                chats: vec![holding(persona)],
                ..Default::default()
            };
            write(plane.path(), &record).unwrap();
            assert_eq!(
                read(plane.path()).chats[0].held,
                holding(persona).held,
                "{persona:?}"
            );
        }
        let held_none = Some(HeldGrants { persona: None });
        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        for (from, to) in [
            (r#""held": """#, r#""held": "../Not A Persona""#),
            (r#""held": """#, r#""held": 7"#),
            // The note around it unsound: the hold still reads.
            (r#""name": "steward 3""#, r#""name": """#),
        ] {
            std::fs::write(path(plane.path()), text.replace(from, to)).unwrap();
            assert_eq!(read(plane.path()).chats[0].held, held_none, "{to}");
        }
    }

    #[test]
    fn a_chat_handed_off_from_the_plane_root_comes_back_knowing_it() {
        // SI-1b: the root is no workspace, and reads back as the root rather than as nothing.
        let plane = tempfile::tempdir().unwrap();
        let from_root = HandedFrom {
            workspace: crate::active::Place::PlaneRoot,
            ..handed()
        };
        let record = Record {
            chats: vec![Chat {
                from: Some(from_root.clone()),
                ..claude("3", None)
            }],
            ..Default::default()
        };
        write(plane.path(), &record).unwrap();

        let text = std::fs::read_to_string(path(plane.path())).unwrap();
        assert!(text.contains(r#""workspace": "plane root""#), "{text}");
        assert_eq!(read(plane.path()).chats[0].from, Some(from_root));
    }

    #[test]
    fn a_chat_the_operator_opened_writes_no_from_key() {
        let plane = tempfile::tempdir().unwrap();
        write(plane.path(), &one_chat()).unwrap();

        let text = std::fs::read_to_string(path(plane.path())).unwrap();

        assert!(!text.contains("\"from\""), "{text}");
    }

    #[test]
    fn a_from_off_disk_charter_would_not_have_written_reads_as_no_handoff() {
        let plane = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
        for from in [
            r#"{"chat":0,"name":"steward 3","workspace":"ops","report":"owed"}"#,
            "{\"chat\":16,\"name\":\"pay\\u202elanigiro\",\"workspace\":\"ops\",\"report\":\"owed\"}",
            r#"{"chat":16,"name":"steward 3","workspace":"../up","report":"owed"}"#,
            r#"{"chat":16,"name":"steward 3","workspace":"","report":"owed"}"#,
            r#"{"chat":16,"name":"steward 3","workspace":"Plane Root","report":"owed"}"#,
        ] {
            std::fs::write(
                path(plane.path()),
                format!(
                    r#"{{"version":1,"at":0,"chats":[{{"program":"claude","name":"3","from":{from}}}]}}"#
                ),
            )
            .unwrap();

            let back = read(plane.path());

            assert_eq!(back.chats.len(), 1, "{from}");
            assert_eq!(back.chats[0].from, None, "{from}");
        }
    }

    #[test]
    fn a_chat_is_shown_under_its_given_name_else_its_persona_or_harness_and_its_own() {
        let steward = Chat {
            persona: Some("steward".into()),
            ..claude("3", None)
        };
        assert_eq!(shown_name(&steward, Some("claude")), "steward 3");
        assert_eq!(shown_name(&claude("4", None), Some("claude")), "claude 4");
        assert_eq!(shown_name(&claude("5", None), None), "5");
        let named = Chat {
            label: Some("billing bug".into()),
            ..steward
        };
        assert_eq!(shown_name(&named, Some("claude")), "billing bug");
    }

    #[test]
    fn a_restart_to_update_is_taken_by_the_next_launch_and_by_no_launch_after_it() {
        // charter-app#251's scope: a plane the launch after the restart did not open keeps its
        // flag on disk, and a launch a week later must not read it as "charter restarted".
        let config = tempfile::tempdir().unwrap();

        mark_restart_to_update(config.path()).unwrap();

        assert!(take_restart_to_update(config.path()), "the next launch");
        assert!(
            !take_restart_to_update(config.path()),
            "the launch after it"
        );
    }

    #[test]
    fn a_launch_no_restart_came_before_takes_nothing() {
        let config = tempfile::tempdir().unwrap();

        assert!(!take_restart_to_update(config.path()));
    }

    #[cfg(unix)]
    #[test]
    fn the_restart_marker_is_not_written_through_a_link() {
        let config = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let dir = crate::machine::dir(config.path());
        fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(
            elsewhere.path().join("planted"),
            dir.join(RESTARTED_TO_UPDATE),
        )
        .unwrap();

        assert!(mark_restart_to_update(config.path()).is_err());
        assert!(!elsewhere.path().join("planted").exists());
    }
}
