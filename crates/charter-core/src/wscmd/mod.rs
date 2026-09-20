//! The `charter workspace` verbs that act on a workspace as a whole, and the plane state
//! they read and write.
//!
//! A port of `charter/commands_workspace.py` and the halves of `charter/workspace.py` those
//! verbs use. What is here is the set that can be ported **whole**; what is not is named in
//! [`crate::wscmd`]'s own gap list below, because a verb ported halfway is worse than one
//! that is plainly absent — an operator who runs it gets a workspace in a state neither
//! charter agrees about.
//!
//! # What is here
//!
//! | Verb | Module |
//! |---|---|
//! | `workspace live [--off]` | [`live`] |
//! | `workspace remove` | [`remove`] |
//! | `workspace use`, `workspace unlock` | [`select`] |
//! | `workspace default` | [`select`] |
//! | `workspace snapshot` | [`snapshot`] |
//! | `workspace create` | [`create`] |
//! | `workspace reinit` | [`reinit`] |
//!
//! # What is NOT here, and what it would take
//!
//! - **`workspace fork`.** The scaffold it shares with `create` is ported
//!   ([`crate::wslayer`]); what is not is `_carry` — copying a parent workspace's charter,
//!   memory and manifest into the fork, and cutting each recorded repo's clone onto a branch
//!   of its own. That is a git verb per repo, not a scaffold.
//! - **A guest CHECKOUT inside a workspace.** [`reinit`] and [`create`] wire the workspace
//!   DIRECTORY; a clone or a linked worktree under it is a git root of its own and needs
//!   [`crate::guest`]'s half — the `.git/info/exclude` block and the four row states that
//!   report on it (`unhidden`, `unlisted`, `unrecorded`, `withheld`), none of which has a
//!   port. [`reinit`] names each such checkout rather than letting "up to date" stand over
//!   one.
//! - **`workspace rename`/`mv`.** The move itself is three lines; what it cannot skip is
//!   `git worktree repair` for every linked worktree of every clone that moved
//!   (charter#963 — git calls a live worktree prunable after the move, and `gc` then deletes
//!   its admin directory while it holds uncommitted work) and the LIVE commit of the tracked
//!   move. Neither has a port.
//! - **`workspace restore`.** Needs `charter clone` per missing repo plus a credentialed
//!   `git pull` per restored branch; the clone half is [`crate::repocmd::clone`] and the pull
//!   half is not ported.
//!
//! # Where this is stricter than Python, on purpose
//!
//! - **Every name is checked before it is joined onto a path**, the pointer files included.
//!   `Path::join` throws the prefix away when handed an absolute path and `..` walks out of
//!   the plane (charter#442).
//! - **A clone charter could not read is work at risk**, which is Python's rule (charter#917)
//!   and is restated here because it is the one that gates a `remove`.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use crate::repocmd::{Say, Sink};

pub mod create;
pub mod ensure;
pub mod live;
pub mod reinit;
pub mod remove;
pub mod select;
pub mod snapshot;

/// One sentence for a workspace directory charter could not look at.
///
/// `workspace.cannot_check_workspace` — the sentence `reinit` and every command that shows
/// the plane's workspaces share, so no two of them send a reader to different repairs for one
/// directory.
///
/// The name is printed as it stands, which is what Python prints. It is a directory entry, so
/// it can hold anything; making it readable here would be this port answering a question
/// charter has not answered.
pub fn cannot_check_workspace(at: &Path, code: Option<i32>) -> String {
    let name = at.file_name().unwrap_or_default().to_string_lossy();
    let dir = at.display().to_string();
    format!(
        "workspace '{name}' cannot be checked — charter changes nothing it cannot see; {}.",
        crate::memstore::uncheckable_fix(code, &dir, &dir)
    )
}

