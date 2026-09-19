//! `charter discover`, `clone` and `sync`, asked of the binary against a forge that is not
//! one.
//!
//! **The stand-in forge.** No test here reaches a network. A clone's URL is the HTTPS one
//! charter builds — `https://github.com/acme/<name>.git` — and the test's own `$HOME/.gitconfig`
//! rewrites that prefix to a directory of bare repositories with `url.<base>.insteadOf`. So
//! the path under test is the real one: charter builds the URL, gates the destination and
//! runs `git clone` through its hardened runner, and git's own config decides where the
//! bytes come from, exactly as it would for an operator with a mirror. `discover` asks a
//! stand-in `gh` — a shell script first on `PATH` that answers recorded API responses and
//! writes down how it was called.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn charter() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_charter"))
}

/// A plane with one workspace, a home whose git config points GitHub's `acme` at a local
/// directory of bare repos, and a directory for stand-in programs.
struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    forge: PathBuf,
    bin: PathBuf,
}

const IDENTITY: [(&str, &str); 6] = [
    ("GIT_AUTHOR_NAME", "Tester"),
    ("GIT_AUTHOR_EMAIL", "t@e.invalid"),
    ("GIT_COMMITTER_NAME", "Tester"),
    ("GIT_COMMITTER_EMAIL", "t@e.invalid"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_TERMINAL_PROMPT", "0"),
];

impl World {
    fn new() -> World {
        let tmp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("plane");
        let home = base.join("home");
        let forge = base.join("forge");
        let bin = base.join("bin");
        for dir in [&root, &home, &forge.join("acme"), &bin] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
        std::fs::write(
            root.join("charter.toml"),
            "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n",
        )
        .unwrap();
        std::fs::write(
            home.join(".gitconfig"),
            format!(
                "[url \"file://{}/acme/\"]\n\tinsteadOf = https://github.com/acme/\n",
                forge.display()
            ),
        )
        .unwrap();
        World {
            _tmp: tmp,
            root,
            home,
            forge,
            bin,
        }
    }

    /// git, for the test's own setup — never charter's runner.
    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", &self.home)
            .envs(IDENTITY)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A repo on the stand-in forge whose default branch is `branch`, with one commit.
    fn remote(&self, name: &str, branch: &str) -> PathBuf {
        let src = self.forge.join(format!("{name}-src"));
        std::fs::create_dir_all(&src).unwrap();
        self.git(&src, &["init", "-q", "-b", branch, "."]);
        std::fs::write(src.join("README.md"), "hello\n").unwrap();
        self.git(&src, &["add", "-A"]);
        self.git(&src, &["commit", "-q", "-m", "one"]);
        let bare = self.forge.join("acme").join(format!("{name}.git"));
        self.git(
            &self.forge,
            &["clone", "-q", "--bare", &src.display().to_string(), &bare.display().to_string()],
        );
        src
    }

    /// Commit `file` = `text` in the source repo and push it to the bare one.
    fn advance(&self, src: &Path, name: &str, file: &str, text: &str) -> String {
        std::fs::write(src.join(file), text).unwrap();
        self.git(src, &["add", "-A"]);
        self.git(src, &["commit", "-q", "-m", &format!("add {file}")]);
        let bare = self.forge.join("acme").join(format!("{name}.git"));
        let branch = self.git(src, &["symbolic-ref", "--short", "HEAD"]);
        self.git(src, &["push", "-q", &bare.display().to_string(), &branch]);
        self.git(src, &["rev-parse", "HEAD"])
    }

    fn inventory(&self, repos: Value) {
        std::fs::create_dir_all(self.root.join("inventory")).unwrap();
        let doc = json!({"group": "acme", "count": repos.as_array().map_or(0, Vec::len), "repos": repos});
        std::fs::write(
            self.root.join("inventory/repos.json"),
            serde_json::to_string_pretty(&doc).unwrap(),
        )
        .unwrap();
    }

    fn record(name: &str, default_branch: &str) -> Value {
        json!({
            "name": name,
            "path_with_namespace": format!("acme/{name}"),
            "ssh_url": format!("git@github.com:acme/{name}.git"),
            "default_branch": default_branch,
            "kind": "app",
            "stack": "unknown",
            "description": "",
            "topics": [],
            "web_url": format!("https://github.com/acme/{name}"),
            "forge": "github"
        })
    }

    fn clone_dir(&self, name: &str) -> PathBuf {
        self.root.join("workspaces/alpha").join(name)
    }

    /// A clone made the way an operator's would be, from the HTTPS URL.
    fn cloned(&self, name: &str) -> PathBuf {
        let dest = self.clone_dir(name);
        self.git(
            &self.root.join("workspaces/alpha"),
            &[
                "clone",
                "-q",
                &format!("https://github.com/acme/{name}.git"),
                &dest.display().to_string(),
            ],
        );
        dest
    }

    fn charter(&self, args: &[&str]) -> Output {
        self.charter_with(args, &[])
    }

    fn charter_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        Command::new(charter())
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("CHARTER_ROOT", &self.root)
            .env("HOME", &self.home)
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("USER", "tester")
            .envs(env.iter().copied())
            .output()
            .expect("the binary runs")
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ---------------------------------------------------------------------------------------
// clone                                                                                   #
// ---------------------------------------------------------------------------------------

#[test]
fn a_clone_checks_out_the_remotes_default_branch_whatever_the_inventory_assumed() {
    // `discover` writes `"main"` whenever a forge names no default branch, and a default can
    // move after `discover` ran. The remote's HEAD is the real answer.
    let w = World::new();
    w.remote("widget", "trunk");
    w.inventory(json!([World::record("widget", "main")]));

    let out = w.charter(&["clone", "widget", "-w", "alpha"]);

    assert!(out.status.success(), "{}", stderr(&out));
    let dest = w.clone_dir("widget");
    assert_eq!(w.git(&dest, &["symbolic-ref", "--short", "HEAD"]), "trunk");
    assert!(
        stderr(&out).contains("✓ widget → workspaces/alpha/widget (widget (trunk) via gh, HTTPS)"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_clone_is_recorded_as_a_member_of_its_workspace_without_a_branch() {
    let w = World::new();
    w.remote("widget", "trunk");
    w.inventory(json!([World::record("widget", "trunk")]));

    let out = w.charter(&["clone", "widget", "-w", "alpha", "--now", "2026-05-04T11:32:17"]);

    assert!(out.status.success(), "{}", stderr(&out));
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(w.root.join("workspaces/alpha/workspace.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["repos"], json!([{"name": "widget"}]));
    assert_eq!(manifest["updated_by"], json!("tester"));
    assert!(
        charter_core::manifest::ownership(Some(
            &std::fs::read_to_string(w.root.join("workspaces/alpha/workspace.json")).unwrap()
        )) == charter_core::manifest::Ownership::Charter,
        "the digest is charter's, so the next writer keeps maintaining the file"
    );
}

#[test]
fn a_name_that_is_a_path_an_option_or_hidden_is_refused_and_nothing_lands_outside() {
    let w = World::new();
    let records: Vec<Value> = ["..", "-rf", ".github", "a/b", "../../escaped"]
        .iter()
        .map(|n| World::record(n, "main"))
        .collect();
    w.inventory(Value::Array(records));
    let before: Vec<_> = std::fs::read_dir(w.root.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    let out = w.charter(&[
        "clone",
        "-w",
        "alpha",
        "--",
        "..",
        "-rf",
        ".github",
        "a/b",
        "../../escaped",
    ]);

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let said = stderr(&out);
    for quoted in ["'..'", "'-rf'", "'.github'", "'a/b'", "'../../escaped'"] {
        assert!(said.contains(&format!("✗ {quoted}: not cloned — ")), "{quoted}: {said}");
    }
    let after: Vec<_> = std::fs::read_dir(w.root.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(before, after, "nothing was created beside the plane");
    let inside: Vec<_> = std::fs::read_dir(w.root.join("workspaces/alpha"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(inside.is_empty(), "nothing was cloned: {inside:?}");
}

#[test]
fn a_clone_never_falls_back_to_ssh_when_git_config_rewrites_the_url() {
    // A global `url.<ssh>.insteadOf https://github.com/` is how many people force SSH, and
    // Python charter's clone then went over SSH without a word.
    let w = World::new();
    std::fs::write(
        w.home.join(".gitconfig"),
        "[url \"ssh://git@127.0.0.1:9/\"]\n\tinsteadOf = https://github.com/acme/\n",
    )
    .unwrap();
    w.inventory(json!([World::record("widget", "main")]));

    let out = w.charter(&["clone", "widget", "-w", "alpha"]);

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("transport 'ssh' not allowed"), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("charter clones over HTTPS with the forge CLI's token only"),
        "{}",
        stderr(&out)
    );
    assert!(!w.clone_dir("widget").exists());
}

#[test]
fn a_url_with_a_credential_in_it_is_refused_and_the_credential_is_never_printed() {
    let w = World::new();
    let mut record = World::record("widget", "main");
    record["web_url"] = json!("https://x-access-token:ghp_NEVERPRINTED@github.com/acme/widget");
    w.inventory(json!([record]));

    let out = w.charter(&["clone", "widget", "-w", "alpha"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(!stderr(&out).contains("ghp_NEVERPRINTED"), "{}", stderr(&out));
    assert!(stderr(&out).contains("✗ 'widget': not cloned — its URL carries a user or a credential"));
    assert!(!w.clone_dir("widget").exists());
}

#[test]
fn a_url_on_a_host_the_plane_does_not_manage_is_refused() {
    let w = World::new();
    let mut record = World::record("widget", "main");
    record["web_url"] = json!("https://evil.example/acme/widget");
    w.inventory(json!([record]));

    let out = w.charter(&["clone", "widget", "-w", "alpha"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("names host 'evil.example'"), "{}", stderr(&out));
}

#[test]
fn a_workspace_that_does_not_exist_is_not_invented() {
    let w = World::new();
    w.remote("widget", "main");
    w.inventory(json!([World::record("widget", "main")]));

    let out = w.charter(&["clone", "widget", "-w", "nope"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("no workspace 'nope'"), "{}", stderr(&out));
    assert!(!w.root.join("workspaces/nope").exists());
}

// ---------------------------------------------------------------------------------------
// sync                                                                                    #
// ---------------------------------------------------------------------------------------

fn head(w: &World, dir: &Path) -> String {
    w.git(dir, &["rev-parse", "HEAD"])
}

#[test]
fn a_clean_clone_is_fast_forwarded() {
    let w = World::new();
    let src = w.remote("widget", "trunk");
    let dest = w.cloned("widget");
    let tip = w.advance(&src, "widget", "NEW.md", "new\n");

    let out = w.charter(&["sync", "-w", "alpha"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(head(&w, &dest), tip);
    assert!(stderr(&out).contains("✓ alpha/widget: up to date on trunk"), "{}", stderr(&out));
}

#[test]
fn a_clone_with_uncommitted_work_is_skipped_and_the_work_is_untouched() {
    let w = World::new();
    let src = w.remote("widget", "main");
    let dest = w.cloned("widget");
    let was = head(&w, &dest);
    w.advance(&src, "widget", "README.md", "upstream\n");
    std::fs::write(dest.join("README.md"), "mine, uncommitted\n").unwrap();

    let out = w.charter(&["sync", "-w", "alpha"]);

    assert!(out.status.success());
    assert!(stderr(&out).contains("uncommitted changes — skipping"), "{}", stderr(&out));
    assert_eq!(head(&w, &dest), was);
    assert_eq!(
        std::fs::read_to_string(dest.join("README.md")).unwrap(),
        "mine, uncommitted\n"
    );
}

#[test]
fn a_diverged_clone_is_left_as_it_is() {
    let w = World::new();
    let src = w.remote("widget", "main");
    let dest = w.cloned("widget");
    w.advance(&src, "widget", "THEIRS.md", "theirs\n");
    std::fs::write(dest.join("MINE.md"), "mine\n").unwrap();
    w.git(&dest, &["add", "-A"]);
    w.git(&dest, &["commit", "-q", "-m", "mine"]);
    let mine = head(&w, &dest);

    let out = w.charter(&["sync", "-w", "alpha"]);

    assert!(stderr(&out).contains("main won't fast-forward"), "{}", stderr(&out));
    assert_eq!(head(&w, &dest), mine);
}

#[test]
fn a_detached_head_is_not_moved() {
    // Python read the branch as `HEAD` and fast-forwarded to `origin/HEAD`.
    let w = World::new();
    let src = w.remote("widget", "main");
    let dest = w.cloned("widget");
    w.git(&dest, &["checkout", "-q", "--detach"]);
    let was = head(&w, &dest);
    w.advance(&src, "widget", "NEW.md", "new\n");

    let out = w.charter(&["sync", "-w", "alpha"]);

    assert!(stderr(&out).contains("HEAD is detached"), "{}", stderr(&out));
    assert_eq!(head(&w, &dest), was);
}

#[test]
fn a_clone_in_the_middle_of_a_rebase_is_not_touched() {
    let w = World::new();
    let src = w.remote("widget", "main");
    w.advance(&src, "widget", "TWO.md", "two\n");
    let dest = w.cloned("widget");
    // A rebase that stops on a CLEAN tree: the exec fails after the one commit is replayed.
    let stopped = Command::new("git")
        .args(["rebase", "-q", "--force-rebase", "--exec", "false", "HEAD~1"])
        .current_dir(&dest)
        .env("HOME", &w.home)
        .envs(IDENTITY)
        .output()
        .unwrap();
    assert!(!stopped.status.success(), "the rebase stopped");
    assert!(dest.join(".git/rebase-merge").exists(), "a rebase is in progress");
    let was = head(&w, &dest);
    w.advance(&src, "widget", "THREE.md", "three\n");

    let out = w.charter(&["sync", "-w", "alpha"]);

    assert!(stderr(&out).contains("a rebase is in progress"), "{}", stderr(&out));
    assert_eq!(head(&w, &dest), was);
    assert!(dest.join(".git/rebase-merge").exists(), "and it is still in progress");
}

#[test]
fn an_ignored_file_the_upstream_starts_tracking_is_not_overwritten() {
    // git treats ignored files as expendable, so a fast-forward replaces them silently. An
    // ignored `config.local` is exactly the file an operator keeps work in.
    let w = World::new();
    let src = w.remote("widget", "main");
    let dest = w.cloned("widget");
    std::fs::write(dest.join(".git/info/exclude"), "config.local\n").unwrap();
    std::fs::write(dest.join("config.local"), "precious\n").unwrap();
    let was = head(&w, &dest);
    w.advance(&src, "widget", "config.local", "upstream\n");

    let out = w.charter(&["sync", "-w", "alpha"]);

    assert_eq!(
        std::fs::read_to_string(dest.join("config.local")).unwrap(),
        "precious\n",
        "{}",
        stderr(&out)
    );
    assert_eq!(head(&w, &dest), was);
    assert!(stderr(&out).contains("was not fast-forwarded"), "{}", stderr(&out));
}

#[test]
fn an_ssh_origin_on_a_host_charter_does_not_manage_is_not_fetched() {
    let w = World::new();
    w.remote("widget", "main");
    let dest = w.cloned("widget");
    w.git(&dest, &["remote", "set-url", "origin", "git@evil.example:acme/widget.git"]);

    let out = w.charter(&["sync", "-w", "alpha"]);

    assert!(
        stderr(&out).contains("origin is an SSH remote on a host this plane does not manage"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_workspace_with_no_clones_says_so() {
    let w = World::new();

    let out = w.charter(&["sync", "-w", "alpha"]);

    assert!(out.status.success());
    assert_eq!(
        stderr(&out),
        "• workspace: alpha  (via --workspace)\n! No cloned repos to sync in workspace 'alpha'.\n"
    );
}

// ---------------------------------------------------------------------------------------
// discover                                                                                #
// ---------------------------------------------------------------------------------------

const TOKEN: &str = "ghs_NEVER_ON_ARGV_1234";

/// A stand-in `gh` that answers recorded responses and writes down its argv and whether
/// the token reached its environment.
fn stub_gh(w: &World, authed: bool) {
    let repos = json!([
        {"id": 11, "name": "widget", "full_name": "acme/widget", "default_branch": "trunk",
         "description": "The widget — made well", "html_url": "https://github.com/acme/widget",
         "ssh_url": "git@github.com:acme/widget.git", "topics": ["core"]},
        {"id": 12, "name": "../evil", "full_name": "acme/evil", "default_branch": "main",
         "description": null, "html_url": "https://github.com/acme/evil",
         "ssh_url": "git@github.com:acme/evil.git", "topics": []},
        {"id": 13, "name": "legacy", "full_name": "acme/legacy", "default_branch": "master",
         "description": "old", "html_url": "https://github.com/acme/legacy",
         "ssh_url": "git@github.com:acme/legacy.git", "topics": []}
    ]);
    std::fs::write(w.bin.join("repos.json"), repos.to_string()).unwrap();
    std::fs::write(
        w.bin.join("tree.json"),
        json!({"tree": [{"path": "Cargo.toml"}, {"path": "src/main.rs"}]}).to_string(),
    )
    .unwrap();
    let log = w.bin.join("calls.log");
    let auth = if authed { "exit 0" } else { "exit 1" };
    let script = format!(
        "#!/bin/sh\n\
         printf 'argv:%s\\n' \"$*\" >> '{log}'\n\
         if [ -n \"$GH_TOKEN\" ]; then echo 'token:present' >> '{log}'; fi\n\
         case \"$*\" in\n\
         \"auth status --hostname github.com\") {auth};;\n\
         \"api --hostname github.com orgs/acme/repos?per_page=100&page=1\") cat '{bin}/repos.json'; exit 0;;\n\
         \"api --hostname github.com repos/acme/widget/git/trees/trunk\") cat '{bin}/tree.json'; exit 0;;\n\
         esac\n\
         echo \"gh: Server Error (HTTP 502)\" >&2\n\
         exit 1\n",
        log = log.display(),
        bin = w.bin.display(),
    );
    let gh = w.bin.join("gh");
    std::fs::write(&gh, script).unwrap();
    std::fs::set_permissions(
        &gh,
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
    )
    .unwrap();
}

#[test]
fn discover_writes_the_inventory_and_the_topology_from_what_the_forge_answered() {
    let w = World::new();
    stub_gh(&w, true);

    let out = w.charter_with(&["discover"], &[("GH_TOKEN", TOKEN)]);

    assert!(out.status.success(), "{}", stderr(&out));
    let doc: Value = serde_json::from_str(
        &std::fs::read_to_string(w.root.join("inventory/repos.json")).unwrap(),
    )
    .unwrap();
    let names: Vec<&str> = doc["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["legacy", "widget"], "a name that is a path never enters it");
    assert_eq!(doc["repos"][1]["stack"], json!("rust"));
    assert_eq!(doc["repos"][0]["stack"], json!("unknown"), "its probe failed");
    // `legacy`'s probe and `../evil`'s both failed: a probe is counted before the name that
    // cannot be one is dropped, as Python counts it.
    assert!(stderr(&out).contains("stack probe FAILED for 2 repo(s)"), "{}", stderr(&out));
    assert!(w.root.join("docs/topology.md").is_file());
}

#[test]
fn the_token_reaches_the_forge_cli_through_its_environment_and_never_its_argv() {
    let w = World::new();
    stub_gh(&w, true);

    let out = w.charter_with(&["discover", "--no-docs"], &[("GH_TOKEN", TOKEN)]);

    assert!(out.status.success(), "{}", stderr(&out));
    let log = std::fs::read_to_string(w.bin.join("calls.log")).unwrap();
    assert!(log.contains("token:present"), "the CLI got its credential: {log}");
    assert!(!log.contains(TOKEN), "no call carried it on the command line: {log}");
    assert!(!stderr(&out).contains(TOKEN));
}

#[test]
fn a_forge_cli_that_is_not_logged_in_refuses_discover_and_writes_nothing() {
    let w = World::new();
    stub_gh(&w, false);

    let out = w.charter(&["discover"]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr(&out),
        "• Querying github org `acme` …\ngh is not authenticated for github.com. Run: gh auth login\n"
    );
    assert!(!w.root.join("inventory/repos.json").exists());
}
