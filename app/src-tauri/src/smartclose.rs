//! Smart close, as the app runs it (ADR 0064, SI-8): the chat is sent one line asking it to
//! write its session record, and its tab closes when `charter session record` says the record
//! is saved — never on anything the chat printed.
//!
//! **The prompt is sent, and this is the deliberate exception to ADR 0061.** A curation prompt
//! is typed and never sent; this one is the operator's own click on a chat they are looking at,
//! naming one skill of charter's whose only write is the record they asked for. It is ONE write:
//! the prompt as a bracketed paste, then a carriage return ([`sent_as`]).
//!
//! **When it is sent is the board's answer, never the screen's.** A chat that is waiting gets it
//! at once. A chat mid-turn has it queued until the turn's `Stop`, because a prompt typed into a
//! turn lands in whatever the harness is doing. A chat asking the operator something mid-turn
//! — a permission or a question (`Board::asking`) — is refused: the operator answers it first.
//! A chat never prompted, or one that has reported nothing, is offered Close only ([`offer`]).
//!
//! **It ends in one of four ways, and only one of them closes the tab.**
//!
//! - `charter session record` in THAT chat says it saved ([`saved`]): the tab closes through
//!   `Held::close_chat`, as Close does. A record from a chat that is not being smart-closed
//!   closes nothing — a line anyone on the socket could send buys, at most, the close of a tab
//!   the operator already asked to close.
//! - The operator cancels it, from the tab's menu or by typing into the chat
//!   ([`Closing::operator_typed`]): the chat stays open and running.
//! - No record arrives within [`GIVES_UP_AFTER`] of the prompt: the tab goes back to normal and
//!   the window says so. **Smart close never closes a chat without its record.**
//! - The chat's program ends on its own ([`Closing::forget`] from the exit): it is an ended chat
//!   like any other, and the window says its record was not written.
//!
//! The window learns each step over [`EVENT`]; the app holds the truth, so a window that reloads
//! still has its tab closed when the record lands.
//!
//! **The smart-close pass** (#1332). A smart close under way IS the chat's pass: the app holds
//! one per chat, and only a person's act issues it, the tab's Smart close ([`begin`]) or the
//! person typing `/smart-close` into a waiting chat ([`typed`]). It is bound to that chat and
//! that close, and it ends with the close: when the tab closes, when the operator cancels, when
//! no record comes in time, or when the chat's program ends. A chat cannot issue one to itself:
//! the typed command is read from the person's own keys in that chat's pane — the line they
//! typed since their last Enter, exactly `/smart-close` or `/purlis:smart-close`, submitted
//! while the chat waited for them ([`line`], #1361). Nothing inside the chat can type into its
//! pane. The `UserPromptSubmit` report that follows only confirms the harness heard that
//! command: its bit can withhold a pass, never issue one, because anything inside the chat can
//! send a report, first or instead.
//!
//! **The record is a brokered write** (ADR 0067 §2): `purlis session record` and the MCP
//! server's `session_record` hand it to the app over the chat's hook socket, and the app
//! writes it ([`record`]) whether or not the chat holds a pass. Under a pass, the tab closes
//! when that turn ends: never while the chat's own call is still waiting for the answer.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use purlis_core::active::Place;
use purlis_core::hookwire::{Answer, Report, SessionSaved};
use purlis_core::reopen::Reopened;
use purlis_core::state::State;

use crate::planes::{Held, PlaneId, Planes};

mod line;

/// What the chat is sent: one line naming charter's `smart-close` skill in words, never a
/// `/slash` command, so every harness that reaches charter's skills can act on it (ADR 0063).
pub const PROMPT: &str =
    "Use purlis's smart-close skill to write this session's record and close the chat.";

/// How long after the prompt is sent a record may take before the tab goes back to normal. The
/// core's, because a chat's `Stop` passes on a saved-record line only while this has not
/// passed (`sessionrecord::relay`, #517): one wait, one number.
pub const GIVES_UP_AFTER: Duration = purlis_core::sessionrecord::relay::PASSED_ON_WITHIN;

/// The event the window is told each step of a smart close on.
pub const EVENT: &str = "smart-close";

/// Whether Smart close is offered on a chat, and the answer the close dialog starts on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SmartCloseOffer {
    pub available: bool,
    /// Why it is not, in a sentence, where it is not.
    pub why: Option<String>,
    /// Whether **Close** is the dialog's default: the chat has had at most one turn, where there
    /// is little to record. A chat put back with its conversation has had more than this app
    /// saw, so it never is.
    pub close_first: bool,
}

/// Where a smart close stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Waiting for the chat's turn to end before the prompt is sent.
    Queued,
    /// The prompt was sent; waiting for the record.
    Sent,
    /// The record is saved and the chat was closed.
    Closed,
    /// The operator cancelled it; the chat is open and running.
    Cancelled,
    /// No record arrived in time; the chat was left open.
    NoRecord,
    /// The chat's program ended before it wrote a record.
    Ended,
    /// The prompt, queued for the chat's turn to end, could not be written to it then; the chat
    /// was left open.
    NotSent,
    /// The chat finished a Smart close and saved its record with no pass: its harness said
    /// `/smart-close`, and no typed one of the person's was behind it. The tab stayed open, and
    /// the window offers Close tab, a plain close that grants nothing (D-1361-7).
    KeptOpen,
}

