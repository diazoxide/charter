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
use std::io;
use std::path::{Path, PathBuf};

use crate::doctor::fsx;
use crate::forge::{self, Forge, Kind};
use crate::repocmd::{Say, Sink};
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
        let got = local_config(repo)
            .remove(&key.to_lowercase())
            .unwrap_or_default();
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
            let _ = git::run(
                repo,
                &["config", "--local", "--add", &url_key, ssh],
                git::READ,
            );
            changed.push(format!("rewrite {ssh} → {https_base}"));
        }
    }
    changed
}

// --------------------------------------------------------------------------------------- //
// the check half, and `charter git-policy`                                                  //
// --------------------------------------------------------------------------------------- //

/// What [`check`] returns — and the ONLY thing it returns — for a repo whose origin host is
/// neither a default forge host nor one DECLARED in this plane's `charter.toml`.
///
/// Distinguished from ordinary drift because it cannot be fixed by [`apply`]: there is no
/// forge to apply a policy FOR. Before this existed an unrecognised host silently fell back to
/// GitLab's policy, so `check` returned "compliant" for a repo it had never verified — a false
/// green, which is worse than no check at all because it still LOOKS like the golden rule is
/// enforced.
pub const UNMANAGED_FORGE: &str = "forge unknown — origin host is not a default forge host and \
                                   isn't declared in charter.toml; the token-only policy can't \
                                   be verified or applied. Declare it under [[forge]] (host = \
                                   \"…\") to bring it under management.";

/// Policy settings that are MISSING or wrong in `repo`'s local config, against THAT repo's own
/// forge. Empty means compliant. Python's `check`.
pub fn check(repo: &Path, root: &Path) -> Vec<String> {
    let Some(forge) = forge_for(repo, root) else {
        return vec![UNMANAGED_FORGE.to_string()];
    };
    let cfg = local_config(repo);
    let policy = [
        ("credential.helper", forge.credential_helper()),
        ("commit.gpgsign", "false".to_string()),
        ("tag.gpgsign", "false".to_string()),
    ];
    let (https_base, ssh_forms) = forge.insteadof();
    let url_key = format!("url.{https_base}.insteadOf");
    let mut drift = Vec::new();
    for (key, want) in &policy {
        let got = cfg.get(&key.to_lowercase());
        if got.and_then(|values| values.last()) != Some(want) {
            drift.push(format!("{key} != {want}"));
        }
    }
    let empty = Vec::new();
    let have = cfg.get(&url_key.to_lowercase()).unwrap_or(&empty);
    for ssh in &ssh_forms {
        if !have.contains(ssh) {
            drift.push(format!("{url_key} missing {ssh}"));
        }
    }
    drift
}

/// A directory under `workspaces/` whose `.git` charter could not look at, and the errno the
/// look met — which is what decides the remedy: a symlink loop is cleared at the loop, and
/// only a refusal by restoring read access. `doctor`'s `Unread`, by another name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unseen {
    pub path: PathBuf,
    pub code: Option<i32>,
}

/// Whether there is a `.git` at `path` — asked the way Python's `Path.exists` asks it, through
/// the link rather than at it.
fn is_git_repo(path: &Path) -> bool {
    std::fs::metadata(path.join(".git")).is_ok()
}

/// The control plane itself plus every repo clone under `workspaces/<ws>/<repo>`, and beside
/// them every directory whose `.git` charter cannot check. Python's `scan`.
///
/// **A level charter cannot LIST is one of them too.** `workspaces/` itself is then the only
/// entry, and one unreadable workspace is named beside the clones of every other — raising
/// there hid every other workspace's clones along with it, and took `charter doctor` down as a
/// whole command rather than one row.
pub fn scan(root: &Path, workspaces_dir: &Path) -> (Vec<PathBuf>, Vec<Unseen>) {
    let mut out = Vec::new();
    let mut unseen = Vec::new();
    if is_git_repo(root) {
        out.push(root.to_path_buf());
    }
    let workspaces = match sorted_dir(workspaces_dir) {
        Ok(entries) => entries,
        Err(e) if fsx::absent(&e) => return (out, unseen),
        Err(e) => {
            // The LISTING, guarded apart from each workspace below: nothing under it can be
            // counted, so it is named on its own and every clone in it is out of the count.
            return (
                out,
                vec![Unseen {
                    path: workspaces_dir.to_path_buf(),
                    code: e.raw_os_error(),
                }],
            );
        }
    };
    for ws in workspaces {
        let clones = match sorted_dir(&ws) {
            Ok(entries) => entries,
            // Not a directory — Finder's `.DS_Store`, or a link to nothing.
            Err(e) if fsx::absent(&e) => continue,
            Err(e) => {
                unseen.push(Unseen {
                    path: ws,
                    code: e.raw_os_error(),
                });
                continue;
            }
        };
        for clone in clones {
            match std::fs::metadata(clone.join(".git")) {
                Ok(_) => out.push(clone),
                Err(e) if fsx::absent(&e) => continue,
                Err(e) => unseen.push(Unseen {
                    path: clone,
                    code: e.raw_os_error(),
                }),
            }
        }
    }
    (out, unseen)
}

