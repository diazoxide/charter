//! `charter gl-refresh` — asking each clone's own forge what it says about the branch that
//! clone is on, and writing the answer into the cache [`crate::cistate`] reads.
//!
//! A port of `charter/glstate.py`'s refresh half and of `charter/commands.py:cmd_gl_refresh`.
//!
//! # The two halves of one feature, and the line between them is a credential
//!
//! [`crate::cistate`] reads `.charter/cache/glstate.json` and can never fetch. This module
//! fetches and never draws. That split is not an accident of layering — it is the whole
//! security property, and `cistate`'s own module docs state the other side of it:
//!
//! **the process that holds the forge token must not be the process that draws.** A render
//! runs on every turn, unattended, and composes values off the network into a line a terminal
//! interprets; a refresh runs rarely, in the background, and writes a file. Putting a token in
//! the first of those is what charter #326 was, one layer up. Python keeps them apart
//! (`glstate.read_for` versus `glstate.refresh`) and so does this: nothing in this module
//! renders anything, and nothing in `cistate` runs a forge CLI.
//!
//! The token itself is never charter's either. It lives in `gh` or `glab`, and
//! [`crate::forge`] only ever runs that CLI — the value is not read here, never put on a
//! command line, and never part of a message.
//!
//! # Why this exists at all, and what it closes
//!
//! Until it did, the app's CI column depended on a **Python** process running behind it:
//! `charter gl-refresh` was the only thing that wrote the cache the panels read, and spec
//! decision 14 forbids Python in any shipped path. `cistate`'s docs say so in as many words
//! ("`charter gl-refresh` — the detached Python process that fills this cache — is not
//! something a shipped path may run"). This is the answer to that paragraph.
//!
//! # The key is a path string, and both sides have to spell it the same way
//!
//! The cache is keyed by `str(<clone directory>)`. The writer walks the plane and the reader
//! walks the plane, and two programs reach the same checkout under two spellings all the time
//! — `/tmp` is a link to `/private/tmp`, `$CHARTER_ROOT` may be set to either. So the writer
//! here builds its keys out of [`crate::repos::clones`], which is the **same function the
//! panel uses to find the row it then asks the cache about**, and `cistate` falls back to
//! `contain::resolved` when the two spellings still differ.
//!
//! Pinned by `tests/the_refresher_writes_the_key_the_panel_reads.rs`, on the direct spelling
//! and through a link, because a fixture that writes the key the reader is about to build
//! cannot catch this by construction.

use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::contain;
use crate::forge::{self, Raised};
use crate::worktree::git;

/// The cache this writes, relative to the plane root — the same constant the reader uses, so
/// the two can never drift to two paths.
pub const CACHE: &str = crate::cistate::CACHE;

/// The spawn lock, beside the cache. `charter/glstate.py:_lock_file`.
///
/// Its CONTENT is the pid of an in-flight refresh (empty when none) and its MTIME is when
/// that last changed. Only the "none" half is written here: a refresh that has finished says
/// so, which is what moves Python's cooldown from the spawn to the completion.
///
/// **Nothing in the Rust charter reads it.** It is written because the Python status line on
/// the same plane does, and a plane the two implementations leave in different states is a
/// plane whose next render behaves differently depending on which charter last ran.
pub const LOCK: &str = ".charter/cache/glstate.refreshing";

/// What one clone's branch got: the entry written under its path.
///
/// The field order is the order `charter/glstate.py:refresh` builds the dict in
/// (`{"branch": …, "ts": …, **state}`), and the order is load-bearing: `json.dumps` writes a
/// dict in insertion order, so a writer that sorted would produce a different file for the
/// same facts.
fn entry(branch: &str, now: f64, state: &State) -> Value {
    let mut row = Map::new();
    row.insert("branch".into(), Value::String(branch.to_string()));
    row.insert("ts".into(), number(now));
    row.insert(
        "change".into(),
        match state.change {
            Some(n) => Value::Number(n.into()),
            None => Value::Null,
        },
    );
    row.insert(
        "ci".into(),
        match &state.ci {
            Some(word) => Value::String(word.clone()),
            None => Value::Null,
        },
    );
    row.insert("sigil".into(), Value::String(state.sigil.clone()));
    Value::Object(row)
}

