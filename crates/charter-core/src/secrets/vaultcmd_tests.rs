//! `charter vault …` read back through a recording [`Io`], against planes in temp dirs. No
//! `op` is on any `PATH` here: a 1password vault is registered and reported, never read.

use serde_json::{Value, json};

use super::*;
use crate::secrets::cmd::tests::{Plane, Rec};

fn req(name: &str, provider: &str) -> AddRequest {
    AddRequest {
        name: name.into(),
        provider: provider.into(),
        ..Default::default()
    }
}

/// The local half's entry for `name`, as `vault add` wrote it.
fn local_entry(plane: &Plane, name: &str) -> Value {
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(plane.ctx.local_registry()).unwrap())
            .unwrap();
    doc["vaults"][name].clone()
}

fn git(root: &std::path::Path, args: &[&str]) {
    let mut c = std::process::Command::new("git");
    c.arg("-C").arg(root).args(args);
    assert!(crate::forklock::output(&mut c).unwrap().status.success());
}

#[test]
fn git_is_asked_whether_it_ignores_a_path_and_a_directory_that_is_no_repository_has_no_answer() {
    let plane = Plane::new(&[]);
    assert_eq!(git_ignores(plane.root(), &plane.root().join("x")), None);
    git(plane.root(), &["init", "-q"]);
    std::fs::write(plane.root().join(".gitignore"), "ignored\n").unwrap();
    assert_eq!(
        git_ignores(plane.root(), &plane.root().join("ignored")),
        Some(true)
    );
    assert_eq!(
        git_ignores(plane.root(), &plane.root().join("kept")),
        Some(false)
    );
}

#[test]
fn a_plain_file_vault_defaults_to_the_state_directory_written_relative_to_the_plane() {
    let plane = Plane::new(&[]);
    let mut io = Rec::default();
    assert_eq!(
        add(&plane.ctx, &req("p", "plain-file"), &mut io),
        0,
        "{}",
        io.said()
    );
    assert_eq!(
        local_entry(&plane, "p"),
        json!({"provider": "plain-file", "persona": null, "config": {"file": ".charter/vaults/p.json"}})
    );
    let said = io.said();
    assert!(
        said.contains("ok: Vault 'p' registered (provider: plain-file) [local only]."),
        "{said}"
    );
    assert!(said.contains("info:   Teammates won't see it."), "{said}");
    assert!(said.contains("info:   not created yet"), "{said}");
    assert!(
        said.ends_with("info:   add secrets with: charter secret set p <key> --stdin"),
        "{said}"
    );
}

#[test]
fn a_reference_vault_defaults_its_file_too_and_says_what_it_stores() {
    let plane = Plane::new(&[]);
    let mut io = Rec::default();
    assert_eq!(
        add(&plane.ctx, &req("r", "reference"), &mut io),
        0,
        "{}",
        io.said()
    );
    assert_eq!(
        local_entry(&plane, "r")["config"],
        json!({"file": ".charter/vaults/r.json"})
    );
    assert!(
        io.said()
            .contains("info:   stores op:// or vault:// URIs; values are fetched at read time."),
        "{}",
        io.said()
    );
}

#[test]
fn a_file_outside_the_plane_is_kept_as_given() {
    let plane = Plane::new(&[]);
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("v.json").to_string_lossy().into_owned();
    let request = AddRequest {
        file: Some(file.clone()),
        ..req("p", "plain-file")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 0, "{}", io.said());
    assert_eq!(local_entry(&plane, "p")["config"]["file"], file.as_str());
}

#[test]
fn a_1password_vault_needs_an_op_vault_and_takes_no_file_unless_given_one() {
    let plane = Plane::new(&[]);
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &req("o", "1password"), &mut io), 1);
    assert!(
        io.said()
            .starts_with("err: --op-vault is required for a 1password vault"),
        "{}",
        io.said()
    );

    let request = AddRequest {
        op_vault: Some("Eng".into()),
        op_item: Some("item".into()),
        account: Some("me.1password.com".into()),
        ..req("o", "1password")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 0, "{}", io.said());
    assert_eq!(
        local_entry(&plane, "o")["config"],
        json!({"op-vault": "Eng", "op-item": "item", "account": "me.1password.com"})
    );
    let said = io.said();
    assert!(said.contains("warn:   op CLI not on PATH"), "{said}");
    assert!(
        said.contains("one 1Password item, 'item' in vault 'Eng', tagged 'charter:o'"),
        "{said}"
    );

    let request = AddRequest {
        op_vault: Some("Eng".into()),
        file: Some("side.json".into()),
        ..req("f", "1password")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 0, "{}", io.said());
    assert_eq!(
        local_entry(&plane, "f")["config"],
        json!({"file": "side.json", "op-vault": "Eng"})
    );
}

