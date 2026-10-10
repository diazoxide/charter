//! **The app reads each kept token from the Keychain once per run** (#1654).
//!
//! The app serves every request with a context of its own: each chat's brokered `secret exec`,
//! each vault tab. Reopening several chats after an update, when the Keychain no longer knows
//! the ad-hoc signed app, asked the person once per request. In the app every context shares
//! the app's memory of the tokens it read, so each item asks once per run.
//!
//! Its own test binary, because it marks the whole process as the app.

use purlis_core::secrets::cmd::{Io, Say};
use purlis_core::secrets::exec::{Request, exec};
use purlis_core::secrets::{Ctx, Env, identity, keyhold, keyring, registry};

/// Two words that are no token's shape: the commit hook scans for those.
const KEPT: &str = "kept-for-the-team";
const REPLACED: &str = "replaced-for-the-team";

#[derive(Default)]
struct Heard {
    out: Vec<u8>,
    said: Vec<String>,
}

impl Io for Heard {
    fn say(&mut self, line: Say) {
        self.said.push(format!("{line:?}"));
    }
    fn out(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }
    fn err(&mut self, bytes: &[u8]) {
        self.said.push(String::from_utf8_lossy(bytes).into_owned());
    }
    fn stdout_is_terminal(&self) -> bool {
        false
    }
    fn stdin_is_terminal(&self) -> bool {
        false
    }
    fn read_stdin(&mut self) -> String {
        String::new()
    }
    fn read_hidden(&mut self, _: &str) -> String {
        String::new()
    }
}

/// `secret exec team --env A=alpha --env B=beta -- sh -c 'printf …'`, as a chat's brokered run.
fn two_values() -> Request {
    Request {
        vault: "team".into(),
        env: vec!["A=alpha".into(), "B=beta".into()],
        file: Vec::new(),
        dotenv: Vec::new(),
        stream: false,
        exec: false,
        command: vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf '%s|%s' \"$A\" \"$B\"".into(),
        ],
    }
}

#[test]
fn the_app_reads_a_kept_token_once_however_many_requests_ask_and_afresh_once_it_is_replaced() {
    keyhold::this_process_is_the_app();
    let tmp = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    stand_in::program(
        bin.path(),
        "op",
        "#!/bin/sh\n[ -n \"$OP_SERVICE_ACCOUNT_TOKEN\" ] || exit 1\nprintf 'value-of-%s' \"${3##*/}\"\n",
    );
    let path = bin.path().to_string_lossy().into_owned();
    // A new context per request, as the app builds them.
    let request = || Ctx::new(tmp.path(), Env::of(&[("PATH", path.as_str())]));
    let setup = request();
    let mut config = serde_json::Map::new();
    config.insert("op-vault".into(), serde_json::json!("Fixture"));
    config.insert(
        "env".into(),
        serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}),
    );
    registry::add_vault(&setup, "team", "1password", config, None, false, false).unwrap();
    let v = registry::vault(&setup, "team").unwrap();
    identity::put_in_keyring(&setup, &v, KEPT).unwrap();
    let stub = setup.state.join(keyring::STUB_FILE);
    let reads = || keyring::stub_reads(&stub);
    let before = reads();

    for _ in 0..3 {
        let mut io = Heard::default();
        assert_eq!(exec(&request(), &two_values(), &mut io), 0, "{:?}", io.said);
        assert_eq!(String::from_utf8_lossy(&io.out), "***|***");
    }
    assert_eq!(
        reads() - before,
        1,
        "three requests in one run of the app ask the Keychain once"
    );

    // A token put in again is a new item: the app reads it, once, and never answers from
    // what it remembered of the one before.
    identity::put_in_keyring(&request(), &v, REPLACED).unwrap();
    for _ in 0..2 {
        let mut io = Heard::default();
        assert_eq!(exec(&request(), &two_values(), &mut io), 0, "{:?}", io.said);
    }
    assert_eq!(reads() - before, 2);
}
