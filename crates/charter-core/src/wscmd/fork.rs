//! `charter workspace fork` — a new workspace pre-loaded with another's context.
//!
//! A port of `commands_workspace.cmd_workspace_fork` and its `_carry`. The scaffold half is
//! [`crate::wslayer`]'s and was ported with M2.22; what is here is the half M2.22 named as a
//! gap, because it is not a scaffold step at all:
//!
//! - **carrying the parent's context** — its living charter, its whole memory store and its
//!   open todos, copied rather than regenerated;
//! - **the manifest**, whose repo rows are the UNION of what the parent recorded and what it
//!   actually has on disk, which needs a `git rev-parse` per clone — a git verb per repo, run
//!   through [`crate::worktree::git`] like every other.
//!
//! # A fork that could not read everything still exists
//!
//! charter#1084's rule, and it is the shape of this whole command: a piece charter could not
//! read is **named**, never raised over. `shutil.copytree` of a directory charter could not
//! list raised out of `fork` as a traceback; of one at mode 666 it copied nothing and then
//! gave the fork's copy that mode, so the fork's own memory was no longer writable. So a
//! directory is listed FIRST and not copied at all when it cannot be, the fork's own note
//! says what it did not inherit — a later reader of that memory is not told it holds context
//! it never received — and the exit code is 1 while everything else is done.
//!
//! # The union, and why a fork needs both answers
//!
//! charter has two answers to "which repos are in this workspace". The directory scan is
//! always current and never portable; the manifest is a snapshot, portable and committed, but
//! written only by `charter workspace snapshot`, which refuses while a repo holds unpushed
//! work. `fork` read the manifest alone, so a workspace with nine clones and no snapshot
//! reported nine repos everywhere a human looked and inherited zero (charter#81). Where both
//! know a repo the MANIFEST's branch wins — it was recorded under `snapshot`'s promise that
//! the branch was pushed — and the names whose branch came off the disk instead are said out
//! loud at the moment of use.
//!
//! # Where this is stricter than Python, on purpose
//!
//! Every path is read and written through the containment gate, so a `workspaces/<parent>`
//! committed as a symlink out of the plane carries **nothing**: Python's `_carry` resolves and
//! copies whatever is on the far end, which is how a fork comes to hold a file of the
//! operator's that no plane declares. The direction is the refusing one — charter copies less,
//! never more — and a parent behind such a link reads as a parent with nothing to carry.
//!
//! # `--restore`
//!
//! Performed, since M2.26. Python ends by calling `cmd_workspace_restore`, and so does this
//! ([`crate::wscmd::restore`]): a `charter clone` per missing repo, then the recorded branch
//! checked out and a credentialed `git pull --ff-only` per row. The exit is charter's —
//! `restore(...) or int(bool(missed))` — so a fork that inherited everything and could not
//! clone one repo exits on the restore's failure, and a fork that could not read a piece
//! still exits 1 with the clones in place.
//!
//! A `--restore` returns at that call, which is charter's shape too: the "Clone its N
//! inherited repo(s)" hint and the LIVE workspace's "Share the fork" line are what a fork
//! that did NOT restore says, and printing them after a restore would tell the operator to
//! do what has just been done.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::repocmd::{Say, Sink};
use crate::wscmd::{self, ensure};

/// What `fork` was asked for.
pub struct Request<'a> {
    pub root: &'a Path,
    /// The workspace to fork FROM.
    pub src: &'a str,
    /// The fork's name.
    pub new: &'a str,
    /// Share the fork's charter, manifest and memory from birth.
    pub live: bool,
    /// Clone the inherited repos now — see the module docs.
    pub restore: bool,
    /// The instant the manifest and the fork's note are stamped with.
    pub now: chrono::DateTime<chrono::Utc>,
}

/// One path `fork` could not read, with the errno behind it.
type Unread = Vec<(std::path::PathBuf, Option<i32>)>;

