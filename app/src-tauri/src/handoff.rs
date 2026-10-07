//! The app's half of `charter handoff`: opening, in a window the operator is looking at, the
//! chat a chat of its own handed work to (charter-app#204).
//!
//! `charter handoff` does every check in front of the open. The operator's consent is the
//! harness's permission prompt in front of that exact command, and nothing here asks again:
//! the operator already answered with the brief on screen. The command then asks the app over
//! the hook socket, in two lines on one connection: a ticket, and the open that spends it.
//! `purlis_core::hookwire::OpenChat` argues what that ticket is worth and what it is not,
//! and it is not repeated here.
//!
//! What this module adds is **the questions only the app can answer**, and the one control
//! that module's argument rests on, which is visibility:
//!
//! - **The asking chat is one this app has open**, and the new chat starts on the **same
//!   profile**, read from the app's own record of that chat and never from the request. A
//!   handoff never changes harness (`charter/commands_handoff.py:_printed_command`), and
//!   taking the profile from the request would let the request choose one.
//! - **The first message carries the stamp** of a handoff from that chat
//!   (`purlis_core::handoff::is_stamped_from`), so the chat the operator finds on the strip
//!   always says where it came from.
//! - **It lands on a strip, and it does not take the operator's screen.** The window is told
//!   ([`ARRIVED`]) and draws a tab in the target workspace's strip without bringing it to the
//!   front and without raising the window. See [`Arrived`] for why.
//! - **It is named for its task** (charter-app#258): the `--name` the handoff carried, or the
//!   ordinary `<persona> <N>`. And it says where it came from by the parent's NAME — in its
//!   first message, and in the note its tab and header draw — never by the parent's number.
//!
//! And it answers the one ask a handed-off chat makes back: **a report** (charter-app#259).
//! The chat it goes to is the parent the app recorded when it opened the chat, never one the
//! reporting chat names, and only for a handoff that asked for one. See [`report_it`].

use std::sync::Arc;
use std::time::Instant;

use purlis_core::active::Place;
use purlis_core::engine::Size;
use purlis_core::hookwire::{Answer, Ask, OpenChat, Row, Tickets};
use purlis_core::reopen::{Chat, HandedFrom, Owed};

use crate::planes::{Held, PlaneId};

/// The event the window is sent when a handoff has opened a chat.
pub const ARRIVED: &str = "handoff-arrived";

/// A chat a handoff opened, as the window needs it to draw a tab.
///
/// # Where it lands, and why not in front
///
/// **A tab in the target workspace's strip, in the same window, behind whatever is in front.**
/// The operator runs many chats at once, and a handoff is by construction work they sent
/// *away* from the chat they are reading. A tab that took the front would cut them off
/// mid-sentence, and one filed in another workspace that took the front would move them to a
/// workspace they did not ask to look at. Python's handoff opens its chat in a background
/// window for the same reason: the design is "opened without taking your screen"
/// (`docs show handoff`).
///
/// **The window is not raised either.** An operator who has switched to another app did so on
/// purpose; the chat is on the strip when they come back.
///
/// **The one exception is a window with no tab at all**, where the new one takes the front:
/// there is nothing to interrupt, and a strip with a tab and nothing in front of it is a blank
/// pane that looks broken.
///
/// That the chat is on a strip at all, visibly, is what makes a handoff from inside the app
/// acceptable: see the justification on `purlis_core::hookwire::OpenChat`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Arrived {
    pub plane: PlaneId,
    pub session: u32,
    /// The chat's own name — its number, which the tab's default puts after the persona.
    pub name: String,
    /// The task name the handoff gave it (`--name`), which the tab says instead of its default
    /// (charter-app#258).
    pub label: Option<String>,
    /// Where it came from, for the note its tab and header draw.
    pub from: Option<crate::HandedFromNote>,
    /// The workspace whose strip it is filed on.
    pub workspace: String,
    pub persona: Option<String>,
    /// The harness it runs, by the word the plane calls it — what its tab's default name puts
    /// before the number when it adopted no persona (charter-app#254).
    pub harness: Option<String>,
}

/// Told when a handoff has opened a chat. The event carries its plane.
pub type Arrivals = Arc<dyn Fn(Arrived) + Send + Sync + 'static>;

/// Answers one ask on the hook socket of `held`'s plane.
///
/// `connection` is the listener's number for the connection the ask came on, which is what a
/// ticket is bound to.
pub fn answer(
    held: &Held,
    plane: &PlaneId,
    tickets: &Tickets,
    connection: u64,
    ask: Ask,
    arrived: &(dyn Fn(Arrived) + Send + Sync),
) -> Answer {
    let now = Instant::now();
    match ask {
        Ask::Ticket { chat } => {
            // Only for a chat this app is running. The ticket's chat is the one whose profile
            // the new chat inherits, so a number the app has no chat under has nothing to
            // inherit and nothing to mint for.
            if !is_open(held, chat) {
                return no(format!("chat {chat} is not one this app has open"));
            }
            match tickets.mint(chat, connection, now) {
                Ok(ticket) => Answer::Ticket { ticket },
                Err(why) => no(why),
            }
        }
        Ask::Open(open) => {
            // Spent FIRST, before anything below can refuse, so a request that is refused for
            // any reason has used its ticket up.
            if let Err(why) = tickets.spend(open.chat, connection, &open.ticket, now) {
                return no(why);
            }
            match open_it(held, plane, &open, STARTING) {
                Ok((it, row)) => {
                    let chat = it.session;
                    arrived(it);
                    Answer::Opened {
                        chat,
                        row: Some(row),
                    }
                }
                Err(why) => no(why),
            }
        }
        Ask::Report(back) => {
            // Spent first, for the open's reason.
            if let Err(why) = tickets.spend(back.chat, connection, &back.ticket, now) {
                return no(why);
            }
            report_it(held, back.chat, &back.summary).unwrap_or_else(no)
        }
        // A brokered write, not a handoff: no ticket, because a record is the chat's own to
        // write and the line can only name the chat whose token it carries (#1332).
        Ask::SessionRecord(record) => crate::smartclose::record(held, &record),
        // Another brokered write: the project's own files, for the chat that asks (#1333).
        Ask::Write(write) => crate::brokered::write(held, &write),
        // A brokered git action (#1335): no ticket, for the record's reason — the line names
        // only the chat whose token it carries, and what it asks is checked against the app's
        // record of that chat.
        Ask::Git(git) => crate::gitbroker::answer(held, &git),
        // `purlis vault list` from a sandboxed chat (#1430): no ticket, because it changes
        // nothing, and whose vaults it lists is the app's record of the chat the token is for.
        Ask::Vaults { chat } => crate::vaults::list_for_chat(held, chat),
        // Where the asking chat is working (#1450): no ticket, for the record's reason. The
        // line names only the chat whose token it carries, and the picture is this app's own
        // record of its open chats.
        Ask::WhereWorking(asks) => held
            .chats()
            .working(asks.chat, asks.tell, &|chat| {
                held.board().glance(chat).state
            })
            .map_or_else(
                || no(format!("chat {} is not one this app has open", asks.chat)),
                |working| Answer::Working(Box::new(working)),
            ),
    }
}

