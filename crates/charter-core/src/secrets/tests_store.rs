//! The vault stores and the registry, one behaviour at a time: what a plain-file or reference
//! vault writes and reports, how the two registry halves are written, and the helpers in
//! [`super`] those lean on. Everything here runs in a temp directory with an [`Env`] built by
//! hand — no test reads this machine's `$HOME`, keychain, `op` or clipboard.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::*;

#[cfg(unix)]
fn mode_of(p: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).unwrap().permissions().mode() & 0o7777
}

#[cfg(unix)]
fn chmod(p: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A directory holding a stand-in called each of `names`, printing `out` and exiting `code` —
/// a vendor CLI that `which` finds and, where a test resolves through it, runs. Written through
/// `stand_in::program`, since one is run the moment it is written (charter-app#81).
#[cfg(unix)]
fn cli_dir(names: &[&str], out: &str, code: i32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in names {
        stand_in::program(
            dir.path(),
            name,
            &format!("#!/bin/sh\nprintf '%s' '{out}'\nprintf 'stderr-{out}' >&2\nexit {code}\n"),
        );
    }
    dir
}

#[cfg(unix)]
fn bin_dir(names: &[&str]) -> tempfile::TempDir {
    cli_dir(names, "", 0)
}

fn vault(name: &str, provider: &str, config: Value) -> registry::Vault {
    registry::Vault {
        name: name.into(),
        provider: provider.into(),
        persona: None,
        config: config.as_object().cloned().unwrap_or_default(),
    }
}

fn day(s: &str) -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

// ---------------------------------------------------------------------------------------------
// `secrets/mod.rs`
// ---------------------------------------------------------------------------------------------

#[test]
fn a_vault_error_prints_as_its_sentence() {
    let e = VaultError::not_found("secret 'K' not found in vault 'v'");
    assert_eq!(e.to_string(), "secret 'K' not found in vault 'v'");
}

#[test]
fn an_environment_and_a_context_debug_print_the_names_they_hold() {
    let env = Env::of(&[("TOKEN", "v1"), ("PATH", "/bin")]);
    assert_eq!(format!("{env:?}"), r#"["TOKEN=***", "PATH=***"]"#);
    let ctx = Ctx::new(Path::new("/plane"), env);
    let shown = format!("{ctx:?}");
    assert!(shown.starts_with("Ctx {"), "{shown}");
    assert!(shown.contains(r#"root: "/plane""#), "{shown}");
    assert!(shown.contains(r#"state: "/plane/.charter""#), "{shown}");
    assert!(shown.contains("TOKEN=***"), "{shown}");
}

#[test]
fn the_process_environment_is_the_one_this_process_was_started_with() {
    let env = Env::from_process();
    assert_eq!(env.vars().len(), std::env::vars_os().count());
    assert_eq!(env.get("PATH"), std::env::var("PATH").ok());
}

#[test]
fn a_context_places_the_vault_directory_and_a_vault_file_under_the_plane() {
    let ctx = Ctx::new(Path::new("/plane"), Env::of(&[("HOME", "/home/me")]));
    assert_eq!(ctx.vaults_dir(), PathBuf::from("/plane/.charter/vaults"));
    assert_eq!(
        ctx.vault_file_path("secrets/app.json"),
        PathBuf::from("/plane/secrets/app.json")
    );
    assert_eq!(
        ctx.vault_file_path("/abs/app.json"),
        PathBuf::from("/abs/app.json")
    );
    assert_eq!(
        ctx.vault_file_path("~/v.json"),
        PathBuf::from("/home/me/v.json")
    );
}

#[test]
fn a_leading_tilde_alone_or_before_a_slash_is_home_and_nothing_else_is() {
    let env = Env::of(&[("HOME", "/home/me")]);
    assert_eq!(expanduser("~", &env), PathBuf::from("/home/me"));
    assert_eq!(expanduser("~/a", &env), PathBuf::from("/home/me/a"));
    assert_eq!(expanduser("~other/a", &env), PathBuf::from("~other/a"));
    assert_eq!(expanduser("a/~", &env), PathBuf::from("a/~"));
    assert_eq!(expanduser("~", &Env::of(&[])), PathBuf::from("~"));
}

#[test]
fn a_context_finds_a_cli_on_its_own_path_and_not_elsewhere() {
    let bin = bin_dir(&["op"]);
    let path = bin.path().to_string_lossy().into_owned();
    let ctx = Ctx::new(Path::new("/plane"), Env::of(&[("PATH", &path)]));
    assert_eq!(ctx.which("op"), Some(bin.path().join("op")));
    assert_eq!(ctx.which("vault"), None);
}

#[test]
fn a_recorded_string_has_every_resolved_value_masked() {
    let values = vec!["tok".to_string()];
    assert_eq!(
        redact_str("k=tok; again tok.", &values),
        "k=***; again ***."
    );
    // A value that is the whole text is masked too.
    assert_eq!(redact_str("tok", &values), "***");
    assert_eq!(redact_str("to", &values), "to");
}

#[cfg(unix)]
#[test]
fn a_vault_file_that_is_not_0600_is_reported_with_its_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("v.json");
    std::fs::write(&p, "{}").unwrap();
    chmod(&p, 0o600);
    assert_eq!(mode_note(&p), "");
    chmod(&p, 0o644);
    assert_eq!(mode_note(&p), "perms 644 (want 600)");
    chmod(&p, 0o4640);
    assert_eq!(mode_note(&p), "perms 640 (want 600)");
    assert_eq!(mode_note(&tmp.path().join("absent.json")), "");
}

/// `outer` (0755) / `state` / `vaults`, inside a 0700 temp directory.
#[cfg(unix)]
fn chain(state_mode: u32, vaults_mode: u32) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    chmod(tmp.path(), 0o700);
    let outer = tmp.path().join("outer");
    let state = outer.join("state");
    let vaults = state.join("vaults");
    std::fs::create_dir_all(&vaults).unwrap();
    chmod(&outer, 0o755);
    chmod(&state, state_mode);
    chmod(&vaults, vaults_mode);
    (tmp, state, vaults)
}

#[cfg(unix)]
#[test]
fn the_loose_directories_run_from_the_leaf_up_to_the_stop_and_no_further() {
    let (_tmp, state, vaults) = chain(0o750, 0o755);
    assert_eq!(
        loose_dirs(&vaults, &state),
        vec![(vaults.clone(), 0o755), (state.clone(), 0o750)],
        "outermost last, and the 0755 directory above the stop is not the vault's business"
    );
}

#[cfg(unix)]
#[test]
fn a_directory_only_its_owner_can_reach_is_not_loose() {
    let (_tmp, state, vaults) = chain(0o755, 0o700);
    assert_eq!(loose_dirs(&vaults, &state), vec![(state.clone(), 0o755)]);
    let (_tmp, state, vaults) = chain(0o700, 0o700);
    assert_eq!(loose_dirs(&vaults, &state), Vec::new());
}

#[cfg(unix)]
#[test]
fn a_leaf_outside_the_stop_answers_for_itself_alone() {
    let (tmp, state, _vaults) = chain(0o700, 0o700);
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    chmod(&elsewhere, 0o755);
    assert_eq!(loose_dirs(&elsewhere, &state), vec![(elsewhere, 0o755)]);
}

// ---------------------------------------------------------------------------------------------
// `secrets/plain_file.rs`
// ---------------------------------------------------------------------------------------------

#[test]
fn a_plain_file_vault_is_the_file_it_configures_and_refuses_without_one() {
    let ctx = Ctx::new(Path::new("/plane"), Env::of(&[]));
    let v = vault("app", "plain-file", json!({"file": "secrets/app.json"}));
    assert_eq!(
        plain_file::path(&ctx, &v).unwrap(),
        PathBuf::from("/plane/secrets/app.json")
    );
    assert_eq!(
        plain_file::file_path(&ctx, &v).unwrap(),
        PathBuf::from("/plane/secrets/app.json")
    );
    for none in [json!({}), json!({"file": ""}), json!({"file": 7})] {
        let e = plain_file::file_path(&ctx, &vault("app", "plain-file", none)).unwrap_err();
        assert_eq!(e.message, "vault 'app' has no 'file' configured");
    }
}

#[test]
fn a_plain_file_vault_reads_its_object_and_a_missing_file_as_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let v = vault("app", "plain-file", json!({"file": "app.json"}));
    assert_eq!(plain_file::load(&ctx, &v, "secret").unwrap(), Map::new());
    assert!(
        !tmp.path().join("app.json").exists(),
        "reading never writes"
    );
    std::fs::write(tmp.path().join("app.json"), r#"{"B": "2", "A": 1}"#).unwrap();
    let data = plain_file::load(&ctx, &v, "secret").unwrap();
    assert_eq!(data.get("B"), Some(&json!("2")));
    assert_eq!(
        plain_file::keys(&ctx, &v).unwrap(),
        ["A".to_string(), "B".to_string()]
    );
    assert_eq!(plain_file::get(&ctx, &v, "A").unwrap(), "1");
    std::fs::write(tmp.path().join("app.json"), r#"["A"]"#).unwrap();
    let e = plain_file::load(&ctx, &v, "secret").unwrap_err();
    assert!(
        e.message
            .ends_with("must be a JSON object of key -> secret"),
        "{}",
        e.message
    );
}

#[cfg(unix)]
#[test]
fn reading_a_value_first_takes_the_file_back_to_0600() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let v = vault("app", "plain-file", json!({"file": "app.json"}));
    let p = tmp.path().join("app.json");
    std::fs::write(&p, r#"{"K": "v"}"#).unwrap();
    chmod(&p, 0o644);
    assert_eq!(plain_file::get(&ctx, &v, "K").unwrap(), "v");
    assert_eq!(mode_of(&p), 0o600);
    // Only a group or other bit is repaired: an owner-only mode is the owner's choice.
    chmod(&p, 0o400);
    assert_eq!(plain_file::get(&ctx, &v, "K").unwrap(), "v");
    assert_eq!(mode_of(&p), 0o400);
}

#[cfg(unix)]
#[test]
fn a_private_write_lands_its_json_in_a_0600_file_even_over_a_looser_one() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("deeper/app.json");
    plain_file::write_private(&p, &json!({"K": "é"})).unwrap();
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "{\n  \"K\": \"é\"\n}\n"
    );
    assert_eq!(mode_of(&p), 0o600);
    assert_eq!(mode_of(p.parent().unwrap()), 0o700);
    chmod(&p, 0o644);
    plain_file::write_private(&p, &json!({})).unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "{}\n");
    assert_eq!(mode_of(&p), 0o600);
}

#[test]
fn a_plain_file_vault_remembers_when_each_key_was_set_and_forgets_a_deleted_one() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let v = vault("app", "plain-file", json!({"file": "app.json"}));
    plain_file::set(&ctx, &v, "A", "1", day("2026-09-01")).unwrap();
    plain_file::set(&ctx, &v, "B", "2", day("2026-09-10")).unwrap();
    // A key written by hand predates tracking.
    let mut data = plain_file::load(&ctx, &v, "secret").unwrap();
    data.insert("C".into(), json!("3"));
    plain_file::write_private(&tmp.path().join("app.json"), &Value::Object(data)).unwrap();
    assert_eq!(
        plain_file::ages(&ctx, &v, day("2026-09-11")).unwrap(),
        vec![
            ("A".to_string(), Some(10)),
            ("B".to_string(), Some(1)),
            ("C".to_string(), None)
        ]
    );
    plain_file::delete(&ctx, &v, "A").unwrap();
    let meta: Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join("app.meta.json")).unwrap())
            .unwrap();
    assert_eq!(meta, json!({"B": {"set_at": "2026-09-10"}}));
    assert_eq!(plain_file::keys(&ctx, &v).unwrap(), ["B", "C"]);
    let e = plain_file::delete(&ctx, &v, "A").unwrap_err();
    assert_eq!(e.kind, Kind::NotFound);
}

