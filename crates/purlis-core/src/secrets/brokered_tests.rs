//! A brokered `secret exec` (#1407): what the chat sees come back, what the app refuses, and
//! what the child's sandbox lets it read.
//!
//! The app's half is driven over an in-memory connection, unwrapped where a test is about the
//! frames, so it runs on a machine that cannot apply a second sandbox (a sandboxed developer
//! session). What the profile holds is tested by applying it for real, where it can be applied.

use super::*;
use crate::harness::Harness;
use crate::sandbox::{Compiled, Machine, Os, Plane, Policy};
use std::sync::Mutex;

const TOKEN: &str = "s3cret-value";
const KUBECONFIG: &str = "apiVersion: v1\nclusters: [k8s-s3cret]\n";

/// A project whose vault `team` is a plain file tagged for persona `devops`, beside a vault
/// `other` tagged for nobody, and a folder `work` for the chat.
fn project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("vaults.json"),
        serde_json::json!({ "vaults": {
            "team": {"provider": "plain-file", "config": {"file": "team.json"}, "persona": "devops"},
            "other": {"provider": "plain-file", "config": {"file": "other.json"}},
        }})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("team.json"),
        serde_json::json!({"TOKEN": TOKEN, "KUBECONFIG": KUBECONFIG}).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("other.json"),
        serde_json::json!({"TOKEN": "n0t-yours"}).to_string(),
    )
    .unwrap();
    std::fs::create_dir(root.join("work")).unwrap();
    tmp
}

fn machine() -> Machine {
    Machine {
        env: Env::of(&[]),
        home: None,
        os: Os::this(),
    }
}

/// What a sandboxed chat of `harness` in `root` on `machine` is held to, with no hosts, as the
/// app records it when the chat starts.
fn confines_of(harness: Harness, root: &Path, machine: &Machine) -> Confines {
    held_to(harness, root, machine).expect("compiles")
}

/// [`confines_of`], or why purlis starts no sandboxed chat of `harness` on `machine`.
fn held_to(
    harness: Harness,
    root: &Path,
    machine: &Machine,
) -> Result<Confines, crate::sandbox::NotStarted> {
    let compiled = Compiled::of(
        &Policy {
            egress: Vec::new(),
            hosts: Vec::new(),
            certificate_checks: false,
            personas: Default::default(),
        },
        &Plane::of(None),
        root,
        machine,
        None,
    );
    let compile = crate::sandbox::compiler(harness).expect("a compiler");
    crate::sandbox::applied_of(harness, compile, &compiled, root, machine)
        .map(|applied| applied.confines().clone())
}

/// A Claude Code chat's, in `root`.
fn compiled(root: &Path) -> Confines {
    confines_of(Harness::ClaudeCode, root, &machine())
}

fn asker(root: &Path, persona: Option<&str>) -> Asker {
    Asker {
        root: root.to_path_buf(),
        env: Env::of(&[("PATH", "/usr/bin:/bin")]),
        chat: 7,
        persona: persona.map(str::to_owned),
        folder: Some(root.join("work")),
        confines: Some(compiled(root)),
    }
}

fn sh(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

fn wanted(root: &Path, vault: &str, command: Vec<String>) -> Wanted {
    Wanted {
        vault: vault.into(),
        command,
        cwd: Some(root.join("work")),
        environment: vec![
            ("PATH".into(), "/usr/bin:/bin".into()),
            (crate::hookwire::TOKEN_ENV.into(), "the-chats-token".into()),
        ],
        ..Wanted::default()
    }
}

/// Bytes written to a connection, kept where the test can read them.
#[derive(Clone, Default)]
struct Wire(Arc<Mutex<Vec<u8>>>);

impl Write for Wire {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Every frame the app wrote for `wanted`, served unwrapped to an asker whose connection has
/// already ended.
fn served_to_one_gone(asker: &Asker, wanted: Wanted) -> Vec<Frame> {
    let wire = Wire::default();
    serve_wrapped(
        asker,
        wanted,
        Box::new(std::io::Cursor::new(Vec::new())),
        Box::new(wire.clone()),
        Wrap::Unwrapped,
    );
    let bytes = wire.0.lock().unwrap().clone();
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("every line is a frame"))
        .collect()
}

/// Every frame the app wrote for `wanted`, served unwrapped to an asker whose connection stays
/// open until the child is done.
fn served(asker: &Asker, wanted: Wanted) -> Vec<Frame> {
    let (reader, _keep) = held_open();
    let wire = Wire::default();
    serve_wrapped(
        asker,
        wanted,
        reader,
        Box::new(wire.clone()),
        Wrap::Unwrapped,
    );
    let bytes = wire.0.lock().unwrap().clone();
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("every line is a frame"))
        .collect()
}

/// A connection's reading end that stays open, with nothing on it, for as long as the second
/// half is held.
fn held_open() -> (Box<dyn BufRead + Send>, std::os::unix::net::UnixStream) {
    let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
    (Box::new(std::io::BufReader::new(a)), b)
}

