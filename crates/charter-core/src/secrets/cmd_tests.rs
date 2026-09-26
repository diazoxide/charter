//! `charter secret …` read back through a recording [`Io`], against planes in temp dirs.
//!
//! Every vault here is a plain-file vault in a temp dir, or a reference vault whose `op` is a
//! shell script on a temp `PATH`: nothing reaches a keychain, 1Password, a clipboard or a live
//! plane.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::*;
use crate::secrets::{Ctx, Env};

/// What a command said, printed and was asked, with the terminals it was told it has.
#[derive(Default)]
pub(crate) struct Rec {
    pub said: Vec<Say>,
    pub out: Vec<u8>,
    pub err: Vec<u8>,
    pub tty_out: bool,
    pub tty_in: bool,
    pub stdin: String,
    pub hidden: String,
    pub prompts: Vec<String>,
}

impl Rec {
    pub fn out(&self) -> String {
        String::from_utf8_lossy(&self.out).into_owned()
    }

    /// Every line said, in order, as `kind: text`.
    pub fn said(&self) -> String {
        self.said
            .iter()
            .map(|s| match s {
                Say::Info(t) => format!("info: {t}"),
                Say::Ok(t) => format!("ok: {t}"),
                Say::Warn(t) => format!("warn: {t}"),
                Say::Err(t) => format!("err: {t}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
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
        self.tty_out
    }
    fn stdin_is_terminal(&self) -> bool {
        self.tty_in
    }
    fn read_stdin(&mut self) -> String {
        std::mem::take(&mut self.stdin)
    }
    fn read_hidden(&mut self, prompt: &str) -> String {
        self.prompts.push(prompt.to_string());
        std::mem::take(&mut self.hidden)
    }
}

/// A plane in a temp dir with its local registry holding `vaults`.
pub(crate) struct Plane {
    pub dir: tempfile::TempDir,
    pub ctx: Ctx,
}

impl Plane {
    pub fn new(env: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx::new(dir.path(), Env::of(env));
        Self { dir, ctx }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Register `name` in the local half, as `vault add` would write it.
    pub fn register(&self, name: &str, provider: &str, config: Value, persona: Option<&str>) {
        let path = self.ctx.local_registry();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut doc: Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| json!({"vaults": {}}));
        doc["vaults"][name] = json!({"provider": provider, "persona": persona, "config": config});
        std::fs::write(&path, doc.to_string()).unwrap();
    }

    /// A plain-file vault `name` whose file holds `secrets`.
    pub fn plain(&self, name: &str, secrets: Value) -> PathBuf {
        let file = self.ctx.vaults_dir().join(format!("{name}.json"));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, secrets.to_string()).unwrap();
        self.register(
            name,
            "plain-file",
            json!({"file": file.to_string_lossy()}),
            None,
        );
        file
    }

    /// Every trace event written under this plane, parsed.
    pub fn trace(&self) -> Vec<Value> {
        let dir = self.root().join(".charter/persona-state/trace");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            out.extend(
                text.lines()
                    .map(|l| serde_json::from_str::<Value>(l).unwrap()),
            );
        }
        out
    }

    pub fn persona(&self, name: &str, frontmatter: &str) {
        let dir = self.root().join("personas");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{name}.md")),
            format!("---\n{frontmatter}\n---\nA persona.\n"),
        )
        .unwrap();
    }
}

/// A directory holding an executable `op` that prints `value` for any `op read`.
pub(crate) fn fake_op(value: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let op = bin.path().join("op");
    std::fs::write(&op, format!("#!/bin/sh\nprintf '%s\\n' '{value}'\n")).unwrap();
    std::fs::set_permissions(&op, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn git(root: &Path, args: &[&str]) {
    let mut c = std::process::Command::new("git");
    c.arg("-C").arg(root).args(args);
    assert!(crate::forklock::output(&mut c).unwrap().status.success());
}

// ---------------------------------------------------------------------------------------
// The provider, whichever it is.

#[test]
fn a_plain_file_vault_is_read_listed_checked_and_deleted_through_its_own_provider() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"b": "two", "a": "one"}));
    let v = provider(&plane.ctx, "p").unwrap();
    assert_eq!(get_value(&plane.ctx, &v, "a").unwrap(), "one");
    assert_eq!(keys(&plane.ctx, &v).unwrap(), ["a", "b"]);
    let (ok, detail) = health(&plane.ctx, &v);
    assert!(ok && detail.starts_with("2 secret(s)"), "{detail}");
    delete(&plane.ctx, &v, "a").unwrap();
    assert_eq!(keys(&plane.ctx, &v).unwrap(), ["b"]);
}

