//! `charter init` and `charter reinit` write into a directory an operator points them at —
//! possibly a repository with content they care about, possibly one somebody else committed.
//!
//! The contract is **additive and idempotent, and never touches existing content**, and each
//! clause is pinned here against the binary itself, comparing whole trees rather than output:
//!
//! - a pre-existing file at every path `init` writes stays byte for byte;
//! - a symlink at every one of those paths that leads out of the plane is written through
//!   by nothing, and nothing outside changes — whether the link resolves or dangles;
//! - a link that stays inside the plane is followed, as the Python charter follows it;
//! - running it twice leaves the tree the first run left.
//!
//! The recorded scenarios (`init-*`, ADR 0046) compare the ordinary cases against the Python
//! charter's answers. These are the containment half, which is this binary's own contract.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn charter() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_charter"))
}

/// One directory to point `init` at, a directory beside it standing for everything outside
/// the plane, and a home of its own so nothing reads the real `~/.claude`.
struct Scene {
    _tmp: tempfile::TempDir,
    plane: PathBuf,
    outside: PathBuf,
    home: PathBuf,
}

impl Scene {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let plane = base.join("plane");
        let outside = base.join("outside");
        let home = base.join("home");
        for dir in [&plane, &outside, &home, &outside.join("dir")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(outside.join("precious"), "PRECIOUS OPERATOR DATA\n").unwrap();
        std::fs::write(outside.join("dir/keep"), "kept\n").unwrap();
        Scene {
            _tmp: tmp,
            plane,
            outside,
            home,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(charter())
            .args(args)
            .current_dir(&self.plane)
            .env_remove("CHARTER_ROOT")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("XDG_CONFIG_HOME")
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("the binary runs")
    }

    fn init(&self) -> Output {
        self.run(&["init", "--forge", "github", "--owner", "acme"])
    }

    /// Make `dir` the top level of a git working tree with an origin on a forge charter
    /// knows. The test's own git, never charter's runner.
    fn git_repo(&self, dir: &Path) {
        for args in [
            vec!["init", "-q", "-b", "main", "."],
            vec![
                "remote",
                "add",
                "origin",
                "https://github.com/acme/widget.git",
            ],
        ] {
            let out = Command::new("git")
                .args(&args)
                .current_dir(dir)
                .env("HOME", &self.home)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .expect("git runs");
            assert!(out.status.success(), "git {args:?}: {:?}", out);
        }
    }

    /// One value out of a clone's LOCAL config — what `gitpolicy` writes and what the first
    /// clone's `origin` was repointed to. The test's own git, with the machine's global and
    /// system config shut out so the answer is the repo's own.
    fn git_config(&self, repo: &Path, key: &str) -> String {
        let out = Command::new("git")
            .args(["config", "--local", "--get", key])
            .current_dir(repo)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("git runs");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn write(&self, rel: &str, body: &str) {
        let path = self.plane.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn link(&self, rel: &str, target: &Path) {
        let path = self.plane.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, path).unwrap();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Node {
    Dir,
    File(Vec<u8>),
    Link(PathBuf),
}

/// Every entry under `root` by path, WITHOUT following a link: a link is recorded as the
/// link, so one that was written through or replaced shows up as a change.
fn tree(root: &Path) -> BTreeMap<PathBuf, Node> {
    let mut out = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                out.insert(rel, Node::Link(std::fs::read_link(&path).unwrap()));
            } else if meta.is_dir() {
                out.insert(rel, Node::Dir);
                todo.push(path);
            } else {
                out.insert(rel, Node::File(std::fs::read(&path).unwrap()));
            }
        }
    }
    out
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/planes")
        .join(name)
}

#[test]
fn init_in_an_empty_directory_leaves_exactly_the_plane_the_python_charter_leaves() {
    // `minimal` IS the Python charter's `init --forge github --owner acme`, byte for byte,
    // plus the empty directories git cannot carry, recorded beside it.
    let scene = Scene::new();
    let out = scene.init();
    assert!(out.status.success(), "{}", stderr(&out));

    let mut want = tree(&fixture("minimal"));
    for dir in std::fs::read_to_string(fixture("minimal.empty-dirs"))
        .unwrap()
        .split_whitespace()
    {
        want.insert(PathBuf::from(dir), Node::Dir);
    }
    // And the one file charter-app writes that the Python charter did not: the merge rules
    // (ADR 0051, docs/plane-format.md `.gitattributes`), which make two machines' appends to
    // the plane's logs and memory indexes merge instead of conflict.
    want.insert(
        PathBuf::from(".gitattributes"),
        Node::File(MERGE_RULES.as_bytes().to_vec()),
    );
    // And the file the baseline `.gitignore` un-ignores, which the Python charter never
    // created (#355): without it an empty `workspaces/` cannot be committed.
    want.insert(PathBuf::from("workspaces/.gitkeep"), Node::File(Vec::new()));
    // And the front door without the Python's `routing: advise`: the key is retired (#369).
    // The fixture keeps it, as a plane the Python made still does.
    let front_door = PathBuf::from("personas/steward/persona.md");
    let Some(Node::File(python)) = want.get(&front_door) else {
        panic!("the fixture has a front door");
    };
    let text = String::from_utf8(python.clone()).expect("UTF-8");
    assert!(text.contains("\nrouting: advise\n"), "{text}");
    want.insert(
        front_door,
        Node::File(text.replacen("\nrouting: advise\n", "\n", 1).into_bytes()),
    );
    // And the ask rule for filing a report, which the Python charter never had (#363, ADR
    // 0059 amended 2026-09-26): appended after the handoff rule in both harnesses' files.
    for (rel, from, to) in [
        (
            ".claude/settings.json",
            r#""Bash(charter handoff *)"]"#,
            r#""Bash(charter handoff *)","Bash(charter report *--yes*)"]"#,
        ),
        (
            "opencode.json",
            "\"charter handoff *\": \"ask\"\n",
            "\"charter handoff *\": \"ask\",\n      \"charter report *--yes*\": \"ask\"\n",
        ),
    ] {
        let Some(Node::File(python)) = want.get(Path::new(rel)) else {
            panic!("the fixture has {rel}");
        };
        let text = String::from_utf8(python.clone()).expect("UTF-8");
        assert!(text.contains(from), "{text}");
        want.insert(
            PathBuf::from(rel),
            Node::File(text.replacen(from, to, 1).into_bytes()),
        );
    }
    assert_eq!(tree(&scene.plane), want);
}

/// The `.gitattributes` block, exactly as `docs/plane-format.md` records it.
const MERGE_RULES: &str = "# >>> charter merge rules (managed by charter) >>>
personas/_dispatch/*.jsonl merge=union
personas/_skills/*.jsonl merge=union
workspaces/*/pieces/*.jsonl merge=union
workspaces/*/changes/log/*.jsonl merge=union
personas/*/memory/MEMORY.md merge=union
workspaces/*/memory/MEMORY.md merge=union
# <<< charter merge rules <<<
";

#[test]
fn running_init_twice_leaves_the_tree_the_first_run_left() {
    let scene = Scene::new();
    assert!(scene.init().status.success());
    let first = tree(&scene.plane);

    let again = scene.init();

    assert!(again.status.success(), "{}", stderr(&again));
    assert!(
        stderr(&again).contains("nothing to do"),
        "{}",
        stderr(&again)
    );
    assert_eq!(tree(&scene.plane), first);
}

#[test]
fn a_file_already_at_every_path_init_writes_is_left_byte_for_byte() {
    // Everything `init` would write, already there in the operator's own words and layout:
    // it has nothing to add, so it must change nothing at all.
    let scene = Scene::new();
    scene.write(
        "charter.toml",
        "# ours\nschema = 1\n\n[[forge]]\nkind = \"gitlab\"\n\n[persona]\ndefault = \"ops\"\n",
    );
    scene.write(
        ".gitignore",
        "dist/\n/workspaces/*/*\n!/workspaces/.gitkeep\n/.charter/\n\
         /.claude/settings.local.json\n/charter.local.toml\n",
    );
    scene.write(
        ".claude/settings.json",
        "{\n    \"env\": {\"CHARTER_HARNESS\": \"claude-code\"},\n    \"permissions\": {\"ask\": \
         [\"Bash(charter handoff *)\", \"Bash(charter report *--yes*)\"]},\n    \"hooks\": {\"PreToolUse\": [{\"matcher\": \"Bash\", \
         \"hooks\": [{\"type\": \"command\", \"command\": \"charter hook pretooluse\"}]}]}\n}",
    );
    scene.write(
        "opencode.json",
        "{\"permission\": {\"bash\": {\"charter handoff *\": \"ask\", \"charter report *--yes*\": \
         \"ask\"}}}",
    );
    scene.write("personas/ops/persona.md", "---\nname: ops\n---\n");
    scene.write("inventory/repos.json", "{}");
    scene.write("workspaces/alpha/workspace.md", "# alpha\n");
    scene.write("workspaces/.gitkeep", "");
    scene.write(".gitattributes", &format!("*.png binary\n{MERGE_RULES}"));
    let before = tree(&scene.plane);

    let out = scene.init();

    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("nothing to do"), "{}", stderr(&out));
    assert_eq!(tree(&scene.plane), before);
}

