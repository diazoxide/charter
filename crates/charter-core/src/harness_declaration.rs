//! Harness declarations: a harness as data ([ADR 0073], FD-14).
//!
//! A **harness declaration** says how to start one harness, what it can do and which levels it
//! runs at. A project declares a harness charter does not ship in `harnesses/<name>.toml`, and
//! the three charter ships (Claude Code, Codex, opencode) are declarations too, in this
//! module's directory, read by the same reader in the same format. `docs/plane-format.md`'s
//! `harnesses/<name>.toml` entry is the schema; this module is its reader.
//!
//! **A declaration is data and never a command.** `program` is a bare name found on `PATH`,
//! never a path or a shell string; the session templates take `{id}` and `{name}` and nothing
//! else; the environment it names is a namespace of the harness's own, and never charter's or
//! one that reads as a credential (ADR 0073 §5, ADR 0022's refusals). A project declaration's
//! program runs on this machine only once the operator has approved it, and again after any
//! change to it ([`crate::profiletrust::declaration_approval_needed`], V24b).
//!
//! **Level 2 is charter code** (V24c). A project declaration that says `hooks = true` is
//! refused: the hooks a harness reports through are armed by an adapter charter ships
//! ([`crate::harness::HarnessAdapter`]), and only a built-in has one. A declared harness runs
//! at level 1, its terminal alone.
//!
//! [ADR 0073]: ../../../docs/adr/0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use sha2::Digest as _;

use crate::harness::{DrawnWhole, SessionId};
use crate::shown;

/// The directory a project declares its harnesses in, at the project root.
pub const DIR: &str = "harnesses";

/// The record of the declarations this clone's operator approved, under `.charter/`.
pub const APPROVED: &str = "harness-declarations-approved.json";

/// The most a declaration file may be. It is read on the path that decides whether a chat
/// starts, so a planted giant is a launch that never finishes.
const MAX_BYTES: u64 = 64 * 1024;

/// Every harness capability charter reads today (ADR 0073 §6). A ticket that reads a new one
/// adds it here and to `docs/plane-format.md`.
pub const CAPABILITIES: [&str; 6] = [
    "reports_its_process",
    "reports_its_start_before_the_first_prompt",
    "keeps_conversations_by_directory",
    "reports_waiting",
    "resumes_by_id",
    "per_chat_plugins",
];

/// Where a declaration came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Shipped with charter, in this module's directory.
    BuiltIn,
    /// The project's `harnesses/<name>.toml`, committed.
    Project,
}

/// Who chooses a new session's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChosenBy {
    /// The harness, and charter learns it only from what the harness reports.
    Harness,
    /// charter, handed over by `[session] new` before the harness starts.
    Charter,
}

/// How a declared harness starts, names and resumes a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub chosen_by: ChosenBy,
    /// The words that start a new session, with `{id}` and `{name}` filled in.
    pub new: Vec<String>,
    /// The words that bring a session back, or none where the declaration says no way.
    pub resume: Option<Vec<String>>,
    /// The flags (or subcommands) by which the operator names a session themselves.
    pub named_by: Vec<String>,
}

/// When a chat just opened can have a curation prompt typed into it (ADR 0061).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyToType {
    /// On the harness's first `SessionStart` report: a hook, so level 2 only.
    OnStart,
    /// Once its terminal is raw and it has then written nothing for a quiet period.
    RawAndQuiet,
    /// Never: there is no way to tell.
    Never,
}

/// What a declared harness's terminal does with what is typed and pasted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terminal {
    /// The bytes it reads as a new line in its input, or none for the terminal's own Enter.
    pub newline: Option<String>,
    /// The biggest paste it draws whole, or none where nobody measured it.
    pub paste_drawn_whole: Option<DrawnWhole>,
    pub ready_to_type: ReadyToType,
}

/// The levels a declared harness offers (ADR 0073 §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Levels {
    /// Level 1: the terminal alone. Every declaration offers it.
    pub terminal: bool,
    /// Level 2: its own hooks, armed by an adapter charter ships.
    pub hooks: bool,
    /// Level 3 over ACP: the argv that starts its ACP agent, which HP-2's client speaks to.
    pub acp: Option<Vec<String>>,
}

/// One harness capability's answer. **Silence never reads as yes**: a capability a
/// declaration does not answer is [`Answer::Unknown`], which charter treats as no.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Yes,
    /// No, and the reason, as the declaration says it.
    No(String),
    Unknown,
}

