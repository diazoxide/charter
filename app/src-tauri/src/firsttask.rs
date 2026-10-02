//! The first task's runs, as the window starts them (FR-28, #621).
//!
//! The script is `charter_core::firsttask`'s: the task's text, what each run is called and the
//! command that shows its diff. This module starts one run: a chat on the profile the operator
//! picked for it, **on a branch of its own** cut off the workspace's clone (GL-1), with the task
//! typed into it once its harness has started **and never sent** — the curation action's typed
//! start (ADR 0061), so the operator reads the task before pressing Enter.
//!
//! A run is started only in a repo's clone: the point of the task is two branches whose diffs
//! can be put side by side, and a chat anywhere else would have no branch to show.

use std::path::Path;
use std::sync::Arc;

use charter_core::engine::Size;
use charter_core::firsttask;
use charter_core::harness::{Harness, ReadyToType};

use crate::curation::{self, ChatTyped};
use crate::planes::{Held, PlaneId, Planes};

/// A run of the first task that started: what the window needs to put its tab on the strip and
/// to open its diff.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FirstTaskRun {
    pub session: u32,
    /// The chat's name, which is its number.
    pub name: String,
    /// What its tab says, which also named its branch: `first task 1`.
    pub label: String,
    pub persona: Option<String>,
    /// The harness, by the word the plane calls it.
    pub harness: Option<String>,
    /// The workspace it is filed under.
    pub workspace: Option<String>,
    /// The branch it works on.
    pub branch: String,
    /// The branch's folder, where its diff is shown.
    pub folder: String,
    /// The command that shows the run's diff, run in `folder`.
    pub diff: String,
}

/// Starts run `run` (1 or 2) of the first task in the repo clone at `cwd`, on `profile`, as
/// `persona`, with the task typed and unsent.
// Its plane is a `PlaneId` the registry vouches for, like every other command's. Not a doc
// comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn first_task_run(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    cwd: String,
    profile: String,
    persona: Option<String>,
    run: u8,
    columns: u16,
    rows: u16,
) -> Result<FirstTaskRun, String> {
    let held = planes.held(&plane)?;
    tauri::async_runtime::spawn_blocking(move || {
        start(
            &held,
            Path::new(&cwd),
            profile,
            persona,
            run,
            Size { columns, rows },
        )
    })
    .await
    .map_err(|why| format!("charter could not start the chat: {why}"))?
}

/// [`first_task_run`], against a plane the registry has already vouched for.
pub fn start(
    held: &Arc<Held>,
    cwd: &Path,
    profile: String,
    persona: Option<String>,
    run: u8,
    size: Size,
) -> Result<FirstTaskRun, String> {
    if !(1..=firsttask::RUNS).contains(&run) {
        return Err(format!(
            "The first task has {} chats, so chat {run} was not started.",
            firsttask::RUNS
        ));
    }
    let root = held.root().to_path_buf();
    if charter_core::chatpiece::clone_at(&root, cwd).is_none() {
        return Err(format!(
            "{} is not a repo's clone in this project, so the first task has no branch to work \
             on and nothing was started.",
            cwd.display()
        ));
    }
    let label = firsttask::label(run);
    let runner = persona.clone();
    let ((started, branch, folder, diff), _said) =
        crate::worktrees::on_a_branch_cut(&root, Some(cwd), Some(&label), true, |dir, cut| {
            let cut = cut.ok_or("charter cut no branch for the chat, so nothing was started.")?;
            // Asked before the chat starts, while the branch is still exactly what it was cut
            // from; a refusal here takes the branch back like any other.
            let from = firsttask::cut_from(&cut.path)?;
            let started = curation::start_typed(
                held,
                ChatTyped {
                    profile,
                    persona,
                    cwd: dir.unwrap_or_else(|| cut.path.clone()),
                    label: label.clone(),
                    prompt: firsttask::prompt(),
                },
                size,
                cannot_type,
                |why| format!("{why}, so the first task was not typed and nothing was started."),
            )?;
            Ok((
                started,
                cut.branch.clone(),
                cut.path.clone(),
                firsttask::diff_command(&from),
            ))
        })?;
    let workspace = charter_core::workspaces::Plane::open(&root)
        .workspace_of(started.ready.cwd.as_deref().unwrap_or(folder.as_path()));
    Ok(FirstTaskRun {
        session: started.session,
        name: started.name,
        label,
        persona: runner,
        harness: started.ready.harness.map(|h| h.name().to_owned()),
        workspace,
        branch,
        folder: folder.display().to_string(),
        diff,
    })
}

