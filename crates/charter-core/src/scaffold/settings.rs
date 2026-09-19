//! The three things `init` puts in harness settings files the plane commits, each only when
//! it is absent: `$CHARTER_HARNESS` in `.claude/settings.json`'s `env`, the ask rule for
//! `charter handoff *` in `.claude/settings.json` and `opencode.json`, and the plane-root
//! guard hook in `.claude/settings.json`.
//!
//! A port of `charter/commands.py`'s `ensure_env_var`, `add_permission_rule`,
//! `_ensure_guard_hook`, and `harness/opencode.py:_apply_rule`, with the restraint they
//! share: **these files are the operator's**. charter touches only the key it owns, only
//! when it is missing, and a file it cannot read the way the harness reads it is left
//! completely alone rather than "repaired" — a repair is a rewrite wearing a helpful word.
//!
//! Every rewrite goes through [`pyjson::dumps`] in the file's own layout
//! ([`pyjson::json_style`]), because Python re-dumps the whole document and a writer that
//! differed by one byte would leave a different file for the same command.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::pyjson;

use super::text;

/// `.claude/settings.json`, under the plane.
pub const SETTINGS: &str = ".claude/settings.json";

/// `opencode.json`, under the plane.
pub const OPENCODE: &str = "opencode.json";

/// The pattern a handoff's consent rule names (`commands.HANDOFF_ASK_PATTERN`).
pub const HANDOFF_PATTERN: &str = "charter handoff *";

/// The same pattern as Claude Code's rule syntax (`commands._as_rule`).
pub const HANDOFF_RULE: &str = "Bash(charter handoff *)";

/// The one hook charter wires itself (`commands._GUARD_HOOK`).
pub fn guard_hook() -> Value {
    json!({
        "matcher": "Bash",
        "hooks": [{"type": "command", "command": "charter hook pretooluse", "timeout": 10}],
    })
}

/// The exact JSON printed for a person whose settings file charter could not touch
/// (`commands._hooks_snippet`).
pub fn hooks_snippet() -> String {
    pyjson::dumps(
        &json!({"hooks": {"PreToolUse": [guard_hook()]}}),
        Some("  "),
        ",",
        ": ",
    )
}

/// What a writer did. The words are charter's statuses, so a caller can report them in the
/// same buckets Python does.
#[derive(Debug)]
pub enum Wrote {
    /// Written now.
    Created,
    /// Already there; nothing written.
    Present,
    /// The file is not one charter can read and write back; nothing written. The string is
    /// what Python puts in its message: the path, and for a wrong-typed key, which one.
    Malformed(String),
    /// `.claude` is there and is not a directory.
    Blocked(PathBuf),
    /// The write itself failed after every check passed.
    Failed(PathBuf, std::io::Error),
}

/// A settings document, as `commands._load_settings` reads one: a missing file is an empty
/// object, and one that does not parse as `JSON.parse` parses — or is not an object — is
/// `None`, never repaired. `raw` is the text it was read from, empty for a missing file.
struct Doc {
    map: Option<Map<String, Value>>,
    raw: String,
}

fn read(path: &Path) -> Doc {
    if !path.exists() {
        return Doc {
            map: Some(Map::new()),
            raw: String::new(),
        };
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Doc {
            map: None,
            raw: String::new(),
        };
    };
    let map = match pyjson::loads_strict(&raw) {
        Some(Value::Object(map)) => Some(map),
        _ => None,
    };
    Doc { map, raw }
}

/// Write `map` in the layout `raw` has, `\n` after it when `raw` ended with one — or always,
/// when `always_newline` says the writer adds one regardless (`add_permission_rule` does).
fn render(
    map: Map<String, Value>,
    raw: &str,
    fresh_indent: Option<&str>,
    always_newline: bool,
) -> String {
    let (indent, item, key) = if raw.is_empty() {
        match fresh_indent {
            Some(pad) => (Some(pad.to_owned()), ",".to_owned(), ": ".to_owned()),
            None => pyjson::json_style(raw),
        }
    } else {
        pyjson::json_style(raw)
    };
    let mut text = pyjson::dumps(&Value::Object(map), indent.as_deref(), &item, &key);
    if always_newline || raw.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn write(path: &Path, text: &str) -> Result<(), Wrote> {
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return Err(if dir.exists() && !dir.is_dir() {
            Wrote::Blocked(dir.to_path_buf())
        } else {
            Wrote::Failed(path.to_path_buf(), e)
        });
    }
    std::fs::write(path, text).map_err(|e| Wrote::Failed(path.to_path_buf(), e))
}

