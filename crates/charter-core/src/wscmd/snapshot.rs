//! `charter workspace snapshot` — capture a workspace's repos and branches into its
//! committed manifest.
//!
//! A port of `commands_workspace.cmd_workspace_snapshot`.
//!
//! **Enforce-push is the whole command.** A manifest branch is a promise to another engineer
//! on another machine: `charter workspace restore` will check that branch out, and a branch
//! that is not on the remote cannot be checked out there. So a repo with uncommitted work,
//! with unpushed commits, or with no upstream at all **blocks** the snapshot, and the refusal
//! names each one. `--force` records the branches as they stand and makes the promise anyway,
//! which is the operator's to make.
//!
//! A clone charter could not read blocks too (charter#917): this list being empty is what
//! lets the manifest claim to capture reality, and a `git status` that failed contributes the
//! same emptiness as one that found nothing — to somebody who has no way to know it was never
//! checked.

use std::path::Path;

use serde_json::Value;

use crate::repocmd::{Say, Sink};
use crate::wscmd;

/// What `snapshot` was asked to record.
pub struct Request<'a> {
    pub root: &'a Path,
    /// The workspace, already resolved through the ladder by the caller.
    pub ws: &'a str,
    /// `--description`, which replaces the manifest's when it is given.
    pub description: Option<&'a str>,
    pub force: bool,
    /// The instant `updated_at` is stamped with, so a test can pin it.
    pub now: chrono::DateTime<chrono::Utc>,
}