impl Answer {
    /// Whether charter may rely on it: only a yes.
    pub fn holds(&self) -> bool {
        matches!(self, Self::Yes)
    }
}

/// One harness, declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The word a profile's `kind` names.
    pub name: String,
    /// What the operator calls it.
    pub title: String,
    /// A bare name found on `PATH`.
    pub program: String,
    /// The harness's own environment namespace: names, or prefixes ending `_*`.
    pub env: Vec<String>,
    /// The versions its facts were measured on, as written (TS1 settles the rest).
    pub tested: Option<String>,
    /// Where the operator logs in, said by a refusal that will not hold their credential.
    pub login: Option<String>,
    pub session: Session,
    pub terminal: Terminal,
    pub levels: Levels,
    capabilities: BTreeMap<String, Answer>,
    pub origin: Origin,
    /// `harnesses/<name>.toml`, or `built-in`.
    pub file: String,
    /// `sha256:<hex>` of the file's bytes: what an approval is of.
    pub digest: String,
    /// The file as written, for `charter harness show`.
    pub text: String,
}

impl Declaration {
    /// The answer the declaration gives for `capability`, and [`Answer::Unknown`] where it
    /// gives none.
    pub fn capability(&self, capability: &str) -> Answer {
        self.capabilities
            .get(capability)
            .cloned()
            .unwrap_or(Answer::Unknown)
    }

    /// The words that start a new session under `id` and `name`, or none where the harness
    /// chooses its own id.
    pub fn new_session_argv(&self, id: &SessionId, name: &str) -> Vec<String> {
        filled(&self.session.new, id, name)
    }

    /// The words that bring session `id` back, or none where the declaration names no way.
    pub fn resume_argv(&self, id: &SessionId, name: &str) -> Option<Vec<String>> {
        self.session
            .resume
            .as_ref()
            .map(|words| filled(words, id, name))
    }

    /// Whether `args` already name a session, so charter adds none of its own: a flag in
    /// `named_by`, alone or with its value attached (`--resume=<id>`).
    pub fn session_named_in(&self, args: &[String]) -> bool {
        args.iter().any(|arg| {
            self.session.named_by.iter().any(|word| {
                arg == word
                    || (word.starts_with('-')
                        && arg.starts_with(word.as_str())
                        && arg.as_bytes().get(word.len()) == Some(&b'='))
            })
        })
    }

    /// Whether charter ships an adapter for this harness's hooks (level 2). Only a built-in
    /// can: a project's `hooks = true` is refused.
    pub fn has_adapter(&self) -> bool {
        self.origin == Origin::BuiltIn && self.levels.hooks
    }
}

fn filled(words: &[String], id: &SessionId, name: &str) -> Vec<String> {
    words
        .iter()
        .map(|word| word.replace("{id}", id.as_str()).replace("{name}", name))
        .collect()
}

/// A declaration charter will not read, and the one sentence saying why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// `harnesses/<file>`, or the directory itself.
    pub file: String,
    pub reason: String,
}

/// Every harness a project has: the built-ins first, then the project's in file-name order,
/// and every declaration refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declarations {
    pub declared: Vec<Declaration>,
    pub refused: Vec<Refused>,
}

impl Declarations {
    pub fn get(&self, name: &str) -> Option<&Declaration> {
        self.declared.iter().find(|d| d.name == name)
    }

    /// The project's own, without the built-ins.
    pub fn projects(&self) -> impl Iterator<Item = &Declaration> {
        self.declared.iter().filter(|d| d.origin == Origin::Project)
    }
}

static BUILT_IN: LazyLock<Vec<Declaration>> = LazyLock::new(|| {
    [
        include_str!("harness_declaration/claude.toml"),
        include_str!("harness_declaration/opencode.toml"),
        include_str!("harness_declaration/codex.toml"),
    ]
    .iter()
    .map(|text| {
        parse(text, Origin::BuiltIn, "built-in")
            .unwrap_or_else(|why| panic!("a built-in declaration charter ships is refused: {why}"))
    })
    .collect()
});

/// The declarations charter ships, in registry order (`claude`, `opencode`, `codex`).
pub fn builtins() -> &'static [Declaration] {
    &BUILT_IN
}

