//! How one operator likes their window: **the layout and the theme, each a file beside
//! `machine.json`** in charter's own config directory.
//!
//! Both are "how one operator likes their window" rather than facts about a plane — the
//! argument ADR 0040 made for pins — so neither is committed to `charter.toml`, where it
//! would arrive with every clone and rearrange or repaint somebody else's window. And neither is
//! a field of the machine store: [`crate::machine`] keeps five things and says the count is
//! load-bearing, and a region arrangement or a colour names nothing that store holds.
//!
//! **Files, because a file is the one form an operator can hand-edit** (M6.9). A layout's `side`
//! and `order` have no control in the window yet, so until one exists the file is the only way
//! to reach them; `docs/design-system.md` documents both formats for exactly that reader.
//!
//! **Read before the window exists, and handed to it at creation.** A webview answers a Tauri
//! command asynchronously, so a layout fetched *from* the window lands after the first paint:
//! the default arrangement is painted and then re-laid-out, which is the flash
//! charter-app#141 left and ADR 0038 removed. So `app/src-tauri` reads both files here, before
//! the window is built, and puts what it read into the page with the window's initialization
//! script. The first frame has it; nothing is fetched.
//!
//! # What this module decides, and what it leaves to the window
//!
//! The **envelope** is decided here: the file is a JSON object, and a layout says which version
//! of the format it is and holds a `regions` array. A file that fails any of that is not in
//! force and the reason travels with the reading ([`Reading::trouble`]), so the window can put
//! it where charter says such things — the alerts drawer — rather than in a console nobody
//! reads.
//!
//! **What a region or a token means is the window's**, because the vocabulary is: the regions
//! are `app/src/regions.ts`'s catalogue and the tokens are `app/src/theme/theme.ts`'s. Both of
//! those `load` functions take whatever this module hands them, fall back field by field and
//! say what they put right. A second copy of either vocabulary here would be a second answer to
//! "which regions exist", and the first one to drift would be the one nobody runs.
//!
//! # Everything read back is attacker-influenced
//!
//! The same rule as the store beside it, and the same read: [`crate::machine::read_beside`] —
//! no link on the way or at the leaf, a plain file asked of the open descriptor, a size bound,
//! `O_NONBLOCK` — because this read happens before there is a window, and a FIFO or a planted
//! giant here is a launch that never finishes. The theme's values are additionally held to a
//! hex grammar by `theme.ts`, which is what stands between a theme file and a stylesheet.

use std::io;
use std::path::{Path, PathBuf};

/// The window's arrangement, inside [`crate::machine::DIR`].
pub const LAYOUT: &str = "layout.json";

/// The operator's own theme, inside [`crate::machine::DIR`]. The address
/// `docs/design-system.md` has given it since M6.
pub const THEME: &str = "theme.json";

/// The one version of the layout format charter writes and reads.
///
/// Any other version is not in force: the window draws the default and says so. A layout is a
/// preference, so there is nothing an old version could hold that is worth guessing at — and a
/// newer charter's format read by an older one is exactly the case a version exists to catch.
pub const LAYOUT_VERSION: u64 = 1;

/// The most either file may be.
///
/// A layout is three placements and a theme is some sixty colours and nine timings; both fit in
/// a few kilobytes with room to spare. Read whole, before there is a window.
pub const MAX_BYTES: u64 = 64 * 1024;

/// What a read of one of these files came back with.
///
/// Serialised as it is into the window's initialization script, so its field names are the
/// ones the window reads (`app/src/regions.ts`, `app/src/theme/userTheme.ts`).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Reading {
    /// Where the file is, or would be — what the window names when it tells the operator to
    /// fix one or to write one. Empty only when the machine has no config home at all.
    pub path: String,
    /// Whether there is a file at all — the one question a first launch after an upgrade asks
    /// before it moves the arrangement web storage used to hold into the file.
    pub found: bool,
    /// The document, when the file was there and charter can use it.
    pub document: Option<serde_json::Value>,
    /// Why a file that is there is not in force, in the words to put in front of whoever wrote
    /// it. `None` whenever [`Self::document`] is `Some`, and whenever there is no file.
    pub trouble: Option<String>,
}

