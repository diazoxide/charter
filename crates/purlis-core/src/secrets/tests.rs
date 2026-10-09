//! The pieces of the secrets module a recorded scenario cannot reach one at a time.

use super::*;

#[test]
fn redaction_masks_the_longer_value_whole_before_the_shorter_one_inside_it() {
    let values = vec!["abc".to_string(), "abcdef".to_string()];
    assert_eq!(redact(b"x=abcdef y=abc", &values), b"x=*** y=***".to_vec());
}

#[test]
fn redaction_ignores_an_empty_value_and_survives_output_that_is_not_text() {
    let values = vec![String::new(), "tok".to_string()];
    assert_eq!(redact(b"\xff tok \xfe", &values), b"\xff *** \xfe".to_vec());
}

#[test]
fn a_size_band_is_a_power_of_two_range_counted_in_utf8_bytes() {
    assert_eq!(fingerprint::size_band(""), "empty");
    assert_eq!(fingerprint::size_band("abc"), "1–15 bytes");
    assert_eq!(fingerprint::size_band(&"x".repeat(16)), "16–31 bytes");
    assert_eq!(fingerprint::size_band(&"x".repeat(1023)), "512–1023 bytes");
    assert_eq!(fingerprint::size_band(&"x".repeat(1024)), "1024+ bytes");
    // Eight Cyrillic characters are sixteen bytes.
    assert_eq!(fingerprint::size_band("абвгдежз"), "16–31 bytes");
}

#[test]
fn a_reference_is_split_the_way_urlsplit_splits_it() {
    let s = reference::urlsplit("op://Fixture Eng/deploy/token");
    assert_eq!(
        (s.scheme.as_str(), s.netloc.as_str(), s.path.as_str()),
        ("op", "Fixture Eng", "/deploy/token")
    );
    let v = reference::urlsplit("vault://secret/data/app#TOKEN");
    assert_eq!(
        (v.netloc.as_str(), v.path.as_str(), v.fragment.as_str()),
        ("secret", "/data/app", "TOKEN")
    );
    assert_eq!(reference::urlsplit("OP://x/y/z").scheme, "op");
    assert_eq!(reference::urlsplit("plain-value").scheme, "");
}

#[test]
fn only_a_supported_scheme_is_a_reference_and_anything_json_holds_is_answered() {
    use serde_json::json;
    assert_eq!(reference::scheme_of(&json!("op://v/i/f")), Some("op"));
    assert_eq!(reference::scheme_of(&json!("https://example.com")), None);
    assert_eq!(reference::scheme_of(&json!(123)), None);
    assert_eq!(reference::scheme_of(&json!(null)), None);
}

#[test]
fn a_malformed_reference_is_refused_before_any_cli_runs() {
    assert!(reference::argv("op://vault-only", "op").is_err());
    assert!(reference::argv("vault://secret/app", "vault").is_err());
    let (argv, cli) = reference::argv("vault://secret/data/app#TOKEN", "vault").unwrap();
    assert_eq!(cli, "vault");
    assert_eq!(
        argv,
        ["vault", "kv", "get", "-field=TOKEN", "secret/data/app"]
    );
}

#[test]
fn a_vault_file_inside_the_plane_that_git_would_commit_is_named() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let git = |args: &[&str]| {
        let mut c = std::process::Command::new("git");
        c.arg("-C").arg(root).args(args);
        crate::forklock::output(&mut c).expect("git runs")
    };
    git(&["init", "-q"]);
    std::fs::write(root.join(".gitignore"), "/.charter/\n").unwrap();
    let ctx = Ctx::new(root, Env::of(&[]));
    assert_eq!(
        vaultcmd::unignored_plaintext(&ctx, "secrets/prod.json")
            .map(|found| found.path)
            .as_deref(),
        Some("secrets/prod.json")
    );
    assert_eq!(
        vaultcmd::unignored_plaintext(&ctx, ".charter/vaults/x.json"),
        None
    );
    let outside = tempfile::tempdir().unwrap();
    assert_eq!(
        vaultcmd::unignored_plaintext(&ctx, &outside.path().join("x.json").to_string_lossy()),
        None
    );
}

#[test]
fn an_identity_binding_resolves_to_the_declared_source_and_never_an_ambient_one() {
    let tmp = tempfile::tempdir().unwrap();
    let mut config = serde_json::Map::new();
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}),
    );
    let v = registry::Vault {
        name: "team".into(),
        provider: "1password".into(),
        persona: None,
        config,
    };
    let bound = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", "t1")]));
    assert_eq!(
        env_overlay(&bound, &v).unwrap(),
        vec![("OP_SERVICE_ACCOUNT_TOKEN".to_string(), "t1".to_string())]
    );
    let ambient = Ctx::new(
        tmp.path(),
        Env::of(&[("OP_SERVICE_ACCOUNT_TOKEN", "someone-else")]),
    );
    let err = env_overlay(&ambient, &v).unwrap_err();
    assert!(err.message.contains("$OP_TEAM_TOKEN, which is unset"));
    assert!(!err.message.contains("someone-else"));
    assert_eq!(identity_note(&v), " (identity from $OP_TEAM_TOKEN)");
}

#[test]
fn the_local_half_overrides_the_shared_one_field_by_field() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::fs::write(
        root.join("vaults.json"),
        r#"{"vaults": {"team": {"provider": "1password", "persona": "devops",
            "config": {"op-vault": "Eng"}}, "bad": "not an object"}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join(".charter/vaults.json"),
        r#"{"vaults": {"team": {"config": {"account": "me.1password.com"}}}}"#,
    )
    .unwrap();
    let ctx = Ctx::new(root, Env::of(&[]));
    let v = registry::vault(&ctx, "team").unwrap();
    assert_eq!(v.provider, "1password");
    assert_eq!(v.persona.as_deref(), Some("devops"));
    assert_eq!(config_str(&v.config, "op-vault"), Some("Eng"));
    assert_eq!(config_str(&v.config, "account"), Some("me.1password.com"));
    assert!(
        registry::vault(&ctx, "bad").is_err(),
        "a string is not a vault"
    );
    assert_eq!(registry::scope_of(&ctx, "team"), "both");
}

