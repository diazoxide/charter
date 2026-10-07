//! `purlis persona approve-mcp` — a person's yes to a persona's credentialed MCP servers
//! (#1451, D-1451-17). A port of `commands_persona._approve_mcp`, which was reached as
//! `persona sync-agents --approve-mcp` until that command was retired.
//!
//! A server that declares `secrets` or `secret_files` is handed a value from the persona's
//! vault, by a command a **committed** file names. So purlis starts it for a chat only once a
//! person on this machine has read the line it would run and said yes
//! ([`super::chatstart::servers`]). What is recorded is a digest of that line ([`super::mcp`]),
//! so any change to the entry or the vault lapses the approval and it is asked again.
//!
//! **Never from inside a chat.** The approval is what stands between a file a chat can write
//! and a vault value, so a chat cannot give it: with the app's chat variable set the command
//! is refused, `--yes` or not. `--yes` is for a terminal with nobody to ask, and `--dry-run`
//! shows the lines and records nothing.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use crate::repocmd::{Say, Sink};

use super::mcp;

/// One y/N question on a terminal: `Some(answer)`, or `None` when the operator interrupted.
pub type Ask<'a> = &'a mut dyn FnMut(&str) -> Option<bool>;

/// [`confirm_on_terminal`]'s question, owned.
pub type Confirm = Box<dyn FnMut(&str) -> Option<bool>>;

/// What `approve-mcp` was asked.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options<'a> {
    /// Only this persona (default: every persona).
    pub persona: Option<&'a str>,
    pub yes: bool,
    pub dry_run: bool,
    /// Whether the command runs inside a chat the app started.
    pub in_chat: bool,
}

/// What a chat that asks is told.
pub const FROM_A_CHAT: &str = "approving a persona's MCP servers hands a value from its vault \
     to a command a committed file names, so it is a person's to give and is not run from \
     inside a chat; nothing was recorded. Ask the operator to run `purlis persona approve-mcp` \
     in a terminal";

/// `purlis persona approve-mcp`, and its exit code: ask about, and record, each credentialed
/// server whose consent line the operator has read.
pub fn approve(root: &Path, options: &Options, mut ask: Option<Ask>, say: Sink) -> u8 {
    if options.in_chat {
        say(Say::Fail(FROM_A_CHAT.into()));
        return 1;
    }
    let one = options.persona.filter(|p| !p.is_empty());
    if let Some(one) = one
        && let Some(refused) = crate::personas::name_refusal(root, one)
    {
        say(Say::Fail(refused));
        return 1;
    }
    if !(options.yes || options.dry_run || ask.is_some()) {
        say(Say::Fail(
            "approve-mcp hands a persona's vault value to a command a COMMITTED file names, so \
             it asks before recording — and there is no terminal to ask on."
                .into(),
        ));
        say(Say::Info(
            "  Re-run in a terminal, or add --dry-run to see what it would ask about without \
             recording anything."
                .into(),
        ));
        return 1;
    }
    let names: Vec<String> = match one {
        Some(one) => vec![one.to_string()],
        None => super::names(root),
    };
    let state = super::state_dir(root);
    let mut any = false;
    for n in &names {
        let declared = mcp::credentialed(root, n);
        if declared.is_empty() {
            continue;
        }
        any = true;
        let mut keep: Vec<String> = Vec::new();
        for c in declared {
            let label = mcp::label(&[n, &c.server]);
            let Some(fp) = c.fingerprint else {
                say(Say::Warn(format!(
                    "  cannot approve {label} — {}",
                    mcp::UNRENDERABLE
                )));
                continue;
            };
            say(Say::Info(format!("  {label} → {}", c.line)));
            if options.dry_run {
                continue;
            }
            if !options.yes {
                let answer = match ask.as_mut() {
                    Some(ask) => ask(&format!("    approve {label}? [y/N] ")),
                    None => Some(false),
                };
                match answer {
                    None => {
                        say(Say::Fail(format!(
                            "interrupted — nothing recorded for {}",
                            mcp::label(&[n])
                        )));
                        return 130;
                    }
                    Some(false) => {
                        say(Say::Info(format!(
                            "    skipped {label} — it stays withheld from this persona's chats"
                        )));
                        continue;
                    }
                    Some(true) => {}
                }
            }
            keep.push(fp);
        }
        if !options.dry_run {
            mcp::approve(root, &state, n, &keep);
        }
    }
    if !any {
        say(Say::Info(
            "No persona here declares an MCP server that takes a credential, so there is \
             nothing to approve."
                .into(),
        ));
        return 0;
    }
    if options.dry_run {
        say(Say::Info(
            "  --dry-run: nothing approved. Re-run without it to be asked.".into(),
        ));
    } else {
        say(Say::Done(
            "Recorded on this machine. A chat started as the persona from now on is started \
             with the servers approved (on Claude Code)."
                .into(),
        ));
    }
    0
}

