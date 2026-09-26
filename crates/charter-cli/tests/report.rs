//! `charter report bug|feature`, asked of the binary against a stand-in `gh` that writes down
//! every call and answers from a script. No test here reaches GitHub.
//!
//! What is pinned: a bare run shows the draft and sends nothing; only `--yes <digest>` naming
//! the draft that was shown files it; what reaches `gh` is scrubbed; `gh` is never handed a
//! token from the environment; and a saved panic becomes a draft bug.

use std::path::PathBuf;
use std::process::{Command, Output};

fn charter() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_charter"))
}

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
}

/// What the stand-in answers `gh issue create` with, and how `gh search issues` does.
const FILED: &str = "https://github.com/diazoxide/charter/issues/999";

impl World {
    fn new() -> World {
        World::with_gh("exit 0", "echo '[]'; exit 0")
    }

    /// `create` and `search` are the shell the stand-in runs for each.
    fn with_gh(create: &str, search: &str) -> World {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("plane");
        let home = base.join("home");
        let bin = base.join("bin");
        for dir in [
            &root.join("workspaces/acme-rollout/billing-api/.git"),
            &home,
            &bin,
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let log = base.join("gh-calls.log");
        let create = if create == "exit 0" {
            format!("echo '{FILED}'; exit 0")
        } else {
            create.to_string()
        };
        let script = format!(
            "#!/bin/sh\n\
             echo '=== call' >> '{log}'\n\
             for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{log}'; done\n\
             if [ -n \"$GH_TOKEN$GITHUB_TOKEN\" ]; then echo 'TOKEN-REACHED-GH' >> '{log}'; fi\n\
             case \"$1 $2\" in\n\
             \"issue create\") {create};;\n\
             \"search issues\") {search};;\n\
             esac\n\
             echo 'gh: unexpected call' >&2\n\
             exit 1\n",
            log = log.display(),
        );
        stand_in::program(&bin, "gh", &script);
        World {
            _tmp: tmp,
            root,
            home,
            bin,
        }
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.bin.parent().unwrap().join("gh-calls.log")).unwrap_or_default()
    }

    fn charter(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        Command::new(charter())
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("CHARTER_ROOT", &self.root)
            .env("HOME", &self.home)
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("CHARTER_PANIC_LOG", self.home.join("no-panics.log"))
            .envs(env.iter().copied())
            .output()
            .expect("charter runs")
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The digest the preview asks `--yes` to name.
fn digest(o: &Output) -> String {
    let text = out(o);
    let at = text.find("--yes ").expect("the preview names --yes") + "--yes ".len();
    text[at..at + 12].to_string()
}

#[test]
fn a_bare_run_shows_the_draft_and_the_duplicates_and_sends_nothing() {
    let w = World::with_gh(
        "exit 0",
        "echo '[{\"number\":7,\"title\":\"Status line is wrong\",\"state\":\"OPEN\",\
         \"url\":\"https://github.com/diazoxide/charter/issues/7\"}]'; exit 0",
    );
    let o = w.charter(
        &["report", "bug", "The status line is wrong\n\nIt says 3."],
        &[],
    );

    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("nothing has been sent"), "{text}");
    assert!(text.contains("Title: The status line is wrong"), "{text}");
    assert!(text.contains("It says 3."), "{text}");
    assert!(text.contains("#7"), "the duplicate is named: {text}");
    assert!(text.contains(&format!("--yes {}", digest(&o))), "{text}");
    let calls = w.calls();
    assert!(
        calls.contains("search\nissues\n--repo\ndiazoxide/charter\n"),
        "{calls}"
    );
    assert!(
        !calls.contains("issue\ncreate"),
        "a bare run filed: {calls}"
    );
}

#[test]
fn yes_with_the_digest_that_was_shown_files_exactly_that_draft_on_the_reporters_own_login() {
    let w = World::new();
    let args = ["report", "feature", "charter should report its own bugs"];
    let shown = w.charter(&args, &[]);
    let d = digest(&shown);

    let o = w.charter(
        &[&args[..], &["--yes", &d]].concat(),
        &[("GH_TOKEN", "a-plane-bot-token-value")],
    );

    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains(FILED), "{}", out(&o));
    let calls = w.calls();
    assert!(
        calls.contains(
            "issue\ncreate\n--repo\ndiazoxide/charter\n--title=charter should report its own bugs\n--body=charter should report its own bugs\n"
        ),
        "{calls}"
    );
    assert!(
        !calls.contains("TOKEN-REACHED-GH"),
        "gh was handed a token from the environment, not the reporter's own login: {calls}"
    );
}

