//! What a repository's own config says about where its files land — READ, never asked.
//!
//! A port of `charter/gitconfig.py`: `configured_work_tree` and the small config reader under it.
//! It exists for one caller, the plane-root guards' subject list
//! ([`crate::planeroot::git_target`]), and for one key.
//!
//! `core.worktree` is git's **third** spelling of the work tree, and the only one that is not in
//! the invocation. `--work-tree` and `GIT_WORK_TREE` are tokens the guard already reads off the
//! command line and the environment. This one is a key in the repository's `.git/config`: a
//! repository carrying it has the named directory as its working tree for EVERY command, so a
//! guard reading argv and environment sees a plain `git checkout feature` typed inside a
//! workspace clone while the plane root's files are the ones replaced (charter #504, verified end
//! to end on git 2.50.1).
//!
//! **Read rather than asked**, because this runs inside every Bash `PreToolUse` and `git
//! rev-parse --show-toplevel` costs a process. One walk up to the repository and one bounded read
//! of a file that is under a kilobyte in every repository anybody has.
//!
//! **What is deliberately NOT read** — the Python's list, kept: `git -c core.worktree=…` (git
//! ignores it, verified), `include`/`includeIf` (a second read with git's own matchers behind it,
//! on the hot path), and the global and system configs (git honours the key there only with
//! `$GIT_DIR` set).
//!
//! **Every failure answers "no work tree named".** This ADDS a subject to a guard's list, so an
//! answer it could not produce leaves the guard exactly as strong as it was before the module
//! existed, while an invented one would refuse a command with nothing wrong with it.
//!
//! # Paths are Python's STRINGS here
//!
//! The Python hands `pathlib.Path` objects around, and what the guard finally compares is the
//! string `realpath` makes of one. So this module speaks in the strings `str(Path(...))` would
//! produce ([`crate::pypath::pure_path`], [`crate::pypath::path_div`]) rather than in
//! `std::path::PathBuf`, whose `join` and normalisation are different functions with the same
//! names — `PathBuf::join` keeps a `.` component that `Path` drops, and a trailing slash that
//! `Path` drops too.
//!
//! # One place this does not follow the Python, and why
//!
//! The Python `open`s `<git dir>/config` without asking what it is, so a FIFO there blocks the
//! hook until the host's own timeout — which a host reads as a non-blocking error, i.e. an
//! ALLOW. This reads only a REGULAR file and answers "no work tree named" for anything else. A
//! directory and a missing file answer that in both implementations already (`open` raises
//! `OSError`); a device or a FIFO is the only input on which the two can differ, and on those the
//! Python has no answer to compare against.

use std::fs::File;
use std::io::{ErrorKind, Read};

use crate::memstore::{is_python_space, py_strip};
use crate::pypath::{is_abs, join, normpath, path_div, pure_path};

/// The most of a repository config that is read before this gives up — `MAX_CONFIG_BYTES`.
///
/// A git config is a small hand-written file. The bound is here so a file that is not that
/// cannot make a `PreToolUse` guard read a gigabyte before it answers; past it the tail is not
/// parsed, which answers "no work tree named" for a key that lived past the bound — the
/// fail-open direction the module commits to, and better than a hook that never returns.
pub const MAX_CONFIG_BYTES: u64 = 262_144;

/// The most of a `.git` FILE that is read — `_MAX_POINTER_BYTES`. It holds one `gitdir:` line.
const MAX_POINTER_BYTES: u64 = 4096;

/// The directory `core.worktree` makes this invocation's work tree, or `None` —
/// `configured_work_tree`.
///
/// `cwd` is where the command runs, after any `-C` (the caller's business). `git_dir` is the
/// repository the invocation NAMED, if it named one: with `--git-dir`/`GIT_DIR` there is no
/// discovery to do, and without one git ascends from the cwd, so this does the same.
///
/// A relative value resolves against the **git directory**, git's own documented rule, which the
/// Python checked rather than assumed: `worktree = ../../plane` in `<clone>/.git/config` answers
/// `<clone>/../plane`.
///
/// Never panics, and every failure is `None`.
pub fn configured_work_tree(cwd: &str, git_dir: Option<&str>) -> Option<String> {
    let gd = match git_dir {
        None => git_dir_at(cwd)?,
        // `_as_path(git_dir)`: `Path(value)`, which cannot fail for a string.
        Some(named) => pure_path(named),
    };
    let value = core_worktree(&gd)?;
    if value.is_empty() {
        return None;
    }
    Some(if is_abs(&value) {
        pure_path(&value)
    } else {
        path_div(&gd, &value)
    })
}

