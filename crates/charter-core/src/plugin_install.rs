//! `charter plugin install` and `uninstall`: charter's hooks, guard and skills for a chat the
//! operator starts in a terminal, not only for one the app starts (#374).
//!
//! # Why this exists
//!
//! Every chat the app starts is armed for that session alone ([`crate::plugin`]): Claude Code
//! loads the bundled plugin with `--plugin-dir`, Codex gets `-c hooks.*` flags. A `claude` or
//! `codex` typed into a terminal gets none of that, and until this command the only thing
//! guarding one was the retired Python charter's `charter@charter` plugin, by coincidence.
//! The operator's ruling (2026-09-26): make the app's own plugin installable for chats started
//! outside the app, one adapter per harness.
//!
//! # The model: a plan, then its application
//!
//! Each [`Adapter`] answers one question — what would have to change on this machine for its
//! harness to carry charter — as a list of [`Step`]s, without writing anything. `--dry-run`
//! prints that list; a real run prints it and applies it. So "show what it would change" and
//! "change it" are one computation, and a second run finds every step already done: that is
//! what idempotent means here, and the test holds it.
//!
//! # Claude Code
//!
//! Measured on 2.1.283 with a throwaway `CLAUDE_CONFIG_DIR`:
//!
//! - A user `settings.json` holding `extraKnownMarketplaces.<m>` with a `directory` source and
//!   `enabledPlugins["charter@<m>"] = true` loads that plugin in every session, with no install
//!   step, no cache copy and no `installed_plugins.json` entry. The session reads the directory
//!   itself, so a later rewrite of it is seen.
//! - A plugin whose hooks name the binary as `"${CHARTER_HOOK_BINARY}"` does NOT guard a
//!   terminal chat: the variable is the app's, it is unset there, the command exits 127, and
//!   Claude Code treats that as a non-blocking error and runs the tool. So the plugin this
//!   command registers is a **copy** of the bundled one, in charter's own directory
//!   ([`plugin_dir`]), whose hooks name this binary by its absolute path.
//! - `--plugin-dir` wins the name `charter`: an app chat still loads `charter@inline` and not
//!   this copy, so installing changes nothing about an app chat.
//! - A setting's `env` overrides the process's own environment, so writing
//!   `CHARTER_HOOK_BINARY` into user settings would override the app's value in its own chats.
//!   That is why the path goes into the copy's hooks and not into an `env` key.
//!
//! # Codex
//!
//! Codex 0.147.0 can take a plugin too, but only by copying it into its own cache, and every
//! hook it brings starts untrusted. The app arms a Codex chat with `-c hooks.*` flags, which
//! Codex runs *beside* hooks from `config.toml`, not instead of them — so an installed plugin
//! would run every state hook twice in an app chat, and brief it twice. This adapter therefore
//! writes only the Bash guard, as one `[[hooks.PreToolUse]]` group in `$CODEX_HOME/config.toml`.
//! Twice in an app chat, the guard only refuses twice. Codex asks the operator to trust it the
//! first time it starts, and until then it does not run — which the command says.
//!
//! # opencode
//!
//! opencode loads every script in its global plugin directory, so this adapter writes one: the
//! opencode shim ([`crate::opencode`]) as `$XDG_CONFIG_HOME/opencode/plugin/charter.ts`, else
//! `~/.config/opencode/plugin/charter.ts`, naming this binary by its absolute path. It carries
//! only the guards before a tool runs, for Codex's reason: an app chat loads it beside the
//! bundled shim, and a doubled guard only refuses twice where doubled state hooks would report
//! and brief twice (ADR 0058). A file already at that name is replaced only when charter wrote
//! it — this command, or the retired Python charter, whose shim guarded and briefed every chat
//! by the `charter` on `PATH` — and any other file there is left alone with a sentence.
//!
//! # The retired plugin
//!
//! Nothing here ever enables `charter@charter`. Where a file this command writes anyway
//! enables it, the same write turns it off and says so. That does not reach a plane whose own
//! `.claude/settings.json` turns it on: measured on 2.1.283, a project `true` beats the user
//! `false`, and both plugins named `charter` then load, with both sets of hooks. The plane's
//! file is the operator's, so `charter doctor` names it rather than this command rewriting it.
//!
//! # Uninstall
//!
//! Takes back what install writes: the two user-settings keys, Claude Code's own record of the
//! marketplace, the copy, and every Codex guard group in the shape install writes — whichever
//! `charter` it names, since install replaces those too rather than adding a second. A Codex
//! trust record for the hook stays in Codex's `[hooks.state]`; it names a hook that is gone.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// The marketplace name the copy is registered under, so its id is `charter@charter-app`: the
/// `charter` plugin, from the charter app.
pub const MARKETPLACE: &str = "charter-app";

/// The id Claude Code gives the installed copy.
pub const INSTALLED_AS: &str = "charter@charter-app";

/// What a person names a harness on the command line — a profile's `kind`.
pub const HARNESSES: [&str; 3] = ["claude", "codex", "opencode"];