#[cfg(unix)]
#[test]
fn a_plain_file_vaults_health_counts_and_names_what_is_loose() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let v = vault(
        "app",
        "plain-file",
        json!({"file": ".charter/vaults/app.json"}),
    );
    let dir = tmp.path().join(".charter/vaults");
    std::fs::create_dir_all(&dir).unwrap();
    chmod(&tmp.path().join(".charter"), 0o700);
    chmod(&dir, 0o700);
    assert_eq!(
        plain_file::health(&ctx, &v),
        (
            true,
            "not created yet (.charter/vaults/app.json)".to_string()
        )
    );
    assert_eq!(plain_file::loose_dirs(&ctx, &v), Vec::new());

    let p = dir.join("app.json");
    std::fs::write(&p, r#"{"A": "1", "B": "2"}"#).unwrap();
    chmod(&p, 0o600);
    assert_eq!(
        plain_file::health(&ctx, &v),
        (true, "2 secret(s)".to_string())
    );
    chmod(&p, 0o644);
    chmod(&dir, 0o755);
    assert_eq!(plain_file::loose_dirs(&ctx, &v), vec![(dir.clone(), 0o755)]);
    assert_eq!(
        plain_file::health(&ctx, &v),
        (
            true,
            "2 secret(s), perms 644 (want 600), listed by other accounts: .charter/vaults 755 \
             (want 700 — chmod 700)"
                .to_string()
        )
    );
    assert_eq!(mode_of(&p), 0o644, "a health check never repairs");
    std::fs::remove_file(&p).unwrap();
    assert_eq!(
        plain_file::health(&ctx, &v),
        (
            true,
            "not created yet (.charter/vaults/app.json), listed by other accounts: \
             .charter/vaults 755 (want 700 — chmod 700)"
                .to_string()
        )
    );
}

