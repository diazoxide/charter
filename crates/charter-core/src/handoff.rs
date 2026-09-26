//! The facts a handoff is made of, and every refusal that stands in front of one.
//!
//! A handoff opens a chat whose first message is a brief the operator approved
//! (`docs/handoff.md`). **The consent is the harness's own permission prompt for the exact
//! spelling `charter handoff …`** — `charter init` writes an `ask` rule for it — so charter
//! refuses every shape that prompt cannot stand in front of. That sentence is the whole
//! design, and each refusal below is one thing the prompt cannot see.
//!
//! This module is `charter/handoff.py`: **no I/O beyond the bytes [`read_brief`] is handed**,
//! and no plane, so every string a handoff produces can be checked without a control plane,
//! a terminal or a workspace on disk. The ordering of the refusals and the writes is
//! `charter/commands_handoff.py`, ported in `charter-cli/src/handoff.rs`.
//!
//! # Which refusals are here, and which are not
//!
//! charter refuses a handoff in two places, and only one of them is a command:
//!
//! | Refusal | Whose | Here? |
//! |---|---|---|
//! | a name that cannot be a workspace, the `--create`/`--vision` pair, a workspace that is or is not there, a `--persona` this plane does not define | the command's | yes |
//! | a brief from a closed stdin, from a terminal, not UTF-8, or empty | the command's | yes |
//! | a brief shaped like a credential | the command's ([`crate::secretshape`]) | yes |
//! | a first message that is empty, starts with `-`, is a single word, carries a NUL, or is past the byte bound | the command's (`commands_frame.background_refusal`) | yes |
//! | there is no frame to open a chat in the background of | the command's | yes: every chat the desktop app did not start, or whose app does not answer — see below |
//! | a spelling the host's rule does not match (`python3 -m charter handoff`, `charter 'handoff'`, a path) | the **hook's** (`hooks.pretooluse` A7) | no |
//! | a brief from a pipe, a file, a here-string, or a heredoc a shell runs | the **hook's** (A7) | no |
//! | a call from a sub-agent, or an unattended run (`bypassPermissions`) | the **hook's** (A7) | no |
//!
//! The four A7 rows each need a fact no command can observe: the *source spelling* of the
//! command line, and the harness's own hook payload (`agent_id`, `permission_mode`). They live
//! in `charter/hooks.py`, which is the PreToolUse guard, and they are ported in
//! [`crate::handoffguard`] — assembled with the other seven arms by [`crate::toolgate`] and
//! answered by `charter hook pretooluse` since M3.1 stage 6.
//!
//! **So a plane running this binary as its `charter` now has both halves**, which it did not
//! before that stage: `charter-cli/src/main.rs:is_a_tool_hook` used to answer every word in
//! the `pretooluse` namespace with exit 2 rather than decide anything. What a plane still does
//! not get from this binary is the hook's ALLOW half — the persona tool-gate — which
//! [`crate::toolgate`]'s header records as a declared gap.
//!
//! # The frame: the desktop app, or the printed command
//!
//! Python's handoff opens a chat in a background window of charter's own tmux server. This
//! charter has no tmux: the desktop app is its frame. A chat the app started asks the app to
//! open the handoff over the hook socket, on a single-use ticket (charter-app#204; what the
//! ticket is worth, and what it is not, is on [`crate::hookwire::OpenChat`]). Every other
//! chat, and every chat whose app does not answer, reaches the frame check and gets Python's
//! answer to a shell that is not a chat: the command to run in a new terminal, and a refusal.
//!
//! Either way the command does every check in front of the open, which is where a handoff's
//! whole value is, because charter fails toward no change, and it never claims to have opened
//! a chat it did not. The printed-command path writes nothing: Python's writes (the workspace,
//! its vision, the todo) all come *after* the frame check. The app path writes the workspace
//! when the call creates one, and after the open, the [`todo_text`] todo in the target
//! workspace and a `handoff` row in the dispatch log (`crate::dispatch::record_handoff`,
//! #372). The arrival mark is the app's own, drawn on its strip. The recorded scenario is
//! `handoff-inside-the-app-opens-the-chat-there-and-records-its-todo` (ADR 0046).

