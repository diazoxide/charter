//! `secret exec` end to end against a plain-file vault in a temp plane, and the pieces of it —
//! the exit status it passes through, the signal wait — that a real run cannot reach on cue.
//!
//! Every child here is `/bin/sh` or a program that does not exist, and every vault is a file
//! under a temp directory. Nothing here signals this test process: a test that needs a real
//! signal delivered runs itself again in a child process of its own. And every `--exec` names
//! a program that does not exist, so a mutant that lets one through cannot replace the test
//! process and exit as a pass.

use super::*;
use crate::secrets::{Ctx, Env};

/// What `exec` said and printed.
#[derive(Default)]
struct Rec {
    said: Vec<Say>,
    out: Vec<u8>,
    err: Vec<u8>,
}

impl Rec {
    fn errors(&self) -> Vec<&str> {
        self.said
            .iter()
            .filter_map(|s| match s {
                Say::Err(m) => Some(m.as_str()),
                _ => None,
            })
            .collect()
    }

    fn out(&self) -> String {
        String::from_utf8_lossy(&self.out).into_owned()
    }
}

impl Io for Rec {
    fn say(&mut self, line: Say) {
        self.said.push(line);
    }
    fn out(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }
    fn err(&mut self, bytes: &[u8]) {
        self.err.extend_from_slice(bytes);
    }
    fn stdout_is_terminal(&self) -> bool {
        false
    }
    fn stdin_is_terminal(&self) -> bool {
        false
    }
    fn read_stdin(&mut self) -> String {
        String::new()
    }
    fn read_hidden(&mut self, _prompt: &str) -> String {
        String::new()
    }
}

const TOKEN: &str = "s3cret-value";
const OTHER: &str = "0ther-value";

/// A plane whose vault `team` is a plain file holding `TOKEN`, `OTHER` and `CR` (a value only
/// dotenv's escaped tier can carry), with `vaults` merged into the registry beside it and
/// `env` as the whole environment besides a `PATH`.
fn plane(vaults: serde_json::Value, env: &[(&str, &str)]) -> (tempfile::TempDir, Ctx) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let mut registry = serde_json::json!({
        "team": {"provider": "plain-file", "config": {"file": "team.json"}}
    });
    for (k, v) in vaults.as_object().into_iter().flatten() {
        registry[k] = v.clone();
    }
    std::fs::write(
        root.join("vaults.json"),
        serde_json::json!({ "vaults": registry }).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("team.json"),
        serde_json::json!({"TOKEN": TOKEN, "OTHER": OTHER, "CR": "a\r\nb"}).to_string(),
    )
    .unwrap();
    let mut vars = vec![("PATH", "/usr/bin:/bin")];
    vars.extend_from_slice(env);
    let ctx = Ctx::new(root, Env::of(&vars));
    (tmp, ctx)
}

