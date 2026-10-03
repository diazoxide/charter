//! A branch's files, as the light editor's view tabs and the explorer's tree reach them (RC-5,
//! FM-1).
//!
//! Thin, as `worktrees.rs` is: which folder, which paths, what a folder holds and what a file
//! holds are `charter_core::files`'s answers, and a refusal crosses as the core's sentence.
//!
//! Every command names a branch as a workspace, a repo and a piece — or no piece, for the
//! repo's own folder (#948) — and never as a directory.

use std::path::Path;

use base64::Engine as _;
use charter_core::files::{self, Branch, Entry, Kind, Mark, Opened};
use charter_core::youreditor::{self, Editor, Launch};

use crate::planes::{PlaneId, Planes};

/// One file of a branch, as the light editor draws it.
#[derive(Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PieceFile {
    /// Text, to draw.
    Text { text: String },
    /// An image, known by its first bytes (FM-2): its media type and its bytes as base64. The
    /// window decodes it into a canvas, so nothing is loaded from a URL.
    Image { mime: String, base64: String },
    /// A file git would call binary, by its size in bytes.
    Binary { bytes: u32 },
    /// An image whose header declares more pixels than the preview draws (40 megapixels): its
    /// type and declared size, and none of its bytes.
    HugeImage {
        mime: String,
        width: u32,
        height: u32,
    },
    /// Past the largest file the preview draws (2 MiB), by its size in bytes.
    TooLarge { bytes: u32 },
}

/// One file of a branch, by its path relative to the branch's folder. Refused for a path that
/// leaves it.
#[tauri::command]
#[specta::specta]
pub fn piece_file(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    path: String,
) -> Result<PieceFile, String> {
    file_of(
        planes.held(&plane)?.root(),
        branch(&workspace, &repo, &piece),
        &path,
    )
}

/// What one entry of a branch's folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    Folder,
    File,
    /// A symbolic link: never expanded, and opened only when it leads to another file the
    /// branch offers.
    Link,
}

/// One entry of a branch's folder, as the explorer's tree draws it (FM-1).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FolderEntry {
    /// Its name in the folder, not its path.
    pub name: String,
    pub kind: EntryKind,
    /// Whether git ignores it: hidden unless the operator asks to see ignored files.
    pub ignored: bool,
    /// Why it does not open, in the core's sentence; `null` when it opens or expands.
    pub refused: Option<String>,
}

impl From<Entry> for FolderEntry {
    fn from(entry: Entry) -> Self {
        Self {
            name: entry.name,
            kind: match entry.kind {
                Kind::Folder => EntryKind::Folder,
                Kind::File => EntryKind::File,
                Kind::Link => EntryKind::Link,
            },
            ignored: entry.ignored,
            refused: entry.refused,
        }
    }
}

/// One folder of a branch, one level deep: its first entries, and how many more it holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FolderListing {
    /// Folders first, then files, each in the order a person reads names; at most 5,000.
    pub entries: Vec<FolderEntry>,
    /// How many entries past those are not listed.
    pub more: u32,
}

/// One folder of a branch, one level deep: folders first, then files, each in the order a
/// person reads names, the first 5,000 and a count of the rest. `""` is the branch's own folder.
// Lazy by design (#1103 story 2): one directory listing and one `git check-ignore` per call, so
// nothing below a folder is read until it is expanded. On a blocking thread and never the one
// that draws (SC-2): git answers in milliseconds on a warm disk, and in seconds on a cold one.
// Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn branch_tree(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    folder: String,
) -> Result<FolderListing, String> {
    // Resolved here, so a project that is not open refuses here rather than in the thread.
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        tree_of(&root, branch(&workspace, &repo, &piece), &folder)
    })
    .await
    .map_err(|err| format!("reading the folder did not finish: {err}"))?
}

/// What a branch did to one path (FM-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeMark {
    /// Its content or its kind changed.
    Changed,
    /// It is new against the branch it was cut from.
    Added,
    /// It is gone.
    Deleted,
    /// It moved here from another path.
    Renamed,
}

