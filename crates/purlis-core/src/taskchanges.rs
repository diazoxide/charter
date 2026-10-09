//! What a task changed, told apart from what its siblings changed (#1511, spec #1483; V100-66,
//! V100-68).
//!
//! Two kinds of task, and two answers.
//!
//! **A task on a branch of its own** changed what its branch holds against the branch it was
//! cut from. Nobody else works in its folder, so git's comparison is the task's and no other's
//! ([`crate::files::status`], [`crate::files::what_changed`]), and nothing here is needed for
//! it.
//!
//! **A task that worked in a folder other chats work in too** cannot be told from them by git:
//! a changed file there is whoever's. What sets a task's files apart is which files its own
//! **edit tools wrote** while it ran ([`keeps`], [`Touched`]): the touching hook's line, which
//! says whether its tool writes. A file it only read is never one of them. Those are kept **in
//! memory only** (D-86a): nothing here writes one, so the list is gone when the app is started
//! again, and the window says so. A path is the task's own word, so it is listed only where git
//! also finds it changed, and marked where another chat's edit tools wrote it too: a sibling
//! task's, the asking chat's or any other chat's this app heard from (#1534).
//!
//! **What this cannot see, and the window says so:** a change made by a shell command (a
//! formatter, `sed`, a script), which no file tool names; a change the task already committed
//! in that folder, which git no longer reports as changed there; a file the person or a shell
//! command changed after this task wrote it, which no file tool names and so is not marked; and
//! anything after the app was started again, or of a task forgotten because more than
//! [`CHATS_KEPT`] chats were heard from since.
//!
//! **Two tasks in one folder are warned about when the second starts** ([`sharing`],
//! [`shared`]): there are no file locks (V100-68), so the warning is what the person and the
//! asking chat get, by both tasks' names, with the way out: a branch of its own for one. Tasks
//! of different asking chats count too (#1534), and the person asking from a tab is told in the
//! dialog, before the task starts ([`said_before_asking`]).

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};

use crate::worktree::{self, name};

/// The most paths kept for one chat. A task that touched more keeps its first, and says so.
pub const PATHS_KEPT: usize = 500;

/// The most chats paths are kept for. Past it, the chat first heard from is forgotten. Every
/// chat's writes are kept, not only a task's, so another writer of a task's file can be named.
pub const CHATS_KEPT: usize = 128;

/// The paths one chat's file tools named.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Paths {
    /// The chat's name as it was last shown when its tool named one: what marks a file another
    /// chat's list holds too. The chat's word where the chat named itself.
    pub name: String,
    /// Relative to the project's root, `/`-separated.
    pub paths: BTreeSet<String>,
    /// Whether some were not kept: the chat named more than [`PATHS_KEPT`].
    pub more: bool,
}

/// **The files each chat's file tools named while this app has been running**, by the chat's
/// id. In memory only, bounded in chats and in paths, and never written (D-86a).
#[derive(Debug, Default)]
pub struct Touched {
    by_chat: HashMap<String, Paths>,
    /// The chats, in the order each was first heard from.
    heard: VecDeque<String>,
}

impl Touched {
    /// Chat `chat`, shown as `name`, had a tool name `path`, which is relative to the project's
    /// root.
    pub fn note(&mut self, chat: &str, name: &str, path: String) {
        if !self.by_chat.contains_key(chat) {
            if self.heard.len() >= CHATS_KEPT
                && let Some(oldest) = self.heard.pop_front()
            {
                self.by_chat.remove(&oldest);
            }
            self.heard.push_back(chat.to_owned());
        }
        let kept = self.by_chat.entry(chat.to_owned()).or_default();
        if kept.name != name {
            name.clone_into(&mut kept.name);
        }
        if kept.paths.len() >= PATHS_KEPT && !kept.paths.contains(&path) {
            kept.more = true;
            return;
        }
        kept.paths.insert(path);
    }

    /// What chat `chat`'s tools named, or `None` for a chat none was heard from: one whose
    /// harness reports no file tool, one that named none, or one from before this app started.
    pub fn of(&self, chat: &str) -> Option<&Paths> {
        self.by_chat.get(chat)
    }