/// One step of one chat's smart close, as the window is told it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SmartClosing {
    pub plane: PlaneId,
    pub session: u32,
    pub phase: Phase,
    /// On [`Phase::Closed`] and [`Phase::KeptOpen`], the record it saved, for the window's
    /// "Session saved" notice and its **Open record** — where the line named one of this
    /// plane's records.
    pub record: Option<SavedRecord>,
}

/// The record a smart close ended on, as its view tab opens it (SI-8d).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SavedRecord {
    /// Plane-relative, as `session_record` reads it.
    pub path: String,
    pub title: String,
}

/// Told each step of every plane's smart closes.
pub type Teller = Arc<dyn Fn(SmartClosing) + Send + Sync + 'static>;

/// What decides whether a chat is offered Smart close — the app's facts about it, read in one
/// place ([`facts_of`]) so the dialog's answer and the command's refusal are one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub state: State,
    /// A permission or a question is open mid-turn (`Board::asking`).
    pub asking: bool,
    /// Prompts that started a turn, as this app heard them.
    pub turns: u32,
    /// Put back with its conversation, so it has history this app did not see.
    pub resumed: bool,
    /// A shell tab: no harness, no profile (SI-5).
    pub shell: bool,
}

/// Whether Smart close is offered on a chat with these facts, and if not, why.
pub fn offer(facts: &Facts) -> SmartCloseOffer {
    let close_first = facts.turns <= 1 && !facts.resumed;
    let why = if facts.shell {
        Some("A shell tab runs no harness, so there is no chat to write a session record.")
    } else if matches!(facts.state, State::Done | State::Failed) {
        Some("This chat's program has ended, so nothing is left to write a session record.")
    } else if facts.state == State::Unknown {
        Some(
            "purlis has heard nothing from this chat's harness, so it cannot tell when to ask \
             it for a session record.",
        )
    } else if facts.turns == 0 && !facts.resumed {
        Some("This chat was never prompted, so it has nothing to record.")
    } else if facts.asking {
        Some("This chat is asking you something. Answer it first, then smart close it.")
    } else {
        None
    };
    SmartCloseOffer {
        available: why.is_none(),
        why: why.map(str::to_owned),
        close_first,
    }
}

/// The bytes the prompt is sent as: one bracketed paste, then Enter, in one write.
pub fn sent_as() -> String {
    format!("{}\r", crate::curation::bracketed(PROMPT))
}

/// Whether `bytes`, sent from a chat's pane, is the operator typing — which cancels a smart
/// close. The terminal's own answers are not ([`crate::curation::only_the_terminal_answering`]),
/// and nor is the mouse: a wheel over a harness that tracks it sends reports, and scrolling
/// back to read what the chat is doing is not taking the chat back.
pub fn typed_by_the_operator(bytes: &[u8]) -> bool {
    !crate::curation::only_the_terminal_answering(bytes) && !only_the_mouse(bytes)
}

/// Whether `bytes` is nothing but mouse reports: SGR (`CSI < b ; x ; y M` or `m`) or the legacy
/// X10 form (`CSI M` and three bytes).
fn only_the_mouse(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        if let Some(body) = rest.strip_prefix(b"\x1b[<") {
            let params = body
                .iter()
                .take_while(|b| b.is_ascii_digit() || **b == b';')
                .count();
            match body.get(params) {
                Some(b'M' | b'm') if params > 0 => rest = &body[params + 1..],
                _ => return false,
            }
        } else if let Some(body) = rest.strip_prefix(b"\x1b[M") {
            if body.len() < 3 {
                return false;
            }
            rest = &body[3..];
        } else {
            return false;
        }
    }
    true
}

/// The smart closes under way in one plane, by chat.
///
/// **One lock for deciding and for sending.** The prompt is written under it, and the operator's
/// input takes it to cancel, so their keys either cancel before the prompt is written or land
/// after it — never inside it.
#[derive(Debug)]
pub struct Closing {
    chats: Mutex<HashMap<u32, Entry>>,
    /// The chats whose pane the person submitted `/smart-close` in while the chat waited for
    /// them, and when ([`Closing::person_typed`]).
    submitted: Mutex<HashMap<u32, std::time::Instant>>,
    /// The line the person has typed into each chat's pane since their last Enter.
    lines: Mutex<HashMap<u32, line::Line>>,
    /// The chats whose last prompt their harness reported as `/smart-close` with no typed
    /// `/smart-close` of the person's behind it: a record such a chat writes leaves its tab
    /// open, and the window offers Close tab ([`Phase::KeptOpen`], D-1361-7).
    unbacked: Mutex<HashSet<u32>>,
    /// Each smart close's number, so a timer set for one that was cancelled and begun again
    /// cannot give up on the new one.
    dealt: AtomicU64,
    gives_up_after: Mutex<Duration>,
}

