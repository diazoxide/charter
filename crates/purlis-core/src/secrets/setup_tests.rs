//! The guided set-up (#1527), against planes in temp dirs, the stub keyring of a fenced build
//! and a stand-in `op` beside them. Nothing reaches the real HOME, a keychain or 1Password.
//!
//! **The stand-in echoes whatever token it is given**, on both streams, whenever it fails, and
//! writes its arguments beside itself: a token that reached an answer, a refusal, a registry
//! file or an argument fails a test here.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::*;
use crate::secrets::cmd::Say;
use crate::secrets::cmd::tests::Rec;
use crate::secrets::vaultcmd::{self, AddRequest};
use crate::secrets::{Env, env_overlay};

/// A made-up token the stand-in signs in with: any that holds `works`.
const GOOD: &str = "fixture-word-that-works-1527";
/// One it refuses.
const BAD: &str = "fixture-word-refused-1527";
const OTHER: &str = "fixture-word-works-too-1527";

/// A project, a folder holding a stand-in `op`, and a context whose PATH is that folder.
struct Set {
    tmp: tempfile::TempDir,
    bin: tempfile::TempDir,
    ctx: Ctx,
}

impl Set {
    /// `refusal` is what the stand-in prints for a token it refuses.
    fn new(refusal: &str) -> Self {
        Self::with_env(refusal, &[])
    }

    fn with_env(refusal: &str, env: &[(&str, &str)]) -> Self {
        crate::secrets::program::stand_ins_live_in_temp_folders();
        let tmp = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$0.args"
case "$1 $2" in
  "account list")
    printf '%s' '[{{"url":"my.1password.com","email":"a@example.test","user_uuid":"AAAA1111"}},{{"url":"acme.1password.eu","email":"b@example.test","user_uuid":"BBBB2222"}},{{"url":"acme.1password.eu","email":"c@example.test","user_uuid":"CCCC3333"}},{{"url":"--evil","email":"d@example.test","user_uuid":"DDDD4444"}}]'
    exit 0;;
esac
case "$OP_SERVICE_ACCOUNT_TOKEN" in
  *works*) ;;
  *)
    echo "[ERROR] {refusal} given=$OP_SERVICE_ACCOUNT_TOKEN" >&2
    echo "$OP_SERVICE_ACCOUNT_TOKEN"
    exit 1;;
esac
case "$1 $2" in
  "item list") printf '%s' '[{{"title":"charter-team"}},{{"title":"another"}}]';;
  "vault list") printf '%s' '[{{"name":"Ops"}},{{"name":"Engineering"}},{{"name":"-rf"}},{{"name":"two\nlines"}}]';;
  *) printf '%s' '{{}}';;
