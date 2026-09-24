//! The 1Password provider against a fake `op`: the argv it is given, the template it is fed on
//! stdin, and what each of its answers means to charter.
//!
//! The fake is a shell script first on a `PATH` this test builds, so the real `op` — and the
//! operator's 1Password — is never reached. Each subcommand answers from files in the fake's
//! directory (`<k>.out`, `<k>.err`, `<k>.code`, for `k` in `read`, `get`, `list`,
//! `listtagged`, `create`, `edit`), and every call is logged to `calls`.

use super::*;
use crate::secrets::{Ctx, Env, Kind};

const FAKE_OP: &str = r#"#!/bin/sh
d="$FAKE_OP_DIR"
printf '%s\n' "$*" >> "$d/calls"
printf '%s\n' "${OP_SERVICE_ACCOUNT_TOKEN-unset}" >> "$d/identity"
case "$1 $2" in
  "read "*) k=read ;;
  "item get") k=get ;;
  "item list") case "$*" in *--tags*) k=listtagged ;; *) k=list ;; esac ;;
  "item create") k=create ;;
  "item edit") k=edit ;;
  *) k=other ;;
esac
case $k in
  create|edit) cat > "$d/$k.stdin"; if [ -f "$d/vanish" ]; then rm -f "$d/bin/op"; fi ;;
esac
if [ -f "$d/$k.out" ]; then cat "$d/$k.out"; fi
if [ -f "$d/$k.err" ]; then cat "$d/$k.err" >&2; fi
exit "$(cat "$d/$k.code" 2>/dev/null || echo 0)"
"#;

/// A plane and a fake `op` in one temp directory.
struct Fake {
    dir: tempfile::TempDir,
    ctx: Ctx,
}

impl Fake {
    fn new(extra_env: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(bin.join("op"), FAKE_OP).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin.join("op"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let d = dir.path().to_string_lossy().into_owned();
        let mut vars = vec![("PATH", path.as_str()), ("FAKE_OP_DIR", d.as_str())];
        vars.extend_from_slice(extra_env);
        let ctx = Ctx::new(dir.path(), Env::of(&vars));
        Self { dir, ctx }
    }

    /// What `op <k>` prints on stdout from now on.
    fn answers(&self, k: &str, out: &str) -> &Self {
        std::fs::write(self.dir.path().join(format!("{k}.out")), out).unwrap();
        self
    }

    /// How `op <k>` exits from now on, and what it says on stderr.
    fn fails(&self, k: &str, code: i32, err: &str) -> &Self {
        std::fs::write(self.dir.path().join(format!("{k}.code")), code.to_string()).unwrap();
        std::fs::write(self.dir.path().join(format!("{k}.err")), err).unwrap();
        self
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The JSON template `op item <k>` was handed on stdin.
    fn template(&self, k: &str) -> Value {
        let text = std::fs::read_to_string(self.dir.path().join(format!("{k}.stdin")))
            .unwrap_or_else(|_| panic!("op item {k} was run"));
        serde_json::from_str(&text).unwrap()
    }
}

fn vault(config: Value) -> Vault {
    Vault {
        name: "team".into(),
        provider: "1password".into(),
        persona: None,
        config: config.as_object().unwrap().clone(),
    }
}

fn eng() -> Vault {
    vault(serde_json::json!({"op-vault": "Eng"}))
}

/// An item with two secrets, a field that holds none, one known only by its id, and one with
/// an empty id.
const ITEM: &str = r#"{"id": "item1", "title": "charter-team", "fields": [
    {"id": "a1", "label": "B", "value": "bee"},
    {"id": "a2", "label": "A", "value": "ay"},
    {"id": "notes", "label": "notesPlain"},
    {"id": "idonly", "label": "", "value": "z"},
    {"id": "nulllabel", "label": null, "value": "w"},
    {"id": "", "label": "C", "value": "sea"}
]}"#;

