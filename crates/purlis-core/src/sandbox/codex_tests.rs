//! The policy compiled for Codex: charter's own wrap around the whole harness (#1123), with
//! Codex's own sandbox off inside it, because Seatbelt applies one profile per process (V21 4).
//! Every expected answer is written out.

use std::path::{Path, PathBuf};

use super::*;

/// The sandbox on, without the `toolchains` preset: that preset gives a chat the project's own
/// package caches (#1337), made under the machine's data home when the chat starts, which these
/// tests' machines do not have. What the caches add to a wrap is `sandbox::tests`' to show.
const ON: &str = "[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"forge\"]\n";

fn plane_saying(toml: &str) -> tempfile::TempDir {
    let plane = tempfile::tempdir().expect("a plane");
    std::fs::write(plane.path().join("charter.toml"), toml).expect("charter.toml");
    plane
}

fn machine_at(home: &Path, os: Os) -> Machine {
    Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(home.to_path_buf()),
        os,
    }
}

fn words(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

/// The line `applied` gives a Codex chat in `cwd` with charter's own words `charters`.
fn line_in(
    applied: &Applied,
    cwd: &Path,
    charters: &str,
    confinement: &Confinement,
) -> Result<Line, String> {
    applied.line(
        Words {
            program: "/opt/codex".to_owned(),
            command: words("--model m"),
            armed: words("-c hooks.Stop=[]"),
            charters: words(charters),
        },
        &At {
            cwd: Some(cwd),
            hook_socket: None,
            confinement: Some(confinement),
            no_opt_out: false,
        },
    )
}

fn real(path: &Path) -> PathBuf {
    path.canonicalize().expect("a real path")
}

#[test]
fn a_codex_chat_in_a_sandboxed_plane_starts_inside_charters_wrap_on_macos() {
    // #1123: the hold-back of ruling V87f is lifted where charter can wrap Codex.
    let plane = plane_saying(ON);
    let home = tempfile::tempdir().expect("a home");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    assert_eq!(applied.harness(), Harness::Codex);
    assert!(matches!(applied.form(), Form::Codex(_)), "{applied:?}");
    assert_eq!(never_on(Harness::Codex, Os::MacOs), None);
}

#[test]
fn on_linux_a_codex_chat_is_refused_until_charter_can_wrap_it_there() {
    let plane = plane_saying(ON);
    let home = tempfile::tempdir().expect("a home");
    let refused = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::Linux),
        &|_| true,
    )
    .expect_err("refused");
    assert_eq!(
        refused,
        NotStarted::Uncompilable(Uncompilable {
            harness: Harness::Codex,
            unheld: Unheld::Wrap(Os::Linux),
        })
    );
    assert!(
        refused
            .to_string()
            .contains("purlis runs Codex inside a sandbox of its own, which it can apply on macOS but not yet on Linux, so it was not started"),
        "{refused}"
    );
    assert_eq!(
        never_on(Harness::Codex, Os::Linux).as_deref(),
        Some("purlis can wrap it on macOS only, so far")
    );
}

#[test]
fn a_keyring_vault_does_not_refuse_a_wrapped_codex_chat_because_the_wrap_holds_the_store() {
    let plane = plane_saying(ON);
    std::fs::write(
        plane.path().join("vaults.json"),
        r#"{"vaults": {"k": {"provider": "keyring"}}}"#,
    )
    .expect("registry");
    let home = tempfile::tempdir().expect("a home");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let cwd = plane.path().join("w");
    std::fs::create_dir(&cwd).expect("a workspace");
    let line = line_in(&applied, &cwd, "", &confinement).expect("starts");
    let keychains = real(home.path()).join("Library/Keychains");
    assert!(
        line.args[1].contains(&format!(
            "(deny file-read* file-write* (subpath \"{}\"))",
            keychains.display()
        )),
        "{}",
        line.args[1]
    );
    assert!(!line.args[1].contains("com.apple.SecurityServer"));
}

#[test]
fn the_wrapped_line_runs_codex_whole_under_sandbox_exec_with_its_own_sandbox_off_last() {
    let plane = plane_saying(ON);
    let cwd = plane.path().join("w");
    std::fs::create_dir(&cwd).expect("a workspace");
    let home = tempfile::tempdir().expect("a home");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, &cwd, "resume ses_1 hello", &confinement).expect("starts");
    assert_eq!(line.program, backend::SANDBOX_EXEC);
    assert_eq!(line.args[0], "-p");
    assert!(line.args[1].starts_with("(version 1)\n(deny default)\n"));
    let after: Vec<&str> = line.args[2..].iter().map(String::as_str).collect();
    assert_eq!(
        after,
        [
            "/opt/codex",
            "--model",
            "m",
            "-c",
            "hooks.Stop=[]",
            // Codex's own sandbox cannot be applied inside charter's, so it is off; what it
            // would ask for is refused rather than asked; no web search; and the features
            // unmeasured against the wrap stay off.
            "--sandbox",
            "danger-full-access",
            "--disable",
            "browser_use",
            "--disable",
            "computer_use",
            "--disable",
            "in_app_browser",
            "-c",
            "approval_policy={ granular = { sandbox_approval = false, request_permissions = false, skill_approval = false, rules = true, mcp_elicitations = true } }",
            "-c",
            "approvals_reviewer=\"user\"",
            "-c",
            "web_search=\"disabled\"",
            "resume",
            "ses_1",
            "hello",
        ]
    );
    // Codex's own sandbox is turned off in front of the subcommand, never after it.
    let off = after.iter().position(|word| *word == "danger-full-access");
    let tail = after.iter().position(|word| *word == "resume");
    assert!(off < tail && off.is_some(), "{after:?}");
    let proxy = confinement.proxy_url();
    for key in ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY", "https_proxy"] {
        assert!(line.env.contains(&(key.to_owned(), proxy.clone())), "{key}");
    }
    assert!(line.env.contains(&("NO_PROXY".to_owned(), String::new())));
    assert!(
        line.env
            .contains(&("TMPDIR".to_owned(), confinement.tmp().display().to_string()))
    );
    assert!(
        line.env
            .contains(&("SSL_CERT_FILE".to_owned(), "/etc/ssl/cert.pem".to_owned()))
    );
    // git over ssh through the chat's own SOCKS port (#1667).
    let route = confinement.ssh_route().expect("an ssh route");
    for pair in route.env() {
        assert!(line.env.contains(&pair), "{pair:?}");
    }
    let config = std::fs::read_to_string(route.config()).expect("its config");
    assert!(
        config.contains(&format!("127.0.0.1:{} %h %p", confinement.socks_port())),
        "{config}"
    );
}