impl From<Mark> for ChangeMark {
    fn from(mark: Mark) -> Self {
        match mark {
            Mark::Changed => Self::Changed,
            Mark::Added => Self::Added,
            Mark::Deleted => Self::Deleted,
            Mark::Renamed => Self::Renamed,
        }
    }
}

/// One path a branch changed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FileChange {
    /// Its path relative to the branch's folder.
    pub path: String,
    pub mark: ChangeMark,
    /// Where a renamed file came from: a name to show, never a path to open.
    pub from: Option<String>,
    /// Whether it is not committed yet.
    pub uncommitted: bool,
}

/// One folder holding changes: the mark they share (`changed` when they differ) and how many.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FolderChanges {
    /// Its path relative to the branch's folder; `""` is the branch's own.
    pub folder: String,
    pub mark: ChangeMark,
    pub count: u32,
}

/// What a branch changed against the branch it was cut from, committed or not.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct BranchStatus {
    /// Sorted by path, at most 10,000.
    pub changes: Vec<FileChange>,
    /// Every folder holding a change, sorted by path.
    pub folders: Vec<FolderChanges>,
    /// How many changes past those are not listed.
    pub more: u32,
    /// The branch the changes are counted against; `null` when against the last commit.
    pub base: Option<String>,
}

/// What a branch changed against the branch it was cut from, committed or not, file by file and
/// rolled up onto its folders: the explorer's markers and its "Changed only" (FM-4).
// Read by gitoxide in the core's bounded reader, a short-lived child of this binary killed past
// its deadline and memory cap; no git process reads the branch's config for this (D-88f,
// D-88h). On a blocking thread and never the one that draws (SC-2). Not a doc comment, because
// the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn branch_status(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
) -> Result<BranchStatus, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        status_of(&root, branch(&workspace, &repo, &piece))
    })
    .await
    .map_err(|err| format!("reading what the branch changed did not finish: {err}"))?
}

fn status_of(plane: &Path, branch: Branch<'_>) -> Result<BranchStatus, String> {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    files::status(&crate::reader(), plane, branch)
        .map(|status| BranchStatus {
            changes: status
                .changes
                .into_iter()
                .map(|one| FileChange {
                    path: one.path,
                    mark: one.mark.into(),
                    from: one.from,
                    uncommitted: one.uncommitted,
                })
                .collect(),
            folders: status
                .folders
                .into_iter()
                .map(|one| FolderChanges {
                    folder: one.folder,
                    mark: one.mark.into(),
                    count: count(one.count),
                })
                .collect(),
            more: count(status.more),
            base: status.base,
        })
        .map_err(|refused| refused.to_string())
}

/// The branch the window named: a piece of a repo, or the repo's own folder. Shared with
/// `filewatch.rs`, which names branches the same way.
pub(crate) fn branch<'a>(
    workspace: &'a str,
    repo: &'a str,
    piece: &'a Option<String>,
) -> Branch<'a> {
    match piece {
        Some(piece) => Branch::piece(workspace, repo, piece),
        None => Branch::repo(workspace, repo),
    }
}

fn tree_of(plane: &Path, branch: Branch<'_>, folder: &str) -> Result<FolderListing, String> {
    files::tree(plane, branch, folder)
        .map(|level| FolderListing {
            entries: level.entries.into_iter().map(FolderEntry::from).collect(),
            more: u32::try_from(level.more).unwrap_or(u32::MAX),
        })
        .map_err(|refused| refused.to_string())
}

/// Which editor the operator chose on the Preferences tab (RC-20, ADR 0081 §3). The window
/// names one of these four and nothing else: never a program, never a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum YourEditor {
    /// Visual Studio Code, through `vscode://`.
    Vscode,
    /// Zed, through `zed://`.
    Zed,
    /// A JetBrains IDE, through `idea://`.
    Idea,
    /// `$VISUAL`, else `$EDITOR`, from charter's own environment, with `+line`.
    Variable,
}

