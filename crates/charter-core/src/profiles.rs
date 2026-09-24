//! Harness profiles: which program a chat runs, with what environment, on THIS machine.
//!
//! A port of `charter/profiles.py`, decided by [ADR 0022]. Every refusal sentence charter
//! WRITES is that file's, word for word, because both implementations read one plane and an
//! operator must not get two different answers about their own file.
//!
//! **Three details are quoted from somewhere else and do not match**, found in review and
//! recorded here rather than left to be discovered: the TOML parser's message for a
//! `charter.local.toml` that will not parse, the operating system's message for one that
//! cannot be read, and the same for `git`. Those are `toml_edit`'s and Rust's words against
//! `tomllib`'s and CPython's. The sentence AROUND them is identical, which is what a
//! differential scenario pins — it masks the parenthesised detail and compares the rest —
//! and what an operator acts on is the sentence and the fix, not the parser's diagnostic.
//!
//! **Why a refusal and not a warning.** A profile's `command` runs on a click. Between
//! pressing "New chat" and the exec there is no harness permission prompt, no tool call a
//! guard could deny, nothing that shows a person the words about to be run. Everything here
//! that looks like suspicion of the operator's own file is that sentence, applied.
//!
//! **Profiles are read only from `charter.local.toml`**, beside `charter.toml` and out of
//! git: a command in the committed file could be changed by a merged pull request and then
//! run on every machine that pulls it. A `[harness.<name>]` table in `charter.toml` is
//! refused with a pointer to the local file.
//!
//! Nothing here runs a subprocess except [`ignore_check`], which runs one `git status` and
//! only where a person asked.
//!
//! [ADR 0022]: https://github.com/diazoxide/charter/blob/main/docs/adr/0022-a-harness-profile-belongs-to-one-machine.md

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::shown;

/// The file profiles live in, beside the plane's `charter.toml`.
pub const LOCAL_FILE: &str = "charter.local.toml";

/// The committed file, which holds `[harness] default` and no profile.
pub const COMMITTED_FILE: &str = "charter.toml";

/// Words an env NAME may not contain, matched case-insensitively.
///
/// Anything set on the harness process reaches the shell the model runs — measured on
/// Claude Code and on Codex — so charter declines to hold a credential in a profile. A
/// pattern can refuse an innocent name (`KEYBOARD_LAYOUT` holds `KEY`); this is a refusal to
/// hold a credential, not a barrier against one, and a wrapper script named in `command` can
/// still export whatever it likes.
const SECRET_WORDS: [&str; 4] = ["KEY", "TOKEN", "SECRET", "PASSWORD"];

/// The prefix of charter's own variables, which charter sets itself (ruling 14).
const CHARTER_PREFIX: &str = "CHARTER_";

/// The one key under `[harness]` that names the default rather than declaring a profile.
const DEFAULT: &str = "default";

/// Everything a profile's table may hold.
const PROFILE_KEYS: [&str; 3] = ["kind", "command", "env"];

/// A harness kind: the word typed after `charter`, the name the registry calls it, and the
/// program a built-in profile of that kind runs.
///
/// **All three kinds, including the one this app cannot yet start.** Parsing is not
/// launching: until M4 the Python charter and this one read the same plane, so an `opencode`
/// profile the Python charter accepts must not be refused here — the operator would get two
/// different answers about one file. What this app will not do is *start* one; that refusal
/// belongs where a chat is launched (spec decision 6), not where a file is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    /// The word typed after `charter`, and a profile's `kind`.
    pub word: &'static str,
    /// The registry's own name for it, which is what `CHARTER_HARNESS` carries.
    pub registry: &'static str,
    /// What a built-in profile of this kind runs.
    pub binary: &'static str,
    /// Where an operator logs in, named by a refusal that will not hold their credential.
    login: &'static str,
}

/// Every kind, in REGISTRY order — which is the order the built-ins are listed in, and the
/// order a refusal names them in. It is not alphabetical (`charter.harness.registry.all()`).
pub const KINDS: [Kind; 3] = [
    Kind {
        word: "claude",
        registry: "claude-code",
        binary: "claude",
        login: "set CLAUDE_CONFIG_DIR and run /login inside Claude Code",
    },
    Kind {
        word: "opencode",
        registry: "opencode",
        binary: "opencode",
        login: "set XDG_DATA_HOME and run opencode auth login",
    },
    Kind {
        word: "codex",
        registry: "codex",
        binary: "codex",
        login: "set CODEX_HOME and run codex login",
    },
];

fn kind_of(word: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.word == word)
}