/// Hands `summary` back from `chat` to the chat whose handoff opened it, or says why not
/// (charter-app#259).
///
/// **The recipient is the app's record, never the request.** `chat` is the one this ticket was
/// minted for, and the chat its report goes to is the parent the app wrote into that chat's own
/// record when it opened it ([`HandedFrom`]). Nothing the reporting chat says can point it
/// anywhere else.
///
/// It reaches the parent in two ways and neither types anything into it: a needs-you item
/// (`<child> reported back`), and the report itself, left in the plane for the parent's next
/// `UserPromptSubmit` hook to hand its turn as context (`purlis_core::handback`). A parent
/// that has closed gets neither; the report is kept for its workspace instead, and the next
/// chat to start there reads it.
fn report_it(held: &Held, chat: u32, summary: &str) -> Result<Answer, String> {
    use purlis_core::handback::{self, For, Handback};

    let from = held.chats().handed_from(chat).ok_or_else(|| {
        format!(
            "chat {chat} was not opened by a handoff, so there is no chat waiting on a report \
             from it"
        )
    })?;
    match from.report {
        Owed::Due => {}
        Owed::Nothing => {
            return Err(format!(
                "the handoff that opened this chat did not ask for a report (it had no \
                 --report), so '{}' is not waiting on one",
                from.name
            ));
        }
        Owed::Sent => {
            return Err(format!(
                "this chat has already reported back to '{}', and a handoff gets one report — \
                 already reported. Hand off again with --report for another",
                from.name
            ));
        }
    }
    let summary = purlis_core::handoff::report_summary(summary).map_err(|bad| bad.say())?;
    let chats = held.chats().open_now();
    let child = chats
        .iter()
        .find(|open| open.session == chat)
        .ok_or_else(|| format!("chat {chat} is not one this app has open"))?;
    let child_name = held
        .chats()
        .shown_name(chat)
        .unwrap_or_else(|| child.name.clone());
    // A parent is reachable when its tab is open AND its program is still running: one that
    // has ended will never fire the prompt its report waits for, so the report goes where the
    // next chat to start will read it, as it does for a parent that has closed.
    let parent_open = chats.iter().any(|open| open.session == from.chat)
        && !matches!(
            held.board().glance(from.chat).state,
            purlis_core::state::State::Done | purlis_core::state::State::Failed
        );
    let to = if parent_open {
        held.chats()
            .shown_name(from.chat)
            .unwrap_or_else(|| from.name.clone())
    } else {
        from.name.clone()
    };
    let report = Handback {
        from: child_name.clone(),
        from_workspace: child
            .cwd
            .as_deref()
            .and_then(|cwd| workspace_of(held.root(), cwd))
            .map_or_else(|| from.workspace.clone(), Place::Workspace),
        to: to.clone(),
        to_workspace: from.workspace.clone(),
        summary,
    };
    let whose = if parent_open {
        For::Chat(from.chat)
    } else {
        For::Place(&from.workspace)
    };
    handback::leave(held.root(), whose, &report)
        .map_err(|why| format!("the report could not be kept ({why})"))?;
    held.chats().owes(chat, Owed::Sent);
    if parent_open {
        held.reported_back(from.chat, &child_name);
    }
    Ok(Answer::Reported {
        to,
        // By `Place::word`: a workspace's name, or the plane root's word (SI-1b).
        kept_for: (!parent_open).then(|| from.workspace.word().to_owned()),
    })
}

/// The workspace a chat standing in `cwd` works in: the directory under the plane's
/// `workspaces/` it is in, where it is in one. The ladder's cwd rung, so this and `charter`
/// cannot answer one directory two ways (SI-1).
fn workspace_of(root: &std::path::Path, cwd: &std::path::Path) -> Option<String> {
    purlis_core::active::workspace_of_tree(root, cwd)
}

fn no(why: String) -> Answer {
    Answer::No { why }
}

fn is_open(held: &Held, chat: u32) -> bool {
    held.chats()
        .open_now()
        .iter()
        .any(|open| open.session == chat)
}

/// **What a chat handed off to `target` holds** (#1362, D-1362-5): the grants the asking chat
/// `asking` itself runs with (`start::runs_with`, from the app's own record of it, its own hold
/// included), unless `target`'s hosts are all among them. So a chat that holds another
/// persona's grants hands off holding them still, and a chain of handoffs never climbs.
fn holds_after_handoff(
    policy: Option<&purlis_core::sandbox::Policy>,
    asking: &Chat,
    root: &std::path::Path,
    target: Option<&str>,
) -> Option<purlis_core::reopen::HeldGrants> {
    let trusted = purlis_core::start::runs_with(asking, root);
    purlis_core::sandbox::persona::held_unless_within(policy, trusted.as_deref(), target)
}

/// The size a handed-off chat starts at, the same one a relaunch uses: it has no pane yet to
/// ask, and the pane it lands in tells it the real one when it is first shown.
const STARTING: Size = Size {
    columns: 80,
    rows: 24,
};

