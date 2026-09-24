//! Which of a harness's own plugins a project has on (charter-app#274, ADR 0050).
//!
//! **Harness-neutral, with one adapter per harness.** A [`Plugin`] is what one harness has
//! installed on this machine, by the id that harness itself uses. A project says, per harness and
//! per plugin, on or off, in `[harness_plugins.<harness>]` of `charter.toml` (Shared) and
//! `charter.local.toml` (Local). An [`Adapter`] knows one harness: what it has installed, whether
//! it can take a set of plugins for one chat, and which plugins charter fixes whatever a file says.
//!
//! # The precedence, and where it comes from
//!
//! #272's ([`crate::extension::project`], ADR 0048), per key: **Local, then the workspace, then
//! Shared**. A chat in a workspace also reads `settings.harness_plugins.<harness>` in that
//! workspace's `workspace.json` (charter-app#282, [`Choices::read_in`]), the layer between the
//! project's two files, through the same reader. With no layer naming a plugin it is **not
//! set**. Not set means the harness decides the way it always did, from its own user and project
//! settings. This is the one place it differs from an extension, which is on by default. A
//! harness plugin is not charter's to turn on: it is the operator's own install, and charter says
//! nothing about it until a project does.
//!
//! **What this machine has installed comes first**, the way this machine's approval comes first
//! for an extension. A file that names a plugin this machine does not have is listed as such and
//! handed to nothing. A file can choose among installed plugins; it cannot install one.
//!
//! **Pins come before everything** ([`Adapter::pinned`]). Claude Code's are the two the app has
//! always written into every chat's `--settings`. `charter-app@inline` is always on, because it
//! carries the hooks and the Bash guard, and a file a chat can write must not be able to switch
//! the guard off. `charter@charter` is always off, by the operator's ruling of 2026-09-23. A file
//! that says otherwise is refused by the settings tab's save ([`refusals`]) and, if it gets there
//! another way, ignored with a sentence ([`Effective::ignored`]).
//!
//! # A harness whose adapter cannot apply
//!
//! It still lists what it has installed, and says [`not_supported`]: "plugins for <harness> are
//! not supported yet", with the measured reason. A chat on it is handed nothing, and every choice
//! a file makes for it is said to be ignored. It is never left out of the list.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::extension::project::{Source, toml_key};
use crate::profiles::{COMMITTED_FILE, LOCAL_FILE};

/// The table both files hold choices in: `[harness_plugins.<harness>]`, `"<id>" = true|false`.
pub const TABLE: &str = "harness_plugins";

/// What one chat is handed: each plugin on (`true`) or off (`false`), by the harness's own id.
/// A plugin it does not name is left to the harness.
pub type Chosen = BTreeMap<String, bool>;

/// One plugin a harness has installed on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    /// The harness's own id for it, which is what a file names and what the harness is handed.
    pub id: String,
    /// The plane's word for the harness: a profile's `kind`.
    pub harness: &'static str,
    /// What a person calls it.
    pub name: String,
    /// Where charter read it from, in a few words.
    pub source: String,
}

/// Whether an adapter can hand one chat a set of plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// For one chat alone, with nothing written anywhere.
    PerChat,
    /// Not yet, and the measured reason, as the end of a sentence.
    NotYet(&'static str),
}

/// A plugin charter fixes whatever a project file says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    pub id: &'static str,
    pub on: bool,
    /// Why, as a clause that starts with the id: "<id> is always on: …".
    pub why: &'static str,
}

/// Where an adapter reads the harness's own files from: the chat's environment first, because
/// a profile may point a harness at another account's directory (`CLAUDE_CONFIG_DIR`,
/// `CODEX_HOME`), then this process's, then the home directory.
#[derive(Debug, Clone)]
pub struct Env<'a> {
    /// The chat's own variables, as the profile and charter set them.
    pub chat: &'a [(String, String)],
    pub home: Option<PathBuf>,
    /// Whether a variable the chat does not set is looked up in this process. Off in a test,
    /// so the machine running it cannot reach the answer.
    pub process: bool,
}

