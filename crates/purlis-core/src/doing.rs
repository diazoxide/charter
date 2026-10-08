//! What a working chat is doing, in one line (#1493, V100-42).
//!
//! A tool hook already sees which tool its chat is about to run. It sorts that into one of a
//! fixed list of kinds ([`Kind`]) and names it to the app on a line of its own
//! ([`crate::hookwire::Doing`]); the app keeps the latest per chat ([`Tracker`]) and tells the
//! window, which says it in purlis's own words under the chat's name. **In memory only**: the
//! line is never spooled, never written to the event log (which keeps only the arguments'
//! digest, ADR 0066), never put in a dispatch record, the Activity view or any other store.
//!
//! # What is said, and the whole of it
//!
//! A kind, and for three kinds one short name:
//!
//! | Kind | The window says | Its name |
//! |---|---|---|
//! | `thinking` | thinking | none |
//! | `command` | running a command; running `<program>` | the command's first word |
//! | `editing` | editing a file; editing `<file>` | the file's base name |
//! | `reading` | reading a file; reading `<file>`; reading 3 files | the file's base name |
//! | `searching` | searching | none |
//! | `fetching` | fetching a page | none |
//! | `helper` | waiting on a helper | none |
//! | `dispatching` | dispatching a task | none |
//! | `asking` | asking a question | none |
//! | `reporting` | writing its report | none |
//! | `tool` | using a tool | none |
//!
//! **Never** a command's arguments, a URL, a search's pattern, a file's folder or contents, a
//! tool's own name, or anything a tool came back with. `thinking` is a turn that is running
//! with no tool in flight. `dispatching`, `asking` and `reporting` are also what the app says
//! by itself when the chat's own purlis command reaches it, so they need no hook.
//!
//! # What a chat can make it say
//!
//! A hook runs in the chat's own process tree, and anything holding the chat's token can write
//! the line. So the name is the chat's word, and the app believes none of it until
//! [`Said::neutral`] has passed it: letters, digits and a handful of marks, no space, no
//! directory, nothing invisible, and short ([`file_name`], [`program`]). A name that fails is
//! dropped whole and the kind is said without one. The hook applies the same rule before it
//! sends, so a path never leaves the hook either. No name has a space in it, so no name can
//! read as a sentence of the app's.
//!
//! # Which tools are heard, per harness
//!
//! Only tools some armed hook runs on ([`crate::hookreg`]):
//!
//! - **Claude Code:** `Bash`; `Read` and `Grep`; `Write`, `Edit` and `MultiEdit`; `Task` and
//!   `Agent`; purlis's own dispatch tools. A tool no hook runs on (`WebFetch`, `Glob`, another
//!   server's tool) says nothing, and the line keeps what it last said.
//! - **opencode:** every tool, since purlis's plugin routes each to a hook
//!   ([`crate::opencode::TOOLS`]) under Claude Code's name for it where it has one.
//! - **Codex:** its shell alone (`plugin::CODEX`), so `command` and `thinking`. With its hooks
//!   untrusted nothing is heard and nothing is said.
//!
//! A chat whose hooks the app has not heard a turn begin from has no line: the app says what
//! it was told and guesses nothing (V100-71).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

/// What a chat is doing, by its fixed word. The module's table is the whole list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A turn is running and no tool is in flight.
    Thinking,
    /// A shell command.
    Command,
    /// A file tool that writes.
    Editing,
    /// A file tool that reads.
    Reading,
    /// A search of files or of the web.
    Searching,
    /// A page fetched.
    Fetching,
    /// A helper the harness started for it, which it waits on.
    Helper,
    /// It is dispatching a task to another chat.
    Dispatching,
    /// It is asking the chat that dispatched it a question.
    Asking,
    /// It is handing in its report.
    Reporting,
    /// A tool purlis has no word for.
    Tool,
}

impl Kind {
    /// The word the window is sent.
    pub fn word(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::Command => "command",
            Self::Editing => "editing",
            Self::Reading => "reading",
            Self::Searching => "searching",
            Self::Fetching => "fetching",
            Self::Helper => "helper",
            Self::Dispatching => "dispatching",
            Self::Asking => "asking",
            Self::Reporting => "reporting",
            Self::Tool => "tool",
        }
    }

    /// Whether the line keeps saying this once its tool has come back: an edit, a read and a
    /// search are over in a moment, and "editing Notice.tsx" is worth more than a flicker.
    fn stays(self) -> bool {
        matches!(self, Self::Editing | Self::Reading | Self::Searching)
    }
}

