//! The one memory fetch across every base — `charter/recall.py`.
//!
//! A plane keeps memory on two independent axes: the active workspace's journal, and the
//! persona's own memory beside the store every persona shares, with the curated `refs/`
//! documents and the session's ephemeral scratch alongside. Storage stays per base; READING
//! is unified here, so a caller asks once and gets ranked hits, each labelled with the base
//! it came from.
//!
//! **Every entry of every base is read through [`memstore::read_files`]**, whose per-entry
//! gate is the one `duplicate_of` was missing when M1.1's last containment hole was found:
//! a committed `leak.md -> /outside/secret.md` read, echoed, and used as an oracle for the
//! rest of the file. This module never opens a memory any other way.
//!
//! What it does NOT decide: which workspace and which persona are "active". charter
//! resolves both through a ladder of pointers and settings this binary has not ported, so
//! a caller names them, and a scope with no owner named is the caller's refusal to make.

use std::path::{Path, PathBuf};

use crate::memstore::{self, Found, Unread};

/// The selectable scopes, in the order they are assembled — `recall.SCOPES`.
pub const SCOPES: [&str; 5] = ["workspace", "persona", "shared", "ephemeral", "refs"];

/// The scopes searched when none are named. `refs` is in and `ephemeral` is not: refs are
/// committed and curated, ephemeral is session scratch (#178).
pub const DEFAULT_SCOPES: [&str; 4] = ["workspace", "persona", "shared", "refs"];

/// Which workspace bases are searched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Workspaces {
    /// This one workspace's journal.
    One(String),
    /// Every workspace's, one base each.
    All,
}

/// What to search, with every owner already named.
#[derive(Debug, Clone)]
pub struct Ask {
    pub scopes: Vec<String>,
    pub workspaces: Option<Workspaces>,
    pub persona: Option<String>,
    /// The session's bucket, for the ephemeral base (`session.bucket`).
    pub session: String,
    pub query: Option<String>,
    /// `0` is no cap. Negative is Python's slice from the end, as `rows[:limit]` gives it.
    pub limit: i64,
    pub since: Option<chrono::NaiveDate>,
}

/// One recalled memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub label: String,
    pub path: PathBuf,
    pub title: String,
    pub score: usize,
    pub date: Option<chrono::NaiveDate>,
}

/// Hits, and what was withheld — `recall.Recalled`.
#[derive(Debug, Clone, Default)]
pub struct Recalled {
    pub hits: Vec<Hit>,
    /// Memories `since` could not place in time and therefore dropped.
    pub undated: usize,
    /// Refs documents `since` excluded: a ref never carries a date, so this is counted apart
    /// from memories that lost one.
    pub undated_refs: usize,
    /// Each base, or entry, that could not be looked at — named by the caller, because a
    /// search that did not look is not a search that found nothing (#1084).
    pub unread: Unread,
}

/// `items[:n]`, with Python's meaning for a negative `n`.
pub fn py_head<T>(mut items: Vec<T>, n: i64) -> Vec<T> {
    let len = items.len() as i64;
    let keep = if n >= 0 { n.min(len) } else { (len + n).max(0) };
    items.truncate(keep as usize);
    items
}

/// The earliest date a `--since` accepts — `recall.parse_since`.
///
/// An age (`14d`, `2w`, `3m`, a month being 30 days) or an ISO date. Anything else is an
/// error rather than no filter: a silently ignored `--since` returns MORE than was asked
/// for, and the reader takes a full corpus as proof that nothing was recorded recently.
pub fn parse_since(value: &str, today: chrono::NaiveDate) -> Result<chrono::NaiveDate, String> {
    let s = memstore::py_strip(value);
    let refused = || {
        format!(
            "unrecognised --since {} — use an age (14d, 2w, 3m) or a date (2026-07-01)",
            crate::pyrepr::repr_str(value)
        )
    };
    if let Some(unit) = s.chars().last()
        && matches!(unit, 'd' | 'w' | 'm')
    {
        let digits = &s[..s.len() - 1];
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            let n: i64 = digits.parse().map_err(|_| refused())?;
            let per = match unit {
                'd' => 1,
                'w' => 7,
                _ => 30,
            };
            let days = n.checked_mul(per).ok_or_else(refused)?;
            return today
                .checked_sub_days(chrono::Days::new(days as u64))
                .filter(|d| chrono::Datelike::year(d) >= 1)
                .ok_or_else(refused);
        }
    }
    from_isoformat(s).ok_or_else(refused)
}

