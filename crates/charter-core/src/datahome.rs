//! `<data>`, charter's data home (ADR 0075, amending ADR 0069): the Machine tier's fourth home,
//! for what is too large or too long-lived for the config home. The host's event log is the
//! first store written here (FD-9); the audit is decided for it too (AU-3).
//!
//! `$CHARTER_DATA_HOME`, else `$XDG_DATA_HOME/charter`, else the OS data directory's `charter/`
//! (`~/Library/Application Support/charter` on macOS, `~/.local/share/charter` on Linux).
//!
//! **Never inside a project or a git work tree** (ADR 0075 §6). What is kept here is device-bound
//! and never committed, and a `<data>` a variable pointed into a repository would be one
//! `git add -A` from being pushed. The writer asks [`refusal`] first.

use std::path::{Path, PathBuf};

/// The variable that moves `<data>`.
pub const HOME_VAR: &str = "PURLIS_DATA_HOME";

/// `<data>`, from this process's environment.
///
/// A fenced build is held here (charter-app#129): a test run that pins no data home of its own
/// would append its throwaway chats' events to the operator's own log.
pub fn root() -> Option<PathBuf> {
    let found = root_in(&crate::envvar::var)?;
    crate::fence::hold(crate::fence::Act::Store, &found);
    Some(found)
}

/// The data home's folder under `$XDG_DATA_HOME` or the OS data directory.
pub(crate) const DIR: &str = "charter";

/// `<data>` as the environment `env` answers it. An empty value names nothing.
pub fn root_in(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let named = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let found = named(HOME_VAR)
        .or_else(|| named("XDG_DATA_HOME").map(|xdg| xdg.join(DIR)))
        .or_else(|| dirs::data_dir().map(|dir| dir.join(DIR)))?;
    // A relative value is taken from the directory charter was started in, so the answer is
    // always an absolute path.
    std::path::absolute(found).ok()
}

/// `dir` as the operating system would reach it.
fn resolved(dir: &Path) -> PathBuf {
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut existing = dir.as_path();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                existing = parent;
            }
            // `..` at the end of a path that does not exist, or the root itself.
            _ => break,
        }
    }
    let mut out = std::fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    for part in rest.into_iter().rev() {
        if part == ".." {
            out.pop();
        } else if part != "." {
            out.push(part);
        }
    }
    out
}

/// Why `dir` cannot be `<data>`, or `None` when it can: it, or a directory above it, is a git
/// work tree or a project.
///
/// Asked of the directory as the operating system resolves it, not as it is spelled: the
/// deepest part that exists is canonicalized (links followed, `..` taken), and the rest is
/// joined on with any `..` in it taken lexically. So neither a link nor a `..` can walk a
/// `<data>` into a repository unseen.
pub fn refusal(dir: &Path) -> Option<String> {
    resolved(dir).ancestors().find_map(|above| {
        if above.join(".git").exists() {
            Some(format!(
                "{} is inside the git work tree at {}, and charter's data home is never \
                 somewhere a commit could take it",
                dir.display(),
                above.display()
            ))
        } else if crate::names::has_manifest(above) {
            Some(format!(
                "{} is inside the project at {}, and charter's data home is never in a project",
                dir.display(),
                above.display()
            ))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn the_data_home_is_the_variable_then_xdg_then_the_os_data_directory() {
        assert_eq!(
            root_in(&env(&[(HOME_VAR, "/d"), ("XDG_DATA_HOME", "/x")])),
            Some(PathBuf::from("/d"))
        );
        assert_eq!(
            root_in(&env(&[("XDG_DATA_HOME", "/x")])),
            Some(PathBuf::from("/x/charter"))
        );
        assert_eq!(
            root_in(&env(&[(HOME_VAR, ""), ("XDG_DATA_HOME", "")])),
            dirs::data_dir().map(|dir| dir.join("charter")),
            "an empty value names nothing"
        );
    }

    #[test]
    fn a_data_home_inside_a_project_marked_only_by_purlis_toml_is_refused() {
        // RN-2a: a project is one by either manifest, and no `.git` here to catch it first.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("purlis.toml"), "schema = 1\n").unwrap();
        let why = refusal(&root.join("data")).expect("refused");
        assert!(why.contains("inside the project"), "{why}");
    }

    #[test]
    fn a_data_home_inside_a_git_work_tree_or_a_plane_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let plane = dir.path().join("plane");
        std::fs::create_dir_all(&plane).unwrap();
        std::fs::write(plane.join(crate::plane::MANIFEST), "").unwrap();

        assert!(
            refusal(&repo.join("data")).is_some(),
            "it would be committed"
        );
        assert!(refusal(&plane.join("deep").join("data")).is_some());
        assert_eq!(refusal(&dir.path().join("elsewhere")), None);
    }

    #[test]
    #[cfg(unix)]
    fn a_data_home_reached_through_a_link_into_a_repository_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let link = dir.path().join("looks-elsewhere");
        std::os::unix::fs::symlink(&repo, &link).unwrap();

        assert!(
            refusal(&link.join("deep").join("data")).is_some(),
            "the link resolves into the repository"
        );
    }

    #[test]
    fn a_relative_data_home_is_taken_from_where_charter_was_started() {
        let root = root_in(&env(&[(HOME_VAR, "rel/data")])).unwrap();

        assert!(root.is_absolute(), "{}", root.display());
        assert!(root.ends_with("rel/data"));
    }

    #[test]
    #[cfg(unix)]
    fn a_data_home_through_a_link_to_a_directory_inside_a_repository_is_refused() {
        // `link/.git` is not there, so a check that walked the spelled path would pass it.
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(repo.join("sub"), &link).unwrap();

        assert!(refusal(&link.join("data")).is_some());
    }
}
