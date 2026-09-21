//! `charter workspace reinit` — bring a workspace's on-disk structure and charter's layer up
//! to what this version writes.
//!
//! Idempotent and additive: existing content is never touched. `--all` repairs every
//! workspace at once, which is what an operator runs after a charter upgrade.
//!
//! # Why the layer is its own set of lines
//!
//! A workspace whose LAYOUT is current can still hold a `.claude/settings.json` generated
//! before the plane's own settings moved. Two facts, so two reports — and the layer is the
//! one this command silently repaired while printing "nothing to do" until it was said out
//! loud.
//!
//! # The checkouts inside it, too
//!
//! A clone or a linked worktree under the workspace is a git root of its own: a chat started
//! there reads project settings from that directory and the walk for agents and skills stops
//! at the git root, so without charter's layer it runs with none of the plane's ask/deny
//! rules, no persona agents and no `$CHARTER_HARNESS`. [`crate::wslayer::wire`] wires each
//! one and its rows arrive here labelled `<checkout>/<path>`, so this command is the repair
//! for a clone an older charter made as well as for the directory around it.
//!
//! **A checkout's file of the operator's is worded differently, and that is not decoration.**
//! Only a checkout is somebody else's repository, where charter's line in `info/exclude`
//! keeps their file hidden while it is there — so the one true piece of advice is `git add
//! -f`, and "move it aside", which the workspace directory's own row says, would be wrong.
//!
//! # Two counters, because they are two units
//!
//! A workspace can need several repairs — the layer is a row per file, the structure bump is
//! another — so a single counter printed against the number of workspaces said *"Healed 32 of
//! 17 workspace(s); the rest were current."* on a 17-workspace plane. `repaired` is a SET OF
//! NAMES drawn from the names walked, so no arithmetic below is in a position to state a count
//! greater than the total.

use std::collections::BTreeSet;
use std::path::Path;

use crate::repocmd::{Say, Sink};
use crate::wscmd::{self, ensure};
use crate::wslayer::{self, Did, Why};

/// The baseline files a LIVE workspace actually SHARES. Healing only `refs/` or the stamp
/// leaves nothing to commit, and advising `save` then sends the operator to a command that
/// prints "Nothing to save".
const LIVE_SHARED: [&str; 3] = ["workspace.md", "workspace.json", "memory/MEMORY.md"];

/// Which workspaces to repair.
pub enum Scope<'a> {
    /// One, by name.
    One(&'a str),
    /// Every workspace this plane has.
    All,
}

