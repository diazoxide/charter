//! The first-task script (FR-28, #621): the guided task FR-1 measures.
//!
//! W10 puts a new user's first ten minutes on **one task, run twice**: the same task given to
//! two chats on their own repo — two harnesses, or one harness and two models — each on a
//! branch of its own, with both diffs openable. And it asks for one "month-two in minute five"
//! moment. The moment this script has is **a lesson carried across harnesses**: the first run
//! records what it learned about the repo, and the second run, on whichever harness, starts
//! with it in its briefing. That needs nothing the first run did not already set up — no
//! vault, no second machine, no forge — and it is the thing a harness on its own cannot do,
//! because the lesson is kept in the project and every harness is briefed from there
//! ([`crate::briefing`]).
//!
//! This module is the script's one copy, so the CI run (`charter-cli`'s
//! `tests/first_task_script.rs`) and the guide a partner follows in the window
//! (`FirstTaskTab.tsx`, through the app's `first_task_run` command) cannot drift apart:
//!
//! - [`prompt`]: the task, typed into each run's chat and never sent (ADR 0061's rule for a
//!   prompt charter types);
//! - [`label`]: what each run's chat is called, which names its branch;
//! - [`diff_command`]: what shows a run's diff, in a shell tab in its branch's folder.
//!
//! **The task writes only in the run's own branch.** Each run's chat starts in a piece cut off
//! the workspace's clone (GL-1), the clone is charter's copy and not the operator's repo
//! (FR-4), and the prompt asks for nothing to be pushed or published.

use crate::worktree::Base;

/// How many runs the script has: the same task, twice.
pub const RUNS: u8 = 2;

/// What run `run` (1 or 2) is called: its chat's label, and so its branch's name
/// (`first-task-1`), by the rule every labelled chat's branch is named by.
pub fn label(run: u8) -> String {
    format!("first task {run}")
}

/// The task, as it is typed into each run's chat.
///
/// Small enough for any repo and any harness, and real: it reads the repo, changes one file,
/// and leaves a fact the next chat can use. The last step is the one the month-two moment
/// rests on, so it names the command a chat records a lesson with, and the persona is the
/// chat's own (`charter persona remember` with one argument resolves it).
///
/// **One line, and short**, because it is typed as one paste and a harness draws a long or
/// many-line paste as a placeholder the operator cannot read before pressing Enter (ADR 0061,
/// amended 2026-09-27; [`crate::harness::Harness::why_drawn_as_a_placeholder`]).
pub fn prompt() -> String {
    "charter's first task, on this chat's own branch: check the memory in your briefing for \
     what an earlier chat learned about this repo, find out how its tests or checks are run, \
     and write that down in a short \"How to check a change\" section of the README (make one \
     if there is none). Change nothing else, and push or publish nothing. Then record the one \
     thing the next chat here should know with `charter persona remember \"<the fact>\"`."
        .to_owned()
}

/// The command that shows a run's diff: everything its branch changed since the branch it was
/// cut from, committed or not, run in the run's folder.
///
/// The command is typed into a shell, and a branch name git accepts can still hold `$`, `;` or
/// a backtick, so the base is quoted as one shell word; the `--` after it keeps it a revision
/// even when a file of the same name is in the tree.
pub fn diff_command(base: &Base) -> String {
    let (Base::Branch(from) | Base::Detached(from)) = base;
    format!("git diff {} --", crate::shellseg::shell_quote(from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_run_is_labelled_by_its_number() {
        assert_eq!(label(1), "first task 1");
        assert_eq!(label(2), "first task 2");
    }

    #[test]
    fn the_task_asks_for_the_lesson_and_for_nothing_to_leave_the_machine() {
        let task = prompt();
        assert!(
            task.contains("`charter persona remember \"<the fact>\"`"),
            "{task}"
        );
        assert!(task.contains("push or publish nothing"), "{task}");
    }

    #[test]
    fn the_task_is_drawn_whole_by_every_harness_charter_types_into() {
        let pasted = crate::curation::pasted(&prompt());
        for harness in crate::harness::Harness::ALL {
            if harness.ready_to_type().is_none() {
                continue;
            }
            assert_eq!(
                harness.why_drawn_as_a_placeholder(&pasted),
                None,
                "{harness:?}"
            );
        }
    }

    #[test]
    fn a_runs_diff_is_against_the_branch_or_the_commit_it_was_cut_from() {
        assert_eq!(
            diff_command(&Base::Branch("main".into())),
            "git diff main --"
        );
        assert_eq!(
            diff_command(&Base::Detached("1a2b3c4".into())),
            "git diff 1a2b3c4 --"
        );
    }

    #[test]
    fn a_branch_name_the_shell_would_read_is_one_quoted_word() {
        assert_eq!(
            diff_command(&Base::Branch("x$(rm -rf ~);y".into())),
            "git diff 'x$(rm -rf ~);y' --"
        );
    }
}