    /// The name of every chat but `but` whose tools named `path`, in the order each was first
    /// heard from: who else may have written a file of `but`'s list.
    pub fn also(&self, but: &str, path: &str) -> Vec<String> {
        self.heard
            .iter()
            .filter(|chat| chat.as_str() != but)
            .filter_map(|chat| self.by_chat.get(chat))
            .filter(|kept| kept.paths.contains(path))
            .map(|kept| kept.name.clone())
            .collect()
    }
}

/// Whether a task's Changes list keeps the path on `touching`: only a path an edit tool wrote
/// ([`crate::touching::writes`]). A file a task only read or searched is not its change, and a
/// line from an older hook, which says neither, is read as a read.
pub fn keeps(touching: &crate::hookwire::Touching) -> bool {
    touching.wrote
}

/// `touched`, a path inside `folder` as [`crate::touching::confine`] answers it, as a path
/// relative to the project at `root`. `None` for a folder outside the project.
pub fn in_project(root: &Path, folder: &Path, touched: &str) -> Option<String> {
    let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let folder = real(folder);
    let inside = folder.strip_prefix(real(root)).ok()?;
    let mut parts: Vec<&str> = Vec::new();
    for part in inside.components() {
        match part {
            Component::Normal(name) => parts.push(name.to_str()?),
            _ => return None,
        }
    }
    parts.extend(touched.split('/').filter(|part| !part.is_empty()));
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Where a path of the project lies, as git can compare it: in a repo's own folder or in a
/// branch's folder, and the path inside that.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Placed {
    pub workspace: String,
    pub repo: String,
    /// The branch folder's name; `None` for the repo's own folder.
    pub piece: Option<String>,
    /// The path inside it, `/`-separated.
    pub path: String,
}

/// Where `path`, relative to the project's root, lies: `workspaces/<ws>/<repo>/…` or
/// `workspaces/<ws>/.worktrees/<repo>/<piece>/…`. Path arithmetic and no git; `None` for a
/// path in no repo (the project's own files, a workspace's own folder), and for one whose
/// names cannot be a workspace's, a repo's or a branch folder's.
pub fn placed(path: &str) -> Option<Placed> {
    let mut parts = path.split('/');
    if parts.next()? != "workspaces" {
        return None;
    }
    let workspace = parts.next()?;
    let second = parts.next()?;
    let (repo, piece) = if second == worktree::DIR_NAME {
        (parts.next()?, Some(parts.next()?))
    } else {
        (second, None)
    };
    let rest: Vec<&str> = parts.collect();
    let named = crate::contain::workspace_name_ok(workspace)
        && crate::contain::repo_name_ok(repo)
        && piece.is_none_or(name::piece_name_ok)
        && !rest.is_empty()
        && rest
            .iter()
            .all(|part| !part.is_empty() && *part != ".." && *part != "." && *part != ".git");
    named.then(|| Placed {
        workspace: workspace.to_owned(),
        repo: repo.to_owned(),
        piece: piece.map(str::to_owned),
        path: rest.join("/"),
    })
}

// ---------------------------------------------------------------------------------------
// two tasks in one folder
// ---------------------------------------------------------------------------------------

/// One task that is still open, as the same-folder question reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Working {
    /// The task's name, as its row has it.
    pub name: String,
    /// The folder it started in.
    pub cwd: PathBuf,
}

/// `path` with its links resolved, or as it is where it cannot be.
fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The names of the tasks among `others` that work in `cwd`: the same folder, by its real
/// path. A task given a branch of its own stands in a folder purlis cut for it alone, so it is
/// never one of them.
pub fn sharing(cwd: &Path, others: &[Working]) -> Vec<String> {
    let here = real(cwd);
    others
        .iter()
        .filter(|other| real(&other.cwd) == here)
        .map(|other| other.name.clone())
        .collect()
}

/// A folder two or more tasks of one chat work in at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shared {
    pub folder: PathBuf,
    /// The tasks' names, in the order given: the order they started in.
    pub tasks: Vec<String>,
}

/// Every folder two or more of `tasks` work in, in the order each folder's first task started.
pub fn shared(tasks: &[Working]) -> Vec<Shared> {
    let mut by_folder: Vec<Shared> = Vec::new();
    for task in tasks {
        let folder = real(&task.cwd);
        match by_folder.iter_mut().find(|one| real(&one.folder) == folder) {
            Some(one) => one.tasks.push(task.name.clone()),
            // As the first task there named it: what the window says is its own path.
            None => by_folder.push(Shared {
                folder: task.cwd.clone(),
                tasks: vec![task.name.clone()],
            }),
        }
    }
    by_folder.retain(|one| one.tasks.len() > 1);
    by_folder
}