impl<'a> Env<'a> {
    /// A chat's environment, over this process's.
    pub fn of(chat: &'a [(String, String)]) -> Self {
        Self {
            chat,
            home: crate::profiles::home(),
            process: true,
        }
    }

    fn var(&self, name: &str) -> Option<String> {
        self.chat
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .or_else(|| self.process.then(|| std::env::var(name).ok()).flatten())
            .filter(|value| !value.is_empty())
    }

    /// `name`'s directory, or `fallback` under the home directory.
    fn dir(&self, name: &str, fallback: &str) -> Option<PathBuf> {
        self.var(name)
            .map(PathBuf::from)
            .or_else(|| self.home.as_ref().map(|home| home.join(fallback)))
    }
}

/// One harness, as far as its plugins go.
pub trait Adapter: Sync {
    /// The plane's word for it: a profile's `kind`, and the key under [`TABLE`].
    fn harness(&self) -> &'static str;
    /// What a person calls it.
    fn title(&self) -> &'static str;
    /// Where it records what it has installed, under `env`: what the settings tab names, so a
    /// listing is never mistaken for another account's.
    fn record(&self, env: &Env<'_>) -> Option<PathBuf>;
    /// Everything it has installed on this machine, read from [`Self::record`] and never
    /// written, or why it could not be read.
    fn installed(&self, env: &Env<'_>) -> Result<Vec<Plugin>, String>;
    fn support(&self) -> Support;
    /// What charter fixes for it, whatever a file says.
    fn pinned(&self) -> &'static [Pin] {
        &[]
    }
}

/// Every harness charter knows, in the registry's order ([`crate::profiles::KINDS`]).
pub static ADAPTERS: [&dyn Adapter; 3] = [&CLAUDE_CODE, &OPENCODE, &CODEX];

/// The adapter for the harness `kind` names.
pub fn adapter(kind: &str) -> Option<&'static dyn Adapter> {
    ADAPTERS.iter().copied().find(|it| it.harness() == kind)
}

/// "plugins for <harness> are not supported yet — <why>", or none for an adapter that applies.
pub fn not_supported(adapter: &dyn Adapter) -> Option<String> {
    match adapter.support() {
        Support::PerChat => None,
        Support::NotYet(why) => Some(format!(
            "plugins for {} are not supported yet — {why}",
            adapter.title()
        )),
    }
}

// ------------------------------------------------------------------------------------------
// Claude Code
// ------------------------------------------------------------------------------------------

/// Claude Code: `installed_plugins.json`, and `enabledPlugins` in the chat's `--settings`.
pub struct ClaudeCode;

pub static CLAUDE_CODE: ClaudeCode = ClaudeCode;

/// Claude Code's pins: the two values every chat the app starts has always carried.
static CLAUDE_PINS: [Pin; 2] = [
    Pin {
        id: crate::plugin::LOADED_AS,
        on: true,
        why: "charter-app@inline is always on: it is charter's own plugin, and it carries \
              charter's hooks and the Bash guard",
    },
    Pin {
        id: crate::plugin::SUPERSEDED,
        on: false,
        why: "charter@charter is always off: it is the Python charter's plugin, and a chat \
              the app starts carrying it too would have two sets of hooks and two handoff skills",
    },
];