/// The `.git` git would discover from `cwd`, without running git — `_git_dir_at`.
///
/// Ascends the way git does, and stops at the first `.git` rather than the first one that looks
/// like a repository: git stops there too. A `.git` **file** (a submodule, a linked worktree) is
/// followed one hop to the directory it names — one, because git writes exactly one level.
///
/// **The ascent is LEXICAL**, because the Python's is: `os.path.abspath` collapses `..` by
/// string before `Path.parents` walks up, so `<dir>/link/..` ascends from `<dir>` whatever `link`
/// points at. That is the oracle's answer and it is kept.
pub fn git_dir_at(cwd: &str) -> Option<String> {
    // `Path(os.path.abspath(Path(cwd)))`.
    let pure = pure_path(cwd);
    let start = if is_abs(&pure) {
        normpath(&pure)
    } else {
        // `os.getcwd()` raising is `except (OSError, ValueError): return None`.
        normpath(&join(&current_dir()?, &pure))
    };
    let start = pure_path(&start);
    let mut directory = start;
    loop {
        let dot = path_div(&directory, ".git");
        match kind_of(&dot) {
            Ok(Kind::Dir) => return Some(dot),
            Ok(Kind::File) => return pointer_target(&dot, &directory),
            Ok(Kind::Other) => {}
            // `except OSError: return None` — an error `pathlib` does not swallow stops the walk.
            Err(()) => return None,
        }
        let parent = parent_of(&directory);
        if parent == directory {
            return None;
        }
        directory = parent;
    }
}

/// `os.getcwd()`, or `None` where it cannot be read (a deleted directory), which the Python
/// raises and `_git_dir_at` answers with `None`.
fn current_dir() -> Option<String> {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
}

/// `Path.parent` for a string [`pure_path`] produced: the last component off, the root kept.
fn parent_of(path: &str) -> String {
    let root = if path.starts_with("//") {
        "//"
    } else if path.starts_with('/') {
        "/"
    } else {
        ""
    };
    let body = &path[root.len()..];
    match body.rfind('/') {
        Some(i) => format!("{root}{}", &body[..i]),
        None if root.is_empty() => ".".to_string(),
        None => root.to_string(),
    }
}

enum Kind {
    Dir,
    File,
    Other,
}

/// `dot.is_dir()` then `dot.is_file()`, with CPython 3.12's error handling: `pathlib` answers
/// `False` for a path that is missing, not a directory, a loop or a bad descriptor, and RAISES
/// for anything else — which `_git_dir_at` turns into `None`.
fn kind_of(path: &str) -> Result<Kind, ()> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => Ok(Kind::Dir),
        Ok(m) if m.is_file() => Ok(Kind::File),
        Ok(_) => Ok(Kind::Other),
        Err(e) => {
            let swallowed = loop_or_bad_descriptor(&e);
            match e.kind() {
                ErrorKind::NotFound | ErrorKind::NotADirectory => Ok(Kind::Other),
                // A NUL is `ValueError` to Python, which `is_dir` also swallows.
                ErrorKind::InvalidInput => Ok(Kind::Other),
                _ if swallowed => Ok(Kind::Other),
                _ => Err(()),
            }
        }
    }
}

/// `ELOOP` or `EBADF` — the two errors `pathlib`'s `_ignore_error` swallows that have no
/// `ErrorKind` of their own on stable Rust.
fn loop_or_bad_descriptor(e: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        [rustix::io::Errno::LOOP, rustix::io::Errno::BADF]
            .iter()
            .any(|n| e.raw_os_error() == Some(n.raw_os_error()))
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        false
    }
}

/// Up to `limit` bytes of a REGULAR file, or `None`.
fn read_bounded(path: &str, limit: u64) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut buf = Vec::new();
    File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