#[test]
fn nothing_that_holds_a_value_prints_it_in_debug() {
    let env = Env::of(&[("TOKEN", "debug-leak-value")]);
    let ctx = Ctx::new(std::path::Path::new("/p"), env.clone());
    let ran = run::Ran {
        code: 0,
        stdout: "debug-leak-value".into(),
        stderr: "debug-leak-value".into(),
    };
    let from = cmd::SetFrom {
        value: Some("debug-leak-value".into()),
        ..Default::default()
    };
    for shown in [
        format!("{env:?}"),
        format!("{ctx:?}"),
        format!("{ran:?}"),
        format!("{from:?}"),
    ] {
        assert!(!shown.contains("debug-leak-value"), "{shown}");
    }
}

/// A plane with one vault registered locally, and the context to reach it.
fn plane_with(provider: &str) -> (tempfile::TempDir, Ctx, registry::Vault) {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let mut config = serde_json::Map::new();
    if provider != "keyring" {
        config.insert(
            "file".into(),
            serde_json::Value::String(
                tmp.path()
                    .join(".charter/vaults/ops.json")
                    .to_string_lossy()
                    .into_owned(),
            ),
        );
    }
    registry::add_vault(&ctx, "ops", provider, config, None, false, false).unwrap();
    let v = registry::vault(&ctx, "ops").unwrap();
    (tmp, ctx, v)
}

#[test]
fn renaming_a_secret_moves_its_value_to_the_new_key_and_the_old_one_is_gone() {
    for provider in ["keyring", "plain-file"] {
        let (_tmp, ctx, v) = plane_with(provider);
        cmd::set_value(&ctx, &v, "OLD", "rename-value-7c1e").unwrap();

        cmd::rename(&ctx, &v, "OLD", "NEW").unwrap();

        assert_eq!(
            cmd::keys(&ctx, &v).unwrap(),
            vec!["NEW".to_string()],
            "{provider}"
        );
        assert_eq!(
            cmd::get_value(&ctx, &v, "NEW").unwrap(),
            "rename-value-7c1e"
        );
    }
}

#[test]
fn a_rename_onto_a_key_that_exists_is_refused_and_both_are_left_as_they_were() {
    let (_tmp, ctx, v) = plane_with("keyring");
    cmd::set_value(&ctx, &v, "A", "value-of-a-19f").unwrap();
    cmd::set_value(&ctx, &v, "B", "value-of-b-2d8").unwrap();

    let err = cmd::rename(&ctx, &v, "A", "B").unwrap_err();

    assert!(err.message.contains("'B'"), "{}", err.message);
    assert!(!err.message.contains("value-of"), "{}", err.message);
    assert_eq!(cmd::get_value(&ctx, &v, "A").unwrap(), "value-of-a-19f");
    assert_eq!(cmd::get_value(&ctx, &v, "B").unwrap(), "value-of-b-2d8");
}

#[test]
fn a_rename_of_a_key_the_vault_does_not_hold_is_not_found() {
    let (_tmp, ctx, v) = plane_with("keyring");
    let err = cmd::rename(&ctx, &v, "MISSING", "NEW").unwrap_err();
    assert_eq!(err.kind, Kind::NotFound);
}

#[test]
fn renaming_in_a_reference_vault_moves_the_reference_and_never_resolves_it() {
    // No `op` on this PATH: a rename that resolved the reference would fail, and one that
    // then stored what it resolved would turn a pointer into a plaintext.
    let (_tmp, ctx, v) = plane_with("reference");
    cmd::set_value(&ctx, &v, "OLD", "op://Eng/deploy/token").unwrap();

    cmd::rename(&ctx, &v, "OLD", "NEW").unwrap();

    assert_eq!(
        reference::reference_for(&ctx, &v, "NEW").unwrap(),
        serde_json::json!("op://Eng/deploy/token")
    );
    assert!(reference::reference_for(&ctx, &v, "OLD").is_err());
}

// ---------------------------------------------------------------------------------------
// A vault's identity, moved into the keyring (#237). A test build's keyring is the stub under
// the plane's state directory, so none of this reaches the operator's.

/// Each identity variable of `v` and where it is held, as `(source, held)`.
fn held_at(ctx: &Ctx, v: &registry::Vault) -> Vec<(String, identity::Held)> {
    identity::held(ctx, v)
        .into_iter()
        .map(|b| (b.source, b.held))
        .collect()
}

const MOVED_TOKEN: &str = "ops_fixture-moved-identity-2b7e91";

/// A plane with a 1Password vault `team` read through `$OP_TEAM_TOKEN`.
fn team_plane() -> (tempfile::TempDir, registry::Vault) {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let mut config = serde_json::Map::new();
    config.insert("op-vault".into(), serde_json::json!("Fixture"));
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}),
    );
    registry::add_vault(&ctx, "team", "1password", config, None, false, false).unwrap();
    let v = registry::vault(&ctx, "team").unwrap();
    (tmp, v)
}

#[test]
fn a_moved_identity_is_read_from_the_keyring_when_the_environment_no_longer_has_it() {
    let (tmp, v) = team_plane();
    let carrying = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", MOVED_TOKEN)]));

    assert_eq!(
        identity::move_to_keyring(&carrying, &v).unwrap(),
        ["OP_TEAM_TOKEN"]
    );

    // A chat, or a plain terminal, that never had the variable.
    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let v = registry::vault(&bare, "team").unwrap();
    assert_eq!(
        env_overlay(&bare, &v).unwrap(),
        vec![(
            "OP_SERVICE_ACCOUNT_TOKEN".to_string(),
            MOVED_TOKEN.to_string()
        )]
    );
}