fn sorted_dir(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    Ok(entries)
}

/// `charter git-policy [--apply]`. Python's `cmd_git_policy`. Always exits 0: drift is a report,
/// not a failure of the command that found it.
pub fn policy(root: &Path, apply_it: bool, say: Sink) -> u8 {
    let workspaces_dir = root.join("workspaces");
    let (targets, unseen) = scan(root, &workspaces_dir);
    // What the scan could not read, named FIRST and with what clears it. Reading the repo list
    // alone reported on the clones it reached and skipped the rest without a word, down to "No
    // git repos found".
    for missed in &unseen {
        let mut named = missed
            .path
            .strip_prefix(root)
            .unwrap_or(&missed.path)
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        if missed.path == workspaces_dir {
            named.push('/');
        }
        // The REMEDY is `doctor`'s, not a second copy of it: one `ELOOP` and one sentence, so
        // the `git auth` row and this command cannot send a reader to different repairs for
        // one directory. The path is named as Python names it here — absolute, in full.
        say(Say::Warn(format!(
            "{named} cannot be checked — {}",
            fsx::uncheckable_fix(missed.code, &missed.path.display().to_string())
        )));
    }
    if targets.is_empty() {
        say(Say::Info(
            "No git repos found (control plane + workspace clones).".into(),
        ));
        return 0;
    }
    let (mut drifted, mut fixed, mut unmanaged) = (0usize, 0usize, 0usize);
    for repo in &targets {
        let rel = repo.strip_prefix(root).unwrap_or(repo).to_string_lossy();
        let name = if rel.is_empty() || rel == "." {
            "control plane".to_string()
        } else {
            rel.into_owned()
        };
        let drift = check(repo, root);
        if drift.is_empty() {
            continue;
        }
        if drift.len() == 1 && drift[0] == UNMANAGED_FORGE {
            // Honest "can't tell" — never silently reported green, and never guessed at via
            // `apply` either.
            unmanaged += 1;
            say(Say::Warn(format!("{name}: {UNMANAGED_FORGE}")));
            continue;
        }
        drifted += 1;
        if apply_it {
            let changes = apply(repo, root);
            fixed += 1;
            say(Say::Done(format!(
                "{name}: applied {} setting(s) — token-only",
                changes.len()
            )));
        } else {
            say(Say::Warn(format!(
                "{name}: {} setting(s) not token-only",
                drift.len()
            )));
            for line in drift.iter().take(4) {
                say(Say::Info(format!("    {line}")));
            }
        }
    }
    if drifted == 0 && unmanaged == 0 {
        say(Say::Done(format!(
            "All {} repo(s) are token-only (each forge's own HTTPS token, no SSH, no signing).",
            targets.len()
        )));
        return 0;
    }
    if apply_it {
        say(Say::Done(format!(
            "Applied the single-credential policy to {fixed} of {} repo(s).",
            targets.len()
        )));
    } else if drifted > 0 {
        say(Say::Info(format!(
            "{drifted} of {} repo(s) drifted — fix: charter git-policy --apply",
            targets.len()
        )));
    }
    if unmanaged > 0 {
        say(Say::Warn(format!(
            "{unmanaged} repo(s) have an unrecognised forge — not covered by any policy. \
             Declare the host in charter.toml's [[forge]] to bring {} under management.",
            if unmanaged == 1 { "it" } else { "them" }
        )));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(origin: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(
            root.join("charter.toml"),
            "[[forge]]\nkind = \"gitlab\"\nhost = \"git.internal\"\n",
        )
        .unwrap();
        let clone = root.join("workspaces/alpha/widget");
        std::fs::create_dir_all(&clone).unwrap();
        // Deliberately NOT through `crate::testgit`: its template writes the very
        // `commit.gpgsign = false` these tests assert the policy writes, which would make them
        // pass without it. Nothing here commits, so no signer is ever asked.
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
        assert!(
            written.contains("helper = !gh auth git-credential"),
            "{written}"
        );
        assert!(written.contains("[commit]\n\tgpgsign = false"), "{written}");
        assert!(written.contains("[tag]\n\tgpgsign = false"), "{written}");
        assert!(
            written.contains(
                "[url \"https://github.com/\"]\n\tinsteadOf = git@github.com:\n\tinsteadOf = ssh://git@github.com/"
            ),
            "{written}"
        );
        assert!(
            apply(&clone, &root).is_empty(),
            "a second apply changes nothing"
        );
    }

    #[test]
    fn a_declared_self_hosted_forge_gets_its_own_host_never_the_default() {
        let (dir, clone) = repo(Some("git@git.internal:team/widget.git"));
        let root = std::fs::canonicalize(dir.path()).unwrap();

        apply(&clone, &root);

        let written = config(&clone);
        assert!(written.contains("!glab auth git-credential"), "{written}");
        assert!(
            written.contains("[url \"https://git.internal/\"]"),
            "{written}"
        );
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