/// The delimiters of the managed `.gitignore` block that records which workspaces are LIVE.
///
/// Liveness lives in `.gitignore` and nowhere else, so it is git-visible and travels with
/// the control plane — there is no second file to disagree with it.
pub const LIVE_BEGIN: &str =
    "# >>> charter live workspaces (managed by `charter workspace live`) >>>";
pub const LIVE_END: &str = "# <<< charter live workspaces <<<";

/// The workspaces marked LIVE — `workspace.live_workspaces`.
///
/// A set, and sorted: the block is rewritten from it, and a set with an order is what keeps
/// two `set_live` calls from producing two different files for the same plane.
pub fn live_workspaces(root: &Path) -> BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string(root.join(".gitignore")) else {
        return BTreeSet::new();
    };
    let mut out = BTreeSet::new();
    let mut inside = false;
    for line in crate::mdsection::split_lines(&text) {
        let line = crate::memstore::py_strip(line);
        if line == LIVE_BEGIN {
            inside = true;
        } else if line == LIVE_END {
            inside = false;
        } else if inside && let Some(name) = live_line(line) {
            out.insert(name);
        }
    }
    out
}

/// The workspace a line inside the managed block un-ignores, or `None`.
///
/// Python's `re.match(r"!/workspaces/([^/]+)/workspace\.json", s)` — `match`, so a PREFIX,
/// which is why the tail is only required to START with `workspace.json`. Faithful rather
/// than tightened: the two implementations must read one plane's `.gitignore` the same way.
fn live_line(line: &str) -> Option<String> {
    let rest = line.strip_prefix("!/workspaces/")?;
    let (name, tail) = rest.split_once('/')?;
    (!name.is_empty() && tail.starts_with("workspace.json")).then(|| name.to_string())
}

/// The managed block un-ignoring every LIVE workspace's shareable paths.
///
/// Each path is listed **twice** — the directory and its contents — because un-ignoring
/// `…/memory` alone re-includes the directory entry and none of the files inside it.
///
/// `changes` needs the pair **and a third line that re-ignores `changes/log`**, and that
/// asymmetry is the design of the store rather than an exception to it: a change record holds
/// intent, which is what a teammate needs and git cannot derive, while
/// `changes/log/<host>.jsonl` holds a past-tense declaration carrying merge shas, appended
/// per host without a lock, and is committed **never**. Re-ignoring works only because its
/// parent was re-included two lines above — git cannot re-include a file whose parent
/// directory is excluded — which is why the three lines are written together.
pub fn live_block<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let mut lines = vec![LIVE_BEGIN.to_string()];
    let mut sorted: Vec<&str> = names.into_iter().collect();
    sorted.sort_unstable();
    for n in sorted {
        for line in [
            format!("!/workspaces/{n}/workspace.json"),
            format!("!/workspaces/{n}/workspace.md"),
            format!("!/workspaces/{n}/memory"),
            format!("!/workspaces/{n}/memory/**"),
            format!("!/workspaces/{n}/todos"),
            format!("!/workspaces/{n}/todos/**"),
            format!("!/workspaces/{n}/changes"),
            format!("!/workspaces/{n}/changes/**"),
            format!("/workspaces/{n}/changes/log/"),
        ] {
            lines.push(line);
        }
    }
    lines.push(LIVE_END.to_string());
    lines.join("\n")
}

/// Rewrite the managed block for exactly `names`, creating it when it is absent.
///
/// The three placements are Python's `_write_live_block`, in its order: replace the block
/// that is there, else put one straight after the `!/workspaces/.gitkeep` line `charter init`
/// writes (so the un-ignores sit with the rule they qualify), else append.
pub fn write_live_block<'a>(
    root: &Path,
    names: impl IntoIterator<Item = &'a str>,
) -> io::Result<()> {
    let path = root.join(".gitignore");
    // The file charter is about to write, gated as ITSELF: a `.gitignore` symlinked out of
    // the plane would otherwise take this write with it.
    crate::contain::no_link_on_the_way(root, &path)?;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let block = live_block(names);
    let next = if text.contains(LIVE_BEGIN) {
        replace_every_block(&text, &block)
    } else if let Some(at) = text.find("!/workspaces/.gitkeep\n") {
        let cut = at + "!/workspaces/.gitkeep\n".len();
        format!("{}{block}\n{}", &text[..cut], &text[cut..])
    } else {
        // `text.rstrip("\n")` — newlines alone, which is what Python strips here. A trailing
        // space in somebody's `.gitignore` is theirs and is kept.
        format!("{}\n{block}\n", text.trim_end_matches('\n'))
    };
    std::fs::write(&path, next)
}