/// The first line of every handoff's first message. Facts charter can observe and no
/// instruction: where it came from, which workspace that was, and when.
///
/// The brackets are `⟨⟩` rather than `<>` so the line cannot be read as markup by anything
/// that renders the transcript.
pub const STAMP: &str = "⟨handoff from chat {chat} · workspace {workspace} · {when}⟩";

/// What the stamp calls a source that has no chat id — a handoff proposed from a shell
/// outside any frame, which is refused, but whose printed command still carries the stamp so
/// the chat it opens is marked the same way.
pub const NO_CHAT: &str = "none";

/// The most a first message may be, in bytes — `commands_frame.FIRST_MESSAGE_MAX_BYTES`.
///
/// charter's own bound, set under tmux's 16,364-byte command limit so a chat's names,
/// directory and identity still fit beside the message. Kept here although this binary
/// starts no tmux, because the bound is what a plane's operators have already written their
/// briefs against: two charters disagreeing about how long a brief may be is a handoff that
/// is refused by one and taken by the other.
pub const FIRST_MESSAGE_MAX_BYTES: usize = 12288;

// ----------------------------------------------------------------------------------------
// the brief
// ----------------------------------------------------------------------------------------

/// Why charter would not read a brief. Each carries its own sentence, and each names the
/// heredoc in the same breath, because a chat that reached one typed the command without it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoBrief {
    /// fd 0 is closed. `charter handoff beta 0<&-` is a spelling the Bash guard allows,
    /// because the two words in front of it are the exact ones.
    Closed,
    /// stdin is a terminal. **Asked before any read, and that order is the whole thing**: a
    /// read on a terminal blocks for ever with nothing on screen to say why, which inside an
    /// agent's Bash tool is a turn that never ends.
    Terminal,
    /// The bytes are not UTF-8. The decision has to be charter's, which is why the command
    /// reads bytes and decodes here rather than taking a decoded string from the runtime
    /// with whatever encoding the environment handed it.
    NotUtf8,
    /// No words at all. `split_whitespace` and never `trim`: `!text.trim().is_empty()` and
    /// `!text.trim_start().is_empty()` answer alike for every string, so the trim is a line
    /// nothing can turn red.
    Empty,
}

impl NoBrief {
    /// The sentence, with `{ws}` filled — `charter/handoff.py`'s four constants verbatim.
    pub fn say(&self, ws: &str) -> String {
        let heredoc = format!(
            "Pass the brief as a quoted heredoc in the same call:\n  charter handoff {ws} \
             <<'BRIEF'\n  <the brief>\n  BRIEF"
        );
        match self {
            Self::Closed => format!(
                "charter handoff: this shell has no stdin at all — the command was run with \
                 its input closed, so there is nothing to read a brief from and nothing was \
                 opened. {heredoc}"
            ),
            Self::Terminal => format!(
                "charter handoff: reads its brief from stdin, and stdin here is a terminal — \
                 nothing was opened. {heredoc}"
            ),
            Self::NotUtf8 => {
                "charter handoff: the brief on stdin is not UTF-8 text — nothing was opened."
                    .to_string()
            }
            // "Pass IT", where the two above say "Pass THE BRIEF". charter words this one
            // differently — the sentence in front of it has just named the brief — and the
            // differential caught the paraphrase, which is the whole reason it compares
            // stderr byte for byte.
            Self::Empty => format!(
                "charter handoff: the brief on stdin is empty — nothing was opened. {}",
                heredoc.replacen("Pass the brief as", "Pass it as", 1)
            ),
        }
    }
}