/// Run it, and give back the exit code.
///
/// **Exit 1 with the fork made** is the charter#1084 shape: a piece that could not be read is
/// a fork missing context, which a script must be able to see, and every other piece is still
/// carried. It is NOT the same failure as a name that is not a workspace, which creates
/// nothing.
pub fn fork(request: &Request, say: Sink) -> u8 {
    let Request {
        root,
        src,
        new,
        live,
        restore,
        now,
    } = *request;
    let plane = crate::workspaces::Plane::open(root);

    // The fork's name first, and BEFORE it is joined onto any path: `Path::join` throws the
    // prefix away when handed an absolute path and `..` walks out of the plane (charter#442).
    let Ok(fresh) = plane.workspace(new) else {
        say(Say::Fail(format!(
            "invalid workspace name '{}' (use lowercase letters, digits, . _ -)",
            crate::personas::one_line(new)
        )));
        return 1;
    };
    if src == new {
        say(Say::Fail(
            "source and fork names are the same — nothing to fork.".to_string(),
        ));
        return 1;
    }
    // A source that is no workspace NAME is a source that is no workspace: charter asks
    // `workspace_dir(src).exists()`, and a name that cannot be joined has no directory to
    // exist. One sentence for both, which is the one charter prints.
    let parent = match plane.workspace(src) {
        Ok(parent) if parent.dir().exists() => parent,
        _ => {
            say(Say::Fail(format!(
                "no workspace '{}'",
                crate::personas::one_line(src)
            )));
            return 1;
        }
    };
    if fresh.dir().exists() {
        say(Say::Fail(format!(
            "workspace '{new}' already exists — pick another name or remove it first."
        )));
        return 1;
    }

    let author = ensure::author();
    if let Err(why) = ensure::ensure(root, new, now, &author) {
        say(Say::Fail(why));
        return 1;
    }

    // Each piece is carried or NAMED, never raised over (charter#1084). Named by what the
    // operator would look for and not by charter's spelling of it: the sentence already says
    // which program could not read it.
    //
    // A `BTreeMap` would order these by name; the report's order is charter's — the charter,
    // then the memory, then the todos — so it is a list.
    let carried: Vec<(&str, Unread)> = vec![
        (
            "workspace.md",
            carry_file(
                root,
                &parent.dir().join("workspace.md"),
                &fresh.dir().join("workspace.md"),
            ),
        ),
        (
            "memory",
            carry_tree(
                root,
                &parent.dir().join("memory"),
                &fresh.dir().join("memory"),
            ),
        ),
        // Open todos travel with the memory, and for the same reason: a fork exists so
        // somebody can pick the task up with full context, and what is still to be done is
        // the most actionable part of it. A COPY, so the two lists diverge from here —
        // closing one in the fork must not rewrite the parent's plan — and copied wholesale
        // rather than re-added one by one, which would restamp every todo with today's date
        // and hide a three-week-old intent behind a fresh one.
        (
            "todos",
            carry_tree(
                root,
                &parent.dir().join("todos"),
                &fresh.dir().join("todos"),
            ),
        ),
    ];
    let missed: Unread = carried
        .iter()
        .flat_map(|(_, unread)| unread.iter().cloned())
        .collect();
    let not_carried = and_list(
        carried
            .iter()
            .filter(|(_, unread)| !unread.is_empty())
            .map(|(piece, _)| *piece),
    );

    // Membership comes from BOTH sources — see [`merge_repo_rows`]. Reading the manifest alone
    // made a fork of an un-snapshotted workspace inherit nothing while `status` cheerfully
    // reported its clones (charter#81).
    let (doc, _owner) = parent.manifest();
    let manifest = match doc {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let recorded: Vec<Value> = manifest
        .get("repos")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let disk = on_disk(root, src);
    let (repos, disk_only) = merge_repo_rows(&recorded, &disk);
    if !repos.is_empty() || !manifest.is_empty() {
        let mut doc = manifest.clone();
        doc.insert("name".into(), Value::String(new.to_string()));
        doc.insert("forked_from".into(), Value::String(src.to_string()));
        doc.insert(
            "repos".into(),
            Value::Array(
                repos
                    .iter()
                    .map(|(name, branch)| serde_json::json!({"name": name, "branch": branch}))
                    .collect(),
            ),
        );
        // `setdefault`: a manifest that never had one gets an empty string, and one that has
        // a description keeps it in the place it had.
        doc.entry("description")
            .or_insert_with(|| Value::String(String::new()));
        doc.insert(
            "updated_at".into(),
            Value::String(now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()),
        );
        doc.insert("updated_by".into(), Value::String(wscmd::git_user(root)));
        let _ = fresh.write_manifest(&Value::Object(doc));
    }

    // The fork's own note says what it did NOT inherit, so a later reader of its memory is
    // not told it holds context it never received (charter#1084).
    let note = if missed.is_empty() {
        format!("Forked from '{src}' — inherited its vision, context, glossary, and memo.")
    } else {
        format!("Forked from '{src}' — without the {not_carried} charter could not read there.")
    };
    let _ = fresh.remember(&note, now.naive_utc());

    if live {
        let _ = wscmd::set_live(root, new, true);
    }
    let mode = if live { "LIVE" } else { "LOCAL" };
    if missed.is_empty() {
        say(Say::Done(format!(
            "Forked '{src}' → '{new}' — charter + context + memo copied ({mode})."
        )));
    } else {
        // Not "copied": the sentence says what the fork does NOT have, each path follows, and
        // the exit at the end says it.
        say(Say::Warn(format!(
            "Forked '{src}' → '{new}' ({mode}) without the {not_carried} charter could not read \
             in '{src}':"
        )));
        for (path, code) in &missed {
            say(Say::Fail(crate::memstore::cannot_check(root, path, *code)));
        }
    }
    let inherited = fresh.todos().map(|t| t.len()).unwrap_or(0);
    if inherited > 0 {
        // Said out loud because the two lists are independent from here: whoever forked now
        // owns a second copy of this intent, not a view of the first.
        say(Say::Info(format!(
            "Inherited {inherited} open todo(s) from '{src}' — the fork's own list now: charter \
             ws todo --workspace {new}"
        )));
    }
    // Which source each repo came from. A snapshot promises its branch was pushed; a clone
    // found on disk promises only that it is checked out here right now, and `restore` on
    // another machine will fail on a branch that was never pushed. Reported at the moment of
    // use beats a drift warning that would fire on every workspace nobody has snapshotted —
    // which is most of them, by design.
    if !disk_only.is_empty() {
        say(Say::Info(format!(
            "  {} of them come from clones on disk, not from a snapshot ({}) — their branch is \
             whatever is checked out now, which may not be pushed. `charter workspace snapshot \
             {src}` records them properly.",
            disk_only.len(),
            disk_only.join(", ")
        )));
    }
    if restore && !repos.is_empty() {
        // Where charter runs `workspace restore`, and now so does this. The fork's exit is
        // still charter's — `restore`'s status OR'd with whether a piece went unread — so a
        // fork that inherited everything and could not clone one repo is not reported as a
        // fork that lost context.
        say(Say::Info(format!(
            "Cloning {} inherited repo(s)…",
            repos.len()
        )));
        // Python's `restore(...) or int(bool(missed))`: the restore's own failure first, and
        // the unread-context exit only where the restore itself went fine.
        let rc = wscmd::restore::after_fork(root, new, now, say);
        return if rc != 0 {
            rc
        } else {
            u8::from(!missed.is_empty())
        };
    } else if !repos.is_empty() {
        say(Say::Info(format!(
            "Clone its {} inherited repo(s): charter workspace restore {new}  (or re-run fork \
             with --restore).",
            repos.len()
        )));
    } else {
        say(Say::Info(format!(
            "No repos on '{src}' — neither a snapshot nor clones on disk. Clone into the fork: \
             charter clone <repo> -w {new}"
        )));
    }
    if live {
        say(Say::Info(format!(
            "Share the fork: charter workspace save {new}"
        )));
    }
    u8::from(!missed.is_empty())
}

/// `and`-joined, charter's way: `"a, b and c"`, and `""` for nothing.
fn and_list<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let parts: Vec<&str> = parts.into_iter().collect();
    match parts.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Copy one FILE, and give back the path charter could not read.
///
/// The destination already exists — the scaffold wrote it a moment ago — so this truncates
/// rather than creates, which is what `shutil.copyfile` does and is why the destination keeps
/// the mode the scaffold gave it rather than the parent's.
///
/// A source that is not there at all carries nothing and is **not** an unread path: charter's
/// `FileNotFoundError` arm returns `[]`, and a parent with no charter is not a parent charter
/// could not read.
fn carry_file(plane: &Path, src: &Path, dst: &Path) -> Unread {
    if crate::contain::readable(plane, src).is_err() {
        // Stricter than Python, in the refusing direction (see the module docs): a link out
        // of the plane is not a file this fork inherits. Reported as unread, because "carried
        // nothing and said nothing" is the one answer a fork may not give.
        return vec![(src.to_path_buf(), None)];
    }
    let text = match std::fs::read(src) {
        Ok(bytes) => bytes,
        Err(ref e) if crate::memstore::is_absent(e) => return Vec::new(),
        Err(e) => return vec![(src.to_path_buf(), e.raw_os_error())],
    };
    match std::fs::write(dst, &text) {
        Ok(()) => Vec::new(),
        Err(e) => vec![(src.to_path_buf(), e.raw_os_error())],
    }
}

/// Copy a DIRECTORY and everything under it, and give back each path charter could not read.
///
/// **Listed first, and not copied at all when the listing is refused or an entry cannot be
/// told about.** That is charter's order and the reason for it is the one in `_carry`'s own
/// docstring: a `copytree` that copied half a memory store and then gave the fork's copy the
/// parent's mode left the fork's own memory unwritable. A source that is not a directory
/// falls through to [`carry_file`], which is charter's own fall-through and reports the same
/// thing charter's `copyfile` does: nothing for a path that is not there, and the errno for
/// one that is.
///
/// **One divergence, one level down.** charter lists only the IMMEDIATE entries before
/// copying and lets `copytree` meet whatever is deeper file by file; this applies the same
/// list-first rule at every level, so a subdirectory charter cannot list costs that
/// subdirectory rather than the files beside it inside it. Same direction — less is carried,
/// never more — and a workspace memory store has no subdirectories to reach it with.
fn carry_tree(plane: &Path, src: &Path, dst: &Path) -> Unread {
    let entries = match std::fs::read_dir(src) {
        Ok(entries) => entries,
        // Absent, a file, or a directory charter may not list. A directory that is there and
        // will not be listed is the one that must be named, with the errno the LISTING met —
        // an `lstat` of it answers, which is why the refusal is not asked for a second time.
        // Everything else is the file fall-through, exactly as charter's `_directory` test
        // leaves it.
        Err(why) => {
            return match std::fs::metadata(src) {
                Ok(meta) if meta.is_dir() => vec![(src.to_path_buf(), why.raw_os_error())],
                _ => carry_file(plane, src, dst),
            };
        }
    };
    // Every entry told about BEFORE a byte is copied.
    let mut unread: Unread = Vec::new();
    let mut found: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => found.push(entry.path()),
            Err(e) => unread.push((src.to_path_buf(), e.raw_os_error())),
        }
    }
    found.sort();
    for path in &found {
        if path.symlink_metadata().is_err() {
            unread.push((path.clone(), last_errno(path)));
        }
    }
    if !unread.is_empty() {
        return unread;
    }
    if std::fs::create_dir_all(dst).is_err() {
        return vec![(src.to_path_buf(), None)];
    }
    for path in &found {
        let Some(name) = path.file_name() else {
            continue;
        };
        let into = dst.join(name);
        let is_dir = path
            .symlink_metadata()
            .map(|m| m.file_type().is_dir())
            .unwrap_or(false);
        if is_dir {
            unread.extend(carry_tree(plane, path, &into));
            continue;
        }
        // The containment gate, at the exact path that is OPENED and never at its parent: a
        // memory that is a link out of the plane is not one this fork inherits.
        if crate::contain::readable(plane, path).is_err() {
            unread.push((path.clone(), None));
            continue;
        }
        match std::fs::read(path) {
            Err(e) => unread.push((path.clone(), e.raw_os_error())),
            Ok(bytes) => match std::fs::write(&into, &bytes) {
                Err(e) => unread.push((path.clone(), e.raw_os_error())),
                // `copy2`'s mode, which is what charter's `copytree` gives the copy — and the
                // differential compares every file's mode, so a fork whose memory came back
                // 0644 where charter's is 0600 is a difference the suite reports.
                Ok(()) => copy_mode(path, &into),
            },
        }
    }
    unread
}

