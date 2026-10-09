//! A helper sub-agent may not dispatch: the one fact about a dispatch only a tool hook knows
//! (#1434, #1436).
//!
//! Every other question about a dispatch is answered by the app, from its own record of the
//! asking chat ([`crate::dispatchdecision`]). This one cannot be: a harness's sub-agent runs
//! inside its chat's process, with the chat's environment and the chat's token, so a `purlis
//! dispatch` it runs reaches the app looking exactly like the chat's own. What tells them apart
//! is the harness's hook payload, which names the sub-agent that made a tool call (`agent_id`,
//! on the harnesses where that was measured to mean one,
//! [`crate::handoffguard::MEASURED_SUBAGENT_HARNESSES`]). So the refusal stands here, in front
//! of the tool call, and the sentence it says is the decision's own.
//!
//! Two calls are read: a Bash command that runs `purlis dispatch` in any segment
//! ([`refusal`]), and a call of purlis's own `dispatch` chat tools ([`tool_refusal`]).
//!
//! **This is advice a sub-agent meets, not a boundary it cannot cross.** It is a recognition
//! of the usual shapes and never a shell, as the handoff guard says of itself: a dispatch
//! behind a variable of the sub-agent's own, in a script file or inside an interpreter is not
//! seen, and on a harness where `agent_id` was never measured (opencode) nothing is refused at
//! all. A sub-agent that gets past it reaches exactly what its chat reaches, and no more: the
//! chat its own chat could have started, listed under that chat where the person sees it. So
//! nothing that grants a chat a dispatch should be described as never reaching its helpers.

use crate::dispatchdecision::{self, Asker, Decision, Mode, Persona, Request};
use crate::handoffguard::Caller;
use crate::proseguard::charter_words;
use crate::{shellseg, shellwrap};

/// The trace reason a refused dispatch is tallied under.
pub const REASON_SUBAGENT: &str = "dispatch-subagent";

/// The word after the program that makes a command a dispatch.
const WORD: &str = "dispatch";

/// Whether `cmd` runs `purlis dispatch`, in any spelling purlis recognises of its own command
/// line: a report back included, which is the chat's own to send.
///
/// Three places are read, each with a reader the other guards already use:
///
/// - **every segment** ([`shellseg::segment_argv`]). Newlines are segment boundaries, so a
///   line of a heredoc body that itself begins `purlis dispatch` counts too;
/// - **one level into a string a shell runs** ([`shellwrap::shell_scripts`]): `bash -c '…'`,
///   `sh -c "…"`, `eval …`, as the handoff guard looks into them;
/// - **the variable that names purlis's own binary** as the program
///   ([`crate::plugin::BINARY_ENV`], under either name), which every chat the app starts has in
///   its environment.
///
/// Each over-match refuses only a sub-agent, which is the safe direction.
pub fn runs_dispatch(cmd: &str) -> bool {
    a_segment_runs_it(cmd) || a_shell_string_runs_it(cmd)
}

fn a_segment_runs_it(cmd: &str) -> bool {
    shellseg::segment_argv(cmd).iter().any(|toks| {
        let (prog, _env, argv) = shellwrap::split_env(toks);
        charter_words(&prog, &argv).is_some_and(|words| words.first().is_some_and(|w| w == WORD))
            || (names_our_binary(&prog) && argv.get(1).is_some_and(|word| word == WORD))
    })
}

/// Whether a program word is the variable the app names purlis's binary in: `$NAME` or
/// `${NAME}`, as the source spells it once its quotes are off.
fn names_our_binary(prog: &str) -> bool {
    let Some(name) = prog.strip_prefix('$') else {
        return false;
    };
    let name = name
        .strip_prefix('{')
        .and_then(|name| name.strip_suffix('}'))
        .unwrap_or(name);
    crate::envvar::spellings(crate::plugin::BINARY_ENV)
        .iter()
        .any(|spelling| spelling == name)
}

/// One level in, and no deeper: a string inside that string is not opened.
fn a_shell_string_runs_it(cmd: &str) -> bool {
    let Ok(toks) = shellseg::lex(&crate::heredoc::strip_reader_heredocs(cmd)) else {
        return false;
    };
    let toks = shellseg::split_punctuation(toks);
    crate::heredoc::segments_of(&toks)
        .iter()
        .any(|(seg, _before)| {
            let words: Vec<String> = seg.iter().map(|tok| tok.text.clone()).collect();
            shellwrap::shell_scripts(&words)
                .iter()
                .any(|inner| a_segment_runs_it(&crate::handoffguard::as_the_shell_reads(inner)))
        })
}

/// `(trace reason, denial)` for a Bash command that dispatches from inside a sub-agent, or
/// `None`.
pub fn refusal(cmd: &str, caller: Caller<'_>) -> Option<(&'static str, String)> {
    (caller.from_a_subagent() && runs_dispatch(cmd)).then(|| (REASON_SUBAGENT, helper_is_told()))
}