/// The brief, or why there is none.
///
/// `bytes` is `None` when fd 0 is closed, which Python meets as `sys.stdin is None`. The
/// brief is kept **verbatim** — leading blank lines, trailing newline and all. It is what the
/// new chat is sent, and a handoff that tidied it would send text nobody approved.
pub fn read_brief(bytes: Option<&[u8]>, is_terminal: bool) -> Result<String, NoBrief> {
    let Some(bytes) = bytes else {
        return Err(NoBrief::Closed);
    };
    // Before any read, and before the decode: see `NoBrief::Terminal`.
    if is_terminal {
        return Err(NoBrief::Terminal);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| NoBrief::NotUtf8)?;
    // Python's `str.split()`, whose whitespace is not `char::is_whitespace`'s.
    if !text
        .split(crate::memstore::is_python_space)
        .any(|w| !w.is_empty())
    {
        return Err(NoBrief::Empty);
    }
    Ok(text.to_string())
}

// ----------------------------------------------------------------------------------------
// what the new chat is sent
// ----------------------------------------------------------------------------------------

/// The stamp line for a handoff leaving `chat` in `workspace`.
///
/// Minutes, not seconds: the stamp is read by a person deciding whether this message is the
/// one they approved a moment ago, and a second's precision answers no question they have.
/// Local time, because the reader is in it.
pub fn stamp(chat: &str, workspace: &str, when: chrono::NaiveDateTime) -> String {
    STAMP
        .replace("{chat}", chat)
        .replace("{workspace}", workspace)
        .replace("{when}", &when.format("%Y-%m-%d %H:%M").to_string())
}

/// A first message read back: the stamp's facts, and the brief after the blank line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamped<'a> {
    /// The chat the handoff left, as `charter handoff` wrote it: the app's number for it.
    pub chat: &'a str,
    /// The workspace it left from.
    pub workspace: &'a str,
    /// The minute it left, as the stamp wrote it.
    pub when: &'a str,
    /// Everything after the blank line: the brief, verbatim.
    pub brief: &'a str,
}

/// `msg` read as a handoff's first message — [`STAMP`], a blank line, the brief — or `None`.
///
/// One line, closed by the only `⟩` on it, and the blank line after it: the shape
/// [`first_message`] writes and nothing looser.
pub fn stamped(msg: &str) -> Option<Stamped<'_>> {
    let (head, _) = STAMP.split_once("{chat}")?;
    let (line, rest) = msg.split_once('\n')?;
    let brief = rest.strip_prefix('\n')?;
    let inside = line.strip_prefix(head)?.strip_suffix('⟩')?;
    if inside.contains('⟩') {
        return None;
    }
    let (chat, tail) = inside.split_once(" · workspace ")?;
    let (workspace, when) = tail.rsplit_once(" · ")?;
    Some(Stamped {
        chat,
        workspace,
        when,
        brief,
    })
}

/// Whether `msg` opens with the stamp of a handoff leaving `chat`, followed by the blank line
/// [`first_message`] puts after it.
///
/// **The app asks this before it opens anything** (charter-app#204). In the app the control
/// on a handed-off chat is that the operator can see it and where it came from, and the
/// stamp is the half of that the chat itself carries. `charter handoff` always writes one,
/// so this refuses only a message that did not come through it: a process writing to the
/// socket directly cannot open an unmarked chat, nor one marked as another chat's.
pub fn is_stamped_from(msg: &str, chat: &str) -> bool {
    stamped(msg).is_some_and(|read| read.chat == chat)
}

/// The stamp the chat a handoff opened is actually sent: the same facts, with the chat it
/// came from named the way the operator sees it (charter-app#258).
///
/// The number in [`STAMP`] is what the app matches the asking chat by, and it has done that
/// job by the time this line is written; the chat reading it, and the operator scrolling back
/// to its first message, want `steward 3`, not `chat 16`.
pub const SHOWN_STAMP: &str = "⟨handoff from {from} · workspace {workspace} · {when}⟩";

/// The line a handoff that wants an answer adds under the stamp (charter-app#259).
///
/// Facts and the one command, like the stamp: the chat is not told what to think, only that
/// the chat that sent it is waiting on one report and how to send it.
pub const REPORT_ASK: &str = "⟨the chat that handed this off wants an answer: when the work is \
done, finish with `charter handoff report \"<summary>\"` — a few lines on what you did and what \
you found. It is sent once, and that chat reads it the next time it is prompted⟩";

