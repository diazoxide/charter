//! The secrets commands, where a property is not a line of output the recorded suite can hold.
//!
//! `tests/fixtures/recorded/behaviour.jsonl` (the `secrets` suite) pins what each command SAYS
//! and exits with, against what the Python charter said. What it cannot pin is here:
//!
//! - the access record — one trace line per credential handed out, holding names and never a
//!   value, in the shape Python writes it (the recorded rows ignore the trace, whose `ts` moves);
//! - a `--stream` child's temp file surviving nothing: removed when charter is SIGTERMed while
//!   the child is still running, which is the ordinary way a long-running child ends;
//! - the Bash guard still refusing a vault read when it is wrapped in `charter secret exec`.
//!
//! Every value in here is fabricated for the test.

#![cfg(unix)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const TOKEN: &str = "tok-test-5e2a91c07d4f";

/// A copy of the committed `daily` fixture plane with a plain-file vault `devops` in it.
fn plane() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().expect("a directory");
    copy(&fixture, &dir.path().join("plane"));
    let root = dir.path().join("plane");
    std::fs::write(
        root.join(".charter/vaults.json"),
        r#"{"vaults": {"devops": {"provider": "plain-file", "persona": "devops",
            "config": {"file": ".charter/vaults/devops.json"}},
            "qa": {"provider": "1password", "persona": null,
            "config": {"op-vault": "QA", "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEST_QA"}}}}}"#,
    )
    .expect("the registry");
    std::fs::write(
        root.join(".charter/vaults/devops.json"),
        format!(r#"{{"API_TOKEN": "{TOKEN}", "KUBECONFIG": "kind: Config\n"}}"#),
    )
    .expect("the vault");
    std::fs::create_dir_all(dir.path().join("tmp")).expect("a temp directory");
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("a readable fixture") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).expect("a copy");
        }
    }
}

/// `charter <args>` in the plane, with an environment of exactly what the test names.
fn charter(tmp: &tempfile::TempDir, args: &[&str]) -> Command {
    let root = tmp.path().join("plane");
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(args)
        .current_dir(&root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", tmp.path())
        .env("TMPDIR", tmp.path().join("tmp"))
        .env("CHARTER_ROOT", &root)
        .env("CHARTER_SESSION_ID", "secrets-test")
        .env("CHARTER_PLANE_FENCE", tmp.path())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn run(tmp: &tempfile::TempDir, args: &[&str]) -> Output {
    charter(tmp, args).output().expect("the binary runs")
}

fn trace(tmp: &tempfile::TempDir) -> Vec<serde_json::Value> {
    let path = tmp
        .path()
        .join("plane/.charter/persona-state/trace/secrets-test.jsonl");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).expect("a trace line is JSON"))
        .collect()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn exec_records_which_command_got_which_keys_and_never_a_value() {
    let tmp = plane();
    let out = run(
        &tmp,
        &[
            "secret",
            "exec",
            "devops",
            "--env",
            "T=API_TOKEN",
            "--file",
            "F=KUBECONFIG",
            "--",
            "sh",
            "-c",
            "echo \"$T\"",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "***\n");
    let lines = trace(&tmp);
    let last = lines.last().expect("one event was recorded");
    // Python's record, key for key and in its order: `ts`, `event`, then the fields as
    // `_trace_secret_use` passed them.
    let keys: Vec<&str> = last
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "ts",
            "event",
            "vault",
            "key_names",
            "env_names",
            "argv0",
            "mode"
        ]
    );
    assert_eq!(last["event"], "secret-exec");
    assert_eq!(last["vault"], "devops");
    assert_eq!(
        last["key_names"],
        serde_json::json!(["API_TOKEN", "KUBECONFIG"])
    );
    assert_eq!(last["env_names"], serde_json::json!(["F", "T"]));
    assert_eq!(last["argv0"], "sh");
    assert_eq!(last["mode"], "capture");
    let raw = std::fs::read_to_string(
        tmp.path()
            .join("plane/.charter/persona-state/trace/secrets-test.jsonl"),
    )
    .expect("the trace");
    assert!(!raw.contains(TOKEN), "the trace holds a value: {raw}");
}

