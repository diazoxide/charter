//! RC-20 (#709): **a file of a piece opens in your editor at a line** (ADR 0081 §3, W7).
//!
//! The window names a piece, a path relative to it, a line and which editor; the core checks
//! the path exactly as the light editor does, and answers with what to launch: a URL in the
//! editor's own scheme, or `$VISUAL`/`$EDITOR` as a program with its arguments, never a shell
//! command line.

mod support;

use purlis_core::files::{self, Branch};
use purlis_core::worktree;
use purlis_core::youreditor::{self, Editor, Launch};

fn cut(f: &support::Fixture, piece: &str) -> std::path::PathBuf {
    let path = worktree::add(&f.plane, &f.ws, &f.repo, piece, None)
        .expect("a piece is cut")
        .path;
    std::fs::canonicalize(path).unwrap()
}

/// The arguments a program is handed: `said`, then the file, as the operating system spells it.
fn words(said: &[&str], file: std::path::PathBuf) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = said.iter().map(|word| word.into()).collect();
    args.push(file.into_os_string());
    args
}

/// No `$VISUAL` and no `$EDITOR`.
fn no_variables(_: &str) -> Option<String> {
    None
}

fn launch(
    f: &support::Fixture,
    path: &str,
    line: u32,
    editor: Editor,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<Launch, files::Refused> {
    files::in_your_editor(
        &f.plane,
        Branch::piece(&f.ws, &f.repo, "piece"),
        path,
        line,
        editor,
        var,
    )
}

#[test]
fn vs_code_is_handed_the_file_and_line_through_its_url_scheme() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");

    let launched = launch(&f, "README.md", 12, Editor::VsCode, &no_variables).unwrap();

    let file = piece.join("README.md");
    assert_eq!(
        launched,
        Launch::Url(format!("vscode://file{}:12:1", file.display()))
    );
}

#[test]
fn zed_is_handed_the_file_and_line_through_its_url_scheme() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");

    let launched = launch(&f, "README.md", 3, Editor::Zed, &no_variables).unwrap();

    let file = piece.join("README.md");
    assert_eq!(
        launched,
        Launch::Url(format!("zed://file{}:3:1", file.display()))
    );
}

#[test]
fn a_jetbrains_ide_is_handed_the_file_and_line_through_its_url_scheme() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");

    let launched = launch(&f, "README.md", 7, Editor::Idea, &no_variables).unwrap();

    let file = piece.join("README.md");
    assert_eq!(
        launched,
        Launch::Url(format!("idea://open?file={}&line=7", file.display()))
    );
}

#[test]
fn a_path_a_url_would_misread_is_percent_encoded() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::create_dir_all(piece.join("my notes")).unwrap();
    std::fs::write(piece.join("my notes/a#b&c?.md"), "x\n").unwrap();

    let vscode = launch(&f, "my notes/a#b&c?.md", 1, Editor::VsCode, &no_variables).unwrap();
    let idea = launch(&f, "my notes/a#b&c?.md", 1, Editor::Idea, &no_variables).unwrap();

    let folder = piece.display();
    assert_eq!(
        vscode,
        Launch::Url(format!(
            "vscode://file{folder}/my%20notes/a%23b%26c%3F.md:1:1"
        ))
    );
    assert_eq!(
        idea,
        Launch::Url(format!(
            "idea://open?file={folder}/my%20notes/a%23b%26c%3F.md&line=1"
        ))
    );
}

#[test]
fn visual_is_run_as_a_program_with_the_line_and_the_file_as_arguments() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let var = |name: &str| match name {
        "VISUAL" => Some("emacsclient -c 'a b'".to_string()),
        "EDITOR" => Some("vi".to_string()),
        _ => None,
    };

    let launched = launch(&f, "README.md", 40, Editor::Variable, &var).unwrap();

    assert_eq!(
        launched,
        Launch::Program {
            program: "emacsclient".to_string(),
            args: words(&["-c", "a b", "+40"], piece.join("README.md")),
        }
    );
}

#[test]
fn editor_is_used_when_visual_is_not_set() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let var = |name: &str| (name == "EDITOR").then(|| "gvim".to_string());

    let launched = launch(&f, "README.md", 2, Editor::Variable, &var).unwrap();

    assert_eq!(
        launched,
        Launch::Program {
            program: "gvim".to_string(),
            args: words(&["+2"], piece.join("README.md")),
        }
    );
}

#[test]
fn a_shell_line_in_editor_is_split_into_words_and_never_run_by_a_shell() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    let var = |name: &str| (name == "EDITOR").then(|| "gvim; touch $HOME/x".to_string());

    let launched = launch(&f, "README.md", 2, Editor::Variable, &var).unwrap();

    assert_eq!(
        launched,
        Launch::Program {
            program: "gvim;".to_string(),
            args: words(&["touch", "$HOME/x", "+2"], piece.join("README.md")),
        }
    );
}