/// What one tool hook says of its chat ([`crate::hookwire::Doing`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "is", rename_all = "lowercase")]
pub enum Said {
    /// A tool is about to run.
    Began {
        /// Which kind of tool.
        kind: Kind,
        /// Its one name, already through [`file_name`] or [`program`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// A tool came back.
    Ended,
}

impl Said {
    /// The same, believing nothing of its name: kept only for a kind that has one, and only
    /// if it passes that kind's rule unchanged.
    pub fn neutral(self) -> Self {
        match self {
            Self::Began { kind, name } => Self::Began {
                kind,
                name: name.filter(|name| match kind {
                    Kind::Command => program(name).as_deref() == Some(name),
                    Kind::Editing | Kind::Reading => file_name(name).as_deref() == Some(name),
                    _ => false,
                }),
            },
            Self::Ended => Self::Ended,
        }
    }
}

/// The longest file name said, in characters. A longer one is not said at all.
pub const LONGEST_FILE_NAME: usize = 48;

/// The longest program name said, in characters. A longer one is not said at all.
pub const LONGEST_PROGRAM: usize = 24;

/// The marks a file name may hold beside letters and digits.
const FILE_MARKS: &str = "._-+@~#";

/// The marks a program's name may hold beside ASCII letters and digits.
const PROGRAM_MARKS: &str = "._+-";

/// The base name of the file a tool was given as `path`, or nothing when it is not one purlis
/// will say: empty, only dots, longer than [`LONGEST_FILE_NAME`], or holding anything but
/// letters, digits and [`FILE_MARKS`]. So no space, no control or formatting character, no
/// mark that changes the direction of text, and no markup.
pub fn file_name(path: &str) -> Option<String> {
    let base = path.rsplit(['/', '\\']).next()?;
    let fits = !base.is_empty()
        && base.chars().count() <= LONGEST_FILE_NAME
        && base.chars().any(char::is_alphanumeric)
        && base
            .chars()
            .all(|one| one.is_alphanumeric() || FILE_MARKS.contains(one));
    fits.then(|| base.to_owned())
}

/// The program a shell command starts with, by its base name, or nothing when its first word
/// is not plainly one: the first word must be ASCII letters, digits, [`PROGRAM_MARKS`] and
/// `/` alone. So a variable set in front of a command, a quoted word, a substitution and a
/// redirect say nothing, and no later word is ever looked at.
pub fn program(command: &str) -> Option<String> {
    let first = command.split_whitespace().next()?;
    if !first
        .chars()
        .all(|one| one.is_ascii_alphanumeric() || PROGRAM_MARKS.contains(one) || one == '/')
    {
        return None;
    }
    let base = first.rsplit('/').next()?;
    let fits =
        base.len() <= LONGEST_PROGRAM && base.starts_with(|one: char| one.is_ascii_alphanumeric());
    fits.then(|| base.to_owned())
}

/// Where a file tool carries its path: Claude Code's spelling, opencode's, `Grep`'s, a
/// notebook's.
const PATH_KEYS: [&str; 4] = ["file_path", "filePath", "path", "notebook_path"];

/// What the hook `word` says its chat is doing, from the payload a harness gave it, or nothing
/// for a hook that is not a tool's.
///
/// A hook before a tool says the tool began; one after says a tool came back. The payload is
/// read for the tool's name and, for a shell or a file tool, the one name of [`Said::Began`].
pub fn of_hook(word: &str, payload: &Value) -> Option<Said> {
    if word.starts_with("posttooluse") {
        return Some(Said::Ended);
    }
    if !word.starts_with("pretooluse") {
        return None;
    }
    let tool = payload["tool_name"].as_str().unwrap_or_default();
    let input = &payload["tool_input"];
    let path = || {
        PATH_KEYS
            .iter()
            .find_map(|key| input[*key].as_str())
            .and_then(file_name)
    };
    let (kind, name) = match tool {
        "Bash" | "bash" | "shell" | "exec_command" | "local_shell" => {
            (Kind::Command, input["command"].as_str().and_then(program))
        }
        "Read" => (Kind::Reading, path()),
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => (Kind::Editing, path()),
        "Grep" | "Glob" | "WebSearch" => (Kind::Searching, None),
        "WebFetch" => (Kind::Fetching, None),
        "Task" | "Agent" => (Kind::Helper, None),
        "mcp__purlis__dispatch" => (Kind::Dispatching, None),
        "mcp__purlis__dispatch_report" => (Kind::Reporting, None),
        _ => (Kind::Tool, None),
    };
    Some(Said::Began { kind, name })
}

/// What one chat is doing, as the window is told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doing {
    /// Which kind.
    pub kind: Kind,
    /// Its one name, where the kind has one and the chat's passed ([`Said::neutral`]).
    pub name: Option<String>,
    /// How many files it has read in a row, for [`Kind::Reading`]; 0 for every other kind.
    pub count: u32,
}

