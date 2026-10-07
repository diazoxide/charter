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
//! - **The asking chat is one this app has open**, and the new chat starts on **its persona's
//!   own profile** where that persona's definition names one, else on the asking chat's, read
//!   from the app's own record of that chat (#1445). Either way it is a profile the project
//!   already offers on this machine and has approved
//!   (`purlis_core::personaprofile::for_dispatch`): a persona's file is one a chat can write,
//!   so a name it gives is only looked up, never run. One this machine does not offer falls
//!   back to the asking chat's profile, and the answer and the new chat's stamp say so
//!   (D-1445-8). The harness follows the profile, so a Claude Code chat can hand work to a
//!   persona that runs on Codex, and the first message reaches it by that harness's own route.
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
//!
//! # A dispatched task (#1436)
//!
//! A **task** is the other mode of the same act (ADR 0090, #1434): one chat starting another,
//! which runs as a persona and owes the chat that asked one report. It is answered here, on the
//! same road: the ticket, the stamped first message, the chat's start, the lineage on its
//! record and the report. What differs is where the facts come from. A handoff's request
//! carries a stamp and a workspace, which this module checks; a dispatch's carries the chat's
//! number, a persona's name, a task's name and a brief, and **everything else is this app's own
//! record of the asking chat** ([`dispatch_it`]): who it is, where it works, what it runs as,
//! and whether it may. `purlis_core::dispatchdecision::decide` answers that last question, for
//! every caller.

use std::sync::Arc;
use std::time::Instant;

use purlis_core::active::Place;
use purlis_core::dispatchdecision::{self, Mode};
use purlis_core::engine::Size;
use purlis_core::hookwire::{Answer, Ask, DispatchAsk, OpenChat, Row, TaskReport, Tickets};
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
    /// The workspace whose strip it is filed on, or none for a task dispatched by a chat at the
    /// project's root, which is filed where that chat is.
    pub workspace: Option<String>,
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
                Ok((it, row, note)) => {
                    let chat = it.session;
                    arrived(it);
                    Answer::Opened {
                        chat,
                        row: Some(row),
                        note,
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
            report_it(held, back.chat, &back.summary, back.task.as_ref()).unwrap_or_else(no)
        }
        Ask::Dispatch(dispatch) => {
            // Spent first, for the open's reason: one run of the command starts one chat.
            if let Err(why) = tickets.spend(dispatch.chat, connection, &dispatch.ticket, now) {
                return no(why);
            }
            match dispatch_it(held, plane, &dispatch, STARTING) {
                Ok(Dispatched::Started(it)) => {
                    let answer = Answer::Dispatched {
                        chat: it.session,
                        name: it.label.clone().unwrap_or_else(|| it.name.clone()),
                        persona: it.persona.clone(),
                    };
                    arrived(*it);
                    answer
                }
                Ok(Dispatched::NeedsGrant { from, to }) => Answer::NeedsGrant { from, to },
                Err(why) => no(why),
            }
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
///
/// **A task's report** (#1436) is the same report with three more parts: its outcome and what
/// changed, which the persona chat says, and its session record's path, which is this app's
/// own record of what it wrote for that chat and never a path the chat named. It raises no
/// needs-you item: a task's report is for the chat that asked, not for the person.
fn report_it(
    held: &Held,
    chat: u32,
    summary: &str,
    task: Option<&TaskReport>,
) -> Result<Answer, String> {
    use purlis_core::handback;

    // From "is one owed" to "one was sent" under one lock, so two reports in flight are one
    // report and one refusal, and a restart of the chat that asked lands on one side of it.
    let _deciding = held.chats().deciding();
    let from = held.chats().handed_from(chat).ok_or_else(|| {
        if task.is_some() {
            format!(
                "chat {chat} was not started by a dispatch, so there is no chat waiting on a \
                 report from it"
            )
        } else {
            format!(
                "chat {chat} was not opened by a handoff, so there is no chat waiting on a \
                 report from it"
            )
        }
    })?;
    match (from.report, from.mode) {
        (Owed::Due, _) => {}
        (Owed::Nothing, _) => {
            return Err(format!(
                "the handoff that opened this chat did not ask for a report (it had no \
                 --report), so '{}' is not waiting on one",
                from.name
            ));
        }
        (Owed::Sent, Mode::Handoff) => {
            return Err(format!(
                "this chat has already reported back to '{}', and a handoff gets one report — \
                 already reported. Hand off again with --report for another",
                from.name
            ));
        }
        (Owed::Sent, Mode::Task) => {
            return Err(format!(
                "this chat has already reported to '{}', and a dispatched task gets one \
                 report — already reported",
                from.name
            ));
        }
        // purlis reported for it (#1443): its program had ended, or its tab was closed and
        // reopened, before it said a word. That was the task's one report.
        (Owed::Failed, _) => {
            return Err(format!(
                "purlis already told '{}' that this chat ended without a report, and a task \
                 gets one report — already reported. Say what you found to the person instead",
                from.name
            ));
        }
    }
    // **Which kind of report it is, is this app's record of how the chat was started**, never
    // what the line says. A handoff's report is its summary, whatever else the line carried.
    // A task's says how it ended, and one that does not is not a task's report.
    let task = match (from.mode, task) {
        (Mode::Handoff, _) => None,
        (Mode::Task, Some(said)) => Some(said),
        (Mode::Task, None) => {
            return Err(format!(
                "this chat was dispatched as a task, and a task's report says how it ended. \
                 Send it with `purlis dispatch report --outcome done \"<what you did and \
                 found>\"`, or --outcome blocked or failed; '{}' is waiting on it",
                from.name
            ));
        }
    };
    let summary = purlis_core::handoff::report_summary(summary).map_err(|bad| bad.say())?;
    // What changed is the chat's words too, and held to the rule its report is.
    let task = match task {
        None => None,
        Some(said) => Some(handback::Task {
            outcome: said.outcome,
            changed: match said.changed.as_deref() {
                None => None,
                Some(changed) => {
                    Some(purlis_core::handoff::report_summary(changed).map_err(|bad| bad.say())?)
                }
            },
            record: held
                .chats()
                .last_record(chat)
                .and_then(|path| handback::record_path(&path)),
            by_person: from.by_person,
            unreported: false,
            stopped: false,
        }),
    };
    let delivered = deliver(held, chat, &from, summary, task)?;
    held.chats().owes(chat, Owed::Sent);
    // A handoff's report is the person's to see, so it is a needs-you item on the chat that
    // asked. A task's is that chat's own to read, on its next turn (#1434).
    if delivered.reached_the_chat && from.mode == Mode::Handoff {
        held.reported_back(from.chat, &delivered.from);
    }
    Ok(Answer::Reported {
        to: delivered.to,
        kept_for: delivered.kept_for,
    })
}

/// Where a report went.
struct Delivered {
    /// The chat that reported, by the name the person sees it under.
    from: String,
    /// The chat it was for, by the name the person sees it under.
    to: String,
    /// The place it is kept for, where the chat it was for is gone: by `Place::word`, a
    /// workspace's name or the plane root's word (SI-1b).
    kept_for: Option<String>,
    /// Whether it waits for the asking chat's own next turn.
    reached_the_chat: bool,
}

/// Leaves a report from chat `chat` for the chat `from` names, or, where that chat is gone,
/// for the place it asked from: the one road every report takes, whoever wrote it — the chat
/// itself ([`report_it`]), or the app in its place ([`ended_without_report`]).
///
/// **A chat is reachable when its tab is open AND its program is still running**: one that has
/// ended will never fire the prompt its report waits for, so the report goes where the next
/// chat to start will read it, as it does for a chat that has closed.
fn deliver(
    held: &Held,
    chat: u32,
    from: &HandedFrom,
    summary: String,
    task: Option<purlis_core::handback::Task>,
) -> Result<Delivered, String> {
    use purlis_core::handback::{self, For, Handback};

    let chats = held.chats().open_now();
    let child = chats
        .iter()
        .find(|open| open.session == chat)
        .ok_or_else(|| format!("chat {chat} is not one this app has open"))?;
    let child_name = held
        .chats()
        .shown_name(chat)
        .unwrap_or_else(|| child.name.clone());
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
        task,
    };
    // **A task the person started, whose tab chat is gone, is the person's and nobody
    // else's** (D-1443-9): its report is not handed to whichever chat starts next in that
    // workspace, which never asked for it and would read it as its own business. It stays
    // with the persona chat, and that chat is marked as needing the person.
    if !parent_open && from.by_person {
        held.reported_back(chat, &child_name);
        return Ok(Delivered {
            from: child_name,
            to,
            kept_for: Some(handback::FOR_THE_PERSON.to_owned()),
            reached_the_chat: false,
        });
    }
    let whose = if parent_open {
        For::Chat(from.chat)
    } else {
        For::Place(&from.workspace)
    };
    handback::leave(held.root(), whose, &report)
        .map_err(|why| format!("the report could not be kept ({why})"))?;
    Ok(Delivered {
        from: child_name,
        to,
        // By `Place::word`: a workspace's name, or the plane root's word (SI-1b).
        kept_for: (!parent_open).then(|| from.workspace.word().to_owned()),
        reached_the_chat: parent_open,
    })
}

/// The hold [`crate::chats::Chats::deciding`] gives: proof, to the functions below, that the
/// caller is the one deciding. They read and then write the chats' records, and none of them
/// takes the lock itself, so one hold covers a whole close.
pub type Deciding<'a> = std::sync::MutexGuard<'a, ()>;

/// Why the app reports in a persona chat's place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreported {
    /// Its program ended on its own before it reported: `failed: ended without a report`.
    Ended,
    /// The person closed it, or stopped it with the chat that asked: `stopped by the operator`.
    Stopped,
}

/// **A persona chat that will not report is reported for** (#1443): chat `chat` still owed
/// its asking chat a task's report, and its program ended (`failed: ended without a report`)
/// or the person closed it (`stopped by the operator`). The app says so in its place, with
/// the path of the session record it wrote for that chat where it wrote one.
///
/// **Once, by nobody's word but the app's, and final** (D-1443-10). Under the caller's hold
/// of the lock a report is taken under, so the program's end, a close of its tab and the
/// chat's own report are one report between them. The record is marked only once the report
/// is kept: one that could not be written is still owed, and is tried again at the chat's
/// close. It goes where that chat's own report would have gone ([`deliver`]).
///
/// **Only for a program that ended on its own, or a close.** Not at a quit, not as the
/// project is let go of, not when every agent is stopped and not for a chat started again in
/// its own place: those chats are kept, and report when they run again. The callers hold to
/// that; this does what it is asked.
pub fn unreported(held: &Held, chat: u32, why: Unreported, _deciding: &Deciding<'_>) {
    use purlis_core::handback;

    let Some(from) = held.chats().owed_task_report(chat) else {
        return;
    };
    let record = held
        .chats()
        .last_record(chat)
        .and_then(|path| handback::record_path(&path));
    let (text, task) = match why {
        Unreported::Ended => (
            handback::UNREPORTED,
            handback::Task::unreported(record, from.by_person),
        ),
        Unreported::Stopped => (
            handback::STOPPED,
            handback::Task::stopped(record, from.by_person),
        ),
    };
    match deliver(held, chat, &from, text.to_owned(), Some(task)) {
        Ok(delivered) => {
            held.chats().owes(chat, Owed::Failed);
            tracing::info!(
                "purlis: chat {chat} {text}, so '{}' is told{}",
                delivered.to,
                delivered
                    .kept_for
                    .map(|place| format!(" (kept for {place}: that chat is gone)"))
                    .unwrap_or_default()
            );
        }
        Err(why) => tracing::warn!(
            "purlis: chat {chat} {text}, and its asking chat could not be told yet ({why})"
        ),
    }
}

/// **A persona chat's program ended on its own** (#1443): [`unreported`], as
/// [`Unreported::Ended`], called as the operating system says the process is gone. That is
/// the bound on how long an asking chat can wait on a task that died.
///
/// It waits for the deciding lock only while the chat still owes a report. A close holds that
/// lock while it ends the chat's program, and has settled what the chat owed before it does;
/// so the end of a program that a close is ending finds nothing owed and returns at once,
/// and never waits on the close that is waiting on it.
pub fn its_program_ended(held: &Held, chat: u32) {
    loop {
        if held.chats().owed_task_report(chat).is_none() {
            return;
        }
        if let Some(deciding) = held.chats().try_deciding() {
            unreported(held, chat, Unreported::Ended, &deciding);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

// ----------------------------------------------------------------------------------------
// a chat's persona chats: where each stands, and what closing their asking chat does (#1443)
// ----------------------------------------------------------------------------------------

/// Where a persona chat stands (`purlis_core::dispatchdecision::Standing`), as the window and
/// a chat's own list of its dispatches are told it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PersonaChatState {
    /// At work, and its report is still to come.
    Running,
    /// Stopped on something only you can answer, in its own tab: a permission prompt or a
    /// question.
    WaitingOnOperator,
    /// It sent its report, and stays open until it is closed.
    Reported,
    /// Its program ended before it reported, and the chat that asked was told it failed.
    Ended,
}

impl From<dispatchdecision::Standing> for PersonaChatState {
    fn from(standing: dispatchdecision::Standing) -> Self {
        use dispatchdecision::Standing;
        match standing {
            Standing::Running => Self::Running,
            Standing::WaitingOnOperator => Self::WaitingOnOperator,
            Standing::Reported => Self::Reported,
            Standing::Ended => Self::Ended,
        }
    }
}

/// One persona chat a chat dispatched as a task, as that chat sees it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct PersonaChat {
    pub session: u32,
    /// The name the person sees it under: its task's name.
    pub name: String,
    /// The persona it runs as.
    pub persona: Option<String>,
    pub state: PersonaChatState,
    /// [`PersonaChatState`] in the words a chat's list of its dispatches says: `running`,
    /// `waiting on the operator`, `reported`, `ended without a report`.
    pub said: String,
    /// Whether closing the chat that asked for it closes it too: it has reported, it is not
    /// at work, and its session record is written or can be asked for ([`close_reported`]).
    pub closes_with_its_asker: bool,
}

/// Where chat `chat`, a persona chat with lineage record `from`, stands: read from the app's
/// records of it and nothing it says.
///
/// **Waiting on the operator is the board's own word that it asks mid-turn** (a permission
/// prompt, a question), or a permission ask this app holds open for it. Its asking chat cannot
/// answer either, and reads this to know the wait is not its own to end.
fn standing_of(held: &Held, chat: u32, from: &HandedFrom) -> dispatchdecision::Standing {
    dispatchdecision::standing(from.report, !still_working(held, chat), asks(held, chat))
}

/// Whether chat `chat` is asking the person something mid-turn, or holds a permission ask
/// open: it cannot go on until they answer in its own tab.
fn asks(held: &Held, chat: u32) -> bool {
    held.board().glance(chat).asking || held.asks().asks.iter().any(|ask| ask.session == chat)
}

/// **The persona chats chat `asker` dispatched as tasks, each with where it stands** (#1443):
/// what that chat's own list of its dispatches reads, and what closing it says.
///
/// `asker` is the app's number for a chat it has open. The list is of the chats still open: a
/// persona chat that was closed is not one anybody waits on. A task the person started from
/// that chat's tab is listed too, by name and state, and that is all the list gives a chat of
/// it: who may steer a listed chat is not this function's to say.
pub fn persona_chats(held: &Held, asker: u32) -> Vec<PersonaChat> {
    held.chats()
        .tasks_of(asker)
        .into_iter()
        .map(|(session, from)| {
            let standing = standing_of(held, session, &from);
            PersonaChat {
                session,
                name: held
                    .chats()
                    .shown_name(session)
                    .unwrap_or_else(|| session.to_string()),
                persona: held
                    .chats()
                    .recorded_chat(session)
                    .and_then(|chat| chat.persona),
                state: standing.into(),
                said: standing.word().to_owned(),
                closes_with_its_asker: closes_with(held, session, &from) != ClosesWith::Stays,
            }
        })
        .collect()
}

/// Whether chat `chat`, started by another, is **at work**: what "Stop them" stops, and what
/// closing a chat above it asks about. Its program runs, and either it is a task that has not
/// reported, or it is mid-turn or asking the person something. A chat that reported and sits
/// idle, and a handoff's chat waiting for its next prompt, are not at work: each is a tab the
/// person can see, with nothing running unseen in it.
fn at_work(held: &Held, chat: u32, from: &HandedFrom) -> bool {
    if !still_working(held, chat) {
        return false;
    }
    let owes_a_report = from.mode == Mode::Task && from.report == Owed::Due;
    owes_a_report
        || held.board().glance(chat).state == purlis_core::state::State::Running
        || asks(held, chat)
}

/// Every chat below chat `asker` that is at work ([`at_work`]), deepest first: the chats it
/// started, as tasks or by a handoff, and the chats they started, through every chat on the
/// way whether or not that one is still at work. A chat that reported may have dispatched
/// before it did, and what it started is still below `asker`.
fn at_work_below(held: &Held, asker: u32) -> Vec<u32> {
    fn below(held: &Held, asker: u32, deeper: u32, seen: &mut Vec<u32>, found: &mut Vec<u32>) {
        if deeper == 0 {
            return;
        }
        for (chat, from) in held.chats().started_by(asker) {
            if seen.contains(&chat) {
                continue;
            }
            seen.push(chat);
            below(held, chat, deeper - 1, seen, found);
            if at_work(held, chat, &from) {
                found.push(chat);
            }
        }
    }
    let mut found = Vec::new();
    // The ceiling no chain passes, and each chat once: a record that names a loop ends.
    below(
        held,
        asker,
        dispatchdecision::DEEPEST,
        &mut vec![asker],
        &mut found,
    );
    found
}

/// The chats at work below chat `asker`, by the names the person sees them under, deepest
/// first: what closing `asker` asks about, keep them running or stop them.
pub fn running_below(held: &Held, asker: u32) -> Vec<String> {
    at_work_below(held, asker)
        .into_iter()
        .map(|chat| {
            held.chats()
                .shown_name(chat)
                .unwrap_or_else(|| chat.to_string())
        })
        .collect()
}

/// **Stops every chat at work below chat `asker`** (#1443): the person's "Stop them", asked
/// once as they close the asking chat. Answers every chat it closed.
///
/// Deepest first, so no chat is closed while one it started still runs under it. Each is
/// closed as a tab's Close closes it ([`Held::close_chat_held`]): a task's asking chat is told
/// it was stopped by the operator, so what was stopped is on record where the next chat reads
/// it. Under the caller's hold of the deciding lock, which is also what a dispatch is decided
/// under: no chat below can start another between the answer and the stop.
pub fn stop_below(held: &Held, asker: u32, deciding: &Deciding<'_>) -> Vec<u32> {
    let stopped = at_work_below(held, asker);
    for &chat in &stopped {
        if let Err(why) = held.close_chat_held(chat, deciding) {
            tracing::warn!(
                "purlis: chat {chat}, stopped with its asking chat, did not end cleanly ({why})"
            );
        }
    }
    stopped
}

/// What closing its asking chat does with a persona chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosesWith {
    /// It stays open: it has not reported, it is at work or asking the person something, or
    /// its session record is gone.
    Stays,
    /// It closes now, on the session record the app wrote for it.
    OnItsRecord(crate::smartclose::SavedRecord),
    /// It has no session record yet: it is asked for one, and closes when that is saved.
    OnceRecorded,
}

