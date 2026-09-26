//! `charter persona lint` — a deterministic check of each persona's definition: a reference
//! that names no persona, a key charter cannot read, a missing role, vault or
//! `delegate-when`, an unfinished draft, a `bin/` file that cannot run, an MCP server that
//! cannot be declared or will run without its credential, a skill no sub-agent can invoke,
//! and a generated sub-agent that no longer matches its persona. A port of
//! `commands_persona.cmd_persona_lint` and `persona.lint`, `structural_errors`,
//! `key_issues`, `bin_issues` and the skill checks beneath them.
//!
//! [`Linter`] is the one implementation, and three readers ask it: this command, and the
//! doctor's `personas` and `persona grant` rows. A doctor that linted in its own words would
//! be a second answer to "is this persona well-formed", and the two would drift the first
//! time a check was added to only one of them.
//!
//! # Skills are looked for where the harness looks
//!
//! A skill a persona declares (`skills:`) or its charter names for agent use (`` `plugin:skill` ``)
//! is checked against `~/.claude/plugins`, `~/.claude/skills` and the plane's own
//! `.claude/skills` — the default config folder's, never one harness profile's, because a
//! persona is declared once for every chat on the plane. With no plugin cache at all the
//! skill checks are skipped rather than failed: charter cannot see a plugin's skills then,
//! and calling every one of them missing would be confidently wrong.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::repocmd::{Say, Sink};

use super::mcp;

/// How bad a finding is: an error makes `lint` exit 1, a warning does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
}

/// One finding about one persona.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub level: Level,
    pub message: String,
}

impl Issue {
    fn error(message: impl Into<String>) -> Self {
        Self {
            level: Level::Error,
            message: message.into(),
        }
    }

    fn warn(message: impl Into<String>) -> Self {
        Self {
            level: Level::Warn,
            message: message.into(),
        }
    }
}

/// `persona.EMPTY_REFERENCE`.
const EMPTY_REFERENCE: &str = "is empty — remove the key, or name a persona";

/// A persona's definition read against one plane: the roster and the installed skills are
/// read once per `Linter`, however many personas it is asked about.
pub struct Linter<'a> {
    root: &'a Path,
    state: &'a Path,
    home: Option<PathBuf>,
    known: BTreeSet<String>,
    skills: OnceLock<Option<BTreeMap<String, bool>>>,
}

impl<'a> Linter<'a> {
    /// A linter for the personas under `root`, whose MCP approvals and vault registry live
    /// in `state`, looking for skills under `home`'s `.claude` (the process's `$HOME` when
    /// `None` is not asked for — see [`Linter::with_home`]).
    pub fn new(root: &'a Path, state: &'a Path) -> Self {
        Self {
            root,
            state,
            home: crate::profiles::home(),
            known: super::names(root).into_iter().collect(),
            skills: OnceLock::new(),
        }
    }

    /// This linter, looking for installed skills under `home` rather than `$HOME`.
    pub fn with_home(mut self, home: Option<PathBuf>) -> Self {
        self.home = home;
        self
    }

    /// Every persona this plane defines, by name — what a bare `lint` walks.
    pub fn names(&self) -> Vec<String> {
        self.known.iter().cloned().collect()
    }

    /// Everything wrong with `name`'s definition — `persona.lint`, and the generated
    /// sub-agent's drift (`_agent_sync_issues`) after it.
    pub fn lint(&self, name: &str) -> Vec<Issue> {
        let mut issues = self.definition(name);
        issues.extend(self.agent_sync(name));
        issues
    }