/// Where things are on this machine, handed in so a test never reaches the operator's own.
#[derive(Debug, Clone)]
pub struct Machine {
    /// Claude Code's config folder: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
    pub claude_config: PathBuf,
    /// Codex's: `$CODEX_HOME`, else `~/.codex`.
    pub codex_home: PathBuf,
    /// opencode's global config folder: `$XDG_CONFIG_HOME/opencode`, else
    /// `~/.config/opencode`.
    pub opencode_config: PathBuf,
    /// charter's own machine directory ([`crate::machine::dir`]), where the copy is kept.
    pub charter_dir: PathBuf,
    /// The `charter` a hook runs — this binary, by its resolved path.
    pub binary: PathBuf,
    /// The plugin the app ships, which the copy is made from. Only install reads it.
    pub bundle: Option<PathBuf>,
}

impl Machine {
    /// This machine, from the environment charter was started with.
    pub fn from_env(binary: PathBuf, bundle: Option<PathBuf>) -> Result<Self, String> {
        let root = crate::machine::config_root();
        Self::from_env_under(root, binary, bundle)
    }

    /// [`Self::from_env`], for a reader: `charter doctor` asks about the copy and writes
    /// nothing, so it takes charter's directory without the fence a writer is held to.
    pub fn from_env_to_read(binary: PathBuf, bundle: Option<PathBuf>) -> Result<Self, String> {
        let root = crate::machine::config_root_to_read_the_plugin_copy();
        Self::from_env_under(root, binary, bundle)
    }

    fn from_env_under(
        config_root: Option<PathBuf>,
        binary: PathBuf,
        bundle: Option<PathBuf>,
    ) -> Result<Self, String> {
        let home = crate::profiles::home();
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        let claude_config = match std::env::var_os("CLAUDE_CONFIG_DIR") {
            Some(dir) if dir.is_empty() => {
                return Err(
                    "CLAUDE_CONFIG_DIR is set to nothing, which Claude Code reads as \
                            its own working directory; charter will not guess which folder \
                            that is. Unset it or name a folder."
                        .to_owned(),
                );
            }
            Some(dir) => PathBuf::from(dir),
            None => home
                .clone()
                .ok_or(
                    "HOME is not set, so charter cannot tell where Claude Code keeps its settings",
                )?
                .join(".claude"),
        };
        let codex_home = match var("CODEX_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => home
                .clone()
                .ok_or("HOME is not set, so charter cannot tell where Codex keeps its config")?
                .join(".codex"),
        };
        let opencode_config = match var("XDG_CONFIG_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => home
                .clone()
                .ok_or("HOME is not set, so charter cannot tell where opencode keeps its config")?
                .join(".config"),
        }
        .join("opencode");
        let charter_dir = config_root
            .map(|root| crate::machine::dir(&root))
            .ok_or("charter cannot tell where its own machine directory is (HOME is not set)")?;
        Ok(Self {
            claude_config,
            codex_home,
            opencode_config,
            charter_dir,
            binary,
            bundle,
        })
    }
}

/// Where the copy of the plugin is kept: `<charter's machine directory>/plugin`.
pub fn plugin_dir(m: &Machine) -> PathBuf {
    m.charter_dir.join("plugin")
}

/// The plugin the app ships beside `exe`: `Contents/Resources/plugin` in a macOS bundle,
/// `/usr/lib/charter/plugin` beside `/usr/bin/charter`, and `plugin/` beside a development
/// build. `None` when none of those holds a plugin manifest.
pub fn bundle_beside(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    [
        dir.join("../Resources/plugin"),
        dir.join("../lib/charter/plugin"),
        dir.join("plugin"),
    ]
    .into_iter()
    .find(|p| p.join(".claude-plugin/plugin.json").is_file())
    .and_then(|p| p.canonicalize().ok())
}

/// One change, as the command prints it and then makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The file or folder it is about.
    pub path: PathBuf,
    /// What changes, in words: "enable charter@charter-app".
    pub what: String,
    /// `false` when it is already so, and nothing will be written for it.
    pub needed: bool,
    /// Which of the plan's writes makes it so, once one is queued.
    by: Option<usize>,
}

/// A file's new content, or its removal — what applying a plan writes.
#[derive(Debug, Clone)]
enum Write {
    File(PathBuf, Vec<u8>),
    Tree(PathBuf, BTreeMap<PathBuf, Vec<u8>>),
    RemoveTree(PathBuf),
    RemoveFile(PathBuf),
}

/// What one adapter would do, and the writes that do it.
#[derive(Debug, Default)]
pub struct Plan {
    pub steps: Vec<Step>,
    writes: Vec<Write>,
    /// Something the operator has to know once it is done, such as a trust prompt.
    pub notes: Vec<String>,
}

impl Plan {
    fn step(&mut self, path: &Path, what: impl Into<String>, needed: bool) {
        self.steps.push(Step {
            path: path.to_path_buf(),
            what: what.into(),
            needed,
            by: None,
        });
    }

    /// Queue `write` as what makes every needed step from `from` on so.
    fn settle(&mut self, from: usize, write: Write) {
        let at = self.writes.len();
        for step in self.steps.iter_mut().skip(from) {
            if step.needed && step.by.is_none() {
                step.by = Some(at);
            }
        }
        self.writes.push(write);
    }

    /// Whether anything would be written.
    pub fn changes(&self) -> bool {
        self.steps.iter().any(|s| s.needed)
    }
}

/// Install or uninstall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Install,
    Uninstall,
}