impl Adapter for ClaudeCode {
    fn harness(&self) -> &'static str {
        "claude"
    }

    fn title(&self) -> &'static str {
        "Claude Code"
    }

    /// `plugins/installed_plugins.json` under the chat's `CLAUDE_CONFIG_DIR`, else `~/.claude`:
    /// the record `claude plugin list` reads. Measured on the operator's 2.1.x install: version
    /// 2, `plugins` keyed by `<name>@<marketplace>`, each a list of installs with a `scope`
    /// (`user`, `project`, `local`). One plugin installed in several scopes is one plugin here.
    fn record(&self, env: &Env<'_>) -> Option<PathBuf> {
        env.dir("CLAUDE_CONFIG_DIR", ".claude")
            .map(|dir| dir.join("plugins").join("installed_plugins.json"))
    }

    fn installed(&self, env: &Env<'_>) -> Result<Vec<Plugin>, String> {
        let Some(path) = self.record(env) else {
            return Ok(Vec::new());
        };
        let Some(text) = read(&path)? else {
            return Ok(Vec::new());
        };
        let doc: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("{} is not JSON charter can read: {err}", path.display()))?;
        let Some(plugins) = doc.get("plugins").and_then(serde_json::Value::as_object) else {
            return Err(format!("{} holds no plugins table", path.display()));
        };
        let mut out: Vec<Plugin> = plugins
            .iter()
            .filter(|(id, _)| id_ok(id))
            .map(|(id, installs)| {
                let mut scopes: Vec<&str> = Vec::new();
                for scope in installs
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|one| one.get("scope").and_then(serde_json::Value::as_str))
                {
                    if !scopes.contains(&scope) {
                        scopes.push(scope);
                    }
                }
                let (name, marketplace) = split(id);
                let mut source = vec![marketplace];
                source.extend(scopes);
                Plugin {
                    id: id.clone(),
                    harness: self.harness(),
                    name: name.to_owned(),
                    source: source.join(", "),
                }
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    fn support(&self) -> Support {
        Support::PerChat
    }

    fn pinned(&self) -> &'static [Pin] {
        &CLAUDE_PINS
    }
}

// ------------------------------------------------------------------------------------------
// Codex
// ------------------------------------------------------------------------------------------

/// Codex: `[plugins."<id>"]` in its `config.toml`.
pub struct Codex;

pub static CODEX: Codex = Codex;

impl Adapter for Codex {
    fn harness(&self) -> &'static str {
        "codex"
    }

    fn title(&self) -> &'static str {
        "Codex"
    }

    /// `[plugins."<name>@<marketplace>"]` in `$CODEX_HOME/config.toml`, else `~/.codex`, which
    /// is where Codex records an installed plugin and whether it is on (its plugin docs, and the
    /// operator's 0.147.0 install).
    fn record(&self, env: &Env<'_>) -> Option<PathBuf> {
        env.dir("CODEX_HOME", ".codex")
            .map(|dir| dir.join("config.toml"))
    }

    fn installed(&self, env: &Env<'_>) -> Result<Vec<Plugin>, String> {
        let Some(path) = self.record(env) else {
            return Ok(Vec::new());
        };
        let Some(text) = read(&path)? else {
            return Ok(Vec::new());
        };
        let doc: toml::Table = text
            .parse()
            .map_err(|err| format!("{} is not TOML charter can read: {err}", path.display()))?;
        let mut out: Vec<Plugin> = doc
            .get("plugins")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flatten()
            .filter(|(id, _)| id_ok(id))
            .map(|(id, table)| {
                let on = table
                    .get("enabled")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(true);
                let (name, marketplace) = split(id);
                Plugin {
                    id: id.clone(),
                    harness: self.harness(),
                    name: name.to_owned(),
                    source: format!(
                        "{marketplace}, {} in config.toml",
                        if on { "on" } else { "off" }
                    ),
                }
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// Measured on codex-cli 0.147.0 (charter-app#274), in a throwaway `CODEX_HOME` holding a
    /// copy of a real config and plugin cache, with `codex debug prompt-input` showing which of
    /// the plugin's skills reached the model. `enabled = false` in `config.toml` took them out.
    /// `-c` did nothing in any shape its parser takes: not `plugins.<id>.enabled=false`, not
    /// `plugins.<id>={enabled=false}`, not `plugins={"<id>"={enabled=false}}`, and not the
    /// other way round over a file that says false. A per-session switch would have to be one
    /// of those. The only other switch is `--disable plugins`, which turns them all off.
    fn support(&self) -> Support {
        Support::NotYet(
            "Codex 0.147.0 turns a plugin on or off only in its own config.toml, and ignores \
             the same key given for one session with -c (measured), and charter never writes \
             Codex's config",
        )
    }
}

// ------------------------------------------------------------------------------------------
// opencode
// ------------------------------------------------------------------------------------------

/// opencode: its config's `plugin` list, and its plugin directories.
pub struct Opencode;

pub static OPENCODE: Opencode = Opencode;

/// The files opencode loads from a plugin directory.
const OPENCODE_SCRIPTS: [&str; 4] = ["js", "ts", "mjs", "mts"];

impl Adapter for Opencode {
    fn harness(&self) -> &'static str {
        "opencode"
    }

    fn title(&self) -> &'static str {
        "opencode"
    }

    /// Its global configuration directory, `$XDG_CONFIG_HOME/opencode`, else
    /// `~/.config/opencode`: the npm packages in `opencode.json`'s `plugin` list, and every
    /// script in `plugin/` and `plugins/` (opencode's plugin docs name `plugins/`; the operator's
    /// install has `plugin/`). `opencode.jsonc` is not read: charter has no JSONC reader, and a
    /// plugin listed only there is not listed here.
    fn record(&self, env: &Env<'_>) -> Option<PathBuf> {
        env.var("XDG_CONFIG_HOME")
            .map(|base| PathBuf::from(base).join("opencode"))
            .or_else(|| env.home.as_ref().map(|home| home.join(".config/opencode")))
    }

    fn installed(&self, env: &Env<'_>) -> Result<Vec<Plugin>, String> {
        let Some(dir) = self.record(env) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        let config = dir.join("opencode.json");
        if let Some(text) = read(&config)? {
            let doc: serde_json::Value = serde_json::from_str(&text).map_err(|err| {
                format!("{} is not JSON charter can read: {err}", config.display())
            })?;
            let mut npm: Vec<&str> = doc
                .get("plugin")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .filter(|id| id_ok(id))
                .collect();
            npm.sort_unstable();
            out.extend(npm.into_iter().map(|id| Plugin {
                id: id.to_owned(),
                harness: self.harness(),
                name: id.to_owned(),
                source: "npm, in opencode.json".to_owned(),
            }));
        }
        for folder in ["plugin", "plugins"] {
            let Ok(entries) = std::fs::read_dir(dir.join(folder)) else {
                continue;
            };
            let mut files: Vec<String> = entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| {
                    Path::new(name)
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| OPENCODE_SCRIPTS.contains(&ext))
                })
                .collect();
            files.sort();
            out.extend(files.into_iter().map(|file| Plugin {
                id: format!("{folder}/{file}"),
                harness: self.harness(),
                name: file,
                source: "a file in the plugin directory".to_owned(),
            }));
        }
        Ok(out)
    }

    fn support(&self) -> Support {
        Support::NotYet(
            "charter does not start opencode chats yet, and opencode has no switch that turns \
             one plugin off: it loads every plugin from every config and plugin directory",
        )
    }
}