/// Every `charter <word>` this plane's CLI answers, which is a name a profile may not take.
///
/// Taken from the Python charter's `cli.command_words()`, which is the surface both binaries
/// share until M2 finishes porting it. A profile named like a command would shadow the
/// command, and the name belongs to the command.
const COMMAND_WORDS: [&str; 53] = [
    "_version-check",
    "browser",
    "change",
    "clone",
    "discover",
    "docs",
    "doctor",
    "frame",
    "frame-bar-rows",
    "frame-chat",
    "frame-chrome",
    "frame-close",
    "frame-density",
    "frame-ended",
    "frame-gather",
    "frame-launch",
    "frame-new-chat",
    "frame-palette",
    "frame-probe",
    "frame-quit",
    "frame-rename",
    "frame-resize",
    "frame-respawn",
    "frame-switch",
    "frame-toggle",
    "frame-transcript",
    "git-policy",
    "gl-refresh",
    "guard",
    "handoff",
    "harness",
    "hook",
    "init",
    "news",
    "panel",
    "persona",
    "recall",
    "reinit",
    "reopen",
    "report",
    "save",
    "secret",
    "status",
    "statusline",
    "sync",
    "trace",
    "update",
    "vault",
    "version",
    "workspace",
    "worktree",
    "ws",
    "wt",
];

/// The names charter itself is invoked under (`charter/hooks.py:837`).
const CHARTER_PROGS: [&str; 2] = ["charter", "edm"];

/// Where a profile came from. A refusal carries the file's name as text, because it may name
/// `charter.toml`, which never sources a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A registered kind nobody declared.
    BuiltIn,
    /// Declared in `charter.local.toml`.
    Local,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built-in",
            Self::Local => LOCAL_FILE,
        }
    }
}

/// One profile: a name, a kind, a command and an environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// `claude-work`; a built-in is named after its kind.
    pub name: String,
    /// The word after `charter`: `claude`, `opencode` or `codex`.
    pub kind: String,
    /// The registry name, which is what `CHARTER_HARNESS` carries: `claude-code`.
    pub harness: String,
    /// argv, **as declared, before `~` is expanded**.
    pub command: Vec<String>,
    /// The harness process's environment, as declared and sorted by name.
    pub env: Vec<(String, String)>,
    pub source: Source,
}

/// A profile charter will not use, and the one sentence saying why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The name, already contained for display; empty for a whole-file refusal.
    pub name: String,
    /// `charter.local.toml` or `charter.toml`.
    pub source: String,
    /// The rule, then the fix, in one sentence.
    pub reason: String,
}

/// Every profile a plane has, every declared one refused, and the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileSet {
    /// Built-ins first in registry order, then declared profiles in file order. A declared
    /// profile that replaces a built-in keeps the BUILT-IN's place, because this order is
    /// what the selector and the listing draw.
    profiles: Vec<Profile>,
    pub refused: Vec<Refused>,
    /// The row the selector starts on. It launches nothing by itself.
    pub default: Option<String>,
    pub default_from: Option<String>,
    /// A `default` that named no profile this machine has, kept so a surface can say so.
    pub default_refused: Option<String>,
}

impl ProfileSet {
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// Put `profile` in, keeping the place a profile of that name already had.
    fn put(&mut self, profile: Profile) {
        match self.profiles.iter_mut().find(|p| p.name == profile.name) {
            Some(slot) => *slot = profile,
            None => self.profiles.push(profile),
        }
    }

    fn take(&mut self, name: &str) {
        self.profiles.retain(|p| p.name != name);
    }
}

/// Whether git would carry the local file: the refusal, and the one fix for its state. Both
/// empty when the file may be used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnoreCheck {
    pub reason: String,
    pub fix: String,
}

impl IgnoreCheck {
    pub fn passes(&self) -> bool {
        self.reason.is_empty()
    }
}

/// One built-in profile per registered kind, named after its kind.
pub fn builtins() -> Vec<Profile> {
    KINDS
        .iter()
        .map(|k| Profile {
            name: k.word.to_owned(),
            kind: k.word.to_owned(),
            harness: k.registry.to_owned(),
            command: vec![k.binary.to_owned()],
            env: Vec::new(),
            source: Source::BuiltIn,
        })
        .collect()
}

/// Every profile this plane has, and every declared one refused with its reason.
///
/// Never fails and runs no subprocess. The two rules that need charter's own command surface
/// are [`current`]'s.
pub fn derive(root: &Path) -> ProfileSet {
    let committed = std::fs::read_to_string(root.join(COMMITTED_FILE)).ok();
    derive_from(committed.as_deref(), read_local(root))
}