#[test]
fn a_flag_of_codexs_own_that_would_widen_what_it_is_handed_still_refuses_the_chat() {
    let plane = plane_saying(ON);
    let cwd = plane.path().join("w");
    std::fs::create_dir(&cwd).expect("a workspace");
    let home = tempfile::tempdir().expect("a home");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let refused = line_in(&applied, &cwd, "--search", &confinement).expect_err("refused");
    assert!(refused.contains("`--search`"), "{refused}");
}

/// The `file-write*` allows of `profile`, one filter each.
fn write_allows(profile: &str) -> Vec<String> {
    let start = profile.find("(allow file-write*\n").expect("a write allow");
    let block = &profile[start..=profile[start..].find("))\n").expect("its end") + start];
    block
        .lines()
        .skip(1)
        .map(|line| line.trim().to_owned())
        .collect()
}

/// The project's own Codex home (D-88q) for a plane, on a machine whose home is `home`.
fn project_home(plane: &Path, home: &Path) -> PathBuf {
    Homes::codex_project(&machine_at(home, Os::MacOs), plane).expect("a data home")
}

#[test]
fn the_profile_writes_the_chat_directory_its_temp_and_only_what_a_codex_turn_writes() {
    let plane = plane_saying(ON);
    let cwd = plane.path().join("w");
    std::fs::create_dir(&cwd).expect("a workspace");
    let home = tempfile::tempdir().expect("a home");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, &cwd, "", &confinement).expect("starts");
    let named = project_home(plane.path(), home.path());
    assert!(
        named.starts_with(
            home.path()
                .join("Library/Application Support/charter/codex-homes")
        ),
        "{}",
        named.display()
    );
    let codex = real(&named);
    let c = codex.display().to_string().replace('.', "\\\\.");
    assert_eq!(
        write_allows(&line.args[1]),
        [
            format!("(subpath \"{}\")", real(&cwd).display()),
            format!("(subpath \"{}\")", real(confinement.tmp()).display()),
            format!("(regex \"^{c}/[a-z_]+_[0-9]+\\\\.sqlite(-wal|-shm|-journal)?$\")"),
            format!(
                "(regex \"^{c}/(sessions|archived_sessions|shell_snapshots|thread-writer-locks|log)/.+\")"
            ),
            format!("(literal \"{}\")", codex.join("history.jsonl").display()),
            format!("(literal \"{}\")", codex.join("installation_id").display()),
            format!("(literal \"{}\")", codex.join("auth.json").display()),
            "(literal \"/dev/null\")".to_owned(),
            "(literal \"/dev/tty\")".to_owned(),
            "(literal \"/dev/ptmx\")".to_owned(),
            "(regex #\"^/dev/ttys[0-9]+$\")".to_owned(),
        ]
    );
    // Codex's own directories are made before the wrap, which lets a chat make none of them.
    for dir in [
        "sessions",
        "archived_sessions",
        "shell_snapshots",
        "thread-writer-locks",
        "log",
    ] {
        assert!(codex.join(dir).is_dir(), "{dir} was not made");
    }
    // And they are among the places a program must not lie.
    let writable = applied.writable();
    assert!(writable.contains(&named.join("sessions")), "{writable:?}");
    assert!(!writable.contains(&named), "{writable:?}");
    // Codex is pointed at it, and never at the operator's own.
    assert!(
        line.env
            .contains(&("CODEX_HOME".to_owned(), named.display().to_string())),
        "{:?}",
        line.env
    );
    let operator = real(home.path()).join(".codex");
    assert!(
        line.args[1].contains(&format!(
            "(deny file-read* file-write* (subpath \"{}\"))",
            operator.display()
        )),
        "{}",
        line.args[1]
    );
    assert!(line.args[1].contains(&format!(
        "(allow file-read* (subpath \"{}\"))\n",
        operator.join("packages").display()
    )));
}

#[test]
fn each_project_has_a_codex_home_of_its_own_and_the_operators_follows_codex_home() {
    let one = plane_saying(ON);
    let two = plane_saying(ON);
    let home = tempfile::tempdir().expect("a home");
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    let data = tempfile::tempdir().expect("a data home");
    let (set, data_set) = (
        elsewhere.path().display().to_string(),
        data.path().display().to_string(),
    );
    let machine = Machine {
        env: crate::secrets::Env::of(&[
            ("CODEX_HOME", set.as_str()),
            ("CHARTER_DATA_HOME", data_set.as_str()),
        ]),
        home: Some(home.path().to_path_buf()),
        os: Os::MacOs,
    };
    let homes: Vec<PathBuf> = [&one, &two]
        .iter()
        .map(|plane| {
            let applied = for_start(Harness::Codex, plane.path(), &machine, &|_| true)
                .expect("starts")
                .expect("sandboxed");
            let Form::Codex(wrap) = applied.form() else {
                panic!("a Codex wrap");
            };
            assert_eq!(wrap.operator.as_deref(), Some(elsewhere.path()));
            assert!(
                wrap.denied
                    .iter()
                    .any(|it| it.path == elsewhere.path() && it.access == Access::ReadWrite),
                "{:?}",
                wrap.denied
            );
            wrap.home.clone().expect("a project home")
        })
        .collect();
    assert!(
        homes[0].starts_with(data.path().join("codex-homes")),
        "{homes:?}"
    );
    assert_ne!(homes[0], homes[1], "two projects share a Codex home");
}

