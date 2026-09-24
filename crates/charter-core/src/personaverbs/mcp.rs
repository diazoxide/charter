//! A persona's MCP servers: what its `mcp.json` declares, how a declared server is carried
//! into its generated sub-agent, and the consent that decides whether that server is handed
//! the persona's vault — `persona.mcp_*` and `charter/mcpseen.py`.
//!
//! # The consent line IS the approval
//!
//! A server that declares `secrets` or `secret_files` is wrapped in `charter secret exec
//! <vault> …` only when this machine approved it, and what was approved is the SHA-256 of
//! the line [`describe`] prints for it. So the line must be the same bytes Python prints, or
//! every approval recorded by either charter lapses under the other: the escape, the
//! quoting, the key order and the ceiling are all Python's, and the recorded scenario
//! `persona-sync-agents-wraps-an-approved-server-in-its-vault` holds a fingerprint Python
//! wrote.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// The sidecar naming a persona's MCP servers — `persona.MCP_FILE`.
pub const MCP_FILE: &str = "mcp.json";

/// Where this machine's approvals are recorded, under the plane's state directory.
pub const APPROVED_FILE: &str = "mcp-approved.json";

/// What a withheld server's line reads when [`describe`] cannot render it.
pub const UNRENDERABLE: &str = "(charter cannot show this entry in full — nothing to approve)";

const MAX_COLS: usize = 80;
const MAX_ROWS: usize = 10;
const MAX_NAME: usize = 35;
const MAX_LABEL: usize = MAX_NAME * 2 + 1;
/// `len("• ") + len("  ") + len(" → ")`, in characters: 2 + 2 + 3. Written as the sum's
/// value, because `2 * 2 + 3` is the same 7 and a mutation test could never tell them apart.
const DECORATION: usize = 7;
/// The longest consent line charter will print and ask about: a screen, less the label.
const MAX_LINE: usize = MAX_COLS * MAX_ROWS - MAX_LABEL - DECORATION;

/// `_MCP_NAME_RE`, full-matched: `[A-Za-z0-9_][A-Za-z0-9._-]{0,63}`.
pub fn name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphanumeric() || first == '_')
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `(kept, refused)` — the lineage's declared servers, parent first and child winning, split
/// by [`name_ok`] — `persona._mcp_declared`. `kept` keeps Python's dict order: a server keeps
/// the position it was first declared at, even when a child overrides its entry.
pub fn declared(root: &Path, name: &str) -> (Map<String, Value>, Vec<String>) {
    let mut kept = Map::new();
    let mut refused: Vec<String> = Vec::new();
    for ancestor in crate::personas::lineage(root, name).iter().rev() {
        let file = root.join("personas").join(ancestor).join(MCP_FILE);
        if !crate::memstore::readable_file(root, &file) {
            continue;
        }
        let Some(doc) = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            continue;
        };
        let Some(doc) = doc.as_object() else {
            continue;
        };
        // Python skips a falsy `mcpServers` and then anything that is not a dict. The only
        // falsy dict is `{}`, which declares nothing either way, so one test is both.
        let Some(Value::Object(servers)) = doc.get("mcpServers") else {
            continue;
        };
        for (server, entry) in servers {
            if name_ok(server) {
                kept.insert(server.clone(), entry.clone());
            } else if !refused.contains(server) {
                refused.push(server.clone());
            }
        }
    }
    (kept, refused)
}

/// `persona.mcp_vault`: the vault an entry would be wrapped with — `None` for none at all and
/// for [`super::NO_VAULT`].
pub fn vault_for(vault: Option<&str>) -> Option<String> {
    let v = crate::memstore::py_strip(vault.unwrap_or_default());
    (!v.is_empty() && v != super::NO_VAULT).then(|| v.to_string())
}

fn non_empty_object(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object).filter(|m| !m.is_empty())
}

/// `mcpseen.declares_credential`.
pub fn declares_credential(entry: &Value) -> bool {
    non_empty_object(entry.get("secrets")).is_some()
        || non_empty_object(entry.get("secret_files")).is_some()
}