/// A file's text, `None` when it is not there, or why it could not be read.
fn read(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("charter could not read {}: {err}", path.display())),
    }
}

/// `<name>@<marketplace>` in two, the marketplace empty when there is none.
fn split(id: &str) -> (&str, &str) {
    id.rsplit_once('@').unwrap_or((id, ""))
}

/// The longest id charter reads or hands on.
const MOST_ID_BYTES: usize = 200;

/// Whether `id` could be a plugin's id: one line, short, and nothing in it draws as nothing.
pub fn id_ok(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= MOST_ID_BYTES && !id.contains(crate::panel::undrawable)
}

// ------------------------------------------------------------------------------------------
// What a project's files say, and the precedence
// ------------------------------------------------------------------------------------------

/// What one layer says: by harness, then by plugin id.
type Said = BTreeMap<String, BTreeMap<String, bool>>;

/// What a project's two files say, by harness and then by plugin id — and, in a workspace, what
/// that workspace's `workspace.json` says (charter-app#282), the layer between them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choices {
    shared: Said,
    /// Empty outside a workspace, and for a workspace whose manifest says nothing.
    workspace: Said,
    local: Said,
    /// The workspace these were read in, when they were.
    workspace_name: Option<String>,
    /// Why `charter.local.toml` is not among these layers, when it is there and git would carry
    /// it (charter-app#319).
    local_left_out: Option<String>,
}

impl Choices {
    /// The two files' text: `None` for a file that is not there.
    pub fn from_text(shared: Option<&str>, local: Option<&str>) -> Self {
        let said = |text: &str| {
            text.parse::<toml::Table>()
                .map(|top| said_in(&top))
                .unwrap_or_default()
        };
        Self {
            shared: shared.map(said).unwrap_or_default(),
            local: local.map(said).unwrap_or_default(),
            ..Self::default()
        }
    }

