//! The things `init` puts in harness settings files the plane commits, each only when it is
//! absent: `$CHARTER_HARNESS` in `.claude/settings.json`'s `env`, the ask rules for
//! `charter report *--yes*` and `charter *todo*promote*` in `.claude/settings.json` and
//! `opencode.json`, and the plane-root guard hook in `.claude/settings.json`.
//!
//! **And one thing it takes back out** ([`handoff_rule_retired`], #1444): the ask rule for
//! `charter handoff *` that `init` wrote until a handoff became a dispatch. Consent to a
//! handoff is purlis's own dispatch grant now, so `init` writes no such rule, and the
//! `handoff-rule` fix removes the one it wrote, that exact rule and nothing near it.
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

/// The pattern the retired handoff rule names (`commands.HANDOFF_ASK_PATTERN`). **`init` no
/// longer writes it** (#1444): a handoff is a dispatch, and what consents to one is purlis's
/// own dispatch grant, asked of the person by the app, the same on every harness. Kept by
/// name for what still reads it: the fix that removes the rule `init` wrote
/// ([`handoff_rule_retired`]), `purlis guard handoff`, which names it and writes nothing, and
/// the guards, which still read a handoff where it sits.
pub const HANDOFF_PATTERN: &str = "charter handoff *";

/// The same pattern as Claude Code's rule syntax (`commands._as_rule`).
pub const HANDOFF_RULE: &str = "Bash(charter handoff *)";

/// The pattern a report's consent rule names: `charter report` with `--yes` anywhere after
/// it, which is the only spelling that files (ADR 0059, amended 2026-09-26).
pub const REPORT_PATTERN: &str = "charter report *--yes*";

/// The same pattern as Claude Code's rule syntax.
pub const REPORT_RULE: &str = "Bash(charter report *--yes*)";

/// The pattern a todo promote's consent rule names (V42): `charter`, then `todo`, then
/// `promote`, with anything between. So it holds for `ws` and `workspace`, and for a `-w` or
/// `--repo` before or after the verb, which `ws todo` reads by position. It also asks for a
/// todo whose text says "promote" after "todo"; asking once too often is the safe side of a
/// command that sends a todo's text to a forge.
pub const PROMOTE_PATTERN: &str = "charter *todo*promote*";

/// The same pattern as Claude Code's rule syntax.
pub const PROMOTE_RULE: &str = "Bash(charter *todo*promote*)";

/// Every consent rule `init` writes, as its pattern: the commands the host asks the operator
/// about before they run. The guard reads this list to refuse the same commands under a name
/// the patterns do not spell ([`crate::consentspelling`]), so the two cannot drift apart.
/// A handoff is not among them since #1444: its consent is the dispatch grant.
pub const CONSENT_PATTERNS: [&str; 2] = [REPORT_PATTERN, PROMOTE_PATTERN];

/// [`HANDOFF_PATTERN`] under the command line's new name (RN-7), retired with it (#1444).
/// `init`, `reinit` and the `rename-plane` fix write each `PURLIS_*` consent rule beside its
/// `charter` one, so either spelling of the command waits for the operator; the guard lets the
/// `purlis` spelling through only in a project that carries the rule
/// ([`carries_consent_rule`]).
pub const PURLIS_HANDOFF_PATTERN: &str = "purlis handoff *";

/// [`PURLIS_HANDOFF_PATTERN`] as Claude Code's rule.
pub const PURLIS_HANDOFF_RULE: &str = "Bash(purlis handoff *)";

/// [`REPORT_PATTERN`] under the new name.
pub const PURLIS_REPORT_PATTERN: &str = "purlis report *--yes*";

/// [`PURLIS_REPORT_PATTERN`] as Claude Code's rule.
pub const PURLIS_REPORT_RULE: &str = "Bash(purlis report *--yes*)";

/// [`PROMOTE_PATTERN`] under the new name.
pub const PURLIS_PROMOTE_PATTERN: &str = "purlis *todo*promote*";

/// [`PURLIS_PROMOTE_PATTERN`] as Claude Code's rule.
pub const PURLIS_PROMOTE_RULE: &str = "Bash(purlis *todo*promote*)";

/// [`CONSENT_PATTERNS`] under the new name, in the same order.
pub const PURLIS_CONSENT_PATTERNS: [&str; 2] = [PURLIS_REPORT_PATTERN, PURLIS_PROMOTE_PATTERN];

/// The retired handoff rule under both names the command line has: what `init` wrote, and
/// what the `handoff-rule` fix removes ([`handoff_rule_retired`]).
pub const RETIRED_HANDOFF_PATTERNS: [&str; 2] = [HANDOFF_PATTERN, PURLIS_HANDOFF_PATTERN];

/// What became of the retired handoff rule in one settings file ([`handoff_rule_retired`],
/// [`opencode_handoff_rule_retired`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Retired {
    /// The file as it should now read, or `None` where nothing in it is purlis's to remove.
    pub text: Option<String>,
    /// Each rule removed, as the file spelt it.
    pub removed: Vec<String>,
    /// Each rule that names a handoff and is **not** the one `init` wrote, as the file spells
    /// it with where it stands (`deny: Bash(purlis handoff *)`): the person's own, left alone.
    pub left: Vec<String>,
}

/// Whether a rule or a glob names the handoff command without being the exact rule `init`
/// wrote: a person's own rule about a handoff, which is named and never removed.
fn names_a_handoff(text: &str) -> bool {
    let lower = text.to_lowercase();
    crate::cliname::INSTALLED
        .iter()
        .any(|name| lower.contains(&format!("{name} handoff")))
}

