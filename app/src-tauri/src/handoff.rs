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

use std::sync::Arc;
use std::time::Instant;

use charter_core::engine::Size;
use charter_core::hookwire::{Answer, Ask, OpenChat, Tickets};
use charter_core::reopen::Chat;

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
    /// The chat's name, which is what the tab is labelled.
    pub name: String,
    /// The workspace whose strip it is filed on.
    pub workspace: String,
    pub persona: Option<String>,
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
    }
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
    if !handoff::is_stamped_from(&open.message, &from.to_string()) {
        return Err(format!(
            "the first message does not open with the stamp of a handoff from chat {from}, \
             and a chat the app opens on request always says where it came from"
        ));
    }
    // The command asked this already; asked again because these bytes are about to become a
    // harness's argv, and the bound and the NUL are facts about argv.
    if let Some(bad) = handoff::bad_message(&open.message) {
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
    let name = format!("handoff from {from}");
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
        .and_then(|harness| handoff::first_message_argv(harness.name(), &open.message))
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
        number: None,
    };
    let session = held
        .chats()
        .start_ready(&chat, &ready, size)
        .map_err(stays)?;
    Ok(Arrived {
        plane: plane.clone(),
        session,
        name,
        workspace: ws.to_owned(),
        persona,
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
        Ask::Open(Box::new(OpenChat {
            chat,
            workspace: "alpha".to_owned(),
            create_vision: None,
            persona: None,
            message,
            ticket: ticket.to_owned(),
        }))
    }

    /// A plane with a workspace `alpha`, a profile `work` running a stand-in `claude` that
    /// writes down the arguments it was started with, and the operator's approval of it.
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
            let argv = root.join("argv");
            // Writes down every argument of every run, one to a line, so the chat's own run can
            // be read back.
            let program = stand_in::program(
                &root,
                "claude-stand-in",
                &format!(
                    "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done >> {argv:?}\n"
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

        fn argv(&self) -> String {
            std::fs::read_to_string(self.root.join("argv")).unwrap_or_default()
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
                name: format!("handoff from {asking}"),
                workspace: "alpha".to_owned(),
                persona: None,
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
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while !plane.argv().contains("⟨handoff from chat") && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            plane.argv().contains(&stamped(asking)),
            "the brief was not the chat's first message: {:?}",
            plane.argv()
        );
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
