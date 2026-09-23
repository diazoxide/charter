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
//! run at every later launch — before any window, with nothing to click.
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

/// The one version of this file this app writes and reads. A record of any other version is
/// ignored whole, the way the frame's own manifest is: a format that changed means the
/// chats in it cannot be trusted to mean what they say.
pub const VERSION: u32 = 1;

/// Where the record lives, relative to a plane root.
pub const IN_PLANE: &str = ".charter/app/reopen.json";

/// The largest record charter will read, matching `contain.MAX_BYTES` on the Python side.
/// A real record is a few hundred bytes per chat; anything approaching this is not one.
pub const MAX_BYTES: u64 = 1_048_576;

/// One chat as it was: what it was running, where, and the conversation to bring back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    /// The program, as it was launched. A path or a bare name.
    pub program: String,
    /// Its arguments, without any charter added — those are decided again at the reopen,
    /// because a resume spells them differently from a start.
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// What the operator calls this chat, and what a harness that takes a name is given.
    pub name: String,
    /// The conversation to bring back, where the app knows one.
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
    /// Whether this chat draws charter's footer in its pane (charter ADR 0029).
    ///
    /// Recorded for the same reason `persona` is: it is a choice the operator made about
    /// THIS chat in the picker, and a relaunch that dropped it would silently blank a footer
    /// they had turned on. A record written before ADR 0029 has no such key, and `false` is
    /// both serde's default and the behaviour every such record was written under.
    pub show_footer: bool,
    /// Whether the operator pinned this chat (charter ADR 0039, stored per ADR 0040).
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
}

/// The most view tabs one record puts back. A window opens one per persona and per extension
/// view at most, and a plane with more personas than this is not one anybody reads tab by tab;
/// the bound exists so a hand-written record cannot make a launch draw ten thousand tabs.
pub const MOST_VIEWS: usize = 64;

/// The most a view tab's recorded title may be, in characters. A tab draws a word or two.
pub const MOST_TITLE: usize = 120;

/// One tab that held a **view** rather than a chat, as it was when the app last wrote this.
///
/// **A tab is a layout of panes, and a pane holds a session or a view** (charter ADR 0043, as
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
    /// What it is about inside that: a persona's name, or empty for the whole plane.
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
    /// Whether the operator pinned it (charter ADR 0039), for [`Chat::pinned`]'s reasons.
    pub pinned: bool,
}