// ---------------------------------------------------------------------------------------------
// `secrets/reference.rs`
// ---------------------------------------------------------------------------------------------

#[test]
fn a_scheme_starts_with_a_letter_and_holds_only_scheme_characters() {
    assert_eq!(reference::urlsplit("1op://v/i/f").scheme, "");
    assert_eq!(reference::urlsplit("o p://v/i/f").scheme, "");
    assert_eq!(reference::urlsplit("v+a.u-lt://x").scheme, "v+a.u-lt");
}

#[test]
fn a_1password_reference_is_read_with_op_read() {
    let (argv, cli) = reference::argv("op://Eng/deploy/token", "op").unwrap();
    assert_eq!(cli, "op");
    assert_eq!(
        argv,
        ["op", "read", "--no-newline", "op://Eng/deploy/token"]
    );
}

fn reference_plane(bins: &[&str]) -> (tempfile::TempDir, tempfile::TempDir, Ctx) {
    let tmp = tempfile::tempdir().unwrap();
    // The plane root is the vault file's directory: only its owner may list it.
    #[cfg(unix)]
    chmod(tmp.path(), 0o700);
    let bin = bin_dir(bins);
    let path = bin.path().to_string_lossy().into_owned();
    let ctx = Ctx::new(tmp.path(), Env::of(&[("PATH", &path)]));
    (tmp, bin, ctx)
}

