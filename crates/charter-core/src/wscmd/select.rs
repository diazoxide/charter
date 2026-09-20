//! `charter workspace use`, `unlock` and `default` — the three verbs that CHOOSE a workspace,
//! and the pointers they write.
//!
//! A port of `commands_workspace.cmd_workspace_use`, `cmd_workspace_unlock` and
//! `cmd_workspace_default`, and of `workspace.set_active` / `is_locked` / `unlock` /
//! `set_declared_default` beneath them. [`crate::active`] is the reader of everything written
//! here, and nothing in this module re-derives a rung it already answers.
//!
//! # Two pointers and a lock, and why there are two
//!
//! `use` writes a **per-terminal** pointer (keyed by a stable pane id, so the pane keeps this
//! workspace across closing and reopening the harness) and a **per-session** pointer (so a
//! status line, which only knows the session id, agrees). It writes no global default:
//! selecting a workspace in one pane never changes another.
//!
//! Confirming a workspace also **locks** the session to it. Once locked, a switch to a
//! different workspace is refused unless `--force`, so the workspace cannot be swapped out
//! from under a running task. Every new session starts unlocked and gets to choose afresh.
//!
//! # The scope is the REACH of what was written
//!
//! `set_active` names the longest-lived pointer that actually landed, and the terminal one
//! outlives the session one. Python assigned these in sequence, so the session branch
//! overwrote the terminal branch and every caller was told `session` — a pane that HAD kept
//! its workspace across restarts was told it had not, and a shell with no pane id at all was
//! told it had, which is the direction that costs somebody their selection with nothing
//! having said so.
//!
//! # The one rung this charter cannot hold: a chat's launch lock
//!
//! Python's `is_locked` answers the **launch lock** first — the workspace a chat was launched
//! in, read from `.charter/frame/<fid>/`, which a chat holds for life (charter#936). That
//! record is the tmux frame's, and `docs/plane-format.md` rules that the app neither reads nor
//! writes there ([`crate::active`] declares the same divergence for the workspace ladder). So
//! this charter's lock is the session lock FILE alone, and `workspace unlock` here can always
//! release it. Inside a Python-launched chat the two disagree: Python refuses the unlock and
//! names the chat's own workspace, this one deletes a file that was not what held the chat.
//! Declared here and pinned by a test rather than discovered.

use std::path::{Path, PathBuf};

use crate::active::Ids;
use crate::repocmd::{Say, Sink};
use crate::wscmd;

/// How far a selection reaches — `set_active`'s return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// A per-terminal pointer landed, so the pane keeps this workspace across restarts.
    Terminal,
    /// Only a per-session pointer landed: this shell reports no pane id.
    Session,
    /// Neither id exists, so nothing persists past this process.
    None,
    /// The session is locked to a different workspace and `force` was not passed. **Nothing
    /// was written.**
    Locked,
}

/// Per-session pointers older than this are dropped when one is written — 30 days.
const SESSION_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(30 * 86400);

/// The plane's machine-local state directory, **as [`crate::active`] reads it**.
///
/// `root/.charter`, and deliberately NOT [`crate::plane::state_dir`], which honours
/// `$CHARTER_HOME`. The reader of everything written here is `active`, whose own module note
/// records that `$CHARTER_HOME` "is not read anywhere in this binary" — a writer that honoured
/// it would put a pointer where the resolution ladder does not look, which is a selection that
/// silently does nothing. The two must move together or not at all; this is the half that
/// keeps them together, and the divergence from Python (whose `config.SESSIONS_DIR` does
/// honour the variable) is the one `active` already declares.
fn state_dir(root: &Path) -> PathBuf {
    root.join(".charter")
}

fn sessions_dir(root: &Path) -> PathBuf {
    state_dir(root).join("sessions")
}

fn terminals_dir(root: &Path) -> PathBuf {
    state_dir(root).join("terminals")
}