/// `date.fromisoformat` as Python 3.11 reads it: `YYYY-MM-DD`, `YYYYMMDD`, and the week
/// forms `YYYY-Www`, `YYYY-Www-D`, `YYYYWww`, `YYYYWwwD`.
pub fn from_isoformat(s: &str) -> Option<chrono::NaiveDate> {
    if !s.is_ascii() {
        return None;
    }
    let num = |t: &str| -> Option<u32> {
        (!t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse().ok())
            .flatten()
    };
    let year = |t: &str| num(t).filter(|y| (1..=9999).contains(y)).map(|y| y as i32);
    let week = |y: i32, w: u32, d: u32| {
        if !(1..=7).contains(&d) {
            return None;
        }
        let weekday = chrono::Weekday::try_from((d - 1) as u8).ok()?;
        chrono::NaiveDate::from_isoywd_opt(y, w, weekday)
    };
    match s.len() {
        10 if &s[4..5] == "-" && &s[7..8] == "-" => {
            chrono::NaiveDate::from_ymd_opt(year(&s[..4])?, num(&s[5..7])?, num(&s[8..])?)
        }
        8 if &s[4..5] == "-" && &s[5..6] == "W" => week(year(&s[..4])?, num(&s[6..])?, 1),
        10 if &s[4..5] == "-" && &s[5..6] == "W" && &s[8..9] == "-" => {
            week(year(&s[..4])?, num(&s[6..8])?, num(&s[9..])?)
        }
        8 if &s[4..5] == "W" => week(year(&s[..4])?, num(&s[5..7])?, num(&s[7..])?),
        7 if &s[4..5] == "W" => week(year(&s[..4])?, num(&s[5..])?, 1),
        8 => chrono::NaiveDate::from_ymd_opt(year(&s[..4])?, num(&s[4..6])?, num(&s[6..])?),
        _ => None,
    }
}

/// Whether an errno is `ELOOP` — a symlink loop, whose fix is the link and not a mode.
#[cfg(unix)]
pub fn is_loop(code: Option<i32>) -> bool {
    code == Some(rustix::io::Errno::LOOP.raw_os_error())
}

/// Whether an errno is `ELOOP`. No platform this runs on without `rustix` reports one.
#[cfg(not(unix))]
pub fn is_loop(_code: Option<i32>) -> bool {
    false
}

/// A directory's entries sorted, or what stopped the listing.
fn listing(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    out.sort();
    Ok(out)
}

/// Whether `path`, through any link, is a directory — `Some(bool)` — or `None` with the
/// errno when the filesystem will not say (`workspace._directory`).
fn directory(path: &Path) -> Result<bool, Option<i32>> {
    match std::fs::metadata(path) {
        Ok(meta) => Ok(meta.is_dir()),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(false)
        }
        Err(e) => Err(e.raw_os_error()),
    }
}

/// The plane's workspace names, and each entry under `workspaces/` whose kind the
/// filesystem would not tell — `workspace.read_workspaces`.
///
/// A name starting `.` is charter's own, and a directory holding a `.git` DIRECTORY is a
/// clone somebody dropped there, not a workspace. A `workspaces/` that cannot be listed is
/// the one error, and it is the caller's to name.
pub fn read_workspaces(root: &Path) -> std::io::Result<(Vec<String>, Unread)> {
    let dir = root.join("workspaces");
    let entries = match listing(&dir) {
        Ok(entries) => entries,
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok((Vec::new(), Vec::new()));
        }
        Err(e) => return Err(e),
    };
    let mut names = Vec::new();
    let mut unread = Vec::new();
    for path in entries {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if name.starts_with('.') {
            continue;
        }
        match directory(&path) {
            Ok(true) if directory(&path.join(".git")) != Ok(true) => names.push(name),
            Ok(_) => {}
            Err(code) => unread.push((path, code)),
        }
    }
    Ok((names, unread))
}

