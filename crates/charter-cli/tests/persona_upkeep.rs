//! `charter persona forget | dedupe | optimize | log` through the binary (#366), on a copy of
//! the committed `daily` fixture plane. The Python oracle recorded none of these, so this is
//! where the command surface is held.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn daily() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().unwrap();
    copy(&fixture, &dir.path().join("plane"));
    std::fs::create_dir_all(dir.path().join("home")).unwrap();
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

fn root(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("plane")
}

fn charter(tmp: &tempfile::TempDir, args: &[&str]) -> Output {
    let root = root(tmp);
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(args)
        .current_dir(&root)
        .env("CHARTER_ROOT", &root)
        .env("HOME", tmp.path().join("home"))
        .env("CHARTER_SESSION_ID", "fixture-session-1")
        .env("NO_COLOR", "1");
    for name in [
        "CLAUDE_CODE_SESSION_ID",
        "CHARTER_WORKSPACE",
        "CHARTER_PERSONA",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
        "CLAUDE_CONFIG_DIR",
    ] {
        command.env_remove(name);
    }
    command.output().expect("the binary runs")
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn out(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const PROD: &str = "personas/devops/memory/cluster-prod-1-lives-in-eu-west-1.md";

fn tree(tmp: &tempfile::TempDir) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let rel = path.strip_prefix(base).unwrap().display().to_string();
                out.push((rel, std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root(tmp).join("personas"), &root(tmp), &mut out);
    out
}

#[test]
fn forget_removes_exactly_one_file_and_refuses_a_multi_segment_slug() {
    let tmp = daily();
    let before = tree(&tmp);
    for slug in ["../steward/persona", "../../charter.toml"] {
        let refused = charter(&tmp, &["persona", "forget", "devops", slug]);
        assert_eq!(refused.status.code(), Some(1), "{slug}");
        assert!(
            err(&refused).contains("is not the slug of one memory"),
            "{}",
            err(&refused)
        );
    }
    assert_eq!(tree(&tmp), before, "a refused forget touches nothing");
    let gone = charter(
        &tmp,
        &[
            "persona",
            "forget",
            "devops",
            "cluster-prod-1-lives-in-eu-west-1",
        ],
    );
    assert!(gone.status.success(), "{}", err(&gone));
    let after = tree(&tmp);
    let lost: Vec<&String> = before
        .iter()
        .filter(|(p, _)| !after.iter().any(|(q, _)| q == p))
        .map(|(p, _)| p)
        .collect();
    assert_eq!(lost, [PROD]);
}

#[test]
fn dedupe_lists_a_near_duplicate_pair_above_the_threshold() {
    let tmp = daily();
    std::fs::write(
        root(&tmp).join("personas/devops/memory/prod-1-region.md"),
        "# Prod-1 region\n\nCluster prod-1 lives in eu-west-1 region\n",
    )
    .unwrap();
    let found = charter(&tmp, &["persona", "dedupe", "devops"]);
    assert!(found.status.success(), "{}", err(&found));
    assert!(
        out(&found).starts_with("Near-duplicate memory pairs for 'devops' (Jaccard ≥ 0.5):"),
        "{}",
        out(&found)
    );
    assert!(out(&found).contains(PROD));
    let none = charter(
        &tmp,
        &["persona", "dedupe", "devops", "--threshold", "0.99"],
    );
    assert_eq!(
        err(&none),
        "✓ no near-duplicate memories for 'devops' (threshold 0.99).\n"
    );
}

#[test]
fn optimize_without_apply_changes_nothing() {
    let tmp = daily();
    let copy = std::fs::read(root(&tmp).join(PROD)).unwrap();
    std::fs::write(root(&tmp).join("personas/devops/memory/copy.md"), copy).unwrap();
    let before = tree(&tmp);
    let read_only = charter(&tmp, &["persona", "optimize"]);
    assert!(read_only.status.success(), "{}", err(&read_only));
    assert!(out(&read_only).contains("would auto-apply (re-run with --apply):"));
    assert_eq!(tree(&tmp), before);
    let applied = charter(&tmp, &["persona", "optimize", "devops", "--apply"]);
    assert!(applied.status.success(), "{}", err(&applied));
    assert_ne!(tree(&tmp), before);
}

#[test]
fn log_notes_and_shows_this_sessions_activity() {
    let tmp = daily();
    let noted = charter(
        &tmp,
        &["persona", "log", "devops", "rotated the kubeconfig"],
    );
    assert!(noted.status.success(), "{}", err(&noted));
    let shown = charter(&tmp, &["persona", "log", "devops"]);
    assert!(shown.status.success(), "{}", err(&shown));
    assert!(
        out(&shown).ends_with("  note       msg=rotated the kubeconfig\n"),
        "{}",
        out(&shown)
    );
}