impl Doing {
    fn of(kind: Kind) -> Self {
        Self {
            kind,
            name: None,
            count: 0,
        }
    }
}

/// One thing to tell the window: what a chat is doing now, or that it has no line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Told {
    /// Which telling this is ([`sequence`]): tellings can reach the window out of order, and
    /// the higher number is the later word.
    pub sequence: u32,
    /// What it is doing, or nothing.
    pub doing: Option<Doing>,
}

/// What the app does about something a [`Tracker`] heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tell {
    /// Tell the window this, now.
    Now(Told),
    /// The chat has been told of too lately: ask [`Tracker::due`] after this long.
    Later(Duration),
    /// Nothing to tell: nothing changed, or a telling is already on its way.
    Nothing,
}

/// The least time between two tellings of one chat: four a second, however fast its tools go.
/// A line taken away is never held back.
pub const AT_MOST_EVERY: Duration = Duration::from_millis(250);

/// The next telling's number. The process's, so a project closed and opened again in one run
/// does not begin again under numbers a window still holds. Saturating.
fn sequence() -> u32 {
    static TAKEN: AtomicU32 = AtomicU32::new(0);
    let was = TAKEN
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |taken| {
            Some(taken.saturating_add(1))
        })
        .unwrap_or_else(|taken| taken);
    was.saturating_add(1)
}

/// One chat, as a [`Tracker`] holds it.
#[derive(Debug, Default)]
struct Chat {
    /// The turn its line is of, by the board's count of its turns.
    turn: u32,
    /// What it is doing.
    doing: Option<Doing>,
    /// What the window was last told, and when.
    told: Option<Doing>,
    told_at: Option<Instant>,
    /// Whether a telling is waiting for [`Tracker::due`].
    waiting: bool,
}

/// What every chat of one project is doing. **In memory only**: nothing here is written
/// anywhere, and a chat's entry goes with its chat.
///
/// It is told two facts by whoever holds the board, and decides nothing about a chat itself:
/// how many turns the chat has begun and whether it is running. A chat that is not running has
/// no line, whatever a hook says of it.
#[derive(Debug, Default)]
pub struct Tracker {
    chats: HashMap<u32, Chat>,
}

impl Tracker {
    /// The board took a report of chat `chat`, which has now begun `turns` turns and is
    /// `running` or not. A new turn begins at `thinking`; a chat that is not running has no
    /// line, which is how a turn's end, a question to the person and the chat's end clear it.
    pub fn reported(&mut self, chat: u32, turns: u32, running: bool, now: Instant) -> Tell {
        if !running && !self.chats.contains_key(&chat) {
            // Never heard running: there is no line to take away, and nothing to keep.
            return Tell::Nothing;
        }
        let held = self.chats.entry(chat).or_default();
        if !running {
            held.doing = None;
        } else if held.turn != turns {
            held.doing = Some(Doing::of(Kind::Thinking));
        }
        held.turn = turns;
        self.settle(chat, now)
    }

    /// A tool hook of chat `chat` said `said`. Nothing for a chat that is not `running`: its
    /// hooks have not been heard to begin a turn, or the turn is over.
    pub fn heard(&mut self, chat: u32, said: Said, running: bool, now: Instant) -> Tell {
        if !running {
            return Tell::Nothing;
        }
        let held = self.chats.entry(chat).or_default();
        held.doing = match (said.neutral(), held.doing.take()) {
            // Reads in a row are counted, and past the first no one file is named.
            (
                Said::Began {
                    kind: Kind::Reading,
                    ..
                },
                Some(Doing {
                    kind: Kind::Reading,
                    count,
                    ..
                }),
            ) => Some(Doing {
                kind: Kind::Reading,
                name: None,
                count: count.saturating_add(1),
            }),
            (Said::Began { kind, name }, _) => Some(Doing {
                kind,
                name,
                count: u32::from(kind == Kind::Reading),
            }),
            (Said::Ended, Some(doing)) if doing.kind.stays() => Some(doing),
            (Said::Ended, _) => Some(Doing::of(Kind::Thinking)),
        };
        self.settle(chat, now)
    }