/// A float as `json.dumps` would write it back. `serde_json` keeps the literal under
/// `arbitrary_precision`, and `pyjson` turns it into CPython's `repr`.
fn number(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// `{"change": …, "ci": …, "sigil": …}` for one clone's branch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub change: Option<u64>,
    pub ci: Option<String>,
    pub sigil: String,
}

/// `glstate._EMPTY`: no change, no CI word, and no sigil either.
///
/// The sigil goes with them, which is not obvious and is deliberate: an entry that names a
/// change number the refresh could not read has nothing to put a `#` or a `!` in front of.
fn empty() -> State {
    State::default()
}

/// A change identifier as the forge answered it, **coerced** —
/// `charter/glstate.py:_change_or_none`.
///
/// `change` was the one forge field that reached the rendered status line unchecked (charter
/// #326): it was whatever the JSON `number` (GitHub) or `iid` (GitLab) field happened to
/// hold, stored verbatim, cached verbatim, and interpolated into a line a terminal
/// interprets. A self-hosted or compromised instance, an altered response, or an edit to the
/// cache file — which nothing signs — could put an escape sequence there.
///
/// **Coerced rather than type-checked**, because a forge that serialises its id as `"42"` is
/// answering the question and only its JSON type is off; dropping that would blank a real
/// change number over a detail. Anything Python's `int()` refuses, and anything that is not a
/// positive identifier on any forge charter speaks to, becomes `None` — which is the same
/// thing "no open change" already looks like.
///
/// `true` is refused ahead of the number branch for the reason Python needs a
/// `isinstance(v, bool)` guard: `int(True)` is 1, and a boolean is not an identifier.
pub fn change_or_none(value: Option<&Value>) -> Option<u64> {
    let found = value?;
    let n: i128 = match found {
        Value::Null | Value::Bool(_) => return None,
        Value::Number(number) => {
            if let Some(whole) = number.as_i64() {
                i128::from(whole)
            } else {
                // `int(12.9)` is 12 and `int(-0.5)` is 0: Python truncates toward zero.
                let f = number.as_f64()?;
                if !f.is_finite() {
                    return None;
                }
                f.trunc() as i128
            }
        }
        Value::String(text) => python_int(text)?,
        // `int([])` and `int({})` are a TypeError, which `_change_or_none` catches.
        Value::Array(_) | Value::Object(_) => return None,
    };
    (n > 0).then_some(n as u64)
}

/// `int(text)` as CPython parses a base-10 string: surrounding whitespace, an optional sign,
/// and digits that may be separated by single underscores.
fn python_int(text: &str) -> Option<i128> {
    let body = text.trim_matches(|c: char| c.is_whitespace());
    let (sign, digits) = match body.strip_prefix('-') {
        Some(rest) => (-1i128, rest),
        None => (1i128, body.strip_prefix('+').unwrap_or(body)),
    };
    if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') {
        return None;
    }
    let mut value: i128 = 0;
    let mut previous_was_underscore = false;
    for c in digits.chars() {
        if c == '_' {
            // `1__0` is a ValueError; `1_0` is ten.
            if previous_was_underscore {
                return None;
            }
            previous_was_underscore = true;
            continue;
        }
        previous_was_underscore = false;
        let digit = c.to_digit(10)?;
        // A change id past this is not one any forge issues, and `checked_mul` is what stops
        // a hostile file turning the parse into a wrap.
        value = value.checked_mul(10)?.checked_add(i128::from(digit))?;
    }
    Some(sign * value)
}