#[test]
fn the_project_home_is_seeded_with_the_operators_login_and_a_config_charter_writes() {
    let plane = plane_saying(ON);
    let cwd = plane.path().join("workspaces/w");
    std::fs::create_dir_all(&cwd).expect("a workspace");
    let home = tempfile::tempdir().expect("a home");
    let operator = home.path().join(".codex");
    std::fs::create_dir_all(&operator).expect("the operator's home");
    std::fs::write(operator.join("auth.json"), "{\"OPENAI_API_KEY\": \"sk-1\"}").expect("a login");
    std::fs::write(
        operator.join("config.toml"),
        "[mcp_servers.x]\ncommand = \"/bin/x\"\n",
    )
    .expect("a config");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    line_in(&applied, &cwd, "", &confinement).expect("starts");
    let project = project_home(plane.path(), home.path());
    assert_eq!(
        std::fs::read_to_string(project.join("auth.json")).expect("seeded"),
        "{\"OPENAI_API_KEY\": \"sk-1\"}"
    );
    // A newer project login, refreshed inside the wrap, is kept; an older one is replaced.
    std::fs::write(
        project.join("auth.json"),
        "{\"OPENAI_API_KEY\": \"sk-1\", \"refreshed\": 2}",
    )
    .expect("a refresh");
    line_in(&applied, &cwd, "", &confinement).expect("starts");
    assert_eq!(
        std::fs::read_to_string(project.join("auth.json")).expect("kept"),
        "{\"OPENAI_API_KEY\": \"sk-1\", \"refreshed\": 2}"
    );
    let config: toml::Table = std::fs::read_to_string(project.join("config.toml"))
        .expect("written")
        .parse()
        .expect("TOML");
    assert!(!config.contains_key("mcp_servers"), "{config}");
    let real_cwd = real(&cwd);
    for dir in [real_cwd.as_path(), real_cwd.parent().expect("a parent")] {
        assert_eq!(
            config["projects"][&dir.display().to_string()]["trust_level"].as_str(),
            Some("untrusted"),
            "{config}"
        );
    }
}

/// A file's modification time set to `stamp` (`touch -t` form), as a chat inside the wrap could.
fn touch(path: &Path, stamp: &str) {
    let status = crate::forklock::status(
        std::process::Command::new("touch")
            .args(["-t", stamp])
            .arg(path),
    )
    .expect("touch runs");
    assert!(status.success());
}

/// A Codex chat in `plane`'s workspace started under the wrap, armed with `armed`, on a machine
/// whose home is `home`: the project home it was given.
fn started_in(plane: &Path, home: &Path, armed: &[String]) -> PathBuf {
    let cwd = plane.join("workspaces/w");
    std::fs::create_dir_all(&cwd).expect("a workspace");
    let applied = for_start(Harness::Codex, plane, &machine_at(home, Os::MacOs), &|_| {
        true
    })
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    applied
        .line(
            Words {
                program: "/opt/codex".to_owned(),
                command: Vec::new(),
                armed: armed.to_vec(),
                charters: Vec::new(),
            },
            &At {
                cwd: Some(&cwd),
                hook_socket: None,
                confinement: Some(&confinement),
                no_opt_out: false,
            },
        )
        .expect("starts");
    project_home(plane, home)
}

/// `bytes` as unpadded base64url, as a JWT's parts are written.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut n = 0u32;
        for (at, byte) in chunk.iter().enumerate() {
            n |= u32::from(*byte) << (16 - 8 * at);
        }
        for at in 0..=chunk.len() {
            out.push(char::from(ALPHABET[((n >> (18 - 6 * at)) & 63) as usize]));
        }
    }
    out
}

