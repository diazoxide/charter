//! A branch's files, as the light editor's view tabs and the explorer's tree reach them (RC-5,
//! FM-1).
//!
//! Thin, as `worktrees.rs` is: which folder, which paths, what a folder holds and what a file
//! holds are `purlis_core::files`'s answers, and a refusal crosses as the core's sentence.
//!
//! Every command names a branch as a workspace, a repo and a piece — or no piece, for the
//! repo's own folder (#948) — and never as a directory.

use std::path::Path;

use base64::Engine as _;
use purlis_core::files::{self, Branch, Entry, Kind, Mark, Opened};
use purlis_core::youreditor::{self, Editor, Launch};

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
// It reads up to the 5 MiB a file may be, so on a blocking thread and never the one that draws
// (#1007), as every command here that reads a branch is. Not a doc comment, because the
// generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn piece_file(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    path: String,
) -> Result<PieceFile, String> {
    // Resolved here, so a project that is not open refuses here rather than in the thread.
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window("reading the file", move || {
        file_of(&root, branch(&workspace, &repo, &piece), &path)
    })
    .await
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

/// How far a branch is from the branch it was cut from: the branch cockpit's header (FM-5).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct AheadBehind {
    /// Commits the branch has that its base does not.
    pub ahead: u32,
    /// Commits its base gained that the branch does not have.
    pub behind: u32,
    /// The base they are counted against; `null` when the branch has no base recorded, and the
    /// counts then mean nothing.
    pub base: Option<String>,
}

/// How far a branch is from the branch it was cut from, in commits ahead and behind.
// Read by gitoxide in the core's bounded reader, as `branch_status` is: no git process reads
// the branch's config for this (V88a, D-88f, D-88h). Not a doc comment, because the generated
// bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn branch_ahead_behind(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
) -> Result<AheadBehind, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        ahead_behind_of(&root, branch(&workspace, &repo, &piece))
    })
    .await
    .map_err(|err| format!("reading how far the branch is from its base did not finish: {err}"))?
}

/// One hunk, in `git diff -U0`'s numbers: lines counted from 1, and a side with no lines naming
/// the line the hunk comes after (`0` for the top). The window's `GitHunk`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GitHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
}

/// One file's change, as the comparison tab draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FileDiff {
    /// Text: each side (empty where the file is absent) and git's hunks between them.
    Text {
        base: String,
        head: String,
        hunks: Vec<GitHunk>,
    },
    /// A side git would call binary: said, not drawn.
    Binary,
    /// A side past the largest file the preview draws (2 MiB), by its size in bytes.
    TooLarge { bytes: u32 },
}

/// One file of a branch against the branch it was cut from: "Show what changed" (FM-11).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct WhatChanged {
    pub mark: ChangeMark,
    /// Where a renamed file came from: a name to show, never a path to open.
    pub from: Option<String>,
    /// Whether some of its change is not committed yet.
    pub uncommitted: bool,
    /// The branch it is compared against; `null` when against the last commit.
    pub base: Option<String>,
    pub diff: FileDiff,
}

/// One file of a branch compared against the branch it was cut from, committed or not: its
/// lines and git's hunks, or what it is when it is not drawn as lines. Refused, in the core's
/// sentence, for a path any file command would refuse and for a file the branch did not change.
// The comparison is read by gitoxide in the core's bounded reader, as `branch_status` is (RC-2,
// D-88f, D-88h). Confining the path first finds the branch's folder as every file command does
// (`files::named`), which runs `git worktree list` or `git rev-parse` in this process; that is
// #1189's to move into the reader. On a blocking thread and never the one that draws (SC-2). Not
// a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn what_changed(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    path: String,
) -> Result<WhatChanged, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        what_changed_of(&root, branch(&workspace, &repo, &piece), &path)
    })
    .await
    .map_err(|err| format!("comparing the file did not finish: {err}"))?
}

