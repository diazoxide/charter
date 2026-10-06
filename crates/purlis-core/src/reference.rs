//! A file, a folder or a range of lines of a branch, handed to a chat (FM-9, #1103 F7, V86).
//!
//! **A neutral model, and one adapter per harness.** A [`Reference`] is a path, an optional
//! range of lines and whether it names a folder — nothing about any harness. Each harness's
//! adapter renders it in that harness's own syntax ([`crate::harness::HarnessAdapter::reference`]):
//! `@src/main.rs#L10-20` for Claude Code, `src/main.rs:10-20` for Codex,
//! `@src/main.rs#10-20` for opencode. A harness charter ships no adapter for gets
//! [`Reference::plain`].
//!
//! **Only ever a path the core confined.** A reference is made in one place, [`of`], from a
//! branch and a path inside it that [`files::place`] resolved: inside the branch's folder, no
//! `..`, through no link, not git's own `.git`. Its text is built here from that resolved path,
//! never from a string the window sent, and a path holding a control character (a line feed is
//! a legal file-name byte) is refused, so the text typed into a chat is always one line.
//!
//! **Typed, never sent** (ADR 0061's rule for a prompt typed into a chat): [`pasted`] is the one
//! bracketed paste a reference is written as, with nothing after it, and [`may_type_into`] is
//! the one answer to whether a running chat may be typed into at all — only at its prompt,
//! never mid-turn, never while it asks the operator something.

use std::path::{Path, PathBuf};

use crate::files::{self, Branch};
use crate::harness::Harness;
use crate::state::State;

/// A range of lines of a file, both ends counted from 1 and included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lines {
    pub first: u32,
    pub last: u32,
}

impl Lines {
    /// One line.
    pub fn one(line: u32) -> Self {
        Self {
            first: line,
            last: line,
        }
    }
}

/// A file, folder or range of lines of a branch, as a chat is handed it. Made only by [`of`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The path as the chat reads it: relative to the chat's own folder when it is inside it,
    /// else absolute. Never holds a control character.
    path: String,
    lines: Option<Lines>,
    folder: bool,
}

/// Why a path cannot be handed to a chat, as the sentence the window shows.
#[derive(Debug, thiserror::Error)]
pub enum NotReferable {
    #[error(transparent)]
    Refused(#[from] files::Refused),
    #[error(
        "'{0}' has a control or invisible character in its name, so purlis cannot type it into \
         a chat as what it shows"
    )]
    ControlCharacter(String),
    #[error("'{0}' is a folder, and a range of lines names part of a file")]
    LinesOfAFolder(String),
    #[error("lines {first} to {last} are not a range of lines: they count from 1, first to last")]
    NotARange { first: u32, last: u32 },
}

/// The reference to `path` of `branch`, for a chat working in `from` — or refused.
///
/// The path is placed by [`files::place`], so everything it refuses is refused here: a path
/// that is empty, absolute or walks up, a link anywhere on it, git's own folder, and a path
/// that is not there. The chat reads the path relative to its own folder when the file is
/// inside it, and absolute otherwise: a chat on another branch, or one whose folder charter
/// does not know (`from` is `None`), is handed the whole path.
pub fn of(
    plane: &Path,
    branch: Branch<'_>,
    path: &str,
    lines: Option<Lines>,
    from: Option<&Path>,
) -> Result<Reference, NotReferable> {
    let placed = files::place(plane, branch, path)?;
    if let Some(Lines { first, last }) = lines {
        if placed.folder {
            return Err(NotReferable::LinesOfAFolder(placed.relative));
        }
        if first == 0 || last < first {
            return Err(NotReferable::NotARange { first, last });
        }
    }
    let path = neutral(as_seen_from(&placed.absolute, from));
    if path.chars().any(not_typeable) {
        return Err(NotReferable::ControlCharacter(placed.relative));
    }
    Ok(Reference {
        path,
        lines,
        folder: placed.folder,
    })
}