    /// The same choices, in the workspace `name` whose `workspace.json` has this text (`None`:
    /// it has none). Its `settings.harness_plugins` go between Shared and Local. A manifest with
    /// none, or one that is not JSON, says nothing, which leaves the project's answer as it was.
    pub fn in_workspace(self, name: &str, manifest: Option<&str>) -> Self {
        self.with_workspace(
            name,
            manifest.and_then(crate::settings::workspace::table_in),
        )
    }

    fn with_workspace(mut self, name: &str, settings: Option<toml::Table>) -> Self {
        self.workspace = settings.as_ref().map(said_in).unwrap_or_default();
        self.workspace_name = Some(name.to_owned());
        self
    }

    /// The plane at `root`'s two files, as [`crate::settings::layer_text`] hands them: a file
    /// that cannot be read says nothing, and neither does a `charter.local.toml` git would carry
    /// (charter-app#308).
    pub fn read(root: &Path) -> Self {
        use crate::settings::{Which, layer_text};
        let local = layer_text(root, Which::Local);
        Self::from_text(layer_text(root, Which::Shared).text(), local.text())
            .with_local_left_out(local.left_out())
    }

    /// The same, told why `charter.local.toml` was left out of them — `None`: it was not.
    pub fn with_local_left_out(mut self, why: Option<&str>) -> Self {
        self.local_left_out = why.map(str::to_owned);
        self
    }

    /// Why `charter.local.toml` is not among the choices, when it is there and git would carry it:
    /// the ignore check's sentence, which the Project settings tab's Local section says too
    /// (charter-app#319). A settings tab says it in every group that shows these in force, so a
    /// value set in Local and not applied is never shown without its reason.
    pub fn local_left_out(&self) -> Option<&str> {
        self.local_left_out.as_deref()
    }

    /// [`Self::read`], in `workspace` when there is one — the same reader
    /// [`crate::extension::project::Choices::read_in`] is. A name that is not one of the plane's
    /// workspaces reads as a workspace with no settings.
    pub fn read_in(root: &Path, workspace: Option<&str>) -> Self {
        let project = Self::read(root);
        match workspace {
            None => project,
            Some(name) => {
                project.with_workspace(name, crate::settings::workspace::read(root, name))
            }
        }
    }

    /// The workspace's file as a sentence names it — `workspaces/<ws>/workspace.json` — or
    /// `None` outside a workspace.
    pub fn workspace_file(&self) -> Option<String> {
        self.workspace_name
            .as_deref()
            .map(crate::settings::workspace::named)
    }

    /// Whether any layer turns any of `harness`'s plugins on or off.
    fn names_any(&self, harness: &str) -> bool {
        [&self.shared, &self.workspace, &self.local]
            .iter()
            .any(|file| file.get(harness).is_some_and(|said| !said.is_empty()))
    }

    /// Every layer, the one with the last word first: its source, the file a sentence names,
    /// where the table sits in that file, and what it says for `harness`.
    fn layers(&self, harness: &str) -> [Layer<'_>; 3] {
        [
            Layer {
                source: Source::Local,
                file: LOCAL_FILE.to_owned(),
                at: "",
                said: self.local.get(harness),
            },
            Layer {
                source: Source::Workspace,
                file: self.workspace_file().unwrap_or_default(),
                at: WORKSPACE_AT,
                said: self.workspace.get(harness),
            },
            Layer {
                source: Source::Shared,
                file: COMMITTED_FILE.to_owned(),
                at: "",
                said: self.shared.get(harness),
            },
        ]
    }
}

/// Where `[harness_plugins]` sits in a workspace's `workspace.json`, as a key path names it.
const WORKSPACE_AT: &str = "settings.";

/// One layer of [`Choices`], for one harness.
struct Layer<'c> {
    source: Source,
    /// The file a sentence names.
    file: String,
    /// Where the table sits in that file.
    at: &'static str,
    said: Option<&'c BTreeMap<String, bool>>,
}