/// The local file as [`derive_from`] takes it: its text, `None` when there is none, or the
/// error that stopped the read.
pub fn read_local(root: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(root.join(LOCAL_FILE)) {
        Ok(text) => Ok(Some(text)),
        // An absent file declares nothing and is not a refusal.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// [`derive`] over the two files' TEXT rather than the plane's — what the files would say, so
/// the Project settings tab can ask it of a file before it is written (charter-app#252) and
/// get the refusals the next read would give, in the same words.
///
/// `local` is the local file's read: `Ok(None)` when there is none, and the error when there is
/// one that could not be read.
pub fn derive_from(committed: Option<&str>, local: std::io::Result<Option<String>>) -> ProfileSet {
    let mut set = ProfileSet {
        profiles: builtins(),
        ..ProfileSet::default()
    };

    // 1. The committed file: a profile there is refused with a pointer to the local file,
    //    `default` is the candidate default, and any other key is ignored as it always was.
    if let Some(committed) = committed.and_then(|text| text.parse::<toml::Table>().ok())
        && let Some(harness) = committed.get("harness").and_then(toml::Value::as_table)
    {
        for (key, value) in harness {
            if value.is_table() {
                let name = shown::short(key);
                set.refused.push(Refused {
                    reason: format!(
                        "[harness.{name}] is in charter.toml, which is committed — a \
                         profile's command runs on a click, so charter reads profiles only \
                         from charter.local.toml, which stays on this machine. Move the \
                         table there; charter.toml's [harness] keeps `default` alone."
                    ),
                    name,
                    source: COMMITTED_FILE.to_owned(),
                });
            } else if key == DEFAULT {
                set.default = value.as_str().map(str::to_owned);
                set.default_from = Some(COMMITTED_FILE.to_owned());
                // A non-string `default` is kept as "named nothing", below.
                if !value.is_str() {
                    set.default = None;
                    set.default_refused = Some(shown::short(&py_str(value)));
                }
            }
        }
    }

    // 2. The local file: only `[harness]` is read here (`[extensions]`, `[theme]` and
    //    `[harness_plugins]` are let through).
    let local = match local {
        Ok(Some(text)) => match text.parse::<toml::Table>() {
            Ok(table) => Some(table),
            Err(e) => {
                set.refused.push(Refused {
                    name: String::new(),
                    source: LOCAL_FILE.to_owned(),
                    reason: format!(
                        "charter.local.toml could not be read ({}), so no declared profile \
                         was loaded — the built-in profiles still are. Fix the file and run \
                         charter harness list.",
                        shown::short(&e.to_string())
                    ),
                });
                None
            }
        },
        // An absent file declares nothing and is not a refusal. Anything else that stops the
        // read IS one, because a file that is there and says nothing is indistinguishable
        // from one nobody wrote.
        Ok(None) => None,
        Err(e) => {
            set.refused.push(Refused {
                name: String::new(),
                source: LOCAL_FILE.to_owned(),
                reason: format!(
                    "charter.local.toml could not be read ({}), so no declared profile was \
                     loaded — the built-in profiles still are. Fix the file and run charter \
                     harness list.",
                    shown::short(&e.to_string())
                ),
            });
            None
        }
    };
    let Some(top) = local else {
        return settle(set);
    };

    for (key, _) in &top {
        // `[extensions]` is this machine's choice of which approved extensions a project has
        // on, and what they are set to (charter-app#253, ADR 0048). It is read by
        // `extension::project`, not here, and it cannot reach past this machine's approval —
        // so it changes nothing a teammate's clone does, which is the reason for the rule below.
        //
        // `[theme]` is the same kind of choice (charter-app#273): which theme this machine draws
        // the project in, read by `extension::project::theme`.
        //
        // `[harness_plugins]` is the same kind of choice among what this machine's harnesses
        // have installed (charter-app#274, ADR 0050), read by `harness_plugin`.
        if key != "harness"
            && key != crate::extension::project::TABLE
            && key != crate::extension::project::theme::TABLE
            && key != crate::harness_plugin::TABLE
        {
            let name = shown::short(key);
            set.refused.push(Refused {
                reason: format!(
                    "[{name}] in charter.local.toml is not read — that file carries \
                     [harness], [extensions], [theme] and [harness_plugins] and nothing else, \
                     because an ignored file must not change plane policy with no trace in git. \
                     Put [{name}] in charter.toml."
                ),
                name,
                source: LOCAL_FILE.to_owned(),
            });
        }
    }
    let harness = match top.get("harness") {
        Some(toml::Value::Table(table)) => table.clone(),
        Some(_) => {
            // Its own sentence (F5): the file parsed, so calling it unreadable would send
            // the reader to fix TOML that is fine — ADR 0009's rule that an answer says its
            // kind.
            set.refused.push(Refused {
                name: String::new(),
                source: LOCAL_FILE.to_owned(),
                reason: "harness in charter.local.toml is not a table, so no declared \
                         profile was read — the file holds a [harness] table with one \
                         [harness.<name>] table per profile. Write it that way."
                    .to_owned(),
            });
            toml::Table::new()
        }
        None => toml::Table::new(),
    };

    for (name, table) in &harness {
        if name == DEFAULT && !table.is_table() {
            set.default = table.as_str().map(str::to_owned);
            set.default_from = Some(LOCAL_FILE.to_owned());
            if !table.is_str() {
                set.default = None;
                set.default_refused = Some(shown::short(&py_str(table)));
            }
            continue;
        }
        if let Some(inner) = table.as_table() {
            // A table under a key that is not a profile key. Once parsed it is either a
            // dotted name written without quotes or a typo'd key holding an inline table —
            // the two cannot be told apart — so the refusal names both readings.
            let nested: Vec<&String> = inner
                .iter()
                .filter(|(key, value)| !PROFILE_KEYS.contains(&key.as_str()) && value.is_table())
                .map(|(key, _)| key)
                .collect();
            for key in &nested {
                let parent = shown::short(name);
                let child = shown::short(key);
                let dotted = shown::short(&format!("{name}.{key}"));
                set.refused.push(Refused {
                    reason: format!(
                        "[harness.{parent}] holds a table {child}, which charter reads \
                         neither way — {child} is not a key a profile has (kind, command \
                         and env), and if a profile named '{dotted}' was meant, a profile's \
                         name is letters, digits, '_' and '-', with no dot — the plane \
                         format fixes that alphabet. Rename the key, or give that profile a \
                         name of its own."
                    ),
                    name: dotted,
                    source: LOCAL_FILE.to_owned(),
                });
            }
            // A parent holding nothing but sub-tables declares nothing, and the built-in of
            // its name stays. One with keys of its own is validated WITH its nested tables:
            // once parsed, `[harness.claude.alt]` and a typo'd `enviroment = { … }` are the
            // same thing, and review 13 needs the typo to refuse the profile rather than
            // drop `CLAUDE_CONFIG_DIR` in silence.
            if !nested.is_empty() && nested.len() == inner.len() {
                continue;
            }
        }
        match refusal(name, table) {
            Some(reason) => {
                // A declared profile that replaces a built-in and is refused takes the name
                // down with it (ruling 37): the operator said how that name runs, and the
                // built-in standing in would run the command they replaced.
                set.take(name);
                set.refused.push(Refused {
                    name: shown::short(name),
                    source: LOCAL_FILE.to_owned(),
                    reason,
                });
            }
            None => {
                let inner = table.as_table().expect("a profile that passed is a table");
                let kind = kind_of(inner["kind"].as_str().expect("a kind that passed is text"))
                    .expect("a kind that passed is one charter knows");
                let mut env: Vec<(String, String)> = inner
                    .get("env")
                    .and_then(toml::Value::as_table)
                    .map(|table| {
                        table
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                            .collect()
                    })
                    .unwrap_or_default();
                env.sort();
                set.put(Profile {
                    name: name.clone(),
                    kind: kind.word.to_owned(),
                    harness: kind.registry.to_owned(),
                    command: inner["command"]
                        .as_array()
                        .expect("a command that passed is an array")
                        .iter()
                        .map(|w| w.as_str().unwrap_or_default().to_owned())
                        .collect(),
                    env,
                    source: Source::Local,
                });
            }
        }
    }
    settle(set)
}

/// Every profile this plane has **and may be launched**: [`current`] with [`ignore_check`]
/// already applied, and the check itself for a caller that also prints its fix.
///
/// The one function a launch surface calls. [`current`] alone is the unchecked read, and
/// pairing it with the git check by hand is a pairing one caller will forget — the thing it
/// would let through is a command out of a file every clone of this plane carries.
pub fn for_launch(root: &Path) -> (ProfileSet, IgnoreCheck) {
    let check = ignore_check(root);
    (with_ignore_check(current(root), &check), check)
}

/// Every profile this plane has, with the two rules that need charter's own command surface:
/// a name `charter <name>` already means, and a command that runs charter itself.
///
/// **This is the unchecked read.** It does not ask git whether `charter.local.toml` would
/// reach every clone of this plane, so nothing that decides a LAUNCH may call it — use
/// [`for_launch`].
pub fn current(root: &Path) -> ProfileSet {
    current_of(derive(root))
}

/// [`current`]'s two rules, applied to a set [`derive`] or [`derive_from`] made.
pub fn current_of(derived: ProfileSet) -> ProfileSet {
    let mut set = derived;
    for profile in set.profiles.clone() {
        let name = shown::short(&profile.name);
        let reason = if COMMAND_WORDS.contains(&profile.name.as_str()) {
            format!(
                "profile '{name}' is named like the command `charter {name}`, and that name \
                 belongs to the command. Rename the table."
            )
        } else if is_charter(&profile.command) {
            format!(
                "profile '{name}' runs charter itself — a profile names the harness a chat \
                 runs, and charter is not a harness. Give it the harness's own command."
            )
        } else {
            continue;
        };
        set.take(&profile.name);
        set.refused.push(Refused {
            name,
            source: profile.source.as_str().to_owned(),
            reason,
        });
    }
    settle(set)
}

/// `set` with every profile the local file declares refused, when `check` says git would
/// carry that file.
///
/// Each of those three refusals says "the profiles in it are refused", so a surface that
/// asked must show them refused — not as ordinary rows with a warning under them. Takes the
/// check rather than running it, so a caller that also prints the fix asks git once.
pub fn with_ignore_check(mut set: ProfileSet, check: &IgnoreCheck) -> ProfileSet {
    if check.passes() {
        return set;
    }
    let moved: Vec<Refused> = set
        .profiles
        .iter()
        .filter(|p| p.source == Source::Local)
        .map(|p| Refused {
            name: shown::short(&p.name),
            source: LOCAL_FILE.to_owned(),
            reason: check.reason.clone(),
        })
        .collect();
    set.profiles.retain(|p| p.source != Source::Local);
    set.refused.extend(moved);
    settle(set)
}

/// A default that names no profile left in the set is refused by value.
fn settle(mut set: ProfileSet) -> ProfileSet {
    if set.default_from.is_none() {
        return set;
    }
    match set.default.take() {
        // The RESULT's own name, never the text the file supplied — the rule charter keeps
        // for a value that ends up on a command line.
        Some(wanted) if set.get(&wanted).is_some() => {
            set.default = set.get(&wanted).map(|p| p.name.clone());
            // The two are mutually exclusive, as they are in Python: a default that stands
            // clears any earlier refusal of one. Without this line a `default` refused in
            // `charter.toml` survived beside a good one from the local file, and the first
            // surface to print it would tell an operator their working default named
            // nothing. Found in review.
            set.default_refused = None;
        }
        Some(wanted) => set.default_refused = Some(shown::short(&wanted)),
        None => {}
    }
    set
}

/// Whether this command is charter itself, including `python3 -m charter`.
///
/// Case-folded for the reason the Python guard is: on a case-insensitive filesystem
/// `CHARTER` runs the same binary, and a guard that matches one casing has a Shift key for
/// a bypass.
fn is_charter(command: &[String]) -> bool {
    let Some(program) = command.first() else {
        return false;
    };
    let base = Path::new(program)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if CHARTER_PROGS.contains(&base.as_str()) {
        return true;
    }
    if !base.starts_with("python") {
        return false;
    }
    command
        .iter()
        .skip(1)
        .position(|arg| arg == "-m")
        .and_then(|i| command.get(i + 2))
        .is_some_and(|module| CHARTER_PROGS.contains(&module.to_lowercase().as_str()))
}

/// Why the profile `name` is refused, or `None`. The FIRST failure wins, in the order the
/// rules are written, so one profile gets one sentence.
fn refusal(name: &str, table: &toml::Value) -> Option<String> {
    let shown_name = shown::short(name);
    let Some(inner) = table.as_table() else {
        return Some(format!(
            "[harness] {shown_name} in charter.local.toml is not a table — a profile is \
             [harness.{shown_name}] with kind, command and optionally env."
        ));
    };
    if !name_ok(name) {
        return Some(format!(
            "profile '{shown_name}' is not a name charter accepts — letters, digits, '_' \
             and '-', starting with a letter or digit, and no dot — the plane format fixes \
             that alphabet. Rename the table."
        ));
    }
    if name == DEFAULT {
        return Some(
            "a profile cannot be named 'default' — `default` is the one key under [harness] \
             that is not a profile. Rename the table."
                .to_owned(),
        );
    }
    let kind = inner.get("kind");
    let word = kind.and_then(toml::Value::as_str).unwrap_or_default();
    let Some(kind_found) = kind_of(word) else {
        let kinds: Vec<&str> = KINDS.iter().map(|k| k.word).collect();
        return Some(format!(
            "profile '{shown_name}' has kind {}, which is not a harness charter can launch \
             — one of: {}. Set kind to one of them.",
            shown::short(&kind.map(py_str).unwrap_or_default()),
            kinds.join(", ")
        ));
    };
    let command = inner.get("command").and_then(toml::Value::as_array);
    let usable = command.is_some_and(|words| {
        !words.is_empty()
            && words
                .iter()
                .all(|w| w.as_str().is_some_and(|w| !w.is_empty()))
    });
    if !usable {
        return Some(format!(
            "profile '{shown_name}' has no usable command — command is a list of arguments, \
             [\"claude\"], never a shell string, because no shell runs it. Write it as a list."
        ));
    }
    let env = inner.get("env");
    let names: Vec<&String> = match env {
        None => Vec::new(),
        Some(toml::Value::Table(table)) if table.values().all(toml::Value::is_str) => {
            let mut names: Vec<&String> = table.keys().collect();
            names.sort();
            names
        }
        Some(_) => {
            return Some(format!(
                "profile '{shown_name}' has an env that is not a table of text values — \
                 write env = {{ NAME = \"value\" }}."
            ));
        }
    };
    for var in &names {
        if var.to_uppercase().starts_with(CHARTER_PREFIX) {
            return Some(format!(
                "profile '{shown_name}' sets {}, one of charter's own variables — charter \
                 sets those itself, and a profile's value would tell every hook the wrong \
                 harness or plane. Remove it.",
                shown::short(var)
            ));
        }
    }
    for var in &names {
        let upper = var.to_uppercase();
        if SECRET_WORDS.iter().any(|word| upper.contains(word)) {
            return Some(format!(
                "profile '{shown_name}' sets {}, which is named like a credential — charter \
                 holds no credential in a profile, because anything set on the harness \
                 reaches the model's own shell. Log in inside that harness instead: {}.",
                shown::short(var),
                kind_found.login
            ));
        }
    }
    for key in inner.keys() {
        if !PROFILE_KEYS.contains(&key.as_str()) {
            return Some(format!(
                "profile '{shown_name}' has {}, which charter does not read — a profile is \
                 kind, command and env. Remove it.",
                shown::short(key)
            ));
        }
    }
    None
}

/// `^[A-Za-z0-9][A-Za-z0-9_-]*$`. No dot: a dot in a name broke tmux targets in charter
/// #695, and a profile's name reaches the same places.
fn name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A TOML value as Python's `str()` renders it, for the one place a refusal repeats a value
/// back that need not be text — [`crate::pyrepr::str_toml`].
///
/// Every spelling, not just the easy ones. A review measured three that TOML and Python
/// disagree about — `true`/`True`, `["claude"]`/`['claude']`, `{ a = 1 }`/`{'a': 1}` — and
/// a refusal that quotes a value back differently is the second answer this port exists to
/// prevent, however wrong the value being quoted.
///
/// **The body is in `pyrepr` now, because this module and `scaffold::planefile` each had
/// one.** The two agreed on those three spellings and parted on a fourth: this one handed a
/// float to `toml`'s own `Display` and wrote `1e300`, where Python's `repr` — and the other
/// port — write `1e+300`.
pub(crate) fn py_str(value: &toml::Value) -> String {
    crate::pyrepr::str_toml(value)
}

/// A value as Python `repr`s it INSIDE a container, where a string gains quotes.
pub(crate) fn py_repr(value: &toml::Value) -> String {
    crate::pyrepr::repr_toml(value)
}

/// `p`'s command with a leading `~` expanded in its first word ONLY — the program. No shell
/// runs it, so nothing else would; an argument is the harness's to interpret.
pub fn expanded_command(p: &Profile, home: &Path) -> Vec<String> {
    let mut argv = p.command.clone();
    if let Some(program) = argv.first_mut() {
        *program = expand_tilde(program, home);
    }
    argv
}

/// `p`'s environment with a leading `~` expanded in every value, which is where a config
/// folder is named and no shell is there to do it.
pub fn expanded_env(p: &Profile, home: &Path) -> BTreeMap<String, String> {
    p.env
        .iter()
        .map(|(name, value)| (name.clone(), expand_tilde(value, home)))
        .collect()
}

/// The operator's home, or `None` where the environment names none — in which case a `~`
/// stays as it was written, as `os.path.expanduser` leaves it.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

fn expand_tilde(value: &str, home: &Path) -> String {
    if value == "~" {
        return home.display().to_string();
    }
    match value.strip_prefix("~/") {
        // `~other/...` is another user's home, which charter does not guess.
        Some(rest) => home.join(rest).display().to_string(),
        None => value.to_owned(),
    }
}

/// `p` as one line a person reads: `NAME=value` for each variable, then the command.
///
/// Each piece through [`shown::readable`], so a control byte is shown escaped and never
/// interpreted (ruling 35) — the file is one a chat can write, and the selector draws this.
pub fn display(p: &Profile) -> String {
    let mut pieces: Vec<String> = p
        .env
        .iter()
        .map(|(name, value)| shown::short(&format!("{name}={value}")))
        .collect();
    // A profile that passed validation has a non-empty command, but this is `pub` and the
    // listing runs it over whatever it is handed — and indexing `command[0]` was an
    // undocumented precondition that aborted `charter harness list` when it did not hold.
    // Found in review. An empty command shows as nothing rather than ending the process.
    let mut words: Vec<String> = p
        .command
        .first()
        .map(|w| program_word(w))
        .into_iter()
        .collect();
    words.extend(p.command.iter().skip(1).map(|w| quote(w)));
    // Nothing rather than the blank marker: `A=b ""` reads as a command that IS two quote
    // characters, and a profile with no command at all is one validation already refused —
    // this path exists so showing one answers instead of ending the process.
    if !words.is_empty() {
        pieces.push(shown::short(&words.join(" ")));
    }
    pieces.join(" ")
}

/// The command's first word as a shell would need it typed to reach the same program.
///
/// [`quote`] puts a leading `~` inside quotes, where a shell does not expand it, and charter
/// DOES expand it — so `'~/.local/bin/codex'` pasted into a terminal names a directory
/// called `~` (charter #1004's proof run). The `~` and its slash stay bare and the rest is
/// quoted. `~other/…` is another user's home and stays quoted whole: the line shows what
/// charter was given rather than guessing which home a shell would pick.
fn program_word(word: &str) -> String {
    if word == "~" {
        return word.to_owned();
    }
    match word.strip_prefix("~/") {
        Some("") => word.to_owned(),
        Some(rest) => format!("~/{}", quote(rest)),
        None => quote(word),
    }
}

/// `shlex.quote`: the word as a shell would need it typed. Its safe set is Python's own,
/// which is ASCII — so a non-ASCII word is quoted, as it is there.
fn quote(word: &str) -> String {
    const SAFE: &str = "%+,-./:=@_";
    if word.is_empty() {
        return "''".to_owned();
    }
    if word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || SAFE.contains(c))
    {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', "'\"'\"'"))
}