/// The built-in declaration named `name`.
pub fn builtin(name: &str) -> Option<&'static Declaration> {
    builtins().iter().find(|d| d.name == name)
}

/// Every harness the project at `root` has: the built-ins, then each `harnesses/*.toml`, and
/// each declaration refused with its reason. Never fails and runs nothing.
pub fn read(root: &Path) -> Declarations {
    let mut out = Declarations {
        declared: builtins().to_vec(),
        refused: Vec::new(),
    };
    let dir = root.join(DIR);
    let entries = match std::fs::symlink_metadata(&dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return out,
        Ok(meta) if meta.is_dir() => match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                out.refused.push(Refused {
                    file: format!("{DIR}/"),
                    reason: format!(
                        "{DIR}/ could not be read ({}), so no harness it declares was \
                         loaded — the built-in harnesses still are.",
                        shown::short(&e.to_string())
                    ),
                });
                return out;
            }
        },
        _ => {
            out.refused.push(Refused {
                file: format!("{DIR}/"),
                reason: format!(
                    "{DIR} is not a directory, so no harness it declares was loaded — a \
                     project declares each harness in {DIR}/<name>.toml."
                ),
            });
            return out;
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    for path in files {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file = format!("{DIR}/{}", shown::short(&format!("{stem}.toml")));
        match read_one(root, &path).and_then(|text| {
            let found = parse(&text, Origin::Project, &file)?;
            if found.name != stem {
                return Err(format!(
                    "{file} declares '{}', and a declaration's file is named after the \
                     harness it declares. Rename the file {}.toml.",
                    shown::short(&found.name),
                    shown::short(&found.name)
                ));
            }
            if out.get(&found.name).is_some() {
                return Err(format!(
                    "{file} declares '{}', which is a harness charter ships — a project adds \
                     harnesses and never replaces one, because a built-in's declaration \
                     decides how its chats are armed. Give it a name of its own.",
                    shown::short(&found.name)
                ));
            }
            Ok(found)
        }) {
            Ok(found) => out.declared.push(found),
            Err(reason) => out.refused.push(Refused { file, reason }),
        }
    }
    out
}