/// `commands.ensure_env_var`: set `env[key]` IF ABSENT.
///
/// An `env` that is not an object, or a key somebody set by hand, is `Present` — reverting a
/// deliberate choice is what these writers exist not to do. A key holding a FALSY value
/// (`""`, `0`, `null`) is not set by hand in Python's eyes and is replaced, in place.
pub fn ensure_env(root: &Path, key: &str, value: &str) -> Wrote {
    let path = root.join(SETTINGS);
    let doc = read(&path);
    let Some(mut map) = doc.map else {
        return Wrote::Malformed(path.display().to_string());
    };
    let mut env = match map.get("env") {
        Some(Value::Object(env)) => env.clone(),
        Some(_) => return Wrote::Present,
        None => Map::new(),
    };
    if env.get(key).is_some_and(text::truthy) {
        return Wrote::Present;
    }
    env.insert(key.to_owned(), Value::String(value.to_owned()));
    map.insert("env".to_owned(), Value::Object(env));
    match write(&path, &render(map, &doc.raw, None, false)) {
        Ok(()) => Wrote::Created,
        Err(wrote) => wrote,
    }
}

/// `commands.add_permission_rule(root, rule, "ask")`: append `rule` to `permissions.ask`.
///
/// `dry_run` is the write path minus the write: every refusal is reached exactly as the write
/// would reach it, which is what lets the handoff gate ask every file before writing any.
pub fn ensure_ask_rule(root: &Path, rule: &str, dry_run: bool) -> Wrote {
    let path = root.join(SETTINGS);
    let doc = read(&path);
    let Some(mut map) = doc.map else {
        return Wrote::Malformed(path.display().to_string());
    };
    let mut perms = match map.get("permissions") {
        Some(Value::Object(perms)) => perms.clone(),
        Some(_) => {
            return Wrote::Malformed(format!(
                "{} (`permissions` is not an object)",
                path.display()
            ));
        }
        None => Map::new(),
    };
    let mut entries = match perms.get("ask") {
        Some(Value::Array(entries)) => entries.clone(),
        Some(_) => {
            return Wrote::Malformed(format!(
                "{} (`permissions.ask` is not a list)",
                path.display()
            ));
        }
        None => Vec::new(),
    };
    if entries.iter().any(|e| e.as_str() == Some(rule)) {
        return Wrote::Present;
    }
    entries.push(Value::String(rule.to_owned()));
    perms.insert("ask".to_owned(), Value::Array(entries));
    map.insert("permissions".to_owned(), Value::Object(perms));
    let text = render(map, &doc.raw, Some("  "), true);
    if dry_run {
        return Wrote::Created;
    }
    match write(&path, &text) {
        Ok(()) => Wrote::Created,
        Err(wrote) => wrote,
    }
}

/// `harness/opencode.py:_apply_rule(root, "charter handoff *", "ask")`.
///
/// Read with Python's lenient `json.loads` there, where the Claude Code file is read
/// strictly — and `serde_json` is strict. A `NaN` in `opencode.json` is therefore a file
/// Python rewrites and this refuses, which is the direction that loses nothing.
///
/// **An existing `allow` for the pattern is turned into `ask`**, as Python does: the check
/// is "is the decision already `ask`", not "is the pattern mentioned".
pub fn ensure_opencode_ask(root: &Path, glob: &str, dry_run: bool) -> Wrote {
    let path = root.join(OPENCODE);
    let mut map = if path.exists() {
        let parsed = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| pyjson::loads_strict(&raw));
        match parsed {
            Some(Value::Object(map)) => map,
            _ => return Wrote::Malformed(path.display().to_string()),
        }
    } else {
        Map::new()
    };
    let mut perms = match map.get("permission") {
        Some(Value::Object(perms)) => perms.clone(),
        Some(_) => {
            return Wrote::Malformed(format!(
                "{} (`permission` is not an object)",
                path.display()
            ));
        }
        None => Map::new(),
    };
    let mut block = match perms.get("bash") {
        Some(Value::Object(block)) => block.clone(),
        Some(_) => {
            return Wrote::Malformed(format!(
                "{} (`permission.bash` is not an object)",
                path.display()
            ));
        }
        None => Map::new(),
    };
    if block.get(glob).and_then(Value::as_str) == Some("ask") {
        return Wrote::Present;
    }
    block.insert(glob.to_owned(), Value::String("ask".to_owned()));
    perms.insert("bash".to_owned(), Value::Object(block));
    map.insert("permission".to_owned(), Value::Object(perms));
    let text = pyjson::dumps_indent2(&Value::Object(map));
    if dry_run {
        return Wrote::Created;
    }
    match write(&path, &text) {
        Ok(()) => Wrote::Created,
        Err(wrote) => wrote,
    }
}