/// One harness's part in charter's plugin.
pub trait Adapter {
    /// The harness, as a profile's `kind` names it.
    fn harness(&self) -> &'static str;
    /// The folder whose absence means this harness was never set up on this machine.
    fn home(&self, m: &Machine) -> PathBuf;
    /// What installing would change.
    fn install(&self, m: &Machine) -> Result<Plan, String>;
    /// What uninstalling would change.
    fn uninstall(&self, m: &Machine) -> Result<Plan, String>;
    /// Whether charter's plugin is installed for this harness, whatever charter it names.
    fn installed(&self, m: &Machine) -> Result<bool, String>;
    /// The `charter` the installed hooks run, when they name one.
    fn runs(&self, m: &Machine) -> Option<PathBuf>;
    /// The files of this harness's own configuration that enable the retired plugin.
    fn superseded(&self, m: &Machine) -> Vec<PathBuf>;
}

/// Every adapter charter has, in [`HARNESSES`] order.
pub fn adapters() -> impl Iterator<Item = &'static dyn Adapter> {
    HARNESSES.iter().filter_map(|h| adapter(h))
}

/// The binary a hook command `'<path>' hook <word>` runs — [`crate::plugin::command_at`]
/// read back.
fn quoted_binary(command: &str) -> Option<PathBuf> {
    let quoted = command.strip_prefix('\'')?;
    let end = quoted.rfind("' hook ")?;
    Some(PathBuf::from(quoted[..end].replace(r"'\''", "'")))
}

/// Whether a Claude Code settings file enables `id`.
fn enables(path: &Path, id: &str) -> bool {
    json_object(path).is_ok_and(|(map, _)| {
        map.get("enabledPlugins")
            .and_then(|p| p.get(id))
            .is_some_and(crate::scaffold::text::truthy)
    })
}

/// Whether the Claude Code settings file at `path` turns the retired plugin on.
pub fn enables_superseded(path: &Path) -> bool {
    enables(path, crate::plugin::SUPERSEDED)
}

/// A plane's own settings files that enable the retired plugin: the two a session in the plane
/// reads. A plane's file is the operator's, and nothing here rewrites it.
pub fn superseded_in_plane(plane: &Path) -> Vec<PathBuf> {
    [".claude/settings.json", ".claude/settings.local.json"]
        .iter()
        .map(|rel| plane.join(rel))
        .filter(|p| enables(p, crate::plugin::SUPERSEDED))
        .collect()
}

/// The adapter for `harness`, if charter has one.
pub fn adapter(harness: &str) -> Option<&'static dyn Adapter> {
    match harness {
        "claude" => Some(&ClaudeCode),
        "codex" => Some(&Codex),
        "opencode" => Some(&Opencode),
        _ => None,
    }
}

/// What happened for one harness.
#[derive(Debug)]
pub struct Outcome {
    pub harness: &'static str,
    /// The plan, or why there is none.
    pub plan: Result<Plan, String>,
    /// Why this harness was left alone, when it was.
    pub skipped: Option<String>,
    /// Why applying the plan failed part-way, when it did.
    pub failed: Option<String>,
    /// How many of the plan's writes were made before it stopped.
    applied: usize,
}

/// Plan — and unless `dry_run`, apply — `verb` for each of `named`, or for every harness
/// charter has an adapter for when `named` is empty. A harness that was not named and whose
/// config folder does not exist is skipped with a sentence rather than set up from nothing.
pub fn run(m: &Machine, verb: Verb, named: &[String], dry_run: bool) -> Vec<Outcome> {
    let explicit = !named.is_empty();
    let harnesses: Vec<&str> = if explicit {
        named.iter().map(String::as_str).collect()
    } else {
        HARNESSES.to_vec()
    };
    let mut out = Vec::new();
    for name in harnesses {
        let Some(a) = adapter(name) else {
            continue;
        };
        let home = a.home(m);
        if !explicit && !home.is_dir() {
            out.push(Outcome {
                harness: a.harness(),
                plan: Ok(Plan::default()),
                skipped: Some(format!(
                    "{} does not exist, so {} is not set up on this machine; name it with \
                     --harness {} to {} anyway",
                    home.display(),
                    a.harness(),
                    a.harness(),
                    match verb {
                        Verb::Install => "install",
                        Verb::Uninstall => "uninstall",
                    }
                )),
                failed: None,
                applied: 0,
            });
            continue;
        }
        let plan = match verb {
            Verb::Install => a.install(m),
            Verb::Uninstall => a.uninstall(m),
        };
        let (applied, failed) = match (&plan, dry_run) {
            (Ok(plan), false) => apply(plan),
            _ => (0, None),
        };
        out.push(Outcome {
            harness: a.harness(),
            plan,
            skipped: None,
            failed,
            applied,
        });
    }
    out
}