/// The workspace this session is locked to, or `None` — `workspace.is_locked`, minus the
/// launch lock this charter has no record for (see the module header).
pub fn is_locked(root: &Path, ids: &Ids) -> Option<String> {
    let sid = ids
        .session
        .as_deref()
        .filter(|id| crate::contain::segment_ok(id))?;
    let path = sessions_dir(root).join(format!("{sid}.lock"));
    crate::contain::no_link_on_the_way(root, &path).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let name = crate::memstore::py_strip(&text);
    (!name.is_empty()).then(|| name.to_string())
}

/// Drop this session's lock file; `true` when one was cleared — `workspace.unlock`.
pub fn unlock(root: &Path, ids: &Ids) -> bool {
    let Some(sid) = ids
        .session
        .as_deref()
        .filter(|id| crate::contain::segment_ok(id))
    else {
        return false;
    };
    let path = sessions_dir(root).join(format!("{sid}.lock"));
    if crate::contain::no_link_on_the_way(root, &path).is_err() {
        return false;
    }
    std::fs::remove_file(&path).is_ok()
}

/// Select `name` for THIS pane and session, and lock the session to it.
///
/// `terminal` of `Some("")` says "this process has no terminal to speak for" and writes no
/// per-terminal pointer; `None` uses the id this process reports.
pub fn set_active(root: &Path, name: &str, ids: &Ids, force: bool) -> Scope {
    if !crate::contain::workspace_name_ok(name) {
        // Never reachable through the commands here, which check first — and checked anyway,
        // because what is on the other side is a pointer file whose CONTENT is joined onto
        // `workspaces/` by every future session (charter#442).
        return Scope::Locked;
    }
    let locked = is_locked(root, ids);
    if let Some(held) = &locked
        && held != name
        && !force
    {
        return Scope::Locked;
    }
    let _ = crate::plane::private_dir(root, &state_dir(root));
    let line = format!("{name}\n");
    let tid = ids
        .terminal
        .as_deref()
        .filter(|id| !id.is_empty() && crate::contain::segment_ok(id));
    if let Some(tid) = tid {
        let dir = terminals_dir(root);
        if crate::plane::private_dir(root, &dir).is_ok() {
            let _ = crate::plane::write_private(
                root,
                &dir.join(format!("{tid}.workspace")),
                line.as_bytes(),
            );
        }
    }
    let sid = ids
        .session
        .as_deref()
        .filter(|id| crate::contain::segment_ok(id));
    if let Some(sid) = sid {
        let dir = sessions_dir(root);
        if crate::plane::private_dir(root, &dir).is_ok() {
            let _ = crate::plane::write_private(
                root,
                &dir.join(format!("{sid}.workspace")),
                line.as_bytes(),
            );
            // Confirming IS locking.
            let _ = crate::plane::write_private(
                root,
                &dir.join(format!("{sid}.lock")),
                line.as_bytes(),
            );
        }
    }
    prune(root);
    // The longest-lived pointer that actually landed — see the module header.
    if tid.is_some() {
        Scope::Terminal
    } else if sid.is_some() {
        Scope::Session
    } else {
        Scope::None
    }
}