/// purlis's chat tools that dispatch, by the names [`crate::chattools`] gives them.
const OUR_TOOLS: [&str; 3] = [
    crate::chattools::DISPATCH,
    crate::chattools::DISPATCH_LIST,
    crate::chattools::DISPATCH_REPORT,
];

/// A chat tool of purlis's, as a harness names it.
#[cfg(test)]
fn tool_id(tool: &str) -> String {
    format!("{}{tool}", crate::names::MCP_TOOL_PREFIX.write)
}

/// The denial for a call of the chat tool `tool` from inside a sub-agent, or `None`: purlis's
/// `dispatch` and `dispatch_report`, under the name the app registers the server by. **The
/// same tools the hook's matcher names** ([`crate::hookreg`]), and a test holds the two lists
/// to each other: a tool this knew and the matcher did not would be a refusal nothing runs.
pub fn tool_refusal(tool: &str, caller: Caller<'_>) -> Option<String> {
    let name = tool.strip_prefix(crate::names::MCP_TOOL_PREFIX.write)?;
    (OUR_TOOLS.contains(&name) && caller.from_a_subagent()).then(helper_is_told)
}

/// What the decision answers a helper sub-agent, in its own sentence.
fn helper_is_told() -> String {
    // Who asks is the first thing the decision reads, so the rest is whatever a project that
    // set nothing would have.
    let limits = crate::dispatchlimits::in_force(
        &crate::dispatchlimits::Table::default(),
        None,
        None,
        None,
        &crate::dispatchlimits::Table::default(),
        &crate::dispatchlimits::Level::unset(),
    );
    let asked = Request {
        asker: Asker::Helper,
        to: Persona::None,
        mode: Mode::Task,
        grant: &crate::dispatchgrant::Covers::NeedsGrant,
        profile: None,
        limits: &limits,
        lineage: &dispatchdecision::Lineage::default(),
    };
    match dispatchdecision::decide(&asked) {
        Decision::Refused(why) => why.say(),
        // The decision refuses a helper before it asks anything else. Were that ever not so,
        // this guard still refuses, in the same words.
        Decision::Start | Decision::NeedsGrant { .. } => dispatchdecision::HELPER.to_owned(),
    }
}

// ----------------------------------------------------------------------------------------
// the text a dispatch subcommand sends, written as the shell will not rewrite it (#1456, #1463)
// ----------------------------------------------------------------------------------------
//
// A report, a follow-up, an answer, a note and a question each send their text as the command's
// own argument, quoted to the chat that reads it. A live substitution in the call is replaced by
// the shell before purlis reads a word, so what is sent is not what was written here, and the
// command it runs is the shell's. The handoff's report is refused for it (A7), and so are these.

/// The trace reason a dispatch's text with a live substitution in its call is refused under.
pub const REASON_TEXT_SOURCE: &str = "dispatch-text-source";

/// The subcommands of `purlis dispatch` that send text as it is written, each with the shape
/// its refusal tells the chat to write instead.
const SENDS_TEXT: [(&str, &str); 5] = [
    ("report", "report --outcome done \"<summary>\""),
    ("tell", "tell <chat> \"<text>\""),
    ("answer", "answer <chat> \"<answer>\""),
    ("note", "note \"<note>\""),
    ("ask", "ask \"<question>\""),
];

/// The text-sending subcommand a segment's words run, as its row of [`SENDS_TEXT`], or `None`.
fn sends_text(toks: &[String]) -> Option<(&'static str, &'static str)> {
    let (prog, _env, argv) = shellwrap::split_env(toks);
    let words = match charter_words(&prog, &argv) {
        Some(words) => words,
        None if names_our_binary(&prog) => argv.get(1..).unwrap_or_default().to_vec(),
        None => return None,
    };
    if words.first().is_none_or(|word| word != WORD) {
        return None;
    }
    let sub = words.get(1)?;
    SENDS_TEXT.into_iter().find(|(word, _)| word == sub)
}

/// A text-sending subcommand a call runs: its row of [`SENDS_TEXT`], and the string a shell
/// runs it in, where it is one level into one.
type TextSubcommand = (&'static str, &'static str, Option<String>);

/// The text-sending subcommand `cmd` runs in any segment, or one level into a string a shell
/// runs ([`shellwrap::shell_scripts`]), as [`runs_dispatch`] reads them.
fn a_text_subcommand(cmd: &str) -> Option<TextSubcommand> {
    let in_a_segment = |text: &str| {
        shellseg::segment_argv(text)
            .iter()
            .find_map(|toks| sends_text(toks))
    };
    if let Some((sub, shape)) = in_a_segment(cmd) {
        return Some((sub, shape, None));
    }
    let toks = shellseg::lex(&crate::heredoc::strip_reader_heredocs(cmd)).ok()?;
    let toks = shellseg::split_punctuation(toks);
    crate::heredoc::segments_of(&toks)
        .iter()
        .find_map(|(seg, _before)| {
            let words: Vec<String> = seg.iter().map(|tok| tok.text.clone()).collect();
            shellwrap::shell_scripts(&words).iter().find_map(|inner| {
                let read = crate::handoffguard::as_the_shell_reads(inner);
                in_a_segment(&read).map(|(sub, shape)| (sub, shape, Some(read)))
            })
        })
}