/// The trees `gl-refresh` fetches for: the workspace's clones, and each clone's worktrees.
///
/// **The same list the panel draws**, which is the point Python's `workspace.repo_trees`
/// docstring makes: "a repo can never be drawn without its forge state having been fetched,
/// or fetched without being drawn. Splitting that decision in two is what left a tree with a
/// permanently empty CI column." Here that means [`crate::repos::clones`] and nothing else,
/// because that is what `app/src-tauri/src/panels.rs` asks.
///
/// Worktrees carry their own branch, so they carry their own pipeline and their own open
/// change, and Python refreshes them for that reason.
pub fn trees(plane: &Path, ws: &str) -> Result<Targets, String> {
    let found = crate::repos::clones(plane, ws).map_err(|why| why.to_string())?;
    // Every clone first, then every clone's worktrees — the order Python builds the list in
    // (`dirs = clones(ws)`, then `dirs += [w for d in dirs for w in dirs_for(…)]`), which is
    // also the order the entries are written to the cache in and therefore the file's own.
    let mut trees: Vec<PathBuf> = found.repos.iter().map(|repo| repo.path.clone()).collect();
    let mut refused = found.refused;
    for repo in &found.repos {
        for piece in worktrees_of(plane, ws, &repo.name) {
            // **The path git is about to be pointed at, gated as ITSELF.** A worktree
            // directory is one `read_dir` away from the plane and nothing above it says where
            // it lands: a committed `workspaces/<ws>/.worktrees/<repo>/<piece> -> elsewhere`
            // travels to every machine that clones the plane, and `git -C` through it would
            // read another repository's `origin` and then ask THAT forge about this branch.
            // `within_workspace` returns the path it checked, and that is the one kept, so
            // the string checked and the string used cannot be two different strings.
            match crate::worktree::confine::within_workspace(plane, ws, &piece) {
                Ok(checked) => trees.push(checked),
                // Named rather than dropped, for the reason `repos::clones` names its own: a
                // worktree that quietly disappears is a row whose CI cell stays empty with
                // nothing anywhere to say why. Python has no such refusal — `dirs_for` lists
                // whatever is there — so this is the Rust charter refreshing LESS, out loud.
                Err(why) => refused.push((
                    piece
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    why.to_string(),
                )),
            }
        }
    }
    Ok(Targets { trees, refused })
}

/// What a workspace offered a refresh, and what it would not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Targets {
    pub trees: Vec<PathBuf>,
    /// Directories charter refused, with the reason — a name it will not take for a repo, a
    /// `.git` that is a symlink, or a worktree that leaves the workspace.
    ///
    /// **Carried so the command can say them.** A repo that quietly disappears reads as "this
    /// workspace has one fewer repo", and the operator is the only one who can tell whether
    /// the link is theirs. Python has no such refusals (its `clones` admits any directory with
    /// a `.git`), so this is a place where the Rust charter refreshes LESS — which is only
    /// honest if it says so.
    pub refused: Vec<(String, String)>,
}

/// A clone's worktree directories, most recently touched first —
/// `charter/worktree.py:dirs_for`.
///
/// Filesystem-only, **no subprocess**: this is on the refresh path beside one `git remote` per
/// tree already, and `git worktree list` per clone would be paid for something one `read_dir`
/// answers. It is not a second registry — `git worktree add` creates these directories and
/// `git worktree remove` deletes them, so listing them IS reading git's own output.
fn worktrees_of(plane: &Path, ws: &str, repo: &str) -> Vec<PathBuf> {
    let base = crate::worktree::root_of(plane, ws).join(repo);
    let Ok(reader) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = reader
        .filter_map(Result::ok)
        .map(|item| item.path())
        .filter(|path| path.is_dir())
        .map(|path| {
            let when = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (when, path)
        })
        .collect();
    // Newest first, and the path breaks a tie so two directories written in the same tick do
    // not come out in `read_dir` order — which is the filesystem's, not an order at all.
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, path)| path).collect()
}