#[test]
fn a_run_that_resolves_nothing_is_still_recorded() {
    let tmp = plane();
    let out = run(
        &tmp,
        &["secret", "exec", "devops", "--stream", "--", "true"],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let last = trace(&tmp).pop().expect("recorded");
    assert_eq!(last["key_names"], serde_json::json!([]));
    assert_eq!(last["env_names"], serde_json::json!([]));
    assert_eq!(last["mode"], "stream");
}

#[test]
fn a_forced_reveal_is_recorded_before_the_value_is_printed() {
    let tmp = plane();
    let out = run(
        &tmp,
        &[
            "secret",
            "get",
            "devops",
            "API_TOKEN",
            "--reveal",
            "--force",
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), format!("{TOKEN}\n"));
    let last = trace(&tmp).pop().expect("recorded");
    assert_eq!(last["event"], "secret-reveal");
    assert_eq!(last["key_names"], serde_json::json!(["API_TOKEN"]));
    assert_eq!(last["forced"], true);
}

#[test]
fn a_masked_get_records_nothing_because_nothing_left() {
    let tmp = plane();
    let before = trace(&tmp).len();
    let out = run(&tmp, &["secret", "get", "devops", "API_TOKEN"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!text(&out.stdout).contains(TOKEN));
    assert_eq!(trace(&tmp).len(), before);
}

#[test]
fn a_fingerprint_is_keyed_by_the_plane_and_the_key_is_made_0600() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = plane();
    let first = text(&run(&tmp, &["secret", "get", "devops", "API_TOKEN"]).stdout);
    let key = tmp.path().join("plane/.charter/fingerprint.key");
    let meta = std::fs::metadata(&key).expect("the key was made on first use");
    assert_eq!(meta.len(), 32);
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    // Stable within the plane…
    let again = text(&run(&tmp, &["secret", "get", "devops", "API_TOKEN"]).stdout);
    assert_eq!(first, again);
    // …and not a function of the value alone: another key, another fingerprint.
    std::fs::write(&key, [7u8; 32]).expect("another key");
    let other = text(&run(&tmp, &["secret", "get", "devops", "API_TOKEN"]).stdout);
    assert_ne!(first, other);
    assert!(first.contains("16–31 bytes · fp:"), "{first}");
}

#[test]
fn a_streamed_childs_temp_file_is_removed_when_charter_is_terminated() {
    let tmp = plane();
    let marker = tmp.path().join("where");
    let script = format!("echo \"$F\" > '{}'; sleep 30", marker.display());
    let mut child = charter(
        &tmp,
        &[
            "secret",
            "exec",
            "devops",
            "--stream",
            "--file",
            "F=API_TOKEN",
            "--",
            "sh",
            "-c",
            &script,
        ],
    )
    .spawn()
    .expect("the binary runs");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let path = loop {
        if let Ok(text) = std::fs::read_to_string(&marker)
            && text.ends_with('\n')
        {
            break PathBuf::from(text.trim_end());
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the child never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(path.exists(), "the temp file exists while the child runs");
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    // Only the process this test started, by its own id.
    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(killed.success());
    let status = child.wait().expect("charter exits");
    assert_eq!(
        status.code(),
        Some(128 + 15),
        "died as a SIGTERM, not as a success"
    );
    assert!(
        !path.exists(),
        "the 0600 credential file survived charter's SIGTERM: {}",
        path.display()
    );
}

#[test]
fn the_guard_still_refuses_a_vault_read_wrapped_in_secret_exec() {
    let tmp = plane();
    let root = tmp.path().join("plane");
    let ask = |command: &str| {
        let payload = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {"command": command},
            "cwd": root,
            "session_id": "secrets-test",
        });
        let mut child = charter(&tmp, &["hook", "pretooluse"])
            .stdin(Stdio::piped())
            .spawn()
            .expect("the binary runs");
        child
            .stdin
            .take()
            .expect("a pipe")
            .write_all(payload.to_string().as_bytes())
            .expect("the payload");
        let out = child.wait_with_output().expect("the hook answers");
        text(&out.stdout)
    };
    for wrapped in [
        "charter secret exec devops -- cat .charter/vaults/devops.json",
        "charter secret exec devops --env T=API_TOKEN -- env cat .charter/vaults/devops.json",
        "charter persona secret exec --persona devops -- cat .charter/vaults/devops.json",
        "charter secret exec devops cat .charter/vaults/devops.json",
    ] {
        let said = ask(wrapped);
        assert!(
            said.contains("\"deny\"") && said.contains("reads a vault/secret file directly"),
            "{wrapped:?} was not refused: {said}"
        );
    }
    for allowed in [
        "charter secret exec devops --env T=API_TOKEN -- kubectl get pods",
        "charter secret list devops",
    ] {
        let said = ask(allowed);
        assert!(
            !said.contains("\"deny\""),
            "{allowed:?} was refused: {said}"
        );
    }
}

#[test]
fn an_unset_identity_is_refused_before_op_is_ever_run() {
    let tmp = plane();
    // An `op` that would betray being run at all.
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    let op = bin.join("op");
    std::fs::write(&op, "#!/bin/sh\necho ran >> \"$HOME/op-ran\"\n").expect("a stub");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&op, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let out = charter(&tmp, &["secret", "list", "qa"])
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("OP_SERVICE_ACCOUNT_TOKEN", "ambient-identity")
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("is read through $OP_TEST_QA, which is unset"));
    assert!(
        !tmp.path().join("op-ran").exists(),
        "op ran under an undeclared identity"
    );
    assert!(!text(&out.stderr).contains("ambient-identity"));
}

#[test]
fn cp_materialises_the_value_at_0600_and_records_where() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = plane();
    let dest = tmp.path().join("kubeconfig");
    let out = run(
        &tmp,
        &[
            "secret",
            "cp",
            "devops",
            "KUBECONFIG",
            &dest.to_string_lossy(),
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!text(&out.stdout).contains("kind: Config"));
    assert!(!text(&out.stderr).contains("kind: Config"));
    assert_eq!(
        std::fs::read_to_string(&dest).expect("written"),
        "kind: Config\n"
    );
    let mode = std::fs::metadata(&dest).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let last = trace(&tmp).pop().expect("recorded");
    assert_eq!(last["event"], "secret-cp");
    assert_eq!(last["dest"], dest.to_string_lossy().as_ref());
    assert_eq!(last["overwrote"], false);
}

#[test]
fn set_writes_the_value_and_its_date_at_0600() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = plane();
    let mut child = charter(&tmp, &["secret", "set", "devops", "NEW", "--stdin"])
        .stdin(Stdio::piped())
        .spawn()
        .expect("runs");
    child
        .stdin
        .take()
        .expect("a pipe")
        .write_all(b"new-test-value\n")
        .expect("written");
    let out = child.wait_with_output().expect("done");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let vaults = tmp.path().join("plane/.charter/vaults");
    let data: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(vaults.join("devops.json")).unwrap())
            .unwrap();
    assert_eq!(
        data["NEW"], "new-test-value",
        "one trailing newline is stripped"
    );
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(vaults.join("devops.meta.json")).expect("a sidecar"),
    )
    .unwrap();
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    assert_eq!(meta["NEW"]["set_at"], today.as_str());
    for f in ["devops.json", "devops.meta.json"] {
        let mode = std::fs::metadata(vaults.join(f))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "{f}");
    }
}