#[test]
fn what_init_adds_to_a_file_of_the_operators_keeps_every_byte_they_wrote() {
    let scene = Scene::new();
    let toml = "# ours\r\nschema = 1\r\n";
    let ignore = "dist/\r\n# build output\r\n*.log";
    scene.write("charter.toml", toml);
    scene.write(".gitignore", ignore);
    scene.write(
        ".claude/settings.json",
        "{\n    \"model\": \"opus\",\n    \"permissions\": {\"allow\": [\"Bash(ls *)\"]}\n}\n",
    );

    let out = scene.init();

    assert!(out.status.success(), "{}", stderr(&out));
    let toml_now = std::fs::read_to_string(scene.plane.join("charter.toml")).unwrap();
    assert!(toml_now.starts_with(toml), "{toml_now:?}");
    let ignore_now = std::fs::read_to_string(scene.plane.join(".gitignore")).unwrap();
    assert!(ignore_now.starts_with(ignore), "{ignore_now:?}");
    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(scene.plane.join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["permissions"]["allow"][0], "Bash(ls *)");
}

/// Every path `init` writes, and what a link planted there points at: a file for a file, a
/// directory for a directory.
const PATHS: [(&str, bool); 8] = [
    ("charter.toml", false),
    (".gitignore", false),
    (".claude/settings.json", false),
    ("opencode.json", false),
    (".claude", true),
    ("personas", true),
    ("inventory", true),
    ("workspaces", true),
];

