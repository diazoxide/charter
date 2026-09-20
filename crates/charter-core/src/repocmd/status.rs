//! `charter status`: where am I, and what is here. Python's `commands.cmd_status`.
//!
//! **This is the command an operator types when something has already gone wrong**, so its
//! output is a contract — with human eyes, and with the agents that read it back. Three
//! decisions in it are the whole command, and each was once the other way:
//!
//! - **It says which rung chose the workspace**, not just which workspace. "Why am I in
//!   `default` again?" is a question a surface that asserts an answer cannot be asked (ADR
//!   0013), and the last rung says *why* nothing answered: a shell with no pane id has no
//!   pointer to fall back on, so every session in it starts on `default` however many times
//!   somebody picks.
//! - **It names the plane it is acting on when that is not the one you are standing in.**
//!   `charter.toml` is tracked, so every clone of a plane is a plane, and `charter clone`
//!   puts clones exactly where the upward walk meets them first (charter #200).
//! - **The table is sized from the values about to be printed**, in cells, by
//!   [`crate::tui`] — two of the three widths this table once had were constants, and a
//!   `node-monorepo` is thirteen characters in a column of twelve.
//!
//! # A tree charter could not read is never drawn as clean
//!
//! The NOTE column asks git two questions per clone and a third only where there is a
//! `.gitmodules`. A `git status` that FAILED writes nothing to stdout, and `""` read as
//! "clean" is charter #917's shape — so the three answers here are `clean`, `dirty` and
//! `unknown`, and `unknown` is what an unreadable tree gets. A submodule that was never
//! initialised leaves an empty directory and a clean `git status`, which is why the third
//! question exists at all (charter #817).
//!
//! # Membership is not presence, and `status` draws presence
//!
//! A repo named in `workspace.json` with nothing on disk is a repo this workspace MEANS to
//! hold ([`crate::repos::declared`]), and the app's panel draws it. `status` does not, and
//! that is Python's answer too: its rows come from the DIRECTORY, so a clone somebody made
//! by hand is a row and a member nobody cloned is not. The differential pins that, because
//! the two lists are a standing invitation to make this command disagree with the panel.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::{Say, Sink};
use crate::forge::{self, py_str};
use crate::tui;
use crate::worktree::git;
use crate::{inventory, plane, repos, workspaces::Plane};

/// Where stdout goes. `status` prints its table AS it reads each clone, because a workspace
/// with twenty of them is forty `git` invocations and the operator should not wait for the
/// last one to see the header.
pub type Out<'a> = &'a mut dyn FnMut(String);

/// What this invocation was asked, and what the ladders already answered for it.
///
/// The active workspace and the sentence naming its rung are passed IN rather than resolved
/// here, for the reason `charter/workspace.py:source` gives about being called with the same
/// `cwd` as `resolve`: a header that named the workspace from one reading and the reason
/// from another would explain the answer by naming a rung that did not decide it.
pub struct Request<'a> {
    pub root: &'a Path,
    /// Where the caller is standing — the nested-plane notice's whole question.
    pub cwd: &'a Path,
    /// The workspace the ladder resolved.
    pub active: &'a str,
    /// `charter/workspace.py:source`'s label for the rung that resolved it.
    pub via: &'a str,
    /// `--all`: detail every workspace rather than the active one.
    pub all: bool,
}