/// The directory a `.git` file names, or `None` — `_pointer_target`.
fn pointer_target(dot: &str, base: &str) -> Option<String> {
    let head = read_bounded(dot, MAX_POINTER_BYTES)?;
    let text = String::from_utf8_lossy(&head);
    let line = py_splitlines(&text).into_iter().next()?;
    let (label, target) = line.split_once(':')?;
    if py_strip(label).to_lowercase() != "gitdir" {
        return None;
    }
    let target = py_strip(target);
    if target.is_empty() {
        return None;
    }
    Some(if is_abs(target) {
        pure_path(target)
    } else {
        path_div(base, target)
    })
}

/// `core.worktree` out of `<git_dir>/config`, as git reads it — `_core_worktree`.
fn core_worktree(git_dir: &str) -> Option<String> {
    let raw = read_bounded(&path_div(git_dir, "config"), MAX_CONFIG_BYTES)?;
    let text = String::from_utf8_lossy(&raw);
    // The common case, answered without parsing: most repositories have no `worktree` key.
    if !text.to_lowercase().contains("worktree") {
        return None;
    }
    let mut found = None;
    for (section, subsection, key, value) in pairs(&text) {
        // A subsection makes it `core.<sub>.worktree`, a different key that relocates nothing.
        if section == "core" && subsection.is_empty() && key == "worktree" {
            found = Some(value);
        }
    }
    found
}

/// `(section, subsection, key, value)` for a git config file's text — `_pairs`.
///
/// A deliberately small subset of git's parser, and the subset is the Python's: section headers
/// with an optional quoted subsection, `key = value` lines, `#`/`;` comments outside quotes,
/// quoted values with git's escapes, and a trailing backslash continuing a line. What it gets
/// wrong yields a value nobody matches, which adds nothing to any caller's list.
pub fn pairs(text: &str) -> Vec<(String, String, String, String)> {
    let mut out = Vec::new();
    let mut section = String::new();
    let mut subsection = String::new();
    for line in logical_lines(text) {
        let mut rest: &str = py_strip(&line);
        if rest.is_empty() {
            continue;
        }
        let header_rest;
        if rest.starts_with('[') {
            let Some(end) = rest.find(']') else {
                continue;
            };
            let head = py_strip(&rest[1..end]);
            header_rest = py_strip(&rest[end + 1..]).to_string();
            if head.contains('"') {
                let (name, sub) = head.split_once('"').unwrap_or((head, ""));
                section = py_strip(name).to_lowercase();
                // `sub.rsplit('"', 1)[0]`.
                subsection = match sub.rfind('"') {
                    Some(i) => sub[..i].to_string(),
                    None => sub.to_string(),
                };
            } else {
                section = head.to_lowercase();
                subsection = String::new();
            }
            if header_rest.is_empty() {
                continue;
            }
            // `[core] worktree = x` on one line is legal git.
            rest = &header_rest;
        }
        if rest.starts_with('#') || rest.starts_with(';') {
            continue;
        }
        let Some((key, value)) = rest.split_once('=') else {
            continue; // a valueless key is a boolean, and never names a directory
        };
        out.push((
            section.clone(),
            subsection.clone(),
            py_strip(key).to_lowercase(),
            scalar(value),
        ));
    }
    out
}

/// `text`'s lines, a trailing backslash joining a line to the next — `_logical_lines`.
pub fn logical_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = String::new();
    for line in py_splitlines(text) {
        let line = format!("{pending}{line}");
        pending.clear();
        if line.ends_with('\\') && !line.ends_with("\\\\") {
            pending = line[..line.len() - 1].to_string();
            continue;
        }
        out.push(line);
    }
    if !pending.is_empty() {
        out.push(pending);
    }
    out
}

/// A git config VALUE as git reads it — `_scalar`.
///
/// Leading whitespace is dropped; trailing whitespace only OUTSIDE quotes, which is why this is a
/// scanner and not a trim — `"/pa th  "` names a directory ending in two spaces. A `#` or `;`
/// outside quotes starts a comment. `\n`, `\t` and `\b` are git's escapes; any other character
/// after a backslash is itself.
pub fn scalar(raw: &str) -> String {
    let chars: Vec<char> = raw.trim_start_matches(is_python_space).chars().collect();
    let mut out: Vec<char> = Vec::new();
    let mut keep = 0usize;
    let mut quoted = false;
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() {
            let nxt = chars[i + 1];
            out.push(match nxt {
                'n' => '\n',
                't' => '\t',
                'b' => '\u{8}',
                other => other,
            });
            keep = out.len();
            i += 2;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
            i += 1;
            continue;
        }
        if (ch == '#' || ch == ';') && !quoted {
            break;
        }
        out.push(ch);
        if quoted || !is_python_space(ch) {
            keep = out.len();
        }
        i += 1;
    }
    out[..keep].iter().collect()
}

