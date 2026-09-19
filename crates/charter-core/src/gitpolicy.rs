//! Golden rule 0 in a clone's own config: one credential per forge, over HTTPS, no signing.
//!
//! A port of the half of `charter/gitpolicy.py` that `clone` runs — `apply` and what it
//! needs. It writes three things into a repo's **local** config, never the global one, each
//! derived from the one forge the repo's `origin` names:
//!
//! 1. `credential.helper = !<cli> auth git-credential` — git asks that forge's CLI;
//! 2. `commit.gpgsign = false`, `tag.gpgsign = false` — no signer prompt can hang an agent;
//! 3. `url.<https-base>.insteadOf` for both of the forge's SSH forms — so a plain `git push`
//!    typed later in that clone goes over HTTPS with the token even if its remote is SSH.
//!
//! What is written is exactly Python's, byte for byte in the clone's `.git/config`, so the
//! Python `charter git-policy` reads a Rust clone as compliant and the other way round.

use std::collections::BTreeMap;
use std::path::Path;

use crate::forge::{self, Forge, Kind};
use crate::worktree::git;

/// The forge that governs `repo`'s policy, from its `origin`; `None` for a host this plane
/// does not manage, which is never answered with another forge's policy. A repo with no
/// origin at all keeps the pre-multi-forge default, GitLab. Python's `forge_for`.
pub fn forge_for(repo: &Path, root: &Path) -> Option<Forge> {
    let url = git::run(repo, &["remote", "get-url", "origin"], git::READ)
        .map(|r| r.line().trim().to_string())
        .unwrap_or_default();
    if url.is_empty() {
        return Some(Forge::default_of(Kind::GitLab));
    }
    forge::resolve_host(&url, root)
}

/// The repo's local config as `{lowercased key: [values]}`, in one git call.
fn local_config(repo: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Ok(listed) = git::run(repo, &["config", "--local", "--list", "-z"], git::READ) else {
        return out;
    };
    if !listed.ok() {
        return out;
    }
    for record in listed.out.split('\0').filter(|r| !r.is_empty()) {
        let (key, value) = record.split_once('\n').unwrap_or((record, ""));
        out.entry(key.trim().to_lowercase())
            .or_default()
            .push(value.to_string());
    }
    out
}

/// Write the token-only policy for `repo`'s own forge into its local config, idempotently.
/// Returns what changed; nothing for a repo that is not one, or whose forge charter cannot
/// name. Python's `apply`.
pub fn apply(repo: &Path, root: &Path) -> Vec<String> {
    if std::fs::symlink_metadata(repo.join(".git")).is_err() {
        return Vec::new();
    }
    let Some(forge) = forge_for(repo, root) else {
        return Vec::new();
    };
    let policy = [
        ("credential.helper", forge.credential_helper()),
        ("commit.gpgsign", "false".to_string()),
        ("tag.gpgsign", "false".to_string()),
    ];
    let (https_base, ssh_forms) = forge.insteadof();
    let url_key = format!("url.{https_base}.insteadOf");
    let mut changed = Vec::new();
    for (key, want) in &policy {
        let got = local_config(repo).remove(&key.to_lowercase()).unwrap_or_default();
        if got.last() != Some(want) {
            let _ = git::run(repo, &["config", "--local", key, want], git::READ);
            changed.push(format!("{key}={want}"));
        }
    }
    let have = local_config(repo)
        .remove(&url_key.to_lowercase())
        .unwrap_or_default();
    for ssh in &ssh_forms {
        if !have.contains(ssh) {
            let _ = git::run(repo, &["config", "--local", "--add", &url_key, ssh], git::READ);
            changed.push(format!("rewrite {ssh} → {https_base}"));
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(origin: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "[[forge]]\nkind = \"gitlab\"\nhost = \"git.internal\"\n").unwrap();
        let clone = root.join("workspaces/alpha/widget");
        std::fs::create_dir_all(&clone).unwrap();
        git::run(&clone, &["init", "-q", "."], git::READ).unwrap();
        if let Some(origin) = origin {
            git::run(&clone, &["remote", "add", "origin", origin], git::READ).unwrap();
        }
        (dir, clone)
    }

    fn config(repo: &Path) -> String {
        std::fs::read_to_string(repo.join(".git/config")).unwrap()
    }

    #[test]
    fn a_github_clone_gets_ghs_helper_no_signing_and_both_ssh_rewrites() {
        let (dir, clone) = repo(Some("https://github.com/acme/widget.git"));
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let changed = apply(&clone, &root);

        assert_eq!(changed.len(), 5, "{changed:?}");
        let written = config(&clone);
        assert!(written.contains("helper = !gh auth git-credential"), "{written}");
        assert!(written.contains("[commit]\n\tgpgsign = false"), "{written}");
        assert!(written.contains("[tag]\n\tgpgsign = false"), "{written}");
        assert!(
            written.contains(
                "[url \"https://github.com/\"]\n\tinsteadOf = git@github.com:\n\tinsteadOf = ssh://git@github.com/"
            ),
            "{written}"
        );
        assert!(apply(&clone, &root).is_empty(), "a second apply changes nothing");
    }

    #[test]
    fn a_declared_self_hosted_forge_gets_its_own_host_never_the_default() {
        let (dir, clone) = repo(Some("git@git.internal:team/widget.git"));
        let root = std::fs::canonicalize(dir.path()).unwrap();

        apply(&clone, &root);

        let written = config(&clone);
        assert!(written.contains("!glab auth git-credential"), "{written}");
        assert!(written.contains("[url \"https://git.internal/\"]"), "{written}");
        assert!(!written.contains("gitlab.com"), "{written}");
    }

    #[test]
    fn a_host_charter_does_not_manage_gets_no_policy_rather_than_another_forges() {
        let (dir, clone) = repo(Some("https://evil.example/acme/widget.git"));
        let root = std::fs::canonicalize(dir.path()).unwrap();

        assert!(apply(&clone, &root).is_empty());
        assert!(!config(&clone).contains("credential"), "{}", config(&clone));
    }
}