fn what_changed_of(plane: &Path, branch: Branch<'_>, path: &str) -> Result<WhatChanged, String> {
    let shown =
        files::what_changed(&crate::reader(), plane, branch, path).map_err(|e| e.to_string())?;
    let diff = match shown.diff {
        files::FileDiff::Text { base, head, hunks } => FileDiff::Text {
            base,
            head,
            hunks: hunks
                .into_iter()
                .map(|one| GitHunk {
                    old_start: one.old_start,
                    old_lines: one.old_lines,
                    new_start: one.new_start,
                    new_lines: one.new_lines,
                })
                .collect(),
        },
        files::FileDiff::Binary => FileDiff::Binary,
        files::FileDiff::TooLarge { bytes } => FileDiff::TooLarge {
            bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
        },
    };
    Ok(WhatChanged {
        mark: shown.change.mark.into(),
        from: shown.change.from,
        uncommitted: shown.change.uncommitted,
        base: shown.base,
        diff,
    })
}

pub(crate) fn ahead_behind_of(plane: &Path, branch: Branch<'_>) -> Result<AheadBehind, String> {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    files::ahead_behind(&crate::reader(), plane, branch)
        .map(|apart| AheadBehind {
            ahead: count(apart.ahead),
            behind: count(apart.behind),
            base: apart.base,
        })
        .map_err(|refused| refused.to_string())
}

pub(crate) fn status_of(plane: &Path, branch: Branch<'_>) -> Result<BranchStatus, String> {
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

/// Which editor the operator chose in Settings (RC-20, ADR 0081 §3). The window
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

/// `$VISUAL`/`$EDITOR` started, on a blocking thread: [`youreditor::start`] waits up to
/// [`youreditor::AT_ONCE`] to hear whether the editor exited at once (#1044), which the window's
/// own thread must never wait for.
async fn started(program: String, args: Vec<std::ffi::OsString>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || youreditor::start(&program, &args))
        .await
        .map_err(|err| format!("starting your editor did not finish: {err}"))?
}

/// One file of a branch, opened in your editor at a line (RC-20). Refused, in the core's
/// sentence, for any path the light editor would refuse.
// The path is checked by `purlis_core::files::in_your_editor` exactly as `piece_file`
// checks it, and the editor is handed the resolved absolute path: as a URL to the operating
// system's opener, or as one argument of the program `$VISUAL`/`$EDITOR` names, never through
// a shell. Not a doc comment, because the generated bindings carry those.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn open_in_your_editor(
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
    let var = purlis_core::envvar::var;
    let launch = launch_of(
        planes.held(&plane)?.root(),
        branch(&workspace, &repo, &piece),
        &path,
        line,
        editor,
        &var,
    )?;
    match launch {
        Launch::Url(url) => app
            .opener()
            .open_url(&url, None::<&str>)
            .map_err(|e| format!("the system did not open {url}: {e}")),
        Launch::Program { program, args } => started(program, args).await,
    }
}

/// **Open file** on a chat's start notice (NO-4): the operator's `AGENTS.md` at the top of a
/// branch, which charter's exclude line hides from `git status`, opened in your editor.
/// Refused, in the core's sentence, for an `AGENTS.md` that is not theirs.
// Placed by name by `purlis_core::guest::their_agents_md_in_your_editor`, never a path the
// window sent; handed to the editor as `open_in_your_editor` hands one. Not a doc comment,
// because the generated bindings carry those.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn open_their_agents_md(
    app: tauri::AppHandle,
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    editor: YourEditor,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    let var = |name: &str| std::env::var(name).ok();
    let launch = purlis_core::guest::their_agents_md_in_your_editor(
        planes.held(&plane)?.root(),
        branch(&workspace, &repo, &piece),
        editor.into(),
        &var,
    )?;
    match launch {
        Launch::Url(url) => app
            .opener()
            .open_url(&url, None::<&str>)
            .map_err(|e| format!("the system did not open {url}: {e}")),
        Launch::Program { program, args } => started(program, args).await,
    }
}