/// `commands._ensure_guard_hook`: add the guard to `hooks.PreToolUse` IF no hook Claude Code
/// would run already dispatches it — and not at all when an enabled plugin already does.
///
/// **Where Python crashes, this refuses.** `"hooks": null`, or a `PreToolUse` that is
/// `false`, `0`, `""` or `{}`, reaches an `.append` on something that is not a list there
/// and ends in a traceback. Here each is `Malformed`: a file charter cannot extend is left
/// as it is, which is the rule for every other shape it does not understand.
pub fn ensure_guard_hook(root: &Path, home: Option<&Path>) -> Wrote {
    let path = root.join(SETTINGS);
    if plugin_dispatches_guard(root, home).is_some() {
        return Wrote::Present;
    }
    if !path.exists() {
        let doc = json!({"hooks": {"PreToolUse": [guard_hook()]}});
        return match write(&path, &pyjson::dumps_indent2(&doc)) {
            Ok(()) => Wrote::Created,
            Err(wrote) => wrote,
        };
    }
    let doc = read(&path);
    let malformed = || Wrote::Malformed(path.display().to_string());
    let Some(mut map) = doc.map else {
        return malformed();
    };
    let mut hooks = match map.get("hooks") {
        None => Map::new(),
        Some(Value::Object(hooks)) => hooks.clone(),
        Some(_) => return malformed(),
    };
    let mut pre = match hooks.get("PreToolUse") {
        None => Vec::new(),
        Some(Value::Array(pre)) => pre.clone(),
        Some(_) => return malformed(),
    };
    if guard_runs_in(&pre) {
        return Wrote::Present;
    }
    pre.push(guard_hook());
    hooks.insert("PreToolUse".to_owned(), Value::Array(pre));
    map.insert("hooks".to_owned(), Value::Object(hooks));
    match write(&path, &render(map, &doc.raw, None, false)) {
        Ok(()) => Wrote::Created,
        Err(wrote) => wrote,
    }
}

/// `doctor._guard_runs_in`: does any hook group run `charter hook pretooluse`? Every level
/// is checked for its type, because every level is a line a chat can write.
pub fn guard_runs_in(groups: &[Value]) -> bool {
    groups.iter().any(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    entry.get("type").and_then(Value::as_str) == Some("command")
                        && entry
                            .get("command")
                            .and_then(Value::as_str)
                            .is_some_and(|c| handlers(c).iter().any(|h| h == "pretooluse"))
                })
            })
    })
}

/// Every `<name>` that `hooks._HOOK_CMD_RE` — `\bcharter\s+hook\s+([A-Za-z0-9_-]+)` — finds
/// in `command`, left to right.
///
/// Hand-rolled to that regex's letter: `charter` must start a word (so `xcharter` does not
/// count and `/usr/bin/charter` does), the gaps are Python's `\s` (wider than Rust's
/// whitespace), and the name is the longest run of `[A-Za-z0-9_-]` — so
/// `charter hook pretooluse-read` names `pretooluse-read`, which is a different handler.
pub fn handlers(command: &str) -> Vec<String> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let space = crate::memstore::is_python_space;
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = command[from..].find("charter") {
        let at = from + found;
        from = at + 1;
        if command[..at].chars().next_back().is_some_and(word) {
            continue;
        }
        let rest = &command[at + "charter".len()..];
        let gap = rest.len() - rest.trim_start_matches(space).len();
        if gap == 0 {
            continue;
        }
        let Some(rest) = rest[gap..].strip_prefix("hook") else {
            continue;
        };
        let gap = rest.len() - rest.trim_start_matches(space).len();
        if gap == 0 {
            continue;
        }
        let name: String = rest[gap..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if name.is_empty() {
            continue;
        }
        from = command.len() - rest[gap + name.len()..].len();
        out.push(name);
    }
    out
}