esac
"#
        );
        stand_in::program(bin.path(), "op", &script);
        let mut vars: Vec<(&str, &str)> = env.to_vec();
        let path = bin.path().to_string_lossy().into_owned();
        vars.push(("PATH", &path));
        let ctx = Ctx::new(tmp.path(), Env::of(&vars));
        Self { tmp, bin, ctx }
    }

    /// Every argument line the stand-in was run with.
    fn args(&self) -> String {
        std::fs::read_to_string(self.bin.path().join("op.args")).unwrap_or_default()
    }

    /// The stub keyring's items, as `service\naccount` to value.
    fn items(&self) -> serde_json::Map<String, Value> {
        std::fs::read_to_string(self.ctx.state.join(keyring::STUB_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// The same project as a process with no environment at all sees it.
    fn bare(&self) -> Ctx {
        Ctx::new(self.tmp.path(), Env::of(&[]))
    }

    fn local(&self) -> Value {
        Value::Object(registry::load_local(&self.ctx).unwrap())
    }

    /// Every file under the project, read as text: where a token must never be.
    fn every_file(&self) -> String {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(self.tmp.path(), &mut files);
        files
            .iter()
            .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some(keyring::STUB_FILE))
            .map(|p| String::from_utf8_lossy(&std::fs::read(p).unwrap_or_default()).into_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Write the committed half.
    fn commit(&self, vaults: Value) {
        std::fs::write(
            self.ctx.shared_registry(),
            json!({ "vaults": vaults }).to_string(),
        )
        .unwrap();
    }
}

fn place(op_vault: &str) -> Place {
    Place {
        op_vault: op_vault.into(),
        ..Default::default()
    }
}

fn request(name: &str, token: &str) -> Request {
    Request {
        name: name.into(),
        place: place("Engineering"),
        persona: None,
        sign_in: SignIn::Token(token.into()),
        share: false,
        force: false,
        also: Vec::new(),
    }
}

/// A 1Password vault `name` in this machine's half, read through `$source`.
fn bound(set: &Set, name: &str, op_vault: &str, source: &str) {
    let config = json!({"op-vault": op_vault, "env": {"OP_SERVICE_ACCOUNT_TOKEN": source}});
    registry::add_vault(
        &set.ctx,
        name,
        "1password",
        config.as_object().unwrap().clone(),
        None,
        false,
        false,
    )
    .unwrap();
}

fn tick(all: &[Alike], name: &str) -> Tick {
    let one = all.iter().find(|a| a.name == name).expect(name);
    Tick {
        name: one.name.clone(),
        digest: one.digest.clone(),
    }
}

// ---- the test ---------------------------------------------------------------------------

#[test]
fn a_test_reads_item_names_only_and_writes_nothing() {
    let set = Set::new("unused");

    let tested = test(
        &set.ctx,
        "team",
        &SignIn::Token(format!("  {GOOD}\n")),
        &place("Engineering"),
    )
    .unwrap();

    assert_eq!(
        tested,
        Tested {
            items: 2,
            item: "charter-team".into(),
            item_there: true
        }
    );
    // One run, of the listing, and the token is in no argument.
    assert_eq!(set.args(), "item list --vault Engineering --format json\n");
    assert!(!set.ctx.local_registry().exists());
    assert!(!set.ctx.shared_registry().exists());
    assert!(set.items().is_empty());
}

#[test]
fn a_test_says_whether_the_vaults_own_item_is_there_yet() {
    let set = Set::new("unused");
    let tested = test(
        &set.ctx,
        "fresh",
        &SignIn::Token(GOOD.into()),
        &place("Ops"),
    )
    .unwrap();
    assert_eq!(tested.item, "charter-fresh");
    assert!(!tested.item_there);
}

#[test]
fn a_failed_test_is_one_of_four_kinds_in_purlis_own_words_and_never_the_programs() {
    for (printed, kind, says) in [
        ("You've been rate-limited", Kind::TryAgain, "rate-limited"),
        (
            "dial tcp: lookup my.1password.com: no such host",
            Kind::TryAgain,
            "could not reach 1Password",
        ),
        (
            "401: Unauthorized",
            Kind::SignIn,
            "refused the sign-in with this token",
        ),
        (
            "failed to DecodeSACredentials",
            Kind::SignIn,
            "refused the sign-in with this token",
        ),
        (
            "Engineering isn't a vault in this account",
            Kind::Other,
            "has no vault of that name",
        ),
        (
            "something nobody has seen",
            Kind::Other,
            "did not recognise why",
        ),
    ] {
        let set = Set::new(printed);

        let failed = test(
            &set.ctx,
            "team",
            &SignIn::Token(BAD.into()),
            &place("Engineering"),
        )
        .unwrap_err();

        assert_eq!(failed.kind, kind, "{printed}");
        assert!(failed.why.contains(says), "{printed}: {}", failed.why);
        // The stand-in printed the token on both streams, and its own words.
        assert!(!failed.why.contains(BAD), "{}", failed.why);
        assert!(!failed.why.contains("given="), "{}", failed.why);
        assert!(!format!("{failed:?}").contains(BAD));
        assert!(!set.ctx.local_registry().exists(), "{printed}");
        assert!(set.items().is_empty(), "{printed}");
    }
}

#[test]
fn a_test_with_no_program_to_run_says_so_as_its_own_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let ctx = Ctx::new(
        tmp.path(),
        Env::of(&[("PATH", &empty.path().to_string_lossy())]),
    );
    let failed = test(&ctx, "team", &SignIn::Token(GOOD.into()), &place("Ops")).unwrap_err();
    assert_eq!(failed.kind, Kind::Program);
    assert!(failed.why.contains("the 1Password CLI"), "{}", failed.why);
    assert!(!failed.why.contains(GOOD));
}

#[test]
fn nothing_that_reads_as_an_option_reaches_the_program() {
    let set = Set::new("unused");
    let token = SignIn::Token(GOOD.into());
    for bad in [
        Place {
            op_vault: "--vault=Private".into(),
            ..Default::default()
        },
        Place {
            op_vault: "Ops".into(),
            op_item: Some("-x".into()),
            ..Default::default()
        },
        Place {
            op_vault: "Ops".into(),
            account: Some("--account=other".into()),
            ..Default::default()
        },
        Place {
            op_vault: "Ops".into(),
            account: Some("my.1password.com --debug".into()),
            ..Default::default()
        },
        Place {
            op_vault: "Ops\n--debug".into(),
            ..Default::default()
        },
    ] {
        let failed = test(&set.ctx, "team", &token, &bad).unwrap_err();
        assert_eq!(failed.kind, Kind::Other, "{bad:?}");
    }
    assert_eq!(set.args(), "", "no program was run for any of them");
    assert!(op_vaults(&set.ctx, &token, Some("-h")).is_err());
    assert_eq!(set.args(), "");
}

#[test]
fn a_sign_in_address_is_an_address_and_a_pasted_link_is_read_as_one() {
    for (given, pinned) in [
        ("my.1password.com", "my.1password.com"),
        (" acme.1password.eu ", "acme.1password.eu"),
        ("https://team.1password.ca/", "team.1password.ca"),
        ("sso.example-corp.com", "sso.example-corp.com"),
        ("ABCD1234EFGH", "ABCD1234EFGH"),
    ] {
        assert_eq!(
            clean_account(Some(given)).unwrap().as_deref(),
            Some(pinned),
            "{given}"
        );
    }
    assert_eq!(clean_account(Some("  ")).unwrap(), None);
    assert_eq!(clean_account(None).unwrap(), None);
    for bad in [
        "-a", "a b", "a;b", "a/b", "$(x)", ".hidden", "a\tb", "é.com",
    ] {
        assert!(clean_account(Some(bad)).is_err(), "{bad}");
    }
}

#[test]
fn a_pasted_token_loses_the_whitespace_around_it_and_one_with_any_inside_is_refused() {
    assert_eq!(clean_token(&format!("\n {GOOD}\r\n")).unwrap(), GOOD);
    for bad in [
        "",
        " \n",
        "two words",
        "line\nbreak",
        "tab\tbed",
        "zero\u{200b}width",
        "right\u{202e}left",
        "nul\0byte",
    ] {
        let refused = clean_token(bad).unwrap_err();
        assert!(
            bad.trim().is_empty() || !refused.message.contains(bad),
            "{}",
            refused.message
        );
    }
}

#[test]
fn the_listed_vaults_are_names_on_one_line_that_can_be_passed_on() {
    let set = Set::new("unused");
    assert_eq!(
        op_vaults(&set.ctx, &SignIn::Token(GOOD.into()), None).unwrap(),
        ["Engineering", "Ops"]
    );
    assert_eq!(set.args(), "vault list --format json\n");
    let failed = op_vaults(&set.ctx, &SignIn::Token(BAD.into()), None).unwrap_err();
    assert!(!failed.why.contains(BAD));
}

#[test]
fn the_accounts_the_app_lists_are_offered_by_address_and_two_on_one_address_by_user() {
    let set = Set::new("unused");
    let listed = accounts(&set.ctx).unwrap();
    let pins: Vec<(&str, &str)> = listed
        .iter()
        .map(|a| (a.address.as_str(), a.pin.as_str()))
        .collect();
    assert_eq!(
        pins,
        [
            ("my.1password.com", "my.1password.com"),
            ("acme.1password.eu", "BBBB2222"),
            ("acme.1password.eu", "CCCC3333"),
        ]
    );
    assert_eq!(set.args(), "account list --format json\n");
}

// ---- create -----------------------------------------------------------------------------

#[test]
fn a_create_registers_the_vault_and_writes_its_record_together() {
    let set = Set::new("unused");

    let marked = create(&set.ctx, &request("team", &format!("{GOOD}\n"))).unwrap();

    assert_eq!(marked, Marked::default());
    let entry = &set.local()["vaults"]["team"];
    assert_eq!(entry["provider"], "1password");
    assert_eq!(entry["config"]["op-vault"], "Engineering");
    assert_eq!(entry["config"]["token"], "keyring");
    assert_eq!(entry["config"]["env"], Value::Null, "no variable is bound");
    let rec = &entry["config"]["identity"];
    assert_eq!(rec["held"], "keyring");
    assert_eq!(
        rec["bindings"],
        json!({"OP_SERVICE_ACCOUNT_TOKEN": "service-account-token"})
    );
    assert_eq!(rec["op_vault"], "Engineering");
    assert_eq!(rec["op_item"], "charter-team");
    assert_eq!(
        rec["op_cmd"],
        set.bin.path().join("op").to_string_lossy().as_ref()
    );
    let id = rec["ids"]["service-account-token"].as_str().unwrap();
    // One item, the cleaned token, under this vault's own random id.
    assert_eq!(
        Value::Object(set.items()),
        json!({ format!("purlis/@identity/{id}\nservice-account-token"): GOOD })
    );
    // Nothing is committed, and the token is in no file of the project.
    assert!(!set.ctx.shared_registry().exists());
    assert!(!set.every_file().contains(GOOD));
    // Read with no variable anywhere, by a process started with an empty environment.
    let bare = set.bare();
    let vault = registry::vault(&bare, "team").unwrap();
    assert_eq!(
        env_overlay(&bare, &vault).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), GOOD.to_string())]
    );
    assert_eq!(
        identity::held(&bare, &vault)[0].held,
        identity::Held::Keyring
    );
}

#[test]
fn a_shared_create_commits_the_declaration_and_keeps_the_record_and_account_here() {
    let set = Set::new("unused");
    let mut req = request("team", GOOD);
    req.share = true;
    req.place.account = Some("acme.1password.eu".into());

    create(&set.ctx, &req).unwrap();

    let shared = Value::Object(registry::load_shared(&set.ctx).unwrap());
    assert_eq!(
        shared["vaults"]["team"]["config"],
        json!({"op-vault": "Engineering", "token": "keyring"})
    );
    let local = &set.local()["vaults"]["team"]["config"];
    assert_eq!(local["account"], "acme.1password.eu");
    assert_eq!(local["identity"]["held"], "keyring");
    assert!(!set.every_file().contains(GOOD));
}

