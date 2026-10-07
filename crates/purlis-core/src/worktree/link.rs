//! Which repository a folder belongs to, read by purlis itself and checked before git is run
//! there (#1055).
//!
//! A clone's `.git` is a directory. A linked worktree's, and a submodule's, is a FILE holding
//! one line, `gitdir: <path>`, and git follows it to wherever it points. That file sits in the
//! folder a chat works in, so a chat may be able to rewrite it; and a git that purlis runs
//! there, outside any sandbox, would then read the configuration of a git directory the chat
//! chose, and run the programs it names.
//!
//! So the runner ([`super::git`]) never lets git discover a repository through a `.git` file.
//! [`of`] reads the file itself, and the call is given the git directory that was checked
//! (`--git-dir`, `--work-tree`), so what git reads is what was checked and not what the file
//! says a moment later.
//!
//! # What is accepted
//!
//! - **Never a git directory inside the folder itself**, nor one in a temp folder the folder
//!   is not in: both are places the folder's own chat writes.
//! - **A folder purlis cut** (`<workspace>/.worktrees/<repo>/<piece>`): only the entry the
//!   clone it was cut from keeps for it, `<workspace>/<repo>/.git/worktrees/<piece>`. That is
//!   [`super::pointer::verified`]'s rule and its code: this module derives the clone and the
//!   piece from where the folder is, and asks it. There is one check of a branch folder's
//!   link in purlis.
//! - **Any other folder**: a worktree entry of some clone's `.git`, checked the same two ways;
//!   or a submodule's git directory, which must lie below the `modules` folder of the
//!   repository that encloses the folder, as that repository's own link was checked.
//! - **Below a workspace** (`…/workspaces/<name>/…`, where chats work) nothing else.
//!   **Elsewhere** a link that names a git directory kept in another place is followed as git
//!   follows it (`git clone --separate-git-dir`): a project's top and a scratch checkout are
//!   laid out by their owner.
//!
//! # What this does not hold
//!
//! It checks where a link points, not who made what is there. Where a sandbox cannot keep a
//! chat from moving a prepared folder into a `.git` (ADR 0067 §5's stated gap, #1065), a
//! worktree entry or a `modules` folder inside a clone can be one the chat made; the rule for
//! a folder purlis cut does not depend on that, and the others do. A clone, whose `.git` is a
//! directory, is left as it is: its `config` and `hooks` are held by the sandbox by name.

use std::path::{Path, PathBuf};

/// What a refused call says where the link names something purlis will not follow.
pub const CHANGED: &str = "this folder's git link was changed, so purlis is not reading it";

/// What a refused call says where the link names a worktree whose own record names another
/// folder: one moved by hand, most often.
pub const MOVED: &str = "this folder's git link does not match its repository's record of it, \
     so purlis is not reading it. If the folder was moved, `git worktree repair` in it writes \
     the record again";

/// What a refused call says where the link names a git directory that is not there.
pub const GONE: &str =
    "this folder's git link names a git directory that is not there, so purlis is not reading it";

/// The repository a folder belongs to, as purlis read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// No `.git` at or above the folder: git answers for itself.
    None,
    /// A clone: `.git` is a directory at `top`.
    Clone { top: PathBuf },
    /// A linked worktree or a submodule at `top`, whose git directory is `git_dir`. Both as
    /// the kernel names them.
    Linked { top: PathBuf, git_dir: PathBuf },
}

/// How much of a linked worktree's own folder is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Everything: the worktree's git directory names the folder back.
    Whole,
    /// All but that: for `git worktree repair`, which is run in a folder that was moved and
    /// writes the name back.
    Moved,
}

