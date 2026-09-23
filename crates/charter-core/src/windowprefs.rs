//! How one operator likes their window: **the layout and the theme, each a file beside
//! `machine.json`** in charter's own config directory.
//!
//! Both are "how one operator likes their window" rather than facts about a plane — the
//! argument charter ADR 0040 made for pins — so neither is committed to `charter.toml`, where it
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
    let document: serde_json::Value = serde_json::from_str(text).map_err(|why| {
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
    if let Err(why) =
        crate::machine::read_beside(config_root, LAYOUT, MAX_BYTES, "the window's layout")
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("charter will not overwrite a layout it could not read: {why}"),
        ));
    }
    let dir = crate::machine::private_dir(config_root)?;
    let target = dir.join(LAYOUT);
    // The store's own temp-name rule: a pid for two processes and a tag for two threads.
    let temp = dir.join(format!(
        "{LAYOUT}.{}.{}.writing",
        std::process::id(),
        crate::workspaces::scratch_tag()
    ));
    let pretty = serde_json::to_string_pretty(&document)
        .expect("a document that was just parsed can always be written");
    crate::machine::write_through(config_root, &target, &temp, (pretty + "\n").as_bytes())
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