/// Give `dst` the permission bits `src` has. Best effort: a filesystem that carries no mode
/// is not a fork that failed.
fn copy_mode(src: &Path, dst: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(src) {
            let mode = meta.permissions().mode();
            let _ = std::fs::set_permissions(dst, std::fs::Permissions::from_mode(mode));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (src, dst);
    }
}

/// The errno a fresh `lstat` of `path` meets, or `None` when it answers.
fn last_errno(path: &Path) -> Option<i32> {
    path.symlink_metadata().err().and_then(|e| e.raw_os_error())
}

/// `{name: branch}` for every clone the workspace HAS on disk — `_repo_branch` per clone.
///
/// The git verb this whole module is here for, and it goes through [`crate::worktree::git`]
/// like every other: `env_clear()`, the fixed binary search, the READ deadline. A tree git
/// cannot name a branch in is `HEAD`, which is the literal git prints and the literal charter
/// records.
fn on_disk(root: &Path, ws: &str) -> BTreeMap<String, String> {
    let Ok(found) = crate::repos::clones(root, ws) else {
        return BTreeMap::new();
    };
    found
        .repos
        .iter()
        .map(|repo| {
            let branch = match crate::repos::state_of(&repo.path) {
                Ok(state) => wscmd::branch_word(&state.head),
                Err(_) => "HEAD".to_string(),
            };
            (repo.name.clone(), branch)
        })
        .collect()
}