#[test]
fn a_link_out_of_the_plane_at_any_path_init_writes_is_written_through_by_nothing() {
    for (rel, is_dir) in PATHS {
        for dangling in [false, true] {
            let scene = Scene::new();
            let target = match (is_dir, dangling) {
                (false, false) => scene.outside.join("precious"),
                (false, true) => scene.outside.join("planted"),
                (true, false) => scene.outside.join("dir"),
                (true, true) => scene.outside.join("nodir"),
            };
            scene.link(rel, &target);
            let outside = tree(&scene.outside);

            let out = scene.init();

            let what = format!("{rel} -> {} (dangling: {dangling})", target.display());
            assert_eq!(out.status.code(), Some(1), "{what}: {}", stderr(&out));
            assert!(
                stderr(&out).contains("which is outside this plane"),
                "{what}: {}",
                stderr(&out)
            );
            assert_eq!(
                tree(&scene.outside),
                outside,
                "{what}: something outside changed"
            );
            assert_eq!(
                std::fs::read_link(scene.plane.join(rel)).ok(),
                Some(target.clone()),
                "{what}: the link itself was replaced"
            );
            // Additive: what the link does not block is still created.
            if rel != "workspaces" {
                assert!(scene.plane.join("workspaces").is_dir(), "{what}");
            }
            if rel != "charter.toml" {
                assert!(scene.plane.join("charter.toml").is_file(), "{what}");
            }
        }
    }
}