/// A token whose claims name `account` and `user`, unsigned: the seed decodes, never verifies.
fn token(account: &str, user: &str) -> String {
    let claims = serde_json::json!({
        "sub": user,
        "https://api.openai.com/auth": {"chatgpt_account_id": account, "chatgpt_user_id": user},
    });
    format!(
        "{}.{}.sig",
        base64url(br#"{"alg":"none"}"#),
        base64url(claims.to_string().as_bytes())
    )
}

/// A ChatGPT login: its tokens for `account` and `user`, and a plain `account_id` beside them.
fn chatgpt_login(account_id: &str, account: &str, user: &str, extra: &str) -> String {
    serde_json::json!({
        "tokens": {
            "id_token": token(account, user),
            "access_token": token(account, user),
            "refresh_token": "r",
            "account_id": account_id,
        },
        "extra": extra,
    })
    .to_string()
}

#[test]
fn the_seed_replaces_an_older_a_future_dated_or_another_accounts_login_and_never_follows_a_link() {
    let plane = plane_saying(ON);
    let home = tempfile::tempdir().expect("a home");
    let operator = home.path().join(".codex");
    std::fs::create_dir_all(&operator).expect("the operator's home");
    let ours = chatgpt_login("acct-operator", "acct-operator", "user-op", "");
    let ours = ours.as_str();
    std::fs::write(operator.join("auth.json"), ours).expect("a login");
    touch(&operator.join("auth.json"), "202601010000");
    let project = started_in(plane.path(), home.path(), &[]);
    let login = project.join("auth.json");
    let read = || std::fs::read_to_string(&login).expect("a login");
    assert_eq!(read(), ours);
    // Older than the operator's: replaced.
    std::fs::write(
        &login,
        chatgpt_login("acct-operator", "acct-operator", "user-op", "old"),
    )
    .expect("old");
    touch(&login, "202501010000");
    started_in(plane.path(), home.path(), &[]);
    assert_eq!(read(), ours);
    // A refresh of the same account, newer: kept.
    let refreshed = chatgpt_login("acct-operator", "acct-operator", "user-op", "refreshed");
    let refreshed = refreshed.as_str();
    std::fs::write(&login, refreshed).expect("refreshed");
    started_in(plane.path(), home.path(), &[]);
    assert_eq!(read(), refreshed);
    // Another account, dated ahead to look newest: replaced either way.
    std::fs::write(
        &login,
        chatgpt_login("acct-other", "acct-other", "user-other", ""),
    )
    .expect("swapped");
    started_in(plane.path(), home.path(), &[]);
    assert_eq!(read(), ours);
    // Review R2-F1: another account's tokens with the operator's plain `account_id` copied
    // beside them: the tokens' claims are what is compared, so it is replaced.
    std::fs::write(
        &login,
        chatgpt_login("acct-operator", "acct-other", "user-other", "copied"),
    )
    .expect("swapped");
    started_in(plane.path(), home.path(), &[]);
    assert_eq!(read(), ours);
    // A token that does not decode: replaced (fail closed).
    std::fs::write(
        &login,
        r#"{"tokens": {"access_token": "not-a-jwt", "account_id": "acct-operator"}}"#,
    )
    .expect("garbled");
    started_in(plane.path(), home.path(), &[]);
    assert_eq!(read(), ours);
    std::fs::write(&login, refreshed).expect("refreshed");
    touch(&login, "209901010000");
    started_in(plane.path(), home.path(), &[]);
    assert_eq!(read(), ours);
    // A link in the project's place is replaced by a regular file, its target untouched.
    let target = home.path().join("elsewhere.json");
    std::fs::write(&target, "untouched").expect("a target");
    std::fs::remove_file(&login).expect("removed");
    std::os::unix::fs::symlink(&target, &login).expect("a link");
    started_in(plane.path(), home.path(), &[]);
    assert!(
        !std::fs::symlink_metadata(&login)
            .expect("there")
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("target"),
        "untouched"
    );
    // An operator's login that is a link is not followed, and nothing is copied from it.
    let other = tempfile::tempdir().expect("another home");
    let other_plane = plane_saying(ON);
    std::fs::create_dir_all(other.path().join(".codex")).expect("a home");
    std::os::unix::fs::symlink(&target, other.path().join(".codex/auth.json")).expect("a link");
    let fresh = started_in(other_plane.path(), other.path(), &[]);
    assert!(!fresh.join("auth.json").exists());
}

#[test]
fn the_project_home_is_its_owners_alone() {
    use std::os::unix::fs::PermissionsExt;
    let plane = plane_saying(ON);
    let home = tempfile::tempdir().expect("a home");
    let project = started_in(plane.path(), home.path(), &[]);
    let mode = std::fs::metadata(&project)
        .expect("made")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn a_codex_chat_with_no_data_home_is_refused_and_no_home_is_made() {
    let plane = plane_saying(ON);
    let cwd = plane.path().join("w");
    std::fs::create_dir(&cwd).expect("a workspace");
    let machine = Machine {
        env: crate::secrets::Env::of(&[]),
        home: None,
        os: Os::MacOs,
    };
    let applied = for_start(Harness::Codex, plane.path(), &machine, &|_| true)
        .expect("starts")
        .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let refused = line_in(&applied, &cwd, "", &confinement).expect_err("refused");
    assert_eq!(
        refused,
        "this project runs every chat sandboxed, and purlis has no data folder on this machine \
         to keep this project's Codex home in, so nothing was started."
    );
}

#[test]
fn a_refused_chat_seeds_no_home() {
    // Review F6: the home is made only once the line is known to start.
    let plane = plane_saying(ON);
    let home = plane.path().join("home");
    std::fs::create_dir(&home).expect("a home inside the plane");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(&home, Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    line_in(&applied, plane.path(), "", &confinement).expect_err("refused");
    assert!(!project_home(plane.path(), &home).exists());
}

/// What charter arms a Codex chat with: its hooks, as its adapter hands them over.
fn charters_hooks() -> Vec<String> {
    let crate::harness::StateHooks::ThisSessionOnly { args, .. } = Harness::Codex.state_hooks(
        crate::harness::Kit {
            binary: Path::new("/Applications/charter.app/Contents/MacOS/charter"),
            plugin: None,
            persona: None,
        },
        None,
        &std::collections::BTreeMap::new(),
        None,
    ) else {
        panic!("armed per session");
    };
    args
}

#[test]
fn codex_trusts_exactly_the_hooks_charter_arms() {
    // D-88r. The records Codex 0.147.0's own "Trust all and continue" wrote for these two hooks,
    // read back from its config (measured), are what charter must write.
    let armed: Vec<String> = [
        "-c",
        "hooks.Stop=[{hooks=[{type=\"command\",command=\"/usr/bin/true\",timeout=5}]}]",
        "-c",
        "hooks.SessionStart=[{matcher=\"startup\",hooks=[{type=\"command\",command=\"/bin/echo a b\",timeout=7}]}]",
        "-c",
        "model=\"m\"",
    ]
    .map(str::to_owned)
    .to_vec();
    let trust = codex::hook_trust(&armed);
    let hash = |key: &str| trust[key]["trusted_hash"].as_str().map(str::to_owned);
    assert_eq!(
        hash("/<session-flags>/config.toml:stop:0:0").as_deref(),
        Some("sha256:16f52c5d3a34fb782ded821e4ac5b18b8cdef051e176b849d06dbfcec407aa7e")
    );
    assert_eq!(
        hash("/<session-flags>/config.toml:session_start:0:0").as_deref(),
        Some("sha256:0d39e54c3eafd162c3ab502d78fdf3791f497f7d2adf5c8b58b3db3fc8abf43f")
    );
    assert_eq!(trust.len(), 2, "{trust:?}");
    // A hook charter does not arm in that shape is not trusted.
    let other = [
        "-c".to_owned(),
        "hooks.Stop=[{hooks=[{type=\"command\",command=\"/usr/bin/true\",timeout=5,async=true}]}]"
            .to_owned(),
    ];
    assert!(codex::hook_trust(&other).is_empty());

    // Charter's own hooks, as its adapter arms them: one record each, written into the project
    // home's config beside the untrusted marks, and nothing more.
    let armed = charters_hooks();
    let groups: usize = armed
        .iter()
        .filter_map(|word| word.strip_prefix("hooks."))
        .filter_map(|pair| pair.split_once('='))
        .map(|(_, value)| {
            let parsed: toml::Table = format!("v = {value}").parse().expect("TOML");
            parsed["v"]
                .as_array()
                .expect("groups")
                .iter()
                .map(|group| group["hooks"].as_array().expect("hooks").len())
                .sum::<usize>()
        })
        .sum();
    assert!(groups > 0);
    assert_eq!(codex::hook_trust(&armed).len(), groups);
    let plane = plane_saying(ON);
    let home = tempfile::tempdir().expect("a home");
    let project = started_in(plane.path(), home.path(), &armed);
    let config: toml::Table = std::fs::read_to_string(project.join("config.toml"))
        .expect("written")
        .parse()
        .expect("TOML");
    assert_eq!(
        config["hooks"]["state"].as_table().expect("trust").len(),
        groups,
        "{config}"
    );
}

#[test]
fn a_chat_whose_directory_holds_codexs_home_is_not_wrapped() {
    let plane = plane_saying(ON);
    let home = plane.path().join("home");
    std::fs::create_dir(&home).expect("a home inside the plane");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(&home, Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let refused = line_in(&applied, plane.path(), "", &confinement).expect_err("refused");
    assert_eq!(
        refused,
        "this project runs every chat sandboxed, and purlis cannot wrap a Codex chat whose \
         directory holds Codex's own files, or is inside them, so nothing was started."
    );
}

#[test]
fn a_wrapped_codex_chat_with_no_confinement_is_refused() {
    let plane = plane_saying(ON);
    let home = tempfile::tempdir().expect("a home");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let cwd = plane.path().join("w");
    std::fs::create_dir(&cwd).expect("a workspace");
    let refused = applied
        .line(
            Words {
                program: "codex".to_owned(),
                command: Vec::new(),
                armed: Vec::new(),
                charters: Vec::new(),
            },
            &At {
                cwd: Some(&cwd),
                hook_socket: None,
                confinement: None,
                no_opt_out: false,
            },
        )
        .expect_err("refused");
    assert_eq!(
        refused,
        "this project runs every chat sandboxed, and purlis's egress proxy was not started for \
         this Codex chat, so nothing was started."
    );
}

/// The line `applied` gives a Codex chat in `cwd` whose hooks report on `socket`.
fn line_reporting_on(
    applied: &Applied,
    cwd: &Path,
    socket: Option<&Path>,
    confinement: &Confinement,
) -> Line {
    applied
        .line(
            Words {
                program: "/opt/codex".to_owned(),
                command: Vec::new(),
                armed: Vec::new(),
                charters: Vec::new(),
            },
            &At {
                cwd: Some(cwd),
                hook_socket: socket,
                confinement: Some(confinement),
                no_opt_out: false,
            },
        )
        .expect("starts")
}

/// A file denial does not stop a connect to a unix socket, which Seatbelt judges as network.
/// So the wrap names no unix socket but the one the chat's hooks report on, and none at all
/// when it has none: a chat cannot reach `charterd.sock` through the credentials' folder it
/// cannot read (FD-27, ADR 0068 §5). Ported from the test of Codex's own profile, which #1123
/// replaced with this wrap.
#[test]
fn the_wrap_allows_codex_no_unix_socket_but_the_hook_socket() {
    let plane = plane_saying(ON);
    let cwd = plane.path().join("w");
    std::fs::create_dir(&cwd).expect("a workspace");
    let home = tempfile::tempdir().expect("a home");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let unix = |line: &Line| -> Vec<String> {
        line.args
            .iter()
            .flat_map(|arg| arg.lines())
            .filter(|rule| rule.to_lowercase().contains("unix"))
            .map(str::to_owned)
            .collect()
    };
    let none = line_reporting_on(&applied, &cwd, None, &confinement);
    assert_eq!(unix(&none), Vec::<String>::new(), "{:?}", none.args);
    let hook = plane.path().join("hooks.sock");
    let hooked = line_reporting_on(&applied, &cwd, Some(&hook), &confinement);
    assert_eq!(
        unix(&hooked),
        [format!(
            "(allow network-outbound (remote unix-socket (path-literal \"{}\")))",
            real(plane.path()).join("hooks.sock").display()
        )]
    );
}

/// #1336: a Codex chat in `ws/repo` is denied the manifests at the root, in `ws` and in its own
/// folder, by path, and no rule names a manifest at any depth.
#[test]
fn a_codex_chat_is_denied_the_manifests_of_its_folder_and_those_above_it_only() {
    let plane = plane_saying(ON);
    let repo = plane.path().join("ws/repo");
    std::fs::create_dir_all(repo.join("sub")).expect("a repo");
    let home = tempfile::tempdir().expect("a home");
    let applied = for_start(
        Harness::Codex,
        plane.path(),
        &machine_at(home.path(), Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let profile = line_in(&applied, &repo, "", &confinement)
        .expect("starts")
        .args[1]
        .clone();
    let root = real(plane.path());
    let denies = |path: &Path| {
        profile.contains(&format!(
            "(deny file-write* (subpath \"{}\"))",
            path.display()
        ))
    };
    for name in MANIFESTS {
        for folder in [root.clone(), root.join("ws"), root.join("ws/repo")] {
            assert!(
                denies(&folder.join(name)),
                "{}: {profile}",
                folder.display()
            );
        }
        assert!(!denies(&root.join("ws/repo/sub").join(name)), "{profile}");
        assert!(
            !profile.contains(&name.replace('.', "\\\\.")),
            "{name}: {profile}"
        );
    }
}

/// The wrap, applied for real: what a Codex chat could do under Codex's own sandbox, measured
/// in earlier rounds, refused under charter's.
#[cfg(target_os = "macos")]
mod live {
    use super::*;
    use std::process::Command;

    /// `script` run by `/bin/sh` under `line`'s profile, in `dir`, and whether it succeeded.
    fn ran_in(line: &Line, dir: &Path, script: &str) -> bool {
        crate::forklock::output(
            Command::new(&line.program)
                .args(&line.args[..2])
                .args(["/bin/sh", "-c", script])
                .current_dir(dir)
                .envs(line.env.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        )
        .expect("sandbox-exec runs")
        .status
        .success()
    }

    struct Wrapped {
        plane: tempfile::TempDir,
        home: tempfile::TempDir,
        applied: Applied,
        confinement: Confinement,
    }

    impl Wrapped {
        fn new() -> Self {
            let plane = plane_saying(ON);
            let home = tempfile::tempdir().expect("a home");
            let applied = for_start(
                Harness::Codex,
                plane.path(),
                &machine_at(home.path(), Os::MacOs),
                &|_| true,
            )
            .expect("starts")
            .expect("sandboxed");
            let confinement = applied.confine().expect("confined").expect("a wrap");
            Self {
                plane,
                home,
                applied,
                confinement,
            }
        }

        fn line(&self, cwd: &Path) -> Line {
            line_in(&self.applied, cwd, "", &self.confinement).expect("starts")
        }
    }

    /// A workspace holding a clone and a `.claude`, as a chat finds it.
    fn workspace(wrapped: &Wrapped, name: &str) -> PathBuf {
        let cwd = wrapped.plane.path().join("workspaces").join(name);
        let clone = cwd.join("repo");
        std::fs::create_dir_all(clone.join(".git/hooks")).expect("a clone");
        std::fs::write(clone.join(".git/config"), "[core]\n").expect("its config");
        std::fs::create_dir_all(cwd.join(".claude")).expect(".claude");
        std::fs::write(cwd.join(".claude/settings.json"), "{}").expect("settings");
        cwd
    }

    /// Every file below `dir`, wherever a command moved it.
    fn files_below(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut todo = vec![dir.to_path_buf()];
        while let Some(at) = todo.pop() {
            for entry in std::fs::read_dir(&at).expect("a folder").flatten() {
                let path = entry.path();
                if entry.file_type().expect("a type").is_dir() {
                    todo.push(path);
                } else {
                    out.push(path);
                }
            }
        }
        out
    }

    #[test]
    fn a_wrapped_codex_chat_writes_a_manifest_only_where_it_cannot_change_a_chats_sandbox() {
        // #1336: below its folder and in its temp folder, yes; in its folder, `ws` and the
        // root, never, nor moved into place.
        let wrapped = Wrapped::new();
        let repo = wrapped.plane.path().join("ws/repo");
        std::fs::create_dir_all(&repo).expect("a repo");
        let line = wrapped.line(&repo);
        for name in MANIFESTS {
            for allowed in [
                format!("mkdir -p sub && echo x > sub/{name}"),
                format!("mkdir -p \"$TMPDIR/x\" && echo x > \"$TMPDIR/x/{name}\""),
            ] {
                assert!(ran_in(&line, &repo, &allowed), "{allowed} was refused");
            }
            for refused in [
                format!("echo x > {name}"),
                format!("echo x > ../{name}"),
                format!("echo x > ../../{name}"),
                format!("mv sub/{name} {name}"),
            ] {
                assert!(!ran_in(&line, &repo, &refused), "{refused} was let through");
            }
            assert!(!repo.join(name).exists(), "{name}");
        }
    }

    #[test]
    fn a_folder_holding_a_protected_name_cannot_be_moved_aside_changed_and_moved_back() {
        let wrapped = Wrapped::new();
        for (at, refused) in [
            // Measured under Codex's own sandbox (verify rounds 4 and 5): each was let through.
            "mv repo repo.x && echo '[core] fsmonitor=x' >> repo.x/.git/config && mv repo.x repo",
            "mv repo repo.z && echo '{}' > repo.z/.mcp.json && mv repo.z repo",
            "mv .claude .cl2 && echo '{\"x\":1}' > .cl2/settings.json && mv .cl2 .claude",
            "mv repo/.git repo/.git2 && echo x > repo/.git2/hooks/pre-commit && mv repo/.git2 repo/.git",
            "mv repo/.git repo/.git2",
            "echo x >> repo/.git/config",
            "echo x > repo/.git/hooks/post-checkout",
            "echo {} > .mcp.json",
            "mkdir -p deep/er && echo {} > deep/er/.mcp.json",
            "mkdir -p x/.codex",
            "mkdir -p .claude/skills/x",
            "mkdir -p .opencode",
            "mkdir -p .agents/skills/x",
            "echo {} > tui.json",
            "echo x > .zshrc",
            // Built in the temp directory, to be moved in with its parent.
            "mkdir -p \"$TMPDIR/p/.git\"",
        ]
        .into_iter()
        .enumerate()
        {
            let cwd = workspace(&wrapped, &format!("w{at}"));
            let line = wrapped.line(&cwd);
            assert!(!ran_in(&line, &cwd, refused), "{refused} was let through");
            for file in files_below(&cwd) {
                let name = file.file_name().and_then(|it| it.to_str()).unwrap_or("");
                let text = std::fs::read_to_string(&file).unwrap_or_default();
                assert!(
                    !text.contains("fsmonitor") && !text.contains("\"x\"") && text != "x\n",
                    "{refused} changed {}",
                    file.display()
                );
                assert!(
                    ![".mcp.json", "pre-commit", "post-checkout", ".zshrc"].contains(&name),
                    "{refused} made {}",
                    file.display()
                );
            }
        }
        let cwd = workspace(&wrapped, "ok");
        let line = wrapped.line(&cwd);
        assert!(ran_in(
            &line,
            &cwd,
            "echo ok > ordinary && mkdir -p a/b && echo x > a/b/c && echo x > repo/.git/objects-ok \
             && mv a a2 && git init -q fresh 2>/dev/null; test -f a2/b/c"
        ));
        assert!(ran_in(&line, &cwd, "echo ok > \"$TMPDIR/scratch\""));
    }

    #[test]
    fn a_temp_or_chat_folder_swapped_for_a_link_reaches_nothing_outside() {
        let wrapped = Wrapped::new();
        let cwd = wrapped.plane.path().join("workspaces/w");
        let inner = cwd.join("inner");
        std::fs::create_dir_all(&inner).expect("a folder below the chat's");
        let outside = tempfile::tempdir().expect("outside");
        let target = outside.path().display().to_string();
        let line = wrapped.line(&cwd);
        // The chat's own temp directory, swapped for a link mid-chat (verify round 5, A): its
        // parent is not the chat's, and a write through it lands nowhere.
        assert!(!ran_in(
            &line,
            &cwd,
            &format!("rmdir \"$TMPDIR\" && ln -s '{target}' \"$TMPDIR\""),
        ));
        ran_in(&line, &cwd, "echo x > \"$TMPDIR/escaped\"");
        // A folder below the chat's, swapped for a link by the chat itself, as a sibling chat
        // would swap another's (verify round 5, B): a write through it is refused.
        assert!(ran_in(
            &line,
            &cwd,
            &format!("mv inner inner.real && ln -s '{target}' inner"),
        ));
        assert!(!ran_in(&line, &cwd, "echo x > inner/escaped"));
        assert!(!ran_in(
            &line,
            &cwd,
            &format!("echo x > '{target}/escaped'")
        ));
        assert_eq!(
            std::fs::read_dir(outside.path()).expect("outside").count(),
            0,
            "something was written outside"
        );
        // The chat's own folder, swapped by a chat above it: the next command runs where the
        // link points and writes nothing there.
        let moved = wrapped.plane.path().join("workspaces/w.real");
        std::fs::rename(&cwd, &moved).expect("moved");
        std::os::unix::fs::symlink(outside.path(), &cwd).expect("linked");
        assert!(!ran_in(
            &line,
            &moved,
            &format!("echo x > '{target}/escaped'")
        ));
        assert!(!ran_in(&line, outside.path(), "echo x > escaped"));
        assert_eq!(
            std::fs::read_dir(outside.path()).expect("outside").count(),
            0,
            "something was written outside"
        );
    }

    #[test]
    fn the_project_codex_home_holds_only_what_a_turn_writes() {
        let wrapped = Wrapped::new();
        let cwd = wrapped.plane.path().join("w");
        std::fs::create_dir(&cwd).expect("a workspace");
        std::fs::write(wrapped.home.path().join(".zshrc"), "").expect("a startup file");
        let line = wrapped.line(&cwd);
        let codex = project_home(wrapped.plane.path(), wrapped.home.path());
        std::fs::create_dir_all(codex.join("skills/x")).expect("skills");
        std::fs::write(codex.join("history.jsonl"), "").expect("history");
        let zshrc = wrapped.home.path().join(".zshrc").display().to_string();
        let at = |script: &str| format!("cd '{}' && {script}", codex.display());
        for ok in [
            "mkdir -p sessions/2026/10/04 && echo x > sessions/2026/10/04/rollout-1.jsonl",
            "echo x > state_5.sqlite && echo x > state_5.sqlite-wal",
            "echo x >> history.jsonl",
            "echo x > installation_id",
            "echo {} > auth.json",
            "echo x > thread-writer-locks/.coordination.lock",
            "echo x > shell_snapshots/s.sh && echo x > log/codex-tui.log",
        ] {
            assert!(ran_in(&line, &cwd, &at(ok)), "{ok} was refused");
        }
        // Ruling D-88s: the stores every thread of the project shares are written here, in the
        // project's own home, since Codex's interactive screen does not start without them.
        for store in codex::SHARED_STORES {
            let write = format!(
                "echo x > {store}_1.sqlite && echo x > {store}_1.sqlite-wal && echo x > {store}_1.sqlite-shm"
            );
            assert!(ran_in(&line, &cwd, &at(&write)), "{store} was refused");
        }
        for refused in [
            // The config charter writes, and what a later Codex loads: skills, plugins, helpers.
            "echo x >> config.toml",
            "echo x > skills/x/SKILL.md",
            "mkdir plugins",
            "mkdir -p tmp/arg0",
            // Its own directories, moved out to be changed and moved back.
            "mv sessions s2",
            "mv log l2",
            // A link where a later Codex writes, made or moved in, and a folder that is not a
            // day's sessions, which could be one moved in with links in it.
            "ln -s /tmp sessions/2026/link",
            &format!("rm history.jsonl && ln -s '{zshrc}' history.jsonl"),
            &format!("rm history.jsonl && ln '{zshrc}' history.jsonl"),
            "mkdir sessions/notaday",
            "mkdir -p sessions/2026/10/04/.git",
            "echo x > sessions/.mcp.json",
        ] {
            assert!(
                !ran_in(&line, &cwd, &at(refused)),
                "{refused} was let through"
            );
        }
        assert_eq!(
            std::fs::read_to_string(wrapped.home.path().join(".zshrc")).expect("zshrc"),
            ""
        );
    }

    #[test]
    fn the_operators_own_codex_home_is_neither_read_nor_written_but_its_program_runs() {
        // D-88q: every thread the operator resumes keeps its state there.
        let wrapped = Wrapped::new();
        let cwd = wrapped.plane.path().join("w");
        std::fs::create_dir(&cwd).expect("a workspace");
        let operator = wrapped.home.path().join(".codex");
        let program = operator.join("packages/standalone/current/bin");
        std::fs::create_dir_all(&program).expect("an installed Codex");
        std::fs::write(program.join("codex"), "#!/bin/sh\necho ran\n").expect("a program");
        crate::forklock::status(
            std::process::Command::new("chmod")
                .args(["+x", &program.join("codex").display().to_string()]),
        )
        .expect("chmod");
        std::fs::write(operator.join("auth.json"), "{}").expect("a login");
        for store in codex::SHARED_STORES {
            std::fs::write(operator.join(format!("{store}_1.sqlite")), "").expect("a store");
        }
        std::fs::create_dir_all(operator.join("sessions/2026/10/04")).expect("sessions");
        let line = wrapped.line(&cwd);
        let at = |script: &str| format!("cd '{}' && {script}", operator.display());
        let shared: Vec<String> = codex::SHARED_STORES
            .iter()
            .flat_map(|store| {
                [
                    format!("cat {store}_1.sqlite"),
                    format!("echo x > {store}_1.sqlite"),
                    format!("echo x > {store}_1.sqlite-wal"),
                ]
            })
            .collect();
        for refused in [
            "cat auth.json",
            "echo x > sessions/2026/10/04/rollout-x.jsonl",
            "ls sessions",
        ]
        .into_iter()
        .chain(shared.iter().map(String::as_str))
        {
            assert!(
                !ran_in(&line, &cwd, &at(refused)),
                "{refused} was let through"
            );
        }
        assert!(ran_in(
            &line,
            &cwd,
            &format!("'{}' | grep -q ran", program.join("codex").display())
        ));
        assert!(!ran_in(
            &line,
            &cwd,
            &format!("echo x > '{}'", program.join("planted").display())
        ));
        assert_eq!(
            std::fs::read_to_string(operator.join("goals_1.sqlite")).expect("goals"),
            ""
        );
    }

    #[test]
    fn another_projects_codex_home_is_not_read_and_the_chats_own_is() {
        // Review F1: another project's sessions are its own.
        let wrapped = Wrapped::new();
        let cwd = wrapped.plane.path().join("w");
        std::fs::create_dir(&cwd).expect("a workspace");
        let line = wrapped.line(&cwd);
        let own = project_home(wrapped.plane.path(), wrapped.home.path());
        let other = own.with_file_name("0123456789abcdef0123456789abcdef");
        std::fs::create_dir_all(other.join("sessions/2026/10/04")).expect("another home");
        std::fs::write(other.join("sessions/2026/10/04/rollout-x.jsonl"), "theirs")
            .expect("their session");
        std::fs::write(own.join("sessions/mine.jsonl"), "mine").expect("our session");
        assert!(!ran_in(
            &line,
            &cwd,
            &format!(
                "cat '{}/sessions/2026/10/04/rollout-x.jsonl'",
                other.display()
            )
        ));
        assert!(!ran_in(&line, &cwd, &format!("ls '{}'", other.display())));
        assert!(ran_in(
            &line,
            &cwd,
            &format!("grep -q mine '{}/sessions/mine.jsonl'", own.display())
        ));
    }

    #[test]
    fn a_folder_moved_into_the_sessions_is_taken_out_at_the_next_start() {
        // Review F3: a rename is a create to Seatbelt, so a folder holding a link can be moved
        // in under a date-shaped name; the next start takes it, and any link, out.
        let wrapped = Wrapped::new();
        let cwd = wrapped.plane.path().join("w");
        std::fs::create_dir(&cwd).expect("a workspace");
        let line = wrapped.line(&cwd);
        let own = project_home(wrapped.plane.path(), wrapped.home.path());
        assert!(ran_in(
            &line,
            &cwd,
            &format!(
                "mkdir -p 2027/01/01 && ln -s /etc/hosts 2027/01/01/rollout.jsonl && mv 2027 '{}/sessions/2027'",
                own.display()
            )
        ));
        assert!(ran_in(
            &line,
            &cwd,
            &format!(
                "mkdir -p evil/x && mv evil '{}/sessions/2028'",
                own.display()
            )
        ));
        std::fs::create_dir_all(own.join("sessions/2026/10/04")).expect("a day");
        std::fs::write(own.join("sessions/2026/10/04/rollout-1.jsonl"), "kept").expect("a session");
        wrapped.line(&cwd);
        assert!(!own.join("sessions/2027/01/01/rollout.jsonl").exists());
        assert!(
            std::fs::symlink_metadata(own.join("sessions/2027/01/01/rollout.jsonl")).is_err(),
            "the link stayed"
        );
        assert!(
            !own.join("sessions/2028/x").exists(),
            "a folder that is not a day stayed"
        );
        assert_eq!(
            std::fs::read_to_string(own.join("sessions/2026/10/04/rollout-1.jsonl")).expect("kept"),
            "kept"
        );
    }

    #[test]
    fn a_wrapped_codex_chat_reaches_only_the_proxy() {
        let wrapped = Wrapped::new();
        let cwd = wrapped.plane.path().join("w");
        std::fs::create_dir(&cwd).expect("a workspace");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a listener");
        let port = listener.local_addr().expect("an address").port();
        let line = wrapped.line(&cwd);
        assert!(!ran_in(
            &line,
            &cwd,
            &format!("/usr/bin/nc -z -w 2 127.0.0.1 {port}")
        ));
        assert!(ran_in(
            &line,
            &cwd,
            &format!(
                "/usr/bin/nc -z -w 2 127.0.0.1 {}",
                wrapped.confinement.proxy_port()
            )
        ));
    }

    /// A home reached through a link (a dotfiles `~/.config`, a config root under `/var` or
    /// `/tmp`): the human scopes' credentials are still unreadable, because Seatbelt matches the
    /// path as the kernel names it and the wrap writes each denial so (FD-27; ported from the
    /// opencode wrap's test, the same profile).
    #[test]
    fn a_denial_under_a_linked_home_is_still_held() {
        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        std::fs::create_dir(&cwd).expect("a workspace");
        let homes = tempfile::tempdir().expect("homes");
        let real_home = homes.path().join("real");
        let credentials = real_home.join(".config/charter/charterd");
        std::fs::create_dir_all(&credentials).expect("the credentials' directory");
        std::fs::write(credentials.join("local-ui"), "c0ffee").expect("a credential");
        let linked_home = homes.path().join("linked");
        std::os::unix::fs::symlink(&real_home, &linked_home).expect("a linked home");
        let applied = for_start(
            Harness::Codex,
            plane.path(),
            &machine_at(&linked_home, Os::MacOs),
            &|_| true,
        )
        .expect("starts")
        .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_reporting_on(&applied, &cwd, None, &confinement);
        // A control: the wrap reads what no class denies.
        assert!(ran_in(
            &line,
            &cwd,
            &format!("cat '{}'", plane.path().join("charter.toml").display())
        ));
        for path in [
            linked_home.join(".config/charter/charterd/local-ui"),
            credentials.join("local-ui"),
        ] {
            assert!(
                !ran_in(&line, &cwd, &format!("cat '{}'", path.display())),
                "read {} under the wrap",
                path.display()
            );
        }
    }

    /// The wrap connects to no unix socket but the hook socket: a socket in the credentials'
    /// folder, and one anywhere else, is refused (FD-27, ADR 0068 §5; ported from the opencode
    /// wrap's test, the same profile).
    #[test]
    fn the_wrap_connects_to_no_unix_socket_but_the_hook_socket() {
        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        std::fs::create_dir(&cwd).expect("a workspace");
        let home = tempfile::tempdir().expect("a home");
        let credentials = home.path().join(".config/charter/charterd");
        std::fs::create_dir_all(&credentials).expect("the credentials' directory");
        let sockets = tempfile::tempdir().expect("sockets");
        let host = credentials.join("charterd.sock");
        let other = sockets.path().join("other.sock");
        let hook = sockets.path().join("hooks.sock");
        let _listening: Vec<std::os::unix::net::UnixListener> = [&host, &other, &hook]
            .into_iter()
            .map(|path| std::os::unix::net::UnixListener::bind(path).expect("a socket"))
            .collect();
        let applied = for_start(
            Harness::Codex,
            plane.path(),
            &machine_at(home.path(), Os::MacOs),
            &|_| true,
        )
        .expect("starts")
        .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_reporting_on(&applied, &cwd, Some(&hook), &confinement);
        let connects = |socket: &Path| {
            // `perl`, on every macOS: macOS's `nc -U` reports failure even on a connect that
            // succeeded.
            let probe = "IO::Socket::UNIX->new(Peer => $ARGV[0]) or exit 1";
            ran_in(
                &line,
                &cwd,
                &format!(
                    "/usr/bin/perl -MIO::Socket::UNIX -e '{probe}' '{}'",
                    socket.display()
                ),
            )
        };
        assert!(connects(&hook), "the hook socket");
        assert!(!connects(&host), "charterd.sock was reached");
        assert!(!connects(&other), "another unix socket was reached");
    }
}