/// Run it, and give back the exit code.
pub fn reinit(root: &Path, scope: Scope, now: chrono::DateTime<chrono::Utc>, say: Sink) -> u8 {
    let plane = crate::workspaces::Plane::open(root);
    let author = ensure::author();
    // Workspaces `--all` could not look at, named here and counted in the closing line. A
    // listing that leaves such a directory out is a walk that never met it, and the line
    // under it said "Up to date — nothing to do" about the whole plane.
    let mut unseen = 0usize;
    let names: Vec<String> = match scope {
        Scope::All => {
            let (names, unread) = plane.read_workspaces().unwrap_or_default();
            for (path, code) in &unread {
                unseen += 1;
                say(Say::Fail(wscmd::cannot_check_workspace(path, *code)));
            }
            names
        }
        Scope::One(name) => {
            let Ok(workspace) = plane.workspace(name) else {
                say(Say::Fail(format!(
                    "invalid workspace name '{}'",
                    crate::personas::one_line(name)
                )));
                return 1;
            };
            // Through the link, and the errno kept: a directory that is a symlink loop is not
            // "no workspace", and telling its operator to restore read access clears no loop.
            match workspace.dir().metadata() {
                Ok(_) => vec![name.to_string()],
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    say(Say::Fail(format!("no workspace '{name}'")));
                    return 1;
                }
                Err(e) => {
                    say(Say::Fail(wscmd::cannot_check_workspace(
                        workspace.dir(),
                        e.raw_os_error(),
                    )));
                    return 1;
                }
            }
        }
    };
    if names.is_empty() && unseen == 0 {
        say(Say::Info("No workspaces to reinitialize.".to_string()));
        return 0;
    }

    let mut repairs = 0usize;
    let mut repaired: BTreeSet<String> = BTreeSet::new();
    // Workspaces whose manifest `reinit` was asked for and could not write. Kept apart from
    // `repaired` so the closing line does not say "nothing to do" — or, worse, "the rest were
    // current" — over an error two lines above it.
    let mut blocked: BTreeSet<String> = BTreeSet::new();
    // Workspaces left holding a state a row names and `reinit` cannot clear: a file charter
    // did not write, one it cannot read or write. "Up to date — nothing to do" printed beside
    // one of them told the operator the plane's rules were in force where they were not.
    let mut unresolved: BTreeSet<String> = BTreeSet::new();

    for name in &names {
        let Ok(workspace) = plane.workspace(name) else {
            continue;
        };
        let dir = workspace.dir().to_path_buf();
        let mut before = wslayer::structure_status(&dir);

        // The layer, written HERE rather than left to the scaffold's own call, because the
        // repair has to report what it DID and only the writer knows that: a `.claude` that
        // cannot be made comes back `blocked`, and reading the pre-state instead would print
        // "wrote it" over a write that never happened. The scaffold below re-runs it and gets
        // `present` for everything, which is what idempotent means.
        let layer: Vec<_> = wslayer::wire(root, &dir)
            .into_iter()
            .filter(|row| row.did != Did::Present)
            .collect();
        wslayer::scaffold(&plane, name, now, &author);
        // Structure is not only what lives inside the workspace directory: which of its paths
        // are SHARED is part of the layout too, and that lives in the managed `.gitignore`
        // block. A plane made LIVE before `todos/` existed lists four paths per workspace and
        // nothing re-runs `set_live` unprompted, so the upgrade command is where it is
        // repaired.
        let live: Vec<String> = wscmd::live_workspaces(root).into_iter().collect();
        let _ = wscmd::write_live_block(root, live.iter().map(String::as_str));

        for row in &layer {
            let rel = &row.rel;
            // Counted off the ROW's own predicates, never off the arm that happens to print:
            // "is this a repair" and "what does this row say" must not be able to disagree,
            // which is what a per-arm `repairs += 1` invites the next arm to get wrong.
            if row.did.is_unresolved() {
                unresolved.insert(name.clone());
            }
            if row.did.is_repair() {
                repairs += 1;
                repaired.insert(name.clone());
            }
            match row.did {
                // Never "remove it" in a checkout: the file is somebody's own in their own
                // repository and stays hidden while it is there, so the one thing to say is
                // how to commit it on purpose. Everywhere else `doctor` says a foreign file
                // stays exactly as it is, and one command may not advise what the other
                // rules out.
                Did::Foreign => match wslayer::checkout_row(&dir, rel) {
                    Some((_, inside)) => say(Say::Warn(format!(
                        "'{name}': {rel} holds content charter did not write — left completely \
                         untouched, and hidden in that checkout while it is there; if this is \
                         your own file and you mean to commit it: git add -f {inside}"
                    ))),
                    None => say(Say::Warn(format!(
                        "'{name}': {rel} was not written by charter — left completely \
                         untouched; charter never overwrites it."
                    ))),
                },
                // Not `foreign` and never "remove it": the approvals in that file are the
                // harness's own, and deleting it to get charter's copy back destroys them.
                Did::HarnessBehind => say(Say::Warn(format!(
                    "'{name}': {rel} is a file charter cannot vouch for as its own write, and \
                     the harness saves its approvals into that file, so charter does not \
                     rewrite it: the plane's machine-local rules it lacks are not in force \
                     there."
                ))),
                Did::Blocked => say(Say::Fail(format!(
                    "'{name}': {rel} could not be written — something is in the way at that \
                     path. charter never deletes or renames existing content."
                ))),
                Did::Unreadable => say(Say::Warn(format!(
                    "'{name}': {rel} cannot be read — left exactly as it is; charter writes \
                     there again only if that file turns out to be exactly what charter last \
                     wrote."
                ))),
                // A sentence of its own: nothing is in the way at that path, so `blocked`'s
                // wording would send the operator looking for an obstruction that is not
                // there. What stopped the write is that the file could not be hidden.
                Did::Withheld => say(Say::Fail(format!(
                    "'{name}': {rel} was not written — charter could not hide it in that \
                     checkout's .git/info/exclude, and a machine-local file it cannot hide \
                     would be committable there."
                ))),
                // charter#1072: every worktree of that clone went unchecked, and "nothing to
                // do" printed over them. Not a repair either, and not one `reinit` can make.
                Did::Unlisted => {
                    let tree = wslayer::checkout_row(&dir, rel).map(|(tree, _)| tree);
                    let (label, fix) = match &tree {
                        Some(tree) => (
                            wslayer::checkout_label(&dir, tree),
                            wslayer::unlisted_fix(tree),
                        ),
                        None => (rel.clone(), String::new()),
                    };
                    say(Say::Warn(format!(
                        "'{name}': git could not list the worktrees of {label}, so none of them \
                         was checked or given charter's layer; {fix}."
                    )));
                }
                // charter#1072: a line left out of a shared exclude because it would hide a
                // file of yours in another checkout reading it. Not a repair — the
                // `wrote`/`refreshed` pair below would call it "wrote .git/info/exclude" —
                // and not one `reinit` can make: the file is yours to commit or move.
                Did::Unhidden => {
                    let why = wslayer::checkout_row(&dir, rel)
                        .map(|(tree, _)| crate::guest::unhidden(root, &tree))
                        .unwrap_or_default();
                    say(Say::Warn(format!(
                        "'{name}': {rel} does not hide all of charter's files in that checkout \
                         — {}.",
                        why.join("; ")
                    )));
                }
                // charter's ruling H: a write charter could not record first is not made, and
                // the lines stay. Not `blocked`: nothing is in the way at that path. With the
                // errno the publish failed with.
                Did::Unrecorded => {
                    let refused = wslayer::checkout_row(&dir, rel)
                        .map(|(tree, _)| crate::guest::unrecorded_fix(root, &tree, "that checkout"))
                        .unwrap_or_default();
                    let tail = if refused.is_empty() {
                        String::new()
                    } else {
                        format!("; {refused}")
                    };
                    say(Say::Warn(format!(
                        "'{name}': {rel} — charter could not publish its record there first, so \
                         it wrote nothing and kept every exclude line it had{tail}."
                    )));
                }
                Did::Removed => {
                    // Its own sentence rather than the `wrote`/`refreshed` pair below, which
                    // would call a deletion "refreshed". A removal is the one repair here
                    // whose CAUSE the operator cannot see in the workspace — the plane stopped
                    // declaring it — so the row has to carry it.
                    say(Say::Done(format!(
                        "Reinitialized '{name}' → removed {rel} — the plane no longer declares \
                         it (charter's harness layer)."
                    )));
                }
                Did::Created | Did::Refreshed => {
                    let what = if row.did == Did::Created {
                        "wrote"
                    } else {
                        "refreshed"
                    };
                    say(Say::Done(format!(
                        "Reinitialized '{name}' → {what} {rel} (charter's harness layer)."
                    )));
                }
                Did::Present => {}
            }
        }

        for found in &before.unreadable {
            // A baseline file charter cannot check is named with what clears it, and the
            // scaffold writes nothing over it. What clears it is the errno's, not one sentence
            // for every cause: "restore read access" for a symlink loop is a repair no
            // permission bit is in the way of.
            unresolved.insert(name.clone());
            // The path is where the errno was MET, which is the rel's own unless a directory
            // above it is what fails — `doctor` names `refs/` where the loop is, and this must
            // not name `refs/README.md`, which is no link at all.
            let stopped = wslayer::stopped_at(&found.path, found.code);
            let shown =
                crate::shown::readable(&stopped.to_string_lossy(), crate::memstore::PATH_LIMIT);
            say(Say::Warn(format!(
                "'{name}': {} cannot be checked — charter writes nothing there it cannot see; \
                 {}.",
                found.rel,
                crate::memstore::uncheckable_fix(found.code, &shown, "that path")
            )));
        }

        for (rel, blocker) in &before.in_the_way {
            // The shapes a path of the layout takes when it is not what the layout needs.
            // "added" would be as wrong as a traceback: the scaffold creates nothing beneath
            // either, and removes neither, so the row says which one was found and the repair
            // that is the operator's to make.
            unresolved.insert(name.clone());
            let fix = match blocker.why {
                Why::Dangling => "is a symlink whose target is not there, and charter writes \
                                  nothing through it; removing or repointing that link clears \
                                  this"
                    .to_string(),
                Why::NotADirectory => "is not a directory, and charter never moves existing \
                                       content; moving it out of the way clears this"
                    .to_string(),
                Why::IsALink => {
                    // Never "repoint it" for a FILE: no target makes a file link one charter
                    // writes through. What it becomes is what the layout has at that path.
                    let real = if blocker.path == dir.join(rel) {
                        "file"
                    } else {
                        "directory"
                    };
                    format!(
                        "is a symlink, and charter writes nothing through one; replacing it \
                         with a real {real} clears this"
                    )
                }
                Why::IsADirectory | Why::NotARegularFile => {
                    "is not a regular file, and charter never moves existing content; moving \
                     it out of the way clears this"
                        .to_string()
                }
            };
            // The stamp is written over, not created, when it is there: "could not be written"
            // is what happened to it, and it is the sentence the version line below gives way
            // to.
            let did = if rel.as_str() == wslayer::STRUCTURE_MARKER {
                "could not be written"
            } else {
                "cannot be created"
            };
            let shown = crate::shown::readable(
                &blocker.path.to_string_lossy(),
                crate::memstore::PATH_LIMIT,
            );
            say(Say::Warn(format!("'{name}': {rel} {did} — {shown} {fix}.")));
        }

        // The BACKFILL half, and the reason it is checked AFTER rather than read off the
        // pre-state: `scaffold_manifest` swallows its own failure, because it runs on a launch
        // path where raising would cost the operator their tab. That is the right trade there
        // and it costs this command its honesty unless the file is looked at again — "added
        // workspace.json" printed over a manifest that is not there is exactly the tick that
        // stops somebody checking.
        let manifest = dir.join("workspace.json");
        let named_in_the_way = before
            .in_the_way
            .iter()
            .any(|(rel, _)| rel.as_str() == "workspace.json");
        // A manifest a row above already NAMES is not written by the scaffold, and that is not
        // a write that failed: "could not be written" beside that row said one link twice, and
        // counted the workspace as a repair that went wrong.
        if !named_in_the_way {
            match manifest.metadata() {
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    blocked.insert(name.clone());
                    before.missing.retain(|rel| *rel != "workspace.json");
                    before.ok = before.missing.is_empty() && before.version >= before.target;
                    say(Say::Fail(format!(
                        "'{name}': workspace.json could not be written — something is in the way \
                         at that path. charter never deletes or renames existing content."
                    )));
                }
                Err(_) => {
                    // An `lstat` that fails proves nothing about the file, and "could not be
                    // written" sends somebody to fix a write.
                    say(Say::Warn(format!(
                        "'{name}': workspace.json cannot be checked — charter cannot say whether \
                         it is there; restoring read access to it clears this."
                    )));
                }
            }
        }

        // The stamp, looked at again for the manifest's reason: a refused stamp write is
        // swallowed, and "added structure v0 → v5" printed over one that was not written — then
        // again on the next run, since the workspace still read as stale.
        let after = wslayer::stamp(&dir).0;
        if after < before.target {
            unresolved.insert(name.clone());
            if !before
                .in_the_way
                .iter()
                .any(|(rel, _)| rel.as_str() == wslayer::STRUCTURE_MARKER)
            {
                say(Say::Warn(format!(
                    "'{name}': {} could not be written, so this workspace still reads as \
                     structure v{after} and stays flagged for reinit.",
                    wslayer::STRUCTURE_MARKER
                )));
            }
            // Files it did add are still reported.
            before.ok = before.missing.is_empty();
        }
        if before.ok {
            continue;
        }
        repairs += 1;
        repaired.insert(name.clone());
        let what = if before.missing.is_empty() {
            format!("structure v{} → v{}", before.version, before.target)
        } else {
            before.missing.join(", ")
        };
        say(Say::Done(format!("Reinitialized '{name}' → added {what}.")));
        if plane.is_live(name) && before.missing.iter().any(|rel| LIVE_SHARED.contains(rel)) {
            say(Say::Info(format!(
                "  '{name}' is LIVE — commit the restored files: charter workspace save {name}"
            )));
        }
    }

    if repaired.is_empty() && blocked.is_empty() && unresolved.is_empty() && unseen == 0 {
        say(Say::Done(format!(
            "Up to date (structure v{}) — nothing to do.",
            wslayer::STRUCTURE_VERSION
        )));
    } else if names.len() + unseen > 1 {
        // Two units, named as two. "Applied N repair(s)" is the number the rows above add up
        // to; "across M of T workspace(s)" is the number an operator checks against the plane
        // they know the size of.
        let stuck = if blocked.is_empty() {
            String::new()
        } else {
            format!("{} could not be repaired; ", blocked.len())
        };
        let left = unresolved.len()
            - unresolved
                .iter()
                .filter(|n| repaired.contains(*n) || blocked.contains(*n))
                .count();
        let kept = if left == 0 {
            String::new()
        } else {
            format!("{left} still hold what the rows above name; ")
        };
        let unchecked = if unseen == 0 {
            String::new()
        } else {
            format!("{unseen} could not be checked; ")
        };
        say(Say::Info(format!(
            "Applied {repairs} repair(s) across {} of {} workspace(s); {stuck}{kept}{unchecked}\
             the rest were current.",
            repaired.len(),
            names.len() + unseen
        )));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(
            dir.path().join(".claude").join("settings.json"),
            r#"{"env":{"CHARTER_HARNESS":"claude-code"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        dir
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        "2026-05-04T11:32:17Z".parse().unwrap()
    }

    fn made(root: &Path, name: &str) {
        ensure::ensure(root, name, now(), "fixture").unwrap();
    }

    fn run(root: &Path, scope: Scope) -> (u8, Vec<String>) {
        let mut lines = Vec::new();
        let code = reinit(root, scope, now(), &mut |s| lines.push(s.to_string()));
        (code, lines)
    }

    #[test]
    fn a_workspace_that_is_current_says_nothing_to_do() {
        let dir = plane();
        made(dir.path(), "gamma");
        let (code, lines) = run(dir.path(), Scope::One("gamma"));
        assert_eq!(code, 0);
        assert_eq!(lines, ["✓ Up to date (structure v5) — nothing to do."]);
    }

    #[test]
    fn a_name_this_plane_does_not_have_is_refused_and_creates_nothing() {
        let dir = plane();
        let (code, lines) = run(dir.path(), Scope::One("nowhere"));
        assert_eq!(code, 1);
        assert_eq!(lines, ["✗ no workspace 'nowhere'"]);
        assert!(!dir.path().join("workspaces/nowhere").exists());
    }

    #[test]
    fn a_missing_baseline_file_is_added_and_reported_by_name() {
        let dir = plane();
        made(dir.path(), "gamma");
        std::fs::remove_file(dir.path().join("workspaces/gamma/refs/README.md")).unwrap();
        let (_code, lines) = run(dir.path(), Scope::One("gamma"));
        assert_eq!(lines, ["✓ Reinitialized 'gamma' → added refs/README.md."]);
        assert!(dir.path().join("workspaces/gamma/refs/README.md").exists());
    }

    #[test]
    fn an_old_stamp_is_bumped_and_the_line_names_both_versions() {
        let dir = plane();
        made(dir.path(), "gamma");
        std::fs::write(
            dir.path().join("workspaces/gamma/.charter-structure"),
            "3\n",
        )
        .unwrap();
        let (_code, lines) = run(dir.path(), Scope::One("gamma"));
        assert_eq!(
            lines,
            ["✓ Reinitialized 'gamma' → added structure v3 → v5."]
        );
    }

    #[test]
    fn a_layer_the_plane_has_moved_past_is_refreshed_and_said_out_loud() {
        let dir = plane();
        made(dir.path(), "gamma");
        std::fs::write(
            dir.path().join(".claude").join("settings.json"),
            r#"{"permissions":{"deny":["Bash(rm -rf *)"]}}"#,
        )
        .unwrap();
        let (_code, lines) = run(dir.path(), Scope::One("gamma"));
        assert_eq!(
            lines,
            [
                "✓ Reinitialized 'gamma' → refreshed .claude/settings.json (charter's harness layer)."
            ]
        );
        let text =
            std::fs::read_to_string(dir.path().join("workspaces/gamma/.claude/settings.json"))
                .unwrap();
        assert!(text.contains("rm -rf"), "{text}");
    }

    #[test]
    fn a_settings_file_charter_did_not_write_is_named_and_never_overwritten() {
        let dir = plane();
        made(dir.path(), "gamma");
        let settings = dir.path().join("workspaces/gamma/.claude/settings.json");
        std::fs::write(&settings, "MINE\n").unwrap();
        let (_code, lines) = run(dir.path(), Scope::One("gamma"));
        assert_eq!(
            lines,
            [
                "! 'gamma': .claude/settings.json was not written by charter — left completely \
                 untouched; charter never overwrites it."
            ]
        );
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "MINE\n");
        // And it is NOT "nothing to do": a row that says the plane's rules are out of force
        // there must not sit under a line saying the plane is current.
        assert!(!lines.iter().any(|l| l.contains("nothing to do")));
    }

    #[test]
    fn all_counts_repairs_and_workspaces_as_two_different_numbers() {
        let dir = plane();
        made(dir.path(), "gamma");
        made(dir.path(), "delta");
        // Two repairs in ONE workspace: a counter that printed repairs against the number of
        // workspaces said "2 of 1".
        std::fs::remove_file(dir.path().join("workspaces/gamma/refs/README.md")).unwrap();
        std::fs::write(
            dir.path().join(".claude").join("settings.json"),
            r#"{"permissions":{"deny":["Bash(rm -rf *)"]}}"#,
        )
        .unwrap();
        let (_code, lines) = run(dir.path(), Scope::All);
        let closing = lines.last().unwrap();
        assert!(
            closing.contains("Applied 3 repair(s) across 2 of 2 workspace(s)"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_baseline_path_with_something_in_the_way_is_named_rather_than_added() {
        let dir = plane();
        made(dir.path(), "gamma");
        std::fs::remove_file(dir.path().join("workspaces/gamma/refs/README.md")).unwrap();
        std::fs::remove_dir(dir.path().join("workspaces/gamma/refs")).unwrap();
        std::fs::write(dir.path().join("workspaces/gamma/refs"), "not a dir\n").unwrap();
        let (_code, lines) = run(dir.path(), Scope::One("gamma"));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("refs/README.md cannot be created")
                    && l.contains("is not a directory")),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.contains("added refs/README.md")));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("workspaces/gamma/refs")).unwrap(),
            "not a dir\n"
        );
    }

    #[test]
    fn a_checkout_inside_the_workspace_is_wired_and_its_rows_are_named_under_it() {
        let dir = plane();
        made(dir.path(), "gamma");
        let clone = dir.path().join("workspaces/gamma/svc");
        std::fs::create_dir_all(clone.join(".git")).unwrap();
        let (_code, lines) = run(dir.path(), Scope::One("gamma"));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("wrote svc/.claude/settings.json")),
            "the whole of M2.24: a chat started in that clone gets the plane's layer — \
             {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("wrote svc/.git/info/exclude")),
            "and it is hidden in the repo charter is a guest in — {lines:?}"
        );
        assert!(
            clone.join(".claude/settings.json").exists(),
            "the row is a report of a write that happened"
        );
        let block = std::fs::read_to_string(clone.join(".git/info/exclude")).unwrap();
        assert!(block.contains("/.claude/settings.json"), "{block}");
    }

    #[test]
    fn a_second_reinit_over_a_wired_checkout_says_there_is_nothing_to_do() {
        // M2.25, and the measurement behind it: with the rows ported, "up to date" is
        // sayable again over a plane that HAS clones. The warning M2.22 shipped fired on
        // every run of every plane with a checkout in it, which is every plane in use.
        let dir = plane();
        made(dir.path(), "gamma");
        let clone = dir.path().join("workspaces/gamma/svc");
        std::fs::create_dir_all(clone.join(".git")).unwrap();
        run(dir.path(), Scope::One("gamma"));
        let (code, lines) = run(dir.path(), Scope::One("gamma"));
        assert_eq!(code, 0);
        assert_eq!(
            lines,
            vec!["✓ Up to date (structure v5) — nothing to do.".to_string()],
            "{lines:?}"
        );
    }

    #[test]
    fn a_file_of_the_operators_in_a_checkout_is_named_with_how_to_commit_it_on_purpose() {
        // A checkout is somebody else's repository, and that is the whole difference: the
        // file stays hidden while it is there, so the only advice that is true is `git add
        // -f`. "Move it aside" is what the workspace directory's own row says, and one
        // command may not advise what the other rules out.
        let dir = plane();
        made(dir.path(), "gamma");
        let clone = dir.path().join("workspaces/gamma/svc");
        std::fs::create_dir_all(clone.join(".git")).unwrap();
        std::fs::create_dir_all(clone.join(".claude")).unwrap();
        std::fs::write(clone.join(".claude/settings.json"), "{\"mine\": true}\n").unwrap();
        let (_code, lines) = run(dir.path(), Scope::One("gamma"));
        assert!(
            lines.iter().any(|l| l
                .contains("svc/.claude/settings.json holds content charter did not write")
                && l.contains("git add -f .claude/settings.json")),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("nothing to do")),
            "and the closing line may not call the plane current over it: {lines:?}"
        );
        assert_eq!(
            std::fs::read_to_string(clone.join(".claude/settings.json")).unwrap(),
            "{\"mine\": true}\n",
            "left exactly as it is"
        );
    }

    /// A plane declaring machine-local rules, a workspace `gamma`, and a checkout in it that
    /// charter has already wired — the only shape the co-written path is reachable from.
    fn a_wired_checkout(dir: &Path) -> std::path::PathBuf {
        std::fs::write(
            dir.join(".claude/settings.local.json"),
            r#"{"permissions":{"deny":["Bash(rm -rf *)"]}}"#,
        )
        .unwrap();
        made(dir, "gamma");
        let clone = dir.join("workspaces/gamma/svc");
        std::fs::create_dir_all(clone.join(".git")).unwrap();
        run(dir, Scope::One("gamma"));
        let local = clone.join(".claude/settings.local.json");
        assert!(local.exists(), "charter writes it in a checkout");
        local
    }

    #[test]
    fn a_machine_local_file_the_harness_added_to_is_current_and_reinit_says_nothing() {
        // `.claude/settings.local.json` is the one path the harness writes into too — "Yes,
        // and don't ask again" lands there — and a workspace directory never wants it,
        // because Claude Code already reads the plane's copy at the git root.
        //
        // **charter's `harness-edited`, and this port called it `harness-behind` until
        // M2.26.** charter's last write IS what the plane wants now, so every rule charter
        // mirrored is still in the file and the harness only added to it: nothing is wrong,
        // and `reinit` says nothing. Measured against the oracle, which answers "Up to date
        // (structure v5) — nothing to do" here.
        let dir = plane();
        let local = a_wired_checkout(dir.path());
        let theirs = "{\"permissions\":{\"allow\":[\"Bash(ls)\"]}}\n";
        std::fs::write(&local, theirs).unwrap();

        let (_code, lines) = run(dir.path(), Scope::One("gamma"));

        assert!(
            lines.iter().any(|l| l.contains("nothing to do")),
            "{lines:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&local).unwrap(),
            theirs,
            "deleting it to get charter's copy back destroys the harness's approvals"
        );
    }

    #[test]
    fn a_machine_local_file_the_harness_rewrote_is_kept_and_never_merged_into() {
        // The other half, and the one that IS a finding: once the plane has moved on since
        // charter's last write, the file the harness keeps no longer holds the plane's
        // machine-local rules — and charter will not merge into it, so they are not in force
        // there and `reinit` says so. Telling the two apart is the whole of the split:
        // reported for both, an operator learns nothing from either.
        let dir = plane();
        let local = a_wired_checkout(dir.path());
        std::fs::write(&local, "{\"permissions\":{\"allow\":[\"Bash(ls)\"]}}\n").unwrap();
        run(dir.path(), Scope::One("gamma"));
        std::fs::write(
            dir.path().join(".claude/settings.local.json"),
            r#"{"permissions":{"deny":["Bash(rm -rf *)","Bash(curl *)"]}}"#,
        )
        .unwrap();

        let (_code, lines) = run(dir.path(), Scope::One("gamma"));

        assert!(
            lines.iter().any(|l| l.contains(
                "svc/.claude/settings.local.json is a file charter cannot vouch for as its own \
                 write"
            )),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("nothing to do")),
            "the plane's machine-local rules are NOT in force there: {lines:?}"
        );
    }

    #[test]
    fn a_checkout_linked_out_of_the_plane_is_refused_whole_and_nothing_in_it_is_written() {
        let dir = plane();
        made(dir.path(), "gamma");
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(outside.path().join(".git")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), dir.path().join("workspaces/gamma/svc"))
            .unwrap();
        let (_code, lines) = run(dir.path(), Scope::One("gamma"));
        let wrote = outside.path().join(".claude").exists();
        assert!(
            !wrote,
            "a tree whose own root has escaped is one charter writes nothing into"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("svc/.git/info/exclude could not be written")),
            "{lines:?}"
        );
    }
}
