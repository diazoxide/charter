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
//! A kind, whether its tool has come back, and for three kinds one short name:
//!
//! | Kind | While the tool is in flight | Once it has come back | Its name |
//! |---|---|---|---|
//! | `thinking` | thinking | | none |
//! | `command` | running a command; running `cargo` | ran a command; ran `cargo` | a program on [`PROGRAMS`] |
//! | `editing` | editing a file; editing `<file>` | edited a file; edited `<file>` | the file's base name |
//! | `reading` | reading a file; reading `<file>`; reading 3 files | read a file; read `<file>`; read 3 files | the file's base name |
//! | `searching` | searching | searched | none |
//! | `fetching` | fetching a page | fetched a page | none |
//! | `helper` | waiting on a helper | a helper finished | none |
//! | `dispatching` | dispatching a task | dispatched a task | none |
//! | `asking` | asking a question | asked a question | none |
//! | `reporting` | writing its report | wrote its report | none |
//! | `tool` | using a tool | used a tool | none |
//!
//! **Never** a command's arguments, a URL, a search's pattern, a file's folder or contents, a
//! tool's own name, or anything a tool came back with.
//!
//! # Nothing is said that was not heard
//!
//! - `thinking` is said only from a turn's start until the first tool heard in it. After that
//!   the line keeps the last thing heard: in the present while its tool is in flight, in the
//!   past once a hook says that tool came back, until the next tool heard.
//! - A tool no hook runs on says nothing, and the line keeps what it last said. So does a hook
//!   after a tool the line is not about.
//! - Where a harness has no hook after a tool (Claude Code's `Read` and `Grep`, all of Codex),
//!   nothing says the tool came back, so the line stays in the present until the next tool
//!   heard. That is the one inexactness left.
//! - `dispatching`, `asking` and `reporting` are said by the app alone, when the chat's own
//!   purlis command reaches it ([`Tracker::asked`]). A line on the wire cannot claim them, nor
//!   `thinking`: from the wire each reads as `tool` ([`Said::neutral`]).
//!
//! # What a chat can make it say
//!
//! A hook runs in the chat's own process tree, and anything holding the chat's token can write
//! the line. So the name is the chat's word, and the app believes none of it until
//! [`Said::neutral`] has passed it:
//!
//! - a **program** is one of [`PROGRAMS`], a fixed list, matched whole against the command's
//!   first word. Every command line the row can show is therefore one purlis wrote;
//! - a **file's name** is ASCII letters and digits and seven marks, at most
//!   [`LONGEST_FILE_NAME`] characters ([`file_name`]). No space, nothing invisible, no letter of
//!   another script: a name in another script reads "editing a file".
//!
//! A name that fails is dropped whole and the kind is said without one. The hook applies the
//! same rule before it sends, so a path never leaves the hook either. A file's base name can
//! itself say something (`acquisition-acme.md`), and it is shown.
//!
//! # Which tools are heard, per harness
//!
//! Only tools some armed hook runs on ([`crate::hookreg`]):
//!
//! - **Claude Code:** before `Bash`, `Read`, `Grep`, `Write`, `Edit`, `MultiEdit`, `Task`,
//!   `Agent` and purlis's own dispatch tools; after `Bash`, `Write`, `Edit`, `MultiEdit`,
//!   `Task`, `Agent` and `Skill`. Nothing for `WebFetch`, `WebSearch`, `Glob`, `SendMessage`
//!   or another server's tool.
//! - **opencode:** every tool before it runs, since purlis's plugin routes each to a hook
//!   ([`crate::opencode::TOOLS`]) under Claude Code's name for it where it has one; after
//!   `write`, `edit`, `task` and `skill`.
//! - **Codex:** its shell alone, before it runs (`plugin::CODEX`). With its hooks untrusted
//!   nothing is heard and nothing is said.
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
    /// A turn has begun and no tool has been heard in it yet.
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

    /// Whether only the app says this, from an ask of the chat's that really reached it.
    fn said_by_the_app(self) -> bool {
        matches!(self, Self::Dispatching | Self::Asking | Self::Reporting)
    }

    /// This kind as a line on the wire may claim it: a kind no hook can see reads as
    /// [`Kind::Tool`], so nothing a chat writes by hand says it made a report, asked a
    /// question or dispatched a task, or that it is only thinking.
    fn as_the_wire_may_say(self) -> Self {
        if self.said_by_the_app() || self == Self::Thinking {
            Self::Tool
        } else {
            self
        }
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
    Ended {
        /// Which kind of tool, where the hook could tell. The line goes to the past only if
        /// this is what it is about.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<Kind>,
    },
}