/// **Move aside…** on a chat's start notice (NO-4), after the window asked: the operator's
/// `AGENTS.md` at the top of a branch renamed to `AGENTS.aside.md` (or the next free
/// `AGENTS.aside-N.md`), never over a file, so `git status` shows it again. Answers its new
/// name. Refused, touching nothing, for an `AGENTS.md` that is not theirs.
// Off the window's thread (#1007): it asks git whether the file is the operator's before it
// renames anything. Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn move_their_agents_md_aside(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
) -> Result<String, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    crate::off_the_window("moving AGENTS.md aside", move || {
        purlis_core::guest::move_agents_md_aside(&root, branch(&workspace, &repo, &piece))
    })
    .await
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

/// One file or folder of a branch, its path put on the clipboard: relative to the branch's
/// folder, or absolute (FM-10). Refused, in the core's sentence, for a path that leaves the
/// branch, a link or git's own folder.
// The path is placed by `purlis_core::files::place` and the absolute one is the core's, never
// one the window joined; it goes to the clipboard here and never back to the window. Placed on
// a blocking thread, never the one that draws (#1007). Not a doc comment, because the generated
// bindings carry those.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn copy_branch_path(
    planes: tauri::State<'_, Planes>,
    clipboard: tauri::State<'_, crate::vaults::SystemClipboard>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    path: String,
    absolute: bool,
) -> Result<(), String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let text = crate::off_the_window("placing the path", move || {
        path_text(&root, branch(&workspace, &repo, &piece), &path, absolute)
    })
    .await?;
    clipboard.put_text(&text)
}

/// One file or folder of a branch, shown in the operating system's file manager: Finder,
/// Files or File Explorer (FM-10). Refused as Copy path is refused.
// The opener plugin's reveal, called here with the path the core placed: the window names a
// branch and a path inside it, never a directory. Placed on a blocking thread, never the one
// that draws (#1007). Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub async fn reveal_branch_path(
    app: tauri::AppHandle,
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    path: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    let root = planes.held(&plane)?.root().to_path_buf();
    let asked = path.clone();
    let placed = crate::off_the_window("placing the path", move || {
        placed_of(&root, branch(&workspace, &repo, &piece), &asked)
    })
    .await?;
    app.opener()
        .reveal_item_in_dir(&placed.absolute)
        .map_err(|e| format!("the system did not show '{path}': {e}"))
}

fn placed_of(plane: &Path, branch: Branch<'_>, path: &str) -> Result<files::Placed, String> {
    files::place(plane, branch, path).map_err(|refused| refused.to_string())
}

/// What Copy path puts on the clipboard: the path in the branch, or the absolute path the core
/// placed. An absolute path is said as a person writes it, without Windows' `\\?\` prefix.
fn path_text(
    plane: &Path,
    branch: Branch<'_>,
    path: &str,
    absolute: bool,
) -> Result<String, String> {
    if !absolute {
        // A link's own path, followed nowhere: what its row copies (`files::named`).
        return files::named(plane, branch, path).map_err(|refused| refused.to_string());
    }
    let placed = placed_of(plane, branch, path)?;
    Ok(dunce::simplified(&placed.absolute).display().to_string())
}