impl Reading {
    fn absent(target: &Path) -> Self {
        Self {
            path: target.display().to_string(),
            ..Self::default()
        }
    }

    fn refused(target: &Path, why: impl std::fmt::Display) -> Self {
        Self {
            path: target.display().to_string(),
            found: true,
            document: None,
            trouble: Some(format!("{} {why}", target.display())),
        }
    }
}

/// Where the layout file is under `config_root`.
pub fn layout_path(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join(LAYOUT)
}

/// Where the operator's theme is under `config_root`.
pub fn theme_path(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join(THEME)
}

/// **Use built-in** (NO-6): the operator's theme file moved aside to `theme.aside.json`, or
/// the next free `theme.aside-N.json`, **never over anything** and never deleted, so the next
/// launch draws what is in force without it and the file is still there to mend. Answers where
/// it went. Only on the operator's press, after the window asked. Refused, moving nothing, when
/// there is no theme file or it is not a plain file (a link is moved by nobody here).
pub fn use_built_in_theme(config_root: &Path) -> Result<String, String> {
    let file = theme_path(config_root);
    match file.symlink_metadata() {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(format!(
                "there is no theme file at {}, so the built-in is already in force",
                file.display()
            ));
        }
        Err(e) => return Err(format!("charter could not look at {}: {e}", file.display())),
        Ok(meta) if !meta.is_file() => {
            return Err(format!(
                "{} is not a plain file, so charter moves nothing",
                file.display()
            ));
        }
        Ok(_) => {}
    }
    let dir = crate::machine::dir(config_root);
    for n in 1..=99 {
        let to = if n == 1 {
            dir.join("theme.aside.json")
        } else {
            dir.join(format!("theme.aside-{n}.json"))
        };
        match crate::guest::rename_new(&file, &to) {
            Ok(true) => return Ok(to.display().to_string()),
            Ok(false) => continue,
            Err(e) => {
                return Err(format!(
                    "charter could not move {} aside: {e}",
                    file.display()
                ));
            }
        }
    }
    Err(format!(
        "every name from theme.aside.json to theme.aside-99.json is taken beside {}, so nothing \
         was moved",
        file.display()
    ))
}

/// The window's layout, as a document the window can load, or why not.
///
/// **This never fails**, because its only caller is a launch. No file is a first launch; a file
/// charter cannot read or cannot use is the default arrangement plus a reason.
pub fn read_layout(config_root: &Path) -> Reading {
    read(config_root, LAYOUT, "the window's layout", layout_problem)
}

/// The operator's theme, as a document `theme.ts`'s `load` can take, or why not.
///
/// Only the envelope is asked about: `load` checks every token against the vocabulary and the
/// hex grammar and reports what it put right, and it is the one place that knows the tokens.
pub fn read_theme(config_root: &Path) -> Reading {
    read(config_root, THEME, "the operator's theme", theme_problem)
}

fn read(
    config_root: &Path,
    name: &str,
    what: &str,
    problem: fn(&serde_json::Value) -> Option<String>,
) -> Reading {
    let target = crate::machine::dir(config_root).join(name);
    let text = match crate::machine::read_beside(config_root, name, MAX_BYTES, what) {
        Ok(None) => return Reading::absent(&target),
        Ok(Some(text)) => text,
        // The refusal already names the path; saying it twice reads as two files.
        Err(why) => {
            return Reading {
                trouble: Some(format!("charter could not read it: {why}")),
                found: true,
                ..Reading::absent(&target)
            };
        }
    };
    let document: serde_json::Value = match serde_json::from_str(&text) {
        Ok(document) => document,
        Err(why) => return Reading::refused(&target, format!("is not JSON: {why}")),
    };
    if let Some(why) = problem(&document) {
        return Reading::refused(&target, why);
    }
    Reading {
        found: true,
        document: Some(document),
        ..Reading::absent(&target)
    }
}