/// `(label, id, value)` of every field of a template, in its order.
fn fields(template: &Value) -> Vec<(String, String, String)> {
    template["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["label"].as_str().unwrap().to_owned(),
                f["id"].as_str().unwrap().to_owned(),
                f["value"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_value_is_read_with_op_read_of_that_one_field_under_the_vaults_identity() {
    let fake = Fake::new(&[
        ("OP_TEAM", "team-token"),
        ("OP_SERVICE_ACCOUNT_TOKEN", "ambient"),
    ]);
    fake.answers("read", "s3cret\n\n");
    let v = vault(serde_json::json!({
        "op-vault": "Eng",
        "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM"}
    }));
    // One trailing newline is `op`'s; the rest is the value's.
    assert_eq!(get(&fake.ctx, &v, "KEY").unwrap(), "s3cret\n");
    assert_eq!(
        fake.calls(),
        ["read --no-newline op://Eng/charter-team/KEY"]
    );
    assert_eq!(
        std::fs::read_to_string(fake.dir.path().join("identity")).unwrap(),
        "team-token\n"
    );
}

#[test]
fn a_read_op_refuses_is_a_missing_secret_named_by_field_item_and_vault() {
    let fake = Fake::new(&[]);
    fake.fails(
        "read",
        1,
        "[ERROR] could not read secret 'op://Eng/charter-team/KEY'",
    );
    let e = get(&fake.ctx, &eng(), "KEY").unwrap_err();
    assert_eq!(e.kind, Kind::NotFound);
    assert_eq!(
        e.message,
        "no secret 'KEY' in vault 'team' (field 'KEY' of 1Password item 'charter-team' in 'Eng')"
    );
}

#[test]
fn the_item_and_account_come_from_the_registry_stripped_and_an_empty_one_is_unset() {
    let fake = Fake::new(&[]);
    fake.answers("read", "v");
    let pinned = vault(serde_json::json!({
        "op-vault": " Eng ", "op-item": " Custom ", "account": " me.1password.com "
    }));
    get(&fake.ctx, &pinned, "K").unwrap();
    // An empty `op-item` falls back to the legacy spelling; a blank account pins nothing.
    let legacy = vault(serde_json::json!({
        "op-vault": "Eng", "op-item": "", "op_item": "Old", "account": "  "
    }));
    get(&fake.ctx, &legacy, "K").unwrap();
    assert_eq!(
        fake.calls(),
        [
            "read --no-newline op://Eng/Custom/K --account me.1password.com",
            "read --no-newline op://Eng/Old/K",
        ]
    );
}

#[test]
fn a_configured_value_op_would_read_as_a_flag_is_refused_before_op_runs() {
    let fake = Fake::new(&[]);
    for (field, config) in [
        ("op-vault", serde_json::json!({"op-vault": "-Eng"})),
        (
            "op-item",
            serde_json::json!({"op-vault": "Eng", "op-item": "--x"}),
        ),
        (
            "account",
            serde_json::json!({"op-vault": "Eng", "account": "--evil"}),
        ),
    ] {
        let e = get(&fake.ctx, &vault(config), "K").unwrap_err();
        assert!(
            e.message
                .starts_with(&format!("vault 'team' has an {field} that starts with '-'")),
            "{}",
            e.message
        );
    }
    let e = get(&fake.ctx, &vault(serde_json::json!({})), "K").unwrap_err();
    assert!(
        e.message.contains("has no 'op-vault' configured"),
        "{}",
        e.message
    );
    assert_eq!(fake.calls(), Vec::<String>::new(), "op never ran");
}

#[test]
fn the_keys_are_the_items_labelled_fields_that_hold_a_value_falling_back_to_the_id() {
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM);
    assert_eq!(
        keys(&fake.ctx, &eng()).unwrap(),
        ["A", "B", "C", "idonly", "nulllabel"]
    );
    assert_eq!(
        fake.calls(),
        ["item get charter-team --vault Eng --format json"]
    );
}

#[test]
fn two_fields_with_one_label_is_an_error_never_a_silent_winner() {
    let fake = Fake::new(&[]);
    fake.answers(
        "get",
        r#"{"fields": [{"label": "A", "value": "1"}, {"label": "A", "value": "2"}]}"#,
    );
    let e = keys(&fake.ctx, &eng()).unwrap_err();
    assert!(
        e.message
            .starts_with("1Password item 'charter-team' has more than one field labelled 'A'"),
        "{}",
        e.message
    );
}

#[test]
fn an_item_op_cannot_get_is_absent_only_when_the_vaults_listing_lacks_it() {
    let fake = Fake::new(&[]);
    fake.fails("get", 1, "isn't an item");
    fake.answers("list", r#"[{"title": "something-else"}]"#);
    assert_eq!(keys(&fake.ctx, &eng()).unwrap(), Vec::<String>::new());
    assert_eq!(
        fake.calls().last().unwrap(),
        "item list --vault Eng --format json"
    );

    fake.answers("list", r#"[{"title": "charter-team"}]"#);
    let e = keys(&fake.ctx, &eng()).unwrap_err();
    assert!(
        e.message
            .starts_with("reading vault 'team' failed (op exit 1). charter did not recognise"),
        "{}",
        e.message
    );
    assert!(
        !e.message.contains("isn't an item"),
        "op's own words are withheld"
    );
}

#[test]
fn a_failure_charter_recognises_is_explained_and_op_s_words_are_never_repeated() {
    let fake = Fake::new(&[]);
    fake.fails("get", 1, "");
    fake.fails(
        "list",
        1,
        "[ERROR] You do not have permission to list items in s3cret-echo",
    );
    let e = keys(&fake.ctx, &eng()).unwrap_err();
    assert!(
        e.message.starts_with(
            "listing vault 'team' failed (op exit 1). 1Password refused this as unauthorised."
        ),
        "{}",
        e.message
    );
    assert!(!e.message.contains("s3cret-echo"));
}

#[test]
fn an_item_that_is_not_a_json_object_is_refused() {
    let fake = Fake::new(&[]);
    fake.answers("get", "[]");
    let e = keys(&fake.ctx, &eng()).unwrap_err();
    assert!(
        e.message.contains("did not return a JSON object"),
        "{}",
        e.message
    );
}

#[test]
fn setting_one_field_rewrites_the_item_with_its_siblings_and_their_ids_then_reads_it_back() {
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM).answers("read", "new-ay");
    set(&fake.ctx, &eng(), "A", "new-ay").unwrap();
    assert_eq!(
        fake.calls(),
        [
            "item get charter-team --vault Eng --format json --reveal",
            "item edit charter-team --vault Eng",
            "read --no-newline op://Eng/charter-team/A",
        ]
    );
    let t = fake.template("edit");
    assert_eq!(t["title"], "charter-team");
    assert_eq!(t["category"], "PASSWORD");
    assert_eq!(t["tags"], serde_json::json!(["charter", "charter:team"]));
    // Sorted by name; a field with an id keeps it, one without is given its label.
    assert_eq!(
        fields(&t),
        [
            ("A".into(), "a2".into(), "new-ay".into()),
            ("B".into(), "a1".into(), "bee".into()),
            ("C".into(), "C".into(), "sea".into()),
            ("idonly".into(), "idonly".into(), "z".into()),
            ("nulllabel".into(), "nulllabel".into(), "w".into()),
        ]
    );
}

#[test]
fn setting_a_field_of_an_item_that_is_not_there_creates_it() {
    let fake = Fake::new(&[]);
    fake.fails("get", 1, "")
        .answers("list", "[]")
        .answers("read", "v");
    set(&fake.ctx, &eng(), "NEW", "v").unwrap();
    assert!(
        fake.calls()
            .contains(&"item create - --vault Eng".to_string()),
        "{:?}",
        fake.calls()
    );
    assert_eq!(
        fields(&fake.template("create")),
        [("NEW".into(), "NEW".into(), "v".into())]
    );
}

#[test]
fn a_read_back_that_differs_from_what_was_written_is_an_error() {
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM).answers("read", "someone-elses");
    let e = set(&fake.ctx, &eng(), "A", "new-ay").unwrap_err();
    assert!(
        e.message
            .contains("reading it back did not return what was written"),
        "{}",
        e.message
    );
}

#[test]
fn a_read_back_that_fails_says_whether_op_read_merely_exited_non_zero() {
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM).fails("read", 1, "");
    let e = set(&fake.ctx, &eng(), "A", "x").unwrap_err();
    assert!(
        e.message.contains("could not read it back"),
        "{}",
        e.message
    );
    assert!(
        e.message.contains("`op read` exited non-zero"),
        "{}",
        e.message
    );

    // `op` gone from PATH between the write and the read: that failure is named as it is.
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM);
    std::fs::write(fake.dir.path().join("vanish"), "").unwrap();
    let e = set(&fake.ctx, &eng(), "A", "x").unwrap_err();
    assert!(
        e.message.contains("could not read it back"),
        "{}",
        e.message
    );
    assert!(e.message.contains("('op') is not on PATH"), "{}", e.message);
    assert!(!e.message.contains("exited non-zero"), "{}", e.message);
}