/// Opens the chat `open` describes, or says why not in a sentence the asker prints.
///
/// The order is `charter/commands_handoff.py`'s after its frame check: every question first,
/// then the workspace (when this call creates it), then the chat. A refusal that comes after
/// the workspace was created says so, the way Python's `NOTHING_ELSE` does, because a
/// handoff that half-happened and says nothing about the half is worse than one that failed.
fn open_it(
    held: &Held,
    plane: &PlaneId,
    open: &OpenChat,
    size: Size,
) -> Result<(Arrived, Row), String> {
    use purlis_core::{handoff, start, wscmd};

    let root = held.root();
    let from = open.chat;
    let asking = held
        .chats()
        .open_now()
        .into_iter()
        .find(|chat| chat.session == from)
        .ok_or_else(|| format!("chat {from} is not one this app has open"))?;
    let Some(stamp) = handoff::stamped(&open.message).filter(|read| read.chat == from.to_string())
    else {
        return Err(format!(
            "the first message does not open with the stamp of a handoff from chat {from}, \
             and a chat the app opens on request always says where it came from"
        ));
    };
    // The parent as the operator sees it, which is what the new chat and its tab are told —
    // never its number (charter-app#258). A copy, so the note still reads once it is closed.
    let parent = held
        .chats()
        .shown_name(from)
        .ok_or_else(|| format!("chat {from} is not one this app has open"))?;
    // A workspace's name, or the plane root (SI-1b): a chat at the root is in no workspace,
    // and its stamp says so rather than naming the one the ladder would have picked.
    let Some(left_from) = stamp.place() else {
        return Err(format!(
            "the stamp names '{}' as the workspace the handoff left from, which cannot be one",
            purlis_core::shown::short(stamp.workspace)
        ));
    };
    // The command held the name to this rule already; held again because the request is what
    // arrived here, and a name is drawn on a tab.
    let label = match open.name.as_deref() {
        Some(raw) => purlis_core::reopen::label(raw)?,
        None => None,
    };
    let message = handoff::delivered(&open.message, &parent, open.report)
        .expect("the stamp was read a moment ago");
    // The command asked this already; asked again because these bytes are about to become a
    // harness's argv, and the bound and the NUL are facts about argv.
    if let Some(bad) = handoff::bad_message(&message) {
        return Err(bad.say());
    }
    let profile = asking.profile.clone().ok_or_else(|| {
        format!(
            "chat {from} is not on a harness profile, so there is no harness to start the new \
             chat on, and a handoff never changes harness"
        )
    })?;
    let ws = open.workspace.as_str();
    let dir = wscmd::workspace_dir(root, ws).ok_or_else(|| {
        format!(
            "'{}' cannot name a workspace",
            purlis_core::shown::short(ws)
        )
    })?;
    let created = match open.create_vision.as_deref() {
        Some(vision) => {
            if wscmd::workspace_dir_exists(root, ws) {
                return Err(format!("workspace '{ws}' already exists"));
            }
            wscmd::ensure::ensure(root, ws, chrono::Utc::now(), &wscmd::ensure::author())?;
            // `create`'s own two calls, in its order: the vision is written into the
            // workspace `ensure` just scaffolded.
            let plane_on_disk = purlis_core::workspaces::Plane::open(root);
            if let Ok(workspace) = plane_on_disk.workspace(ws) {
                let _ = workspace.set_vision(vision);
            }
            true
        }
        None => {
            if !dir.is_dir() {
                return Err(format!(
                    "workspace '{ws}' has no directory to open a chat in"
                ));
            }
            false
        }
    };
    let stays = |why: String| {
        if created {
            format!("{why} The workspace '{ws}' was created and stays; no chat was opened.")
        } else {
            why
        }
    };
    // `persona=args.persona or ""` in Python, which is the frame's own default: a handed-off
    // chat does not inherit the asking chat's persona, it gets the one the operator named or
    // the plane's.
    let persona = open
        .persona
        .clone()
        .or_else(|| start::persona_for_a_new_chat(root));
    // **A handoff never widens what the asking chat reaches** (#1362, D-1362-5): where the
    // persona it hands to has hosts the asking chat does not run with, the new chat holds the
    // asking chat's grants — read from this app's record of that chat, never from the
    // request — until the person allows its own on its tab.
    let asking_chat = held
        .chats()
        .recorded_chat(from)
        .ok_or_else(|| format!("chat {from} is not one this app has open"))?;
    // Into the workspace the asking chat is in, or another: the dispatch row's one fact about
    // where the work went, from this app's record of that chat and never from the stamp, which
    // the chat wrote (#1421, D-1421-11).
    let placement = match asking_chat
        .cwd
        .as_deref()
        .and_then(|cwd| workspace_of(root, cwd))
    {
        Some(asking_ws) if asking_ws == ws => purlis_core::dispatch::Placement::Here,
        _ => purlis_core::dispatch::Placement::Elsewhere,
    };
    let held_grants = holds_after_handoff(
        purlis_core::sandbox::Plane::read(root)
            .in_force(&purlis_core::sandbox::policy::Locks::of(root))
            .as_ref(),
        &asking_chat,
        root,
        persona.as_deref(),
    );
    // Its own name is a number no chat in this plane has had, dealt now so the chat can be
    // started under it: with no task name its tab says `<persona> <N>`, the ordinary default,
    // and four handoffs from one chat are four different tabs (charter-app#258).
    let number = held.chats().sessions().deal();
    let name = number.to_string();
    let mut ready = start::ready(
        &start::Start {
            profile: Some(profile.clone()),
            persona: persona.clone(),
            name: name.clone(),
            cwd: Some(dir),
            resume: None,
            // The picker's footer box is one operator choice for one chat, and nobody made it
            // for this one.
            show_footer: false,
            resuming: None,
            without_sandbox: None,
            held: held_grants.clone(),
            grants: Default::default(),
        },
        root,
    )
    .map_err(stays)?;
    let Some(first) = ready
        .harness
        .and_then(|harness| handoff::first_message_argv(harness.name(), &message))
    else {
        return Err(stays(format!(
            "profile '{profile}' runs a harness purlis has not measured the first message of, \
             so it cannot be started on the brief."
        )));
    };
    // Last on the line: a positional prompt is what nothing may come after, and the app's own
    // hook arguments go in FRONT of these (`Chats::open_it`).
    ready.args.extend(first);
    let chat = Chat {
        program: ready.program.clone(),
        // What the RECORD keeps, which is the profile's own words and not the brief: a
        // relaunch resumes the conversation, and sending the brief a second time would be a
        // message the operator did not approve twice.
        args: Vec::new(),
        cwd: ready.cwd.clone(),
        name: name.clone(),
        resume: ready.session.clone(),
        active: false,
        profile: Some(profile),
        persona: persona.clone(),
        show_footer: false,
        pinned: false,
        number: Some(number),
        label: label.clone(),
        from: Some(HandedFrom {
            chat: from,
            name: parent,
            workspace: left_from,
            report: if open.report {
                Owed::Due
            } else {
                Owed::Nothing
            },
            // The work moved, so the chat opens as a tab. A task dispatch records
            // `Mode::Task` through this same field.
            mode: purlis_core::reopen::Mode::Handoff,
        }),
        held: held_grants,
        renamed_from: None,
        ..Default::default()
    };
    let session = held
        .chats()
        .start_ready(&chat, &ready, size)
        .map_err(stays)?;
    let row = handoff_row(root, held.config(), placement, created);
    let arrived = Arrived {
        plane: plane.clone(),
        session,
        name,
        label,
        from: chat
            .from
            .as_ref()
            .map(|from| crate::HandedFromNote::of(from, chat.has_tab())),
        workspace: ws.to_owned(),
        persona,
        harness: ready.harness.map(|harness| harness.name().to_owned()),
    };
    Ok((arrived, row))
}