/// The union of what a workspace RECORDED and what it HAS — `workspace.merge_repo_rows`.
///
/// `(rows sorted by name, the names whose BRANCH came off the disk)`.
///
/// **Membership without a branch does not count as recorded**, and since charter#884 that is
/// the ordinary case: every manifest charter writes itself lists the repos with no branch, so
/// keying this on "is the repo in the manifest" would stop `fork` saying *"their branch is
/// whatever is checked out now"* about exactly the repos that sentence is true of. What the
/// caller warns about is the provenance of the BRANCH, so that is what is asked.
fn merge_repo_rows(
    recorded: &[Value],
    disk: &BTreeMap<String, String>,
) -> (Vec<(String, String)>, Vec<String>) {
    let mut by_name: BTreeMap<String, String> = disk.clone();
    let mut disk_only: std::collections::BTreeSet<String> = disk.keys().cloned().collect();
    for row in recorded {
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let pinned = row
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !pinned.is_empty() {
            disk_only.remove(&name);
        }
        let branch = if pinned.is_empty() {
            by_name.get(&name).cloned().unwrap_or_default()
        } else {
            pinned
        };
        let branch = if branch.is_empty() {
            "HEAD".to_string()
        } else {
            branch
        };
        by_name.insert(name, branch);
    }
    (
        by_name.into_iter().collect(),
        disk_only.into_iter().collect(),
    )
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

    fn run(root: &Path, src: &str, new: &str, live: bool) -> (u8, Vec<String>) {
        let mut lines = Vec::new();
        let code = fork(
            &Request {
                root,
                src,
                new,
                live,
                restore: false,
                now: now(),
            },
            &mut |s| lines.push(s.to_string()),
        );
        (code, lines)
    }

    fn a_parent(root: &Path, name: &str) {
        ensure::ensure(root, name, now(), "fixture").unwrap();
        let plane = crate::workspaces::Plane::open(root);
        let ws = plane.workspace(name).unwrap();
        ws.set_vision("Ship the widget").unwrap();
        ws.remember("The API returns 418 on Mondays", now().naive_utc())
            .unwrap();
        ws.add_todo("Write the migration", now().naive_utc())
            .unwrap();
    }

    #[test]
    fn a_fork_inherits_the_charter_the_memory_and_the_open_todos() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        let (code, lines) = run(dir.path(), "alpha", "gamma", false);
        assert_eq!(code, 0, "{lines:?}");
        let gamma = dir.path().join("workspaces/gamma");
        let charter = std::fs::read_to_string(gamma.join("workspace.md")).unwrap();
        assert!(charter.contains("Ship the widget"), "{charter}");
        // The parent's memory, and the fork's own note beside it.
        let index = std::fs::read_to_string(gamma.join("memory/MEMORY.md")).unwrap();
        assert!(index.contains("The API returns 418 on Mondays"), "{index}");
        assert!(index.contains("Forked from 'alpha'"), "{index}");
        let todos = std::fs::read_to_string(gamma.join("todos/MEMORY.md")).unwrap();
        assert!(todos.contains("Write the migration"), "{todos}");
        assert!(
            lines.iter().any(|l| l.contains("Inherited 1 open todo(s)")),
            "{lines:?}"
        );
    }

    #[test]
    fn the_two_todo_lists_diverge_from_the_fork() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        run(dir.path(), "alpha", "gamma", false);
        let plane = crate::workspaces::Plane::open(dir.path());
        // Closing one in the fork must not rewrite the parent's plan.
        let slug = plane.workspace("gamma").unwrap().todos().unwrap()[0]
            .slug
            .clone();
        plane
            .workspace("gamma")
            .unwrap()
            .close_todo(&slug, now().naive_utc())
            .unwrap();
        assert_eq!(plane.workspace("gamma").unwrap().todos().unwrap().len(), 0);
        assert_eq!(plane.workspace("alpha").unwrap().todos().unwrap().len(), 1);
    }

    #[test]
    fn the_forks_manifest_names_the_parent_and_keeps_the_key_order_it_read() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        let alpha = dir.path().join("workspaces/alpha/workspace.json");
        std::fs::write(
            &alpha,
            r#"{
  "name": "alpha",
  "description": "the widget",
  "repos": [
    {
      "name": "svc",
      "branch": "main"
    }
  ],
  "updated_at": "2026-03-02T09:00:00+00:00",
  "updated_by": "fixture",
  "charter_generated": "0000000000000000000000000000000000000000000000000000000000000000"
}
"#,
        )
        .unwrap();
        run(dir.path(), "alpha", "gamma", false);
        let text =
            std::fs::read_to_string(dir.path().join("workspaces/gamma/workspace.json")).unwrap();
        let keys: Vec<&str> = text
            .lines()
            .filter_map(|l| l.trim().strip_prefix('"'))
            .filter_map(|l| l.split_once("\": "))
            .map(|(k, _)| k)
            .collect();
        // charter assigns into the dict it read, so a key that is already there keeps its
        // position and `forked_from` — the one key a fork adds — lands last.
        assert_eq!(
            keys.first().copied(),
            Some("name"),
            "the parent's own key order is kept: {text}"
        );
        assert_eq!(keys.last().copied(), Some("forked_from"), "{text}");
        assert!(text.contains("\"description\": \"the widget\""), "{text}");
        assert!(text.contains("\"forked_from\": \"alpha\""), "{text}");
    }

    #[test]
    fn a_manifest_branch_beats_the_disk_and_a_disk_only_repo_is_said_out_loud() {
        let recorded = vec![
            serde_json::json!({"name": "svc", "branch": "main"}),
            // Membership with no branch: charter's own writer's shape. It must NOT take the
            // repo off the disk-only list, which is the whole of charter#884's correction.
            serde_json::json!({"name": "tool"}),
        ];
        let disk = BTreeMap::from([
            ("svc".to_string(), "feature/x".to_string()),
            ("tool".to_string(), "trunk".to_string()),
            ("extra".to_string(), "main".to_string()),
        ]);
        let (rows, disk_only) = merge_repo_rows(&recorded, &disk);
        assert_eq!(
            rows,
            vec![
                ("extra".to_string(), "main".to_string()),
                ("svc".to_string(), "main".to_string()),
                ("tool".to_string(), "trunk".to_string()),
            ]
        );
        assert_eq!(disk_only, vec!["extra".to_string(), "tool".to_string()]);
    }

    #[test]
    fn a_workspace_that_recorded_a_repo_it_no_longer_has_still_inherits_it() {
        // The other half of the union: a teammate who has just cloned the plane has no repos
        // on disk at all, and the snapshot is the only thing that can tell the fork what to
        // clone.
        let recorded = vec![serde_json::json!({"name": "svc", "branch": "main"})];
        let (rows, disk_only) = merge_repo_rows(&recorded, &BTreeMap::new());
        assert_eq!(rows, vec![("svc".to_string(), "main".to_string())]);
        assert!(disk_only.is_empty(), "{disk_only:?}");
    }

    #[test]
    fn a_memory_store_charter_cannot_list_is_named_and_nothing_of_it_is_copied() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        let memory = dir.path().join("workspaces/alpha/memory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&memory, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let (code, lines) = run(dir.path(), "alpha", "gamma", false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&memory, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // The fork EXISTS and the exit says what it is missing. Both halves matter: raising
        // here cost the operator the whole fork.
        assert_eq!(code, 1, "{lines:?}");
        assert!(dir.path().join("workspaces/gamma/workspace.md").exists());
        assert!(
            lines[0].contains("without the memory charter could not read in 'alpha'"),
            "{lines:?}"
        );
        assert!(lines[1].contains("cannot be checked"), "{lines:?}");
        // The fork's own note says what it did not inherit, so a later reader of that memory
        // is not told it holds context it never received.
        let index =
            std::fs::read_to_string(dir.path().join("workspaces/gamma/memory/MEMORY.md")).unwrap();
        assert!(index.contains("without the memory"), "{index}");
        assert!(
            !index.contains("The API returns 418"),
            "nothing of the parent's memory was copied: {index}"
        );
    }

    #[test]
    fn a_memory_that_is_a_link_out_of_the_plane_is_named_and_never_copied() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.md"), "not yours\n").unwrap();
        let planted = dir.path().join("workspaces/alpha/memory/leak.md");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret.md"), &planted).unwrap();
        let (code, lines) = run(dir.path(), "alpha", "gamma", false);
        assert_eq!(code, 1, "{lines:?}");
        let leaked = dir.path().join("workspaces/gamma/memory/leak.md");
        assert!(
            !leaked.exists(),
            "a link out of the plane is not a file this fork inherits"
        );
        assert!(
            lines.iter().any(|l| l.contains("leak.md")),
            "carrying nothing and saying nothing is the one answer a fork may not give: \
             {lines:?}"
        );
    }

    #[test]
    fn a_name_that_is_not_a_workspace_name_forks_nothing() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        let (code, lines) = run(dir.path(), "alpha", "../esc", false);
        assert_eq!(code, 1);
        assert!(
            lines[0].contains("invalid workspace name '../esc'"),
            "{lines:?}"
        );
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(!dir.path().parent().unwrap().join("esc").exists());
    }

    #[test]
    fn forking_onto_a_workspace_that_exists_changes_neither() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        ensure::ensure(dir.path(), "beta", now(), "fixture").unwrap();
        let before =
            std::fs::read_to_string(dir.path().join("workspaces/beta/workspace.md")).unwrap();
        let (code, lines) = run(dir.path(), "alpha", "beta", false);
        assert_eq!(code, 1);
        assert!(
            lines[0].contains("workspace 'beta' already exists"),
            "{lines:?}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("workspaces/beta/workspace.md")).unwrap(),
            before
        );
    }

    #[test]
    fn forking_a_workspace_onto_its_own_name_is_refused_before_anything_is_read() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        let (code, lines) = run(dir.path(), "alpha", "alpha", false);
        assert_eq!(code, 1);
        assert!(lines[0].contains("nothing to fork"), "{lines:?}");
    }

    #[test]
    fn a_source_that_is_no_workspace_forks_nothing() {
        let dir = plane();
        let (code, lines) = run(dir.path(), "nowhere", "gamma", false);
        assert_eq!(code, 1);
        assert!(lines[0].contains("no workspace 'nowhere'"), "{lines:?}");
        assert!(!dir.path().join("workspaces/gamma").exists());
    }

    #[test]
    fn a_live_fork_is_un_ignored_and_told_how_to_share_itself() {
        let dir = plane();
        a_parent(dir.path(), "alpha");
        let (_code, lines) = run(dir.path(), "alpha", "gamma", true);
        assert!(lines[0].contains("(LIVE)"), "{lines:?}");
        assert!(
            lines
                .last()
                .unwrap()
                .contains("charter workspace save gamma"),
            "{lines:?}"
        );
        let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(
            ignore.contains("!/workspaces/gamma/workspace.json"),
            "{ignore}"
        );
    }

    #[test]
    fn the_pieces_a_fork_could_not_read_are_listed_the_way_charter_lists_them() {
        assert_eq!(and_list([]), "");
        assert_eq!(and_list(["memory"]), "memory");
        assert_eq!(and_list(["memory", "todos"]), "memory and todos");
        assert_eq!(
            and_list(["workspace.md", "memory", "todos"]),
            "workspace.md, memory and todos"
        );
    }
}