/// What is wrong with a layout's envelope, or `None` when the window can load it.
fn layout_problem(document: &serde_json::Value) -> Option<String> {
    let Some(object) = document.as_object() else {
        return Some("is not a layout: it is not a JSON object".to_owned());
    };
    match object.get("version") {
        None => {
            return Some(format!(
                "says no version, so charter cannot say what it means; a layout says \
                 \"version\": {LAYOUT_VERSION}"
            ));
        }
        Some(found) if found.as_u64() == Some(LAYOUT_VERSION) => {}
        Some(found) => {
            return Some(format!(
                "is version {found}, and this charter reads version {LAYOUT_VERSION}"
            ));
        }
    }
    if !object
        .get("regions")
        .is_some_and(serde_json::Value::is_array)
    {
        return Some("has no \"regions\" list".to_owned());
    }
    None
}

/// What is wrong with a theme's envelope, or `None` when `theme.ts` can load it.
fn theme_problem(document: &serde_json::Value) -> Option<String> {
    (!document.is_object()).then(|| "is not a theme: it is not a JSON object".to_owned())
}

/// Writes the window's layout.
///
/// `text` is the document as the window serialised it. It is parsed and written back out
/// rather than passed through, so what lands on disk is always a JSON document charter wrote —
/// and it is held to the envelope [`read_layout`] asks for, so the window can never write a
/// file the next launch refuses.
///
/// **A file charter could not read is never overwritten** — a link, a FIFO, a giant, a
/// permission it lacks. That is [`crate::machine::update`]'s rule, for its reason: content that
/// did not parse is replaced, because the operator acted on a window that told them so, but a
/// path charter could not read is a compromised path or a failing disk, and writing over it
/// fixes nothing.
pub fn write_layout(config_root: &Path, text: &str) -> io::Result<()> {
    let mut document: serde_json::Value = serde_json::from_str(text).map_err(|why| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the window sent a layout that is not JSON: {why}"),
        )
    })?;
    if let Some(why) = layout_problem(&document) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the window sent a layout that {why}"),
        ));
    }
    let _held = crate::machine::Lock::named(config_root, LAYOUT_LOCK);
    let on_disk =
        match crate::machine::read_beside(config_root, LAYOUT, MAX_BYTES, "the window's layout") {
            Ok(text) => text,
            Err(why) => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("charter will not overwrite a layout it could not read: {why}"),
                ));
            }
        };
    // **The dismissals are the file's, never the window's** (NO-2): a window holds what it read
    // at its launch, and another window may have dismissed since. [`set_dismissed`] writes them.
    let kept = on_disk
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|mut was| was.get_mut(DISMISSED).map(serde_json::Value::take));
    let object = document
        .as_object_mut()
        .expect("layout_problem refuses a document that is not an object");
    match kept {
        Some(dismissed) => object.insert(DISMISSED.to_owned(), dismissed),
        None => object.remove(DISMISSED),
    };
    write_document(config_root, &document)
}

fn write_document(config_root: &Path, document: &serde_json::Value) -> io::Result<()> {
    let dir = crate::machine::private_dir(config_root)?;
    let pretty = serde_json::to_string_pretty(document)
        .expect("a document that was just parsed can always be written");
    crate::machine::write_beside(config_root, &dir.join(LAYOUT), (pretty + "\n").as_bytes())
}

/// The layout's field for the Notices dismissed until their cause changes (NO-2, V91j).
pub const DISMISSED: &str = "dismissed";

/// The lock a read-modify-write of the layout file holds, beside it.
const LAYOUT_LOCK: &str = "layout.json.lock";

/// The most the dismissals may take of the file, written out: half of [`MAX_BYTES`], so the
/// regions, the sizes and the editor always have the other half, and the file is never one the
/// next launch refuses whole for its size (F4).
pub const DISMISSED_MOST_BYTES: u64 = MAX_BYTES / 2;