/// The repository `dir` belongs to, or the sentence that refuses the call.
pub fn of(dir: &Path, check: Check) -> Result<Link, &'static str> {
    let Ok(here) = std::fs::canonicalize(dir) else {
        return Ok(Link::None);
    };
    for top in here.ancestors() {
        let entry = top.join(".git");
        let Ok(found) = std::fs::symlink_metadata(&entry) else {
            continue;
        };
        if found.file_type().is_dir() {
            return Ok(Link::Clone {
                top: top.to_path_buf(),
            });
        }
        if !found.file_type().is_file() {
            // A link, or anything else, standing where `.git` goes: followed only where a
            // person's own layout is, only to a directory, and never to one the folder's
            // own chat writes.
            let followed = std::fs::canonicalize(&entry).map_err(|_| CHANGED)?;
            if !below_a_workspace(top) && followed.is_dir() && !a_chats_to_make(top, &followed) {
                return Ok(Link::Clone {
                    top: top.to_path_buf(),
                });
            }
            return Err(CHANGED);
        }
        let named = named_by(&entry).ok_or(CHANGED)?;
        let named = if named.is_absolute() {
            named
        } else {
            top.join(named)
        };
        let git_dir = std::fs::canonicalize(&named).map_err(|_| GONE)?;
        checked(top, &git_dir, check)?;
        return Ok(Link::Linked {
            top: top.to_path_buf(),
            git_dir,
        });
    }
    Ok(Link::None)
}

/// The path a `.git` file names: its one `gitdir: <path>` line, read as
/// [`super::pointer::named_by`] reads it.
fn named_by(file: &Path) -> Option<PathBuf> {
    let text = small(file)?;
    super::pointer::named_by(&text).map(Path::to_path_buf)
}

/// A link file's text ([`super::pointer::line_of`]): a small regular file, opened once.
fn small(file: &Path) -> Option<String> {
    super::pointer::line_of(file)
}

/// Why `git_dir`, as the kernel names it, is not one `top`'s `.git` file may name, or nothing.
fn checked(top: &Path, git_dir: &Path, check: Check) -> Result<(), &'static str> {
    if a_chats_to_make(top, git_dir) {
        return Err(CHANGED);
    }
    if let Some(clone) = cut_from(top) {
        // A folder purlis cut is the piece its clone keeps an entry for, and nothing else:
        // the one rule, whoever asks.
        return held_to(&clone.join(".git"), top, git_dir);
    }
    if let Some(common) = worktree_of(git_dir) {
        if !commondir_is(git_dir, &common) {
            return Err(CHANGED);
        }
        return match check == Check::Moved || names_back(git_dir, top) {
            true => Ok(()),
            false => Err(MOVED),
        };
    }
    if a_module_of_the_enclosing_repository(top, git_dir) || !below_a_workspace(top) {
        return Ok(());
    }
    Err(CHANGED)
}

/// Whether `top` is below a workspace: `…/workspaces/<name>` or deeper. That is where purlis
/// starts chats and keeps the clones and branch folders they work in.
pub fn below_a_workspace(top: &Path) -> bool {
    let mut parts = top.components().map(|part| part.as_os_str());
    parts.any(|part| part == "workspaces") && parts.next().is_some()
}

/// Why the folder `top`, whose `.git` file names `git_dir`, is not the piece of that name the
/// clone whose git directory is `clones` keeps, or nothing: [`super::pointer::verified`]'s
/// answer, in the runner's sentences. A link that does name the clone's entry for it, where
/// the rest does not hold, is a folder that was moved, most often.
pub fn held_to(clones: &Path, top: &Path, git_dir: &Path) -> Result<(), &'static str> {
    let piece = top
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(CHANGED)?;
    match super::pointer::verified(clones, top, piece) {
        Ok(own) if own == git_dir => Ok(()),
        Ok(_) => Err(CHANGED),
        Err(_) => {
            let its_entry = std::fs::canonicalize(clones)
                .is_ok_and(|clones| clones.join("worktrees").join(piece) == git_dir);
            Err(if its_entry { MOVED } else { CHANGED })
        }
    }
}

/// Whether `place` is where the chat working in `top` writes: inside `top` itself, or in a
/// temp folder that `top` is not in.
fn a_chats_to_make(top: &Path, place: &Path) -> bool {
    place.starts_with(top)
        || temp_folders()
            .iter()
            .any(|temp| place.starts_with(temp) && !top.starts_with(temp))
}