/// What the command prints.
pub fn render(outcomes: &[Outcome], dry_run: bool) -> String {
    let mut text = String::new();
    for o in outcomes {
        text.push_str(&format!("{}:\n", o.harness));
        if let Some(why) = &o.skipped {
            text.push_str(&format!("  skipped: {why}\n"));
            continue;
        }
        match &o.plan {
            Err(why) => text.push_str(&format!("  nothing changed: {why}\n")),
            Ok(plan) => {
                for s in &plan.steps {
                    let verb = match (s.needed, dry_run) {
                        (false, _) => "already",
                        (true, true) => "would",
                        (true, false) if s.by.is_some_and(|by| by < o.applied) => "done",
                        (true, false) => "not done",
                    };
                    text.push_str(&format!(
                        "  {verb:<8} {} \u{2014} {}\n",
                        s.what,
                        crate::shown::one_line(&s.path.display().to_string(), 1024)
                    ));
                }
                if let Some(why) = &o.failed {
                    text.push_str(&format!("  FAILED: {why}\n"));
                }
                if o.failed.is_none() && plan.changes() {
                    for note in &plan.notes {
                        text.push_str(&format!("  note: {note}\n"));
                    }
                }
            }
        }
    }
    if dry_run {
        text.push_str("Nothing was written (--dry-run).\n");
    }
    text
}

/// Whether any harness could not be planned or applied.
pub fn failed(outcomes: &[Outcome]) -> bool {
    outcomes
        .iter()
        .any(|o| o.plan.is_err() || o.failed.is_some())
}

/// Make the plan's writes in order, stopping at the first that fails: how many were made,
/// and why the next one was not.
fn apply(plan: &Plan) -> (usize, Option<String>) {
    for (done, w) in plan.writes.iter().enumerate() {
        let made = match w {
            Write::File(path, bytes) => write_file(path, bytes),
            Write::Tree(path, files) => write_tree(path, files),
            Write::RemoveTree(path) => {
                hold(path);
                match std::fs::remove_dir_all(path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                }
            }
            Write::RemoveFile(path) => {
                hold(path);
                match std::fs::remove_file(path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                }
            }
        };
        if let Err(e) = made {
            return (
                done,
                Some(format!(
                    "{} could not be written: {e}",
                    crate::shown::one_line(&path_of(w).display().to_string(), 1024)
                )),
            );
        }
    }
    (plan.writes.len(), None)
}

fn path_of(w: &Write) -> &Path {
    match w {
        Write::File(p, _) | Write::Tree(p, _) | Write::RemoveTree(p) | Write::RemoveFile(p) => p,
    }
}

/// The fence, asked about `path` by its nearest existing ancestor: a folder not made yet has
/// no resolved spelling, and the fence compares resolved paths.
fn hold(path: &Path) {
    let mut at = path;
    while !at.exists() {
        match at.parent() {
            Some(up) => at = up,
            None => break,
        }
    }
    crate::fence::hold(crate::fence::Act::HarnessConfig, at);
}

/// Write `bytes` to `path` in one step: a temporary file beside it, synced, then renamed over
/// it, keeping the mode the old file had. A reader sees the old file or the new one.
///
/// A file that is a symbolic link — a dotfiles manager's — is written through: the file it
/// points at is replaced, and the link stays a link.
fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let linked = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    let target = if linked {
        path.canonicalize()?
    } else {
        path.to_path_buf()
    };
    let path = target.as_path();
    let dir = path.parent().unwrap_or(Path::new("."));
    hold(dir);
    std::fs::create_dir_all(dir)?;
    let mode = std::fs::metadata(path).ok().map(|m| m.permissions());
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    if let Some(mode) = mode {
        temp.as_file().set_permissions(mode)?;
    }
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Replace the directory `path` with exactly `files`: built beside it, then moved into place
/// with two renames, so a harness starting meanwhile finds the old copy or the new one for all
/// but the instant between them. A staging directory that fails is removed.
fn write_tree(path: &Path, files: &BTreeMap<PathBuf, Vec<u8>>) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    hold(parent);
    std::fs::create_dir_all(parent)?;
    let staged = tempfile::Builder::new()
        .prefix(".plugin-")
        .tempdir_in(parent)?;
    for (rel, bytes) in files {
        let to = staged.path().join(rel);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&to, bytes)?;
    }
    let old = tempfile::Builder::new()
        .prefix(".plugin-old-")
        .tempdir_in(parent)?;
    let aside = old.path().join("plugin");
    let had = match std::fs::rename(path, &aside) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    };
    if let Err(e) = std::fs::rename(staged.path(), path) {
        if had {
            let _ = std::fs::rename(&aside, path);
        }
        return Err(e);
    }
    // `staged` now names nothing, and `old` takes the previous copy with it when dropped.
    Ok(())
}

/// Every regular file under `dir`, by path relative to it. `None` when `dir` is not there.
fn read_tree(dir: &Path) -> std::io::Result<Option<BTreeMap<PathBuf, Vec<u8>>>> {
    if !dir.exists() {
        return Ok(None);
    }
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at)? {
            let entry = entry?;
            let path = entry.path();
            // Followed, as Claude Code follows it: a linked skill is part of the plugin.
            let kind = std::fs::metadata(&path)?.file_type();
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                let rel = path.strip_prefix(dir).unwrap_or(&path).to_path_buf();
                out.insert(rel, std::fs::read(&path)?);
            }
        }
    }
    Ok(Some(out))
}

// ------------------------------------------------------------------------------------------
// Claude Code
// ------------------------------------------------------------------------------------------

/// Claude Code: a copy of the bundled plugin, registered in the user settings as a local
/// marketplace and enabled there.
pub struct ClaudeCode;

impl ClaudeCode {
    fn settings(m: &Machine) -> PathBuf {
        m.claude_config.join("settings.json")
    }