#[test]
fn a_reference_vault_stores_uris_sorted_and_private_and_drops_them_by_name() {
    let (tmp, _bin, ctx) = reference_plane(&[]);
    let v = vault("refs", "reference", json!({"file": "refs.json"}));
    reference::set(&ctx, &v, "B", "vault://secret/app#K").unwrap();
    reference::set(&ctx, &v, "A", "  op://Eng/item/field\n").unwrap();
    // A browser reference is stored; it is refused only when read.
    reference::set(&ctx, &v, "C", "browser://anything").unwrap();
    assert_eq!(reference::keys(&ctx, &v).unwrap(), ["A", "B", "C"]);
    let p = tmp.path().join("refs.json");
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        "{\n  \"A\": \"op://Eng/item/field\",\n  \"B\": \"vault://secret/app#K\",\n  \"C\": \
         \"browser://anything\"\n}\n"
    );
    #[cfg(unix)]
    assert_eq!(mode_of(&p), 0o600);

    reference::delete(&ctx, &v, "B").unwrap();
    assert_eq!(reference::keys(&ctx, &v).unwrap(), ["A", "C"]);
    assert_eq!(
        reference::delete(&ctx, &v, "B").unwrap_err().kind,
        Kind::NotFound
    );
}

#[test]
fn a_malformed_reference_is_refused_when_stored_and_nothing_is_written() {
    let (tmp, _bin, ctx) = reference_plane(&[]);
    let v = vault("refs", "reference", json!({"file": "refs.json"}));
    let e = reference::set(&ctx, &v, "A", "op://vault-only").unwrap_err();
    assert!(
        e.message.contains("malformed 1Password reference"),
        "{}",
        e.message
    );
    assert!(reference::set(&ctx, &v, "A", "plain-value").is_err());
    assert!(!tmp.path().join("refs.json").exists());
}