/// Claude Code's `.claude/settings.json`, as `raw`, **without the handoff ask rule `init`
/// wrote** (#1444): `Bash(charter handoff *)` and `Bash(purlis handoff *)` under
/// `permissions.ask`, each exactly so and nowhere else.
///
/// **Only that exact rule.** A rule for a handoff in `deny` or `allow`, or one spelt any other
/// way (`Bash(purlis handoff:*)`, a narrower glob), is one a person wrote or edited: it stays,
/// and [`Retired::left`] names it. An `ask` list left empty stays as an empty list, and
/// nothing else in the file moves: it is rendered in its own layout, as every writer here
/// renders it.
///
/// `Err` for a file purlis cannot read the way the harness reads it, which is left as it is.
pub fn handoff_rule_retired(raw: &str) -> Result<Retired, String> {
    let Some(Value::Object(mut map)) = pyjson::loads_strict(raw) else {
        return Err(format!(
            "{SETTINGS} is not a JSON object purlis can read, so nothing in it was changed"
        ));
    };
    let wrote: Vec<String> = RETIRED_HANDOFF_PATTERNS
        .iter()
        .map(|pattern| format!("Bash({pattern})"))
        .collect();
    let mut retired = Retired::default();
    if let Some(Value::Object(perms)) = map.get_mut("permissions") {
        for (bucket, rules) in perms.iter_mut() {
            let Value::Array(rules) = rules else {
                continue;
            };
            // The one `init` wrote: that exact text, under `ask`, and nowhere else.
            let ours = |rule: &str| bucket == "ask" && wrote.iter().any(|w| w == rule);
            for rule in rules.iter().filter_map(Value::as_str) {
                if ours(rule) {
                    retired.removed.push(rule.to_owned());
                } else if names_a_handoff(rule) {
                    retired.left.push(format!("{bucket}: {rule}"));
                }
            }
            rules.retain(|rule| !rule.as_str().is_some_and(ours));
        }
    }
    if !retired.removed.is_empty() {
        retired.text = Some(render(map, raw, None, false));
    }
    Ok(retired)
}