impl From<YourEditor> for Editor {
    fn from(editor: YourEditor) -> Self {
        match editor {
            YourEditor::Vscode => Self::VsCode,
            YourEditor::Zed => Self::Zed,
            YourEditor::Idea => Self::Idea,
            YourEditor::Variable => Self::Variable,
        }
    }
}

/// One file of a branch, opened in your editor at a line (RC-20). Refused, in the core's
/// sentence, for any path the light editor would refuse.
// The path is checked by `charter_core::files::in_your_editor` exactly as `piece_file`
// checks it, and the editor is handed the resolved absolute path: as a URL to the operating
// system's opener, or as one argument of the program `$VISUAL`/`$EDITOR` names, never through
// a shell. Not a doc comment, because the generated bindings carry those.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub fn open_in_your_editor(
    app: tauri::AppHandle,
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    path: String,
    line: u32,
    editor: YourEditor,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    let var = |name: &str| std::env::var(name).ok();
    match launch_of(
        planes.held(&plane)?.root(),
        branch(&workspace, &repo, &piece),
        &path,
        line,
        editor,
        &var,
    )? {
        Launch::Url(url) => app
            .opener()
            .open_url(&url, None::<&str>)
            .map_err(|e| format!("the system did not open {url}: {e}")),
        Launch::Program { program, args } => youreditor::start(&program, &args),
    }
}

