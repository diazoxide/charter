//! The plane paths an extension declares it writes (charter-app#341, ADR 0053 "Write scope").
//!
//! **A declaration and a report, never a fence.** An extension runs as the operator
//! (`super::RUNS_AS_YOU`), so nothing here stops a write anywhere. What the declaration buys is
//! three things charter does: it shows the paths in the approval prompt (inside the manifest's
//! bytes, so inside the fingerprint); it hands the program the paths resolved against the plane
//! with every request, so an extension does not guess where the plane is; and after each
//! question it compares the plane with what it was before and reports a change outside them
//! (`crate::executor`). ADR 0041 ruled out a sandbox; this is detection.
//!
//! **A path is a plane-relative glob, one directory level at a time.** `*` and `?` match within
//! one segment, as a shell's do, and never a leading `.`. A path ending in `/` is a directory
//! and everything below it; one without is exactly the files it names. What a path may not
//! name is refused at parse, with the path in the sentence:
//!
//! - anything hidden (`.git`, `.charter`, a harness's `.claude`) — a segment starting with `.`;
//! - a file charter reads settings, grants or vaults from (`charter.toml`, `charter.local.toml`,
//!   `vaults.json`, a workspace's `workspace.json`) — ADR 0053: no capability adds a permission,
//!   a hook or a harness setting;
//! - a first segment that is a pattern, so `*/` cannot declare the whole plane in one word;
//! - `**`, `[`, `{`, `..`, an absolute path, an empty segment.

use std::path::{Path, PathBuf};

/// The most write paths one extension may declare. Each one is a line the operator reads.
const MOST_WRITES: usize = 16;

/// The most bytes one declared path may be.
const MOST_PATH_BYTES: usize = 200;

/// Files charter reads its settings, its grants or a plane's vaults from, as plane paths — `None`
/// standing for any one name, a workspace's. A write path that could cover one is refused: a
/// declaration that let an extension own one would be a capability that changes what a chat is
/// allowed to do.
const NEVER_WRITTEN: [&[Option<&str>]; 4] = [
    &[Some(crate::profiles::COMMITTED_FILE)],
    &[Some(crate::profiles::LOCAL_FILE)],
    &[Some("vaults.json")],
    &[
        Some("workspaces"),
        None,
        Some(crate::settings::workspace::FILE),
    ],
];

/// How [`NEVER_WRITTEN`] is named in a refusal.
const NEVER_WRITTEN_SAID: &str =
    "charter.toml, charter.local.toml, vaults.json and a workspace's workspace.json";

/// How a segment is matched: as a shell does, never a leading `.`.
const MATCHING: glob::MatchOptions = glob::MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: true,
};

/// The write paths a manifest's `contributes.writes` declares, or why charter will not read them.
pub(super) fn writes_of(value: &serde_json::Value) -> Result<Vec<String>, String> {
    let list = value
        .as_array()
        .ok_or("has a 'contributes.writes' that is not an array of plane paths")?;
    if list.is_empty() {
        return Err("declares no plane paths under 'contributes.writes'".into());
    }
    if list.len() > MOST_WRITES {
        return Err(format!(
            "declares {} write paths, and charter lists at most {MOST_WRITES} from one extension",
            list.len()
        ));
    }
    let mut writes: Vec<String> = Vec::with_capacity(list.len());
    for raw in list {
        let path = raw
            .as_str()
            .ok_or("declares a write path that is not text")?;
        refused(path).map_err(|why| {
            format!(
                "declares the write path {:?}, which {why}",
                crate::shown::readable(path, MOST_PATH_BYTES)
            )
        })?;
        if writes.iter().any(|seen| seen == path) {
            return Err(format!("declares the write path {path:?} twice"));
        }
        writes.push(path.to_owned());
    }
    Ok(writes)
}