/// One declaration file's text, through the gates every read of a committed file that decides
/// a launch takes: no link on the way, a plain file, a bound, and text.
fn read_one(root: &Path, path: &Path) -> Result<String, String> {
    let name = shown::short(&path.display().to_string());
    let unreadable =
        |why: String| format!("{name} could not be read ({why}), so it declares nothing.");
    let mut open = crate::contain::open_no_link(root, path)
        .map_err(|e| unreadable(shown::short(&e.to_string())))?;
    let meta = open
        .metadata()
        .map_err(|e| unreadable(shown::short(&e.to_string())))?;
    if !meta.file_type().is_file() {
        return Err(unreadable("it is not a plain file".to_owned()));
    }
    if meta.len() > MAX_BYTES {
        return Err(unreadable(format!("it is over {MAX_BYTES} bytes")));
    }
    let mut text = String::new();
    io::Read::read_to_string(&mut open, &mut text)
        .map_err(|e| unreadable(shown::short(&e.to_string())))?;
    Ok(text)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    title: Option<String>,
    program: String,
    #[serde(default)]
    env: Vec<String>,
    tested: Option<String>,
    login: Option<String>,
    #[serde(default)]
    session: RawSession,
    #[serde(default)]
    terminal: RawTerminal,
    #[serde(default)]
    levels: RawLevels,
    #[serde(default)]
    capabilities: BTreeMap<String, String>,
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawSession {
    chosen_by: Option<String>,
    new: Option<Vec<String>>,
    resume: Option<Vec<String>>,
    #[serde(default)]
    named_by: Vec<String>,
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawTerminal {
    newline: Option<String>,
    paste_drawn_whole: Option<RawPaste>,
    ready_to_type: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPaste {
    lines: Option<u32>,
    chars: Option<u32>,
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawLevels {
    terminal: Option<bool>,
    #[serde(default)]
    hooks: bool,
    acp: Option<Vec<String>>,
}

/// `text` as a declaration from `origin`, named `file` in every refusal — or the one sentence
/// saying why it is not one. The first failure wins, so one file gets one sentence.
pub fn parse(text: &str, origin: Origin, file: &str) -> Result<Declaration, String> {
    let raw: Raw = toml::from_str(text).map_err(|e| {
        format!(
            "{file} is not a harness declaration charter reads ({}), so it declares nothing. \
             Fix the file.",
            shown::short(e.message())
        )
    })?;
    let at = |what: &str| format!("{file}'s {what}");
    if !crate::profiles::name_ok(&raw.name) {
        return Err(format!(
            "{} {} is not a name charter accepts — letters, digits, '_' and '-', starting \
             with a letter or digit. Rename it.",
            at("name"),
            shown::short(&raw.name)
        ));
    }
    if !bare_word(&raw.program) {
        return Err(format!(
            "{} {} is not a bare program name — a declaration names a program found on PATH, \
             never a path or a shell string, because it runs on a click. Name the program \
             alone; a profile in charter.local.toml can point a chat at a path on this \
             machine.",
            at("program"),
            shown::short(&raw.program)
        ));
    }
    if origin == Origin::Project
        && let Some(why) = launcher(&raw.program)
    {
        return Err(format!(
            "{} {} is {why} — a declaration names the harness's own program, and charter \
             does not run one that would run whatever its words say. Name the harness's \
             program.",
            at("program"),
            shown::short(&raw.program)
        ));
    }
    for name in &raw.env {
        env_ok(name).map_err(|why| format!("{} {}: {why}", at("env entry"), shown::short(name)))?;
    }
    let session = session(raw.session, &at, origin)?;
    let terminal = terminal(raw.terminal, &at)?;
    let levels = Levels {
        terminal: raw.levels.terminal.unwrap_or(true),
        hooks: raw.levels.hooks,
        acp: raw.levels.acp,
    };
    if !levels.terminal {
        return Err(format!(
            "{} says terminal = false — every harness charter runs starts in its terminal \
             (level 1), and a chat in a tab always does. Remove the line.",
            at("[levels]")
        ));
    }
    if levels.hooks && origin != Origin::BuiltIn {
        return Err(format!(
            "{} says hooks = true — level 2 needs an adapter charter ships to arm a harness's \
             hooks, and a declaration cannot carry one. Remove the line; the harness runs in \
             its terminal.",
            at("[levels]")
        ));
    }
    if terminal.ready_to_type == ReadyToType::OnStart && !levels.hooks {
        return Err(format!(
            "{} is on-start, which waits for a hook to report the start, and this harness \
             has no hooks charter arms. Write raw-and-quiet or never.",
            at("[terminal] ready_to_type")
        ));
    }
    if let Some(acp) = &levels.acp {
        let program_ok = acp.first().is_some_and(|program| {
            bare_word(program) && (origin == Origin::BuiltIn || launcher(program).is_none())
        });
        if !program_ok {
            return Err(format!(
                "{} is not a command charter runs — a list of words whose first is a bare \
                 program name found on PATH, and not a shell, an interpreter, a launcher or a \
                 harness charter ships. Write it as one.",
                at("[levels] acp")
            ));
        }
        template(&acp[1..], at("[levels] acp"), origin)?;
    }
    let mut capabilities = BTreeMap::new();
    for (key, value) in raw.capabilities {
        if !CAPABILITIES.contains(&key.as_str()) {
            return Err(format!(
                "{} {} is not a harness capability charter reads — one of: {}. Remove it.",
                at("[capabilities]"),
                shown::short(&key),
                CAPABILITIES.join(", ")
            ));
        }
        let answer = match value.as_str() {
            "yes" => Answer::Yes,
            "unknown" => Answer::Unknown,
            other => match other.strip_prefix("no:").map(str::trim) {
                Some(reason) if !reason.is_empty() => Answer::No(reason.to_owned()),
                _ => {
                    return Err(format!(
                        "{} {key} is {}, and a capability is \"yes\", \"no: <the reason>\" or \
                         \"unknown\". Write one of them.",
                        at("[capabilities]"),
                        shown::short(other)
                    ));
                }
            },
        };
        capabilities.insert(key, answer);
    }
    if capabilities.get("resumes_by_id") == Some(&Answer::Yes) && session.resume.is_none() {
        return Err(format!(
            "{} says resumes_by_id = \"yes\" and [session] has no resume — say how it \
             resumes, or answer no.",
            at("[capabilities]")
        ));
    }
    Ok(Declaration {
        title: raw.title.unwrap_or_else(|| raw.name.clone()),
        name: raw.name,
        program: raw.program,
        env: raw.env,
        tested: raw.tested,
        login: raw.login,
        session,
        terminal,
        levels,
        capabilities,
        origin,
        file: file.to_owned(),
        digest: format!(
            "sha256:{}",
            crate::extension::hex(&sha2::Sha256::digest(text.as_bytes()))
        ),
        text: text.to_owned(),
    })
}

fn session(
    raw: RawSession,
    at: &dyn Fn(&str) -> String,
    origin: Origin,
) -> Result<Session, String> {
    let chosen_by = match raw.chosen_by.as_deref() {
        None | Some("harness") => ChosenBy::Harness,
        Some("charter") => ChosenBy::Charter,
        Some(other) => {
            return Err(format!(
                "{} is {}, and it is \"harness\" or \"charter\". Write one of them.",
                at("[session] chosen_by"),
                shown::short(other)
            ));
        }
    };
    let new = raw.new.unwrap_or_default();
    template(&new, at("[session] new"), origin)?;
    let gives_id = new.iter().any(|word| word.contains("{id}"));
    match chosen_by {
        ChosenBy::Charter if !gives_id => {
            return Err(format!(
                "{} says chosen_by = \"charter\" and [session] new hands over no {{id}} — say \
                 how the id is handed over, or write chosen_by = \"harness\".",
                at("[session]")
            ));
        }
        ChosenBy::Harness if gives_id => {
            return Err(format!(
                "{} hands over an {{id}}, and chosen_by is \"harness\" — charter chooses an id \
                 only for a harness that takes one. Write chosen_by = \"charter\".",
                at("[session] new")
            ));
        }
        _ => {}
    }
    if let Some(resume) = &raw.resume {
        template(resume, at("[session] resume"), origin)?;
        if !resume.iter().any(|word| word.contains("{id}")) {
            return Err(format!(
                "{} names no {{id}}, so it could not bring any one session back. Put {{id}} \
                 where the harness takes it.",
                at("[session] resume")
            ));
        }
    }
    if raw.named_by.iter().any(String::is_empty) {
        return Err(format!(
            "{} holds an empty word. Remove it.",
            at("[session] named_by")
        ));
    }
    Ok(Session {
        chosen_by,
        new,
        resume: raw.resume,
        named_by: raw.named_by,
    })
}

fn terminal(raw: RawTerminal, at: &dyn Fn(&str) -> String) -> Result<Terminal, String> {
    let ready_to_type = match raw.ready_to_type.as_deref() {
        None | Some("never") => ReadyToType::Never,
        Some("raw-and-quiet") => ReadyToType::RawAndQuiet,
        Some("on-start") => ReadyToType::OnStart,
        Some(other) => {
            return Err(format!(
                "{} is {}, and it is \"on-start\", \"raw-and-quiet\" or \"never\". Write one \
                 of them.",
                at("[terminal] ready_to_type"),
                shown::short(other)
            ));
        }
    };
    if raw.newline.as_deref() == Some("") {
        return Err(format!(
            "{} is empty. Remove the line, and the chat keeps the terminal's own Enter.",
            at("[terminal] newline")
        ));
    }
    let paste_drawn_whole = match raw.paste_drawn_whole {
        None => None,
        Some(RawPaste { lines: Some(0), .. })
        | Some(RawPaste { chars: Some(0), .. })
        | Some(RawPaste {
            lines: None,
            chars: None,
        }) => {
            return Err(format!(
                "{} measures nothing — give lines, chars or both, each at least 1, and leave \
                 out the one that has no limit.",
                at("[terminal] paste_drawn_whole")
            ));
        }
        Some(RawPaste { lines, chars }) => Some(DrawnWhole {
            lines: lines.map_or(usize::MAX, |n| n as usize),
            chars: chars.map_or(usize::MAX, |n| n as usize),
        }),
    };
    Ok(Terminal {
        newline: raw.newline,
        paste_drawn_whole,
        ready_to_type,
    })
}

/// Every word after a program — a session template's, a resume's, an ACP command's — held to
/// the one shape charter allows (ruling V66c), and checked against the launchers too.
///
/// **Why an allowlist** (rulings V66, V66c): every word here is appended to a program's
/// command line on a click, out of a committed file. A list of refused shapes missed a
/// response file (`@args`), a glued value (`-I.`), a bare relative filename, a path after `=`;
/// so a word is taken only in a shape a reader can take at a glance for a flag or a value:
///
/// - a flag: `-x`, or `--name` (letters, digits and `-`);
/// - `--name=value`, the value plain: letters, digits and `-_.:,+`, no leading `.` and no
///   drive letter (`C:`), and
///   `{id}` or `{name}` inside it (`--title={name}`);
/// - a subcommand: lowercase letters, digits and `-`, no dot (`exec`, `run`);
/// - a bare `{id}` or `{name}`, for a built-in only, which passes them that way today.
fn template(words: &[String], what: String, origin: Origin) -> Result<(), String> {
    for word in words {
        if let Some(why) = word_refusal(word, origin) {
            return Err(format!(
                "{what} holds {}, which {why}. A word after the program is a flag (-x, \
                 --name), --name=<a plain value>, or a lowercase subcommand, and takes {{id}} \
                 and {{name}} only inside a flag's value (--session={{id}}). Rewrite it.",
                shown::short(word)
            ));
        }
    }
    Ok(())
}

/// Why `word` may not be a word after the program, or nothing (ruling V66c).
fn word_refusal(word: &str, origin: Origin) -> Option<&'static str> {
    if word.is_empty() {
        return Some("is empty");
    }
    if origin == Origin::BuiltIn && (word == "{id}" || word == "{name}") {
        return None;
    }
    let literal = word.replace("{id}", "").replace("{name}", "");
    if literal.contains('{') || literal.contains('}') {
        return Some("is a placeholder charter does not fill");
    }
    // `{name}` is the chat's name, which the operator types: at the start of a word it could
    // make the word a flag (`--yolo`).
    if word.starts_with("{name}") {
        return Some("starts with {name}, and a chat's name could make it a flag");
    }
    if word == "{id}" {
        return Some("is a bare {id}, which a project's declaration hands over as a flag's value");
    }
    if word.contains('/') || word.starts_with('~') || word.starts_with('.') {
        return Some("is a path");
    }
    if is_assignment(word) {
        return Some("is a VAR=value assignment");
    }
    if word.contains('@') {
        return Some("names a file to read more words from (@file)");
    }
    if word
        .chars()
        .any(|c| c.is_whitespace() || !c.is_ascii() || ";|&$`<>()[]*?!#'\"\\^".contains(c))
    {
        return Some("holds shell syntax or a space");
    }
    let flag_name = |name: &str| {
        let mut chars = name.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if let Some(long) = word.strip_prefix("--") {
        let (name, value) = match long.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (long, None),
        };
        if !flag_name(name) {
            return Some("is not a flag charter takes: a flag's name is letters, digits and '-'");
        }
        // A bare `--name` is a flag; `--name=value` has its value judged.
        let value = value?;
        let plain = value.replace("{id}", "").replace("{name}", "");
        // No `%`: some programs decode `%2F` into a path, and no measured harness passes one.
        // No drive letter: `C:` is a path on Windows.
        let drive = {
            let mut chars = value.chars();
            chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':')
        };
        if value.is_empty()
            || value.starts_with('.')
            || drive
            || !plain.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | ',' | '+')
            })
        {
            return Some(
                "gives a flag a value that is not plain: letters, digits and -_.:,+ with no \
                 leading dot and no drive letter",
            );
        }
        return word_launcher(&plain, origin);
    }
    if let Some(short) = word.strip_prefix('-') {
        let mut chars = short.chars();
        let one = chars.next().is_some_and(|c| c.is_ascii_alphanumeric()) && chars.next().is_none();
        return (!one).then_some("glues a value to a short flag, or is not a flag charter takes");
    }
    let subcommand = word
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !subcommand {
        return Some(
            "is not a flag, a flag's value or a lowercase subcommand (a bare filename, a dot or \
             a capital is none of them)",
        );
    }
    word_launcher(word, origin)
}