#[test]
fn a_reference_vaults_health_names_the_clis_it_needs_and_what_is_wrong() {
    let v = vault("refs", "reference", json!({"file": "refs.json"}));
    let write = |ctx: &Ctx, body: &str| {
        let p = ctx.root.join("refs.json");
        std::fs::write(&p, body).unwrap();
        #[cfg(unix)]
        chmod(&p, 0o600);
    };

    let (_tmp, _bin, ctx) = reference_plane(&["op", "vault"]);
    assert_eq!(
        reference::health(&ctx, &v),
        (true, "not created yet (refs.json)".to_string())
    );
    write(&ctx, "{}");
    assert_eq!(
        reference::health(&ctx, &v),
        (true, "no references yet".to_string())
    );
    write(
        &ctx,
        r#"{"A": "op://v/i/f", "B": "vault://secret/app#K", "C": "op://v/i/g"}"#,
    );
    assert_eq!(
        reference::health(&ctx, &v),
        (true, "3 reference(s) via op, vault".to_string())
    );
    write(&ctx, r#"{"A": "op://v/i/f", "B": "plain", "C": 5}"#);
    assert_eq!(
        reference::health(&ctx, &v),
        (
            false,
            "3 reference(s), but 2 not a supported URI (charter vault verify names them)"
                .to_string()
        )
    );

    let (_tmp, _bin, ctx) = reference_plane(&[]);
    write(&ctx, r#"{"A": "op://v/i/f", "B": "vault://secret/app#K"}"#);
    assert_eq!(
        reference::health(&ctx, &v),
        (
            false,
            "2 reference(s), but not on PATH: op, vault".to_string()
        )
    );
    write(&ctx, r#"{"A": "browser://x", "B": "nope"}"#);
    assert_eq!(
        reference::health(&ctx, &v),
        (
            false,
            "2 reference(s), but 1 not a supported URI (charter vault verify names them); not \
             on PATH: npx"
                .to_string()
        )
    );
}

// ---------------------------------------------------------------------------------------------
// `secrets/registry.rs`
// ---------------------------------------------------------------------------------------------

#[test]
fn a_vault_name_is_a_letter_or_digit_then_name_characters_and_never_dot_dot() {
    for good in ["team", "a", "9", "a.b_c-d"] {
        assert!(registry::name_ok(good), "{good}");
    }
    for bad in ["", "-x", ".x", "a/b", "a b", "a..b", "a.."] {
        assert!(!registry::name_ok(bad), "{bad}");
    }
}

#[test]
fn a_decode_error_is_worded_the_way_python_words_it() {
    let e = serde_json::from_str::<Value>("{").unwrap_err();
    let said = registry::py_json_error(&e);
    assert!(said.ends_with(": line 1 column 1"), "{said}");
    assert!(!said.contains(" at line "), "{said}");
    assert!(said.len() > ": line 1 column 1".len(), "{said}");
}

#[test]
fn an_empty_registration_is_no_registration() {
    let doc = json!({"vaults": {"team": {}}});
    let e = registry::vault_in(doc.as_object().unwrap(), "team").unwrap_err();
    assert_eq!(e.kind, Kind::NotConfigured);
    assert!(
        e.message.starts_with("no vault named 'team'"),
        "{}",
        e.message
    );
}

fn registry_plane() -> (tempfile::TempDir, Ctx) {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    (tmp, ctx)
}

fn cfg(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

#[test]
fn a_vault_is_registered_locally_by_default_and_both_are_kept() {
    let (tmp, ctx) = registry_plane();
    registry::add_vault(
        &ctx,
        "a",
        "plain-file",
        cfg(json!({"file": "a.json"})),
        Some("ops"),
        false,
        false,
    )
    .unwrap();
    registry::add_vault(
        &ctx,
        "b",
        "reference",
        cfg(json!({"file": "b.json"})),
        None,
        false,
        false,
    )
    .unwrap();
    assert!(
        !tmp.path().join("vaults.json").exists(),
        "nothing was shared"
    );
    let local = tmp.path().join(".charter/vaults.json");
    #[cfg(unix)]
    assert_eq!(mode_of(&local), 0o600);
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&local).unwrap()).unwrap();
    assert_eq!(
        doc,
        json!({"vaults": {
            "a": {"provider": "plain-file", "persona": "ops", "config": {"file": "a.json"}},
            "b": {"provider": "reference", "persona": null, "config": {"file": "b.json"}},
        }})
    );
    assert_eq!(registry::scope_of(&ctx, "a"), "local");
    assert_eq!(
        registry::vault(&ctx, "a").unwrap().persona.as_deref(),
        Some("ops")
    );
}

#[test]
fn a_registry_half_whose_vaults_are_not_an_object_is_replaced_by_one() {
    let (tmp, ctx) = registry_plane();
    std::fs::create_dir_all(tmp.path().join(".charter")).unwrap();
    std::fs::write(
        tmp.path().join(".charter/vaults.json"),
        r#"{"vaults": "junk"}"#,
    )
    .unwrap();
    registry::add_vault(
        &ctx,
        "a",
        "plain-file",
        cfg(json!({"file": "a.json"})),
        None,
        false,
        false,
    )
    .unwrap();
    assert_eq!(registry::vault(&ctx, "a").unwrap().provider, "plain-file");
}

#[test]
fn a_shared_vault_goes_to_the_committed_half_and_its_account_stays_on_this_machine() {
    let (tmp, ctx) = registry_plane();
    registry::add_vault(
        &ctx,
        "team",
        "1password",
        cfg(json!({"op-vault": "Eng", "account": "me.1password.com"})),
        Some("devops"),
        false,
        true,
    )
    .unwrap();
    let shared_path = tmp.path().join("vaults.json");
    #[cfg(unix)]
    assert_eq!(mode_of(&shared_path), 0o644);
    let shared: Value =
        serde_json::from_str(&std::fs::read_to_string(&shared_path).unwrap()).unwrap();
    assert_eq!(
        shared,
        json!({"vaults": {"team": {"provider": "1password", "persona": "devops",
                                   "config": {"op-vault": "Eng"}}}})
    );
    let local: Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".charter/vaults.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        local,
        json!({"vaults": {"team": {"config": {"account": "me.1password.com"}}}})
    );
    assert_eq!(registry::scope_of(&ctx, "team"), "both");
    let v = registry::vault(&ctx, "team").unwrap();
    assert_eq!(config_str(&v.config, "account"), Some("me.1password.com"));
    assert_eq!(config_str(&v.config, "op-vault"), Some("Eng"));
}

#[test]
fn a_vault_shared_with_nothing_local_to_keep_is_shared_alone() {
    let (_tmp, ctx) = registry_plane();
    registry::add_vault(
        &ctx,
        "team",
        "plain-file",
        cfg(json!({"file": "t.json"})),
        None,
        false,
        true,
    )
    .unwrap();
    assert_eq!(registry::scope_of(&ctx, "team"), "shared");
}

#[test]
fn replacing_a_registration_is_refused_and_names_where_its_secrets_are() {
    let (_tmp, ctx) = registry_plane();
    registry::add_vault(
        &ctx,
        "a",
        "plain-file",
        cfg(json!({"file": "a.json"})),
        None,
        false,
        false,
    )
    .unwrap();
    let e =
        registry::add_vault(&ctx, "a", "reference", Map::new(), None, false, false).unwrap_err();
    assert!(
        e.message
            .starts_with("vault 'a' is already registered with provider 'plain-file' (a.json)."),
        "{}",
        e.message
    );
    registry::add_vault(
        &ctx,
        "b",
        "plain-file",
        cfg(json!({"file": ""})),
        None,
        false,
        false,
    )
    .unwrap();
    let e =
        registry::add_vault(&ctx, "b", "reference", Map::new(), None, false, false).unwrap_err();
    assert!(
        e.message
            .starts_with("vault 'b' is already registered with provider 'plain-file'. "),
        "{}",
        e.message
    );
}

#[test]
fn a_vault_is_removed_from_whichever_half_holds_it() {
    let (_tmp, ctx) = registry_plane();
    registry::add_vault(
        &ctx,
        "loc",
        "plain-file",
        cfg(json!({"file": "l.json"})),
        None,
        false,
        false,
    )
    .unwrap();
    registry::add_vault(
        &ctx,
        "sh",
        "plain-file",
        cfg(json!({"file": "s.json"})),
        None,
        false,
        true,
    )
    .unwrap();
    registry::remove_vault(&ctx, "loc").unwrap();
    registry::remove_vault(&ctx, "sh").unwrap();
    for name in ["loc", "sh"] {
        assert_eq!(
            registry::vault(&ctx, name).unwrap_err().kind,
            Kind::NotConfigured
        );
    }
    let e = registry::remove_vault(&ctx, "loc").unwrap_err();
    assert_eq!(e.kind, Kind::NotConfigured);
    assert_eq!(e.message, "no vault named 'loc'");
}

#[test]
fn identity_variables_are_both_halves_of_each_binding_by_vault() {
    let doc = json!({"vaults": {
        "a": {"config": {"env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_A_TOKEN"}}},
        "b": {"config": {}},
        "c": {"config": {"env": {"TARGET": ""}}},
    }});
    assert_eq!(
        registry::identity_vars(doc.as_object().unwrap()),
        vec![
            (
                "a".to_string(),
                vec![
                    "OP_SERVICE_ACCOUNT_TOKEN".to_string(),
                    "OP_A_TOKEN".to_string()
                ]
            ),
            ("b".to_string(), vec![]),
            ("c".to_string(), vec!["TARGET".to_string()]),
        ]
    );
}