/// Every `BEGIN … END` span in `text`, replaced by `block` —
/// `re.sub(BEGIN.*?END, block, text, flags=DOTALL)`.
///
/// **EVERY span, not the first**, and the differential is what found that: Python's `re.sub`
/// takes `count=0` by default, which means all. A plane whose `.gitignore` holds two managed
/// blocks — one the fixture carries and one a hand or an older charter added — had the second
/// left behind by a `live --off`, so the workspace the operator had just made private was
/// still un-ignored by the block further down, and the next `charter save` would have
/// committed its memory. Non-greedy, so two blocks in a row are two spans and not one.
///
/// A BEGIN with no END after it matches nothing and is left exactly as it is, which is what
/// `re.sub` does: half a managed block is something an operator edited, and this is not the
/// command that repairs it.
fn replace_every_block(text: &str, block: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(LIVE_BEGIN) {
        let Some(offset) = rest[start..].find(LIVE_END) else {
            break;
        };
        let end = start + offset + LIVE_END.len();
        out.push_str(&rest[..start]);
        out.push_str(block);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Mark `name` LIVE or LOCAL; `true` when the liveness actually changed.
pub fn set_live(root: &Path, name: &str, live: bool) -> io::Result<bool> {
    let names = live_workspaces(root);
    if names.contains(name) == live {
        return Ok(false);
    }
    let mut next = names;
    if live {
        next.insert(name.to_string());
    } else {
        next.remove(name);
    }
    write_live_block(root, next.iter().map(String::as_str))?;
    Ok(true)
}

/// A LIVE workspace's shareable paths, plane-relative — what `live --off` and `save` hand to
/// git (`commands_workspace._ws_meta_paths`).
///
/// **Filtered by existence deliberately**: these go to git as literal paths, and
/// `git rm --cached` on one that was never tracked fails the whole call — taking the manifest
/// and the memory down with a workspace that simply had no todos.
///
/// `changes/` is asked a sharper question, because for it existence is not a safe proxy for
/// the one being asked: `todos/` is born with its index and is never empty afterwards, while
/// a `changes/` can be emptied by `charter change forget` and one holding nothing but the
/// never-committed `changes/log/` is the same case. Both are "exists, nothing tracked", which
/// is exactly the shape that fails the whole call.
pub fn meta_paths(root: &Path, name: &str) -> Vec<String> {
    let dir = root.join("workspaces").join(name);
    let mut out: Vec<String> = ["workspace.json", "workspace.md", "memory", "todos"]
        .into_iter()
        .filter(|rel| dir.join(rel).exists())
        .map(|rel| format!("workspaces/{name}/{rel}"))
        .collect();
    if has_change_records(&dir) {
        out.push(format!("workspaces/{name}/changes"));
    }
    out
}

/// Whether this workspace holds at least one change record — `change.has_records`.
fn has_change_records(workspace_dir: &Path) -> bool {
    let Ok(reader) = std::fs::read_dir(workspace_dir.join("changes")) else {
        return false;
    };
    reader
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().ends_with(".json"))
}

/// Whether the plane has a directory for `name`.
///
/// `symlink_metadata` and not `exists()`: a dangling link at `workspaces/<name>` is not a
/// workspace and is not nothing either — `exists()` answers false for one, and a `create`
/// that believed it would then write through the link.
pub fn workspace_dir_exists(root: &Path, name: &str) -> bool {
    root.join("workspaces")
        .join(name)
        .symlink_metadata()
        .is_ok()
}

/// The workspace's directory, name-checked first.
pub fn workspace_dir(root: &Path, name: &str) -> Option<PathBuf> {
    crate::contain::workspace_name_ok(name).then(|| root.join("workspaces").join(name))
}

// ----------------------------------------------------------------------------------------
// work that removing a workspace would destroy
// ----------------------------------------------------------------------------------------

/// One reason a workspace holds work that removing it would discard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtRisk {
    /// What to look at: a clone's name, or `<repo>/<piece>` for a worktree.
    pub what: String,
    /// The sentence, already joined to the name: `alpha: 2 unpushed commit(s)`.
    pub said: String,
}