/// `absolute` as a chat in `from` reads it: relative when it is inside `from` (resolved, so a
/// folder named through a link compares as the disk has it), `.` for `from` itself, else whole.
fn as_seen_from(absolute: &Path, from: Option<&Path>) -> String {
    let from: Option<PathBuf> = from.and_then(|from| std::fs::canonicalize(from).ok());
    match from
        .as_deref()
        .and_then(|from| absolute.strip_prefix(from).ok())
    {
        Some(inside) if inside.as_os_str().is_empty() => ".".to_owned(),
        Some(inside) => slashed(inside),
        None => absolute.to_string_lossy().into_owned(),
    }
}

/// A character a reference never holds: a control character, or one a composer draws as nothing
/// or that moves what is around it — a format character (zero-width, bidirectional overrides and
/// isolates), any other default-ignorable one (HP-6's `drawn_otherwise`), and the line and
/// paragraph separators. A name holding one would show something else than what is typed.
pub fn not_typeable(c: char) -> bool {
    crate::harness::hooked::drawn_otherwise(c) || matches!(c, '\u{2028}' | '\u{2029}')
}

/// `path` as no harness reads as a mode or a command: a relative path that does not begin with a
/// letter, a digit, `_` or `.` is written `./<path>`. Measured: Claude Code 2.1.288 turns a paste
/// beginning with `!` into an empty input into bash mode, and codex-cli 0.147.0 runs a submitted
/// `!…` as a shell command; `/` begins a command in all three, and `#`, `@`, `&` and `$` mean
/// something at the start of one or another. `./` names the same file to every agent. An
/// absolute path begins with `/`, and each adapter quotes or prefixes it so it never stands first
/// unquoted ([`starts_safely`]).
fn neutral(path: String) -> String {
    let plain = path
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '/');
    if plain { path } else { format!("./{path}") }
}

/// Whether `rendered` — what an adapter hands a harness — begins with nothing a harness reads as
/// a mode or a command: `@`, a quote, `.`, or a letter, digit or `_`. Every adapter's answer is
/// held to it by the tests.
pub fn starts_safely(rendered: &str) -> bool {
    rendered
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '@' | '"'))
}