/// The handoff's row in the project's dispatch log (`purlis_core::dispatch::record_handoff`),
/// written here, by the app that opened the chat (#1421): a sandboxed chat may not write the
/// project's `personas/_dispatch/`, and the app is not sandboxed. Its four fields name no
/// workspace and nothing of the brief. A row that could not be written is said back to the
/// command, which tells the chat; the chat is open either way.
///
/// **The log is named from `config`, the machine store the app resolved at startup**
/// ([`Held::config`]), as a brokered git action's piece log is (#1335): nothing on the hook
/// listener's path resolves the store again, which a fenced test build refuses.
fn handoff_row(
    root: &std::path::Path,
    config: Option<&std::path::Path>,
    placement: purlis_core::dispatch::Placement,
    created: bool,
) -> Row {
    match purlis_core::dispatch::record_handoff(
        root,
        placement,
        created,
        chrono::Utc::now(),
        &purlis_core::dispatch::log_name(config, &purlis_core::dispatch::host()),
    ) {
        Ok(_) => Row::Written,
        Err(why) => Row::Unwritten {
            why: purlis_core::rewrite::os_words(&why),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use purlis_core::hookwire::NO_TICKET;

    use super::*;
    use crate::planes::Planes;

    /// The stamp `charter handoff` writes for a handoff leaving `chat`, and a brief.
    fn stamped(chat: u32) -> String {
        format!(
            "⟨handoff from chat {chat} · workspace default · 2026-05-04 11:32⟩\n\n# Ship it\nnow"
        )
    }

    fn an_open(chat: u32, ticket: &str, message: String) -> Ask {
        a_named_open(chat, ticket, message, None, false)
    }

    fn a_named_open(
        chat: u32,
        ticket: &str,
        message: String,
        name: Option<&str>,
        report: bool,
    ) -> Ask {
        Ask::Open(Box::new(OpenChat {
            chat,
            workspace: "alpha".to_owned(),
            create_vision: None,
            persona: None,
            message,
            ticket: ticket.to_owned(),
            name: name.map(str::to_owned),
            report,
        }))
    }

    /// A plane with a workspace `alpha`, a profile `work` running a stand-in `claude` that
    /// writes down the arguments it was started with, and the operator's approval of it.
    ///
    /// **One file per run, and it appears whole** (charter-app#268). Two runs of the stand-in
    /// are alive at once in a handoff — the asking chat's and the one it opens — and when both
    /// appended to one file, one `printf` to a line, their lines interleaved: the brief came
    /// back split around the other run's `--session-id`, and the test failed about one run in
    /// twenty (one in two under load). So each run writes its arguments, NUL-separated since an
    /// argument can hold a newline, to a temporary file of its own and renames it into `runs/`
    /// when it is done: a file there is a run's complete argv, never part of one.
    struct Plane {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Plane {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a directory");
            let root = dir.path().join("plane");
            std::fs::create_dir_all(root.join("workspaces").join("alpha")).expect("alpha");
            std::fs::write(root.join(purlis_core::plane::MANIFEST), "").expect("charter.toml");
            let runs = root.join("runs");
            std::fs::create_dir_all(&runs).expect("runs");
            let program = stand_in::program(
                &root,
                "claude-stand-in",
                // And then stays running, as a harness does: a chat whose program has ended is
                // one a report cannot reach, and the tests below are about one that is there.
                &format!(
                    "#!/bin/sh\nat=$(mktemp {runs:?}/.writing.XXXXXX) || exit 1\n\
                     for a in \"$@\"; do printf '%s\\0' \"$a\"; done > \"$at\"\n\
                     mv \"$at\" {runs:?}/run.$$\n\
                     sleep 10\n"
                ),
            );
            std::fs::write(
                root.join(purlis_core::profiles::LOCAL_FILE),
                format!(
                    "[harness.work]\nkind = \"claude\"\ncommand = [{:?}]\n",
                    program.display().to_string()
                ),
            )
            .expect("the profile");
            let set = purlis_core::profiles::current(&root);
            let work = set.get("work").expect("the profile reads");
            purlis_core::profiletrust::record_launched(
                &root,
                "work",
                &purlis_core::profiletrust::fingerprint(work),
            )
            .expect("approved");
            Self { _dir: dir, root }
        }

        /// The argv of every run of the stand-in that has finished writing it.
        fn runs(&self) -> Vec<Vec<String>> {
            let Ok(entries) = std::fs::read_dir(self.root.join("runs")) else {
                return Vec::new();
            };
            entries
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("run."))
                .map(|entry| {
                    let written = std::fs::read_to_string(entry.path()).unwrap_or_default();
                    written.split_terminator('\0').map(str::to_owned).collect()
                })
                .collect()
        }
    }

    fn planes() -> Planes {
        Planes::telling(Arc::new(|_| {}), crate::Shipped::default(), None)
    }

    /// A chat on the `work` profile, started the way the picker starts one.
    fn a_chat_on_work(held: &Held, root: &Path) -> u32 {
        let ready = purlis_core::start::ready(
            &purlis_core::start::Start {
                profile: Some("work".to_owned()),
                persona: None,
                name: "1".to_owned(),
                cwd: Some(root.to_path_buf()),
                resume: None,
                show_footer: false,
                resuming: None,
                without_sandbox: None,
                held: None,
                grants: Default::default(),
            },
            root,
        )
        .expect("the asking chat starts");
        let chat = Chat {
            program: ready.program.clone(),
            args: Vec::new(),
            cwd: ready.cwd.clone(),
            name: "1".to_owned(),
            resume: ready.session.clone(),
            active: false,
            profile: Some("work".to_owned()),
            persona: None,
            show_footer: false,
            pinned: false,
            number: None,
            label: None,
            from: None,
            renamed_from: None,
            ..Default::default()
        };
        held.chats()
            .start_ready(&chat, &ready, STARTING)
            .expect("it runs")
    }

    /// A shell chat: not on any profile.
    fn a_shell_chat(held: &Held) -> u32 {
        let chat = Chat {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 5".to_owned()],
            cwd: None,
            name: "sh".to_owned(),
            resume: None,
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
        };
        held.chats().start(&chat, STARTING).expect("it runs")
    }

    fn nobody(_: Arrived) {}

    fn nothing_opens(arrived: Arrived) {
        panic!("nothing was opened, so nothing is told: {arrived:?}")
    }

    fn ticket(held: &Held, plane: &PlaneId, tickets: &Tickets, chat: u32) -> String {
        match answer(held, plane, tickets, 1, Ask::Ticket { chat }, &nobody) {
            Answer::Ticket { ticket } => ticket,
            other => panic!("a ticket, not {other:?}"),
        }
    }

    const PERSONAS: &str = "[sandbox]\nmode = \"on\"\negress = []\n\
                            [sandbox.personas.devops]\nhosts = [\"10.100.39.145:6443\"]\n\
                            [sandbox.personas.qa]\nhosts = []\n";

    /// #1362, D-1362-5: a chat that holds another persona's grants hands off holding them, so a
    /// chain of handoffs (qa → devops → devops) never climbs past the first chat's grants; a chat
    /// that holds its own hands off widening nothing it already reaches.
    #[test]
    fn a_held_chat_s_own_handoff_stays_held() {
        let policy = purlis_core::sandbox::Plane::of(Some(PERSONAS))
            .said()
            .policy;
        let root = tempfile::tempdir().expect("a project");
        let qa = Chat {
            persona: Some("qa".to_owned()),
            ..Default::default()
        };
        let b_holds = holds_after_handoff(policy.as_ref(), &qa, root.path(), Some("devops"))
            .expect("qa to devops is held");
        assert_eq!(b_holds.persona.as_deref(), Some("qa"));
        let b = Chat {
            persona: Some("devops".to_owned()),
            held: Some(b_holds),
            ..Default::default()
        };
        let c_holds = holds_after_handoff(policy.as_ref(), &b, root.path(), Some("devops"))
            .expect("B's own handoff to devops stays held");
        assert_eq!(c_holds.persona.as_deref(), Some("qa"));
        // B once allowed hands off to devops holding its own.
        let allowed = Chat { held: None, ..b };
        assert_eq!(
            holds_after_handoff(policy.as_ref(), &allowed, root.path(), Some("devops")),
            None
        );
    }

    /// #1362: Allow clears a chat's hold, and the record written says so, so a relaunch starts
    /// it with its own persona's grants.
    #[test]
    fn allow_own_grants_clears_the_hold_and_the_record_keeps_it_cleared() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let chat = Chat {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 5".to_owned()],
            name: "sh".to_owned(),
            persona: Some("devops".to_owned()),
            held: Some(purlis_core::reopen::HeldGrants {
                persona: Some("qa".to_owned()),
            }),
            ..Default::default()
        };
        let session = held.chats().start(&chat, STARTING).expect("it runs");
        assert_eq!(
            held.chats().grants_held(session),
            Some((None, Some("devops".to_owned())))
        );
        assert!(
            held.chats()
                .record()
                .chats
                .iter()
                .any(|chat| chat.held.is_some()),
            "the record holds it until it is allowed"
        );

        assert!(held.chats().allow_own_grants(session));

        assert_eq!(held.chats().grants_held(session), None);
        assert!(
            !held.chats().allow_own_grants(session),
            "nothing left to allow"
        );
        // What the record is written from: the hold is gone from it too.
        let recorded = held.chats().record();
        assert!(
            recorded.chats.iter().all(|chat| chat.held.is_none()),
            "{recorded:?}"
        );
    }

    #[test]
    fn an_opened_handoff_s_row_is_written_by_the_app_and_says_so() {
        // #1421: a sandboxed chat may not write the project's dispatch log, so the app writes
        // the handoff's row where it opens the chat, and tells the command it did.
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let ticket = ticket(&held, &id, &tickets, asking);
        // The stamp is the chat's own words, and it claims the work stays in `alpha`.
        let claims_here = format!(
            "⟨handoff from chat {asking} · workspace alpha · 2026-05-04 11:32⟩\n\n# Ship it\nnow"
        );

        let said = answer(
            &held,
            &id,
            &tickets,
            1,
            an_open(asking, &ticket, claims_here),
            &|_| {},
        );

        let Answer::Opened { row, .. } = said else {
            panic!("opened, not {said:?}")
        };
        assert_eq!(row, Some(Row::Written));
        let rows: Vec<_> = purlis_core::dispatch::rows(&plane.root)
            .into_iter()
            .filter(|row| row["event"] == "handoff")
            .collect();
        assert_eq!(rows.len(), 1, "{rows:?}");
        // The app's record has the asking chat at the project's root, and the work goes to
        // `alpha`: elsewhere, whatever the stamp claimed (D-1421-11).
        assert_eq!(rows[0]["placement"], "elsewhere");
        assert_eq!(rows[0]["created"], false);
    }

    #[test]
    fn a_handoff_opens_a_chat_on_the_asking_chats_profile_with_the_brief_as_its_first_message() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let told = Mutex::new(Vec::new());
        let ticket = ticket(&held, &id, &tickets, asking);

        let said = answer(
            &held,
            &id,
            &tickets,
            1,
            an_open(asking, &ticket, stamped(asking)),
            &|arrived| told.lock().unwrap().push(arrived),
        );

        let Answer::Opened { chat, .. } = said else {
            panic!("opened, not {said:?}")
        };
        assert_eq!(
            told.lock().unwrap().clone(),
            vec![Arrived {
                plane: id.clone(),
                session: chat,
                // Its own number: the tab says `claude <N>`, the ordinary default.
                name: chat.to_string(),
                label: None,
                from: Some(crate::HandedFromNote {
                    name: "claude 1".to_owned(),
                    workspace: "default".to_owned(),
                    chat: 1,
                    task: false,
                    tab: true,
                }),
                workspace: "alpha".to_owned(),
                persona: None,
                harness: Some("claude".to_owned()),
            }],
            "the window is told, so the tab lands on alpha's strip"
        );
        let opened = held
            .chats()
            .open_now()
            .into_iter()
            .find(|open| open.session == chat)
            .expect("the new chat is one the app has open");
        assert_eq!(
            opened.profile.as_deref(),
            Some("work"),
            "the asking chat's profile"
        );
        assert_eq!(
            opened.cwd.as_deref(),
            // The root the registry settled on, which on macOS is `/private/var/…` for a
            // temporary directory handed in as `/var/…`.
            Some(held.root().join("workspaces").join("alpha").as_path()),
            "it stands in the workspace it was handed to"
        );
        // The harness is handed the stamped brief as its last argument, which is Claude
        // Code's positional first message. The program runs on its own, so it is waited for.
        let told = first_message_of(&plane);
        assert_eq!(
            told,
            "⟨handoff from claude 1 · workspace default · 2026-05-04 11:32⟩\n\n# Ship it\nnow",
            "the brief was not the chat's first message, stamped with its parent's name: {told:?}"
        );
        assert!(
            !told.contains(&format!("chat {asking}")),
            "never the parent's number: {told:?}"
        );
    }

    /// The first message a handed-off chat was started on — the last argument of the stand-in's
    /// run that got one — once that run has written its argv whole. Empty if none came.
    fn first_message_of(plane: &Plane) -> String {
        let handed = |plane: &Plane| {
            plane
                .runs()
                .into_iter()
                .filter_map(|argv| argv.last().cloned())
                .find(|last| last.starts_with("⟨handoff from"))
        };
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        while handed(plane).is_none() && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        handed(plane).unwrap_or_default()
    }

    /// Opens a handoff from `asking` and answers the new chat's number.
    fn hand_off(
        held: &Held,
        id: &PlaneId,
        tickets: &Tickets,
        asking: u32,
        name: Option<&str>,
        report: bool,
    ) -> Result<(u32, Arrived), String> {
        hand_off_with(held, id, tickets, asking, stamped(asking), name, report)
    }

    /// The stamp `charter handoff` writes for a handoff leaving a chat at the plane root
    /// (SI-1b), and a brief.
    fn stamped_at_the_root(chat: u32) -> String {
        format!("⟨handoff from chat {chat} · plane root · 2026-05-04 11:32⟩\n\n# Ship it\nnow")
    }

    /// [`hand_off`], with the first message `message`.
    fn hand_off_with(
        held: &Held,
        id: &PlaneId,
        tickets: &Tickets,
        asking: u32,
        message: String,
        name: Option<&str>,
        report: bool,
    ) -> Result<(u32, Arrived), String> {
        let ticket = ticket(held, id, tickets, asking);
        let told = Mutex::new(None);
        match answer(
            held,
            id,
            tickets,
            1,
            a_named_open(asking, &ticket, message, name, report),
            &|arrived| *told.lock().unwrap() = Some(arrived),
        ) {
            Answer::Opened { chat, .. } => Ok((chat, told.into_inner().unwrap().expect("told"))),
            Answer::No { why } => Err(why),
            other => panic!("opened or refused, not {other:?}"),
        }
    }

    /// `child` reports `summary` back, on a ticket of its own.
    fn report(held: &Held, id: &PlaneId, tickets: &Tickets, child: u32, summary: &str) -> Answer {
        let ticket = ticket(held, id, tickets, child);
        answer(
            held,
            id,
            tickets,
            1,
            Ask::Report(Box::new(purlis_core::hookwire::ReportBack {
                chat: child,
                summary: summary.to_owned(),
                ticket,
            })),
            &nothing_opens,
        )
    }

    // ----- where a chat is working (#1450) -----

    /// What the app answers chat `chat` asking where it is working.
    fn working(
        held: &Held,
        id: &PlaneId,
        chat: u32,
        tell: purlis_core::awareness::Tell,
    ) -> purlis_core::awareness::Working {
        match answer(
            held,
            id,
            &Tickets::default(),
            1,
            Ask::WhereWorking(purlis_core::hookwire::WhereWorking { chat, tell }),
            &nothing_opens,
        ) {
            Answer::Working(working) => *working,
            other => panic!("where it is working, not {other:?}"),
        }
    }

    fn names(rows: &[purlis_core::awareness::Row]) -> Vec<&str> {
        rows.iter().map(|row| row.name.as_str()).collect()
    }

    #[test]
    fn a_handed_off_chat_is_told_who_asked_and_its_sibling_and_never_a_word_of_a_brief() {
        use purlis_core::awareness::{Parent, Tell};
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let (first, _) =
            hand_off(&held, &id, &tickets, asking, Some("drop commons"), true).expect("opened");
        hand_off(&held, &id, &tickets, asking, Some("lint"), false).expect("opened");

        let told = working(&held, &id, first, Tell::Asked);

        assert_eq!(told.picture.me.name, "drop commons");
        assert_eq!(
            told.picture.parent,
            Some(Parent {
                name: "claude 1".to_owned(),
                open: true
            })
        );
        assert_eq!(names(&told.picture.siblings), ["lint"]);
        assert_eq!(
            told.picture.siblings[0].workspace,
            purlis_core::active::Place::Workspace("alpha".to_owned())
        );
        // Both chats were opened on a brief, and none of either is in what one learns of the
        // other: not its words, and not the stamp it opened with.
        let wire = serde_json::to_string(&told).expect("json");
        for of_a_brief in ["Ship it", "handoff from", "2026-05-04"] {
            assert!(!wire.contains(of_a_brief), "{of_a_brief:?} in {wire}");
        }
        // The chat that asked was asked for by nobody.
        assert!(working(&held, &id, asking, Tell::Asked).picture.is_alone());
    }

    #[test]
    fn a_turn_is_told_a_sibling_started_and_reported_once_each_and_nothing_in_between() {
        use purlis_core::awareness::{Tell, What};
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let (first, _) =
            hand_off(&held, &id, &tickets, asking, Some("drop commons"), false).expect("opened");
        working(&held, &id, first, Tell::Start);
        assert_eq!(working(&held, &id, first, Tell::Turn).changes, Vec::new());

        let (second, _) =
            hand_off(&held, &id, &tickets, asking, Some("lint"), true).expect("opened");
        // Asked by the command in between: the turn is still told.
        working(&held, &id, first, Tell::Asked);
        let started = working(&held, &id, first, Tell::Turn).changes;
        assert_eq!(started.len(), 1, "{started:?}");
        assert_eq!(
            (started[0].what, started[0].row.name.as_str()),
            (What::Started, "lint")
        );
        assert_eq!(working(&held, &id, first, Tell::Turn).changes, Vec::new());

        report(&held, &id, &tickets, second, "Linted.");
        let reported = working(&held, &id, first, Tell::Turn).changes;
        assert_eq!(reported.len(), 1, "{reported:?}");
        assert_eq!(reported[0].what, What::Reported);
        assert!(!format!("{reported:?}").contains("Linted"), "{reported:?}");
        assert_eq!(working(&held, &id, first, Tell::Turn).changes, Vec::new());
    }

    #[test]
    fn where_a_chat_this_app_does_not_have_open_is_working_is_refused() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let said = answer(
            &held,
            &id,
            &Tickets::default(),
            1,
            Ask::WhereWorking(purlis_core::hookwire::WhereWorking {
                chat: 41,
                tell: purlis_core::awareness::Tell::Start,
            }),
            &nothing_opens,
        );

        assert_eq!(
            said,
            Answer::No {
                why: "chat 41 is not one this app has open".to_owned()
            }
        );
    }

    // ----- named for its task (charter-app#258) -----

    #[test]
    fn a_handoff_with_a_task_name_opens_a_chat_called_that() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);

        let (chat, arrived) = hand_off(
            &held,
            &id,
            &Tickets::default(),
            asking,
            Some(" drop commons "),
            false,
        )
        .expect("opened");

        assert_eq!(arrived.label.as_deref(), Some("drop commons"));
        assert_eq!(
            held.chats().shown_name(chat).as_deref(),
            Some("drop commons")
        );
        assert_eq!(
            held.chats()
                .record()
                .chats
                .last()
                .and_then(|c| c.label.clone())
                .as_deref(),
            Some("drop commons"),
            "the name rides the record"
        );
    }

    #[test]
    fn four_handoffs_from_one_chat_are_four_distinguishable_tabs() {
        // The operator's report: four handoffs, four tabs, every one "handoff from 16".
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();

        let shown: Vec<String> = (0..4)
            .map(|_| {
                let (chat, _) =
                    hand_off(&held, &id, &tickets, asking, None, false).expect("opened");
                held.chats().shown_name(chat).expect("open")
            })
            .collect();

        let mut distinct = shown.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 4, "{shown:?}");
        assert!(
            shown.iter().all(|name| name.starts_with("claude ")),
            "the ordinary default, `<harness> <N>`: {shown:?}"
        );
    }

    #[test]
    fn a_task_name_charter_would_not_draw_opens_nothing() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let before = held.chats().open_now().len();

        let refused = hand_off(
            &held,
            &id,
            &Tickets::default(),
            asking,
            Some("drop\u{200b}commons"),
            false,
        )
        .expect_err("refused");

        assert!(refused.contains("invisible"), "{refused}");
        assert_eq!(held.chats().open_now().len(), before);
    }

    #[test]
    fn the_new_chat_knows_its_parent_by_the_name_the_operator_gave_it() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        held.chats().rename(asking, "platform steward").unwrap();

        let (_, arrived) =
            hand_off(&held, &id, &Tickets::default(), asking, None, false).expect("opened");

        assert_eq!(
            arrived.from,
            Some(crate::HandedFromNote {
                name: "platform steward".to_owned(),
                workspace: "default".to_owned(),
                chat: asking,
                task: false,
                tab: true,
            })
        );
        assert!(first_message_of(&plane).contains("⟨handoff from platform steward · workspace"));
    }

    // ----- a report back (charter-app#259) -----

    #[test]
    fn a_handoff_that_wants_an_answer_tells_the_new_chat_how_to_give_one() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);

        hand_off(&held, &id, &Tickets::default(), asking, None, true).expect("opened");

        assert!(
            first_message_of(&plane).contains(handoff_report_ask()),
            "{:?}",
            plane.runs()
        );
    }

    fn handoff_report_ask() -> &'static str {
        purlis_core::handoff::REPORT_ASK
    }

    #[test]
    fn a_report_reaches_the_chat_that_asked_as_a_needs_you_item_and_its_next_turn() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let (child, _) =
            hand_off(&held, &id, &tickets, asking, Some("drop commons"), true).expect("opened");

        let said = report(&held, &id, &tickets, child, "Dropped it.");

        assert_eq!(
            said,
            Answer::Reported {
                to: "claude 1".to_owned(),
                kept_for: None
            }
        );
        assert_eq!(
            held.hooks().board().reports(asking),
            vec!["drop commons".to_owned()],
            "`drop commons reported back`, on the chat that asked"
        );
        assert!(held.hooks().board().needs_you().contains(&asking));
        let waiting =
            purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(asking));
        assert_eq!(waiting.len(), 1, "left for its next turn");
        assert_eq!(waiting[0].summary, "Dropped it.");
        assert_eq!(waiting[0].from, "drop commons");
        assert_eq!(
            waiting[0].from_workspace,
            purlis_core::active::Place::Workspace("alpha".to_owned())
        );
    }

    #[test]
    fn a_chat_reports_once_and_a_prompt_afterwards_does_not_let_it_report_again() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let (child, _) = hand_off(&held, &id, &tickets, asking, None, true).expect("opened");
        report(&held, &id, &tickets, child, "first");

        let again = report(&held, &id, &tickets, child, "second");
        assert!(
            matches!(&again, Answer::No { why } if why.contains("already reported")),
            "{again:?}"
        );

        // The operator gives the child another turn, as its own harness reports it over the
        // socket: a prompt is not the parent asking again (the operator's ruling, #259).
        let conversation = held
            .hooks()
            .board()
            .conversation(child)
            .map(str::to_owned)
            .expect("the child was started under a conversation charter chose");
        purlis_core::hookwire::send(
            held.hooks().socket().expect("the plane listens"),
            Some(&held.hooks().token_for(child)),
            &purlis_core::hookwire::Report {
                chat: child,
                event: purlis_core::state::Event::UserPromptSubmit,
                conversation: purlis_core::hookwire::Conversation::Named(conversation),
                pid: Some(4242),
                agent: None,
                detail: purlis_core::state::Detail::default(),
            },
        )
        .expect("the prompt is sent");
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while held.hooks().board().state(child) != purlis_core::state::State::Running
            && Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(
            held.hooks().board().state(child),
            purlis_core::state::State::Running,
            "the prompt reached the board"
        );

        let after = report(&held, &id, &tickets, child, "third");
        assert!(
            matches!(&after, Answer::No { why } if why.contains("--report for another")),
            "a prompt does not re-arm it: {after:?}"
        );
    }

    #[test]
    fn a_report_from_a_handoff_that_did_not_ask_is_refused_saying_why() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let (child, _) = hand_off(&held, &id, &tickets, asking, None, false).expect("opened");

        let said = report(&held, &id, &tickets, child, "done");

        assert!(
            matches!(&said, Answer::No { why } if why.contains("did not ask for a report")),
            "{said:?}"
        );
        assert!(held.hooks().board().reports(asking).is_empty());
    }

    #[test]
    fn a_chat_no_handoff_opened_has_nobody_to_report_to() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);

        let said = report(&held, &id, &Tickets::default(), asking, "done");

        assert!(
            matches!(&said, Answer::No { why } if why.contains("not opened by a handoff")),
            "{said:?}"
        );
    }

    #[test]
    fn a_report_charter_would_not_hand_back_is_refused_and_still_owed() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let (child, _) = hand_off(&held, &id, &tickets, asking, None, true).expect("opened");

        let said = report(&held, &id, &tickets, child, "done\u{202e}enod");

        assert!(matches!(&said, Answer::No { .. }), "{said:?}");
        assert!(
            matches!(
                report(&held, &id, &tickets, child, "done"),
                Answer::Reported { .. }
            ),
            "a refused report used up nothing"
        );
    }

    #[test]
    fn a_report_whose_parent_has_closed_is_kept_for_its_workspace() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let (child, _) =
            hand_off(&held, &id, &tickets, asking, Some("drop commons"), true).expect("opened");
        held.chats().close(asking).unwrap();

        let said = report(&held, &id, &tickets, child, "Dropped it.");

        assert_eq!(
            said,
            Answer::Reported {
                to: "claude 1".to_owned(),
                kept_for: Some("default".to_owned()),
            }
        );
        let kept = purlis_core::handback::take(
            held.root(),
            purlis_core::handback::For::Place(&purlis_core::active::Place::Workspace(
                "default".to_owned(),
            )),
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].to, "claude 1");
    }

    // ----- a handoff from the plane root (SI-1b) -----

    #[test]
    fn a_handoff_from_the_plane_root_opens_in_the_workspace_it_names_and_says_where_it_left() {
        // The defect: the stamp named the ladder's workspace (the plane's default), because a
        // stamp had to name one and the app refused anything that was not a workspace's name.
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);

        let (chat, arrived) = hand_off_with(
            &held,
            &id,
            &Tickets::default(),
            asking,
            stamped_at_the_root(asking),
            None,
            false,
        )
        .expect("opened");

        // Where the brief sent it, as from anywhere else.
        assert_eq!(arrived.workspace, "alpha");
        let record = held.chats().handed_from(chat).expect("recorded");
        assert_eq!(record.workspace, purlis_core::active::Place::PlaneRoot);
        assert_eq!(
            arrived.from.map(|from| from.workspace),
            Some("plane root".to_owned())
        );
        assert!(
            first_message_of(&plane).starts_with("⟨handoff from claude 1 · plane root · "),
            "{:?}",
            plane.runs()
        );
    }

    #[test]
    fn a_report_whose_root_parent_has_closed_is_kept_for_the_plane_root() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let (child, _) = hand_off_with(
            &held,
            &id,
            &tickets,
            asking,
            stamped_at_the_root(asking),
            Some("drop commons"),
            true,
        )
        .expect("opened");
        held.chats().close(asking).unwrap();

        let said = report(&held, &id, &tickets, child, "Dropped it.");

        assert_eq!(
            said,
            Answer::Reported {
                to: "claude 1".to_owned(),
                kept_for: Some("plane root".to_owned()),
            }
        );
        let kept = purlis_core::handback::take(
            held.root(),
            purlis_core::handback::For::Place(&purlis_core::active::Place::PlaneRoot),
        );
        assert_eq!(kept.len(), 1);
        // The child is in the workspace the brief named, whatever its parent was in.
        assert_eq!(
            kept[0].from_workspace,
            purlis_core::active::Place::Workspace("alpha".to_owned())
        );
    }

    #[test]
    fn a_stamp_that_says_workspace_and_then_the_plane_roots_words_is_refused() {
        // Only the root's own shape says the root; `workspace plane root` is a workspace name
        // that cannot be one.
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);

        let refused = hand_off_with(
            &held,
            &id,
            &Tickets::default(),
            asking,
            format!(
                "⟨handoff from chat {asking} · workspace plane root · 2026-05-04 11:32⟩\n\nbody"
            ),
            None,
            false,
        )
        .expect_err("refused");
        assert!(refused.contains("cannot be one"), "{refused}");
    }

    #[test]
    fn a_ticket_is_minted_only_for_a_chat_this_app_has_open() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let said = answer(
            &held,
            &id,
            &Tickets::default(),
            1,
            Ask::Ticket { chat: 42 },
            &nobody,
        );

        assert_eq!(
            said,
            Answer::No {
                why: "chat 42 is not one this app has open".to_owned()
            }
        );
    }

    #[test]
    fn an_open_without_a_ticket_opens_nothing() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let before = held.chats().open_now().len();

        let said = answer(
            &held,
            &id,
            &Tickets::default(),
            1,
            an_open(asking, "made-up", stamped(asking)),
            &nothing_opens,
        );

        assert_eq!(
            said,
            Answer::No {
                why: NO_TICKET.to_owned()
            }
        );
        assert_eq!(held.chats().open_now().len(), before);
    }

    #[test]
    fn a_message_without_the_stamp_opens_nothing_and_still_spends_the_ticket() {
        // The stamp is what says, on the strip, where a chat came from. A process writing to
        // the socket directly could otherwise open an unmarked one.
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let ticket = ticket(&held, &id, &tickets, asking);
        let before = held.chats().open_now().len();

        let said = answer(
            &held,
            &id,
            &tickets,
            1,
            an_open(asking, &ticket, "# Ship it\nnow".to_owned()),
            &nothing_opens,
        );

        assert!(
            matches!(&said, Answer::No { why } if why.contains("stamp")),
            "{said:?}"
        );
        assert_eq!(held.chats().open_now().len(), before);
        assert_eq!(
            answer(
                &held,
                &id,
                &tickets,
                1,
                an_open(asking, &ticket, stamped(asking)),
                &nothing_opens
            ),
            Answer::No {
                why: NO_TICKET.to_owned()
            },
            "a refused open used its ticket up"
        );
    }

    #[test]
    fn a_stamp_naming_another_chat_opens_nothing() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let ticket = ticket(&held, &id, &tickets, asking);

        let said = answer(
            &held,
            &id,
            &tickets,
            1,
            an_open(asking, &ticket, stamped(asking + 1)),
            &nothing_opens,
        );

        assert!(
            matches!(&said, Answer::No { why } if why.contains("stamp")),
            "{said:?}"
        );
    }

    #[test]
    fn a_chat_on_no_profile_cannot_hand_off_because_a_handoff_never_changes_harness() {
        let plane = Plane::new();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_shell_chat(&held);
        let tickets = Tickets::default();
        let ticket = ticket(&held, &id, &tickets, asking);

        let said = answer(
            &held,
            &id,
            &tickets,
            1,
            an_open(asking, &ticket, stamped(asking)),
            &nothing_opens,
        );

        assert!(
            matches!(&said, Answer::No { why } if why.contains("not on a harness profile")),
            "{said:?}"
        );
    }
}