#[test]
fn a_create_the_keyring_refuses_registers_nothing() {
    let set = Set::new("unused");
    // A keyring that cannot be written: a folder where its file belongs.
    std::fs::create_dir_all(set.ctx.state.join(keyring::STUB_FILE)).unwrap();

    let refused = create(&set.ctx, &request("team", GOOD)).unwrap_err();

    assert!(!refused.message.contains(GOOD), "{}", refused.message);
    assert!(registry::vault(&set.ctx, "team").is_err());
    assert!(!set.ctx.local_registry().exists());
}

#[test]
fn a_create_whose_registry_write_fails_leaves_no_token_and_no_vault() {
    use std::os::unix::fs::PermissionsExt;
    let set = Set::new("unused");
    bound(&set, "older", "Ops", "OP_TEAM_TOKEN");
    let was = std::fs::read_to_string(set.ctx.local_registry()).unwrap();
    // The committed half cannot be written: nothing can be made in the project's folder. This
    // machine's half, in the state folder under it, still can, so the token is stored first.
    let mode = |m: u32| {
        std::fs::set_permissions(set.tmp.path(), std::fs::Permissions::from_mode(m)).unwrap();
    };
    let mut req = request("team", GOOD);
    req.share = true;

    mode(0o500);
    // An account the modes do not hold (root, in some containers) can write all the same: there
    // is then no failure to make here, and the test has nothing to say.
    let probe = set.tmp.path().join("probe");
    if std::fs::write(&probe, "").is_ok() {
        mode(0o700);
        return;
    }
    let refused = create(&set.ctx, &req);
    mode(0o700);

    let refused = refused.unwrap_err();
    assert!(!refused.message.contains(GOOD), "{}", refused.message);
    assert!(set.items().is_empty(), "a token nothing refers to");
    assert!(!set.ctx.shared_registry().exists());
    let now: Value =
        serde_json::from_str(&std::fs::read_to_string(set.ctx.local_registry()).unwrap()).unwrap();
    assert_eq!(now, serde_json::from_str::<Value>(&was).unwrap());
}

#[test]
fn a_committed_entry_that_adds_to_the_new_vault_undoes_the_whole_create() {
    // What a `git pull`, or a chat that writes project files, can do between the test and the
    // create: name the vault in the committed half with more than the person was shown.
    for extra in [
        json!({"env": {"HTTPS_PROXY": "SOME_VARIABLE"}}),
        json!({"op-item": "another-item"}),
    ] {
        let set = Set::new("unused");
        set.commit(json!({"team": {"provider": "1password", "persona": null, "config": extra}}));
        let mut req = request("team", GOOD);
        req.force = true;

        let refused = create(&set.ctx, &req).unwrap_err();

        assert!(
            refused.message.contains("the committed registry"),
            "{}",
            refused.message
        );
        assert!(set.items().is_empty(), "{extra}: the token was left behind");
        assert!(!set.ctx.local_registry().exists(), "{extra}");
        let vault = registry::vault(&set.ctx, "team").unwrap();
        assert!(!identity::in_keyring(&set.ctx, &vault));
    }
}

#[test]
fn a_committed_persona_on_the_new_vault_undoes_the_create_too() {
    let set = Set::new("unused");
    std::fs::create_dir_all(set.tmp.path().join("personas/intruder")).unwrap();
    set.commit(json!({"team": {"provider": "1password", "persona": "intruder", "config": {}}}));
    let mut req = request("team", GOOD);
    req.force = true;
    assert!(create(&set.ctx, &req).is_err());
    assert!(set.items().is_empty());
}

#[test]
fn a_name_already_registered_is_refused_before_anything_is_stored() {
    let set = Set::new("unused");
    bound(&set, "team", "Ops", "OP_TEAM_TOKEN");
    let refused = create(&set.ctx, &request("team", GOOD)).unwrap_err();
    assert!(refused.message.contains("already registered"));
    assert!(set.items().is_empty());
}

#[test]
fn a_forced_create_replaces_the_record_and_deletes_the_token_it_replaced() {
    let set = Set::new("unused");
    create(&set.ctx, &request("team", GOOD)).unwrap();
    let mut again = request("team", OTHER);
    again.force = true;

    create(&set.ctx, &again).unwrap();

    let items = set.items();
    assert_eq!(items.len(), 1, "{:?}", items.keys());
    assert_eq!(items.values().next().unwrap(), OTHER);
}

#[test]
fn the_app_sign_in_registers_the_account_pin_and_no_credential() {
    let set = Set::new("unused");
    let mut req = request("mine", "unused");
    req.sign_in = SignIn::App;
    req.place.account = Some("https://acme.1password.eu/".into());

    create(&set.ctx, &req).unwrap();

    assert_eq!(
        set.local()["vaults"]["mine"]["config"],
        json!({"op-vault": "Engineering", "account": "acme.1password.eu"})
    );
    assert!(set.items().is_empty());
    let vault = registry::vault(&set.ctx, "mine").unwrap();
    assert!(identity::bindings(&vault).is_empty());
}

// ---- what the declaration means ---------------------------------------------------------

#[test]
fn a_committed_declaration_makes_no_record_and_names_no_variable_to_read() {
    // The committed half may say a vault keeps its token in the keyring. It cannot make this
    // machine have one, and a variable of the kept source's name is never read for it.
    let set = Set::with_env(
        "unused",
        &[
            ("service-account-token", BAD),
            ("OP_SERVICE_ACCOUNT_TOKEN", BAD),
        ],
    );
    set.commit(
        json!({"pulled": {"provider": "1password", "persona": null, "config": {
        "op-vault": "Ops", "token": "keyring",
        "identity": {"held": "keyring", "ids": {"service-account-token": "0000000000000000"}}}}}),
    );
    let vault = registry::vault(&set.ctx, "pulled").unwrap();

    assert_eq!(
        identity::held(&set.ctx, &vault)[0].held,
        identity::Held::Unset
    );
    let refused = env_overlay(&set.ctx, &vault).unwrap_err();
    assert!(
        refused.message.contains("this machine has none for it"),
        "{}",
        refused.message
    );
    // Declared by the committed half alone: the tab, never a terminal command.
    assert!(
        !refused.message.contains("--token-stdin"),
        "{}",
        refused.message
    );
    assert!(
        refused.message.contains(COMMITTED_ONLY),
        "{}",
        refused.message
    );
    assert!(!refused.message.contains(BAD));
}

#[test]
fn a_binding_of_the_tokens_variable_beside_the_declaration_is_not_honoured() {
    let set = Set::with_env("unused", &[("OP_PLANTED", BAD)]);
    create(&set.ctx, &request("team", GOOD)).unwrap();
    // The committed half now binds the token's variable to one of its choosing.
    set.commit(
        json!({"team": {"provider": "1password", "persona": null, "config": {
        "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_PLANTED"}}}}),
    );
    let vault = registry::vault(&set.ctx, "team").unwrap();

    assert_eq!(
        identity::bindings(&vault),
        [(
            "OP_SERVICE_ACCOUNT_TOKEN".to_string(),
            "service-account-token".to_string()
        )]
    );
    assert_eq!(
        env_overlay(&set.ctx, &vault).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), GOOD.to_string())]
    );
}

