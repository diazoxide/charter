//! `charter <extension id> <command> …` — an extension's commands on the command line
//! (charter-app#342, ADR 0053).
//!
//! **Asked before clap, and only about a word clap does not answer.** A first argument that is
//! one of charter's own commands — its name, another name clap takes for it, or `help` — is
//! never looked up here, so no extension can stand where a core command does, whatever a record
//! says; the registry refuses such an id besides (`extension::cli::CORE_WORDS`). A word that is
//! neither charter's nor an installed extension's goes on to clap, which says what it always
//! said — with one line after it naming where else charter looked
//! ([`note_after_an_unknown_word`]).
//!
//! **What the program prints, and its exit status, are passed on as they are.** The core runs it
//! ([`purlis_core::executor::Executor::command`]): the record and the fingerprint are checked
//! now, so an extension that was never approved, is turned off here or has changed on disk
//! since its yes runs nothing and says which. Anything charter adds is its own line on stderr,
//! after the program's: a report of what the command changed outside the plane paths it
//! declares, and nothing else.
//!
//! **Built-in extensions are reached from the bundle this binary shipped in** (#1366). The app
//! finds its built-ins from its own resource path; this binary finds the same directory from its
//! own executable's real path (`extension::BuiltIn::of_this_program`), and never from the
//! environment. A binary outside a bundle — a development build, a copy — reaches none, and the
//! sentence after an unknown word says so (ADR 0041, amended 2026-10-09).
//!
//! **A core-owned alias** (`extension::cli::aliases`) is a core command whose words forward to an
//! extension's command — how `charter ws todo` keeps its words once todos is an extension. None
//! exists in a release build; a test build has two onto the probe, which is how the mechanism is
//! proven.

use std::ffi::OsString;
use std::process::ExitCode;

use purlis_core::extension;

/// Run `argv` here when it names a core-owned alias or an installed extension, or `None` to hand
/// it to clap. `answers` says whether clap answers a first word itself.
pub fn intercept(argv: &[OsString], answers: impl Fn(&str) -> bool) -> Option<ExitCode> {
    // Every word as text, the one reading of the command line both the routing and the request
    // take: a word that is not UTF-8 goes lossily, since the request is JSON.
    let words = rest_of(argv, 1);
    if let Some(forward) = extension::cli::alias_of(&words) {
        return Some(run(
            forward.extension,
            forward.command,
            &words[forward.words.len()..],
        ));
    }
    let first = words.first()?.as_str();
    if first.starts_with('-') || answers(first) || !extension::project::id_ok(first) {
        return None;
    }
    // Installed, in whatever standing: the executor is what says it is not approved, turned off
    // or changed. A word nothing installed is called goes on to clap.
    let config_root = purlis_core::machine::config_root_if_there()?;
    let entry = extension::read(&config_root, &extension::BuiltIn::of_this_program())
        .entry(first)?
        .clone();
    match words.get(1).map(String::as_str) {
        None => Some(list_commands(&entry.path, first, ExitCode::FAILURE)),
        Some("--help" | "-h") => Some(list_commands(&entry.path, first, ExitCode::SUCCESS)),
        Some(command) => Some(run(first, command, &words[2..])),
    }
}

/// What charter says after clap's own error for a first word nothing answered — `first`, which
/// clap does not answer.
pub fn note_after_an_unknown_word(first: &str) {
    if first.starts_with('-') || !extension::project::id_ok(first) {
        return;
    }
    let in_a_bundle = extension::BuiltIn::of_this_program().root().is_some();
    eprintln!("{}", unknown_word_note(first, in_a_bundle));
}

/// The line after clap's error for `first`: where else purlis looked — installed extensions,
/// and the built-ins of the bundle this binary is in, or a word that it is in none.
fn unknown_word_note(first: &str, in_a_bundle: bool) -> String {
    if in_a_bundle {
        format!(
            "No extension on this machine is called '{first}' either, installed or built into \
             the app."
        )
    } else {
        format!(
            "No extension installed on this machine is called '{first}' either. This purlis is \
             not inside the app's bundle, so it does not reach the app's built-in extensions."
        )
    }
}

/// The arguments after the first `skip`, as text; one that is not UTF-8 is passed lossily, since
/// the request is JSON.
fn rest_of(argv: &[OsString], skip: usize) -> Vec<String> {
    argv.iter()
        .skip(skip)
        .map(|it| it.to_string_lossy().into_owned())
        .collect()
}

/// Run `extension`'s `command`, and pass on what it printed and its status.
fn run(extension: &str, command: &str, args: &[String]) -> ExitCode {
    use std::io::Write;
    let Some(config_root) = purlis_core::machine::config_root() else {
        eprintln!("purlis: purlis cannot find its config home, where it records extensions.");
        return ExitCode::FAILURE;
    };
    let project = choices();
    let ran =
        crate::extensions::executor().command(&config_root, &project, extension, command, args);
    match ran {
        Ok(ran) => {
            let _ = std::io::stdout().write_all(&ran.stdout);
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().write_all(&ran.stderr);
            if let Some(seen) = &ran.overreach {
                eprintln!("purlis: {seen}");
            }
            let _ = std::io::stderr().flush();
            // A program's own exit status is 0 to 255 on unix, the one platform that runs one.
            ExitCode::from(u8::try_from(ran.status).unwrap_or(1))
        }
        Err(why) => {
            eprintln!("purlis: {why}");
            ExitCode::FAILURE
        }
    }
}

/// What this invocation's project and workspace say about extensions — the plane it stands in,
/// or none.
fn choices() -> extension::project::Choices {
    let Ok(here) = crate::Here::read() else {
        return extension::project::Choices::default();
    };
    // None in a plane-root chat, which is in no workspace: the project's choices alone.
    let workspace = here.workspace_if_any(None).filter(|ws| !ws.is_empty());
    extension::project::Choices::read_in(here.plane.root(), workspace.as_deref())
}

/// `charter <extension>` with no command, or with `--help`: what it adds, as usage, from its
/// manifest at `dir` — a listing, and never evidence that it may run (the executor asks that).
/// `code` is how it exits when it could list them.
fn list_commands(dir: &std::path::Path, id: &str, code: ExitCode) -> ExitCode {
    match extension::manifest_at(dir) {
        Ok(manifest) if !manifest.cli.is_empty() => {
            eprintln!("usage: purlis {id} <command> …\n\n'{id}' adds these commands:");
            for command in &manifest.cli {
                let writes = if command.writes { " (writes)" } else { "" };
                eprintln!("  {} — {}{writes}", command.name, command.title);
            }
            code
        }
        Ok(_) => {
            eprintln!("purlis: '{id}' adds no commands to purlis's command line.");
            ExitCode::FAILURE
        }
        Err(why) => {
            eprintln!("purlis: purlis could not read '{id}': {why}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::unknown_word_note;

    #[test]
    fn an_unknown_word_says_where_else_purlis_looked() {
        assert_eq!(
            unknown_word_note("stauts", true),
            "No extension on this machine is called 'stauts' either, installed or built into the \
             app."
        );
        let outside = unknown_word_note("stauts", false);
        assert!(
            outside.starts_with("No extension installed on this machine is called 'stauts'")
                && outside.contains("not inside the app's bundle"),
            "{outside}"
        );
    }
}
