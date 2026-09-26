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
//! **The prompt is typed once the chat's harness says it has started, and never sent.**
//! It is held here, in the app, per chat ([`Typed`]), until the first hook report of a
//! `SessionStart` that began a session reaches the plane's board — hooks only (spec: nothing
//! parses harness output to decide anything). It is then written into the terminal as ONE
//! bracketed paste, `ESC [200~ … ESC [201~`, with nothing after it: no carriage return, no line
//! feed, no Enter ([`bracketed`]), and only once the terminal hands keys to the harness rather
//! than editing lines itself ([`type_once_it_reads_keys`]). The operator reads it and presses
//! Enter. A harness whose `SessionStart` comes at the first prompt rather than at launch is not
//! given one at all (`Harness::reports_its_start_before_the_first_prompt`): its prompt would
//! land after whatever the operator had already sent.
//!
//! **Bracketed whether or not the program asked for it.** Whether a program has turned
//! bracketed paste on (`?2004h`) is a mode of the terminal the core draws, and the core's
//! engine offers no way to ask it; the three harnesses charter starts all turn it on, and a
//! paste is still one write of text with no carriage return in it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use charter_core::curation::{self, Resolved, Source, Subject};
use charter_core::harness::Harness;
use charter_core::hookwire::Report;
use charter_core::state::Event;

use crate::planes::{PlaneId, Planes};
use charter_core::engine::Size;
use charter_core::reopen::Chat;

/// Where a bracketed paste begins and ends (xterm's `?2004` mode).
const PASTE_BEGINS: &str = "\x1b[200~";
const PASTE_ENDS: &str = "\x1b[201~";

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
pub fn curation_offers(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    subjects: Vec<String>,
) -> Result<Curations, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    Ok(offers(&root, &subjects))
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
fn launch_profile(root: &Path) -> Result<String, String> {
    let (set, _) = charter_core::profiles::for_launch(root);
    let profile = set
        .default
        .as_deref()
        .and_then(|name| set.get(name))
        .or_else(|| set.profiles().first())
        .ok_or_else(|| {
            "This project has no harness profile to open a curation chat on.".to_owned()
        })?;
    takes_a_typed_prompt(&profile.name, Harness::of_kind(&profile.kind))?;
    Ok(profile.name.clone())
}