/// Why `path` is not a write path charter will list, or nothing.
fn refused(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > MOST_PATH_BYTES {
        return Err(format!("is empty or longer than {MOST_PATH_BYTES} bytes"));
    }
    if path.contains(crate::panel::undrawable) || path.contains('\\') {
        return Err("holds a character charter will not draw in a prompt".into());
    }
    if path.starts_with('/') {
        return Err("is absolute, and a write path is relative to the plane".into());
    }
    for (at, segment) in segments(path).iter().enumerate() {
        if segment.is_empty() {
            return Err("has an empty segment".into());
        }
        if *segment == "." || *segment == ".." {
            return Err("names '.' or '..', and a write path names the plane's own paths".into());
        }
        if segment.starts_with('.') {
            return Err(
                "names something hidden, and charter's own state, git and every harness's \
                 settings live there"
                    .into(),
            );
        }
        if segment.contains("**") || segment.contains(['[', ']', '{', '}']) {
            return Err(
                "uses a pattern charter does not read — '*' and '?' within one segment are all a \
                 write path may use"
                    .into(),
            );
        }
        if at == 0 && wild(segment) {
            return Err(
                "starts with a pattern, and a write path starts with a directory of the plane \
                 named in full"
                    .into(),
            );
        }
    }
    if NEVER_WRITTEN.iter().any(|file| reaches(path, file)) {
        return Err(format!(
            "covers a file charter reads settings, grants or vaults from ({NEVER_WRITTEN_SAID}), \
             and no capability lets an extension change what a chat is allowed to do"
        ));
    }
    Ok(())
}

/// A path's segments, without the trailing `/` that says "a directory and everything below".
fn segments(path: &str) -> Vec<&str> {
    path.strip_suffix('/').unwrap_or(path).split('/').collect()
}

fn wild(segment: &str) -> bool {
    segment.contains(['*', '?'])
}

/// One segment as a glob. Every segment reaching here has passed [`refused`], so it holds no
/// `[` — the one character that can make a `glob::Pattern` fail — and a failure would read as a
/// pattern that matches only the empty name, which no path segment is.
fn glob_segment(segment: &str) -> glob::Pattern {
    glob::Pattern::new(segment).unwrap_or_default()
}

/// The declared paths resolved against `plane`, absolute, as a request hands them.
///
/// **A pattern segment on the way is matched against the directories on disk now; a named one
/// is taken as it is, there or not.** So `workspaces/*/todos/` in a plane with workspaces `a` and
/// `b` is `<plane>/workspaces/a/todos/` and `<plane>/workspaces/b/todos/`, whether or not either
/// `todos` exists yet — an extension may be the first to write there. Only a real directory
/// matches on the way: a file, and a link, which could lead out of the plane, never do.
///
/// **The last segment is handed as it is declared**, pattern and all: `notes/*.md` is
/// `<plane>/notes/*.md`, because what it covers includes files the extension has not created
/// yet, and a list of the ones that exist would tell it it may write only those. A directory path
/// keeps its `/`.
pub fn resolve(plane: &Path, writes: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for path in writes {
        let mut reached: Vec<PathBuf> = vec![plane.to_path_buf()];
        let all = segments(path);
        let (last, on_the_way) = all.split_last().unwrap_or((&"", &[]));
        for segment in on_the_way {
            reached = if wild(segment) {
                let pattern = glob_segment(segment);
                let mut next = Vec::new();
                for dir in &reached {
                    let Ok(entries) = std::fs::read_dir(dir) else {
                        continue;
                    };
                    let mut names: Vec<String> = entries
                        .filter_map(Result::ok)
                        // `file_type` does not follow a link, so a link is never a directory here.
                        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                        .filter_map(|entry| entry.file_name().into_string().ok())
                        .filter(|name| pattern.matches_with(name, MATCHING))
                        .collect();
                    names.sort();
                    next.extend(names.into_iter().map(|name| dir.join(name)));
                }
                next
            } else {
                reached.iter().map(|dir| dir.join(segment)).collect()
            };
        }
        let reached = reached.into_iter().map(|dir| dir.join(last));
        let slash = if path.ends_with('/') { "/" } else { "" };
        out.extend(reached.map(|at| format!("{}{slash}", at.display())));
    }
    out
}

/// Whether `rel`, a `/`-separated path relative to the plane, is inside what `writes` declares.
pub fn covers(writes: &[String], rel: &str) -> bool {
    let path: Vec<Option<&str>> = rel
        .split('/')
        .filter(|it| !it.is_empty())
        .map(Some)
        .collect();
    writes.iter().any(|declared| reaches(declared, &path))
}