/// `commands._plugin_dispatches_guard`: the enabled plugin whose own `hooks.json` runs
/// `charter hook pretooluse`, or `None`.
///
/// Installed, enabled and wired are three different states and only the third protects
/// anything, so all three are asked. The config folder is `~/.claude` whatever
/// `$CLAUDE_CONFIG_DIR` says (#969): the file being written is the plane's COMMITTED
/// settings, which the sessions of every folder read, and it must not change with one
/// person's shell.
///
/// Every failure to read answers "none charter can see", and that is the safe direction
/// rather than a claim: the guard hook is then written, and a guard declared twice runs
/// twice and is reported, where a guard declared nowhere is a hole.
pub fn plugin_dispatches_guard(root: &Path, home: Option<&Path>) -> Option<String> {
    let folder = home?.join(".claude");
    let enabled = enabled_plugins(root, &folder);
    if enabled.is_empty() {
        return None;
    }
    let installs = installed_plugins(&folder)?;
    installs.into_iter().find_map(|(id, paths)| {
        (enabled.contains(&id) && paths.iter().any(|p| dispatches_guard(p))).then_some(id)
    })
}

/// `doctor._settings_files` for `root`, with `folder` as the user half: the plane's two
/// settings files, the repository root's local file when the plane is not that root, and
/// the folder's own `settings.json` — each once, by resolved path.
fn settings_files(root: &Path, folder: &Path) -> Vec<PathBuf> {
    let mut files = vec![
        root.join(SETTINGS),
        root.join(".claude/settings.local.json"),
    ];
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if let Some(top) = local_settings_root(root)
        && top != canon(root)
    {
        files.push(top.join(".claude/settings.local.json"));
    }
    files.push(folder.join("settings.json"));
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for file in files {
        let key = canon(&file);
        if !seen.contains(&key) {
            seen.push(key);
            out.push(file);
        }
    }
    out
}

/// `doctor._local_settings_root`: where Claude Code keeps the local settings for a session at
/// `here` — the git common directory's parent, or the toplevel when the git directory lives
/// elsewhere. `None` outside a repository and at the home directory.
fn local_settings_root(here: &Path) -> Option<PathBuf> {
    use crate::worktree::git;
    let common = git::run(
        here,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        git::READ,
    )
    .ok()?;
    if !common.ok() {
        return None;
    }
    let common = PathBuf::from(common.line());
    let top = if common.file_name().is_some_and(|n| n == ".git") {
        common.parent()?.to_path_buf()
    } else {
        let top = git::run(here, &["rev-parse", "--show-toplevel"], git::READ).ok()?;
        PathBuf::from(top.line())
    };
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let top = canon(&top);
    let home = crate::profiles::home().map(|h| canon(&h));
    (Some(&top) != home.as_ref()).then_some(top)
}

/// `doctor._enabled_plugin_ids`: every plugin id a settings file in force enables. An
/// `enabledPlugins` that is not an object enables nothing from that file.
fn enabled_plugins(root: &Path, folder: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for file in settings_files(root, folder) {
        let Some(Value::Object(doc)) = std::fs::read_to_string(&file)
            .ok()
            .and_then(|raw| pyjson::loads_strict(&raw))
        else {
            continue;
        };
        if let Some(Value::Object(enabled)) = doc.get("enabledPlugins") {
            for (id, on) in enabled {
                if text::truthy(on) && !out.contains(id) {
                    out.push(id.clone());
                }
            }
        }
    }
    out
}

/// The scopes Claude Code 2.1.272 accepts in its install list (`doctor._INSTALL_SCOPES`).
const INSTALL_SCOPES: [&str; 4] = ["managed", "user", "project", "local"];
/// The optional fields that schema types, and refuses the whole list over when mistyped.
const OPTIONAL_STRINGS: [&str; 6] = [
    "projectPath",
    "version",
    "installedAt",
    "lastUpdated",
    "gitCommitSha",
    "resolvedVersion",
];
const OPTIONAL_BOOLEANS: [&str; 1] = ["auto"];