#[test]
fn once_moved_the_keyring_is_read_before_the_environment() {
    let (tmp, v) = team_plane();
    let carrying = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", MOVED_TOKEN)]));
    identity::move_to_keyring(&carrying, &v).unwrap();

    let stale = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", "a-stale-export")]));
    let v = registry::vault(&stale, "team").unwrap();

    assert_eq!(env_overlay(&stale, &v).unwrap()[0].1, MOVED_TOKEN);
}

#[test]
fn a_vault_whose_identity_was_never_moved_does_not_ask_the_keyring() {
    let (tmp, v) = team_plane();
    // A store that cannot be read: a lookup that asked it would fail.
    std::fs::write(tmp.path().join(".charter/keyring-stub.json"), "not json").unwrap();
    let carrying = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", "from-the-env-44")]));

    assert_eq!(env_overlay(&carrying, &v).unwrap()[0].1, "from-the-env-44");
    assert_eq!(
        held_at(&carrying, &v),
        [("OP_TEAM_TOKEN".to_string(), identity::Held::Environment)]
    );
}

#[test]
fn a_moved_identity_that_the_keyring_lost_falls_back_to_the_environment_then_says_both() {
    let (tmp, v) = team_plane();
    let carrying = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", MOVED_TOKEN)]));
    identity::move_to_keyring(&carrying, &v).unwrap();
    std::fs::remove_file(tmp.path().join(".charter/keyring-stub.json")).unwrap();

    let v = registry::vault(&carrying, "team").unwrap();
    assert_eq!(env_overlay(&carrying, &v).unwrap()[0].1, MOVED_TOKEN);

    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let err = env_overlay(&bare, &v).unwrap_err();
    assert!(err.message.contains("$OP_TEAM_TOKEN"), "{}", err.message);
    assert!(err.message.contains("keyring"), "{}", err.message);
}

#[test]
fn moving_an_identity_that_is_not_set_is_refused_and_marks_nothing() {
    let (tmp, v) = team_plane();
    let bare = Ctx::new(tmp.path(), Env::of(&[]));

    let err = identity::move_to_keyring(&bare, &v).unwrap_err();

    assert!(err.message.contains("OP_TEAM_TOKEN"), "{}", err.message);
    assert_eq!(
        held_at(&bare, &v),
        [("OP_TEAM_TOKEN".to_string(), identity::Held::Unset)]
    );
    assert!(!tmp.path().join(".charter/keyring-stub.json").exists());
}

#[test]
fn a_committed_registry_cannot_mark_an_identity_as_held_in_the_keyring() {
    // The keyring is this machine's, and so is the mark: a `vaults.json` that arrives by
    // `git pull` saying "read this from the keyring" would make charter hand a keyring item to
    // whatever `op` it runs.
    let (tmp, v) = team_plane();
    let ctx = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", "from-the-env-55")]));
    identity::move_to_keyring(&ctx, &v).unwrap();
    // Move the mark from the local half into the committed one.
    let mut local = registry::load_local(&ctx).unwrap();
    let entry = local["vaults"]["team"].clone();
    local["vaults"].as_object_mut().unwrap().remove("team");
    registry::save_local(&ctx, &local).unwrap();
    let mut shared = registry::load_shared(&ctx).unwrap();
    shared["vaults"]
        .as_object_mut()
        .unwrap()
        .insert("team".into(), entry);
    registry::save_shared(&ctx, &shared).unwrap();

    let v = registry::vault(&ctx, "team").unwrap();
    assert_eq!(env_overlay(&ctx, &v).unwrap()[0].1, "from-the-env-55");
    assert_eq!(held_at(&ctx, &v)[0].1, identity::Held::Environment);
}

#[test]
fn no_refusal_or_debug_of_an_identity_move_carries_the_token() {
    let (tmp, v) = team_plane();
    let carrying = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", MOVED_TOKEN)]));
    // A keyring that cannot be read, so the move fails after the token was read.
    std::fs::write(tmp.path().join(".charter/keyring-stub.json"), "not json").unwrap();

    let err = identity::move_to_keyring(&carrying, &v).unwrap_err();

    for shown in [
        err.message.clone(),
        format!("{err:?}"),
        format!("{carrying:?}"),
    ] {
        assert!(!shown.contains(MOVED_TOKEN), "{shown}");
    }
    assert_eq!(held_at(&carrying, &v)[0].1, identity::Held::Environment);
}

// Regression tests for the #271 adversarial review (U5, U6). Fabricated values only. Each began
// as a proof that the exploit worked; the assertion is now that it does not.