/// The first message the new chat is actually sent: the wire message `msg` with its stamp
/// naming the parent as `from`, and [`REPORT_ASK`] under it when `report` — or `None` for a
/// message that is not stamped at all.
pub fn delivered(msg: &str, from: &str, report: bool) -> Option<String> {
    let read = stamped(msg)?;
    let line = SHOWN_STAMP
        .replace("{workspace}", read.workspace)
        .replace("{when}", read.when)
        // Last, so a name that happened to spell `{when}` is not filled in again.
        .replace("{from}", from);
    let ask = if report {
        format!("\n{REPORT_ASK}")
    } else {
        String::new()
    };
    Some(format!("{line}{ask}\n\n{}", read.brief))
}

// ----------------------------------------------------------------------------------------
// a report back (charter-app#259)
// ----------------------------------------------------------------------------------------

/// The most a report may be, in bytes. It becomes context on another chat's turn, which is
/// a summary's job and not a transcript's: a report that needs more names the file it wrote.
pub const MOST_REPORT_BYTES: usize = 4096;

/// A report charter will not hand back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadReport {
    /// Nothing but whitespace.
    Empty,
    /// Past [`MOST_REPORT_BYTES`], by this many bytes in all.
    TooLong(usize),
    /// A control character other than a line break, or an invisible formatting one.
    Undrawable,
}

impl BadReport {
    pub fn say(&self) -> String {
        match self {
            Self::Empty => "the report is empty — nothing was sent. Say what was done in a \
                            few lines: charter handoff report \"<summary>\""
                .to_owned(),
            Self::TooLong(n) => format!(
                "the report is {n} bytes, and a report is at most {MOST_REPORT_BYTES} — \
                 nothing was sent. Summarise, and name any longer write-up by its path."
            ),
            Self::Undrawable => "the report holds a control character or an invisible \
                                 formatting one, which charter will not hand to another chat \
                                 — nothing was sent. Plain text and line breaks only."
                .to_owned(),
        }
    }
}

/// A report as charter will hand it back: trimmed, bounded, and drawable — or why not.
///
/// **Refused, never stripped**, for [`crate::reopen::label`]'s reason: a character that makes
/// text read as something else is not tidied into the text the chat meant. A line break is the
/// one control character a summary may carry, because a summary of several lines is still
/// plain text.
pub fn report_summary(raw: &str) -> Result<String, BadReport> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(BadReport::Empty);
    }
    if text.len() > MOST_REPORT_BYTES {
        return Err(BadReport::TooLong(text.len()));
    }
    if text
        .chars()
        .any(|c| c != '\n' && crate::panel::undrawable(c))
    {
        return Err(BadReport::Undrawable);
    }
    Ok(text.to_owned())
}

/// The whole of what the new chat is sent: the stamp, a blank line, the brief verbatim.
///
/// The blank line is what keeps the stamp from reading as the brief's first line — which is
/// also the line [`title`] turns into a todo.
pub fn first_message(stamp_line: &str, brief: &str) -> String {
    format!("{stamp_line}\n\n{brief}")
}

/// `brief`'s first non-blank line, stripped — the todo's title and nothing else.
///
/// Non-blank rather than the first line: a heredoc written with the delimiter on its own line
/// often opens with a newline, and a todo titled by an empty string is one the store refuses.
///
/// **Split on `\n` and nothing else.** Python's `str.splitlines` also breaks on `\r`, `\x0b`,
/// `\x0c`, `\x1c`–`\x1e`, U+2028 and U+2029, so a brief whose first line carried any of those
/// was titled by a PREFIX of the line the operator wrote — charter's "first line" and theirs
/// meaning different things, silently. A shell heredoc ends a line at `\n`, which is what the
/// operator typed into. What a control character then does on screen is a rendering question,
/// answered where the rendering is ([`crate::shown::one_line`]), not by cutting the text short
/// here.
pub fn title(brief: &str) -> String {
    for line in brief.split('\n') {
        let stripped = crate::memstore::py_strip(line);
        if !stripped.is_empty() {
            return stripped.to_string();
        }
    }
    String::new()
}

