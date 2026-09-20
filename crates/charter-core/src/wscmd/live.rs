//! `charter workspace live [--off]` — the one switch that decides whether a workspace's
//! memory is committed.
//!
//! A port of `commands_workspace.cmd_workspace_live`.
//!
//! **The direction, spelled out, because it is the thing to get wrong.** `charter workspace
//! live <ws>` makes a workspace **LIVE**: its `workspace.json`, `workspace.md`, `memory/`,
//! `todos/` and `changes/` stop being ignored, so they are committed and shared. `charter
//! workspace live <ws> --off` makes it **LOCAL**: those paths are ignored again and nothing
//! is committed. LOCAL is the default a workspace is created in, and "live" is the verb
//! rather than the state — `live --off` reads as "the live switch, off", not as "make it
//! live, off".
//!
//! # `--off` untracks before it re-ignores, and that order is the whole command
//!
//! Re-ignoring a path git already **tracks** changes nothing: git keeps tracking a file it
//! has in the index whatever `.gitignore` says. So `--off` first runs
//! `git rm -r --cached -q -- <paths>` — which drops the files from the index and **keeps them
//! on disk** — and only then rewrites the managed block. The other order leaves a workspace
//! the operator just made private still committed, and the next `charter save` publishes its
//! memory.
//!
//! It does **not** commit. The untracking is staged and the operator is told to finish it
//! with `charter save`, because a command that made a workspace private and then pushed the
//! deletion of its memory to a shared remote is doing two things under one name.
//!
//! # Turning it ON scaffolds first
//!
//! `charter workspace live <ws>` calls `workspace.scaffold` before it flips the block, so a
//! workspace whose `memory/MEMORY.md` was never created is not made LIVE with nothing to
//! share. **That scaffold has no port** ([`crate::wscmd`]'s gap list: the harness layer), so
//! this command says what it did not do rather than doing half of it — see [`live`].

use std::path::Path;

use crate::repocmd::{Say, Sink};
use crate::wscmd;

/// `charter workspace live <name> [--off]`, and its exit code.
///
/// `off` is the `--off` flag: `false` makes the workspace LIVE, `true` makes it LOCAL.
pub fn live(root: &Path, name: &str, off: bool, say: Sink) -> u8 {
    if wscmd::workspace_dir(root, name).is_none() {
        say(Say::Fail(format!(
            "invalid workspace name '{}' (use letters, digits, '.', '_', '-'; must not start \
             with a dot)",
            crate::personas::one_line(name)
        )));
        return 1;
    }
    if !wscmd::workspace_dir_exists(root, name) {
        say(Say::Fail(format!(
            "no workspace '{name}' (create it: charter workspace create {name})"
        )));
        return 1;
    }
    if off {
        return local(root, name, say);
    }
    match wscmd::set_live(root, name, true) {
        Ok(false) => say(Say::Info(format!("Workspace '{name}' is already LIVE."))),
        Ok(true) => say(Say::Done(format!(
            "Workspace '{name}' is now LIVE — manifest + memory are committed + shared + \
             auto-saved."
        ))),
        Err(why) => {
            say(Say::Fail(format!(
                "could not write the plane's .gitignore, so '{name}' is unchanged ({why})."
            )));
            return 1;
        }
    }
    // **Said, because this charter did not scaffold.** Python's `live` runs
    // `workspace.scaffold(name)` first, so a workspace with no `memory/MEMORY.md` gets one
    // before it is shared. This binary has no workspace scaffold yet, and a LIVE workspace
    // with nothing to commit is a `charter save` that says "Nothing to save" for a reason the
    // operator cannot see.
    if !root
        .join("workspaces")
        .join(name)
        .join("memory")
        .join(crate::memstore::INDEX)
        .exists()
    {
        say(Say::Warn(format!(
            "'{name}' has no memory/{} yet, and this charter does not create one — it is \
             LIVE with nothing to share. The Python charter's `charter workspace live {name}` \
             scaffolds it; so does `charter workspace remember`.",
            crate::memstore::INDEX
        )));
    }
    say(Say::Info(format!(
        "Record its repos: charter workspace snapshot {name}  ·  share: charter workspace \
         save {name}"
    )));
    0
}