#[test]
fn a_reference_vault_resolves_through_the_cli_its_uri_names() {
    let bin = fake_op("resolved-token");
    let path = bin.path().to_string_lossy().into_owned();
    let plane = Plane::new(&[("PATH", &path)]);
    let file = plane.ctx.vaults_dir().join("r.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, r#"{"tok": "op://Eng/deploy/token"}"#).unwrap();
    plane.register(
        "r",
        "reference",
        json!({"file": file.to_string_lossy()}),
        None,
    );
    let v = provider(&plane.ctx, "r").unwrap();
    assert_eq!(get_value(&plane.ctx, &v, "tok").unwrap(), "resolved-token");
    assert_eq!(keys(&plane.ctx, &v).unwrap(), ["tok"]);
    let (ok, detail) = health(&plane.ctx, &v);
    assert!(
        ok && detail.starts_with("1 reference(s) via op"),
        "{detail}"
    );
    set_value(&plane.ctx, &v, "other", "op://Eng/x/y").unwrap();
    delete(&plane.ctx, &v, "tok").unwrap();
    assert_eq!(keys(&plane.ctx, &v).unwrap(), ["other"]);
}

#[test]
fn a_keyring_vaults_health_is_read_from_its_index_and_not_asked_of_another_provider() {
    let plane = Plane::new(&[]);
    plane.register("k", "keyring", json!({}), None);
    let v = provider(&plane.ctx, "k").unwrap();
    assert_eq!(
        health(&plane.ctx, &v),
        (true, "no secrets yet in the system keyring".to_string())
    );
}

// ---------------------------------------------------------------------------------------
// The access record.

#[test]
fn a_recorded_field_has_every_resolved_value_masked_at_any_depth_and_in_keys() {
    let plane = Plane::new(&[]);
    let values = vec!["s3cr3t".to_string()];
    trace_secret_use(
        &plane.ctx,
        "secret-test",
        &values,
        &[
            ("vault", json!("v-s3cr3t")),
            ("nested", json!({"k-s3cr3t": ["x s3cr3t", 7, true, null]})),
        ],
    );
    let events = plane.trace();
    assert_eq!(events.len(), 1, "{events:?}");
    let e = &events[0];
    assert_eq!(e["event"], "secret-test");
    assert_eq!(e["vault"], "v-***");
    assert_eq!(e["nested"], json!({"k-***": ["x ***", 7, true, null]}));
    assert!(!e.to_string().contains("s3cr3t"));
}

// ---------------------------------------------------------------------------------------
// list / get / audit / set / rm

#[test]
fn list_prints_the_key_names_one_a_line_and_never_a_value() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"b": "value-b", "a": "value-a"}));
    let mut io = Rec::default();
    assert_eq!(list(&plane.ctx, "p", &mut io), 0);
    assert_eq!(io.out(), "a\nb\n");
    assert!(io.said.is_empty(), "{}", io.said());
}

#[test]
fn list_of_an_empty_vault_says_so_on_stderr_and_prints_nothing() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({}));
    let mut io = Rec::default();
    assert_eq!(list(&plane.ctx, "p", &mut io), 0);
    assert_eq!(io.out(), "");
    assert_eq!(io.said, [Say::Info("Vault 'p' has no secrets.".into())]);
}

#[test]
fn list_of_a_vault_that_is_not_registered_fails() {
    let plane = Plane::new(&[]);
    let mut io = Rec::default();
    assert_eq!(list(&plane.ctx, "nope", &mut io), 1);
    assert!(matches!(io.said.as_slice(), [Say::Err(_)]), "{}", io.said());
}

#[test]
fn get_without_reveal_prints_a_masked_shape_and_records_nothing() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"k": "hunter2-value"}));
    let mut io = Rec::default();
    assert_eq!(get(&plane.ctx, "p", "k", false, false, &mut io), 0);
    let out = io.out();
    assert!(out.starts_with("p/k: present · "), "{out}");
    assert!(out.contains("1–15 bytes"), "{out}");
    assert!(!out.contains("hunter2-value"), "{out}");
    assert!(plane.trace().is_empty());
}