/// A y/N question for a caller on a terminal: the question on stderr, one line read from
/// stdin, and only an explicit `y`/`yes` is a yes. EOF is a no. `None` off a terminal.
pub fn confirm_on_terminal() -> Option<Confirm> {
    if !std::io::stdin().is_terminal() {
        return None;
    }
    Some(Box::new(|prompt: &str| {
        // The question is the command's output on the terminal, not a diagnostic, so it is
        // written to standard error directly and never through the log (#647).
        let _ = write!(std::io::stderr(), "{prompt}");
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => {
                let _ = writeln!(std::io::stderr());
                Some(false)
            }
            Ok(_) => {
                let answer = crate::memstore::py_strip(&line).to_lowercase();
                Some(answer == "y" || answer == "yes")
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personaverbs::tests_plane::{Heard, OPS_APPROVED, Plane};

    fn run(plane: &Plane, options: &Options, ask: Option<Ask>) -> (u8, Heard) {
        let mut heard = Heard::default();
        let rc = approve(plane.root(), options, ask, &mut heard.sink());
        (rc, heard)
    }

    fn approved(plane: &Plane) -> Vec<String> {
        mcp::approved(&plane.state(), "ops").into_iter().collect()
    }

    #[test]
    fn a_yes_on_the_terminal_records_that_server_and_a_no_leaves_it_withheld() {
        let plane = Plane::daily_with_ops();
        let mut asked: Vec<String> = Vec::new();
        let mut answer = |question: &str| {
            asked.push(question.to_owned());
            Some(question.contains("grafana"))
        };
        let (rc, heard) = run(&plane, &Options::default(), Some(&mut answer));
        assert_eq!(rc, 0, "{}", heard.err);
        assert_eq!(
            asked,
            [
                "    approve ops/grafana? [y/N] ",
                "    approve ops/gsc? [y/N] "
            ]
        );
        let grafana = mcp::credentialed(plane.root(), "ops")[0]
            .fingerprint
            .clone()
            .expect("a line");
        assert!(OPS_APPROVED.contains(&grafana.as_str()));
        assert_eq!(approved(&plane), [grafana]);
        assert!(
            heard
                .err
                .contains("skipped ops/gsc — it stays withheld from this persona's chats"),
            "{}",
            heard.err
        );
        // And the chat's start now carries the one that was approved.
        let servers = crate::personaverbs::chatstart::servers(
            plane.root(),
            &plane.state(),
            "ops",
            Path::new("/bin/purlis"),
        );
        assert!(servers.started.contains_key("grafana"));
        assert_eq!(servers.withheld.len(), 1);
    }

    #[test]
    fn a_chat_cannot_approve_its_own_persona_s_servers_whatever_it_passes() {
        // The approval stands between a file a chat can write and a vault value.
        let plane = Plane::daily_with_ops();
        for yes in [false, true] {
            let options = Options {
                yes,
                in_chat: true,
                ..Options::default()
            };
            let (rc, heard) = run(&plane, &options, None);
            assert_eq!(rc, 1);
            assert!(
                heard.err.contains("is not run from inside a chat"),
                "{}",
                heard.err
            );
        }
        assert!(approved(&plane).is_empty());
        assert!(!mcp::approvals_path(&plane.state()).exists());
    }

    #[test]
    fn off_a_terminal_it_refuses_without_yes_and_a_dry_run_records_nothing() {
        let plane = Plane::daily_with_ops();
        let (rc, heard) = run(&plane, &Options::default(), None);
        assert_eq!(rc, 1);
        assert!(
            heard.err.contains("there is no terminal to ask on"),
            "{}",
            heard.err
        );
        let dry = Options {
            dry_run: true,
            ..Options::default()
        };
        let (rc, heard) = run(&plane, &dry, None);
        assert_eq!(rc, 0);
        assert!(
            heard
                .err
                .contains("ops/grafana → run npx -y grafana-mcp@1.2.0"),
            "{}",
            heard.err
        );
        assert!(approved(&plane).is_empty());
        let yes = Options {
            yes: true,
            persona: Some("ops"),
            ..Options::default()
        };
        assert_eq!(run(&plane, &yes, None).0, 0);
        assert_eq!(approved(&plane).len(), 2);
    }

    #[test]
    fn a_persona_nobody_defines_is_refused_and_a_project_with_nothing_to_approve_says_so() {
        let plane = Plane::fixture("minimal");
        let ghost = Options {
            persona: Some("ghost"),
            yes: true,
            ..Options::default()
        };
        assert_eq!(run(&plane, &ghost, None).0, 1);
        let all = Options {
            yes: true,
            ..Options::default()
        };
        let (rc, heard) = run(&plane, &all, None);
        assert_eq!(rc, 0);
        assert!(
            heard.err.contains("there is nothing to approve"),
            "{}",
            heard.err
        );
    }
}