impl Said {
    /// **Whether this can say its chat got past a prompt it was stopped on** (#1601): the
    /// person answered it in the chat's own pane, which no hook says. A tool of the chat's own
    /// that came back says it, once one began after the chat asked
    /// ([`Said::starts_a_tool_of_its_own`], `state::Chat::tool_said`): a tool runs only once
    /// the prompt before it is answered, and one already at work when the chat asked comes back
    /// whatever the person does.
    ///
    /// - **Not a tool about to run**: that may be the very call the prompt is about, heard
    ///   after its prompt was.
    /// - **Not a helper that came back**: another of its helpers may be the one asking.
    pub fn goes_on_past_a_prompt(&self) -> bool {
        matches!(self, Self::Ended { kind } if *kind != Some(Kind::Helper))
    }

    /// **Whether this says a tool of its chat's own began** (#1601): heard after the chat came
    /// to wait on its prompt, it is what a tool that comes back must follow before it says the
    /// chat got past the prompt (`state::Chat::tool_said`). Not a helper: another of its
    /// helpers may be the one asking.
    pub fn starts_a_tool_of_its_own(&self) -> bool {
        matches!(self, Self::Began { kind, .. } if *kind != Kind::Helper)
    }

    /// The same, believing nothing of it: a kind only a hook can see
    /// ([`Kind::as_the_wire_may_say`]), and a name only for a kind that has one, and only if it
    /// passes that kind's rule unchanged.
    pub fn neutral(self) -> Self {
        match self {
            Self::Began { kind, name } => {
                let kind = kind.as_the_wire_may_say();
                Self::Began {
                    kind,
                    name: name.filter(|name| match kind {
                        Kind::Command => program(name).as_deref() == Some(name),
                        Kind::Editing | Kind::Reading => file_name(name).as_deref() == Some(name),
                        _ => false,
                    }),
                }
            }
            Self::Ended { kind } => Self::Ended {
                kind: kind.map(Kind::as_the_wire_may_say),
            },
        }
    }
}

/// The longest file name said, in characters. A longer one is not said at all.
pub const LONGEST_FILE_NAME: usize = 48;

/// The marks a file name may hold beside ASCII letters and digits.
const FILE_MARKS: &str = "._-+@~#";

/// The base name of the file a tool was given as `path`, or nothing when it is not one purlis
/// will say: empty, with no letter or digit, longer than [`LONGEST_FILE_NAME`], or holding
/// anything but ASCII letters, ASCII digits and [`FILE_MARKS`].
///
/// **ASCII, and not "letters"**: the letters of every script include ones that draw as a
/// blank, ones that stack on the letter before, and whole alphabets styled to look like
/// another face, so a rule over them cannot promise what a name looks like. A name in another
/// script is not said, and the row reads "editing a file".
pub fn file_name(path: &str) -> Option<String> {
    let base = path.rsplit(['/', '\\']).next()?;
    let fits = base.len() <= LONGEST_FILE_NAME
        && base.chars().any(|one| one.is_ascii_alphanumeric())
        && base
            .chars()
            .all(|one| one.is_ascii_alphanumeric() || FILE_MARKS.contains(one));
    fits.then(|| base.to_owned())
}