/// Every well-formed `"<id>" = true|false` under each `[harness_plugins.<harness>]` in `top` — a
/// whole TOML file, or a workspace's `settings` read as one. What is not well formed is left out
/// here and refused by [`refusals`].
fn said_in(top: &toml::Table) -> Said {
    top.get(TABLE)
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter_map(|(harness, plugins)| {
            let plugins = plugins
                .as_table()?
                .iter()
                .filter(|(id, _)| id_ok(id))
                .filter_map(|(id, on)| Some((id.clone(), on.as_bool()?)))
                .collect();
            Some((harness.clone(), plugins))
        })
        .collect()
}

/// A value a file set that charter did not use: which file, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    pub source: Source,
    pub why: String,
}

/// One plugin, in this project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub id: String,
    pub name: String,
    /// Where charter found it installed, or empty when it is not.
    pub origin: String,
    /// On, off, or `None` for not set: the harness decides.
    pub wanted: Option<bool>,
    /// Which layer decided [`Self::wanted`], or none (and for a pin, none).
    pub source: Source,
    /// Whether this machine has it installed for this harness.
    pub installed: bool,
    /// Why charter fixes it whatever a file says ([`Pin::why`]), or none for a plugin a project
    /// may choose.
    pub pinned: Option<&'static str>,
    pub ignored: Vec<Ignored>,
}

