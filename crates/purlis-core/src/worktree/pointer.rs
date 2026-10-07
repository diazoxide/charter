//! Which git directory a branch's folder belongs to, said by purlis's own record and checked
//! against the folder before git runs in it (D-1453-27).
//!
//! A branch's folder is a linked worktree: its `.git` is one line naming a directory under the
//! clone it was cut from (`<clone>/.git/worktrees/<piece>`), and git run in the folder follows
//! that line wherever it leads. The folder is the place a chat writes, so the line is not
//! something purlis reads a repository from. For a git action it takes on its own account in
//! such a folder, purlis **derives** the git directory from what it recorded (the clone, and
//! the piece's name), checks that the folder still names exactly that directory and that the
//! directory names the folder back, and hands git both paths itself
//! ([`super::git::Isolated::pinned`]). A folder that names anything else is refused, and no
//! git runs in it.
//!
//! [`verified`] is the whole check, and it runs no git.

use std::path::{Path, PathBuf};

use super::name;

/// The longest `.git` line read. git writes one path; nothing honest is longer.
const AT_MOST: u64 = 8 * 1024;

/// A folder that is not, or is no longer, the folder of the piece purlis recorded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the folder of '{piece}' does not name the git directory this clone keeps for it, so no git \
     is run in it"
)]
pub struct NotItsFolder {
    pub piece: String,
}

impl NotItsFolder {
    /// The one sentence the person reads in the window.
    pub fn in_window(&self) -> String {
        format!(
            "The folder of '{}' is no longer wired to the branch purlis cut it on, so purlis \
             will not run git in it; nothing was changed, and deleting the folder yourself \
             leaves the branch where it is.",
            self.piece
        )
    }
}

/// The path a `.git` file names: the text is exactly `gitdir: <path>` and at most one line
/// end. Anything else (a second line, another key, nothing after the colon) names none.
pub fn named_by(text: &str) -> Option<&Path> {
    let line = text.strip_suffix('\n').unwrap_or(text);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let path = line.strip_prefix("gitdir: ")?;
    if path.is_empty() || path.contains(['\n', '\r', '\0']) {
        return None;
    }
    Some(Path::new(path))
}

/// The one line of the file at `at`, a plain file and no link, or `None`.
fn line_of(at: &Path) -> Option<String> {
    let meta = at.symlink_metadata().ok()?;
    if !meta.file_type().is_file() || meta.len() > AT_MOST {
        return None;
    }
    std::fs::read_to_string(at).ok()
}