#[test]
fn get_of_a_missing_secret_fails_and_prints_nothing() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({}));
    let mut io = Rec::default();
    assert_eq!(get(&plane.ctx, "p", "k", true, true, &mut io), 1);
    assert_eq!(io.out(), "");
    assert!(matches!(io.said.as_slice(), [Say::Err(_)]), "{}", io.said());
}

#[test]
fn reveal_refuses_a_stdout_that_is_not_a_terminal_unless_forced() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"k": "hunter2-value"}));
    let mut io = Rec::default();
    assert_eq!(get(&plane.ctx, "p", "k", true, false, &mut io), 2);
    assert_eq!(io.out(), "");
    assert!(
        io.said().contains("Refusing to print a secret"),
        "{}",
        io.said()
    );
    assert!(
        plane.trace().is_empty(),
        "nothing left, so nothing is recorded"
    );

    let mut forced = Rec::default();
    assert_eq!(get(&plane.ctx, "p", "k", true, true, &mut forced), 0);
    assert_eq!(forced.out(), "hunter2-value\n");
    let events = plane.trace();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event"], "secret-reveal");
    assert_eq!(events[0]["forced"], true);
    assert_eq!(events[0]["key_names"], json!(["k"]));
}

#[test]
fn reveal_to_a_terminal_prints_the_value_once_newline_terminated_and_records_it() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"k": "line\n", "j": "bare"}));
    let mut io = Rec {
        tty_out: true,
        ..Default::default()
    };
    assert_eq!(get(&plane.ctx, "p", "k", true, false, &mut io), 0);
    assert_eq!(get(&plane.ctx, "p", "j", true, false, &mut io), 0);
    assert_eq!(io.out(), "line\nbare\n");
    assert!(io.said().contains("warn: Revealing secret plaintext"));
    let events = plane.trace();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["forced"], false);
    assert_eq!(events[0]["vault"], "p");
}

/// Today less `days`, as the plain-file sidecar records a set.
fn days_ago(days: i64) -> String {
    (chrono::Local::now().date_naive() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

#[test]
fn audit_warns_of_each_secret_at_least_the_threshold_old_oldest_first() {
    let plane = Plane::new(&[]);
    let file = plane.plain("p", json!({"new": "1", "edge": "2", "old": "3"}));
    std::fs::write(
        file.with_file_name("p.meta.json"),
        json!({
            "new": {"set_at": days_ago(3)},
            "edge": {"set_at": days_ago(30)},
            "old": {"set_at": days_ago(400)},
        })
        .to_string(),
    )
    .unwrap();
    let mut io = Rec::default();
    assert_eq!(audit(&plane.ctx, "p", 30, &mut io), 1);
    assert_eq!(
        io.said,
        [
            Say::Warn("p/old: 400 days old — consider rotating".into()),
            Say::Warn("p/edge: 30 days old — consider rotating".into()),
        ]
    );
}

#[test]
fn audit_with_nothing_stale_says_so_and_names_the_keys_it_cannot_date() {
    let plane = Plane::new(&[]);
    let file = plane.plain("p", json!({"new": "1", "b": "2", "a": "3"}));
    std::fs::write(
        file.with_file_name("p.meta.json"),
        json!({"new": {"set_at": days_ago(1)}}).to_string(),
    )
    .unwrap();
    let mut io = Rec::default();
    assert_eq!(audit(&plane.ctx, "p", 90, &mut io), 0);
    assert_eq!(
        io.said,
        [
            Say::Ok("no secrets in 'p' older than 90 days.".into()),
            Say::Info("age unknown (set before tracking): a, b".into()),
        ]
    );
}

#[test]
fn audit_of_a_fully_dated_vault_says_nothing_about_unknown_ages() {
    let plane = Plane::new(&[]);
    let file = plane.plain("p", json!({"k": "1"}));
    std::fs::write(
        file.with_file_name("p.meta.json"),
        json!({"k": {"set_at": days_ago(0)}}).to_string(),
    )
    .unwrap();
    let mut io = Rec::default();
    assert_eq!(audit(&plane.ctx, "p", 90, &mut io), 0);
    assert_eq!(
        io.said,
        [Say::Ok("no secrets in 'p' older than 90 days.".into())]
    );
}

#[test]
fn audit_dates_a_keyring_vaults_secrets_from_its_keys_index() {
    let plane = Plane::new(&[]);
    plane.register("k", "keyring", json!({}), None);
    let v = provider(&plane.ctx, "k").unwrap();
    let store = crate::secrets::keyring::store(&plane.ctx);
    let old = format!("{}T00:00:00Z", days_ago(400));
    crate::secrets::keyring::set_with(&*store, &plane.ctx, &v, "old", "1", &old).unwrap();
    let mut io = Rec::default();
    assert_eq!(audit(&plane.ctx, "k", 30, &mut io), 1);
    assert_eq!(
        io.said,
        [Say::Warn("k/old: 400 days old — consider rotating".into())]
    );
}

#[test]
fn audit_leaves_a_vault_that_rotates_elsewhere_alone() {
    let plane = Plane::new(&[]);
    let file = plane.ctx.vaults_dir().join("r.json");
    plane.register(
        "r",
        "reference",
        json!({"file": file.to_string_lossy()}),
        None,
    );
    let mut io = Rec::default();
    assert_eq!(audit(&plane.ctx, "r", 1, &mut io), 0);
    assert_eq!(
        io.said,
        [Say::Info(
            "vault 'r' (reference) manages rotation externally — no age tracking.".into()
        )]
    );
}

#[test]
fn audit_of_an_empty_vault_or_an_unknown_one() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({}));
    let mut io = Rec::default();
    assert_eq!(audit(&plane.ctx, "p", 1, &mut io), 0);
    assert_eq!(io.said, [Say::Info("vault 'p' has no secrets.".into())]);
    let mut io = Rec::default();
    assert_eq!(audit(&plane.ctx, "nope", 1, &mut io), 1);
}