/// The folder of a branch a shell tab starts in (FM-10), resolved as the tree resolves it: inside
/// the branch, reached through no link, not git's. `""` is the branch's own folder.
pub(crate) fn shell_folder(
    plane: &Path,
    branch: Branch<'_>,
    folder: &str,
) -> Result<std::path::PathBuf, String> {
    files::folder(plane, branch, folder).map_err(|refused| refused.to_string())
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
        let ran = purlis_core::forklock::output(
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
        let piece = purlis_core::worktree::add(&root, "alpha", "thing", "piece", None)
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
    fn what_changed_in_a_file_crosses_with_both_sides_and_gits_hunks() {
        let (_dir, root, piece) = plane();
        std::fs::write(piece.join("README.md"), "one\ntwo\n").unwrap();

        let shown = what_changed_of(&root, PIECE, "README.md").unwrap();

        assert_eq!(
            shown,
            WhatChanged {
                mark: ChangeMark::Changed,
                from: None,
                uncommitted: true,
                base: Some("main".to_string()),
                diff: FileDiff::Text {
                    base: "one\n".to_string(),
                    head: "one\ntwo\n".to_string(),
                    hunks: vec![GitHunk {
                        old_start: 1,
                        old_lines: 0,
                        new_start: 2,
                        new_lines: 1,
                    }],
                },
            }
        );
    }

    #[test]
    fn what_changed_refuses_in_the_cores_sentence() {
        let (_dir, root, piece) = plane();
        std::fs::write(piece.join("other.txt"), "\n").unwrap();

        assert_eq!(
            what_changed_of(&root, PIECE, "README.md"),
            Err("'README.md' is not a file this branch changed against its base".to_string())
        );
        assert_eq!(
            what_changed_of(&root, PIECE, ".git/config"),
            Err("'.git/config' is not a path inside the branch's folder".to_string())
        );
    }

    #[test]
    fn how_far_a_branch_is_from_its_base_crosses_with_the_base_named() {
        let (_dir, root, piece) = plane();
        std::fs::write(piece.join("new.txt"), "\n").unwrap();
        git(&piece, &["add", "-A"]);
        git(&piece, &["commit", "-q", "-m", "on the branch"]);

        let apart = ahead_behind_of(&root, PIECE).unwrap();

        assert_eq!(
            apart,
            AheadBehind {
                ahead: 1,
                behind: 0,
                base: Some("main".to_string()),
            }
        );
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
    fn copy_path_is_the_path_in_the_branch_or_the_one_the_core_resolved() {
        let (_dir, root, piece) = plane();
        std::fs::create_dir_all(piece.join("src")).unwrap();
        std::fs::write(piece.join("src/lib.rs"), "\n").unwrap();
        let resolved = std::fs::canonicalize(&piece).unwrap();

        let relative = path_text(&root, PIECE, "./src/lib.rs", false);
        let absolute = path_text(&root, PIECE, "src", true);

        assert_eq!(relative, Ok("src/lib.rs".to_string()));
        assert_eq!(absolute, Ok(resolved.join("src").display().to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn a_links_relative_path_is_copied_and_its_absolute_path_and_reveal_are_refused() {
        let (_dir, root, piece) = plane();
        std::os::unix::fs::symlink(piece.join("README.md"), piece.join("CLAUDE.md")).unwrap();
        let refused = "'CLAUDE.md' is not a path inside the branch's folder".to_string();

        assert_eq!(
            path_text(&root, PIECE, "CLAUDE.md", false),
            Ok("CLAUDE.md".to_string())
        );
        assert_eq!(
            path_text(&root, PIECE, "CLAUDE.md", true),
            Err(refused.clone())
        );
        assert_eq!(
            placed_of(&root, PIECE, "CLAUDE.md").map(|placed| placed.absolute),
            Err(refused)
        );
    }

    #[test]
    fn a_path_outside_the_branch_is_refused_before_it_is_copied_or_revealed() {
        let (_dir, root, _piece) = plane();

        assert_eq!(
            path_text(&root, PIECE, "../../charter.toml", true),
            Err("'../../charter.toml' is not a path inside the branch's folder".to_string())
        );
        assert_eq!(
            placed_of(&root, PIECE, ".git").map(|placed| placed.absolute),
            Err("'.git' is not a path inside the branch's folder".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_starts_only_in_a_folder_of_the_branch_reached_through_no_link() {
        let (_dir, root, piece) = plane();
        std::fs::create_dir_all(piece.join("src")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), piece.join("out")).unwrap();
        let resolved = std::fs::canonicalize(&piece).unwrap();

        assert_eq!(shell_folder(&root, PIECE, "src"), Ok(resolved.join("src")));
        assert_eq!(shell_folder(&root, PIECE, ""), Ok(resolved));
        assert_eq!(
            shell_folder(&root, PIECE, "out"),
            Err("'out' is not a path inside the branch's folder".to_string())
        );
        assert_eq!(
            shell_folder(&root, PIECE, "README.md"),
            Err("'README.md' is not a folder".to_string())
        );
        assert_eq!(
            shell_folder(&root, PIECE, "../.."),
            Err("'../..' is not a path inside the branch's folder".to_string())
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