    /// The copy of the plugin this machine should hold.
    fn copy(m: &Machine) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
        let bundle = m.bundle.as_ref().ok_or(
            "charter cannot find the plugin the app ships beside this binary, so there is \
             nothing to install from. Run the `charter` inside the app, or name the plugin's \
             folder with --plugin-from",
        )?;
        let mut files = read_tree(bundle)
            .map_err(|e| format!("{} could not be read: {e}", bundle.display()))?
            .ok_or_else(|| format!("{} does not exist", bundle.display()))?;
        let manifest = PathBuf::from(".claude-plugin/plugin.json");
        let mut plugin: Map<String, Value> = files
            .get(&manifest)
            .and_then(|raw| serde_json::from_slice(raw).ok())
            .ok_or_else(|| format!("{} has no plugin manifest", bundle.display()))?;
        if plugin.get("name").and_then(Value::as_str) != Some(crate::plugin::NAME) {
            return Err(format!(
                "{} is not charter's plugin: its manifest does not name it `{}`",
                bundle.display(),
                crate::plugin::NAME
            ));
        }
        plugin.insert(
            "description".to_owned(),
            json!(
                "charter's hooks, guard and skills for chats started outside the charter app. \
                 Written by `charter plugin install`; `charter plugin uninstall` removes it."
            ),
        );
        files.insert(manifest, pretty(&Value::Object(plugin)));
        files.insert(
            PathBuf::from(".claude-plugin/marketplace.json"),
            pretty(&json!({
                "name": MARKETPLACE,
                "owner": {"name": "charter"},
                "plugins": [{"name": crate::plugin::NAME, "source": "./"}],
            })),
        );
        let binary = m.binary.clone();
        files.insert(
            PathBuf::from(crate::plugin::HOOKS_FILE),
            crate::plugin::hooks_json_running(
                |word| crate::plugin::command_at(&binary, word),
                "Written by `charter plugin install` from charter_core::hookreg, naming the \
                 charter that installed it. Re-run it after moving or updating the app.",
            )
            .into_bytes(),
        );
        Ok(files)
    }
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(v).expect("a JSON value serialises");
    text.push('\n');
    text.into_bytes()
}

/// A settings file as an object, or why charter will not touch it. A missing file is empty.
fn json_object(path: &Path) -> Result<(Map<String, Value>, String), String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Map::new(), String::new()));
        }
        Err(e) => return Err(format!("{} could not be read: {e}", path.display())),
    };
    match crate::pyjson::loads_strict(&raw) {
        Some(Value::Object(map)) => Ok((map, raw)),
        _ => Err(format!(
            "{} is not a JSON object charter can read, so it was left as it is",
            path.display()
        )),
    }
}

/// `map`, indented the way `raw` was (two spaces for a new file), non-ASCII kept as itself the
/// way Claude Code writes it, ending in a newline.
fn render_json(map: Map<String, Value>, raw: &str) -> Vec<u8> {
    use serde::Serialize as _;
    let indent = crate::pyjson::json_style(raw)
        .0
        .filter(|pad| !pad.is_empty())
        .unwrap_or_else(|| "  ".to_owned());
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    Value::Object(map)
        .serialize(&mut ser)
        .expect("a JSON value serialises");
    out.push(b'\n');
    out
}

/// The object at `map[key]`, created when absent — or why it cannot be.
fn object_at<'a>(
    map: &'a mut Map<String, Value>,
    key: &str,
    path: &Path,
) -> Result<&'a mut Map<String, Value>, String> {
    let slot = map
        .entry(key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    slot.as_object_mut().ok_or_else(|| {
        format!(
            "{}'s `{key}` is not an object, so it was left as it is",
            path.display()
        )
    })
}