/// How long [`ignore_check`] waits for its one `git status`. A constant rather than a knob:
/// a knob nobody turns is a second place the answer could come from. The one other deadline
/// is a test's, through [`ignore_check_within`], and nothing but a test passes one.
///
/// **Thirty seconds, not five.** An unknown here is a refusal — every declared profile stops
/// being startable — so this deadline decides whether an operator can open a chat at all. Five
/// seconds was enough when the only thing racing it was git; it is not enough on a machine
/// running dozens of harnesses, where a cold `status` routinely takes longer, and the operator
/// would be told charter could not say whether their own file is ignored. Measured on a cold
/// macOS CI runner blowing the same five seconds on a three-file repository
/// (charter-app run 35462478106). The deadline still exists so a hung git cannot stop the
/// picker for ever; it no longer turns a slow answer into a refusal.
const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The single-character status codes that mean git is tracking the path.
const TRACKED_STATUS: &str = " MTADRCU";

/// Whether git would carry `root`'s local file, as a refusal and the fix for that state.
///
/// A pass when the file is absent (it declares nothing), when the plane is not a git
/// repository (nothing to commit to), or when git ignores the file and tracks it not.
/// Otherwise one of three refusals, each with its OWN fix — `charter reinit` adds the ignore
/// line, and an ignore rule does not apply to a path git already tracks.
///
/// The only function here that runs git, and never on a config read: one `git status` where
/// a person asked.
pub fn ignore_check(root: &Path) -> IgnoreCheck {
    ignore_check_with(root, Path::new("git"))
}

