//! `<data>`, charter's data home (ADR 0075, amending ADR 0069): the Machine tier's fourth home,
//! for what is too large or too long-lived for the config home. The host's event log is the
//! first store written here (FD-9); the audit is decided for it too (AU-3).
//!
//! `$CHARTER_DATA_HOME`, else `$XDG_DATA_HOME/charter`, else the OS data directory's `charter/`
//! (`~/Library/Application Support/charter` on macOS, `~/.local/share/charter` on Linux).
//!
//! **Never inside a plane or a git work tree** (ADR 0075 §6). What is kept here is device-bound
//! and never committed, and a `<data>` a variable pointed into a repository would be one
//! `git add -A` from being pushed. The writer asks [`refusal`] first.

use std::path::{Path, PathBuf};

/// The variable that moves `<data>`.
pub const HOME_VAR: &str = "CHARTER_DATA_HOME";

/// `<data>`, from this process's environment.
pub fn root() -> Option<PathBuf> {
    root_in(&|name| std::env::var(name).ok())
}

/// `<data>` as the environment `env` answers it. An empty value names nothing.
pub fn root_in(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let named = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    named(HOME_VAR)
        .or_else(|| named("XDG_DATA_HOME").map(|xdg| xdg.join("charter")))
        .or_else(|| dirs::data_dir().map(|dir| dir.join("charter")))
}

/// Why `dir` cannot be `<data>`, or `None` when it can: it, or a directory above it, is a git
/// work tree or a plane.
pub fn refusal(dir: &Path) -> Option<String> {
    dir.ancestors().find_map(|above| {
        if above.join(".git").exists() {
            Some(format!(
                "{} is inside the git work tree at {}, and charter's data home is never \
                 somewhere a commit could take it",
                dir.display(),
                above.display()
            ))
        } else if above.join(crate::plane::MANIFEST).is_file() {
            Some(format!(
                "{} is inside the plane at {}, and charter's data home is never in a plane",
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
}
