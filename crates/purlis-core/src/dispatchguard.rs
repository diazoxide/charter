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

use crate::dispatchdecision::{self, Asker, Decision, Grant, Limits, Mode, Persona, Request};
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
const OUR_TOOLS: [&str; 2] = [
    crate::chattools::DISPATCH,
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
    let asked = Request {
        asker: Asker::Helper,
        to: Persona::None,
        mode: Mode::Task,
        grant: Grant::Missing,
        limits: Limits::default(),
    };
    match dispatchdecision::decide(&asked) {
        Decision::Refused(why) => why.say(),
        // The decision refuses a helper before it asks anything else. Were that ever not so,
        // this guard still refuses, in the same words.
        Decision::Start | Decision::NeedsGrant { .. } => dispatchdecision::HELPER.to_owned(),
    }
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
            "a dispatch is refused from inside a sub-agent. A persona chat belongs to a chat \
             the person can see, open and stop, and a sub-agent is not one. Return what you \
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
}
