//! How often one `purlis secret exec`, and `purlis vault list`, make the Keychain ask the
//! person (#1638, #1180).
//!
//! A 1Password vault whose service-account token is kept in the keyring is read by running
//! `op` once per value, and every `op` is handed the token. Where the `purlis` command is the
//! reader, each read of that item is one Keychain question to the person, so these tests count
//! the reads of the fenced build's stub keyring ([`keyring::stub_reads`]) through the whole
//! command ([`exec`]).

use super::*;
use crate::secrets::cmd::tests::Rec;
use crate::secrets::{Ctx, Env, identity, keyring, registry};

/// A word that is no token's shape: the commit hook scans for those.
const KEPT: &str = "kept-for-the-team";

/// A plane whose 1Password vault `team` is read through a token kept in the stub keyring, an
/// `op` stand-in that answers `op read` with `value-of-<key>` only when it was handed a token,
/// and the directory that `op` is in.
fn kept_plane() -> (tempfile::TempDir, tempfile::TempDir) {
    crate::secrets::program::stand_ins_live_in_temp_folders();
    let tmp = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    stand_in::program(
        bin.path(),
        "op",
        "#!/bin/sh\n[ -n \"$OP_SERVICE_ACCOUNT_TOKEN\" ] || exit 1\nprintf 'value-of-%s' \"${3##*/}\"\n",
    );
    let ctx = on_path(tmp.path(), bin.path(), &[]);
    let mut config = serde_json::Map::new();
    config.insert("op-vault".into(), serde_json::json!("Fixture"));
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}),
    );
    registry::add_vault(&ctx, "team", "1password", config, None, false, false).unwrap();
    let v = registry::vault(&ctx, "team").unwrap();
    identity::put_in_keyring(&ctx, &v, KEPT).unwrap();
    (tmp, bin)
}

/// A context on `root` whose PATH is `bin` alone, with `more` beside it.
fn on_path(root: &std::path::Path, bin: &std::path::Path, more: &[(&str, &str)]) -> Ctx {
    let path = bin.to_string_lossy().into_owned();
    let mut vars = vec![("PATH", path.as_str())];
    vars.extend_from_slice(more);
    Ctx::new(root, Env::of(&vars))
}

/// `secret exec team --env A=alpha --env B=beta --env C=gamma -- sh -c 'printf …'`.
fn three_values() -> Request {
    Request {
        vault: "team".into(),
        env: vec!["A=alpha".into(), "B=beta".into(), "C=gamma".into()],
        file: Vec::new(),
        dotenv: Vec::new(),
        stream: false,
        exec: false,
        command: vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf '%s|%s|%s' \"$A\" \"$B\" \"$C\"".into(),
        ],
    }
}

/// What a chat the app started sandboxed carries, where no app listens on its socket any more.
fn sandboxed_no_app(tmp: &tempfile::TempDir) -> Vec<(&'static str, String)> {
    let gone = tmp.path().join("no-app-listens.sock");
    vec![
        (crate::hookwire::SANDBOXED_ENV, "1".to_owned()),
        (
            crate::hookwire::SOCKET_ENV,
            gone.to_string_lossy().into_owned(),
        ),
        (crate::hookwire::CHAT_ENV, "7".to_owned()),
    ]
}

/// What a refusal of `get`, `cp` or any other read tells a sandboxed chat to run instead: on
/// macOS `secret exec` through the app; elsewhere the app runs none for a chat yet
/// ([`crate::secrets::brokered::NO_WRAP`]), so a terminal outside the chat.
const ROUTE: &str = if cfg!(target_os = "macos") {
    "secret exec team"
} else {
    "terminal outside the chat"
};

/// What the refusal of a `secret exec` no app took tells a sandboxed chat to do instead.
const EXEC_ROUTE: &str = if cfg!(target_os = "macos") {
    "while the app that started this chat is open"
} else {
    "terminal outside the chat"
};

fn reads(ctx: &Ctx) -> usize {
    keyring::stub_reads(&ctx.state.join(keyring::STUB_FILE))
}

#[test]
fn one_secret_exec_reads_a_kept_token_from_the_keychain_once_however_many_values_it_hands_on() {
    let (tmp, bin) = kept_plane();
    let ctx = on_path(tmp.path(), bin.path(), &[]);
    let before = reads(&ctx);
    let mut io = Rec::default();

    let code = exec(&ctx, &three_values(), &mut io);

    assert_eq!(code, 0, "{}", io.said());
    // Every value was resolved (and is masked in what the child printed).
    assert_eq!(String::from_utf8_lossy(&io.out), "***|***|***");
    assert_eq!(
        reads(&ctx) - before,
        1,
        "each read of the kept token is one Keychain question to the person"
    );
}