/// `charter workspace snapshot [<name>] [--description …] [--force]`, and its exit code.
pub fn snapshot(request: &Request, say: Sink) -> u8 {
    let Request {
        root,
        ws,
        description,
        force,
        now,
    } = *request;

    let Ok(workspace) = crate::workspaces::Plane::open(root).workspace(ws) else {
        say(Say::Fail(format!(
            "invalid workspace name '{}'",
            crate::personas::one_line(ws)
        )));
        return 1;
    };
    let found = match crate::repos::clones(root, ws) {
        Ok(found) => found,
        Err(why) => {
            say(Say::Fail(format!(
                "could not read workspace '{ws}' — {why}"
            )));
            return 1;
        }
    };
    // Said, never dropped: a clone charter refused to look at is one this snapshot will not
    // record, and the operator is the only one who can tell whether the link is theirs.
    for (name, why) in &found.refused {
        say(Say::Warn(format!("{name} is not recorded — {why}")));
    }
    if found.repos.is_empty() {
        say(Say::Fail(format!(
            "workspace '{ws}' has no repo clones to snapshot."
        )));
        return 1;
    }
    let blockers = wscmd::restore_blockers(root, ws);
    if !blockers.is_empty() && !force {
        say(Say::Fail(format!(
            "Refusing to snapshot '{ws}' — push repo work first so the branch captures the \
             real state:"
        )));
        wscmd::say_each(say, blockers);
        say(Say::Info(
            "Commit + push inside each repo, then retry (or --force to snapshot branches \
             as-is)."
                .to_string(),
        ));
        return 2;
    }

    // The manifest on disk is the starting document, so a `description` and any key an
    // operator added keep their place: Python assigns into the dict it read, and a key that
    // is already there keeps the position it had.
    let (doc, _owner) = workspace.manifest();
    let mut doc = match doc {
        Some(Value::Object(map)) => Value::Object(map),
        // Whatever else is in the file — a list, a number, nothing at all — is not a
        // manifest, and `snapshot` is the DELIBERATE writer: an operator who typed this is
        // asking for the file to be rewritten.
        _ => Value::Object(serde_json::Map::new()),
    };
    let rows: Vec<Value> = found
        .repos
        .iter()
        .map(|repo| {
            let branch = match crate::repos::state_of(&repo.path) {
                Ok(state) => wscmd::branch_word(&state.head),
                // `git rev-parse --abbrev-ref HEAD` prints `HEAD` for a tree it cannot name a
                // branch in, and Python's `_repo_branch` falls back to the same literal.
                Err(_) => "HEAD".to_string(),
            };
            serde_json::json!({"name": repo.name, "branch": branch})
        })
        .collect();
    let map = doc.as_object_mut().expect("just built as an object");
    map.insert("name".into(), Value::String(ws.to_string()));
    if let Some(text) = description {
        map.insert("description".into(), Value::String(text.to_string()));
    }
    // `setdefault`: a manifest that never had one gets an empty string, and one that has a
    // description keeps it.
    map.entry("description")
        .or_insert_with(|| Value::String(String::new()));
    map.insert("repos".into(), Value::Array(rows.clone()));
    map.insert(
        "updated_at".into(),
        Value::String(now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()),
    );
    map.insert("updated_by".into(), Value::String(wscmd::git_user(root)));

    if let Err(why) = workspace.write_manifest(&doc) {
        say(Say::Fail(format!(
            "could not write workspaces/{ws}/workspace.json ({why}) — nothing was recorded."
        )));
        return 1;
    }
    say(Say::Done(format!(
        "Snapshot '{ws}' → workspaces/{ws}/workspace.json  ({} repo(s)):",
        rows.len()
    )));
    for row in &rows {
        say(Say::Info(format!(
            "  {} @ {}",
            row["name"].as_str().unwrap_or_default(),
            row["branch"].as_str().unwrap_or_default()
        )));
    }
    say(Say::Info(
        "Share it with the team: charter workspace save   (commits + pushes manifest + \
         memory)."
            .to_string(),
    ));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        dir
    }

    fn git(at: &Path, argv: &[&str]) {
        let run = crate::worktree::git::run(at, argv, crate::worktree::git::READ).unwrap();
        assert!(run.ok(), "git {argv:?}: {}", run.err);
    }

    /// A clone with an upstream, so the enforce-push guard has something real to read.
    fn pushed_clone(plane: &Path, ws: &str, name: &str) -> PathBuf {
        let origin = plane.join("origins").join(name);
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "-q", "--bare", "-b", "main"]);
        let seed = plane.join("seed").join(name);
        std::fs::create_dir_all(&seed).unwrap();
        git(&seed, &["init", "-q", "-b", "main"]);
        git(&seed, &["config", "user.email", "t@example.com"]);
        git(&seed, &["config", "user.name", "T"]);
        std::fs::write(seed.join("README.md"), "hi\n").unwrap();
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "-qm", "first"]);
        git(
            &seed,
            &["remote", "add", "origin", &origin.display().to_string()],
        );
        git(&seed, &["push", "-q", "-u", "origin", "main"]);

        let clone = plane.join("workspaces").join(ws).join(name);
        std::fs::create_dir_all(clone.parent().unwrap()).unwrap();
        git(
            plane,
            &[
                "clone",
                "-q",
                &origin.display().to_string(),
                &clone.display().to_string(),
            ],
        );
        git(&clone, &["config", "user.email", "t@example.com"]);
        git(&clone, &["config", "user.name", "T"]);
        clone
    }

    fn run(root: &Path, ws: &str, force: bool) -> (u8, Vec<String>) {
        let mut said = Vec::new();
        let code = snapshot(
            &Request {
                root,
                ws,
                description: None,
                force,
                now: "2026-05-04T11:32:17Z".parse().unwrap(),
            },
            &mut |line: Say| said.push(line.to_string()),
        );
        (code, said)
    }

    fn manifest(root: &Path, ws: &str) -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(root.join("workspaces").join(ws).join("workspace.json"))
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn a_workspace_with_no_clones_has_nothing_to_snapshot() {
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("workspaces/beta")).unwrap();
        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 1);
        assert_eq!(
            said,
            vec!["✗ workspace 'beta' has no repo clones to snapshot."]
        );
    }

    #[test]
    fn a_pushed_clone_is_recorded_with_its_branch() {
        let dir = plane();
        pushed_clone(dir.path(), "beta", "svc");
        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0, "{said:?}");
        let doc = manifest(dir.path(), "beta");
        assert_eq!(doc["name"], "beta");
        assert_eq!(doc["description"], "");
        assert_eq!(doc["repos"][0]["name"], "svc");
        assert_eq!(doc["repos"][0]["branch"], "main");
        assert_eq!(doc["updated_at"], "2026-05-04T11:32:17+00:00");
        assert!(
            doc.get(crate::manifest::KEY).is_some(),
            "charter stamps what it wrote"
        );
        assert!(said.iter().any(|l| l.contains("  svc @ main")), "{said:?}");
    }

    #[test]
    fn unpushed_work_blocks_the_snapshot_and_force_records_it_anyway() {
        let dir = plane();
        let clone = pushed_clone(dir.path(), "beta", "svc");
        std::fs::write(clone.join("second.md"), "x\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-qm", "second"]);

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(said[0].contains("Refusing to snapshot 'beta'"), "{said:?}");
        assert!(said[1].contains("svc: 1 unpushed commit(s)"), "{said:?}");
        assert!(
            !dir.path().join("workspaces/beta/workspace.json").exists(),
            "a refused snapshot records nothing"
        );

        let (code, said) = run(dir.path(), "beta", true);
        assert_eq!(code, 0, "{said:?}");
        assert_eq!(manifest(dir.path(), "beta")["repos"][0]["branch"], "main");
    }

    #[test]
    fn a_branch_with_no_upstream_blocks_because_restore_could_not_check_it_out() {
        let dir = plane();
        let clone = pushed_clone(dir.path(), "beta", "svc");
        git(&clone, &["checkout", "-q", "-b", "local-only"]);

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(
            said[1].contains("svc: branch 'local-only' isn't pushed to a remote"),
            "{said:?}"
        );
    }

    #[test]
    fn uncommitted_work_blocks_it() {
        let dir = plane();
        let clone = pushed_clone(dir.path(), "beta", "svc");
        std::fs::write(clone.join("README.md"), "changed\n").unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(said[1].contains("svc: uncommitted changes"), "{said:?}");
    }

    #[test]
    fn a_clone_charter_could_not_read_blocks_it_rather_than_contributing_nothing() {
        // charter#917: the manifest would otherwise assert a state nobody measured.
        let dir = plane();
        pushed_clone(dir.path(), "beta", "svc");
        let broken = dir.path().join("workspaces/beta/other");
        std::fs::create_dir_all(broken.join(".git")).unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 2, "{said:?}");
        assert!(
            said.iter().any(|l| l.contains("other: could not be read")),
            "{said:?}"
        );
    }

    #[test]
    fn a_description_is_set_where_one_is_given_and_kept_where_it_is_not() {
        let dir = plane();
        pushed_clone(dir.path(), "beta", "svc");
        let mut said = Vec::new();
        let code = snapshot(
            &Request {
                root: dir.path(),
                ws: "beta",
                description: Some("the billing work"),
                force: false,
                now: "2026-05-04T11:32:17Z".parse().unwrap(),
            },
            &mut |line: Say| said.push(line.to_string()),
        );
        assert_eq!(code, 0, "{said:?}");
        assert_eq!(
            manifest(dir.path(), "beta")["description"],
            "the billing work"
        );

        let (code, _said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0);
        assert_eq!(
            manifest(dir.path(), "beta")["description"],
            "the billing work",
            "a snapshot with no --description keeps the one that is there"
        );
    }

    #[test]
    fn a_manifest_that_is_not_an_object_is_replaced_rather_than_crashed_over() {
        let dir = plane();
        pushed_clone(dir.path(), "beta", "svc");
        std::fs::write(
            dir.path().join("workspaces/beta/workspace.json"),
            "[\"not a manifest\"]",
        )
        .unwrap();
        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0, "{said:?}");
        assert_eq!(manifest(dir.path(), "beta")["name"], "beta");
    }
}
