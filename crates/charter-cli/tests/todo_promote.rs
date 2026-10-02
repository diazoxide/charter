//! `charter ws todo promote <slug> --repo <repo>` (ADR 0088 §5), asked of the binary against a
//! stand-in `gh` that writes down every call and answers from a script. No test here reaches a
//! forge, and no issue is created anywhere.

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

/// What the stand-in answers for the repo, as GitHub's `GET repos/{o}/{r}` would.
fn repo_answer(visibility: &str) -> String {
    format!(
        "{{\"full_name\": \"acme/api\", \"visibility\": \"{visibility}\", \"has_issues\": true, \
         \"permissions\": {{\"pull\": true}}}}"
    )
}

const ISSUE: &str = "{\"id\": 1012, \"node_id\": \"I_kwDOAcme12\", \"number\": 12, \
                     \"title\": \"Port the picker\", \"state\": \"open\", \
                     \"html_url\": \"https://github.com/acme/api/issues/12\"}";

impl World {
    fn new(visibility: &str) -> World {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("plane");
        let home = base.join("home");
        let bin = base.join("bin");
        let ws = root.join("workspaces/alpha");
        for dir in [&ws, &home, &bin, &root.join("inventory")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        std::fs::write(
            ws.join("workspace.json"),
            "{\"name\": \"alpha\", \"repos\": [{\"name\": \"api\"}]}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("inventory/repos.json"),
            "{\"group\": \"acme\", \"repos\": [{\"name\": \"api\", \"path_with_namespace\": \
             \"acme/api\", \"forge\": \"github\", \"web_url\": \"https://github.com/acme/api\"}]}\n",
        )
        .unwrap();
        let log = base.join("gh-calls.log");
        let script = format!(
            "#!/bin/sh\n\
             echo '=== call' >> '{log}'\n\
             for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{log}'; done\n\
             case \"$*\" in\n\
             \"api --hostname github.com repos/acme/api\") echo '{repo}'; exit 0;;\n\
             \"api --hostname github.com -X POST repos/acme/api/issues \"*) echo '{ISSUE}'; exit 0;;\n\
             esac\n\
             echo 'gh: unexpected call' >&2\n\
             exit 1\n",
            log = log.display(),
            repo = repo_answer(visibility),
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

    fn charter(&self, args: &[&str]) -> Output {
        Command::new(charter())
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("CHARTER_ROOT", &self.root)
            .env("HOME", &self.home)
            .env("CHARTER_CONFIG_HOME", self.home.join("config"))
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("CHARTER_PANIC_LOG", self.home.join("no-panics.log"))
            .output()
            .expect("charter runs")
    }

    /// Record the picker todo, and answer its stem.
    fn the_picker(&self) -> String {
        let o = self.charter(&[
            "ws",
            "todo",
            "-w",
            "alpha",
            "Port the picker",
            "--now",
            "2026-10-02T08:00:00",
        ]);
        assert!(o.status.success(), "{}", err(&o));
        "20261002-080000-port-the-picker".to_string()
    }

    fn todos(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.root.join("workspaces/alpha/todos"))
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n != "MEMORY.md")
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn a_promote_names_the_repo_and_its_readers_opens_the_issue_and_closes_the_todo() {
    let w = World::new("private");
    let stem = w.the_picker();
    assert_eq!(w.todos(), [format!("{stem}.md")]);

    let o = w.charter(&[
        "ws",
        "todo",
        "-w",
        "alpha",
        "promote",
        "port-the-picker",
        "--repo",
        "api",
    ]);
    let said = format!("{}{}", out(&o), err(&o));
    assert!(o.status.success(), "{said}");
    let named = said
        .find("Opening an issue in api (acme/api at github.com, private)")
        .unwrap_or_else(|| panic!("the repo and its readers are named: {said}"));
    let opened = said
        .find("Opened github:github.com/acme/api#12")
        .unwrap_or_else(|| panic!("the issue's key is said: {said}"));
    assert!(named < opened, "named before it is sent: {said}");

    let calls = w.calls();
    let read = calls.find("repos/acme/api\n").expect("the repo was read");
    let sent = calls
        .find("-X\nPOST\nrepos/acme/api/issues")
        .expect("the issue was opened");
    assert!(read < sent, "read before anything was sent: {calls}");
    assert!(calls.contains("-f\ntitle=Port the picker\n"), "{calls}");
    assert!(
        calls.contains("-f\nlabels[]=ws:alpha\n"),
        "a private repo's issue is labelled: {calls}"
    );

    assert_eq!(w.todos(), Vec::<String>::new(), "the todo is closed");
    let memory = w.root.join("workspaces/alpha/memory");
    let journal: String = std::fs::read_dir(&memory)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name() != "MEMORY.md")
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect();
    assert!(
        journal.contains("Promoted todo: Port the picker → github:github.com/acme/api#12"),
        "{journal}"
    );
    let logs: Vec<PathBuf> = std::fs::read_dir(w.root.join("workspaces/alpha/work"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(logs.len(), 1, "one log, this device's");
    let line: serde_json::Value =
        serde_json::from_str(std::fs::read_to_string(&logs[0]).unwrap().trim()).unwrap();
    assert_eq!(line["op"], "alias");
    assert_eq!(line["from"], format!("todo:alpha/{stem}"));
    assert_eq!(line["to"], "github:github.com/acme/api#12");
    assert_eq!(line["cause"], "promoted");
}

#[test]
fn a_public_repo_is_said_to_be_public_before_anything_is_sent() {
    let w = World::new("public");
    w.the_picker();
    let o = w.charter(&["ws", "todo", "-w", "alpha", "promote", "port-the-picker"]);
    let said = format!("{}{}", out(&o), err(&o));
    assert!(o.status.success(), "{said}");
    assert!(
        said.contains("PUBLIC: everyone can read what is sent"),
        "{said}"
    );
    assert!(
        !w.calls().contains("labels[]"),
        "no workspace label on a public repo"
    );
}

#[test]
fn promote_with_no_slug_and_repo_without_promote_are_refused_and_nothing_is_recorded() {
    let w = World::new("private");
    let o = w.charter(&["ws", "todo", "-w", "alpha", "promote"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        err(&o).contains("`todo promote` needs the slug"),
        "{}",
        err(&o)
    );
    let o = w.charter(&["ws", "todo", "-w", "alpha", "Something", "--repo", "api"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("--repo is taken only with"), "{}", err(&o));
    assert_eq!(w.todos(), Vec::<String>::new(), "neither recorded a todo");
    assert_eq!(w.calls(), "", "nothing was asked of the forge");
}

#[test]
fn a_repo_that_is_not_the_workspaces_is_refused_before_the_forge_is_asked() {
    let w = World::new("private");
    w.the_picker();
    let o = w.charter(&[
        "ws",
        "todo",
        "-w",
        "alpha",
        "promote",
        "port-the-picker",
        "--repo",
        "web",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        err(&o).contains("'web' is not a repo of workspace 'alpha'"),
        "{}",
        err(&o)
    );
    assert_eq!(w.calls(), "");
    assert_eq!(w.todos().len(), 1, "the todo stays open");
}
