//! A vendor CLI run the way a vault runs one: found on the vault's `PATH`, bounded, stopped
//! by a signal, and its status reported the way Python reports `returncode`.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use super::*;

fn script(dir: &Path, name: &str, body: &str, mode: u32) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    p
}

fn sh(body: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), body.into()]
}

#[test]
fn which_takes_the_first_executable_file_on_the_path_and_skips_what_is_not_one() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    script(first.path(), "tool", "true", 0o644);
    std::fs::create_dir(first.path().join("dir-tool")).unwrap();
    let found = script(second.path(), "tool", "true", 0o755);
    let path = format!("{}:{}", first.path().display(), second.path().display());
    assert_eq!(which("tool", Some(&path)), Some(found.clone()));
    assert_eq!(which("dir-tool", Some(&path)), None);
    assert_eq!(which("absent", Some(&path)), None);
    assert_eq!(which("tool", None), None, "no PATH finds nothing by name");
    assert_eq!(which("", Some(&path)), None);
    let named = found.to_string_lossy().into_owned();
    assert_eq!(
        which(&named, None),
        Some(found),
        "a path is checked as it stands"
    );
    let not_exec = first.path().join("tool").to_string_lossy().into_owned();
    assert_eq!(which(&not_exec, Some(&path)), None);
}

#[test]
fn a_cli_is_found_on_the_vaults_path_and_hears_only_its_input_and_overlay() {
    let bin = tempfile::tempdir().unwrap();
    script(
        bin.path(),
        "vendor",
        "IFS= read -r fed; printf '%s|%s|%s' \"$1\" \"$TOKEN\" \"$fed\"; printf 'warned' >&2",
        0o755,
    );
    let path = bin.path().to_string_lossy().into_owned();
    let env = Env::of(&[("PATH", &path), ("TOKEN", "ambient")]);
    let ran = run(
        &env,
        &["vendor".into(), "arg".into()],
        Some("fed"),
        &[("TOKEN".into(), "bound".into())],
        Some(Duration::from_secs(30)),
    )
    .unwrap();
    assert_eq!(ran.code, 0);
    assert_eq!(ran.stdout, "arg|bound|fed");
    assert_eq!(ran.stderr, "warned");

    let ran = run(&env, &["vendor".into(), "x".into()], None, &[], None).unwrap();
    assert_eq!(
        ran.stdout, "x|ambient|",
        "no input is /dev/null, never a hang"
    );
}

#[test]
fn a_cli_that_finishes_inside_its_timeout_is_answered_not_stopped() {
    let ran = run(
        &Env::of(&[]),
        &sh("sleep 0.2; echo done"),
        None,
        &[],
        Some(Duration::from_secs(30)),
    )
    .unwrap();
    assert_eq!((ran.code, ran.stdout.as_str()), (0, "done\n"));
}

#[test]
fn a_cli_that_runs_past_its_timeout_is_stopped() {
    let started = std::time::Instant::now();
    let err = run(
        &Env::of(&[]),
        &sh("sleep 30"),
        None,
        &[],
        Some(Duration::from_millis(200)),
    )
    .unwrap_err();
    assert!(matches!(err, RunError::Timeout), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn an_exit_status_is_python_s_returncode() {
    let env = Env::of(&[]);
    assert_eq!(run(&env, &sh("exit 3"), None, &[], None).unwrap().code, 3);
    assert_eq!(run(&env, &sh("exit 0"), None, &[], None).unwrap().code, 0);
    assert_eq!(
        run(&env, &sh("kill -9 $$"), None, &[], None).unwrap().code,
        -9,
        "a death by signal N is -N"
    );
}

#[test]
fn what_a_cli_said_is_debugged_as_its_size_and_its_status_as_itself() {
    let ran = Ran {
        code: 2,
        stdout: "value".into(),
        stderr: String::new(),
    };
    assert_eq!(
        format!("{ran:?}"),
        "Ran { code: 2, stdout: *** (5 bytes), stderr: *** (0 bytes) }"
    );
}

#[test]
fn a_cli_that_cannot_be_started_says_so() {
    let err = run(
        &Env::of(&[]),
        &["/nonexistent/charter-vendor-cli".into()],
        None,
        &[],
        None,
    )
    .unwrap_err();
    assert!(matches!(err, RunError::Spawn(_)), "{err:?}");
}

/// Set on the child of [`a_signal_charter_was_sent_stops_the_cli_it_is_waiting_on`].
const INTERRUPT_PROBE: &str = "SECRETS_RUN_INTERRUPT_PROBE";

/// The interrupt flag is process-wide, so it is raised in a child of this test binary that
/// runs this one test, never beside the others.
#[test]
fn a_signal_charter_was_sent_stops_the_cli_it_is_waiting_on() {
    if std::env::var_os(INTERRUPT_PROBE).is_some() {
        assert_eq!(interrupted(), None);
        interrupt_flag().store(15, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(interrupted(), Some(15));
        let started = std::time::Instant::now();
        let err = run(&Env::of(&[]), &sh("sleep 30"), None, &[], None).unwrap_err();
        assert!(matches!(err, RunError::Interrupted(15)), "{err:?}");
        assert!(started.elapsed() < Duration::from_secs(20));
        return;
    }
    crate::testrun::rerun(
        &["secrets::run::tests::a_signal_charter_was_sent_stops_the_cli_it_is_waiting_on"],
        &[(INTERRUPT_PROBE, "1".as_ref())],
    );
}