/// **The precedence, in one place**, for one harness: every plugin it has installed, every one
/// a project file names for it, and every pin, with what this project has it at and which file
/// decided that. In id order.
pub fn resolve(adapter: &dyn Adapter, installed: &[Plugin], choices: &Choices) -> Vec<Effective> {
    let harness = adapter.harness();
    let layers = choices.layers(harness);
    let mut ids: BTreeSet<&str> = installed.iter().map(|it| it.id.as_str()).collect();
    for said in layers.iter().filter_map(|layer| layer.said) {
        ids.extend(said.keys().map(String::as_str));
    }
    ids.extend(adapter.pinned().iter().map(|pin| pin.id));
    let cannot = not_supported(adapter);

    ids.into_iter()
        .map(|id| {
            let here = installed.iter().find(|it| it.id == id);
            // Local, then the workspace, then Shared: the first that names it decides.
            let said: Vec<(&Layer<'_>, bool)> = layers
                .iter()
                .filter_map(|layer| Some((layer, *layer.said?.get(id)?)))
                .collect();
            let (mut wanted, mut source) =
                said.first().map_or((None, Source::Default), |(layer, on)| {
                    (Some(*on), layer.source)
                });
            let mut ignored = Vec::new();
            let pin = adapter.pinned().iter().find(|pin| pin.id == id);
            for (layer, on) in said {
                let (file, at) = (&layer.file, layer.at);
                let key = format!("{at}{TABLE}.{harness}.{}", toml_key(id));
                if let Some(pin) = pin.filter(|pin| pin.on != on) {
                    ignored.push(Ignored {
                        source: layer.source,
                        why: format!("{file} sets {key} to {on}, and {}", pin.why),
                    });
                } else if let Some(cannot) = &cannot {
                    ignored.push(Ignored {
                        source: layer.source,
                        why: format!("{file} sets {key}, and {cannot}; charter hands it nothing"),
                    });
                }
            }
            if let Some(pin) = pin {
                wanted = Some(pin.on);
                source = Source::Default;
            }
            Effective {
                id: id.to_owned(),
                name: here.map_or_else(|| split(id).0.to_owned(), |it| it.name.clone()),
                origin: here.map(|it| it.source.clone()).unwrap_or_default(),
                wanted,
                source,
                installed: here.is_some(),
                pinned: pin.map(|pin| pin.why),
                ignored,
            }
        })
        .collect()
}

/// **What a chat on this harness is handed**: every pin, and every installed plugin a file
/// turned on or off. Nothing for a harness whose adapter cannot apply, and nothing not set: the
/// harness decides those as it always did.
pub fn chosen(adapter: &dyn Adapter, all: &[Effective]) -> Chosen {
    if adapter.support() != Support::PerChat {
        return Chosen::new();
    }
    all.iter()
        .filter(|it| it.pinned.is_some() || it.installed)
        .filter_map(|it| Some((it.id.clone(), it.wanted?)))
        .collect()
}

/// What a chat of `kind`, started in the plane at `root` — in `workspace`, when it is in one —
/// with `env`, is handed.
///
/// **The harness's record is read only when a layer names one of its plugins**: with nothing
/// chosen, the answer is the pins whatever is installed, so a project that says nothing costs no
/// read of anybody's home directory. Where the record cannot be read, the pins alone: a listing
/// charter could not read is not a reason to start a chat without its guard.
pub fn for_start(kind: &str, root: &Path, workspace: Option<&str>, env: &Env<'_>) -> Chosen {
    let Some(adapter) = adapter(kind) else {
        return Chosen::new();
    };
    let choices = Choices::read_in(root, workspace);
    let installed = if choices.names_any(adapter.harness()) {
        adapter.installed(env).unwrap_or_default()
    } else {
        Vec::new()
    };
    chosen(adapter, &resolve(adapter, &installed, &choices))
}

/// One harness in the settings tab.
#[derive(Clone)]
pub struct Group {
    pub adapter: &'static dyn Adapter,
    /// Where its listing was read from ([`Adapter::record`]).
    pub record: Option<PathBuf>,
    /// Why the harness's own record could not be read, if it could not.
    pub trouble: Option<String>,
    pub plugins: Vec<Effective>,
}

/// Every harness, with what it has installed and what `choices` — a project's, or a project's in
/// one workspace ([`Choices::read_in`]) — have each at.
pub fn survey(choices: &Choices, env: &Env<'_>) -> Vec<Group> {
    ADAPTERS
        .iter()
        .map(|&adapter| {
            let (installed, trouble) = match adapter.installed(env) {
                Ok(installed) => (installed, None),
                Err(why) => (Vec::new(), Some(why)),
            };
            Group {
                adapter,
                record: adapter.record(env),
                trouble,
                plugins: resolve(adapter, &installed, choices),
            }
        })
        .collect()
}

/// Everything in `text`'s `[harness_plugins]` that charter would not read or would not honour,
/// as `file` holds it, one sentence each, in the file's order.
pub fn refusals(text: &str, file: &str) -> Vec<String> {
    let Ok(top) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    refusals_in(&top, file, "")
}

/// [`refusals`], of a table already read — a whole TOML file, or a workspace's `settings` read as
/// one (charter-app#282) — whose `[harness_plugins]` sits at `at` in `file` (`"settings."` in a
/// `workspace.json`), so each sentence names the key where it is written.
pub fn refusals_in(top: &toml::Table, file: &str, at: &str) -> Vec<String> {
    let Some(table) = top.get(TABLE) else {
        return Vec::new();
    };
    let shape = "each harness is [harness_plugins.<harness>], holding \"<plugin id>\" = true or \
                 false";
    let Some(table) = table.as_table() else {
        return vec![format!("{at}{TABLE} in {file} is not a table — {shape}")];
    };
    let mut out = Vec::new();
    for (harness, plugins) in table {
        let Some(adapter) = adapter(harness) else {
            let known: Vec<&str> = ADAPTERS.iter().map(|it| it.harness()).collect();
            out.push(format!(
                "[{at}{TABLE}.{}] in {file} is not a harness charter knows — one of: {}",
                toml_key(harness),
                known.join(", ")
            ));
            continue;
        };
        let Some(plugins) = plugins.as_table() else {
            out.push(format!(
                "{at}{TABLE}.{harness} in {file} is not a table — {shape}"
            ));
            continue;
        };
        for (id, on) in plugins {
            let key = format!("{at}{TABLE}.{harness}.{}", toml_key(id));
            if !id_ok(id) {
                out.push(format!(
                    "{key} in {file} is not a plugin id — one line of at most {MOST_ID_BYTES} \
                     bytes, with nothing invisible in it"
                ));
                continue;
            }
            let Some(on) = on.as_bool() else {
                out.push(format!("{key} in {file} is not true or false"));
                continue;
            };
            if let Some(pin) = adapter
                .pinned()
                .iter()
                .find(|pin| pin.id == id && pin.on != on)
            {
                out.push(format!("{key} in {file} cannot be {on}: {}", pin.why));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
