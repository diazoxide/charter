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
//! ([`charter_core::executor::Executor::command`]): the record and the fingerprint are checked
//! now, so an extension that was never approved, is turned off here or has changed on disk
//! since its yes runs nothing and says which. Anything charter adds is its own line on stderr,
//! after the program's: a report of what the command changed outside the plane paths it
//! declares, and nothing else.
//!
//! **Built-in extensions are not reached from here yet.** The app finds its built-ins from its
//! own resource path (`extension::BuiltIn`), and this binary does not look for the bundle it
//! was shipped in; ADR 0041's amendment of 2026-09-25 (charter-app#342) records why. None of
//! them adds a command today, and the sentences here say so where it matters.
//!
//! **A core-owned alias** (`extension::cli::aliases`) is a core command whose words forward to an
//! extension's command — how `charter ws todo` keeps its words once todos is an extension. None
//! exists in a release build; a test build has two onto the probe, which is how the mechanism is
//! proven.

use std::ffi::OsString;
use std::process::ExitCode;

use charter_core::extension;

/// Run `argv` here when it names a core-owned alias or an installed extension, or `None` to hand
/// it to clap. `answers` says whether clap answers a first word itself.
pub fn intercept(argv: &[OsString], answers: impl Fn(&str) -> bool) -> Option<ExitCode> {
    let words: Vec<&str> = argv.iter().skip(1).map_while(|it| it.to_str()).collect();
    if let Some(forward) = extension::cli::alias_of(&words) {
        let rest = rest_of(argv, 1 + forward.words.len());
        return Some(run(forward.extension, forward.command, &rest));
    }
    let first = *words.first()?;
    if first.starts_with('-') || answers(first) || !extension::project::id_ok(first) {
        return None;
    }
    // Installed, in whatever standing: the executor is what says it is not approved, turned off
    // or changed. A word nothing installed is called goes on to clap.
    let config_root = charter_core::machine::config_root_if_there()?;
    extension::read(&config_root, &extension::BuiltIn::none()).entry(first)?;
    let Some(command) = words.get(1) else {
        return Some(list_commands(&config_root, first));
    };
    let rest = rest_of(argv, 3);
    Some(run(first, command, &rest))
}

/// What charter says after clap's own error for a first word nothing answered.
pub fn note_after_an_unknown_word(argv: &[OsString]) {
    let Some(first) = argv.get(1).and_then(|it| it.to_str()) else {
        return;
    };
    if first.starts_with('-') || !extension::project::id_ok(first) {
        return;
    }
    eprintln!(
        "No extension installed on this machine is called '{first}' either. The command line \
         runs the commands of extensions you installed, and does not reach the app's built-in \
         extensions yet."
    );
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
    let Some(config_root) = charter_core::machine::config_root() else {
        eprintln!("charter: charter cannot find its config home, where it records extensions.");
        return ExitCode::FAILURE;
    };
    let project = choices();
    let ran = charter_core::executor::Executor::default().command(
        &config_root,
        &project,
        extension,
        command,
        args,
    );
    match ran {
        Ok(ran) => {
            let _ = std::io::stdout().write_all(&ran.stdout);
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().write_all(&ran.stderr);
            if let Some(seen) = &ran.overreach {
                eprintln!("charter: {seen}");
            }
            let _ = std::io::stderr().flush();
            // An exit status is a byte to the shell; one outside it is what the shell would have
            // made of it.
            ExitCode::from(u8::try_from(ran.status.rem_euclid(256)).unwrap_or(1))
        }
        Err(why) => {
            eprintln!("charter: {why}");
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
    let workspace = here.active_workspace(None);
    let workspace = (!workspace.is_empty()).then_some(workspace);
    extension::project::Choices::read_in(here.plane.root(), workspace.as_deref())
}

/// `charter <extension>` with no command: what it adds, as usage.
fn list_commands(config_root: &std::path::Path, id: &str) -> ExitCode {
    let entry = extension::read(config_root, &extension::BuiltIn::none())
        .entry(id)
        .cloned();
    let declared = entry.map(|entry| extension::manifest_at(&entry.path));
    match declared {
        Some(Ok(manifest)) if !manifest.cli.is_empty() => {
            eprintln!("usage: charter {id} <command> …\n\n'{id}' adds these commands:");
            for command in &manifest.cli {
                let writes = if command.writes { " (writes)" } else { "" };
                eprintln!("  {} — {}{writes}", command.name, command.title);
            }
        }
        Some(Ok(_)) => eprintln!("charter: '{id}' adds no commands to charter's command line."),
        Some(Err(why)) => eprintln!("charter: charter could not read '{id}': {why}"),
        None => eprintln!("charter: no extension called '{id}' is installed on this machine."),
    }
    ExitCode::FAILURE
}