#[test]
fn a_committed_env_binding_cannot_redirect_a_locally_marked_vault_to_another_token() {
    // #271 review U5. The operator moved two tokens; `team` is committed with its binding in the
    // shared half and only the local mark beside it. A teammate's commit that rewrites team's
    // committed source to prod's variable must not make team hand out prod's token.
    let (tmp, team) = team_plane();
    let mut config = serde_json::Map::new();
    config.insert("op-vault".into(), serde_json::json!("Prod"));
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_PROD_TOKEN"}),
    );
    registry::add_vault(
        &Ctx::new(tmp.path(), Env::of(&[])),
        "prod",
        "1password",
        config,
        None,
        false,
        false,
    )
    .unwrap();
    let prod = registry::vault(&Ctx::new(tmp.path(), Env::of(&[])), "prod").unwrap();
    let carrying = Ctx::new(
        tmp.path(),
        Env::of(&[
            ("OP_TEAM_TOKEN", "ops_fixture-team-1"),
            ("OP_PROD_TOKEN", "ops_fixture-prod-2"),
        ]),
    );
    identity::move_to_keyring(&carrying, &team).unwrap();
    identity::move_to_keyring(&carrying, &prod).unwrap();

    // Put team's binding in the committed half and leave only its (full) local record beside it,
    // as a shared vault whose token was moved locally looks.
    let mut local = registry::load_local(&carrying).unwrap();
    let record = local["vaults"]["team"]["config"]["identity"].clone();
    let mut shared_entry = local["vaults"]["team"].clone();
    shared_entry["config"]
        .as_object_mut()
        .unwrap()
        .remove("identity");
    local["vaults"]["team"] = serde_json::json!({"config": {"identity": record}});
    registry::save_local(&carrying, &local).unwrap();
    let mut shared = registry::load_shared(&carrying).unwrap();
    shared
        .entry("vaults")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .unwrap()
        .insert("team".into(), shared_entry);
    registry::save_shared(&carrying, &shared).unwrap();

    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let v = registry::vault(&bare, "team").unwrap();
    assert_eq!(env_overlay(&bare, &v).unwrap()[0].1, "ops_fixture-team-1");

    // The hostile commit: team's committed source becomes prod's variable.
    let mut shared = registry::load_shared(&bare).unwrap();
    shared["vaults"]["team"]["config"]["env"] =
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_PROD_TOKEN"});
    registry::save_shared(&bare, &shared).unwrap();

    // The recorded binding no longer matches, so the mark is not honoured: team reads from the
    // environment (empty here) and is refused, rather than handing out prod's moved token.
    let v = registry::vault(&bare, "team").unwrap();
    let err = env_overlay(&bare, &v).unwrap_err();
    assert!(err.message.contains("$OP_PROD_TOKEN"), "{}", err.message);
    assert!(
        !err.message.contains("ops_fixture-prod-2"),
        "{}",
        err.message
    );
    assert_eq!(identity::held(&bare, &v)[0].held, identity::Held::Unset);
}

#[test]
fn a_declared_identity_source_is_stripped_and_the_op_prefix_is_case_insensitive() {
    // #271 review U6. `OP_` is matched case-insensitively, and a source of another spelling
    // (`--token-env PROD_1P_TOKEN`) is caught by NAME through `identity_vars`, which the chat
    // builder unions with the prefix strip.
    use std::ffi::OsStr;
    assert!(identity::kept_from_chats(OsStr::new("OP_TEAM_TOKEN")));
    assert!(identity::kept_from_chats(OsStr::new("op_team_token")));
    assert!(identity::kept_from_chats(OsStr::new("Op_Mixed")));
    assert!(!identity::kept_from_chats(OsStr::new("PROD_1P_TOKEN")));

    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let mut config = serde_json::Map::new();
    config.insert("op-vault".into(), serde_json::json!("Prod"));
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "PROD_1P_TOKEN"}),
    );
    registry::add_vault(&ctx, "p", "1password", config, None, false, false).unwrap();
    // The declared source is listed, so the chat builder strips it though it has no OP_ prefix.
    let doc = registry::load_registry(&ctx).unwrap();
    let names: Vec<String> = registry::identity_vars(&doc)
        .into_iter()
        .flat_map(|(_, v)| v)
        .collect();
    assert!(names.contains(&"PROD_1P_TOKEN".to_string()), "{names:?}");
    assert!(
        names.contains(&"OP_SERVICE_ACCOUNT_TOKEN".to_string()),
        "{names:?}"
    );
}

// ---------------------------------------------------------------------------------------
// The password-box path and the pinned `op` (#237, #271 review U1/U3). The `op` here is a
// stand-in on a temp PATH, unsigned, so `codesign` reports no team for it; the keyring is the
// test build's stub.

const PASTED_TOKEN: &str = "ops_fixture-pasted-identity-5c3a08";

/// A plane with a 1Password vault `team` read through `$OP_TEAM_TOKEN`, with an op-vault and an
/// account, and a stand-in `op` that prints `out` — the directory holding it, and its path.
fn pinned_plane(
    out: &str,
) -> (
    tempfile::TempDir,
    tempfile::TempDir,
    std::path::PathBuf,
    registry::Vault,
) {
    crate::secrets::program::stand_ins_live_in_temp_folders();
    let tmp = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let op = stand_in::program(
        bin.path(),
        "op",
        &format!("#!/bin/sh\nprintf '%s' '{out}'\n"),
    );
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let mut config = serde_json::Map::new();
    config.insert("op-vault".into(), serde_json::json!("Fixture"));
    config.insert("account".into(), serde_json::json!("acme.1password.com"));
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}),
    );
    registry::add_vault(&ctx, "team", "1password", config, None, false, false).unwrap();
    let v = registry::vault(&ctx, "team").unwrap();
    (tmp, bin, op, v)
}

/// A context on `root` whose PATH is `bin` alone.
fn on_path(root: &std::path::Path, bin: &std::path::Path) -> Ctx {
    Ctx::new(root, Env::of(&[("PATH", &bin.to_string_lossy())]))
}

/// The identity record this machine's half keeps for `team`.
fn identity_record(ctx: &Ctx) -> serde_json::Value {
    registry::load_local(ctx).unwrap()["vaults"]["team"]["config"]["identity"].clone()
}

