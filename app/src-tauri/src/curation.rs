//! Curation actions as the window reaches them: the "Curate ▸" menus, the palette's rows, and a
//! chat opened with the action's prompt typed into it and never sent (ADR 0061, SI-2).
//!
//! **What a subject is offered is the core's answer, asked again every time.**
//! `charter_core::curation::resolve` says which actions there are, who runs each, where and with
//! what text; this layer converts it for the window and never decides any of it. The window
//! names an action by its id and nothing else, and [`curate`] resolves the subject again before
//! it opens anything, so the prompt typed into the chat is the one the core renders now — never
//! text the window held, and never one from before a persona's file changed.
//!
//! **The prompt is typed once the chat's harness has started, and never sent.**
//! It is held here, in the app, per chat ([`Typed`]), until after the first hook report of a
//! `SessionStart` that began a session reaches the plane's board — hooks only (spec: nothing
//! parses harness output to decide anything). It is then written into the terminal as ONE
//! bracketed paste, `ESC [200~ … ESC [201~`, with nothing after it: no carriage return, no line
//! feed, no Enter ([`bracketed`]), and only once the terminal hands keys to the harness rather
//! than editing lines itself ([`type_once_it_reads_keys`]). The operator reads it and presses
//! Enter. Codex, whose `SessionStart` comes with the first prompt rather than at launch, is
//! typed into instead once its terminal is raw and has then been quiet for [`QUIET`]
//! ([`type_once_raw_and_quiet`]): the kernel's line discipline and the moment bytes last
//! arrived, never what they said. opencode is not typed into at all: it goes quiet while still
//! starting, and a paste then is lost (`Harness::ready_to_type`).
//!
//! **A prompt the operator could not read is not typed, and one they typed past is dropped**
//! (ADR 0061, amended 2026-09-27). A prompt the harness would draw as a placeholder refuses the
//! start (`Harness::why_drawn_as_a_placeholder`). A prompt is held until the moment it is
//! written, and any input the operator sends the chat before then drops it
//! ([`Typed::operator_sent`]) — except the terminal's own answers to the harness's questions
//! ([`only_the_terminal_answering`]).
//!
//! **Bracketed whether or not the program asked for it.** Whether a program has turned
//! bracketed paste on (`?2004h`) is a mode of the terminal the core draws, and the core's
//! engine offers no way to ask it; the three harnesses charter starts all turn it on, and a
//! paste is still one write of text with no carriage return in it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use charter_core::curation::{self, Resolved, Source, Subject};
use charter_core::harness::{Harness, ReadyToType};
use charter_core::hookwire::Report;
use charter_core::state::Event;

use crate::planes::{Held, PlaneId, Planes};
use charter_core::engine::Size;
use charter_core::reopen::Chat;

#[cfg(test)]
use charter_core::curation::{PASTE_BEGINS, PASTE_ENDS};

/// One curation action a subject is offered, as the window draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct CurationAction {
    /// `charter/<id>` or `<persona>/<id>` — what the window hands back to [`curate`].
    pub id: String,
    pub label: String,
    /// The persona that declared it, or none for one of charter's own.
    pub declared_by: Option<String>,
    /// The persona the chat starts as, or none for no persona.
    pub runner: Option<String>,
    /// The directory the chat starts in.
    pub cwd: String,
    /// The prompt, as it would be typed. Shown, never sent from here.
    pub prompt: String,
}

/// An action the core left out of a subject's list, and why. Never dropped silently: the menu
/// draws it as a row that cannot run, with `why` as its reason.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct LeftOut {
    /// The file it was declared in, where the core's sentence names one.
    pub what: String,
    /// The core's whole sentence.
    pub why: String,
}

/// What one subject is offered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SubjectCurations {
    /// `workspace:<name>`, `persona:<name>` or `plane` — the core's own spelling of a subject.
    pub subject: String,
    /// The name the operator knows it by: the workspace's or persona's, or the plane's
    /// directory.
    pub name: String,
    pub actions: Vec<CurationAction>,
    pub left_out: Vec<LeftOut>,
    /// Why this subject has no list at all — a workspace deleted a moment ago, say.
    pub trouble: Option<String>,
}

/// Every subject the window asked about, and whether any action can be opened right now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Curations {
    pub subjects: Vec<SubjectCurations>,
    /// Why no curation chat can be opened in this project right now — its default harness
    /// profile cannot be typed into — or none when one can. The menu draws every action
    /// disabled with this as its reason, rather than leaving the operator to find out on a
    /// click.
    pub cannot: Option<String>,
}

/// A curation chat that started: what the window needs to put its tab on the right strip.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Curating {
    pub session: u32,
    /// The chat's name, which is its number, as a handed-off chat's is.
    pub name: String,
    /// What its tab says: the action and its subject, `Safe remove · smart-ide`.
    pub label: String,
    pub persona: Option<String>,
    /// The harness, by the word the plane calls it.
    pub harness: Option<String>,
    /// The workspace it is filed under, or none when it works at the plane root or anywhere
    /// else outside one.
    pub workspace: Option<String>,
}

/// What each subject the window names is offered (`subjects` in the core's spelling:
/// `workspace:<name>`, `persona:<name>`, `plane`), in the order asked.
// Its plane is a `PlaneId` the registry vouches for, like every other command's. Not a doc
// comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
// On a blocking thread: each subject's list is read out of its personas' curation files (SC-2).
pub async fn curation_offers(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    subjects: Vec<String>,
) -> Result<Curations, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window("reading the curation actions", move || {
        Ok(offers(&root, &subjects))
    })
    .await
}

/// Opens a chat for one curation action on one subject, with the action's prompt typed into it
/// once its harness has started, and never sent.
///
/// The harness is the project's default profile, as a new chat's is; the persona and the
/// directory are the core's answer. A refusal comes back before anything starts.
#[tauri::command]
#[specta::specta]
pub fn curate(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    subject: String,
    action: String,
    columns: u16,
    rows: u16,
) -> Result<Curating, String> {
    let held = planes.held(&plane)?;
    open(&held, &subject, &action, Size { columns, rows })
}

/// Every subject's list, against a root the registry has already vouched for.
pub fn offers(root: &Path, subjects: &[String]) -> Curations {
    Curations {
        subjects: subjects.iter().map(|spec| offered(root, spec)).collect(),
        cannot: launch_profile(root).err(),
    }
}

/// One subject's list.
fn offered(root: &Path, spec: &str) -> SubjectCurations {
    let listed = Subject::parse(spec).and_then(|subject| {
        let resolution = curation::resolve(root, &subject)?;
        Ok((subject, resolution))
    });
    match listed {
        Ok((subject, resolution)) => SubjectCurations {
            subject: spec.to_owned(),
            name: name_of(root, &subject),
            actions: resolution.actions.iter().map(drawn).collect(),
            left_out: resolution.warnings.into_iter().map(left_out).collect(),
            trouble: None,
        },
        Err(why) => SubjectCurations {
            subject: spec.to_owned(),
            name: spec.to_owned(),
            actions: Vec::new(),
            left_out: Vec::new(),
            trouble: Some(why),
        },
    }
}

/// An action as the window draws it.
fn drawn(action: &Resolved) -> CurationAction {
    CurationAction {
        id: action.id.clone(),
        label: action.label.clone(),
        declared_by: match &action.source {
            Source::Charter => None,
            Source::Persona(persona) => Some(persona.clone()),
        },
        runner: action.runner.clone(),
        cwd: action.cwd.display().to_string(),
        prompt: action.prompt.clone(),
    }
}

/// A warning the core gave, with the file it names pulled out for the row's words. The core
/// writes each one `<file> is not offered: <why>`; one that does not read so is still shown,
/// whole, under a plain title.
fn left_out(warning: String) -> LeftOut {
    let what = warning.split_once(" is not offered").map_or_else(
        || "a curation action".to_owned(),
        |(file, _)| file.to_owned(),
    );
    LeftOut { what, why: warning }
}