#[test]
fn a_write_op_refuses_is_an_error_with_its_recognised_cause() {
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM)
        .answers("read", "x")
        .fails("edit", 1, "[ERROR] rate-limited");
    let e = set(&fake.ctx, &eng(), "A", "x").unwrap_err();
    assert!(
        e.message.starts_with(
            "updating 1Password item 'charter-team' failed (op exit 1). 1Password rate-limited"
        ),
        "{}",
        e.message
    );
    assert!(
        !fake.calls().iter().any(|c| c.starts_with("read")),
        "no read-back"
    );
}

#[test]
fn deleting_a_field_rewrites_the_item_without_it_and_a_missing_one_is_not_found() {
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM);
    delete(&fake.ctx, &eng(), "A").unwrap();
    let names: Vec<String> = fields(&fake.template("edit"))
        .into_iter()
        .map(|(n, _, _)| n)
        .collect();
    assert_eq!(names, ["B", "C", "idonly", "nulllabel"]);

    let fake = Fake::new(&[]);
    fake.answers("get", ITEM);
    let e = delete(&fake.ctx, &eng(), "Z").unwrap_err();
    assert_eq!(e.kind, Kind::NotFound);
    assert_eq!(e.message, "no secret 'Z' in vault 'team'");
    assert!(
        !fake.calls().iter().any(|c| c.contains("edit")),
        "nothing written"
    );
}