/// Whether a chat on `harness` can have a prompt typed into it on its start, in a sentence
/// when it cannot. One answer for the menu's rows and for the start itself.
fn takes_a_typed_prompt(profile: &str, harness: Option<Harness>) -> Result<(), String> {
    match harness {
        Some(harness) if harness.reports_its_start_before_the_first_prompt() => Ok(()),
        Some(harness) => Err(format!(
            "The default profile '{profile}' runs {}, which says nothing until your first \
             prompt, so charter has no moment to type a curation prompt into it. Make a \
             Claude Code profile the default to curate from here.",
            harness.name()
        )),
        None => Err(format!(
            "The default profile '{profile}' runs a program charter has not measured, so it \
             cannot tell when to type a curation prompt into it."
        )),
    }
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
fn open(
    held: &crate::planes::Held,
    spec: &str,
    action: &str,
    size: Size,
) -> Result<Curating, String> {
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
    let number = held.chats().sessions().deal();
    let name = number.to_string();
    let ready = charter_core::start::ready(
        &charter_core::start::Start {
            profile: Some(profile.clone()),
            persona: chosen.runner.clone(),
            name: name.clone(),
            cwd: Some(chosen.cwd.clone()),
            resume: None,
            // The picker's footer box is one operator choice for one chat, and nobody made it
            // for this one — a handoff's rule.
            show_footer: false,
        },
        &root,
    )?;
    // The same question the menu asked, of the harness this start actually resolved.
    takes_a_typed_prompt(&profile, ready.harness)?;
    let chat = Chat {
        program: ready.program.clone(),
        // What the RECORD keeps: the profile's own words. Never the prompt — a relaunch
        // resumes the conversation, and the prompt was only ever typed once.
        args: Vec::new(),
        cwd: ready.cwd.clone(),
        name: name.clone(),
        resume: ready.session.clone(),
        active: false,
        profile: Some(profile),
        persona: chosen.runner.clone(),
        show_footer: false,
        pinned: false,
        number: Some(number),
        label: Some(label.clone()),
        from: None,
        renamed_from: None,
    };
    held.typed().hold(number, chosen.prompt);
    let session = held
        .chats()
        .start_ready(&chat, &ready, size)
        .inspect_err(|_| held.typed().forget(number))?;
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

/// The prompts waiting for their chat's harness to start, by chat.
///
/// **The app's, not the window's**: a window can close, reload or be a second window on the
/// same project, and the chat's harness reports to the plane whichever of them is showing it.
#[derive(Debug, Default)]
pub struct Typed {
    waiting: Mutex<HashMap<u32, String>>,
}

impl Typed {
    /// Holds `prompt` for chat `session` until its harness reports its start.
    pub fn hold(&self, session: u32, prompt: String) {
        self.held().insert(session, prompt);
    }

    /// Lets go of a chat's prompt without typing it: the chat ended, closed, or never started.
    pub fn forget(&self, session: u32) {
        self.held().remove(&session);
    }

    /// A hook report the plane's board took. Answers what to type into that chat now — its
    /// prompt, once, on the `SessionStart` that began its session — or nothing.
    ///
    /// A chat that reports a prompt, a turn's end or its own end before it reports a start has
    /// had something sent into it already, and a prompt typed after that would be typed into
    /// the middle of it: it is dropped, never typed late.
    pub fn heard(&self, report: &Report) -> Option<String> {
        match report.event {
            Event::SessionStart if report.detail.started.began_a_session() => self
                .held()
                .remove(&report.chat)
                .map(|prompt| bracketed(&prompt)),
            Event::UserPromptSubmit | Event::Stop | Event::SessionEnd => {
                self.forget(report.chat);
                None
            }
            Event::SessionStart | Event::Notification | Event::SubagentStop => None,
        }
    }

    /// Whether a prompt is waiting for chat `session`. Only a test asks.
    #[cfg(test)]
    pub fn waiting(&self, session: u32) -> bool {
        self.held().contains_key(&session)
    }

    fn held(&self) -> MutexGuard<'_, HashMap<u32, String>> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// How long a typed prompt waits, after its chat's start, for the harness to read keys itself.
/// Claude Code's canonical moment after its trust question lasted about a third of a second.
pub const KEYS_READ_WITHIN: Duration = Duration::from_secs(10);

/// How often the terminal is asked again while it waits.
const ASKED_EVERY: Duration = Duration::from_millis(20);

/// Types `text` once the chat's terminal hands keys to its program rather than editing lines
/// itself (`edits_lines`), and never while it does.
///
/// **Why it waits at all.** A harness's `SessionStart` says it has started, not that it is
/// reading keys. On Claude Code 2.1.283 in a folder it has just been told to trust, the hook
/// fires inside a third of a second when the terminal is back in canonical mode: text written
/// then is echoed onto the screen and cut at its first line break — measured, with the second
/// line of a two-line prompt lost. So this asks the terminal, a bounded number of times, and
/// types the moment it is raw. A terminal still editing lines at `within` is not typed into at
/// all: a prompt cut in two is worse than none, and it says so on stderr.
///
/// A platform that cannot say is typed into at once, and a chat that has gone is not typed
/// into. Each answer is the terminal's line discipline, never anything the harness printed.
pub fn type_once_it_reads_keys(
    text: &str,
    edits_lines: impl Fn() -> Result<Option<bool>, String>,
    write: impl FnOnce(&str) -> Result<(), String>,
    within: Duration,
) {
    let until = Instant::now() + within;
    loop {
        match edits_lines() {
            // Gone: nothing to type into, and nothing owed to anyone about it.
            Err(_) => return,
            Ok(Some(true)) if Instant::now() < until => std::thread::sleep(ASKED_EVERY),
            Ok(Some(true)) => {
                eprintln!(
                    "charter: a curation chat's terminal was still editing lines {}s after its \
                     harness started, so its prompt was not typed",
                    within.as_secs()
                );
                return;
            }
            Ok(Some(false) | None) => {
                // A chat that ended between the question and this write has nothing to type
                // into, and nothing is owed to anyone about it either.
                let _ = write(text);
                return;
            }
        }
    }
}

/// `prompt` as one bracketed paste, with nothing after it.
///
/// **Nothing in it can submit.** Line breaks are line feeds, inside the paste, where a harness
/// reads them as newlines in its input. Every other control character is taken out — an
/// `ESC [201~` in a persona's prompt would otherwise end the paste early and leave the rest to
/// be read as keys, and a carriage return there is Enter — and so is the trailing white space a
/// file's last line leaves, so the operator's cursor ends on the prompt's last word.
pub fn bracketed(prompt: &str) -> String {
    let lines = prompt.replace("\r\n", "\n").replace('\r', "\n");
    let text: String = lines
        .trim_end()
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    format!("{PASTE_BEGINS}{text}{PASTE_ENDS}")
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

    #[test]
    fn a_held_prompt_is_typed_once_on_the_first_start_its_chat_reports() {
        let typed = Typed::default();
        typed.hold(4, "Compact steward.".to_owned());

        assert_eq!(
            typed.heard(&start(3)),
            None,
            "another chat's start types nothing"
        );
        assert_eq!(
            typed.heard(&start(4)).as_deref(),
            Some("\x1b[200~Compact steward.\x1b[201~")
        );
        assert_eq!(typed.heard(&start(4)), None, "a second start types nothing");
        assert!(!typed.waiting(4));
    }

    #[test]
    fn a_compaction_is_not_the_start_a_prompt_waits_for() {
        let typed = Typed::default();
        typed.hold(4, "Compact steward.".to_owned());

        assert_eq!(
            typed.heard(&report(4, Event::SessionStart, Started::Compacted)),
            None
        );
        assert!(typed.waiting(4));
    }

    #[test]
    fn a_prompt_is_dropped_rather_than_typed_after_something_was_sent() {
        for first in [Event::UserPromptSubmit, Event::Stop, Event::SessionEnd] {
            let typed = Typed::default();
            typed.hold(4, "Compact steward.".to_owned());

            assert_eq!(typed.heard(&report(4, first, Started::Unsaid)), None);
            assert_eq!(typed.heard(&start(4)), None, "typed late after {first:?}");
        }
    }

    #[test]
    fn a_prompt_forgotten_when_its_chat_ends_is_never_typed() {
        let typed = Typed::default();
        typed.hold(4, "Compact steward.".to_owned());

        typed.forget(4);

        assert_eq!(typed.heard(&start(4)), None);
    }

    /// A terminal that answers `answers` in turn, then the last one for ever.
    fn a_terminal(
        answers: Vec<Result<Option<bool>, String>>,
    ) -> impl Fn() -> Result<Option<bool>, String> {
        let left = Mutex::new(answers);
        move || {
            let mut left = left.lock().unwrap();
            if left.len() > 1 {
                left.remove(0)
            } else {
                left[0].clone()
            }
        }
    }

    #[test]
    fn a_prompt_waits_for_the_terminal_to_hand_keys_to_the_harness() {
        let asked = std::sync::atomic::AtomicU32::new(0);
        let answers = a_terminal(vec![Ok(Some(true)), Ok(Some(true)), Ok(Some(false))]);
        let written = Mutex::new(Vec::new());

        type_once_it_reads_keys(
            "typed",
            || {
                asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                answers()
            },
            |text| {
                written.lock().unwrap().push(text.to_owned());
                Ok(())
            },
            Duration::from_secs(5),
        );

        assert_eq!(asked.into_inner(), 3, "asked until it was raw");
        assert_eq!(written.into_inner().unwrap(), ["typed"], "then typed once");
    }

    #[test]
    fn a_terminal_that_never_hands_keys_over_is_never_typed_into() {
        let written = Mutex::new(Vec::<String>::new());

        type_once_it_reads_keys(
            "typed",
            a_terminal(vec![Ok(Some(true))]),
            |text| {
                written.lock().unwrap().push(text.to_owned());
                Ok(())
            },
            Duration::from_millis(60),
        );

        assert!(written.into_inner().unwrap().is_empty());
    }

    #[test]
    fn a_chat_gone_while_its_prompt_waits_is_not_typed_into() {
        let written = Mutex::new(Vec::<String>::new());

        type_once_it_reads_keys(
            "typed",
            a_terminal(vec![Ok(Some(true)), Err("gone".to_owned())]),
            |text| {
                written.lock().unwrap().push(text.to_owned());
                Ok(())
            },
            Duration::from_secs(5),
        );

        assert!(written.into_inner().unwrap().is_empty());
    }

    #[test]
    fn a_terminal_that_cannot_say_is_typed_into_at_once() {
        let written = Mutex::new(Vec::<String>::new());

        type_once_it_reads_keys(
            "typed",
            a_terminal(vec![Ok(None)]),
            |text| {
                written.lock().unwrap().push(text.to_owned());
                Ok(())
            },
            Duration::from_secs(5),
        );

        assert_eq!(written.into_inner().unwrap(), ["typed"]);
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
                ("ops/tidy", Some("ops")),
            ]
        );
        let tidy = &alpha.actions[2];
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
    fn a_project_whose_default_harness_cannot_be_typed_into_says_so_on_every_list() {
        let plane = Plane::new("codex");

        let said = offers(&plane.root, &["workspace:alpha".to_owned()]);

        let cannot = said
            .cannot
            .expect("a codex default cannot take a typed prompt");
        assert!(cannot.contains("'work'"), "{cannot}");
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
            &Report {
                chat: curating.session,
                event: Event::SessionStart,
                conversation: Conversation::Named(conversation),
                pid: Some(std::process::id()),
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

    #[test]
    fn a_codex_default_opens_nothing_rather_than_typing_into_its_first_turn() {
        let plane = Plane::new("codex");
        let planes = planes();
        let id = planes.open(&plane.root);
        let held = planes.held(&id).expect("held");

        let refused = open(&held, "workspace:alpha", "charter/compact", SIZE).unwrap_err();

        assert!(refused.contains("first prompt"), "{refused}");
        assert!(held.chats().open_now().is_empty());
    }

    #[test]
    fn only_a_harness_that_reports_its_start_at_launch_is_typed_into() {
        assert!(takes_a_typed_prompt("claude", Some(Harness::ClaudeCode)).is_ok());
        let codex = takes_a_typed_prompt("work", Some(Harness::Codex)).unwrap_err();
        assert!(
            codex.contains("'work'") && codex.contains("first prompt"),
            "{codex}"
        );
        assert!(takes_a_typed_prompt("custom", None).is_err());
    }
}