#[test]
fn reinit_writes_through_no_link_out_of_the_plane_either() {
    for rel in [
        "personas",
        "inventory",
        "workspaces",
        ".gitignore",
        ".claude",
    ] {
        let scene = Scene::new();
        assert!(scene.init().status.success());
        let path = scene.plane.join(rel);
        if path.is_dir() {
            std::fs::remove_dir_all(&path).unwrap();
        } else {
            std::fs::remove_file(&path).unwrap();
        }
        let target = scene
            .outside
            .join(if rel.starts_with('.') && rel != ".claude" {
                "precious"
            } else {
                "nodir"
            });
        scene.link(rel, &target);
        let outside = tree(&scene.outside);

        let out = scene.run(&["reinit"]);

        assert_eq!(out.status.code(), Some(1), "{rel}: {}", stderr(&out));
        assert_eq!(
            tree(&scene.outside),
            outside,
            "{rel}: something outside changed"
        );
    }
}

#[test]
fn a_link_that_stays_inside_the_plane_is_followed() {
    let scene = Scene::new();
    scene.write("team/gitignore", "node_modules/\n");
    scene.link(".gitignore", Path::new("team/gitignore"));
    std::fs::create_dir_all(scene.plane.join("team/roster")).unwrap();
    scene.link("personas", Path::new("team/roster"));

    let out = scene.init();

    assert!(out.status.success(), "{}", stderr(&out));
    let ignore = std::fs::read_to_string(scene.plane.join("team/gitignore")).unwrap();
    assert!(ignore.starts_with("node_modules/\n"), "{ignore:?}");
    assert!(ignore.contains("!/workspaces/.gitkeep"), "{ignore:?}");
    assert!(scene.plane.join("team/roster/steward/persona.md").is_file());
    assert!(
        std::fs::symlink_metadata(scene.plane.join(".gitignore"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn a_link_into_the_planes_own_git_directory_is_not_followed() {
    let scene = Scene::new();
    let exclude = "# git ls-files --others --exclude-from=.git/info/exclude\n";
    scene.write(".git/info/exclude", exclude);
    scene.link(".gitignore", Path::new(".git/info/exclude"));

    let out = scene.init();

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(scene.plane.join(".git/info/exclude")).unwrap(),
        exclude
    );
}

#[test]
fn a_file_where_a_directory_goes_is_named_and_left_alone_and_the_rest_is_created() {
    let scene = Scene::new();
    scene.write("personas", "not a directory\n");

    let out = scene.init();

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("personas/ can't be created"),
        "{}",
        stderr(&out)
    );
    assert_eq!(
        std::fs::read_to_string(scene.plane.join("personas")).unwrap(),
        "not a directory\n"
    );
    assert!(scene.plane.join("inventory").is_dir());
    assert!(scene.plane.join(".claude/settings.json").is_file());
}

#[test]
fn a_link_inside_the_plane_that_leads_nowhere_is_a_blocker_not_a_crash() {
    let scene = Scene::new();
    scene.link("inventory", Path::new("elsewhere/inventory"));

    let out = scene.init();

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("inventory/ can't be created"),
        "{}",
        stderr(&out)
    );
    assert!(!scene.plane.join("elsewhere").exists());
}

#[test]
fn a_settings_file_claude_code_cannot_read_is_left_byte_for_byte() {
    let scene = Scene::new();
    let body = "{\"cleanupPeriodDays\": NaN}\n";
    scene.write(".claude/settings.json", body);

    let out = scene.init();

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("left it completely untouched"),
        "{}",
        stderr(&out)
    );
    assert_eq!(
        std::fs::read_to_string(scene.plane.join(".claude/settings.json")).unwrap(),
        body
    );
    // All or nothing: the handoff rule is not left in force under opencode alone.
    assert!(!scene.plane.join("opencode.json").exists());
}