#[derive(Debug, Clone)]
struct Entry {
    sent: bool,
    number: u64,
    /// The record the app wrote for it under this pass, plane-relative and with its title:
    /// the tab closes when the turn that wrote it ends.
    recorded: Option<SavedRecord>,
}

/// How long after the person submits `/smart-close` in a chat's pane the `UserPromptSubmit`
/// it caused may arrive and still be taken as theirs ([`Closing::heard_prompt`]). A hook fires
/// within milliseconds of the prompt; this is room for a loaded machine and nothing like the
/// time it would take something to wait for the person's next `/smart-close`.
pub const A_TYPED_SMART_CLOSE_IS_HEARD_WITHIN: Duration = Duration::from_secs(5);

impl Default for Closing {
    fn default() -> Self {
        Self {
            chats: Mutex::new(HashMap::new()),
            submitted: Mutex::new(HashMap::new()),
            lines: Mutex::new(HashMap::new()),
            unbacked: Mutex::new(HashSet::new()),
            dealt: AtomicU64::new(0),
            gives_up_after: Mutex::new(GIVES_UP_AFTER),
        }
    }
}

impl Closing {
    fn chats(&self) -> MutexGuard<'_, HashMap<u32, Entry>> {
        self.chats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Where chat `session`'s smart close stands, or none when it is not being smart-closed.
    /// Only a test asks one chat; the window asks for every chat at once ([`Closing::now`]).
    #[cfg(test)]
    pub fn phase(&self, session: u32) -> Option<Phase> {
        self.chats().get(&session).map(|entry| {
            if entry.sent {
                Phase::Sent
            } else {
                Phase::Queued
            }
        })
    }

    /// Every chat being smart-closed, and where each stands.
    pub fn now(&self) -> Vec<(u32, Phase)> {
        let mut now: Vec<(u32, Phase)> = self
            .chats()
            .iter()
            .map(|(session, entry)| {
                (
                    *session,
                    if entry.sent {
                        Phase::Sent
                    } else {
                        Phase::Queued
                    },
                )
            })
            .collect();
        now.sort_unstable_by_key(|(session, _)| *session);
        now
    }

    /// Lets go of chat `session`'s smart close without closing anything: it was closed, or its
    /// program ended. Answers whether it was being smart-closed.
    pub fn forget(&self, session: u32) -> bool {
        self.submitted().remove(&session);
        self.lines().remove(&session);
        self.unbacked().remove(&session);
        self.chats().remove(&session).is_some()
    }

    /// Lets go of chat `session`'s smart close because its program ended: none when it was
    /// not being smart-closed, else the record the app wrote for it under its pass, if any.
    pub fn ended(&self, session: u32) -> Option<Option<SavedRecord>> {
        self.submitted().remove(&session);
        self.lines().remove(&session);
        self.unbacked().remove(&session);
        self.chats().remove(&session).map(|entry| entry.recorded)
    }

    /// Whether chat `session` holds a smart-close pass: a person started its smart close.
    #[cfg(test)]
    pub fn holds_pass(&self, session: u32) -> bool {
        self.chats().contains_key(&session)
    }

    fn submitted(&self) -> MutexGuard<'_, HashMap<u32, std::time::Instant>> {
        self.submitted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn lines(&self) -> MutexGuard<'_, HashMap<u32, line::Line>> {
        self.lines.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn unbacked(&self) -> MutexGuard<'_, HashSet<u32>> {
        self.unbacked.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Chat `session`'s harness reported a `UserPromptSubmit` at `now`, which it says was
    /// `/smart-close` or not (`smart_close`). Answers what that comes to:
    ///
    /// - [`Heard::Pass`]: the person submitted `/smart-close` in its pane moments before, and
    ///   the harness says that is what it ran. Both, always: the keys are the key, and the
    ///   report can only withhold.
    /// - [`Heard::Unbacked`]: the report says `/smart-close` and no typed one is behind it —
    ///   a way in the keys cannot follow, or something in the chat. Remembered, so the record
    ///   it writes raises Close tab in the window and never closes anything itself.
    /// - [`Heard::Nothing`]: any other prompt, which also takes the person's `/smart-close` if
    ///   there was one, so it stands behind one prompt and no more.
    pub fn heard_prompt(&self, session: u32, smart_close: bool, now: std::time::Instant) -> Heard {
        let person = self.took_submission(session, now);
        let heard = match (smart_close, person) {
            (true, true) => Heard::Pass,
            (true, false) => Heard::Unbacked,
            (false, _) => Heard::Nothing,
        };
        if heard == Heard::Unbacked {
            self.unbacked().insert(session);
        } else {
            self.unbacked().remove(&session);
        }
        heard
    }

    /// Takes whether chat `session`'s last prompt was a `/smart-close` its harness reported
    /// with no typed one behind it ([`Heard::Unbacked`]).
    fn took_unbacked(&self, session: u32) -> bool {
        self.unbacked().remove(&session)
    }