/// What the approval dialog says beside `word` when it names a file or folder at the project
/// root (`root`), or nothing: a flag's value or a subcommand-shaped word the program may
/// resolve in the chat's folder. The shapes allow it; the operator sees it (ruling of
/// 2026-10-03, V66c's residual).
pub fn names_in_project(root: &Path, word: &str) -> Option<String> {
    let named = match word.strip_prefix("--") {
        Some(long) => long.split_once('=')?.1,
        None if word.starts_with('-') => return None,
        None => word,
    };
    if named.is_empty() || named.contains('{') {
        return None;
    }
    let found = std::fs::symlink_metadata(root.join(named)).ok()?;
    let what = if found.is_dir() { "folder" } else { "file" };
    Some(format!(
        "{} names the {what} {} in this project; the program may read it",
        shown::readable(word, usize::MAX),
        shown::readable(named, usize::MAX)
    ))
}

/// Why a subcommand or a flag's value names a program that would run whatever follows it, or
/// nothing: the launcher check, over every word, as defence in depth (ruling V66c). `exec` is a
/// subcommand (`codex exec`), and no program on PATH.
///
/// A built-in's words are charter's own, and are not asked: the built-ins are what
/// [`builtins`] is being made from when they are read, so asking it would wait on itself.
fn word_launcher(word: &str, origin: Origin) -> Option<&'static str> {
    if origin == Origin::BuiltIn || word.is_empty() || word == "exec" {
        return None;
    }
    let lower = word.to_ascii_lowercase();
    let family = INTERPRETER_FAMILIES.iter().any(|family| {
        lower
            .strip_prefix(family)
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || c == '.'))
    });
    (family
        || LAUNCHERS.contains(&lower.as_str())
        || builtins()
            .iter()
            .any(|d| d.program.eq_ignore_ascii_case(word)))
    .then_some("names a shell, an interpreter, a launcher or a harness charter ships")
}