impl std::fmt::Display for AtRisk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.said)
    }
}

/// Clones **and worktrees** in the workspace holding uncommitted or unpushed work —
/// `commands_workspace._work_at_risk`.
///
/// **Only that.** Open todos are deliberately not here, though `remove` reports them: this
/// list is what is *unrecoverable*, and a todo is a note about the future, not work that
/// ceases to exist. A workspace whose todos were all abandoned is precisely the one worth
/// deleting, and making that case demand `--force` would teach the habit of reaching for
/// `--force` — which is how a guard stops protecting the commits it exists for.
///
/// **A clone charter could not read is at risk by definition** (charter#917), and this is the
/// sharpest instance of that rule in the whole plane: what this list is empty of is what
/// [`remove`] hands to a recursive delete. Reading a `git status` that FAILED as "no
/// uncommitted changes" makes a clone charter could not look at indistinguishable from one it
/// looked at and found empty — and then deletes it.
pub fn work_at_risk(root: &Path, ws: &str) -> Vec<AtRisk> {
    let mut out = Vec::new();
    let found = match crate::repos::clones(root, ws) {
        Ok(found) => found,
        // The workspace directory itself could not be read or is not contained. Nothing below
        // can be checked, so the whole workspace is at risk rather than empty.
        Err(why) => {
            return vec![AtRisk {
                what: ws.to_string(),
                said: format!("{ws}: could not be read — {why}"),
            }];
        }
    };
    // A clone charter refused to look at is one it cannot say anything about, and `remove`
    // is about to delete it. Said, never dropped — `repos::clones`' own rule.
    for (name, why) in &found.refused {
        out.push(AtRisk {
            what: name.clone(),
            said: format!("{name}: could not be read — {why}"),
        });
    }
    for repo in &found.repos {
        match crate::repos::state_of(&repo.path) {
            Err(why) => out.push(AtRisk {
                what: repo.name.clone(),
                said: format!("{}: could not be read — {why}", repo.name),
            }),
            Ok(state) if !state.clean() => out.push(AtRisk {
                what: repo.name.clone(),
                said: format!("{}: uncommitted changes", repo.name),
            }),
            // Unpushed only where git names an upstream — a branch with none is not "ahead
            // of" anything, and charter does not call a fresh local branch work at risk.
            Ok(state) if state.upstream.is_some() && state.ahead > 0 => out.push(AtRisk {
                what: repo.name.clone(),
                said: format!("{}: {} unpushed commit(s)", repo.name, state.ahead),
            }),
            Ok(_) => {}
        }
    }
    out.extend(worktrees_at_risk(root, ws, &found.repos));
    out
}