/// [`ignore_check`] against a named `git`, which is how a test drives the answer git cannot
/// give.
pub fn ignore_check_with(root: &Path, git: &Path) -> IgnoreCheck {
    ignore_check_within(root, git, GIT_TIMEOUT)
}

/// [`ignore_check_with`] waiting `timeout` for git instead of [`GIT_TIMEOUT`].
///
/// For the test of a git that never answers, and for nothing else. Waiting out the real
/// thirty seconds made that one test half of `cargo test -p charter-core`'s wall clock —
/// and the nightly mutation run pays the whole suite once per mutant, six thousand times.
pub fn ignore_check_within(root: &Path, git: &Path, timeout: std::time::Duration) -> IgnoreCheck {
    if !root.join(LOCAL_FILE).exists() {
        return IgnoreCheck::default();
    }
    check_of(git_path_state(root, git, timeout))
}

/// Whether git would carry the local file **once it is written** — [`ignore_check`] for a
/// writer, which has to ask before the file exists (charter-app#252).
///
/// [`ignore_check`] passes an absent file, because an absent file declares nothing. A writer
/// cannot take that pass: the file it is about to create would be committed by the next
/// `git add -A` if nothing ignores it. So where there is no file yet this asks git whether the
/// path is IGNORED (`git check-ignore`, which answers for a path that does not exist and counts
/// a tracked path as not ignored), and says it in [`ignore_check`]'s own sentences.
pub fn ignore_check_before_writing(root: &Path) -> IgnoreCheck {
    ignore_check_before_writing_within(root, Path::new("git"), GIT_TIMEOUT)
}