#[test]
fn a_delete_op_refuses_is_an_error() {
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM).fails("edit", 1, "");
    let e = delete(&fake.ctx, &eng(), "A").unwrap_err();
    assert!(
        e.message
            .starts_with("updating 1Password item 'charter-team' failed (op exit 1)."),
        "{}",
        e.message
    );
}

#[test]
fn health_counts_the_secrets_and_never_reads_one() {
    let fake = Fake::new(&[]);
    fake.answers("get", ITEM);
    assert_eq!(
        health(&fake.ctx, &eng()),
        (true, "5 secret(s) in 1Password item 'charter-team'".into())
    );
    assert!(!fake.calls().iter().any(|c| c.starts_with("read")));
}

#[test]
fn health_without_op_on_path_says_so() {
    let empty = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(
        empty.path(),
        Env::of(&[("PATH", &empty.path().to_string_lossy())]),
    );
    assert_eq!(health(&ctx, &eng()), (false, "op CLI not on PATH".into()));
}

#[test]
fn health_of_an_empty_vault_tells_old_one_item_per_key_items_apart() {
    let fake = Fake::new(&[]);
    fake.fails("get", 1, "").answers("list", "[]");
    fake.answers("listtagged", "[]");
    assert_eq!(
        health(&fake.ctx, &eng()),
        (true, "no secrets yet in item 'charter-team'".into())
    );
    assert!(
        fake.calls()
            .contains(&"item list --vault Eng --tags charter:team --format json".to_string()),
        "{:?}",
        fake.calls()
    );

    fake.answers(
        "listtagged",
        r#"[{"title": "charter-team-A"}, {"title": "charter-team-B"}, {"title": "other"}]"#,
    );
    let (ok, said) = health(&fake.ctx, &eng());
    assert!(!ok);
    assert!(
        said.starts_with("2 item(s) from the old one-item-per-key layout"),
        "{said}"
    );
}

#[test]
fn health_of_an_unreadable_vault_is_the_first_sentence_of_why() {
    let fake = Fake::new(&[]);
    fake.fails("get", 1, "").fails("list", 1, "rate-limited");
    assert_eq!(
        health(&fake.ctx, &eng()),
        (false, "listing vault 'team' failed (op exit 1)".into())
    );
}