/// A relative path with `/` between its parts, on every platform.
fn slashed(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

impl Reference {
    /// The path as the chat reads it, without anything a harness adds.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The lines it names, for a part of a file.
    pub fn lines(&self) -> Option<Lines> {
        self.lines
    }

    /// Whether it names a folder.
    pub fn folder(&self) -> bool {
        self.folder
    }

    /// The reference in no harness's syntax, for a program charter ships no adapter for: the
    /// path (in double quotes when it has white space or is absolute, so it never begins with
    /// `/`), a folder ending in `/`, and lines as `:10` or `:10-20` — what an agent reading plain
    /// words takes for a file and its lines.
    pub fn plain(&self) -> String {
        let body = format!("{}{}", self.path_with_slash(), self.lines_after(":", ""));
        quoted_if_spaced_or_absolute(&body)
    }

    /// The path, with a `/` after it when it is a folder.
    pub fn path_with_slash(&self) -> String {
        if self.folder && !self.path.ends_with('/') {
            format!("{}/", self.path)
        } else {
            self.path.clone()
        }
    }

    /// `lines` as `<before><first>` or `<before><first>-<between><last>`, or nothing.
    pub fn lines_after(&self, before: &str, between: &str) -> String {
        match self.lines {
            None => String::new(),
            Some(Lines { first, last }) if first == last => format!("{before}{first}"),
            Some(Lines { first, last }) => format!("{before}{first}-{between}{last}"),
        }
    }

    /// A reference made without a branch, for a test of how an adapter renders one. Tests
    /// only: everywhere else a reference is made by [`of`], from a confined path.
    #[cfg(test)]
    pub(crate) fn for_tests(path: &str, lines: Option<Lines>, folder: bool) -> Self {
        assert!(!path.chars().any(not_typeable));
        Self {
            path: neutral(path.to_owned()),
            lines,
            folder,
        }
    }
}

/// `text` in double quotes when it has white space or begins with `/` (an absolute path, which
/// unquoted would be read as a command), and has no quote of its own. One that has a quote and
/// begins with `/` is prefixed `./` from the root instead — `.//abs` is the same path — so it
/// never begins with `/`.
pub fn quoted_if_spaced_or_absolute(text: &str) -> String {
    let wants = text.chars().any(char::is_whitespace) || text.starts_with('/');
    if !wants {
        text.to_owned()
    } else if !text.contains('"') {
        format!("\"{text}\"")
    } else if text.starts_with('/') {
        format!(".{text}")
    } else {
        text.to_owned()
    }
}

/// The bytes a reference rendered as `rendered` is typed into a chat as: ONE bracketed paste of
/// the reference and a space, so the operator's next words follow it, and nothing after the
/// paste — no carriage return, no line feed, no Enter. Every control character is taken out
/// first, so nothing inside can end the paste early or submit.
pub fn pasted(rendered: &str) -> String {
    let text: String = rendered.chars().filter(|c| !not_typeable(*c)).collect();
    crate::curation::bracketed_then_a_space(&text)
}

/// What a running chat's harness, board and terminal say now, for [`may_type_into`].
#[derive(Debug, Clone, Copy)]
pub struct Now {
    /// What its hooks last reported.
    pub state: State,
    /// Whether it stopped mid-turn to ask the operator something.
    pub asking: bool,
    /// The terminal's line discipline: `Some(true)` while it edits lines itself.
    pub edits_lines: Option<bool>,
}

/// Why a reference is not typed into a running chat now, as the sentence the window shows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotNow {
    #[error("purlis cannot tell when {0} is at its prompt, so it does not type into it")]
    CannotBeTyped(&'static str),
    #[error("the chat is in the middle of a turn")]
    MidTurn,
    #[error("the chat is asking you something")]
    Asking,
    #[error("the chat has ended")]
    Ended,
    #[error("the chat has not reached its prompt yet")]
    NotYetAtItsPrompt,
    #[error("purlis has not heard from the chat yet")]
    NotHeardFrom,
}

/// Whether a reference may be typed into a running chat on `harness` now: **only at its
/// prompt**.
///
/// - A harness charter cannot type into (`Harness::ready_to_type`, opencode) never is.
/// - A turn in flight never is: its hooks said `UserPromptSubmit` and not yet `Stop`.
/// - A chat asking the operator something mid-turn — a permission, a question — never is, so a
///   paste can never land in a dialog as its answer.
/// - An ended chat never is.
/// - A chat its hooks have handed back (`Waiting`, not asking) is, once its terminal hands keys
///   to it (raw) — the kernel's line discipline, never anything the harness drew.
/// - A chat its hooks have said nothing about yet never is, on any harness. A drop can come at
///   any moment of a chat's life, and a Codex chat whose hooks are not trusted stays silent for
///   all of it — through an approval dialog that is raw and quiet like an input. Only a hook's
///   `Waiting` says the chat is at its prompt.
pub fn may_type_into(harness: Harness, now: Now) -> Result<(), NotNow> {
    if harness.ready_to_type().is_none() {
        return Err(NotNow::CannotBeTyped(harness.title()));
    }
    let raw = now.edits_lines != Some(true);
    match now.state {
        State::Running => Err(NotNow::MidTurn),
        State::Done | State::Failed => Err(NotNow::Ended),
        State::Unknown => Err(NotNow::NotHeardFrom),
        State::Waiting if now.asking => Err(NotNow::Asking),
        State::Waiting if raw => Ok(()),
        State::Waiting => Err(NotNow::NotYetAtItsPrompt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(state: State) -> Now {
        Now {
            state,
            asking: false,
            edits_lines: Some(false),
        }
    }

    #[test]
    fn a_paste_holds_one_line_and_nothing_after_it_can_submit() {
        let typed = pasted("@a\x1b[201~\r\nb\u{202E}c\u{200B}d\u{2028}");
        assert_eq!(typed, "\x1b[200~@a[201~bcd \x1b[201~");
        assert!(!typed.contains(['\r', '\n']));
        assert!(typed.ends_with(crate::curation::PASTE_ENDS));
    }

    #[test]
    fn invisible_and_bidirectional_characters_are_not_typeable() {
        for c in [
            '\u{202A}', '\u{202E}', '\u{2066}', '\u{2069}', '\u{200B}', '\u{200F}',
        ] {
            assert!(not_typeable(c), "U+{:04X}", u32::from(c));
        }
        for c in ['\u{2060}', '\u{FEFF}', '\u{2028}', '\u{2029}', '\n', '\x1b'] {
            assert!(not_typeable(c), "U+{:04X}", u32::from(c));
        }
        for c in ['a', 'é', '/', ' ', '#', '!'] {
            assert!(!not_typeable(c), "{c:?}");
        }
    }

    #[test]
    fn a_path_a_harness_would_read_as_a_mode_or_a_command_is_written_from_here() {
        assert_eq!(neutral("!rm${IFS}-rf".to_owned()), "./!rm${IFS}-rf");
        assert_eq!(neutral("#note".to_owned()), "./#note");
        assert_eq!(neutral("@x".to_owned()), "./@x");
        assert_eq!(neutral("-rf".to_owned()), "./-rf");
        assert_eq!(neutral("src/main.rs".to_owned()), "src/main.rs");
        assert_eq!(neutral(".github".to_owned()), ".github");
        assert_eq!(neutral("/abs/x".to_owned()), "/abs/x");
    }

    #[test]
    fn a_chat_mid_turn_is_never_typed_into() {
        for harness in [Harness::ClaudeCode, Harness::Codex] {
            assert_eq!(
                may_type_into(harness, now(State::Running)),
                Err(NotNow::MidTurn)
            );
        }
    }

    #[test]
    fn a_chat_asking_the_operator_is_never_typed_into() {
        let asking = Now {
            asking: true,
            ..now(State::Waiting)
        };
        assert_eq!(
            may_type_into(Harness::ClaudeCode, asking),
            Err(NotNow::Asking)
        );
    }

    #[test]
    fn a_chat_handed_back_is_typed_into_once_its_terminal_is_raw() {
        assert_eq!(
            may_type_into(Harness::ClaudeCode, now(State::Waiting)),
            Ok(())
        );
        let canonical = Now {
            edits_lines: Some(true),
            ..now(State::Waiting)
        };
        assert_eq!(
            may_type_into(Harness::ClaudeCode, canonical),
            Err(NotNow::NotYetAtItsPrompt)
        );
    }

    #[test]
    fn a_chat_no_hook_has_spoken_for_is_never_typed_into_on_any_harness() {
        for harness in [Harness::ClaudeCode, Harness::Codex] {
            assert_eq!(
                may_type_into(harness, now(State::Unknown)),
                Err(NotNow::NotHeardFrom),
                "{harness:?}"
            );
        }
        assert_eq!(may_type_into(Harness::Codex, now(State::Waiting)), Ok(()));
    }

    #[test]
    fn opencode_and_an_ended_chat_are_never_typed_into() {
        assert_eq!(
            may_type_into(Harness::Opencode, now(State::Waiting)),
            Err(NotNow::CannotBeTyped("opencode"))
        );
        assert_eq!(
            may_type_into(Harness::ClaudeCode, now(State::Done)),
            Err(NotNow::Ended)
        );
    }

    #[test]
    fn plain_words_name_a_file_a_folder_and_lines() {
        let r = |path, lines, folder| Reference::for_tests(path, lines, folder).plain();
        assert_eq!(r("src/main.rs", None, false), "src/main.rs");
        assert_eq!(r("src", None, true), "src/");
        assert_eq!(
            r(
                "src/main.rs",
                Some(Lines {
                    first: 10,
                    last: 20
                }),
                false
            ),
            "src/main.rs:10-20"
        );
        assert_eq!(
            r("my dir/a b.txt", Some(Lines::one(3)), false),
            "\"my dir/a b.txt:3\""
        );
    }
}