/// `NAME=…` with `NAME` a variable's name: what `env` and a shell read as an assignment.
fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A program name found on `PATH`: letters, digits, `.`, `_`, `+` and `-`, starting with a
/// letter or a digit — so never a path, a flag, `~`, or a shell string.
fn bare_word(word: &str) -> bool {
    let mut chars = word.chars();
    word.len() <= 128
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

/// Shells, interpreters and launchers: programs that run whatever their arguments say, so a
/// declaration naming one would be a script in a file (ruling V66). Matched without case.
const LAUNCHERS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "mksh",
    "csh",
    "tcsh",
    "ash",
    "busybox",
    "nu",
    "xonsh",
    "elvish",
    "env",
    "sudo",
    "doas",
    "su",
    "nohup",
    "exec",
    "xargs",
    "time",
    "nice",
    "timeout",
    "script",
    "watch",
    "parallel",
    "node",
    "nodejs",
    "deno",
    "bun",
    "npx",
    "npm",
    "pnpm",
    "pnpx",
    "yarn",
    "uv",
    "uvx",
    "pipx",
    "pip",
    "perl",
    "ruby",
    "php",
    "lua",
    "luajit",
    "tclsh",
    "wish",
    "expect",
    "osascript",
    "pwsh",
    "powershell",
    "cmd",
    "java",
    "awk",
    "gawk",
    "mawk",
    "nawk",
    "sed",
    "make",
    "docker",
    "podman",
    "ssh",
    "open",
    "xdg-open",
    "launchctl",
    "systemd-run",
    "flatpak",
    "snap",
    "nix",
    "nix-shell",
    "guix",
    "arch",
    "caffeinate",
    "xcrun",
    "sandbox-exec",
    "stdbuf",
    "chrt",
    "taskset",
    "strace",
    "ltrace",
    "dtrace",
    "dtruss",
    "tmux",
    "screen",
    "go",
    "cargo",
    "rustc",
    "swift",
    "swiftc",
    "dotnet",
    "bunx",
    "corepack",
    "rscript",
    "r",
    "julia",
    "tsx",
    "ts-node",
    "qjs",
    "jshell",
    "groovy",
    "scala",
    "kotlin",
    "kotlinc",
    "elixir",
    "iex",
    "mix",
    "erl",
    "escript",
    "ghci",
    "runghc",
    "stack",
    "cabal",
    "racket",
    "guile",
    "sbcl",
    "clisp",
    "ocaml",
    "utop",
    "vim",
    "nvim",
    "vi",
    "emacs",
    "ex",
    "ed",
    "nano",
    "gdb",
    "lldb",
    "git",
    "just",
    "bundle",
    "bundler",
    "rake",
    "gem",
    "poetry",
    "pipenv",
    "conda",
    "mamba",
    "micromamba",
    "mise",
    "rtx",
    "asdf",
    "direnv",
    "volta",
    "fnm",
    "nvm",
    "nodenv",
    "pyenv",
    "rbenv",
    "goenv",
    "jenv",
    "sdk",
    "zx",
    "nushell",
    "yash",
    "rc",
    "osh",
    "oil",
    "ion",
    "find",
    "chroot",
    "unshare",
    "nsenter",
    "firejail",
    "bwrap",
    "proot",
    "setsid",
    "runuser",
    "pkexec",
    "gosu",
    "su-exec",
    "entr",
    "nodemon",
    "watchexec",
    "foreman",
    "honcho",
    "pm2",
    "at",
    "batch",
    "crontab",
    "toybox",
    "automator",
    "shortcuts",
    "login",
    "charter",
    "edm",
];