/// `(trace reason, denial)` for a call that runs `purlis dispatch report`, `tell`, `answer`,
/// `note` or `ask` and holds a live substitution anywhere, or `None`.
///
/// **Scoped to the whole call**, as A5, A6 and the handoff's report are, for their reason:
/// which argument a substitution lands in is more parser in the direction that fails open.
/// **On every harness and for the chat itself**, not only a helper: it is a fact about the
/// shell, and the text is the chat's own to write out. A quoted heredoc's body is data and is
/// not a substitution ([`crate::livesub::live_substitution`]).
pub fn text_refusal(cmd: &str) -> Option<(&'static str, String)> {
    if !crate::livesub::may_substitute(cmd) {
        return None;
    }
    let ((sub, shape), hit) = in_the_call(cmd).or_else(|| in_a_body_a_shell_runs(cmd))?;
    let (kind, effect) = if crate::livesub::is_process_substitution(hit) {
        ("process", "run and replace with a path")
    } else {
        ("command", "replace")
    };
    // A6's remedy, measured there: a report carries markdown code spans, and most carry an
    // apostrophe, so single quotes alone fail on exactly the text this refuses.
    let tool = if sub == "report" {
        format!(
            " Or send it with the `{}` tool, whose text no shell reads.",
            crate::chattools::DISPATCH_REPORT
        )
    } else {
        String::new()
    };
    Some((
        REASON_TEXT_SOURCE,
        format!(
            "`purlis dispatch {sub}` sends its text as it is written here, and this call has a \
             live {kind} substitution in it, which the shell would {effect} before purlis \
             reads it. Write the text out in plain words: purlis dispatch {shape}. Keep a \
             backtick literal by backslash-escaping each one (`\\`code\\``), which leaves \
             apostrophes working, or single-quote the whole text when it holds none.{tool}"
        ),
    ))
}

/// A text subcommand found in the call's segments, and a live substitution in the call as
/// written or in the string a shell runs it in: a substitution single-quoted on the outside is
/// live when that string is run.
fn in_the_call(cmd: &str) -> Option<((&'static str, &'static str), &'static str)> {
    let (sub, shape, inner) = a_text_subcommand(cmd)?;
    let hit = crate::livesub::live_substitution(cmd)
        .or_else(|| inner.as_deref().and_then(crate::livesub::live_substitution))?;
    Some(((sub, shape), hit))
}

/// A text subcommand in a heredoc body a shell runs (`bash <<'EOF'`), and a live substitution
/// in that body **as the shell reads it**: quoting the delimiter keeps the body from expanding
/// on its way in, and the shell then runs every line of it. Which bodies a shell runs is the
/// plan A7 and the leak guard share ([`crate::leakguard::lines_a_command_could_run`]).
fn in_a_body_a_shell_runs(cmd: &str) -> Option<((&'static str, &'static str), &'static str)> {
    let run: Vec<String> = crate::leakguard::lines_a_command_could_run(cmd)
        .into_iter()
        .filter(|(_, in_a_run_body)| *in_a_run_body)
        .map(|(row, _)| row)
        .collect();
    if run.is_empty() {
        return None;
    }
    let body = run.join("\n");
    let ((sub, shape), hit) = in_the_call(&body)?;
    Some(((sub, shape), hit))
}

// ----------------------------------------------------------------------------------------
// a rider beside a pre-allowed dispatch or handoff (D-1444-14)
// ----------------------------------------------------------------------------------------
//
// A Claude Code chat the app starts is handed an `allow` for `purlis dispatch …` and
// `purlis handoff …`, by each spelling ([`crate::harness`]), because what consents to one is
// the dispatch grant. A harness matches such a rule against the command as written, and
// whether it then asks about a second command in the same call has not been measured. So
// purlis does not lean on it: where a call runs a dispatch or a handoff **and anything else**,
// the hook answers `ask` for the whole call, and names the other command.

/// What a rider is called where it is a substitution: the shell runs a command there, and
/// which one is not this guard's to quote.
pub const A_SUBSTITUTION: &str = "a command substitution";

/// What a rider is called where purlis cannot read the call: a quote or an escape left open,
/// so where the dispatch ends cannot be told.
pub const UNREADABLE: &str = "one purlis could not read";

/// The trace key an `ask` for a rider is tallied under.
pub const REASON_RIDER: &str = "dispatch-rider";

/// `$CHARTER_HARNESS` in a Claude Code chat: the one harness handed the allow.
const CLAUDE_CODE: &str = "claude-code";

/// The two commands a Claude Code chat is handed an allow for.
const HANDOFF: &str = "handoff";

/// Which of the two a segment's words run, if either: `dispatch` or `handoff`.
fn ours(toks: &[String]) -> Option<&'static str> {
    let (prog, _env, argv) = shellwrap::split_env(toks);
    let first = match charter_words(&prog, &argv) {
        Some(words) => words.first().cloned(),
        None => names_our_binary(&prog)
            .then(|| argv.get(1).cloned())
            .flatten(),
    }?;
    [WORD, HANDOFF].into_iter().find(|word| *word == first)
}