/// `doctor._installed_plugins`: each installed plugin id and the install paths recorded for
/// it — or `None` when charter cannot tell what Claude Code has installed. A list that does
/// not match 2.1.272's version-2 schema loads no plugin at all in Claude Code, so it
/// dispatches nothing here either.
fn installed_plugins(folder: &Path) -> Option<Vec<(String, Vec<String>)>> {
    if std::env::var_os("CLAUDE_CODE_PLUGIN_CACHE_DIR").is_some_and(|v| !v.is_empty()) {
        return None;
    }
    let raw = std::fs::read_to_string(folder.join("plugins/installed_plugins.json")).ok()?;
    let Some(Value::Object(doc)) = pyjson::loads_strict(&raw) else {
        return None;
    };
    if doc.get("version").and_then(Value::as_f64) != Some(2.0)
        || doc.get("version").is_some_and(Value::is_boolean)
    {
        return None;
    }
    let Some(Value::Object(plugins)) = doc.get("plugins") else {
        return None;
    };
    let mut out = Vec::new();
    for (id, records) in plugins {
        let records = records.as_array()?;
        if !plugin_id_ok(id) || !records.iter().all(install_ok) {
            return None;
        }
        let paths = records
            .iter()
            .filter_map(|r| r.get("installPath").and_then(Value::as_str))
            .map(str::to_owned)
            .collect();
        out.push((id.clone(), paths));
    }
    Some(out)
}

/// `[A-Za-z0-9][-A-Za-z0-9._]*@[A-Za-z0-9][-A-Za-z0-9._]*`, whole.
fn plugin_id_ok(id: &str) -> bool {
    let side = |s: &str| {
        let mut chars = s.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
    };
    match id.split_once('@') {
        Some((name, market)) => side(name) && side(market),
        None => false,
    }
}

/// `doctor._claude_code_reads_install`.
fn install_ok(record: &Value) -> bool {
    let Some(record) = record.as_object() else {
        return false;
    };
    record
        .get("scope")
        .and_then(Value::as_str)
        .is_some_and(|s| INSTALL_SCOPES.contains(&s))
        && record.get("installPath").is_some_and(Value::is_string)
        && OPTIONAL_STRINGS
            .iter()
            .all(|k| record.get(*k).is_none_or(Value::is_string))
        && OPTIONAL_BOOLEANS
            .iter()
            .all(|k| record.get(*k).is_none_or(Value::is_boolean))
}

/// `doctor._dispatches_guard`: does the plugin installed at `install_path` run the guard
/// from its `hooks/hooks.json`? A file charter cannot read or parse dispatches nothing here.
fn dispatches_guard(install_path: &str) -> bool {
    if install_path.contains('\0') {
        return false;
    }
    let file = Path::new(install_path).join("hooks").join("hooks.json");
    let Some(doc) = std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| pyjson::loads_strict(&raw))
    else {
        return false;
    };
    doc.get("hooks")
        .and_then(Value::as_object)
        .and_then(|events| events.get("PreToolUse"))
        .and_then(Value::as_array)
        .is_some_and(|groups| guard_runs_in(groups))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verified against CPython 3.14: `hooks._HOOK_CMD_RE.findall(command)`.
    #[test]
    fn a_handler_is_named_where_pythons_pattern_names_it() {
        for (command, want) in [
            ("charter hook pretooluse", vec!["pretooluse"]),
            ("/usr/bin/charter hook pretooluse", vec!["pretooluse"]),
            ("xcharter hook pretooluse", vec![]),
            ("charter hook pretooluse-read", vec!["pretooluse-read"]),
            ("charter hook pretooluse;echo", vec!["pretooluse"]),
            (
                "cmd;charter  hook\tstop && charter hook sessionstart",
                vec!["stop", "sessionstart"],
            ),
            ("charter hookpretooluse", vec![]),
            ("charter\u{1f}hook\u{1f}pretooluse", vec!["pretooluse"]),
            ("charter hook", vec![]),
        ] {
            assert_eq!(handlers(command), want, "{command:?}");
        }
    }

    #[test]
    fn only_a_command_entry_that_runs_the_guard_counts_as_the_guard() {
        let groups: Vec<Value> = serde_json::from_str(
            r#"[{"matcher": "charter hook pretooluse", "hooks": [
                   {"type": "command", "command": "charter hook pretooluse-read"},
                   {"type": "prompt", "command": "charter hook pretooluse"}]}]"#,
        )
        .unwrap();
        assert!(!guard_runs_in(&groups));
        let wired: Vec<Value> = serde_json::from_str(
            r#"[{"hooks": [{"type": "command", "command": "charter hook pretooluse"}]}]"#,
        )
        .unwrap();
        assert!(guard_runs_in(&wired));
    }

    #[test]
    fn a_plugin_id_is_read_as_claude_codes_schema_reads_it() {
        assert!(plugin_id_ok("charter@charter"));
        assert!(plugin_id_ok("a-b.c_d@m0"));
        assert!(!plugin_id_ok("charter"));
        assert!(!plugin_id_ok("-x@y"));
        assert!(!plugin_id_ok("x@y@z"));
    }
}