    /// Chat `chat`'s own purlis command reached the app: `kind` is one of
    /// [`Kind::Dispatching`], [`Kind::Asking`] and [`Kind::Reporting`], and nothing of the
    /// command is kept.
    pub fn asked(&mut self, chat: u32, kind: Kind, running: bool, now: Instant) -> Tell {
        self.heard(chat, Said::Began { kind, name: None }, running, now)
    }

    /// The telling [`Tell::Later`] put off: what the chat is doing by now, if that is not what
    /// the window was last told.
    pub fn due(&mut self, chat: u32, now: Instant) -> Option<Told> {
        let held = self.chats.get_mut(&chat)?;
        held.waiting = false;
        (held.doing != held.told).then(|| held.tell(now))
    }

    /// Chat `chat` is gone, and what was held of it.
    pub fn closed(&mut self, chat: u32) {
        self.chats.remove(&chat);
    }

    /// What chat `chat` is doing, as held now.
    pub fn doing(&self, chat: u32) -> Option<&Doing> {
        self.chats.get(&chat)?.doing.as_ref()
    }

    /// Every chat that has a line, with it, each numbered as a telling of now: what a window
    /// that has just opened is answered, under whatever it is told meanwhile.
    pub fn all(&self) -> Vec<(u32, Told)> {
        let mut all: Vec<(u32, Told)> = self
            .chats
            .iter()
            .filter(|(_, held)| held.doing.is_some())
            .map(|(chat, held)| {
                let told = Told {
                    sequence: sequence(),
                    doing: held.doing.clone(),
                };
                (*chat, told)
            })
            .collect();
        all.sort_by_key(|(chat, _)| *chat);
        all
    }

    /// What to do about chat `chat` now that what it is doing may have changed.
    fn settle(&mut self, chat: u32, now: Instant) -> Tell {
        let Some(held) = self.chats.get_mut(&chat) else {
            return Tell::Nothing;
        };
        if held.doing == held.told {
            return Tell::Nothing;
        }
        let since = held
            .told_at
            .map(|at| now.saturating_duration_since(at))
            .filter(|since| *since < AT_MOST_EVERY);
        match since {
            // A line taken away is told at once: a finished chat never wears a stale one.
            Some(_) if held.doing.is_none() => Tell::Now(held.tell(now)),
            Some(_) if held.waiting => Tell::Nothing,
            Some(since) => {
                held.waiting = true;
                Tell::Later(AT_MOST_EVERY - since)
            }
            None => Tell::Now(held.tell(now)),
        }
    }
}