fn launch_of(
    plane: &Path,
    branch: Branch<'_>,
    path: &str,
    line: u32,
    editor: YourEditor,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<Launch, String> {
    files::in_your_editor(plane, branch, path, line, editor.into(), var)
        .map_err(|refused| refused.to_string())
}

fn file_of(plane: &Path, branch: Branch<'_>, path: &str) -> Result<PieceFile, String> {
    // A size crosses as a `u32`: specta refuses a `u64` for TypeScript, which has no integer
    // that wide. A binary or oversized file past 4 GiB is said as 4 GiB.
    let bytes = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
    files::open(plane, branch, path)
        .map(|opened| match opened {
            Opened::Text { text } => PieceFile::Text { text },
            Opened::Image { mime, data } => PieceFile::Image {
                mime: mime.to_string(),
                base64: base64::engine::general_purpose::STANDARD.encode(data),
            },
            Opened::HugeImage {
                mime,
                width,
                height,
            } => PieceFile::HugeImage {
                mime: mime.to_string(),
                width,
                height,
            },
            Opened::Binary { bytes: n } => PieceFile::Binary { bytes: bytes(n) },
            Opened::TooLarge { bytes: n } => PieceFile::TooLarge { bytes: bytes(n) },
        })
        .map_err(|refused| refused.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn git(dir: &Path, args: &[&str]) {
        let ran = charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "user.email=t@e.invalid", "-c", "user.name=t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args),
        )
        .unwrap();
        assert!(ran.status.success(), "git {args:?}: {ran:?}");
    }

    const PIECE: Branch<'static> = Branch {
        ws: "alpha",
        repo: "thing",
        piece: Some("piece"),
    };

    /// A plane with a clone and one piece cut from it.
    fn plane() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let clone = root.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main", "."]);
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "one"]);
        let piece = charter_core::worktree::add(&root, "alpha", "thing", "piece", None)
            .unwrap()
            .path;
        (dir, root, piece)
    }

    #[test]
    fn a_pieces_file_crosses_to_the_window() {
        let (_dir, root, _piece) = plane();

        let file = file_of(&root, PIECE, "README.md").unwrap();

        assert_eq!(
            file,
            PieceFile::Text {
                text: "one\n".to_string()
            }
        );
    }

    #[test]
    fn an_image_crosses_to_the_window_as_its_type_and_base64() {
        let (_dir, root, piece) = plane();
        std::fs::write(piece.join("dot.gif"), b"GIF89a\x01\x00\x01\x00\x00\x00\x00").unwrap();

        let file = file_of(&root, PIECE, "dot.gif").unwrap();

        assert_eq!(
            file,
            PieceFile::Image {
                mime: "image/gif".to_string(),
                base64: "R0lGODlhAQABAAAAAA==".to_string(),
            }
        );
    }

    #[test]
    fn the_editor_the_window_names_reaches_the_core_as_a_url_for_that_editor() {
        let (_dir, root, piece) = plane();
        let none = |_: &str| None;
        let file = std::fs::canonicalize(piece).unwrap().join("README.md");

        let vscode = launch_of(&root, PIECE, "README.md", 4, YourEditor::Vscode, &none);
        let refused = launch_of(&root, PIECE, "../x", 4, YourEditor::Zed, &none);

        assert_eq!(
            vscode,
            Ok(Launch::Url(format!("vscode://file{}:4:1", file.display())))
        );
        assert_eq!(
            refused,
            Err("'../x' is not a path inside the branch's folder".to_string())
        );
    }

    #[test]
    fn a_folder_of_a_branch_crosses_one_level_deep_with_each_entrys_kind() {
        let (_dir, root, piece) = plane();
        std::fs::create_dir_all(piece.join("src")).unwrap();
        std::fs::write(piece.join("src/lib.rs"), "\n").unwrap();

        // The worktree's own `.git` is there too, and hidden with what git ignores.
        let top: Vec<FolderEntry> = tree_of(&root, PIECE, "")
            .unwrap()
            .entries
            .into_iter()
            .filter(|one| !one.ignored)
            .collect();
        let repo = tree_of(&root, branch("alpha", "thing", &None), "").unwrap();

        assert_eq!(
            top,
            [
                FolderEntry {
                    name: "src".to_string(),
                    kind: EntryKind::Folder,
                    ignored: false,
                    refused: None,
                },
                FolderEntry {
                    name: "README.md".to_string(),
                    kind: EntryKind::File,
                    ignored: false,
                    refused: None,
                },
            ]
        );
        assert!(
            repo.entries.iter().any(|one| one.name == "README.md"),
            "{repo:?}"
        );
        assert_eq!(
            tree_of(&root, PIECE, ".."),
            Err("'..' is not a path inside the branch's folder".to_string())
        );
    }

    #[test]
    fn what_a_branch_changed_crosses_with_each_files_mark_and_its_folders() {
        let (_dir, root, piece) = plane();
        std::fs::create_dir_all(piece.join("src")).unwrap();
        std::fs::write(piece.join("src/lib.rs"), "\n").unwrap();

        let status = status_of(&root, PIECE).unwrap();

        assert_eq!(
            status.changes,
            [FileChange {
                path: "src/lib.rs".to_string(),
                mark: ChangeMark::Added,
                from: None,
                uncommitted: true,
            }]
        );
        assert!(
            status
                .folders
                .iter()
                .any(|one| one.folder == "src" && one.mark == ChangeMark::Added && one.count == 1),
            "{status:?}"
        );
        assert_eq!(status.base.as_deref(), Some("main"));
    }

    #[test]
    fn the_editors_cross_from_the_window_by_their_wire_names() {
        let named: Vec<YourEditor> =
            serde_json::from_str(r#"["vscode", "zed", "idea", "variable"]"#).unwrap();

        assert_eq!(
            named,
            [
                YourEditor::Vscode,
                YourEditor::Zed,
                YourEditor::Idea,
                YourEditor::Variable
            ]
        );
    }

    #[test]
    fn a_refusal_crosses_as_the_cores_sentence() {
        let (_dir, root, _piece) = plane();

        let refused = file_of(&root, PIECE, "../../README.md").unwrap_err();

        assert_eq!(
            refused,
            "'../../README.md' is not a path inside the branch's folder"
        );
    }
}