/// `(which of the two the call runs, the other command beside it)`, or `None` for a call that
/// runs neither, or runs one alone.
///
/// Read with the readers the other guards share, and none of its own:
///
/// - **the lines a shell would run** ([`crate::leakguard::lines_a_command_could_run`]): a
///   brief, and any other heredoc body a reader takes, is data and is not a command here;
/// - **their segments** ([`shellseg::segment_argv`]): every command a `;`, `&&`, `||`, `|`, `&`
///   or a newline separates. The first that runs a dispatch or a handoff is the call's own, and
///   **any other segment is a rider**, a second dispatch or handoff among them: only the first
///   is judged by the handoff guard;
/// - **a live substitution anywhere in the call** ([`crate::livesub::live_substitution`]):
///   the shell runs a command there;
/// - **a call the lexer cannot read** that names either command is a rider too: where it
///   cannot tell, it asks.
///
/// **What it does not see** is what the shell reader does not: a redirection on the
/// dispatch's own line is not a second command, and a dispatch inside a string a shell runs is
/// not at the start of the call, so no rule of the harness allows it.
fn beside(cmd: &str) -> Option<(&'static str, String)> {
    let runs: Vec<String> = crate::leakguard::lines_a_command_could_run(cmd)
        .into_iter()
        .map(|(row, _)| row)
        .collect();
    let text = runs.join("\n");
    if shellseg::lex(&text).is_err() {
        let lower = text.to_lowercase();
        return [WORD, HANDOFF]
            .into_iter()
            .find(|word| {
                crate::cliname::INSTALLED
                    .iter()
                    .any(|name| lower.contains(&format!("{name} {word}")))
            })
            .map(|word| (word, UNREADABLE.to_owned()));
    }
    let segments = shellseg::segment_argv(&text);
    let (at, own) = segments
        .iter()
        .enumerate()
        .find_map(|(at, toks)| ours(toks).map(|own| (at, own)))?;
    if let Some((_, other)) = segments
        .iter()
        .enumerate()
        .find(|(i, toks)| *i != at && !toks.is_empty())
    {
        return Some((own, crate::shown::short(&other.join(" "))));
    }
    crate::livesub::live_substitution(cmd).map(|_| (own, A_SUBSTITUTION.to_owned()))
}

/// The other command a tool call runs beside a dispatch or a handoff, as a person would
/// recognise it, or `None` ([`beside`]).
pub fn rider(cmd: &str) -> Option<String> {
    beside(cmd).map(|(_, other)| other)
}