/// What closing its asking chat does with chat `chat`, a persona chat with lineage `from`.
///
/// **Only a chat that reported and is at rest closes with its asker.** One that reported and
/// was then given more to do (the person typed in it, a follow-up arrived) is working, and
/// its work is not ended because the chat that once asked it something has closed. And **one
/// whose saved record is gone is not closed**: the record is what a close leaves of it.
fn closes_with(held: &Held, chat: u32, from: &HandedFrom) -> ClosesWith {
    use purlis_core::state::State;

    if from.mode != Mode::Task || !matches!(from.report, Owed::Sent | Owed::Failed) {
        return ClosesWith::Stays;
    }
    let state = held.board().glance(chat).state;
    if state == State::Running || asks(held, chat) {
        return ClosesWith::Stays;
    }
    match held.chats().last_record(chat) {
        Some(path) => {
            match purlis_core::sessionrecord::saved(held.root(), &held.root().join(&path)) {
                Some(listed) => ClosesWith::OnItsRecord(crate::smartclose::SavedRecord {
                    path: listed.shown,
                    title: listed.title,
                }),
                None => ClosesWith::Stays,
            }
        }
        // Asked for its record, which only a chat whose program still runs can write.
        None if matches!(state, State::Done | State::Failed) => ClosesWith::Stays,
        None => ClosesWith::OnceRecorded,
    }
}

/// The persona chats chat `asker` dispatched that close with it, each with how
/// ([`closes_with`]): read before `asker` is closed, and carried out after ([`close_reported`]).
pub fn reported_by(held: &Held, asker: u32) -> Vec<(u32, ClosesWith)> {
    held.chats()
        .tasks_of(asker)
        .into_iter()
        .map(|(chat, from)| (chat, closes_with(held, chat, &from)))
        .filter(|(_, how)| *how != ClosesWith::Stays)
        .collect()
}

/// **Closes the persona chats in `reported`, whose asking chat has just closed, each once its
/// session record is written** (#1443).
///
/// One whose record the app has written closes now, and the window is told as it is told of a
/// Smart close that ended on its record. One with no record yet is smart-closed: it is asked
/// to write its record, and closes when that is saved. **None is closed without its record.**
pub fn close_reported(held: &Arc<Held>, reported: Vec<(u32, ClosesWith)>, deciding: &Deciding<'_>) {
    for (chat, how) in reported {
        match how {
            ClosesWith::Stays => {}
            ClosesWith::OnItsRecord(record) => {
                if let Err(why) = held.close_chat_held(chat, deciding) {
                    tracing::warn!(
                        "purlis: chat {chat}, closed with its asking chat, did not end cleanly \
                         ({why})"
                    );
                }
                held.tell_smart_closed(chat, Some(record));
            }
            ClosesWith::OnceRecorded => match crate::smartclose::begin(held, chat) {
                Ok(_) => tracing::info!(
                    "purlis: chat {chat} reported and its asking chat closed, so it is asked \
                     for its session record and closes on it"
                ),
                Err(why) => tracing::info!(
                    "purlis: chat {chat} reported and its asking chat closed; it stays open \
                     with no session record ({why})"
                ),
            },
        }
    }
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
) -> Result<(Arrived, Row, Option<String>), String> {
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
    // `persona=args.persona or ""` in Python, which is the frame's own default: a handed-off
    // chat does not inherit the asking chat's persona, it gets the one the operator named or
    // the plane's.
    let persona = open
        .persona
        .clone()
        .or_else(|| start::persona_for_a_new_chat(root));
    // **The profile is the persona's own, else the asking chat's** (#1445): the first from the
    // persona's definition, held to what the project offers; the second from this app's record
    // of the asking chat, never from the request. The request names none today.
    let on = profile_for(root, asking.profile.as_deref(), persona.as_deref())?;
    let profile = on.chosen.profile.clone();
    // Where the persona's own profile is not offered on this machine, the chat runs on the
    // asking chat's and is told so under its stamp; the asking chat is told in the answer
    // (D-1445-8).
    let note = on.chosen.note();
    let message = handoff::delivered_noting(&open.message, &parent, open.report, note.as_deref())
        .expect("the stamp was read a moment ago");
    // The command asked this already; asked again because these bytes are about to become a
    // harness's argv, and the bound and the NUL are facts about argv.
    if let Some(bad) = handoff::bad_message(&message) {
        return Err(bad.say());
    }
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
    let handed_from = HandedFrom {
        chat: from,
        name: parent,
        workspace: left_from,
        report: if open.report {
            Owed::Due
        } else {
            Owed::Nothing
        },
        // The work moved, so the chat opens as a tab.
        mode: Mode::Handoff,
        // One below the chat that asks, as a task is: a handoff is a dispatch too, and a chain
        // is as deep as its dispatches whichever kind each was.
        depth: (asking_chat.from.as_ref().map_or(0, |from| from.depth) + 1)
            .min(dispatchdecision::DEEPEST),
        by_person: false,
    };
    let arrived = start_on(
        held,
        plane,
        |name| start::Start {
            profile: Some(profile.clone()),
            persona: persona.clone(),
            name,
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
        &Opening {
            message: &message,
            label,
            from: handed_from,
            workspace: Some(ws.to_owned()),
            number: None,
        },
        // From the read the profile was chosen from: git is asked once, and the profile that
        // was judged is the one that runs.
        (&on.declared, &on.launch),
        size,
    )
    .map_err(stays)?;
    let row = handoff_row(root, held.config(), placement, created);
    Ok((arrived, row, note))
}

/// What a chat is opened on and filed as, whichever mode opened it.
struct Opening<'a> {
    /// Its first message, as it is sent: the stamp, then the brief.
    message: &'a str,
    /// The task's name, where it was given one.
    label: Option<String>,
    /// Its lineage, which rides its record.
    from: HandedFrom,
    /// [`Arrived::workspace`].
    workspace: Option<String>,
    /// The number it starts under, where one was dealt already: a task's, dealt as its slot
    /// was reserved. `None` deals one now.
    number: Option<u32>,
}

/// The project's profiles as one read of them: what a profile is chosen from and the chat is
/// then started from ([`purlis_core::start::ready_read`]).
type LaunchRead<'a> = (
    &'a purlis_core::harness_declaration::Declarations,
    &'a (
        purlis_core::profiles::ProfileSet,
        purlis_core::profiles::IgnoreCheck,
    ),
);

/// Starts a chat on `opening`'s first message and records it, for a handoff and a task alike:
/// a name no chat here has had, the start `start_of` makes under that name, the harness's own
/// way of taking a first message ([`told_first`]), and the record that keeps the lineage.
///
/// **The record keeps the profile's own words and not the brief**: a relaunch resumes the
/// conversation, and sending the brief a second time would be a message nobody sent twice.
fn start_on(
    held: &Held,
    plane: &PlaneId,
    start_of: impl FnOnce(String) -> purlis_core::start::Start,
    opening: &Opening<'_>,
    (declared, launch): LaunchRead<'_>,
    size: Size,
) -> Result<Arrived, String> {
    use purlis_core::start;

    // Its own name is a number no chat in this plane has had, dealt now so the chat can be
    // started under it: with no task name its tab says `<persona> <N>`, the ordinary default,
    // and four handoffs from one chat are four different tabs (charter-app#258).
    let number = opening
        .number
        .unwrap_or_else(|| held.chats().sessions().deal());
    let name = number.to_string();
    let start = start_of(name.clone());
    let ready = start::ready_read(&start, held.root(), declared, launch).and_then(|ready| {
        told_first(
            ready,
            start.profile.as_deref().unwrap_or_default(),
            opening.message,
        )
    })?;
    let chat = Chat {
        program: ready.program.clone(),
        args: Vec::new(),
        cwd: ready.cwd.clone(),
        name: name.clone(),
        resume: ready.session.clone(),
        active: false,
        profile: start.profile.clone(),
        persona: start.persona.clone(),
        show_footer: false,
        pinned: false,
        number: Some(number),
        label: opening.label.clone(),
        from: Some(opening.from.clone()),
        held: start.held.clone(),
        renamed_from: None,
        ..Default::default()
    };
    let session = held.chats().start_ready(&chat, &ready, size)?;
    Ok(Arrived {
        plane: plane.clone(),
        session,
        name,
        label: opening.label.clone(),
        from: chat
            .from
            .as_ref()
            .map(|from| crate::HandedFromNote::of(from, chat.has_tab())),
        workspace: opening.workspace.clone(),
        persona: start.persona,
        harness: ready.harness.map(|harness| harness.name().to_owned()),
    })
}

/// What became of a dispatch the app was asked for.
enum Dispatched {
    /// The persona chat is running. Boxed: it is the whole arrival, and the other answer is
    /// two names.
    Started(Box<Arrived>),
    /// Nothing was started: this pair of personas needs a dispatch grant from the person.
    NeedsGrant { from: Option<String>, to: String },
}

/// Dispatches the task `ask` describes, or says why not in a sentence the asking chat reads
/// (#1436).
///
/// **The asker is this app's record of the chat whose token the line carried, never the
/// request.** Its persona, its profile, its folder, the name the new chat is told it came
/// from, where it stands in its lineage: each is read here from [`crate::chats::Chats`]. The
/// request says which persona, what the task is called and the brief, and nothing else it says
/// is read ([`DispatchAsk`] has nowhere to say it).
///
/// **One decision** ([`dispatchdecision::decide`], asked through
/// [`dispatchdecision::asked_by_a_chat`], which builds the request from the record): start,
/// needs a grant, or refused. A grant is never in force here yet: how one is given is #1437's,
/// and until then another persona answers "needs a grant" and starts nothing.
///
/// **Decided and reserved under one lock** ([`crate::chats::Chats::deciding`]), so asks in
/// flight on other threads cannot each be let past a limit the other is about to fill. The
/// lock is not held while the chat starts.
///
/// **The new chat is the project's, not the asking chat's.** It starts on the asking chat's
/// profile, in its folder, and that is all it takes from it
/// ([`dispatchdecision::start_for`]): its sandbox is compiled for its own persona, and the
/// asking chat's per-chat grants, the grants it holds, its opt-out and whatever mode its
/// harness was switched to are not carried.
fn dispatch_it(
    held: &Held,
    plane: &PlaneId,
    ask: &DispatchAsk,
    size: Size,
) -> Result<Dispatched, String> {
    dispatch_from(
        held,
        plane,
        &Wanted {
            from: ask.chat,
            to: ask.to.as_deref(),
            name: &ask.name,
            brief: &ask.brief,
            by: By::Chat,
        },
        size,
    )
}

/// Who asked for a dispatch: the one fact the two roads to [`dispatch_from`] differ in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum By {
    /// The chat itself, over the hook socket (`purlis dispatch`, the `dispatch` tool).
    Chat,
    /// The person, from that chat's tab in the window (#1438).
    Person,
}

/// One dispatch, as either road asks for it.
struct Wanted<'a> {
    /// The asking chat: the one whose token the line carried, or whose tab the person used.
    from: u32,
    /// The persona asked for, or none for the asking chat's own.
    to: Option<&'a str>,
    /// What the task is called.
    name: &'a str,
    /// The brief a chat sent, or what the person typed.
    brief: &'a str,
    by: By,
}