#[test]
fn a_committed_change_of_the_item_unpins_a_record_made_now() {
    let set = Set::new("unused");
    create(&set.ctx, &request("team", GOOD)).unwrap();
    set.commit(
        json!({"team": {"provider": "1password", "persona": null, "config": {
        "op-item": "another-item"}}}),
    );
    let vault = registry::vault(&set.ctx, "team").unwrap();
    assert!(!identity::in_keyring(&set.ctx, &vault));
    assert!(env_overlay(&set.ctx, &vault).is_err());
}

#[test]
fn a_way_of_keeping_a_token_this_purlis_does_not_know_is_refused_not_read_as_none() {
    let set = Set::new("unused");
    let config = json!({"op-vault": "Ops", "token": "somewhere-new"});
    registry::add_vault(
        &set.ctx,
        "later",
        "1password",
        config.as_object().unwrap().clone(),
        None,
        false,
        false,
    )
    .unwrap();
    let vault = registry::vault(&set.ctx, "later").unwrap();
    assert!(env_overlay(&set.ctx, &vault).is_err());
    assert!(crate::secrets::identity_missing(&set.ctx, &vault).is_some());
}

#[test]
fn a_kept_token_is_in_no_environment_to_move_from_or_to_warn_about() {
    let set = Set::with_env("unused", &[("service-account-token", BAD)]);
    create(&set.ctx, &request("team", GOOD)).unwrap();
    let vault = registry::vault(&set.ctx, "team").unwrap();
    assert!(identity::app_env_holds_a_token(&set.ctx, &vault).is_empty());
    let refused = identity::move_to_keyring(&set.ctx, &vault).unwrap_err();
    assert!(refused.message.contains("reads it through no variable"));
    assert!(!refused.message.contains(BAD));
    assert_eq!(set.items().values().next().unwrap(), GOOD);
}

// ---- one token for several vaults -------------------------------------------------------

