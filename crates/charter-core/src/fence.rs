//! The tree a test build may act in, and the death when it leaves that tree.
//!
//! **charter-app#129.** `charter-app` is checked out at `workspaces/ide/charter-app`, which
//! is *inside* the operator's own control plane. [`crate::plane::find_root`] walks up, finds
//! it, and answers with it — correctly, because that is what "the plane this directory sits
//! in" means. Nothing about that walk knows it is running under a test, so a test process
//! started anywhere in this checkout resolves the operator's live plane and acts on it. On
//! 2026-09-21 that put 49 of the scenario suite's chats into
//! `/Users/aharon/IdeaProjects/charter/.charter/app/reopen.json`, and the operator found
//! them by opening the app: 49 dead tabs, every one failing on a binary that no longer
//! existed.
//!
//! **It is a trust-model hole, not only a nuisance.** The reopen record is an execution
//! input (ADR 0035): putting it back STARTS the programs it names, before any window and
//! with nothing to click. A suite that can write that file in a real plane can write
//! programs into it.
//!
//! **What the fence establishes.** A *fenced* build — one compiled with the `fenced` cargo
//! feature, which only a test build turns on — carries a tree it is allowed to act in, and
//! **dies** the moment it resolves, opens, reads or writes a plane outside that tree. It
//! does not quietly answer `None`, does not fall back, and does not leave the caller a value
//! to ignore: the run is over and the message says which plane and which fence.
//!
//! That is deliberately stronger than "the suite pins `$CHARTER_ROOT`". Pinning is how a run
//! *avoids* the write, and a suite that can reach the operator's plane and merely chooses
//! not to is one refactor away from reaching it again — the refactor lands green and the
//! damage shows up in somebody's app a week later. Under the fence that refactor is a red
//! run on the first execution, in the job that made it.
//!
//! **The fence is never in a shipped build.** `hold` compiles to nothing at all without the
//! feature, and nothing in the shipping graph enables it: it is turned on by
//! `[dev-dependencies]` (so every `cargo test` build has it, and no `cargo build --release`
//! does) and by the app's own `e2e` feature (so the binary the scenario tests drive has it).

use std::path::{Path, PathBuf};

/// The variable that names the tree a fenced process may act in, as a path list in the
/// platform's own spelling (`:` on unix, `;` on Windows) — [`std::env::split_paths`] reads
/// it, so it is spelled the way `$PATH` is.
///
/// Unset, the fence is this machine's temporary directory, which is where every fixture in
/// this repository already lives: `tempfile::tempdir()` in Rust, `mkdtempSync(tmpdir())` in
/// the scenario harness. So no test has to opt in, and a test that reaches past its own
/// fixture is the only thing that notices.
pub const VAR: &str = "CHARTER_PLANE_FENCE";

/// Whether this build carries the fence, for anything that wants to say so out loud.
pub const FENCED: bool = cfg!(feature = "fenced");

/// What a fenced process was about to do, for the line it dies on.
///
/// Named separately rather than collapsed into one, because they fail differently: a
/// [`Self::Resolve`] names a plane everything downstream will then use, a [`Self::Read`]
/// takes an execution input out of it, a [`Self::Write`] leaves one behind, an
/// [`Self::Open`] attaches the whole of a plane's live state — board, chats and hook socket
/// — and a [`Self::Store`] is not about a plane at all. Whoever reads the message wants to
/// know which of those had already happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Naming it as the plane this process acts on.
    Resolve,
    /// Reading its reopen record, which says what to run.
    Read,
    /// Writing its reopen record, which says what the next launch runs.
    Write,
    /// Attaching it: its board, its chats and its hook socket.
    Open,
    /// Keeping this machine's store there — which projects charter remembers, and which the
    /// operator has approved (ADR 0034). It is not in a plane; it is under
    /// `$CHARTER_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config`, so a run that pins
    /// no config home of its own writes its throwaway projects and their trust into the
    /// operator's own — the same class of damage as charter-app#129, one ladder over.
    Store,
}

impl Act {
    /// The verb and what it takes, for the refusal.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Resolve => "resolve the plane",
            Self::Read => "read the reopen record of the plane",
            Self::Write => "write the reopen record of the plane",
            Self::Open => "open the plane",
            Self::Store => "keep this machine's charter store at",
        }
    }
}