/// Run `status`. Returns the exit status, which is Python's constant 0 — this command
/// reports, and a plane in a state worth reporting is not a plane that failed.
pub fn status(req: &Request, out: Out, say: Sink) -> u8 {
    let root = req.root;
    let cfg = match forge::load_config(root) {
        Ok(cfg) => cfg,
        Err(why) => {
            say(Say::Plain(why));
            return 1;
        }
    };
    let group = forge::group_of(&cfg, 0);
    let exclude = forge::exclude_of(&cfg, 0);
    let doc = match inventory::load(root, &group) {
        Ok(doc) => doc,
        Err(why) => {
            say(Say::Plain(why));
            return 1;
        }
    };
    // `inventory::repos`, not the raw document: the plane's own repo is clonable without
    // `discover` at all, so counting only what the file lists would report "0 repos
    // available to clone" beside a `charter clone` that works.
    //
    // Keyed by NAME and counted by key, as Python's dict comprehension counts: two records
    // sharing a bare name are one row in the table and one in this count.
    let mut by_name: BTreeMap<String, Value> = BTreeMap::new();
    for record in inventory::repos(root, &doc, &exclude) {
        if let Some(name) = record.get("name").and_then(Value::as_str) {
            by_name.insert(name.to_string(), record.clone());
        }
    }

    let plane_at = Plane::open(root);
    let (all_ws, unread) = match plane_at.read_workspaces() {
        Ok(found) => found,
        // **A `workspaces/` charter could not LIST is not a plane with no workspaces**, and
        // every number below would be a count of nothing presented as a count of an empty
        // plane — this module's third answer, one directory up. So nothing is printed:
        // `status` answers "what is here", and here it cannot. Python raises the `OSError`
        // out of `read_workspaces_aloud`, before its first line of stdout too; this says the
        // sentence instead of the traceback.
        Err(why) => {
            say(Say::Fail(format!(
                "workspaces/ cannot be listed ({why}) — charter cannot say what this plane \
                 holds, and an empty answer would not be the same thing. {}",
                crate::memstore::uncheckable_fix(
                    why.raw_os_error(),
                    &root.join("workspaces").display().to_string(),
                    "it",
                )
            )));
            return 1;
        }
    };
    // Said BEFORE the counts that leave it out, as `read_workspaces_aloud` says it: the
    // header's `N workspace(s)` is a count of the workspaces charter could read, and a
    // reader who is about to be handed that number is owed the ones it could not.
    for (path, code) in &unread {
        say(Say::Fail(cannot_check_workspace(path, *code)));
    }

    out(format!(
        "{group}: {} repos in inventory · {} workspace(s) · active: {} (via {})",
        by_name.len(),
        all_ws.len(),
        req.active,
        req.via
    ));
    if let Some(note) = nested_plane_note(root, req.cwd) {
        out(note);
    }
    out(String::new());

    if !all_ws.is_empty() {
        for name in &all_ws {
            let mark = if name == req.active { "*" } else { " " };
            // The roster COUNTS, and says nothing about what it refused: with `--all` every
            // one of these workspaces gets a section below, and a refusal said once there is
            // a refusal the operator reads once. Python's roster is silent here too.
            out(format!("  {mark} {name}  ({} cloned)", counted(root, name)));
        }
        out(String::new());
    }

    let which: Vec<&str> = if req.all {
        all_ws.iter().map(String::as_str).collect()
    } else {
        vec![req.active]
    };
    for ws in which {
        for_workspace(root, ws, req.active, &by_name, &mut *out, &mut *say);
    }

    let legacy = legacy_flat_clones(root);
    if !legacy.is_empty() {
        say(Say::Warn(format!(
            "Legacy clones sit directly under workspaces/ (pre-workspace layout): {}. Move them \
             into workspaces/{}/ or re-clone with a workspace.",
            legacy.join(", "),
            crate::active::plane_default_workspace(root)
        )));
    }
    out(format!(
        "({} repos available to clone — see docs/topology.md)",
        by_name.len()
    ));
    0
}

/// One workspace's section: its header, then its clones as a table, or the hint that it has
/// none. Python's `_status_for_workspace`.
fn for_workspace(
    root: &Path,
    ws: &str,
    active: &str,
    by_name: &BTreeMap<String, Value>,
    out: Out,
    say: Sink,
) {
    let found = drawn(root, ws, &mut *say);
    let marker = if ws == active { " (active)" } else { "" };
    out(format!(
        "— workspace: {ws}{marker} · {} repo(s) —",
        found.len()
    ));
    if found.is_empty() {
        out(format!(
            "  (empty; `charter clone <repo> --workspace {ws}` to populate)"
        ));
        // Python's `print(f"…\n")`, which is the hint and then a blank line.
        out(String::new());
        return;
    }

    // **Sized from the two columns that cost nothing to know, and only those.** A repo name
    // is a directory entry and a stack is an inventory lookup; the third column runs two
    // `git` invocations per clone, so sizing from it would mean collecting every row before
    // printing any — on a workspace with twenty clones the operator waits for forty git
    // calls before the header appears. Nothing is lost: the last column has nothing to its
    // right, so it needs no width at all.
    let stacks: Vec<String> = found
        .iter()
        .map(|repo| stack_of(by_name, &repo.name))
        .collect();
    // No `cap`: a clipped repo name is one the reader cannot go and act on, and this whole
    // command is for a reader who is about to act.
    let nw = tui::column("REPO", found.iter().map(|r| r.name.as_str()), 2, None);
    let sw = tui::column("STACK", stacks.iter().map(String::as_str), 2, None);
    // Header and data rows through ONE function: they are sibling rows of one table, and two
    // code paths that each believe they agree about the widths is the fastest way back to a
    // misaligned report (charter #508's own finding, one command over).
    let line = |repo: &str, stack: &str, note: &str| {
        let row = format!(
            "  {}{}{note}",
            tui::pad(repo, nw, tui::Align::Left),
            tui::pad(stack, sw, tui::Align::Left)
        );
        // Python's `.rstrip()`, not `tui.finish`: charter rstrips the plain string here, and
        // `finish` also removes whitespace hiding behind a trailing SGR — a difference no row
        // charter writes can show, and one this differential would.
        crate::memstore::py_rstrip(&row).to_string()
    };
    out(line("REPO", "STACK", "BRANCH / NOTE"));
    for (repo, stack) in found.iter().zip(&stacks) {
        out(line(&repo.name, stack, &clone_note(&repo.path)));
    }
    out(String::new());
}