/// `team` bound to `$OP_TEAM_TOKEN` with three others bound to it too: `edge` in this machine's
/// half, `pulled` in the committed half only, `both` in both; and `prod` bound to another.
fn several() -> Set {
    let set = Set::new("unused");
    bound(&set, "team", "Engineering", "OP_TEAM_TOKEN");
    bound(&set, "edge", "Edge", "OP_TEAM_TOKEN");
    bound(&set, "prod", "Prod", "OP_PROD_TOKEN");
    bound(&set, "both", "Both", "OP_TEAM_TOKEN");
    let committed = |op_vault: &str| {
        json!({"provider": "1password", "persona": null, "config": {
        "op-vault": op_vault, "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}}})
    };
    set.commit(json!({"pulled": committed("Pulled"), "both": committed("Both")}));
    set
}

#[test]
fn the_others_are_listed_with_what_would_be_pinned_and_a_committed_one_starts_unticked() {
    let set = several();

    let listed = alike_of(&set.ctx, "team");

    let rows: Vec<(&str, &str, &str, &str, bool)> = listed
        .iter()
        .map(|a| {
            (
                a.name.as_str(),
                a.op_vault.as_str(),
                a.op_item.as_str(),
                a.half.as_str(),
                a.ticked,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("both", "Both", "charter-both", "both", false),
            ("edge", "Edge", "charter-edge", "local", true),
            ("pulled", "Pulled", "charter-pulled", "shared", false),
        ]
    );
    // Listing wrote nothing and read no keyring.
    assert!(set.items().is_empty());
}

#[test]
fn a_committed_vault_is_never_marked_without_its_tick() {
    let set = several();
    let listed = alike_of(&set.ctx, "team");

    let marked = change(
        &set.ctx,
        "team",
        &SignIn::Token(GOOD.into()),
        None,
        &[tick(&listed, "edge")],
    )
    .unwrap();

    assert_eq!(marked.marked, ["edge"]);
    assert!(marked.skipped.is_empty());
    let bare = set.bare();
    for untouched in ["pulled", "both", "prod"] {
        let vault = registry::vault(&bare, untouched).unwrap();
        assert!(!identity::in_keyring(&bare, &vault), "{untouched}");
        assert!(env_overlay(&bare, &vault).is_err(), "{untouched}");
    }
    assert_eq!(set.local()["vaults"]["pulled"], Value::Null);
    // Each vault given the token has its own item.
    assert_eq!(set.items().len(), 2, "{:?}", set.items().keys());
    let edge = registry::vault(&bare, "edge").unwrap();
    assert_eq!(
        env_overlay(&bare, &edge).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), GOOD.to_string())]
    );
}

#[test]
fn a_ticked_committed_vault_is_marked_with_its_own_item_and_record() {
    let set = several();
    let listed = alike_of(&set.ctx, "team");

    let marked = change(
        &set.ctx,
        "team",
        &SignIn::Token(GOOD.into()),
        None,
        &[tick(&listed, "pulled")],
    )
    .unwrap();

    assert_eq!(marked.marked, ["pulled"]);
    let rec = &set.local()["vaults"]["pulled"]["config"]["identity"];
    assert_eq!(rec["op_vault"], "Pulled");
    assert_ne!(
        rec["ids"]["OP_TEAM_TOKEN"],
        set.local()["vaults"]["team"]["config"]["identity"]["ids"]["service-account-token"]
    );
}

#[test]
fn a_binding_changed_between_the_showing_and_the_store_is_not_marked_and_is_said() {
    let set = several();
    let listed = alike_of(&set.ctx, "team");
    let ticks = [tick(&listed, "pulled"), tick(&listed, "edge")];
    // After the person was shown it, the committed half points `pulled` somewhere else.
    set.commit(
        json!({"pulled": {"provider": "1password", "persona": null, "config": {
        "op-vault": "Somewhere-else", "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}}}}),
    );

    let marked = change(&set.ctx, "team", &SignIn::Token(GOOD.into()), None, &ticks).unwrap();

    assert_eq!(marked.marked, ["edge"]);
    assert_eq!(marked.skipped, [("pulled".to_string(), NotMarked::Changed)]);
    let bare = set.bare();
    let pulled = registry::vault(&bare, "pulled").unwrap();
    assert!(!identity::in_keyring(&bare, &pulled));
    assert_eq!(set.local()["vaults"]["pulled"], Value::Null);
}

#[test]
fn a_tick_for_a_vault_that_is_not_alike_any_more_or_was_never_listed_is_skipped() {
    let set = several();
    let listed = alike_of(&set.ctx, "team");
    let prod = Tick {
        name: "prod".into(),
        digest: listed[0].digest.clone(),
    };
    let none = Tick {
        name: "nobody".into(),
        digest: String::new(),
    };

    let marked = change(
        &set.ctx,
        "team",
        &SignIn::Token(GOOD.into()),
        None,
        &[prod, none],
    )
    .unwrap();

    assert!(marked.marked.is_empty());
    assert_eq!(
        marked.skipped,
        [
            ("prod".to_string(), NotMarked::Gone),
            ("nobody".to_string(), NotMarked::Gone)
        ]
    );
    assert_eq!(set.items().len(), 1, "team's alone");
}

#[test]
fn a_new_vault_lists_the_others_that_keep_a_token_and_ticks_none_for_the_person() {
    let set = Set::new("unused");
    create(&set.ctx, &request("first", GOOD)).unwrap();
    bound(&set, "bound", "Ops", "OP_TEAM_TOKEN");

    let listed = alike(&set.ctx, &kept_sources(), "second");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "first");
    assert_eq!(listed[0].held, identity::Held::Keyring);
    assert!(!listed[0].ticked, "nothing says it is the same token");

    // Ticked, it is given the new token under a new item, and its old item is deleted.
    let mut req = request("second", OTHER);
    req.also = vec![tick(&listed, "first")];
    let marked = create(&set.ctx, &req).unwrap();
    assert_eq!(marked.marked, ["first"]);
    let items = set.items();
    assert_eq!(items.len(), 2, "{:?}", items.keys());
    assert!(items.values().all(|v| v == OTHER));
}

// ---- change and convert -----------------------------------------------------------------

#[test]
fn a_vault_bound_to_a_variable_is_converted_and_read_at_once_with_no_export() {
    let set = Set::new("unused");
    bound(&set, "team", "Engineering", "OP_TEAM_TOKEN");
    let bare = set.bare();
    let before = registry::vault(&bare, "team").unwrap();
    assert!(env_overlay(&bare, &before).is_err());

    change(&set.ctx, "team", &SignIn::Token(GOOD.into()), None, &[]).unwrap();

    // The same process, the next read: no restart, no variable.
    let after = registry::vault(&bare, "team").unwrap();
    assert_eq!(
        env_overlay(&bare, &after).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), GOOD.to_string())]
    );
    let config = &set.local()["vaults"]["team"]["config"];
    assert_eq!(config["token"], "keyring");
    assert_eq!(config["env"], Value::Null, "the binding is replaced");
    assert_eq!(config["op-vault"], "Engineering");
    // The variable it was bound to is no longer one any chat needs stripped for it.
    let doc = registry::load_registry(&bare).unwrap();
    assert!(
        registry::identity_vars(&doc)
            .iter()
            .all(|(_, names)| names.is_empty())
    );
    assert!(!set.every_file().contains(GOOD));
}

#[test]
fn a_converted_vault_names_the_variable_nothing_reads_any_more() {
    // #1542: the export in the person's shell profile is the one copy of the token left
    // outside the keyring, and nothing strips it from chats once no vault declares it.
    let set = Set::new("unused");
    bound(&set, "team", "Engineering", "OP_TEAM_TOKEN");

    let marked = change(&set.ctx, "team", &SignIn::Token(GOOD.into()), None, &[]).unwrap();

    assert_eq!(marked.no_longer_read, ["OP_TEAM_TOKEN"]);
    assert!(marked.marked.is_empty() && marked.skipped.is_empty());
}

#[test]
fn a_variable_another_vault_still_reads_is_not_named_after_a_conversion() {
    let set = Set::new("unused");
    bound(&set, "team", "Engineering", "OP_TEAM_TOKEN");
    bound(&set, "edge", "Edge", "OP_TEAM_TOKEN");

    let marked = change(&set.ctx, "team", &SignIn::Token(GOOD.into()), None, &[]).unwrap();
    assert!(marked.no_longer_read.is_empty(), "edge still reads it");

    // Ticked too, edge keeps its binding and reads the keyring first: the variable is still
    // one a vault declares, so it is not named either.
    let set = Set::new("unused");
    bound(&set, "team", "Engineering", "OP_TEAM_TOKEN");
    bound(&set, "edge", "Edge", "OP_TEAM_TOKEN");
    let listed = alike_of(&set.ctx, "team");
    let marked = change(
        &set.ctx,
        "team",
        &SignIn::Token(GOOD.into()),
        None,
        &[tick(&listed, "edge")],
    )
    .unwrap();
    assert_eq!(marked.marked, ["edge"]);
    assert!(marked.no_longer_read.is_empty());
}

#[test]
fn a_vault_changed_to_the_app_names_the_variable_it_was_read_through() {
    let set = Set::new("unused");
    bound(&set, "team", "Engineering", "OP_TEAM_TOKEN");
    let marked = change(&set.ctx, "team", &SignIn::App, None, &[]).unwrap();
    assert_eq!(marked.no_longer_read, ["OP_TEAM_TOKEN"]);

    // A vault whose token was kept names no variable: there was none to export.
    let set = Set::new("unused");
    create(&set.ctx, &request("team", GOOD)).unwrap();
    let marked = change(&set.ctx, "team", &SignIn::Token(OTHER.into()), None, &[]).unwrap();
    assert!(marked.no_longer_read.is_empty());
}

#[test]
fn vault_add_on_a_vault_bound_to_a_variable_says_to_remove_the_old_export() {
    let set = Set::new("unused");
    bound(&set, "team", "Engineering", "OP_TEAM_TOKEN");
    let mut rec = Rec {
        stdin: GOOD.into(),
        ..Default::default()
    };
    let mut req = add_request("team");
    req.op_vault = None;

    assert_eq!(vaultcmd::add(&set.ctx, &req, &mut rec), 0, "{}", rec.said());

    let said = rec.said();
    assert!(said.contains("$OP_TEAM_TOKEN"), "{said}");
    assert!(said.contains("shell's startup files"), "{said}");
    assert!(!all_of(&rec).contains(GOOD));
}

#[test]
fn a_vault_the_committed_half_binds_is_converted_in_this_machines_half_alone() {
    let set = Set::new("unused");
    set.commit(
        json!({"pulled": {"provider": "1password", "persona": null, "config": {
        "op-vault": "Pulled", "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}}}}),
    );
    let committed = std::fs::read_to_string(set.ctx.shared_registry()).unwrap();

    change(&set.ctx, "pulled", &SignIn::Token(GOOD.into()), None, &[]).unwrap();

    assert_eq!(
        std::fs::read_to_string(set.ctx.shared_registry()).unwrap(),
        committed
    );
    let bare = set.bare();
    let vault = registry::vault(&bare, "pulled").unwrap();
    assert_eq!(
        env_overlay(&bare, &vault).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), GOOD.to_string())]
    );
}

#[test]
fn a_changed_token_replaces_the_old_item() {
    let set = Set::new("unused");
    create(&set.ctx, &request("team", GOOD)).unwrap();
    change(&set.ctx, "team", &SignIn::Token(OTHER.into()), None, &[]).unwrap();
    let items = set.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items.values().next().unwrap(), OTHER);
}

#[test]
fn a_vault_read_through_more_than_the_token_is_not_converted() {
    let set = Set::new("unused");
    let config = json!({"op-vault": "Ops", "env": {
        "OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN", "OP_CONNECT_HOST": "OP_HOST"}});
    registry::add_vault(
        &set.ctx,
        "odd",
        "1password",
        config.as_object().unwrap().clone(),
        None,
        false,
        false,
    )
    .unwrap();
    assert!(change(&set.ctx, "odd", &SignIn::Token(GOOD.into()), None, &[]).is_err());
    assert!(set.items().is_empty());
}

#[test]
fn changing_to_the_app_takes_the_token_out_of_the_keyring_and_pins_the_account() {
    let set = Set::new("unused");
    create(&set.ctx, &request("team", GOOD)).unwrap();

    change(
        &set.ctx,
        "team",
        &SignIn::App,
        Some("acme.1password.eu"),
        &[],
    )
    .unwrap();

    assert!(set.items().is_empty());
    assert_eq!(
        set.local()["vaults"]["team"]["config"],
        json!({"op-vault": "Engineering", "account": "acme.1password.eu"})
    );
    let vault = registry::vault(&set.ctx, "team").unwrap();
    assert!(identity::bindings(&vault).is_empty());
}

#[test]
fn changing_to_the_app_is_refused_where_the_committed_half_declares_the_token() {
    let set = Set::new("unused");
    let mut req = request("team", GOOD);
    req.share = true;
    create(&set.ctx, &req).unwrap();

    let refused = change(&set.ctx, "team", &SignIn::App, None, &[]).unwrap_err();

    assert!(
        refused.message.contains("vaults.json"),
        "{}",
        refused.message
    );
    assert_eq!(set.items().len(), 1, "the token is kept");
}

// ---- the terminal -----------------------------------------------------------------------

fn add_request(name: &str) -> AddRequest {
    AddRequest {
        name: name.into(),
        provider: "1password".into(),
        op_vault: Some("Engineering".into()),
        token_stdin: true,
        ..Default::default()
    }
}

/// Everything a command said and printed.
fn all_of(rec: &Rec) -> String {
    format!(
        "{}\n{}\n{}",
        rec.said(),
        rec.out(),
        String::from_utf8_lossy(&rec.err)
    )
}

#[test]
fn vault_add_takes_the_token_from_standard_input_tests_it_and_registers_in_one_step() {
    let set = Set::new("unused");
    let mut rec = Rec {
        stdin: format!("{GOOD}\n"),
        ..Default::default()
    };

    let code = vaultcmd::add(&set.ctx, &add_request("team"), &mut rec);

    assert_eq!(code, 0, "{}", rec.said());
    assert!(rec.prompts.is_empty());
    assert!(
        rec.said().contains(
            "ok: Signed in to 1Password: 2 item(s) in vault 'Engineering'; item \
             'charter-team' is there."
        ),
        "{}",
        rec.said()
    );
    assert!(rec.said().contains("Its token is in the system keyring"));
    assert!(!all_of(&rec).contains(GOOD));
    let bare = set.bare();
    let vault = registry::vault(&bare, "team").unwrap();
    assert_eq!(
        env_overlay(&bare, &vault).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), GOOD.to_string())]
    );
    assert!(!set.args().contains(GOOD), "the token reached an argument");
}