/// The tree this process may act in: `$CHARTER_PLANE_FENCE` if it names one, else this
/// machine's temporary directory.
///
/// The entries come back as the environment spells them, and [`inside`] is what resolves
/// them — so a fence handed in by a test and a fence read from the environment are judged by
/// one rule rather than by two that can drift.
pub fn fence() -> Vec<PathBuf> {
    match std::env::var_os(VAR).filter(|value| !value.is_empty()) {
        Some(value) => std::env::split_paths(&value)
            .filter(|entry| !entry.as_os_str().is_empty())
            .collect(),
        None => vec![std::env::temp_dir()],
    }
}

/// Whether `root` is inside `fence` — at one of its entries, or below one.
///
/// **Both sides are canonicalised**, because they are spelled differently on the machine
/// that found the bug: macOS's `/tmp` is a link to `/private/tmp` and its `$TMPDIR` is a
/// `/var/folders/…` path under `/private/var`, so a fence written one way and a plane
/// resolved the other are one directory and have to compare equal. A path that does not
/// resolve is kept as written rather than dropped — a fence naming a directory the run has
/// not made yet is a fence, not a mistake, and dropping it would silently widen the fence.
///
/// `starts_with` is the *component-wise* one [`Path::starts_with`] gives and never a string
/// prefix: the scenario harness makes `charter-scenario` and `charter-scenario-plane-daily`
/// side by side, and a fence of the first must not admit the second.
///
/// An empty fence admits nothing. That is what `$CHARTER_PLANE_FENCE=":"` deserves —
/// somebody meant to name a tree and named none — and it fails closed.
pub fn inside(root: &Path, fence: &[PathBuf]) -> bool {
    // PROOF ONLY, never merge — charter-app#129's guard, mutated so that every plane reads as
    // inside the fence and `hold` never refuses.
    let _ = (root, fence);
    true
}

/// What the process says on its way out.
///
/// Split from [`hold`] so that the words are testable without a build that dies to read
/// them, and so that the one place they are written is the one place they are read.
pub fn refusal(act: Act, root: &Path, fence: &[PathBuf]) -> String {
    let listed = fence
        .iter()
        .map(|entry| format!("    {}", entry.display()))
        .collect::<Vec<_>>()
        .join("\n");
    let listed = if listed.is_empty() {
        "    (nothing: the fence names no directory at all)".to_owned()
    } else {
        listed
    };
    format!(
        "charter: a fenced build refused to {verb}\n    {root}\n  \
         because this process is fenced to\n{listed}\n  and that is outside it.\n\n  \
         A fenced build is a TEST build, and a test must never act on a plane the run did \
         not make.\n  This is charter-app#129: a test run resolved the operator's live plane \
         — the checkout sits\n  inside it — and left 49 of the suite's chats in its reopen \
         record, which is an execution\n  input (ADR 0035): the next launch of that plane \
         starts what the test left behind.\n\n  Pin the plane this run means with \
         ${root_var} and its store with ${store_var}, or widen the fence\n  with \
         ${fence_var}.\n",
        verb = act.verb(),
        root = root.display(),
        root_var = "CHARTER_ROOT",
        store_var = crate::machine::HOME_VAR,
        fence_var = VAR,
    )
}

/// Refuses `root` unless it is inside the fence — and a refusal ends the process.
///
/// In a build without the `fenced` feature this is not a check that passes; it is nothing at
/// all, inlined away, so shipped charter carries neither the cost nor the behaviour.
///
/// It **panics**, and the message goes to standard error first. The panic is what fails the
/// test that reached too far, names it, and — in the app, whose panic hook writes the same
/// block to `$CHARTER_PANIC_LOG` before the process aborts — survives a window that has
/// nowhere to print. Returning an error instead was the other candidate and is the wrong
/// one: every caller of [`crate::plane::resolve`] in this workspace already has an arm for
/// "no plane here", so an error would have been handled, logged at most, and the run would
/// have gone green.
#[cfg(feature = "fenced")]
pub fn hold(act: Act, root: &Path) {
    let fence = fence();
    if inside(root, &fence) {
        return;
    }
    let said = refusal(act, root, &fence);
    // Before the panic, because the panic hook is not always installed and this message is
    // the whole point of the mechanism.
    let _ = std::io::Write::write_all(&mut std::io::stderr(), said.as_bytes());
    panic!("{said}");
}