/// **Keeps the Notices dismissed in one project** (NO-2, V91j), replacing only that project's
/// list in the layout file, under the file's lock: every other project's, and everything else
/// in the file, is as the file has it. Two windows each dismissing in their own project both
/// keep theirs. An empty list takes the project out.
///
/// **Bounded** ([`DISMISSED_MOST_BYTES`]): past it, the projects opened longest ago (the
/// machine store's recents) lose theirs first, and `project` keeps what fits last. A dismissal
/// dropped this way only shows its Notice again.
///
/// No file yet is a layout with nothing in it but this; a file charter could not read, or
/// cannot use, is refused and left as it is.
pub fn set_dismissed(config_root: &Path, project: &str, causes: &[String]) -> io::Result<()> {
    let _held = crate::machine::Lock::named(config_root, LAYOUT_LOCK);
    let text = crate::machine::read_beside(config_root, LAYOUT, MAX_BYTES, "the window's layout")
        .map_err(|why| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("charter will not write into a layout it could not read: {why}"),
        )
    })?;
    let mut document = match text {
        None => serde_json::json!({ "version": LAYOUT_VERSION, "regions": [] }),
        Some(text) => {
            let document: serde_json::Value = serde_json::from_str(&text).map_err(|why| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("the layout is not JSON, so nothing is written into it: {why}"),
                )
            })?;
            if let Some(why) = layout_problem(&document) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("the layout {why}, so nothing is written into it"),
                ));
            }
            document
        }
    };
    let object = document
        .as_object_mut()
        .expect("layout_problem refuses a document that is not an object");
    let mut dismissed = match object.remove(DISMISSED) {
        Some(serde_json::Value::Object(held)) => held,
        _ => serde_json::Map::new(),
    };
    dismissed.remove(project);
    if !causes.is_empty() {
        dismissed.insert(project.to_owned(), serde_json::json!(causes));
    }
    within_bound(config_root, project, &mut dismissed);
    if !dismissed.is_empty() {
        object.insert(DISMISSED.to_owned(), serde_json::Value::Object(dismissed));
    }
    write_document(config_root, &document)
}

/// Drops dismissals until they fit [`DISMISSED_MOST_BYTES`]: other projects, opened longest
/// ago first, then `project`'s own from the end.
fn within_bound(
    config_root: &Path,
    project: &str,
    dismissed: &mut serde_json::Map<String, serde_json::Value>,
) {
    let size = |dismissed: &serde_json::Map<String, serde_json::Value>| {
        serde_json::to_string_pretty(dismissed).map_or(u64::MAX, |text| text.len() as u64)
    };
    if size(dismissed) <= DISMISSED_MOST_BYTES {
        return;
    }
    let store = crate::machine::read(config_root).store;
    let opened = |one: &str| store.recent(Path::new(one)).map_or(0, |entry| entry.opened);
    let mut others: Vec<String> = dismissed
        .keys()
        .filter(|one| *one != project)
        .cloned()
        .collect();
    others.sort_by_key(|one| std::cmp::Reverse(opened(one)));
    while size(dismissed) > DISMISSED_MOST_BYTES {
        let Some(oldest) = others.pop() else { break };
        dismissed.remove(&oldest);
    }
    // Then `project`'s own, last first. Each cause is counted off as it goes (its text, its
    // quotes, the indent, the comma and the line), and the whole is measured again after, so a
    // list of thousands is not written out once per cause.
    let mut over = size(dismissed).saturating_sub(DISMISSED_MOST_BYTES);
    while over > 0 {
        let Some(serde_json::Value::Array(own)) = dismissed.get_mut(project) else {
            return;
        };
        let Some(last) = own.pop() else {
            dismissed.remove(project);
            return;
        };
        if own.is_empty() {
            dismissed.remove(project);
            return;
        }
        over = over.saturating_sub(last.to_string().len() as u64 + 8);
        if over == 0 {
            over = size(dismissed).saturating_sub(DISMISSED_MOST_BYTES);
        }
    }
}