#[test]
fn a_1password_setting_that_reads_as_a_flag_is_refused() {
    let plane = Plane::new(&[]);
    let request = AddRequest {
        op_vault: Some("--dangerous".into()),
        ..req("o", "1password")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 1);
    assert!(!plane.ctx.local_registry().exists());
}

#[test]
fn a_bad_name_or_an_unknown_persona_is_refused_before_anything_is_written() {
    let plane = Plane::new(&[]);
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &req("bad name", "plain-file"), &mut io), 1);
    let request = AddRequest {
        persona: Some("ghost".into()),
        ..req("p", "plain-file")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 1);
    assert!(io.said().contains("ghost"), "{}", io.said());
    assert!(!plane.ctx.local_registry().exists());
}

#[test]
fn an_identity_binding_is_stored_by_name_and_a_malformed_one_is_refused() {
    let plane = Plane::new(&[]);
    let request = AddRequest {
        token_env: Some("OP_TEAM_TOKEN".into()),
        env: vec![" OTHER = SOURCE_VAR ".into()],
        ..req("p", "plain-file")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 0, "{}", io.said());
    assert_eq!(
        local_entry(&plane, "p")["config"]["env"],
        json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN", "OTHER": "SOURCE_VAR"})
    );

    for (env, refusal) in [
        ("NOEQUALS", "--env expects TARGET=SOURCE, got 'NOEQUALS'"),
        (
            "1BAD=SRC",
            "--env target '1BAD' is not a valid environment variable name",
        ),
        (
            "OK=bad-src",
            "--env source 'bad-src' is not a valid environment variable name",
        ),
    ] {
        let request = AddRequest {
            env: vec![env.into()],
            ..req("q", "plain-file")
        };
        let mut io = Rec::default();
        assert_eq!(add(&plane.ctx, &request, &mut io), 1);
        assert_eq!(io.said(), format!("err: {refusal}"));
    }
}

#[test]
fn two_vaults_over_one_file_are_refused_but_a_vault_may_be_re_registered_over_its_own() {
    let plane = Plane::new(&[]);
    let shared = AddRequest {
        file: Some("secrets/v.json".into()),
        ..req("a", "reference")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &shared, &mut io), 0, "{}", io.said());

    let other = AddRequest {
        name: "b".into(),
        ..shared.clone()
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &other, &mut io), 1);
    assert!(
        io.said()
            .starts_with("err: 'secrets/v.json' is already the file of vault 'a'."),
        "{}",
        io.said()
    );

    let again = AddRequest {
        force: true,
        provider: "plain-file".into(),
        ..shared
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &again, &mut io), 0, "{}", io.said());
    assert!(
        io.said().contains(
            "warn: Replaced the previous 'a' registration (provider: reference). Its secrets \
             were NOT migrated. They remain at secrets/v.json."
        ),
        "{}",
        io.said()
    );
}

#[test]
fn replacing_a_vault_that_had_no_file_does_not_say_where_its_secrets_remain() {
    let plane = Plane::new(&[]);
    plane.register(
        "o",
        "1password",
        json!({"op-vault": "Eng", "file": ""}),
        None,
    );
    let request = AddRequest {
        force: true,
        ..req("o", "plain-file")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 0, "{}", io.said());
    assert!(
        io.said().contains(
            "warn: Replaced the previous 'o' registration (provider: 1password). Its secrets \
             were NOT migrated.\n"
        ),
        "{}",
        io.said()
    );
}

#[test]
fn a_plain_file_vault_git_would_commit_is_refused_and_a_reference_one_is_not() {
    let plane = Plane::new(&[]);
    git(plane.root(), &["init", "-q"]);
    let request = AddRequest {
        file: Some("v.json".into()),
        ..req("p", "plain-file")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 1);
    assert!(
        io.said()
            .starts_with("err: 'v.json' is inside the control plane and NOT gitignored"),
        "{}",
        io.said()
    );
    assert!(
        io.said().contains("the default under vaults/"),
        "{}",
        io.said()
    );

    let request = AddRequest {
        file: Some("v.json".into()),
        ..req("r", "reference")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 0, "{}", io.said());
}