/// The todo a handoff records in the TARGET workspace: the title and where it came from.
///
/// **The brief is not in it, and that is not brevity.** A LIVE workspace commits `todos/**`,
/// and a brief never reaches a committed file. The title is enough for the person reading the
/// list to recognise the work, and the chat that was opened is holding the rest.
pub fn todo_text(brief: &str, source_chat: &str, source_workspace: &str) -> String {
    format!(
        "{}\n\nHanded off from chat {source_chat} · workspace {source_workspace}. The full \
         brief is private to the chat it opened.",
        title(brief)
    )
}

// ----------------------------------------------------------------------------------------
// what the first message may be
// ----------------------------------------------------------------------------------------

/// A first message charter will not start a chat on — `commands_frame.background_refusal`'s
/// message half, which is every reason that is about the MESSAGE rather than about tmux.
///
/// Cheapest first, and in Python's order, because the order is what an operator reads: a
/// message that is both empty and short is refused for being empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadMessage {
    /// A chat nobody has told anything sits at its prompt, and one opened in the background
    /// is then silence by construction.
    Empty,
    /// The harness would read it as one of its own flags, not as a message.
    Flag,
    /// A CLI with subcommands may match it against them (`codex login`). A handoff's stamp
    /// makes a real first message several words, so only a direct caller of the seam gets
    /// here. The word is already through [`crate::personas::one_line`]: it is printed into a
    /// refusal, and a value crossing into a line with structure forges one.
    OneWord(String),
    /// No command-line argument can carry a NUL byte.
    Nul,
    /// Past [`FIRST_MESSAGE_MAX_BYTES`].
    TooLong(usize),
}

impl BadMessage {
    pub fn say(&self) -> String {
        match self {
            Self::Empty => "cannot open a chat with an empty first message — a chat nobody \
                            has told anything sits at its prompt, and one opened in the \
                            background is then silence by construction. Nothing was opened."
                .to_string(),
            Self::Flag => "cannot open a chat whose first message starts with `-` — the \
                           harness would read it as one of its own flags, not as a message. \
                           Nothing was opened; start the message with a word."
                .to_string(),
            Self::OneWord(word) => format!(
                "cannot open a chat whose first message is the single word '{word}' — the \
                 harness may read it as one of its own subcommands (`codex login`). Nothing \
                 was opened; say it as a sentence."
            ),
            Self::Nul => "cannot open a chat whose first message contains a NUL byte — a \
                          command-line argument cannot carry one. Nothing was opened."
                .to_string(),
            Self::TooLong(n) => format!(
                "cannot open a chat with a {n}-byte first message — charter refuses one past \
                 {FIRST_MESSAGE_MAX_BYTES} bytes, its own bound, so the chat's names, directory \
                 and identity still fit beside it on the harness's command line. Nothing was opened; shorten it, and name long material by \
                 its path instead of pasting it."
            ),
        }
    }
}

/// What the byte bound's refusal cannot say for itself: only the handoff knows the message is
/// a stamp, a blank line and a brief, so only the handoff can say why a brief that fits alone
/// was refused.
pub fn stamped_message_note(brief_bytes: usize) -> String {
    format!(
        "  The bytes counted are the message the new chat is sent: the stamp line, a blank \
         line, then your brief, which is {brief_bytes} bytes on its own. A brief that fits \
         alone can be over once it is stamped."
    )
}

/// Every reason charter would not start a chat on `msg`, or `None`.
///
/// The byte count is of the bytes `exec` is handed, which for a Rust `String` is its UTF-8
/// length — Python counts `os.fsencode(msg)`, and for a string that came off a UTF-8 decode
/// those are the same bytes.
pub fn bad_message(msg: &str) -> Option<BadMessage> {
    // `str.split()` with no separator, which is Python's whitespace and not Rust's: the file
    // separators U+001C–U+001F are whitespace to Python and not to `char::is_whitespace`, so
    // a message of nothing but those is "no words" to charter and one word to a naive port.
    let mut words = msg
        .split(crate::memstore::is_python_space)
        .filter(|w| !w.is_empty());
    let Some(first) = words.next() else {
        return Some(BadMessage::Empty);
    };
    if msg.starts_with('-') {
        return Some(BadMessage::Flag);
    }
    if words.next().is_none() {
        return Some(BadMessage::OneWord(crate::personas::one_line(first)));
    }
    if msg.contains('\0') {
        return Some(BadMessage::Nul);
    }
    if msg.len() > FIRST_MESSAGE_MAX_BYTES {
        return Some(BadMessage::TooLong(msg.len()));
    }
    None
}