/// A tree's branch, read straight from `HEAD` — `charter/util.py:branch_of`.
///
/// `?` when unreadable, a short sha when detached, and the full ref name otherwise (branch
/// names legitimately contain slashes, so only `refs/heads/` is stripped).
///
/// **Filesystem-only on purpose**, and it must stay that way for a second reason beyond cost:
/// this value is written into the cache entry, and `cistate::about` compares its own reading
/// of the branch against it. Python shares one implementation between the two sides for
/// exactly that reason — "two implementations of 'what branch is this' is exactly the kind of
/// drift that shows up as a CI column that silently never renders".
pub fn branch_of(tree: &Path) -> String {
    let Some(gitdir) = git_dir(tree) else {
        return "?".into();
    };
    let Ok(text) = std::fs::read_to_string(gitdir.join("HEAD")) else {
        return "?".into();
    };
    let text = text.trim();
    if text.starts_with("ref:") {
        // Python's `txt.split("/", 2)[-1]`, spelled the same way rather than tidied: it splits
        // the WHOLE line — `ref:` and all — at the first two slashes and takes what is left,
        // so `ref: refs/heads/release/1.2` keeps its slash and a HEAD holding something that
        // is not a ref path at all (`ref: main`) comes back as it was written rather than as
        // `?`. The two are different answers, and the cache entry carries whichever it is.
        let name = text.splitn(3, '/').last().unwrap_or_default();
        return if name.is_empty() {
            "?".into()
        } else {
            name.to_string()
        };
    }
    if text.is_empty() {
        return "?".into();
    }
    // A detached HEAD: the short sha, by characters as Python slices a string.
    text.chars().take(7).collect()
}

/// The git directory backing `tree`, or `None` when `tree` is not a working tree.
///
/// Both shapes, because both are normal and a reader that handled only the first reported
/// every linked worktree's branch as `?`: a **clone**'s `.git` is a directory holding `HEAD`,
/// and a linked **worktree**'s `.git` is a FILE reading `gitdir: <path>`.
fn git_dir(tree: &Path) -> Option<PathBuf> {
    let dot = tree.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let rest = text.trim().strip_prefix("gitdir:")?;
    let named = PathBuf::from(rest.trim());
    // `git worktree add` writes an absolute path, but the format permits a relative one (and
    // `git worktree repair` produces it), resolved against the tree.
    Some(if named.is_absolute() {
        named
    } else {
        tree.join(named)
    })
}

