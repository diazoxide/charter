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
        vaultcmd::unignored_plaintext(&ctx, "secrets/prod.json").as_deref(),
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