    /// The person sent chat `session` `bytes` at `at`. Followed into the line they are typing
    /// ([`line::Line`]); an Enter in them that submits exactly `/smart-close`, while the chat
    /// waits for the person and asks nothing (`waiting`), is the person submitting it — what a
    /// smart-close pass is issued on (#1361). The terminal's own answers and the mouse are not
    /// the person typing, and are not followed.
    pub fn person_typed(
        &self,
        session: u32,
        bytes: &[u8],
        at: std::time::Instant,
        waiting: impl FnOnce() -> bool,
    ) {
        if !typed_by_the_operator(bytes) {
            return;
        }
        let submitted = self.lines().entry(session).or_default().follow(bytes);
        if submitted.is_some_and(|line| line.is_smart_close()) && waiting() {
            self.submitted().insert(session, at);
        }
    }

    /// Whether the person has a line of their own in chat `session`'s prompt, or may have: bytes
    /// typed since their last Enter, or keys since then that this could not follow
    /// ([`line::Line::Unknown`]). A line purlis types into a chat ends in Enter, which would
    /// send theirs with it, so none is typed while this holds (#1441).
    pub fn person_has_a_line(&self, session: u32) -> bool {
        self.lines()
            .get(&session)
            .is_some_and(|line| !line.is_empty())
    }

    /// Takes the person's `/smart-close` in chat `session`, answering whether they submitted
    /// one within [`A_TYPED_SMART_CLOSE_IS_HEARD_WITHIN`] of `now`. Taken whatever the answer, so
    /// one Enter stands behind one prompt and no more.
    fn took_submission(&self, session: u32, now: std::time::Instant) -> bool {
        self.submitted().remove(&session).is_some_and(|at| {
            now.checked_duration_since(at)
                .is_some_and(|since| since <= A_TYPED_SMART_CLOSE_IS_HEARD_WITHIN)
        })
    }

    /// The operator sent chat `session` `bytes`. Answers whether that cancelled its smart close:
    /// their typing, unless the chat is asking them something (`asking`) — answering the chat's
    /// own question, a permission the skill needs, is not taking the chat back.
    pub fn operator_typed(
        &self,
        session: u32,
        bytes: &[u8],
        asking: impl FnOnce() -> bool,
    ) -> bool {
        let mut chats = self.chats();
        if !chats.contains_key(&session) || !typed_by_the_operator(bytes) || asking() {
            return false;
        }
        chats.remove(&session);
        true
    }

    /// How long a record may take from now on. Only a test changes it.
    #[cfg(test)]
    pub fn give_up_after(&self, after: Duration) {
        *self
            .gives_up_after
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = after;
    }

    fn gives_up_after(&self) -> Duration {
        *self
            .gives_up_after
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// The facts [`offer`] decides on, for chat `session` of `held`.
fn facts_of(held: &Held, session: u32) -> Result<Facts, String> {
    let open = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == session)
        .ok_or_else(|| "That chat is not open any more.".to_owned())?;
    let board = held.board().glance(session);
    Ok(Facts {
        state: board.state,
        asking: board.asking,
        turns: board.turns,
        resumed: matches!(open.how, Reopened::Resumed(_)),
        shell: open.harness.is_none() && open.profile.is_none(),
    })
}

/// Whether chat `session` is offered Smart close — the close dialog's question.
pub fn offer_for(held: &Held, session: u32) -> Result<SmartCloseOffer, String> {
    facts_of(held, session).map(|facts| offer(&facts))
}

/// Starts smart-closing chat `session`: the prompt is sent now to a waiting chat and queued for
/// the next `Stop` of a running one. Refused, with [`offer`]'s sentence, where it is not offered.
/// A chat already being smart-closed is left as it is.
pub fn begin(held: &Arc<Held>, session: u32) -> Result<Phase, String> {
    let closing = held.closing();
    let mut chats = closing.chats();
    if let Some(entry) = chats.get(&session) {
        return Ok(if entry.sent {
            Phase::Sent
        } else {
            Phase::Queued
        });
    }
    // Read under the lock `reported` takes before it looks at the board, so a `Stop` landing
    // now is either seen here or finds the chat queued there — never missed by both.
    let facts = facts_of(held, session)?;
    let offered = offer(&facts);
    if let Some(why) = offered.why {
        return Err(why);
    }
    let number = closing.dealt.fetch_add(1, Ordering::SeqCst);
    let phase = if facts.state == State::Waiting {
        held.chats()
            .sessions()
            .input(session, sent_as().as_bytes())?;
        chats.insert(
            session,
            Entry {
                sent: true,
                number,
                recorded: None,
            },
        );
        Phase::Sent
    } else {
        chats.insert(
            session,
            Entry {
                sent: false,
                number,
                recorded: None,
            },
        );
        Phase::Queued
    };
    drop(chats);
    if phase == Phase::Sent {
        give_up_later(held, session, number);
    }
    held.tell_smart_close(session, phase);
    Ok(phase)
}

/// The operator cancelled chat `session`'s smart close from its tab's menu. Nothing is sent to
/// the chat, and nothing is closed.
pub fn cancel(held: &Held, session: u32) {
    if held.closing().forget(session) {
        held.tell_smart_close(session, Phase::Cancelled);
    }
}

/// The board took chat `session`'s `report`: a `UserPromptSubmit` beside the person's own
/// `/smart-close`, typed and submitted in its pane ([`Closing::person_typed`]), issues
/// the chat a pass, as the tab's Smart close does. Nothing is sent: the person's prompt is
/// already the skill's.
///
/// The report's own bit is a check, never a key (#1361): a report that says the prompt was
/// something else withholds the pass, but one that says `/smart-close` issues nothing without
/// the person's line behind it ([`Closing::heard_prompt`]). Such a smart close still writes its
/// record, and the window then offers Close tab: never a silent miss (D-1361-7).
pub fn heard(held: &Arc<Held>, report: &Report) {
    if report.event != purlis_core::state::Event::UserPromptSubmit {
        return;
    }
    match held.closing().heard_prompt(
        report.chat,
        report.detail.smart_close,
        std::time::Instant::now(),
    ) {
        Heard::Pass => typed(held, report.chat),
        // Not a warning: a picker chosen with an arrow, Tab or a history key is a person too.
        Heard::Unbacked => tracing::info!(
            "purlis: chat {} began a /smart-close that purlis did not see typed in its pane, so \
             it holds no smart-close pass and its tab stays open after its record",
            report.chat
        ),
        Heard::Nothing => {}
    }
}

/// What a chat's `UserPromptSubmit` comes to for its smart close ([`Closing::heard_prompt`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    Pass,
    Unbacked,
    Nothing,
}

