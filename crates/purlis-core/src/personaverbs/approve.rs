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

/// What the command says when the approval for `persona` could not be written to `state`
/// (#1458, D-1458-4): nothing was approved for it, and the approvals of `recorded`, the personas
/// this run already wrote, stand. Inside a sandboxed chat that is the sandbox holding the record,
/// which is the person's to write: the refusal says so, as [`FROM_A_CHAT`] does, rather than
/// blaming the disk.
fn not_recorded(
    persona: &str,
    err: &std::io::Error,
    sandboxed: bool,
    state: &Path,
    recorded: &[String],
) -> String {
    let mut said = if sandboxed {
        format!(
            "this chat's sandbox holds the approvals, so nothing was approved for {persona}: \
             {FROM_A_CHAT}"
        )
    } else {
        format!(
            "purlis could not record the approval for {persona} ({err}), so nothing was \
             approved for it and its servers stay withheld. Check that {} can be written, then \
             run it again",
            crate::shown::short(&state.display().to_string())
        )
    };
    if !recorded.is_empty() {
        said.push_str(&format!(
            ". What this run recorded before it stands: {}",
            recorded.join(", ")
        ));
    }
    said
}

/// `purlis persona approve-mcp`, and its exit code: ask about, and record, each credentialed
/// server whose consent line the operator has read.
pub fn approve(root: &Path, options: &Options, ask: Option<Ask>, say: Sink) -> u8 {
    approve_in(root, options, ask, say, &crate::envvar::var)
}