/// Interpreters that come in versioned names (`python3.12`, `ruby3`, `pwsh-preview`): a
/// program starting with one is refused, and a word that is one plus a version.
const INTERPRETER_FAMILIES: &[&str] = &[
    "python", "pypy", "ruby", "perl", "php", "node", "lua", "bash", "zsh", "pwsh", "julia",
];

/// Why `program` may not be a project declaration's program, or nothing.
fn launcher(program: &str) -> Option<&'static str> {
    let lower = program.to_ascii_lowercase();
    if builtins()
        .iter()
        .any(|d| d.program.eq_ignore_ascii_case(program))
    {
        return Some(
            "the program of a harness charter ships, which runs only armed, through its adapter",
        );
    }
    let interpreter = INTERPRETER_FAMILIES
        .iter()
        .any(|family| lower.starts_with(family));
    (interpreter || LAUNCHERS.contains(&lower.as_str()))
        .then_some("a shell, an interpreter or a launcher")
}

/// Variables that change what a process loads or runs, or where it finds programs: a namespace
/// reaching one would let a declaration steer the harness's own process (ruling V66).
const INJECTORS: &[&str] = &[
    "LD_*",
    "DYLD_*",
    "NODE_OPTIONS",
    "NODE_PATH",
    "PYTHONPATH",
    "PYTHONHOME",
    "PYTHONSTARTUP",
    "PERL5LIB",
    "PERL5OPT",
    "PERLLIB",
    "RUBYOPT",
    "RUBYLIB",
    "JAVA_TOOL_OPTIONS",
    "BASH_ENV",
    "ENV",
    "PATH",
    "HOME",
    "SHELL",
    "ZDOTDIR",
    "PROMPT_COMMAND",
    "IFS",
    "GIT_*",
];