#[test]
fn neither_variable_set_is_refused_with_a_sentence() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");
    let blank = |_: &str| Some("   ".to_string());

    for var in [&no_variables as &dyn Fn(&str) -> Option<String>, &blank] {
        let refused = launch(&f, "README.md", 1, Editor::Variable, var).unwrap_err();
        assert!(refused.to_string().contains("$VISUAL"), "{refused}");
    }
}

#[test]
fn line_zero_is_refused() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    cut(&f, "piece");

    let refused = launch(&f, "README.md", 0, Editor::VsCode, &no_variables);

    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_path_the_light_editor_would_refuse_is_refused_here_too() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::write(piece.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(piece.join(".env"), "TOKEN=x\n").unwrap();
    std::fs::write(f.plane.join("charter.local.toml"), "secret = 1\n").unwrap();

    for path in [
        "../../../../charter.local.toml",
        "/etc/hosts",
        "src/../../README.md",
        "",
        ".env",
        ".git/config",
        "not-there.md",
    ] {
        let refused = launch(&f, path, 1, Editor::VsCode, &no_variables);
        assert!(refused.is_err(), "{path:?} was handed on: {refused:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_link_that_leaves_the_piece_is_refused() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::os::unix::fs::symlink("/etc/hosts", piece.join("hosts")).unwrap();

    let refused = launch(&f, "hosts", 1, Editor::VsCode, &no_variables);

    assert!(refused.is_err(), "{refused:?}");
}

#[cfg(unix)]
#[test]
fn the_editor_program_is_started_with_each_argument_as_it_was_given() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("thing");
    let piece = cut(&f, "piece");
    std::fs::create_dir_all(piece.join("a dir")).unwrap();
    std::fs::write(piece.join("a dir/it's $HOME.md"), "x\n").unwrap();
    let heard = f.plane.join("heard");
    let editor = stand_in::program(
        &f.plane,
        "an-editor",
        &format!(
            "#!/bin/sh\nfor one in \"$@\"; do printf '%s\\n' \"$one\"; done > '{}.part'\nmv '{0}.part' '{0}'\n",
            heard.display()
        ),
    );
    let var = |name: &str| (name == "EDITOR").then(|| format!("'{}' --wait", editor.display()));

    let launched = launch(&f, "a dir/it's $HOME.md", 9, Editor::Variable, &var).unwrap();
    let Launch::Program { program, args } = launched else {
        panic!("$EDITOR is a program: {launched:?}");
    };
    youreditor::start(&program, &args).unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !heard.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let said = std::fs::read_to_string(&heard).expect("the editor ran");
    assert_eq!(
        said,
        format!(
            "--wait\n+9\n{}\n",
            piece.join("a dir/it's $HOME.md").display()
        )
    );
}

/// A stand-in editor that runs `body` as a shell script, started through [`youreditor::start`]
/// as `$EDITOR` would be: what `start` answered, and how long it took.
#[cfg(unix)]
fn started_as_editor(
    dir: &std::path::Path,
    name: &str,
    body: &str,
) -> (Result<(), String>, std::time::Duration) {
    let editor = stand_in::program(dir, name, &format!("#!/bin/sh\n{body}\n"));
    let began = std::time::Instant::now();
    let answered = youreditor::start(&editor.display().to_string(), &[]);
    (answered, began.elapsed())
}

#[cfg(unix)]
#[test]
fn an_editor_that_fails_at_once_is_said_and_not_dropped() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();

    // What vi does with no terminal: it reads the end of its input and exits 1, saying nothing.
    let (answered, _) = started_as_editor(dir.path(), "a-terminal-editor", "exit 1");

    let refused = answered.unwrap_err();
    assert!(refused.contains("exited at once, with code 1"), "{refused}");
    assert!(
        refused.contains("a terminal editor needs a terminal"),
        "{refused}"
    );
}

#[cfg(unix)]
#[test]
fn an_editor_still_open_after_a_moment_is_an_editor_that_opened() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();

    let (answered, took) = started_as_editor(dir.path(), "a-windowed-editor", "sleep 4");

    assert_eq!(answered, Ok(()));
    assert!(
        took < std::time::Duration::from_millis(3500),
        "start waited for the editor to close: {took:?}"
    );
}

#[cfg(unix)]
#[test]
fn an_editor_that_hands_the_file_on_and_exits_with_success_opened() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();

    // What `gvim` and `emacsclient -n` do: hand the file to a window and exit 0.
    let (answered, _) = started_as_editor(dir.path(), "a-forking-editor", "exit 0");

    assert_eq!(answered, Ok(()));
}