/// **What the person is asked where a dispatch or a handoff shares its call** (D-1444-14), or
/// `None`: the sentence the hook answers `ask` with, naming the other command.
///
/// Only for a Claude Code chat, the one harness handed the allow this stands behind. The
/// answer is an `ask`, never a refusal: the dispatch is still the app's to decide, and the
/// other command is the person's.
pub fn rider_ask(cmd: &str, caller: Caller<'_>) -> Option<String> {
    if caller.harness != Some(CLAUDE_CODE) {
        return None;
    }
    let (own, other) = beside(cmd)?;
    Some(format!(
        "this call runs `purlis {own}` beside another command (`{other}`). purlis lets a \
         dispatch or a handoff run without your harness asking, and that is for the {own} \
         alone, so the person is asked about this call as a whole. Run the {own} in a call of \
         its own."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_sub_agent() -> Caller<'static> {
        Caller {
            agent_id: Some("sub-1"),
            harness: Some("claude-code"),
            permission_mode: None,
        }
    }

    fn the_chat() -> Caller<'static> {
        Caller {
            agent_id: None,
            harness: Some("claude-code"),
            permission_mode: None,
        }
    }

    #[test]
    fn every_tool_this_refuses_is_one_the_hook_s_matcher_names_and_the_other_way_round() {
        // Two lists that have to agree: the tools `tool_refusal` knows, and the tools the
        // harness runs `pretooluse-dispatch` in front of.
        let matcher = crate::hookreg::find("pretooluse-dispatch")
            .and_then(|hook| hook.matcher)
            .expect("a matcher");
        let named: Vec<&str> = matcher
            .split('|')
            .filter(|tool| tool.starts_with("mcp__"))
            .collect();
        assert_eq!(named, OUR_TOOLS.map(tool_id));
        for tool in named {
            assert!(tool_refusal(tool, a_sub_agent()).is_some(), "{tool}");
        }
    }

    const DISPATCH: &str = "purlis dispatch --name \"check the queue\" <<'BRIEF'\nlook\nBRIEF";

    #[test]
    fn a_sub_agent_that_dispatches_is_refused_and_told_to_return_to_its_chat() {
        let (reason, said) = refusal(DISPATCH, a_sub_agent()).expect("refused");
        assert_eq!(reason, REASON_SUBAGENT);
        assert_eq!(
            said,
            "a dispatch is refused from inside a helper. A task belongs to a chat the person \
             can see, open and stop, and a helper is not one. Return what you \
             found to your chat, and let that chat dispatch."
        );
    }

    #[test]
    fn the_chat_itself_dispatches_past_this_guard() {
        assert_eq!(refusal(DISPATCH, the_chat()), None);
        // An empty `agent_id` is the main conversation on a harness that always sends one.
        let empty = Caller {
            agent_id: Some(""),
            ..the_chat()
        };
        assert_eq!(refusal(DISPATCH, empty), None);
    }

    #[test]
    fn a_harness_whose_agent_id_was_never_measured_is_not_read_as_a_sub_agent() {
        let unmeasured = Caller {
            harness: Some("opencode"),
            ..a_sub_agent()
        };
        assert_eq!(refusal(DISPATCH, unmeasured), None);
    }

    #[test]
    fn a_sub_agent_is_refused_in_every_spelling_of_the_command_line_and_a_report_too() {
        for cmd in [
            "purlis dispatch --to devops --name x <<'B'\nb\nB",
            "charter dispatch --name x <<'B'\nb\nB",
            "cd svc && purlis dispatch --name x <<'B'\nb\nB",
            "FOO=1 purlis dispatch --name x",
            "purlis dispatch report --outcome done \"did it\"",
            "ls\npurlis dispatch --name x",
        ] {
            assert!(refusal(cmd, a_sub_agent()).is_some(), "{cmd:?}");
        }
    }

    #[test]
    fn a_dispatch_one_level_inside_a_string_a_shell_runs_is_seen() {
        // What the readers the other guards share already parse: a `-c` string, an `eval`.
        for cmd in [
            "bash -c 'purlis dispatch --name x'",
            "sh -c \"purlis dispatch --name x\"",
            "eval 'purlis dispatch --name x'",
            "eval purlis dispatch --name x",
            "cd svc && bash -c 'cd x; purlis dispatch report --outcome done ok'",
        ] {
            assert!(refusal(cmd, a_sub_agent()).is_some(), "{cmd:?}");
            assert_eq!(refusal(cmd, the_chat()), None, "{cmd:?}");
        }
    }

    #[test]
    fn a_dispatch_run_by_the_variable_that_names_purlis_s_own_binary_is_seen() {
        // Every chat the app starts has it in its environment, under either name.
        for cmd in [
            "$PURLIS_HOOK_BINARY dispatch --name x",
            "\"$PURLIS_HOOK_BINARY\" dispatch --name x",
            "${PURLIS_HOOK_BINARY} dispatch --name x",
            "\"${CHARTER_HOOK_BINARY}\" dispatch report --outcome done ok",
            "bash -c '$CHARTER_HOOK_BINARY dispatch --name x'",
        ] {
            assert!(refusal(cmd, a_sub_agent()).is_some(), "{cmd:?}");
        }
        for cmd in [
            "$PURLIS_HOOK_BINARY hook stop",
            "$PURLIS_HOOK_BINARY workspace todo dispatch",
            "$OTHER dispatch --name x",
            "bash -c 'echo dispatch'",
            "eval 'git log --grep dispatch'",
        ] {
            assert_eq!(refusal(cmd, a_sub_agent()), None, "{cmd:?}");
        }
    }

    #[test]
    fn a_sub_agent_running_anything_else_is_left_alone() {
        for cmd in [
            "purlis workspace todo \"dispatch the fix\"",
            "echo purlis dispatch",
            "git log --grep dispatch",
            "purlis persona stats",
            "dispatch --name x",
        ] {
            assert_eq!(refusal(cmd, a_sub_agent()), None, "{cmd:?}");
        }
    }

    #[test]
    fn the_pre_allowed_list_of_tasks_is_still_refused_to_a_helper_and_starts_nothing() {
        // #1463: `dispatch_list` runs without the harness asking, as `purlis dispatch list`
        // does. The pre-allow is the harness's prompt and no more: a helper's call is still
        // refused in front of it, and the tools that start a chat or send a report are not
        // among the reads.
        let list = format!("mcp__purlis__{}", crate::chattools::DISPATCH_LIST);
        assert!(crate::chattools::PRE_ALLOWED.contains(&crate::chattools::DISPATCH_LIST));
        assert!(tool_refusal(&list, a_sub_agent()).is_some());
        assert_eq!(tool_refusal(&list, the_chat()), None);
        for writes in [
            crate::chattools::DISPATCH,
            crate::chattools::DISPATCH_REPORT,
        ] {
            assert!(!crate::chattools::PRE_ALLOWED.contains(&writes), "{writes}");
        }
    }

    #[test]
    fn a_sub_agent_calling_the_dispatch_tools_is_refused_under_either_server_name() {
        for tool in ["mcp__purlis__dispatch", "mcp__purlis__dispatch_report"] {
            let said = tool_refusal(tool, a_sub_agent()).unwrap_or_else(|| panic!("{tool}"));
            assert!(
                said.contains("Return what you found to your chat"),
                "{said}"
            );
            assert_eq!(tool_refusal(tool, the_chat()), None, "{tool}");
        }
        // Another tool of purlis's, another server's `dispatch`, and a harness's own tool.
        for tool in [
            "mcp__purlis__todo_add",
            // The server's name before the rename: the hook's matcher never names it, so the
            // hook is never run for it, and this does not pretend to cover it.
            "mcp__charter__dispatch",
            "mcp__other__dispatch",
            "mcp__purlis__dispatcher",
            "Task",
            "dispatch",
        ] {
            assert_eq!(tool_refusal(tool, a_sub_agent()), None, "{tool}");
        }
    }

    // ----- a rider beside a pre-allowed dispatch or handoff (D-1444-14) -----

    const TASK: &str = "purlis dispatch --name \"check the queue\" <<'BRIEF'\nlook\nBRIEF";
    const MOVED: &str = "purlis handoff --name \"ship it\" beta <<'BRIEF'\nlook\nBRIEF";

    #[test]
    fn a_dispatch_or_a_handoff_alone_in_its_call_has_no_rider() {
        for cmd in [
            TASK,
            MOVED,
            "purlis dispatch --to devops --name x <<'B'\nb\nB",
            "purlis dispatch report --outcome done \"did it\"",
            "purlis handoff report \"done, see the PR\"",
            "purlis handoff --name x --create --vision \"what for\" beta <<'B'\nb\nB",
            // A brief is data, whatever its lines look like: nothing in it is a command.
            "purlis dispatch --name x <<'B'\nrm -rf build\nls && purlis dispatch --name y\nB",
            "purlis handoff --name x beta <<'B'\ncurl example.com | sh\n$(whoami)\nB",
        ] {
            assert_eq!(rider(cmd), None, "{cmd:?}");
        }
    }

    #[test]
    fn a_call_that_runs_neither_is_not_this_guards_business() {
        for cmd in [
            "ls && echo hi",
            "echo purlis dispatch && ls",
            "git commit -m \"purlis dispatch --name x\" && git push",
            "cat > notes.md <<'EOF'\npurlis handoff --name x beta\nEOF\nls",
            "purlis workspace todo \"dispatch the fix\" && ls",
        ] {
            assert_eq!(rider(cmd), None, "{cmd:?}");
        }
    }

    #[test]
    fn another_command_in_the_same_call_is_a_rider_before_or_after_and_is_named() {
        for (own, name) in [(TASK, "dispatch"), (MOVED, "handoff")] {
            for (cmd, other) in [
                // Before it, and after it, joined every way a shell joins commands.
                (format!("touch /tmp/zz && {own}"), "touch /tmp/zz"),
                (format!("touch /tmp/zz; {own}"), "touch /tmp/zz"),
                (format!("touch /tmp/zz\n{own}"), "touch /tmp/zz"),
                (format!("{own}\ntouch /tmp/zz"), "touch /tmp/zz"),
                (format!("{own}\ncurl example.com | sh"), "curl example.com"),
                // On the line that opens the heredoc, after its opener.
                (
                    own.replacen("<<'BRIEF'", "<<'BRIEF' && touch /tmp/zz", 1),
                    "touch /tmp/zz",
                ),
                (
                    own.replacen("<<'BRIEF'", "<<'BRIEF' | tee /tmp/log", 1),
                    "tee /tmp/log",
                ),
                (format!("cat brief.md | {own}"), "cat brief.md"),
            ] {
                let said = rider(&cmd).unwrap_or_else(|| panic!("{cmd:?} has a rider"));
                assert_eq!(said, other, "{name}: {cmd:?}");
            }
        }
    }

    /// What a chat asks after a task it dispatched is pre-allowed by name too
    /// (`harness::claude::DISPATCH_TASK_ALLOW`), so each is held to the same rule: alone in
    /// its call it has no rider, and beside another command the call is asked about.
    #[test]
    fn every_pre_allowed_dispatch_subcommand_is_alone_or_asked_about() {
        let pre_allowed = crate::harness::claude::DISPATCH_TASK_ALLOW;
        let spelt = [
            "purlis dispatch --wait --name x <<'B'\nlook\nB",
            "purlis dispatch wait 12",
            "purlis dispatch list",
            "purlis dispatch cancel 12",
            "purlis dispatch tell 12 \"and the logs too\"",
            "purlis dispatch note \"half way\"",
            "purlis dispatch ask \"which cluster?\"",
            "purlis dispatch answer 12 \"the staging one\"",
        ];
        // One spelling here for each rule the chat is handed, so a rule added there is met here.
        assert_eq!(spelt.len(), pre_allowed.len());
        for (cmd, rule) in spelt.iter().zip(pre_allowed) {
            let glob = rule
                .strip_prefix("Bash(")
                .and_then(|rule| rule.strip_suffix(')'))
                .expect("a Bash rule");
            assert!(
                crate::pypath::fnmatch(cmd, glob),
                "{cmd:?} is not the spelling {rule} allows"
            );
            assert_eq!(rider(cmd), None, "{cmd:?}");
            for with in [
                format!("{cmd}\ntouch /tmp/zz"),
                format!("touch /tmp/zz && {cmd}"),
            ] {
                assert_eq!(rider(&with).as_deref(), Some("touch /tmp/zz"), "{with:?}");
                assert!(rider_ask(&with, the_chat()).is_some(), "{with:?}");
            }
        }
    }

    /// A dispatch may say where its chat works (`--in`, #1453). The word is one more argument
    /// of a line a rule already allows, so the rule for a shared call sees it wherever it
    /// stands (D-T61-3); and no rule allows a line that opens with it, so that spelling is the
    /// harness's own to ask about, alone or not.
    #[test]
    fn a_dispatch_that_says_where_it_works_is_alone_or_asked_about_too() {
        use crate::harness::claude::{DISPATCH_ALLOW, DISPATCH_TASK_ALLOW, HANDOFF_ALLOW};
        let allowed = |cmd: &str| {
            DISPATCH_ALLOW
                .iter()
                .chain(DISPATCH_TASK_ALLOW.iter())
                .chain(HANDOFF_ALLOW.iter())
                .any(|rule| {
                    let glob = rule
                        .strip_prefix("Bash(")
                        .and_then(|rule| rule.strip_suffix(')'))
                        .expect("a Bash rule");
                    crate::pypath::fnmatch(cmd, glob)
                })
        };
        for (cmd, pre_allowed) in [
            (
                "purlis dispatch --name x --in worktree <<'B'\nlook\nB",
                true,
            ),
            (
                "purlis dispatch --name x --in workspace:beta <<'B'\nlook\nB",
                true,
            ),
            (
                "purlis dispatch --to devops --in worktree --name x <<'B'\nlook\nB",
                true,
            ),
            (
                "purlis dispatch --wait --in workspace:beta --name x <<'B'\nlook\nB",
                true,
            ),
            // No rule opens with `--in`: these the harness asks about itself.
            (
                "purlis dispatch --in worktree --name x <<'B'\nlook\nB",
                false,
            ),
            (
                "purlis dispatch --in workspace:beta --name x <<'B'\nlook\nB",
                false,
            ),
            (
                "purlis dispatch --in=worktree --name x <<'B'\nlook\nB",
                false,
            ),
        ] {
            assert_eq!(allowed(cmd), pre_allowed, "{cmd:?}");
            assert_eq!(rider(cmd), None, "{cmd:?}");
            for with in [
                format!("{cmd}\ntouch /tmp/zz"),
                format!("touch /tmp/zz && {cmd}"),
                cmd.replacen("<<'B'", "<<'B' && touch /tmp/zz", 1),
            ] {
                assert_eq!(rider(&with).as_deref(), Some("touch /tmp/zz"), "{with:?}");
                assert!(rider_ask(&with, the_chat()).is_some(), "{with:?}");
            }
        }
    }

    #[test]
    fn a_second_dispatch_or_handoff_in_the_call_is_a_rider_too() {
        // Only the first handoff in a call is judged by the handoff guard, so a second one is
        // another command like any other.
        for cmd in [format!("{TASK}\n{MOVED}"), format!("{MOVED}\n{TASK}")] {
            assert!(rider(&cmd).is_some(), "{cmd:?}");
        }
    }

    #[test]
    fn a_substitution_beside_it_is_a_rider_and_so_is_a_call_purlis_cannot_read() {
        for cmd in [
            "purlis dispatch --name \"$(whoami)\" <<'B'\nb\nB",
            "purlis dispatch report --outcome done \"$(cat notes.md)\"",
        ] {
            assert_eq!(rider(cmd).as_deref(), Some(A_SUBSTITUTION), "{cmd:?}");
        }
        // One the shell reader unpicks is named as the command it is.
        assert_eq!(
            rider("purlis dispatch --name x --to `cat who` <<'B'\nb\nB").as_deref(),
            Some("cat who")
        );
        assert_eq!(
            rider("purlis dispatch --name x <(cat brief.md)").as_deref(),
            Some("cat brief.md")
        );
        // A quote left open: where the command ends cannot be read, so it is asked about.
        for cmd in [
            "purlis dispatch --name 'x",
            "purlis handoff --name \"x beta",
        ] {
            assert_eq!(rider(cmd).as_deref(), Some(UNREADABLE), "{cmd:?}");
        }
    }

    #[test]
    fn a_live_substitution_in_the_text_a_dispatch_subcommand_sends_is_refused() {
        // #1456, #1463: as the handoff's report is. Whoever asks, on every harness.
        for (cmd, sub) in [
            (
                "purlis dispatch report --outcome done \"$(cat notes.md)\"",
                "report",
            ),
            (
                "purlis dispatch report --outcome done \"`git log -1`\"",
                "report",
            ),
            ("purlis dispatch tell 9 \"use $(whoami)\"", "tell"),
            ("purlis dispatch answer 9 \"$(env)\"", "answer"),
            ("purlis dispatch note \"at $(date)\"", "note"),
            ("purlis dispatch ask \"which one? $(ls)\"", "ask"),
            ("charter dispatch note \"$(id)\"", "note"),
            ("cd svc && purlis dispatch tell 3 \"$(cat x)\"", "tell"),
            ("$PURLIS_HOOK_BINARY dispatch note \"$(id)\"", "note"),
            ("bash -c 'purlis dispatch note \"$(id)\"'", "note"),
            (
                "purlis dispatch report --outcome done \"$(cat <<'EOF'\nfixed it\nEOF\n)\"",
                "report",
            ),
            // A body a shell runs: its quoted delimiter keeps it from expanding on the way
            // in, and the shell then runs the line, substitution and all.
            ("bash <<'EOF'\npurlis dispatch note \"$(id)\"\nEOF", "note"),
            (
                "sh <<'EOF'\ncd svc\npurlis dispatch tell 9 \"`id`\"\nEOF",
                "tell",
            ),
        ] {
            let (reason, said) = text_refusal(cmd).unwrap_or_else(|| panic!("{cmd:?}"));
            assert_eq!(reason, REASON_TEXT_SOURCE);
            assert!(
                said.starts_with(&format!("`purlis dispatch {sub}` sends its text as it is")),
                "{said}"
            );
        }
        assert_eq!(
            text_refusal("purlis dispatch note \"$(id)\"").map(|(_, said)| said),
            Some(
                "`purlis dispatch note` sends its text as it is written here, and this call \
                 has a live command substitution in it, which the shell would replace before \
                 purlis reads it. Write the text out in plain words: purlis dispatch note \
                 \"<note>\". Keep a backtick literal by backslash-escaping each one \
                 (`\\`code\\``), which leaves apostrophes working, or single-quote the whole \
                 text when it holds none."
                    .to_owned()
            )
        );
        // A report can also go by the tool, which no shell reads.
        let (_, said) =
            text_refusal("purlis dispatch report --outcome done \"$(id)\"").expect("refused");
        assert!(
            said.ends_with(
                "Or send it with the `dispatch_report` tool, whose text no shell reads."
            ),
            "{said}"
        );
        let (_, said) = text_refusal("purlis dispatch tell 9 \"$(cat <(id))\"").expect("refused");
        assert!(said.contains("live command substitution"), "{said}");
        let (_, said) = text_refusal("purlis dispatch tell 9 <(id)").expect("refused");
        assert!(
            said.contains(
                "live process substitution in it, which the shell would run and \
                           replace with a path"
            ),
            "{said}"
        );
    }

    #[test]
    fn text_written_out_and_everything_but_a_text_subcommand_pass_this_refusal() {
        for cmd in [
            // The text as written: single quotes, an escaped dollar, a parameter.
            "purlis dispatch report --outcome done 'cost $(nothing)'",
            "purlis dispatch note \"a \\$(literal) one\"",
            "purlis dispatch tell 9 \"$NAME is set\"",
            "purlis dispatch answer 9 \"the blue one\"",
            // A body a reader takes is data, whatever it mentions.
            "cat > notes.md <<'EOF'\npurlis dispatch note \"$(id)\"\nEOF",
            // A dispatch's brief is its heredoc, judged as a brief, not here.
            "purlis dispatch --name x <<'B'\nrun $(this) later\nB",
            // Subcommands that send no text.
            "purlis dispatch wait 9 && echo \"$(date)\"",
            "purlis dispatch list; echo $(date)",
            "purlis dispatch cancel 9 \"$(id)\"",
            // Not purlis's dispatch, or not a dispatch at all.
            "echo purlis dispatch note \"$(id)\"",
            "purlis workspace note \"dispatch $(id)\"",
            "git commit -m \"$(cat msg)\"",
        ] {
            assert_eq!(text_refusal(cmd), None, "{cmd:?}");
        }
    }

    #[test]
    fn the_ask_names_the_other_command_and_is_only_claude_code_s() {
        let cmd = format!("{TASK}\ntouch /tmp/zz");
        let said = rider_ask(&cmd, the_chat()).expect("asked about");
        assert_eq!(
            said,
            "this call runs `purlis dispatch` beside another command (`touch /tmp/zz`). purlis \
             lets a dispatch or a handoff run without your harness asking, and that is for the \
             dispatch alone, so the person is asked about this call as a whole. Run the \
             dispatch in a call of its own."
        );
        let said = rider_ask(&format!("ls -la && {MOVED}"), the_chat()).expect("asked about");
        assert!(
            said.starts_with("this call runs `purlis handoff` beside another command (`ls -la`)."),
            "{said}"
        );
        // The allow this stands behind is one only a Claude Code chat is handed.
        for harness in [Some("codex"), Some("opencode"), None] {
            let other = Caller {
                harness,
                ..the_chat()
            };
            assert_eq!(rider_ask(&cmd, other), None, "{harness:?}");
        }
        assert_eq!(rider_ask(TASK, the_chat()), None);
        assert_eq!(rider_ask(MOVED, the_chat()), None);
    }
}