#[test]
fn a_vault_reference_whose_path_or_field_reads_as_a_flag_is_refused() {
    for uri in ["vault://-rf/app#TOKEN", "vault://secret/app#-field"] {
        let e = reference::argv(uri, "vault").unwrap_err();
        assert!(e.message.starts_with("malformed Vault reference"), "{uri}");
    }
}

#[test]
fn a_stored_reference_is_answered_as_stored_and_a_missing_one_by_name() {
    let (_tmp, _bin, ctx) = reference_plane(&[]);
    let v = vault("refs", "reference", json!({"file": "refs.json"}));
    reference::set(&ctx, &v, "A", "op://Eng/item/field").unwrap();
    assert_eq!(
        reference::reference_for(&ctx, &v, "A").unwrap(),
        json!("op://Eng/item/field")
    );
    let e = reference::reference_for(&ctx, &v, "B").unwrap_err();
    assert_eq!(e.kind, Kind::NotFound);
    assert_eq!(e.message, "no secret 'B' in vault 'refs'");
}

/// A plane with one reference, `A`, and a stand-in `op` that prints `out` and exits `code`.
fn resolving_plane(out: &str, code: i32) -> (tempfile::TempDir, tempfile::TempDir, Ctx) {
    let tmp = tempfile::tempdir().unwrap();
    let bin = cli_dir(&["op"], out, code);
    let path = bin.path().to_string_lossy().into_owned();
    let ctx = Ctx::new(tmp.path(), Env::of(&[("PATH", &path)]));
    let v = vault("refs", "reference", json!({"file": "refs.json"}));
    reference::set(&ctx, &v, "A", "op://Eng/item/field").unwrap();
    (tmp, bin, ctx)
}