#[test]
fn vault_add_asks_at_a_terminal_without_showing_what_is_typed() {
    let set = Set::new("unused");
    let mut rec = Rec {
        tty_in: true,
        hidden: GOOD.into(),
        stdin: "never read".into(),
        ..Default::default()
    };

    assert_eq!(vaultcmd::add(&set.ctx, &add_request("team"), &mut rec), 0);

    assert_eq!(
        rec.prompts,
        ["Service-account token for vault 'team' (not shown): "]
    );
    assert_eq!(rec.stdin, "never read");
}

#[test]
fn a_token_that_does_not_sign_in_registers_nothing_from_a_terminal() {
    let set = Set::new("401: Unauthorized");
    let mut rec = Rec {
        stdin: BAD.into(),
        ..Default::default()
    };

    let code = vaultcmd::add(&set.ctx, &add_request("team"), &mut rec);

    assert_eq!(code, 1);
    assert!(
        rec.said().contains("refused the sign-in with this token"),
        "{}",
        rec.said()
    );
    assert!(rec.said().contains("Nothing was registered"));
    assert!(!all_of(&rec).contains(BAD), "{}", all_of(&rec));
    assert!(registry::vault(&set.ctx, "team").is_err());
    assert!(set.items().is_empty());
}

#[test]
fn inside_a_chat_vault_add_reads_no_token_and_says_where_one_is_given() {
    for mark in ["PURLIS_CHAT", "PURLIS_SESSION_ID", "PURLIS_SANDBOXED"] {
        let set = Set::with_env("unused", &[(mark, "1")]);
        let mut rec = Rec {
            stdin: GOOD.into(),
            tty_in: mark == "PURLIS_CHAT",
            hidden: GOOD.into(),
            ..Default::default()
        };

        let code = vaultcmd::add(&set.ctx, &add_request("team"), &mut rec);

        assert_eq!(code, 1, "{mark}");
        assert!(
            rec.said().contains("never given from inside a chat"),
            "{mark}: {}",
            rec.said()
        );
        // Standard input was not read, and nobody was asked to type.
        assert_eq!(rec.stdin, GOOD, "{mark}");
        assert!(rec.prompts.is_empty(), "{mark}");
        assert_eq!(set.args(), "", "{mark}: no program was run");
        assert!(registry::vault(&set.ctx, "team").is_err(), "{mark}");
        assert!(set.items().is_empty(), "{mark}");
    }
}

#[test]
fn a_process_below_a_chats_program_is_inside_it_whatever_its_environment_says() {
    // The variables cleared in front of the command: the project's record of its open chats
    // still names the program this process runs below.
    let set = Set::new("unused");
    assert_eq!(
        in_a_chat(&set.ctx),
        Where::Outside,
        "a terminal of the person's"
    );
    let chat = |pid: u32| crate::reopen::Chat {
        program: "/bin/zsh".into(),
        name: "chat 3".into(),
        number: Some(3),
        pid: Some(pid),
        ..Default::default()
    };
    let record = |pid: u32| crate::reopen::Record {
        chats: vec![chat(pid)],
        dealt: 3,
        ..Default::default()
    };
    crate::reopen::write(set.tmp.path(), &record(std::process::id())).unwrap();

    assert_eq!(in_a_chat(&set.ctx), Where::Inside);
    let mut rec = Rec {
        stdin: GOOD.into(),
        ..Default::default()
    };
    assert_eq!(vaultcmd::add(&set.ctx, &add_request("team"), &mut rec), 1);
    assert_eq!(rec.stdin, GOOD, "nothing was read");
    assert!(registry::vault(&set.ctx, "team").is_err());

    // A program that is no ancestor of this process, and `init`, vouch for nothing.
    crate::reopen::write(set.tmp.path(), &record(1)).unwrap();
    assert_eq!(in_a_chat(&set.ctx), Where::Outside);
}

#[test]
fn the_token_flag_is_refused_beside_a_variable_or_for_another_provider() {
    let set = Set::new("unused");
    let mut with_env = add_request("team");
    with_env.token_env = Some("OP_TEAM_TOKEN".into());
    let mut keyring = add_request("team");
    keyring.provider = "keyring".into();
    let mut no_vault = add_request("team");
    no_vault.op_vault = None;
    for request in [with_env, keyring, no_vault] {
        let mut rec = Rec {
            stdin: GOOD.into(),
            ..Default::default()
        };
        assert_eq!(vaultcmd::add(&set.ctx, &request, &mut rec), 1);
        assert!(matches!(rec.said.first(), Some(Say::Err(_))));
        assert_eq!(rec.stdin, GOOD, "nothing was read");
        assert!(registry::vault(&set.ctx, "team").is_err());
    }
}

#[test]
fn an_empty_standard_input_is_no_token_and_registers_nothing() {
    let set = Set::new("unused");
    let mut rec = Rec::default();
    assert_eq!(vaultcmd::add(&set.ctx, &add_request("team"), &mut rec), 1);
    assert!(rec.said().contains("no token was given"), "{}", rec.said());
    assert_eq!(set.args(), "");
}

