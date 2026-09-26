//! The app's half of `charter handoff`: opening, in a window the operator is looking at, the
//! chat a chat of its own handed work to (charter-app#204).
//!
//! `charter handoff` does every check in front of the open. The operator's consent is the
//! harness's permission prompt in front of that exact command, and nothing here asks again:
//! the operator already answered with the brief on screen. The command then asks the app over
//! the hook socket, in two lines on one connection: a ticket, and the open that spends it.
//! `charter_core::hookwire::OpenChat` argues what that ticket is worth and what it is not,
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
//!   (`charter_core::handoff::is_stamped_from`), so the chat the operator finds on the strip
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

use charter_core::engine::Size;
use charter_core::hookwire::{Answer, Ask, OpenChat, Tickets};
use charter_core::reopen::{Chat, HandedFrom, Owed};

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
/// acceptable: see the justification on `charter_core::hookwire::OpenChat`.
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
                Ok(it) => {
                    let chat = it.session;
                    arrived(it);
                    Answer::Opened { chat }
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
/// `UserPromptSubmit` hook to hand its turn as context (`charter_core::handback`). A parent
/// that has closed gets neither; the report is kept for its workspace instead, and the next
/// chat to start there reads it.
fn report_it(held: &Held, chat: u32, summary: &str) -> Result<Answer, String> {
    use charter_core::handback::{self, For, Handback};

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
    let summary = charter_core::handoff::report_summary(summary).map_err(|bad| bad.say())?;
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
            held.hooks().board().state(from.chat),
            charter_core::state::State::Done | charter_core::state::State::Failed
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
            .unwrap_or_else(|| from.workspace.clone()),
        to: to.clone(),
        to_workspace: from.workspace.clone(),
        summary,
    };
    let whose = if parent_open {
        For::Chat(from.chat)
    } else {
        For::Workspace(&from.workspace)
    };
    handback::leave(held.root(), whose, &report)
        .map_err(|why| format!("the report could not be kept ({why})"))?;
    held.chats().owes(chat, Owed::Sent);
    if parent_open {
        held.reported_back(from.chat, &child_name);
    }
    Ok(Answer::Reported {
        to,
        kept_for: (!parent_open).then(|| from.workspace.clone()),
    })
}

/// The workspace a chat standing in `cwd` works in: the directory under the plane's
/// `workspaces/` it is in, where it is in one.
fn workspace_of(root: &std::path::Path, cwd: &std::path::Path) -> Option<String> {
    let inside = cwd.strip_prefix(root.join("workspaces")).ok()?;
    let first = inside.components().next()?.as_os_str().to_str()?;
    charter_core::contain::workspace_name_ok(first).then(|| first.to_owned())
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
fn open_it(held: &Held, plane: &PlaneId, open: &OpenChat, size: Size) -> Result<Arrived, String> {
    use charter_core::{handoff, start, wscmd};

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
    let left_from = stamp.workspace.to_owned();
    if !charter_core::contain::workspace_name_ok(&left_from) {
        return Err(format!(
            "the stamp names '{}' as the workspace the handoff left from, which cannot be one",
            charter_core::shown::short(&left_from)
        ));
    }
    // The command held the name to this rule already; held again because the request is what
    // arrived here, and a name is drawn on a tab.
    let label = match open.name.as_deref() {
        Some(raw) => charter_core::reopen::label(raw)?,
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
            charter_core::shown::short(ws)
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
            let plane_on_disk = charter_core::workspaces::Plane::open(root);
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
        },
        root,
    )
    .map_err(stays)?;
    let Some(first) = ready
        .harness
        .and_then(|harness| handoff::first_message_argv(harness.name(), &message))
    else {
        return Err(stays(format!(
            "profile '{profile}' runs a harness charter has not measured the first message of, \
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
        }),
        renamed_from: None,
    };
    let session = held
        .chats()
        .start_ready(&chat, &ready, size)
        .map_err(stays)?;
    Ok(Arrived {
        plane: plane.clone(),
        session,
        name,
        label,
        from: chat.from.as_ref().map(crate::HandedFromNote::from),
        workspace: ws.to_owned(),
        persona,
        harness: ready.harness.map(|harness| harness.name().to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use charter_core::hookwire::NO_TICKET;

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
            std::fs::write(root.join(charter_core::plane::MANIFEST), "").expect("charter.toml");
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
                root.join(charter_core::profiles::LOCAL_FILE),
                format!(
                    "[harness.work]\nkind = \"claude\"\ncommand = [{:?}]\n",
                    program.display().to_string()
                ),
            )
            .expect("the profile");
            let set = charter_core::profiles::current(&root);
            let work = set.get("work").expect("the profile reads");
            charter_core::profiletrust::record_launched(
                &root,
                "work",
                &charter_core::profiletrust::fingerprint(work),
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
        let ready = charter_core::start::ready(
            &charter_core::start::Start {
                profile: Some("work".to_owned()),
                persona: None,
                name: "1".to_owned(),
                cwd: Some(root.to_path_buf()),
                resume: None,
                show_footer: false,
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

        let Answer::Opened { chat } = said else {
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
        let ticket = ticket(held, id, tickets, asking);
        let told = Mutex::new(None);
        match answer(
            held,
            id,
            tickets,
            1,
            a_named_open(asking, &ticket, stamped(asking), name, report),
            &|arrived| *told.lock().unwrap() = Some(arrived),
        ) {
            Answer::Opened { chat } => Ok((chat, told.into_inner().unwrap().expect("told"))),
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
            Ask::Report(Box::new(charter_core::hookwire::ReportBack {
                chat: child,
                summary: summary.to_owned(),
                ticket,
            })),
            &nothing_opens,
        )
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
        charter_core::handoff::REPORT_ASK
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
            charter_core::handback::take(held.root(), charter_core::handback::For::Chat(asking));
        assert_eq!(waiting.len(), 1, "left for its next turn");
        assert_eq!(waiting[0].summary, "Dropped it.");
        assert_eq!(waiting[0].from, "drop commons");
        assert_eq!(waiting[0].from_workspace, "alpha");
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
        charter_core::hookwire::send(
            held.hooks().socket().expect("the plane listens"),
            &charter_core::hookwire::Report {
                chat: child,
                event: charter_core::state::Event::UserPromptSubmit,
                conversation: charter_core::hookwire::Conversation::Named(conversation),
                pid: Some(4242),
                detail: charter_core::state::Detail::default(),
            },
        )
        .expect("the prompt is sent");
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while held.hooks().board().state(child) != charter_core::state::State::Running
            && Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(
            held.hooks().board().state(child),
            charter_core::state::State::Running,
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
        let kept = charter_core::handback::take(
            held.root(),
            charter_core::handback::For::Workspace("default"),
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].to, "claude 1");
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