/// The name the operator knows a subject by: `{subject.name}` in a prompt, spelled as the core
/// spells it.
fn name_of(root: &Path, subject: &Subject) -> String {
    match &subject.name {
        Some(name) => name.clone(),
        None => root.file_name().map_or_else(
            || root.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
    }
}

/// The profile a curation chat starts on: the project's default, or — with none named — the
/// first the picker lists, which is the row a new chat's picker starts on. Refused when that
/// profile's harness cannot be typed into on its start.
///
/// **A default that cannot start refuses; it never falls back.** The first profile is for a
/// project that names no default. One that names a default charter will not use — a profile
/// gone, or `charter.local.toml` refused because git would carry it — would otherwise open the
/// chat on some other harness or account than the one the operator chose.
fn launch_profile(root: &Path) -> Result<String, String> {
    let (profile, kind) = default_profile(root, "no curation chat was opened")?;
    when_typed(
        &profile,
        Harness::of_kind(&kind),
        charter_core::harness_card::named(root, &kind).as_ref(),
    )?;
    Ok(profile)
}

/// The profile a chat charter opens on the operator's behalf starts on, and its `kind`: the
/// project's default, or — with none named — the first the picker lists. A default that
/// cannot start refuses, ending in `so <outcome>.`, and never falls back.
pub(crate) fn default_profile(root: &Path, outcome: &str) -> Result<(String, String), String> {
    let (set, check) = charter_core::profiles::for_launch(root);
    if let Some(wanted) = &set.default_refused {
        let why = set
            .refused
            .iter()
            .find(|refused| refused.name == *wanted)
            .map(|refused| format!(" {}", refused.reason))
            .or_else(|| (!check.passes()).then(|| format!(" {}", check.fix)))
            .unwrap_or_default();
        return Err(format!(
            "The default profile '{wanted}' cannot start here, so {outcome}.{why}"
        ));
    }
    let profile = set
        .default
        .as_deref()
        .and_then(|name| set.get(name))
        .or_else(|| set.profiles().first())
        .ok_or_else(|| format!("This project has no harness profile, so {outcome}."))?;
    Ok((profile.name.clone(), profile.kind.clone()))
}

/// When a chat on `harness` can have a prompt typed into it, or why it cannot, in a sentence:
/// the line its capability card (`card`) says for it (HP-19, ADR 0072 §3). One answer for the
/// menu's rows and for the start itself.
fn when_typed(
    profile: &str,
    harness: Option<Harness>,
    card: Option<&charter_core::harness_card::Card>,
) -> Result<ReadyToType, String> {
    harness
        .and_then(Harness::ready_to_type)
        .ok_or_else(|| match cannot_be_typed_into(card) {
            Some(lacks) => format!(
                "No curation chat can open on the default profile '{profile}': {lacks} Make \
                 another profile the default to curate from here."
            ),
            None => format!(
                "The default profile '{profile}' runs a program charter has not measured, so it \
                 cannot tell when to type a curation prompt into it."
            ),
        })
}

/// What a harness's card says while a prompt cannot be typed into it: its line and its label,
/// or none where there is no card to say it — a program no declaration describes.
pub(crate) fn cannot_be_typed_into(
    card: Option<&charter_core::harness_card::Card>,
) -> Option<String> {
    card.and_then(|card| card.lacks(charter_core::harness_card::READY_TO_TYPE))
}

/// What a curation chat's tab says: `<label> · <subject>`, held to a chat name's length.
fn tab_label(label: &str, subject: &str) -> String {
    let whole = format!("{label} · {subject}");
    let most = charter_core::reopen::MOST_LABEL;
    if whole.chars().count() <= most {
        return whole;
    }
    let mut cut: String = whole.chars().take(most - 1).collect();
    cut.push('…');
    cut
}

/// Opens the chat. The prompt is held for it BEFORE it starts, because its harness can report
/// its start before `start_ready` returns, and let go again when nothing started.
fn open(held: &Arc<Held>, spec: &str, action: &str, size: Size) -> Result<Curating, String> {
    let root = held.root().to_path_buf();
    let subject = Subject::parse(spec)?;
    let resolution = curation::resolve(&root, &subject)?;
    let chosen = resolution
        .actions
        .into_iter()
        .find(|offered| offered.id == action)
        .ok_or_else(|| {
            format!("'{action}' is not offered on {spec} any more, so nothing was opened.")
        })?;
    let profile = launch_profile(&root)?;
    let name_shown = name_of(&root, &subject);
    let label = tab_label(&chosen.label, &name_shown);
    let typed = start_typed(
        held,
        ChatTyped {
            profile,
            persona: chosen.runner.clone(),
            cwd: chosen.cwd.clone(),
            label: label.clone(),
            prompt: chosen.prompt,
            then_a_space: false,
        },
        size,
        when_typed,
        |why| {
            format!(
                "{why}, which you could not read before sending it, so no curation chat was \
                 opened. Shorten the prompt to one line that names a skill, and let the skill \
                 hold the steps."
            )
        },
    )?;
    let (session, name, ready) = (typed.session, typed.name, typed.ready);
    Ok(Curating {
        session,
        name,
        label,
        persona: chosen.runner,
        harness: ready.harness.map(|harness| harness.name().to_owned()),
        // The one question the window files every chat by, and the one `start::ready` tells
        // the chat its workspace by: a chat at the plane root, or in a persona's directory, is
        // in none.
        workspace: charter_core::workspaces::Plane::open(&root).workspace_of(&chosen.cwd),
    })
}

/// A chat to open with a prompt typed into it and never sent: a curation action's, or a run of
/// the first task (FR-28).
pub struct ChatTyped {
    /// The harness profile it starts on.
    pub profile: String,
    pub persona: Option<String>,
    /// Where it starts.
    pub cwd: std::path::PathBuf,
    /// What its tab says.
    pub label: String,
    /// What is typed into it.
    pub prompt: String,
    /// Whether one space follows the prompt inside its paste: a reference to a file (FM-9),
    /// so the operator's first word does not stick to it.
    pub then_a_space: bool,
}

/// A typed chat that started.
pub struct StartedTyped {
    pub session: u32,
    /// Its name, which is its number.
    pub name: String,
    /// What `start::ready` answered for it: its harness and its directory.
    pub ready: charter_core::start::Ready,
    /// Whether its prompt is held to be typed: false only from
    /// [`start_typed_where_it_can_be`], on a harness it cannot be typed into.
    pub typed: bool,
}

/// Opens `chat` with its prompt typed once its harness has started, and never sent.
///
/// `cannot_type` says when the resolved harness can be typed into, or why not, in the caller's
/// words; `not_drawn` turns the core's reason a paste would be drawn as a placeholder into the
/// caller's refusal (ADR 0061, amended 2026-09-27: a prompt the operator could not read is not
/// typed). **The prompt is held BEFORE the chat starts**, because its harness can report its
/// start before `start_ready` returns, and let go again when nothing started.
pub fn start_typed(
    held: &Arc<Held>,
    chat: ChatTyped,
    size: Size,
    cannot_type: impl FnOnce(
        &str,
        Option<Harness>,
        Option<&charter_core::harness_card::Card>,
    ) -> Result<ReadyToType, String>,
    not_drawn: impl FnOnce(String) -> String,
) -> Result<StartedTyped, String> {
    start_typed_where_it_can_be(
        held,
        chat,
        size,
        |profile, harness, card| cannot_type(profile, harness, card).map(Some),
        not_drawn,
    )
}

/// [`start_typed`], for a caller that starts the chat even on a harness its prompt cannot be
/// typed into: `when` answers `Ok(None)` for one, and the chat starts with nothing held and
/// nothing typed (`StartedTyped::typed` is false). "Start a chat here" (FM-9) does, and puts
/// the reference on the clipboard instead.
pub fn start_typed_where_it_can_be(
    held: &Arc<Held>,
    chat: ChatTyped,
    size: Size,
    when: impl FnOnce(
        &str,
        Option<Harness>,
        Option<&charter_core::harness_card::Card>,
    ) -> Result<Option<ReadyToType>, String>,
    not_drawn: impl FnOnce(String) -> String,
) -> Result<StartedTyped, String> {
    let root = held.root().to_path_buf();
    let number = held.chats().sessions().deal();
    let name = number.to_string();
    let ready = charter_core::start::ready(
        &charter_core::start::Start {
            profile: Some(chat.profile.clone()),
            persona: chat.persona.clone(),
            name: name.clone(),
            cwd: Some(chat.cwd.clone()),
            resume: None,
            // The picker's footer box is one operator choice for one chat, and nobody made it
            // for this one — a handoff's rule.
            show_footer: false,
            resuming: None,
            without_sandbox: None,
        },
        &root,
    )?;
    // The same question the caller's menu asked, of the harness this start actually resolved.
    // Its harness's card, built-in or declared, for the line a refusal says (HP-19).
    let card = charter_core::profiles::for_launch(&root)
        .0
        .get(&chat.profile)
        .and_then(|profile| charter_core::harness_card::named(&root, &profile.kind));
    let when = when(&chat.profile, ready.harness, card.as_ref())?;
    let inside = if chat.then_a_space {
        format!("{} ", curation::pasted(&chat.prompt))
    } else {
        curation::pasted(&chat.prompt)
    };
    if let Some(why) = ready
        .harness
        .filter(|_| when.is_some())
        .and_then(|h| h.why_drawn_as_a_placeholder(&inside))
    {
        return Err(not_drawn(why));
    }
    let record = Chat {
        program: ready.program.clone(),
        // What the RECORD keeps: the profile's own words. Never the prompt — a relaunch
        // resumes the conversation, and the prompt was only ever typed once.
        args: Vec::new(),
        cwd: ready.cwd.clone(),
        name: name.clone(),
        resume: ready.session.clone(),
        active: false,
        profile: Some(chat.profile),
        persona: chat.persona,
        show_footer: false,
        pinned: false,
        number: Some(number),
        label: Some(chat.label),
        from: None,
        renamed_from: None,
        ..Default::default()
    };
    let paste = if chat.then_a_space {
        curation::bracketed_then_a_space(&chat.prompt)
    } else {
        curation::bracketed(&chat.prompt)
    };
    if let Some(until) = when {
        held.typed().hold_paste(number, paste, until);
    }
    let session = held
        .chats()
        .start_ready(&record, &ready, size)
        .inspect_err(|_| held.typed().forget(number))?;
    if when == Some(ReadyToType::WhenRawAndQuiet) {
        type_when_raw_and_quiet(held, session);
    }
    Ok(StartedTyped {
        session,
        name,
        ready,
        typed: when.is_some(),
    })
}

/// The prompts waiting for their chat's harness to start, by chat.
///
/// **The app's, not the window's**: a window can close, reload or be a second window on the
/// same project, and the chat's harness reports to the plane whichever of them is showing it.
///
/// **A prompt is held until the moment it is written**, and taken and written under one lock
/// ([`Typed::type_now`]). Input the operator sends the chat ([`Typed::operator_sent`]) takes
/// the same lock, so it either drops the prompt before the paste is written or is written after
/// it — never a paste landing in the middle of what the operator began (SI-2, Q29).
#[derive(Debug, Default)]
pub struct Typed {
    waiting: Mutex<HashMap<u32, Holding>>,
}

/// One chat's waiting prompt, and the moment it waits for.
#[derive(Debug)]
struct Holding {
    /// The one bracketed paste it is typed as, built when it was held.
    paste: String,
    until: ReadyToType,
    /// Its start was reported, and a typist is waiting for its terminal to read keys.
    started: bool,
}

impl Typed {
    /// Holds `prompt` for chat `session` until its harness reports its start. Tests only: a
    /// start holds the paste it built ([`Self::hold_paste`]).
    #[cfg(test)]
    pub fn hold(&self, session: u32, prompt: String) {
        self.hold_paste(
            session,
            bracketed(&prompt),
            ReadyToType::WhenItReportsItsStart,
        );
    }

    /// Holds `paste`, already one bracketed paste ([`charter_core::curation::bracketed`] or
    /// [`charter_core::curation::bracketed_then_a_space`]), for chat `session` until `until`.
    pub fn hold_paste(&self, session: u32, paste: String, until: ReadyToType) {
        self.held().insert(
            session,
            Holding {
                paste,
                until,
                started: false,
            },
        );
    }

    /// Holds `prompt` for chat `session` until its terminal is raw and quiet
    /// ([`type_once_raw_and_quiet`] types it then). A start report does not type it: on a
    /// harness held this way, that report comes with the first prompt, so it drops it.
    #[cfg(test)]
    pub fn hold_until_quiet(&self, session: u32, prompt: String) {
        self.hold_paste(session, bracketed(&prompt), ReadyToType::WhenRawAndQuiet);
    }

    /// Types chat `session`'s prompt with `write`, as the one paste it is typed as, if it is
    /// still held — and lets go of it, so it is typed once. The lock is held across the write,
    /// which never blocks (the session queues it), so no input the operator sends can come
    /// between the prompt being taken and being written.
    pub fn type_now(&self, session: u32, write: impl FnOnce(&str) -> Result<(), String>) {
        let mut held = self.held();
        if let Some(holding) = held.remove(&session) {
            // A chat that ended between the question and this write has nothing to type into,
            // and nothing is owed to anyone about it.
            let _ = write(&holding.paste);
        }
    }

    /// Lets go of a chat's prompt without typing it: the chat ended, closed, never started, or
    /// was not ready in time.
    pub fn forget(&self, session: u32) {
        self.held().remove(&session);
    }

    /// The operator sent chat `session` something of their own before its prompt was typed:
    /// the prompt is dropped, never typed after it. The app knows this itself — it is the one
    /// sending the input — so no harness's output is read to decide it.
    pub fn operator_sent(&self, session: u32) {
        self.forget(session);
    }

    /// A hook report the plane's board took. True when it is the `SessionStart` that began the
    /// session of a chat whose prompt waits for it, the first time: a typist should now wait for
    /// the chat's terminal to read keys and type it ([`type_once_it_reads_keys`]).
    ///
    /// A chat that reports a prompt, a turn's end or its own end before its prompt is typed has
    /// had something sent into it already, and a prompt typed after that would be typed into
    /// the middle of it: it is dropped, never typed late.
    pub fn heard(&self, report: &Report) -> bool {
        match report.event {
            Event::SessionStart if report.detail.started.began_a_session() => {
                let mut held = self.held();
                match held.get_mut(&report.chat) {
                    Some(holding)
                        if holding.until == ReadyToType::WhenItReportsItsStart
                            && !holding.started =>
                    {
                        holding.started = true;
                        true
                    }
                    Some(holding) if holding.until == ReadyToType::WhenRawAndQuiet => {
                        held.remove(&report.chat);
                        false
                    }
                    _ => false,
                }
            }
            Event::UserPromptSubmit | Event::Stop | Event::SessionEnd => {
                self.forget(report.chat);
                false
            }
            Event::SessionStart | Event::Notification | Event::SubagentStop => false,
        }
    }

    /// Whether a prompt is waiting for chat `session`.
    pub fn waiting(&self, session: u32) -> bool {
        self.held().contains_key(&session)
    }

    fn held(&self) -> MutexGuard<'_, HashMap<u32, Holding>> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Whether `bytes`, sent from a chat's pane, is nothing but the terminal answering questions
/// its program asked — never the operator's own input.
///
/// **Why the pane's input is not all the operator's.** xterm.js hands the pane its answers to a
/// program's questions through the same `onData` it hands keys through. Measured at start
/// (2026-09-27): Claude Code 2.1.283 asks for device attributes, the terminal's version and
/// the kitty keyboard flags, and turns focus reports on; Codex 0.147.0 asks for the cursor's
/// position, device attributes and both colours, and turns focus reports on — so the tab a
/// curation chat opens in answers several times, and says "focus in", before its prompt is
/// typed. None of that is the operator, and none of it drops the prompt.
///
/// These are the terminal's own answers, read from the input the app itself is sending, never
/// anything the harness wrote. Each is whole in one send: device attributes (`CSI ? … c`,
/// `CSI > … c`), a cursor position (`CSI … R`), focus in and out (`CSI I`, `CSI O`), the kitty
/// keyboard flags (`CSI ? … u`), a mode report (`CSI … $ y`), a status report (`CSI 0 n`), a
/// window report (`CSI … t`), and any OSC or DCS string (colours, the terminal's version).
/// Shift+F3 reads `CSI 1 ; 2 R`, the same bytes as a cursor at row 1, column 2, and is taken
/// for the answer; every other key, a paste and a mouse report are the operator's.
pub fn only_the_terminal_answering(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        match one_answer(rest) {
            Some(len) => rest = &rest[len..],
            None => return false,
        }
    }
    true
}

/// The length of the terminal's answer `bytes` begins with, or `None` when it begins with
/// anything else ([`only_the_terminal_answering`]).
fn one_answer(bytes: &[u8]) -> Option<usize> {
    const ST: &[u8] = b"\x1b\\";
    let string_end = |body: &[u8], bel: bool| {
        body.windows(ST.len())
            .position(|w| w == ST)
            .map(|at| at + ST.len())
            .into_iter()
            .chain(
                bel.then(|| body.iter().position(|b| *b == 0x07).map(|at| at + 1))
                    .flatten(),
            )
            .min()
    };
    if let Some(body) = bytes.strip_prefix(b"\x1b]") {
        return string_end(body, true).map(|len| 2 + len);
    }
    if let Some(body) = bytes.strip_prefix(b"\x1bP") {
        return string_end(body, false).map(|len| 2 + len);
    }
    let body = bytes.strip_prefix(b"\x1b[")?;
    let params_len = body
        .iter()
        .take_while(|b| matches!(b, b'0'..=b'9' | b';' | b'?' | b'>' | b'=' | b'$'))
        .count();
    let params = &body[..params_len];
    let answer = match body.get(params_len)? {
        b'c' => matches!(params.first(), Some(b'?' | b'>' | b'=')),
        b'R' => !params.is_empty(),
        b'I' | b'O' => params.is_empty(),
        b'u' => params.first() == Some(&b'?'),
        b'y' => params.ends_with(b"$"),
        b'n' => params == b"0",
        b't' => !params.is_empty(),
        _ => false,
    };
    answer.then_some(2 + params_len + 1)
}

/// How long a typed prompt waits, after its chat's start, for the harness to read keys itself.
/// Claude Code's canonical moment after its trust question lasted about a third of a second.
pub const KEYS_READ_WITHIN: Duration = Duration::from_secs(10);

/// How often the terminal is asked again while it waits.
const ASKED_EVERY: Duration = Duration::from_millis(20);

/// Types a chat's held prompt once its terminal hands keys to its program rather than editing
/// lines itself (`edits_lines`), and never while it does.
///
/// **Why it waits at all.** A harness's `SessionStart` says it has started, not that it is
/// reading keys. On Claude Code 2.1.283 in a folder it has just been told to trust, the hook
/// fires inside a third of a second when the terminal is back in canonical mode: text written
/// then is echoed onto the screen and cut at its first line break — measured, with the second
/// line of a two-line prompt lost. So this asks the terminal, a bounded number of times, and
/// types the moment it is raw. A terminal still editing lines at `within` is not typed into at
/// all: a prompt cut in two is worse than none, and it says so on stderr.
///
/// The prompt stays held while this waits, so input the operator sends meanwhile drops it
/// ([`Typed::operator_sent`]) and this types nothing. A platform that cannot say is typed into
/// at once, and a chat that has gone is not typed into. Each answer is the terminal's line
/// discipline, never anything the harness printed.
pub fn type_once_it_reads_keys(chat: &impl Waiting, within: Duration) {
    let until = Instant::now() + within;
    loop {
        if !chat.still_held() {
            return;
        }
        match chat.edits_lines() {
            // Gone: nothing to type into, and nothing owed to anyone about it.
            Err(_) => return chat.let_go(),
            Ok(Some(true)) if Instant::now() < until => std::thread::sleep(ASKED_EVERY),
            Ok(Some(true)) => {
                chat.let_go();
                tracing::warn!(
                    "charter: a curation chat's terminal was still editing lines {}s after its \
                     harness started, so its prompt was not typed",
                    within.as_secs()
                );
                return;
            }
            Ok(Some(false) | None) => return chat.type_now(),
        }
    }
}

/// How long a Codex chat must have written nothing, after its terminal went raw, before its
/// prompt is typed.
///
/// **Measured** (ADR 0061, amended 2026-09-27). codex-cli 0.147.0 draws its first screen in
/// three bursts within about 0.25 s of going raw, and its longest silence between them was
/// 0.18 s idle and 0.3 s with every core busy. It took a paste at any moment after going raw —
/// 80 ms in, mid-draw, it still landed whole in its input — so this is not what makes the paste
/// land. It is what keeps one out of a screen still animating: its welcome, before a folder is
/// trusted, redraws every 80 ms and is never this quiet.
pub const QUIET: Duration = Duration::from_secs(1);

/// How long a Codex chat is waited on, from its start, for its terminal to be raw and quiet.
/// Longer than [`KEYS_READ_WITHIN`], because this counts from the start and the quiet period is
/// inside it.
pub const QUIET_WITHIN: Duration = Duration::from_secs(15);

/// The quiet period a prompt waits for, and how long it waits for one.
#[derive(Debug, Clone, Copy)]
pub struct Wait {
    pub quiet: Duration,
    pub within: Duration,
}

/// A chat a prompt is waiting to be typed into, as [`type_once_raw_and_quiet`] asks it. An
/// `Err` is a chat, or a project, that has gone.
pub trait Waiting {
    /// The terminal's line discipline: `Some(true)` while it is canonical, `None` where the
    /// platform cannot say.
    fn edits_lines(&self) -> Result<Option<bool>, String>;
    /// How long the program has written nothing.
    fn quiet_for(&self) -> Result<Duration, String>;
    /// Whether its prompt still waits: not dropped by the operator's input, a prompt, a turn's
    /// end, the chat's end or its close.
    fn still_held(&self) -> bool;
    /// Types its prompt, as one paste, if it is still held — taken and written at once
    /// ([`Typed::type_now`]), so it is typed once and never after the operator's own input.
    fn type_now(&self);
    /// Lets go of its prompt without typing it.
    fn let_go(&self);
}

/// Types a Codex chat's prompt once its terminal is raw and has then been quiet for
/// `wait.quiet`, and never otherwise.
///
/// **Codex does not say it is ready.** It reports its start only with the first prompt, so no
/// hook marks the moment (`Harness::ready_to_type`). What is left that is not the harness's
/// output is the kernel's: whether the terminal is raw (`edits_lines`), and when bytes last
/// arrived (`quiet_for`). Nothing here reads what the harness wrote, so no wording, colour or
/// layout of its can change when a prompt is typed; a harness that keeps drawing — Codex's
/// animated welcome, before a folder is trusted — is simply never quiet, and is not typed into.
///
/// The quiet period counts from the later of the terminal going raw and its last output, so a
/// harness silent while it still edits lines is not typed into the moment it goes raw. A
/// prompt dropped while it waits is not typed. One not typed within `wait.within` is let go of
/// and stderr says so: typed late, it would land in whatever the operator had begun.
pub fn type_once_raw_and_quiet(chat: &impl Waiting, wait: Wait) {
    let until = Instant::now() + wait.within;
    let mut raw_since: Option<Instant> = None;
    loop {
        if !chat.still_held() {
            return;
        }
        let asked = chat
            .edits_lines()
            .and_then(|edits| Ok((edits, chat.quiet_for()?)));
        let Ok((edits, quiet_for)) = asked else {
            // Gone: nothing to type into, and nothing owed to anyone about it.
            return chat.let_go();
        };
        if edits == Some(true) {
            raw_since = None;
        } else {
            let since = *raw_since.get_or_insert_with(Instant::now);
            if quiet_for.min(since.elapsed()) >= wait.quiet {
                return chat.type_now();
            }
        }
        if Instant::now() >= until {
            chat.let_go();
            tracing::warn!(
                "charter: a curation chat's terminal was not raw and quiet {}s after it \
                 started, so its prompt was not typed",
                wait.within.as_secs()
            );
            return;
        }
        std::thread::sleep(ASKED_EVERY);
    }
}

/// One of a project's chats, asked through the project for as long as it is open.
struct InPlane {
    held: Weak<Held>,
    session: u32,
}

impl InPlane {
    fn held(&self) -> Result<Arc<Held>, String> {
        self.held
            .upgrade()
            .ok_or_else(|| "the project was closed".to_owned())
    }
}

impl Waiting for InPlane {
    fn edits_lines(&self) -> Result<Option<bool>, String> {
        Ok(self
            .held()?
            .chats()
            .sessions()
            .readiness(self.session)?
            .edits_lines)
    }
    fn quiet_for(&self) -> Result<Duration, String> {
        Ok(self
            .held()?
            .chats()
            .sessions()
            .readiness(self.session)?
            .quiet_for)
    }
    fn still_held(&self) -> bool {
        self.held()
            .is_ok_and(|held| held.typed().waiting(self.session))
    }
    fn type_now(&self) {
        if let Ok(held) = self.held() {
            held.typed().type_now(self.session, |text| {
                held.chats().sessions().input(self.session, text.as_bytes())
            });
        }
    }
    fn let_go(&self) {
        if let Ok(held) = self.held() {
            held.typed().forget(self.session);
        }
    }
}

/// Waits, on a thread of its own, to type chat `session`'s held prompt once its terminal reads
/// keys ([`type_once_it_reads_keys`]) — off the hook socket's thread, which every other chat's
/// report waits on. Weak, so a closed project is not kept open by a prompt waiting in it.
pub fn type_when_it_reads_keys(held: &Arc<Held>, session: u32) {
    let chat = InPlane {
        held: Arc::downgrade(held),
        session,
    };
    let _ = std::thread::Builder::new()
        .name("charter-curation-typing".into())
        .spawn(move || type_once_it_reads_keys(&chat, KEYS_READ_WITHIN));
}

/// Waits, on a thread of its own, to type chat `session`'s held prompt once its terminal is
/// raw and quiet. Weak, so a closed project is not kept open by a prompt waiting in it.
fn type_when_raw_and_quiet(held: &Arc<Held>, session: u32) {
    let chat = InPlane {
        held: Arc::downgrade(held),
        session,
    };
    let _ = std::thread::Builder::new()
        .name("charter-curation-typing".into())
        .spawn(move || {
            type_once_raw_and_quiet(
                &chat,
                Wait {
                    quiet: QUIET,
                    within: QUIET_WITHIN,
                },
            );
        });
}

/// `prompt` as one bracketed paste, with nothing after it.
///
/// **Nothing in it can submit.** Line breaks are line feeds, inside the paste, where a harness
/// reads them as newlines in its input. Every other control character is taken out — an
/// `ESC [201~` in a persona's prompt would otherwise end the paste early and leave the rest to
/// be read as keys, and a carriage return there is Enter — and so is the trailing white space a
/// file's last line leaves, so the operator's cursor ends on the prompt's last word.
pub fn bracketed(prompt: &str) -> String {
    curation::bracketed(prompt)
}

/// Whether `bytes` would submit anything: a carriage return or a line feed outside a paste.
/// The tests' one definition of "Enter was sent".
#[cfg(test)]
fn submits(bytes: &str) -> bool {
    let Some(inside) = bytes
        .strip_prefix(PASTE_BEGINS)
        .and_then(|rest| rest.strip_suffix(PASTE_ENDS))
    else {
        return bytes.contains(['\r', '\n']);
    };
    inside.contains('\r') || inside.contains(PASTE_ENDS) || inside.contains('\x1b')
}

#[cfg(test)]
mod tests {
    use charter_core::hookwire::Conversation;
    use charter_core::state::{Detail, Started};

    use super::*;

    fn report(chat: u32, event: Event, started: Started) -> Report {
        Report {
            chat,
            event,
            conversation: Conversation::Unknown,
            pid: None,
            agent: None,
            detail: Detail {
                started,
                ..Detail::default()
            },
        }
    }

    fn start(chat: u32) -> Report {
        report(chat, Event::SessionStart, Started::Freshly)
    }

    #[test]
    fn a_prompt_is_typed_as_one_bracketed_paste_with_nothing_after_it() {
        let typed = bracketed("Retire smart-ide.\nFirst audit it.\n\nThen remove it.\n");

        assert_eq!(
            typed,
            "\x1b[200~Retire smart-ide.\nFirst audit it.\n\nThen remove it.\x1b[201~"
        );
        assert!(!submits(&typed));
        assert!(!typed.contains('\r'), "a carriage return is Enter");
        assert!(typed.ends_with(PASTE_ENDS), "nothing may follow the paste");
    }

    #[test]
    fn a_carriage_return_in_a_prompt_becomes_a_newline_inside_the_paste() {
        // A prompt file saved with Windows line endings, or a lone CR someone pasted into it.
        let typed = bracketed("one\r\ntwo\rthree");

        assert_eq!(typed, "\x1b[200~one\ntwo\nthree\x1b[201~");
    }

    #[test]
    fn a_prompt_cannot_end_the_paste_early_and_type_the_rest_as_keys() {
        // `ESC [201~` inside the text would close the paste, and what followed would be keys —
        // the line feed after it an Enter.
        let typed = bracketed("before\x1b[201~\nafter\x07\x7f");

        assert_eq!(typed, "\x1b[200~before[201~\nafter\x1b[201~");
        assert!(!submits(&typed));
    }

    #[test]
    fn the_test_s_own_check_sees_an_enter_where_there_is_one() {
        // `submits` is what every test here trusts, so it is itself shown to fail.
        assert!(submits("\x1b[200~text\x1b[201~\r"));
        assert!(submits("\x1b[200~te\rxt\x1b[201~"));
        assert!(submits("plain text\n"));
        assert!(!submits("\x1b[200~te\nxt\x1b[201~"));
    }

    /// What `typed` types for chat `session` now, if anything.
    fn typed_now(typed: &Typed, session: u32) -> Option<String> {
        let mut out = None;
        typed.type_now(session, |text| {
            out = Some(text.to_owned());
            Ok(())
        });
        out
    }

    #[test]
    fn a_held_prompt_is_typed_once_after_the_first_start_its_chat_reports() {
        let typed = Typed::default();
        typed.hold(4, "Compact steward.".to_owned());

        assert!(
            !typed.heard(&start(3)),
            "another chat's start types nothing"
        );
        assert!(typed.heard(&start(4)));
        assert!(!typed.heard(&start(4)), "a second start types nothing more");
        assert!(typed.waiting(4), "held until it is written");
        assert_eq!(
            typed_now(&typed, 4).as_deref(),
            Some("\x1b[200~Compact steward.\x1b[201~")
        );
        assert_eq!(typed_now(&typed, 4), None, "typed once");
        assert!(!typed.waiting(4));
    }

    #[test]
    fn a_compaction_is_not_the_start_a_prompt_waits_for() {
        let typed = Typed::default();
        typed.hold(4, "Compact steward.".to_owned());

        assert!(!typed.heard(&report(4, Event::SessionStart, Started::Compacted)));
        assert!(typed.waiting(4));
    }

    #[test]
    fn a_prompt_is_dropped_rather_than_typed_after_something_was_sent() {
        for first in [Event::UserPromptSubmit, Event::Stop, Event::SessionEnd] {
            let typed = Typed::default();
            typed.hold(4, "Compact steward.".to_owned());

            assert!(!typed.heard(&report(4, first, Started::Unsaid)));
            assert!(!typed.heard(&start(4)), "typed late after {first:?}");
            assert_eq!(typed_now(&typed, 4), None);
        }
    }

    #[test]
    fn a_prompt_forgotten_when_its_chat_ends_is_never_typed() {
        let typed = Typed::default();
        typed.hold(4, "Compact steward.".to_owned());

        typed.forget(4);

        assert!(!typed.heard(&start(4)));
        assert_eq!(typed_now(&typed, 4), None);
    }

    #[test]
    fn input_the_operator_sends_before_the_start_drops_the_prompt() {
        let typed = Typed::default();
        typed.hold(4, "Compact steward.".to_owned());

        typed.operator_sent(4);

        assert!(!typed.heard(&start(4)));
        assert_eq!(typed_now(&typed, 4), None);
    }

    #[test]
    fn input_the_operator_sends_after_the_start_but_before_the_paste_drops_the_prompt() {
        // The start was heard and a typist waits for the terminal to read keys: the paste has
        // not landed, so the operator's input still drops it.
        for hold in [Typed::hold, Typed::hold_until_quiet] {
            let typed = Typed::default();
            hold(&typed, 4, "Compact steward.".to_owned());
            let _ = typed.heard(&start(4));

            typed.operator_sent(4);

            assert_eq!(typed_now(&typed, 4), None);
            assert!(!typed.waiting(4));
        }
    }

    #[test]
    fn another_chat_s_input_leaves_a_prompt_waiting() {
        let typed = Typed::default();
        typed.hold(4, "Compact steward.".to_owned());

        typed.operator_sent(3);

        assert!(typed.waiting(4));
    }

    #[test]
    fn the_terminal_s_own_answers_are_told_from_the_operator_s_input() {
        for answer in [
            &b"\x1b[?1;2c"[..],
            b"\x1b[>0;276;0c",
            b"\x1b[24;80R",
            b"\x1b[I",
            b"\x1b[O",
            b"\x1b[?0u",
            b"\x1b[?2004;1$y",
            b"\x1b[0n",
            b"\x1b[8;24;80t",
            b"\x1b]11;rgb:1e1e/1e1e/1e1e\x1b\\",
            b"\x1b]10;rgb:ffff/ffff/ffff\x07",
            b"\x1bP>|xterm.js(6.0.0)\x1b\\",
            b"\x1b[I\x1b[?1;2c",
        ] {
            assert!(
                only_the_terminal_answering(answer),
                "{:?}",
                String::from_utf8_lossy(answer)
            );
        }
        for input in [
            &b"a"[..],
            b"\r",
            b"\x1b",
            b"\x1b[A",
            b"\x1bOQ",
            b"\x1b[3~",
            b"\x1b[200~hello\x1b[201~",
            b"\x1b[<0;10;5M",
            b"\x1b[M !!",
            b"\x1b[99;5u",
            b"\x1b[I!",
            b"\x1b]11;rgb:1e1e/1e1e/1e1e",
            b"",
        ] {
            assert!(
                !only_the_terminal_answering(input),
                "{:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn a_prompt_waits_for_the_terminal_to_hand_keys_to_the_harness() {
        let chat = AChat::new(|t| Ok(Some(t < Duration::from_millis(100))), |t| t);

        type_once_it_reads_keys(&chat, Duration::from_secs(5));

        let written = chat.written();
        assert_eq!(written.len(), 1, "{written:?}");
        assert_eq!(written[0].1, PASTED);
        assert!(
            written[0].0 >= Duration::from_millis(100),
            "typed while canonical"
        );
    }

    #[test]
    fn a_terminal_that_never_hands_keys_over_is_never_typed_into() {
        let chat = AChat::new(|_| Ok(Some(true)), |t| t);

        type_once_it_reads_keys(&chat, Duration::from_millis(60));

        assert!(chat.written().is_empty());
        assert!(!chat.still_held(), "a prompt given up on stays waiting");
    }

    #[test]
    fn a_chat_gone_while_its_prompt_waits_is_not_typed_into() {
        let chat = AChat::new(
            |t| {
                if t < Duration::from_millis(50) {
                    Ok(Some(true))
                } else {
                    Err("gone".to_owned())
                }
            },
            |t| t,
        );

        type_once_it_reads_keys(&chat, Duration::from_secs(5));

        assert!(chat.written().is_empty());
    }

    #[test]
    fn a_terminal_that_cannot_say_is_typed_into_at_once() {
        let chat = AChat::new(|_| Ok(None), |t| t);

        type_once_it_reads_keys(&chat, Duration::from_secs(5));

        assert_eq!(chat.written().len(), 1);
    }

    #[test]
    fn a_prompt_dropped_while_it_waits_for_keys_is_never_typed() {
        let mut chat = AChat::new(|_| Ok(Some(true)), |t| t);
        chat.held = Box::new(|t| t < Duration::from_millis(100));
        let began = Instant::now();

        type_once_it_reads_keys(&chat, Duration::from_secs(5));

        assert!(chat.written().is_empty());
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "it kept waiting for a prompt that was gone"
        );
    }

    /// A chat the raw-and-quiet waiter is asked about, answering from the time since it was
    /// made: `raw(t)` is `edits_lines`, `quiet(t)` how long its program has written nothing, and
    /// `held(t)` whether its prompt is still waiting. Writes down what it is sent, and when.
    struct AChat {
        born: Instant,
        raw: Box<dyn Fn(Duration) -> Result<Option<bool>, String> + Send + Sync>,
        quiet: Box<dyn Fn(Duration) -> Duration + Send + Sync>,
        held: Box<dyn Fn(Duration) -> bool + Send + Sync>,
        prompt: Mutex<Option<String>>,
        written: Mutex<Vec<(Duration, String)>>,
    }

    impl AChat {
        fn new(
            raw: impl Fn(Duration) -> Result<Option<bool>, String> + Send + Sync + 'static,
            quiet: impl Fn(Duration) -> Duration + Send + Sync + 'static,
        ) -> Self {
            Self {
                born: Instant::now(),
                raw: Box::new(raw),
                quiet: Box::new(quiet),
                held: Box::new(|_| true),
                prompt: Mutex::new(Some(bracketed("Retire alpha.\nAudit it first."))),
                written: Mutex::new(Vec::new()),
            }
        }

        fn written(&self) -> Vec<(Duration, String)> {
            self.written.lock().unwrap().clone()
        }
    }

    impl Waiting for AChat {
        fn edits_lines(&self) -> Result<Option<bool>, String> {
            (self.raw)(self.born.elapsed())
        }
        fn quiet_for(&self) -> Result<Duration, String> {
            Ok((self.quiet)(self.born.elapsed()))
        }
        fn still_held(&self) -> bool {
            (self.held)(self.born.elapsed()) && self.prompt.lock().unwrap().is_some()
        }
        fn type_now(&self) {
            if !(self.held)(self.born.elapsed()) {
                return;
            }
            if let Some(text) = self.prompt.lock().unwrap().take() {
                self.written
                    .lock()
                    .unwrap()
                    .push((self.born.elapsed(), text));
            }
        }
        fn let_go(&self) {
            self.prompt.lock().unwrap().take();
        }
    }

    const PASTED: &str = "\x1b[200~Retire alpha.\nAudit it first.\x1b[201~";
    const A_WHILE: Wait = Wait {
        quiet: Duration::from_millis(150),
        within: Duration::from_secs(5),
    };

    #[test]
    fn a_harness_that_goes_raw_late_is_typed_into_a_quiet_period_after_it_does() {
        // Silent from its start, canonical for 200 ms: the quiet period counts from the moment
        // it went raw, not from its last output, so a paste cannot land while it still edits
        // lines.
        let chat = AChat::new(|t| Ok(Some(t < Duration::from_millis(200))), |t| t);

        type_once_raw_and_quiet(&chat, A_WHILE);

        let written = chat.written();
        assert_eq!(written.len(), 1, "{written:?}");
        assert_eq!(written[0].1, PASTED);
        assert!(!submits(&written[0].1));
        assert!(
            written[0].0 >= Duration::from_millis(350),
            "typed {:?} after it started, before a quiet period had passed since it went raw",
            written[0].0
        );
    }

    #[test]
    fn a_harness_that_keeps_printing_is_never_typed_into_and_its_prompt_is_let_go() {
        let chat = AChat::new(|_| Ok(Some(false)), |_| Duration::from_millis(20));

        type_once_raw_and_quiet(
            &chat,
            Wait {
                within: Duration::from_millis(500),
                ..A_WHILE
            },
        );

        assert!(chat.written().is_empty());
        assert!(!chat.still_held(), "a prompt given up on stays waiting");
    }

    #[test]
    fn a_harness_raw_and_quiet_is_typed_into_once_with_nothing_after_the_paste() {
        // It draws for 300 ms after going raw, then stops.
        let chat = AChat::new(
            |_| Ok(Some(false)),
            |t| t.saturating_sub(Duration::from_millis(300)),
        );

        type_once_raw_and_quiet(&chat, A_WHILE);

        let written = chat.written();
        assert_eq!(
            written
                .iter()
                .map(|(_, text)| text.as_str())
                .collect::<Vec<_>>(),
            [PASTED]
        );
        assert!(!written[0].1.contains('\r'), "a carriage return is Enter");
        assert!(
            written[0].0 >= Duration::from_millis(450),
            "typed at {:?}, while it was still drawing",
            written[0].0
        );
    }

    #[test]
    fn a_prompt_dropped_while_it_waits_for_quiet_is_never_typed() {
        let mut chat = AChat::new(|_| Ok(Some(false)), |_| Duration::ZERO);
        chat.held = Box::new(|t| t < Duration::from_millis(100));
        let began = Instant::now();

        type_once_raw_and_quiet(&chat, A_WHILE);

        assert!(chat.written().is_empty());
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "it kept waiting for a prompt that was gone"
        );
    }

    #[test]
    fn a_chat_gone_while_it_waits_for_quiet_is_not_typed_into() {
        let chat = AChat::new(
            |t| {
                if t < Duration::from_millis(50) {
                    Ok(Some(false))
                } else {
                    Err("gone".to_owned())
                }
            },
            |_| Duration::ZERO,
        );

        type_once_raw_and_quiet(&chat, A_WHILE);

        assert!(chat.written().is_empty());
    }

    #[test]
    fn a_terminal_that_cannot_say_whether_it_is_raw_waits_for_quiet_alone() {
        let chat = AChat::new(|_| Ok(None), |t| t);

        type_once_raw_and_quiet(&chat, A_WHILE);

        assert_eq!(chat.written().len(), 1);
    }

    #[test]
    fn a_prompt_waiting_for_quiet_is_dropped_by_a_start_report_rather_than_typed() {
        // Codex reports `SessionStart` inside the first turn, opencode's shim at the first
        // prompt: that report means something was sent already.
        let typed = Typed::default();
        typed.hold_until_quiet(4, "Compact steward.".to_owned());

        assert!(!typed.heard(&start(4)));
        assert!(!typed.waiting(4));
        assert_eq!(typed_now(&typed, 4), None);
    }

    #[test]
    fn a_prompt_waiting_for_quiet_is_typed_once_as_one_paste() {
        let typed = Typed::default();
        typed.hold_until_quiet(4, "Compact steward.\n".to_owned());

        assert_eq!(
            typed_now(&typed, 4).as_deref(),
            Some("\x1b[200~Compact steward.\x1b[201~")
        );
        assert_eq!(typed_now(&typed, 4), None);
    }

    #[test]
    fn a_tab_says_the_action_and_its_subject_within_a_chat_name_s_length() {
        assert_eq!(
            tab_label("Safe remove", "smart-ide"),
            "Safe remove · smart-ide"
        );
        let long = tab_label("Compact & improve", &"w".repeat(80));
        assert_eq!(long.chars().count(), charter_core::reopen::MOST_LABEL);
        assert!(long.ends_with('…'));
        assert!(charter_core::reopen::label(&long).is_ok());
    }

    #[test]
    fn a_warning_names_the_file_it_left_out() {
        let said = left_out(
            "personas/ops/curation/bad.md is not offered: no label. `charter persona lint \
             ops` lists every problem"
                .to_owned(),
        );
        assert_eq!(said.what, "personas/ops/curation/bad.md");
        assert!(said.why.contains("no label"));
        assert_eq!(
            left_out("something else".to_owned()).what,
            "a curation action"
        );
    }

    /// A plane with a workspace `alpha`, a persona `ops` declaring one good action and one
    /// broken one, and a profile `work` of `kind` running a stand-in harness that puts its
    /// terminal in raw mode, says it is ready and writes down every byte it is sent.
    struct Plane {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
    }

    impl Plane {
        fn new(kind: &str) -> Self {
            let dir = tempfile::tempdir().expect("a directory");
            let root = dir.path().join("plane");
            let alpha = root.join("workspaces").join("alpha");
            std::fs::create_dir_all(&alpha).unwrap();
            std::fs::write(alpha.join("workspace.md"), "# alpha\n").unwrap();
            std::fs::write(root.join(charter_core::plane::MANIFEST), "").unwrap();
            let ops = root.join("personas").join("ops");
            std::fs::create_dir_all(ops.join("curation")).unwrap();
            std::fs::write(
                ops.join("persona.md"),
                "---\nname: ops\nrole: R\nvault: none\ndelegate-when: w\n---\n\n# ops\n",
            )
            .unwrap();
            std::fs::write(
                ops.join("curation").join("tidy.md"),
                "---\nlabel: Tidy\non: workspace\nruns-in: subject\n---\n\n\
                 Tidy {subject.name}.\nThen say so.\n",
            )
            .unwrap();
            std::fs::write(
                ops.join("curation").join("long.md"),
                format!(
                    "---\nlabel: Long\non: workspace\n---\n\n{}\n",
                    "x".repeat(1001)
                ),
            )
            .unwrap();
            std::fs::write(
                ops.join("curation").join("bad.md"),
                "---\non: workspace\n---\n\nNo label.\n",
            )
            .unwrap();
            let program = stand_in::program(
                &root,
                "claude-stand-in",
                &format!(
                    "#!/bin/sh\nprintf '%s|%s' \"$CHARTER_WORKSPACE\" \"$CHARTER_PLANE_ROOT_SESSION\" \
                     > {:?}\nstty raw -echo\n: > {:?}\nexec cat > {:?}\n",
                    root.join("standing"),
                    root.join("ready"),
                    root.join("typed")
                ),
            );
            std::fs::write(
                root.join(charter_core::profiles::LOCAL_FILE),
                format!(
                    "[harness]\ndefault = \"work\"\n\n[harness.work]\nkind = {kind:?}\n\
                     command = [{:?}]\n",
                    program.display().to_string()
                ),
            )
            .unwrap();
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
    }

    fn planes() -> Planes {
        Planes::telling(std::sync::Arc::new(|_| {}), crate::Shipped::default(), None)
    }

    const SIZE: Size = Size {
        columns: 80,
        rows: 24,
    };

    #[test]
    fn a_workspace_is_offered_charter_s_actions_then_each_persona_s_with_what_was_left_out() {
        let plane = Plane::new("claude");

        let said = offers(&plane.root, &["workspace:alpha".to_owned()]);

        assert_eq!(said.cannot, None);
        let alpha = &said.subjects[0];
        assert_eq!(alpha.name, "alpha");
        assert_eq!(
            alpha
                .actions
                .iter()
                .map(|a| (a.id.as_str(), a.declared_by.as_deref()))
                .collect::<Vec<_>>(),
            [
                ("charter/safe-remove", None),
                ("charter/compact", None),
                ("ops/long", Some("ops")),
                ("ops/tidy", Some("ops")),
            ]
        );
        let tidy = &alpha.actions[3];
        assert_eq!(tidy.runner.as_deref(), Some("ops"));
        assert!(tidy.cwd.ends_with("workspaces/alpha"), "{}", tidy.cwd);
        assert_eq!(tidy.prompt, "Tidy alpha.\nThen say so.");
        assert_eq!(alpha.left_out.len(), 1);
        assert_eq!(alpha.left_out[0].what, "personas/ops/curation/bad.md");
    }

    #[test]
    fn a_subject_that_is_not_there_says_why_instead_of_listing_nothing() {
        let plane = Plane::new("claude");

        let said = offers(
            &plane.root,
            &["workspace:gone".to_owned(), "plane".to_owned()],
        );

        assert!(said.subjects[0].actions.is_empty());
        assert!(said.subjects[0].trouble.is_some());
        assert_eq!(said.subjects[1].name, "plane");
        assert_eq!(said.subjects[1].trouble, None);
    }

    #[test]
    fn a_codex_default_can_curate() {
        let plane = Plane::new("codex");

        let said = offers(&plane.root, &["workspace:alpha".to_owned()]);

        assert_eq!(said.cannot, None);
        assert!(!said.subjects[0].actions.is_empty());
    }

    #[test]
    fn a_project_whose_default_is_opencode_says_it_cannot_curate_on_every_list() {
        let plane = Plane::new("opencode");

        let said = offers(&plane.root, &["workspace:alpha".to_owned()]);

        let cannot = said
            .cannot
            .expect("an opencode default cannot take a typed prompt");
        assert!(
            cannot.contains("'work'") && cannot.contains("opencode"),
            "{cannot}"
        );
        assert!(
            !said.subjects[0].actions.is_empty(),
            "the list is still shown"
        );
    }

    #[cfg(unix)]
    #[test]
    fn choosing_an_action_opens_a_chat_as_its_runner_where_it_runs_and_types_its_prompt_unsent() {
        let plane = Plane::new("claude");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let curating = open(&held, "workspace:alpha", "ops/tidy", SIZE).expect("it opens");

        assert_eq!(curating.label, "Tidy · alpha");
        assert_eq!(curating.persona.as_deref(), Some("ops"));
        assert_eq!(curating.harness.as_deref(), Some("claude"));
        assert_eq!(curating.workspace.as_deref(), Some("alpha"));
        assert_eq!(curating.name, curating.session.to_string());
        let opened = held
            .chats()
            .open_now()
            .into_iter()
            .find(|open| open.session == curating.session)
            .expect("the app has it open");
        assert_eq!(
            opened.profile.as_deref(),
            Some("work"),
            "the default profile"
        );
        assert_eq!(opened.persona.as_deref(), Some("ops"));
        assert_eq!(opened.label.as_deref(), Some("Tidy · alpha"));
        assert_eq!(
            opened.cwd.as_deref(),
            Some(held.root().join("workspaces").join("alpha").as_path())
        );
        assert!(
            held.typed().waiting(curating.session),
            "held until it starts"
        );

        // The harness reports its start the way Claude Code does: its pid and the
        // conversation charter chose for it.
        let conversation = held
            .chats()
            .record()
            .chats
            .into_iter()
            .find(|chat| chat.number == Some(curating.session))
            .and_then(|chat| chat.resume)
            .expect("charter chose a conversation")
            .to_string();
        let ready = plane.root.join("ready");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !ready.exists() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        charter_core::hookwire::send(
            held.hooks().socket().expect("listening"),
            Some(&held.hooks().token_for(curating.session)),
            &Report {
                chat: curating.session,
                event: Event::SessionStart,
                conversation: Conversation::Named(conversation),
                pid: Some(std::process::id()),
                agent: None,
                detail: Detail::default(),
            },
        )
        .expect("the report reaches the plane");

        let wanted = "\x1b[200~Tidy alpha.\nThen say so.\x1b[201~";
        let typed = plane.root.join("typed");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while std::fs::read(&typed).map_or(0, |b| b.len()) < wanted.len()
            && std::time::Instant::now() < until
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        let bytes = std::fs::read_to_string(&typed).unwrap_or_default();
        assert_eq!(bytes, wanted, "exactly one paste, and no Enter after it");
        assert!(!submits(&bytes));
        held.close_chat(curating.session).unwrap();
    }

    #[test]
    fn a_default_profile_that_cannot_start_refuses_rather_than_falling_back() {
        let plane = Plane::new("claude");
        std::fs::write(
            plane.root.join(charter_core::profiles::LOCAL_FILE),
            "[harness]\ndefault = \"gone\"\n",
        )
        .unwrap();

        let said = offers(&plane.root, &["plane".to_owned()]);

        let cannot = said
            .cannot
            .expect("a default that is not there is not replaced");
        assert!(cannot.contains("'gone'"), "{cannot}");
    }

    #[cfg(unix)]
    #[test]
    fn an_action_run_at_the_plane_root_opens_a_plane_root_chat() {
        // `charter/safe-remove` runs at the plane root, because its workspace is going away:
        // its chat is on the plane root's tab, and is told it is at the root (SI-1).
        let plane = Plane::new("claude");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let curating =
            open(&held, "workspace:alpha", "charter/safe-remove", SIZE).expect("it opens");

        assert_eq!(curating.workspace, None);
        assert_eq!(curating.label, "Safe remove · alpha");
        let standing = plane.root.join("standing");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !standing.exists() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(std::fs::read_to_string(&standing).unwrap_or_default(), "|1");
        held.close_chat(curating.session).unwrap();
    }

    #[test]
    fn an_action_that_is_no_longer_offered_opens_nothing() {
        let plane = Plane::new("claude");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let refused = open(&held, "workspace:alpha", "ops/gone", SIZE).unwrap_err();

        assert!(refused.contains("ops/gone"), "{refused}");
        assert!(held.chats().open_now().is_empty());
    }

    /// Waits for `path` to hold at least `len` bytes, then a moment for anything after them.
    #[cfg(unix)]
    fn typed_into(path: &Path, len: usize) -> String {
        let until = Instant::now() + Duration::from_secs(20);
        while std::fs::read(path).map_or(0, |b| b.len()) < len && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(300));
        std::fs::read_to_string(path).unwrap_or_default()
    }

    #[cfg(unix)]
    #[test]
    fn a_codex_chat_is_typed_into_once_raw_and_quiet_with_no_hook_and_no_enter() {
        let plane = Plane::new("codex");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let curating = open(&held, "workspace:alpha", "ops/tidy", SIZE).expect("it opens");

        assert_eq!(curating.harness.as_deref(), Some("codex"));
        let wanted = "\x1b[200~Tidy alpha.\nThen say so.\x1b[201~";
        let bytes = typed_into(&plane.root.join("typed"), wanted.len());
        assert_eq!(bytes, wanted, "exactly one paste, and no Enter after it");
        assert!(!submits(&bytes));
        assert!(!held.typed().waiting(curating.session));
        held.close_chat(curating.session).unwrap();
    }

    #[test]
    fn an_opencode_default_opens_nothing_rather_than_pasting_into_its_silent_boot() {
        let plane = Plane::new("opencode");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let refused = open(&held, "workspace:alpha", "charter/compact", SIZE).unwrap_err();

        // What opencode lacks, in its capability card's line (HP-19).
        assert!(refused.contains("finished starting"), "{refused}");
        assert!(held.chats().open_now().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_codex_chat_whose_operator_sends_a_prompt_first_is_never_typed_into() {
        let plane = Plane::new("codex");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");
        let curating = open(&held, "workspace:alpha", "ops/tidy", SIZE).expect("it opens");

        // What Codex 0.147.0 reports inside its first turn, before the quiet period is up.
        for event in [Event::SessionStart, Event::UserPromptSubmit] {
            charter_core::hookwire::send(
                held.hooks().socket().expect("listening"),
                Some(&held.hooks().token_for(curating.session)),
                &Report {
                    chat: curating.session,
                    event,
                    conversation: Conversation::Unknown,
                    pid: None,
                    agent: None,
                    detail: Detail::default(),
                },
            )
            .expect("the report reaches the plane");
        }
        std::thread::sleep(QUIET + Duration::from_secs(1));

        assert_eq!(
            std::fs::read_to_string(plane.root.join("typed")).unwrap_or_default(),
            "",
            "typed after the operator had sent something"
        );
        held.close_chat(curating.session).unwrap();
    }

    #[test]
    fn a_prompt_codex_would_draw_as_a_placeholder_opens_nothing() {
        let plane = Plane::new("codex");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let refused = open(&held, "workspace:alpha", "ops/long", SIZE).unwrap_err();

        assert!(
            refused.contains("1001") && refused.contains("1000"),
            "{refused}"
        );
        assert!(held.chats().open_now().is_empty());
        assert!(!held.typed().waiting(1));
    }

    #[test]
    fn a_prompt_claude_code_would_draw_as_a_placeholder_opens_nothing() {
        let plane = Plane::new("claude");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let refused = open(&held, "workspace:alpha", "ops/long", SIZE).unwrap_err();

        assert!(
            refused.contains("Claude Code") && refused.contains("800") && refused.contains("1001"),
            "{refused}"
        );
        assert!(refused.contains("no curation chat was opened"), "{refused}");
        assert!(held.chats().open_now().is_empty());
        assert!(!held.typed().waiting(1));
    }

    #[test]
    fn claude_code_is_typed_into_on_its_start_codex_once_raw_and_quiet_and_opencode_never() {
        let card = |harness| Some(charter_core::harness_card::built_in_card(harness));
        assert_eq!(
            when_typed(
                "claude",
                Some(Harness::ClaudeCode),
                card(Harness::ClaudeCode).as_ref()
            ),
            Ok(ReadyToType::WhenItReportsItsStart)
        );
        assert_eq!(
            when_typed("work", Some(Harness::Codex), card(Harness::Codex).as_ref()),
            Ok(ReadyToType::WhenRawAndQuiet)
        );
        let opencode = when_typed(
            "work",
            Some(Harness::Opencode),
            card(Harness::Opencode).as_ref(),
        )
        .unwrap_err();
        // What it lacks, in its capability card's line and label (HP-19).
        assert!(
            opencode.contains("'work'")
                && opencode.contains(
                    "opencode cannot have a prompt typed in for you, because charter cannot tell \
                     when it has finished starting. See What opencode can do here."
                ),
            "{opencode}"
        );
        let custom = when_typed("custom", None, None).unwrap_err();
        assert!(custom.contains("'custom'"), "{custom}");
    }

    #[test]
    fn a_harness_the_project_declares_is_refused_in_its_cards_line() {
        // HP-19's review, M1: charter types into a harness only through an adapter it ships,
        // so a declared harness is refused whatever its terminal says — in its card's words.
        let declared = charter_core::harness_declaration::parse(
            "name = \"eager\"\nprogram = \"eager\"\n[terminal]\nready_to_type = \"raw-and-quiet\"\n",
            charter_core::harness_declaration::Origin::Project,
            "harnesses/eager.toml",
        )
        .expect("a declaration");
        let card = charter_core::harness_card::of(&declared);

        let refused = when_typed("work", None, Some(&card)).unwrap_err();

        assert!(
            refused.contains(
                "eager cannot have a prompt typed in for you, because charter cannot tell when it \
                 has finished starting. See What eager can do here."
            ),
            "{refused}"
        );
    }
}