/// [`approve`], reading whether this process's writes are held to a chat's sandbox from `env`.
fn approve_in(
    root: &Path,
    options: &Options,
    mut ask: Option<Ask>,
    say: Sink,
    env: &dyn Fn(&str) -> Option<String>,
) -> u8 {
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
    let mut recorded: Vec<String> = Vec::new();
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
        if !options.dry_run
            && let Err(err) = mcp::approve(root, &state, n, &keep)
        {
            say(Say::Fail(not_recorded(
                &mcp::label(&[n]),
                &err,
                crate::sandbox::writes_are_sandboxed_in(env, crate::sandbox::started()),
                &state,
                &recorded,
            )));
            return 1;
        }
        if !options.dry_run {
            recorded.push(mcp::label(&[n]));
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

/// One of a persona's credentialed servers this machine has not approved, as the window shows
/// it (#1460): its name, the line a yes approves, and that line's digest, which the window
/// sends back with the yes. `None` for an entry purlis cannot show in full, which nobody can
/// approve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub server: String,
    pub line: String,
    pub fingerprint: Option<String>,
}

/// The servers of `persona` that wait for this machine's approval, by name.
pub fn waiting(root: &Path, persona: &str) -> Vec<Waiting> {
    let ok = mcp::approved(&super::state_dir(root), persona);
    mcp::credentialed(root, persona)
        .into_iter()
        .filter(|c| c.fingerprint.as_ref().is_none_or(|fp| !ok.contains(fp)))
        .map(|c| Waiting {
            server: c.server,
            line: c.line,
            fingerprint: c.fingerprint,
        })
        .collect()
}

/// **A person's yes to one server, given in the window** (#1460): `server` of `persona` is
/// approved on this machine, beside what was approved already, **only if its line is still the
/// one the window showed** (`shown`, the line's digest). A line that changed since, or a server
/// that is gone, is refused and nothing is recorded, so a file changed between the showing and
/// the press is asked about again. The caller is the window alone: a chat never reaches this.
pub fn approve_shown(root: &Path, persona: &str, server: &str, shown: &str) -> Result<(), String> {
    if let Some(refused) = crate::personas::name_refusal(root, persona) {
        return Err(refused);
    }
    let label = mcp::label(&[persona, server]);
    let now = mcp::credentialed(root, persona)
        .into_iter()
        .find(|c| c.server == server)
        .ok_or_else(|| {
            format!(
                "{label} is no longer a server that takes a credential, so nothing was approved."
            )
        })?;
    let Some(fingerprint) = now.fingerprint else {
        return Err(format!("cannot approve {label}: {}", mcp::UNRENDERABLE));
    };
    if fingerprint != shown {
        return Err(format!(
            "{label} changed since it was shown, so nothing was approved. Read what it runs now, \
             then approve it again."
        ));
    }
    let state = super::state_dir(root);
    let mut keep: Vec<String> = mcp::approved(&state, persona).into_iter().collect();
    keep.push(fingerprint);
    mcp::approve(root, &state, persona, &keep).map_err(|err| {
        format!("purlis could not record the approval for {label} ({err}), so it stays withheld.")
    })
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
        run_in(plane, options, ask, false)
    }

    /// [`run`], in a process whose chat's sandbox is `sandboxed`, whatever the real one says.
    fn run_in(plane: &Plane, options: &Options, ask: Option<Ask>, sandboxed: bool) -> (u8, Heard) {
        let env = |name: &str| {
            (sandboxed && name == crate::hookwire::SANDBOXED_ENV).then(|| "1".to_owned())
        };
        let mut heard = Heard::default();
        let rc = approve_in(plane.root(), options, ask, &mut heard.sink(), &env);
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
    fn the_window_approves_one_server_as_it_was_shown_and_keeps_what_stood() {
        // #1460: the window shows each waiting server's line and sends its digest back.
        let plane = Plane::daily_with_ops();
        let waiting = waiting(plane.root(), "ops");
        let servers: Vec<&str> = waiting.iter().map(|w| w.server.as_str()).collect();
        assert_eq!(servers, ["grafana", "gsc"]);
        let shown = |at: usize| waiting[at].fingerprint.clone().expect("a line");

        approve_shown(plane.root(), "ops", "grafana", &shown(0)).expect("approved");
        assert_eq!(approved(&plane), [shown(0)]);
        let left: Vec<String> = super::waiting(plane.root(), "ops")
            .into_iter()
            .map(|w| w.server)
            .collect();
        assert_eq!(left, ["gsc"]);

        approve_shown(plane.root(), "ops", "gsc", &shown(1)).expect("approved");
        let mut both = vec![shown(0), shown(1)];
        both.sort();
        assert_eq!(
            approved(&plane),
            both,
            "the first approval stands beside the second"
        );
        assert!(super::waiting(plane.root(), "ops").is_empty());
    }

    #[test]
    fn a_line_that_changed_since_it_was_shown_is_refused_and_nothing_is_recorded() {
        let plane = Plane::daily_with_ops();
        let refused = approve_shown(plane.root(), "ops", "grafana", "0123abcd").unwrap_err();
        assert!(refused.contains("changed since it was shown"), "{refused}");
        let gone = approve_shown(plane.root(), "ops", "nope", "0123abcd").unwrap_err();
        assert!(
            gone.contains("no longer a server that takes a credential"),
            "{gone}"
        );
        assert!(approved(&plane).is_empty());
        assert!(!mcp::approvals_path(&plane.state()).exists());
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
    fn an_approval_that_cannot_be_written_is_refused_and_never_said_to_be_recorded() {
        // #1458, D-1458-4: a sandboxed chat is denied the record, so a write that fails is where
        // a chat that got past the refusal above ends up. Here the record's name is a folder.
        // The sandbox is set both ways here, never read from the process running the test.
        let plane = Plane::daily_with_ops();
        std::fs::create_dir_all(mcp::approvals_path(&plane.state())).expect("in the way");
        let yes = Options {
            yes: true,
            persona: Some("ops"),
            ..Options::default()
        };
        let (rc, heard) = run_in(&plane, &yes, None, false);
        assert_eq!(rc, 1);
        assert!(
            heard.err.contains("could not record the approval for ops"),
            "{}",
            heard.err
        );
        assert!(!heard.err.contains("sandbox"), "{}", heard.err);
        assert!(!heard.err.contains("Recorded"), "{}", heard.err);

        let (rc, heard) = run_in(&plane, &yes, None, true);
        assert_eq!(rc, 1);
        assert!(
            heard
                .err
                .contains("this chat's sandbox holds the approvals"),
            "{}",
            heard.err
        );
        assert!(heard.err.contains("is a person's to give"), "{}", heard.err);
        assert!(!heard.err.contains("Recorded"), "{}", heard.err);
    }

    #[test]
    fn a_failed_write_says_which_approvals_of_this_run_already_stand() {
        let err = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let state = Path::new("/srv/state");
        let first = not_recorded("ops", &err, false, state, &[]);
        assert!(!first.contains("stands"), "{first}");
        assert!(first.contains("/srv/state"), "{first}");
        let later = not_recorded("solo", &err, false, state, &["ops".to_owned()]);
        assert!(
            later.contains("What this run recorded before it stands: ops"),
            "{later}"
        );
        assert!(!later.contains("nothing else changed"), "{later}");
        let sandboxed = not_recorded("ops", &err, true, state, &[]);
        assert!(
            sandboxed.contains("sandbox holds the approvals"),
            "{sandboxed}"
        );
        assert!(
            sandboxed.contains("purlis persona approve-mcp"),
            "{sandboxed}"
        );
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