/// Worktrees of this workspace holding work that removing it would destroy (charter#91).
///
/// The clone loop cannot see these and never could: it iterates the CLONES, and a clone's
/// `.git` is a directory where a linked worktree's is a FILE. That exclusion is right for
/// counting repos, and it is exactly what left worktrees unguarded while the delete took them
/// anyway.
///
/// **The rule differs from the clone rule, not just the paths.** A worktree is at risk when
/// it holds commits reachable from no other ref — the work that would actually cease to exist
/// — which `charter wt remove` uses too, so the two guards refuse on identical grounds.
/// Keeping them identical is the point: a workspace that removed what a worktree refused to
/// would be the original bug again.
///
/// Not "has no upstream", which is what this was first written as (charter#91) and then
/// narrowed (charter#104): a parallel agent's piece has no upstream from the moment it is
/// created, so that reading refused over pieces with nothing to lose.
///
/// This fires even when the worktree directory lives outside the workspace — a relocated
/// worktree root, which a recursive delete of `workspaces/<ws>` never touches. That is not an
/// oversight: a linked worktree keeps its objects in the CLONE's object store, so removing
/// the clone destroys those commits whether or not the directory survives.
fn worktrees_at_risk(root: &Path, ws: &str, repos: &[crate::repos::Repo]) -> Vec<AtRisk> {
    let mut out = Vec::new();
    for repo in repos {
        let pieces = match crate::worktree::list(root, ws, &repo.name) {
            Ok(pieces) => pieces,
            // git would not list them, so charter does not know what is there. Named in the
            // shape this function already uses for its other "could not be checked".
            Err(why) => {
                out.push(AtRisk {
                    what: repo.name.clone(),
                    said: format!("{}: could not be checked for worktrees — {why}", repo.name),
                });
                continue;
            }
        };
        for piece in pieces {
            let label = format!("{}/{}", repo.name, piece.piece);
            // A registration whose directory is gone holds nothing.
            if piece.prunable.is_some() {
                continue;
            }
            match crate::worktree::dirt_of(&piece.path) {
                crate::worktree::Dirt::Unknown => {
                    out.push(AtRisk {
                        what: label.clone(),
                        said: format!("{label}: could not be checked for uncommitted changes"),
                    });
                    continue;
                }
                crate::worktree::Dirt::Dirty => {
                    out.push(AtRisk {
                        what: label.clone(),
                        said: format!("{label}: uncommitted changes"),
                    });
                    continue;
                }
                crate::worktree::Dirt::Clean => {}
            }
            match crate::worktree::unique_commits_of(&piece.path, piece.branch.as_deref()) {
                None => out.push(AtRisk {
                    what: label.clone(),
                    said: format!("{label}: could not be checked for unique commits"),
                }),
                Some(0) => {}
                Some(alone) => out.push(AtRisk {
                    what: label.clone(),
                    said: format!("{label}: {alone} commit(s) that exist nowhere else"),
                }),
            }
        }
    }
    out
}

/// Repos whose current branch would not restore for another engineer —
/// `commands_workspace._restore_blockers`.
///
/// The 'enforce push' guard: a manifest branch is only meaningful if it is actually on the
/// remote. A clone charter could not read blocks too (charter#917) — this list being empty is
/// what lets [`snapshot`] write a manifest that claims to capture reality, and a `git status`
/// that failed contributes the same emptiness as one that found nothing.
pub fn restore_blockers(root: &Path, ws: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(found) = crate::repos::clones(root, ws) else {
        return vec![format!("{ws}: could not be read")];
    };
    for (name, why) in &found.refused {
        out.push(format!("{name}: could not be read — {why}"));
    }
    for repo in &found.repos {
        match crate::repos::state_of(&repo.path) {
            Err(why) => out.push(format!("{}: could not be read — {why}", repo.name)),
            Ok(state) if !state.clean() => {
                out.push(format!("{}: uncommitted changes", repo.name));
            }
            Ok(state) if state.upstream.is_none() => out.push(format!(
                "{}: branch '{}' isn't pushed to a remote",
                repo.name,
                branch_word(&state.head)
            )),
            Ok(state) if state.ahead > 0 => {
                out.push(format!("{}: {} unpushed commit(s)", repo.name, state.ahead));
            }
            Ok(_) => {}
        }
    }
    out
}