/// Issues chat `session` a smart-close pass because the person typed `/smart-close` into it. A
/// chat already being smart-closed keeps the pass it has.
fn typed(held: &Arc<Held>, session: u32) {
    let closing = held.closing();
    let mut chats = closing.chats();
    if chats.contains_key(&session) {
        return;
    }
    let number = closing.dealt.fetch_add(1, Ordering::SeqCst);
    chats.insert(
        session,
        Entry {
            sent: true,
            number,
            recorded: None,
        },
    );
    drop(chats);
    give_up_later(held, session, number);
    held.tell_smart_close(session, Phase::Sent);
}

/// A report reached the plane: a smart close whose record the app wrote closes its tab once
/// that turn has ended, and a queued one whose chat has now ended its turn — waiting, and asking
/// nothing — is sent its prompt.
pub fn reported(held: &Arc<Held>, report: &Report) {
    reported_sending(held, report, |chat, prompt| {
        held.chats().sessions().input(chat, prompt.as_bytes())
    });
}

/// [`reported`], with what writes the prompt to the chat handed in: no pty refuses a write on
/// demand, so this is how a test has the write fail.
pub(crate) fn reported_sending(
    held: &Arc<Held>,
    report: &Report,
    send: impl FnOnce(u32, String) -> Result<(), String>,
) {
    let closing = held.closing();
    let mut chats = closing.chats();
    let Some(entry) = chats.get_mut(&report.chat) else {
        return;
    };
    let ready = {
        let board = held.board().glance(report.chat);
        board.state == State::Waiting && !board.asking
    };
    if ready && let Some(record) = entry.recorded.clone() {
        chats.remove(&report.chat);
        drop(chats);
        close_on_its_record(held, report.chat, Some(record));
        return;
    }
    if entry.sent || !ready {
        return;
    }
    if send(report.chat, sent_as()).is_err() {
        // It ends here and the window is told, as every other end is. Whoever takes the entry
        // out says so, and this is who took it: a chat whose program ended between the report
        // and the write finds nothing left to forget when its exit is heard, so the exit says
        // nothing and this is the one step the window gets.
        chats.remove(&report.chat);
        drop(chats);
        held.tell_smart_close(report.chat, Phase::NotSent);
        return;
    }
    entry.sent = true;
    let number = entry.number;
    drop(chats);
    give_up_later(held, report.chat, number);
    held.tell_smart_close(report.chat, Phase::Sent);
}

/// `charter session record` said a record is saved. The chat it names closes if — and only if —
/// it is being smart-closed.
pub fn saved(held: &Held, saved: &SessionSaved) {
    if !held.closing().forget(saved.chat) {
        tracing::warn!(
            "purlis: chat {} wrote a session record while it was not being smart-closed, so \
             nothing was closed",
            saved.chat
        );
        return;
    }
    // Read through the one reading of a record's path there is, so a line naming anything else
    // still closes the tab and names nothing.
    let record =
        purlis_core::sessionrecord::saved(held.root(), &saved.session_saved).map(|listed| {
            SavedRecord {
                path: listed.shown,
                title: listed.title,
            }
        });
    close_on_its_record(held, saved.chat, record);
}