/// Dispatches the task `wanted` describes: the body of [`dispatch_it`], and of the person's
/// own dispatch ([`ask_persona`]).
///
/// **The person's differs in three things, and each is said to the chats that read it**: the
/// decision is asked as [`dispatchdecision::Asker::Person`], which needs no grant, because the
/// person is the one a grant is asked of; the first message says the person typed the request
/// (`purlis_core::handoff::person_task_message`); and the new chat's record says the person
/// started it, which its report then says to the chat it goes to. Everything else is the same
/// road: the same checks, the same limits, and the same start, which takes the asking chat's
/// profile and folder and nothing else of it.
fn dispatch_from(
    held: &Held,
    plane: &PlaneId,
    ask: &Wanted<'_>,
    size: Size,
) -> Result<Dispatched, String> {
    use purlis_core::dispatchdecision::{Decision, Grant, Limits};
    use purlis_core::{handoff, start};

    let root = held.root();
    let from = ask.from;
    let not_open = || format!("chat {from} is not one this app has open");
    let asking = held.chats().recorded_chat(from).ok_or_else(not_open)?;
    // The asking chat as the person sees it, which is what the new chat is told (never its
    // number), and a copy, so the note still reads once that chat is closed.
    let asker = held.chats().shown_name(from).ok_or_else(not_open)?;
    // Held to a task's own rule. The command asked already; asked again because the request
    // is what arrived here, and this name is drawn in a tree and on purlis's own lines.
    let label = dispatchdecision::task_name(ask.name)?;
    // Where the asking chat works, from this app's record of it: what the stamp says, and
    // where the report goes when that chat is gone.
    let workspace = asking
        .cwd
        .as_deref()
        .and_then(|cwd| workspace_of(root, cwd));
    let place = workspace.clone().map_or(Place::PlaneRoot, Place::Workspace);
    let when = chrono::Local::now().naive_local();
    let message = match ask.by {
        By::Chat => handoff::task_message(&asker, &place, when, ask.brief),
        By::Person => handoff::person_task_message(&asker, &place, when, ask.brief),
    };
    // The command measured this already, with a name standing in for the one written here;
    // measured again because these bytes are about to become a harness's argv.
    if let Some(bad) = handoff::bad_message(&message) {
        return Err(bad.say());
    }

    // The persona a new chat adopts by default, which a chat that names none runs as.
    let default = start::persona_for_a_new_chat(root);
    // **Decided, and its slot reserved, under one lock.** Asks arrive a thread each, and a
    // start takes seconds: two dispatches that each read the counts before either chat was
    // open would both be let past a limit. So the decision and the slot that makes it count
    // are one step, and the lock is let go before anything starts.
    let (to, lineage_of_it, number, _slot) = {
        let _deciding = held.chats().deciding();
        let lineage = held
            .chats()
            .lineage(from, default.as_deref(), &|chat| still_working(held, chat));
        let asked = match (ask.by, ask.to) {
            (By::Person, Some(named)) => dispatchdecision::asked_by_the_person(
                root,
                &asking,
                named,
                default.as_deref(),
                &lineage,
                limits_in_force(
                    root,
                    workspace.as_deref(),
                    asking.persona.as_deref().or(default.as_deref()),
                    Some(named.trim()),
                ),
            ),
            // The person's ask always names a persona ([`ask_persona`]); one that named none
            // is asked as the chat's own would be, which is never more than the chat may do.
            (By::Chat, named) | (By::Person, named @ None) => dispatchdecision::asked_by_a_chat(
                root,
                &asking,
                named,
                default.as_deref(),
                &lineage,
                Grant::Missing,
                Limits::default(),
            ),
        };

        match asked.decision {
            Decision::Start => {}
            Decision::NeedsGrant { from, to } => return Ok(Dispatched::NeedsGrant { from, to }),
            Decision::Refused(why) => {
                return Err(match ask.by {
                    By::Chat => why.say(),
                    By::Person => said_to_the_person(&why),
                });
            }
        }
        let its_lineage = HandedFrom {
            chat: from,
            name: asker,
            workspace: place,
            report: Owed::Due,
            mode: Mode::Task,
            depth: asked.depth,
            by_person: ask.by == By::Person,
        };
        // Its number is dealt here, so the slot is the chat it is about to be.
        let number = held.chats().sessions().deal();
        let slot = held.chats().reserve(
            number,
            Chat {
                name: number.to_string(),
                profile: asking.profile.clone(),
                persona: asked.to.clone(),
                from: Some(its_lineage.clone()),
                ..Default::default()
            },
        );
        (asked.to, its_lineage, number, slot)
    };
    // The project's profiles, read once for the start.
    let declared = purlis_core::harness_declaration::read(root);
    let launch = purlis_core::profiles::for_launch_in(root, &declared);
    // The slot is let go when this returns: the chat is open by then and counts for itself,
    // or its start was refused and nothing does.
    let arrived = start_on(
        held,
        plane,
        |name| dispatchdecision::start_for(&asking, to.clone(), name),
        &Opening {
            message: &message,
            label: Some(label),
            from: lineage_of_it,
            workspace,
            number: Some(number),
        },
        (&declared, &launch),
        size,
    )?;
    Ok(Dispatched::Started(Box::new(arrived)))
}

/// Whether chat `chat`'s program still runs, by the board: one that has ended, with or
/// without a report, owes no more work and is not counted against a limit (D-1436-18).
fn still_working(held: &Held, chat: u32) -> bool {
    !matches!(
        held.board().glance(chat).state,
        purlis_core::state::State::Done | purlis_core::state::State::Failed
    )
}

/// The profile a handed-off chat starts on, with the one read of the project's profiles it
/// was chosen from, which is the read the chat is then started from.
struct On {
    chosen: purlis_core::personaprofile::Chosen,
    declared: purlis_core::harness_declaration::Declarations,
    launch: (
        purlis_core::profiles::ProfileSet,
        purlis_core::profiles::IgnoreCheck,
    ),
}

/// **The profile a chat handed to `persona` starts on** (#1445): the persona's own where its
/// definition names one, else `asking`, the profile of the chat that asked as this app
/// recorded it. Held to the profiles the project offers on this machine, approved
/// (`purlis_core::personaprofile::for_dispatch`); the refusal is its sentence. A persona's own
/// profile this machine does not offer falls back to `asking`, and the answer says so
/// (D-1445-8).
///
/// The request names no profile today, so that function's first rule has nothing to answer.
fn profile_for(
    root: &std::path::Path,
    asking: Option<&str>,
    persona: Option<&str>,
) -> Result<On, String> {
    use purlis_core::personaprofile;
    let declared = purlis_core::harness_declaration::read(root);
    let launch = purlis_core::profiles::for_launch_in(root, &declared);
    let chosen = personaprofile::for_dispatch(
        &persona
            .map(|who| personaprofile::named_by(root, who))
            .unwrap_or_default(),
        asking,
        None,
        &personaprofile::offers_of(root, &launch.0, &declared),
    )
    .map_err(|refused| refused.say())?;
    Ok(On {
        chosen,
        declared,
        launch,
    })
}

/// `ready` with `message` as the chat's first message, **by the route the harness it runs
/// takes one** (`purlis_core::handoff::first_message_argv`): the harness is the one `profile`
/// names, whatever the asking chat runs, so nothing here knows which harness asked.
///
/// Last on the line: a positional prompt is what nothing may come after, and the app's own
/// hook arguments go in FRONT of these (`Chats::open_it`).
fn told_first(
    mut ready: purlis_core::start::Ready,
    profile: &str,
    message: &str,
) -> Result<purlis_core::start::Ready, String> {
    let Some(first) = ready
        .harness
        .and_then(|harness| purlis_core::handoff::first_message_argv(harness.name(), message))
    else {
        return Err(format!(
            "profile '{profile}' runs a harness purlis has not measured the first message of, \
             so it cannot be started on the brief."
        ));
    };
    ready.args.extend(first);
    Ok(ready)
}

// ----------------------------------------------------------------------------------------
// the person's own dispatch, from a chat's tab (#1438)
// ----------------------------------------------------------------------------------------

/// What "Ask <persona>…" offers on the chats of one project (#1438): the personas it can ask,
/// or why it offers none, and the asks policy takes off one chat's tab.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AskOffer {
    /// Every persona the project defines that is finished, by name, in order. A draft runs no
    /// chat (`purlis_core::dispatchdecision::Refused::Draft`), so it is not offered one.
    pub personas: Vec<String>,
    /// Why nothing is offered, where an administrator's policy locks all dispatch: what is
    /// locked, and who set it. The action is then absent, and the palette says this.
    pub locked: Option<String>,
    /// The asks policy locks for one chat: a pair of personas no chat dispatches across,
    /// where that chat runs as the first and the ask is to the second. That ask is not on
    /// that chat's tab, and the palette's row says why.
    pub locked_for: Vec<AskLocked>,
}

/// One ask policy takes off one chat's tab ([`AskOffer::locked_for`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AskLocked {
    /// The chat whose tab it is.
    pub session: u32,
    /// The persona that chat's tab does not ask.
    pub persona: String,
    /// What policy forbids, and who set it.
    pub why: String,
}

/// What "Ask <persona>…" offers in the project at `root`, under `locks`, on `chats`: each
/// open chat by number, with the persona it runs as.
///
/// **Policy binds the person here as it does everywhere else** (D-1438-9). A lock on all
/// dispatch takes the action away. A locked pair takes "Ask B…" off the tabs of chats running
/// as A: what the ask does that a chat opened from the picker does not is send B's report into
/// A's next turn, and that pairing is what the lock is for. The person still starts a chat as
/// B from the picker, where nothing flows to A.
pub fn ask_offer(
    root: &std::path::Path,
    locks: &purlis_core::sandbox::policy::Locks,
    chats: &[(u32, Option<String>)],
) -> AskOffer {
    if locks.forbids_dispatch() {
        return AskOffer {
            personas: Vec::new(),
            locked: locks.dispatch_refused(None, ""),
            locked_for: Vec::new(),
        };
    }
    let personas: Vec<String> = purlis_core::workspaces::Plane::open(root)
        .personas()
        .unwrap_or_default()
        .into_iter()
        .filter(|name| !purlis_core::personaverbs::is_draft(root, name))
        .collect();
    let locked_for = chats
        .iter()
        .flat_map(|(session, runs_as)| {
            personas.iter().filter_map(|persona| {
                Some(AskLocked {
                    session: *session,
                    persona: persona.clone(),
                    why: locks.dispatch_refused(runs_as.as_deref(), persona)?,
                })
            })
        })
        .collect();
    AskOffer {
        personas,
        locked: None,
        locked_for,
    }
}

/// What the person typed into "Ask <persona>…" on a chat's tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonAsk {
    /// The chat whose tab it was: the new chat starts under it, and reports to it.
    pub chat: u32,
    /// The persona to ask.
    pub persona: String,
    /// What the task is called.
    pub name: String,
    /// What to ask.
    pub ask: String,
}

/// **The person dispatches to a persona from a chat's tab** (#1438): a persona chat starts
/// under chat `asked.chat`, running as `asked.persona`, on what the person typed.
///
/// **A window command, and only that.** [`answer`], which is everything a hook line can reach,
/// has no arm that comes here: [`Ask`] has no variant for the person's dispatch, so a chat
/// cannot name the person as its asker however it writes its line. That is the whole of why no
/// grant is needed. The person is the one a grant is asked of, and the press is theirs.
///
/// **It starts as that persona and holds nothing of the chat it was launched from**
/// ([`dispatchdecision::start_for`]): the project's sandbox for that persona, its hosts and
/// its vaults. So "Ask devops…" on a steward chat's tab starts a chat a brokered `secret exec`
/// answers as devops, with no Notice and no handoff brief.
///
/// The limits in force and every other check of [`dispatchdecision::decide`] hold for the
/// person too, and so does policy: where `locks` forbid all dispatch, or the pair from the
/// persona that chat runs as to the one asked ([`ask_offer`]), nothing starts, whatever the
/// window drew. A refusal is said to the person, in the window's words ([`said_to_the_person`]).
pub fn ask_persona(
    held: &Held,
    plane: &PlaneId,
    asked: &PersonAsk,
    locks: &purlis_core::sandbox::policy::Locks,
    size: Size,
) -> Result<Arrived, String> {
    let persona = asked.persona.trim();
    if persona.is_empty() {
        return Err("Choose the persona to ask.".to_owned());
    }
    let runs_as = runs_as(held, asked.chat);
    if let Some(why) = locks.dispatch_refused(runs_as.as_deref(), persona) {
        return Err(why);
    }
    if asked.ask.trim().is_empty() {
        return Err("Write what to ask, so the new chat has something to start on.".to_owned());
    }
    let wanted = Wanted {
        from: asked.chat,
        to: Some(persona),
        name: &asked.name,
        brief: &asked.ask,
        by: By::Person,
    };
    match dispatch_from(held, plane, &wanted, size)? {
        Dispatched::Started(arrived) => Ok(*arrived),
        // The decision never asks the person for a grant of their own dispatch.
        Dispatched::NeedsGrant { .. } => {
            Err("purlis did not start the chat: it asked for a grant you do not need.".to_owned())
        }
    }
}

/// The persona chat `chat` runs as, by the app's record of it: its own, else the one a new
/// chat adopts by default. `None` for a chat on none, or one not open.
fn runs_as(held: &Held, chat: u32) -> Option<String> {
    held.chats()
        .recorded_chat(chat)?
        .persona
        .or_else(|| purlis_core::start::persona_for_a_new_chat(held.root()))
}

/// Why the person's ask starts nothing, **said to the person**. [`Refused::say`] is written
/// for the chat that asked ("say in your report what is left", "ask the person to raise the
/// limit"), and the person reading the dialog is not that chat: each refusal says what is
/// true of the chat whose tab they asked from, and what they can do.
///
/// [`Refused::say`]: dispatchdecision::Refused::say
fn said_to_the_person(why: &dispatchdecision::Refused) -> String {
    use dispatchdecision::Refused;
    match why {
        Refused::NoProfile => "This tab is not on a harness profile, so there is no harness to \
                               start the new chat on. Ask from a chat that was started on a \
                               profile."
            .to_owned(),
        Refused::NoPersona(name) => format!(
            "This project has no persona '{}' that loads.",
            purlis_core::shown::short(name)
        ),
        Refused::Draft(name) => format!(
            "Persona '{}' is still a draft, and a draft persona runs no chat. Finish its \
             definition first.",
            purlis_core::shown::short(name)
        ),
        Refused::Loop(name) => format!(
            "Persona '{}' is already above this chat in its own chain of chats, and a chain \
             never goes back to a persona above it. Ask from a chat that persona did not start.",
            purlis_core::shown::short(name)
        ),
        Refused::TooDeep { limit: 0 }
        | Refused::TooManyRunning { limit: 0 }
        | Refused::LineageFull { limit: 0 } => "Dispatch is switched off here: a limit it runs \
                                                under is set to 0. You can change it in \
                                                Settings › Project › Dispatch."
            .to_owned(),
        Refused::TooDeep { limit } => format!(
            "This chat is {limit} chats below the one you started, which is as deep as a \
             chain goes here. Ask from a chat higher up, or raise the depth in Settings › \
             Project › Dispatch."
        ),
        Refused::TooManyRunning { limit } => format!(
            "This chat already has {limit} persona chats that have not reported, which is as \
             many as it may have at once. Close one or wait for one to report, or raise the \
             limit in Settings › Project › Dispatch."
        ),
        Refused::LineageFull { limit } => format!(
            "This chat's chain already holds {limit} chats that have not reported, which is \
             as many as it may hold. Close one or wait for one to report, or raise the limit \
             in Settings › Project › Dispatch."
        ),
        // Never the person's: a helper is not a tab, a held chat's tab may ask, and the
        // person's ask always names a persona. Said as the chat is told, should one arise.
        Refused::Helper | Refused::Held | Refused::NoPersonaNamed => why.say(),
    }
}

/// **The limits in force for the person's ask** from a chat working in `workspace` and running
/// as `asking`, to `target` (#1439, #1440): the project's, the workspace's and the personas',
/// lowered by this machine's own and by policy's ceiling. A chat running as `asking` may also
/// be capped in how many it dispatches, which is one more bound on what it has running.
fn limits_in_force(
    root: &std::path::Path,
    workspace: Option<&str>,
    asking: Option<&str>,
    target: Option<&str>,
) -> dispatchdecision::Limits {
    let limits = purlis_core::dispatchlimits::of(root, workspace, asking, target);
    dispatchdecision::Limits {
        running: limits
            .may_dispatch
            .map_or(limits.running, |cap| cap.min(limits.running)),
        lineage: limits.lineage,
        depth: limits.depth,
    }
}

/// What "Ask <persona>…" offers on this project's chats: its finished personas, or why it
/// offers none, and the asks policy takes off one chat's tab.
// Its plane is a `PlaneId` the registry vouches for. On a blocking thread: it reads each
// persona's definition (SC-2).
#[tauri::command]
#[specta::specta]
pub async fn ask_persona_offer(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: PlaneId,
) -> Result<AskOffer, String> {
    let held = planes.held(&plane)?;
    tauri::async_runtime::spawn_blocking(move || {
        let default = purlis_core::start::persona_for_a_new_chat(held.root());
        let chats: Vec<(u32, Option<String>)> = held
            .chats()
            .open_now()
            .into_iter()
            .map(|open| (open.session, open.persona.or_else(|| default.clone())))
            .collect();
        ask_offer(
            held.root(),
            &purlis_core::sandbox::policy::Locks::of(held.root()),
            &chats,
        )
    })
    .await
    .map_err(|err| format!("purlis could not read this project's personas: {err}"))
}

/// Ask `persona` from chat `session`'s tab: starts a chat as that persona, under that chat, on
/// what you typed. Its report goes to that chat, marked as started by you. Answers the new
/// chat's number; the window is told of it as it is told of any chat another chat started.
// On a blocking thread, as `start_chat` is: a chat on a profile resolves its launch.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn ask_persona_chat(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: PlaneId,
    session: u32,
    persona: String,
    name: String,
    ask: String,
    columns: u16,
    rows: u16,
) -> Result<u32, String> {
    let held = planes.held(&plane)?;
    let asked = PersonAsk {
        chat: session,
        persona,
        name,
        ask,
    };
    let arrived = tauri::async_runtime::spawn_blocking(move || {
        let locks = purlis_core::sandbox::policy::Locks::of(held.root());
        ask_persona(&held, &plane, &asked, &locks, Size { columns, rows })
    })
    .await
    .map_err(|err| format!("purlis could not start the chat: {err}"))??;
    let started = arrived.session;
    planes.arrived(arrived);
    Ok(started)
}