    /// `persona.lint`: the definition alone, without the generated sub-agent's drift — what
    /// the doctor's `personas` row counts.
    pub fn definition(&self, name: &str) -> Vec<Issue> {
        let root = self.root;
        let Some((pairs, charter)) = crate::personas::load_with_charter(root, name) else {
            let why = definition_refusal(root, name).or_else(|| read_refusal(root, name));
            return vec![Issue::error(match why {
                Some(why) => format!("persona.md: {why}"),
                None => format!("persona '{}' does not load", crate::shown::short(name)),
            })];
        };
        let meta: BTreeMap<&str, &str> = pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let mut issues = self.declared_skill_issues(name);
        if meta.get("role").is_none_or(|r| r.is_empty()) {
            issues.push(Issue::warn("no role"));
        }
        let vault = super::vault(root, self.state, name);
        if matches!(
            vault,
            crate::personas::Vault::Unnamed | crate::personas::Vault::RegistryUnreadable(_)
        ) {
            issues.push(Issue::warn(format!(
                "no vault named — add `vault:` or `vault: {}` if this persona holds no \
                 credentials",
                super::NO_VAULT
            )));
        }
        if meta
            .get("delegate-when")
            .is_none_or(|d| crate::memstore::py_strip(d).is_empty())
        {
            issues.push(Issue::warn("no delegate-when → weak auto-routing"));
        }
        if super::is_draft(root, name) {
            issues.push(Issue::warn(
                "draft: true → charter unfinished; no sub-agent is generated and it cannot be \
                 dispatched. Finish the charter, drop the line, then `charter persona \
                 sync-agents`",
            ));
        }
        // A key charter neither reads nor emits does nothing — a warning, because a harness's
        // own field is a legitimate thing to carry. A key that is a known one in another case
        // is the error `structural_errors` names, and is not said twice.
        for key in meta.keys() {
            if !crate::personagrant::known_key(key)
                && crate::personagrant::misspelled_key(key).is_none()
            {
                issues.push(Issue::warn(format!(
                    "frontmatter key '{}' is neither read by charter nor emitted into the \
                     sub-agent — it does nothing (typo?)",
                    crate::shown::short(key)
                )));
            }
        }
        issues.extend(self.structural_errors(name));
        issues.extend(bin_issues(root, name));
        issues.extend(self.skill_ref_issues(&charter));
        issues.extend(self.mcp_issues(name));
        issues
    }

    /// `persona.structural_errors`: the error half — a key charter cannot honour, and a
    /// reference the resolver cannot build the persona from.
    pub fn structural_errors(&self, name: &str) -> Vec<Issue> {
        let root = self.root;
        let mut issues: Vec<Issue> = super::agents::key_issues(root, name)
            .into_iter()
            .map(Issue::error)
            .collect();
        if let Some(refused) = definition_refusal(root, name) {
            issues.push(Issue::error(format!("persona.md: {refused}")));
        }
        for used in crate::personagrant::uses_of(root, name) {
            issues.extend(self.reference_problem("uses", &used));
        }
        for borrowed in crate::personagrant::borrows_of(root, name).unwrap_or_default() {
            issues.extend(self.reference_problem("borrows", &borrowed));
        }
        let parent = crate::personas::load(root, name)
            .and_then(|pairs| {
                pairs
                    .into_iter()
                    .rev()
                    .find(|(k, _)| k == "extends")
                    .map(|(_, v)| v)
            })
            .map(|v| crate::memstore::py_strip(&v).to_string())
            .unwrap_or_default();
        if !parent.is_empty() {
            match self.reference_problem("extends", &parent) {
                Some(problem) => issues.push(problem),
                None => {
                    if let Some(broken) = crate::personas::ancestor_that_does_not_load(root, name) {
                        issues.push(Issue::error(format!(
                            "extends: persona '{broken}' does not load from {} (see why: \
                             charter persona lint {broken})",
                            super::def_rel(root, &broken)
                        )));
                    }
                }
            }
        }
        if let Some(cycle) = inherits_cycle(root, name) {
            issues.push(Issue::error(format!(
                "extends: inheritance cycle ({cycle})"
            )));
        }
        issues
    }

    /// `_reference_problem`: why `reference`, read out of `field`, names no persona.
    fn reference_problem(&self, field: &str, reference: &str) -> Option<Issue> {
        if let Some(refused) = reference_refusal(reference) {
            return Some(Issue::error(format!("{field}: {refused}")));
        }
        (!self.known.contains(reference)).then(|| {
            Issue::error(format!(
                "{field}: '{reference}' — no such persona (dangling)"
            ))
        })
    }