#[test]
fn a_reference_resolves_to_what_its_cli_printed() {
    let (_tmp, _bin, ctx) = resolving_plane("resolved-value", 0);
    let v = vault("refs", "reference", json!({"file": "refs.json"}));
    assert_eq!(reference::get(&ctx, &v, "A").unwrap(), "resolved-value");
}

#[test]
fn a_cli_that_fails_is_reported_by_its_exit_status_and_never_by_what_it_said() {
    let (_tmp, _bin, ctx) = resolving_plane("leaked-value", 3);
    let v = vault("refs", "reference", json!({"file": "refs.json"}));
    let e = reference::get(&ctx, &v, "A").unwrap_err();
    assert!(
        e.message
            .starts_with("resolving 'A' via op failed (exit 3)."),
        "{}",
        e.message
    );
    assert!(!e.message.contains("leaked-value"), "{}", e.message);
}

#[test]
fn a_badly_named_vault_is_refused_by_the_rule_it_broke() {
    let doc = json!({"vaults": {}});
    let e = registry::vault_in(doc.as_object().unwrap(), "a/b").unwrap_err();
    assert_eq!(e.kind, Kind::NotConfigured);
    assert_eq!(
        e.message,
        "'a/b' is not a vault name charter accepts: letters, digits, '.', '_' and '-', starting \
         with a letter or digit, and never '..'."
    );
}

#[test]
fn only_a_well_named_object_is_a_usable_vault() {
    let half = json!({"vaults": {
        "ok": {"provider": "plain-file"},
        "../escape": {"provider": "plain-file"},
        "text": "not an object",
    }});
    let usable = registry::usable_vaults(half.as_object().unwrap());
    assert_eq!(usable.keys().collect::<Vec<_>>(), ["ok"]);
}

#[test]
fn a_personas_vaults_are_the_ones_tagged_with_it_by_name() {
    let doc = json!({"vaults": {
        "zeta": {"persona": "ops"},
        "beta": {"persona": "dev"},
        "alpha": {"persona": "ops"},
        "untagged": {},
    }});
    let doc = doc.as_object().unwrap();
    assert_eq!(registry::vaults_for_persona(doc, "ops"), ["alpha", "zeta"]);
    assert_eq!(
        registry::vaults_for_persona(doc, "nobody"),
        Vec::<String>::new()
    );
}

#[test]
fn a_null_in_the_local_half_leaves_the_shared_field_as_it_was() {
    let (tmp, ctx) = registry_plane();
    std::fs::create_dir_all(tmp.path().join(".charter")).unwrap();
    std::fs::write(
        tmp.path().join("vaults.json"),
        r#"{"vaults": {"team": {"provider": "1password", "persona": "devops"}}}"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join(".charter/vaults.json"),
        r#"{"vaults": {"team": {"persona": null, "provider": "plain-file"}}}"#,
    )
    .unwrap();
    let v = registry::vault(&ctx, "team").unwrap();
    assert_eq!(v.persona.as_deref(), Some("devops"));
    assert_eq!(v.provider, "plain-file");
}