/// A clone's raw `origin` URL, for inferring which forge hosts it.
///
/// Through the hardened runner, never a bare `Command::new("git")`: with `GIT_DIR` exported
/// this answered the EXPORTED repository's origin, and the refresh then asked that forge
/// about this clone's branch (charter #964). `git::run` clears the environment, so the
/// variable cannot reach the child at all.
fn remote_url(tree: &Path, timeout: std::time::Duration) -> Option<String> {
    let run = git::run(tree, &["remote", "get-url", "origin"], timeout).ok()?;
    let url = run.out.trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// What the forge says about one clone's branch — `glstate.state_for_repo`.
///
/// Takes the clone DIRECTORY, not a namespace string: the forge is inferred from that clone's
/// own `origin`, which is what makes a mixed-forge workspace work — two clones side by side
/// can be hosted on different forges, and each is asked over its own CLI.
///
/// **Best-effort by contract.** No remote, an unknown host, a missing CLI or an API failure
/// all degrade to the empty state. So does a JSON shape the call did not expect
/// ([`Raised`]), and it blanks all three fields together because Python's single `try`
/// around the whole dict does.
pub fn state_for_repo(plane: &Path, tree: &Path, branch: &str) -> State {
    // Two calls with two budgets, and BOTH are made before either is judged — which is what
    // Python does (`url = _remote_url(d); path = _remote_path(d); if not url or not path`).
    // `_remote_url` gets 10 seconds and `_remote_path` 3; kept as two rather than folded into
    // one, because the deadline is the observable difference and a port that quietly halved a
    // budget would be a port that blanks a column on a slow disk.
    let url = remote_url(tree, std::time::Duration::from_secs(10));
    let path = remote_url(tree, std::time::Duration::from_secs(3));
    let (Some(url), Some(path)) = (url, path.as_deref().and_then(forge::namespace_of)) else {
        return empty();
    };
    // Recognises a self-hosted forge this plane's own `charter.toml` declares, not just each
    // kind's default host. A host neither declared nor default is UNMANAGED, and charter
    // asks nothing about it rather than guessing which backend it might speak.
    let Some(found) = forge::resolve_host(&url, plane) else {
        return empty();
    };
    let change = match found.open_change(&path, branch) {
        Ok(value) => change_or_none(value.as_ref()),
        Err(Raised) => return empty(),
    };
    let ci = match found.ci_status(&path, branch) {
        Ok(word) => word,
        Err(Raised) => return empty(),
    };
    State {
        change,
        ci,
        sigil: found.kind.change_sigil().to_string(),
    }
}

/// The cache as it is on disk, or an empty document for every way there is none.
/// `charter/glstate.py:load`, which catches everything for the same reason.
pub fn load(plane: &Path) -> Map<String, Value> {
    std::fs::read_to_string(plane.join(CACHE))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|doc| doc.as_object().cloned())
        .unwrap_or_default()
}

/// Refresh every tree in `trees` and rewrite the cache. `charter/glstate.py:refresh`.
///
/// `now` is one instant for the whole run, not one per tree: Python takes `time.time()` once
/// before the loop, so every entry a refresh writes ages together.
///
/// Returns the cache as written, which is what the command's own report reads back.
pub fn refresh(plane: &Path, trees: &[PathBuf], now: f64) -> Map<String, Value> {
    let mut cache = load(plane);
    for tree in trees {
        let branch = branch_of(tree);
        let state = if branch.is_empty() || branch == "?" {
            empty()
        } else {
            state_for_repo(plane, tree, &branch)
        };
        cache.insert(key_for(tree), entry(&branch, now, &state));
    }
    let _ = save(plane, &cache);
    // The work is over — stop claiming to be in flight. LAST, so a refresh that dies partway
    // leaves its pid behind and is recognised as crashed rather than as finished.
    mark_done(plane);
    cache
}

/// The cache key for a tree: `str(d)` — the path as this walk spelled it.
///
/// A function rather than an inline `display()` so there is one place to point at when the
/// reader and the writer are compared, and so nothing here can start canonicalising: the
/// reader's fallback (`contain::resolved`) is what bridges two spellings, and a writer that
/// resolved would change the key on every plane reached through a link.
pub fn key_for(tree: &Path) -> String {
    tree.display().to_string()
}

/// Write the cache, private to the operator. Best-effort, as `glstate._save` is.
fn save(plane: &Path, cache: &Map<String, Value>) -> io::Result<()> {
    let path = plane.join(CACHE);
    private_dir(plane, path.parent().unwrap_or(plane))?;
    write_private(
        plane,
        &path,
        crate::pyjson::dumps(&Value::Object(cache.clone()), None, ", ", ": ").as_bytes(),
    )
}

/// Record that no refresh is in flight. `glstate._mark_done` → `_write_lock(None)`.
fn mark_done(plane: &Path) {
    let path = plane.join(LOCK);
    if private_dir(plane, path.parent().unwrap_or(plane)).is_ok() {
        let _ = write_private(plane, &path, b"");
    }
}