/// Whether two names-or-prefixes (`NAME`, `PREFIX*`) could name one variable, ignoring case.
fn overlap(one: &str, other: &str) -> bool {
    let (one, other) = (one.to_ascii_uppercase(), other.to_ascii_uppercase());
    match (one.strip_suffix('*'), other.strip_suffix('*')) {
        (None, None) => one == other,
        (None, Some(prefix)) => one.starts_with(prefix),
        (Some(prefix), None) => other.starts_with(prefix),
        (Some(a), Some(b)) => a.starts_with(b) || b.starts_with(a),
    }
}

/// Why `name` may not be in a declaration's environment namespace, or nothing.
fn env_ok(name: &str) -> Result<(), String> {
    let base = name.strip_suffix('*').unwrap_or(name);
    let shaped = base.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && base
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && (base.len() == name.len() || base.ends_with('_'));
    if !shaped {
        return Err(
            "a namespace entry is a variable's name in capitals, digits and '_', or a prefix \
             ending '_*', such as GEMINI_*. Write it that way."
                .to_owned(),
        );
    }
    if base.starts_with("CHARTER_") || "CHARTER_".starts_with(base) {
        return Err(
            "it reaches charter's own variables, which charter sets itself. Remove it.".to_owned(),
        );
    }
    if crate::profiles::named_like_a_credential(base)
        || crate::chatenv::CREDENTIALS
            .iter()
            .any(|credential| overlap(name, credential))
    {
        return Err(
            "it is named like a credential, or reaches a family of credentials (a forge's, a \
             cloud's, a model provider's or a registry's), and a declaration never passes one \
             into a chat. Remove it."
                .to_owned(),
        );
    }
    if INJECTORS.iter().any(|injector| overlap(name, injector)) {
        return Err(
            "it reaches a variable that changes what a process loads, runs or finds on PATH, \
             and a declaration never sets how the harness's own process starts. Remove it."
                .to_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
