//! Streams as the recording compared them: newlines, masks, and the side's own directory.

use std::path::Path;

use regex::Regex;
use serde_json::Value;

/// The run's own directories, and the app's version, as the tokens the fixture holds instead
/// of them.
///
/// Each run's scratch directory is somewhere new, so the recorded texts say `<side>` where the
/// side's directory was and `<scratch>` where the directory holding it was — each in both
/// spellings, as made and as `realpath` gives it, longest first, because on macOS one is a
/// prefix of the other (`/var/…` and `/private/var/…`). The side goes first: it is inside the
/// scratch directory, and the resolved scratch spelling can be longer than the side's own. The
/// version is `<app-version>`, so a release does not rewrite every row that prints it.
pub struct Tokens {
    spellings: Vec<(String, &'static str)>,
    side: String,
    scratch: String,
    version: &'static str,
}

pub const SIDE: &str = "<side>";
pub const SCRATCH: &str = "<scratch>";
pub const VERSION: &str = "<app-version>";

impl Tokens {
    pub fn new(side: &Path) -> Tokens {
        let scratch = side
            .parent()
            .expect("a side is inside its scratch directory");
        let mut spellings = Vec::new();
        for (dir, token) in [(side, SIDE), (scratch, SCRATCH)] {
            let mut these = vec![dir.to_string_lossy().into_owned()];
            if let Ok(real) = dir.canonicalize() {
                these.push(real.to_string_lossy().into_owned());
            }
            these.sort_by_key(|s| std::cmp::Reverse(s.len()));
            these.dedup();
            spellings.extend(these.into_iter().map(|s| (s, token)));
        }
        Tokens {
            spellings,
            side: side.to_string_lossy().into_owned(),
            scratch: scratch.to_string_lossy().into_owned(),
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    /// A path or a path-bearing string, with the run's directories as tokens.
    pub fn paths(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for (spelling, token) in &self.spellings {
            text = text.replace(spelling.as_str(), token);
        }
        text
    }

    /// A stream: the run's directories as tokens and the app's version as `<app-version>`.
    pub fn stream(&self, text: &str) -> String {
        version_as_token(&self.paths(text), self.version)
    }

    /// The inverse of [`Tokens::paths`], for what the fixture says to lay down or run.
    pub fn unpaths(&self, text: &str) -> String {
        text.replace(SIDE, &self.side)
            .replace(SCRATCH, &self.scratch)
    }

    /// File contents, with the run's directories as tokens.
    pub fn bytes(&self, data: &[u8]) -> Vec<u8> {
        let mut data = data.to_vec();
        for (spelling, token) in &self.spellings {
            data = replace_bytes(&data, spelling.as_bytes(), token.as_bytes());
        }
        data
    }

    pub fn unbytes(&self, data: &[u8]) -> Vec<u8> {
        let data = replace_bytes(data, SIDE.as_bytes(), self.side.as_bytes());
        replace_bytes(&data, SCRATCH.as_bytes(), self.scratch.as_bytes())
    }
}

/// `text` with each standalone `version` as `<app-version>`. Standalone means not inside
/// another version or identifier: a fixture's own `gsc-mcp==0.3.0` is a different package's
/// pin, and on the release that shares its number it must stay as recorded.
fn version_as_token(text: &str, version: &str) -> String {
    let joined = |c: char| c.is_ascii_alphanumeric() || "=.-+@".contains(c);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(version) {
        let before = rest[..at].chars().next_back().or(out.chars().next_back());
        let after = &rest[at + version.len()..];
        let mut next = after.chars();
        let continues = match next.next() {
            Some('.') => next.next().is_some_and(|c| c.is_ascii_digit()),
            Some(c) => c.is_ascii_alphanumeric(),
            None => false,
        };
        out.push_str(&rest[..at]);
        if before.is_some_and(joined) || continues {
            out.push_str(version);
        } else {
            out.push_str(VERSION);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Checked before any scenario runs: this runner is not libtest's, so a `#[test]` here would never run.
pub fn only_the_apps_own_version_becomes_the_token() {
    let v = "0.3.0";
    assert_eq!(
        version_as_token("charter    0.3.0\n", v),
        "charter    <app-version>\n"
    );
    assert_eq!(
        version_as_token("version = \"0.3.0\"", v),
        "version = \"<app-version>\""
    );
    assert_eq!(
        version_as_token("charter 0.3.0.", v),
        "charter <app-version>."
    );
    assert_eq!(
        version_as_token("uvx gsc-mcp==0.3.0  type", v),
        "uvx gsc-mcp==0.3.0  type"
    );
    assert_eq!(
        version_as_token("10.3.0 0.3.0.1 v0.3.0", v),
        "10.3.0 0.3.0.1 v0.3.0"
    );
    assert_eq!(
        version_as_token("charter 0.3.0-dev.4", v),
        "charter <app-version>-dev.4"
    );
}

fn replace_bytes(data: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    if from.is_empty() || data.len() < from.len() {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

/// A captured stream as the recording read it: UTF-8, with Python's universal newlines —
/// `subprocess.run(text=True)` turned every `\r\n` and lone `\r` into `\n` before anything
/// was compared, so the recorded texts hold no `\r` at all.
pub fn captured(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// `[pattern, replacement]` pairs, applied in order.
pub fn masks(value: &Value) -> Vec<(Regex, String)> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|pair| {
            let pattern = pair[0].as_str().expect("a mask's pattern");
            let replacement = pair[1].as_str().expect("a mask's replacement");
            (
                Regex::new(pattern).unwrap_or_else(|e| panic!("mask {pattern:?}: {e}")),
                replacement.to_owned(),
            )
        })
        .collect()
}

pub fn masked(text: &str, masks: &[(Regex, String)]) -> String {
    let mut text = text.to_owned();
    for (pattern, replacement) in masks {
        text = pattern
            .replace_all(&text, replacement.as_str())
            .into_owned();
    }
    text
}

/// Split into lines, keeping each line's own `\n` — Python's `splitlines(keepends=True)` for
/// text that has already been through [`captured`].
pub fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The `permissionDecisionReason` a `PreToolUse` hook printed, or `None` for anything that is
/// not a deny.
pub fn decision(stdout: &str) -> Option<String> {
    if stdout.trim().is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(stdout).ok()?;
    let out = value.get("hookSpecificOutput")?;
    if out.get("permissionDecision")?.as_str()? != "deny" {
        return None;
    }
    out.get("permissionDecisionReason")?
        .as_str()
        .map(str::to_owned)
}

/// What a hook's stdout SAYS: the raw stream, and every string in each line of JSON it
/// printed, decoded.
pub fn spoken(stdout: &str) -> String {
    fn leaves(value: &Value, said: &mut Vec<String>) {
        match value {
            Value::String(s) => said.push(s.clone()),
            Value::Array(items) => items.iter().for_each(|v| leaves(v, said)),
            Value::Object(map) => map.values().for_each(|v| leaves(v, said)),
            _ => {}
        }
    }
    let mut said = vec![stdout.to_owned()];
    for line in stdout.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            leaves(&value, &mut said);
        }
    }
    said.join("\n")
}

/// What makes a status-line row an alert row: the `⚠` each opens with, reset straight after.
pub const ALERT_MARK: &str = "⚠\x1b[0m ";

/// The first lines where two texts part, for a failure message.
pub fn diff(want: &str, got: &str) -> Vec<String> {
    let w: Vec<&str> = want.split('\n').collect();
    let g: Vec<&str> = got.split('\n').collect();
    let first = w
        .iter()
        .zip(&g)
        .position(|(a, b)| a != b)
        .unwrap_or(w.len().min(g.len()));
    let mut out = vec![format!("      first difference at line {}:", first + 1)];
    for (label, side) in [("recorded", &w), ("now     ", &g)] {
        for line in side.iter().skip(first).take(4) {
            out.push(format!("      {label} {line:?}"));
        }
        if side.len() <= first {
            out.push(format!("      {label} <end>"));
        }
    }
    out
}