// ----------------------------------------------------------------------------------------
// the command charter prints when it cannot open a chat itself
// ----------------------------------------------------------------------------------------

/// How a harness takes a first message, or `None` for one charter has not measured
/// (`harness.base.first_message_argv`).
///
/// Measured per harness, never guessed: `claude` and `codex` take a positional prompt
/// (`claude --help` on 2.1.268, `codex --help` on codex-cli 0.147.0), and opencode's
/// positional is `[project]`, so its prompt is `--prompt` (`opencode --help` on 1.18.23).
pub fn first_message_argv(kind: &str, text: &str) -> Option<Vec<String>> {
    match kind {
        "claude" | "codex" => Some(vec![text.to_string()]),
        "opencode" => Some(vec!["--prompt".to_string(), text.to_string()]),
        _ => None,
    }
}

/// A word as a POSIX shell will read it back unchanged — `shlex.quote`.
///
/// Single quotes and the `'\''` dance, because a single-quoted string is the one shell
/// quoting form with no escapes inside it at all.
pub fn quote(word: &str) -> String {
    // Python's `shlex.quote` leaves a word alone when every character is in its safe set,
    // and answers `''` for the empty string.
    if word.is_empty() {
        return "''".to_string();
    }
    const SAFE: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789@%+=:,./-_";
    if word.chars().all(|c| SAFE.contains(c)) {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> chrono::NaiveDateTime {
        text.parse().unwrap()
    }

    #[test]
    fn the_stamp_is_facts_and_no_instruction() {
        assert_eq!(
            stamp("c1", "beta", at("2026-05-04T11:32:17")),
            "⟨handoff from chat c1 · workspace beta · 2026-05-04 11:32⟩",
            "minutes, not seconds"
        );
    }

    #[test]
    fn a_message_is_stamped_from_the_chat_its_stamp_names_and_no_other() {
        let msg = first_message(
            &stamp("7", "default", at("2026-05-04T11:32:17")),
            "# Goal\nbody",
        );

        assert!(is_stamped_from(&msg, "7"));
        assert!(
            !is_stamped_from(&msg, "70"),
            "a prefix of another chat's number"
        );
        assert!(!is_stamped_from(&msg, "3"), "another chat's stamp");
        assert!(!is_stamped_from("# Goal\nbody", "7"), "no stamp at all");
        assert!(
            !is_stamped_from(&msg.replacen("\n\n", "\n", 1), "7"),
            "the blank line is part of the shape"
        );
        assert!(
            !is_stamped_from("⟨handoff from chat 7 · workspace x", "7"),
            "one line"
        );
    }

    #[test]
    fn the_first_message_keeps_the_stamp_off_the_briefs_first_line() {
        let msg = first_message("S", "# Goal\nbody");
        assert_eq!(msg, "S\n\n# Goal\nbody");
        assert_eq!(
            title(&msg),
            "S",
            "the stamp is the message's own first line"
        );
    }

    #[test]
    fn the_title_is_the_first_non_blank_line_split_on_newline_alone() {
        assert_eq!(
            title("\n\n  # Retry the webhooks  \nrest"),
            "# Retry the webhooks"
        );
        assert_eq!(
            title("   \n \t \n"),
            "",
            "a brief of whitespace titles nothing"
        );
        // `\x0b` is a line break to Python's `splitlines` and not to a shell heredoc. The
        // whole line the operator typed is the title.
        assert_eq!(title("a\x0bb\nnext"), "a\x0bb");
        assert_eq!(
            title("a\rb\nnext"),
            "a\rb",
            "a bare CR does not end the line"
        );
    }

    #[test]
    fn a_closed_stdin_is_classified_and_never_a_crash() {
        assert_eq!(read_brief(None, false), Err(NoBrief::Closed));
        assert!(
            NoBrief::Closed
                .say("beta")
                .contains("charter handoff beta <<'BRIEF'")
        );
    }

    #[test]
    fn the_empty_brief_names_the_heredoc_in_charters_own_words() {
        // charter says "Pass IT" here and "Pass THE BRIEF" in the other two — the sentence in
        // front of this one has just named the brief. The differential caught the paraphrase,
        // which is the whole reason it compares stderr byte for byte.
        assert!(
            NoBrief::Empty
                .say("beta")
                .contains("nothing was opened. Pass it as a quoted heredoc in the same call:")
        );
        for other in [NoBrief::Closed, NoBrief::Terminal] {
            assert!(
                other
                    .say("beta")
                    .contains("Pass the brief as a quoted heredoc"),
                "{other:?}"
            );
        }
    }

    #[test]
    fn a_terminal_is_refused_before_any_read() {
        // The bytes here would be a perfectly good brief; the refusal is about the terminal,
        // and asking it second is a read that blocks for ever.
        assert_eq!(read_brief(Some(b"# a brief"), true), Err(NoBrief::Terminal));
    }

    #[test]
    fn a_brief_that_is_not_utf8_is_refused_and_one_of_whitespace_is_empty() {
        assert_eq!(
            read_brief(Some(&[0xff, 0xfe]), false),
            Err(NoBrief::NotUtf8)
        );
        assert_eq!(read_brief(Some(b"  \n\t\n "), false), Err(NoBrief::Empty));
    }

    #[test]
    fn a_brief_is_kept_verbatim() {
        let brief = "\n# Goal\n\nbody\n";
        assert_eq!(read_brief(Some(brief.as_bytes()), false).unwrap(), brief);
    }

    #[test]
    fn the_todo_carries_the_title_and_the_provenance_and_never_the_brief() {
        let text = todo_text("# Retry the webhooks\n\nSECRET-BODY", "c1", "alpha");
        assert!(text.starts_with("# Retry the webhooks\n\n"));
        assert!(
            !text.contains("SECRET-BODY"),
            "a LIVE workspace commits todos/**"
        );
        assert!(text.contains("Handed off from chat c1 · workspace alpha"));
    }

    #[test]
    fn every_reason_a_first_message_is_refused_is_reported_cheapest_first() {
        assert_eq!(bad_message("   "), Some(BadMessage::Empty));
        assert_eq!(bad_message("-p hello there"), Some(BadMessage::Flag));
        assert_eq!(
            bad_message("login"),
            Some(BadMessage::OneWord("login".into()))
        );
        assert_eq!(bad_message("two words\0here"), Some(BadMessage::Nul));
        let long = "x ".repeat(FIRST_MESSAGE_MAX_BYTES);
        assert_eq!(bad_message(&long), Some(BadMessage::TooLong(long.len())));
        assert_eq!(bad_message("a real first message"), None);
    }

    #[test]
    fn the_byte_bound_is_the_bound_and_not_one_byte_either_side() {
        let at_bound = format!("a {}", "b".repeat(FIRST_MESSAGE_MAX_BYTES - 2));
        assert_eq!(at_bound.len(), FIRST_MESSAGE_MAX_BYTES);
        assert_eq!(bad_message(&at_bound), None);
        let over = format!("{at_bound}c");
        assert_eq!(bad_message(&over), Some(BadMessage::TooLong(over.len())));
    }

    #[test]
    fn a_multibyte_brief_is_counted_in_bytes_and_not_characters() {
        // 4096 three-byte characters is 12,288 bytes — exactly the bound — so one more
        // character is over it although the message is a third of the bound in `chars`.
        let msg = format!("x {}", "€".repeat(4096));
        assert!(msg.chars().count() < FIRST_MESSAGE_MAX_BYTES);
        assert!(matches!(bad_message(&msg), Some(BadMessage::TooLong(_))));
    }

    #[test]
    fn each_harnesss_first_message_argv_is_the_measured_one() {
        assert_eq!(
            first_message_argv("claude", "hi"),
            Some(vec!["hi".to_string()])
        );
        assert_eq!(
            first_message_argv("codex", "hi"),
            Some(vec!["hi".to_string()])
        );
        assert_eq!(
            first_message_argv("opencode", "hi"),
            Some(vec!["--prompt".to_string(), "hi".to_string()]),
            "opencode's positional is [project], not a prompt"
        );
        assert_eq!(first_message_argv("aider", "hi"), None, "never a guess");
    }

    #[test]
    fn quoting_is_shlexs() {
        assert_eq!(quote(""), "''");
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), "'it'\"'\"'s'");
    }

    // ----- what the chat it opens is told (charter-app#258, #259) --------------------------

    fn wire() -> String {
        first_message(
            &stamp("16", "platform-next", at("2026-09-24T11:32:05")),
            "# Drop account-console-commons\nbody",
        )
    }

    #[test]
    fn the_chat_it_opens_is_told_its_parent_by_name_and_never_by_number() {
        let told = delivered(&wire(), "steward 3", false).expect("a stamped message");

        assert_eq!(
            told,
            "⟨handoff from steward 3 · workspace platform-next · 2026-09-24 11:32⟩\n\n\
             # Drop account-console-commons\nbody"
        );
        assert!(!told.contains("chat 16"), "{told}");
    }

    #[test]
    fn a_handoff_that_wants_an_answer_tells_the_chat_how_to_give_one() {
        let told = delivered(&wire(), "steward 3", true).expect("a stamped message");

        let (stamp_line, rest) = told.split_once('\n').unwrap();
        assert_eq!(
            stamp_line,
            "⟨handoff from steward 3 · workspace platform-next · 2026-09-24 11:32⟩"
        );
        let (ask, brief) = rest.split_once("\n\n").unwrap();
        assert!(
            ask.contains("charter handoff report \"<summary>\""),
            "{ask}"
        );
        assert_eq!(brief, "# Drop account-console-commons\nbody");
    }

    #[test]
    fn a_message_without_the_stamp_is_delivered_as_nothing() {
        assert_eq!(delivered("# Goal\nbody", "steward 3", false), None);
    }

    #[test]
    fn the_stamp_says_where_the_handoff_left_from() {
        let msg = wire();
        let read = stamped(&msg).expect("stamped");

        assert_eq!(read.chat, "16");
        assert_eq!(read.workspace, "platform-next");
        assert_eq!(read.brief, "# Drop account-console-commons\nbody");
    }

    #[test]
    fn a_report_is_trimmed_and_may_run_over_several_lines() {
        assert_eq!(
            report_summary("  Dropped it.\nTwo repos changed.\n"),
            Ok("Dropped it.\nTwo repos changed.".to_owned())
        );
    }

    #[test]
    fn an_empty_report_is_refused() {
        assert_eq!(report_summary(" \n\t "), Err(BadReport::Empty));
    }

    #[test]
    fn a_report_past_the_cap_is_refused_and_one_at_it_is_kept() {
        let at_cap = "a".repeat(MOST_REPORT_BYTES);
        assert_eq!(report_summary(&at_cap), Ok(at_cap.clone()));
        assert_eq!(
            report_summary(&format!("{at_cap}b")),
            Err(BadReport::TooLong(MOST_REPORT_BYTES + 1))
        );
    }

    #[test]
    fn a_report_with_an_invisible_or_control_character_is_refused() {
        for sneaky in [
            "done\u{202e}enod",
            "a\u{200b}b",
            "bell\u{7}",
            "cr\rlf",
            "esc\u{1b}[2J",
        ] {
            assert_eq!(
                report_summary(sneaky),
                Err(BadReport::Undrawable),
                "{sneaky:?} was let through"
            );
        }
    }

    #[test]
    fn every_report_refusal_says_nothing_was_sent() {
        for bad in [
            BadReport::Empty,
            BadReport::TooLong(5000),
            BadReport::Undrawable,
        ] {
            assert!(bad.say().contains("nothing was sent"), "{bad:?}");
        }
    }
}