#[test]
fn a_plane_from_a_newer_charter_is_refused_before_anything_is_written() {
    let scene = Scene::new();
    scene.write("charter.toml", "schema = 2\n");
    let before = tree(&scene.plane);

    for command in [&["init"][..], &["reinit"][..]] {
        let out = scene.run(command);
        assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
        assert!(
            stderr(&out).contains("declares schema 2"),
            "{}",
            stderr(&out)
        );
        assert_eq!(tree(&scene.plane), before);
    }
}

#[test]
fn no_front_door_scaffolds_no_persona_and_declares_none() {
    let scene = Scene::new();

    let out = scene.run(&["init", "--no-front-door"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!scene.plane.join("personas/steward").exists());
    let toml = std::fs::read_to_string(scene.plane.join("charter.toml")).unwrap();
    assert!(!toml.contains("[persona]"), "{toml}");
}

#[test]
fn reinit_outside_a_plane_scaffolds_nothing() {
    let scene = Scene::new();

    let out = scene.run(&["reinit"]);

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("no control plane found"),
        "{}",
        stderr(&out)
    );
    assert!(tree(&scene.plane).is_empty());
}

#[test]
fn reinit_heals_a_missing_directory_and_then_has_nothing_to_do() {
    let scene = Scene::new();
    assert!(scene.init().status.success());
    let whole = tree(&scene.plane);
    std::fs::remove_dir(scene.plane.join("inventory")).unwrap();

    let healed = scene.run(&["reinit"]);
    assert!(healed.status.success(), "{}", stderr(&healed));
    assert!(
        stderr(&healed).contains("added inventory/"),
        "{}",
        stderr(&healed)
    );
    assert_eq!(tree(&scene.plane), whole);

    let again = scene.run(&["reinit"]);
    assert!(again.status.success(), "{}", stderr(&again));
    assert!(stderr(&again).contains("Up to date"), "{}", stderr(&again));
    assert_eq!(tree(&scene.plane), whole);
}

// ------------------------------------------------------------------------------------------
// ADR 0035 / spec decision 27: `init` at the top of a repository does not colonise it
// ------------------------------------------------------------------------------------------
//
// The DECLARED divergence from the Python charter, which still scaffolds into the repository
// and prints the first-clone offer afterwards. The recorded scenario holds that difference by
// name (a `divergence` note, ADR 0046); what is pinned here is the Rust side's own contract:
// exactly which runs refuse, and that a refusing run writes nothing at all.

#[test]
fn init_at_the_top_of_a_git_repo_writes_nothing_into_it() {
    let scene = Scene::new();
    scene.git_repo(&scene.plane);
    let before = tree(&scene.plane);

    let out = scene.init();

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(
        tree(&scene.plane),
        before,
        "a refusing `init` wrote into the repository"
    );
    let said = stderr(&out);
    // Named from its origin, not from the directory the plane copy happens to sit in.
    assert!(said.contains("this is the git repo 'widget'"), "{said}");
    // The opt-in is on the screen, and it says what it will do.
    assert!(said.contains("charter init --plane-is-this-repo"), "{said}");
    assert!(
        said.contains(
            "That writes charter.toml, .claude/settings.json, opencode.json, personas/, \
             inventory/, workspaces/ into this repo, and charter's own rules into its \
             tracked .gitignore."
        ),
        "{said}"
    );
    // The flags the operator typed come back with both suggestions, ready to paste.
    assert!(
        said.contains("charter init --forge github --owner acme"),
        "{said}"
    );
    assert!(
        said.contains("charter init --plane-is-this-repo --forge github --owner acme"),
        "{said}"
    );
    // And where the next reader finds out why.
    assert!(
        said.contains("docs/adr/0035-a-plane-is-untrusted-until-the-operator-opens-it.md"),
        "{said}"
    );
    assert!(said.contains("spec decision 27"), "{said}");
}