impl Chat {
    fn tell(&mut self, now: Instant) -> Told {
        self.told.clone_from(&self.doing);
        self.told_at = Some(now);
        Told {
            sequence: sequence(),
            doing: self.doing.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn began(kind: Kind, name: Option<&str>) -> Option<Said> {
        Some(Said::Began {
            kind,
            name: name.map(str::to_owned),
        })
    }

    fn now_doing(tell: Tell) -> Option<Doing> {
        match tell {
            Tell::Now(told) => told.doing,
            other => panic!("told now, not {other:?}"),
        }
    }

    fn named(kind: Kind, name: &str) -> Said {
        Said::Began {
            kind,
            name: Some(name.to_owned()),
        }
    }

    // --- what a hook says, per harness ---------------------------------------------------

    #[test]
    fn a_claude_code_tool_hook_says_its_kind_and_one_name() {
        let said = |word: &str, payload: Value| of_hook(word, &payload);
        assert_eq!(
            said(
                "pretooluse",
                json!({"hook_event_name": "PreToolUse", "session_id": "s", "tool_name": "Bash",
                    "tool_input": {"command": "cargo test -p purlis-core -- --nocapture",
                        "description": "Run the core's tests"}})
            ),
            began(Kind::Command, Some("cargo"))
        );
        assert_eq!(
            said(
                "pretooluse-read",
                json!({"tool_name": "Read", "tool_input": {"file_path": "/w/app/src/Notice.tsx"}})
            ),
            began(Kind::Reading, Some("Notice.tsx"))
        );
        for tool in ["Write", "Edit", "MultiEdit"] {
            assert_eq!(
                said(
                    "pretooluse-edit",
                    json!({"tool_name": tool, "tool_input": {"file_path": "/w/src/state.rs",
                        "content": "CANARY", "old_string": "CANARY", "new_string": "CANARY"}})
                ),
                began(Kind::Editing, Some("state.rs")),
                "{tool}"
            );
        }
        assert_eq!(
            said(
                "pretooluse-read",
                json!({"tool_name": "Grep", "tool_input": {"pattern": "secret", "path": "/w/src"}})
            ),
            began(Kind::Searching, None)
        );
        for tool in ["Task", "Agent"] {
            assert_eq!(
                said(
                    "pretooluse-dispatch",
                    json!({"tool_name": tool, "tool_input": {"prompt": "look into it"}})
                ),
                began(Kind::Helper, None)
            );
        }
        assert_eq!(
            said(
                "pretooluse-dispatch",
                json!({"tool_name": "mcp__purlis__dispatch", "tool_input": {"brief": "x"}})
            ),
            began(Kind::Dispatching, None)
        );
        assert_eq!(
            said(
                "pretooluse-dispatch",
                json!({"tool_name": "mcp__purlis__dispatch_report", "tool_input": {}})
            ),
            began(Kind::Reporting, None)
        );
        for word in [
            "posttooluse",
            "posttooluse-skill",
            "posttooluse-dispatch",
            "posttooluse-blocked",
            "posttoolusefailure-blocked",
        ] {
            assert_eq!(
                said(
                    word,
                    json!({"tool_name": "Bash", "tool_response": {"stdout": "CANARY"}})
                ),
                Some(Said::Ended),
                "{word}"
            );
        }
    }

    #[test]
    fn an_opencode_tool_says_the_same_through_purlis_s_plugin() {
        // The plugin's payload: Claude Code's name where the tool has one, opencode's id where
        // it has none, and opencode's own spelling of the arguments.
        let said = |word: &str, tool: &str, input: Value| {
            of_hook(
                word,
                &json!({"hook_event_name": "PreToolUse", "session_id": "ses_1", "cwd": "/w",
                    "tool_name": tool, "tool_input": input}),
            )
        };
        assert_eq!(
            said("pretooluse", "Bash", json!({"command": "npm run build"})),
            began(Kind::Command, Some("npm"))
        );
        assert_eq!(
            said("pretooluse-read", "Read", json!({"filePath": "/w/b.ts"})),
            began(Kind::Reading, Some("b.ts"))
        );
        assert_eq!(
            said("pretooluse-edit", "Edit", json!({"filePath": "/w/b.ts"})),
            began(Kind::Editing, Some("b.ts"))
        );
        // The tools with no hook of their own go to the Bash guard's word.
        assert_eq!(
            said("pretooluse", "Glob", json!({"pattern": "**/*.rs"})),
            began(Kind::Searching, None)
        );
        assert_eq!(
            said(
                "pretooluse",
                "WebFetch",
                json!({"url": "https://example.com/a?token=CANARY"})
            ),
            began(Kind::Fetching, None)
        );
        assert_eq!(
            said("pretooluse", "todowrite", json!({"todos": []})),
            began(Kind::Tool, None)
        );
        assert_eq!(
            said(
                "pretooluse",
                "github_create_issue",
                json!({"title": "CANARY"})
            ),
            began(Kind::Tool, None)
        );
    }

    #[test]
    fn a_codex_shell_call_says_the_program_and_nothing_else_is_heard_of_codex() {
        for tool in ["Bash", "shell", "exec_command", "local_shell"] {
            assert_eq!(
                of_hook(
                    "pretooluse",
                    &json!({"session_id": "c", "tool_name": tool,
                        "tool_input": {"command": "git status --short"}})
                ),
                began(Kind::Command, Some("git")),
                "{tool}"
            );
        }
        // Codex is armed with no hook after a tool, and none of these words is a tool's.
        for word in [
            "sessionstart",
            "userpromptsubmit",
            "stop",
            "sessionend",
            "notification",
        ] {
            assert_eq!(of_hook(word, &json!({"tool_name": "Bash"})), None, "{word}");
        }
    }

    #[test]
    fn a_payload_that_names_no_tool_is_a_tool_and_never_a_guess() {
        assert_eq!(of_hook("pretooluse", &json!({})), began(Kind::Tool, None));
        assert_eq!(of_hook("pretooluse", &Value::Null), began(Kind::Tool, None));
        assert_eq!(
            of_hook(
                "pretooluse",
                &json!({"tool_name": "Bash", "tool_input": {"command": 7}})
            ),
            began(Kind::Command, None)
        );
    }

    // --- the two names -------------------------------------------------------------------

    #[test]
    fn a_command_says_its_first_word_and_never_a_later_one() {
        assert_eq!(program("cargo test --all").as_deref(), Some("cargo"));
        assert_eq!(program("  npm   ci").as_deref(), Some("npm"));
        assert_eq!(program("/usr/bin/git log").as_deref(), Some("git"));
        assert_eq!(program("./gradlew build").as_deref(), Some("gradlew"));
        assert_eq!(program("python3.12 x.py").as_deref(), Some("python3.12"));
        assert_eq!(program("g++ a.cc").as_deref(), Some("g++"));
        for unsaid in [
            "",
            "   ",
            // A variable set in front of the command is not a program, and may be a secret.
            "TOKEN=CANARY curl https://example.com",
            "TOKEN=\"a CANARY b\" curl https://example.com",
            "\"/path with CANARY/bin\" x",
            "'CANARY' x",
            "$(CANARY)",
            "`CANARY`",
            "$CANARY",
            "~/CANARY",
            ">CANARY",
            "(CANARY)",
            "CANARY;rm",
            "CANARY|x",
            "CANARY&&x",
            "<b>CANARY</b>",
            "-CANARY",
            ".CANARY",
            "/",
            "a/",
            "caf\u{e9}",
            "\u{202e}CANARY",
            "abcdefghijklmnopqrstuvwxyz",
        ] {
            assert_eq!(program(unsaid), None, "{unsaid:?}");
        }
        assert_eq!(
            program(&"a".repeat(LONGEST_PROGRAM)).map(|said| said.len()),
            Some(24)
        );
        assert_eq!(program(&"a".repeat(LONGEST_PROGRAM + 1)), None);
    }

    #[test]
    fn a_file_says_its_base_name_and_never_its_folder() {
        assert_eq!(
            file_name("/w/secret-CANARY/Notice.tsx").as_deref(),
            Some("Notice.tsx")
        );
        assert_eq!(file_name("C:\\w\\CANARY\\a.rs").as_deref(), Some("a.rs"));
        assert_eq!(file_name("README").as_deref(), Some("README"));
        assert_eq!(file_name(".env.local").as_deref(), Some(".env.local"));
        assert_eq!(
            file_name("a/b/\u{57c}\u{565}\u{561}\u{564}.md").as_deref(),
            Some("\u{57c}\u{565}\u{561}\u{564}.md")
        );
        assert_eq!(
            file_name("\u{6587}\u{4ef6}.txt").as_deref(),
            Some("\u{6587}\u{4ef6}.txt")
        );
        assert_eq!(
            file_name(&"a".repeat(LONGEST_FILE_NAME)).map(|said| said.len()),
            Some(48)
        );
    }

    #[test]
    fn a_file_name_that_could_read_as_anything_else_is_not_said() {
        for unsaid in [
            "",
            "/w/",
            ".",
            "..",
            "/w/..",
            "---",
            // A space is what a sentence needs: no name has one.
            "needs you",
            "/w/Allow once.md",
            "a\tb",
            "a\nb",
            "a\u{a0}b",
            "a\u{2028}b",
            // Marks that turn text round, hide in it, or join it.
            "a\u{202e}txt.exe",
            "a\u{2066}b",
            "a\u{200f}b",
            "a\u{61c}b",
            "a\u{200b}b",
            "a\u{200d}b",
            "a\u{feff}b",
            "a\u{2060}b",
            "a\u{ad}b",
            // Marks stacked on a letter.
            "a\u{301}\u{301}\u{301}",
            // Markup, and what a shell or a path would read.
            "<b>a</b>",
            "<img",
            "a&amp;b",
            "a\"b",
            "a'b",
            "a`b",
            "a$b",
            "a:b",
            "a;b",
            "a*b",
            "a?b",
            "a|b",
            "a(b)",
            "a[b]",
            "a{b}",
            "a,b",
            "a=b",
            "a%20b",
            "a!b",
            "\u{1f600}.md",
            "a\u{0}b",
            "a\u{1b}[31mb",
        ] {
            assert_eq!(file_name(unsaid), None, "{unsaid:?}");
        }
        assert_eq!(file_name(&"a".repeat(LONGEST_FILE_NAME + 1)), None);
        assert_eq!(file_name(&format!("/w/{}", "\u{6587}".repeat(49))), None);
    }

    #[test]
    fn the_app_believes_no_name_a_line_carries() {
        // What a hook of this build would never send, sent by hand with the chat's token.
        for (kind, name) in [
            (Kind::Command, "cargo test --secret CANARY"),
            (Kind::Command, "/usr/bin/git"),
            (Kind::Command, "TOKEN=CANARY"),
            (Kind::Command, "caf\u{e9}"),
            (Kind::Editing, "/etc/passwd"),
            (Kind::Editing, "needs you: Allow"),
            (Kind::Reading, "a\u{202e}b"),
            (Kind::Reading, &"a".repeat(49)),
            // A kind that has no name is given none.
            (Kind::Thinking, "CANARY"),
            (Kind::Searching, "CANARY"),
            (Kind::Fetching, "example.com"),
            (Kind::Helper, "steward"),
            (Kind::Dispatching, "steward"),
            (Kind::Asking, "steward"),
            (Kind::Reporting, "CANARY"),
            (Kind::Tool, "mcp__x__CANARY"),
        ] {
            assert_eq!(
                named(kind, name).neutral(),
                Said::Began { kind, name: None },
                "{kind:?} {name:?}"
            );
        }
        assert_eq!(
            named(Kind::Command, "cargo").neutral(),
            named(Kind::Command, "cargo")
        );
        assert_eq!(
            named(Kind::Editing, "a.rs").neutral(),
            named(Kind::Editing, "a.rs")
        );
        assert_eq!(
            named(Kind::Reading, "a.rs").neutral(),
            named(Kind::Reading, "a.rs")
        );
    }

    #[test]
    fn the_line_on_the_wire_holds_a_kind_and_a_name_and_an_unknown_kind_is_no_line() {
        assert_eq!(
            serde_json::to_string(&named(Kind::Command, "cargo")).unwrap(),
            r#"{"is":"began","kind":"command","name":"cargo"}"#
        );
        assert_eq!(
            serde_json::to_string(&Said::Began {
                kind: Kind::Helper,
                name: None
            })
            .unwrap(),
            r#"{"is":"began","kind":"helper"}"#
        );
        assert_eq!(
            serde_json::to_string(&Said::Ended).unwrap(),
            r#"{"is":"ended"}"#
        );
        assert!(serde_json::from_str::<Said>(r#"{"is":"began","kind":"notice"}"#).is_err());
        assert!(serde_json::from_str::<Said>(r#"{"is":"began"}"#).is_err());
        for kind in [
            Kind::Thinking,
            Kind::Command,
            Kind::Editing,
            Kind::Reading,
            Kind::Searching,
            Kind::Fetching,
            Kind::Helper,
            Kind::Dispatching,
            Kind::Asking,
            Kind::Reporting,
            Kind::Tool,
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.word()));
        }
    }

    // --- the tracker ---------------------------------------------------------------------

    /// A moment `ms` after `start`.
    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn a_turn_begins_thinking_and_each_tool_replaces_what_was_said() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        assert_eq!(
            now_doing(tracker.reported(4, 1, true, start)),
            Some(Doing::of(Kind::Thinking))
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000))),
            Some(Doing {
                kind: Kind::Command,
                name: Some("cargo".to_owned()),
                count: 0
            })
        );
        // The command came back: no tool is in flight.
        assert_eq!(
            now_doing(tracker.heard(4, Said::Ended, true, at(start, 2000))),
            Some(Doing::of(Kind::Thinking))
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Editing, "a.rs"), true, at(start, 3000))),
            Some(Doing {
                kind: Kind::Editing,
                name: Some("a.rs".to_owned()),
                count: 0
            })
        );
        // An edit is over in a moment, and the line goes on saying it.
        assert_eq!(
            tracker.heard(4, Said::Ended, true, at(start, 4000)),
            Tell::Nothing
        );
        assert_eq!(
            tracker.doing(4).map(|doing| doing.kind),
            Some(Kind::Editing)
        );
        assert_eq!(
            now_doing(tracker.asked(4, Kind::Dispatching, true, at(start, 5000))),
            Some(Doing::of(Kind::Dispatching))
        );
    }

    #[test]
    fn reads_in_a_row_are_counted_and_anything_else_begins_the_count_again() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Reading, "a.rs"), true, at(start, 1000))),
            Some(Doing {
                kind: Kind::Reading,
                name: Some("a.rs".to_owned()),
                count: 1
            })
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Reading, "b.rs"), true, at(start, 2000))),
            Some(Doing {
                kind: Kind::Reading,
                name: None,
                count: 2
            })
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Reading, "c.rs"), true, at(start, 3000))),
            Some(Doing {
                kind: Kind::Reading,
                name: None,
                count: 3
            })
        );
        tracker.heard(
            4,
            Said::Began {
                kind: Kind::Searching,
                name: None,
            },
            true,
            at(start, 4000),
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Reading, "d.rs"), true, at(start, 5000))),
            Some(Doing {
                kind: Kind::Reading,
                name: Some("d.rs".to_owned()),
                count: 1
            })
        );
    }

    #[test]
    fn the_line_is_cleared_when_the_turn_ends_and_the_next_turn_begins_thinking() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000));
        // `Stop`: the board says it is no longer running.
        assert_eq!(
            now_doing(tracker.reported(4, 1, false, at(start, 2000))),
            None
        );
        assert_eq!(tracker.doing(4), None);
        assert!(tracker.all().is_empty());
        // A hook that lands after the turn is over says nothing.
        assert_eq!(
            tracker.heard(4, named(Kind::Command, "cargo"), false, at(start, 3000)),
            Tell::Nothing
        );
        assert_eq!(tracker.doing(4), None);
        assert_eq!(
            now_doing(tracker.reported(4, 2, true, at(start, 4000))),
            Some(Doing::of(Kind::Thinking))
        );
    }

    #[test]
    fn a_line_taken_away_is_told_at_once_however_lately_the_chat_was_told() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        assert_eq!(now_doing(tracker.reported(4, 1, false, at(start, 1))), None);
    }

    #[test]
    fn a_report_within_a_turn_does_not_put_the_line_back_to_thinking() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000));
        // A helper's `SubagentStop`, say: the same turn, still running.
        assert_eq!(tracker.reported(4, 1, true, at(start, 2000)), Tell::Nothing);
        assert_eq!(
            tracker.doing(4).map(|doing| doing.kind),
            Some(Kind::Command)
        );
    }

    #[test]
    fn a_chat_whose_hooks_never_began_a_turn_has_no_line() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        // Tool hooks alone: nothing told the board a turn began, so it is not running.
        assert_eq!(
            tracker.heard(9, named(Kind::Command, "cargo"), false, start),
            Tell::Nothing
        );
        assert_eq!(
            tracker.asked(9, Kind::Reporting, false, start),
            Tell::Nothing
        );
        // A session that began and waits for its first prompt.
        assert_eq!(tracker.reported(9, 0, false, start), Tell::Nothing);
        assert_eq!(tracker.doing(9), None);
        assert_eq!(tracker.due(9, at(start, 1000)), None);
    }

    #[test]
    fn a_fast_tool_loop_tells_the_window_four_times_a_second_and_ends_on_the_last_word() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        // A thousand tool calls, one a millisecond, for a second.
        let mut now_told = 0;
        let mut later = Vec::new();
        for ms in 1..=1000 {
            let name = format!("f{ms}.rs");
            // Flush what was put off, when it falls due, as the app's timer would.
            if let Some(due) = later.first().copied().filter(|due| *due <= ms) {
                later.clear();
                if tracker.due(4, at(start, due)).is_some() {
                    now_told += 1;
                }
            }
            match tracker.heard(4, named(Kind::Editing, &name), true, at(start, ms)) {
                Tell::Now(_) => now_told += 1,
                Tell::Later(wait) => {
                    assert!(wait <= AT_MOST_EVERY, "{wait:?}");
                    assert!(later.is_empty(), "one telling waits at a time");
                    later.push(ms + u64::try_from(wait.as_millis()).unwrap());
                }
                Tell::Nothing => {}
            }
        }
        assert!(
            (3..=5).contains(&now_told),
            "{now_told} tellings in a second"
        );
        // What was put off last is the last thing said, never a stale one.
        let last = tracker
            .due(4, at(start, 1300))
            .expect("the last word is told");
        assert_eq!(
            last.doing.and_then(|doing| doing.name).as_deref(),
            Some("f1000.rs")
        );
        assert_eq!(tracker.due(4, at(start, 2000)), None, "and told once");
    }

    #[test]
    fn the_same_thing_said_again_is_not_told_again() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000));
        assert_eq!(
            tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 2000)),
            Tell::Nothing
        );
    }

    #[test]
    fn each_telling_is_numbered_after_the_one_before_it() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        let Tell::Now(first) = tracker.reported(4, 1, true, start) else {
            panic!("told")
        };
        let Tell::Now(second) = tracker.reported(4, 1, false, at(start, 1000)) else {
            panic!("told")
        };
        assert!(second.sequence > first.sequence);
    }

    #[test]
    fn one_chat_s_tools_say_nothing_of_another_and_a_closed_chat_is_forgotten() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        tracker.reported(5, 1, true, start);
        tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000));
        assert_eq!(tracker.doing(5), Some(&Doing::of(Kind::Thinking)));
        assert_eq!(tracker.all().len(), 2);
        tracker.closed(4);
        assert_eq!(tracker.doing(4), None);
        assert_eq!(tracker.due(4, at(start, 2000)), None);
        let left = tracker.all();
        assert_eq!(left.iter().map(|(chat, _)| *chat).collect::<Vec<_>>(), [5]);
        assert_eq!(left[0].1.doing, Some(Doing::of(Kind::Thinking)));
    }
}