fn stdout(frames: &[Frame]) -> String {
    let mut out = Vec::new();
    for frame in frames {
        if let Frame::Stdout(bytes) = frame {
            out.extend(unb64(bytes));
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn stderr(frames: &[Frame]) -> String {
    let mut out = Vec::new();
    for frame in frames {
        if let Frame::Stderr(bytes) = frame {
            out.extend(unb64(bytes));
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn a_chats_command_gets_its_personas_secret_and_never_the_value_back() {
    let project = project();
    let mut want = wanted(
        project.path(),
        "team",
        sh(r#"test -n "$X" && echo ok; echo "leak: $X""#),
    );
    want.env = vec!["X=TOKEN".into()];
    let frames = served(&asker(project.path(), Some("devops")), want);
    assert_eq!(frames.first(), Some(&Frame::Started), "{frames:?}");
    assert_eq!(frames.last(), Some(&Frame::Exit(0)), "{frames:?}");
    assert_eq!(stdout(&frames), "ok\nleak: ***\n");
    let wire = serde_json::to_string(&frames).unwrap();
    assert!(!wire.contains(&b64(TOKEN.as_bytes())), "{wire}");
}

#[test]
fn the_childs_status_and_stderr_come_back_redacted() {
    let project = project();
    let mut want = wanted(project.path(), "team", sh(r#"echo "$X" >&2; exit 3"#));
    want.env = vec!["X=TOKEN".into()];
    let frames = served(&asker(project.path(), Some("devops")), want);
    assert_eq!(frames.last(), Some(&Frame::Exit(3)));
    assert_eq!(stderr(&frames), "***\n");
}

#[test]
fn a_vault_the_persona_may_not_use_is_refused_with_a_sentence_and_nothing_runs() {
    let project = project();
    let marker = project.path().join("work/ran");
    let mut want = wanted(
        project.path(),
        "other",
        sh(&format!("touch {}", marker.display())),
    );
    want.env = vec!["X=TOKEN".into()];
    let frames = served(&asker(project.path(), Some("devops")), want);
    let [Frame::Refused { why, code }] = frames.as_slice() else {
        panic!("one refusal: {frames:?}");
    };
    assert_eq!(*code, 1);
    assert!(
        why.contains("vault 'other' is not one persona 'devops' may use"),
        "{why}"
    );
    assert!(why.contains("'team'"), "names the vaults it may use: {why}");
    assert!(!marker.exists(), "nothing ran");
}

#[test]
fn a_chat_on_no_persona_is_handed_no_vault() {
    let project = project();
    let frames = served(
        &asker(project.path(), None),
        wanted(project.path(), "team", sh("echo hi")),
    );
    let [Frame::Refused { why, .. }] = frames.as_slice() else {
        panic!("one refusal: {frames:?}");
    };
    assert!(why.starts_with("this chat runs as no persona"), "{why}");
}

#[test]
fn a_chat_the_app_did_not_start_sandboxed_is_left_to_run_it_itself() {
    let project = project();
    let marker = project.path().join("work/ran");
    let mut unsandboxed = asker(project.path(), Some("devops"));
    unsandboxed.confines = None;
    let frames = served(
        &unsandboxed,
        wanted(
            project.path(),
            "team",
            sh(&format!("touch {}", marker.display())),
        ),
    );
    assert_eq!(
        frames,
        vec![Frame::Refused {
            why: crate::hookwire::NOTHING_ANSWERS.to_owned(),
            code: 1
        }]
    );
    assert!(!marker.exists(), "the app ran nothing");
}

/// M1: a Codex chat's own sandbox denies the operator's Codex home (D-88q), so a run for it does
/// too: the run is held to the list the chat's harness was compiled to, not a neutral one.
#[cfg(target_os = "macos")]
#[test]
fn a_codex_chats_run_is_denied_the_operators_codex_home_as_the_chat_is() {
    let project = project();
    let root = project.path().canonicalize().unwrap();
    let home = tempfile::tempdir().unwrap();
    let machine = Machine {
        env: Env::of(&[]),
        home: Some(home.path().canonicalize().unwrap()),
        os: Os::MacOs,
    };
    let codex_home = home.path().canonicalize().unwrap().join(".codex");
    let confines = confines_of(Harness::Codex, &root, &machine);
    assert!(
        confines.denied.iter().any(|d| d.path == codex_home),
        "{:?}",
        confines.denied
    );
    let profile = profile_on(&confines, &[], &root.join("work"), &root, 4242).expect("a profile");
    assert!(
        profile.contains(&format!(
            "(deny file-read* file-write* (subpath \"{}\"))",
            crate::sandbox::real(&codex_home).display()
        )),
        "{profile}"
    );
    let claude = confines_of(Harness::ClaudeCode, &root, &machine);
    assert!(
        !claude.denied.iter().any(|d| d.path == codex_home),
        "a Claude Code chat's own sandbox does not deny it, so neither does its run"
    );
}

#[test]
fn whatever_the_child_left_running_ends_with_it() {
    let project = project();
    let began = std::time::Instant::now();
    let marker = project.path().join("work/survived");
    let frames = served(
        &asker(project.path(), Some("devops")),
        wanted(
            project.path(),
            "team",
            sh(&format!(
                "(sleep 8; touch {}) & echo started",
                marker.display()
            )),
        ),
    );
    assert_eq!(stdout(&frames), "started\n");
    assert_eq!(frames.last(), Some(&Frame::Exit(0)));
    assert!(
        began.elapsed() < Duration::from_secs(6),
        "the run did not wait for what it left: {:?}",
        began.elapsed()
    );
    std::thread::sleep(Duration::from_secs(9).saturating_sub(began.elapsed()));
    assert!(
        !marker.exists(),
        "the background job was killed with its group"
    );
}

#[test]
fn a_chat_has_at_most_a_few_runs_at_once() {
    let project = project();
    let asker = asker(project.path(), Some("devops"));
    let held: Vec<Slot> = (0..RUNS_AT_ONCE)
        .map(|_| Slot::take(&asker.root, asker.chat).expect("a place"))
        .collect();
    let frames = served(&asker, wanted(project.path(), "team", sh("echo hi")));
    let [Frame::Refused { why, .. }] = frames.as_slice() else {
        panic!("one refusal: {frames:?}");
    };
    assert!(why.contains("already has 4 commands running"), "{why}");
    assert!(
        Slot::take(&asker.root, asker.chat + 1).is_some(),
        "another chat's runs are its own"
    );
    drop(held);
    assert_eq!(
        stdout(&served(
            &asker,
            wanted(project.path(), "team", sh("echo hi"))
        )),
        "hi\n"
    );
}

#[test]
fn a_credential_folder_reached_through_a_link_is_refused() {
    let project = project();
    let vaults = project.path().join("vaults-here");
    let elsewhere = project.path().join("work/readable");
    std::fs::create_dir_all(&vaults).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, vaults.join(EXEC_DIR)).unwrap();
    assert!(private_dir(&vaults).is_err(), "exec is a link");
    let linked = project.path().join("vaults-link");
    std::os::unix::fs::symlink(project.path().join("work"), &linked).unwrap();
    assert!(private_dir(&linked).is_err(), "the vaults folder is a link");
    let fine = project.path().join("vaults-fine");
    assert_eq!(private_dir(&fine).unwrap(), fine.join(EXEC_DIR));
}

#[test]
fn a_credential_file_left_behind_is_swept_when_the_project_opens() {
    let project = project();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let dir = private_dir(&ctx.vaults_dir()).unwrap();
    std::fs::write(dir.join("charter-secret-abc"), "left").unwrap();
    std::fs::write(dir.join("other"), "not ours").unwrap();
    sweep(&ctx);
    assert!(!dir.join("charter-secret-abc").exists());
    assert!(dir.join("other").exists(), "only the files a run makes");
}

#[test]
fn a_file_credential_is_kept_where_chats_are_denied_and_removed_after() {
    let project = project();
    let mut want = wanted(
        project.path(),
        "team",
        sh(r#"echo "$KUBECONFIG"; cat "$KUBECONFIG" | head -c 10; echo"#),
    );
    want.file = vec!["KUBECONFIG=KUBECONFIG".into()];
    let frames = served(&asker(project.path(), Some("devops")), want);
    assert_eq!(frames.last(), Some(&Frame::Exit(0)), "{frames:?}");
    let out = stdout(&frames);
    let path = PathBuf::from(out.lines().next().unwrap());
    let vaults = Ctx::new(project.path(), Env::of(&[])).vaults_dir();
    assert!(
        path.starts_with(vaults.join(EXEC_DIR)),
        "{} is under the vaults directory",
        path.display()
    );
    assert_eq!(out.lines().nth(1), Some("apiVersion"));
    assert!(!path.exists(), "removed once the child ended");
}

#[test]
fn the_chats_token_and_socket_never_reach_the_child() {
    let project = project();
    let mut want = wanted(
        project.path(),
        "team",
        sh(&format!(
            r#"echo "[${}]"; echo "[$HTTPS_PROXY]" | cut -c1-17"#,
            crate::hookwire::TOKEN_ENV
        )),
    );
    want.env = vec!["X=TOKEN".into()];
    let frames = served(&asker(project.path(), Some("devops")), want);
    assert_eq!(stdout(&frames), "[]\n[http://127.0.0.1\n", "{frames:?}");
}

#[test]
fn the_askers_stdin_reaches_the_child() {
    let project = project();
    let mut want = wanted(project.path(), "team", sh("cat"));
    want.stdin = true;
    let input = [
        serde_json::to_string(&Input::Stdin(b64(b"hello "))).unwrap(),
        serde_json::to_string(&Input::Stdin(b64(b"world"))).unwrap(),
        serde_json::to_string(&Input::StdinClosed).unwrap(),
    ]
    .join("\n")
        + "\n";
    // Written on a connection that stays open, so the child ends of its own accord.
    let (a, mut b) = std::os::unix::net::UnixStream::pair().unwrap();
    b.write_all(input.as_bytes()).unwrap();
    let wire = Wire::default();
    serve_wrapped(
        &asker(project.path(), Some("devops")),
        want,
        Box::new(std::io::BufReader::new(a)),
        Box::new(wire.clone()),
        Wrap::Unwrapped,
    );
    let text = String::from_utf8(wire.0.lock().unwrap().clone()).unwrap();
    let frames: Vec<Frame> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(stdout(&frames), "hello world");
}

#[test]
fn an_asker_that_goes_away_has_its_command_stopped() {
    let project = project();
    let began = std::time::Instant::now();
    let frames = served_to_one_gone(
        &asker(project.path(), Some("devops")),
        wanted(project.path(), "team", sh("sleep 30")),
    );
    assert!(
        began.elapsed() < Duration::from_secs(10),
        "{:?}",
        began.elapsed()
    );
    assert!(matches!(frames.last(), Some(Frame::Exit(code)) if *code != 0));
}

// ---- where the app looks for a provider's program (#1516) ---------------------------------

/// A stand-in `op` that answers every `op read` with `value`, and leaves `ran` beside itself.
fn an_op(dir: &Path, value: &str) {
    crate::secrets::program::stand_ins_live_in_temp_folders();
    std::fs::create_dir_all(dir).unwrap();
    stand_in::program(
        dir,
        "op",
        &format!("#!/bin/sh\n: > \"$(dirname \"$0\")/ran\"\nprintf %s '{value}'\n"),
    );
}

/// A project whose vault `prod` is a 1Password one tagged for persona `devops`.
fn project_on_1password() -> tempfile::TempDir {
    let tmp = project();
    std::fs::write(
        tmp.path().join("vaults.json"),
        serde_json::json!({ "vaults": {
            "prod": {"provider": "1password", "config": {"op-vault": "Prod"}, "persona": "devops"},
        }})
        .to_string(),
    )
    .unwrap();
    tmp
}

/// The app as the Dock starts it: the system's short `PATH`, and the person's home.
fn asker_from_the_dock(root: &Path, home: &Path) -> Asker {
    Asker {
        env: Env::of(&[
            ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin"),
            ("HOME", &home.to_string_lossy()),
        ]),
        ..asker(root, Some("devops"))
    }
}

#[test]
fn an_app_started_from_the_dock_finds_the_providers_program_where_a_login_shell_would() {
    let project = project_on_1password();
    let home = tempfile::tempdir().unwrap();
    an_op(&home.path().join(".local/bin"), TOKEN);
    let mut want = wanted(
        project.path(),
        "prod",
        sh(r#"test -n "$X" && echo ok; echo "leak: $X""#),
    );
    want.env = vec!["X=TOKEN".into()];
    let frames = served(&asker_from_the_dock(project.path(), home.path()), want);
    assert_eq!(frames.last(), Some(&Frame::Exit(0)), "{frames:?}");
    assert_eq!(stdout(&frames), "ok\nleak: ***\n");
}

#[test]
fn nothing_in_a_chats_environment_chooses_the_providers_program() {
    let project = project_on_1password();
    let home = tempfile::tempdir().unwrap();
    an_op(&home.path().join(".local/bin"), TOKEN);
    // The chat's own `PATH` and `HOME` both lead to an `op` it wrote in its folder.
    let planted = project.path().join("work/bin");
    an_op(&planted, "planted");
    an_op(&project.path().join("work/.local/bin"), "planted");
    let mut want = wanted(project.path(), "prod", sh(r#"printf %s "$X" | wc -c"#));
    want.env = vec!["X=TOKEN".into()];
    want.environment = vec![
        (
            "PATH".into(),
            format!("{}:/usr/bin:/bin", planted.display()),
        ),
        (
            "HOME".into(),
            project.path().join("work").display().to_string(),
        ),
    ];
    let asker = asker_from_the_dock(project.path(), home.path());
    let frames = served(&asker, want.clone());
    assert_eq!(frames.last(), Some(&Frame::Exit(0)), "{frames:?}");
    assert_eq!(stdout(&frames).trim(), TOKEN.len().to_string());
    assert!(home.path().join(".local/bin/ran").exists());
    assert!(!planted.join("ran").exists());

    // With no `op` where the app looks, nothing runs, and the refusal names the app's
    // directories and none of the chat's.
    std::fs::remove_file(home.path().join(".local/bin/op")).unwrap();
    let frames = served(&asker, want);
    let why = refusal(&frames);
    assert!(
        why.contains("could not find the 1Password CLI ('op')"),
        "{why}"
    );
    assert!(
        why.contains("It looked in: /usr/bin, /bin, /usr/sbin, /sbin, "),
        "{why}"
    );
    assert!(!why.contains(&planted.display().to_string()), "{why}");
    assert!(!planted.join("ran").exists());
    assert!(!project.path().join("work/.local/bin/ran").exists());
}

/// D-1516-9: the app was started with a `PATH` that names a folder in the project (a terminal
/// with a project's own `bin` on it), and the only `op` anywhere is the one a chat put there.
#[test]
fn a_providers_program_a_chat_could_have_written_is_never_run_for_it() {
    let project = project_on_1password();
    let home = tempfile::tempdir().unwrap();
    let planted = project.path().join("work/bin");
    an_op(&planted, "planted");
    let asker = Asker {
        env: Env::of(&[
            ("PATH", &format!("{}:/usr/bin:/bin", planted.display())),
            ("HOME", &home.path().to_string_lossy()),
        ]),
        ..asker(project.path(), Some("devops"))
    };
    let frames = served(&asker, wants_token(project.path(), "prod"));
    let why = refusal(&frames);
    assert!(
        why.starts_with(&format!(
            "purlis found the 1Password CLI ('op') only where a chat can write: {}, so it was \
             not run. Keep the program outside the project and outside what a chat may write. \
             It looked in: ",
            planted.join("op").display()
        )),
        "{why}"
    );
    assert!(!planted.join("ran").exists());
}

/// M1: the chat's own folder is outside the project, and the person let this one chat write a
/// second folder. Both are the app's record of the chat, and neither is in the project.
#[test]
fn what_the_asking_chat_may_write_is_the_apps_record_of_it() {
    let project = project_on_1password();
    let home = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let granted = tempfile::tempdir().unwrap();
    for (dir, asker_of) in [
        (
            folder.path().join("bin"),
            Box::new(|asker: Asker| Asker {
                folder: Some(folder.path().to_path_buf()),
                ..asker
            }) as Box<dyn Fn(Asker) -> Asker>,
        ),
        (
            granted.path().to_path_buf(),
            Box::new(|asker: Asker| Asker {
                confines: Some(Confines {
                    writable: vec![granted.path().to_path_buf()],
                    ..asker.confines.clone().unwrap()
                }),
                ..asker
            }),
        ),
    ] {
        an_op(&dir, TOKEN);
        let from_a_terminal = Asker {
            env: Env::of(&[
                ("PATH", &format!("{}:/usr/bin:/bin", dir.display())),
                ("HOME", &home.path().to_string_lossy()),
            ]),
            ..asker(project.path(), Some("devops"))
        };
        let mut want = wants_token(project.path(), "prod");
        want.cwd = None;
        // Nobody recorded that a chat may write it: it is the person's own program.
        let frames = served(&from_a_terminal, want.clone());
        assert_eq!(frames.last(), Some(&Frame::Exit(0)), "{frames:?}");
        std::fs::remove_file(dir.join("ran")).unwrap();
        // The app recorded that this chat may: it is not run.
        let frames = served(&asker_of(from_a_terminal), want);
        let why = refusal(&frames);
        assert!(why.contains("only where a chat can write"), "{why}");
        assert!(!dir.join("ran").exists());
    }
}

#[test]
fn a_program_that_is_not_there_is_not_found() {
    let project = project();
    let frames = served(
        &asker(project.path(), Some("devops")),
        wanted(project.path(), "team", vec!["no-such-program-1407".into()]),
    );
    assert_eq!(
        frames,
        vec![Frame::Refused {
            why: "command not found: no-such-program-1407".into(),
            code: 127
        }]
    );
}

#[test]
fn a_value_split_across_two_chunks_is_still_masked() {
    let mut r = Redactor::new(&["abcdef".to_owned()]);
    let mut shown = r.push(b"xx abc");
    shown.extend(r.push(b"def yy abcdeX"));
    shown.extend(r.finish());
    assert_eq!(String::from_utf8(shown).unwrap(), "xx *** yy abcdeX");
}

#[test]
fn output_with_no_value_in_it_is_shown_as_it_comes_but_for_a_values_length() {
    let mut r = Redactor::new(&["abcd".to_owned()]);
    assert_eq!(r.push(b"hello world"), b"hello wo");
    assert_eq!(r.finish(), b"rld");
}

#[test]
fn the_childs_profile_gives_back_a_read_of_its_credential_file_alone() {
    let project = project();
    let root = project.path().canonicalize().unwrap();
    let compiled = compiled(&root);
    let vaults = Ctx::new(&root, Env::of(&[])).vaults_dir();
    let file = vaults.join(EXEC_DIR).join("charter-secret-x");
    let profile = profile_on(
        &compiled,
        std::slice::from_ref(&file),
        &root.join("work"),
        &root,
        4242,
    )
    .expect("a profile");
    let denied = profile
        .find(&format!(
            "(deny file-read* file-write* (subpath \"{}\"))",
            crate::sandbox::real(&vaults).display()
        ))
        .expect("the vaults directory is denied");
    let given = profile
        .find(&format!(
            "(allow file-read* (literal \"{}\"))",
            crate::sandbox::real(&file).display()
        ))
        .expect("the file is given back");
    assert!(denied < given, "the read is given back after the denial");
    assert!(profile.contains("(remote ip \"localhost:4242\")"));
    assert!(!profile.contains("unix-socket"), "no hook socket");
}

/// The profile applied for real, where this machine can apply one: the child reads its own
/// credential file and nothing else of the vaults class.
#[cfg(target_os = "macos")]
#[test]
fn a_wrapped_child_reads_its_credential_and_not_the_vault() {
    let can_apply = crate::forklock::status(
        std::process::Command::new(crate::sandbox::backend::SANDBOX_EXEC).args([
            "-p",
            "(version 1)(allow default)",
            "/usr/bin/true",
        ]),
    )
    .is_ok_and(|s| s.success());
    if !can_apply {
        let _ = writeln!(
            std::io::stderr(),
            "skipped: this process is already sandboxed, so no second profile applies"
        );
        return;
    }
    let project = project();
    let root = project.path().canonicalize().unwrap();
    let mut asker = asker(&root, Some("devops"));
    asker.confines = Some(compiled(&root));
    // Served with the connection held open, so the child runs to its end.
    let (reader, _keep) = held_open();
    let wire = Wire::default();
    // The vault's own plain file is a vaults-class path the chat is denied.
    let mut want = wanted(
        &root,
        "team",
        sh(&format!(
            r#"head -c 10 "$KUBECONFIG"; echo; cat {} >/dev/null 2>&1 && echo vault-read || echo vault-denied; touch ran && echo wrote"#,
            root.join("team.json").display()
        )),
    );
    want.file = vec!["KUBECONFIG=KUBECONFIG".into()];
    serve_wrapped(&asker, want, reader, Box::new(wire.clone()), Wrap::Seatbelt);
    let text = String::from_utf8(wire.0.lock().unwrap().clone()).unwrap();
    let frames: Vec<Frame> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        stdout(&frames),
        "apiVersion\nvault-denied\nwrote\n",
        "{frames:?}"
    );
    assert!(
        root.join("work/ran").exists(),
        "the chat's folder is written"
    );
}

/// What `secret exec` said and printed in the chat.
#[derive(Default)]
struct Rec {
    said: Vec<Say>,
    out: Vec<u8>,
    err: Vec<u8>,
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
        false
    }
    fn stdin_is_terminal(&self) -> bool {
        false
    }
    fn read_stdin(&mut self) -> String {
        String::new()
    }
    fn read_hidden(&mut self, _prompt: &str) -> String {
        String::new()
    }
}

/// The app's hook channel, answering a brokered `secret exec` as `answer` does, and
/// the environment of chat 7 inside it: sandboxed, with its socket and its token.
fn an_app(
    answer: crate::hookwire::SecretExecuting,
    asked: Arc<AtomicBool>,
) -> (
    tempfile::TempDir,
    crate::hookwire::Reading,
    Vec<(String, String)>,
) {
    an_app_answering(
        answer,
        Box::new(|_, _| crate::hookwire::Answer::No { why: String::new() }),
        asked,
    )
}

/// [`an_app`], answering every ask as `answerer` does.
fn an_app_answering(
    answer: crate::hookwire::SecretExecuting,
    answerer: crate::hookwire::Answerer,
    asked: Arc<AtomicBool>,
) -> (
    tempfile::TempDir,
    crate::hookwire::Reading,
    Vec<(String, String)>,
) {
    use crate::hookwire::{Hearing, Listener};
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hooks.sock");
    let listener = Listener::bind(dir.path(), &socket).unwrap();
    let token = listener.tokens().issue_to_this_process(7).unwrap();
    let reading = listener.hear(Hearing {
        secret_exec: Box::new(move |ask, reader, writer| {
            asked.store(true, Ordering::SeqCst);
            answer(ask, reader, writer);
        }),
        blocked: Box::new(|_| {}),
        doing: Box::new(|_| {}),
        touching: Box::new(|_| {}),
        each: Box::new(|_| Ok(())),
        answer: answerer,
        noticed: Box::new(|_| {}),
        saved: Box::new(|_| {}),
        refused: Box::new(|_| Ok(())),
        tool: Box::new(|_| Ok(())),
        permission: Box::new(|_| None),
    });
    let env = vec![
        ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
        (crate::hookwire::SANDBOXED_ENV.to_owned(), "1".to_owned()),
        (
            crate::hookwire::SOCKET_ENV.to_owned(),
            socket.display().to_string(),
        ),
        (crate::hookwire::CHAT_ENV.to_owned(), "7".to_owned()),
        (
            crate::hookwire::TOKEN_ENV.to_owned(),
            token.expose().to_owned(),
        ),
    ];
    (dir, reading, env)
}

fn chat_ctx(root: &Path, env: &[(String, String)]) -> Ctx {
    let pairs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    Ctx::new(root, Env::of(&pairs))
}

/// The app that runs it: the project's chat 7, as persona `devops`, unwrapped.
fn the_app_runs_it(root: &Path) -> crate::hookwire::SecretExecuting {
    let asker = asker(root, Some("devops"));
    Box::new(move |ask, reader, writer| {
        serve_wrapped(&asker, ask.secret_exec, reader, writer, Wrap::Unwrapped);
    })
}

#[test]
fn inside_a_sandboxed_chat_secret_exec_is_run_by_the_app() {
    let project = project();
    let asked = Arc::new(AtomicBool::new(false));
    let (_dir, _reading, env) = an_app(the_app_runs_it(project.path()), Arc::clone(&asked));
    // The chat cannot read the vault: its file is gone from where the chat looks, and only
    // the app's own process reads it.
    let ctx = chat_ctx(project.path(), &env);
    let req = Request {
        vault: "team".into(),
        env: vec!["X=TOKEN".into()],
        command: sh(r#"test -n "$X" && echo ok"#),
        ..Request::default()
    };
    let mut rec = Rec::default();
    let code = exec::exec(&ctx, &req, &mut rec);
    assert!(asked.load(Ordering::SeqCst), "the app was asked");
    assert_eq!(code, 0, "{:?}", rec.said);
    assert_eq!(String::from_utf8_lossy(&rec.out), "ok\n");
}

#[test]
fn a_refusal_from_the_app_is_the_chats_error_and_nothing_runs_here() {
    let project = project();
    let asked = Arc::new(AtomicBool::new(false));
    let (_dir, _reading, env) = an_app(the_app_runs_it(project.path()), Arc::clone(&asked));
    let ctx = chat_ctx(project.path(), &env);
    let marker = project.path().join("ran-here");
    let req = Request {
        vault: "other".into(),
        env: vec!["X=TOKEN".into()],
        command: sh(&format!("touch {}", marker.display())),
        ..Request::default()
    };
    let mut rec = Rec::default();
    let code = exec::exec(&ctx, &req, &mut rec);
    assert_eq!(code, 1);
    assert!(
        matches!(rec.said.as_slice(), [Say::Err(why)] if why.contains("is not one persona 'devops' may use")),
        "{:?}",
        rec.said
    );
    assert!(!marker.exists());
}

#[test]
fn an_app_with_nothing_that_answers_leaves_the_command_to_run_here() {
    let project = project();
    let asked = Arc::new(AtomicBool::new(false));
    let (_dir, _reading, env) = an_app(
        Box::new(|_, _, writer| not_answered(writer)),
        Arc::clone(&asked),
    );
    let ctx = chat_ctx(project.path(), &env);
    let req = Request {
        vault: "team".into(),
        env: vec!["X=TOKEN".into()],
        command: sh(r#"test -n "$X" && echo local"#),
        ..Request::default()
    };
    let mut rec = Rec::default();
    let code = exec::exec(&ctx, &req, &mut rec);
    assert!(asked.load(Ordering::SeqCst));
    assert_eq!(code, 0, "{:?}", rec.said);
    assert_eq!(String::from_utf8_lossy(&rec.out), "local\n");
}

#[test]
fn a_chat_that_is_not_sandboxed_reads_its_vault_itself() {
    let project = project();
    let asked = Arc::new(AtomicBool::new(false));
    let (_dir, _reading, mut env) = an_app(the_app_runs_it(project.path()), Arc::clone(&asked));
    env.retain(|(k, _)| k != crate::hookwire::SANDBOXED_ENV);
    let ctx = chat_ctx(project.path(), &env);
    let req = Request {
        vault: "team".into(),
        env: vec!["X=TOKEN".into()],
        command: sh(r#"echo "$X""#),
        ..Request::default()
    };
    let mut rec = Rec::default();
    assert_eq!(exec::exec(&ctx, &req, &mut rec), 0);
    assert!(!asked.load(Ordering::SeqCst), "the app was not asked");
    assert_eq!(String::from_utf8_lossy(&rec.out), "***\n");
}

/// The asker's half against the app's, on a connection pair: the line the chat writes is the
/// one the app reads, token and all, and what the app writes back is what the chat prints.
#[test]
fn what_the_chat_writes_the_app_reads_and_what_the_app_writes_the_chat_prints() {
    let project = project();
    let (chat_end, app_end) = std::os::unix::net::UnixStream::pair().unwrap();
    let asker = asker(project.path(), Some("devops"));
    let app = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(app_end.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let mut value: Value = serde_json::from_str(&line).unwrap();
        let token = value.as_object_mut().unwrap().remove("token");
        let ask: Ask = serde_json::from_value(value).unwrap();
        serve_wrapped(
            &asker,
            ask.secret_exec,
            Box::new(reader),
            Box::new(app_end),
            Wrap::Unwrapped,
        );
        (ask.chat, token)
    });
    let env = [("PATH", "/usr/bin:/bin")];
    let ctx = Ctx::new(project.path(), Env::of(&env));
    let req = Request {
        vault: "team".into(),
        env: vec!["X=TOKEN".into()],
        ..Request::default()
    };
    let mut rec = Rec::default();
    let code = converse(
        &ctx,
        &req,
        &sh(r#"test -n "$X" && echo ok; echo "$X" >&2; exit 4"#),
        7,
        Some(crate::hookwire::ChatToken::from("tok")),
        chat_end,
        &mut rec,
    );
    assert_eq!(code, Some(4), "{:?}", rec.said);
    assert_eq!(String::from_utf8_lossy(&rec.out), "ok\n");
    assert_eq!(String::from_utf8_lossy(&rec.err), "***\n");
    let (chat, token) = app.join().unwrap();
    assert_eq!(chat, 7);
    assert_eq!(token, Some(Value::String("tok".into())));
}

#[test]
fn a_vault_the_persona_names_only_in_its_own_file_is_not_opened_for_a_chat() {
    let project = project();
    let dir = project.path().join("personas/ops");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("persona.md"),
        "---\nname: ops\nvault: other\n---\n# ops\n",
    )
    .unwrap();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    assert_eq!(
        cmd::persona_vault(&ctx, "ops").as_deref(),
        Ok("other"),
        "the file names it"
    );
    let why = authorise(&ctx, Some("ops"), "other").unwrap_err();
    assert!(why.contains("a chat can edit that file"), "{why}");
    assert_eq!(authorise(&ctx, Some("devops"), "team"), Ok(()));
}

/// D-1407-9: a `secret exec` carrying the chat's token from a process outside the chat is told
/// why, and never falls back to a run here that would only meet the vault's denial.
#[test]
fn from_outside_the_chat_secret_exec_says_why_and_runs_nothing() {
    use crate::hookwire::{Hearing, Listener, Program};
    let project = project();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hooks.sock");
    let listener = Listener::bind(dir.path(), &socket).unwrap();
    // The chat's program is a process this test is not inside.
    let mut program = crate::forklock::spawn(
        std::process::Command::new("/bin/sleep")
            .arg("30")
            .stdin(std::process::Stdio::null()),
    )
    .unwrap();
    let token = listener.tokens().issue(7).unwrap();
    listener
        .tokens()
        .bind(7, Program::of(program.id()).expect("the program, read"));
    let _reading = listener.hear(Hearing {
        secret_exec: Box::new(|_, _, _| panic!("nothing outside the chat is run")),
        blocked: Box::new(|_| {}),
        doing: Box::new(|_| {}),
        touching: Box::new(|_| {}),
        each: Box::new(|_| Ok(())),
        answer: Box::new(|_, _| crate::hookwire::Answer::No { why: String::new() }),
        noticed: Box::new(|_| {}),
        saved: Box::new(|_| {}),
        refused: Box::new(|_| Ok(())),
        tool: Box::new(|_| Ok(())),
        permission: Box::new(|_| None),
    });
    let env = vec![
        ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
        (crate::hookwire::SANDBOXED_ENV.to_owned(), "1".to_owned()),
        (
            crate::hookwire::SOCKET_ENV.to_owned(),
            socket.display().to_string(),
        ),
        (crate::hookwire::CHAT_ENV.to_owned(), "7".to_owned()),
        (
            crate::hookwire::TOKEN_ENV.to_owned(),
            token.expose().to_owned(),
        ),
    ];
    let marker = project.path().join("ran-here");
    let req = Request {
        vault: "team".into(),
        env: vec!["X=TOKEN".into()],
        command: sh(&format!("touch {}", marker.display())),
        ..Request::default()
    };
    let mut rec = Rec::default();
    let code = exec::exec(&chat_ctx(project.path(), &env), &req, &mut rec);
    assert_eq!(code, 1);
    assert!(
        matches!(rec.said.as_slice(), [Say::Err(why)] if why == crate::hookwire::OUTSIDE_THE_CHAT),
        "{:?}",
        rec.said
    );
    assert!(!marker.exists(), "no run here, nor in the app");
    let _ = program.kill();
    let _ = program.wait();
}

// ---- a refused vault offers a way forward, and `vault list` from a chat (#1430) ------------

/// A project as [`project`]'s, with a third vault, `ops`, tagged for persona `ops`, which the
/// project defines.
fn project_with_ops() -> tempfile::TempDir {
    let tmp = project();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("personas/ops")).unwrap();
    std::fs::write(
        root.join("personas/ops/persona.md"),
        "---\nname: ops\n---\n\nOps.\n",
    )
    .unwrap();
    std::fs::write(
        root.join("vaults.json"),
        serde_json::json!({ "vaults": {
            "team": {"provider": "plain-file", "config": {"file": "team.json"}, "persona": "devops"},
            "other": {"provider": "plain-file", "config": {"file": "other.json"}},
            "ops": {"provider": "plain-file", "config": {"file": "team.json"}, "persona": "ops"},
        }})
        .to_string(),
    )
    .unwrap();
    tmp
}

/// The one refusal `frames` is.
fn refusal(frames: &[Frame]) -> &str {
    let [Frame::Refused { why, code: 1 }] = frames else {
        panic!("one refusal: {frames:?}");
    };
    why
}

/// `echo ok` with the vault's `TOKEN`, asked of `vault`.
fn wants_token(root: &Path, vault: &str) -> Wanted {
    let mut want = wanted(root, vault, sh(r#"test -n "$X" && echo ok"#));
    want.env = vec!["X=TOKEN".into()];
    want
}

/// An administrator's policy for this test's thread, put back to none when dropped.
struct Policed;

impl Policed {
    fn by(json: &str) -> Self {
        crate::sandbox::policy::set_for_this_test(crate::sandbox::policy::Locks::parse(
            json,
            Path::new("/etc/purlis/policy.json"),
        ));
        Self
    }
}

impl Drop for Policed {
    fn drop(&mut self) {
        crate::sandbox::policy::set_for_this_test(crate::sandbox::policy::Locks::none());
    }
}

#[test]
fn a_vault_tagged_for_another_persona_is_refused_naming_both_ways_forward() {
    let project = project_with_ops();
    let root = project.path();
    let frames = served(&asker(root, Some("devops")), wants_token(root, "ops"));
    assert_eq!(
        refusal(&frames),
        "vault 'ops' is not one persona 'devops' may use, so purlis did not open it (it may use \
         'team'). Two ways forward: ask the operator to press Allow in the notice on this \
         chat's tab, then run the command again (no restart is needed); or, to have 'ops', the \
         persona the vault is tagged for, do the work, dispatch to it: `purlis dispatch --to \
         ops`. A chat's persona is fixed for its life, so nothing run in this chat changes \
         which vaults it may use."
    );
}

#[test]
fn a_refusal_never_suggests_changing_the_chats_own_persona() {
    let project = project_with_ops();
    let root = project.path();
    for vault in ["ops", "other"] {
        let frames = served(&asker(root, Some("devops")), wants_token(root, vault));
        let why = refusal(&frames);
        assert!(!why.contains("persona use"), "{why}");
        assert!(!why.contains("vault add"), "{why}");
        assert!(why.contains("fixed for its life"), "{why}");
    }
}

#[test]
fn a_vault_tagged_for_nobody_names_the_one_way_forward() {
    let project = project_with_ops();
    let root = project.path();
    let frames = served(&asker(root, Some("devops")), wants_token(root, "other"));
    let why = refusal(&frames);
    assert!(why.contains("The way forward: ask the operator"), "{why}");
    assert!(!why.contains("--persona"), "nobody to hand off to: {why}");
}

#[test]
fn a_tag_that_names_no_persona_the_project_defines_offers_no_dispatch() {
    // The registry's tag is any text its committed half holds (#1057 allows a label), so a
    // dispatch is named only for a persona the project defines.
    let project = project_with_ops();
    let root = project.path();
    std::fs::write(
        root.join("vaults.json"),
        serde_json::json!({ "vaults": {
            "team": {"provider": "plain-file", "config": {"file": "team.json"}, "persona": "devops"},
            "label": {"provider": "plain-file", "config": {"file": "team.json"},
                      "persona": "the platform team"},
            "unknown": {"provider": "plain-file", "config": {"file": "team.json"},
                        "persona": "ghost"},
        }})
        .to_string(),
    )
    .unwrap();
    let ctx = Ctx::new(root, Env::of(&[]));
    for vault in ["label", "unknown"] {
        let not = not_tagged(&ctx, Some("devops"), vault).expect("still one to allow");
        assert_eq!(dispatchable(&ctx, &not), None, "{vault}");
        let frames = served(&asker(root, Some("devops")), wants_token(root, vault));
        let why = refusal(&frames);
        assert!(why.contains("The way forward: ask the operator"), "{why}");
        assert!(!why.contains("dispatch"), "{why}");
        assert!(!why.contains("--persona"), "{why}");
    }
    let ops = NotTagged {
        vault: "ops".into(),
        persona: "devops".into(),
        tagged_for: Some("ops".into()),
    };
    assert_eq!(dispatchable(&ctx, &ops).as_deref(), Some("ops"));
}

#[test]
fn a_vault_you_allowed_for_the_persona_runs_the_same_command_with_no_restart() {
    let project = project_with_ops();
    let root = project.path();
    // One chat, as the app recorded it when it started: never rebuilt below.
    let chat = asker(root, Some("devops"));
    assert!(matches!(
        served(&chat, wants_token(root, "ops")).as_slice(),
        [Frame::Refused { .. }]
    ));
    assert_eq!(
        not_tagged(&Ctx::new(root, chat.env.clone()), Some("devops"), "ops"),
        Some(NotTagged {
            vault: "ops".into(),
            persona: "devops".into(),
            tagged_for: Some("ops".into()),
        })
    );

    crate::sandbox::local::grant_vault(root, "ops", "devops").unwrap();

    let frames = served(&chat, wants_token(root, "ops"));
    assert_eq!(frames.last(), Some(&Frame::Exit(0)), "{frames:?}");
    assert_eq!(stdout(&frames), "ok\n");
    assert_eq!(
        not_tagged(&Ctx::new(root, chat.env.clone()), Some("devops"), "ops"),
        None,
        "nothing left to allow"
    );
    // The registry is as it was: the grant is this machine's, beside it.
    let registry: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("vaults.json")).unwrap()).unwrap();
    assert_eq!(registry["vaults"]["ops"]["persona"], "ops");
    // And it is for that persona alone.
    assert!(matches!(
        served(&asker(root, Some("steward")), wants_token(root, "ops")).as_slice(),
        [Frame::Refused { .. }]
    ));

    crate::sandbox::local::revoke_vault(root, "ops", "devops").unwrap();
    assert!(
        matches!(
            served(&chat, wants_token(root, "ops")).as_slice(),
            [Frame::Refused { .. }]
        ),
        "a revoke reaches the next run too"
    );
}

#[test]
fn a_grant_for_a_vault_the_registry_no_longer_holds_opens_nothing() {
    let project = project_with_ops();
    let root = project.path();
    crate::sandbox::local::grant_vault(root, "gone", "devops").unwrap();
    let frames = served(&asker(root, Some("devops")), wants_token(root, "gone"));
    // In the registry's own words, with no way forward named: there is no Notice to press
    // Allow on, and nobody to dispatch to. A mistyped name reads the same.
    assert_eq!(
        refusal(&frames),
        "no vault named 'gone'. Register one with `purlis vault add gone`."
    );
    let typo = served(&asker(root, Some("devops")), wants_token(root, "op"));
    assert_eq!(
        refusal(&typo),
        "no vault named 'op'. Register one with `purlis vault add op`."
    );
    for why in [refusal(&frames), refusal(&typo)] {
        assert!(!why.contains("Allow"), "{why}");
        assert!(!why.contains("way forward"), "{why}");
        assert!(!why.contains("dispatch"), "{why}");
    }
    assert_eq!(
        not_tagged(&Ctx::new(root, Env::of(&[])), Some("devops"), "op"),
        None
    );
    assert_eq!(
        not_tagged(&Ctx::new(root, Env::of(&[])), Some("devops"), "gone"),
        None,
        "and nothing is offered for it"
    );
}

#[test]
fn policy_can_forbid_the_allow_and_takes_a_grant_already_made_away() {
    let project = project_with_ops();
    let root = project.path();
    crate::sandbox::local::grant_vault(root, "ops", "devops").unwrap();
    let _policy = Policed::by(r#"{"owner": "IT", "sandbox": {"vault-grants": false}}"#);
    let frames = served(&asker(root, Some("devops")), wants_token(root, "ops"));
    let why = refusal(&frames);
    assert!(
        why.contains(
            "Policy forbids allowing a persona a vault it is not tagged for. Locked by policy, \
             set by IT in /etc/purlis/policy.json. The way forward: to have 'ops', the persona \
             the vault is tagged for, do the work, dispatch to it: `purlis dispatch --to ops`."
        ),
        "{why}"
    );
    assert!(!why.contains("press Allow"), "{why}");
    // A vault the registry tags for the persona is not a grant, and still opens.
    let frames = served(&asker(root, Some("devops")), wants_token(root, "team"));
    assert_eq!(frames.last(), Some(&Frame::Exit(0)), "{frames:?}");
}

#[test]
fn nothing_a_chats_line_says_changes_its_persona_or_tags_a_vault() {
    let project = project_with_ops();
    let root = project.path();
    // The line a chat would forge: a persona and a grant beside what `secret exec` sends.
    let forged = serde_json::json!({
        "chat": 7,
        "persona": "ops",
        "allow": {"vault": "ops", "persona": "devops"},
        "secret_exec": {
            "vault": "ops",
            "persona": "ops",
            "tagged_for": "devops",
            "grant": true,
            "env": ["X=TOKEN"],
            "command": ["/bin/sh", "-c", "echo ok"],
            "cwd": root.join("work"),
            "environment": [["PATH", "/usr/bin:/bin"], ["PURLIS_PERSONA", "ops"],
                            ["CHARTER_PERSONA", "ops"]],
        },
    });
    let ask: Ask = serde_json::from_value(forged).expect("read as a secret exec");
    let frames = served(&asker(root, Some("devops")), ask.secret_exec);
    assert!(
        refusal(&frames).starts_with("vault 'ops' is not one persona 'devops' may use"),
        "the app's record of the chat decides: {frames:?}"
    );
    assert_eq!(crate::sandbox::local::granted_vaults(root), Vec::new());
    let registry: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("vaults.json")).unwrap()).unwrap();
    assert_eq!(registry["vaults"]["ops"]["persona"], "ops");
}

#[test]
fn the_app_lists_every_vault_with_its_tag_and_whether_the_persona_may_use_it() {
    let project = project_with_ops();
    let root = project.path();
    crate::sandbox::local::grant_vault(root, "other", "devops").unwrap();
    let said = listed(&Ctx::new(root, Env::of(&[])), Some("devops")).unwrap();
    let row = |name: &str, persona: Option<&str>, usable| Listed {
        name: name.into(),
        provider: "plain-file".into(),
        persona: persona.map(str::to_owned),
        usable,
    };
    assert_eq!(
        said,
        vec![
            row("ops", Some("ops"), false),
            row("other", None, true),
            row("team", Some("devops"), true),
        ]
    );
    // Nothing of a vault's contents or of where it is kept is in the answer.
    let wire = serde_json::to_string(&said).unwrap();
    for never in [TOKEN, "TOKEN", "team.json", "other.json", "config"] {
        assert!(!wire.contains(never), "{never} in {wire}");
    }
    // A chat on no persona may use none.
    assert!(
        listed(&Ctx::new(root, Env::of(&[])), None)
            .unwrap()
            .iter()
            .all(|one| !one.usable)
    );
    // And under the policy lock a grant made here lists as it opens: not at all.
    let _policy = Policed::by(r#"{"sandbox": {"vault-grants": false}}"#);
    assert_eq!(
        listed(&Ctx::new(root, Env::of(&[])), Some("devops")).unwrap(),
        vec![
            row("ops", Some("ops"), false),
            row("other", None, false),
            row("team", Some("devops"), true),
        ]
    );
}

/// The table a chat reads for [`project_with_ops`], as persona `devops`.
const LISTED_FOR_DEVOPS: &str = "VAULT  PROVIDER    PERSONA  THIS CHAT\n\
                                 -----  ----------  -------  ---------\n\
                                 ops    plain-file  ops      not allowed\n\
                                 other  plain-file  —        not allowed\n\
                                 team   plain-file  devops   may use\n";

#[test]
fn a_chat_reads_each_vault_its_tag_and_whether_it_may_use_it_and_the_ways_forward() {
    let project = project_with_ops();
    let vaults = listed(&Ctx::new(project.path(), Env::of(&[])), Some("devops")).unwrap();
    let said = |listing: Listing| {
        let mut rec = Rec::default();
        super::super::vaultcmd::list_for_a_chat(&listing, &mut rec);
        (String::from_utf8_lossy(&rec.out).into_owned(), rec.said)
    };
    let devops = |vaults: &[Listed], allow_locked| Listing {
        persona: Some("devops".into()),
        vaults: vaults.to_vec(),
        allow_locked,
    };
    let (out, lines) = said(devops(&vaults, false));
    assert_eq!(out, LISTED_FOR_DEVOPS);
    assert_eq!(
        lines,
        vec![
            Say::Info("This chat runs as 'devops'.".into()),
            Say::Info(super::super::vaultcmd::NOT_ALLOWED_ROUTES.into()),
        ]
    );
    // Where policy forbids the Allow, it is not named: only the dispatch is.
    let (out, lines) = said(devops(&vaults, true));
    assert_eq!(out, LISTED_FOR_DEVOPS);
    assert_eq!(
        lines[1],
        Say::Info(super::super::vaultcmd::NOT_ALLOWED_ROUTES_LOCKED.into())
    );
    assert!(!super::super::vaultcmd::NOT_ALLOWED_ROUTES_LOCKED.contains("Allow"));
    assert!(super::super::vaultcmd::NOT_ALLOWED_ROUTES.contains("press Allow"));
    // With nothing refused there is no way forward to name, and a chat on no persona says so.
    let (_, lines) = said(devops(&vaults[2..], false));
    assert_eq!(lines, vec![Say::Info("This chat runs as 'devops'.".into())]);
    let (_, lines) = said(Listing {
        persona: None,
        vaults: Vec::new(),
        allow_locked: false,
    });
    assert_eq!(lines, vec![Say::Info("No vaults configured.".into())]);
}

/// The app's answer to every ask: the vaults of the project at `root`, for a chat it started
/// as `devops`, whatever the line says.
fn the_app_lists(root: &Path) -> crate::hookwire::Answerer {
    let root = root.to_path_buf();
    Box::new(move |_, ask| match ask {
        crate::hookwire::Ask::Vaults { chat: 7 } => {
            match listed(&Ctx::new(&root, Env::of(&[])), Some("devops")) {
                Ok(vaults) => crate::hookwire::Answer::Vaults {
                    persona: Some("devops".into()),
                    vaults,
                    allow_locked: false,
                },
                Err(why) => crate::hookwire::Answer::No { why },
            }
        }
        _ => crate::hookwire::Answer::No {
            why: "not a listing".into(),
        },
    })
}

#[test]
fn what_a_sandboxed_chat_is_compiled_to_denies_it_every_place_a_listing_would_read() {
    // Why `vault list` in a chat is the app's answer: the chat's own sandbox denies it the
    // provider's session and the project's vaults, reads included, on every harness purlis
    // sandboxes on this system. Where it has no wrap for one yet (Codex and opencode off
    // macOS), no sandboxed chat of it starts, so there is no chat to deny anything.
    let project = project_with_ops();
    let root = project.path().canonicalize().unwrap();
    let home = tempfile::tempdir().unwrap();
    let home = home.path().canonicalize().unwrap();
    for harness in [Harness::ClaudeCode, Harness::Codex, Harness::Opencode] {
        let machine = Machine {
            env: Env::of(&[]),
            home: Some(home.clone()),
            os: Os::this(),
        };
        // As `sandbox/tests.rs` holds it: off macOS, the wrap of Codex and of opencode is
        // refused, by name.
        if harness != Harness::ClaudeCode && machine.os != Os::MacOs {
            assert_eq!(
                held_to(harness, &root, &machine).err(),
                Some(crate::sandbox::NotStarted::Uncompilable(
                    crate::sandbox::Uncompilable {
                        harness,
                        unheld: crate::sandbox::Unheld::Wrap(machine.os),
                    }
                ))
            );
            continue;
        }
        let confined = confines_of(harness, &root, &machine);
        let denied_to_read = |path: &Path| {
            confined.denied.iter().any(|denial| {
                denial.class == crate::sandbox::Class::Vaults
                    && denial.access == crate::sandbox::Access::ReadWrite
                    && path.starts_with(&denial.path)
            })
        };
        for place in [
            home.join(".config/op/config"),
            home.join(".op/config"),
            Ctx::new(&root, Env::of(&[])).vaults_dir().join("team.json"),
        ] {
            assert!(denied_to_read(&place), "{harness:?}: {}", place.display());
        }
    }
}

#[test]
fn vault_list_in_a_sandboxed_chat_is_the_apps_answer_and_asks_no_provider() {
    let project = project_with_ops();
    let root = project.path();
    let (_dir, _reading, env) = an_app_answering(
        Box::new(|_, _, writer| not_answered(writer)),
        the_app_lists(root),
        Arc::new(AtomicBool::new(false)),
    );
    // The chat's own view of the project has no registry to read at all.
    let elsewhere = tempfile::tempdir().unwrap();
    let mut rec = Rec::default();
    let code = super::super::vaultcmd::list(&chat_ctx(elsewhere.path(), &env), &mut rec);
    assert_eq!(code, 0, "{:?}", rec.said);
    assert_eq!(String::from_utf8_lossy(&rec.out), LISTED_FOR_DEVOPS);
    assert_eq!(
        rec.said,
        vec![
            Say::Info("This chat runs as 'devops'.".into()),
            Say::Info(super::super::vaultcmd::NOT_ALLOWED_ROUTES.into()),
        ]
    );
}

#[test]
fn vault_list_outside_a_sandboxed_chat_asks_no_app() {
    let project = project_with_ops();
    let asked = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&asked);
    let (_dir, _reading, mut env) = an_app_answering(
        Box::new(|_, _, writer| not_answered(writer)),
        Box::new(move |_, _| {
            seen.store(true, Ordering::SeqCst);
            crate::hookwire::Answer::No {
                why: "asked".into(),
            }
        }),
        Arc::new(AtomicBool::new(false)),
    );
    env.retain(|(k, _)| k != crate::hookwire::SANDBOXED_ENV);
    assert_eq!(listing_from_the_app(&chat_ctx(project.path(), &env)), None);
    assert!(!asked.load(Ordering::SeqCst));
}

// ----------------------------------------------------------------------------------------
// a host the run's sandbox refused (the brokered half of the block flow, #1338/#1342)

/// What a brokered command's own output gives the chat when the run's proxy refused its host:
/// a client's word for the `403` (kubectl's "Forbidden"), which names no host and is no
/// refusal the chat's block hook knows. So the chat alone can raise no Notice for it.
#[test]
fn a_clients_word_for_the_runs_refusal_names_no_block_the_chat_could_raise() {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": "purlis secret exec devops -- kubectl get nodes"},
        "tool_response": {
            "stdout": "",
            "stderr": "Unable to connect to the server: Forbidden\n",
        },
    });
    let place = crate::sandboxblock::Place {
        root: Path::new("/plane"),
        chat: Path::new("/plane/workspaces/runners"),
        cwd: Path::new("/plane/workspaces/runners"),
        home: None,
    };
    assert_eq!(crate::sandboxblock::detect(&payload, &place), Vec::new());
}

/// Every frame, and every block told for the chat's Notice, of `wanted` served unwrapped with
/// the run's proxy refusing `refused` (`host:port`) while it runs.
fn served_refusing(
    asker: &Asker,
    wanted: Wanted,
    refused: &'static str,
) -> (Vec<Frame>, Vec<crate::hookwire::SandboxBlocked>) {
    let (reader, _keep) = held_open();
    let wire = Wire::default();
    let told: Arc<Mutex<Vec<crate::hookwire::SandboxBlocked>>> = Arc::default();
    serve_in(
        asker,
        wanted,
        reader,
        Box::new(wire.clone()),
        Wrap::Refusing(refused),
        {
            let told = Arc::clone(&told);
            Arc::new(move |block| told.lock().unwrap().push(block))
        },
    );
    let frames = String::from_utf8(wire.0.lock().unwrap().clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("every line is a frame"))
        .collect();
    let told = told.lock().unwrap().clone();
    (frames, told)
}

fn notes(frames: &[Frame]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            Frame::Note(note) => Some(note.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_host_the_runs_sandbox_refused_raises_the_asking_chats_block_naming_it_whole() {
    let project = project();
    let host = "api.cluster.example-k8s.com:6443";
    let (frames, told) = served_refusing(
        &asker(project.path(), Some("devops")),
        wanted(
            project.path(),
            "team",
            sh("echo 'Unable to connect to the server: Forbidden' >&2; exit 1"),
        ),
        "api.cluster.example-k8s.com:6443",
    );
    assert_eq!(
        told,
        vec![crate::hookwire::SandboxBlocked {
            chat: 7,
            sandbox_blocked: crate::sandboxblock::Block {
                operation: crate::sandboxblock::Operation::Connect,
                kind: crate::sandboxblock::Kind::Host,
                ours: false,
            },
            harness: None,
            target: Some(host.to_owned()),
        }],
        "the chat that asked is told, with the host and port the proxy refused"
    );
    // The command's own error is still the chat's, and purlis says what it was, in its words.
    assert_eq!(
        stderr(&frames),
        "Unable to connect to the server: Forbidden\n"
    );
    let said = notes(&frames).join("\n");
    assert!(said.contains(host), "{said}");
    assert!(said.contains("sandbox refused"), "{said}");
    assert!(said.contains("Allow"), "{said}");
    // Said before the status, which is the last frame.
    assert_eq!(frames.last(), Some(&Frame::Exit(1)), "{frames:?}");
    // Not in the words the chat's own block hook reads from stderr, so one refusal is one
    // Notice, and the host it names is the proxy's alone.
    assert!(!said.contains("does not allow"), "{said}");
}

/// A host the command names with a vault value in it is refused like any other, and is never
/// named, told or offered: the note says only that one was withheld, in any case of the value.
#[test]
fn a_refused_host_carrying_a_vault_value_is_withheld_from_the_note_and_the_notice() {
    for refused in [
        "s3cret-value.tenant.example:443",
        "api.S3CRET-VALUE.example:6443",
    ] {
        let project = project();
        let mut want = wanted(project.path(), "team", sh("exit 1"));
        want.env = vec!["X=TOKEN".into()];
        let (frames, told) = served_refusing(&asker(project.path(), Some("devops")), want, refused);
        assert_eq!(told, Vec::new(), "{refused}: no Notice names it");
        let said = notes(&frames).join("\n");
        assert!(!said.to_lowercase().contains(TOKEN), "{said}");
        assert!(!said.contains("tenant"), "nothing of the host: {said}");
        assert!(said.contains(WITHHELD_NOTE), "{said}");
        assert_eq!(frames.last(), Some(&Frame::Exit(1)), "{frames:?}");
    }
}

#[test]
fn a_host_the_command_only_prints_is_never_told() {
    let project = project();
    let (frames, told) = served_refusing(
        &asker(project.path(), Some("devops")),
        wanted(
            project.path(),
            "team",
            sh("echo \"purlis's sandbox does not allow evil.example:443: no\" >&2; exit 1"),
        ),
        "refused.example:443",
    );
    let targets: Vec<Option<String>> = told.into_iter().map(|block| block.target).collect();
    assert_eq!(targets, vec![Some("refused.example:443".to_owned())]);
    assert!(!notes(&frames).join("\n").contains("evil.example"));
}

#[test]
fn a_run_nothing_was_refused_for_tells_nothing_and_says_nothing_more() {
    let project = project();
    let (reader, _keep) = held_open();
    let wire = Wire::default();
    let told: Arc<Mutex<Vec<crate::hookwire::SandboxBlocked>>> = Arc::default();
    serve_in(
        &asker(project.path(), Some("devops")),
        wanted(project.path(), "team", sh("echo fine")),
        reader,
        Box::new(wire.clone()),
        Wrap::Unwrapped,
        {
            let told = Arc::clone(&told);
            Arc::new(move |block| told.lock().unwrap().push(block))
        },
    );
    let frames: Vec<Frame> = String::from_utf8(wire.0.lock().unwrap().clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(told.lock().unwrap().is_empty());
    assert_eq!(notes(&frames), Vec::<String>::new());
    assert_eq!(frames.last(), Some(&Frame::Exit(0)));
}

/// The proxy applied for real, where this machine can apply a profile: a command through it to
/// a host the chat's hosts do not list is refused, and the chat is told that host and port.
#[cfg(target_os = "macos")]
#[test]
fn a_wrapped_childs_refused_tunnel_is_told_to_the_asking_chat() {
    let can_apply = crate::forklock::status(
        std::process::Command::new(crate::sandbox::backend::SANDBOX_EXEC).args([
            "-p",
            "(version 1)(allow default)",
            "/usr/bin/true",
        ]),
    )
    .is_ok_and(|s| s.success());
    if !can_apply {
        let _ = writeln!(
            std::io::stderr(),
            "skipped: this process is already sandboxed, so no second profile applies"
        );
        return;
    }
    let project = project();
    let root = project.path().canonicalize().unwrap();
    let (reader, _keep) = held_open();
    let wire = Wire::default();
    let told: Arc<Mutex<Vec<crate::hookwire::SandboxBlocked>>> = Arc::default();
    serve_in(
        &asker(&root, Some("devops")),
        wanted(
            &root,
            "team",
            // Refused before any connection is made, so no network is needed.
            sh(r#"/usr/bin/curl -sS -x "$HTTPS_PROXY" https://refused.invalid:6443/ 2>&1; exit 1"#),
        ),
        reader,
        Box::new(wire.clone()),
        Wrap::Seatbelt,
        {
            let told = Arc::clone(&told);
            Arc::new(move |block| told.lock().unwrap().push(block))
        },
    );
    let frames: Vec<Frame> = String::from_utf8(wire.0.lock().unwrap().clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let targets: Vec<Option<String>> = told
        .lock()
        .unwrap()
        .iter()
        .map(|block| block.target.clone())
        .collect();
    assert_eq!(
        targets,
        vec![Some("refused.invalid:6443".to_owned())],
        "{frames:?}"
    );
    assert!(
        notes(&frames).join("\n").contains("refused.invalid:6443"),
        "{frames:?}"
    );
}