/// `[(label, directory)]` for the scopes asked, in charter's order: workspace, persona,
/// ephemeral, shared, refs — `recall.sources`. What could not be read under a `refs/` goes
/// to the second list and the rest is still offered.
pub fn sources(root: &Path, ask: &Ask) -> (Vec<(String, PathBuf)>, Unread) {
    let has = |scope: &str| ask.scopes.iter().any(|s| s == scope);
    let mut out = Vec::new();
    let mut missed = Vec::new();
    if has("workspace") {
        match &ask.workspaces {
            Some(Workspaces::All) => {
                for name in read_workspaces(root).map(|(n, _)| n).unwrap_or_default() {
                    let dir = root.join("workspaces").join(&name).join("memory");
                    out.push((format!("workspace:{name}"), dir));
                }
            }
            Some(Workspaces::One(name)) => {
                let dir = root.join("workspaces").join(name).join("memory");
                out.push((format!("workspace:{name}"), dir));
            }
            None => {}
        }
    }
    let personas = root.join("personas");
    if let Some(name) = &ask.persona {
        if has("persona") {
            out.push((
                format!("persona:{name}"),
                personas.join(name).join("memory"),
            ));
        }
        if has("ephemeral") {
            let dir = ephemeral_dir(root, &ask.session, name);
            out.push((format!("persona:{name}:ephemeral"), dir));
        }
    }
    if has("shared") {
        out.push((
            "shared".to_string(),
            personas.join(crate::contain::SHARED_PERSONA).join("memory"),
        ));
    }
    if has("refs") {
        if let Some(name) = &ask.persona {
            for dir in ref_dirs(root, &personas.join(name).join("refs"), &mut missed) {
                out.push((format!("refs:{name}"), dir));
            }
        }
        let shared = personas.join(crate::contain::SHARED_PERSONA).join("refs");
        for dir in ref_dirs(root, &shared, &mut missed) {
            out.push(("refs:shared".to_string(), dir));
        }
    }
    (out, missed)
}

/// A persona's ephemeral scratch for one session:
/// `.charter/persona-state/ephemeral/<session>/<persona>`.
pub fn ephemeral_dir(root: &Path, session: &str, persona: &str) -> PathBuf {
    root.join(".charter/persona-state/ephemeral")
        .join(session)
        .join(persona)
}