/// `mcpseen.needs_consent`.
pub fn needs_consent(vault: Option<&str>, entry: &Value) -> bool {
    vault.is_some_and(|v| !v.is_empty()) && declares_credential(entry)
}

/// `contain.escaped(text, quote=True)`: printable ASCII kept, `\` and `"` escaped, everything
/// else as its fixed-width `\uXXXX`/`\UXXXXXXXX`.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            ' '..='~' => out.push(c),
            _ => out.push_str(&crate::shown::escape_char(c)),
        }
    }
    out
}

/// `mcpseen._safe`: escaped, runs of spaces collapsed, ends stripped.
fn safe(text: &str) -> String {
    let escaped = crate::shown::escaped(text);
    let mut out = String::with_capacity(escaped.len());
    let mut last_space = false;
    for c in escaped.chars() {
        if c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out.trim_matches(' ').to_string()
}

/// `mcpseen._tok`: a bare word, or quoted when it is empty or holds a space.
fn tok(text: &str) -> String {
    let shown = esc(text);
    if !shown.is_empty() && !shown.contains(' ') {
        shown
    } else {
        format!("\"{shown}\"")
    }
}

/// Python's `json.dumps` of one number.
fn number(n: &serde_json::Number) -> String {
    crate::pyjson::dumps(&Value::Number(n.clone()), None, ", ", ": ")
}

/// `mcpseen._val`: any JSON value as one self-delimiting piece of a consent line.
fn val(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{}\"", esc(s)),
        Value::Null => "null".into(),
        Value::Bool(b) => if *b { "true" } else { "false" }.into(),
        Value::Number(n) => esc(&number(n)),
        Value::Array(items) => {
            format!("[{}]", items.iter().map(val).collect::<Vec<_>>().join(", "))
        }
        Value::Object(map) => format!(
            "{{{}}}",
            sorted(map)
                .into_iter()
                .map(|(k, v)| format!("{}: {}", val(&Value::String(k.clone())), val(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// `mcpseen._sorted`: items ordered by `(key, _val(value))`.
fn sorted(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut items: Vec<(&String, &Value)> = map.iter().collect();
    items.sort_by(|a, b| (a.0, val(a.1)).cmp(&(b.0, val(b.1))));
    items
}

/// `mcpseen._pairs`: `"VAR"="value", …`.
fn pairs(map: &Map<String, Value>) -> String {
    sorted(map)
        .into_iter()
        .map(|(k, v)| format!("{}={}", val(&Value::String(k.clone())), val(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Python's `str(value)` for the one question it is asked here: whether it renders as
/// anything. Only a string can be blank — `str()` of any other value, `None` and `0`
/// included, is a word.
fn names_anything(value: &Value) -> bool {
    match value {
        Value::String(s) => !safe(s).is_empty(),
        _ => true,
    }
}

/// `mcpseen._names_something`: the entry names a command, an argument or a URL. `command`
/// and `url` are taken `or ""`, and a non-list `args` is `[raw] if raw else []`, so a falsy
/// one of those names nothing; every element of a list `args` is asked.
fn names_something(entry: &Map<String, Value>) -> bool {
    let truthy = |v: &&Value| crate::dispatch::truthy(Some(v));
    let mut parts: Vec<&Value> = Vec::new();
    parts.extend(entry.get("command").filter(truthy));
    match entry.get("args") {
        Some(Value::Array(items)) => parts.extend(items.iter()),
        other => parts.extend(other.filter(truthy)),
    }
    parts.extend(entry.get("url").filter(truthy));
    parts.into_iter().any(names_anything)
}

/// The keys [`describe`] has a readable form for — `mcpseen._READABLE`.
const READABLE: [&str; 7] = [
    "command",
    "args",
    "type",
    "url",
    "env",
    "secrets",
    "secret_files",
];

/// `mcpseen.describe`: the consent line for one entry — every key it holds, charter's own
/// words bare and committed text quoted — or `""` when it names nothing or would not fit on
/// the screen the question is asked on.
pub fn describe(vault: Option<&str>, entry: &Value) -> String {
    let Some(entry) = entry.as_object() else {
        return String::new();
    };
    if !names_something(entry) {
        return String::new();
    }
    let mut spent: BTreeSet<&str> = BTreeSet::new();
    let mut shown: Vec<String> = Vec::new();
    if let Some(Value::String(command)) = entry.get("command") {
        spent.insert("command");
        let mut words = vec![tok(command)];
        if let Some(Value::Array(argv)) = entry.get("args")
            && argv.iter().all(Value::is_string)
        {
            spent.insert("args");
            words.extend(argv.iter().filter_map(Value::as_str).map(tok));
        }
        shown.push(format!("run {}", words.join(" ")));
    }
    for key in READABLE {
        let Some(value) = entry.get(key) else {
            continue;
        };
        if spent.contains(key) {
            continue;
        }
        spent.insert(key);
        let body = match value {
            Value::Object(map)
                if matches!(key, "env" | "secrets" | "secret_files") && !map.is_empty() =>
            {
                pairs(map)
            }
            other => val(other),
        };
        shown.push(format!("{key} {body}"));
    }
    let mut rest: Vec<&String> = entry
        .keys()
        .filter(|k| !spent.contains(k.as_str()))
        .collect();
    rest.sort();
    for key in rest {
        shown.push(format!(
            "{} {}",
            val(&Value::String(key.clone())),
            val(&entry[key])
        ));
    }
    shown.push(format!(
        "vault {}",
        val(&Value::String(vault.unwrap_or("None").to_string()))
    ));
    let line = shown.join("  ");
    if line.chars().count() > MAX_LINE {
        String::new()
    } else {
        line
    }
}

/// `mcpseen.fingerprint`: the SHA-256 of the consent line, or `None` for an entry that needs
/// no consent or cannot be shown.
pub fn fingerprint(vault: Option<&str>, entry: &Value) -> Option<String> {
    use sha2::Digest as _;
    if !needs_consent(vault, entry) {
        return None;
    }
    let line = describe(vault, entry);
    if line.is_empty() {
        return None;
    }
    Some(format!("{:x}", sha2::Sha256::digest(line.as_bytes())))
}

/// `mcpseen._name`: one part of a label, escaped and clipped.
fn label_part(part: &str) -> String {
    let mut shown = safe(part);
    if shown.chars().count() > MAX_NAME {
        shown = shown.chars().take(MAX_NAME - 3).collect::<String>() + "...";
    }
    if shown.is_empty() {
        "\"\"".into()
    } else {
        shown
    }
}

/// `mcpseen.label`: `persona/server`, each part escaped and clipped.
pub fn label(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|p| label_part(p))
        .collect::<Vec<_>>()
        .join("/")
}

/// The approvals file — `mcpseen.path()`.
pub fn approvals_path(state: &Path) -> PathBuf {
    state.join(APPROVED_FILE)
}

fn approvals(state: &Path) -> Map<String, Value> {
    std::fs::read_to_string(approvals_path(state))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// `mcpseen.approved`: the fingerprints this machine approved for `persona`.
pub fn approved(state: &Path, persona: &str) -> BTreeSet<String> {
    approvals(state)
        .get(persona)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// `mcpseen.approve`: record `fingerprints` as `persona`'s approved set, REPLACING what was
/// there, written as `json.dumps(doc, indent=2, ensure_ascii=False)` at 0600.
pub fn approve(plane: &Path, state: &Path, persona: &str, fingerprints: &[String]) {
    let mut doc = approvals(state);
    let set: BTreeSet<&String> = fingerprints.iter().filter(|f| !f.is_empty()).collect();
    doc.insert(
        persona.to_string(),
        Value::Array(set.into_iter().map(|f| Value::String(f.clone())).collect()),
    );
    // `json.dumps(doc, indent=2, ensure_ascii=False) + "\n"`; the writer adds the newline.
    let text = crate::pyjson::dumps_indent2_unicode(&Value::Object(doc));
    if crate::plane::private_dir(plane, state).is_ok() {
        let _ = crate::plane::write_private(plane, &approvals_path(state), text.as_bytes());
    }
}

/// `persona.mcp_render_entry`: one declared server as the generated agent carries it.
/// `secrets`/`secret_files` never reach the agent; a credentialed server whose consent line
/// this machine approved is wrapped in `charter secret exec <vault> … --`, and every other
/// one is passed through with its credentials withheld.
pub fn render_entry(vault: Option<&str>, entry: &Value, approved: &BTreeSet<String>) -> Value {
    let Some(map) = entry.as_object() else {
        return entry.clone();
    };
    // Rebuilt rather than `remove`d: removal from an insertion-ordered map moves the last key
    // into the hole, and the order of this object is the order its JSON is written in.
    let mut out: Map<String, Value> = map
        .iter()
        .filter(|(k, _)| k.as_str() != "secrets" && k.as_str() != "secret_files")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let vault = vault_for(vault);
    let secrets = non_empty_object(map.get("secrets"));
    let files = non_empty_object(map.get("secret_files"));
    let Some(vault) = vault else {
        return Value::Object(out);
    };
    if secrets.is_none() && files.is_none() {
        return Value::Object(out);
    }
    match fingerprint(Some(&vault), entry) {
        Some(fp) if approved.contains(&fp) => {}
        _ => return Value::Object(out),
    }
    // f"{env}={key}" is Python's `str()` of whatever the file holds: `None`, `True`, a list
    // as `[1, 'x']`.
    let text = crate::pyrepr::str_json;
    let mut args: Vec<Value> = vec!["secret".into(), "exec".into(), vault.clone().into()];
    for (env, key) in secrets.into_iter().flatten() {
        args.push("--env".into());
        args.push(format!("{env}={}", text(key)).into());
    }
    for (env, key) in files.into_iter().flatten() {
        args.push("--file".into());
        args.push(format!("{env}={}", text(key)).into());
    }
    args.push(
        if files.is_some() {
            "--stream"
        } else {
            "--exec"
        }
        .into(),
    );
    args.push("--".into());
    if let Some(command) = out
        .get("command")
        .filter(|c| crate::dispatch::truthy(Some(c)))
    {
        args.push(command.clone());
    }
    // `list(out.get("args") or [])`: a list as it is, a string as its characters and an
    // object as its keys — what Python's `list()` makes of each.
    match out.get("args") {
        Some(Value::Array(original)) => args.extend(original.iter().cloned()),
        Some(Value::String(s)) => args.extend(s.chars().map(|c| Value::String(c.to_string()))),
        Some(Value::Object(m)) => args.extend(m.keys().map(|k| Value::String(k.clone()))),
        _ => {}
    }
    out.insert("command".into(), "charter".into());
    out.insert("args".into(), Value::Array(args));
    Value::Object(out)
}

/// One credentialed server: its name, entry, fingerprint and consent line —
/// `persona.mcp_credentialed`'s tuple.
pub struct Credentialed {
    pub server: String,
    pub fingerprint: Option<String>,
    pub line: String,
}

/// `persona.mcp_credentialed`: every server of `name` that would carry a credential, by
/// server name.
pub fn credentialed(root: &Path, name: &str) -> Vec<Credentialed> {
    let vault = super::resolve(root, name).and_then(|r| vault_for(r.get("vault")));
    let (servers, _) = declared(root, name);
    let mut out: Vec<Credentialed> = servers
        .iter()
        .filter(|(_, entry)| needs_consent(vault.as_deref(), entry))
        .map(|(server, entry)| Credentialed {
            server: server.clone(),
            fingerprint: fingerprint(vault.as_deref(), entry),
            line: describe(vault.as_deref(), entry),
        })
        .collect();
    out.sort_by(|a, b| a.server.cmp(&b.server));
    out
}

/// `persona.mcp_withheld`: `(server, consent line)` for each credentialed server this machine
/// has not approved.
pub fn withheld(root: &Path, state: &Path, name: &str) -> Vec<(String, String)> {
    let ok = approved(state, name);
    credentialed(root, name)
        .into_iter()
        .filter(|c| c.fingerprint.as_ref().is_none_or(|fp| !ok.contains(fp)))
        .map(|c| (c.server, c.line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_ceiling_is_a_screen_less_the_label() {
        assert_eq!(MAX_LINE, 722);
    }

    #[test]
    fn a_server_name_is_an_identifier_and_nothing_else() {
        assert!(name_ok("grafana"));
        assert!(name_ok("_a.b-c"));
        assert!(!name_ok("bad name"));
        assert!(!name_ok("ok\n"));
        assert!(!name_ok(".dot"));
        assert!(!name_ok(&"a".repeat(65)));
        assert!(name_ok(&"a".repeat(64)));
    }

    #[test]
    fn the_consent_line_reads_like_the_command_it_authorises() {
        let entry = json!({"type": "stdio", "command": "uvx", "args": ["gsc-mcp==0.3.0"],
            "secret_files": {"GOOGLE_APPLICATION_CREDENTIALS": "GOOGLE_SA"}});
        assert_eq!(
            describe(Some("ops"), &entry),
            "run uvx gsc-mcp==0.3.0  type \"stdio\"  secret_files \
             \"GOOGLE_APPLICATION_CREDENTIALS\"=\"GOOGLE_SA\"  vault \"ops\""
        );
    }

    #[test]
    fn a_word_with_a_space_or_nothing_in_it_is_quoted_and_every_key_is_shown() {
        let entry = json!({"command": "npx", "args": ["my server", ""], "zeta": [1, true, null],
            "alpha": {"b": 2, "a": "é"}});
        assert_eq!(
            describe(None, &entry),
            "run npx \"my server\" \"\"  \"alpha\" {\"a\": \"\\u00e9\", \"b\": 2}  \
             \"zeta\" [1, true, null]  vault \"None\""
        );
    }

    #[test]
    fn an_entry_naming_nothing_has_no_line_and_no_fingerprint() {
        let entry = json!({"command": "   ", "secrets": {"T": "k"}});
        assert_eq!(describe(Some("v"), &entry), "");
        assert_eq!(fingerprint(Some("v"), &entry), None);
    }

    #[test]
    fn an_approved_server_is_wrapped_and_keeps_its_other_keys_in_place() {
        let entry = json!({"type": "stdio", "command": "npx", "args": ["-y", "g@1"],
            "env": {"U": "x"}, "secrets": {"T": "tok"}});
        let fp = fingerprint(Some("ops"), &entry).unwrap();
        let ok: BTreeSet<String> = [fp].into();
        let out = render_entry(Some("ops"), &entry, &ok);
        assert_eq!(
            crate::pyjson::dumps(&out, None, ", ", ": "),
            r#"{"type": "stdio", "command": "charter", "args": ["secret", "exec", "ops", "--env", "T=tok", "--exec", "--", "npx", "-y", "g@1"], "env": {"U": "x"}}"#
        );
        let withheld = render_entry(Some("ops"), &entry, &BTreeSet::new());
        assert_eq!(
            crate::pyjson::dumps(&withheld, None, ", ", ": "),
            r#"{"type": "stdio", "command": "npx", "args": ["-y", "g@1"], "env": {"U": "x"}}"#
        );
        let no_vault = render_entry(Some("none"), &entry, &BTreeSet::new());
        assert_eq!(no_vault, withheld);
    }

    #[test]
    fn a_label_is_escaped_and_clipped() {
        assert_eq!(label(&["ops", "bad name"]), "ops/bad name");
        assert_eq!(label(&["ops", "\u{3164}"]), "ops/\\u3164");
        assert_eq!(label(&["ops", "   "]), "ops/\"\"");
        assert_eq!(label(&[&"x".repeat(40)]), format!("{}...", "x".repeat(32)));
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod recorded;