#[test]
fn a_sign_in_never_prints_its_token_in_debug() {
    assert_eq!(
        format!("{:?}", request("team", GOOD)).matches(GOOD).count(),
        0
    );
}

// ---- #1527 review: the chat check fails closed ------------------------------------------

/// A project whose record of open chats names one chat, its program `pid`.
fn with_a_chat(set: &Set, pid: u32) {
    let record = crate::reopen::Record {
        chats: vec![crate::reopen::Chat {
            program: "/bin/zsh".into(),
            name: "chat 3".into(),
            number: Some(3),
            pid: Some(pid),
            ..Default::default()
        }],
        dealt: 3,
        ..Default::default()
    };
    crate::reopen::write(set.tmp.path(), &record).unwrap();
}

#[test]
fn a_process_in_a_chats_session_is_inside_it_though_its_parent_has_gone() {
    // Process 70's parent is launchd now (it was started by the chat, then left it), and its
    // session is still the chat's: every chat's program leads its own session.
    let set = Set::new("unused");
    with_a_chat(&set, 30);
    let parents = |pid: u32| match pid {
        70 => Some(1),
        _ => None,
    };
    assert_eq!(
        in_a_chat_with(&set.ctx, 70, parents, |_| Ok(30)),
        Where::Inside
    );
    assert_eq!(
        in_a_chat_with(&set.ctx, 70, parents, |_| Ok(70)),
        Where::Outside,
        "its own session, and launchd above it"
    );
}

#[test]
fn where_purlis_cannot_tell_it_says_so_and_takes_no_token() {
    let set = Set::new("unused");
    with_a_chat(&set, 30);
    // A parent that cannot be read, or a session that cannot be.
    assert!(matches!(
        in_a_chat_with(&set.ctx, 70, |_| None, |_| Ok(70)),
        Where::Unsure(_)
    ));
    assert!(matches!(
        in_a_chat_with(
            &set.ctx,
            70,
            |_| Some(1),
            |_| Err(std::io::Error::other("gone"))
        ),
        Where::Unsure(_)
    ));

    // A record of open chats that is not one this purlis can read.
    let record = crate::reopen::path(set.tmp.path());
    std::fs::write(&record, "not a record").unwrap();
    let Where::Unsure(why) = in_a_chat(&set.ctx) else {
        panic!("a garbled record was read as no chats");
    };
    let mut rec = Rec {
        stdin: GOOD.into(),
        ..Default::default()
    };
    assert_eq!(vaultcmd::add(&set.ctx, &add_request("team"), &mut rec), 1);
    assert!(
        rec.said()
            .contains("cannot tell whether this runs inside a chat"),
        "{}",
        rec.said()
    );
    assert!(rec.said().contains(&why));
    assert_eq!(rec.stdin, GOOD, "nothing was read");
    assert_eq!(set.args(), "", "no program was run");

    // One that cannot be read at all.
    std::fs::remove_file(&record).unwrap();
    std::fs::create_dir_all(&record).unwrap();
    assert!(matches!(in_a_chat(&set.ctx), Where::Unsure(_)));
}

// ---- #1527 review: alike is the whole binding, and the digest what was shown -------------

#[test]
fn a_vault_that_binds_the_variable_to_another_target_is_not_offered() {
    let set = several();
    let config = json!({"op-vault": "Odd", "env": {"OP_CONNECT_TOKEN": "OP_TEAM_TOKEN"}});
    registry::add_vault(
        &set.ctx,
        "odd",
        "1password",
        config.as_object().unwrap().clone(),
        None,
        false,
        false,
    )
    .unwrap();
    let names: Vec<String> = alike_of(&set.ctx, "team")
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(names, ["both", "edge", "pulled"]);
}

#[test]
fn a_token_stored_for_a_ticked_vault_after_it_was_shown_is_not_replaced() {
    let set = several();
    let listed = alike_of(&set.ctx, "team");
    let ticks = [tick(&listed, "edge")];
    // From its own tab, after the list was shown: "has no token yet" is no longer true.
    let edge = registry::vault(&set.ctx, "edge").unwrap();
    identity::put_in_keyring(&set.ctx, &edge, OTHER).unwrap();

    let marked = change(&set.ctx, "team", &SignIn::Token(GOOD.into()), None, &ticks).unwrap();

    assert_eq!(marked.skipped, [("edge".to_string(), NotMarked::Changed)]);
    let bare = set.bare();
    let edge = registry::vault(&bare, "edge").unwrap();
    assert_eq!(
        env_overlay(&bare, &edge).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), OTHER.to_string())]
    );
}

// ---- #1527 review: what a failed create leaves ------------------------------------------

#[test]
fn a_forced_create_that_fails_keeps_the_old_record_and_its_token() {
    let set = Set::new("unused");
    create(&set.ctx, &request("team", GOOD)).unwrap();
    let local_before = std::fs::read_to_string(set.ctx.local_registry()).unwrap();
    let items_before = set.items();
    // The committed half tags it for a persona the new registration does not say.
    std::fs::create_dir_all(set.tmp.path().join("personas/intruder")).unwrap();
    set.commit(json!({"team": {"provider": "1password", "persona": "intruder", "config": {}}}));
    let mut again = request("team", OTHER);
    again.force = true;

    assert!(create(&set.ctx, &again).is_err());

    assert_eq!(
        std::fs::read_to_string(set.ctx.local_registry()).unwrap(),
        local_before
    );
    assert_eq!(
        set.items(),
        items_before,
        "the old item is kept, the new one gone"
    );
    let bare = set.bare();
    let vault = registry::vault(&bare, "team").unwrap();
    assert_eq!(
        env_overlay(&bare, &vault).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), GOOD.to_string())]
    );
}

#[test]
fn a_create_whose_local_write_fails_after_the_token_was_stored_leaves_neither() {
    use std::os::unix::fs::PermissionsExt;
    let set = Set::new("unused");
    bound(&set, "older", "Ops", "OP_TEAM_TOKEN");
    let half = set.ctx.local_registry();
    let was = std::fs::read_to_string(&half).unwrap();
    // This machine's half can be read and not replaced: a file made read-only is the
    // person's to open up again.
    std::fs::set_permissions(&half, std::fs::Permissions::from_mode(0o400)).unwrap();

    let refused = create(&set.ctx, &request("team", GOOD));
    std::fs::set_permissions(&half, std::fs::Permissions::from_mode(0o600)).unwrap();

    let refused = refused.unwrap_err();
    assert!(!refused.message.contains(GOOD), "{}", refused.message);
    assert!(
        set.items().is_empty(),
        "the token stored first was deleted again"
    );
    assert_eq!(std::fs::read_to_string(&half).unwrap(), was);
    assert!(registry::vault(&set.ctx, "team").is_err());
}

// ---- #1527 review: the command a missing token's sentence prints -------------------------

