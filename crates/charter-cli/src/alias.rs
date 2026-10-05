//! `charter`: the name the command line had before it was `purlis`, kept as an alias for the
//! rename's window (RN-3, #1255; V93k). Removing it is its own ticket at 1.0 (V93l).
//!
//! **It runs the `purlis` beside it and nothing else.** Beside the file it really is, links
//! resolved, so the `charter` link the PATH install puts in `/usr/local/bin` still reaches the
//! app's own `purlis` and never one a `PATH` lookup would find (an older install, or one in the
//! checkout a chat is standing in). With none there it says so and fails.
//!
//! **The whole invocation is handed on**: the arguments, `argv[0]`, stdin, the environment and
//! the working directory. On unix it `exec`s, so the process, its pid, its signals and its exit
//! code are `purlis`'s own, and a hook run through this name is the same process a hook run
//! through `purlis` is. Elsewhere it waits and exits with `purlis`'s code.
//!
//! An alias program rather than a second build of the CLI, because the app bundle and the
//! `.deb` carry each name as a file of its own, and a second copy of the CLI would double both.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// The name the command line ships as. Kept here, and not imported, so this program links
/// nothing of charter's.
const PRIMARY: &str = "purlis";

fn main() -> ExitCode {
    let Some(primary) = beside() else {
        return said("cannot tell where this program is, so it cannot find the purlis beside it");
    };
    if !primary.is_file() {
        return said(&format!(
            "no purlis at {}. `charter` is an alias of purlis and runs only the one installed \
             beside it — reinstall the app, or call purlis by its own path",
            primary.display()
        ));
    }
    let mut args = std::env::args_os();
    let arg0 = args.next();
    let mut command = Command::new(&primary);
    command.args(args);
    run(command, &primary, arg0)
}

/// `<the directory this program really is in>/purlis`.
fn beside() -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let me = me.canonicalize().unwrap_or(me);
    Some(me.with_file_name(format!("{PRIMARY}{}", std::env::consts::EXE_SUFFIX)))
}

#[cfg(unix)]
fn run(
    mut command: Command,
    primary: &std::path::Path,
    arg0: Option<std::ffi::OsString>,
) -> ExitCode {
    use std::os::unix::process::CommandExt;
    if let Some(arg0) = arg0 {
        command.arg0(arg0);
    }
    // Returns only when the exec failed.
    let error = command.exec();
    said(&format!("could not run {}: {error}", primary.display()))
}

#[cfg(not(unix))]
fn run(
    mut command: Command,
    primary: &std::path::Path,
    _arg0: Option<std::ffi::OsString>,
) -> ExitCode {
    match command.status() {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(error) => said(&format!("could not run {}: {error}", primary.display())),
    }
}

/// One line on stderr, and the exit code a shell gives a command it could not run.
fn said(what: &str) -> ExitCode {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "charter: {what}.");
    ExitCode::from(127)
}
