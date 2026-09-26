//! `charter shell-guard`: what a shell tab's shims run in front of a harness (ADR 0062).
//!
//! Everything it decides is `charter_core::shellguard::plan`'s. This is the doing of it, in
//! the order that keeps the operator's harness starting whatever goes wrong: the line on
//! standard error, the notice to the app, and then the real program in this process's place.

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use charter_core::hookwire;
use charter_core::shellguard;

/// Runs the guard for `word`, handing `args` to the real program unchanged.
pub fn run(shims: &Path, word: &str, args: &[OsString]) -> ExitCode {
    let plan = shellguard::plan(
        word,
        shims,
        &|name| std::env::var_os(name),
        std::env::current_dir().ok(),
    );
    if let Some(warning) = &plan.warning {
        eprintln!("{warning}");
    }
    // Dropped whatever it answers: an app that is not there, or not reading, costs the banner
    // on the tab and nothing else.
    if let Some((socket, notice)) = &plan.tell {
        let _ = hookwire::tell(socket, notice);
    }
    let Some(program) = plan.program else {
        eprintln!("charter: no {word} on PATH outside charter's shell-tab shims");
        // What a shell answers for a command it cannot find, because that is what this is.
        return ExitCode::from(127);
    };
    let mut command = std::process::Command::new(&program);
    command.args(args);
    // Only where there was one to take the shims out of: a `PATH` that was never set stays
    // unset rather than becoming an empty one.
    if std::env::var_os("PATH").is_some() {
        command.env("PATH", &plan.path);
    }
    started(command, word, &program)
}

/// The real program, in this process's place, under the name the operator typed.
#[cfg(unix)]
fn started(mut command: std::process::Command, word: &str, program: &Path) -> ExitCode {
    use std::os::unix::process::CommandExt;

    // `exec` only comes back when it failed.
    let failed = command.arg0(word).exec();
    eprintln!("charter: could not run {}: {failed}", program.display());
    ExitCode::from(126)
}

/// Off unix there is no `exec`: the program runs as a child and its exit is this one's. No
/// shim calls this there today (`Shims::write` refuses), so it is here to be correct rather
/// than to be used.
#[cfg(not(unix))]
fn started(mut command: std::process::Command, _word: &str, program: &Path) -> ExitCode {
    match command.status() {
        Ok(status) => ExitCode::from(status.code().map_or(1, |code| code as u8)),
        Err(failed) => {
            eprintln!("charter: could not run {}: {failed}", program.display());
            ExitCode::from(126)
        }
    }
}