/// Every program a row may name: "running cargo".
///
/// **A list, and not a rule over the command's first word.** A rule shows whatever a chat
/// puts first: a secret run by mistake as a command, a word chosen to read as one of the
/// app's, or a wrapper that says nothing true (`cd x && npm test` is not "running cd"). With
/// a list every command line the row can show is one purlis wrote, and the whole set can be
/// read here. So it holds tools a developer knows at a glance, and no shell builtin and no
/// wrapper (`cd`, `sudo`, `env`, `bash`, `sh`, `time`, `nohup`, `xargs`): those, and every
/// program not here, read "running a command". Adding a tool is adding a line.
pub const PROGRAMS: &[&str] = &[
    "cargo",
    "rustc",
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "node",
    "deno",
    "bun",
    "python",
    "python3",
    "pip",
    "uv",
    "pytest",
    "go",
    "make",
    "cmake",
    "git",
    "gh",
    "docker",
    "kubectl",
    "helm",
    "terraform",
    "purlis",
    "tsc",
    "eslint",
    "prettier",
    "vitest",
    "jest",
    "ruff",
    "mypy",
    "mvn",
    "gradle",
    "dotnet",
    "swift",
    "xcodebuild",
    "rg",
    "grep",
    "ls",
    "cat",
    "sed",
    "awk",
    "curl",
    "wget",
    "jq",
];

/// The program a shell command starts with, where its first word is, whole and as written,
/// one of [`PROGRAMS`]: no path in front, no extension, no other case. No later word is ever
/// looked at, so a variable set in front of a command, a wrapper and a path all say nothing.
pub fn program(command: &str) -> Option<String> {
    let first = command.split_whitespace().next()?;
    PROGRAMS
        .iter()
        .find(|listed| **listed == first)
        .map(|listed| (*listed).to_owned())
}

/// Where a file tool carries its path: Claude Code's spelling, opencode's, `Grep`'s, a
/// notebook's.
const PATH_KEYS: [&str; 4] = ["file_path", "filePath", "path", "notebook_path"];

/// The kind of the tool a hook's payload names.
fn kind_of(tool: &str) -> Kind {
    match tool {
        "Bash" | "bash" | "shell" | "exec_command" | "local_shell" => Kind::Command,
        "Read" => Kind::Reading,
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => Kind::Editing,
        "Grep" | "Glob" | "WebSearch" => Kind::Searching,
        "WebFetch" => Kind::Fetching,
        "Task" | "Agent" => Kind::Helper,
        // purlis's own dispatch tools among them: the app says what those are when the ask
        // itself arrives.
        _ => Kind::Tool,
    }
}

/// What the hook `word` says its chat is doing, from the payload a harness gave it, or nothing
/// for a hook that is not a tool's.
///
/// A hook before a tool says the tool began; one after says a tool of that kind came back. The
/// payload is read for the tool's name and, for a shell or a file tool, the one name of
/// [`Said::Began`].
pub fn of_hook(word: &str, payload: &Value) -> Option<Said> {
    let kind = kind_of(payload["tool_name"].as_str().unwrap_or_default());
    if word.starts_with("posttooluse") {
        return Some(Said::Ended { kind: Some(kind) });
    }
    if !word.starts_with("pretooluse") {
        return None;
    }
    let input = &payload["tool_input"];
    let name = match kind {
        Kind::Command => input["command"].as_str().and_then(program),
        Kind::Reading | Kind::Editing => PATH_KEYS
            .iter()
            .find_map(|key| input[*key].as_str())
            .and_then(file_name),
        _ => None,
    };
    Some(Said::Began { kind, name })
}

/// What a tool hook that answered `decision` sends the app of its chat, or nothing.
///
/// **Only for a call that will run**: one the hook allowed or said nothing about. A call it
/// refused did not run, and one the person is being asked about may yet be refused, so the row
/// never says "running" of a command that nobody let run.
pub fn of_answered_hook(
    word: &str,
    payload: &Value,
    decision: crate::hookwire::Decision,
) -> Option<Said> {
    use crate::hookwire::Decision;
    matches!(decision, Decision::Allow | Decision::None)
        .then(|| of_hook(word, payload))
        .flatten()
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
    /// Whether a hook said its tool came back: the window then says it in the past.
    pub over: bool,
}