#[test]
fn a_sandboxed_chat_no_app_answers_is_refused_a_kept_token_and_the_keychain_is_never_read() {
    let (tmp, bin) = kept_plane();
    let chat = sandboxed_no_app(&tmp);
    let chat: Vec<(&str, &str)> = chat.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let ctx = on_path(tmp.path(), bin.path(), &chat);
    let before = reads(&ctx);
    let mut io = Rec::default();

    let code = exec(&ctx, &three_values(), &mut io);

    assert_eq!(code, 1);
    assert!(io.out.is_empty(), "nothing was run");
    let said = io.said();
    assert!(said.contains("vault 'team'"), "{said}");
    assert!(said.contains("app"), "{said}");
    assert!(said.contains(EXEC_ROUTE), "{said}");
    assert_eq!(
        reads(&ctx) - before,
        0,
        "the chat never reaches the Keychain"
    );
}

#[test]
fn a_sandboxed_chat_no_app_answers_still_runs_a_vault_the_keychain_does_not_hold() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("vaults.json"),
        serde_json::json!({"vaults": {"team": {"provider": "plain-file", "config": {"file": "team.json"}}}})
            .to_string(),
    )
    .unwrap();
    let ctx = Ctx::new(tmp.path(), Env::of(&[]));
    let v = registry::vault(&ctx, "team").unwrap();
    for key in ["alpha", "beta", "gamma"] {
        crate::secrets::cmd::set_value(&ctx, &v, key, "a-plain-value").unwrap();
    }
    let chat = sandboxed_no_app(&tmp);
    let chat: Vec<(&str, &str)> = chat.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let ctx = Ctx::new(tmp.path(), Env::of(&chat));
    let mut io = Rec::default();

    let code = exec(&ctx, &three_values(), &mut io);

    assert_eq!(code, 0, "{}", io.said());
    assert_eq!(String::from_utf8_lossy(&io.out), "***|***|***");
}

#[test]
fn a_sandboxed_chat_is_refused_secret_get_of_a_kept_token_and_the_keychain_is_never_read() {
    let (tmp, bin) = kept_plane();
    let ctx = on_path(
        tmp.path(),
        bin.path(),
        &[(crate::hookwire::SANDBOXED_ENV, "1")],
    );
    let before = reads(&ctx);
    let mut io = Rec::default();

    let code = crate::secrets::cmd::get(&ctx, "team", "alpha", false, false, &mut io);

    assert_eq!(code, 1);
    assert!(io.out.is_empty(), "nothing was said of the value");
    let said = io.said();
    assert!(said.contains(ROUTE), "{said}");
    assert_eq!(
        reads(&ctx) - before,
        0,
        "the chat never reaches the Keychain"
    );

    // Outside a sandboxed chat the same read goes on, through one Keychain read.
    let person = on_path(tmp.path(), bin.path(), &[]);
    let mut io = Rec::default();
    assert_eq!(
        crate::secrets::cmd::get(&person, "team", "alpha", false, false, &mut io),
        0,
        "{}",
        io.said()
    );
    assert_eq!(reads(&person) - before, 1);
}

#[test]
fn any_other_read_of_a_kept_token_in_a_sandboxed_chat_is_refused_where_the_keyring_is_read() {
    let (tmp, bin) = kept_plane();
    let ctx = on_path(
        tmp.path(),
        bin.path(),
        &[(crate::hookwire::SANDBOXED_ENV, "1")],
    );
    let before = reads(&ctx);
    let mut io = Rec::default();

    // Listing a 1Password vault runs `op`, which is handed the token.
    let code = crate::secrets::cmd::list(&ctx, "team", &mut io);

    assert_ne!(code, 0);
    assert!(io.said().contains(ROUTE), "{}", io.said());
    assert_eq!(
        reads(&ctx) - before,
        0,
        "the chat never reaches the Keychain"
    );
}

#[test]
fn vault_list_never_reads_a_kept_token_from_the_keychain_to_draw_its_status() {
    let (tmp, bin) = kept_plane();
    let ctx = on_path(tmp.path(), bin.path(), &[]);
    let before = reads(&ctx);
    let mut io = Rec::default();

    let code = crate::secrets::vaultcmd::list(&ctx, &mut io);

    assert_eq!(code, 0, "{}", io.said());
    let out = String::from_utf8_lossy(&io.out).into_owned();
    let row = out
        .lines()
        .find(|line| line.starts_with("team"))
        .unwrap_or_else(|| panic!("no row for team in {out}"));
    assert!(row.contains(keyring::STORE_NAME), "{row}");
    assert!(row.contains("purlis vault verify team"), "{row}");
    assert_eq!(
        reads(&ctx) - before,
        0,
        "drawing the list never makes the Keychain ask the person"
    );
}