/// `--off`: untrack what the plane is committing for this workspace, then re-ignore it.
fn local(root: &Path, name: &str, say: Sink) -> u8 {
    let paths = wscmd::meta_paths(root, name);
    if !paths.is_empty() {
        // `-r`, because `memory/`, `todos/` and `changes/` are directories; `--cached`,
        // because the files stay on disk — this makes the workspace private, it does not
        // delete anybody's notes. Through the hardened runner like every other git charter
        // runs: `env_clear`, no hooks, no fsmonitor.
        let mut argv: Vec<&str> = vec!["rm", "-r", "--cached", "-q", "--"];
        argv.extend(paths.iter().map(String::as_str));
        match crate::worktree::git::run(root, &argv, crate::worktree::git::READ) {
            // Python ignores this call's exit code, and so does this: `git rm --cached` on a
            // path git never tracked exits non-zero, which is the ordinary case for a
            // workspace that was LIVE and had nothing staged yet. What must not happen is the
            // block being rewritten while the files stay tracked, and that is the ORDER
            // below, not this code.
            Ok(_) => {}
            Err(why) => {
                say(Say::Fail(format!(
                    "could not run git to untrack '{name}'s shared files, so nothing was \
                     changed and it is still LIVE ({why})."
                )));
                return 1;
            }
        }
    }
    if let Err(why) = wscmd::set_live(root, name, false) {
        say(Say::Fail(format!(
            "'{name}'s files were untracked, but the plane's .gitignore could not be written, \
             so it is still recorded LIVE ({why}). Re-run this command."
        )));
        return 1;
    }
    say(Say::Done(format!(
        "Workspace '{name}' is now LOCAL (private). Its manifest + memory are no longer \
         committed. Finalize the untracking: charter save"
    )));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repocmd::Say;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        dir
    }

    fn workspace(root: &Path, name: &str) {
        std::fs::create_dir_all(root.join("workspaces").join(name).join("memory")).unwrap();
        std::fs::write(
            root.join("workspaces")
                .join(name)
                .join("memory")
                .join(crate::memstore::INDEX),
            "# Memory Index\n",
        )
        .unwrap();
    }

    fn run(root: &Path, name: &str, off: bool) -> (u8, Vec<String>) {
        let mut said = Vec::new();
        let code = live(root, name, off, &mut |line: Say| said.push(line.to_string()));
        (code, said)
    }

    #[test]
    fn on_makes_the_workspace_live_and_off_makes_it_local() {
        let dir = plane();
        workspace(dir.path(), "beta");

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0);
        assert!(crate::wscmd::live_workspaces(dir.path()).contains("beta"), "{said:?}");
        assert!(said.iter().any(|l| l.contains("is now LIVE")), "{said:?}");

        let (code, said) = run(dir.path(), "beta", true);
        assert_eq!(code, 0);
        assert!(!crate::wscmd::live_workspaces(dir.path()).contains("beta"), "{said:?}");
        assert!(said.iter().any(|l| l.contains("is now LOCAL")), "{said:?}");
    }

    #[test]
    fn turning_it_on_twice_says_so_and_changes_nothing() {
        let dir = plane();
        workspace(dir.path(), "beta");
        run(dir.path(), "beta", false);
        let before = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();

        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0);
        assert!(said.iter().any(|l| l.contains("already LIVE")), "{said:?}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            before
        );
    }

    #[test]
    fn a_workspace_this_plane_does_not_have_is_refused_and_nothing_is_written() {
        let dir = plane();
        let (code, said) = run(dir.path(), "nope", false);
        assert_eq!(code, 1);
        assert_eq!(said, vec!["✗ no workspace 'nope' (create it: charter workspace create nope)"]);
        assert!(!dir.path().join(".gitignore").exists(), "nothing was written");
    }

    #[test]
    fn a_name_that_cannot_be_a_workspace_is_refused_before_any_path_is_built() {
        let dir = plane();
        for name in ["../../esc", ".hidden", "Bad Name"] {
            let (code, said) = run(dir.path(), name, false);
            assert_eq!(code, 1, "{name}");
            assert!(said[0].contains("invalid workspace name"), "{name}: {said:?}");
        }
        assert!(!dir.path().join(".gitignore").exists());
    }

    #[test]
    fn a_newline_in_a_refused_name_cannot_forge_a_line_of_the_refusal() {
        let dir = plane();
        let (_code, said) = run(dir.path(), "a\nb", false);
        assert!(said[0].contains("'a\\x0ab'"), "{said:?}");
        assert_eq!(said.len(), 1, "one line, not two: {said:?}");
    }

    #[test]
    fn off_leaves_the_files_on_disk() {
        let dir = plane();
        workspace(dir.path(), "beta");
        run(dir.path(), "beta", false);
        run(dir.path(), "beta", true);
        assert!(
            dir.path()
                .join("workspaces/beta/memory")
                .join(crate::memstore::INDEX)
                .exists(),
            "`git rm --cached` keeps the working tree; making a workspace private is not \
             deleting anybody's notes"
        );
    }

    #[test]
    fn a_live_workspace_with_no_memory_index_is_told_it_has_nothing_to_share() {
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("workspaces").join("beta")).unwrap();
        let (code, said) = run(dir.path(), "beta", false);
        assert_eq!(code, 0);
        assert!(
            said.iter().any(|l| l.contains("nothing to share")),
            "{said:?}"
        );
    }
}