#[test]
fn set_from_shows_where_a_value_comes_from_but_never_the_value() {
    let from = SetFrom {
        stdin: true,
        from_file: Some("f.txt".into()),
        value: Some("v".into()),
        allow_empty: true,
    };
    assert_eq!(
        format!("{from:?}"),
        r#"SetFrom { stdin: true, from_file: Some("f.txt"), value: Some("***"), allow_empty: true }"#
    );
}

/// `set` into a fresh plain-file vault `p`, then the value it stored.
fn set_and_read(from: &SetFrom, io: &mut Rec) -> (i32, Option<String>) {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({}));
    let code = set(&plane.ctx, "p", "k", from, io);
    let v = provider(&plane.ctx, "p").unwrap();
    (code, get_value(&plane.ctx, &v, "k").ok())
}

#[test]
fn set_takes_a_file_whole() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("value.txt");
    std::fs::write(&f, "from-file\n").unwrap();
    let from = SetFrom {
        from_file: Some(f.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let mut io = Rec::default();
    assert_eq!(
        set_and_read(&from, &mut io),
        (0, Some("from-file\n".into()))
    );
    assert_eq!(
        io.said,
        [Say::Ok(
            "Set 'k' in vault 'p' (1–15 bytes). Value not shown.".into()
        )]
    );
}

#[test]
fn set_from_an_unreadable_file_fails_without_storing() {
    let from = SetFrom {
        from_file: Some("/nonexistent/charter-value".into()),
        ..Default::default()
    };
    let mut io = Rec::default();
    assert_eq!(set_and_read(&from, &mut io), (1, None));
    assert!(io.said().contains("cannot read /nonexistent/charter-value"));
}

#[test]
fn set_takes_an_inline_value_with_a_warning() {
    let from = SetFrom {
        value: Some("inline".into()),
        ..Default::default()
    };
    let mut io = Rec::default();
    assert_eq!(set_and_read(&from, &mut io), (0, Some("inline".into())));
    assert!(io.said().starts_with("warn: Value passed via --value"));
}

#[test]
fn set_reads_a_pipe_without_being_asked_and_drops_one_trailing_newline() {
    let mut io = Rec {
        stdin: "piped\n\n".into(),
        hidden: "typed".into(),
        ..Default::default()
    };
    assert_eq!(
        set_and_read(&SetFrom::default(), &mut io),
        (0, Some("piped\n".into()))
    );
    assert!(io.prompts.is_empty());
}

#[test]
fn set_reads_stdin_when_asked_even_from_a_terminal() {
    let from = SetFrom {
        stdin: true,
        ..Default::default()
    };
    let mut io = Rec {
        tty_in: true,
        stdin: "asked".into(),
        hidden: "typed".into(),
        ..Default::default()
    };
    assert_eq!(set_and_read(&from, &mut io), (0, Some("asked".into())));
    assert!(io.prompts.is_empty());
}

#[test]
fn set_at_a_terminal_prompts_with_echo_off() {
    let mut io = Rec {
        tty_in: true,
        stdin: "not-this".into(),
        hidden: "typed".into(),
        ..Default::default()
    };
    assert_eq!(
        set_and_read(&SetFrom::default(), &mut io),
        (0, Some("typed".into()))
    );
    assert_eq!(io.prompts, ["Value for 'k' (hidden): "]);
}

#[test]
fn set_refuses_an_empty_value_and_says_an_empty_pipe_is_why() {
    let mut io = Rec::default();
    assert_eq!(set_and_read(&SetFrom::default(), &mut io), (1, None));
    let said = io.said();
    assert!(
        said.contains("refusing to store an empty value for 'k'"),
        "{said}"
    );
    assert!(said.contains("Nothing arrived on stdin"), "{said}");

    let mut tty = Rec {
        tty_in: true,
        ..Default::default()
    };
    assert_eq!(set_and_read(&SetFrom::default(), &mut tty), (1, None));
    assert!(!tty.said().contains("Nothing arrived on stdin"));
}

#[test]
fn set_stores_an_empty_value_when_it_is_allowed() {
    let from = SetFrom {
        allow_empty: true,
        ..Default::default()
    };
    let mut io = Rec::default();
    assert_eq!(set_and_read(&from, &mut io), (0, Some(String::new())));
}

#[test]
fn set_refuses_a_plain_file_vault_git_would_commit_but_not_a_reference_one() {
    let plane = Plane::new(&[]);
    git(plane.root(), &["init", "-q"]);
    for (name, provider) in [("p", "plain-file"), ("r", "reference")] {
        plane.register(
            name,
            provider,
            json!({"file": format!("{name}.json")}),
            None,
        );
    }
    let from = SetFrom {
        value: Some("op://Eng/item/field".into()),
        ..Default::default()
    };
    let mut io = Rec::default();
    assert_eq!(set(&plane.ctx, "p", "k", &from, &mut io), 1);
    assert!(
        io.said()
            .contains("refusing to write: 'p.json' is inside the control plane"),
        "{}",
        io.said()
    );
    assert!(!plane.root().join("p.json").exists());

    let mut io = Rec::default();
    assert_eq!(
        set(&plane.ctx, "r", "k", &from, &mut io),
        0,
        "{}",
        io.said()
    );
    assert!(plane.root().join("r.json").exists());
}

#[test]
fn rm_removes_the_key_and_fails_on_one_that_is_not_there() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"k": "v"}));
    let mut io = Rec::default();
    assert_eq!(rm(&plane.ctx, "p", "k", &mut io), 0);
    assert_eq!(io.said, [Say::Ok("Removed 'k' from vault 'p'.".into())]);
    let mut io = Rec::default();
    assert_eq!(rm(&plane.ctx, "p", "k", &mut io), 1);
    assert!(matches!(io.said.as_slice(), [Say::Err(_)]));
}