impl Doing {
    fn of(kind: Kind) -> Self {
        Self {
            kind,
            name: None,
            count: 0,
            over: false,
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

    /// A tool hook of chat `chat` said `said`, of which nothing is believed
    /// ([`Said::neutral`]). Nothing for a chat that is not `running`: its hooks have not been
    /// heard to begin a turn, or the turn is over.
    ///
    /// A tool that began replaces what the line said. A tool that came back puts the line in
    /// the past only if the line is about that kind of tool; otherwise the line stays as it
    /// is, since which tool came back was not heard.
    pub fn heard(&mut self, chat: u32, said: Said, running: bool, now: Instant) -> Tell {
        if !running {
            return Tell::Nothing;
        }
        match said.neutral() {
            Said::Began { kind, name } => self.began(chat, kind, name, now),
            Said::Ended { kind } => {
                let ends = |doing: &Doing| {
                    !doing.over
                        && (kind == Some(doing.kind)
                            // What the app said of a purlis command is over when the shell or
                            // the tool that carried the command comes back.
                            || (doing.kind.said_by_the_app()
                                && matches!(kind, Some(Kind::Command | Kind::Tool))))
                };
                let Some(doing) = self
                    .chats
                    .get_mut(&chat)
                    .and_then(|held| held.doing.as_mut())
                    .filter(|doing| ends(doing))
                else {
                    return Tell::Nothing;
                };
                doing.over = true;
                self.settle(chat, now)
            }
        }
    }

    /// Chat `chat`'s own purlis command reached the app: `kind` is one of
    /// [`Kind::Dispatching`], [`Kind::Asking`] and [`Kind::Reporting`], and nothing of the
    /// command is kept. Only the app calls this, from an ask it received; no line on the wire
    /// reaches it.
    pub fn asked(&mut self, chat: u32, kind: Kind, running: bool, now: Instant) -> Tell {
        if !running {
            return Tell::Nothing;
        }
        self.began(chat, kind, None, now)
    }

    fn began(&mut self, chat: u32, kind: Kind, name: Option<String>, now: Instant) -> Tell {
        let held = self.chats.entry(chat).or_default();
        held.doing = Some(match held.doing.take() {
            // Reads in a row are counted, and past the first no one file is named.
            Some(Doing {
                kind: Kind::Reading,
                count,
                ..
            }) if kind == Kind::Reading => Doing {
                kind,
                name: None,
                count: count.saturating_add(1),
                over: false,
            },
            _ => Doing {
                kind,
                name,
                count: u32::from(kind == Kind::Reading),
                over: false,
            },
        });
        self.settle(chat, now)
    }

    /// The telling [`Tell::Later`] put off: what the chat is doing by now, if that is not what
    /// the window was last told.
    pub fn due(&mut self, chat: u32, now: Instant) -> Option<Told> {
        let held = self.chats.get_mut(&chat)?;
        held.waiting = false;
        (held.doing != held.told).then(|| held.tell(now))
    }

    /// Chat `chat` is gone, and what was held of it. Answers the telling that takes its line
    /// away from a window that may still hold one, for a chat anything was held of.
    pub fn closed(&mut self, chat: u32) -> Option<Told> {
        self.chats.remove(&chat).map(|_| Told {
            sequence: sequence(),
            doing: None,
        })
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

    fn ended(kind: Kind) -> Said {
        Said::Ended { kind: Some(kind) }
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

    fn unnamed(kind: Kind) -> Said {
        Said::Began { kind, name: None }
    }

    #[test]
    fn only_a_tool_of_its_own_that_came_back_says_a_chat_got_past_its_prompt() {
        // #1601: a chat stopped on its harness's prompt goes on once the person answers it in
        // its pane, which no hook says. A tool of its own that came back says it: the call
        // the prompt was about ran, or the turn went on to the next one.
        for kind in [Kind::Command, Kind::Editing, Kind::Reading, Kind::Tool] {
            assert!(ended(kind).goes_on_past_a_prompt(), "{kind:?}");
        }
        assert!(Said::Ended { kind: None }.goes_on_past_a_prompt());
        // A helper that came back is no answer: another helper may be the one asking.
        assert!(!ended(Kind::Helper).goes_on_past_a_prompt());
        // A tool about to run may be the very call being asked about, heard late.
        assert!(!unnamed(Kind::Command).goes_on_past_a_prompt());
        assert!(!named(Kind::Editing, "a.rs").goes_on_past_a_prompt());
    }

    #[test]
    fn a_tool_of_its_own_about_to_run_is_one_that_began_and_a_helper_is_not() {
        // #1601: what a tool that comes back must follow to say its chat got past its prompt.
        for kind in [Kind::Command, Kind::Editing, Kind::Reading, Kind::Tool] {
            assert!(unnamed(kind).starts_a_tool_of_its_own(), "{kind:?}");
            assert!(!ended(kind).starts_a_tool_of_its_own(), "{kind:?}");
        }
        assert!(named(Kind::Editing, "a.rs").starts_a_tool_of_its_own());
        assert!(!unnamed(Kind::Helper).starts_a_tool_of_its_own());
    }

    /// A line as the window is told it: in flight, or `over`.
    fn line(kind: Kind, name: Option<&str>, count: u32, over: bool) -> Option<Doing> {
        Some(Doing {
            kind,
            name: name.map(str::to_owned),
            count,
            over,
        })
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
        // purlis's own dispatch tools are a tool to a hook: the app says what they are when
        // the ask itself arrives.
        for tool in [
            "mcp__purlis__dispatch",
            "mcp__purlis__dispatch_report",
            "mcp__purlis__dispatch_list",
        ] {
            assert_eq!(
                said(
                    "pretooluse-dispatch",
                    json!({"tool_name": tool, "tool_input": {"brief": "x"}})
                ),
                began(Kind::Tool, None),
                "{tool}"
            );
        }
        // A hook after a tool says which kind came back, and nothing the tool came back with.
        for (word, tool, kind) in [
            ("posttooluse", "Edit", Kind::Editing),
            ("posttooluse-skill", "Skill", Kind::Tool),
            ("posttooluse-dispatch", "Task", Kind::Helper),
            ("posttooluse-message", "SendMessage", Kind::Tool),
            ("posttooluse-blocked", "Bash", Kind::Command),
            ("posttoolusefailure-blocked", "Bash", Kind::Command),
        ] {
            assert_eq!(
                said(
                    word,
                    json!({"tool_name": tool, "tool_input": {"command": "cargo CANARY",
                        "file_path": "/w/CANARY.rs"}, "tool_response": {"stdout": "CANARY"}})
                ),
                Some(ended(kind)),
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
        assert_eq!(
            said("posttooluse", "Write", json!({"filePath": "/w/b.ts"})),
            Some(ended(Kind::Editing))
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
        // Codex sends a command as a list in some shapes: no first word is read off it.
        assert_eq!(
            of_hook(
                "pretooluse",
                &json!({"tool_name": "shell", "tool_input": {"command": ["cargo", "test"]}})
            ),
            began(Kind::Command, None)
        );
    }

    #[test]
    fn only_a_call_that_will_run_is_said() {
        use crate::hookwire::Decision;
        let payload = json!({"tool_name": "Bash", "tool_input": {"command": "cargo test"}});
        for runs in [Decision::Allow, Decision::None] {
            assert_eq!(
                of_answered_hook("pretooluse", &payload, runs),
                began(Kind::Command, Some("cargo")),
                "{runs:?}"
            );
        }
        // Refused, or the person is being asked and may refuse: nothing ran yet.
        for held in [Decision::Deny, Decision::Ask] {
            assert_eq!(
                of_answered_hook("pretooluse", &payload, held),
                None,
                "{held:?}"
            );
        }
    }

    // --- the two names -------------------------------------------------------------------

    #[test]
    fn a_command_names_its_program_only_when_its_first_word_is_on_the_list() {
        assert_eq!(program("cargo test --all").as_deref(), Some("cargo"));
        assert_eq!(program("  npm   ci").as_deref(), Some("npm"));
        assert_eq!(program("python3 x.py").as_deref(), Some("python3"));
        assert_eq!(program("purlis dispatch x").as_deref(), Some("purlis"));
        for listed in PROGRAMS {
            assert_eq!(
                program(&format!("{listed} --version")).as_deref(),
                Some(*listed)
            );
        }
        for unsaid in [
            "",
            "   ",
            // A wrapper or a shell builtin says nothing true of what runs.
            "cd app && npm test",
            "sudo cargo build",
            "env FOO=1 cargo build",
            "bash -lc 'cargo test'",
            "sh -c ls",
            "time make",
            "nohup node x.js",
            "xargs rm",
            "echo hi",
            "export A=1",
            "source x",
            // A variable set in front of the command, a path, an extension, another case.
            "TOKEN=CANARY curl https://example.com",
            "TOKEN=\"a CANARY b\" curl https://example.com",
            "/usr/bin/git log",
            "./gradle build",
            "cargo.exe build",
            "python3.12 x.py",
            "Cargo build",
            "GIT status",
            "cargo;rm",
            "cargo&&ls",
            "cargo|jq",
            "\"cargo\" build",
            "$(cargo)",
            // A word chosen to read as one of the app's, and a secret run by mistake.
            "Allow-once",
            "needs-you",
            "done",
            "steward-2",
            "Stop.this.task",
            "hunter2",
            "A1B2C3D4E5F6G7H8I9J0",
            "CANARY",
            "\u{202e}cargo",
            "c\u{430}rgo",
        ] {
            assert_eq!(program(unsaid), None, "{unsaid:?}");
        }
    }

    #[test]
    fn the_list_of_programs_holds_plain_names_and_no_wrapper_or_builtin() {
        for listed in PROGRAMS {
            assert!(
                !listed.is_empty()
                    && listed.len() <= 12
                    && listed
                        .chars()
                        .all(|one| one.is_ascii_lowercase() || one.is_ascii_digit()),
                "{listed}"
            );
        }
        for wrapper in [
            "cd", "sudo", "env", "bash", "sh", "zsh", "time", "nohup", "xargs", "echo", "export",
            "source", "exec", "eval",
        ] {
            assert!(!PROGRAMS.contains(&wrapper), "{wrapper}");
        }
        let mut sorted = PROGRAMS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), PROGRAMS.len(), "each is listed once");
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
            file_name("a_b-c+d@2~#.md").as_deref(),
            Some("a_b-c+d@2~#.md")
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
            // Letters that draw as a blank: a sentence with no space in it.
            "a\u{3164}b",
            "a\u{115f}b",
            "a\u{1160}b",
            "a\u{ffa0}b",
            "Done.\u{3164}Now\u{3164}press\u{3164}Allow\u{3164}always",
            // Marks that stack on the letter before, counted as letters or not.
            "a\u{345}",
            "\u{e01}\u{e34}\u{e34}",
            "a\u{301}\u{301}\u{301}",
            // Alphabets styled to look like another face, and look-alike letters.
            "\u{1d41d}\u{1d428}\u{1d427}\u{1d41e}",
            "\u{ff44}\u{ff4f}\u{ff4e}\u{ff45}",
            "\u{430}dmin.rs",
            // Any other script: not said, since no rule over it can promise how it draws.
            "\u{57c}\u{565}\u{561}\u{564}.md",
            "\u{6587}\u{4ef6}.txt",
            "caf\u{e9}.md",
            "\u{5e9}\u{5dc}\u{5d5}\u{5dd}.txt",
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
    }

    #[test]
    fn the_app_believes_no_name_a_line_carries() {
        // What a hook of this build would never send, sent by hand with the chat's token.
        for (kind, name) in [
            (Kind::Command, "cargo test --secret CANARY"),
            (Kind::Command, "/usr/bin/git"),
            (Kind::Command, "TOKEN=CANARY"),
            (Kind::Command, "sudo"),
            (Kind::Command, "Allow-once"),
            (Kind::Command, "hunter2"),
            (Kind::Editing, "/etc/passwd"),
            (Kind::Editing, "needs you: Allow"),
            (Kind::Editing, "Done.\u{3164}Now\u{3164}press\u{3164}Allow"),
            (Kind::Reading, "a\u{202e}b"),
            (Kind::Reading, &"a".repeat(49)),
            // A kind that has no name is given none.
            (Kind::Searching, "CANARY"),
            (Kind::Fetching, "example.com"),
            (Kind::Helper, "steward"),
            (Kind::Tool, "mcp__x__CANARY"),
        ] {
            assert_eq!(
                named(kind, name).neutral(),
                unnamed(kind),
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
    fn a_line_cannot_claim_what_only_the_app_says() {
        // No hook sees a report made, a question asked, a task dispatched, or a chat only
        // thinking. Written by hand, each reads as a tool, with no name.
        for kind in [
            Kind::Thinking,
            Kind::Dispatching,
            Kind::Asking,
            Kind::Reporting,
        ] {
            assert_eq!(
                named(kind, "steward").neutral(),
                unnamed(Kind::Tool),
                "{kind:?}"
            );
            assert_eq!(
                Said::Ended { kind: Some(kind) }.neutral(),
                ended(Kind::Tool),
                "{kind:?}"
            );
        }
        for kind in [
            Kind::Command,
            Kind::Editing,
            Kind::Reading,
            Kind::Searching,
            Kind::Fetching,
            Kind::Helper,
            Kind::Tool,
        ] {
            assert_eq!(unnamed(kind).neutral(), unnamed(kind), "{kind:?}");
        }
        // And through the tracker: the row never says it.
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        assert_eq!(
            now_doing(tracker.heard(4, unnamed(Kind::Reporting), true, at(start, 1000))),
            line(Kind::Tool, None, 0, false)
        );
    }

    #[test]
    fn the_line_on_the_wire_holds_a_kind_and_a_name_and_an_unknown_kind_is_no_line() {
        assert_eq!(
            serde_json::to_string(&named(Kind::Command, "cargo")).unwrap(),
            r#"{"is":"began","kind":"command","name":"cargo"}"#
        );
        assert_eq!(
            serde_json::to_string(&unnamed(Kind::Helper)).unwrap(),
            r#"{"is":"began","kind":"helper"}"#
        );
        assert_eq!(
            serde_json::to_string(&ended(Kind::Command)).unwrap(),
            r#"{"is":"ended","kind":"command"}"#
        );
        assert_eq!(
            serde_json::from_str::<Said>(r#"{"is":"ended"}"#).unwrap(),
            Said::Ended { kind: None }
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
    fn a_tool_is_said_in_the_present_while_it_runs_and_in_the_past_once_it_is_back() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        assert_eq!(
            now_doing(tracker.reported(4, 1, true, start)),
            line(Kind::Thinking, None, 0, false)
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000))),
            line(Kind::Command, Some("cargo"), 0, false)
        );
        // The command came back: "ran cargo", and it stays until the next tool heard.
        assert_eq!(
            now_doing(tracker.heard(4, ended(Kind::Command), true, at(start, 2000))),
            line(Kind::Command, Some("cargo"), 0, true)
        );
        assert_eq!(
            tracker.heard(4, ended(Kind::Command), true, at(start, 2500)),
            Tell::Nothing,
            "it came back once"
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Editing, "a.rs"), true, at(start, 3000))),
            line(Kind::Editing, Some("a.rs"), 0, false)
        );
        assert_eq!(
            now_doing(tracker.heard(4, ended(Kind::Editing), true, at(start, 4000))),
            line(Kind::Editing, Some("a.rs"), 0, true)
        );
    }

    #[test]
    fn thinking_is_said_only_until_the_first_tool_heard_in_a_turn() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000));
        tracker.heard(4, ended(Kind::Command), true, at(start, 2000));
        // Minutes pass, in a tool no hook runs on perhaps: nothing is heard, nothing is said.
        assert_eq!(
            tracker.doing(4),
            line(Kind::Command, Some("cargo"), 0, true).as_ref()
        );
        // A report within the turn (a helper's end, say) changes nothing either.
        assert_eq!(tracker.reported(4, 1, true, at(start, 3000)), Tell::Nothing);
        assert_ne!(
            tracker.doing(4).map(|doing| doing.kind),
            Some(Kind::Thinking)
        );
    }

    #[test]
    fn a_tool_coming_back_that_the_line_is_not_about_changes_nothing() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        // Before any tool was heard: a skill came back, which no hook saw begin.
        assert_eq!(
            tracker.heard(4, ended(Kind::Tool), true, at(start, 500)),
            Tell::Nothing
        );
        assert_eq!(
            tracker.doing(4),
            line(Kind::Thinking, None, 0, false).as_ref()
        );
        // A command runs; another kind of tool comes back, and one that did not say which.
        tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000));
        for other in [
            ended(Kind::Editing),
            ended(Kind::Tool),
            Said::Ended { kind: None },
        ] {
            assert_eq!(
                tracker.heard(4, other.clone(), true, at(start, 2000)),
                Tell::Nothing,
                "{other:?}"
            );
        }
        assert_eq!(
            tracker.doing(4),
            line(Kind::Command, Some("cargo"), 0, false).as_ref()
        );
    }

    #[test]
    fn reads_in_a_row_are_counted_and_anything_else_begins_the_count_again() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Reading, "a.rs"), true, at(start, 1000))),
            line(Kind::Reading, Some("a.rs"), 1, false)
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Reading, "b.rs"), true, at(start, 2000))),
            line(Kind::Reading, None, 2, false)
        );
        // A read that came back and one more after it: still reads in a row.
        tracker.heard(4, ended(Kind::Reading), true, at(start, 2500));
        assert_eq!(
            tracker.doing(4),
            line(Kind::Reading, None, 2, true).as_ref()
        );
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Reading, "c.rs"), true, at(start, 3000))),
            line(Kind::Reading, None, 3, false)
        );
        tracker.heard(4, unnamed(Kind::Searching), true, at(start, 4000));
        assert_eq!(
            now_doing(tracker.heard(4, named(Kind::Reading, "d.rs"), true, at(start, 5000))),
            line(Kind::Reading, Some("d.rs"), 1, false)
        );
    }

    #[test]
    fn what_the_app_says_of_a_purlis_command_is_over_when_what_carried_it_comes_back() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        // `purlis dispatch …` in a shell: the hook, then the ask itself, then the shell back.
        tracker.heard(4, named(Kind::Command, "purlis"), true, at(start, 1000));
        assert_eq!(
            now_doing(tracker.asked(4, Kind::Dispatching, true, at(start, 2000))),
            line(Kind::Dispatching, None, 0, false)
        );
        assert_eq!(
            tracker.heard(4, ended(Kind::Helper), true, at(start, 2500)),
            Tell::Nothing
        );
        assert_eq!(
            now_doing(tracker.heard(4, ended(Kind::Command), true, at(start, 3000))),
            line(Kind::Dispatching, None, 0, true)
        );
        // Through purlis's own tool, which a hook sees as a tool.
        tracker.heard(4, unnamed(Kind::Tool), true, at(start, 4000));
        tracker.asked(4, Kind::Reporting, true, at(start, 5000));
        assert_eq!(
            now_doing(tracker.heard(4, ended(Kind::Tool), true, at(start, 6000))),
            line(Kind::Reporting, None, 0, true)
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
            line(Kind::Thinking, None, 0, false)
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
        assert_eq!(tracker.closed(9), None, "nothing was held of it");
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
    fn one_chat_s_tools_say_nothing_of_another_and_a_closed_chat_s_line_is_taken_away() {
        let start = Instant::now();
        let mut tracker = Tracker::default();
        tracker.reported(4, 1, true, start);
        tracker.reported(5, 1, true, start);
        let Tell::Now(last) =
            tracker.heard(4, named(Kind::Command, "cargo"), true, at(start, 1000))
        else {
            panic!("told")
        };
        assert_eq!(
            tracker.doing(5),
            line(Kind::Thinking, None, 0, false).as_ref()
        );
        assert_eq!(tracker.all().len(), 2);
        // Closed: the window is told it has no line, after whatever it was told last.
        let gone = tracker.closed(4).expect("its line is taken away");
        assert_eq!(gone.doing, None);
        assert!(gone.sequence > last.sequence);
        assert_eq!(tracker.doing(4), None);
        assert_eq!(tracker.due(4, at(start, 2000)), None);
        assert_eq!(tracker.closed(4), None, "and once");
        let left = tracker.all();
        assert_eq!(left.iter().map(|(chat, _)| *chat).collect::<Vec<_>>(), [5]);
        assert_eq!(left[0].1.doing, line(Kind::Thinking, None, 0, false));
    }
}