/// The STACK cell: what the inventory says about this repo, or `?` when it says nothing.
///
/// `inventory/repos.json` is a TRACKED file, so this value is whatever somebody committed —
/// and Python's `.get("stack", "?")` only defaults a MISSING key. A record carrying
/// `"stack": null`, or a number, reaches `tui.width` there as a non-string and takes the
/// whole command down with a `TypeError`. Here a null reads as "says nothing" and anything
/// else is rendered as Python would `str()` it, so a hand-edited inventory costs a row that
/// looks odd rather than a `status` that cannot run. The value still goes through
/// [`crate::tui::sanitize`] before it is measured or printed.
fn stack_of(by_name: &BTreeMap<String, Value>, name: &str) -> String {
    match by_name.get(name).and_then(|r| r.get("stack")) {
        None | Some(Value::Null) => "?".to_string(),
        Some(found) => py_str(found),
    }
}

/// The clones of one workspace, sorted, with anything charter refused said rather than
/// dropped.
///
/// A repo that quietly disappears from the table reads as "this workspace has one fewer
/// repo", which is the same class of lie as a `git status` that failed reading as clean —
/// and the operator is the only one who can tell whether the link is theirs.
fn drawn(root: &Path, ws: &str, say: Sink) -> Vec<repos::Repo> {
    if not_there_yet(root, ws) {
        return Vec::new();
    }
    match repos::clones(root, ws) {
        Ok(found) => {
            for (name, why) in found.refused {
                say(Say::Warn(format!("{ws}: {name} is not drawn — {why}")));
            }
            found.repos
        }
        Err(why) => {
            say(Say::Warn(why.to_string()));
            Vec::new()
        }
    }
}

/// How many clones a workspace holds, silently — the roster's `(N cloned)`.
fn counted(root: &Path, ws: &str) -> usize {
    if not_there_yet(root, ws) {
        return 0;
    }
    repos::clones(root, ws).map(|f| f.repos.len()).unwrap_or(0)
}

/// Has this workspace no directory at all yet?
///
/// **A workspace nobody has created is not a refusal.** The ladder always ends on a name —
/// `[workspace] default`, and under that the literal `default` — so a plane with no
/// `workspaces/` still has an active workspace to report on, and `status` reports on it as
/// empty. `confine::workspace_dir` answers "not a directory charter can resolve" for absent
/// and present-but-not-a-directory alike, and only the first of those is ordinary; Python
/// separates them too (`workspace.clones`: `if not wd.exists(): return []`).
///
/// Asked with `symlink_metadata`, so a DANGLING link wearing the workspace's name is still
/// the refusal it is rather than an absence. And asked only of a name that can BE a
/// workspace: `-w` is taken as typed, so joining an unchecked one onto the plane is the
/// very escape the gate below exists to stop.
fn not_there_yet(root: &Path, ws: &str) -> bool {
    crate::contain::workspace_name_ok(ws)
        && matches!(
            std::fs::symlink_metadata(root.join("workspaces").join(ws)),
            // `NotFound` and nothing else. Any other errno is charter failing to LOOK — a
            // `workspaces/` it may not search answers `EACCES` — and reading that as "not
            // created yet" would be the same conflation this file is about, three lines
            // long instead of a whole command.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
        )
}

/// The NOTE column of one row. Python's `_clone_note`.
///
/// Two `git` invocations, and a third only for a tree that has a `.gitmodules` at all — so
/// the common row still costs the two it always did.
fn clone_note(at: &Path) -> String {
    let branch = branch_of(at);
    let dirty = match repos::state_of(at) {
        // A display site kept honest for one line's worth of change: this row already has a
        // written record of printing `clean` over a tree that was not.
        Err(_) => "unknown",
        Ok(state) if state.clean() => "clean",
        Ok(_) => "dirty",
    };
    let mut note = format!("{branch} · {dirty}");
    let (absent, moved) = super::submodules::drift(at);
    if !absent.is_empty() {
        note.push_str(&format!(" · {} submodule(s) not initialised", absent.len()));
    }
    if !moved.is_empty() {
        note.push_str(&format!(" · {} out of date", moved.len()));
    }
    note
}