/// What the `handoff-rule` fix found in the project's harness files, and did with it
/// ([`retire_handoff_rule`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetiredIn {
    /// Each file a rule was removed from, relative to the project, with the rules removed.
    pub removed: Vec<(&'static str, Vec<String>)>,
    /// Each file that holds a person's own rule about a handoff, with those rules.
    pub left: Vec<(&'static str, Vec<String>)>,
}

/// The project's machine-local Claude Code settings: a person's own, never rewritten here.
const LOCAL_SETTINGS: &str = ".claude/settings.local.json";

/// **The `handoff-rule` fix's whole write** (#1444): the handoff ask rule `init` wrote, taken
/// out of the project's `.claude/settings.json` and `opencode.json`
/// ([`handoff_rule_retired`], [`opencode_handoff_rule_retired`]). With `dry_run`, what it would
/// do, writing nothing: what `purlis doctor`'s row reads.
///
/// **Both files or neither.** Each is read and judged before either is written, so a file
/// purlis cannot read stops the whole fix: a rule in force under one harness and not the other
/// is the split `purlis guard` exists to prevent. A missing file holds no rule.
///
/// **The machine-local file is read and never written.** `.claude/settings.local.json` is one
/// person's own on one machine, and `init` never wrote there, so a handoff rule in it is
/// theirs: it is named in [`RetiredIn::left`] with the rest.
pub fn retire_handoff_rule(root: &Path, dry_run: bool) -> Result<RetiredIn, String> {
    let text_of = |rel: &str| -> Result<Option<String>, String> {
        let path = root.join(rel);
        match std::fs::read_to_string(&path) {
            Ok(raw) => Ok(Some(raw)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!(
                "{rel} could not be read ({e}), so nothing was changed"
            )),
        }
    };
    let claude = text_of(SETTINGS)?
        .map(|raw| handoff_rule_retired(&raw))
        .transpose()?
        .unwrap_or_default();
    let opencode = text_of(OPENCODE)?
        .map(|raw| opencode_handoff_rule_retired(&raw))
        .transpose()?
        .unwrap_or_default();
    // A local file purlis cannot read is not a reason to leave the shared ones as they are:
    // nothing in it is purlis's, so it is only ever named.
    let local = text_of(LOCAL_SETTINGS)
        .ok()
        .flatten()
        .and_then(|raw| handoff_rule_retired(&raw).ok())
        .map(|found| {
            let asks = found.removed.iter().map(|rule| format!("ask: {rule}"));
            asks.chain(found.left.iter().cloned()).collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut out = RetiredIn::default();
    for (rel, found) in [(SETTINGS, &claude), (OPENCODE, &opencode)] {
        if !found.left.is_empty() {
            out.left.push((rel, found.left.clone()));
        }
    }
    if !local.is_empty() {
        out.left.push((LOCAL_SETTINGS, local));
    }
    for (rel, found) in [(SETTINGS, claude), (OPENCODE, opencode)] {
        let Some(text) = found.text else {
            continue;
        };
        if !dry_run {
            std::fs::write(root.join(rel), text).map_err(|e| {
                let before: Vec<&str> = out.removed.iter().map(|(rel, _)| *rel).collect();
                if before.is_empty() {
                    format!("{rel} could not be written ({e}), so nothing was changed")
                } else {
                    format!(
                        "{rel} could not be written ({e}); {} was already rewritten",
                        before.join(", ")
                    )
                }
            })?;
        }
        out.removed.push((rel, found.removed));
    }
    Ok(out)
}

/// opencode's `opencode.json`, as `raw`, **without the handoff ask rule `init` wrote**
/// (#1444): `"charter handoff *": "ask"` and `"purlis handoff *": "ask"` under
/// `permission.bash`. The same glob with another decision (a `deny`, an `allow`) and any other
/// glob that names a handoff are the person's, left and named, as
/// [`handoff_rule_retired`] leaves them.
pub fn opencode_handoff_rule_retired(raw: &str) -> Result<Retired, String> {
    let Some(Value::Object(mut map)) = pyjson::loads_strict(raw) else {
        return Err(format!(
            "{OPENCODE} is not a JSON object purlis can read, so nothing in it was changed"
        ));
    };
    let mut retired = Retired::default();
    if let Some(Value::Object(bash)) = map
        .get_mut("permission")
        .and_then(|permission| permission.get_mut("bash"))
    {
        let ours = |glob: &str, decision: &Value| {
            RETIRED_HANDOFF_PATTERNS.contains(&glob) && decision.as_str() == Some("ask")
        };
        for (glob, decision) in bash.iter() {
            if ours(glob, decision) {
                retired.removed.push(glob.clone());
            } else if names_a_handoff(glob) {
                let decision = decision.as_str().unwrap_or("not a decision");
                retired.left.push(format!("{decision}: {glob}"));
            }
        }
        bash.retain(|glob, decision| !ours(glob, decision));
    }
    if !retired.removed.is_empty() {
        retired.text = Some(pyjson::dumps_indent2(&Value::Object(map)));
    }
    Ok(retired)
}

/// The consent pattern `pattern` (spelt `charter …`, one of [`CONSENT_PATTERNS`]) spelt with the
/// program name `name` instead, or `None` for a pattern that does not start with `charter `.
pub fn consent_pattern_for(pattern: &str, name: &str) -> Option<String> {
    let rest = pattern
        .strip_prefix(crate::cliname::ALIAS)?
        .strip_prefix(' ')?;
    Some(format!("{name} {rest}"))
}

/// Whether the project at `root` makes the host ask before a command `pattern` names, in every
/// harness `init` writes a consent rule for: an `ask` (or a `deny`, which stops it outright) for
/// `Bash(<pattern>)` in Claude Code's `.claude/settings.json`, and the same for `<pattern>` in
/// opencode's `permission.bash`.
///
/// **Fails closed.** A file that is missing, unreadable or not the shape the harness reads
/// answers `false`, and so does a rule in one harness and not the other: a command the guard
/// let through on the strength of a rule one harness lacks would run there with no prompt.
pub fn carries_consent_rule(root: &Path, pattern: &str) -> bool {
    let held = |decision: Option<String>| matches!(decision.as_deref(), Some("ask" | "deny"));
    held(claude_decision(root, pattern)) && held(opencode_decision(root, pattern))
}

/// Whether both harnesses ASK before `pattern` — not deny it: the rule `init` and `reinit` write
/// a `purlis` twin beside. A `charter` rule the operator turned into a deny gets no `ask` twin,
/// which would let the same command through under the other spelling after one click.
pub fn asks_in_every_harness(root: &Path, pattern: &str) -> bool {
    claude_decision(root, pattern).as_deref() == Some("ask")
        && opencode_decision(root, pattern).as_deref() == Some("ask")
}

/// What Claude Code's `.claude/settings.json` decides for `Bash(<pattern>)`: `deny` when it denies
/// it (deny is weighed first, whatever the order), else `ask`, else `None` — `None` too for a
/// file that is missing or that charter cannot read.
fn claude_decision(root: &Path, pattern: &str) -> Option<String> {
    claude_decision_in(&root.join(SETTINGS), pattern)
}

/// [`claude_decision`] of the settings file at `path`.
fn claude_decision_in(path: &Path, pattern: &str) -> Option<String> {
    let rule = format!("Bash({pattern})");
    let doc = pyjson::loads_strict(&read_for_the_guard(path)?)?;
    let perms = doc.get("permissions")?.as_object()?;
    ["deny", "ask"]
        .into_iter()
        .find(|bucket| {
            perms
                .get(*bucket)
                .and_then(Value::as_array)
                .is_some_and(|rules| rules.iter().any(|r| r.as_str() == Some(rule.as_str())))
        })
        .map(str::to_owned)
}

/// What opencode's `permission.bash` decides for `pattern`, or `None`.
fn opencode_decision(root: &Path, pattern: &str) -> Option<String> {
    let raw = read_for_the_guard(&root.join(OPENCODE))?;
    let doc = pyjson::loads_strict(&raw)?;
    doc.get("permission")?
        .get("bash")?
        .get(pattern)?
        .as_str()
        .map(str::to_owned)
}

/// How strictly a decision holds a command back: a deny over an ask over nothing.
fn strictness(decision: Option<&str>) -> u8 {
    match decision {
        Some("deny") => 2,
        Some("ask") => 1,
        _ => 0,
    }
}

/// Whether the consent rule `pattern` (spelt `charter …`) holds the command spelt with `name`
/// at least as strictly as it holds the `charter` spelling, everywhere the host may read it for
/// a call in the project at `plane` (RN-7, D-RN7-4).
///
/// `anchors` are the directories the host's settings may have been loaded for: the folder the
/// session started in (`$CLAUDE_PROJECT_DIR`, where Claude Code loads its settings) and the
/// call's own `cwd`, which follows the shell's `cd`. Each one present is asked, because the two
/// differ after a `cd` and the guard cannot tell which one the host applies.
///
/// Asked of each file: the project's `.claude/settings.json`, the nearest
/// `.claude/settings.json` at or above each anchor inside the project (a workspace's or a
/// clone's layer), and the project's `opencode.json`. In each, the twin must be there, and be a
/// deny where the `charter` rule is one. **Fails closed**: no anchor at all, an anchor this
/// cannot place inside the project, a file missing or unreadable, or a twin weaker than its rule
/// in any of them answers `false`.
pub fn twin_in_force(plane: &Path, anchors: &[&Path], pattern: &str, name: &str) -> bool {
    let Some(twin) = consent_pattern_for(pattern, name) else {
        return false;
    };
    let held = |twin_says: Option<String>, rule_says: Option<String>| {
        let twin = strictness(twin_says.as_deref());
        twin > 0 && twin >= strictness(rule_says.as_deref())
    };
    let anchors: Vec<&Path> = anchors
        .iter()
        .copied()
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect();
    if anchors.is_empty() {
        return false;
    }
    let mut claude = vec![plane.join(SETTINGS)];
    for anchor in anchors {
        let Some(layer) = layer_settings(plane, anchor) else {
            return false;
        };
        claude.extend(layer);
    }
    claude.iter().all(|file| {
        held(
            claude_decision_in(file, &twin),
            claude_decision_in(file, pattern),
        )
    }) && held(
        opencode_decision(plane, &twin),
        opencode_decision(plane, pattern),
    )
}

/// The most a settings file the guard reads on a tool call may hold: far past any real one.
const GUARD_READ_MAX: u64 = 1024 * 1024;

/// The text of a host settings file the guard reads on a tool call, or `None` for one that is
/// missing, not a plain file (a FIFO would hang the hook), larger than [`GUARD_READ_MAX`] or not
/// UTF-8 (#1286). A link is followed, as the host follows it — an operator who keeps
/// `.claude/settings.json` in a dotfiles tool has one there, and its rules are in force — and
/// what it names must be a plain file too. Opened `O_NONBLOCK`, so a FIFO cannot hold the open,
/// and its kind and size are asked of the open file, so nothing swapped in after a check is
/// read instead.
pub(crate) fn read_for_the_guard(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let open = options.open(path).ok()?;
    let meta = open.metadata().ok()?;
    if !meta.is_file() || meta.len() > GUARD_READ_MAX {
        return None;
    }
    let mut bytes = Vec::new();
    open.take(GUARD_READ_MAX + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > GUARD_READ_MAX {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// The settings file of the layer a call in `cwd` runs under: the nearest
/// `.claude/settings.json` at or above `cwd` and below the project root. `Some(None)` when there
/// is none (the root's own applies), `None` when `cwd` cannot be placed inside the project.
pub(crate) fn layer_settings(plane: &Path, cwd: &Path) -> Option<Option<std::path::PathBuf>> {
    if cwd.as_os_str().is_empty() {
        return Some(None);
    }
    let plane = plane.canonicalize().ok()?;
    let cwd = cwd.canonicalize().ok()?;
    let below = cwd.strip_prefix(&plane).ok()?;
    let mut dir = plane.join(below);
    while dir != plane {
        let file = dir.join(SETTINGS);
        // Asked of the entry itself, so asking cannot block: a link there is the layer, read
        // through as the host reads it, and a FIFO is the layer [`read_for_the_guard`] refuses
        // to read, which fails closed.
        if std::fs::symlink_metadata(&file).is_ok() {
            return Some(Some(file));
        }
        if !dir.pop() {
            break;
        }
    }
    Some(None)
}

/// The one hook charter wires itself (`commands._GUARD_HOOK`).
///
/// Spelt `charter` in every project, migrated or not, for the rename's whole window
/// (D-RN7-11): the alias resolves on every build, and a hook command that does not resolve
/// fails open. Renamed at 1.0 with the alias itself.
pub fn guard_hook() -> Value {
    json!({
        "matcher": "Bash",
        "hooks": [{"type": "command", "command": format!("{} hook pretooluse", crate::cliname::ALIAS), "timeout": 10}],
    })
}

/// The harness variable `init` sets in `.claude/settings.json`'s `env`, as the project at `plane`
/// writes it: `CHARTER_HARNESS` until it is migrated, `PURLIS_HARNESS` after.
pub fn harness_env_for(plane: &Path) -> String {
    format!("{}HARNESS", crate::names::ENV_PREFIX.writes_for(plane))
}

/// [`ensure_env`] for the harness variable, under the plane's spelling ([`harness_env_for`]),
/// and `Present` when ANY spelling of it is set already: a migrated project's
/// `PURLIS_HARNESS` is never joined by a `CHARTER_HARNESS` again, nor the other way round.
pub fn ensure_harness_env(root: &Path, value: &str) -> Wrote {
    let doc = read(&root.join(SETTINGS));
    if let Some(env) = doc
        .map
        .as_ref()
        .and_then(|map| map.get("env"))
        .and_then(Value::as_object)
    {
        let set = crate::names::ENV_PREFIX.spellings().any(|prefix| {
            env.get(&format!("{prefix}HARNESS"))
                .is_some_and(text::truthy)
        });
        if set {
            return Wrote::Present;
        }
    }
    ensure_env(root, &harness_env_for(root), value)
}

/// The project's `.claude/settings.json` with charter's own entries under purlis's names (RN-7,
/// the `rename-plane` fix), or `Ok(None)` when it has nothing to rename or is not there.
///
/// - `env.CHARTER_HARNESS` becomes `env.PURLIS_HARNESS`, where it stood. Beside a
///   `PURLIS_HARNESS` that says the same, it is dropped; beside one that says something else it
///   is refused, because which of the two the operator meant is not charter's to choose.
/// - Hook and `statusLine` commands are left as they are: they keep `charter` for the window
///   (D-RN7-11).
/// - Each `ask` and `deny` rule `Bash(charter …)` gets its `Bash(purlis …)` twin right after it,
///   and keeps its own place: `charter` still runs the command line for the rename's window, so
///   a rule removed would be a command that stopped asking. `allow` rules get no twin — a grant
///   copied to a spelling nobody granted is a permission nobody clicked for.
///
/// Written in the file's own layout, as every writer here writes it. A file charter cannot
/// read the way the harness reads it is refused, never repaired.
pub fn renamed_to_purlis(root: &Path) -> Result<Option<String>, String> {
    let path = root.join(SETTINGS);
    if !path.exists() {
        return Ok(None);
    }
    let doc = read(&path);
    let Some(mut map) = doc.map else {
        return Err(format!(
            "{} is not a JSON object charter can read, so its hooks and rules cannot be renamed",
            SETTINGS
        ));
    };
    let old_env = format!("{}HARNESS", crate::names::ENV_PREFIX.newest_old());
    let new_env = format!("{}HARNESS", crate::names::ENV_PREFIX.write);
    if let Some(Value::Object(env)) = map.get_mut("env")
        && let Some(value) = env.get(&old_env).cloned()
    {
        match env.get(&new_env) {
            Some(new) if *new == value => {
                env.shift_remove(&old_env);
            }
            Some(_) => {
                return Err(format!(
                    "{SETTINGS} sets both {old_env} and {new_env}, to different values; keep \
                     the one you mean and remove the other, then run the fix again"
                ));
            }
            None => {
                let renamed: Map<String, Value> = std::mem::take(env)
                    .into_iter()
                    .map(|(k, v)| {
                        if k == old_env {
                            (new_env.clone(), v)
                        } else {
                            (k, v)
                        }
                    })
                    .collect();
                *env = renamed;
            }
        }
    }
    if let Some(Value::Object(perms)) = map.get_mut("permissions") {
        for bucket in ["ask", "deny"] {
            if let Some(Value::Array(rules)) = perms.get_mut(bucket) {
                // A twin already anywhere in the list stays where it is, and is not added twice.
                let mut have: std::collections::HashSet<String> = rules
                    .iter()
                    .filter_map(|r| r.as_str().map(str::to_owned))
                    .collect();
                let mut out: Vec<Value> = Vec::with_capacity(rules.len());
                for rule in std::mem::take(rules) {
                    let twin = rule.as_str().and_then(|rule| rule_twin(bucket, rule));
                    out.push(rule);
                    if let Some(twin) = twin
                        && have.insert(twin.clone())
                    {
                        out.push(Value::String(twin));
                    }
                }
                *rules = out;
            }
        }
    }
    let text = render(map, &doc.raw, None, false);
    Ok((text != doc.raw).then_some(text))
}

/// [`purlis_rule_twin`] of a rule in `bucket` (`ask` or `deny`), and none for the retired
/// handoff ask (#1444): purlis writes that rule nowhere now, the rename included. A project
/// that still carries `Bash(charter handoff *)` as an ask keeps it until the `handoff-rule`
/// fix removes it, and is given no `purlis` copy of it. A deny on it is an operator's own.
fn rule_twin(bucket: &str, rule: &str) -> Option<String> {
    if bucket == "ask" && rule == HANDOFF_RULE {
        return None;
    }
    purlis_rule_twin(rule)
}

/// `Bash(purlis …)` for a rule `Bash(charter …)`, else `None`.
fn purlis_rule_twin(rule: &str) -> Option<String> {
    if let Some(twin) = mcp_rule_twin(rule) {
        return Some(twin);
    }
    let inner = rule.strip_prefix("Bash(")?.strip_suffix(')')?;
    let pattern = consent_pattern_for(inner, crate::cliname::PRIMARY)?;
    Some(format!("Bash({pattern})"))
}

/// The purlis twin of a permission rule on charter's own MCP server under its old name (#1266,
/// D-RN8-12): `mcp__charter__<tool>` → `mcp__purlis__<tool>`, and the server-wide
/// `mcp__charter` → `mcp__purlis`. The server is the plugin's name, so an operator's `ask` or
/// `deny` on the old one matches nothing once it is renamed. `None` for any other rule.
pub fn mcp_rule_twin(rule: &str) -> Option<String> {
    let old = crate::names::MCP_TOOL_PREFIX.newest_old();
    let new = crate::names::MCP_TOOL_PREFIX.write;
    if let Some(tool) = rule.strip_prefix(old) {
        return (!tool.is_empty()).then(|| format!("{new}{tool}"));
    }
    (rule == old.trim_end_matches('_')).then(|| new.trim_end_matches('_').to_owned())
}

/// Every `ask` and `deny` rule in the Claude Code settings files `paths` that names charter's
/// MCP server by its old name, as its purlis twin, by bucket: what a chat the app starts is
/// handed beside them (D-RN8-12), so the operator's ask or deny still holds (ADR 0064). Allow
/// rules get none: a missing allow only asks. A file that is missing or unreadable adds none.
pub fn mcp_rule_twins_in(paths: &[std::path::PathBuf]) -> (Vec<String>, Vec<String>) {
    let mut ask: Vec<String> = Vec::new();
    let mut deny: Vec<String> = Vec::new();
    for path in paths {
        let Some(map) = read(path).map else {
            continue;
        };
        let Some(perms) = map.get("permissions").and_then(Value::as_object) else {
            continue;
        };
        for (bucket, out) in [("ask", &mut ask), ("deny", &mut deny)] {
            let rules = perms.get(bucket).and_then(Value::as_array);
            for twin in rules
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(mcp_rule_twin)
            {
                if !out.contains(&twin) {
                    out.push(twin);
                }
            }
        }
    }
    (ask, deny)
}

/// Every `ask` and `deny` rule in the Claude Code settings file at `path`; empty for a file
/// that is missing or unreadable.
pub fn permission_rules_in(path: &Path) -> Vec<String> {
    let Some(map) = read(path).map else {
        return Vec::new();
    };
    let Some(perms) = map.get("permissions").and_then(Value::as_object) else {
        return Vec::new();
    };
    ["ask", "deny"]
        .iter()
        .filter_map(|bucket| perms.get(*bucket).and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The `ask` and `deny` globs in `opencode.json`'s `permission.bash` that spell `charter …`,
/// each with its decision: what the `rename-plane` fix writes a `purlis …` twin of, through
/// [`ensure_opencode_rule`]. `Err` for a file charter cannot read; empty for none.
pub fn opencode_rules_to_twin(root: &Path) -> Result<Vec<(String, String)>, String> {
    let path = root.join(OPENCODE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let doc = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| pyjson::loads_strict(&raw));
    let Some(Value::Object(map)) = doc else {
        return Err(format!(
            "{OPENCODE} is not a JSON object charter can read, so its rules cannot be renamed"
        ));
    };
    let Some(bash) = map
        .get("permission")
        .and_then(|p| p.get("bash"))
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };
    Ok(bash
        .iter()
        .filter_map(|(glob, decision)| {
            let decision = decision.as_str().filter(|d| matches!(*d, "ask" | "deny"))?;
            // Not the retired handoff ask (#1444), as in Claude Code's file.
            if decision == "ask" && glob.as_str() == HANDOFF_PATTERN {
                return None;
            }
            let twin = consent_pattern_for(glob, crate::cliname::PRIMARY)?;
            Some((twin, decision.to_owned()))
        })
        .collect())
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
    /// The harness already denies exactly this; nothing written. A rule charter adds never
    /// takes a command a deny holds back and makes it something the operator can answer
    /// (FR-17's review, B1).
    Denied,
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
    ensure_rule(&root.join(SETTINGS), "ask", rule, dry_run)
}

/// `commands.add_permission_rule(root, rule, bucket, local)`: append `rule` to
/// `permissions.<bucket>` of the settings file at `path` — the plane's shared
/// `.claude/settings.json`, or its machine-local `.claude/settings.local.json`.
pub fn ensure_rule(path: &Path, bucket: &str, rule: &str, dry_run: bool) -> Wrote {
    let path = path.to_path_buf();
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
    let mut entries = match perms.get(bucket) {
        Some(Value::Array(entries)) => entries.clone(),
        Some(_) => {
            return Wrote::Malformed(format!(
                "{} (`permissions.{bucket}` is not a list)",
                path.display()
            ));
        }
        None => Vec::new(),
    };
    if entries.iter().any(|e| e.as_str() == Some(rule)) {
        return Wrote::Present;
    }
    // Claude Code weighs `deny` before `ask` and `allow` whatever the order, so a rule added
    // here never outranks a broader deny; an exact one is said, and nothing is added beside it.
    if bucket != "deny"
        && perms
            .get("deny")
            .and_then(Value::as_array)
            .is_some_and(|denied| denied.iter().any(|e| e.as_str() == Some(rule)))
    {
        return Wrote::Denied;
    }
    entries.push(Value::String(rule.to_owned()));
    perms.insert(bucket.to_owned(), Value::Array(entries));
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
    ensure_opencode_rule(root, glob, "ask", dry_run)
}

/// [`ensure_opencode_ask`] for either decision: `permission.bash[glob] = decision`, where the
/// check is "is the decision already this one".
///
/// **Never weaker than a deny that is there.** opencode decides a command by the last rule in
/// `permission.bash` that matches it (opencode.ai/docs/permissions: "the last matching rule
/// winning"), so where Python appends, this:
///
/// - leaves an exact `deny` for `glob` as it is, and answers [`Wrote::Denied`];
/// - puts a new `glob` **right after the last entry that is not a deny and whose pattern
///   matches `glob`'s own text** — `"cargo *": "allow"` for `cargo publish *` — so no allow or
///   ask that would answer the same command comes after it and outranks it, which in an
///   allowlist (`"*": "deny"`, then what is let through) would leave the new rule doing
///   nothing. A deny written after that entry still comes after the new rule and still decides;
/// - with no such entry, puts it **before the first `deny`**, so every deny that matches the
///   same command still comes after it: `"*": "deny"` stays the last word on `cargo publish`;
/// - with neither, appends it, as Python does.
///
/// Patterns are compared as opencode compares them, `*` and `?` as wildcards, against the
/// glob's text. An existing entry for `glob` with another decision (an `allow` made an `ask`)
/// changes where it stands, as Python changes it, unless a later non-deny entry matches it; then
/// it is moved by the same rule, so that broader allow cannot outrank it. A `deny` is appended,
/// the last word wherever it would otherwise go.
pub fn ensure_opencode_rule(root: &Path, glob: &str, decision: &str, dry_run: bool) -> Wrote {
    let path = root.join(OPENCODE);
    let map = if path.exists() {
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
    let perms = match map.get("permission") {
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
    let had = block.get(glob).and_then(Value::as_str);
    if had == Some(decision) {
        return Wrote::Present;
    }
    if had == Some("deny") {
        return Wrote::Denied;
    }
    let matches =
        |pattern: &str| glob::Pattern::new(pattern).is_ok_and(|pattern| pattern.matches(glob));
    // An entry already there with another decision changes where it stands, as Python changes
    // it, unless a later entry that is not a deny matches it: that one would answer first, so
    // the entry is taken out and placed again by the rule below.
    if had.is_some() {
        let outranked = block
            .iter()
            .skip_while(|(pattern, _)| pattern.as_str() != glob)
            .skip(1)
            .any(|(pattern, value)| value.as_str() != Some("deny") && matches(pattern));
        if !outranked {
            block.insert(glob.to_owned(), Value::String(decision.to_owned()));
            return finish(&path, map, perms, block, dry_run);
        }
        block.shift_remove(glob);
    }
    let after_match = block
        .iter()
        .enumerate()
        .rev()
        .find(|(_, (pattern, value))| value.as_str() != Some("deny") && matches(pattern))
        .map(|(i, _)| i + 1);
    let first_deny = block
        .values()
        .position(|value| value.as_str() == Some("deny"));
    match after_match.or(first_deny) {
        Some(at) if decision != "deny" && at < block.len() => {
            let mut placed = Map::new();
            for (i, (key, value)) in std::mem::take(&mut block).into_iter().enumerate() {
                if i == at {
                    placed.insert(glob.to_owned(), Value::String(decision.to_owned()));
                }
                placed.insert(key, value);
            }
            block = placed;
        }
        _ => {
            block.insert(glob.to_owned(), Value::String(decision.to_owned()));
        }
    }
    finish(&path, map, perms, block, dry_run)
}

/// Puts `block` back as `permission.bash` and writes `opencode.json`, or says it would.
fn finish(
    path: &Path,
    mut map: Map<String, Value>,
    mut perms: Map<String, Value>,
    block: Map<String, Value>,
    dry_run: bool,
) -> Wrote {
    perms.insert("bash".to_owned(), Value::Object(block));
    map.insert("permission".to_owned(), Value::Object(perms));
    let text = pyjson::dumps_indent2(&Value::Object(map));
    if dry_run {
        return Wrote::Created;
    }
    match write(path, &text) {
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
/// in `command`, left to right, under every name the command line is installed as (RN-3): an
/// installed plugin records the `purlis` path, and its guard is the same guard.
///
/// Hand-rolled to that regex's letter: `charter` must start a word (so `xcharter` does not
/// count and `/usr/bin/charter` does), the gaps are Python's `\s` (wider than Rust's
/// whitespace), and the name is the longest run of `[A-Za-z0-9_-]` — so
/// `charter hook pretooluse-read` names `pretooluse-read`, which is a different handler.
pub fn handlers(command: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = crate::cliname::INSTALLED
        .iter()
        .flat_map(|name| handlers_named(command, name))
        .collect();
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, handler)| handler).collect()
}

/// [`handlers`] for one program name, each with where its name starts.
fn handlers_named(command: &str, program: &str) -> Vec<(usize, String)> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let space = crate::memstore::is_python_space;
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = command[from..].find(program) {
        let at = from + found;
        from = at + 1;
        if command[..at].chars().next_back().is_some_and(word) {
            continue;
        }
        let rest = &command[at + program.len()..];
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
        out.push((at, name));
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

    /// A settings file the guard reads on every tool call is read only when it is, or a link
    /// names, a plain file of a sane size: a FIFO would hang the hook and a huge file would
    /// stall it (#1286).
    #[cfg(unix)]
    #[test]
    fn the_guard_reads_a_plain_settings_file_through_a_link_and_nothing_else() {
        let dir = tempfile::tempdir().expect("a directory");
        let plain = dir.path().join("plain.json");
        std::fs::write(&plain, "{}").expect("a file");
        assert_eq!(read_for_the_guard(&plain).as_deref(), Some("{}"));

        let fifo = dir.path().join("fifo.json");
        let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&fifo))
            .expect("mkfifo runs");
        assert!(made.success());
        assert_eq!(read_for_the_guard(&fifo), None, "a FIFO is not read");

        let big = dir.path().join("big.json");
        let file = std::fs::File::create(&big).expect("a file");
        file.set_len(GUARD_READ_MAX + 1).expect("sized");
        assert_eq!(
            read_for_the_guard(&big),
            None,
            "an oversized file is not read"
        );

        // A link is followed, as the host follows it; what it names must be a plain file too.
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&plain, &link).expect("a link");
        assert_eq!(read_for_the_guard(&link).as_deref(), Some("{}"));
        let to_fifo = dir.path().join("to-fifo.json");
        std::os::unix::fs::symlink(&fifo, &to_fifo).expect("a link");
        assert_eq!(
            read_for_the_guard(&to_fifo),
            None,
            "a link to a FIFO is not read"
        );
        let dangling = dir.path().join("dangling.json");
        std::os::unix::fs::symlink(dir.path().join("gone"), &dangling).expect("a link");
        assert_eq!(read_for_the_guard(&dangling), None);

        assert_eq!(read_for_the_guard(&dir.path().join("missing.json")), None);
    }

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
            // The name the command line ships as since RN-3: an installed plugin records the
            // `purlis` path, and its guard is the same guard.
            ("purlis hook pretooluse", vec!["pretooluse"]),
            (
                "/opt/p.app/Contents/MacOS/purlis hook pretooluse",
                vec!["pretooluse"],
            ),
            ("xpurlis hook pretooluse", vec![]),
            (
                "purlis hook stop && charter hook sessionstart",
                vec!["stop", "sessionstart"],
            ),
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

    /// Each purlis consent rule is its charter one with the program name swapped, so the two
    /// tables cannot drift (RN-7).
    #[test]
    fn each_purlis_consent_rule_is_the_charter_one_under_the_new_name() {
        for (charter, purlis) in CONSENT_PATTERNS.iter().zip(PURLIS_CONSENT_PATTERNS) {
            assert_eq!(
                consent_pattern_for(charter, crate::cliname::PRIMARY).as_deref(),
                Some(purlis)
            );
        }
        for (rule, pattern) in [
            (PURLIS_HANDOFF_RULE, PURLIS_HANDOFF_PATTERN),
            (PURLIS_REPORT_RULE, PURLIS_REPORT_PATTERN),
            (PURLIS_PROMOTE_RULE, PURLIS_PROMOTE_PATTERN),
        ] {
            assert_eq!(rule, format!("Bash({pattern})"));
        }
        assert_eq!(consent_pattern_for("charterx y", "purlis"), None);
        assert_eq!(consent_pattern_for("git push *", "purlis"), None);
    }

    /// The rule `init` wrote for a handoff, under both names, and nothing else (#1444).
    #[test]
    fn the_handoff_rule_init_wrote_is_removed_and_every_other_rule_stays_where_it_was() {
        let raw = concat!(
            r#"{"env":{"CHARTER_HARNESS":"claude-code"},"permissions":{"ask":["#,
            r#""Bash(charter handoff *)","Bash(charter report *--yes*)","#,
            r#""Bash(purlis handoff *)","Bash(terraform apply *)"],"allow":["Bash(ls *)"]}}"#,
            "\n"
        );
        let retired = handoff_rule_retired(raw).expect("a settings file");
        assert_eq!(
            retired.removed,
            ["Bash(charter handoff *)", "Bash(purlis handoff *)"]
        );
        assert_eq!(retired.left, Vec::<String>::new());
        assert_eq!(
            retired.text.as_deref(),
            Some(concat!(
                r#"{"env":{"CHARTER_HARNESS":"claude-code"},"permissions":{"ask":["#,
                r#""Bash(charter report *--yes*)","Bash(terraform apply *)"],"#,
                r#""allow":["Bash(ls *)"]}}"#,
                "\n"
            ))
        );
    }

    /// A rule about a handoff that is not the one `init` wrote is a person's: a deny, an
    /// allow, another spelling. It is named and the file is not rewritten for it.
    #[test]
    fn a_handoff_rule_a_person_wrote_or_edited_is_left_and_named() {
        let raw = r#"{"permissions": {"ask": ["Bash(purlis handoff:*)", "Bash(purlis handoff beta *)"],
            "deny": ["Bash(charter handoff *)"], "allow": ["Bash(purlis handoff *)"]}}"#;
        let retired = handoff_rule_retired(raw).expect("a settings file");
        assert_eq!(retired.text, None, "nothing here is purlis's to remove");
        assert_eq!(retired.removed, Vec::<String>::new());
        assert_eq!(
            retired.left,
            [
                "ask: Bash(purlis handoff:*)",
                "ask: Bash(purlis handoff beta *)",
                "deny: Bash(charter handoff *)",
                "allow: Bash(purlis handoff *)",
            ]
        );
        // And beside the one it wrote: that one goes, the person's stays and is named.
        let both = r#"{"permissions": {"ask": ["Bash(purlis handoff *)"], "deny": ["Bash(charter handoff *)"]}}"#;
        let retired = handoff_rule_retired(both).expect("a settings file");
        assert_eq!(retired.removed, ["Bash(purlis handoff *)"]);
        assert_eq!(retired.left, ["deny: Bash(charter handoff *)"]);
        assert_eq!(
            retired.text.as_deref(),
            Some(r#"{"permissions": {"ask": [], "deny": ["Bash(charter handoff *)"]}}"#)
        );
    }

    #[test]
    fn a_settings_file_with_no_handoff_rule_or_that_does_not_parse_is_not_rewritten() {
        for raw in [
            "{}",
            r#"{"permissions": {"ask": ["Bash(terraform apply *)"]}}"#,
            r#"{"permissions": "nope"}"#,
            r#"{"permissions": {"ask": "Bash(purlis handoff *)"}}"#,
        ] {
            assert_eq!(handoff_rule_retired(raw), Ok(Retired::default()), "{raw}");
        }
        for raw in ["", "[]", "{not json", r#"{"a": NaN}"#] {
            assert!(handoff_rule_retired(raw).is_err(), "{raw:?}");
            assert!(opencode_handoff_rule_retired(raw).is_err(), "{raw:?}");
        }
    }

    #[test]
    fn opencode_s_handoff_ask_is_removed_and_another_decision_for_it_is_left_and_named() {
        let raw = r#"{"permission": {"bash": {"charter handoff *": "ask", "ls *": "allow",
            "charter report *--yes*": "ask", "purlis handoff *": "ask"}}}"#;
        let retired = opencode_handoff_rule_retired(raw).expect("an opencode file");
        assert_eq!(retired.removed, ["charter handoff *", "purlis handoff *"]);
        assert_eq!(retired.left, Vec::<String>::new());
        let now: Value = serde_json::from_str(retired.text.as_deref().expect("rewritten")).unwrap();
        assert_eq!(
            now,
            json!({"permission": {"bash": {"ls *": "allow", "charter report *--yes*": "ask"}}})
        );

        let theirs = r#"{"permission": {"bash": {"purlis handoff *": "deny", "charter handoff *": "allow",
            "purlis handoff beta *": "ask"}}}"#;
        let retired = opencode_handoff_rule_retired(theirs).expect("an opencode file");
        assert_eq!(retired.text, None);
        assert_eq!(
            retired.left,
            [
                "deny: purlis handoff *",
                "allow: charter handoff *",
                "ask: purlis handoff beta *",
            ]
        );
        assert_eq!(
            opencode_handoff_rule_retired(r#"{"permission": "ask"}"#),
            Ok(Retired::default())
        );
    }

    /// The rename gives every `charter …` ask or deny its `purlis …` twin, and not the retired
    /// handoff ask: purlis writes that rule nowhere now (#1444). A deny on it is an operator's
    /// own and is twinned like any other.
    #[test]
    fn the_rename_writes_no_twin_of_the_retired_handoff_ask() {
        assert_eq!(rule_twin("ask", "Bash(charter handoff *)"), None);
        assert_eq!(
            rule_twin("deny", "Bash(charter handoff *)").as_deref(),
            Some("Bash(purlis handoff *)")
        );
        assert_eq!(
            rule_twin("ask", "Bash(charter report *--yes*)").as_deref(),
            Some("Bash(purlis report *--yes*)")
        );
        assert_eq!(
            rule_twin("ask", "Bash(charter handoff beta *)").as_deref(),
            Some("Bash(purlis handoff beta *)"),
            "a narrower rule is a person's own"
        );
    }

    /// A migrated project's harness variable is never joined by the old one, nor the other way.
    #[test]
    fn the_harness_variable_is_written_once_under_either_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(
            root.join(SETTINGS),
            r#"{"env": {"PURLIS_HARNESS": "claude-code"}}"#,
        )
        .unwrap();
        assert!(matches!(
            ensure_harness_env(root, "claude-code"),
            Wrote::Present
        ));
        std::fs::write(root.join(SETTINGS), "{}").unwrap();
        assert!(matches!(
            ensure_harness_env(root, "claude-code"),
            Wrote::Created
        ));
        let text = std::fs::read_to_string(root.join(SETTINGS)).unwrap();
        assert!(text.contains("CHARTER_HARNESS"), "unmigrated: {text}");
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