// ---------------------------------------------------------------------------------------
// persona secret

#[test]
fn a_persona_s_declared_vault_is_its_vault_when_registered() {
    let plane = Plane::new(&[]);
    plane.plain("team", json!({}));
    plane.persona("ops", "vault: team");
    assert_eq!(persona_vault(&plane.ctx, "ops"), Ok("team".into()));
}

#[test]
fn a_declared_vault_wins_over_one_tagged_for_the_persona() {
    let plane = Plane::new(&[]);
    plane.plain("team", json!({}));
    plane.register(
        "tagged",
        "plain-file",
        json!({"file": "t.json"}),
        Some("ops"),
    );
    plane.persona("ops", "vault: team");
    assert_eq!(persona_vault(&plane.ctx, "ops"), Ok("team".into()));
}

#[test]
fn a_declared_vault_that_is_not_registered_here_says_how_to_create_it() {
    let plane = Plane::new(&[]);
    plane.persona("ops", "vault: team");
    let err = persona_vault(&plane.ctx, "ops").unwrap_err();
    assert!(
        err.starts_with("persona 'ops' vault 'team' isn't set up on this machine"),
        "{err}"
    );
}

#[test]
fn a_persona_that_declares_no_vault_is_told_so_rather_than_that_it_has_none() {
    let plane = Plane::new(&[]);
    plane.register("none", "plain-file", json!({"file": "n.json"}), Some("ops"));
    plane.persona("ops", "vault: none");
    let err = persona_vault(&plane.ctx, "ops").unwrap_err();
    assert!(
        err.starts_with("persona 'ops' declares `vault: none`"),
        "{err}"
    );
}