/// What `git rev-parse --abbrev-ref HEAD` prints for a tree — `_repo_branch`.
///
/// `HEAD` for a detached one, which is the literal word git prints and is what charter
/// records; an unborn branch has a name and git prints it.
pub fn branch_word(head: &crate::repos::Head) -> String {
    match head {
        crate::repos::Head::Branch(b) | crate::repos::Head::Unborn(b) => b.clone(),
        crate::repos::Head::Detached(_) => "HEAD".to_string(),
    }
}

/// `git config user.name`, or `$USER`, or `unknown` — `_git_user`.
pub fn git_user(root: &Path) -> String {
    if let Ok(run) =
        crate::worktree::git::run(root, &["config", "user.name"], crate::worktree::git::READ)
        && run.ok()
        && !run.line().is_empty()
    {
        return run.line().to_string();
    }
    std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Say a list of reasons under a headline, one per line, the way charter does.
pub(crate) fn say_each(say: Sink, lines: impl IntoIterator<Item = String>) {
    for line in lines {
        say(Say::Fail(format!("  {line}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        dir
    }

    #[test]
    fn the_block_lists_each_path_twice_and_re_ignores_the_landing_log() {
        let block = live_block(["beta"]);
        let lines: Vec<&str> = block.lines().collect();
        assert_eq!(lines[0], LIVE_BEGIN);
        assert_eq!(lines[lines.len() - 1], LIVE_END);
        assert!(block.contains("!/workspaces/beta/memory\n!/workspaces/beta/memory/**"));
        assert!(block.contains("!/workspaces/beta/todos\n!/workspaces/beta/todos/**"));
        assert!(
            block.contains("!/workspaces/beta/changes/**\n/workspaces/beta/changes/log/"),
            "the log is re-ignored, and only after its parent is re-included"
        );
    }

    #[test]
    fn the_block_is_written_where_charter_init_left_room_for_it() {
        let dir = plane();
        std::fs::write(
            dir.path().join(".gitignore"),
            "/workspaces/*/*\n!/workspaces/.gitkeep\nnode_modules/\n",
        )
        .unwrap();
        write_live_block(dir.path(), ["beta"]).unwrap();
        let text = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        let at_gitkeep = text.find("!/workspaces/.gitkeep").unwrap();
        let at_block = text.find(LIVE_BEGIN).unwrap();
        let at_modules = text.find("node_modules/").unwrap();
        assert!(at_gitkeep < at_block && at_block < at_modules, "{text}");
    }

    #[test]
    fn rewriting_replaces_the_block_and_keeps_everything_around_it() {
        let dir = plane();
        write_live_block(dir.path(), ["beta"]).unwrap();
        std::fs::write(
            dir.path().join(".gitignore"),
            format!(
                "before\n{}\nafter\n",
                std::fs::read_to_string(dir.path().join(".gitignore"))
                    .unwrap()
                    .trim()
            ),
        )
        .unwrap();
        write_live_block(dir.path(), ["alpha"]).unwrap();
        let text = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(text.starts_with("before\n"), "{text}");
        assert!(text.ends_with("after\n"), "{text}");
        assert_eq!(text.matches(LIVE_BEGIN).count(), 1, "{text}");
        assert!(!text.contains("/beta/"), "{text}");
        assert!(text.contains("!/workspaces/alpha/workspace.json"), "{text}");
    }

    #[test]
    fn liveness_round_trips_through_the_block_and_nothing_else() {
        let dir = plane();
        assert!(live_workspaces(dir.path()).is_empty());
        assert!(set_live(dir.path(), "beta", true).unwrap());
        assert!(
            !set_live(dir.path(), "beta", true).unwrap(),
            "already LIVE is not a change"
        );
        assert_eq!(
            live_workspaces(dir.path()),
            BTreeSet::from(["beta".to_string()])
        );
        assert!(set_live(dir.path(), "beta", false).unwrap());
        assert!(live_workspaces(dir.path()).is_empty());
        assert!(
            !set_live(dir.path(), "beta", false).unwrap(),
            "already LOCAL is not a change"
        );
    }

    #[test]
    fn a_second_managed_block_is_rewritten_too_and_never_left_behind() {
        // The differential found this: Python's `re.sub` replaces EVERY span, and a first
        // port replaced one. A `live --off` then left the workspace un-ignored by the block
        // further down, so the next `charter save` would have committed its memory.
        let dir = plane();
        let block = live_block(["alpha"]);
        std::fs::write(
            dir.path().join(".gitignore"),
            format!("head\n{block}\nmiddle\n{block}\ntail\n"),
        )
        .unwrap();
        assert_eq!(
            live_workspaces(dir.path()),
            BTreeSet::from(["alpha".to_string()])
        );

        assert!(set_live(dir.path(), "alpha", false).unwrap());
        let text = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(!text.contains("/alpha/"), "{text}");
        assert_eq!(text.matches(LIVE_BEGIN).count(), 2, "{text}");
        assert!(text.starts_with("head\n"), "{text}");
        assert!(text.contains("\nmiddle\n"), "{text}");
        assert!(text.ends_with("\ntail\n"), "{text}");
    }

    #[test]
    fn a_begin_with_no_end_after_it_is_left_exactly_as_it_is() {
        let dir = plane();
        let torn = format!("head\n{LIVE_BEGIN}\n!/workspaces/alpha/workspace.json\n");
        std::fs::write(dir.path().join(".gitignore"), &torn).unwrap();
        // The line is inside an unterminated block, so it still reads as LIVE…
        assert_eq!(
            live_workspaces(dir.path()),
            BTreeSet::from(["alpha".to_string()])
        );
        // …and the rewrite matches nothing, exactly as `re.sub` would.
        set_live(dir.path(), "alpha", false).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            torn
        );
    }

    #[test]
    fn a_line_outside_the_managed_block_does_not_make_a_workspace_live() {
        let dir = plane();
        std::fs::write(
            dir.path().join(".gitignore"),
            "!/workspaces/sneaky/workspace.json\n",
        )
        .unwrap();
        assert!(live_workspaces(dir.path()).is_empty());
    }

    #[test]
    fn the_shareable_paths_are_only_the_ones_that_are_there() {
        let dir = plane();
        let ws = dir.path().join("workspaces").join("beta");
        std::fs::create_dir_all(ws.join("memory")).unwrap();
        std::fs::write(ws.join("workspace.md"), "").unwrap();
        assert_eq!(
            meta_paths(dir.path(), "beta"),
            vec!["workspaces/beta/workspace.md", "workspaces/beta/memory"],
            "a path git was never given is one `git rm --cached` fails the whole call on"
        );
    }

    #[test]
    fn a_changes_directory_with_no_record_in_it_is_not_a_shareable_path() {
        let dir = plane();
        let ws = dir.path().join("workspaces").join("beta");
        std::fs::create_dir_all(ws.join("changes").join("log")).unwrap();
        assert_eq!(meta_paths(dir.path(), "beta"), Vec::<String>::new());
        std::fs::write(ws.join("changes").join("r.json"), "{}").unwrap();
        assert_eq!(
            meta_paths(dir.path(), "beta"),
            vec!["workspaces/beta/changes".to_string()]
        );
    }

    #[test]
    fn a_dangling_link_at_a_workspaces_name_is_not_nothing() {
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("nowhere"),
            dir.path().join("workspaces").join("beta"),
        )
        .unwrap();
        assert!(!dir.path().join("workspaces").join("beta").exists());
        assert!(
            workspace_dir_exists(dir.path(), "beta"),
            "exists() follows the link; a workspace charter would write through is not absent"
        );
    }
}