/// `git rev-parse --abbrev-ref HEAD`, through the hardened runner, as Python reads it:
/// stdout stripped, and `""` for a git that could not answer.
///
/// Not read off [`repos::state_of`]'s `--branch` line, although that would save a process:
/// the two do not agree, and Python prints THIS one. `rev-parse --abbrev-ref` on a detached
/// HEAD says `HEAD`, where the porcelain header says `HEAD (no branch)`; on an unborn branch
/// `rev-parse` still names the branch, where the header says `No commits yet on <name>`.
fn branch_of(at: &Path) -> String {
    match git::run(at, &["rev-parse", "--abbrev-ref", "HEAD"], git::READ) {
        Ok(run) => crate::memstore::py_strip(&run.out).to_string(),
        Err(_) => String::new(),
    }
}

/// Git repos sitting directly under `workspaces/` — the pre-workspace layout.
/// Python's `workspace.legacy_flat_clones`.
///
/// A `.git` that is a DIRECTORY, through any link, which is Python's `_directory` question.
/// An entry charter cannot look at is no clone this can report, and is named by the
/// workspace listing above rather than twice.
fn legacy_flat_clones(root: &Path) -> Vec<String> {
    let dir = root.join("workspaces");
    let Ok(reader) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = reader
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join(".git").is_dir())
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// One sentence for a workspace directory charter could not look at.
/// Python's `workspace.cannot_check_workspace`, which is the sentence `reinit` and every
/// command that shows the plane's workspaces share — so no two of them send a reader to
/// different repairs for one directory.
///
/// The name is printed as it stands, which is what Python prints. It is a directory entry,
/// so it can hold anything; making it readable here would be this port answering a question
/// charter has not answered, and the two would then disagree about one unreadable path.
fn cannot_check_workspace(at: &Path, code: Option<i32>) -> String {
    let name = at.file_name().unwrap_or_default().to_string_lossy();
    let dir = at.display().to_string();
    format!(
        "workspace '{name}' cannot be checked — charter changes nothing it cannot see; {}.",
        crate::memstore::uncheckable_fix(code, &dir, &dir)
    )
}

/// One sentence when the caller is standing in a plane charter did **not** act on, else
/// `None` — the ordinary case, which says nothing. Python's `util.nested_plane_note`.
///
/// Both planes are named by PATH, never by their directory name: clone charter into its own
/// plane and both directories are called `charter`, which is how the old wording came out as
/// "memory and vault go to charter, not charter" (charter #200).
fn nested_plane_note(root: &Path, cwd: &Path) -> Option<String> {
    let origin = plane::standing_in_nested_plane(cwd)?;
    let here = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    if origin != here {
        return Some(format!(
            "you are standing in {}, which is a plane too — charter is acting on {}",
            short_path(&origin),
            short_path(&here)
        ));
    }
    // The hop was overridden by `$CHARTER_ROOT`, so charter really is acting on the inner
    // plane and the hazard is live. A warning, not a notice.
    let outer = plane::enclosing(&here)?;
    Some(format!(
        "nested plane: acting on {}, inside {}'s workspaces/ — memory and vaults go to the inner \
         one. Unset $CHARTER_ROOT to use {}",
        short_path(&here),
        short_path(&outer),
        short_path(&outer)
    ))
}

/// A path an operator can tell apart from another, short enough for one row.
/// Python's `util.short_path`.
///
/// **Never for a path with a segment that starts with `~`** (charter #969). Nothing expands
/// a `~` that arrives inside a value, so abbreviating such a path renders `~/…`: the home
/// folder, which the code never read.
fn short_path(path: &Path) -> String {
    let text = path.display().to_string();
    if path
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('~'))
    {
        return text;
    }
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return text;
    };
    match path.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_the_inventory_does_not_carry_is_a_question_mark() {
        let mut by_name = BTreeMap::new();
        by_name.insert("a".to_string(), serde_json::json!({"stack": "rust"}));
        by_name.insert("b".to_string(), serde_json::json!({"name": "b"}));

        assert_eq!(stack_of(&by_name, "a"), "rust");
        assert_eq!(stack_of(&by_name, "b"), "?", "a record with no stack");
        assert_eq!(stack_of(&by_name, "c"), "?", "no record at all");
    }

    #[test]
    fn a_path_is_shortened_only_where_a_tilde_cannot_be_read_as_a_home() {
        // #969: nothing expands a `~` inside a value, so a path holding one is shown whole.
        let odd = std::path::PathBuf::from("/srv/~acct2/plane");
        assert_eq!(short_path(&odd), "/srv/~acct2/plane");
    }
}