#[test]
fn a_persona_with_no_declaration_takes_the_vault_tagged_for_it() {
    let plane = Plane::new(&[]);
    plane.register("b", "plain-file", json!({"file": "b.json"}), Some("ops"));
    plane.register("a", "plain-file", json!({"file": "a.json"}), Some("ops"));
    plane.persona("ops", "role: x");
    assert_eq!(persona_vault(&plane.ctx, "ops"), Ok("a".into()));
}

#[test]
fn a_persona_with_no_vault_anywhere_is_told_how_to_add_one() {
    let plane = Plane::new(&[]);
    plane.persona("ops", "role: x");
    let err = persona_vault(&plane.ctx, "ops").unwrap_err();
    assert!(err.starts_with("persona 'ops' has no vault."), "{err}");
}

#[test]
fn a_vault_is_inherited_up_extends_and_the_nearest_non_empty_one_wins() {
    let plane = Plane::new(&[]);
    plane.plain("base", json!({}));
    plane.plain("mid", json!({}));
    plane.persona("root", "vault: base");
    plane.persona("child", "extends: root\nvault:");
    plane.persona("grandchild", "extends: mid-p");
    plane.persona("mid-p", "extends: root\nvault: mid");
    assert_eq!(persona_vault(&plane.ctx, "child"), Ok("base".into()));
    assert_eq!(persona_vault(&plane.ctx, "grandchild"), Ok("mid".into()));
}

// ---------------------------------------------------------------------------------------
// cp

/// A plane with a plain-file vault `p` holding `k = "the-plaintext"`.
fn cp_plane() -> Plane {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"k": "the-plaintext"}));
    plane
}

fn mode(p: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[test]
fn cp_writes_a_new_file_at_0600_and_records_that_it_did() {
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("token");
    let shown = dest.to_string_lossy().into_owned();
    let mut io = Rec::default();
    assert_eq!(cp(&plane.ctx, "p", "k", &shown, false, &mut io), 0);
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "the-plaintext");
    assert_eq!(mode(&dest), 0o600);
    assert_eq!(
        io.said,
        [Say::Ok(format!(
            "Wrote 'p/k' to {shown} (0600). Value not shown."
        ))]
    );
    let events = plane.trace();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event"], "secret-cp");
    assert_eq!(events[0]["overwrote"], false);
    assert_eq!(events[0]["dest"], shown.as_str());
}

#[test]
fn cp_with_force_to_a_new_file_is_not_an_overwrite() {
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("token");
    let mut io = Rec::default();
    assert_eq!(
        cp(&plane.ctx, "p", "k", &dest.to_string_lossy(), true, &mut io),
        0
    );
    assert!(matches!(io.said.as_slice(), [Say::Ok(_)]), "{}", io.said());
    assert_eq!(plane.trace()[0]["overwrote"], false);
}