/// `names`, each quoted and as a line can carry it, joined as a sentence lists them:
/// `'a'`, `'a' and 'b'`, `'a', 'b' and 'c'`.
pub fn listed(names: &[String]) -> String {
    let quoted: Vec<String> = names
        .iter()
        .map(|name| format!("'{}'", crate::shown::short(name)))
        .collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [most @ .., last] => format!("{} and {last}", most.join(", ")),
    }
}

/// What the asking chat is told beside the start of `new`, a task that works in `folder` where
/// `others`, tasks it already asked for, work too: both by name, and the way out.
pub fn told_the_asker(new: &str, others: &[String], folder: &str) -> String {
    let (verb, them) = if others.len() == 1 {
        ("is", "the two")
    } else {
        ("are", "them")
    };
    format!(
        "'{}' works in {} with no branch of its own, and {} {verb} still working there. There \
         are no file locks between tasks, so a file two of them change cannot be told apart \
         afterwards. Give a task a branch of its own with `--in {}` to keep {them} apart",
        crate::shown::short(new),
        crate::shown::short(folder),
        listed(others),
        crate::dispatchplace::WORKTREE,
    )
}

/// What the person reads in the window of `tasks`, two or more tasks of one chat working in
/// `folder`: every one by name, and no flag of a command.
pub fn said_in_window(tasks: &[String], folder: &str) -> String {
    let both = if tasks.len() == 2 { "both" } else { "all" };
    format!(
        "{} {both} work in {} with no branch of their own. There are no file locks between \
         tasks: a file two of them change cannot be told apart afterwards. To keep them \
         apart, ask for one on a branch of its own.",
        listed(tasks),
        crate::shown::short(folder),
    )
}