#[test]
fn a_pasted_token_is_kept_under_a_random_item_and_pins_the_binding_and_the_op_on_path() {
    let (tmp, bin, op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());

    assert_eq!(
        identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap(),
        ["OP_TEAM_TOKEN"]
    );

    let rec = identity_record(&ctx);
    assert_eq!(rec["held"], "keyring");
    assert_eq!(
        rec["bindings"],
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"})
    );
    assert_eq!(rec["op_vault"], "Fixture");
    assert_eq!(rec["account"], "acme.1password.com");
    assert_eq!(rec["op_cmd"], op.to_string_lossy().as_ref());
    assert_eq!(rec["op_team"], "", "an unsigned `op` carries no team");
    let id = rec["ids"]["OP_TEAM_TOKEN"].as_str().unwrap().to_owned();
    assert!(
        id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit()),
        "{id:?}"
    );

    // The item is `purlis/@identity/<id>`, account the source variable.
    let stub: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".charter/keyring-stub.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        stub,
        serde_json::json!({ format!("purlis/@identity/{id}\nOP_TEAM_TOKEN"): PASTED_TOKEN })
    );

    // Read back where the variable is not set, running exactly the pinned `op`.
    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    assert_eq!(
        env_overlay(&bare, &v).unwrap(),
        vec![(
            "OP_SERVICE_ACCOUNT_TOKEN".to_string(),
            PASTED_TOKEN.to_string()
        )]
    );
    assert_eq!(identity::pinned_op(&bare, &v).unwrap(), Some(op));
}

/// Put `items` (`(service, value)`, account `OP_TEAM_TOKEN`) in the stub keyring in place of
/// whatever it held.
fn plant(tmp: &tempfile::TempDir, items: &[(String, &str)]) {
    let map: serde_json::Map<String, serde_json::Value> = items
        .iter()
        .map(|(service, value)| {
            (
                format!("{service}\nOP_TEAM_TOKEN"),
                serde_json::json!(value),
            )
        })
        .collect();
    std::fs::write(
        tmp.path().join(".charter/keyring-stub.json"),
        serde_json::Value::Object(map).to_string(),
    )
    .unwrap();
}

/// Set (or with `None`, remove) the identity record's `base` field.
fn set_base(ctx: &Ctx, base: Option<&str>) {
    let mut local = registry::load_local(ctx).unwrap();
    let rec = local["vaults"]["team"]["config"]["identity"]
        .as_object_mut()
        .unwrap();
    match base {
        Some(b) => rec.insert("base".into(), serde_json::json!(b)),
        None => rec.remove("base"),
    };
    registry::save_local(ctx, &local).unwrap();
}

#[test]
fn a_new_identity_record_names_the_purlis_base_and_reads_only_there() {
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    let rec = identity_record(&ctx);
    assert_eq!(rec["base"], "purlis");
    let id = rec["ids"]["OP_TEAM_TOKEN"].as_str().unwrap().to_owned();

    // The field survives the record being read and written again.
    let local = registry::load_local(&ctx).unwrap();
    registry::save_local(&ctx, &local).unwrap();
    assert_eq!(identity_record(&ctx)["base"], "purlis");

    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let read = || identity::from_keyring(&bare, &v, "OP_TEAM_TOKEN").unwrap();
    assert_eq!(read().as_deref(), Some(PASTED_TOKEN));

    // A `charter/` item beside it is never read in its place, nor instead of a missing one.
    plant(
        &tmp,
        &[
            (format!("charter/@identity/{id}"), "old-token"),
            (format!("purlis/@identity/{id}"), PASTED_TOKEN),
        ],
    );
    assert_eq!(read().as_deref(), Some(PASTED_TOKEN));
    plant(&tmp, &[(format!("charter/@identity/{id}"), "old-token")]);
    assert_eq!(read(), None);
}

#[test]
fn a_record_from_before_the_rename_reads_only_its_charter_item() {
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    let id = identity_record(&ctx)["ids"]["OP_TEAM_TOKEN"]
        .as_str()
        .unwrap()
        .to_owned();
    // A record a build from before the rename wrote: no `base`.
    set_base(&ctx, None);
    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let read = || identity::from_keyring(&bare, &v, "OP_TEAM_TOKEN");

    plant(&tmp, &[(format!("charter/@identity/{id}"), "old-token")]);
    assert_eq!(read().unwrap().as_deref(), Some("old-token"));

    // An item planted under the purlis name never shadows it, nor stands in for a missing one.
    plant(
        &tmp,
        &[
            (format!("charter/@identity/{id}"), "old-token"),
            (format!("purlis/@identity/{id}"), "planted"),
        ],
    );
    assert_eq!(read().unwrap().as_deref(), Some("old-token"));
    plant(&tmp, &[(format!("purlis/@identity/{id}"), "planted")]);
    assert_eq!(read().unwrap(), None);

    // The switch (the keychain copy's, RN-6) is the field, and only the field.
    set_base(&ctx, Some("purlis"));
    assert_eq!(read().unwrap().as_deref(), Some("planted"));

    // A base charter does not know is refused, never guessed.
    set_base(&ctx, Some("charterx"));
    let refused = read().unwrap_err().message;
    assert!(refused.contains("keyring base"), "{refused}");
    assert!(!refused.contains("  "), "{refused:?}");
}

/// Take the `op_item` out of the identity record, as a build from before #1527 wrote it.
fn as_made_before_the_item_was_pinned(ctx: &Ctx) {
    let mut local = registry::load_local(ctx).unwrap();
    local["vaults"]["team"]["config"]["identity"]
        .as_object_mut()
        .unwrap()
        .remove("op_item");
    registry::save_local(ctx, &local).unwrap();
}