/// Whether the declared path `declared` covers the plane path `path`, whose `None` segments
/// stand for any one name.
fn reaches(declared: &str, path: &[Option<&str>]) -> bool {
    let wanted = segments(declared);
    let long_enough = if declared.ends_with('/') {
        path.len() >= wanted.len()
    } else {
        path.len() == wanted.len()
    };
    long_enough
        && wanted.iter().zip(path).all(|(glob, segment)| {
            segment.is_none_or(|name| glob_segment(glob).matches_with(name, MATCHING))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|&it| it.to_owned()).collect()
    }

    #[test]
    fn a_directory_path_covers_everything_below_it_and_nothing_beside_it() {
        let writes = declared(&["workspaces/*/todos/"]);
        assert!(covers(&writes, "workspaces/alpha/todos/one.md"));
        assert!(covers(&writes, "workspaces/alpha/todos/deep/two.md"));
        assert!(!covers(&writes, "workspaces/alpha/notes.md"));
        assert!(!covers(&writes, "workspaces/alpha/todosx/one.md"));
        assert!(!covers(&writes, "workspaces/.hidden/todos/one.md"));
        assert!(!covers(&writes, "personas/steward/persona.md"));
    }

    #[test]
    fn a_file_path_covers_exactly_the_files_it_names() {
        let writes = declared(&["notes/*.md"]);
        assert!(covers(&writes, "notes/a.md"));
        assert!(!covers(&writes, "notes/a.txt"));
        assert!(!covers(&writes, "notes/deeper/a.md"));
    }

    #[test]
    fn what_a_write_path_may_not_name_is_refused_by_name() {
        for (path, said) in [
            ("/etc/x", "is absolute"),
            ("../x", "'..'"),
            (".charter/x", "hidden"),
            ("workspaces/*/.claude/", "hidden"),
            ("*/", "starts with a pattern"),
            ("workspaces/**/x", "a pattern charter does not read"),
            ("workspaces/[ab]/x", "a pattern charter does not read"),
            ("charter.toml", "settings, grants or vaults"),
            ("vaults.json", "settings, grants or vaults"),
            ("workspaces/*/workspace.json", "settings, grants or vaults"),
            ("workspaces/*/*.json", "settings, grants or vaults"),
            ("workspaces/alpha/", "settings, grants or vaults"),
            ("workspaces//x", "empty segment"),
        ] {
            let why = writes_of(&serde_json::json!([path])).expect_err(path);
            assert!(why.contains(said), "{path}: {why}");
        }
        assert!(writes_of(&serde_json::json!(["workspaces/*/todos/", "notes/*.md"])).is_ok());
    }

    #[test]
    fn exactly_the_most_write_paths_are_listed_and_one_more_is_refused() {
        let paths = |count: usize| -> serde_json::Value {
            (0..count).map(|at| format!("notes/{at}/")).collect()
        };
        assert_eq!(
            writes_of(&paths(MOST_WRITES)).expect("the most").len(),
            MOST_WRITES
        );
        let why = writes_of(&paths(MOST_WRITES + 1)).expect_err("one more");
        assert!(
            why.contains(&format!("declares {} write paths", MOST_WRITES + 1)),
            "{why}"
        );
    }

    #[test]
    fn a_write_path_of_exactly_the_most_bytes_is_listed_and_an_empty_or_longer_one_is_not() {
        let most = format!("notes/{}", "a".repeat(MOST_PATH_BYTES - "notes/".len()));
        assert_eq!(most.len(), MOST_PATH_BYTES);
        assert!(refused(&most).is_ok());
        for path in [String::new(), format!("{most}a")] {
            let why = refused(&path).expect_err(&path);
            assert!(why.starts_with("is empty or longer than"), "{path}: {why}");
        }
    }

    #[test]
    fn a_write_path_holding_a_backslash_or_a_control_character_is_refused() {
        for path in ["notes\\x", "notes/\u{7}x"] {
            let why = refused(path).expect_err(path);
            assert!(why.contains("will not draw"), "{path:?}: {why}");
        }
    }

    #[test]
    fn a_path_resolves_against_what_the_plane_has_now_and_keeps_what_is_named() {
        let plane = tempfile::tempdir().expect("a plane");
        for dir in ["workspaces/a", "workspaces/b", "workspaces/.hidden"] {
            std::fs::create_dir_all(plane.path().join(dir)).expect("a workspace");
        }
        // A file and a link beside the workspaces are not workspaces a path can go through.
        std::fs::write(plane.path().join("workspaces/README.md"), "").expect("a file");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/", plane.path().join("workspaces/out")).expect("a link");

        let resolved = resolve(
            plane.path(),
            &declared(&["workspaces/*/todos/", "notes/*.md"]),
        );
        let root = plane.path().display();
        assert_eq!(
            resolved,
            [
                format!("{root}/workspaces/a/todos/"),
                format!("{root}/workspaces/b/todos/"),
                // The last segment as declared: it covers files not written yet.
                format!("{root}/notes/*.md"),
            ]
        );
    }
}