#[test]
fn cp_refuses_an_existing_file_unless_forced_and_then_says_it_overwrote_it() {
    use std::os::unix::fs::PermissionsExt;
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("token");
    std::fs::write(&dest, "keep me").unwrap();
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o644)).unwrap();
    let shown = dest.to_string_lossy().into_owned();

    let mut io = Rec::default();
    assert_eq!(cp(&plane.ctx, "p", "k", &shown, false, &mut io), 2);
    assert!(io.said().contains("already exists"), "{}", io.said());
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "keep me");
    assert!(plane.trace().is_empty());

    let mut io = Rec::default();
    assert_eq!(cp(&plane.ctx, "p", "k", &shown, true, &mut io), 0);
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "the-plaintext");
    assert_eq!(mode(&dest), 0o600);
    assert_eq!(
        io.said[0],
        Say::Warn(format!("Overwrote {shown} and set it to 0600."))
    );
    assert_eq!(plane.trace()[0]["overwrote"], true);
}

#[test]
fn cp_refuses_an_empty_destination() {
    let plane = cp_plane();
    let mut io = Rec::default();
    assert_eq!(cp(&plane.ctx, "p", "k", "", true, &mut io), 2);
    assert_eq!(
        io.said,
        [Say::Err(
            "Refusing to write a secret: the destination path is empty.".into()
        )]
    );
}

#[test]
fn cp_refuses_a_symlink_even_forced_and_leaves_its_target_alone() {
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("target");
    std::fs::write(&target, "t").unwrap();
    let link = out.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut io = Rec::default();
    assert_eq!(
        cp(&plane.ctx, "p", "k", &link.to_string_lossy(), true, &mut io),
        2
    );
    assert!(io.said().contains("is a symlink"), "{}", io.said());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "t");
}

#[test]
fn cp_names_what_a_destination_that_is_not_a_file_is() {
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    let fifo = out.path().join("fifo");
    let mut mkfifo = std::process::Command::new("mkfifo");
    mkfifo.arg(&fifo);
    assert!(crate::forklock::status(&mut mkfifo).unwrap().success());
    for (dest, kind) in [
        (out.path().to_path_buf(), "a directory"),
        (fifo, "a FIFO"),
        (PathBuf::from("/dev/null"), "a character device"),
    ] {
        let shown = dest.to_string_lossy().into_owned();
        let mut io = Rec::default();
        assert_eq!(cp(&plane.ctx, "p", "k", &shown, true, &mut io), 2);
        assert!(
            io.said()
                .contains(&format!("{shown} is {kind}, not a regular file")),
            "{}",
            io.said()
        );
    }
}

#[test]
fn cp_says_a_destination_it_cannot_inspect_cannot_be_inspected() {
    // Only a destination that is not there yet is let through to be created; one whose
    // `lstat` fails any other way (here ENOTDIR, a path under a regular file) is refused
    // before anything is opened, and the refusal says why.
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    std::fs::write(out.path().join("plain"), "x").unwrap();
    let dest = out.path().join("plain").join("token");
    let shown = dest.to_string_lossy().into_owned();
    let mut io = Rec::default();
    assert_eq!(cp(&plane.ctx, "p", "k", &shown, true, &mut io), 2);
    assert!(
        io.said()
            .contains(&format!("{shown} cannot be inspected (Not a directory")),
        "{}",
        io.said()
    );
}

#[test]
fn cp_refuses_a_path_inside_the_plane_git_would_commit_but_not_an_ignored_one() {
    let plane = cp_plane();
    git(plane.root(), &["init", "-q"]);
    std::fs::write(plane.root().join(".gitignore"), "/.charter/\n").unwrap();

    let committed = plane.root().join("token");
    let mut io = Rec::default();
    assert_eq!(
        cp(
            &plane.ctx,
            "p",
            "k",
            &committed.to_string_lossy(),
            false,
            &mut io
        ),
        2
    );
    assert!(
        io.said().contains("'token' is inside the control plane"),
        "{}",
        io.said()
    );
    assert!(!committed.exists());

    let ignored = plane.root().join(".charter/token");
    let mut io = Rec::default();
    assert_eq!(
        cp(
            &plane.ctx,
            "p",
            "k",
            &ignored.to_string_lossy(),
            false,
            &mut io
        ),
        0,
        "{}",
        io.said()
    );
    assert_eq!(std::fs::read_to_string(&ignored).unwrap(), "the-plaintext");
}