/// [`hold`], in a build that is not fenced: nothing.
#[cfg(not(feature = "fenced"))]
#[inline(always)]
pub fn hold(_act: Act, _root: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plane_below_the_fence_is_inside_it() {
        let dir = tempfile::tempdir().expect("a directory");
        let fence = vec![dir.path().to_path_buf()];
        let plane = dir.path().join("a").join("plane");
        std::fs::create_dir_all(&plane).expect("the plane's directory");

        assert!(inside(&plane, &fence));
        assert!(inside(dir.path(), &fence), "the fence itself is inside it");
    }

    #[test]
    fn a_plane_beside_the_fence_is_outside_it() {
        let dir = tempfile::tempdir().expect("a directory");
        let fence = vec![dir.path().join("run")];
        std::fs::create_dir_all(dir.path().join("run")).expect("the run's tree");
        let elsewhere = dir.path().join("somebody-elses-plane");
        std::fs::create_dir_all(&elsewhere).expect("a plane beside it");

        assert!(!inside(&elsewhere, &fence));
    }

    #[test]
    fn a_sibling_whose_name_merely_begins_with_the_fence_s_is_outside_it() {
        // `starts_with` on a string would admit this one, and the scenario harness makes
        // exactly this pair of names: `charter-scenario` beside `charter-scenario-plane-…`.
        let dir = tempfile::tempdir().expect("a directory");
        let fence = vec![dir.path().join("charter-scenario")];
        std::fs::create_dir_all(dir.path().join("charter-scenario")).expect("the fence");
        let sibling = dir.path().join("charter-scenario-plane-daily");
        std::fs::create_dir_all(&sibling).expect("the sibling");

        assert!(!inside(&sibling, &fence));
    }

    #[test]
    fn the_two_sides_are_compared_after_the_links_in_them_are_followed() {
        // The machine this bug was found on spells its temporary directory two ways:
        // `/tmp` is a link to `/private/tmp`, and a fence written one way with a plane
        // resolved the other is one directory, not two.
        let dir = tempfile::tempdir().expect("a directory");
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("plane")).expect("the real tree");
        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).expect("a link to it");
        #[cfg(not(unix))]
        std::os::windows::fs::symlink_dir(&real, &link).expect("a link to it");

        assert!(
            inside(&link.join("plane"), &[real]),
            "a plane reached through a link is the plane it points at"
        );
    }

    #[test]
    fn a_fence_that_names_nothing_admits_nothing() {
        let dir = tempfile::tempdir().expect("a directory");

        assert!(
            !inside(dir.path(), &[]),
            "an empty fence must fail closed, not open"
        );
    }

    #[test]
    fn the_refusal_names_the_plane_the_fence_and_what_was_about_to_happen() {
        let said = refusal(
            Act::Write,
            Path::new("/Users/o/plane"),
            &[PathBuf::from("/tmp/run")],
        );

        assert!(said.contains("/Users/o/plane"), "{said}");
        assert!(said.contains("/tmp/run"), "{said}");
        assert!(said.contains("write the reopen record of"), "{said}");
        assert!(said.contains("CHARTER_ROOT"), "{said}");
        assert!(said.contains(VAR), "{said}");
        assert!(said.contains("charter-app#129"), "{said}");
    }

    #[test]
    fn every_act_says_something_different_about_what_was_about_to_happen() {
        // The machine store is not a plane and the reopen record is not the whole of one, so
        // a single sentence for all five would describe four of them wrongly. Written as a
        // list so that a new act cannot be added without a verb of its own.
        let verbs = [Act::Resolve, Act::Read, Act::Write, Act::Open, Act::Store].map(Act::verb);
        let mut seen: Vec<&str> = verbs.to_vec();
        seen.sort_unstable();
        seen.dedup();

        assert_eq!(seen.len(), verbs.len(), "two acts read the same: {verbs:?}");
        assert!(
            refusal(Act::Store, Path::new("/home/o/.config"), &[]).contains("machine's charter"),
            "the store's refusal calls it a plane"
        );
    }

    #[test]
    fn a_fence_naming_a_directory_that_is_not_there_is_kept_rather_than_dropped() {
        // Dropping it would leave the fence empty, and an empty fence that came from
        // somebody's typo must refuse everything rather than admit everything.
        let dir = tempfile::tempdir().expect("a directory");
        let missing = dir.path().join("not-made-yet");

        let fence = vec![missing.clone()];
        assert!(!inside(dir.path(), &fence));
        assert!(inside(&missing, &fence));
    }
}