#[test]
fn a_shared_vault_says_it_is_shared_and_does_not_tell_teammates_to_share_it() {
    let plane = Plane::new(&[]);
    let request = AddRequest {
        share: true,
        ..req("p", "plain-file")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 0, "{}", io.said());
    let said = io.said();
    assert!(said.contains("[shared — commit vaults.json]"), "{said}");
    assert!(!said.contains("Teammates won't see it"), "{said}");
    assert!(plane.ctx.shared_registry().exists());
}

#[test]
fn a_vault_added_for_a_persona_is_tagged_with_it_and_says_so() {
    let plane = Plane::new(&[]);
    plane.persona("ops", "role: x");
    let request = AddRequest {
        persona: Some("ops".into()),
        ..req("p", "plain-file")
    };
    let mut io = Rec::default();
    assert_eq!(add(&plane.ctx, &request, &mut io), 0, "{}", io.said());
    assert_eq!(local_entry(&plane, "p")["persona"], "ops");
    assert!(
        io.said().contains(
            "ok: Vault 'p' registered (provider: plain-file, persona: ops) [local only]."
        ),
        "{}",
        io.said()
    );
}

#[test]
fn list_of_a_registry_that_cannot_be_read_fails() {
    let plane = Plane::new(&[]);
    std::fs::create_dir_all(plane.ctx.state.clone()).unwrap();
    std::fs::write(plane.ctx.local_registry(), "not json").unwrap();
    let mut io = Rec::default();
    assert_eq!(list(&plane.ctx, &mut io), 1);
    assert!(io.said().starts_with("err: "), "{}", io.said());
    let mut io = Rec::default();
    assert_eq!(verify(&plane.ctx, None, &mut io), 1);
    assert!(io.said().starts_with("err: "), "{}", io.said());
}

#[test]
fn list_with_no_vaults_says_how_to_add_one() {
    let plane = Plane::new(&[]);
    let mut io = Rec::default();
    assert_eq!(list(&plane.ctx, &mut io), 0);
    assert_eq!(io.out(), "");
    assert!(io.said().starts_with("info: No vaults configured."));
}

#[test]
fn list_is_a_table_whose_status_never_holds_a_value() {
    let plane = Plane::new(&[]);
    plane.persona("ops", "role: x");
    plane.plain("aa", json!({"k": "list-leak-value"}));
    plane.register("bb", "1password", json!({"op-vault": "Eng"}), Some("ops"));
    plane.register("cc", "plain-file", json!({"file": "c.json"}), Some("ghost"));
    plane.register(
        "dd",
        "reference",
        json!({"file": "d.json", "env": {"OP_SERVICE_ACCOUNT_TOKEN": "UNSET_SRC"}}),
        Some(""),
    );
    let mut io = Rec::default();
    assert_eq!(list(&plane.ctx, &mut io), 0);
    let out = io.out();
    assert!(!out.contains("list-leak-value"));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 6, "{out}");
    assert_eq!(
        lines[0],
        "VAULT  PROVIDER    PERSONA                  SCOPE  STATUS"
    );
    assert_eq!(
        lines[1],
        "-----  ----------  -----------------------  -----  ------"
    );
    assert!(
        lines[2].starts_with("aa     plain-file  —                        local  1 secret(s)"),
        "{out}"
    );
    assert_eq!(
        lines[3],
        "bb     1password   ops                      local  op CLI not on PATH"
    );
    assert!(
        lines[4].starts_with("cc     plain-file  ghost (no such persona)  local  not created yet"),
        "{out}"
    );
    assert_eq!(
        lines[5],
        "dd     reference   —                        local  vault 'dd' is read through $UNSET_SRC, \
         which is unset. charter will not fall back to an ambient $OP_SERVICE_ACCOUNT_TOKEN: \
         that would read this vault under an identity it does not declare, and the failure \
         would look like a missing secret rather than a wrong credential."
    );
}