#[test]
fn the_printed_command_gives_a_registered_vault_its_token_and_drops_nothing() {
    let set = Set::new("unused");
    let mut first = request("team", GOOD);
    first.place.op_item = Some("deploy-keys".into());
    first.place.account = Some("acme.1password.eu".into());
    create(&set.ctx, &first).unwrap();
    // Tagged for a persona on this machine alone: the tag a registration again would drop.
    let mut local = registry::load_local(&set.ctx).unwrap();
    local["vaults"]["team"]["persona"] = json!("devops");
    registry::save_local(&set.ctx, &local).unwrap();
    let config_before = set.local()["vaults"]["team"]["config"].clone();
    // The token is gone from this machine, and the sentence says how to give it again.
    std::fs::remove_file(set.ctx.state.join(keyring::STUB_FILE)).unwrap();
    let vault = registry::vault(&set.ctx, "team").unwrap();
    let said = env_overlay(&set.ctx, &vault).unwrap_err().message;
    let printed = token_again("team");
    assert!(said.contains(&printed), "{said}");
    assert_eq!(
        printed,
        "purlis vault add team --provider 1password --token-stdin"
    );

    // Exactly what that command asks for, as `purlis-cli` parses it (its own test holds the
    // parse): the name, the provider and the flag, and nothing else.
    let request = AddRequest {
        name: "team".into(),
        provider: "1password".into(),
        token_stdin: true,
        ..Default::default()
    };
    let mut rec = Rec {
        stdin: format!("{OTHER}\n"),
        ..Default::default()
    };
    assert_eq!(
        vaultcmd::add(&set.ctx, &request, &mut rec),
        0,
        "{}",
        rec.said()
    );

    assert!(rec.said().contains("Its other settings are as they were"));
    let config_after = &set.local()["vaults"]["team"]["config"];
    for key in ["op-vault", "op-item", "account", "token"] {
        assert_eq!(config_after[key], config_before[key], "{key}");
    }
    assert_eq!(set.local()["vaults"]["team"]["persona"], "devops");
    let bare = set.bare();
    let vault = registry::vault(&bare, "team").unwrap();
    assert_eq!(
        env_overlay(&bare, &vault).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), OTHER.to_string())]
    );
    // Tested where its items already are.
    assert!(
        set.args()
            .ends_with("item list --vault Engineering --format json --account acme.1password.eu\n"),
        "{}",
        set.args()
    );
}

#[test]
fn the_printed_command_converts_a_vault_bound_to_a_variable_too() {
    let set = Set::new("unused");
    bound(&set, "team", "Engineering", "OP_TEAM_TOKEN");
    let mut rec = Rec {
        stdin: GOOD.into(),
        ..Default::default()
    };
    let request = AddRequest {
        name: "team".into(),
        provider: "1password".into(),
        token_stdin: true,
        ..Default::default()
    };
    assert_eq!(
        vaultcmd::add(&set.ctx, &request, &mut rec),
        0,
        "{}",
        rec.said()
    );
    let bare = set.bare();
    let vault = registry::vault(&bare, "team").unwrap();
    assert_eq!(
        env_overlay(&bare, &vault).unwrap(),
        [("OP_SERVICE_ACCOUNT_TOKEN".to_string(), GOOD.to_string())]
    );
}

#[test]
fn a_setting_given_beside_the_token_of_a_registered_vault_is_refused_not_taken() {
    let set = Set::new("unused");
    create(&set.ctx, &request("team", GOOD)).unwrap();
    let before = std::fs::read_to_string(set.ctx.local_registry()).unwrap();
    let mut other_vault = add_request("team");
    other_vault.op_vault = Some("Ops".into());
    let mut same_vault = add_request("team");
    same_vault.op_vault = Some("Engineering".into());

    let mut rec = Rec {
        stdin: OTHER.into(),
        ..Default::default()
    };
    assert_eq!(vaultcmd::add(&set.ctx, &other_vault, &mut rec), 1);
    assert!(
        rec.said().contains("--op-vault would change it too"),
        "{}",
        rec.said()
    );
    assert_eq!(rec.stdin, OTHER, "nothing was read");
    assert_eq!(
        std::fs::read_to_string(set.ctx.local_registry()).unwrap(),
        before
    );

    // The setting it already has is no change.
    let mut rec = Rec {
        stdin: OTHER.into(),
        ..Default::default()
    };
    assert_eq!(
        vaultcmd::add(&set.ctx, &same_vault, &mut rec),
        0,
        "{}",
        rec.said()
    );
}

#[test]
fn a_listing_that_fails_for_a_reason_purlis_does_not_know_says_the_listing_failed() {
    let set = Set::new("something nobody has seen");
    let failed = op_vaults(&set.ctx, &SignIn::Token(BAD.into()), None).unwrap_err();
    assert_eq!(failed.kind, Kind::Other);
    assert!(
        failed.why.starts_with("purlis could not list"),
        "{}",
        failed.why
    );
    assert!(
        !failed.why.contains("The test did not pass"),
        "{}",
        failed.why
    );
    let refused = Set::new("401: Unauthorized");
    let failed = op_vaults(&refused.ctx, &SignIn::Token(BAD.into()), None).unwrap_err();
    assert_eq!(failed.kind, Kind::SignIn);
}

#[test]
fn a_vault_only_the_committed_half_declares_is_given_no_token_from_a_terminal() {
    // #1527 re-review: the record would pin settings a commit chose, which nobody was shown.
    let set = Set::new("unused");
    set.commit(
        json!({"pulled": {"provider": "1password", "persona": null, "config": {
        "op-vault": "Pulled", "op-item": "another-item", "token": "keyring"}}}),
    );
    // A local entry that only pins an account does not declare the vault here.
    let mut local = registry::load_local(&set.ctx).unwrap();
    local.insert(
        "vaults".into(),
        json!({"pulled": {"config": {"account": "acme.1password.eu"}}}),
    );
    registry::save_local(&set.ctx, &local).unwrap();
    let before = std::fs::read_to_string(set.ctx.local_registry()).unwrap();
    assert!(!declared_here(&set.ctx, "pulled"));
    assert_eq!(token_again_for(&set.ctx, "pulled"), None);

    // Exactly what the old sentence printed, at a terminal and from a pipe.
    for tty in [false, true] {
        let mut rec = Rec {
            stdin: GOOD.into(),
            tty_in: tty,
            hidden: GOOD.into(),
            ..Default::default()
        };
        let request = AddRequest {
            name: "pulled".into(),
            provider: "1password".into(),
            token_stdin: true,
            ..Default::default()
        };
        assert_eq!(vaultcmd::add(&set.ctx, &request, &mut rec), 1);
        assert!(
            rec.said()
                .contains("declared by the committed vaults.json alone"),
            "{}",
            rec.said()
        );
        assert!(rec.said().contains(COMMITTED_ONLY), "{}", rec.said());
        assert_eq!(rec.stdin, GOOD, "nothing was read");
        assert!(rec.prompts.is_empty(), "nobody was asked");
    }
    assert_eq!(set.args(), "", "no program was run");
    assert!(set.items().is_empty());
    assert_eq!(
        std::fs::read_to_string(set.ctx.local_registry()).unwrap(),
        before
    );
    let vault = registry::vault(&set.ctx, "pulled").unwrap();
    assert!(!identity::in_keyring(&set.ctx, &vault));

    // Registered on this machine, the same command works (the printed one, by `token_again_for`).
    let mut local = registry::load_local(&set.ctx).unwrap();
    local["vaults"]["pulled"]["provider"] = json!("1password");
    registry::save_local(&set.ctx, &local).unwrap();
    assert_eq!(
        token_again_for(&set.ctx, "pulled").as_deref(),
        Some("purlis vault add pulled --provider 1password --token-stdin")
    );
}
