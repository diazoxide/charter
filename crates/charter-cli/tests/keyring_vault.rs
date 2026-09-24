//! The `keyring` vault provider through the command line (ADR 0047).
//!
//! **No test here reaches the operating system's keyring.** The binary these tests spawn is a
//! fenced build (`charter-core/fenced`, turned on from `[dev-dependencies]`), and a fenced build
//! keeps a keyring vault's values in `.charter/keyring-stub.json` under the plane it was given —
//! never in the login keychain, the Secret Service or the Credential Manager. The stub is the
//! seam; what is asserted is what the commands say and what they leave on disk.
//!
//! Every value in here is fabricated for the test.

#![cfg(unix)]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const VALUE: &str = "kr-fixture-7c1e0a95d3b2";

/// A copy of the committed `daily` fixture plane with no vault registered.
fn plane() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().expect("a directory");
    copy(&fixture, &dir.path().join("plane"));
    let root = dir.path().join("plane");
    let _ = std::fs::remove_file(root.join(".charter/vaults.json"));
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
        .env("CHARTER_SESSION_ID", "keyring-test")
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

fn run_with_stdin(tmp: &tempfile::TempDir, args: &[&str], stdin: &str) -> Output {
    let mut child = charter(tmp, args)
        .stdin(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .take()
        .expect("a stdin")
        .write_all(stdin.as_bytes())
        .expect("stdin written");
    child.wait_with_output().expect("the binary finishes")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn said(out: &Output) -> String {
    format!("{}{}", text(&out.stdout), text(&out.stderr))
}

#[test]
fn a_vault_registered_without_a_provider_is_a_keyring_vault() {
    let tmp = plane();

    let out = run(&tmp, &["vault", "add", "ops"]);

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(
        text(&out.stderr).contains("Vault 'ops' registered (provider: keyring)"),
        "{}",
        said(&out)
    );
    let registry: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("plane/.charter/vaults.json"))
            .expect("the local registry"),
    )
    .expect("JSON");
    assert_eq!(registry["vaults"]["ops"]["provider"], "keyring");
}

/// `ops`, a keyring vault, holding `API_TOKEN` = [`VALUE`].
fn with_a_secret() -> tempfile::TempDir {
    let tmp = plane();
    let out = run(&tmp, &["vault", "add", "ops"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let out = run_with_stdin(
        &tmp,
        &["secret", "set", "ops", "API_TOKEN", "--stdin"],
        VALUE,
    );
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    tmp
}

fn index(tmp: &tempfile::TempDir) -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("plane/.charter/vaults/ops.keys.json"))
            .expect("the keys index"),
    )
    .expect("JSON")
}

#[test]
fn a_secret_set_is_listed_by_name_and_read_back_masked() {
    let tmp = with_a_secret();

    let list = run(&tmp, &["secret", "list", "ops"]);
    assert_eq!(text(&list.stdout), "API_TOKEN\n", "{}", said(&list));

    let get = run(&tmp, &["secret", "get", "ops", "API_TOKEN"]);
    assert_eq!(get.status.code(), Some(0), "{}", said(&get));
    assert!(
        text(&get.stdout).starts_with("ops/API_TOKEN: present · 16–31 bytes · fp:"),
        "{}",
        said(&get)
    );
    assert!(!said(&get).contains(VALUE));

    let reveal = run(
        &tmp,
        &["secret", "get", "ops", "API_TOKEN", "--reveal", "--force"],
    );
    assert_eq!(
        text(&reveal.stdout),
        format!("{VALUE}\n"),
        "{}",
        said(&reveal)
    );
}