#[test]
fn verify_resolves_every_key_and_says_how_many() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"a": "1", "b": "2"}));
    plane.plain("e", json!({}));
    let mut io = Rec::default();
    assert_eq!(verify(&plane.ctx, Some("p"), &mut io), 0);
    assert_eq!(io.said(), "ok: p: 2 reference(s) resolved");
    let mut io = Rec::default();
    assert_eq!(verify(&plane.ctx, None, &mut io), 0);
    let said = io.said();
    assert!(said.contains("info: e: no references to verify"), "{said}");
    assert!(said.contains("ok: p: 2 reference(s) resolved"), "{said}");
}

#[test]
fn verify_names_each_reference_that_does_not_resolve_and_fails() {
    let plane = Plane::new(&[]);
    let file = plane.ctx.vaults_dir().join("r.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, r#"{"bad": "raw-value", "gone": "op://Eng/x/y"}"#).unwrap();
    plane.register(
        "r",
        "reference",
        json!({"file": file.to_string_lossy()}),
        None,
    );
    let mut io = Rec::default();
    assert_eq!(verify(&plane.ctx, Some("r"), &mut io), 1);
    let said = io.said();
    assert!(
        said.starts_with("err: r: 2 of 2 reference(s) did NOT resolve"),
        "{said}"
    );
    assert!(
        said.ends_with(
            "the vault being reachable says nothing about the item behind the reference."
        ),
        "{said}"
    );
    let out = io.out();
    assert!(
        out.starts_with("    bad: 'bad' in vault 'r' is not a supported reference"),
        "{out}"
    );
    assert!(
        out.contains("\n    gone: 'gone' needs the 'op' CLI"),
        "{out}"
    );
    assert!(!out.contains("raw-value"), "{out}");
}

#[test]
fn verify_of_a_vault_that_cannot_be_listed_fails_on_it_whole() {
    let plane = Plane::new(&[]);
    let file = plane.plain("p", json!({}));
    std::fs::write(&file, "not json").unwrap();
    let mut io = Rec::default();
    assert_eq!(verify(&plane.ctx, Some("p"), &mut io), 1);
    assert!(
        io.said()
            .starts_with("err: p: 1 of 1 reference(s) did NOT resolve"),
        "{}",
        io.said()
    );
    assert!(io.out().starts_with("    *: vault file "), "{}", io.out());
}

#[test]
fn verify_of_a_vault_that_is_not_registered_has_nothing_to_verify() {
    let plane = Plane::new(&[]);
    let mut io = Rec::default();
    assert_eq!(verify(&plane.ctx, Some("nope"), &mut io), 0);
    let said = io.said();
    assert!(said.starts_with("err: nope: "), "{said}");
    assert!(said.ends_with("info: No vaults to verify."), "{said}");
}

#[test]
fn remove_unregisters_and_fails_on_a_vault_that_is_not_there() {
    let plane = Plane::new(&[]);
    plane.plain("p", json!({"k": "v"}));
    let mut io = Rec::default();
    assert_eq!(remove(&plane.ctx, "p", &mut io), 0);
    assert!(
        io.said()
            .starts_with("ok: Vault 'p' removed from the registry.")
    );
    assert!(
        plane.ctx.vaults_dir().join("p.json").exists(),
        "the file stays"
    );
    let mut io = Rec::default();
    assert_eq!(remove(&plane.ctx, "p", &mut io), 1);
    assert!(io.said().starts_with("err: "));
}

#[test]
fn a_keyring_vault_is_said_to_keep_its_key_names_in_its_index() {
    let plane = Plane::new(&[]);
    let mut io = Rec::default();
    assert_eq!(
        add(&plane.ctx, &req("k", "keyring"), &mut io),
        0,
        "{}",
        io.said()
    );
    let said = io.said();
    assert!(
        said.contains(
            // Not from its first word: the command-name scan reads `charter <word>` in a literal
            // here as a command this file suggests.
            "keeps each secret as one item in the system keyring, and the key names — never \
             the values — in .charter/vaults/k.keys.json."
        ),
        "{said}"
    );
    assert!(
        said.ends_with("info:   add secrets with: charter secret set k <key> --stdin"),
        "{said}"
    );
}

#[test]
fn removing_a_keyring_vault_says_its_secrets_stay_and_where_they_are_named() {
    let plane = Plane::new(&[]);
    plane.register("k", "keyring", json!({}), None);
    plane.plain("p", json!({"x": "v"}));
    let mut io = Rec::default();
    assert_eq!(remove(&plane.ctx, "k", &mut io), 0);
    assert_eq!(
        io.said(),
        "ok: Vault 'k' removed from the registry. (Any underlying file is left on disk \
         untouched.)\ninfo:   Its secrets stay in the system keyring, named in \
         .charter/vaults/k.keys.json; registering 'k' again as a keyring vault finds them."
    );
    // A vault of any other provider says nothing of a keyring.
    let mut io = Rec::default();
    assert_eq!(remove(&plane.ctx, "p", &mut io), 0);
    assert!(!io.said().contains("keyring"), "{}", io.said());
}

/// A keyring that refuses every delete, as a locked or unreachable one does.
struct RefusesDeletes(crate::secrets::keyring::FileStore);

impl crate::secrets::keyring::Store for RefusesDeletes {
    fn get(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<crate::secrets::keyring::Secret>, crate::secrets::VaultError> {
        self.0.get(service, account)
    }
    fn set(
        &self,
        service: &str,
        account: &str,
        value: &str,
    ) -> Result<(), crate::secrets::VaultError> {
        self.0.set(service, account, value)
    }
    fn delete(&self, _service: &str, _account: &str) -> Result<bool, crate::secrets::VaultError> {
        Err(crate::secrets::VaultError::new("the keyring is locked"))
    }
}

#[test]
fn destroying_a_keyring_vault_deletes_every_entry_from_the_keyring_and_then_unregisters_it() {
    use crate::secrets::keyring;
    let plane = Plane::new(&[]);
    plane.register("k", "keyring", json!({}), None);
    let v = registry::vault(&plane.ctx, "k").unwrap();
    keyring::set(&plane.ctx, &v, "B_TOKEN", "destroy-value-b-51").unwrap();
    keyring::set(&plane.ctx, &v, "A_TOKEN", "destroy-value-a-37").unwrap();
    let service = keyring::load_index(&plane.ctx, &v)
        .unwrap()
        .service
        .unwrap();

    let gone = destroy(&plane.ctx, "k").unwrap();

    assert_eq!(gone.provider, "keyring");
    assert_eq!(gone.destroyed, ["A_TOKEN", "B_TOKEN"]);
    let store = keyring::store(&plane.ctx);
    for key in ["A_TOKEN", "B_TOKEN"] {
        assert!(
            store.get(&service, key).unwrap().is_none(),
            "{key} is still in the keyring"
        );
    }
    assert!(
        !keyring::index_path(&plane.ctx, &v).exists(),
        "the keys index is left behind"
    );
    assert!(
        registry::vault(&plane.ctx, "k").is_err(),
        "still registered"
    );
}

#[test]
fn a_keyring_entry_that_cannot_be_deleted_leaves_the_vault_registered_to_try_again() {
    use crate::secrets::keyring;
    let plane = Plane::new(&[]);
    plane.register("k", "keyring", json!({}), None);
    let v = registry::vault(&plane.ctx, "k").unwrap();
    keyring::set(&plane.ctx, &v, "A_TOKEN", "destroy-value-a-37").unwrap();
    let locked = RefusesDeletes(keyring::FileStore::at(
        plane.ctx.state.join(keyring::STUB_FILE),
    ));

    let refused = destroy_with(&locked, &plane.ctx, "k").unwrap_err();

    assert!(refused.message.contains("locked"), "{}", refused.message);
    assert!(
        registry::vault(&plane.ctx, "k").is_ok(),
        "unregistered anyway"
    );
    assert_eq!(keyring::keys(&plane.ctx, &v).unwrap(), ["A_TOKEN"]);
}

#[test]
fn destroying_a_plain_file_vault_unregisters_it_and_leaves_its_file_on_disk() {
    let plane = Plane::new(&[]);
    let file = plane.plain("p", json!({"k": "v"}));

    let gone = destroy(&plane.ctx, "p").unwrap();

    assert_eq!(gone.provider, "plain-file");
    assert!(gone.destroyed.is_empty(), "{:?}", gone.destroyed);
    assert!(file.exists(), "the file went");
    assert!(
        registry::vault(&plane.ctx, "p").is_err(),
        "still registered"
    );
}

#[test]
fn destroying_a_vault_that_is_not_registered_is_refused() {
    let plane = Plane::new(&[]);
    let refused = destroy(&plane.ctx, "nope").unwrap_err();
    assert!(refused.message.contains("nope"), "{}", refused.message);
}