#[test]
fn the_flag_that_asks_for_it_makes_the_repo_the_plane_and_offers_the_first_clone() {
    let scene = Scene::new();
    scene.git_repo(&scene.plane);

    let out = scene.run(&[
        "init",
        "--plane-is-this-repo",
        "--forge",
        "github",
        "--owner",
        "acme",
    ]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert!(scene.plane.join("charter.toml").is_file());
    // Python's offer, unchanged: what the old default printed after scaffolding.
    assert!(
        stderr(&out).contains("You are standing in the git repo 'widget'"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn clone_this_repo_makes_the_plane_here_and_clones_the_repo_into_the_first_workspace() {
    // It asks for a clone INTO the plane this run makes here, so it is not turned back by
    // the repo check — and since `gitpolicy` was ported (M2.5, #64) there is nothing left
    // for it to refuse over either (charter-app#175). Python's `_clone_first_workspace`,
    // and the differential holds the two byte for byte.
    let scene = Scene::new();
    scene.git_repo(&scene.plane);

    let out = scene.run(&["init", "--clone-this-repo", "--forge", "github"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert!(scene.plane.join("charter.toml").is_file());
    let clone = scene.plane.join("workspaces/default/widget");
    assert!(clone.join(".git").exists(), "{}", stderr(&out));
    // Named from the origin, and the origin is repointed at the source's own upstream: left
    // at the plane root it would look right and fail at the first push.
    assert_eq!(
        scene.git_config(&clone, "remote.origin.url"),
        "https://github.com/acme/widget.git"
    );
    // Golden rule 0, applied to the first clone as it is to every later one.
    assert_eq!(
        scene.git_config(&clone, "credential.helper"),
        "!gh auth git-credential"
    );
    assert!(
        stderr(&out).contains("widget → workspaces/default/widget"),
        "{}",
        stderr(&out)
    );
}

// ------------------------------------------------------------------------------------------
// `--adopt`: ADR 0035's default, as something charter DOES (charter-app#175)
// ------------------------------------------------------------------------------------------
//
// The record's sentence is "`charter init` on an existing repo adopts that repo as the plane's
// first clone and makes the plane beside it". Where "beside" IS is typed, never guessed —
// `init` writes nothing outside the directory it was pointed at, and picking `../x-plane` for
// the operator would be exactly that write. So the plane is this directory and `--adopt` names
// the repo.

#[test]
fn adopt_makes_the_plane_here_and_the_repo_its_first_clone_and_writes_nothing_into_the_repo() {
    let scene = Scene::new();
    let repo = scene.outside.join("widget");
    std::fs::create_dir_all(&repo).unwrap();
    scene.git_repo(&repo);
    let untouched = tree(&repo);

    let out = scene.run(&[
        "init",
        "--forge",
        "github",
        "--owner",
        "acme",
        "--adopt",
        repo.to_str().unwrap(),
    ]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert!(scene.plane.join("charter.toml").is_file());
    let clone = scene.plane.join("workspaces/default/widget");
    assert!(clone.join(".git").exists(), "{}", stderr(&out));
    assert_eq!(
        scene.git_config(&clone, "remote.origin.url"),
        "https://github.com/acme/widget.git"
    );
    assert_eq!(
        scene.git_config(&clone, "credential.helper"),
        "!gh auth git-credential"
    );
    // The whole subject of ADR 0035: the repository the operator pointed at is read, and
    // only read. Not one byte of it moved, `.git` included.
    assert_eq!(tree(&repo), untouched, "the adopted repo was written into");
}

#[test]
fn adopt_refuses_a_directory_that_is_not_the_top_of_a_repository() {
    let scene = Scene::new();

    let out = scene.run(&[
        "init",
        "--forge",
        "github",
        "--adopt",
        scene.outside.join("dir").to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("is not the top level of a git working tree"),
        "{}",
        stderr(&out)
    );
    // The plane itself is still created, as Python creates it before cloning.
    assert!(scene.plane.join("charter.toml").is_file());
    assert!(!scene.plane.join("workspaces/default").exists());
}

#[test]
fn adopt_refuses_the_planes_own_directory_and_names_the_flag_that_means_that() {
    let scene = Scene::new();
    scene.git_repo(&scene.plane);

    let out = scene.run(&[
        "init",
        "--plane-is-this-repo",
        "--forge",
        "github",
        "--adopt",
        scene.plane.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("ask for `--clone-this-repo`"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn adopt_refuses_a_repository_this_plane_is_inside() {
    // The clone would land inside the repository it came from — a write into somebody's
    // repo arriving through the flag that is supposed to be the safe half of ADR 0035.
    let scene = Scene::new();
    let repo = scene.outside.join("widget");
    let under = repo.join("planes").join("acme");
    std::fs::create_dir_all(&under).unwrap();
    scene.git_repo(&repo);

    let out = Command::new(charter())
        .args([
            "init",
            "--forge",
            "github",
            "--adopt",
            repo.to_str().unwrap(),
        ])
        .current_dir(&under)
        .env_remove("CHARTER_ROOT")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", &scene.home)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("the binary runs");

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("would write the clone into the repository it came from"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn adopt_and_clone_this_repo_together_are_refused_before_anything_is_written() {
    // Two flags naming two different sources for one first clone. Ranking them silently is
    // how charter would clone one repo while the operator read the other one's name back.
    let scene = Scene::new();
    let repo = scene.outside.join("widget");
    std::fs::create_dir_all(&repo).unwrap();
    scene.git_repo(&repo);
    let before = tree(&scene.plane);

    let out = scene.run(&[
        "init",
        "--forge",
        "github",
        "--clone-this-repo",
        "--adopt",
        repo.to_str().unwrap(),
    ]);

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    // clap says it at the door; `scaffold::init` says it again for the app, which has no
    // argument parser between the dialog and the core (`opener::create_project`).
    assert!(
        stderr(&out).contains("cannot be used with '--adopt"),
        "{}",
        stderr(&out)
    );
    assert_eq!(tree(&scene.plane), before, "a refused run wrote something");
}

#[test]
fn the_refusal_in_a_repository_names_the_command_that_adopts_it() {
    // Before charter-app#175 this printed three commands ending in `charter clone <name>`,
    // which is the printed-command shape with a window around it. It is now two, and the
    // second one actually adopts.
    let scene = Scene::new();
    scene.git_repo(&scene.plane);

    let out = scene.init();

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let said = stderr(&out);
    assert!(
        said.contains("charter init --forge github --owner acme --adopt ../plane"),
        "{said}"
    );
    assert!(
        said.contains("That clones this repo into the plane's first workspace."),
        "{said}"
    );
    assert!(
        !said.contains("charter discover && charter clone"),
        "{said}"
    );
}

#[test]
fn a_repo_that_is_already_a_plane_is_healed_rather_than_refused() {
    // Every re-run of `init`, and charter's own plane, which IS a repository. A refusal here
    // would break the one flow this repo develops itself in.
    let scene = Scene::new();
    assert!(scene.init().status.success());
    scene.git_repo(&scene.plane);
    let whole = tree(&scene.plane);

    let again = scene.init();

    assert!(again.status.success(), "{}", stderr(&again));
    assert!(
        stderr(&again).contains("nothing to do"),
        "{}",
        stderr(&again)
    );
    assert_eq!(tree(&scene.plane), whole);
}

#[test]
fn a_directory_that_merely_sits_inside_a_repo_still_gets_its_plane() {
    // `_is_repo_top_level`'s equality case, and the reason for it: a `$HOME` kept under git
    // would otherwise refuse to hold a plane anywhere beneath it.
    let scene = Scene::new();
    scene.git_repo(&scene.plane);
    let under = scene.plane.join("planes").join("acme");
    std::fs::create_dir_all(&under).unwrap();

    let out = Command::new(charter())
        .args(["init", "--forge", "github", "--owner", "acme"])
        .current_dir(&under)
        .env_remove("CHARTER_ROOT")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", &scene.home)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("the binary runs");

    assert!(out.status.success(), "{}", stderr(&out));
    assert!(under.join("charter.toml").is_file());
}

#[test]
fn clone_this_repo_outside_a_repository_is_refused_and_the_plane_still_made() {
    let scene = Scene::new();

    let out = scene.run(&["init", "--clone-this-repo"]);

    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("there is no repo here to clone"),
        "{}",
        stderr(&out)
    );
    assert!(scene.plane.join("charter.toml").is_file());
}