/// Every chat that was open, and the numbers this plane has already spent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
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
    /// id. Codex reports its id only through a hook, inside its first turn.
    NoConversationRecorded,
    /// Its program is not a harness charter has measured a resume for — a shell, say.
    NoResumeForThisProgram,
    /// Its own arguments already name a session, so charter added none of its own. What
    /// happens then is between the operator and the harness.
    SessionNamedByTheOperator,
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
                (argv, chosen, Reopened::Fresh(Fresh::NoConversationRecorded))
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
    plane_root.join(IN_PLANE)
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
/// (charter ADR 0028). A `symlink_metadata` on the path and a `read_to_string` of the path
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
                "{} is not a plain file, and charter reads its record from nothing else",
                file.display()
            ),
        ));
    }
    // Read whole at launch, so a planted giant is a hang with nothing to click on.
    if found.len() > MAX_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, and charter's record is never larger than {MAX_BYTES}",
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
pub fn write(plane_root: &Path, record: &Record) -> std::io::Result<()> {
    // Belt as well as braces. A root gets here from [`crate::plane::resolve`], which a
    // fenced build has already held — but also from a caller that was handed one, and this
    // file is the exact thing charter-app#129 damaged. The place where the record is written
    // is worth guarding on its own account, whatever route the root took to reach it.
    crate::fence::hold(crate::fence::Act::Write, plane_root);
    let file = path(plane_root);
    no_link_on_the_way(plane_root, &file)?;
    let dir = file.parent().expect("the record's path has a directory");
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(&OnDisk::from(record))
        .expect("the record is plain data serde can always write");

    // Beside itself, then renamed over: a rename is atomic on every platform charter runs
    // on, so a launch reading this file sees the whole of one record or the whole of the
    // one before it. Quitting is when the app is most likely to be killed halfway.
    // The write lands HERE, so this is the path that has to be checked. Guarding only the
    // file renamed onto left a committed `reopen.json.writing -> outside` writing the whole
    // record out of the plane, with no race at all — the same "gate one level shallower than
    // the write" this guard exists to stop.
    //
    // `contain::create_no_link` and not `no_link_on_the_way` + `fs::write`: the walk answers
    // about a name and the write opens that name again, and a link planted in between put
    // the whole record outside the plane 7600 times per 20,000 writes when it was measured
    // (charter ADR 0028). The flag makes the kernel answer the last component at the instant
    // of the create instead.
    let beside = file.with_extension("json.writing");
    {
        use std::io::Write;
        let mut out = crate::contain::create_no_link(plane_root, &beside)?;
        out.write_all((text + "\n").as_bytes())?;
    }
    std::fs::rename(&beside, &file)
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
    // Reading is guarded as well as writing, and the reason is the whole of ADR 0035: this
    // file says what to RUN. A test that reads a real plane's record starts the operator's
    // programs — which is what the reproduction for charter-app#129 did, from a checkout
    // one directory inside a plane: "1 of 1 chats back", in a plane the run never made.
    crate::fence::hold(crate::fence::Act::Read, plane_root);
    let file = path(plane_root);
    // A record reached through a link is not this plane's record, and what it holds is a
    // command line this launch would run — so the walk AND the open both answer, and the
    // open's answer is the kernel's at the instant it happens (charter ADR 0028). Measured
    // before that flag: 1881 launches per 20,000 took a planted command line from outside
    // the plane.
    let mut open = match crate::contain::open_no_link(plane_root, &file) {
        Ok(open) => open,
        // No record is a first launch. A record that exists and cannot be read — no
        // permission, a failing disk — is a defect with a repair, and saying "nothing to
        // reopen" would send the operator looking in the wrong place.
        Err(gone) if gone.kind() == std::io::ErrorKind::NotFound => return Ok(Record::default()),
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
        return Ok(Record::default());
    };
    if on_disk.version != VERSION {
        return Ok(Record::default());
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
    };
    // Held to the same invariant on the way in as on the way out: a file whose counter sits
    // below a number it still names — hand-edited, or written by a charter that did not know
    // about numbers — would otherwise deal that number to a second chat, which is the defect
    // (charter-app#90). Raising it here costs a gap in the counting and nothing else.
    Ok(Record {
        dealt: highest_dealt(&record),
        ..record
    })
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
        // in. A persona's name passes; a path does not.
        if !self.key.is_empty() && !id_ok(&self.key) {
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
        })
    }
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
                })
                .collect(),
            dealt: highest_dealt(record),
            views: record.views.iter().map(ViewOnDisk::from).collect(),
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        std::os::unix::fs::symlink(outside.join("planted.json"), plane.join(IN_PLANE)).unwrap();

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
    fn a_link_at_the_path_the_write_actually_lands_on_is_refused() {
        // The record is written beside itself and renamed over, so the `.json.writing` path
        // is where the bytes land — guarding only the renamed-onto name left this open.
        let held = tempfile::tempdir().unwrap();
        let plane = held.path().join("plane");
        let outside = held.path().join("outside");
        std::fs::create_dir_all(plane.join(".charter/app")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let captured = outside.join("captured.json");
        std::os::unix::fs::symlink(&captured, plane.join(".charter/app/reopen.json.writing"))
            .unwrap();

        let refused = write(&plane, &one_chat());

        assert!(refused.is_err(), "the temp path was written through");
        assert!(
            !captured.exists(),
            "the record was written outside the plane"
        );
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

    // ----- a pinned chat (charter ADR 0039, stored per ADR 0040) -----

    #[test]
    fn a_pinned_chat_comes_back_pinned() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            views: Vec::new(),
            dealt: 0,
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
        // **This is why there is no version bump**, although charter ADR 0039 expected one:
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

    // ----- view tabs (charter ADR 0043, as amended: a pane holds a session or a view) -----

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
        }
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
                },
            ],
            dealt: 0,
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
}