#[test]
fn cp_makes_one_missing_directory_level_at_0700_and_no_more() {
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    let one = out.path().join("new/token");
    let mut io = Rec::default();
    assert_eq!(
        cp(&plane.ctx, "p", "k", &one.to_string_lossy(), false, &mut io),
        0,
        "{}",
        io.said()
    );
    assert_eq!(mode(one.parent().unwrap()), 0o700);

    let two = out.path().join("a/b/token");
    let mut io = Rec::default();
    assert_eq!(
        cp(&plane.ctx, "p", "k", &two.to_string_lossy(), false, &mut io),
        2
    );
    let parent = two.parent().unwrap().display().to_string();
    assert!(
        io.said().contains(&format!(
            "cannot create {parent} (No such file or directory). charter creates at most one \
             missing directory level"
        )),
        "the OS's own sentence, without Rust's `(os error N)`: {}",
        io.said()
    );
}

/// A parent that is a dangling symlink is not a missing directory: `mkdir` finds the name
/// taken and lets it be, and the open that follows is what refuses.
#[test]
fn cp_under_a_dangling_symlink_is_refused_at_the_open_not_the_mkdir() {
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(out.path().join("gone"), out.path().join("link")).unwrap();
    let dest = out.path().join("link/token");
    let shown = dest.to_string_lossy().into_owned();
    let mut io = Rec::default();
    assert_eq!(cp(&plane.ctx, "p", "k", &shown, false, &mut io), 2);
    assert!(
        io.said().starts_with(&format!(
            "err: Refusing to write a secret: cannot open {shown}"
        )),
        "{}",
        io.said()
    );
    assert!(!out.path().join("gone").exists());
}

#[test]
fn cp_of_a_secret_that_is_not_there_leaves_no_file_behind() {
    let plane = cp_plane();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("token");
    let mut io = Rec::default();
    assert_eq!(
        cp(
            &plane.ctx,
            "p",
            "missing",
            &dest.to_string_lossy(),
            false,
            &mut io
        ),
        1
    );
    assert!(!dest.exists());
    assert!(plane.trace().is_empty());
}

/// Set on the child of [`cp_refuses_charter_s_own_standard_output_by_identity`]: where it
/// writes what `cp` answered.
const OWN_STREAM_PROBE: &str = "SECRETS_CP_OWN_STREAM_PROBE";

/// Set on that child: the regular file its stdout is.
const OWN_STREAM_FILE: &str = "SECRETS_CP_OWN_STREAM_FILE";

/// `cp` to the very file charter's stdout is — by its own name, and (on macOS) by
/// `/dev/fd/1`, which `lstat`s as a regular file on another device and is only known for
/// stdout once it is opened. Run in a child of this test binary whose stdout IS that file,
/// since this process's stdout is whatever the test runner made it.
#[test]
fn cp_refuses_charter_s_own_standard_output_by_identity() {
    if let Some(probe) = std::env::var_os(OWN_STREAM_PROBE) {
        let own = std::env::var(OWN_STREAM_FILE).unwrap();
        let plane = cp_plane();
        let mut report = String::new();
        let mut dests = vec![own.clone()];
        if cfg!(target_os = "macos") {
            dests.push("/dev/fd/1".into());
        }
        for dest in dests {
            let mut io = Rec::default();
            let code = cp(&plane.ctx, "p", "k", &dest, true, &mut io);
            report.push_str(&format!("{code}\t{}\n", io.said()));
        }
        std::fs::write(probe, report).unwrap();
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let own = dir.path().join("stdout");
    let probe = dir.path().join("probe");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "--test-threads=1",
            "secrets::cmd::tests::cp_refuses_charter_s_own_standard_output_by_identity",
        ])
        .env(OWN_STREAM_PROBE, &probe)
        .env(OWN_STREAM_FILE, &own)
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(&own).unwrap());
    assert!(crate::forklock::status(&mut child).unwrap().success());
    let report = std::fs::read_to_string(&probe).unwrap();
    let lines: Vec<&str> = report.lines().collect();
    let expected = if cfg!(target_os = "macos") { 2 } else { 1 };
    assert_eq!(lines.len(), expected, "{report}");
    for line in lines {
        assert!(
            line.starts_with("2\terr: Refusing to write a secret: ")
                && line.contains("is charter's own standard output — the channel"),
            "{report}"
        );
    }
    assert!(
        !std::fs::read_to_string(&own)
            .unwrap()
            .contains("the-plaintext")
    );
}