/// #356: a reference vault's file is `0600` from the instant it exists. Since #429 the bytes
/// go to a temp file beside it and are renamed over it, so the file that is ever opened for
/// writing is that temp — created 0600, seen at the open before any chmod could run — and a
/// vault left at `0644` by something else is never opened for writing at all.
#[cfg(unix)]
#[test]
fn a_reference_vault_is_0600_before_any_content_reaches_it() {
    use std::os::unix::fs::PermissionsExt;
    let (tmp, _bin, ctx) = reference_plane(&[]);
    let v = vault("refs", "reference", json!({"file": "refs.json"}));
    let p = tmp.path().join("refs.json");
    let seen: std::rc::Rc<std::cell::RefCell<Vec<(u32, u64)>>> = Default::default();
    let log = seen.clone();
    let _watch = crate::rewrite::hook::watch_created(move |file| {
        let meta = file.metadata().unwrap();
        log.borrow_mut()
            .push((meta.permissions().mode() & 0o777, meta.len()));
    });

    reference::set(&ctx, &v, "A", "op://Eng/item/field").unwrap();
    chmod(&p, 0o644);
    reference::set(&ctx, &v, "B", "op://Eng/item/other").unwrap();

    assert_eq!(*seen.borrow(), [(0o600, 0), (0o600, 0)]);
    assert_eq!(mode_of(&p), 0o600);
    assert_eq!(reference::keys(&ctx, &v).unwrap(), ["A", "B"]);
}

/// #429: a vault is replaced whole. A write that dies between the complete temp file and the
/// rename — where a crash lands — leaves the old vault whole and no temp beside it. Written in
/// place, the same crash left the vault truncated: every secret in it gone.
#[test]
fn a_vault_write_that_dies_before_its_rename_leaves_the_old_vault_whole() {
    for provider in ["plain-file", "reference"] {
        let (tmp, _bin, ctx) = reference_plane(&[]);
        let v = vault("app", provider, json!({"file": "vault/app.json"}));
        let set = |key: &str| match provider {
            "plain-file" => plain_file::set(&ctx, &v, key, "value", day("2026-09-01")),
            _ => reference::set(&ctx, &v, key, "op://Eng/item/field"),
        };
        set("A").unwrap();
        let p = tmp.path().join("vault/app.json");
        let before = std::fs::read_to_string(&p).unwrap();
        let _hook = crate::rewrite::hook::set(|target, temp| {
            assert!(std::fs::read_to_string(temp).unwrap().contains("\"B\""));
            assert!(!std::fs::read_to_string(target).unwrap().contains("\"B\""));
            Err(std::io::Error::other("killed"))
        });

        let err = set("B").unwrap_err();

        assert!(err.to_string().contains("killed"), "{provider}: {err}");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), before, "{provider}");
        let left: Vec<String> = std::fs::read_dir(p.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(left.is_empty(), "{provider}: {left:?}");
    }
}

/// #429: a vault path that is a symlink is refused, with a message that says so, and the file
/// it points at is untouched. Before, the secrets were written through the link to wherever
/// it pointed.
#[cfg(unix)]
#[test]
fn a_vault_that_is_a_symlink_is_refused_and_what_it_points_at_is_untouched() {
    for provider in ["plain-file", "reference"] {
        let (tmp, _bin, ctx) = reference_plane(&[]);
        let elsewhere = tempfile::tempdir().unwrap();
        let target = elsewhere.path().join("real.json");
        std::fs::write(&target, "{}\n").unwrap();
        let p = tmp.path().join("app.json");
        std::os::unix::fs::symlink(&target, &p).unwrap();
        let v = vault("app", provider, json!({"file": "app.json"}));

        let err = match provider {
            "plain-file" => plain_file::set(&ctx, &v, "K", "value", day("2026-09-01")),
            _ => reference::set(&ctx, &v, "K", "op://Eng/item/field"),
        }
        .unwrap_err();

        assert!(
            err.to_string().contains("is a symlink"),
            "{provider}: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "{}\n",
            "{provider}"
        );
        assert!(
            std::fs::symlink_metadata(&p).unwrap().is_symlink(),
            "{provider}: the link itself was replaced"
        );
    }
}

/// #429: the replaced vault, and its sidecar, are 0600 — a new one and one left looser.
#[cfg(unix)]
#[test]
fn a_replaced_vault_and_its_sidecar_are_0600() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let v = vault("app", "plain-file", json!({"file": "app.json"}));
    let p = tmp.path().join("app.json");
    plain_file::set(&ctx, &v, "A", "1", day("2026-09-01")).unwrap();
    chmod(&p, 0o644);
    chmod(&tmp.path().join("app.meta.json"), 0o644);
    plain_file::set(&ctx, &v, "B", "2", day("2026-09-02")).unwrap();
    assert_eq!(mode_of(&p), 0o600);
    assert_eq!(mode_of(&tmp.path().join("app.meta.json")), 0o600);
}