/// [`ignore_check_before_writing`] against a named `git`, waiting `timeout` for it.
pub fn ignore_check_before_writing_within(
    root: &Path,
    git: &Path,
    timeout: std::time::Duration,
) -> IgnoreCheck {
    if root.join(LOCAL_FILE).exists() {
        return ignore_check_within(root, git, timeout);
    }
    check_of(git_ignore_state(root, git, timeout))
}

/// The refusal, and its fix, for what git said about the local file.
fn check_of(state: GitState) -> IgnoreCheck {
    match state {
        GitState::Tracked => IgnoreCheck {
            reason: "git tracks charter.local.toml, so the profiles in it would reach every \
                     clone of this plane — charter refuses them until it is untracked: git \
                     rm --cached charter.local.toml, commit that removal, then charter \
                     reinit."
                .to_owned(),
            fix: "git rm --cached charter.local.toml, commit that removal, then charter \
                  reinit"
                .to_owned(),
        },
        GitState::Committable => IgnoreCheck {
            reason: "git would commit charter.local.toml, so the profiles in it are refused \
                     until it is ignored — charter reinit adds /charter.local.toml to \
                     .gitignore."
                .to_owned(),
            fix: "charter reinit".to_owned(),
        },
        GitState::Unknown(why) => {
            let why = shown::short(&why);
            IgnoreCheck {
                reason: format!(
                    "git could not say whether charter.local.toml is ignored ({why}), so the \
                     profiles in it are refused — an unknown is not a pass. Run git status \
                     --ignored -- charter.local.toml in the plane to see what git says."
                ),
                fix: format!(
                    "run git status --ignored -- charter.local.toml in the plane by hand; \
                     git said: {why}"
                ),
            }
        }
        GitState::NotARepo | GitState::Ignored => IgnoreCheck::default(),
    }
}