/// Closes chat `session`, whose pass has been let go of, on its record, and tells the window.
fn close_on_its_record(held: &Held, session: u32, record: Option<SavedRecord>) {
    if let Err(why) = held.close_chat(session) {
        tracing::warn!(
            "purlis: chat {session} wrote its session record and did not close cleanly ({why})"
        );
    }
    held.tell_smart_closed(session, record);
}

/// Chat `ask.chat` asked the app to write its session record: a brokered write (ADR 0067 §2,
/// #1332), made with the core's own writer ([`purlis_core::sessionrecord::brokered`]) from the
/// app's record of the chat, never the request's.
///
/// It is written whether or not the chat holds a pass: a record is the chat's own to write.
/// Under a pass whose prompt has reached the chat (or that the person typed), the answer says
/// the tab closes, and it does once this turn ends ([`reported`]),
/// so the chat's own call has its answer before anything is closed under it. The chat the line
/// names is the one whose token it carries: the listener checks that before this is asked.
pub fn record(held: &Held, ask: &purlis_core::hookwire::RecordAsk) -> Answer {
    let Some(open) = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == ask.chat)
    else {
        return Answer::No {
            why: format!("chat {} is not one this app has open", ask.chat),
        };
    };
    // Resolved once, as the chat started (`chats::Open::workspace`, #1333).
    let place = open.workspace.map_or(Place::PlaneRoot, Place::Workspace);
    let asker = purlis_core::sessionrecord::Asker {
        number: ask.chat,
        place,
        persona: open.persona,
    };
    let recorded = match purlis_core::sessionrecord::brokered(
        held.root(),
        &asker,
        ask,
        chrono::Local::now().naive_local(),
    ) {
        Ok(recorded) => recorded,
        Err(why) => return Answer::No { why },
    };
    // What a task's report names as this chat's record (#1436): the path the app wrote.
    held.chats().wrote_record(ask.chat, &recorded.shown);
    // The dispatches this chat asked for are listed on its record, and the one it worked on
    // names it (#1452): from the app's record of the chat, never from the request.
    crate::dispatches::session_recorded(held, ask.chat, &recorded.shown);
    let title = purlis_core::sessionrecord::saved(held.root(), &recorded.path)
        .map_or_else(|| ask.title.trim().to_owned(), |listed| listed.title);
    let saved = SavedRecord {
        path: recorded.shown.clone(),
        title,
    };
    let (closes, smart_closing) = {
        let closing = held.closing();
        let mut chats = closing.chats();
        let smart_closing = chats.contains_key(&ask.chat);
        let closes = match chats.get_mut(&ask.chat) {
            // Only a close whose prompt has gone, or the person typed: a record the chat wrote
            // mid-turn on its own, before a queued smart close's prompt reached it, is its own
            // record and closes nothing. The prompt is still sent when the turn ends.
            Some(entry) if entry.sent => {
                entry.recorded = Some(saved.clone());
                true
            }
            _ => false,
        };
        (closes, smart_closing)
    };
    // A Smart close the harness ran with no typed `/smart-close` behind it: never a silent miss.
    // The window offers Close tab, the person's own plain close (D-1361-7). A chat already being
    // smart-closed from its tab is that close's, and is left to it.
    if !smart_closing && held.closing().took_unbacked(ask.chat) {
        held.tell_smart_kept_open(ask.chat, saved);
    }
    if closes {
        tracing::info!(
            "purlis: chat {} wrote its session record {} under its smart-close pass; the tab \
             closes when this turn ends",
            ask.chat,
            recorded.shown
        );
    } else {
        tracing::info!(
            "purlis: chat {} wrote its session record with no smart-close pass under way, so its \
             tab stays open",
            ask.chat
        );
    }
    Answer::Recorded {
        record: recorded.shown,
        closes,
        warnings: recorded.warnings,
    }
}

/// Gives up on smart close `number` of chat `session` if no record has arrived by then. On a
/// thread of its own, holding the plane only weakly: a closed project is not kept open by it.
fn give_up_later(held: &Arc<Held>, session: u32, number: u64) {
    let after = held.closing().gives_up_after();
    let held = Arc::downgrade(held);
    let _ = std::thread::Builder::new()
        .name("charter-smart-close".into())
        .spawn(move || {
            std::thread::sleep(after);
            let Some(held) = held.upgrade() else { return };
            let gave_up = {
                let mut chats = held.closing().chats();
                let still = chats
                    .get(&session)
                    .is_some_and(|entry| entry.sent && entry.number == number);
                if still { chats.remove(&session) } else { None }
            };
            match gave_up {
                // Its record is written and its turn never ended: the person asked for the
                // close, and the record it waited for is there.
                Some(Entry {
                    recorded: Some(record),
                    ..
                }) => close_on_its_record(&held, session, Some(record)),
                Some(_) => held.tell_smart_close(session, Phase::NoRecord),
                None => {}
            }
        });
}