#[test]
fn a_record_made_before_the_item_was_pinned_pins_it_at_its_next_read() {
    // #1542: an old record named no item, so a commit that changed `op-item` read another
    // item of the same 1Password vault through it.
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    as_made_before_the_item_was_pinned(&ctx);
    assert_eq!(identity_record(&ctx)["op_item"], serde_json::Value::Null);

    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    assert_eq!(
        identity::from_keyring(&bare, &v, "OP_TEAM_TOKEN")
            .unwrap()
            .as_deref(),
        Some(PASTED_TOKEN)
    );
    // The item it was read with is pinned now, and nothing else of the record changed.
    let rec = identity_record(&ctx);
    assert_eq!(rec["op_item"], "charter-team");
    assert_eq!(rec["op_vault"], "Fixture");
    assert_eq!(rec["held"], "keyring");

    // From then on a committed change of the item unpins it, as it does a record made now.
    std::fs::write(
        ctx.shared_registry(),
        serde_json::json!({"vaults": {"team": {"provider": "1password", "persona": null,
            "config": {"op-item": "another-item"}}}})
        .to_string(),
    )
    .unwrap();
    let moved = registry::vault(&bare, "team").unwrap();
    assert!(!identity::in_keyring(&bare, &moved));
    assert_eq!(
        identity::from_keyring(&bare, &moved, "OP_TEAM_TOKEN").unwrap(),
        None
    );
}

/// Write the committed half with `config` for `team`.
fn commit_team(ctx: &Ctx, config: serde_json::Value) {
    std::fs::write(
        ctx.shared_registry(),
        serde_json::json!({"vaults": {"team": {"provider": "1password", "persona": null,
            "config": config}}})
        .to_string(),
    )
    .unwrap();
}

#[test]
fn an_old_record_whose_item_a_commit_chose_is_refused_and_never_pinned() {
    // #1542 review M1: a commit set the item before the first read after the upgrade. Pinning
    // it would be a committed entry changing a record (ADR 0047), so the token is not used
    // until it is given again from the vault's tab, where the settings are shown.
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    as_made_before_the_item_was_pinned(&ctx);
    commit_team(&ctx, serde_json::json!({"op-item": "another-item"}));
    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let moved = registry::vault(&bare, "team").unwrap();

    let refused = identity::from_keyring(&bare, &moved, "OP_TEAM_TOKEN").unwrap_err();
    assert!(
        refused.message.contains("committed vaults.json"),
        "{}",
        refused.message
    );
    assert!(
        refused.message.contains("vault's tab"),
        "{}",
        refused.message
    );
    assert!(
        refused.message.contains("another-item"),
        "{}",
        refused.message
    );
    assert!(!refused.message.contains(PASTED_TOKEN));
    assert_eq!(identity_record(&ctx)["op_item"], serde_json::Value::Null);
    assert!(!identity::in_keyring(&bare, &moved));
    assert_eq!(identity::pinned_op(&bare, &moved).unwrap(), None);
    // The read says why, and so does the panel that reads no keyring.
    let read = env_overlay(&bare, &moved).unwrap_err().message;
    assert!(read.contains("vault's tab"), "{read}");
    let listed = crate::secrets::identity_missing(&bare, &moved)
        .unwrap()
        .message;
    assert!(listed.contains("committed vaults.json"), "{listed}");

    // A revert of the commit gives the token back, still unpinned until that read.
    std::fs::remove_file(ctx.shared_registry()).unwrap();
    assert_eq!(
        identity::from_keyring(&bare, &v, "OP_TEAM_TOKEN")
            .unwrap()
            .as_deref(),
        Some(PASTED_TOKEN)
    );
    assert_eq!(identity_record(&ctx)["op_item"], "charter-team");
}