/// Create `dir` and every level of it charter has to create, at 0700 —
/// `charter/config.py:private_mkdir`.
///
/// **The umask must not decide the mode of the plane's state directory.** A plain
/// `create_dir_all` makes each level at `0o777 & ~umask`, which on the default `umask 022` is
/// 0755 — and `.charter/` is the directory the vault registry lives in. Rust's `DirBuilder`
/// applies its mode to every level it creates, which is the part CPython's `pathlib` does not.
///
/// A directory that already exists is left exactly as it is, which is Python's rule too:
/// charter tightens what it creates and reports what it did not, because `$CHARTER_HOME` can
/// point the state directory at a home or a shared team directory.
fn private_dir(plane: &Path, dir: &Path) -> io::Result<()> {
    crate::plane::private_dir(plane, dir)
}

/// Write charter's own state at 0600 — [`crate::plane::write_private`], which is where the
/// ordering of the chmod and the truncate is argued.
fn write_private(plane: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    crate::plane::write_private(plane, path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn taken(text: &str) -> Option<u64> {
        let value: Value = serde_json::from_str(text).unwrap();
        change_or_none(Some(&value))
    }

    #[test]
    fn a_change_is_coerced_the_way_python_coerces_it_and_refused_the_way_python_refuses() {
        assert_eq!(taken("41"), Some(41));
        // Coerced, not type-checked: a forge that serialises its id as a string is still
        // answering the question.
        assert_eq!(taken("\"42\""), Some(42));
        assert_eq!(taken("\" 42 \""), Some(42), "int() strips whitespace");
        assert_eq!(taken("\"1_0\""), Some(10), "int() takes digit separators");
        assert_eq!(taken("12.9"), Some(12), "int() truncates toward zero");
        // Not an identifier on any forge charter speaks to.
        assert_eq!(taken("0"), None);
        assert_eq!(taken("-3"), None);
        assert_eq!(taken("\"-3\""), None);
        assert_eq!(taken("null"), None);
        assert_eq!(taken("[]"), None);
        assert_eq!(taken("{}"), None);
        assert_eq!(taken("\"\""), None);
        assert_eq!(taken("\"1__0\""), None);
        assert_eq!(taken("\"0x10\""), None);
        // The one #326 is about: an escape sequence where a number was promised.
        assert_eq!(taken("\"\\u001b[2J\""), None);
        // `int(True)` is 1, and a boolean is not a change. Without the bool arm ahead of the
        // number arm this is `Some(1)` and every repo grows a phantom `#1`.
        assert_eq!(taken("true"), None);
        assert_eq!(change_or_none(None), None);
    }

    #[test]
    fn a_branch_is_the_ref_name_with_its_slashes_kept() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("clone");
        std::fs::create_dir_all(tree.join(".git")).unwrap();

        std::fs::write(tree.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(branch_of(&tree), "main");

        std::fs::write(tree.join(".git/HEAD"), "ref: refs/heads/release/1.2\n").unwrap();
        assert_eq!(
            branch_of(&tree),
            "release/1.2",
            "only refs/heads/ is stripped"
        );

        std::fs::write(tree.join(".git/HEAD"), "ref: refs/heads/\n").unwrap();
        assert_eq!(branch_of(&tree), "?", "a ref pointing at no branch");

        // Python splits the whole line at the first two slashes and takes the rest, so a HEAD
        // that names no ref path comes back as it was written. Not a shape git produces — and
        // the point is that the two charters answer a file neither of them wrote identically.
        std::fs::write(tree.join(".git/HEAD"), "ref: main\n").unwrap();
        assert_eq!(branch_of(&tree), "ref: main");

        std::fs::write(tree.join(".git/HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(
            branch_of(&tree),
            "0123456",
            "a detached HEAD is a short sha"
        );

        std::fs::write(tree.join(".git/HEAD"), "\n").unwrap();
        assert_eq!(branch_of(&tree), "?");

        std::fs::remove_file(tree.join(".git/HEAD")).unwrap();
        assert_eq!(branch_of(&tree), "?");
    }

    #[test]
    fn a_linked_worktrees_branch_is_read_through_its_gitdir_file() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("store/worktrees/piece");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("HEAD"), "ref: refs/heads/piece-x\n").unwrap();
        let tree = dir.path().join("piece");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();

        // Without the file form every linked worktree reads `?`, which is the whole branch
        // column of a monorepo plane.
        assert_eq!(branch_of(&tree), "piece-x");
    }

    #[test]
    fn an_entry_is_written_in_the_order_python_builds_it() {
        let state = State {
            change: Some(41),
            ci: Some("success".into()),
            sigil: "#".into(),
        };
        let written = crate::pyjson::dumps(&entry("main", 1777980737.0, &state), None, ", ", ": ");

        // Verified against CPython:
        // json.dumps({"branch": "main", "ts": 1777980737.0, "change": 41,
        //             "ci": "success", "sigil": "#"})
        assert_eq!(
            written,
            r##"{"branch": "main", "ts": 1777980737.0, "change": 41, "ci": "success", "sigil": "#"}"##
        );
    }

    #[test]
    fn an_entry_with_nothing_fetched_still_names_the_branch_and_the_instant() {
        let written =
            crate::pyjson::dumps(&entry("main", 1777980737.0, &empty()), None, ", ", ": ");

        assert_eq!(
            written,
            r#"{"branch": "main", "ts": 1777980737.0, "change": null, "ci": null, "sigil": ""}"#
        );
    }

    #[test]
    fn the_cache_and_the_lock_are_private_and_the_reader_finds_the_cache_where_it_looks() {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        let mut cache = Map::new();
        cache.insert("/x/svc".into(), json!({"branch": "main"}));

        save(&plane, &cache).unwrap();
        mark_done(&plane);

        assert_eq!(
            std::fs::read_to_string(plane.join(CACHE)).unwrap(),
            r#"{"/x/svc": {"branch": "main"}}"#
        );
        assert_eq!(std::fs::read_to_string(plane.join(LOCK)).unwrap(), "");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for named in [CACHE, LOCK] {
                let mode = std::fs::metadata(plane.join(named))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777;
                assert_eq!(mode, 0o600, "{named} is {mode:o}");
            }
        }
    }

    #[test]
    fn a_cache_reached_through_a_link_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(dir.path()).unwrap();
        let plane = here.join("plane");
        let outside = here.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(plane.join(".charter")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, plane.join(".charter/cache")).unwrap();

        // A link at a DIRECTORY on the way. `private_dir` refuses it and the write never
        // starts.
        assert!(save(&plane, &Map::new()).is_err());
        assert!(!outside.join("glstate.json").exists());
    }

    #[test]
    fn a_cache_that_is_itself_a_link_is_not_written_through() {
        // **The other half, and the one a check on the parent cannot see.** `cistate` says it
        // about the read side in as many words — "a link at `glstate.json` redirects the read
        // exactly as one at the directory does, and a check on the parent cannot see it" —
        // and the write side is the same path with the arrow reversed. `.charter/cache/` here
        // is an ordinary directory; only the file is a link.
        //
        // It is held by TWO things at once, deliberately: the walk in `write_private` and the
        // `O_NOFOLLOW` on the open. That is `contain::open_no_link`'s own pairing — the walk
        // answers about a path and does not hold it, and the flag moves the last component's
        // answer to the instant of the open — so neither alone going missing shows up here.
        // What this pins is the property, and what a mutation of ONE of them proves is only
        // that the other is still there.
        let dir = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(dir.path()).unwrap();
        let plane = here.join("plane");
        let outside = here.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("theirs.json"), "NOT CHARTER'S\n").unwrap();
        std::fs::create_dir_all(plane.join(".charter/cache")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("theirs.json"), plane.join(CACHE)).unwrap();

        assert!(save(&plane, &Map::new()).is_err());
        assert_eq!(
            std::fs::read_to_string(outside.join("theirs.json")).unwrap(),
            "NOT CHARTER'S\n",
            "the cache was written through the link"
        );
    }
}