#[test]
fn yes_with_a_digest_that_is_not_this_drafts_sends_nothing() {
    let w = World::new();
    let o = w.charter(
        &["report", "bug", "something broke", "--yes", "000000000000"],
        &[],
    );
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert!(err(&o).contains("not the draft"), "{}", err(&o));
    assert!(!w.calls().contains("issue\ncreate"), "{}", w.calls());
}

#[test]
fn a_secret_a_vault_value_or_a_private_path_never_reaches_gh() {
    let w = World::new();
    let token = format!("ghp_{}", "Zz9Yy8Xx".repeat(5));
    let body = format!(
        "sync failed in acme-rollout for billing-api\n\
         value was vault-value-7f3a9c\n\
         {token}\n\
         at {}/workspaces/acme-rollout/x",
        w.root.display()
    );
    let file = w.home.join("report.md");
    std::fs::write(&file, &body).unwrap();
    let args = ["report", "bug", "--from-file", file.to_str().unwrap()];
    let env = [("DEPLOY_KEY", "vault-value-7f3a9c")];
    let d = digest(&w.charter(&args, &env));
    let o = w.charter(&[&args[..], &["--yes", &d]].concat(), &env);

    assert!(o.status.success(), "{}", err(&o));
    let calls = w.calls();
    for leak in [
        "acme-rollout",
        "billing-api",
        "vault-value-7f3a9c",
        token.as_str(),
        w.root.to_str().unwrap(),
    ] {
        assert!(!calls.contains(leak), "{leak} reached gh: {calls}");
        assert!(
            !out(&o).contains(leak),
            "{leak} was shown as sent: {}",
            out(&o)
        );
    }
    assert!(calls.contains("[env $DEPLOY_KEY]"), "{calls}");
    assert!(
        calls.contains("sync failed in [workspace] for [repo]"),
        "{calls}"
    );
}

#[test]
fn a_saved_panic_is_offered_and_becomes_a_draft_bug_with_its_place_and_version() {
    let w = World::new();
    let log = w.home.join("panics.log");
    std::fs::write(
        &log,
        "charter-panic pid 4 at 5 (seconds since 1970)\n\
         version: 1.2.3\n\
         thread: main\n\
         place: crates/charter-core/src/engine.rs:88:9\n\
         message: index out of bounds\n\
         backtrace:\n   0: somewhere\n\
         charter-panic end\n",
    )
    .unwrap();
    let env = [("CHARTER_PANIC_LOG", log.to_str().unwrap())];

    let bare = w.charter(&["report", "bug"], &env);
    assert_eq!(bare.status.code(), Some(1));
    assert!(
        err(&bare).contains("`charter report bug --panic`"),
        "the saved panic is offered: {}",
        err(&bare)
    );

    let o = w.charter(&["report", "bug", "--panic"], &env);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("Title: charter panicked at crates/charter-core/src/engine.rs:88:9"),
        "{text}"
    );
    assert!(text.contains("**charter version:** 1.2.3"), "{text}");
    assert!(text.contains("> index out of bounds"), "{text}");
    assert!(
        !text.contains("somewhere"),
        "the backtrace stays home: {text}"
    );
}

#[test]
fn a_bug_with_no_description_and_no_panic_is_refused() {
    let w = World::new();
    let o = w.charter(&["report", "bug"], &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("needs a description"), "{}", err(&o));
    assert!(w.calls().is_empty(), "{}", w.calls());
}

#[test]
fn when_gh_cannot_file_the_prefilled_link_is_printed_and_the_exit_says_it_failed() {
    let w = World::with_gh("echo 'HTTP 401: Bad credentials' >&2; exit 1", "exit 1");
    let args = ["report", "gap", "one more thing"];
    let shown = w.charter(&args, &[]);
    assert!(
        out(&shown).contains("could not search"),
        "a failed search is said, and the draft is still shown: {}",
        out(&shown)
    );
    let o = w.charter(&[&args[..], &["--yes", &digest(&shown)]].concat(), &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("Bad credentials"), "{}", err(&o));
    assert!(
        err(&o)
            .contains("https://github.com/diazoxide/charter/issues/new?title=one%20more%20thing"),
        "{}",
        err(&o)
    );
}