fn sh(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

fn run(ctx: &Ctx, req: Request) -> (i32, Rec) {
    let mut rec = Rec::default();
    let code = exec(ctx, &req, &mut rec);
    (code, rec)
}

fn req(command: Vec<String>) -> Request {
    Request {
        vault: "team".into(),
        command,
        ..Default::default()
    }
}

#[test]
fn an_env_binding_hands_the_value_to_the_child_and_its_output_comes_back_redacted() {
    // The variable is already set in charter's environment: the binding replaces it.
    let (_tmp, ctx) = plane(serde_json::json!({}), &[("TOKEN_ENV", "stale")]);
    let (code, rec) = run(
        &ctx,
        Request {
            env: vec!["TOKEN_ENV=TOKEN".into()],
            ..req(sh(
                r#"test "$TOKEN_ENV" = s3cret-value && echo "got $TOKEN_ENV"; exit 7"#,
            ))
        },
    );
    assert_eq!(rec.errors(), Vec::<&str>::new());
    assert_eq!(code, 7, "the child's own status");
    assert_eq!(rec.out(), "got ***\n");
}

#[test]
fn a_child_that_exits_high_on_its_own_still_has_both_streams_printed_redacted() {
    // 130 looks like a death by Ctrl-C, but charter caught no signal: what it said is shown.
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let (code, rec) = run(
        &ctx,
        Request {
            env: vec!["T=TOKEN".into()],
            ..req(sh(r#"echo "out $T"; echo "err $T" >&2; exit 130"#))
        },
    );
    assert_eq!(code, 130);
    assert_eq!(rec.out(), "out ***\n");
    assert_eq!(String::from_utf8_lossy(&rec.err), "err ***\n");
}

/// Set on the child that [`a_child_stopped_by_a_signal_charter_caught_has_nothing_printed`]
/// re-runs itself as.
#[cfg(unix)]
const STOP: &str = "SECRETS_EXEC_TEST_STOP_ON_A_SIGNAL";

#[cfg(unix)]
#[test]
fn a_child_stopped_by_a_signal_charter_caught_has_nothing_printed() {
    if std::env::var_os(STOP).is_none() {
        // The child signals the process running `exec`, so it runs alone, in a child.
        crate::testrun::rerun(
            &[
                "secrets::exec::tests::a_child_stopped_by_a_signal_charter_caught_has_nothing_printed",
            ],
            &[(STOP, "1".as_ref())],
        );
        return;
    }
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let started = std::time::Instant::now();
    let (code, rec) = run(
        &ctx,
        req(sh(r#"echo "said before"; kill -TERM $PPID; sleep 30"#)),
    );
    assert_eq!(code, 128 + 15);
    assert_eq!(
        rec.out(),
        "",
        "the child is gone and nothing it said is printed"
    );
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "killed, not waited out"
    );
}

#[test]
fn a_binding_without_a_variable_name_is_refused_before_anything_runs() {
    let (tmp, ctx) = plane(serde_json::json!({}), &[]);
    let marker = tmp.path().join("ran");
    let touch = sh(&format!("touch '{}'", marker.display()));
    for (env, file, said) in [
        (
            vec!["=TOKEN"],
            vec![],
            "--env expects NAME=key, got '=TOKEN'",
        ),
        (vec!["TOKEN"], vec![], "--env expects NAME=key, got 'TOKEN'"),
        (
            vec![],
            vec!["=TOKEN"],
            "--file expects ENVVAR=key, got '=TOKEN'",
        ),
        (
            vec![],
            vec!["TOKEN"],
            "--file expects ENVVAR=key, got 'TOKEN'",
        ),
    ] {
        let (code, rec) = run(
            &ctx,
            Request {
                env: env.into_iter().map(String::from).collect(),
                file: file.into_iter().map(String::from).collect(),
                ..req(touch.clone())
            },
        );
        assert_eq!((code, rec.errors()), (2, vec![said]));
        assert!(!marker.exists(), "{said}: nothing was started");
    }
}

#[test]
fn a_file_binding_is_a_0600_temp_file_holding_the_value_and_removed_after() {
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let (code, rec) = run(
        &ctx,
        Request {
            file: vec!["TOKEN_FILE=TOKEN".into()],
            ..req(sh(r#"echo "$TOKEN_FILE"
test "$(cat "$TOKEN_FILE")" = s3cret-value && echo same
stat -f %Lp "$TOKEN_FILE" 2>/dev/null || stat -c %a "$TOKEN_FILE""#))
        },
    );
    assert_eq!((code, rec.errors()), (0, vec![]));
    let out = rec.out();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    let path = std::path::Path::new(lines[0]);
    assert!(
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("charter-secret-"),
        "{out}"
    );
    assert_eq!(&lines[1..], ["same", "600"]);
    assert!(
        !path.exists(),
        "the temp file is removed once the child is done"
    );
}

#[test]
fn dotenv_entries_sharing_a_variable_merge_into_one_file_in_flag_order() {
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let (code, rec) = run(
        &ctx,
        Request {
            dotenv: vec![
                "ENVF=A:TOKEN".into(),
                "ENVF=B:OTHER".into(),
                // The same NAME in a different file is no conflict.
                "SEP=A:OTHER".into(),
            ],
            ..req(sh(
                r#"cut -d= -f1 "$ENVF" | tr '\n' ,; echo; cut -d= -f1 "$SEP" | tr '\n' ,; echo
cat "$ENVF""#,
            ))
        },
    );
    assert_eq!((code, rec.errors()), (0, vec![]));
    assert_eq!(rec.out(), "A,B,\nA,\nA='***'\nB='***'\n");
}

#[test]
fn a_dotenv_name_given_twice_for_one_file_is_refused() {
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let (code, rec) = run(
        &ctx,
        Request {
            dotenv: vec!["F=A:TOKEN".into(), "F=A:OTHER".into()],
            ..req(sh("exit 0"))
        },
    );
    assert_eq!(code, 2);
    assert_eq!(rec.errors().len(), 1);
    assert!(
        rec.errors()[0].starts_with("--dotenv defines 'A' twice for F;"),
        "{:?}",
        rec.errors()
    );
}

#[test]
fn a_dotenv_spec_needs_a_variable_a_name_and_a_key() {
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    for spec in ["F=A:", "F=:TOKEN", "=A:TOKEN", "F=A", "FA:TOKEN"] {
        let (code, rec) = run(
            &ctx,
            Request {
                dotenv: vec![spec.into()],
                ..req(sh("exit 0"))
            },
        );
        assert_eq!(
            (code, rec.errors()),
            (
                2,
                vec![format!("--dotenv expects ENVVAR=NAME:key, got '{spec}'").as_str()]
            )
        );
    }
}

#[test]
fn the_escaped_form_a_dotenv_file_holds_is_redacted_too() {
    // A carriage return is carried only by the double-quoted tier, as `\r\n` spelled out: the
    // child printing its file prints that spelling, not the value's own bytes.
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let (code, rec) = run(
        &ctx,
        Request {
            dotenv: vec!["F=C:CR".into()],
            ..req(sh(r#"cat "$F""#))
        },
    );
    assert_eq!((code, rec.errors()), (0, vec![]));
    assert_eq!(rec.out(), "C=\"***\"\n");
}

#[test]
fn exec_refuses_what_it_could_never_clean_up_and_names_each_flag() {
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let base = Request {
        exec: true,
        // Never a program that exists: an `--exec` this test expects refused but that is let
        // through would replace the test process, and its exit would read as a pass.
        ..req(vec!["/nonexistent/charter-exec-test".into()])
    };
    for (file, dotenv, named) in [
        (true, false, "--file cannot be combined with --exec"),
        (false, true, "--dotenv cannot be combined with --exec"),
        (
            true,
            true,
            "--file and --dotenv cannot be combined with --exec",
        ),
    ] {
        let (code, rec) = run(
            &ctx,
            Request {
                file: if file { vec!["F=TOKEN".into()] } else { vec![] },
                dotenv: if dotenv {
                    vec!["D=A:TOKEN".into()]
                } else {
                    vec![]
                },
                ..base.clone()
            },
        );
        assert_eq!(code, 2);
        assert!(rec.errors()[0].starts_with(named), "{:?}", rec.errors());
    }
    let (code, rec) = run(
        &ctx,
        Request {
            stream: true,
            ..base
        },
    );
    assert_eq!(code, 2);
    assert!(rec.errors()[0].starts_with("--exec and --stream"));
}

#[test]
fn exec_with_nothing_to_clean_up_replaces_the_process_and_a_missing_program_is_127() {
    // `--exec` with only an `--env` binding is allowed through to the replacement; a program
    // that does not exist is the one way it comes back, and in this process.
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let (code, rec) = run(
        &ctx,
        Request {
            exec: true,
            env: vec!["T=TOKEN".into()],
            ..req(vec!["/nonexistent/charter-exec-test".into()])
        },
    );
    assert_eq!(
        (code, rec.errors()),
        (
            127,
            vec!["command not found: /nonexistent/charter-exec-test"]
        )
    );
}

#[test]
fn a_child_that_cannot_start_is_127_when_missing_and_1_otherwise() {
    let (tmp, ctx) = plane(serde_json::json!({}), &[]);
    for stream in [false, true] {
        let (code, rec) = run(
            &ctx,
            Request {
                stream,
                ..req(vec!["/nonexistent/charter-exec-test".into()])
            },
        );
        assert_eq!(
            (code, rec.errors()),
            (
                127,
                vec!["command not found: /nonexistent/charter-exec-test"]
            )
        );
    }
    // A directory exists but cannot be run: named, and 1.
    let dir = tmp.path().to_string_lossy().into_owned();
    let (code, rec) = run(&ctx, req(vec![dir.clone()]));
    assert_eq!(code, 1);
    assert!(
        rec.errors()[0].starts_with(&format!("cannot run {dir}: ")),
        "{:?}",
        rec.errors()
    );
}

#[test]
fn the_child_keeps_this_vaults_identity_and_loses_every_other_vaults() {
    let (_tmp, ctx) = plane(
        serde_json::json!({
            "team": {"provider": "plain-file",
                     "config": {"file": "team.json", "env": {"TEAM_TARGET": "TEAM_SOURCE"}}},
            "other": {"provider": "1password",
                      "config": {"op-vault": "X", "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_OTHER"}}}
        }),
        &[
            ("TEAM_SOURCE", "ts"),
            ("TEAM_TARGET", "tt"),
            ("OP_OTHER", "oo"),
            ("OP_SERVICE_ACCOUNT_TOKEN", "sa"),
            ("KEEP", "k"),
        ],
    );
    let (code, rec) = run(
        &ctx,
        req(sh(
            "echo ${TEAM_SOURCE-unset} ${TEAM_TARGET-unset} ${OP_OTHER-unset} \
             ${OP_SERVICE_ACCOUNT_TOKEN-unset} ${KEEP-unset}",
        )),
    );
    assert_eq!((code, rec.errors()), (0, vec![]));
    assert_eq!(rec.out(), "ts tt unset unset k\n");
}

#[cfg(unix)]
#[test]
fn a_child_killed_by_a_signal_exits_as_python_s_sys_exit_of_minus_n_leaves_it() {
    use std::os::unix::process::ExitStatusExt;
    // A raw wait status: signal number in the low bits, exit code in the second byte.
    assert_eq!(exit_status(&std::process::ExitStatus::from_raw(9)), 247);
    assert_eq!(exit_status(&std::process::ExitStatus::from_raw(15)), 241);
    assert_eq!(exit_status(&std::process::ExitStatus::from_raw(7 << 8)), 7);
    assert_eq!(exit_status(&std::process::ExitStatus::from_raw(0)), 0);
}

/// A [`Termination`] that has already caught `sig`, with no handler installed: supervision
/// read in isolation, with no signal sent to anything.
#[cfg(unix)]
fn caught(sig: i32) -> Termination {
    Termination {
        flag: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(sig as usize)),
        ids: Vec::new(),
    }
}

#[cfg(unix)]
fn spawn(script: &str) -> std::process::Child {
    let mut c = Command::new("/bin/sh");
    c.args(["-c", script]);
    crate::forklock::spawn(&mut c).unwrap()
}

#[cfg(unix)]
#[test]
fn after_a_ctrl_c_the_child_is_given_its_quarter_second_to_finish() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("finished");
    let child = spawn(&format!("sleep 0.02; touch '{}'", marker.display()));
    assert_eq!(supervise(child, &caught(Termination::SIGINT)), 128 + 2);
    assert!(marker.exists(), "the child finished on its own, not killed");
}

#[cfg(unix)]
#[test]
fn after_any_other_terminating_signal_the_child_is_killed_at_once() {
    let started = std::time::Instant::now();
    let child = spawn("sleep 30");
    assert_eq!(supervise(child, &caught(15)), 128 + 15);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn with_no_signal_the_childs_own_status_passes_through() {
    assert_eq!(supervise(spawn("exit 3"), &caught(0)), 3);
}

#[cfg(unix)]
#[test]
fn the_signals_taken_over_are_the_terminating_ones() {
    use signal_hook::consts::*;
    let taken = Termination::signals();
    for sig in [SIGINT, SIGHUP, SIGQUIT, SIGTERM, SIGUSR1, SIGUSR2] {
        assert!(taken.contains(&sig), "{sig} is taken over");
    }
    for sig in [SIGKILL, SIGSTOP, SIGPIPE, SIGSEGV, SIGCHLD, 0] {
        assert!(!taken.contains(&sig), "{sig} is left alone");
    }
}

/// Set on the child that [`the_handlers_are_gone_once_exec_returns`] re-runs itself as.
#[cfg(unix)]
const DELIVER: &str = "SECRETS_EXEC_TEST_DELIVER_A_SIGNAL";

#[cfg(unix)]
#[test]
fn the_handlers_are_gone_once_exec_returns() {
    if std::env::var_os(DELIVER).is_none() {
        // A signal is sent to the process running this, so it runs alone, in a child.
        crate::testrun::rerun(
            &["secrets::exec::tests::the_handlers_are_gone_once_exec_returns"],
            &[(DELIVER, "1".as_ref())],
        );
        return;
    }
    let (_tmp, ctx) = plane(serde_json::json!({}), &[]);
    let (code, _) = run(
        &ctx,
        Request {
            env: vec!["T=TOKEN".into()],
            ..req(sh("exit 0"))
        },
    );
    assert_eq!(code, 0);
    // A handler of this test's own, so the signal is observed and does not end the process.
    let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let id = signal_hook::flag::register(signal_hook::consts::SIGUSR1, seen.clone()).unwrap();
    let status = crate::forklock::status(
        Command::new("/bin/kill").args(["-USR1", &std::process::id().to_string()]),
    )
    .unwrap();
    assert!(status.success());
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !seen.load(std::sync::atomic::Ordering::SeqCst) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(seen.load(std::sync::atomic::Ordering::SeqCst), "delivered");
    signal_hook::low_level::unregister(id);
    assert_eq!(
        super::super::run::interrupted(),
        None,
        "exec's handlers no longer record a signal"
    );
}