/// `str.splitlines()` — every line boundary Python knows, not only `\n`.
///
/// `\r\n` is one boundary; `\r`, `\n`, `\x0b`, `\x0c`, `\x1c`–`\x1e`, U+0085, U+2028 and U+2029
/// are one each. A trailing boundary does not make an empty last line.
pub fn py_splitlines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(std::mem::take(&mut cur));
            }
            '\n' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => out.push(std::mem::take(&mut cur)),
            other => cur.push(other),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_with_no_worktree_key_names_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let gd = dir.path().join(".git");
        std::fs::create_dir(&gd).unwrap();
        std::fs::write(gd.join("config"), "[core]\n\tbare = false\n").unwrap();
        assert_eq!(
            configured_work_tree(dir.path().to_str().unwrap(), None),
            None
        );
    }

    #[test]
    fn a_relative_worktree_resolves_against_the_git_directory_not_the_work_tree() {
        let dir = tempfile::tempdir().unwrap();
        let clone = dir.path().join("ws").join("clone");
        let gd = clone.join(".git");
        std::fs::create_dir_all(&gd).unwrap();
        std::fs::write(gd.join("config"), "[core]\n\tworktree = ../../plane\n").unwrap();
        let got = configured_work_tree(clone.join("sub").to_str().unwrap(), None).unwrap();
        assert_eq!(got, format!("{}/../../plane", gd.display()));
    }

    #[test]
    fn a_dot_git_file_is_followed_one_hop() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.git");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("config"), "[core] worktree = /abs/plane\n").unwrap();
        let wt = dir.path().join("wt");
        std::fs::create_dir(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("GitDir: {}\n", real.display())).unwrap();
        assert_eq!(
            configured_work_tree(wt.to_str().unwrap(), None).as_deref(),
            Some("/abs/plane")
        );
    }

    #[test]
    fn a_subsection_worktree_is_a_different_key() {
        let got = pairs("[core \"x\"]\n\tworktree = /p\n");
        assert_eq!(got[0].1, "x");
        let dir = tempfile::tempdir().unwrap();
        let gd = dir.path().join(".git");
        std::fs::create_dir(&gd).unwrap();
        std::fs::write(gd.join("config"), "[core \"x\"]\n\tworktree = /p\n").unwrap();
        assert_eq!(
            configured_work_tree(dir.path().to_str().unwrap(), None),
            None
        );
    }

    #[test]
    fn a_quoted_value_keeps_its_trailing_spaces_and_its_comment_characters() {
        assert_eq!(scalar(" \"/pa th  \" # c"), "/pa th  ");
        assert_eq!(scalar("\"a;b\"#x"), "a;b");
        assert_eq!(scalar("a\\tb"), "a\tb");
        assert_eq!(scalar("a \u{1f}"), "a");
    }

    #[test]
    fn a_trailing_backslash_joins_the_next_line_and_a_doubled_one_does_not() {
        assert_eq!(logical_lines("a\\\nb\nc\\\\\nd"), vec!["ab", "c\\\\", "d"]);
    }

    #[test]
    fn splitlines_breaks_where_python_breaks() {
        assert_eq!(
            py_splitlines("a\r\nb\rc\u{2028}d\u{1c}e\n"),
            vec!["a", "b", "c", "d", "e"]
        );
        assert!(py_splitlines("").is_empty());
    }

    #[test]
    fn a_fifo_config_answers_nothing_instead_of_hanging() {
        let dir = tempfile::tempdir().unwrap();
        let gd = dir.path().join(".git");
        std::fs::create_dir(&gd).unwrap();
        let fifo = gd.join("config");
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&fifo))
            .map(|s| s.success())
            .unwrap_or(false);
        if !made {
            return; // no mkfifo here; the regular-file test above still holds the rule
        }
        assert_eq!(
            configured_work_tree(dir.path().to_str().unwrap(), None),
            None
        );
    }
}