/// The machine's temp folders, as the kernel names them.
fn temp_folders() -> Vec<PathBuf> {
    let mut all = vec![std::env::temp_dir()];
    all.extend(["/tmp", "/var/tmp"].map(PathBuf::from));
    let mut real: Vec<PathBuf> = all
        .iter()
        .filter_map(|folder| std::fs::canonicalize(folder).ok())
        .collect();
    real.dedup();
    real
}

/// Whether `git_dir` is a submodule's git directory of the repository that encloses `top`:
/// below the `modules` folder of that repository's git directory, which is read by this same
/// check, never taken from the path's shape.
fn a_module_of_the_enclosing_repository(top: &Path, git_dir: &Path) -> bool {
    let Some(above) = top.parent() else {
        return false;
    };
    let enclosing = match of(above, Check::Whole) {
        Ok(Link::Clone { top }) => top.join(".git"),
        // A worktree's modules are its clone's.
        Ok(Link::Linked { git_dir, .. }) => worktree_of(&git_dir).unwrap_or(git_dir),
        _ => return false,
    };
    let Ok(modules) = std::fs::canonicalize(enclosing.join("modules")) else {
        return false;
    };
    git_dir.starts_with(&modules) && git_dir != modules
}

/// The `.git` directory `git_dir` is a worktree of, where its path is
/// `<that>/worktrees/<name>`.
pub fn worktree_of(git_dir: &Path) -> Option<PathBuf> {
    let holder = git_dir.parent()?;
    let common = holder.parent()?;
    (holder.file_name()? == "worktrees" && common.file_name()? == ".git")
        .then(|| common.to_path_buf())
}

/// Whether the worktree's `commondir` names `common`.
fn commondir_is(git_dir: &Path, common: &Path) -> bool {
    let Some(text) = small(&git_dir.join("commondir")) else {
        return false;
    };
    let named = Path::new(text.trim_end_matches(['\n', '\r']));
    let named = if named.is_absolute() {
        named.to_path_buf()
    } else {
        git_dir.join(named)
    };
    same(&named, common)
}

/// Whether the worktree's `gitdir` names `top`'s `.git` file.
fn names_back(git_dir: &Path, top: &Path) -> bool {
    let Some(text) = small(&git_dir.join("gitdir")) else {
        return false;
    };
    let named = Path::new(text.trim_end_matches(['\n', '\r']));
    // Relative to the worktree's git directory, where git was asked to write it so.
    let named = if named.is_absolute() {
        named.to_path_buf()
    } else {
        git_dir.join(named)
    };
    named.file_name().is_some_and(|name| name == ".git")
        && named.parent().is_some_and(|folder| same(folder, top))
}

/// The clone purlis cut `top` from, where `top` is `<workspace>/.worktrees/<repo>/<piece>`.
pub fn cut_from(top: &Path) -> Option<PathBuf> {
    let of_repo = top.parent()?;
    let holder = of_repo.parent()?;
    if holder.file_name()? != super::DIR_NAME {
        return None;
    }
    Some(holder.parent()?.join(of_repo.file_name()?))
}