/// Drop every per-session and per-terminal marker past the cutoff — `workspace._prune`.
///
/// **The DIRECTORY, not a list of suffixes.** Python enumerated five (`*.workspace`,
/// `*.lock`, `*.configver`, `*.memnudge`, `*.usage`) and read as an exhaustive list of the
/// marker family while being nothing of the kind: three families were missing by the time
/// anyone looked. Both directories are charter's own state and hold nothing but per-session
/// and per-terminal markers, so there is no member for which keeping it past the cutoff is
/// the right answer. Files only: a directory in here is not a marker.
fn prune(root: &Path) {
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return;
    };
    let Some(cutoff) = now.checked_sub(SESSION_MAX_AGE) else {
        return;
    };
    for dir in [sessions_dir(root), terminals_dir(root)] {
        let Ok(reader) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in reader.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(found) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !found.is_file() {
                continue;
            }
            let stale = found
                .modified()
                .ok()
                .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
                .is_some_and(|when| when < cutoff);
            if stale {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

// ----------------------------------------------------------------------------------------
// charter workspace use
// ----------------------------------------------------------------------------------------

/// What a refused switch says — `commands_workspace._locked_msg`, its non-chat branch.
///
/// Python's other branch is for a caller inside a chat, whose lock is the workspace it was
/// launched in: there it says a chat belongs to its workspace for life and names how to open
/// one elsewhere. This charter holds no launch record (see the module header), so only this
/// branch exists.
fn locked_msg(target: &str, locked: &str) -> String {
    format!(
        "Workspace is 🔒 locked to '{locked}' for this session — switching to '{target}' \
         mid-session is disabled (never mix workspaces). Start a new session to pick another, \
         or force it: `charter workspace use {target} --force` (or `charter workspace unlock` \
         first)."
    )
}

/// `charter workspace use <name> [--create] [--force]`, and its exit code.
///
/// **A typo used to be a one-way door**: the name was only validated for SHAPE, so
/// `use fature-x` created `fature-x`, took the session lock, and the correction then hit
/// `✗ Workspace is 🔒 locked to 'fature-x' for this session`. Creating is deliberate
/// (`--create`), and an unknown name is a question rather than an action.
///
/// `default` is always selectable whether or not its directory exists: it is the
/// always-present workspace a plane lands on when nobody chose, and the unknown-name guard
/// refused it on every fresh plane.
///
/// **`--create` is REFUSED by this charter rather than creating**, and that is the one
/// divergence here: creating a workspace means scaffolding it, and `workspace create` has no
/// port yet ([`crate::wscmd`]'s gap list). A `use --create` that made a bare directory would
/// be the very state `workspace.ensure` exists to prevent — a workspace that exists and is
/// not one — and the status line would flag it for reinit for ever.
pub fn use_workspace(
    root: &Path,
    name: &str,
    ids: &Ids,
    create: bool,
    force: bool,
    say: Sink,
) -> u8 {
    if !crate::contain::workspace_name_ok(name) {
        say(Say::Fail(format!(
            "invalid workspace name '{}'",
            crate::personas::one_line(name)
        )));
        return 1;
    }
    let plane = crate::workspaces::Plane::open(root);
    let existing = plane.workspaces().unwrap_or_default();
    let always = crate::active::plane_default_workspace(root);
    let there = existing.iter().any(|n| n == name) || name == always;
    if !there {
        if !create {
            say(Say::Fail(format!("no workspace named '{name}'.")));
            let close = near(name, &existing);
            if !close.is_empty() {
                say(Say::Info(format!("  Did you mean: {}?", close.join(", "))));
            } else if !existing.is_empty() {
                say(Say::Info(format!("  Existing: {}", existing.join(", "))));
            }
            say(Say::Info(format!(
                "  Create it: charter workspace use {name} --create"
            )));
            return 1;
        }
        say(Say::Fail(format!(
            "`use --create` makes a workspace, and this charter cannot scaffold one yet — \
             nothing was created and nothing was selected. Create '{name}' with the Python \
             charter's `charter workspace create {name}`, then select it here."
        )));
        return 1;
    }

    let before = is_locked(root, ids);
    match set_active(root, name, ids, force) {
        Scope::Locked => {
            let held = before.unwrap_or_else(|| "?".to_string());
            say(Say::Fail(locked_msg(name, &held)));
            2
        }
        scope => {
            announce(root, name, scope, before.as_deref(), ids, say);
            warn_env_override(name, say);
            0
        }
    }
}

/// Say what a selection DID — `commands_workspace._announce_selection`.
///
/// Built from what [`set_active`] actually WROTE, the lock that stood before the call and the
/// lock that stands after it, never from what the command was asked to do (ADR 0013). Each
/// clause below is a place the sentence was measured lying:
///
/// - **[`Scope::None`] wrote nothing.** No session id and no pane id means no pointer and no
///   lock, and this said "Active workspace set to 'gamma'." while the next command resolved to
///   `default`.
/// - **"re-locked to" only where this call MOVED a lock that stood before it.** The verb used
///   to read `--force`, which is what was asked: `--force` in a session that had never been
///   locked answered "re-locked to 'beta'" beside a lock that did not exist a moment earlier,
///   and forcing the workspace the lock already names moves nothing.
fn announce(root: &Path, name: &str, scope: Scope, before: Option<&str>, ids: &Ids, say: Sink) {
    if scope == Scope::None {
        say(Say::Warn(format!(
            "Nothing was persisted: this process has no session id and no pane id, so there is \
             nowhere to keep '{name}' selected. Pass `--workspace {name}` per command, or set \
             `$CHARTER_WORKSPACE={name}`."
        )));
        return;
    }
    let locked = is_locked(root, ids);
    let verb = match (before, locked.as_deref()) {
        (Some(was), now) if now != Some(was) => "re-locked to",
        _ => "set to",
    };
    say(Say::Done(format!(
        "Active workspace {verb} '{name}'{} — {}.",
        scope_note(root, scope),
        lock_words(name, locked.as_deref())
    )));
}

/// How far the selection reaches, in the words the reader needs —
/// `commands_workspace._scope_note`.
///
/// Only a terminal pointer survives closing and reopening the harness. A session-scoped
/// selection is gone the moment the session is, and saying so is the whole point: the reader
/// who is not told goes looking for a bug the next time the status line says `default`.
///
/// **Where a new session starts is the ladder, not a constant** (charter#936): a plane with a
/// nominated default is where a new session with no pointer and no pane id lands, and the
/// note named `default` regardless.
///
/// Python has a third branch above these — "this chat only" — for a caller inside a chat,
/// read from the frame's launch record. This charter holds no such record (see the module
/// header), so the branch is absent rather than guessed at.
fn scope_note(root: &Path, scope: Scope) -> String {
    match scope {
        Scope::Terminal => " (this terminal only — kept across closing/reopening Claude)".into(),
        Scope::Session => {
            let starts = crate::active::declared_default_workspace(root)
                .unwrap_or_else(|| crate::active::plane_default_workspace(root));
            format!(
                " (this session only — this terminal reports no pane id, so a new session \
                 starts at '{starts}')"
            )
        }
        Scope::None | Scope::Locked => String::new(),
    }
}

/// How the lock stands relative to the workspace a sentence names —
/// `commands_workspace._lock_words`, always with its `after_switch` half.
///
/// Every sentence that reports the lock after a workspace command succeeded asks this one
/// function, because the sites that decided it for themselves drifted: `use` learned to name
/// a chat's launch lock while `create --use` went on announcing "🔒 locked for this session"
/// beside a workspace that was not the lock, and `rename` kept a fourth copy, "(still 🔒
/// locked)", which asked nothing and was measured false in three states.
fn lock_words(name: &str, locked: Option<&str>) -> String {
    match locked {
        None => "unlocked".to_string(),
        Some(held) if held == name => "🔒 locked for this session".to_string(),
        Some(held) => format!("this session's commands only; 🔒 still locked to '{held}'"),
    }
}

/// `$CHARTER_WORKSPACE` outranks every pointer this command wrote, so a selection it will
/// override is said out loud — `commands_workspace._warn_env_override` (charter#1055).
///
/// The name resolution RANKS, not the raw variable: a blank one outranks nothing, and warning
/// that `' '` takes precedence sent the operator to a variable that was not deciding. The
/// name is bounded to one line because it comes out of a shell, and a line separator in it
/// would write a line of this output that charter did not.
fn warn_env_override(name: &str, say: Sink) {
    let env = std::env::var(crate::active::WORKSPACE_ENV).unwrap_or_default();
    let env = crate::memstore::py_strip(&env);
    if env.is_empty() || env == name {
        return;
    }
    let shown = crate::personas::one_line(env);
    say(Say::Warn(format!(
        "$CHARTER_WORKSPACE='{shown}' is set and takes precedence — commands in this session \
         will still act on '{shown}', not '{name}'."
    )));
}

/// Up to three of `names` that look like `name` — `difflib.get_close_matches(n=3, cutoff=0.6)`.
///
/// **Not difflib's ratio.** Python's is `SequenceMatcher`, whose quick-ratio ordering is not
/// something to reproduce from memory; what this promises is only "a short list of near
/// misses", and the suggestion line is advice rather than behaviour. The differential
/// scenario for a missing workspace therefore names one that is not near anything, so the two
/// implementations print the same line.
fn near(name: &str, names: &[String]) -> Vec<String> {
    let mut scored: Vec<(usize, &String)> = names
        .iter()
        .filter_map(|other| {
            let d = distance(name, other);
            let longest = name.chars().count().max(other.chars().count());
            // 0.6 of difflib's similarity, approximated as 1 - distance/longest.
            (longest > 0 && (longest - d) * 5 >= longest * 3).then_some((d, other))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().take(3).map(|(_, n)| n.clone()).collect()
}

/// Levenshtein distance, in characters.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            let next = (row[j + 1] + 1).min(row[j] + 1).min(previous + cost);
            previous = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

// ----------------------------------------------------------------------------------------
// charter workspace unlock
// ----------------------------------------------------------------------------------------

/// `charter workspace unlock`, and its exit code.
pub fn unlock_command(root: &Path, ids: &Ids, say: Sink) -> u8 {
    if unlock(root, ids) {
        say(Say::Done(
            "Workspace unlocked for this session — `charter workspace use <name>` can switch \
             now."
                .to_string(),
        ));
    } else {
        say(Say::Info(
            "No workspace lock was set for this session (nothing to unlock).".to_string(),
        ));
    }
    0
}

// ----------------------------------------------------------------------------------------
// charter workspace default
// ----------------------------------------------------------------------------------------

/// `workspaces/.default` — the committed nomination.
pub fn default_file(root: &Path) -> PathBuf {
    root.join("workspaces").join(".default")
}

/// `charter workspace default [<name>] [--clear]`, and its exit code.
///
/// **`--clear` is answered FIRST, before the name is looked at**, because it needs none
/// (charter#955). Below the show branch, the form a person types —
/// `charter workspace default --clear` — printed the current default, exited 0 and removed
/// nothing, so clearing was reachable only by also typing a name nobody checked.
pub fn default_command(root: &Path, name: Option<&str>, clear: bool, say: Sink) -> u8 {
    if clear {
        return match std::fs::remove_file(default_file(root)) {
            Ok(()) => {
                say(Say::Done(
                    "Cleared the declared default workspace.".to_string(),
                ));
                0
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                say(Say::Info("No default workspace was declared.".to_string()));
                0
            }
            // The OS's own words beside the path, and no cause of charter's: a read-only
            // `workspaces/` and a directory sitting at that name both land here, and picking
            // one to name is the ADR 0009 failure. A failed unlink removes nothing, so there
            // is no partial state to read back.
            Err(e) => {
                say(Say::Fail(format!(
                    "could not remove {} ({e}) — the declared default is left as it was.",
                    default_file(root).display()
                )));
                1
            }
        };
    }
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return match crate::active::declared_default_workspace(root) {
            Some(cur) => {
                say(Say::Info(format!("Declared default workspace: {cur}")));
                0
            }
            None => {
                say(Say::Info(format!(
                    "No declared default — a session with nothing else selected lands on '{}'. \
                     Set one: charter workspace default <ws>",
                    crate::active::plane_default_workspace(root)
                )));
                0
            }
        };
    };
    // The name FIRST, and separately from "does it exist". `workspace_dir(name).exists()`
    // accepted `../../esc` — the directory is really there, it is simply not a workspace —
    // and wrote it into a committed file every future session reads (charter#442). "A path
    // that exists" was never the question being asked.
    if !crate::contain::workspace_name_ok(name) {
        say(Say::Fail(format!(
            "'{}' is not a workspace name (letters, digits, '.', '_', '-'; must not start \
             with a dot). This file is committed and read on every session start, so a value \
             that is not a name is refused here.",
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
    let path = default_file(root);
    // A fixed name directly under `workspaces/`, which the default ignore rule
    // (`/workspaces/*/*`) does not match — so it is an ordinarily committable path, and a
    // link there redirects this write (charter#349).
    if let Err(why) = crate::contain::no_link_on_the_way(root, &path)
        .and_then(|()| std::fs::create_dir_all(root.join("workspaces")))
        .and_then(|()| std::fs::write(&path, format!("{name}\n")))
    {
        say(Say::Fail(format!(
            "could not write {} ({why}) — the declared default is left as it was.",
            path.display()
        )));
        return 1;
    }
    say(Say::Done(format!(
        "Default workspace set to '{name}' — sessions land here when nothing else has decided."
    )));
    say(Say::Info(
        "  Committed, so it travels with the plane. It is read LAST, so an explicit \
         `--workspace`, $CHARTER_WORKSPACE, the tree you are standing in, or a \
         session/terminal selection all still win."
            .to_string(),
    ));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        dir
    }

    fn workspace(root: &Path, name: &str) {
        std::fs::create_dir_all(root.join("workspaces").join(name)).unwrap();
    }

    fn ids(session: Option<&str>, terminal: Option<&str>) -> Ids {
        Ids {
            session: session.map(str::to_string),
            terminal: terminal.map(str::to_string),
        }
    }

    fn lines(f: impl FnOnce(Sink) -> u8) -> (u8, Vec<String>) {
        let mut said = Vec::new();
        let code = f(&mut |line: Say| said.push(line.to_string()));
        (code, said)
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::symlink_metadata(path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn selecting_writes_both_pointers_and_the_lock_at_0600() {
        let dir = plane();
        workspace(dir.path(), "beta");
        let ids = ids(Some("s1"), Some("t1"));

        assert_eq!(set_active(dir.path(), "beta", &ids, false), Scope::Terminal);
        let state = state_dir(dir.path());
        for rel in [
            "sessions/s1.workspace",
            "sessions/s1.lock",
            "terminals/t1.workspace",
        ] {
            let path = state.join(rel);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "beta\n", "{rel}");
            #[cfg(unix)]
            assert_eq!(mode(&path), 0o600, "{rel}");
        }
        #[cfg(unix)]
        {
            assert_eq!(mode(&state), 0o700);
            assert_eq!(mode(&state.join("sessions")), 0o700);
        }
    }

    #[test]
    fn the_scope_is_the_longest_lived_pointer_that_landed() {
        let dir = plane();
        workspace(dir.path(), "beta");
        // **Both ids, first**, because that is the case the rule is about and the one a first
        // cut of this test left out: Python assigned the two branches in sequence, so the
        // session branch overwrote the terminal branch and every caller was told `session`.
        // A mutation that swapped the two arms left this test green until this line existed.
        assert_eq!(
            set_active(dir.path(), "beta", &ids(Some("s0"), Some("t0")), false),
            Scope::Terminal,
            "the terminal pointer outlives the session one, so it is what the scope names"
        );
        assert_eq!(
            set_active(dir.path(), "beta", &ids(Some("s1"), None), false),
            Scope::Session,
            "a shell with no pane id is told its selection does not survive it"
        );
        assert_eq!(
            set_active(dir.path(), "beta", &ids(None, None), false),
            Scope::None
        );
        assert_eq!(
            set_active(dir.path(), "beta", &ids(None, Some("t2")), false),
            Scope::Terminal
        );
    }

    #[test]
    fn a_lock_refuses_a_switch_and_writes_nothing() {
        let dir = plane();
        workspace(dir.path(), "beta");
        workspace(dir.path(), "gamma");
        let ids = ids(Some("s1"), Some("t1"));
        set_active(dir.path(), "beta", &ids, false);

        assert_eq!(set_active(dir.path(), "gamma", &ids, false), Scope::Locked);
        let state = state_dir(dir.path());
        assert_eq!(
            std::fs::read_to_string(state.join("sessions/s1.workspace")).unwrap(),
            "beta\n",
            "a refused switch leaves every pointer where it was"
        );
    }

    #[test]
    fn re_selecting_the_workspace_you_are_locked_to_is_never_refused() {
        let dir = plane();
        workspace(dir.path(), "beta");
        let ids = ids(Some("s1"), Some("t1"));
        set_active(dir.path(), "beta", &ids, false);
        assert_eq!(set_active(dir.path(), "beta", &ids, false), Scope::Terminal);
    }

    #[test]
    fn force_moves_the_lock() {
        let dir = plane();
        workspace(dir.path(), "beta");
        workspace(dir.path(), "gamma");
        let ids = ids(Some("s1"), Some("t1"));
        set_active(dir.path(), "beta", &ids, false);
        assert_eq!(set_active(dir.path(), "gamma", &ids, true), Scope::Terminal);
        assert_eq!(is_locked(dir.path(), &ids).as_deref(), Some("gamma"));
    }

    #[test]
    fn unlock_clears_the_file_once_and_says_so_the_second_time() {
        let dir = plane();
        workspace(dir.path(), "beta");
        let ids = ids(Some("s1"), None);
        set_active(dir.path(), "beta", &ids, false);

        let (code, said) = lines(|say| unlock_command(dir.path(), &ids, say));
        assert_eq!(code, 0);
        assert!(said[0].contains("Workspace unlocked"), "{said:?}");
        assert_eq!(is_locked(dir.path(), &ids), None);

        let (code, said) = lines(|say| unlock_command(dir.path(), &ids, say));
        assert_eq!(code, 0);
        assert!(said[0].contains("nothing to unlock"), "{said:?}");
    }

    #[test]
    fn use_refuses_a_workspace_this_plane_does_not_have_and_offers_the_near_miss() {
        let dir = plane();
        workspace(dir.path(), "feature-x");
        let ids = ids(Some("s1"), None);

        let (code, said) =
            lines(|say| use_workspace(dir.path(), "fature-x", &ids, false, false, say));
        assert_eq!(code, 1);
        assert!(
            said[0].contains("no workspace named 'fature-x'"),
            "{said:?}"
        );
        assert!(said[1].contains("Did you mean: feature-x?"), "{said:?}");
        assert!(
            is_locked(dir.path(), &ids).is_none(),
            "a typo must not take the session lock"
        );
    }

    #[test]
    fn the_always_present_workspace_is_selectable_before_its_directory_exists() {
        let dir = plane();
        let ids = ids(Some("s1"), None);
        let (code, said) =
            lines(|say| use_workspace(dir.path(), "default", &ids, false, false, say));
        assert_eq!(code, 0, "{said:?}");
        assert_eq!(is_locked(dir.path(), &ids).as_deref(), Some("default"));
    }

    #[test]
    fn use_create_is_refused_rather_than_leaving_a_bare_directory() {
        let dir = plane();
        let ids = ids(Some("s1"), None);
        let (code, said) =
            lines(|say| use_workspace(dir.path(), "brand-new", &ids, true, false, say));
        assert_eq!(code, 1);
        assert!(said[0].contains("cannot scaffold one yet"), "{said:?}");
        assert!(!dir.path().join("workspaces/brand-new").exists());
    }

    #[test]
    fn a_name_that_is_not_one_never_reaches_a_path() {
        let dir = plane();
        let ids = ids(Some("s1"), None);
        let (code, said) =
            lines(|say| use_workspace(dir.path(), "../../esc", &ids, false, false, say));
        assert_eq!(code, 1);
        assert!(said[0].contains("invalid workspace name"), "{said:?}");
    }

    #[test]
    fn the_declared_default_is_shown_set_and_cleared() {
        let dir = plane();
        workspace(dir.path(), "beta");

        let (code, said) = lines(|say| default_command(dir.path(), None, false, say));
        assert_eq!(code, 0);
        assert!(said[0].contains("No declared default"), "{said:?}");

        let (code, said) = lines(|say| default_command(dir.path(), Some("beta"), false, say));
        assert_eq!(code, 0, "{said:?}");
        assert_eq!(
            std::fs::read_to_string(default_file(dir.path())).unwrap(),
            "beta\n"
        );
        assert_eq!(
            crate::active::declared_default_workspace(dir.path()).as_deref(),
            Some("beta")
        );

        let (code, said) = lines(|say| default_command(dir.path(), None, false, say));
        assert_eq!(code, 0);
        assert!(
            said[0].contains("Declared default workspace: beta"),
            "{said:?}"
        );

        let (code, said) = lines(|say| default_command(dir.path(), None, true, say));
        assert_eq!(code, 0);
        assert!(said[0].contains("Cleared"), "{said:?}");
        assert!(!default_file(dir.path()).exists());

        let (code, said) = lines(|say| default_command(dir.path(), None, true, say));
        assert_eq!(code, 0);
        assert!(
            said[0].contains("No default workspace was declared."),
            "{said:?}"
        );
    }

    #[test]
    fn clear_is_answered_before_a_name_is_even_looked_at() {
        // charter#955: below the show branch, `--clear` printed the current default and
        // removed nothing.
        let dir = plane();
        workspace(dir.path(), "beta");
        default_command(dir.path(), Some("beta"), false, &mut |_| {});
        let (code, _said) = lines(|say| default_command(dir.path(), None, true, say));
        assert_eq!(code, 0);
        assert!(!default_file(dir.path()).exists());
    }

    #[test]
    fn a_default_that_is_not_a_name_is_refused_before_the_committed_file_is_written() {
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("workspaces/../outside")).unwrap();
        let (code, said) = lines(|say| default_command(dir.path(), Some("../outside"), false, say));
        assert_eq!(code, 1);
        assert!(said[0].contains("is not a workspace name"), "{said:?}");
        assert!(!default_file(dir.path()).exists());
    }

    #[test]
    fn a_default_naming_a_workspace_that_is_not_there_is_refused() {
        let dir = plane();
        let (code, said) = lines(|say| default_command(dir.path(), Some("nope"), false, say));
        assert_eq!(code, 1);
        assert!(said[0].contains("no workspace 'nope'"), "{said:?}");
        assert!(!default_file(dir.path()).exists());
    }

    #[test]
    fn a_stale_pointer_is_pruned_when_the_next_selection_is_written() {
        let dir = plane();
        workspace(dir.path(), "beta");
        let sessions = sessions_dir(dir.path());
        std::fs::create_dir_all(&sessions).unwrap();
        let old = sessions.join("ancient.workspace");
        std::fs::write(&old, "gone\n").unwrap();
        let long_ago =
            std::time::SystemTime::now() - SESSION_MAX_AGE - std::time::Duration::from_secs(60);
        filetime_set(&old, long_ago);

        set_active(dir.path(), "beta", &ids(Some("s1"), None), false);
        assert!(!old.exists(), "a pointer 30 days old is not a selection");
        assert!(sessions.join("s1.workspace").exists());
    }

    /// `utimes` on one file, so the prune test does not have to wait a month.
    fn filetime_set(path: &Path, when: std::time::SystemTime) {
        let times = std::fs::FileTimes::new()
            .set_modified(when)
            .set_accessed(when);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(times)
            .unwrap();
    }
}
