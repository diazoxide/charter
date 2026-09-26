//! The commands an extension adds to the `charter` command line, under its own id
//! (charter-app#342, ADR 0053).
//!
//! **`charter <extension id> <command> …`, and never a word of charter's own.** A command is
//! reached only through the extension's id, so an extension can add words below its own name
//! and none beside charter's. An id that *is* one of charter's own command words is refused when
//! the manifest is read ([`CORE_WORDS`]), so it is refused at install, at approval and at every
//! later read, and no extension can stand where a core command stands.
//!
//! **A command says whether it writes.** One that does is listed in the approval prompt, and is
//! held to the plane paths the extension declares it writes (`super::writes`): it is handed them
//! resolved, and a change outside them is reported after it ran — detected, not prevented, as
//! for an action (ADR 0041). One that says it only reads is handed no path at all, and any change
//! to the plane while it ran is reported. A chat's `charter <id> <command>` is judged by the same
//! tool guard as any `charter` call, and one that writes is never waved through by a persona's
//! grant ([`crate::personagate`]).

use super::{ok_in_a_part_id, title_of};

/// Every word `charter` itself answers as its first argument: each core command's name, each
/// other name clap accepts for it (`ws`), and clap's own `help`. **An extension whose id is one
/// of these is refused**, so `charter <word>` always means what the core says it means.
///
/// **Written here, and held to the command line's own definition by a test there.** The core
/// cannot read the `charter` binary's parser — the app approves extensions and does not link the
/// command line — so the list lives with the refusal, and a test in the `charter` binary
/// (`core_word_tests` in `charter-cli/src/main.rs`) reads every word off the parser itself and
/// fails for a core word this list is missing, or one it lists that is gone. So a new core
/// command is not protected by itself: CI goes red on the change that adds it until the word is
/// here, and that change does not merge before it is. The `charter` binary needs no list: it
/// asks its parser before it looks for an extension.
pub const CORE_WORDS: [&str; 31] = [
    "browser",
    "clone",
    "discover",
    "docs",
    "doctor",
    "git-policy",
    "gl-refresh",
    "guard",
    "handoff",
    "harness",
    "help",
    "hook",
    "init",
    "news",
    "persona",
    "plugin",
    "recall",
    "reinit",
    "root",
    "save",
    "secret",
    "status",
    "statusline",
    "sync",
    "update",
    "vault",
    "version",
    "workspace",
    "worktree",
    "ws",
    "wt",
];

/// Why `id` may not be an extension's id, when it is one of [`CORE_WORDS`].
pub(super) fn takes_a_core_word(id: &str) -> Option<String> {
    CORE_WORDS.contains(&id).then(|| {
        format!(
            "has the id \"{id}\", which is one of charter's own commands (`charter {id}`). An \
             extension is reached at the command line by its id, and none may stand where a core \
             command does, so charter will not load it. Give it an id charter does not use."
        )
    })
}

/// A core command that forwards to an extension's command, keeping its own words — how
/// `charter ws todo` keeps its words and its output once todos is an extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alias {
    /// The words it is typed as, after `charter`.
    pub words: &'static [&'static str],
    /// The extension it forwards to, by id.
    pub extension: &'static str,
    /// The command it runs there.
    pub command: &'static str,
}

/// Every core-owned alias this build has. **None in a release build**: nothing has moved into
/// an extension yet. A build carrying the plane fence — a test build, never a release
/// (`crate::fence`) — has two onto the `extension-probe`, so forwarding is proven to give exactly
/// the direct command's output.
///
/// Here rather than in the command line, so that the tool guard reads a chat's aliased call as
/// the extension command it runs ([`extension_command`]).
pub fn aliases() -> &'static [Alias] {
    const PROBE: &[Alias] = &[
        Alias {
            words: &["ws", "probe-echo"],
            extension: "extension-probe",
            command: "echo",
        },
        Alias {
            words: &["ws", "probe-fail"],
            extension: "extension-probe",
            command: "fail",
        },
    ];
    if crate::fence::FENCED { PROBE } else { &[] }
}

/// The alias `words` begin with, if any.
pub fn alias_of<S: AsRef<str>>(words: &[S]) -> Option<&'static Alias> {
    aliases().iter().find(|alias| {
        words.len() >= alias.words.len()
            && alias
                .words
                .iter()
                .zip(words)
                .all(|(want, got)| *want == got.as_ref())
    })
}