/// `base` and every directory under it, each offered as a base of its own —
/// `recall._ref_dirs`.
///
/// Refs NEST where memory is flat, and each directory is one `read_files` will list, so each
/// is contained first: one committed link at the top of `refs/` is enough to leave the plane
/// (#336). A link to a directory is offered and not walked — one back up the tree would
/// never end. A directory it can tell is one and cannot list is still offered, and the
/// listing names it; an entry whose kind cannot be told is unread.
fn ref_dirs(root: &Path, base: &Path, unread: &mut Unread) -> Vec<PathBuf> {
    match directory(base) {
        Err(code) => {
            unread.push((base.to_path_buf(), code));
            return Vec::new();
        }
        Ok(false) => return Vec::new(),
        Ok(true) => {}
    }
    if crate::contain::readable(root, base).is_err() {
        return Vec::new();
    }
    let mut out = vec![base.to_path_buf()];
    walk(root, base, &mut out, unread);
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>, unread: &mut Unread) {
    let Ok(entries) = listing(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for path in entries {
        match directory(&path) {
            Ok(true) => subdirs.push(path),
            Ok(false) => {}
            Err(code) => unread.push((path, code)),
        }
    }
    for sub in subdirs {
        if crate::contain::readable(root, &sub).is_err() {
            continue;
        }
        out.push(sub.clone());
        let is_link = std::fs::symlink_metadata(&sub).is_ok_and(|m| m.file_type().is_symlink());
        if !is_link {
            walk(root, &sub, out, unread);
        }
    }
}

/// The label of the first source `path` resolves inside, or `?` — `recall._label_of`.
///
/// By where the file LANDS: a journal entry that is a link into the shared store is labelled
/// `shared` by a search, which is what it is.
fn label_of(path: &Path, sources: &[(String, PathBuf)]) -> String {
    let Some(landed) = crate::contain::resolved(path) else {
        return "?".to_string();
    };
    for (label, dir) in sources {
        if let Some(base) = crate::contain::resolved(dir)
            && landed.starts_with(&base)
        {
            return label.clone();
        }
    }
    "?".to_string()
}

/// THE memory fetch — `recall.recall`.
///
/// With a query, keyword-ranked hits across every base in scope. Without, every memory in
/// scope, newest first by recorded date, undated last. `since` narrows and never re-ranks.
pub fn recall(root: &Path, ask: &Ask) -> Recalled {
    let (srcs, mut unread) = sources(root, ask);
    let dirs: Vec<PathBuf> = srcs.iter().map(|(_, d)| d.clone()).collect();
    let mut undated = 0;
    let mut undated_refs = 0;
    let dated = |path: &Path| {
        memstore::read_text(path).and_then(|text| {
            memstore::memory_date(
                &text,
                &path.file_name().unwrap_or_default().to_string_lossy(),
            )
        })
    };
    let mut rows: Vec<Hit> = Vec::new();

    if let Some(query) = ask.query.as_deref().filter(|q| !q.is_empty()) {
        for (found, score) in memstore::search(root, &dirs, query, 10_000, &mut unread) {
            let date = dated(&found.path);
            let label = label_of(&found.path, &srcs);
            if let Some(since) = ask.since
                && date.is_none_or(|d| d < since)
            {
                if label.starts_with("refs") {
                    undated_refs += 1;
                } else if date.is_none() {
                    undated += 1;
                }
                continue;
            }
            rows.push(Hit {
                label,
                path: found.path,
                title: found.title,
                score,
                date,
            });
        }
    } else {
        let mut items: Vec<(chrono::NaiveDate, Hit)> = Vec::new();
        for (label, dir) in &srcs {
            let (found, missed) = memstore::read_entries(root, dir);
            unread.extend(missed);
            for Found { path, title, text } in found {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let date = memstore::memory_date(&text, &name);
                if let Some(since) = ask.since
                    && date.is_none_or(|d| d < since)
                {
                    if label.starts_with("refs") {
                        undated_refs += 1;
                    } else if date.is_none() {
                        undated += 1;
                    }
                    continue;
                }
                // `date.min` puts the undated last without dropping them.
                items.push((
                    date.unwrap_or(chrono::NaiveDate::MIN),
                    Hit {
                        label: label.clone(),
                        path,
                        title,
                        score: 0,
                        date,
                    },
                ));
            }
        }
        // Stable, as Python's `sort(reverse=True)` is.
        items.sort_by_key(|item| std::cmp::Reverse(item.0));
        rows = items.into_iter().map(|(_, hit)| hit).collect();
    }
    // Once each: a path met twice is one thing unread.
    let mut seen = Vec::new();
    unread.retain(|item| {
        if seen.contains(item) {
            false
        } else {
            seen.push(item.clone());
            true
        }
    });
    let hits = if ask.limit != 0 {
        py_head(rows, ask.limit)
    } else {
        rows
    };
    Recalled {
        hits,
        undated,
        undated_refs,
        unread,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> chrono::NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn an_age_counts_back_from_today_and_a_month_is_thirty_days() {
        let today = day("2026-05-04");
        assert_eq!(parse_since("14d", today), Ok(day("2026-04-20")));
        assert_eq!(parse_since("2w", today), Ok(day("2026-04-20")));
        assert_eq!(parse_since("3m", today), Ok(day("2026-02-03")));
        assert_eq!(parse_since(" 0d ", today), Ok(today), "stripped first");
    }

    #[test]
    fn an_iso_date_is_read_in_every_form_python_reads() {
        let today = day("2026-05-04");
        assert_eq!(parse_since("2026-07-01", today), Ok(day("2026-07-01")));
        assert_eq!(parse_since("20260701", today), Ok(day("2026-07-01")));
        assert_eq!(parse_since("2026-W27-3", today), Ok(day("2026-07-01")));
        assert_eq!(parse_since("2026W273", today), Ok(day("2026-07-01")));
        assert_eq!(parse_since("2026-W27", today), Ok(day("2026-06-29")));
    }

    #[test]
    fn anything_else_is_refused_rather_than_read_as_no_filter() {
        let today = day("2026-05-04");
        for bad in [
            "yesterday",
            "",
            "14",
            "d",
            "2026-13-01",
            "0000-01-01",
            "14x",
            "-3d",
        ] {
            assert_eq!(
                parse_since(bad, today),
                Err(format!(
                    "unrecognised --since {} — use an age (14d, 2w, 3m) or a date (2026-07-01)",
                    crate::pyrepr::repr_str(bad)
                )),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_negative_limit_counts_from_the_end_as_a_python_slice_does() {
        assert_eq!(py_head(vec![1, 2, 3], 2), vec![1, 2]);
        assert_eq!(py_head(vec![1, 2, 3], -1), vec![1, 2]);
        assert_eq!(py_head(vec![1, 2, 3], -5), Vec::<i32>::new());
        assert_eq!(py_head(vec![1, 2, 3], 9), vec![1, 2, 3]);
    }
}