/// Whether chat `session` is offered Smart close, and the answer the close dialog starts on.
// Its plane is a `PlaneId` the registry vouches for. Not a doc comment, because the generated
// bindings carry those.
#[tauri::command]
#[specta::specta]
pub fn smart_close_offer(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<SmartCloseOffer, String> {
    let held = planes.held(&plane)?;
    offer_for(&held, session)
}

/// Smart-closes a chat: sends it the prompt to write its session record, now or when its turn
/// ends, and closes its tab when the record is saved.
#[tauri::command]
#[specta::specta]
pub fn smart_close(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<Phase, String> {
    begin(&planes.held(&plane)?, session)
}

/// Cancels a chat's smart close. The chat stays open and running.
#[tauri::command]
#[specta::specta]
pub fn cancel_smart_close(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<(), String> {
    let held = planes.held(&plane)?;
    cancel(&held, session);
    Ok(())
}

/// Every chat of a plane being smart-closed, so a window that has just drawn it knows which
/// tabs are wrapping up.
#[tauri::command]
#[specta::specta]
pub fn smart_closing(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<SmartClosing>, String> {
    let held = planes.held(&plane)?;
    Ok(held
        .closing()
        .now()
        .into_iter()
        .map(|(session, phase)| SmartClosing {
            plane: plane.clone(),
            session,
            phase,
            record: None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_chat(state: State, turns: u32) -> Facts {
        Facts {
            state,
            asking: false,
            turns,
            resumed: false,
            shell: false,
        }
    }

    #[test]
    fn a_waiting_chat_with_turns_behind_it_is_offered_smart_close_as_the_default() {
        let offered = offer(&a_chat(State::Waiting, 4));
        assert!(offered.available);
        assert_eq!(offered.why, None);
        assert!(!offered.close_first);
    }

    #[test]
    fn a_running_chat_is_offered_smart_close_too() {
        assert!(offer(&a_chat(State::Running, 2)).available);
    }

    #[test]
    fn close_is_the_default_for_a_chat_with_at_most_one_turn() {
        let offered = offer(&a_chat(State::Waiting, 1));
        assert!(offered.available);
        assert!(offered.close_first);
    }

    #[test]
    fn a_chat_put_back_with_its_conversation_has_history_this_app_did_not_see() {
        let offered = offer(&Facts {
            resumed: true,
            ..a_chat(State::Waiting, 0)
        });
        assert!(offered.available, "a resumed chat has something to record");
        assert!(!offered.close_first);
    }

    #[test]
    fn a_chat_never_prompted_is_offered_close_only() {
        let offered = offer(&a_chat(State::Waiting, 0));
        assert!(!offered.available);
        assert!(offered.close_first);
        assert!(
            offered
                .why
                .is_some_and(|why| why.contains("never prompted"))
        );
    }

    #[test]
    fn a_chat_whose_state_is_unknown_is_offered_close_only() {
        let offered = offer(&a_chat(State::Unknown, 3));
        assert!(!offered.available);
        assert!(offered.why.is_some_and(|why| why.contains("heard nothing")));
    }

    #[test]
    fn a_chat_asking_you_something_is_answered_first() {
        let offered = offer(&Facts {
            asking: true,
            ..a_chat(State::Waiting, 3)
        });
        assert!(!offered.available);
        assert!(
            offered
                .why
                .is_some_and(|why| why.contains("Answer it first"))
        );
    }

    #[test]
    fn a_shell_tab_and_an_ended_chat_are_offered_close_only() {
        let shell = offer(&Facts {
            shell: true,
            ..a_chat(State::Waiting, 3)
        });
        assert!(!shell.available);
        assert!(shell.why.is_some_and(|why| why.contains("shell tab")));
        for ended in [State::Done, State::Failed] {
            assert!(!offer(&a_chat(ended, 3)).available);
        }
    }

    #[test]
    fn the_prompt_is_one_paste_then_enter() {
        assert_eq!(
            sent_as(),
            "\x1b[200~Use purlis's smart-close skill to write this session's record and close \
             the chat.\x1b[201~\r"
        );
    }

    #[test]
    fn keys_and_pastes_are_the_operator_typing() {
        for bytes in [
            &b"a"[..],
            b"\r",
            b"\x1b",
            b"\x1b[A",
            b"\x1b[200~hi\x1b[201~",
        ] {
            assert!(typed_by_the_operator(bytes), "a key read as not typed");
        }
    }

    #[test]
    fn the_terminal_answering_and_the_mouse_are_not_the_operator_typing() {
        for bytes in [
            &b"\x1b[I"[..],
            b"\x1b[O",
            b"\x1b[12;1R",
            b"\x1b[<64;10;5M",
            b"\x1b[<65;10;5M\x1b[<65;10;5M",
            b"\x1b[<0;3;4m",
            b"\x1b[M`!!",
        ] {
            assert!(!typed_by_the_operator(bytes), "a report read as typed");
        }
        assert!(
            typed_by_the_operator(b"\x1b[<64;10;5Mq"),
            "a key after a report"
        );
    }

    /// Whether chat 7's `keys`, each one write into a waiting chat, leave a `/smart-close`
    /// the person submitted for the next `UserPromptSubmit` to take (#1361).
    fn the_person_submitted_smart_close(keys: &[&[u8]]) -> bool {
        let closing = Closing::default();
        let now = std::time::Instant::now();
        for key in keys {
            closing.person_typed(7, key, now, || true);
        }
        closing.took_submission(7, now)
    }

    #[test]
    fn a_report_racing_ahead_of_the_person_s_own_prompt_finds_no_smart_close_behind_it() {
        // The person's Enter on their own words: a forged `/smart-close` report that arrives
        // before the hook's own takes nothing.
        assert!(!the_person_submitted_smart_close(&[b"go on", b"\r"]));
        assert!(!the_person_submitted_smart_close(&[b"go on\r"]));
    }

    #[test]
    fn an_enter_that_submitted_nothing_is_no_smart_close() {
        assert!(!the_person_submitted_smart_close(&[b"\r"]));
        // A picker's choice, which the board may not have flagged.
        assert!(!the_person_submitted_smart_close(&[b"\x1b[B", b"\r"]));
        assert!(!the_person_submitted_smart_close(&[b"2", b"\r"]));
    }

    #[test]
    fn a_shift_or_option_enter_adds_a_line_and_is_no_smart_close() {
        let newline = purlis_core::harness::Harness::ClaudeCode
            .newline()
            .as_bytes();
        assert!(!the_person_submitted_smart_close(&[
            b"/smart-close",
            newline
        ]));
        assert!(!the_person_submitted_smart_close(&[b"/smart-close\x1b\r"]));
        // Nor the Enter that follows it: the prompt is two lines.
        assert!(!the_person_submitted_smart_close(&[
            b"/smart-close",
            newline,
            b"\r"
        ]));
    }

    #[test]
    fn the_person_typing_smart_close_and_enter_is_a_smart_close() {
        assert!(the_person_submitted_smart_close(&[b"/smart-close\r"]));
        let mut keys: Vec<&[u8]> = b"/purlis:smart-clsoe".chunks(1).collect();
        keys.extend([&b"\x7f"[..], b"\x7f", b"\x7f", b"o", b"s", b"e", b"\r"]);
        assert!(the_person_submitted_smart_close(&keys));
    }

    #[test]
    fn a_smart_close_into_a_chat_not_waiting_for_the_person_is_none() {
        let closing = Closing::default();
        let now = std::time::Instant::now();
        closing.person_typed(7, b"/smart-close\r", now, || false);
        assert!(!closing.took_submission(7, now));
    }

    /// What chat 7's prompt comes to when the person's `keys`, each one write into a waiting
    /// chat, are followed by a `UserPromptSubmit` that says `/smart-close` or not.
    fn heard_after(keys: &[&[u8]], smart_close: bool) -> (Heard, bool) {
        let closing = Closing::default();
        let now = std::time::Instant::now();
        for key in keys {
            closing.person_typed(7, key, now, || true);
        }
        let heard = closing.heard_prompt(7, smart_close, now);
        (heard, closing.took_unbacked(7))
    }

    #[test]
    fn a_picker_s_enter_on_sm_is_a_pass_when_the_harness_says_smart_close() {
        assert_eq!(heard_after(&[b"/sm", b"\r"], true), (Heard::Pass, false));
        assert_eq!(heard_after(&[b"/purlis:sm\r"], true), (Heard::Pass, false));
        assert_eq!(
            heard_after(&[b"/smart-close now\r"], true),
            (Heard::Pass, false)
        );
    }

    #[test]
    fn a_picker_s_enter_on_s_alone_is_no_pass() {
        assert_eq!(heard_after(&[b"/s", b"\r"], true), (Heard::Unbacked, true));
        assert_eq!(heard_after(&[b"/", b"\r"], true), (Heard::Unbacked, true));
    }

    #[test]
    fn a_picker_s_enter_on_sm_is_no_pass_when_the_harness_says_otherwise() {
        assert_eq!(heard_after(&[b"/sm\r"], false), (Heard::Nothing, false));
    }

    #[test]
    fn a_picker_chosen_with_an_arrow_is_no_pass() {
        assert_eq!(
            heard_after(&[b"/sm", b"\x1b[B", b"\r"], true),
            (Heard::Unbacked, true)
        );
    }

    #[test]
    fn a_report_with_no_keys_behind_it_is_remembered_until_the_next_prompt() {
        let closing = Closing::default();
        let now = std::time::Instant::now();
        assert_eq!(closing.heard_prompt(7, true, now), Heard::Unbacked);
        assert_eq!(closing.heard_prompt(7, false, now), Heard::Nothing);
        assert!(
            !closing.took_unbacked(7),
            "a later prompt kept an earlier report"
        );
        assert_eq!(closing.heard_prompt(7, true, now), Heard::Unbacked);
        assert!(closing.took_unbacked(7));
        assert!(!closing.took_unbacked(7), "taken once");
    }
}