/// **The git directory of `piece`'s folder, derived and checked**: `<git_dir>/worktrees/<piece>`
/// where `git_dir` is the clone's own git directory (one already checked to be it), returned
/// with its links resolved, for the caller to pin git to.
///
/// It is returned only where all of this holds, and nothing here runs git:
///
/// - `piece` is a piece's name, and `<git_dir>/worktrees/<piece>` is a directory reached
///   through no link, so it is the clone's own entry and nothing a link leads to (a folder of
///   temporary files, the piece's folder, another clone), and it is not inside `folder`;
/// - `folder/.git` is a plain file, no link and no directory, holding the one line git writes,
///   and the path it names resolves to that directory;
/// - that directory's own `gitdir` names `folder/.git` back.
///
/// The contract is the one every app-side git call in a folder a chat writes is held to: the
/// git directory comes from the app's record, the folder only confirms it.
pub fn verified(git_dir: &Path, folder: &Path, piece: &str) -> Result<PathBuf, NotItsFolder> {
    let refused = || NotItsFolder {
        piece: piece.to_owned(),
    };
    if !name::piece_name_ok(piece) {
        return Err(refused());
    }
    // The one directory accepted, spelled from the clone's git directory and reached through
    // no link: neither `worktrees` nor the entry in it may lead anywhere else.
    let clone_s = std::fs::canonicalize(git_dir).map_err(|_| refused())?;
    let own = clone_s.join("worktrees").join(piece);
    let reached = std::fs::canonicalize(&own).map_err(|_| refused())?;
    if reached != own || !own.is_dir() {
        return Err(refused());
    }
    // Never a directory inside the folder it would be the git directory of: what is in the
    // folder is the chat's.
    let folder_s = std::fs::canonicalize(folder).map_err(|_| refused())?;
    if own.starts_with(&folder_s) || clone_s.starts_with(&folder_s) {
        return Err(refused());
    }
    let pointer = folder.join(".git");
    let text = line_of(&pointer).ok_or_else(refused)?;
    let named = named_by(&text).ok_or_else(refused)?;
    // A relative path is read from the folder, as git reads it.
    let named = std::fs::canonicalize(folder.join(named)).map_err(|_| refused())?;
    if named != own {
        return Err(refused());
    }
    let back = line_of(&own.join("gitdir")).ok_or_else(refused)?;
    let back = back.trim_end_matches(['\n', '\r']);
    if back.is_empty() {
        return Err(refused());
    }
    let back = std::fs::canonicalize(own.join(back)).map_err(|_| refused())?;
    let pointer = std::fs::canonicalize(&pointer).map_err(|_| refused())?;
    if back != pointer {
        return Err(refused());
    }
    Ok(own)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pointer_file_names_a_path_only_as_the_one_line_git_writes() {
        assert_eq!(
            named_by("gitdir: /c/.git/worktrees/p\n"),
            Some(Path::new("/c/.git/worktrees/p"))
        );
        assert_eq!(
            named_by("gitdir: ../../r/.git/worktrees/p"),
            Some(Path::new("../../r/.git/worktrees/p"))
        );
        assert_eq!(
            named_by("gitdir: /c/.git/worktrees/p\r\n"),
            Some(Path::new("/c/.git/worktrees/p"))
        );
        for not_one in [
            "",
            "\n",
            "gitdir: ",
            "gitdir: \n",
            "gitdir:/c/.git/worktrees/p\n",
            " gitdir: /c/.git/worktrees/p\n",
            "gitdir: /c/.git/worktrees/p\n\n",
            "gitdir: /c/.git/worktrees/p\ngitdir: /elsewhere\n",
            "# gitdir: /c/.git/worktrees/p\n",
            "[core]\n\tbare = false\n",
            "gitdir: /c/.git/worktrees/p\0\n",
        ] {
            assert_eq!(named_by(not_one), None, "{not_one:?}");
        }
    }

    /// A clone's git directory under another name (the sandbox this is built in refuses a
    /// directory called `.git`), holding one piece, and that piece's folder.
    struct Made {
        _dir: tempfile::TempDir,
        git_dir: PathBuf,
        folder: PathBuf,
    }

    fn made(piece: &str) -> Made {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let git_dir = root.join("clone-git");
        let own = git_dir.join("worktrees").join(piece);
        let folder = root.join("pieces").join(piece);
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        point(&folder, &own);
        std::fs::write(
            own.join("gitdir"),
            format!("{}\n", folder.join(".git").display()),
        )
        .unwrap();
        Made {
            _dir: dir,
            git_dir,
            folder,
        }
    }

    fn point(folder: &Path, at: &Path) {
        let pointer = folder.join(".git");
        let _ = std::fs::remove_file(&pointer);
        std::fs::write(pointer, format!("gitdir: {}\n", at.display())).unwrap();
    }

    #[test]
    fn a_folder_that_names_its_own_git_directory_is_verified_to_it() {
        let made = made("task-0a1b2c3d");
        assert_eq!(
            verified(&made.git_dir, &made.folder, "task-0a1b2c3d"),
            Ok(made.git_dir.join("worktrees").join("task-0a1b2c3d"))
        );
    }

    #[test]
    fn a_folder_whose_pointer_was_rewritten_is_refused_whatever_it_names() {
        let piece = "task-0a1b2c3d";
        let refused = Err(NotItsFolder {
            piece: piece.to_owned(),
        });

        // A git directory of the chat's own making, inside its folder.
        let made_one = made(piece);
        let planted = made_one.folder.join("planted");
        std::fs::create_dir_all(&planted).unwrap();
        point(&made_one.folder, &planted);
        assert_eq!(
            verified(&made_one.git_dir, &made_one.folder, piece),
            refused
        );

        // Another piece's directory in the same clone: a real one, and not this folder's.
        let made_two = made(piece);
        let other = made_two.git_dir.join("worktrees").join("other-99999999");
        std::fs::create_dir_all(&other).unwrap();
        point(&made_two.folder, &other);
        assert_eq!(
            verified(&made_two.git_dir, &made_two.folder, piece),
            refused
        );

        // The clone's git directory itself.
        let made_three = made(piece);
        point(&made_three.folder, &made_three.git_dir);
        assert_eq!(
            verified(&made_three.git_dir, &made_three.folder, piece),
            refused
        );

        // No pointer at all, a directory in its place, and more than the one line.
        let made_four = made(piece);
        let pointer = made_four.folder.join(".git");
        std::fs::remove_file(&pointer).unwrap();
        assert_eq!(
            verified(&made_four.git_dir, &made_four.folder, piece),
            refused
        );
        let own = made_four.git_dir.join("worktrees").join(piece);
        std::fs::write(
            &pointer,
            format!("gitdir: {}\ngitdir: /elsewhere\n", own.display()),
        )
        .unwrap();
        assert_eq!(
            verified(&made_four.git_dir, &made_four.folder, piece),
            refused
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_pointer_that_is_a_link_is_refused_even_to_the_right_text() {
        let piece = "task-0a1b2c3d";
        let made = made(piece);
        let pointer = made.folder.join(".git");
        let real = made.folder.join("real-pointer");
        std::fs::rename(&pointer, &real).unwrap();
        std::os::unix::fs::symlink(&real, &pointer).unwrap();
        assert_eq!(
            verified(&made.git_dir, &made.folder, piece),
            Err(NotItsFolder {
                piece: piece.to_owned()
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_entry_of_the_clone_that_is_a_link_is_refused_wherever_it_leads() {
        let piece = "task-0a1b2c3d";
        let refused = Err(NotItsFolder {
            piece: piece.to_owned(),
        });
        // Into the folder itself, into a folder of temporary files, and for `worktrees` whole:
        // each made to agree both ways, so only the link is what is wrong with it.
        for inside_the_folder in [true, false] {
            let made = made(piece);
            let other = tempfile::tempdir().unwrap();
            let elsewhere = if inside_the_folder {
                made.folder.join("planted")
            } else {
                std::fs::canonicalize(other.path()).unwrap().join("planted")
            };
            let own = made.git_dir.join("worktrees").join(piece);
            std::fs::rename(&own, &elsewhere).unwrap();
            std::os::unix::fs::symlink(&elsewhere, &own).unwrap();
            point(&made.folder, &own);
            assert_eq!(verified(&made.git_dir, &made.folder, piece), refused);
        }
        let made = made(piece);
        let kept = made.git_dir.join("worktrees");
        let moved = made.git_dir.join("moved");
        std::fs::rename(&kept, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &kept).unwrap();
        assert_eq!(verified(&made.git_dir, &made.folder, piece), refused);
    }

    #[test]
    fn a_clone_s_git_directory_inside_the_folder_is_refused() {
        let piece = "task-0a1b2c3d";
        let dir = tempfile::tempdir().unwrap();
        let folder = std::fs::canonicalize(dir.path()).unwrap().join(piece);
        let git_dir = folder.join("clone-git");
        let own = git_dir.join("worktrees").join(piece);
        std::fs::create_dir_all(&own).unwrap();
        point(&folder, &own);
        std::fs::write(
            own.join("gitdir"),
            format!("{}\n", folder.join(".git").display()),
        )
        .unwrap();
        assert_eq!(
            verified(&git_dir, &folder, piece),
            Err(NotItsFolder {
                piece: piece.to_owned()
            })
        );
    }

    #[test]
    fn a_git_directory_that_names_another_folder_back_is_refused() {
        let piece = "task-0a1b2c3d";
        let made = made(piece);
        // A copy of the folder elsewhere carries the same pointer; the clone knows one folder.
        let copy = made.folder.parent().unwrap().join("copy");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::copy(made.folder.join(".git"), copy.join(".git")).unwrap();
        assert_eq!(
            verified(&made.git_dir, &copy, piece),
            Err(NotItsFolder {
                piece: piece.to_owned()
            })
        );
    }

    #[test]
    fn a_name_that_is_no_piece_s_is_refused_before_any_path_is_built() {
        let made = made("task-0a1b2c3d");
        for name in ["", "..", "../task-0a1b2c3d", "a/b", "."] {
            assert!(
                verified(&made.git_dir, &made.folder, name).is_err(),
                "{name:?}"
            );
        }
    }

    #[test]
    fn the_window_is_told_in_one_sentence_and_never_the_word_worktree() {
        let said = NotItsFolder {
            piece: "task-0a1b2c3d".to_owned(),
        }
        .in_window();
        assert!(said.contains("'task-0a1b2c3d'"), "{said}");
        assert!(!said.to_lowercase().contains("worktree"), "{said}");
        assert_eq!(said.matches(". ").count(), 0, "{said}");
    }
}