/// When a run's harness can have the task typed into it, or why not.
fn cannot_type(profile: &str, harness: Option<Harness>) -> Result<ReadyToType, String> {
    match harness {
        Some(harness) => harness.ready_to_type().ok_or_else(|| {
            format!(
                "'{profile}' starts {}, which goes quiet while it is still starting, so \
                 charter has no moment to type the first task into it. Start this chat with \
                 Claude Code or Codex.",
                harness.title()
            )
        }),
        None => Err(format!(
            "'{profile}' starts a program charter has not measured, so it cannot tell when \
             to type the first task into it."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size {
        columns: 80,
        rows: 24,
    };

    /// A project with a workspace `shop` holding one clone, `shop`, with one commit, and a
    /// profile `work` of `kind` running a stand-in harness that waits on its input.
    struct Project {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
        clone: std::path::PathBuf,
    }

    impl Project {
        fn new(kind: &str) -> Self {
            let dir = tempfile::tempdir().expect("a directory");
            let root = std::fs::canonicalize(dir.path()).expect("it resolves");
            std::fs::write(root.join(charter_core::plane::MANIFEST), "schema = 1\n").unwrap();
            let clone = root.join("workspaces/shop/shop");
            std::fs::create_dir_all(&clone).unwrap();
            std::fs::write(root.join("workspaces/shop/workspace.md"), "# shop\n").unwrap();
            git(&clone, &["init", "-q", "-b", "main", "."]);
            std::fs::write(clone.join("README.md"), "# shop\n").unwrap();
            git(&clone, &["add", "-A"]);
            git(&clone, &["commit", "-q", "-m", "start"]);
            let program = stand_in::program(
                &root,
                "harness-stand-in",
                "#!/bin/sh\nstty raw -echo\nexec cat > /dev/null\n",
            );
            std::fs::write(
                root.join(charter_core::profiles::LOCAL_FILE),
                format!(
                    "[harness]\ndefault = \"work\"\n\n[harness.work]\nkind = {kind:?}\n\
                     command = [{:?}]\n",
                    program.display().to_string()
                ),
            )
            .unwrap();
            let set = charter_core::profiles::current(&root);
            let work = set.get("work").expect("the profile reads");
            charter_core::profiletrust::record_launched(
                &root,
                "work",
                &charter_core::profiletrust::fingerprint(work),
            )
            .expect("approved");
            Self {
                _dir: dir,
                root,
                clone,
            }
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let ran = charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "user.email=t@e.invalid", "-c", "user.name=t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args),
        )
        .unwrap();
        assert!(ran.status.success(), "git {args:?}: {ran:?}");
    }

    fn planes() -> Planes {
        Planes::telling(Arc::new(|_| {}), crate::Shipped::default(), None)
    }

    #[cfg(unix)]
    #[test]
    fn a_run_starts_on_its_own_branch_with_the_task_held_to_type_and_says_how_to_see_its_diff() {
        let project = Project::new("claude");
        let planes = planes();
        let id = planes.open(&project.root);
        let held = planes.held(&id).expect("held");

        let run = start(&held, &project.clone, "work".into(), None, 1, SIZE).expect("it starts");

        assert_eq!(run.label, "first task 1");
        assert_eq!(run.branch, "first-task-1");
        assert_eq!(run.harness.as_deref(), Some("claude"));
        assert_eq!(run.workspace.as_deref(), Some("shop"));
        // Against the commit the branch was cut from, not the clone's moving `main`.
        let head = charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&project.clone)
                .args(["rev-parse", "HEAD"]),
        )
        .unwrap();
        let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
        assert_eq!(run.diff, format!("git add -N -A && git diff {head} --"));
        let folder = Path::new(&run.folder);
        assert!(folder.join("README.md").is_file(), "{}", run.folder);
        let opened = held
            .chats()
            .open_now()
            .into_iter()
            .find(|open| open.session == run.session)
            .expect("the app has it open");
        assert_eq!(opened.cwd.as_deref(), Some(folder));
        assert_eq!(opened.label.as_deref(), Some("first task 1"));
        assert!(held.typed().waiting(run.session), "held until it starts");
        held.close_chat(run.session).unwrap();
    }

    #[test]
    fn a_run_anywhere_but_a_repo_s_clone_starts_nothing() {
        let project = Project::new("claude");
        let planes = planes();
        let id = planes.open(&project.root);
        let held = planes.held(&id).expect("held");

        let refused = start(
            &held,
            &project.root.join("workspaces/shop"),
            "work".into(),
            None,
            1,
            SIZE,
        )
        .unwrap_err();

        assert!(refused.contains("is not a repo's clone"), "{refused}");
        assert!(held.chats().open_now().is_empty());
    }

    #[test]
    fn a_run_on_opencode_starts_nothing_and_leaves_no_branch() {
        let project = Project::new("opencode");
        let planes = planes();
        let id = planes.open(&project.root);
        let held = planes.held(&id).expect("held");

        let refused = start(&held, &project.clone, "work".into(), None, 2, SIZE).unwrap_err();

        assert!(refused.contains("with Claude Code or Codex"), "{refused}");
        assert!(held.chats().open_now().is_empty());
        assert!(
            !project
                .root
                .join("workspaces/shop/.worktrees/shop/first-task-2")
                .exists(),
            "the branch's folder was taken back"
        );
        let branches = charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&project.clone)
                .args(["branch", "--list", "first-task-2"]),
        )
        .unwrap();
        assert!(branches.stdout.is_empty(), "the branch was taken back");
    }

    #[test]
    fn there_is_no_third_run() {
        let project = Project::new("claude");
        let planes = planes();
        let id = planes.open(&project.root);
        let held = planes.held(&id).expect("held");

        let refused = start(&held, &project.clone, "work".into(), None, 3, SIZE).unwrap_err();

        assert!(refused.contains("has 2 chats"), "{refused}");
    }
}