    /// The MCP rows: a server name that is refused, a credential with no vault to hold it,
    /// and a credential this machine withholds.
    fn mcp_issues(&self, name: &str) -> Vec<Issue> {
        let (servers, refused) = mcp::declared(self.root, name);
        let mut issues: Vec<Issue> = refused
            .iter()
            .map(|bad| {
                Issue::error(format!(
                    "mcp: server name '{}' is refused and the server is not declared — a name \
                     is emitted into the generated agent's YAML and into `mcp__<server>__*`, \
                     so it may hold only letters, digits, '_', '.' and '-' (64 max). Rename it \
                     in `{}`",
                    mcp::label(&[bad]),
                    mcp::MCP_FILE
                ))
            })
            .collect();
        if !servers.is_empty() {
            let vault =
                super::resolve(self.root, name).and_then(|r| mcp::vault_for(r.get("vault")));
            let mut wants: Vec<&String> = servers
                .iter()
                .filter(|(_, entry)| mcp::declares_credential(entry))
                .map(|(server, _)| server)
                .collect();
            wants.sort();
            if !wants.is_empty() && vault.is_none() {
                issues.push(Issue::error(format!(
                    "mcp: server(s) {} declare `secrets` or `secret_files` but this persona \
                     names no vault — add `vault:` or drop the declaration",
                    wants
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
        for (server, line) in mcp::withheld(self.root, self.state, name) {
            let shown = mcp::label(&[&server]);
            issues.push(if line.is_empty() {
                Issue::error(format!(
                    "mcp: '{shown}' declares a credential and {} — it can never be approved, \
                     so the vault is withheld permanently. Fix the entry in `{}`",
                    mcp::UNRENDERABLE,
                    mcp::MCP_FILE
                ))
            } else {
                Issue::warn(format!(
                    "mcp: '{shown}' declares a credential this machine has not approved, so \
                     the generated sub-agent runs it WITHOUT the vault and it will fail to \
                     authenticate. Read the command and approve it with `charter persona \
                     sync-agents --approve-mcp`, or drop `secrets`/`secret_files` from `{}` \
                     if it should hold no credential",
                    mcp::MCP_FILE
                ))
            });
        }
        issues
    }

    /// `_agent_sync_issues`: is `.claude/agents/<name>.md` what `sync-agents` would write?
    fn agent_sync(&self, name: &str) -> Vec<Issue> {
        let Some(def) = super::resolve(self.root, name) else {
            return Vec::new();
        };
        if super::is_draft(self.root, name) {
            return Vec::new();
        }
        let path = super::agents::agents_dir(self.root).join(format!("{name}.md"));
        let Ok(current) = std::fs::read_to_string(&path) else {
            return if path.exists() {
                Vec::new()
            } else {
                vec![Issue::warn(
                    "no generated sub-agent — run `charter persona sync-agents`",
                )]
            };
        };
        if !current.contains(super::agents::MARKER) {
            return Vec::new();
        }
        let rendered = super::agents::render(self.root, self.state, name, &def);
        if crate::memstore::py_strip(&current) != crate::memstore::py_strip(&rendered) {
            return vec![Issue::warn(
                "generated sub-agent is stale — run `charter persona sync-agents`",
            )];
        }
        Vec::new()
    }

    /// Every installed skill by name, and whether a model may invoke it — `None` when this
    /// machine has no plugin cache to look in.
    fn installed_skills(&self) -> Option<&BTreeMap<String, bool>> {
        self.skills
            .get_or_init(|| {
                let home = self.home.as_ref()?.join(".claude");
                let roots = [
                    home.join("plugins"),
                    home.join("skills"),
                    self.root.join(".claude").join("skills"),
                ];
                if !roots[0].exists() {
                    return None;
                }
                let mut out: BTreeMap<String, bool> = BTreeMap::new();
                for root in roots.iter().filter(|r| r.exists()) {
                    let mut found = Vec::new();
                    skill_files(root, &mut found, 0);
                    found.sort();
                    for file in found {
                        if let Some((name, invokable)) = read_skill(&file) {
                            let was = out.get(&name).copied().unwrap_or(false);
                            out.insert(name, was || invokable);
                        }
                    }
                }
                (!out.is_empty()).then_some(out)
            })
            .as_ref()
    }

    /// `declared_skill_issues`: each `skills:` entry is installed and a model may invoke it.
    fn declared_skill_issues(&self, name: &str) -> Vec<Issue> {
        let declared = super::declared_skills(self.root, name);
        if declared.is_empty() {
            return Vec::new();
        }
        let Some(skills) = self.installed_skills() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for reference in declared {
            let leaf = reference
                .split_once(':')
                .map_or(reference.as_str(), |(_, l)| l);
            match skills.get(leaf) {
                None => out.push(Issue::error(format!(
                    "declares skill `{reference}` — not found in ~/.claude/plugins, \
                     ~/.claude/skills or this plane's .claude/skills. It is preloaded into the \
                     agent, so this fails silently at dispatch. Install it, generate it, or \
                     drop the entry"
                ))),
                Some(false) => out.push(Issue::warn(format!(
                    "declares skill `{reference}`, which is human-only \
                     (disable-model-invocation) — preloading its text is harmless but the agent \
                     can never invoke it"
                ))),
                Some(true) => {}
            }
        }
        out
    }

    /// `_skill_ref_issues`: every `` `plugin:skill` `` the charter names, for a plugin the
    /// plane enables, is installed and model-invokable.
    fn skill_ref_issues(&self, charter: &str) -> Vec<Issue> {
        let Some(skills) = self.installed_skills() else {
            return Vec::new();
        };
        let plugins = enabled_plugins(self.root);
        if plugins.is_empty() {
            return Vec::new();
        }
        let pattern = regex::Regex::new(r"`([a-z0-9][a-z0-9-]*):([a-z0-9][a-z0-9-]*)`")
            .expect("the pattern compiles");
        let mut out = Vec::new();
        for found in pattern.captures_iter(charter) {
            let (plugin, skill) = (&found[1], &found[2]);
            if !plugins.contains(plugin) {
                continue;
            }
            match skills.get(skill) {
                None => out.push(Issue::error(format!(
                    "charter names `{plugin}:{skill}` — not an installed skill \
                     (renamed/removed upstream?); fix it or pin the marketplace"
                ))),
                Some(false) => out.push(Issue::error(format!(
                    "charter names `{plugin}:{skill}` for agent use, but it's human-only \
                     (disable-model-invocation) — a sub-agent can't invoke it; use the /slash \
                     form for a human step"
                ))),
                Some(true) => {}
            }
        }
        out
    }
}

/// `persona.definition_refusal`: the definition, or its directory, resolves out of the plane.
fn definition_refusal(root: &Path, name: &str) -> Option<String> {
    let path = crate::personas::def_path(root, name);
    if !path.exists() {
        return None;
    }
    let parent = path.parent()?;
    crate::contain::readable(root, parent)
        .err()
        .or_else(|| crate::contain::readable(root, &path).err())
        .map(|refused| refused.to_string())
}

/// `persona.read_refusal`: the definition is there and charter may read it, and it does not
/// come back as text.
fn read_refusal(root: &Path, name: &str) -> Option<String> {
    let path = crate::personas::def_path(root, name);
    if !path.exists() || definition_refusal(root, name).is_some() {
        return None;
    }
    let shown = crate::shown::short(&path.display().to_string());
    match std::fs::read(&path) {
        Err(e) => Some(format!("'{shown}' cannot be read ({e})")),
        Ok(bytes) => std::str::from_utf8(&bytes).err().map(|e| {
            format!(
                "'{shown}' is not utf-8 text (invalid utf-8 at offset {})",
                e.valid_up_to()
            )
        }),
    }
}

/// `persona.reference_refusal`: why `reference`, read from a committed file, cannot name a
/// persona — a path, or a name outside the alphabet charter mints.
fn reference_refusal(reference: &str) -> Option<String> {
    if reference.is_empty() {
        return Some(EMPTY_REFERENCE.to_string());
    }
    let shown = crate::shown::short(reference);
    if !crate::contain::segment_ok(reference) {
        return Some(format!(
            "'{shown}' is not a name — it is a path. This is read from a committed file and \
             joined onto a directory, so it may name one entry there and nothing else: no '/', \
             no '\\', no '.' or '..', and nothing absolute"
        ));
    }
    if crate::personas::valid_name(reference) {
        return None;
    }
    Some(format!(
        "'{shown}' is not a persona name{}. Charter mints these itself — `persona create` \
         enforces exactly this — so a reference can only name one: a lowercase letter or digit \
         first, then lowercase letters, digits, '.', '_' or '-'",
        alphabet_detail(reference)
    ))
}

/// `persona._alphabet_detail`: which part of the alphabet `reference` broke.
fn alphabet_detail(reference: &str) -> String {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-_".contains(c);
    let bad: BTreeSet<char> = reference.chars().filter(|c| !allowed(*c)).collect();
    if !bad.is_empty() && bad.iter().all(|c| matches!(c, '"' | '\'')) {
        return " — the quotes are part of the value. charter's frontmatter parser does not \
                strip them, so `extends: \"parent\"` asks for a persona whose name includes \
                the quote marks; remove them"
            .to_string();
    }
    if !bad.is_empty() {
        return format!(
            " — {} cannot appear in one",
            bad.iter()
                .map(|c| crate::pyrepr::repr_str(&c.to_string()))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    match reference.chars().next() {
        Some('_') => " — a leading '_' is reserved for the shared namespace".to_string(),
        Some(first) => format!(
            " — it starts with {}",
            crate::pyrepr::repr_str(&first.to_string())
        ),
        None => String::new(),
    }
}

/// `persona._inherits_cycle`: the loop `extends:` makes, as `a → b → a`.
fn inherits_cycle(root: &Path, name: &str) -> Option<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut current = name.to_string();
    loop {
        if let Some(at) = seen.iter().position(|s| *s == current) {
            let mut path = seen[at..].to_vec();
            path.push(current);
            return Some(path.join(" → "));
        }
        seen.push(current.clone());
        let pairs = crate::personas::load(root, &current)?;
        let parent = pairs
            .into_iter()
            .rev()
            .find(|(k, _)| k == "extends")
            .map(|(_, v)| crate::memstore::py_strip(&v).to_string())
            .filter(|v| !v.is_empty())?;
        current = parent;
    }
}

/// `persona.bin_issues`: a file in the persona's own `bin/` that cannot run.
fn bin_issues(root: &Path, name: &str) -> Vec<Issue> {
    let dir = root
        .join("personas")
        .join(name)
        .join(crate::personagrant::BIN_DIR);
    let Ok(reader) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = reader
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    files
        .into_iter()
        .filter(|f| !crate::personagrant::is_executable_file(f))
        .map(|f| {
            let base = f.file_name().unwrap_or_default().to_string_lossy();
            Issue::warn(format!(
                "`{}/{base}` is not executable — it will fail at the moment it is needed; \
                 `chmod +x {}`",
                crate::personagrant::BIN_DIR,
                super::rel(root, &f)
            ))
        })
        .collect()
}

/// The plugin names `.claude/settings.json` enables, the part before `@`.
fn enabled_plugins(root: &Path) -> BTreeSet<String> {
    let path = root.join(".claude").join("settings.json");
    if !crate::contain::within_plane(root, &path) {
        return BTreeSet::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("enabledPlugins").and_then(|p| p.as_object()).cloned())
        .map(|plugins| {
            plugins
                .keys()
                .map(|k| k.split('@').next().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Every `SKILL.md` under `dir`, links not followed and the walk bounded.
fn skill_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 12 {
        return;
    }
    let Ok(reader) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in reader.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            skill_files(&path, out, depth + 1);
        } else if kind.is_file() && entry.file_name() == "SKILL.md" {
            out.push(path);
        }
    }
}

/// A `SKILL.md`'s name (its `name:`, else its directory's) and whether a model may invoke it.
fn read_skill(file: &Path) -> Option<(String, bool)> {
    let text = std::fs::read_to_string(file).ok()?;
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return None;
    }
    let mut name: Option<String> = None;
    let mut human_only = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(value.trim().trim_matches(['"', '\'']).to_string());
        } else if let Some(value) = line.strip_prefix("disable-model-invocation:") {
            human_only = value.trim().eq_ignore_ascii_case("true");
        }
    }
    let name = name.filter(|n| !n.is_empty()).or_else(|| {
        file.parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
    })?;
    Some((name, !human_only))
}

/// `charter persona lint [name] [--only KEY]`, and its exit code: 1 when any finding is an
/// error (every finding is, under `--only`).
pub fn lint_command(linter: &Linter, name: Option<&str>, only: Option<&str>, say: Sink) -> u8 {
    let name = name.filter(|n| !n.is_empty());
    if let Some(name) = name
        && !crate::personas::valid_name(name)
    {
        say(Say::Fail(
            crate::personas::name_refusal(linter.root, name).unwrap_or_default(),
        ));
        return 1;
    }
    let names = match name {
        Some(name) => vec![name.to_string()],
        None => linter.names(),
    };
    if names.is_empty() {
        say(Say::Info("No personas to lint.".into()));
        return 0;
    }
    let only = only
        .map(crate::memstore::py_strip)
        .filter(|o| !o.is_empty());
    let mut errors = 0;
    for name in &names {
        let shown = crate::shown::short(name);
        let mut issues = linter.lint(name);
        if let Some(only) = only {
            issues = issues
                .into_iter()
                .filter(|i| i.message.contains(only))
                .map(|i| Issue::error(i.message))
                .collect();
        }
        if issues.is_empty() {
            say(Say::Done(format!("{shown}: ok")));
            continue;
        }
        for issue in issues {
            match issue.level {
                Level::Error => {
                    errors += 1;
                    say(Say::Fail(format!("{shown}: {}", issue.message)));
                }
                Level::Warn => say(Say::Warn(format!("{shown}: {}", issue.message))),
            }
        }
    }
    if errors > 0 {
        say(Say::Fail(format!(
            "{errors} error(s) — dangling reuse or unloadable persona."
        )));
        return 1;
    }
    0
}

#[cfg(test)]
#[path = "lint_tests.rs"]
mod tests;