/// The extension and the command `charter <args…>` runs, when it runs one — through a core-owned
/// alias, or as `charter <id> <command>` with a first word that is not a core word. `None` for a
/// core command. The command is `""` when none is named.
pub fn extension_command<S: AsRef<str>>(args: &[S]) -> Option<(&str, &str)> {
    if let Some(alias) = alias_of(args) {
        return Some((alias.extension, alias.command));
    }
    let first = args.first()?.as_ref();
    if first.starts_with('-') || CORE_WORDS.contains(&first) {
        return None;
    }
    Some((first, args.get(1).map_or("", AsRef::as_ref)))
}

/// Whether the installed extension `id` declares `command` as one that only reads — the one case
/// a persona's grant may wave a chat's extension command through. Anything charter cannot read,
/// or a command it does not declare, is not one: charter cannot say it only reads.
///
/// **The manifest as it is on disk, and not the approval.** This decides whether the operator is
/// asked, not whether anything runs: the executor refuses an extension that changed since its yes
/// when the command starts, so a manifest edited to call a writing command a reading one is
/// refused there rather than run.
pub fn only_reads(config_root: &std::path::Path, id: &str, command: &str) -> bool {
    let loaded = super::read(config_root, &super::BuiltIn::none());
    let Some(entry) = loaded.entry(id) else {
        return false;
    };
    super::manifest_at(&entry.path).is_ok_and(|manifest| {
        manifest
            .cli
            .iter()
            .any(|it| it.name == command && !it.writes)
    })
}

/// The most commands one extension may add. Each one is a word below its id that a chat or a
/// script may type, and a handful is what one extension needs.
const MOST_COMMANDS: usize = 16;

/// The keys a declared command may carry.
const COMMAND_KEYS: [&str; 3] = ["name", "title", "writes"];

/// One command an extension adds to the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliCommand {
    /// The word after the extension's id: one segment, unique within the extension.
    pub name: String,
    /// What it does, in a line — what the approval prompt says of a command that writes.
    pub title: String,
    /// Whether the manifest says it writes. Said, never assumed: a manifest that left it out
    /// has not made the choice the operator reads in the prompt.
    pub writes: bool,
}

/// The commands a manifest's `contributes.cli` declares, or why charter will not read them.
///
/// `program` is the manifest's `runs`, and `declares_writes` whether it declares plane paths it
/// writes: a command that writes is held to those paths, so one declared without any is a
/// write the prompt could not say where.
pub(super) fn commands_of(
    value: &serde_json::Value,
    program: Option<&str>,
    declares_writes: bool,
) -> Result<Vec<CliCommand>, String> {
    let list = value
        .as_array()
        .ok_or("has a 'contributes.cli' that is not an array")?;
    if list.is_empty() {
        return Err(
            "declares no commands under 'contributes.cli', so there is nothing it would add".into(),
        );
    }
    if program.is_none() {
        return Err(
            "declares commands and no program ('runs') to run them, so charter would offer \
             commands that can never do anything"
                .into(),
        );
    }
    if list.len() > MOST_COMMANDS {
        return Err(format!(
            "declares {} commands, and charter adds at most {MOST_COMMANDS} from one extension",
            list.len()
        ));
    }
    let mut commands: Vec<CliCommand> = Vec::with_capacity(list.len());
    for (at, raw) in list.iter().enumerate() {
        let object = raw
            .as_object()
            .ok_or_else(|| format!("declares a command at {at} that is not an object"))?;
        for key in object.keys() {
            if !COMMAND_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "declares a command at {at} carrying {key:?}, which is not part of what a \
                     command may say — a command is {}",
                    COMMAND_KEYS.join(", ")
                ));
            }
        }
        let name = object
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("declares a command at {at} with no name"))?;
        if !ok_in_a_part_id(name) {
            return Err(format!(
                "declares the command {name:?}, and a command's name is letters, digits, '-' and \
                 '_', starting with a letter or a digit"
            ));
        }
        if commands.iter().any(|seen| seen.name == name) {
            return Err(format!("declares two commands called {name:?}"));
        }
        let title = title_of(object.get("title"), "the command", name)?;
        let writes = object
            .get("writes")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| {
                format!(
                    "declares the command {name:?} without 'writes' — a command says whether it \
                     writes, true or false"
                )
            })?;
        if writes && !declares_writes {
            return Err(format!(
                "declares the command {name:?} as one that writes, and declares no plane paths it \
                 writes ('contributes.writes'), so charter could not tell you where"
            ));
        }
        commands.push(CliCommand {
            name: name.to_owned(),
            title,
            writes,
        });
    }
    Ok(commands)
}

/// One line for the approval prompt, for a command that writes. A command that only reads is
/// not listed: the capability's own line says the extension adds commands.
pub(super) fn declares(command: &CliCommand, id: &str) -> String {
    format!(
        "a command that writes, `charter {id} {}` — {}; it writes to the plane paths listed here",
        command.name, command.title
    )
}