impl Adapter for ClaudeCode {
    fn harness(&self) -> &'static str {
        "claude"
    }

    fn home(&self, m: &Machine) -> PathBuf {
        m.claude_config.clone()
    }

    fn install(&self, m: &Machine) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let dir = plugin_dir(m);
        let want = Self::copy(m)?;
        let have =
            read_tree(&dir).map_err(|e| format!("{} could not be read: {e}", dir.display()))?;
        let fresh = have.as_ref() == Some(&want);
        plan.step(
            &dir,
            "a copy of charter's plugin whose hooks run this charter",
            !fresh,
        );
        if !fresh {
            plan.settle(0, Write::Tree(dir.clone(), want));
        }
        let from = plan.steps.len();

        let path = Self::settings(m);
        let (mut map, raw) = json_object(&path)?;
        let source = json!({"source": {"source": "directory", "path": dir.display().to_string()}});
        let markets = object_at(&mut map, "extraKnownMarketplaces", &path)?;
        let registered = markets.get(MARKETPLACE) == Some(&source);
        markets.insert(MARKETPLACE.to_owned(), source);
        plan.step(
            &path,
            format!("register that copy as the `{MARKETPLACE}` marketplace"),
            !registered,
        );
        let enabled = object_at(&mut map, "enabledPlugins", &path)?;
        let on = enabled.get(INSTALLED_AS) == Some(&Value::Bool(true));
        enabled.insert(INSTALLED_AS.to_owned(), Value::Bool(true));
        plan.step(&path, format!("enable {INSTALLED_AS}"), !on);
        if enabled
            .get(crate::plugin::SUPERSEDED)
            .is_some_and(crate::scaffold::text::truthy)
        {
            enabled.insert(crate::plugin::SUPERSEDED.to_owned(), Value::Bool(false));
            plan.step(
                &path,
                format!(
                    "turn off {}, the retired Python charter's plugin",
                    crate::plugin::SUPERSEDED
                ),
                true,
            );
        }
        if plan.steps[from..].iter().any(|s| s.needed) {
            plan.settle(from, Write::File(path, render_json(map, &raw)));
        }
        plan.notes.push(
            "a `claude` started from now on loads it; one already running does not. A chat the \
             app starts still loads the app's own copy instead."
                .to_owned(),
        );
        Ok(plan)
    }

    fn installed(&self, m: &Machine) -> Result<bool, String> {
        let path = Self::settings(m);
        json_object(&path)?;
        Ok(enables(&path, INSTALLED_AS))
    }

    fn runs(&self, m: &Machine) -> Option<PathBuf> {
        let hooks = std::fs::read(plugin_dir(m).join(crate::plugin::HOOKS_FILE)).ok()?;
        let doc: Value = serde_json::from_slice(&hooks).ok()?;
        let command = doc["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str()?;
        quoted_binary(command)
    }

    fn superseded(&self, m: &Machine) -> Vec<PathBuf> {
        let path = Self::settings(m);
        if enables(&path, crate::plugin::SUPERSEDED) {
            vec![path]
        } else {
            Vec::new()
        }
    }

    fn uninstall(&self, m: &Machine) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let path = Self::settings(m);
        let (mut map, raw) = json_object(&path)?;
        let mut changed = false;
        for (key, id, what) in [
            (
                "enabledPlugins",
                INSTALLED_AS,
                format!("stop enabling {INSTALLED_AS}"),
            ),
            (
                "extraKnownMarketplaces",
                MARKETPLACE,
                format!("forget the `{MARKETPLACE}` marketplace"),
            ),
        ] {
            let had = match map.get_mut(key) {
                Some(Value::Object(inner)) => inner.remove(id).is_some(),
                _ => false,
            };
            changed |= had;
            plan.step(&path, what, had);
        }
        if changed {
            plan.settle(0, Write::File(path, render_json(map, &raw)));
        }
        // Claude Code records a marketplace it has seen in its own list; only that entry goes.
        let known = m.claude_config.join("plugins/known_marketplaces.json");
        if let Ok((mut map, raw)) = json_object(&known)
            && map.remove(MARKETPLACE).is_some()
        {
            plan.step(
                &known,
                format!("drop Claude Code's record of `{MARKETPLACE}`"),
                true,
            );
            plan.settle(0, Write::File(known, render_json(map, &raw)));
        }
        let dir = plugin_dir(m);
        let there = dir.exists();
        plan.step(&dir, "remove charter's copy of its plugin", there);
        if there {
            plan.settle(0, Write::RemoveTree(dir));
        }
        Ok(plan)
    }
}

// ------------------------------------------------------------------------------------------
// Codex
// ------------------------------------------------------------------------------------------

/// Codex: charter's Bash guard as one hook group in the user `config.toml`.
pub struct Codex;

/// An inline table as a table, and an array of inline tables (or an empty array) as an array
/// of tables — what `toml_edit` keeps private as `make_item`. Anything else is left as it is.
fn spell_out(item: &mut toml_edit::Item) {
    let taken = std::mem::take(item);
    let taken = match taken.into_table() {
        Ok(t) => toml_edit::Item::Table(t),
        Err(i) => i,
    };
    *item = match taken {
        toml_edit::Item::Value(toml_edit::Value::Array(a)) if a.is_empty() => {
            toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new())
        }
        other => match other.into_array_of_tables() {
            Ok(a) => toml_edit::Item::ArrayOfTables(a),
            Err(i) => i,
        },
    };
}

/// The guard's timeout, in seconds: the registry's, as every other arming of it uses.
fn guard_timeout() -> u32 {
    crate::hookreg::find("pretooluse").map_or(10, |h| h.timeout)
}

/// The guard's group, as this binary writes it.
fn codex_guard(binary: &Path) -> toml_edit::ArrayOfTables {
    let mut hook = toml_edit::Table::new();
    hook.insert("type", toml_edit::value("command"));
    hook.insert(
        "command",
        toml_edit::value(crate::plugin::command_at(binary, "pretooluse")),
    );
    hook.insert("timeout", toml_edit::value(i64::from(guard_timeout())));
    let mut hooks = toml_edit::ArrayOfTables::new();
    hooks.push(hook);
    let mut group = toml_edit::Table::new();
    group.insert("matcher", toml_edit::value("Bash"));
    group.insert("hooks", toml_edit::Item::ArrayOfTables(hooks));
    let mut groups = toml_edit::ArrayOfTables::new();
    groups.push(group);
    groups
}