/// Writes the layout **only when there is no file yet**, and says whether it did.
///
/// This is the one-time move of the arrangement web storage used to hold (`charter.layout`)
/// into the file. It must never replace a file: the operator may have written one by hand
/// before the first launch of this version, and a migration that clobbered it would be the
/// upgrade throwing their edit away. Anything at the path — a file, a link, something charter
/// cannot read — means the move has nothing to do.
pub fn adopt_layout(config_root: &Path, text: &str) -> io::Result<bool> {
    match std::fs::symlink_metadata(layout_path(config_root)) {
        Ok(_) => return Ok(false),
        Err(gone) if gone.kind() == io::ErrorKind::NotFound => {}
        Err(other) => return Err(other),
    }
    write_layout(config_root, text).map(|()| true)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    const A_LAYOUT: &str =
        r#"{"version":1,"regions":[{"id":"explorer","side":"right","order":0,"collapsed":false}]}"#;

    fn home() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temp config home")
    }

    fn put(home: &Path, name: &str, text: &str) {
        std::fs::create_dir_all(crate::machine::dir(home)).unwrap();
        std::fs::write(crate::machine::dir(home).join(name), text).unwrap();
    }

    fn trouble(reading: &Reading) -> &str {
        reading.trouble.as_deref().unwrap_or_default()
    }

    // ---------------------------------------------------------------- read

    #[test]
    fn no_file_is_a_first_launch_and_nothing_to_complain_about() {
        let home = home();
        for (reading, path) in [
            (read_layout(home.path()), layout_path(home.path())),
            (read_theme(home.path()), theme_path(home.path())),
        ] {
            assert!(!reading.found);
            assert_eq!(reading.document, None);
            assert_eq!(reading.trouble, None);
            // Named all the same, so the window can say where one would go.
            assert_eq!(reading.path, path.display().to_string());
        }
    }

    #[test]
    fn a_layout_on_disk_is_handed_to_the_window_as_it_was_written() {
        let home = home();
        put(home.path(), LAYOUT, A_LAYOUT);
        let reading = read_layout(home.path());
        assert!(reading.found);
        assert_eq!(reading.trouble, None);
        let document = reading.document.expect("a layout in force");
        assert_eq!(document["regions"][0]["side"], "right");
    }

    #[test]
    fn a_region_charter_does_not_know_is_the_windows_to_judge_not_a_refusal() {
        // The catalogue is `regions.ts`'s. A hand-edited file naming a region this build does
        // not have is still a layout: the window drops that one row and says so.
        let home = home();
        put(
            home.path(),
            LAYOUT,
            r#"{"version":1,"regions":[{"id":"minimap","side":"left","order":0}]}"#,
        );
        let reading = read_layout(home.path());
        assert_eq!(reading.trouble, None);
        assert!(reading.document.is_some());
    }

    #[test]
    fn a_layout_that_is_not_json_is_not_in_force_and_says_where_and_why() {
        let home = home();
        put(home.path(), LAYOUT, "{\"version\": 1, \"regions\": [");
        let reading = read_layout(home.path());
        assert!(reading.found);
        assert_eq!(reading.document, None);
        assert!(trouble(&reading).contains("is not JSON"), "{reading:?}");
        assert!(trouble(&reading).contains("layout.json"), "{reading:?}");
    }

    #[test]
    fn a_layout_with_no_version_or_another_one_is_not_in_force() {
        let home = home();
        put(home.path(), LAYOUT, r#"{"regions":[]}"#);
        assert!(trouble(&read_layout(home.path())).contains("says no version"));
        put(home.path(), LAYOUT, r#"{"version":2,"regions":[]}"#);
        assert!(trouble(&read_layout(home.path())).contains("is version 2"));
    }

    #[test]
    fn a_layout_that_is_not_an_object_or_holds_no_regions_is_not_in_force() {
        let home = home();
        put(home.path(), LAYOUT, "[1, 2, 3]");
        assert!(trouble(&read_layout(home.path())).contains("not a JSON object"));
        put(
            home.path(),
            LAYOUT,
            r#"{"version":1,"regions":{"explorer":"left"}}"#,
        );
        assert!(trouble(&read_layout(home.path())).contains("no \"regions\" list"));
    }

    #[test]
    fn a_link_where_the_layout_should_be_is_refused_rather_than_followed() {
        let home = home();
        let elsewhere = home.path().join("elsewhere.json");
        std::fs::write(&elsewhere, A_LAYOUT).unwrap();
        std::fs::create_dir_all(crate::machine::dir(home.path())).unwrap();
        std::os::unix::fs::symlink(&elsewhere, layout_path(home.path())).unwrap();
        let reading = read_layout(home.path());
        assert!(reading.found);
        assert_eq!(reading.document, None);
        assert!(trouble(&reading).contains("could not read"), "{reading:?}");
    }

    #[test]
    fn a_giant_layout_is_refused_before_it_is_read() {
        let home = home();
        put(
            home.path(),
            LAYOUT,
            &" ".repeat(usize::try_from(MAX_BYTES).unwrap() + 1),
        );
        assert!(trouble(&read_layout(home.path())).contains("never larger than"));
    }

    #[test]
    fn a_layout_exactly_as_large_as_the_bound_is_read() {
        // Padded with the whitespace JSON ignores, to the byte: 64 KiB is the bound.
        let home = home();
        let padding = usize::try_from(MAX_BYTES).unwrap() - A_LAYOUT.len();
        put(
            home.path(),
            LAYOUT,
            &format!("{A_LAYOUT}{}", " ".repeat(padding)),
        );
        let reading = read_layout(home.path());
        assert_eq!(reading.trouble, None, "{reading:?}");
        assert!(reading.document.is_some());
        assert_eq!(MAX_BYTES, 65_536);
    }

    #[test]
    fn a_theme_is_read_whole_and_judged_only_on_being_an_object() {
        let home = home();
        put(
            home.path(),
            THEME,
            r##"{"name":"Mine","appearance":"light","tokens":{"surface.base":"#fff","nonsense":1}}"##,
        );
        let reading = read_theme(home.path());
        assert_eq!(reading.trouble, None);
        assert_eq!(reading.document.expect("a theme")["name"], "Mine");

        put(home.path(), THEME, "\"charter-light\"");
        assert!(trouble(&read_theme(home.path())).contains("not a JSON object"));
    }

    // ---------------------------------------------------------------- use the built-in

    /// **Use built-in** (NO-6): the operator's theme file is moved aside, never deleted and
    /// never over another file, so the next launch draws what is in force without it and the
    /// file is still theirs to mend.
    #[test]
    fn using_the_built_in_moves_the_theme_aside_and_never_over_a_file() {
        let home = home();
        put(home.path(), THEME, "[[[ not json");
        let dir = crate::machine::dir(home.path());

        let first = use_built_in_theme(home.path()).expect("moved aside");
        assert_eq!(first, dir.join("theme.aside.json").display().to_string());
        assert!(!read_theme(home.path()).found);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "[[[ not json");

        put(home.path(), THEME, "{}");
        let second = use_built_in_theme(home.path()).expect("moved aside again");
        assert_eq!(second, dir.join("theme.aside-2.json").display().to_string());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "[[[ not json");
    }

    #[test]
    fn using_the_built_in_with_no_theme_file_or_a_link_moves_nothing() {
        let home = home();
        let none = use_built_in_theme(home.path()).expect_err("nothing to move");
        assert!(none.contains("no theme file"), "{none}");

        #[cfg(unix)]
        {
            let target = home.path().join("elsewhere.json");
            std::fs::write(&target, "{}").unwrap();
            std::fs::create_dir_all(crate::machine::dir(home.path())).unwrap();
            std::os::unix::fs::symlink(&target, theme_path(home.path())).unwrap();
            let link = use_built_in_theme(home.path()).expect_err("a link is not moved");
            assert!(link.contains("not a plain file"), "{link}");
            assert!(theme_path(home.path()).symlink_metadata().is_ok());
        }
    }

    // ---------------------------------------------------------------- write

    #[test]
    fn what_the_window_wrote_is_what_the_next_launch_reads() {
        let home = home();
        write_layout(home.path(), A_LAYOUT).expect("the layout is written");
        let reading = read_layout(home.path());
        assert_eq!(reading.trouble, None);
        assert_eq!(
            reading.document,
            Some(serde_json::from_str(A_LAYOUT).unwrap())
        );
    }

    #[test]
    fn the_layout_is_written_for_a_person_to_read_and_for_nobody_else_to() {
        let home = home();
        write_layout(home.path(), A_LAYOUT).unwrap();
        let path = layout_path(home.path());
        let text = std::fs::read_to_string(&path).unwrap();
        // Pretty, because this is the file an operator opens to move a region by hand.
        assert!(text.contains("\n  \"version\": 1"), "{text}");
        let mode = |at: &Path| std::fs::metadata(at).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&crate::machine::dir(home.path())), 0o700);
    }

    #[test]
    fn the_window_cannot_write_a_layout_the_next_launch_would_refuse() {
        let home = home();
        for sent in ["not json", "[]", r#"{"regions":[]}"#, r#"{"version":1}"#] {
            let refused = write_layout(home.path(), sent).expect_err(sent);
            assert_eq!(refused.kind(), io::ErrorKind::InvalidInput, "{sent}");
        }
        assert!(!layout_path(home.path()).exists());
    }

    #[test]
    fn the_text_sizes_the_window_keeps_beside_the_regions_are_written_and_read_back() {
        // charter-app#283: the window and terminal text sizes are in this file too, and what
        // they mean is the window's (`textSize.ts`), so the envelope carries them untouched.
        let home = home();
        let with_text = r#"{"version":1,"regions":[],"text":{"window":16,"terminal":12}}"#;
        write_layout(home.path(), with_text).expect("written");
        let document = read_layout(home.path()).document.expect("in force");
        assert_eq!(document["text"]["window"], 16);
        assert_eq!(document["text"]["terminal"], 12);
    }

    // ---------------------------------------------------------------- dismissed (NO-2)

    fn dismissed_in(home: &Path) -> serde_json::Value {
        read_layout(home).document.expect("in force")["dismissed"].clone()
    }

    #[test]
    fn two_windows_dismissing_in_different_projects_both_keep_theirs() {
        // Each window holds only what it read at its launch; writing that whole map back would
        // take the other window's dismissal away. Each writes its own project's, under the lock.
        let home = home();
        write_layout(home.path(), A_LAYOUT).unwrap();
        set_dismissed(home.path(), "/one", &["pin-dormant:a".to_owned()]).unwrap();
        set_dismissed(home.path(), "/two", &["pin-dormant:b".to_owned()]).unwrap();

        assert_eq!(
            dismissed_in(home.path()),
            serde_json::json!({ "/one": ["pin-dormant:a"], "/two": ["pin-dormant:b"] })
        );
    }

    #[test]
    fn a_layout_written_by_a_window_leaves_the_dismissals_on_disk_alone() {
        // A window's layout carries the dismissals it read at launch, or none: either is stale.
        let home = home();
        set_dismissed(home.path(), "/one", &["pin-dormant:a".to_owned()]).unwrap();
        write_layout(
            home.path(),
            r#"{"version":1,"regions":[],"dismissed":{"/one":[],"/old":["pin-dormant:x"]}}"#,
        )
        .unwrap();
        write_layout(home.path(), A_LAYOUT).unwrap();

        assert_eq!(
            dismissed_in(home.path()),
            serde_json::json!({ "/one": ["pin-dormant:a"] })
        );
        assert_eq!(
            read_layout(home.path()).document.unwrap()["regions"][0]["side"],
            "right"
        );
    }

    #[test]
    fn a_project_with_nothing_dismissed_is_taken_out() {
        let home = home();
        set_dismissed(home.path(), "/one", &["pin-dormant:a".to_owned()]).unwrap();
        set_dismissed(home.path(), "/one", &[]).unwrap();
        assert!(
            dismissed_in(home.path()).is_null(),
            "no field once nothing is dismissed"
        );
    }

    #[test]
    fn dismissals_never_grow_the_file_past_what_a_launch_reads() {
        // F4: a file over MAX_BYTES is refused whole at the next launch, layout and all. So
        // the projects opened longest ago lose theirs first, and the one being written last.
        let home = home();
        crate::machine::update(home.path(), |store| {
            for (at, project) in (0u64..).zip(["/p0", "/p1", "/p2", "/p3", "/p4", "/p5"]) {
                store.remember(Path::new(project), 100 + at);
            }
        })
        .unwrap();
        let causes = |n: &str| -> Vec<String> {
            (0..200)
                .map(|i| format!("pin-dormant:{n}-{i:0>40}"))
                .collect()
        };
        for project in ["/p5", "/p0", "/p1", "/p2", "/p3", "/p4"] {
            set_dismissed(home.path(), project, &causes(project)).unwrap();
        }

        let size = std::fs::metadata(layout_path(home.path())).unwrap().len();
        assert!(size <= DISMISSED_MOST_BYTES + 1024, "{size} bytes");
        let kept = dismissed_in(home.path());
        let kept = kept.as_object().unwrap();
        assert!(kept.contains_key("/p4"), "the one just written is kept");
        assert!(kept.contains_key("/p5"), "the one opened last is kept");
        assert!(!kept.contains_key("/p0"), "the one opened first goes first");

        // And one project alone past the bound keeps what fits of its own.
        let many: Vec<String> = (0..2000).map(|i| format!("pin-dormant:{i:0>60}")).collect();
        set_dismissed(home.path(), "/p5", &many).unwrap();
        assert!(std::fs::metadata(layout_path(home.path())).unwrap().len() <= MAX_BYTES);
        assert!(read_layout(home.path()).trouble.is_none());
    }

    #[test]
    fn dismissals_are_never_written_into_a_layout_charter_cannot_use() {
        let home = home();
        put(home.path(), LAYOUT, "{ half a layout");
        set_dismissed(home.path(), "/one", &["pin-dormant:a".to_owned()]).expect_err("refused");
        assert_eq!(
            std::fs::read_to_string(layout_path(home.path())).unwrap(),
            "{ half a layout"
        );
    }

    #[test]
    fn a_layout_that_did_not_parse_is_replaced_by_the_next_change() {
        // The window drew the default and said so; the operator then moved something, and
        // that is theirs to keep.
        let home = home();
        put(home.path(), LAYOUT, "{ half a layout");
        write_layout(home.path(), A_LAYOUT).expect("replaced");
        assert_eq!(read_layout(home.path()).trouble, None);
    }

    #[test]
    fn a_layout_charter_could_not_read_is_never_written_over() {
        let home = home();
        let elsewhere = home.path().join("elsewhere.json");
        std::fs::write(&elsewhere, "theirs").unwrap();
        std::fs::create_dir_all(crate::machine::dir(home.path())).unwrap();
        std::os::unix::fs::symlink(&elsewhere, layout_path(home.path())).unwrap();
        let refused = write_layout(home.path(), A_LAYOUT).expect_err("a link is not written");
        assert!(
            refused.to_string().contains("will not overwrite"),
            "{refused}"
        );
        assert_eq!(std::fs::read_to_string(&elsewhere).unwrap(), "theirs");
    }

    // ---------------------------------------------------------------- migrate

    #[test]
    fn the_arrangement_web_storage_held_moves_into_the_file_once() {
        let home = home();
        assert!(adopt_layout(home.path(), A_LAYOUT).expect("adopted"));
        assert_eq!(
            read_layout(home.path()).document,
            Some(serde_json::from_str(A_LAYOUT).unwrap())
        );
        // A second launch that still found the old key has nothing to move.
        let later = r#"{"version":1,"regions":[]}"#;
        assert!(!adopt_layout(home.path(), later).expect("nothing to do"));
        assert_eq!(
            read_layout(home.path()).document,
            Some(serde_json::from_str(A_LAYOUT).unwrap())
        );
    }

    #[test]
    fn the_move_never_replaces_a_file_the_operator_wrote_first_even_a_broken_one() {
        let home = home();
        put(home.path(), LAYOUT, "{ written by hand, not finished");
        assert!(!adopt_layout(home.path(), A_LAYOUT).expect("nothing to do"));
        assert_eq!(
            std::fs::read_to_string(layout_path(home.path())).unwrap(),
            "{ written by hand, not finished"
        );
    }
}