/// Whether two paths name one place, links resolved. Two that cannot be resolved are not.
fn same(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clone at `<root>/workspaces/alpha/thing` and a worktree of it cut where purlis cuts
    /// one, laid out as git lays them out. No git is run.
    struct Laid {
        _dir: tempfile::TempDir,
        clone: PathBuf,
        tree: PathBuf,
        admin: PathBuf,
    }

    fn laid() -> Laid {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("resolves");
        let clone = root.join("workspaces/alpha/thing");
        let tree = root.join("workspaces/alpha/.worktrees/thing/piece");
        let admin = clone.join(".git/worktrees/piece");
        std::fs::create_dir_all(&admin).expect("the worktree's git directory");
        std::fs::create_dir_all(tree.join("src")).expect("the worktree");
        std::fs::write(admin.join("commondir"), "../..\n").expect("commondir");
        std::fs::write(
            admin.join("gitdir"),
            format!("{}\n", tree.join(".git").display()),
        )
        .expect("gitdir");
        std::fs::write(tree.join(".git"), format!("gitdir: {}\n", admin.display()))
            .expect("the link");
        Laid {
            _dir: dir,
            clone,
            tree,
            admin,
        }
    }

    fn point(tree: &Path, at: &Path) {
        std::fs::write(tree.join(".git"), format!("gitdir: {}\n", at.display())).expect("a link");
    }

    #[test]
    fn a_worktree_git_laid_out_is_read_with_its_own_git_directory_from_any_folder_in_it() {
        let laid = laid();
        let linked = Link::Linked {
            top: laid.tree.clone(),
            git_dir: laid.admin.clone(),
        };
        assert_eq!(of(&laid.tree, Check::Whole), Ok(linked.clone()));
        assert_eq!(of(&laid.tree.join("src"), Check::Whole), Ok(linked));
    }

    #[test]
    fn a_clone_and_a_folder_in_no_repository_are_left_to_git() {
        let laid = laid();
        assert_eq!(
            of(&laid.clone, Check::Whole),
            Ok(Link::Clone {
                top: laid.clone.clone()
            })
        );
        let dir = tempfile::tempdir().expect("a temp dir");
        let plain = dir.path().join("plain");
        std::fs::create_dir_all(&plain).expect("a folder");
        // The temp folder may itself be inside a repository on a developer's machine: only a
        // refusal would be wrong here.
        assert!(of(&plain, Check::Whole).is_ok());
        assert_eq!(of(&plain.join("missing"), Check::Whole), Ok(Link::None));
    }

    #[test]
    fn a_link_rewritten_to_a_git_directory_of_another_name_is_refused() {
        let laid = laid();
        // What a chat can write: a folder holding a `config`, under a name no rule holds.
        let own = laid.tree.join("store");
        std::fs::create_dir_all(own.join("worktrees/piece")).expect("a folder of its own");
        std::fs::write(own.join("config"), "[filter \"x\"]\n\tclean = ./run\n").expect("config");
        std::fs::write(own.join("worktrees/piece/commondir"), "../..\n").expect("commondir");
        std::fs::write(
            own.join("worktrees/piece/gitdir"),
            format!("{}\n", laid.tree.join(".git").display()),
        )
        .expect("gitdir");
        for at in [own.clone(), own.join("worktrees/piece")] {
            point(&laid.tree, &at);
            assert_eq!(
                of(&laid.tree, Check::Whole),
                Err(CHANGED),
                "{}",
                at.display()
            );
            assert_eq!(of(&laid.tree.join("src"), Check::Whole), Err(CHANGED));
        }
    }

    #[test]
    fn a_link_to_another_clone_s_worktree_is_refused_where_purlis_cut_the_folder() {
        let laid = laid();
        // A real worktree's git directory, of a clone that is not the one this was cut from.
        let other = laid.clone.with_file_name("other");
        let admin = other.join(".git/worktrees/piece");
        std::fs::create_dir_all(&admin).expect("another clone");
        std::fs::write(admin.join("commondir"), "../..\n").expect("commondir");
        std::fs::write(
            admin.join("gitdir"),
            format!("{}\n", laid.tree.join(".git").display()),
        )
        .expect("gitdir");
        point(&laid.tree, &admin);
        assert_eq!(of(&laid.tree, Check::Whole), Err(CHANGED));
    }

    #[test]
    fn a_link_to_a_sibling_worktree_s_git_directory_is_refused() {
        let laid = laid();
        // The same clone's other worktree: its git directory names its own folder back.
        let sibling = laid.clone.join(".git/worktrees/sibling");
        std::fs::create_dir_all(&sibling).expect("a sibling");
        std::fs::write(sibling.join("commondir"), "../..\n").expect("commondir");
        std::fs::write(
            sibling.join("gitdir"),
            format!(
                "{}\n",
                laid.tree.with_file_name("sibling").join(".git").display()
            ),
        )
        .expect("gitdir");
        point(&laid.tree, &sibling);
        assert_eq!(of(&laid.tree, Check::Whole), Err(CHANGED));
        // The folder's own entry, whose record names another folder: one that was moved.
        point(&laid.tree, &laid.admin);
        std::fs::write(
            laid.admin.join("gitdir"),
            format!(
                "{}\n",
                laid.tree.with_file_name("was-here").join(".git").display()
            ),
        )
        .expect("gitdir");
        assert_eq!(of(&laid.tree, Check::Whole), Err(MOVED));
        assert!(MOVED.contains("`git worktree repair`"));
        // A folder purlis cut is repaired from its clone's side, so nothing is let through
        // for it from its own.
        assert!(of(&laid.tree, Check::Moved).is_err());
    }

    /// A worktree entry `<holder>/.git/worktrees/piece` that names `tree` back, as git lays
    /// one out.
    fn entry_in(holder: &Path, tree: &Path) -> PathBuf {
        let entry = holder.join(".git/worktrees/piece");
        std::fs::create_dir_all(&entry).expect("an entry");
        std::fs::write(entry.join("commondir"), "../..\n").expect("commondir");
        std::fs::write(
            entry.join("gitdir"),
            format!("{}\n", tree.join(".git").display()),
        )
        .expect("gitdir");
        entry
    }

    #[test]
    fn a_git_directory_inside_the_folder_itself_is_refused_whatever_its_shape() {
        // What a chat can leave in its own folder where a sandbox cannot hold the name: a
        // whole `.git`, with a worktree entry or a module in it, each well formed.
        let laid = laid();
        let worktree = entry_in(&laid.tree.join("own"), &laid.tree);
        let module = laid.tree.join("own/.git/modules/lib");
        std::fs::create_dir_all(&module).expect("a module");
        for at in [worktree, module] {
            point(&laid.tree, &at);
            assert_eq!(
                of(&laid.tree, Check::Whole),
                Err(CHANGED),
                "{}",
                at.display()
            );
            assert_eq!(
                of(&laid.tree, Check::Moved),
                Err(CHANGED),
                "{}",
                at.display()
            );
        }
        // The same in a folder that is no branch folder of purlis's, in a workspace and out.
        for loose in [
            laid.clone.with_file_name("loose"),
            laid.clone.join("../../../outside-any-workspace"),
        ] {
            std::fs::create_dir_all(&loose).expect("a folder");
            let loose = std::fs::canonicalize(&loose).expect("resolves");
            let worktree = entry_in(&loose.join("own"), &loose);
            let module = loose.join("own/.git/modules/lib");
            std::fs::create_dir_all(&module).expect("a module");
            for at in [worktree, module, loose.join("own")] {
                point(&loose, &at);
                assert_eq!(of(&loose, Check::Whole), Err(CHANGED), "{}", at.display());
            }
        }
    }

    #[test]
    fn a_folder_purlis_cut_is_a_worktree_of_its_clone_and_never_a_submodule() {
        let laid = laid();
        // Its own clone's module, and a module of a clone beside it: well formed, and not
        // what a branch folder is.
        for holder in [laid.clone.clone(), laid.clone.with_file_name("other")] {
            let module = holder.join(".git/modules/lib");
            std::fs::create_dir_all(&module).expect("a module");
            point(&laid.tree, &module);
            assert_eq!(
                of(&laid.tree, Check::Whole),
                Err(CHANGED),
                "{}",
                module.display()
            );
        }
    }

    #[test]
    fn a_module_is_one_only_of_the_repository_that_encloses_the_folder() {
        let laid = laid();
        // A folder inside the clone, linked to the clone's own module: a submodule.
        let inside = laid.clone.join("lib");
        std::fs::create_dir_all(&inside).expect("a folder");
        let own = laid.clone.join(".git/modules/lib");
        std::fs::create_dir_all(&own).expect("a module");
        point(&inside, &own);
        assert!(matches!(of(&inside, Check::Whole), Ok(Link::Linked { .. })));
        // The same folder linked to another repository's module, and a folder in no
        // repository linked to any: the path's shape is not enough.
        let theirs = laid.clone.with_file_name("other").join(".git/modules/lib");
        std::fs::create_dir_all(&theirs).expect("a module");
        point(&inside, &theirs);
        assert_eq!(of(&inside, Check::Whole), Err(CHANGED));
        let loose = laid.clone.with_file_name("loose");
        std::fs::create_dir_all(&loose).expect("a folder");
        for module in [own, theirs] {
            point(&loose, &module);
            assert_eq!(
                of(&loose, Check::Whole),
                Err(CHANGED),
                "{}",
                module.display()
            );
        }
    }

    #[test]
    fn a_place_the_folders_chat_writes_is_inside_it_or_in_a_temp_folder_it_is_not_in() {
        let temp = std::fs::canonicalize(std::env::temp_dir()).expect("a temp folder");
        let project = Path::new("/somewhere/project/workspaces/alpha/thing");
        assert!(a_chats_to_make(project, &project.join("store")));
        assert!(a_chats_to_make(project, &temp.join("store")));
        assert!(!a_chats_to_make(
            project,
            Path::new("/somewhere/project.gitdir")
        ));
        // A project that itself lives in a temp folder, as a test's does, is not refused its
        // own neighbours there.
        let in_temp = temp.join("project/top");
        assert!(!a_chats_to_make(&in_temp, &temp.join("project/top.gitdir")));
        assert!(a_chats_to_make(&in_temp, &in_temp.join("store")));
    }

    #[cfg(unix)]
    #[test]
    fn a_record_that_is_a_link_or_no_regular_file_is_not_read() {
        let laid = laid();
        // `commondir` as a link to a file that says the right thing: not followed.
        let said = laid.tree.join("says");
        std::fs::write(&said, "../..\n").expect("a file");
        std::fs::remove_file(laid.admin.join("commondir")).expect("removed");
        std::os::unix::fs::symlink(&said, laid.admin.join("commondir")).expect("a link");
        assert!(of(&laid.tree, Check::Whole).is_err());
        // And a folder standing where the record goes.
        std::fs::remove_file(laid.admin.join("commondir")).expect("removed");
        std::fs::create_dir(laid.admin.join("commondir")).expect("a folder");
        assert!(of(&laid.tree, Check::Whole).is_err());
        assert_eq!(small(&laid.tree), None);
        assert_eq!(small(&laid.tree.join("says")).as_deref(), Some("../..\n"));
    }

    #[test]
    fn a_worktree_whose_commondir_names_another_place_is_refused() {
        let laid = laid();
        let own = laid.tree.join("store");
        std::fs::create_dir_all(&own).expect("a folder of its own");
        std::fs::write(laid.admin.join("commondir"), format!("{}\n", own.display()))
            .expect("commondir");
        assert!(of(&laid.tree, Check::Whole).is_err());
    }

    #[test]
    fn a_link_that_names_nothing_there_says_so_and_one_that_is_not_a_link_is_changed() {
        let laid = laid();
        point(&laid.tree, Path::new("/nonexistent/nowhere"));
        assert_eq!(of(&laid.tree, Check::Whole), Err(GONE));
        for text in ["", "gitdir:\n", "worktree: x\n", "gitdir: a\ngitdir: b\n"] {
            std::fs::write(laid.tree.join(".git"), text).expect("a link");
            assert_eq!(of(&laid.tree, Check::Whole), Err(CHANGED), "{text:?}");
        }
        let long = format!("gitdir: {}\n{}", laid.admin.display(), "\n".repeat(9000));
        std::fs::write(laid.tree.join(".git"), long).expect("a link");
        assert_eq!(of(&laid.tree, Check::Whole), Err(CHANGED));
    }

    #[cfg(unix)]
    #[test]
    fn a_dot_git_that_is_a_link_to_a_folder_is_refused() {
        let laid = laid();
        std::fs::remove_file(laid.tree.join(".git")).expect("removed");
        std::os::unix::fs::symlink(laid.clone.join(".git"), laid.tree.join(".git"))
            .expect("a link");
        assert_eq!(of(&laid.tree, Check::Whole), Err(CHANGED));
    }

    #[test]
    fn outside_a_workspace_a_git_directory_kept_elsewhere_is_a_persons_own_layout() {
        // `git clone --separate-git-dir`: a project's top, or a scratch checkout.
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("resolves");
        let store = root.join("project.gitdir");
        let top = root.join("project");
        std::fs::create_dir_all(&store).expect("the git directory");
        std::fs::create_dir_all(top.join("docs")).expect("the project");
        point(&top, &store);
        let linked = Link::Linked {
            top: top.clone(),
            git_dir: store.clone(),
        };
        assert_eq!(of(&top, Check::Whole), Ok(linked.clone()));
        assert_eq!(of(&top.join("docs"), Check::Whole), Ok(linked));
        // But never one inside the folder itself, which whoever works in it writes.
        let inside = top.join("docs/store");
        std::fs::create_dir_all(&inside).expect("a folder");
        point(&top, &inside);
        assert_eq!(of(&top, Check::Whole), Err(CHANGED));
        point(&top, &store);
        // The same file below a workspace of that project is not followed.
        let clone = top.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).expect("a folder in a workspace");
        point(&clone, &store);
        assert_eq!(of(&clone, Check::Whole), Err(CHANGED));
        let workspace = top.join("workspaces/alpha");
        point(&workspace, &store);
        assert_eq!(of(&workspace, Check::Whole), Err(CHANGED));

        assert!(below_a_workspace(Path::new("/p/workspaces/alpha")));
        assert!(below_a_workspace(Path::new(
            "/p/workspaces/alpha/thing/vendor"
        )));
        assert!(!below_a_workspace(Path::new("/p/workspaces")));
        assert!(!below_a_workspace(Path::new("/p")));
        assert!(!below_a_workspace(Path::new("/tmp/x-workspaces-y/clone")));
    }

    #[test]
    fn a_submodule_s_link_into_its_superproject_is_read() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("resolves");
        let module = root.join("super/.git/modules/lib");
        let tree = root.join("super/lib");
        std::fs::create_dir_all(&module).expect("the module");
        std::fs::create_dir_all(&tree).expect("the submodule");
        std::fs::write(tree.join(".git"), "gitdir: ../.git/modules/lib\n").expect("a link");
        assert_eq!(
            of(&tree, Check::Whole),
            Ok(Link::Linked {
                top: tree.clone(),
                git_dir: module
            })
        );
        // The folder that holds the modules is not one, where that matters: in a workspace.
        let inside = root.join("workspaces/alpha/super");
        std::fs::create_dir_all(inside.join(".git/modules/lib")).expect("the module");
        std::fs::create_dir_all(inside.join("lib")).expect("the submodule");
        std::fs::write(inside.join("lib/.git"), "gitdir: ../.git/modules/lib\n").expect("a link");
        assert!(matches!(
            of(&inside.join("lib"), Check::Whole),
            Ok(Link::Linked { .. })
        ));
        std::fs::write(inside.join("lib/.git"), "gitdir: ../.git/modules\n").expect("a link");
        assert_eq!(of(&inside.join("lib"), Check::Whole), Err(CHANGED));
    }

    #[test]
    fn a_worktree_outside_purlis_s_layout_is_held_to_the_clone_its_link_names() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("resolves");
        let admin = root.join("elsewhere/.git/worktrees/loose");
        let tree = root.join("workspaces/alpha/loose");
        std::fs::create_dir_all(&admin).expect("the git directory");
        std::fs::create_dir_all(&tree).expect("the worktree");
        std::fs::write(admin.join("commondir"), "../..\n").expect("commondir");
        std::fs::write(
            admin.join("gitdir"),
            format!("{}\n", tree.join(".git").display()),
        )
        .expect("gitdir");
        point(&tree, &admin);
        assert_eq!(
            of(&tree, Check::Whole),
            Ok(Link::Linked {
                top: tree.clone(),
                git_dir: admin
            })
        );
        assert_eq!(cut_from(&tree), None);
        assert_eq!(
            cut_from(&root.join("workspaces/alpha/.worktrees/thing/piece")),
            Some(root.join("workspaces/alpha/thing"))
        );
    }
}