/// Whether `group` is a guard group charter wrote: matcher `Bash`, and one command hook that
/// runs `charter hook pretooluse` by a quoted path — the shape [`codex_guard`] writes.
fn is_charters_guard(group: &toml_edit::Table) -> bool {
    let Some(hooks) = group
        .get("hooks")
        .and_then(toml_edit::Item::as_array_of_tables)
    else {
        return false;
    };
    group.get("matcher").and_then(toml_edit::Item::as_str) == Some("Bash")
        && hooks.len() == 1
        && hooks.iter().all(|h| {
            h.get("command")
                .and_then(toml_edit::Item::as_str)
                .is_some_and(|c| {
                    // `'<path>' hook pretooluse`, as `plugin::command_at` spells it.
                    c.len() > "'' hook pretooluse".len()
                        && c.starts_with('\'')
                        && c.ends_with("' hook pretooluse")
                })
        })
}

impl Codex {
    fn config(m: &Machine) -> PathBuf {
        m.codex_home.join("config.toml")
    }

    fn read(path: &Path) -> Result<toml_edit::DocumentMut, String> {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("{} could not be read: {e}", path.display())),
        };
        raw.parse().map_err(|e: toml_edit::TomlError| {
            format!(
                "{} is not TOML charter can read, so it was left as it is: {}",
                path.display(),
                crate::shown::one_line(&e.to_string(), 160)
            )
        })
    }

    /// `hooks.PreToolUse`, or why it is not an array of tables.
    fn guard_groups<'a>(
        doc: &'a mut toml_edit::DocumentMut,
        path: &Path,
        create: bool,
    ) -> Result<Option<&'a mut toml_edit::ArrayOfTables>, String> {
        let bad = |what: &str| {
            format!(
                "{}'s `{what}` is not what Codex reads, so it was left as it is",
                path.display()
            )
        };
        if !doc.contains_key("hooks") {
            if !create {
                return Ok(None);
            }
            let mut t = toml_edit::Table::new();
            t.set_implicit(true);
            doc.insert("hooks", toml_edit::Item::Table(t));
        }
        // Codex reads the inline spellings too — `hooks = { PreToolUse = [ … ] }` — and
        // `toml_edit` edits them only as tables, so an inline one is spelled out first.
        spell_out(&mut doc["hooks"]);
        let hooks = doc["hooks"]
            .as_table_like_mut()
            .ok_or_else(|| bad("hooks"))?;
        if hooks.get("PreToolUse").is_none() {
            if !create {
                return Ok(None);
            }
            hooks.insert(
                "PreToolUse",
                toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
            );
        }
        let groups = hooks.get_mut("PreToolUse").expect("present");
        spell_out(groups);
        let groups = groups
            .as_array_of_tables_mut()
            .ok_or_else(|| bad("hooks.PreToolUse"))?;
        for group in groups.iter_mut() {
            if let Some(inner) = group.get_mut("hooks") {
                spell_out(inner);
            }
        }
        Ok(Some(groups))
    }
}

impl Adapter for Codex {
    fn harness(&self) -> &'static str {
        "codex"
    }

    fn home(&self, m: &Machine) -> PathBuf {
        m.codex_home.clone()
    }

    fn install(&self, m: &Machine) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let path = Self::config(m);
        let mut doc = Self::read(&path)?;
        let want = codex_guard(&m.binary);
        let want_group = want.get(0).expect("one group").clone();
        let groups = Self::guard_groups(&mut doc, &path, true)?.expect("created");
        let ours: Vec<usize> = (0..groups.len())
            .filter(|&i| groups.get(i).is_some_and(is_charters_guard))
            .collect();
        let command = crate::plugin::command_at(&m.binary, "pretooluse");
        let current = ours.len() == 1
            && groups.get(ours[0]).is_some_and(|g| {
                g.get("hooks")
                    .and_then(toml_edit::Item::as_array_of_tables)
                    .and_then(|hooks| hooks.get(0))
                    .is_some_and(|h| {
                        h.get("command").and_then(toml_edit::Item::as_str) == Some(&command)
                            && h.get("type").and_then(toml_edit::Item::as_str) == Some("command")
                            && h.get("timeout").and_then(toml_edit::Item::as_integer)
                                == Some(i64::from(guard_timeout()))
                    })
            });
        if !current {
            for i in ours.into_iter().rev() {
                groups.remove(i);
            }
            groups.push(want_group);
        }
        plan.step(
            &path,
            "charter's Bash guard as a PreToolUse hook that runs this charter",
            !current,
        );
        let mut retired = false;
        if let Some(plugin) = doc
            .get_mut("plugins")
            .and_then(|p| p.get_mut(crate::plugin::SUPERSEDED))
            .and_then(toml_edit::Item::as_table_like_mut)
            && plugin.get("enabled").and_then(toml_edit::Item::as_bool) == Some(true)
        {
            plugin.insert("enabled", toml_edit::value(false));
            retired = true;
        }
        if retired {
            plan.step(
                &path,
                format!(
                    "turn off {}, the retired Python charter's plugin",
                    crate::plugin::SUPERSEDED
                ),
                true,
            );
        }
        if !current || retired {
            plan.settle(0, Write::File(path, doc.to_string().into_bytes()));
        }
        plan.notes.push(
            "Codex asks you to review and trust this hook the next time it starts, and until you \
             trust it, it does not run. Only the guard is installed for Codex: the state hooks \
             and the briefing reach a Codex chat only when the app starts it."
                .to_owned(),
        );
        Ok(plan)
    }

    fn installed(&self, m: &Machine) -> Result<bool, String> {
        Self::read(&Self::config(m))?;
        Ok(self.runs(m).is_some())
    }

    fn runs(&self, m: &Machine) -> Option<PathBuf> {
        let mut doc = Self::read(&Self::config(m)).ok()?;
        let groups = Self::guard_groups(&mut doc, &Self::config(m), false).ok()??;
        groups.iter().find(|g| is_charters_guard(g)).and_then(|g| {
            let hooks = g.get("hooks")?.as_array_of_tables()?;
            quoted_binary(hooks.get(0)?.get("command")?.as_str()?)
        })
    }

    fn superseded(&self, m: &Machine) -> Vec<PathBuf> {
        let path = Self::config(m);
        let on = Self::read(&path).is_ok_and(|doc| {
            doc.get("plugins")
                .and_then(|p| p.get(crate::plugin::SUPERSEDED))
                .and_then(|p| p.get("enabled"))
                .and_then(toml_edit::Item::as_bool)
                == Some(true)
        });
        if on { vec![path] } else { Vec::new() }
    }

    fn uninstall(&self, m: &Machine) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let path = Self::config(m);
        if !path.exists() {
            plan.step(&path, "remove charter's Bash guard", false);
            return Ok(plan);
        }
        let mut doc = Self::read(&path)?;
        let mut removed = false;
        if let Some(groups) = Self::guard_groups(&mut doc, &path, false)? {
            let before = groups.len();
            groups.retain(|g| !is_charters_guard(g));
            removed = groups.len() != before;
            let empty = groups.is_empty();
            if removed && empty {
                let hooks = doc["hooks"].as_table_like_mut().expect("read above");
                hooks.remove("PreToolUse");
                if hooks.is_empty() {
                    doc.remove("hooks");
                }
            }
        }
        plan.step(&path, "remove charter's Bash guard", removed);
        if removed {
            plan.settle(0, Write::File(path, doc.to_string().into_bytes()));
        }
        Ok(plan)
    }
}