/// What closing chat `session` would do with the chats below it: what the close dialog says
/// before it is answered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct ClosingChat {
    /// The persona chats it dispatched as tasks, each with where it stands.
    pub tasks: Vec<PersonaChat>,
    /// Every chat below it that is at work, by name, deepest first: its tasks that have not
    /// reported, the chats it handed work to that are mid-turn, and the same below those. With
    /// any, the close asks once: keep them running, or stop them.
    pub running: Vec<String>,
}

/// What closing chat `session` would do with the chats below it: its persona chats, each with
/// where it stands and whether it closes too, and every chat below it that is at work.
// Its plane is a `PlaneId` the registry vouches for.
#[tauri::command]
#[specta::specta]
pub fn persona_chats_of(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<ClosingChat, String> {
    let held = planes.held(&plane)?;
    Ok(ClosingChat {
        tasks: persona_chats(&held, session),
        running: running_below(&held, session),
    })
}

/// What a close that stops the chats below does next with the chat itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ThenClose {
    /// Close it now.
    Close,
    /// Smart close it: it is asked for its session record, and closes when that is saved.
    SmartClose,
}

/// **Stop them**, your answer to what closing chat `session` asks: every chat at work below
/// it is ended, deepest first, and then it is closed (`then: close`), in one step, so it
/// cannot start another in between. For a Smart close (`then: smart_close`) the chats below
/// are ended now, and any it starts while it writes its record are ended as it closes.
/// Answers every chat this closed. A stopped task's asking chat is told it was stopped by
/// the operator.
// On a blocking thread: ending a program waits for it to go.
#[tauri::command]
#[specta::specta]
pub async fn close_chat_stopping(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: PlaneId,
    session: u32,
    then: ThenClose,
) -> Result<Vec<u32>, String> {
    let held = planes.held(&plane)?;
    tauri::async_runtime::spawn_blocking(move || {
        held.close_chat_stopping(session, then == ThenClose::Close)
    })
    .await
    .map_err(|err| format!("purlis could not stop those chats: {err}"))
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
    use crate::host::pretend::Pretend;
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

    impl Plane {
        /// A persona `ops` whose definition names the profile `cx`, and that profile: a
        /// stand-in `codex`, approved, which writes its arguments down as `run.codex.<pid>`.
        fn with_a_codex_persona(self) -> Self {
            let runs = self.root.join("runs");
            let program = stand_in::program(
                &self.root,
                "codex-stand-in",
                &format!(
                    "#!/bin/sh\nat=$(mktemp {runs:?}/.writing.XXXXXX) || exit 1\n\
                     for a in \"$@\"; do printf '%s\\0' \"$a\"; done > \"$at\"\n\
                     mv \"$at\" {runs:?}/run.codex.$$\n\
                     sleep 10\n"
                ),
            );
            let local = self.root.join(purlis_core::profiles::LOCAL_FILE);
            let mut text = std::fs::read_to_string(&local).expect("the local file");
            text.push_str(&format!(
                "[harness.cx]\nkind = \"codex\"\ncommand = [{:?}]\n",
                program.display().to_string()
            ));
            std::fs::write(&local, text).expect("the profile");
            let set = purlis_core::profiles::current(&self.root);
            purlis_core::profiletrust::record_launched(
                &self.root,
                "cx",
                &purlis_core::profiletrust::fingerprint(set.get("cx").expect("cx reads")),
            )
            .expect("approved");
            self.a_persona("ops", "profile: cx\n")
        }

        /// A persona `name`, whose definition holds `frontmatter` under its name.
        fn a_persona(self, name: &str, frontmatter: &str) -> Self {
            let dir = self.root.join("personas").join(name);
            std::fs::create_dir_all(&dir).expect("the persona's folder");
            std::fs::write(
                dir.join("persona.md"),
                format!("---\nname: {name}\nvault: none\n{frontmatter}---\n\n# {name}\n"),
            )
            .expect("the definition");
            self
        }

        /// The argv the stand-in `codex` was started with, once it has written it whole.
        fn codex_run(&self) -> Vec<String> {
            let run = || {
                std::fs::read_dir(self.root.join("runs"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .find(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("run.codex.")
                    })
                    .map(|entry| {
                        let written = std::fs::read_to_string(entry.path()).unwrap_or_default();
                        written
                            .split_terminator('\0')
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
            };
            let deadline = Instant::now() + std::time::Duration::from_secs(30);
            while run().is_none() && Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            run().unwrap_or_default()
        }
    }

    /// A handoff from `asking` to `persona`, answered.
    fn hand_off_to(
        held: &Held,
        id: &PlaneId,
        tickets: &Tickets,
        asking: u32,
        persona: &str,
    ) -> Result<(u32, Arrived), String> {
        let ticket = ticket(held, id, tickets, asking);
        let told = Mutex::new(None);
        let open = Ask::Open(Box::new(OpenChat {
            chat: asking,
            workspace: "alpha".to_owned(),
            create_vision: None,
            persona: Some(persona.to_owned()),
            message: stamped(asking),
            ticket,
            name: None,
            report: true,
        }));
        match answer(held, id, tickets, 1, open, &|arrived| {
            *told.lock().unwrap() = Some(arrived)
        }) {
            Answer::Opened { chat, .. } => Ok((chat, told.into_inner().unwrap().expect("told"))),
            Answer::No { why } => Err(why),
            other => panic!("opened or refused, not {other:?}"),
        }
    }

    /// #1445, at the start seam, with no chat running: a Claude Code chat's handoff to a
    /// persona whose profile is Codex resolves to that profile, and the start it is given
    /// runs the Codex program with the stamped brief the way Codex takes a first message.
    #[test]
    fn the_start_for_a_codex_persona_runs_codex_on_the_brief_whatever_harness_asked() {
        let plane = Plane::new().with_a_codex_persona();
        let root = &plane.root;

        let on = profile_for(root, Some("work"), Some("ops")).expect("a profile");
        let profile = on.chosen.profile.clone();
        assert_eq!(profile, "cx", "the persona's own, not the asking chat's");
        assert_eq!(on.chosen.note(), None, "nothing fell back");
        assert_eq!(
            profile_for(root, Some("work"), None)
                .map(|on| on.chosen.profile)
                .as_deref(),
            Ok("work"),
            "no persona, so the asking chat's"
        );

        let message = purlis_core::handoff::delivered(&stamped(1), "claude 1", true)
            .expect("a stamped message");
        let ready = purlis_core::start::ready_read(
            &purlis_core::start::Start {
                profile: Some(profile.clone()),
                persona: Some("ops".to_owned()),
                name: "2".to_owned(),
                cwd: Some(root.join("workspaces").join("alpha")),
                ..Default::default()
            },
            root,
            &on.declared,
            &on.launch,
        )
        .and_then(|ready| told_first(ready, &profile, &message))
        .expect("the chat may start");

        assert_eq!(ready.harness, Some(purlis_core::harness::Harness::Codex));
        assert!(
            ready.program.ends_with("codex-stand-in"),
            "the Codex profile's own program: {}",
            ready.program
        );
        assert_eq!(
            ready.args.last(),
            Some(&message),
            "Codex takes its first message as its last, positional argument"
        );
        assert!(
            message.starts_with("⟨handoff from claude 1 · workspace default · ")
                && message.contains(purlis_core::handoff::REPORT_ASK)
                && message.ends_with("# Ship it\nnow"),
            "{message:?}"
        );
        assert!(
            !ready.args.iter().any(|word| word == "--prompt"),
            "{:?}",
            ready.args
        );
    }

    /// D-1445-8, at the same seam: a persona's own profile this machine does not offer falls
    /// back to the asking chat's, and the new chat reads so under its stamp. What the
    /// definition names is never run. A profile nobody approved is still a refusal.
    #[test]
    fn the_start_for_a_persona_falls_back_from_an_unoffered_profile_and_refuses_an_unapproved_one()
    {
        let plane = Plane::new()
            .with_a_codex_persona()
            .a_persona("rogue", "profile: /bin/sh -c evil\n");
        let on = profile_for(&plane.root, Some("work"), Some("rogue")).expect("it falls back");
        assert_eq!(on.chosen.profile, "work", "the asking chat's profile");
        let note = on.chosen.note().expect("and says so");
        assert!(
            note.starts_with("persona 'rogue' names profile '/bin/sh -c evil'")
                && note.ends_with("runs on the asking chat's profile, 'work'"),
            "{note}"
        );
        let message =
            purlis_core::handoff::delivered_noting(&stamped(1), "claude 1", false, Some(&note))
                .expect("a stamped message");
        let mut lines = message.lines();
        assert!(
            lines
                .next()
                .unwrap()
                .starts_with("⟨handoff from claude 1 · ")
        );
        assert_eq!(lines.next(), Some(format!("⟨{note}⟩").as_str()));
        assert!(message.ends_with("\n\n# Ship it\nnow"), "{message:?}");
        // No profile to fall back to: nothing is started.
        let refused = profile_for(&plane.root, None, Some("rogue"))
            .map(|on| on.chosen)
            .unwrap_err();
        assert!(
            refused.starts_with("persona 'rogue' names profile")
                && refused.contains("nothing was started"),
            "{refused}"
        );

        let local = plane.root.join(purlis_core::profiles::LOCAL_FILE);
        let text = std::fs::read_to_string(&local).expect("the local file");
        std::fs::write(
            &local,
            format!("{text}env = {{ CODEX_HOME = \"/elsewhere\" }}\n"),
        )
        .expect("changed");
        let refused = profile_for(&plane.root, Some("work"), Some("ops"))
            .map(|on| on.chosen)
            .unwrap_err();
        assert!(refused.contains("has not been approved"), "{refused}");
    }

    /// #1445: the chat a persona is handed work in starts on that persona's own profile, on
    /// whatever harness it runs. The asking chat is Claude Code; the persona's profile is
    /// Codex; the stamp and the brief reach Codex the way Codex takes a first message, as its
    /// last, positional argument.
    #[test]
    fn a_claude_chat_hands_off_to_a_persona_on_a_codex_profile_and_codex_gets_the_brief() {
        let plane = Plane::new().with_a_codex_persona();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();

        let (chat, arrived) =
            hand_off_to(&held, &id, &tickets, asking, "ops").expect("the handoff opens");

        assert_eq!(arrived.harness.as_deref(), Some("codex"));
        assert_eq!(arrived.persona.as_deref(), Some("ops"));
        let opened = held
            .chats()
            .open_now()
            .into_iter()
            .find(|open| open.session == chat)
            .expect("the new chat is one the app has open");
        assert_eq!(
            opened.profile.as_deref(),
            Some("cx"),
            "the persona's own profile, not the asking chat's"
        );
        let argv = plane.codex_run();
        let first = argv.last().cloned().unwrap_or_default();
        assert!(
            first.starts_with("⟨handoff from claude 1 · workspace default · 2026-05-04 11:32⟩\n")
                && first.ends_with("\n\n# Ship it\nnow"),
            "the stamp and the brief were not Codex's first message: {argv:?}"
        );
        assert!(
            first.contains(purlis_core::handoff::REPORT_ASK),
            "it is told how to report back: {first:?}"
        );
        assert!(
            !argv.iter().any(|word| word == "--prompt"),
            "Codex takes its first message as a positional argument: {argv:?}"
        );
        // And it can report back to the Claude Code chat that asked.
        assert!(
            matches!(
                report(&held, &id, &tickets, chat, "done"),
                Answer::Reported { .. }
            ),
            "the Codex chat's report reaches the chat that asked"
        );
    }

    /// D-1445-8: a persona's own file is one a chat can write, and it is committed while a
    /// local profile is one machine's. A profile it names that this machine does not offer is
    /// never run: the chat starts on the asking chat's profile, reads so under its stamp, and
    /// the asking chat is told in the answer.
    #[test]
    fn a_handoff_to_a_persona_naming_an_unoffered_profile_runs_on_the_asking_chats_and_says_so() {
        let plane = Plane::new().a_persona("ops", "profile: /bin/sh -c evil\n");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let ticket = ticket(&held, &id, &tickets, asking);
        let open = Ask::Open(Box::new(OpenChat {
            chat: asking,
            workspace: "alpha".to_owned(),
            create_vision: None,
            persona: Some("ops".to_owned()),
            message: stamped(asking),
            ticket,
            name: None,
            report: false,
        }));

        let said = answer(&held, &id, &tickets, 1, open, &nobody);

        let Answer::Opened { chat, note, .. } = said else {
            panic!("opened, not {said:?}")
        };
        let note = note.expect("the answer says it fell back");
        assert!(
            note.contains("'/bin/sh -c evil'") && note.ends_with("'work'"),
            "{note}"
        );
        let opened = held
            .chats()
            .open_now()
            .into_iter()
            .find(|open| open.session == chat)
            .expect("the new chat is one the app has open");
        assert_eq!(opened.profile.as_deref(), Some("work"));
        let told = first_message_of(&plane);
        assert!(told.contains(&format!("\n⟨{note}⟩\n")), "{told:?}");
    }

    /// And one the project offers but nobody has approved is never started by a handoff: its
    /// command is shown to a person first, and that is the picker's to do.
    #[test]
    fn a_handoff_to_a_persona_on_a_profile_nobody_approved_opens_nothing() {
        let plane = Plane::new().with_a_codex_persona();
        // The profile changes after its approval: what would run is not what was approved.
        let local = plane.root.join(purlis_core::profiles::LOCAL_FILE);
        let text = std::fs::read_to_string(&local).expect("the local file");
        std::fs::write(
            &local,
            format!("{text}env = {{ CODEX_HOME = \"/elsewhere\" }}\n"),
        )
        .expect("changed");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();

        let refused =
            hand_off_to(&held, &id, &tickets, asking, "ops").expect_err("nothing is opened");

        assert!(refused.contains("has not been approved"), "{refused}");
        assert!(plane.runs().iter().all(|argv| {
            !argv
                .last()
                .is_some_and(|last| last.starts_with("⟨handoff from"))
        }));
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
                    chat: asking,
                    task: false,
                    tab: true,
                    reported: false,
                    unreported: false,
                }),
                workspace: Some("alpha".to_owned()),
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
                task: None,
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
                reported: false,
                unreported: false,
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
        assert_eq!(arrived.workspace.as_deref(), Some("alpha"));
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

    // ----- a dispatched task (#1436) -----

    /// A project like [`Plane`]'s, with personas `steward` and `devops`, a draft `intern`, and
    /// `steward` as the persona a new chat adopts.
    fn a_plane_with_personas() -> Plane {
        let plane = Plane::new();
        for (name, front) in [
            ("steward", "description: keeps the project"),
            ("devops", "description: runs the cluster"),
            ("intern", "description: learning\ndraft: true"),
        ] {
            let dir = plane.root.join("personas").join(name);
            std::fs::create_dir_all(&dir).expect("a persona");
            std::fs::write(
                dir.join("persona.md"),
                format!("---\nname: {name}\n{front}\n---\n# {name}\n"),
            )
            .expect("its definition");
        }
        std::fs::write(
            plane.root.join(purlis_core::plane::MANIFEST),
            "[persona]\ndefault = \"steward\"\n",
        )
        .expect("the manifest");
        plane
    }

    /// A chat on the `work` profile running as `persona`, standing in `cwd`.
    fn a_chat_as(held: &Held, root: &Path, persona: Option<&str>, cwd: &Path) -> u32 {
        let start = purlis_core::start::Start {
            profile: Some("work".to_owned()),
            persona: persona.map(str::to_owned),
            name: "1".to_owned(),
            cwd: Some(cwd.to_path_buf()),
            ..Default::default()
        };
        let ready = purlis_core::start::ready(&start, root).expect("the asking chat starts");
        let chat = Chat {
            program: ready.program.clone(),
            cwd: ready.cwd.clone(),
            name: "1".to_owned(),
            resume: ready.session.clone(),
            profile: Some("work".to_owned()),
            persona: persona.map(str::to_owned),
            ..Default::default()
        };
        held.chats()
            .start_ready(&chat, &ready, STARTING)
            .expect("it runs")
    }

    fn a_dispatch(chat: u32, ticket: &str, to: Option<&str>, name: &str) -> Ask {
        Ask::Dispatch(Box::new(DispatchAsk {
            chat,
            to: to.map(str::to_owned),
            name: name.to_owned(),
            brief: "# Check the queue\nSay how many are stuck.\n".to_owned(),
            ticket: ticket.to_owned(),
        }))
    }

    /// Dispatches a task from `asking` and answers what the app said, and what it told the
    /// window.
    fn dispatch(
        held: &Held,
        id: &PlaneId,
        tickets: &Tickets,
        asking: u32,
        to: Option<&str>,
        name: &str,
    ) -> (Answer, Option<Arrived>) {
        let ticket = ticket(held, id, tickets, asking);
        let told = Mutex::new(None);
        let said = answer(
            held,
            id,
            tickets,
            1,
            a_dispatch(asking, &ticket, to, name),
            &|arrived| *told.lock().unwrap() = Some(arrived),
        );
        (said, told.into_inner().unwrap())
    }

    /// The first message a task was started on, once the stand-in has written its argv whole.
    fn tasks_first_message(plane: &Plane) -> String {
        let handed = |plane: &Plane| {
            plane
                .runs()
                .into_iter()
                .filter_map(|argv| argv.last().cloned())
                .find(|last| last.starts_with("⟨task from"))
        };
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        while handed(plane).is_none() && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        handed(plane).unwrap_or_default()
    }

    #[test]
    fn a_task_for_the_asking_chats_own_persona_starts_one_chat_beside_it_on_the_stamp_and_brief() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        let before = held.chats().open_now().len();

        let (said, told) = dispatch(
            &held,
            &id,
            &Tickets::default(),
            asking,
            None,
            " check the queue ",
        );

        let Answer::Dispatched {
            chat,
            name,
            persona,
        } = said
        else {
            panic!("dispatched, not {said:?}")
        };
        assert_eq!(name, "check the queue");
        assert_eq!(persona.as_deref(), Some("steward"));
        assert_eq!(held.chats().open_now().len(), before + 1, "one chat");
        // The window is told, with what the explorer lists it under.
        let told = told.expect("the window is told");
        assert_eq!(told.session, chat);
        assert_eq!(told.label.as_deref(), Some("check the queue"));
        assert_eq!(told.workspace.as_deref(), Some("alpha"));
        assert_eq!(
            told.from,
            Some(crate::HandedFromNote {
                name: "steward 1".to_owned(),
                workspace: "alpha".to_owned(),
                chat: asking,
                task: true,
                // Listed in the Chats section, with no tab until the person opens it (#1447).
                tab: false,
                reported: false,
                unreported: false,
            })
        );
        // Its lineage is on its own record: who asked, that it is a task, and what it owes.
        assert_eq!(
            held.chats().handed_from(chat),
            Some(HandedFrom {
                chat: asking,
                name: "steward 1".to_owned(),
                workspace: Place::Workspace("alpha".to_owned()),
                report: Owed::Due,
                mode: Mode::Task,
                depth: 1,
                by_person: false,
            })
        );
        // In the asking chat's folder, on its profile, as its persona.
        let opened = held
            .chats()
            .open_now()
            .into_iter()
            .find(|open| open.session == chat)
            .expect("open");
        assert_eq!(opened.cwd.as_deref(), Some(alpha.as_path()));
        assert_eq!(opened.profile.as_deref(), Some("work"));
        assert_eq!(opened.persona.as_deref(), Some("steward"));
        // Its first message: purlis's two lines from this app's record, then the brief.
        let first = tasks_first_message(&plane);
        let (stamp, rest) = first.split_once('\n').expect("a stamp line");
        assert!(
            stamp.starts_with("⟨task from `steward 1` · workspace alpha · "),
            "{stamp}"
        );
        assert_eq!(
            rest,
            format!(
                "{}\n\n# Check the queue\nSay how many are stuck.\n",
                purlis_core::handoff::TASK_NOTE
            )
        );
    }

    #[test]
    fn a_chat_on_no_persona_dispatches_as_the_projects_default_which_is_what_it_runs_as() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, None, &plane.root);

        let (said, told) = dispatch(&held, &id, &Tickets::default(), asking, None, "tidy up");

        assert!(
            matches!(&said, Answer::Dispatched { persona: Some(persona), .. } if persona == "steward"),
            "{said:?}"
        );
        // A chat at the project's root is in no workspace, and its task is filed where it is.
        assert_eq!(told.expect("told").workspace, None);
        // It names no persona of its own, so the person sees it by its harness.
        let first = tasks_first_message(&plane);
        assert!(
            first.starts_with("⟨task from `claude 1` · plane root · "),
            "{first}"
        );
    }

    #[test]
    fn a_dispatch_to_another_persona_needs_a_grant_and_starts_nothing() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let before = held.chats().open_now().len();

        let (said, told) = dispatch(
            &held,
            &id,
            &Tickets::default(),
            asking,
            Some("devops"),
            "check the cluster",
        );

        assert_eq!(
            said,
            Answer::NeedsGrant {
                from: Some("steward".to_owned()),
                to: "devops".to_owned(),
            }
        );
        assert_eq!(told, None, "nothing was started, so nothing is told");
        assert_eq!(held.chats().open_now().len(), before);
    }

    #[test]
    fn a_dispatch_naming_no_such_persona_or_a_draft_is_refused_in_a_sentence_and_starts_nothing() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let tickets = Tickets::default();
        let before = held.chats().open_now().len();

        for (to, says) in [
            ("ghost", "has no persona 'ghost' that loads"),
            ("intern", "persona 'intern' is still a draft"),
            ("../steward", "has no persona"),
        ] {
            let (said, told) = dispatch(&held, &id, &tickets, asking, Some(to), "x y");
            assert!(
                matches!(&said, Answer::No { why } if why.contains(says)),
                "{to}: {said:?}"
            );
            assert_eq!(told, None);
        }
        assert_eq!(held.chats().open_now().len(), before);
    }

    #[test]
    fn a_chat_on_no_profile_cannot_dispatch_and_is_told_what_to_do() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_shell_chat(&held);

        let (said, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "x y");

        assert_eq!(
            said,
            Answer::No {
                why: purlis_core::dispatchdecision::Refused::NoProfile.say()
            }
        );
    }

    #[test]
    fn a_dispatch_without_a_ticket_or_with_a_name_purlis_would_not_draw_starts_nothing() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let tickets = Tickets::default();
        let before = held.chats().open_now().len();

        let said = answer(
            &held,
            &id,
            &tickets,
            1,
            a_dispatch(asking, "made-up", None, "x y"),
            &nothing_opens,
        );
        assert_eq!(
            said,
            Answer::No {
                why: NO_TICKET.to_owned()
            }
        );

        for bad in ["check\u{200b}queue", "   "] {
            let (said, told) = dispatch(&held, &id, &tickets, asking, None, bad);
            assert!(matches!(&said, Answer::No { .. }), "{bad:?}: {said:?}");
            assert_eq!(told, None);
        }
        assert_eq!(held.chats().open_now().len(), before);
    }

    #[test]
    fn the_asker_is_the_chat_whose_ticket_was_spent_and_no_other_chats_record_is_read() {
        // Two chats: `steward` and `devops`. A dispatch asked by the steward chat for its own
        // persona starts a steward chat, whatever else is open: the asker is the app's record
        // of the chat the ticket was minted for, and the request has nowhere to name another.
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let devops = a_chat_as(&held, &plane.root, Some("devops"), &plane.root);
        let tickets = Tickets::default();

        let (said, _) = dispatch(&held, &id, &tickets, steward, None, "tidy up");
        assert!(
            matches!(&said, Answer::Dispatched { persona: Some(p), .. } if p == "steward"),
            "{said:?}"
        );

        // A ticket minted for the steward chat does not dispatch as the devops chat.
        let stolen = ticket(&held, &id, &tickets, steward);
        let said = answer(
            &held,
            &id,
            &tickets,
            1,
            a_dispatch(devops, &stolen, None, "x y"),
            &nothing_opens,
        );
        assert_eq!(
            said,
            Answer::No {
                why: NO_TICKET.to_owned()
            }
        );
    }

    #[test]
    fn a_task_inherits_none_of_the_asking_chats_own_grants_hold_or_opt_out() {
        // The asking chat holds another persona's grants and ran its last run without the
        // sandbox. The chat it dispatches is recorded holding nothing and opted out of nothing:
        // what it runs with is compiled for its own persona, from the project.
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = Chat {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 5".to_owned()],
            name: "sh".to_owned(),
            profile: Some("work".to_owned()),
            persona: Some("steward".to_owned()),
            cwd: Some(plane.root.clone()),
            unsandboxed: true,
            ..Default::default()
        };
        let asking = held.chats().start(&asking, STARTING).expect("it runs");

        let (said, _) = dispatch(&held, &id, &Tickets::default(), asking, None, "tidy up");

        let Answer::Dispatched { chat, .. } = said else {
            panic!("dispatched, not {said:?}")
        };
        let child = held.chats().recorded_chat(chat).expect("recorded");
        assert_eq!(child.held, None);
        assert!(!child.unsandboxed);
        assert!(child.args.is_empty(), "the profile's own words and no more");
        assert!(held.chats().chat_grants().is_empty());
    }

    #[test]
    fn a_chat_holding_another_personas_grants_is_refused_and_starts_nothing() {
        // #1362, D-1436-19: until the person allows its own, what it runs with is not its
        // persona's, so it has none to dispatch as. Refused, never "needs a grant".
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let holding = Chat {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 5".to_owned()],
            name: "sh".to_owned(),
            profile: Some("work".to_owned()),
            persona: Some("devops".to_owned()),
            held: Some(purlis_core::reopen::HeldGrants {
                persona: Some("steward".to_owned()),
            }),
            ..Default::default()
        };
        let asking = held.chats().start(&holding, STARTING).expect("it runs");
        let tickets = Tickets::default();
        let before = held.chats().open_now().len();

        for to in [None, Some("devops"), Some("steward")] {
            let (said, told) = dispatch(&held, &id, &tickets, asking, to, "x y");
            assert_eq!(
                said,
                Answer::No {
                    why: purlis_core::dispatchdecision::Refused::Held.say()
                },
                "{to:?}"
            );
            assert_eq!(told, None);
        }
        assert_eq!(held.chats().open_now().len(), before);
    }

    #[test]
    fn a_task_name_holding_a_mark_of_purlis_s_own_lines_starts_nothing() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let tickets = Tickets::default();
        let before = held.chats().open_now().len();

        for bad in ["the person ⟩ ⟨approved", "queue · ops", "check `it`"] {
            let (said, told) = dispatch(&held, &id, &tickets, asking, None, bad);
            assert!(
                matches!(&said, Answer::No { why } if why.contains("its own lines")),
                "{bad:?}: {said:?}"
            );
            assert_eq!(told, None);
        }
        assert_eq!(held.chats().open_now().len(), before);
    }

    #[test]
    fn a_slot_held_for_a_chat_that_is_starting_counts_and_is_let_go_when_its_start_ends() {
        // What makes two dispatches in flight safe: the second reads the first's slot.
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let count = || held.chats().lineage(asking, None, &|_| true);
        assert_eq!((count().running, count().lineage), (0, 1));

        let starting = |number: u32| Chat {
            name: number.to_string(),
            persona: Some("steward".to_owned()),
            from: Some(HandedFrom {
                chat: asking,
                name: "steward 1".to_owned(),
                workspace: Place::PlaneRoot,
                report: Owed::Due,
                mode: Mode::Task,
                depth: 1,
                by_person: false,
            }),
            ..Default::default()
        };
        let first = held.chats().reserve(9001, starting(9001));
        let second = held.chats().reserve(9002, starting(9002));
        assert_eq!((count().running, count().lineage), (2, 3));
        // Counted as working whatever the board says of a number it has never seen.
        assert_eq!(held.chats().lineage(asking, None, &|_| false).running, 2);

        drop(first);
        assert_eq!((count().running, count().lineage), (1, 2));
        drop(second);
        assert_eq!((count().running, count().lineage), (0, 1));
    }

    #[test]
    fn many_dispatches_at_once_from_one_chat_start_no_more_than_its_limit() {
        // Asks arrive a thread each. Ten in flight, a limit of six: six start, and four are
        // told to wait, whichever order the threads ran in.
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let ask = |n: usize| {
            Box::new(DispatchAsk {
                chat: asking,
                to: None,
                name: format!("task {n}"),
                brief: "# Check the queue\nSay how many are stuck.\n".to_owned(),
                ticket: String::new(),
            })
        };

        let answers: Vec<Result<bool, String>> = std::thread::scope(|scope| {
            let running: Vec<_> = (0..10)
                .map(|n| {
                    let (held, id) = (&held, &id);
                    scope.spawn(move || {
                        dispatch_it(held, id, &ask(n), STARTING)
                            .map(|it| matches!(it, Dispatched::Started(_)))
                    })
                })
                .collect();
            running
                .into_iter()
                .map(|thread| thread.join().expect("a dispatch"))
                .collect()
        });

        let started = answers.iter().filter(|said| **said == Ok(true)).count();
        let refused: Vec<&String> = answers
            .iter()
            .filter_map(|said| said.as_ref().err())
            .collect();
        assert_eq!(started, 6, "{answers:?}");
        assert_eq!(refused.len(), 4, "{answers:?}");
        let full = purlis_core::dispatchdecision::Refused::TooManyRunning { limit: 6 }.say();
        assert!(refused.iter().all(|why| **why == full), "{refused:?}");
        assert_eq!(held.chats().lineage(asking, None, &|_| true).running, 6);
    }

    #[test]
    fn a_task_whose_program_ended_without_a_report_is_not_counted_as_running() {
        // D-1436-18: it failed. It is still open, for the person to read, and it no longer
        // holds one of the asking chat's six.
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let tickets = Tickets::default();
        let (said, _) = dispatch(&held, &id, &tickets, asking, None, "check the queue");
        let Answer::Dispatched { chat: child, .. } = said else {
            panic!("dispatched, not {said:?}")
        };
        let count = |ended: Option<u32>| {
            held.chats()
                .lineage(asking, None, &|chat| Some(chat) != ended)
        };
        assert_eq!((count(None).running, count(None).lineage), (1, 2));
        assert_eq!(
            (count(Some(child)).running, count(Some(child)).lineage),
            (0, 1)
        );
    }

    #[test]
    fn a_chat_started_again_keeps_its_tasks_and_the_reports_waiting_for_it() {
        // A restart gives a chat a new number: the ordinary end of Allow on a sandbox block.
        // Its tasks are still its tasks, and a report already left for it is still its own.
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        let tickets = Tickets::default();
        let dispatched = |name: &str| {
            let (said, _) = dispatch(&held, &id, &tickets, asking, None, name);
            match said {
                Answer::Dispatched { chat, .. } => chat,
                other => panic!("dispatched, not {other:?}"),
            }
        };
        let (first, second) = (dispatched("one"), dispatched("two"));
        // The first reports before the restart: its report waits under the old number.
        let _ = tasks_report(
            &held,
            &id,
            &tickets,
            first,
            purlis_core::handback::Outcome::Done,
            None,
        );

        let again = held
            .start_chat_fresh(asking, STARTING)
            .expect("it starts again");

        assert_ne!(again, asking);
        for task in [first, second] {
            assert_eq!(
                held.chats().handed_from(task).map(|from| from.chat),
                Some(again),
                "its asking chat, under its new number"
            );
        }
        // The one still working is still counted against the chat that asked.
        assert_eq!(held.chats().lineage(again, None, &|_| true).running, 1);
        // The report left before the restart is the new number's to take, and was not sent
        // to the workspace when the old one closed.
        let waiting =
            purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(again));
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].from, "one");
        assert!(
            purlis_core::handback::take(
                held.root(),
                purlis_core::handback::For::Place(&Place::Workspace("alpha".to_owned()))
            )
            .is_empty()
        );
        // And the second's report, sent after it, reaches the chat that asked.
        let said = tasks_report(
            &held,
            &id,
            &tickets,
            second,
            purlis_core::handback::Outcome::Done,
            None,
        );
        assert!(
            matches!(&said, Answer::Reported { kept_for: None, .. }),
            "{said:?}"
        );
    }

    #[test]
    fn which_kind_of_report_it_is_is_the_record_s_and_not_the_line_s() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        let tickets = Tickets::default();

        // A task that sends a report with no outcome is told how a task reports, and still
        // owes its one report.
        let (said, _) = dispatch(&held, &id, &tickets, asking, None, "check the queue");
        let Answer::Dispatched { chat: task, .. } = said else {
            panic!("dispatched, not {said:?}")
        };
        let plain = report(&held, &id, &tickets, task, "done");
        assert!(
            matches!(&plain, Answer::No { why } if why.contains("purlis dispatch report --outcome")),
            "{plain:?}"
        );
        assert!(matches!(
            tasks_report(
                &held,
                &id,
                &tickets,
                task,
                purlis_core::handback::Outcome::Done,
                None
            ),
            Answer::Reported { .. }
        ));
        let _ = purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(asking));

        // A handed-off chat that sends a task's line reports as a handoff does: its summary,
        // with no outcome, and as a needs-you item.
        let (handed, _) =
            hand_off(&held, &id, &tickets, asking, Some("drop commons"), true).expect("opened");
        let said = tasks_report(
            &held,
            &id,
            &tickets,
            handed,
            purlis_core::handback::Outcome::Failed,
            Some("svc: 2 files"),
        );
        assert!(matches!(&said, Answer::Reported { .. }), "{said:?}");
        let waiting =
            purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(asking));
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].task, None);
        assert_eq!(
            held.hooks().board().reports(asking),
            vec!["drop commons".to_owned()]
        );
    }

    #[test]
    fn a_chat_no_dispatch_started_has_nobody_to_send_a_task_s_report_to() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);

        let said = tasks_report(
            &held,
            &id,
            &Tickets::default(),
            asking,
            purlis_core::handback::Outcome::Done,
            None,
        );

        assert!(
            matches!(&said, Answer::No { why } if why.contains("not started by a dispatch")),
            "{said:?}"
        );
    }

    /// `child` sends a task's report, on a ticket of its own.
    fn tasks_report(
        held: &Held,
        id: &PlaneId,
        tickets: &Tickets,
        child: u32,
        outcome: purlis_core::handback::Outcome,
        changed: Option<&str>,
    ) -> Answer {
        let ticket = ticket(held, id, tickets, child);
        answer(
            held,
            id,
            tickets,
            1,
            Ask::Report(Box::new(purlis_core::hookwire::ReportBack {
                chat: child,
                summary: "Forty are stuck.".to_owned(),
                ticket,
                task: Some(TaskReport {
                    outcome,
                    changed: changed.map(str::to_owned),
                }),
            })),
            &nothing_opens,
        )
    }

    #[test]
    fn a_task_s_report_waits_for_the_asking_chats_next_turn_and_is_no_needs_you_item() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        let tickets = Tickets::default();
        let (said, _) = dispatch(&held, &id, &tickets, asking, None, "check the queue");
        let Answer::Dispatched { chat: child, .. } = said else {
            panic!("dispatched, not {said:?}")
        };
        // The app wrote this chat's session record, and knows where.
        held.chats()
            .wrote_record(child, "workspaces/alpha/sessions/20261007-143900-queue.md");

        let said = tasks_report(
            &held,
            &id,
            &tickets,
            child,
            purlis_core::handback::Outcome::Blocked,
            Some("svc: 2 files"),
        );

        assert_eq!(
            said,
            Answer::Reported {
                to: "steward 1".to_owned(),
                kept_for: None
            }
        );
        let waiting =
            purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(asking));
        assert_eq!(waiting.len(), 1, "left for its next turn");
        assert_eq!(waiting[0].from, "check the queue");
        assert_eq!(waiting[0].summary, "Forty are stuck.");
        assert_eq!(
            waiting[0].task,
            Some(purlis_core::handback::Task {
                outcome: purlis_core::handback::Outcome::Blocked,
                changed: Some("svc: 2 files".to_owned()),
                record: Some("workspaces/alpha/sessions/20261007-143900-queue.md".to_owned()),
                by_person: false,
                unreported: false,
                stopped: false,
            })
        );
        // For the chat that asked, not for the person (#1434).
        assert!(held.hooks().board().reports(asking).is_empty());
        assert!(!held.hooks().board().needs_you().contains(&asking));
        // One report: the task is no longer one the asking chat has running.
        assert_eq!(held.chats().lineage(asking, None, &|_| true).running, 0);
        let again = tasks_report(
            &held,
            &id,
            &tickets,
            child,
            purlis_core::handback::Outcome::Done,
            None,
        );
        assert!(
            matches!(&again, Answer::No { why } if why.contains("already reported")),
            "{again:?}"
        );
    }

    #[test]
    fn a_task_s_report_whose_asking_chat_has_closed_is_kept_for_its_workspace() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        let tickets = Tickets::default();
        let (said, _) = dispatch(&held, &id, &tickets, asking, None, "check the queue");
        let Answer::Dispatched { chat: child, .. } = said else {
            panic!("dispatched, not {said:?}")
        };
        held.chats().close(asking).unwrap();

        let said = tasks_report(
            &held,
            &id,
            &tickets,
            child,
            purlis_core::handback::Outcome::Done,
            None,
        );

        assert_eq!(
            said,
            Answer::Reported {
                to: "steward 1".to_owned(),
                kept_for: Some("alpha".to_owned()),
            }
        );
        let kept = purlis_core::handback::take(
            held.root(),
            purlis_core::handback::For::Place(&Place::Workspace("alpha".to_owned())),
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(
            kept[0].task.as_ref().map(|task| task.outcome),
            Some(purlis_core::handback::Outcome::Done)
        );
    }

    #[test]
    fn a_seventh_task_is_refused_while_six_are_running_and_says_to_wait() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let tickets = Tickets::default();
        for n in 0..6 {
            let (said, _) = dispatch(&held, &id, &tickets, asking, None, &format!("task {n}"));
            assert!(matches!(said, Answer::Dispatched { .. }), "{n}: {said:?}");
        }

        let (said, told) = dispatch(&held, &id, &tickets, asking, None, "one more");

        assert_eq!(
            said,
            Answer::No {
                why: purlis_core::dispatchdecision::Refused::TooManyRunning { limit: 6 }.say()
            }
        );
        assert_eq!(told, None);
    }

    // ----- the person's own dispatch, from a chat's tab (#1438) -----

    /// Planes whose chats run on `host`, which runs nothing: no pty, so these run wherever
    /// the tests do. What a chat was started on is `host.openings()`.
    fn planes_on(host: &Pretend) -> Planes {
        let host = host.clone();
        planes().running_sessions_on(Arc::new(move |_| Box::new(host.clone())))
    }

    /// The person asks `persona` from chat `asking`'s tab, with dispatch locked by nobody.
    fn ask_from_the_tab(
        held: &Held,
        id: &PlaneId,
        asking: u32,
        persona: &str,
        name: &str,
    ) -> Result<Arrived, String> {
        ask_persona(
            held,
            id,
            &PersonAsk {
                chat: asking,
                persona: persona.to_owned(),
                name: name.to_owned(),
                ask: "Is prod healthy? Say what you checked.\n".to_owned(),
            },
            &purlis_core::sandbox::policy::Locks::none(),
            STARTING,
        )
    }

    /// The vault registry of the operator's case: `prod` is tagged for `devops`.
    fn a_vault_for_devops(root: &Path) {
        std::fs::write(
            root.join("vaults.json"),
            serde_json::json!({ "vaults": {
                "prod": {"provider": "plain-file", "config": {"file": "prod.json"}, "persona": "devops"},
            }})
            .to_string(),
        )
        .expect("the registry");
    }

    /// Whether a brokered `secret exec` for vault `prod` is authorised for chat `chat`, asked
    /// as the app asks it: of its own record of that chat's persona.
    fn may_use_prod(held: &Held, chat: u32) -> Result<(), String> {
        let persona = held.chats().recorded_chat(chat).expect("open").persona;
        purlis_core::secrets::brokered::authorise(
            &purlis_core::secrets::Ctx::new(held.root(), purlis_core::secrets::Env::of(&[])),
            persona.as_deref(),
            "prod",
        )
    }

    #[test]
    fn from_a_steward_chat_ask_devops_starts_a_devops_chat_that_may_use_the_devops_vault() {
        // The operator's case of 2026-10-07: a steward chat is refused the devops vault, and
        // nothing that chat runs can change that. The person asks devops from its tab, and the
        // chat that starts is devops: its `secret exec` is authorised, with no Notice to
        // answer first and no handoff brief to write.
        let plane = a_plane_with_personas();
        a_vault_for_devops(&plane.root);
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        assert!(
            may_use_prod(&held, steward).is_err(),
            "the steward chat is refused the devops vault"
        );
        let before = held.chats().open_now().len();

        let arrived = ask_from_the_tab(&held, &id, steward, "devops", "check prod")
            .expect("the devops chat starts");

        assert_eq!(held.chats().open_now().len(), before + 1, "one chat");
        let devops = arrived.session;
        assert_eq!(arrived.persona.as_deref(), Some("devops"));
        assert_eq!(arrived.label.as_deref(), Some("check prod"));
        assert_eq!(arrived.workspace.as_deref(), Some("alpha"));
        // Its brokered `secret exec` for the devops vault is authorised.
        assert_eq!(may_use_prod(&held, devops), Ok(()));
        // No Notice: it holds nobody else's grants, so there is nothing to allow and restart
        // for. What the project compiles for its persona is `dispatchdecision`'s own test.
        let child = held.chats().recorded_chat(devops).expect("recorded");
        assert_eq!(child.persona.as_deref(), Some("devops"));
        assert_eq!(child.held, None);
        assert_eq!(held.chats().grants_held(devops), None);
        assert!(!child.unsandboxed);
        assert!(held.chats().chat_grants().is_empty());
        // In the steward chat's folder, on its profile: the two things it takes from it.
        assert_eq!(child.cwd.as_deref(), Some(alpha.as_path()));
        assert_eq!(child.profile.as_deref(), Some("work"));
        // Under the steward chat, as a task that owes it one report, started by the person.
        assert_eq!(
            held.chats().handed_from(devops),
            Some(HandedFrom {
                chat: steward,
                name: "steward 1".to_owned(),
                workspace: Place::Workspace("alpha".to_owned()),
                report: Owed::Due,
                mode: Mode::Task,
                depth: 1,
                by_person: true,
            })
        );
        // No handoff brief: its first message is purlis's two lines saying the person asked,
        // then what the person typed.
        let first = host
            .openings()
            .last()
            .and_then(|opening| opening.args.last().cloned())
            .expect("the devops chat's first message");
        assert_eq!(purlis_core::handoff::stamped(&first), None, "{first}");
        let (stamp, rest) = first.split_once('\n').expect("a stamp line");
        assert!(
            stamp.starts_with("⟨the person asks, from the tab of `steward 1` · workspace alpha · "),
            "{stamp}"
        );
        assert_eq!(
            rest,
            format!(
                "{}\n\nIs prod healthy? Say what you checked.\n",
                purlis_core::handoff::PERSON_TASK_NOTE
            )
        );
    }

    #[test]
    fn the_person_s_ask_takes_nothing_the_asking_chat_holds_or_was_allowed() {
        // The chat whose tab the person asks from holds another persona's grants and ran
        // without the sandbox. Its own dispatch would need a grant even for its own persona;
        // the person's needs none, and the chat that starts holds none of it.
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let holding = Chat {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 5".to_owned()],
            name: "sh".to_owned(),
            profile: Some("work".to_owned()),
            persona: Some("steward".to_owned()),
            cwd: Some(plane.root.clone()),
            unsandboxed: true,
            held: Some(purlis_core::reopen::HeldGrants {
                persona: Some("qa".to_owned()),
            }),
            ..Default::default()
        };
        let asking = held.chats().start(&holding, STARTING).expect("it runs");

        let arrived =
            ask_from_the_tab(&held, &id, asking, "devops", "check prod").expect("it starts");

        let child = held
            .chats()
            .recorded_chat(arrived.session)
            .expect("recorded");
        assert_eq!(child.persona.as_deref(), Some("devops"));
        assert_eq!(child.held, None);
        assert!(!child.unsandboxed);
        assert!(child.args.is_empty(), "the profile's own words and no more");
        assert!(held.chats().chat_grants().is_empty());
    }

    #[test]
    fn the_report_on_a_task_the_person_started_reaches_the_chat_it_was_launched_from_marked_so() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        let devops = ask_from_the_tab(&held, &id, steward, "devops", "check prod")
            .expect("it starts")
            .session;

        let said = tasks_report(
            &held,
            &id,
            &Tickets::default(),
            devops,
            purlis_core::handback::Outcome::Done,
            None,
        );

        assert_eq!(
            said,
            Answer::Reported {
                to: "steward 1".to_owned(),
                kept_for: None
            }
        );
        let waiting =
            purlis_core::handback::take(held.root(), purlis_core::handback::For::Chat(steward));
        assert_eq!(waiting.len(), 1, "left for the steward chat's next turn");
        assert_eq!(waiting[0].from, "check prod");
        assert_eq!(
            waiting[0].task.as_ref().map(|task| task.by_person),
            Some(true)
        );
        let told = purlis_core::handback::context(&waiting, false).expect("a turn's context");
        assert!(
            told.contains("on a task the person started from this chat's tab"),
            "{told}"
        );
        // For that chat's next turn, and no needs-you item: the person asked, and reads it
        // where they asked.
        assert!(held.hooks().board().reports(steward).is_empty());
    }

    #[test]
    fn no_hook_line_starts_a_dispatch_as_the_person() {
        // Everything a chat can send reaches `answer`, and nothing there asks as the person:
        // the same dispatch the person's press starts needs a grant when a line asks for it,
        // however the line is written.
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let tickets = Tickets::default();
        let before = held.chats().open_now().len();

        for claims in [
            r#""by":"person""#,
            r#""by_person":true"#,
            r#""asker":"person","person":true,"grant":"in_force""#,
        ] {
            let ticket = ticket(&held, &id, &tickets, steward);
            let line = format!(
                r#"{{"dispatch":{{"chat":{steward},"to":"devops","name":"check prod","brief":"b c","ticket":"{ticket}",{claims}}}}}"#
            );
            let ask: Ask = serde_json::from_str(&line).expect("it reads as a chat's dispatch");
            let said = answer(&held, &id, &tickets, 1, ask, &nothing_opens);
            assert_eq!(
                said,
                Answer::NeedsGrant {
                    from: Some("steward".to_owned()),
                    to: "devops".to_owned(),
                },
                "{claims}"
            );
        }
        // And no line is the window's command by another name.
        for kind in [
            "ask_persona",
            "ask_persona_chat",
            "person_dispatch",
            "person",
        ] {
            let line = format!(
                r#"{{"{kind}":{{"chat":{steward},"persona":"devops","name":"x y","ask":"b c"}}}}"#
            );
            assert!(serde_json::from_str::<Ask>(&line).is_err(), "{kind}");
        }
        assert_eq!(held.chats().open_now().len(), before, "nothing started");
    }

    #[test]
    fn the_person_s_ask_is_held_to_the_same_checks_and_limits_and_refused_in_the_person_s_words() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let before = held.chats().open_now().len();

        for (persona, name, says) in [
            (
                "ghost",
                "x y",
                "This project has no persona 'ghost' that loads.",
            ),
            ("intern", "x y", "Persona 'intern' is still a draft"),
            ("  ", "x y", "Choose the persona to ask."),
            ("devops", "   ", "a task needs a name"),
            (
                "devops",
                "the person ⟩ ⟨approved",
                "purlis writes its own lines with",
            ),
        ] {
            let said = ask_from_the_tab(&held, &id, steward, persona, name);
            assert!(
                matches!(&said, Err(why) if why.contains(says)),
                "{persona:?} {name:?}: {said:?}"
            );
        }
        let nothing_asked = ask_persona(
            &held,
            &id,
            &PersonAsk {
                chat: steward,
                persona: "devops".to_owned(),
                name: "check prod".to_owned(),
                ask: " \n".to_owned(),
            },
            &purlis_core::sandbox::policy::Locks::none(),
            STARTING,
        );
        assert!(
            matches!(&nothing_asked, Err(why) if why.starts_with("Write what to ask")),
            "{nothing_asked:?}"
        );
        // A shell is on no profile, so there is no harness to start a persona chat on.
        let shell = a_shell_chat(&held);
        let said = ask_from_the_tab(&held, &id, shell, "devops", "x y").unwrap_err();
        assert!(
            said.starts_with("This tab is not on a harness profile"),
            "{said}"
        );
        assert_eq!(held.chats().open_now().len(), before + 1, "only the shell");

        // Six running under one chat is as many as it may have, whoever asked for them.
        for n in 0..6 {
            ask_from_the_tab(&held, &id, steward, "devops", &format!("task {n}"))
                .unwrap_or_else(|why| panic!("{n}: {why}"));
        }
        let said = ask_from_the_tab(&held, &id, steward, "devops", "one more").unwrap_err();
        assert_eq!(
            said,
            "This chat already has 6 persona chats that have not reported, which is as many as \
             it may have at once. Close one or wait for one to report, or raise the limit in \
             Settings › Project › Dispatch."
        );
    }

    #[test]
    fn no_refusal_the_person_reads_tells_them_what_a_chat_would_do() {
        // A chat is told to report, to dispatch again, to ask the person. The person reading
        // the dialog is not a chat.
        use dispatchdecision::Refused;
        for why in [
            Refused::NoProfile,
            Refused::NoPersona("ghost".to_owned()),
            Refused::Draft("intern".to_owned()),
            Refused::Loop("steward".to_owned()),
            Refused::TooDeep { limit: 3 },
            Refused::TooDeep { limit: 0 },
            Refused::TooManyRunning { limit: 6 },
            Refused::TooManyRunning { limit: 0 },
            Refused::LineageFull { limit: 16 },
            Refused::LineageFull { limit: 0 },
        ] {
            let said = said_to_the_person(&why);
            for theirs in [
                "your report",
                "dispatch again",
                "ask the person",
                "the person",
                "purlis persona list",
            ] {
                assert!(!said.contains(theirs), "{why:?}: {said}");
            }
            assert!(said.ends_with('.') && !said.contains('\n'), "{said}");
        }
    }

    #[test]
    fn the_person_s_ask_runs_under_the_limits_the_project_sets() {
        // The project lets one chat have one persona chat running.
        let plane = a_plane_with_personas();
        std::fs::write(
            plane.root.join(purlis_core::plane::MANIFEST),
            "[persona]\ndefault = \"steward\"\n[dispatch]\nrunning-per-chat = 1\n",
        )
        .expect("the manifest");
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);

        ask_from_the_tab(&held, &id, steward, "devops", "the one").expect("the first starts");
        let said = ask_from_the_tab(&held, &id, steward, "devops", "one more").unwrap_err();

        assert!(
            said.starts_with("This chat already has 1 persona chats that have not reported"),
            "{said}"
        );
    }

    /// An administrator's policy, as this machine's file would hold it.
    fn policy(dispatch: &str) -> purlis_core::sandbox::policy::Locks {
        purlis_core::sandbox::policy::Locks::parse(
            &format!(r#"{{"owner": "the platform team", "dispatch": {dispatch}}}"#),
            Path::new("/etc/purlis/policy.json"),
        )
    }

    /// Every open chat by number with the persona it runs as, as the offer is asked.
    fn chats_as(held: &Held) -> Vec<(u32, Option<String>)> {
        held.chats()
            .open_now()
            .into_iter()
            .map(|open| (open.session, open.persona))
            .collect()
    }

    #[test]
    fn where_policy_locks_all_dispatch_the_ask_offers_nothing_says_who_and_starts_nothing() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let before = held.chats().open_now().len();
        let none = purlis_core::sandbox::policy::Locks::none();

        // Unlocked: every finished persona, and never the draft.
        assert_eq!(
            ask_offer(held.root(), &none, &chats_as(&held)),
            AskOffer {
                personas: vec!["devops".to_owned(), "steward".to_owned()],
                locked: None,
                locked_for: Vec::new(),
            }
        );
        // Locked: none, and why, in policy's own sentence.
        let locks = policy(r#"{"allow": false}"#);
        assert!(
            locks.forbids_dispatch(),
            "the fixture is a policy purlis reads"
        );
        let offer = ask_offer(held.root(), &locks, &chats_as(&held));
        assert_eq!(offer.personas, Vec::<String>::new());
        let why = offer.locked.clone().expect("it says why");
        assert!(
            why.starts_with("Policy forbids one chat dispatching to another. Locked by policy"),
            "{why}"
        );
        // And a press the window should not have offered starts nothing.
        let said = ask_persona(
            &held,
            &id,
            &PersonAsk {
                chat: steward,
                persona: "devops".to_owned(),
                name: "check prod".to_owned(),
                ask: "Is prod healthy?".to_owned(),
            },
            &locks,
            STARTING,
        );
        assert_eq!(said.err(), offer.locked);
        assert_eq!(held.chats().open_now().len(), before);
    }

    // ----- a persona chat that fails, is orphaned or outlives its asker (#1443) -----

    use purlis_core::handback::{For, Outcome};
    use purlis_core::state::Event;

    /// A project with personas, held on `host`, and a steward chat in workspace `alpha`.
    fn a_steward_chat(host: &Pretend, plane: &Plane) -> (Planes, PlaneId, u32) {
        let planes = planes_on(host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        (planes, id, steward)
    }

    /// Chat `asking` dispatches a task to its own persona, as a chat does: the task's number.
    fn a_task_of(held: &Held, id: &PlaneId, asking: u32, name: &str) -> u32 {
        match dispatch(held, id, &Tickets::default(), asking, None, name).0 {
            Answer::Dispatched { chat, .. } => chat,
            other => panic!("dispatched, not {other:?}"),
        }
    }

    /// The task sends its report.
    fn reports(held: &Held, id: &PlaneId, task: u32) -> Answer {
        tasks_report(held, id, &Tickets::default(), task, Outcome::Done, None)
    }

    /// The board hears `event` from chat `chat`'s own harness: in the conversation the app
    /// started it under and from one process, which is what makes a report that chat's own.
    fn the_board_hears(held: &Held, chat: u32, event: Event) {
        use purlis_core::hookwire::Conversation;
        let conversation = held
            .board()
            .conversation(chat)
            .map_or(Conversation::Unknown, Conversation::Named);
        held.hooks()
            .board()
            .reported(&purlis_core::hookwire::Report {
                chat,
                event,
                conversation,
                // One pid for the chat's whole run, as its harness has.
                pid: Some(4000 + chat),
                agent: None,
                detail: purlis_core::state::Detail::default(),
            });
    }

    /// Chat `chat` is mid-turn.
    fn works(held: &Held, chat: u32) {
        the_board_hears(held, chat, Event::UserPromptSubmit);
    }

    /// Chat `chat` has had a turn and waits for its next prompt.
    fn rests(held: &Held, chat: u32) {
        the_board_hears(held, chat, Event::UserPromptSubmit);
        the_board_hears(held, chat, Event::Stop);
    }

    /// The app writes chat `chat`'s session record, as it does when the chat asks: the path.
    fn writes_its_record(held: &Held, chat: u32) -> String {
        match crate::smartclose::record(
            held,
            &purlis_core::hookwire::RecordAsk {
                chat,
                title: format!("Record of {chat}"),
                body: "## Goal\n\ng\n\n## Done\n\nd\n\n## Decisions\n\nx\n\n## Open\n\no\n\n## \
                       How to resume\n\nr\n"
                    .to_owned(),
                pieces: Vec::new(),
                cwd: None,
            },
        ) {
            Answer::Recorded { record, .. } => record,
            other => panic!("recorded, not {other:?}"),
        }
    }

    /// Where chat `task` stands in its asking chat `asker`'s list of its dispatches.
    fn stands(held: &Held, asker: u32, task: u32) -> Option<(PersonaChatState, String)> {
        persona_chats(held, asker)
            .into_iter()
            .find(|one| one.session == task)
            .map(|one| (one.state, one.said))
    }

    fn open_chats(held: &Held) -> Vec<u32> {
        let mut open: Vec<u32> = held
            .chats()
            .open_now()
            .iter()
            .map(|open| open.session)
            .collect();
        open.sort_unstable();
        open
    }

    /// What waits for `whose`, as `(from, the app's own voice, a deliberate stop)`.
    fn waiting(held: &Held, whose: For<'_>) -> Vec<(String, bool, bool)> {
        purlis_core::handback::take(held.root(), whose)
            .into_iter()
            .map(|report| {
                let task = report.task.expect("a task's report");
                (report.from, task.unreported, task.stopped)
            })
            .collect()
    }

    const KILLED: fn() -> purlis_core::session::Exit =
        || purlis_core::session::Exit::Signal("SIGKILL".to_owned());

    #[test]
    fn a_killed_persona_chat_gives_its_asking_chat_failed_within_a_bounded_time() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let task = a_task_of(&held, &id, steward, "check prod");
        // The app wrote this chat's session record, and knows where.
        held.chats()
            .wrote_record(task, "workspaces/alpha/sessions/20261007-143900-prod.md");
        assert_eq!(held.chats().lineage(steward, None, &|_| true).running, 1);

        let killed_at = Instant::now();
        host.program_ends(task, KILLED());

        // Bounded: it is there as soon as the operating system has said the program is gone.
        let bound = std::time::Duration::from_secs(5);
        let mut left = Vec::new();
        while left.is_empty() && killed_at.elapsed() < bound {
            left = purlis_core::handback::take(held.root(), For::Chat(steward));
            if left.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        assert!(
            killed_at.elapsed() < bound,
            "the asking chat was never told"
        );
        assert_eq!(
            left.len(),
            1,
            "one report, for the steward chat's next turn"
        );
        assert_eq!(left[0].from, "check prod");
        assert_eq!(left[0].summary, "ended without a report");
        assert_eq!(
            left[0].task,
            Some(purlis_core::handback::Task {
                outcome: Outcome::Failed,
                changed: None,
                record: Some("workspaces/alpha/sessions/20261007-143900-prod.md".to_owned()),
                by_person: false,
                unreported: true,
                stopped: false,
            })
        );
        let told = purlis_core::handback::context(&left, false).expect("a turn's context");
        assert!(
            told.starts_with("⬢ **`check prod` failed: ended without a report**"),
            "{told}"
        );
        assert!(
            told.ends_with(
                "Its session record: `workspaces/alpha/sessions/20261007-143900-prod.md`"
            ),
            "{told}"
        );
        assert_eq!(
            stands(&held, steward, task),
            Some((PersonaChatState::Ended, "ended without a report".to_owned()))
        );
        // Final: a report from it now (that tab, started again) is refused as already made,
        // and closing its tab afterwards says nothing more.
        let again = reports(&held, &id, task);
        assert!(
            matches!(&again, Answer::No { why } if why.contains("already told 'steward 1'")),
            "{again:?}"
        );
        let _ = held.close_chat(task);
        assert!(purlis_core::handback::take(held.root(), For::Chat(steward)).is_empty());
    }

    #[test]
    fn a_failed_report_that_could_not_be_kept_is_still_owed_and_sent_when_it_can_be() {
        // The debt is marked answered only once the report is kept.
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let task = a_task_of(&held, &id, steward, "check prod");
        // Nothing can be left: where the reports wait is a file, not a directory.
        let reports_dir = purlis_core::handback::dir(held.root());
        std::fs::create_dir_all(reports_dir.parent().unwrap()).unwrap();
        std::fs::write(&reports_dir, "").unwrap();

        host.program_ends(task, KILLED());

        assert_eq!(
            held.chats().handed_from(task).map(|from| from.report),
            Some(Owed::Due),
            "not marked as told when nobody was"
        );
        // It can be kept again, and the chat's close sends it.
        std::fs::remove_file(&reports_dir).unwrap();
        held.close_chat(task).expect("closed");
        assert_eq!(
            waiting(&held, For::Chat(steward))
                .iter()
                .map(|(from, by_purlis, _)| (from.as_str(), *by_purlis))
                .collect::<Vec<_>>(),
            [("check prod", true)]
        );
    }

    #[test]
    fn a_persona_chat_the_person_closes_before_it_reports_is_said_stopped_by_the_operator() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let unreported = a_task_of(&held, &id, steward, "check prod");
        let reported = a_task_of(&held, &id, steward, "check staging");
        assert!(matches!(
            reports(&held, &id, reported),
            Answer::Reported { .. }
        ));

        // The person closes both tabs.
        held.close_chat(unreported).expect("closed");
        held.close_chat(reported).expect("closed");

        let left = purlis_core::handback::take(held.root(), For::Chat(steward));
        assert_eq!(left.len(), 2, "{left:?}");
        // The one that reported said it itself.
        assert_eq!(left[0].from, "check staging");
        assert!(!left[0].task.as_ref().unwrap().unreported);
        // The one closed first is said by purlis, as a stop and not as a failure of its own.
        assert_eq!(left[1].from, "check prod");
        assert_eq!(left[1].summary, "stopped by the operator");
        let told = purlis_core::handback::context(&left[1..], false).expect("context");
        assert!(
            told.starts_with("⬢ **`check prod` was stopped by the operator**"),
            "{told}"
        );
        assert!(!told.contains("failed"), "{told}");
        assert!(told.ends_with("It wrote no session record."), "{told}");
    }

    #[test]
    fn a_handoff_that_asked_for_a_report_and_a_chat_nobody_dispatched_are_not_reported_for() {
        // Only a task owes its asking chat an outcome. A handoff's work is the person's to
        // follow, and its tab ending is theirs to see.
        let plane = Plane::new();
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let parent = a_chat_on_work(&held, &plane.root);
        let tickets = Tickets::default();
        let ticket = ticket(&held, &id, &tickets, parent);
        let Answer::Opened { chat: handed, .. } = answer(
            &held,
            &id,
            &tickets,
            1,
            a_named_open(parent, &ticket, stamped(parent), None, true),
            &nobody,
        ) else {
            panic!("the handoff opens")
        };

        host.program_ends(handed, KILLED());
        host.program_ends(parent, KILLED());

        assert!(purlis_core::handback::take(held.root(), For::Chat(parent)).is_empty());
        assert_eq!(
            held.chats().handed_from(handed).map(|from| from.report),
            Some(Owed::Due)
        );
    }

    #[test]
    fn a_persona_chat_restarted_in_its_own_place_still_owes_its_report() {
        // Restart chat ends the old run and starts the same chat again: that is the chat
        // going on, and nothing is reported for it.
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let task = a_task_of(&held, &id, steward, "check prod");

        let again = held
            .start_chat_fresh(task, STARTING)
            .expect("it starts again");

        assert!(purlis_core::handback::take(held.root(), For::Chat(steward)).is_empty());
        assert_eq!(
            held.chats().handed_from(again).map(|from| from.report),
            Some(Owed::Due)
        );
        assert_eq!(held.chats().lineage(steward, None, &|_| true).running, 1);
    }

    #[test]
    fn stopping_every_agent_fails_no_task_and_each_reports_once_it_is_started_again() {
        // D-1443-10: the stop-all switch ends every program at once. That is the person
        // stopping the machine, not a task failing, and "failed" is final.
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let task = a_task_of(&held, &id, steward, "check prod");

        let _ = planes.kill_switch().stop(purlis_core::halt::Actor::Window);
        host.program_ends(task, KILLED());
        host.program_ends(steward, KILLED());

        assert!(purlis_core::handback::take(held.root(), For::Chat(steward)).is_empty());
        let alpha = Place::Workspace("alpha".to_owned());
        assert!(purlis_core::handback::take(held.root(), For::Place(&alpha)).is_empty());
        assert_eq!(
            held.chats().handed_from(task).map(|from| from.report),
            Some(Owed::Due),
            "still owed"
        );
        // Its real report is taken when it comes.
        assert!(matches!(reports(&held, &id, task), Answer::Reported { .. }));
    }

    #[test]
    fn a_reported_persona_chat_stays_open_and_typeable_until_it_is_closed() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let task = a_task_of(&held, &id, steward, "check prod");
        assert_eq!(
            stands(&held, steward, task),
            Some((PersonaChatState::Running, "running".to_owned()))
        );

        let said = reports(&held, &id, task);

        assert!(matches!(said, Answer::Reported { .. }), "{said:?}");
        // Marked reported, and still a chat: open, and the person's keys reach it.
        assert_eq!(
            stands(&held, steward, task),
            Some((PersonaChatState::Reported, "reported".to_owned()))
        );
        assert!(open_chats(&held).contains(&task));
        assert_eq!(held.operator_input(task, b"one more thing\r"), Ok(()));
        // Typing in it, and a turn of its own afterwards, leave it reported.
        works(&held, task);
        assert_eq!(
            stands(&held, steward, task).map(|(state, _)| state),
            Some(PersonaChatState::Reported)
        );
        // Until the person closes it, which reports nothing more.
        held.close_chat(task).expect("closed");
        assert_eq!(stands(&held, steward, task), None);
        assert_eq!(
            waiting(&held, For::Chat(steward)).len(),
            1,
            "its own report"
        );
    }

    #[test]
    fn the_asking_chat_s_list_says_waiting_on_the_operator_for_a_persona_chat_at_a_permission_prompt()
     {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let task = a_task_of(&held, &id, steward, "check prod");
        works(&held, task);
        assert_eq!(
            stands(&held, steward, task),
            Some((PersonaChatState::Running, "running".to_owned()))
        );

        // Its harness stops mid-turn on a permission prompt, in its own tab.
        the_board_hears(&held, task, Event::Notification);

        assert_eq!(
            stands(&held, steward, task),
            Some((
                PersonaChatState::WaitingOnOperator,
                "waiting on the operator".to_owned()
            ))
        );
        // The person answers there, and it is at work again.
        works(&held, task);
        assert_eq!(
            stands(&held, steward, task).map(|(state, _)| state),
            Some(PersonaChatState::Running)
        );
        // The list is the asking chat's own: no other chat's tasks are in it.
        assert!(persona_chats(&held, task).is_empty());
    }

    /// The tree "Stop them" is asked about: under the steward chat, a task that **reported**
    /// and had dispatched one that is still running; a task still running; a handoff's chat
    /// mid-turn, opened by that running task; and a handoff's chat at rest.
    struct Tree {
        steward: u32,
        reported: u32,
        under_reported: u32,
        running: u32,
        handed_mid_turn: u32,
        handed_at_rest: u32,
    }

    fn a_tree(held: &Held, id: &PlaneId, steward: u32) -> Tree {
        let reported = a_task_of(held, id, steward, "tidy up");
        let under_reported = a_task_of(held, id, reported, "read the logs");
        assert!(matches!(
            reports(held, id, reported),
            Answer::Reported { .. }
        ));
        rests(held, reported);
        let running = a_task_of(held, id, steward, "check prod");
        let hand_off = |from: u32| {
            let tickets = Tickets::default();
            let ticket = ticket(held, id, &tickets, from);
            match answer(
                held,
                id,
                &tickets,
                1,
                a_named_open(from, &ticket, stamped(from), None, false),
                &nobody,
            ) {
                Answer::Opened { chat, .. } => chat,
                other => panic!("opened, not {other:?}"),
            }
        };
        let handed_mid_turn = hand_off(running);
        works(held, handed_mid_turn);
        let handed_at_rest = hand_off(steward);
        rests(held, handed_at_rest);
        Tree {
            steward,
            reported,
            under_reported,
            running,
            handed_mid_turn,
            handed_at_rest,
        }
    }

    #[test]
    fn closing_asks_about_every_chat_at_work_below_through_chats_that_are_not() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let tree = a_tree(&held, &id, steward);

        // Deepest first: the task under the one that reported, the handoff's chat mid-turn
        // under the running task, then that task. Never the reported one or the chat at rest.
        assert_eq!(
            at_work_below(&held, tree.steward),
            [tree.under_reported, tree.handed_mid_turn, tree.running]
        );
        assert_eq!(running_below(&held, tree.steward).len(), 3);
        // A chat whose own tasks have all reported is still asked about what runs below them.
        held.close_chat(tree.running).expect("closed");
        assert_eq!(
            at_work_below(&held, tree.steward),
            [tree.under_reported],
            "below a chat that is not itself at work"
        );
    }

    #[test]
    fn stop_them_ends_every_chat_at_work_below_and_the_asking_chat_in_one_step() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let tree = a_tree(&held, &id, steward);

        let closed = held.close_chat_stopping(tree.steward, true);

        assert_eq!(
            closed,
            [
                tree.under_reported,
                tree.handed_mid_turn,
                tree.running,
                tree.steward
            ]
        );
        // What is left is what was not at work: the reported task, which has no session
        // record to close on, and the handoff's chat at rest.
        assert_eq!(open_chats(&held), [tree.reported, tree.handed_at_rest]);
        // What was stopped is on record, as a stop: for the chat that asked it where that is
        // still open, and for the workspace where it closed in the same step.
        assert_eq!(
            waiting(&held, For::Chat(tree.reported)),
            [("read the logs".to_owned(), true, true)]
        );
        let alpha = Place::Workspace("alpha".to_owned());
        let for_alpha = waiting(&held, For::Place(&alpha));
        assert!(
            for_alpha.contains(&("check prod".to_owned(), true, true)),
            "{for_alpha:?}"
        );
    }

    #[test]
    fn a_smart_close_s_stop_them_also_ends_what_the_chat_starts_while_it_writes_its_record() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let first = a_task_of(&held, &id, steward, "check prod");

        // Stop them, then Smart close: the chats below end now, and the chat is not closed.
        assert_eq!(held.close_chat_stopping(steward, false), [first]);
        assert!(open_chats(&held).contains(&steward));
        // Still running, it dispatches again while it wraps up.
        let late = a_task_of(&held, &id, steward, "one more thing");

        // Its record lands and it closes: what it started in between goes with it.
        held.close_chat(steward).expect("closed");
        assert_eq!(open_chats(&held), Vec::<u32>::new());
        let _ = late;
    }

    #[test]
    fn a_smart_close_that_ends_without_closing_takes_its_stop_them_with_it() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        assert_eq!(held.close_chat_stopping(steward, false), Vec::<u32>::new());
        // The person cancels the Smart close, and later starts a task they mean to keep.
        held.tell_smart_close(steward, crate::smartclose::Phase::Cancelled);
        let kept = a_task_of(&held, &id, steward, "check prod");

        held.close_chat(steward).expect("closed");

        assert_eq!(
            open_chats(&held),
            [kept],
            "the old answer is not this close's"
        );
    }

    #[test]
    fn a_persona_chat_kept_running_reports_to_the_workspace_once_its_asking_chat_is_gone() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let kept = a_task_of(&held, &id, steward, "check prod");
        let dies = a_task_of(&held, &id, steward, "check staging");

        // Keep them running: the asking chat closes, and they do not.
        held.close_chat(steward).expect("closed");

        assert_eq!(open_chats(&held), [kept, dies], "both kept running");
        // One reports, and one's program dies: both go to the workspace the chat asked from.
        assert_eq!(
            reports(&held, &id, kept),
            Answer::Reported {
                to: "steward 1".to_owned(),
                kept_for: Some("alpha".to_owned()),
            }
        );
        host.program_ends(dies, KILLED());
        let alpha = Place::Workspace("alpha".to_owned());
        assert_eq!(
            waiting(&held, For::Place(&alpha)),
            [
                ("check prod".to_owned(), false, false),
                ("check staging".to_owned(), true, false)
            ]
        );
    }

    #[test]
    fn the_report_of_a_task_the_person_started_stays_with_it_when_its_tab_chat_is_gone() {
        // D-1443-9: the person asked, from a tab that has since closed. The report is theirs:
        // it is not handed to whichever chat starts next in that workspace.
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let (planes, id, steward) = a_steward_chat(&host, &plane);
        let held = planes.held(&id).expect("held");
        let task = ask_from_the_tab(&held, &id, steward, "devops", "check prod")
            .expect("it starts")
            .session;
        works(&held, task);
        held.close_chat(steward).expect("closed");

        let said = reports(&held, &id, task);

        assert_eq!(
            said,
            Answer::Reported {
                to: "steward 1".to_owned(),
                kept_for: Some("the person".to_owned()),
            }
        );
        let alpha = Place::Workspace("alpha".to_owned());
        assert!(purlis_core::handback::take(held.root(), For::Place(&alpha)).is_empty());
        assert!(
            !purlis_core::handback::dir(held.root())
                .join("chat-1")
                .exists()
        );
        // The task chat needs the person, and says its report is why.
        assert!(held.hooks().board().needs_you().contains(&task));
        assert_eq!(held.hooks().board().reports(task), ["check prod"]);
    }

    #[test]
    fn closing_an_asking_chat_closes_only_its_reported_chats_that_are_at_rest_on_a_record() {
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let closes: Arc<Mutex<Vec<crate::smartclose::SmartClosing>>> = Arc::default();
        let planes = {
            let closes = Arc::clone(&closes);
            planes_on(&host).telling_smart_close(Arc::new(move |step| {
                closes.lock().unwrap().push(step);
            }))
        };
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        let task = |name: &str| a_task_of(&held, &id, steward, name);
        // Each of these has reported and has its session record written.
        let at_rest = task("at rest");
        let working_again = task("working again");
        let asking_you = task("asking you");
        let record_gone = task("record gone");
        for chat in [at_rest, working_again, asking_you, record_gone] {
            writes_its_record(&held, chat);
            assert!(matches!(reports(&held, &id, chat), Answer::Reported { .. }));
            rests(&held, chat);
        }
        // The person gave one more to do, one is at a permission prompt, and one's record was
        // deleted from the project.
        works(&held, working_again);
        works(&held, asking_you);
        the_board_hears(&held, asking_you, Event::Notification);
        let gone = held.chats().last_record(record_gone).expect("its record");
        std::fs::remove_file(held.root().join(&gone)).expect("deleted");
        // And two that have not reported: closing asks about them, and this close keeps them.
        let running = task("still running");
        let unrecorded = task("reported with no record");
        assert!(matches!(
            reports(&held, &id, unrecorded),
            Answer::Reported { .. }
        ));

        // What the dialog is told closes with it: the one at rest on its record, and the one
        // that can still be asked for its record. Never one at work, asking, or unrecorded
        // for good.
        let closes_with: Vec<String> = persona_chats(&held, steward)
            .into_iter()
            .filter(|one| one.closes_with_its_asker)
            .map(|one| one.name)
            .collect();
        assert_eq!(closes_with, ["at rest", "reported with no record"]);

        held.close_chat(steward).expect("closed");

        assert_eq!(
            open_chats(&held),
            [working_again, asking_you, record_gone, running, unrecorded]
        );
        let told: Vec<(u32, crate::smartclose::Phase)> = closes
            .lock()
            .unwrap()
            .iter()
            .map(|step| (step.session, step.phase))
            .collect();
        assert!(
            told.contains(&(at_rest, crate::smartclose::Phase::Closed)),
            "{told:?}"
        );
        for kept in [working_again, asking_you, record_gone, running] {
            assert!(
                !told.iter().any(|(session, _)| *session == kept),
                "{kept}: {told:?}"
            );
        }
    }

    // What only a real program and a real sandbox show (the pretend host ends a program on the
    // test's own thread, ends none on a close, and compiles no project that has a sandbox).

    #[cfg(unix)]
    #[test]
    fn a_task_chat_whose_real_program_is_killed_reports_failed_to_the_chat_that_asked() {
        let plane = a_plane_with_personas();
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let alpha = held.root().join("workspaces").join("alpha");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &alpha);
        let task = a_task_of(&held, &id, steward, "check prod");
        // Its program is running: the stand-in has written what it was started on.
        assert!(!tasks_first_message(&plane).is_empty());
        let pid = held
            .chats()
            .sessions()
            .process_id(task)
            .expect("its program's pid");

        let killed = purlis_core::forklock::status(
            std::process::Command::new("kill").args(["-9", &pid.to_string()]),
        )
        .expect("kill runs");
        assert!(killed.success());

        // The exit is heard on the session's own thread, and the report is left from there.
        let until = Instant::now() + std::time::Duration::from_secs(30);
        let mut left = Vec::new();
        while left.is_empty() && Instant::now() < until {
            left = purlis_core::handback::take(held.root(), For::Chat(steward));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(left.len(), 1, "the asking chat was never told");
        assert_eq!(left[0].summary, "ended without a report");
        assert_eq!(
            held.chats().handed_from(task).map(|from| from.report),
            Some(Owed::Failed)
        );
        // And a chat closed while it runs is told once, as a stop, by the close: the end of
        // the program the close ended says nothing more.
        let other = a_task_of(&held, &id, steward, "check staging");
        held.close_chat(other).expect("closed");
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(
            waiting(&held, For::Chat(steward)),
            [("check staging".to_owned(), true, true)]
        );
    }

    #[test]
    fn in_a_sandboxed_project_the_person_s_ask_from_an_unsandboxed_chat_is_sandboxed_or_not_started()
     {
        // The project is sandboxed, and the steward chat was started without the sandbox: the
        // person's one-off choice for that chat. The chat they ask for from its tab is put to
        // the project's sandbox decision, not to that choice. Here the profile's program is
        // one the sandbox cannot vouch for, so the decision is "not started", and it says so.
        // What the project compiles for the persona asked is `dispatchdecision`'s own test
        // (`the_persona_chats_compiled_sandbox_is_the_projects_for_its_persona_and_not_the_askers`).
        let plane = a_plane_with_personas();
        std::fs::write(
            plane.root.join(purlis_core::plane::MANIFEST),
            format!("[persona]\ndefault = \"steward\"\n{PERSONAS}"),
        )
        .unwrap();
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let asking = Chat {
            program: "/bin/sh".to_owned(),
            name: "1".to_owned(),
            profile: Some("work".to_owned()),
            persona: Some("steward".to_owned()),
            cwd: Some(plane.root.clone()),
            unsandboxed: true,
            ..Default::default()
        };
        let steward = held.chats().start(&asking, STARTING).expect("it runs");
        let before = open_chats(&held);

        let said = ask_from_the_tab(&held, &id, steward, "devops", "check prod").unwrap_err();

        assert!(
            said.contains("runs every chat sandboxed") && said.contains("not started sandboxed"),
            "{said}"
        );
        assert_eq!(open_chats(&held), before, "never started without it");
        // And the slot it held is let go: the next ask is not counted against a chat that
        // never ran.
        assert_eq!(held.chats().lineage(steward, None, &|_| true).running, 0);
    }

    #[test]
    fn a_pair_policy_locks_is_off_the_tabs_of_chats_running_as_its_first_and_starts_nothing() {
        // D-1438-9: steward to devops is locked. A steward chat's tab does not ask devops; a
        // devops chat's tab still asks steward, and a steward chat's still asks steward.
        let plane = a_plane_with_personas();
        let host = Pretend::default();
        let planes = planes_on(&host);
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let steward = a_chat_as(&held, &plane.root, Some("steward"), &plane.root);
        let devops = a_chat_as(&held, &plane.root, Some("devops"), &plane.root);
        let locks = policy(r#"{"locked": [{"from": "steward", "to": "devops"}]}"#);
        assert_eq!(
            locks.locked_pairs().len(),
            1,
            "the fixture is a policy purlis reads"
        );
        let before = held.chats().open_now().len();

        let offer = ask_offer(held.root(), &locks, &chats_as(&held));

        assert_eq!(offer.locked, None);
        assert_eq!(offer.personas, ["devops", "steward"]);
        assert_eq!(offer.locked_for.len(), 1, "{:?}", offer.locked_for);
        let locked = &offer.locked_for[0];
        assert_eq!(
            (locked.session, locked.persona.as_str()),
            (steward, "devops")
        );
        assert!(
            locked
                .why
                .starts_with("Policy forbids steward chats dispatching to devops. Locked by"),
            "{}",
            locked.why
        );
        // The press starts nothing, and says the same sentence.
        let ask = |chat: u32, persona: &str| {
            ask_persona(
                &held,
                &id,
                &PersonAsk {
                    chat,
                    persona: persona.to_owned(),
                    name: "x y".to_owned(),
                    ask: "Is prod healthy?".to_owned(),
                },
                &locks,
                STARTING,
            )
        };
        assert_eq!(
            ask(steward, "devops").err().as_deref(),
            Some(locked.why.as_str())
        );
        assert_eq!(held.chats().open_now().len(), before);
        // The other way round, and a chat's own persona, are not that pair.
        ask(devops, "steward").expect("devops asks steward");
        ask(steward, "steward").expect("steward asks steward");
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
    fn a_chat_on_no_profile_cannot_hand_off_to_a_persona_that_names_none() {
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