enum GitState {
    NotARepo,
    Tracked,
    Ignored,
    Committable,
    Unknown(String),
}

/// What git says about the local file, from ONE `git status` ([`git_answer`] says why its
/// flags). `--untracked-files=all` overrides an operator's `status.showUntrackedFiles=no`,
/// which would otherwise hide `??`.
fn git_path_state(root: &Path, git: &Path, timeout: std::time::Duration) -> GitState {
    let out = match git_answer(
        root,
        git,
        &[
            "status",
            "--porcelain=v1",
            "--ignored=matching",
            "--untracked-files=all",
            "--",
            LOCAL_FILE,
        ],
        timeout,
    ) {
        Ok(out) => out,
        Err(state) => return state,
    };
    let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    if !out.status.success() {
        return failed(&out, &said);
    }
    // What the porcelain lines say, kept apart from the state they decide: a tracked line
    // anywhere wins, because the next commit carries the file whatever else is printed.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let (mut tracked, mut untracked) = (false, false);
    for line in &lines {
        let code: String = line.chars().take(2).collect();
        if code == "??" {
            untracked = true;
        } else if code == "!!" {
            continue;
        } else if !code.is_empty() && code.chars().all(|c| TRACKED_STATUS.contains(c)) {
            tracked = true;
        } else {
            return GitState::Unknown(format!("git status printed {line:?}"));
        }
    }
    if tracked || lines.is_empty() {
        GitState::Tracked
    } else if untracked {
        GitState::Committable
    } else {
        GitState::Ignored
    }
}

