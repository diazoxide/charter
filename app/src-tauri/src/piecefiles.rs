//! A piece's files, as the light editor's view tab reaches them (RC-5).
//!
//! Thin, as `worktrees.rs` is: which folder, which paths and what a file holds are
//! `charter_core::piecefiles`'s answers, and a refusal crosses as the core's sentence.

use std::path::Path;

use charter_core::piecefiles::{self, Opened};
use charter_core::youreditor::{self, Editor, Launch};

use crate::planes::{PlaneId, Planes};

/// One file of a piece, as the light editor draws it.
#[derive(Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PieceFile {
    /// Text, to draw.
    Text { text: String },
    /// A file git would call binary, by its size in bytes.
    Binary { bytes: u32 },
    /// Past the largest file the light editor draws (5 MiB), by its size in bytes.
    TooLarge { bytes: u32 },
}

/// The files of a piece, relative to it: what git tracks and what it does not ignore.
// Its plane is a `PlaneId` the registry vouches for, and the piece is named, never given as a
// directory (charter-app#127, `worktrees.rs`). Not a doc comment, because the generated
// bindings carry those and this is about the Rust.
#[tauri::command]
#[specta::specta]
pub fn piece_files(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: String,
) -> Result<Vec<String>, String> {
    files_of(planes.held(&plane)?.root(), &workspace, &repo, &piece)
}

/// One file of a piece, by its path relative to the piece. Refused for a path that leaves it.
#[tauri::command]
#[specta::specta]
pub fn piece_file(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: String,
    path: String,
) -> Result<PieceFile, String> {
    file_of(
        planes.held(&plane)?.root(),
        &workspace,
        &repo,
        &piece,
        &path,
    )
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

/// One file of a piece, opened in your editor at a line (RC-20). Refused, in the core's
/// sentence, for any path the light editor would refuse.
// The path is checked by `charter_core::piecefiles::in_your_editor` exactly as `piece_file`
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
    piece: String,
    path: String,
    line: u32,
    editor: YourEditor,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    let var = |name: &str| std::env::var(name).ok();
    match launch_of(
        planes.held(&plane)?.root(),
        &workspace,
        &repo,
        &piece,
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

#[allow(clippy::too_many_arguments)]
fn launch_of(
    plane: &Path,
    workspace: &str,
    repo: &str,
    piece: &str,
    path: &str,
    line: u32,
    editor: YourEditor,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<Launch, String> {
    piecefiles::in_your_editor(
        plane,
        workspace,
        repo,
        piece,
        path,
        line,
        editor.into(),
        var,
    )
    .map_err(|refused| refused.to_string())
}

fn files_of(plane: &Path, workspace: &str, repo: &str, piece: &str) -> Result<Vec<String>, String> {
    piecefiles::list(plane, workspace, repo, piece).map_err(|refused| refused.to_string())
}

fn file_of(
    plane: &Path,
    workspace: &str,
    repo: &str,
    piece: &str,
    path: &str,
) -> Result<PieceFile, String> {
    // A size crosses as a `u32`: specta refuses a `u64` for TypeScript, which has no integer
    // that wide. A binary or oversized file past 4 GiB is said as 4 GiB.
    let bytes = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
    piecefiles::open(plane, workspace, repo, piece, path)
        .map(|opened| match opened {
            Opened::Text { text } => PieceFile::Text { text },
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
    fn a_pieces_file_list_and_a_file_cross_to_the_window() {
        let (_dir, root, _piece) = plane();

        let files = files_of(&root, "alpha", "thing", "piece").unwrap();
        let file = file_of(&root, "alpha", "thing", "piece", "README.md").unwrap();

        assert!(files.contains(&"README.md".to_string()), "{files:?}");
        assert_eq!(
            file,
            PieceFile::Text {
                text: "one\n".to_string()
            }
        );
    }

    #[test]
    fn the_editor_the_window_names_reaches_the_core_as_a_url_for_that_editor() {
        let (_dir, root, piece) = plane();
        let none = |_: &str| None;
        let file = std::fs::canonicalize(piece).unwrap().join("README.md");

        let vscode = launch_of(
            &root,
            "alpha",
            "thing",
            "piece",
            "README.md",
            4,
            YourEditor::Vscode,
            &none,
        );
        let refused = launch_of(
            &root,
            "alpha",
            "thing",
            "piece",
            "../x",
            4,
            YourEditor::Zed,
            &none,
        );

        assert_eq!(
            vscode,
            Ok(Launch::Url(format!("vscode://file{}:4:1", file.display())))
        );
        assert_eq!(
            refused,
            Err("'../x' is not a path inside the worktree".to_string())
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
    fn a_refusal_crosses_as_the_cores_sentence() {
        let (_dir, root, _piece) = plane();

        let refused = file_of(&root, "alpha", "thing", "piece", "../../README.md").unwrap_err();

        assert_eq!(
            refused,
            "'../../README.md' is not a path inside the worktree"
        );
    }
}