#[test]
fn the_keys_index_names_each_key_its_size_band_and_when_and_never_the_value() {
    let tmp = with_a_secret();

    let path = tmp.path().join("plane/.charter/vaults/ops.keys.json");
    let raw = std::fs::read_to_string(&path).expect("the keys index");
    assert!(!raw.contains(VALUE), "{raw}");
    let doc = index(&tmp);
    let service = doc["service"].as_str().expect("a service");
    assert!(service.starts_with("charter/ops/"), "{service}");
    assert_eq!(doc["keys"]["API_TOKEN"]["size"], "16–31 bytes");
    let updated = doc["keys"]["API_TOKEN"]["updated"]
        .as_str()
        .expect("a time");
    assert!(
        updated.len() == 20 && updated.ends_with('Z') && updated.as_bytes()[10] == b'T',
        "{updated}"
    );
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn a_fenced_build_keeps_the_value_in_the_planes_stub_and_nowhere_else_on_disk() {
    let tmp = with_a_secret();

    let stub = tmp.path().join("plane/.charter/keyring-stub.json");
    assert!(
        std::fs::read_to_string(&stub)
            .expect("the stub keyring")
            .contains(VALUE)
    );
    // The registry says which provider and nothing else.
    let registry = std::fs::read_to_string(tmp.path().join("plane/.charter/vaults.json"))
        .expect("the registry");
    assert!(!registry.contains(VALUE));
}

#[test]
fn rm_deletes_the_item_and_its_line_in_the_index() {
    let tmp = with_a_secret();

    let out = run(&tmp, &["secret", "rm", "ops", "API_TOKEN"]);

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(index(&tmp)["keys"], serde_json::json!({}));
    let stub = std::fs::read_to_string(tmp.path().join("plane/.charter/keyring-stub.json"))
        .expect("the stub keyring");
    assert!(!stub.contains(VALUE), "{stub}");
    let again = run(&tmp, &["secret", "get", "ops", "API_TOKEN"]);
    assert_eq!(again.status.code(), Some(1));
    assert!(
        text(&again.stderr).contains("secret 'API_TOKEN' not found in vault 'ops'"),
        "{}",
        said(&again)
    );
}

#[test]
fn exec_hands_the_value_to_the_child_and_redacts_it_from_what_the_child_printed() {
    let tmp = with_a_secret();

    let out = run(
        &tmp,
        &[
            "secret",
            "exec",
            "ops",
            "--env",
            "T=API_TOKEN",
            "--",
            "sh",
            "-c",
            "echo \"[$T]\"; test \"$T\" = kr-fixture-7c1e0a95d3b2 && echo same",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(text(&out.stdout), "[***]\nsame\n");
}

#[test]
fn cp_materialises_the_value_at_0600() {
    let tmp = with_a_secret();
    let dest = tmp.path().join("token.txt");

    let out = run(
        &tmp,
        &[
            "secret",
            "cp",
            "ops",
            "API_TOKEN",
            dest.to_str().expect("a path"),
        ],
    );

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(std::fs::read_to_string(&dest).expect("the file"), VALUE);
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&dest).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert!(!said(&out).contains(VALUE));
}

#[test]
fn a_personas_keyring_vault_answers_persona_secret() {
    // The fixture's `devops` persona declares `vault: devops`.
    let tmp = plane();
    let out = run(&tmp, &["vault", "add", "devops", "--persona", "devops"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let out = run_with_stdin(
        &tmp,
        &[
            "persona",
            "secret",
            "set",
            "--persona",
            "devops",
            "API_TOKEN",
            "--stdin",
        ],
        VALUE,
    );
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));

    let list = run(&tmp, &["persona", "secret", "list", "--persona", "devops"]);

    assert_eq!(text(&list.stdout), "API_TOKEN\n", "{}", said(&list));
}

#[test]
fn vault_list_counts_from_the_index_and_verify_reads_every_item() {
    let tmp = with_a_secret();

    let list = run(&tmp, &["vault", "list"]);
    assert_eq!(list.status.code(), Some(0), "{}", said(&list));
    let row = text(&list.stdout)
        .lines()
        .find(|l| l.starts_with("ops "))
        .map(str::to_owned)
        .expect("a row for ops");
    assert!(row.contains("keyring"), "{row}");
    assert!(row.contains("1 secret(s) in the system keyring"), "{row}");

    let verify = run(&tmp, &["vault", "verify", "ops"]);
    assert_eq!(verify.status.code(), Some(0), "{}", said(&verify));
    assert!(text(&verify.stderr).contains("ops: 1 reference(s) resolved"));
}

#[test]
fn verify_names_a_key_the_index_holds_and_the_keyring_does_not() {
    let tmp = with_a_secret();
    // The item went out of the keyring behind charter's back — Keychain Access, another tool.
    std::fs::write(tmp.path().join("plane/.charter/keyring-stub.json"), "{}").expect("emptied");

    let verify = run(&tmp, &["vault", "verify", "ops"]);

    assert_eq!(verify.status.code(), Some(1), "{}", said(&verify));
    assert!(
        text(&verify.stdout).contains("API_TOKEN: secret 'API_TOKEN' not found in vault 'ops'"),
        "{}",
        said(&verify)
    );
}

#[test]
fn remove_unregisters_and_leaves_the_items_and_the_index() {
    let tmp = with_a_secret();

    let out = run(&tmp, &["vault", "remove", "ops"]);

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(
        tmp.path()
            .join("plane/.charter/vaults/ops.keys.json")
            .exists()
    );
    // Said, because registering the name again brings the old secrets back.
    assert!(
        text(&out.stderr).contains(
            "Its secrets stay in the system keyring, named in .charter/vaults/ops.keys.json; \
             registering 'ops' again as a keyring vault finds them."
        ),
        "{}",
        said(&out)
    );
    let gone = run(&tmp, &["secret", "list", "ops"]);
    assert_eq!(gone.status.code(), Some(1));
}

#[test]
fn audit_reads_each_keys_age_from_the_index() {
    let tmp = with_a_secret();
    let path = tmp.path().join("plane/.charter/vaults/ops.keys.json");
    let mut doc = index(&tmp);
    doc["keys"]["API_TOKEN"]["updated"] = serde_json::json!("2020-01-01T00:00:00Z");
    std::fs::write(&path, doc.to_string()).expect("aged");

    let out = run(&tmp, &["secret", "audit", "ops", "--days", "30"]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(
        text(&out.stderr).contains("ops/API_TOKEN:") && text(&out.stderr).contains("days old"),
        "{}",
        said(&out)
    );
}

#[test]
fn an_index_that_points_at_another_programs_items_is_refused() {
    let tmp = with_a_secret();
    let path = tmp.path().join("plane/.charter/vaults/ops.keys.json");
    let mut doc = index(&tmp);
    doc["service"] = serde_json::json!("github.com");
    std::fs::write(&path, doc.to_string()).expect("pointed elsewhere");

    let out = run(&tmp, &["secret", "get", "ops", "API_TOKEN"]);

    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(
        text(&out.stderr).contains("not one charter writes"),
        "{}",
        said(&out)
    );
}

#[test]
fn no_error_repeats_a_value_or_a_stored_entry() {
    let tmp = with_a_secret();
    // A stub that is not an object: charter must name the file and never quote it.
    std::fs::write(
        tmp.path().join("plane/.charter/keyring-stub.json"),
        format!("[\"{VALUE}\"]"),
    )
    .expect("broken");

    for args in [
        &["secret", "get", "ops", "API_TOKEN"][..],
        &["secret", "list", "ops"][..],
        &["secret", "rm", "ops", "API_TOKEN"][..],
        &["vault", "verify", "ops"][..],
        &[
            "secret",
            "exec",
            "ops",
            "--env",
            "T=API_TOKEN",
            "--",
            "true",
        ][..],
    ] {
        let out = run(&tmp, args);
        assert!(!said(&out).contains(VALUE), "{args:?}: {}", said(&out));
    }
}