/// What git says about a local file that does not exist yet, from ONE `git check-ignore`:
/// exit 0 is ignored, exit 1 is not — which is also its answer for a path git tracks.
fn git_ignore_state(root: &Path, git: &Path, timeout: std::time::Duration) -> GitState {
    let out = match git_answer(
        root,
        git,
        &["check-ignore", "-q", "--", LOCAL_FILE],
        timeout,
    ) {
        Ok(out) => out,
        Err(state) => return state,
    };
    match out.status.code() {
        Some(0) => GitState::Ignored,
        Some(1) => GitState::Committable,
        _ => failed(&out, String::from_utf8_lossy(&out.stderr).trim()),
    }
}

/// A git that exited non-zero: not a repository, or an unknown with git's first line.
fn failed(out: &std::process::Output, said: &str) -> GitState {
    if out.status.code() == Some(128) && said.contains("not a git repository") {
        return GitState::NotARepo;
    }
    GitState::Unknown(match said.lines().next() {
        Some(line) => line.to_owned(),
        None => format!("git exited {:?}", out.status.code()),
    })
}

/// One git command in `root`, bounded by `timeout`, with its output — or the unknown it came
/// to. `--no-optional-locks` because a plain `status` refreshes the index and takes
/// `index.lock` when it can, which broke a concurrent `charter save` (charter #917). `LC_ALL=C`
/// because "not a git repository" is git's own sentence and a translated git says it in
/// another language.
fn git_answer(
    root: &Path,
    git: &Path,
    args: &[&str],
    timeout: std::time::Duration,
) -> Result<std::process::Output, GitState> {
    let mut child = match crate::forklock::spawn(
        std::process::Command::new(git)
            .args(["--no-optional-locks", "-C", &root.display().to_string()])
            .args(args)
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    ) {
        Ok(child) => child,
        Err(e) => return Err(GitState::Unknown(e.to_string())),
    };
    // A timeout is an unknown, not a pass, so the wait is bounded and the child is ended.
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            // `<` and `<=` differ only at the one instant that equals the deadline, which no
            // test can land on: `.cargo/mutants.toml` excludes that mutant as equivalent.
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitState::Unknown(format!(
                    "git did not answer within {} seconds",
                    timeout.as_secs()
                )));
            }
            Err(e) => return Err(GitState::Unknown(e.to_string())),
        }
    }
    child
        .wait_with_output()
        .map_err(|e| GitState::Unknown(e.to_string()))
}