/// What the person reads in the dialog that asks for a task in `folder`, where `others`, open
/// tasks with no branch of their own, already work: each by name, before anything starts.
pub fn said_before_asking(others: &[String], folder: &str) -> String {
    let (verb, own) = if others.len() == 1 {
        ("is", "its")
    } else {
        ("are", "their")
    };
    format!(
        "{} {verb} already working in {} with no branch of {own} own. There are no file locks \
         between tasks: a file two of them change cannot be told apart afterwards. To keep them \
         apart, choose a branch of its own for this one.",
        listed(others),
        crate::shown::short(folder),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn working(name: &str, cwd: &str) -> Working {
        Working {
            name: name.to_owned(),
            cwd: PathBuf::from(cwd),
        }
    }

    #[test]
    fn a_second_task_in_the_same_folder_is_warned_about_by_both_names() {
        let open = [
            working("talk", "/p/workspaces/alpha"),
            working(
                "on a branch",
                "/p/workspaces/alpha/.worktrees/api/on-a-branch-0000aaaa",
            ),
            working("elsewhere", "/p/workspaces/beta"),
        ];

        let others = sharing(Path::new("/p/workspaces/alpha"), &open);

        assert_eq!(others, ["talk"]);
        assert_eq!(
            told_the_asker("review", &others, "workspaces/alpha"),
            "'review' works in workspaces/alpha with no branch of its own, and 'talk' is still \
             working there. There are no file locks between tasks, so a file two of them \
             change cannot be told apart afterwards. Give a task a branch of its own with `--in \
             worktree` to keep the two apart"
        );
        assert_eq!(
            said_in_window(
                &["talk".to_owned(), "review".to_owned()],
                "workspaces/alpha"
            ),
            "'talk' and 'review' both work in workspaces/alpha with no branch of their own. \
             There are no file locks between tasks: a file two of them change cannot be told \
             apart afterwards. To keep them apart, ask for one on a branch of its own."
        );
    }

    #[test]
    fn a_task_alone_in_its_folder_or_on_a_branch_of_its_own_is_not_warned_about() {
        let open = [
            working(
                "on a branch",
                "/p/workspaces/alpha/.worktrees/api/on-a-branch-0000aaaa",
            ),
            working("elsewhere", "/p/workspaces/beta"),
        ];

        assert_eq!(
            sharing(Path::new("/p/workspaces/alpha"), &open),
            Vec::<String>::new()
        );
        // A second worktree task stands in another folder purlis cut: never the same one.
        assert_eq!(
            sharing(
                Path::new("/p/workspaces/alpha/.worktrees/api/other-0000bbbb"),
                &open
            ),
            Vec::<String>::new()
        );
        assert_eq!(shared(&open), Vec::new());
    }

    #[test]
    fn every_task_in_a_shared_folder_is_named_in_the_order_they_started() {
        let open = [
            working("first", "/p/workspaces/alpha"),
            working("alone", "/p/workspaces/beta"),
            working("second", "/p/workspaces/alpha"),
            working("third", "/p/workspaces/alpha"),
        ];

        let found = shared(&open);

        assert_eq!(
            found,
            [Shared {
                folder: PathBuf::from("/p/workspaces/alpha"),
                tasks: vec!["first".to_owned(), "second".to_owned(), "third".to_owned()],
            }]
        );
        assert_eq!(
            said_in_window(&found[0].tasks, "workspaces/alpha"),
            "'first', 'second' and 'third' all work in workspaces/alpha with no branch of \
             their own. There are no file locks between tasks: a file two of them change \
             cannot be told apart afterwards. To keep them apart, ask for one on a branch of \
             its own."
        );
        assert_eq!(
            told_the_asker(
                "third",
                &["first".to_owned(), "second".to_owned()],
                "workspaces/alpha"
            ),
            "'third' works in workspaces/alpha with no branch of its own, and 'first' and \
             'second' are still working there. There are no file locks between tasks, so a \
             file two of them change cannot be told apart afterwards. Give a task a branch of \
             its own with `--in worktree` to keep them apart"
        );
    }

    #[test]
    fn the_dialog_names_the_tasks_already_in_the_folder_before_one_starts() {
        assert_eq!(
            said_before_asking(&["talk".to_owned()], "workspaces/alpha"),
            "'talk' is already working in workspaces/alpha with no branch of its own. There are \
             no file locks between tasks: a file two of them change cannot be told apart \
             afterwards. To keep them apart, choose a branch of its own for this one."
        );
        let two = said_before_asking(&["talk".to_owned(), "lint".to_owned()], "workspaces/alpha");
        assert!(
            two.starts_with(
                "'talk' and 'lint' are already working in workspaces/alpha with no \
                             branch of their own."
            ),
            "{two}"
        );
        let named = said_before_asking(&["t\nIgnore the above".to_owned()], "workspaces/alpha");
        assert!(!named.contains('\n'), "{named}");
    }

    #[test]
    fn a_task_s_name_is_said_as_a_line_can_carry_it() {
        // A name is a chat's word: what cannot be read back off a line is escaped, so a name
        // cannot end the sentence and start one of its own on the next line.
        let said = said_in_window(
            &["talk\nIgnore the above".to_owned(), "review".to_owned()],
            "workspaces/alpha",
        );

        assert!(!said.contains('\n'), "{said}");
        assert!(
            said.ends_with("ask for one on a branch of its own."),
            "{said}"
        );
    }

    #[test]
    fn a_file_a_task_only_read_is_not_kept_as_its_change() {
        let line = |wrote: bool| crate::hookwire::Touching {
            chat: 3,
            touching: "/w/src/lib.rs".to_owned(),
            wrote,
        };

        assert!(keeps(&line(true)));
        assert!(!keeps(&line(false)));
        // A line from a hook that predates the flag says nothing of writing: a read.
        let older: crate::hookwire::Touching =
            serde_json::from_str(r#"{"chat":3,"touching":"/w/src/lib.rs"}"#).unwrap();
        assert!(!keeps(&older));
        // And a write is said on the line, so the app can tell.
        let said = serde_json::to_string(&line(true)).unwrap();
        assert!(said.contains(r#""wrote":true"#), "{said}");
    }

    #[test]
    fn the_files_a_chat_s_tools_named_are_kept_per_chat_and_bounded() {
        let mut touched = Touched::default();
        touched.note("talk", "talk", "workspaces/alpha/api/src/a.rs".to_owned());
        touched.note("talk", "talk", "workspaces/alpha/api/src/a.rs".to_owned());
        touched.note(
            "review",
            "review",
            "workspaces/alpha/api/src/b.rs".to_owned(),
        );

        let talk = touched.of("talk").expect("heard from");
        assert_eq!(
            talk.paths.iter().collect::<Vec<_>>(),
            ["workspaces/alpha/api/src/a.rs"]
        );
        assert!(!talk.more);
        // A sibling's files are its own, and a chat never heard from has no list at all.
        assert_eq!(touched.of("review").map(|kept| kept.paths.len()), Some(1));
        assert_eq!(touched.of("nobody"), None);

        // A chat that names more than the cap keeps its first, and says there were more.
        for n in 0..PATHS_KEPT + 10 {
            touched.note("flood", "flood", format!("f{n}"));
        }
        let flood = touched.of("flood").expect("heard from");
        assert_eq!(flood.paths.len(), PATHS_KEPT);
        assert!(flood.more);

        // And past the most chats kept, the one first heard from is forgotten.
        for n in 0..CHATS_KEPT {
            touched.note(&format!("chat-{n}"), "a chat", "x".to_owned());
        }
        assert_eq!(touched.of("talk"), None);
        assert!(touched.of(&format!("chat-{}", CHATS_KEPT - 1)).is_some());
    }

    #[test]
    fn every_other_chat_that_wrote_a_file_is_named_and_the_task_itself_is_not() {
        // #1534: not only a sibling task. The asking chat and any other chat this app heard
        // from mark a task's file too, by the name each was last shown under.
        let mut touched = Touched::default();
        let file = "workspaces/alpha/api/src/a.rs";
        touched.note("task", "review", file.to_owned());
        touched.note("asker", "steward 3", file.to_owned());
        touched.note("sibling", "talk", file.to_owned());
        touched.note(
            "elsewhere",
            "lint",
            "workspaces/alpha/api/src/b.rs".to_owned(),
        );
        touched.note(
            "asker",
            "steward 3 renamed",
            "workspaces/alpha/x".to_owned(),
        );

        assert_eq!(touched.also("task", file), ["steward 3 renamed", "talk"]);
        assert_eq!(
            touched.also("sibling", file),
            ["review", "steward 3 renamed"]
        );
        assert_eq!(
            touched.also("task", "workspaces/alpha/none"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_touched_path_is_placed_in_its_repo_or_its_branch_folder_and_nowhere_else() {
        assert_eq!(
            placed("workspaces/alpha/api/src/a.rs"),
            Some(Placed {
                workspace: "alpha".to_owned(),
                repo: "api".to_owned(),
                piece: None,
                path: "src/a.rs".to_owned(),
            })
        );
        assert_eq!(
            placed("workspaces/alpha/.worktrees/api/fix-0000aaaa/src/a.rs"),
            Some(Placed {
                workspace: "alpha".to_owned(),
                repo: "api".to_owned(),
                piece: Some("fix-0000aaaa".to_owned()),
                path: "src/a.rs".to_owned(),
            })
        );
        for nowhere in [
            // The project's own files, and a workspace's.
            "README.md",
            "personas/steward/persona.md",
            "workspaces/alpha",
            "workspaces/alpha/todo.md",
            // A repo's folder itself, and a branch folder itself, name no file.
            "workspaces/alpha/api",
            "workspaces/alpha/.worktrees/api/fix-0000aaaa",
            // Names that climb, and git's own.
            "workspaces/../etc/passwd",
            "workspaces/alpha/api/../../x",
            "workspaces/alpha/api/.git/config",
            "workspaces/alpha/api//a.rs",
        ] {
            assert_eq!(placed(nowhere), None, "{nowhere}");
        }
    }

    #[test]
    fn a_path_a_tool_named_is_said_from_the_project_s_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let folder = root.join("workspaces/alpha");
        std::fs::create_dir_all(&folder).unwrap();

        assert_eq!(
            in_project(&root, &folder, "api/src/a.rs").as_deref(),
            Some("workspaces/alpha/api/src/a.rs")
        );
        // A chat at the project's root.
        assert_eq!(
            in_project(&root, &root, "workspaces/alpha/api/a.rs").as_deref(),
            Some("workspaces/alpha/api/a.rs")
        );
        // A folder outside the project names nothing in it.
        let outside = tempfile::tempdir().unwrap();
        assert_eq!(in_project(&root, outside.path(), "a.rs"), None);
    }
}