#[test]
fn an_old_record_whose_item_this_machine_names_is_pinned_to_it() {
    let (tmp, bin, _op, _v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    let mut local = registry::load_local(&ctx).unwrap();
    local["vaults"]["team"]["config"]["op-item"] = serde_json::json!("mine");
    registry::save_local(&ctx, &local).unwrap();
    let v = registry::vault(&ctx, "team").unwrap();
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    as_made_before_the_item_was_pinned(&ctx);
    // A committed item does not outrank this machine's own.
    commit_team(&ctx, serde_json::json!({"op-item": "theirs"}));
    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let v = registry::vault(&bare, "team").unwrap();

    assert_eq!(
        identity::from_keyring(&bare, &v, "OP_TEAM_TOKEN")
            .unwrap()
            .as_deref(),
        Some(PASTED_TOKEN)
    );
    assert_eq!(identity_record(&ctx)["op_item"], "mine");
}

#[test]
fn a_legacy_item_spelling_here_outranked_by_a_committed_one_is_refused() {
    // The modern `op-item` is read before the legacy `op_item` across the merged map, so a
    // committed modern key decides the item even where this machine holds the legacy one.
    let (tmp, bin, _op, _v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    let mut local = registry::load_local(&ctx).unwrap();
    local["vaults"]["team"]["config"]["op_item"] = serde_json::json!("mine");
    registry::save_local(&ctx, &local).unwrap();
    let v = registry::vault(&ctx, "team").unwrap();
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    as_made_before_the_item_was_pinned(&ctx);
    commit_team(&ctx, serde_json::json!({"op-item": "theirs"}));
    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let v = registry::vault(&bare, "team").unwrap();

    assert!(identity::from_keyring(&bare, &v, "OP_TEAM_TOKEN").is_err());
    assert_eq!(identity_record(&ctx)["op_item"], serde_json::Value::Null);
}

#[test]
fn an_old_record_that_does_not_match_the_vault_is_not_given_an_item() {
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    as_made_before_the_item_was_pinned(&ctx);
    // The vault now names another 1Password vault: the record is not honoured, and so pins
    // nothing for the vault as it now is.
    let mut local = registry::load_local(&ctx).unwrap();
    local["vaults"]["team"]["config"]["op-vault"] = serde_json::json!("Elsewhere");
    registry::save_local(&ctx, &local).unwrap();
    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let moved = registry::vault(&bare, "team").unwrap();
    assert_eq!(
        identity::from_keyring(&bare, &moved, "OP_TEAM_TOKEN").unwrap(),
        None
    );
    assert_eq!(identity_record(&ctx)["op_item"], serde_json::Value::Null);
}

#[test]
fn two_pasted_tokens_are_kept_under_two_different_items() {
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    let first = identity_record(&ctx)["ids"]["OP_TEAM_TOKEN"].clone();
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    assert_ne!(identity_record(&ctx)["ids"]["OP_TEAM_TOKEN"], first);
}

#[test]
fn a_vault_whose_identity_is_not_in_the_keyring_pins_no_op() {
    let (tmp, _bin, _op, v) = pinned_plane("");
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    assert_eq!(identity::pinned_op(&ctx, &v).unwrap(), None);
}

#[test]
fn a_pinned_op_that_is_missing_gone_or_signed_by_another_team_is_refused() {
    let (tmp, bin, op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    let set_field = |field: &str, value: &str| {
        let mut local = registry::load_local(&ctx).unwrap();
        local["vaults"]["team"]["config"]["identity"][field] = serde_json::json!(value);
        registry::save_local(&ctx, &local).unwrap();
    };

    set_field("op_team", identity::ONEPASSWORD_TEAM_ID);
    let err = identity::pinned_op(&ctx, &v).unwrap_err();
    assert!(err.message.contains("different team"), "{}", err.message);

    set_field("op_team", "");
    std::fs::remove_file(&op).unwrap();
    let err = identity::pinned_op(&ctx, &v).unwrap_err();
    assert!(err.message.contains("is gone"), "{}", err.message);

    set_field("op_cmd", "");
    let err = identity::pinned_op(&ctx, &v).unwrap_err();
    assert!(
        err.message.contains("no `op` was pinned"),
        "{}",
        err.message
    );
}

#[test]
fn a_token_pasted_where_no_op_is_on_path_is_kept_and_every_read_is_refused_until_it_is_put_again() {
    let (tmp, _bin, _op, v) = pinned_plane("");
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    let rec = identity_record(&ctx);
    assert_eq!(
        (&rec["op_cmd"], &rec["op_team"]),
        (&serde_json::json!(""), &serde_json::json!(""))
    );
    let err = identity::pinned_op(&ctx, &v).unwrap_err();
    assert!(
        err.message.contains("no `op` was pinned"),
        "{}",
        err.message
    );
}

/// D-1516-9: the pin is what every later read runs with the token out of the keyring, so an
/// `op` a chat could have written is never the one pinned.
#[test]
fn an_op_found_only_where_a_chat_can_write_is_not_pinned_when_a_token_is_stored() {
    let (tmp, _bin, _op, v) = pinned_plane("");
    let planted = tmp.path().join("tools");
    std::fs::create_dir_all(&planted).unwrap();
    stand_in::program(&planted, "op", "#!/bin/sh\nprintf planted\n");
    let ctx = on_path(tmp.path(), &planted);
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    let rec = identity_record(&ctx);
    assert_eq!(
        (&rec["op_cmd"], &rec["op_team"]),
        (&serde_json::json!(""), &serde_json::json!(""))
    );
    let err = identity::pinned_op(&ctx, &v).unwrap_err();
    assert!(
        err.message.contains("no `op` was pinned"),
        "{}",
        err.message
    );
}

#[test]
fn the_app_environment_is_said_to_hold_a_token_only_for_a_source_set_to_something() {
    let (tmp, _bin, _op, v) = pinned_plane("");
    let set = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", PASTED_TOKEN)]));
    assert_eq!(identity::app_env_holds_a_token(&set, &v), ["OP_TEAM_TOKEN"]);
    let empty = Ctx::new(tmp.path(), Env::of(&[("OP_TEAM_TOKEN", "")]));
    assert!(identity::app_env_holds_a_token(&empty, &v).is_empty());
    let unset = Ctx::new(tmp.path(), Env::of(&[]));
    assert!(identity::app_env_holds_a_token(&unset, &v).is_empty());
}

#[test]
fn a_keyring_held_reference_runs_the_pinned_op_and_refuses_any_other_cli() {
    let (tmp, bin, _op, _team) = pinned_plane("resolved-through-the-pin");
    let root = tmp.path();
    let setup = on_path(root, bin.path());
    let mut config = serde_json::Map::new();
    config.insert("file".into(), serde_json::json!("refs.json"));
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_REF_TOKEN"}),
    );
    registry::add_vault(&setup, "refs", "reference", config, None, false, false).unwrap();
    let v = registry::vault(&setup, "refs").unwrap();
    reference::set(&setup, &v, "A", "op://Eng/item/field").unwrap();
    reference::set(&setup, &v, "B", "vault://secret/data/app#TOKEN").unwrap();
    identity::put_in_keyring(&setup, &v, PASTED_TOKEN).unwrap();

    // No PATH at all: only the pinned absolute `op` can answer.
    let bare = Ctx::new(root, Env::of(&[]));
    assert_eq!(
        reference::get(&bare, &v, "A").unwrap(),
        "resolved-through-the-pin"
    );
    let err = reference::get(&bare, &v, "B").unwrap_err();
    assert!(err.message.contains("pins only `op`"), "{}", err.message);
}

// ---------------------------------------------------------------------------------------
// #1526: the way out of an unset identity, and one token for every vault read through it.

#[test]
fn an_unset_identity_is_told_the_keyring_first_then_the_export_and_last_how_to_unbind() {
    let (tmp, v) = team_plane();
    let bare = Ctx::new(tmp.path(), Env::of(&[]));

    let said = env_overlay(&bare, &v).unwrap_err().message;

    let (why, ways) = said.split_once('\n').expect("two lines");
    assert!(
        why.starts_with("vault 'team' is read through $OP_TEAM_TOKEN, which is unset."),
        "{why}"
    );
    assert_eq!(
        ways,
        "  Put the token in the system keyring: open this vault's tab in the app and paste it \
         into the box there. Or export OP_TEAM_TOKEN=… where purlis runs. If this vault should \
         not be bound to that identity at all: purlis vault add team --provider 1password --force"
    );
}

/// Register a 1Password vault `name` kept in `op_vault` and read through `$source`.
fn read_through(ctx: &Ctx, name: &str, op_vault: &str, source: &str) -> registry::Vault {
    let mut config = serde_json::Map::new();
    config.insert("op-vault".into(), serde_json::json!(op_vault));
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": source}),
    );
    registry::add_vault(ctx, name, "1password", config, None, false, false).unwrap();
    registry::vault(ctx, name).unwrap()
}

#[test]
fn a_store_marks_the_one_vault_and_names_the_others_read_through_the_same_variable() {
    // D-1526-7: no vault is given a token the person did not put in from its own tab, whichever
    // half of the registry names it.
    let (tmp, bin, _op, team) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    read_through(&ctx, "edge", "Edge", "OP_TEAM_TOKEN");
    read_through(&ctx, "prod", "Prod", "OP_PROD_TOKEN");
    // One only the committed half names.
    let mut shared = registry::load_shared(&ctx).unwrap();
    shared.insert(
        "vaults".into(),
        serde_json::json!({"pulled": {"provider": "1password", "persona": null, "config": {
            "op-vault": "Pulled", "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}}}}),
    );
    registry::save_shared(&ctx, &shared).unwrap();

    identity::put_in_keyring(&ctx, &team, PASTED_TOKEN).unwrap();

    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    let local = registry::load_local(&bare).unwrap();
    for other in ["edge", "pulled"] {
        let v = registry::vault(&bare, other).unwrap();
        assert_eq!(
            held_at(&bare, &v),
            [("OP_TEAM_TOKEN".to_string(), identity::Held::Unset)],
            "{other}"
        );
        assert!(env_overlay(&bare, &v).is_err(), "{other}");
        assert_eq!(
            local["vaults"][other]["config"]["identity"],
            serde_json::Value::Null,
            "{other}"
        );
    }
    // One item in the keyring: team's.
    let stub: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".charter/keyring-stub.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stub.len(), 1, "{:?}", stub.keys());

    // The others are named for the tab to point at, and the one read through another variable
    // is not. A vault whose token is kept is no longer named.
    let team = registry::vault(&bare, "team").unwrap();
    assert_eq!(identity::unset_alike(&bare, &team), ["edge", "pulled"]);
    let edge = registry::vault(&bare, "edge").unwrap();
    assert_eq!(identity::unset_alike(&bare, &edge), ["pulled"]);
    let prod = registry::vault(&bare, "prod").unwrap();
    assert!(identity::unset_alike(&bare, &prod).is_empty());
}

/// The stub keyring's items, as `service\naccount` to value.
fn stub_items(tmp: &tempfile::TempDir) -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".charter/keyring-stub.json")).unwrap(),
    )
    .unwrap()
}

#[test]
fn a_replaced_token_is_deleted_from_the_keyring_once_the_new_one_is_in_place() {
    const REPLACEMENT: &str = "ops_fixture-replacement-1526-9d";
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    let old = identity_record(&ctx)["ids"]["OP_TEAM_TOKEN"]
        .as_str()
        .unwrap()
        .to_owned();

    identity::put_in_keyring(&ctx, &v, REPLACEMENT).unwrap();

    let new = identity_record(&ctx)["ids"]["OP_TEAM_TOKEN"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(old, new);
    assert_eq!(
        serde_json::Value::Object(stub_items(&tmp)),
        serde_json::json!({ format!("purlis/@identity/{new}\nOP_TEAM_TOKEN"): REPLACEMENT })
    );
    let bare = Ctx::new(tmp.path(), Env::of(&[]));
    assert_eq!(
        identity::from_keyring(&bare, &v, "OP_TEAM_TOKEN")
            .unwrap()
            .as_deref(),
        Some(REPLACEMENT)
    );
}

#[test]
fn a_replaced_token_kept_under_the_old_base_is_deleted_there() {
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();
    let old = identity_record(&ctx)["ids"]["OP_TEAM_TOKEN"]
        .as_str()
        .unwrap()
        .to_owned();
    // A record made before the rename: no base, and its item under the old one.
    set_base(&ctx, None);
    plant(&tmp, &[(format!("charter/@identity/{old}"), PASTED_TOKEN)]);

    identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap();

    let items = stub_items(&tmp);
    assert_eq!(items.len(), 1, "{:?}", items.keys());
    assert!(
        items.keys().all(|k| k.starts_with("purlis/@identity/")),
        "{:?}",
        items.keys()
    );
}

#[test]
fn a_store_whose_record_cannot_be_saved_leaves_no_token_and_no_half_record() {
    let (tmp, bin, _op, v) = pinned_plane("");
    let ctx = on_path(tmp.path(), bin.path());
    // This machine's half cannot be read, so the record cannot be written beside it.
    let half = ctx.local_registry();
    let was = std::fs::read_to_string(&half).unwrap();
    std::fs::write(&half, "not json").unwrap();

    let err = identity::put_in_keyring(&ctx, &v, PASTED_TOKEN).unwrap_err();

    assert!(!err.message.contains(PASTED_TOKEN), "{}", err.message);
    let kept =
        std::fs::read_to_string(tmp.path().join(".charter/keyring-stub.json")).unwrap_or_default();
    assert!(!kept.contains(PASTED_TOKEN), "a token nothing refers to");
    std::fs::write(&half, was).unwrap();
    assert_eq!(identity_record(&ctx), serde_json::Value::Null);
}