// ------------------------------------------------------------------------------------------
// opencode
// ------------------------------------------------------------------------------------------

/// opencode: the guard shim, in its global plugin directory.
pub struct Opencode;

/// Who wrote a file at the shim's name.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Author {
    Nobody,
    /// `charter plugin install`, with the text it holds.
    Charter(String),
    /// The retired Python charter.
    Python,
    /// Somebody else, or a file charter cannot read.
    Other,
}

impl Opencode {
    /// `<opencode config>/plugin/charter.ts`.
    fn installed_path(m: &Machine) -> PathBuf {
        m.opencode_config
            .join("plugin")
            .join(crate::opencode::FILE_NAME)
    }

    fn author(path: &Path) -> Result<Author, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Author::Nobody),
            Err(e) => return Err(format!("{} could not be read: {e}", path.display())),
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return Ok(Author::Other);
        };
        Ok(if text.starts_with(crate::opencode::MARK) {
            Author::Charter(text)
        } else if text.starts_with(crate::opencode::PYTHON_MARK) {
            Author::Python
        } else {
            Author::Other
        })
    }
}

impl Adapter for Opencode {
    fn harness(&self) -> &'static str {
        "opencode"
    }

    fn home(&self, m: &Machine) -> PathBuf {
        m.opencode_config.clone()
    }

    fn install(&self, m: &Machine) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let path = Self::installed_path(m);
        let want = crate::opencode::shim(crate::opencode::Arming::GuardOnly(&m.binary));
        let what = "charter's guard as an opencode plugin that runs this charter";
        match Self::author(&path)? {
            Author::Other => {
                return Err(format!(
                    "{} is there and charter did not write it, so it was left as it is; move it \
                     aside and run this again",
                    path.display()
                ));
            }
            Author::Charter(text) if text == want => plan.step(&path, what, false),
            Author::Python => plan.step(
                &path,
                "replace the retired Python charter's opencode shim with charter's guard",
                true,
            ),
            Author::Charter(_) | Author::Nobody => plan.step(&path, what, true),
        }
        if plan.changes() {
            plan.settle(0, Write::File(path, want.into_bytes()));
        }
        plan.notes.push(
            "Only the guard is installed for opencode: the state hooks and the briefing reach an \
             opencode chat only when the app starts it."
                .to_owned(),
        );
        Ok(plan)
    }

    fn installed(&self, m: &Machine) -> Result<bool, String> {
        Ok(matches!(
            Self::author(&Self::installed_path(m))?,
            Author::Charter(_)
        ))
    }

    fn runs(&self, m: &Machine) -> Option<PathBuf> {
        match Self::author(&Self::installed_path(m)).ok()? {
            Author::Charter(text) => crate::opencode::binary_in(&text),
            _ => None,
        }
    }

    /// The Python charter's shim, which guards and briefs every opencode chat by whichever
    /// `charter` is on `PATH`.
    fn superseded(&self, m: &Machine) -> Vec<PathBuf> {
        let path = Self::installed_path(m);
        if Self::author(&path) == Ok(Author::Python) {
            vec![path]
        } else {
            Vec::new()
        }
    }

    fn uninstall(&self, m: &Machine) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let path = Self::installed_path(m);
        let ours = matches!(Self::author(&path)?, Author::Charter(_));
        plan.step(&path, "remove charter's opencode guard", ours);
        if ours {
            plan.settle(0, Write::RemoveFile(path));
        }
        Ok(plan)
    }
}

#[cfg(test)]
mod tests;
