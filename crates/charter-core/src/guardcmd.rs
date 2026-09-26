//! `charter guard ask|allow|handoff|list`: the plane's force-prompt and stop-prompting rules,
//! written in each harness's own syntax (#364).
//!
//! A port of the Python charter's `cmd_guard_*` (`commands.py` at `cli-final`), with its rules:
//!
//! - **charter keeps no list of its own** (ADR 0014). The rule goes into the file each harness
//!   reads — Claude Code's `permissions.<bucket>` in `.claude/settings.json`, opencode's
//!   `permission.bash` in `opencode.json` — through the same writers `charter init` uses
//!   ([`crate::scaffold::settings`]), which touch only the one key and never repair a file.
//! - **Every harness charter knows, not only the running one.** Codex has no command-pattern
//!   permissions, and says so; opencode has no machine-local file, and says so for `--local`.
//! - **All or nothing across harnesses.** Each is asked first with the write path minus the
//!   write; a file one of them cannot read blocks every write, so the plane is never left with
//!   the rule in one harness and not the other because of a broken file.
//! - **An `allow` rule reaches the plane root only.** A workspace's and a clone's generated
//!   settings carry the plane's `ask` and `deny`, never `allow` ([`crate::layer`]), because a
//!   grant copied sideways is a permission nobody clicked for. The command says so.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::scaffold::settings::{self, Wrote};

/// The pattern a handoff's consent rule names; `charter guard handoff` writes it.
pub const HANDOFF_PATTERN: &str = settings::HANDOFF_PATTERN;

/// The tools a Claude Code rule can name bare or as `Tool(pattern)`.
const RULE_TOOLS: [&str; 9] = [
    "Bash",
    "Read",
    "Edit",
    "Write",
    "Grep",
    "Glob",
    "WebFetch",
    "NotebookEdit",
    "Task",
];

/// Which way a rule decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    /// Always prompt first.
    Ask,
    /// Stop prompting.
    Allow,
}

impl Bucket {
    fn key(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Allow => "allow",
        }
    }
}

/// `commands._as_rule`: a Claude Code rule for `pattern` — `Tool(pattern)`, a bare tool name
/// or an MCP name as they are, and any other command wrapped as `Bash(...)`. An MCP name with
/// a wildcard or arguments is refused: no rule can say it, and wrapping it would write one
/// that matches nothing.
pub fn as_rule(pattern: &str) -> Result<String, String> {
    let p = pattern.trim();
    if p.is_empty() {
        return Err("Nothing to add.".to_owned());
    }
    let tool_rule = RULE_TOOLS.iter().any(|tool| {
        p.strip_prefix(tool)
            .is_some_and(|rest| rest.starts_with('(') && p.ends_with(')'))
    });
    let mcp = |s: &str| {
        s.strip_prefix("mcp__").is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
    };
    if tool_rule || RULE_TOOLS.contains(&p) || mcp(p) {
        return Ok(p.to_owned());
    }
    if p.starts_with("mcp__") {
        return Err(format!(
            "charter cannot write {p:?} as a permission rule. An MCP rule names a server \
             (`mcp__slack`) or one of its tools (`mcp__slack__send`) exactly — no wildcard and \
             no arguments. Wrapping it as `Bash(...)` would write a rule that matches nothing, \
             so charter writes nothing."
        ));
    }
    Ok(format!("Bash({p})"))
}

/// What one harness answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Written (or, in the check, would be), to this file.
    Added(PathBuf),
    /// Already so in this file.
    Present(PathBuf),
    /// The harness has nowhere to put this rule, and never will: said, not resolved.
    Unsupported(String),
    /// A file charter cannot read the way the harness reads it; nothing was written anywhere.
    Malformed(String),
    /// The file passed the check and then would not take the write.
    Unwritable(String),
}

fn answer(wrote: Wrote, path: &Path) -> Answer {
    match wrote {
        Wrote::Created => Answer::Added(path.to_path_buf()),
        Wrote::Present => Answer::Present(path.to_path_buf()),
        Wrote::Malformed(what) => Answer::Malformed(what),
        Wrote::Blocked(dir) => Answer::Unwritable(format!("{} is not a directory", dir.display())),
        Wrote::Failed(path, e) => Answer::Unwritable(format!("{} ({e})", path.display())),
    }
}

/// One harness's rule writer.
fn write(
    harness: &str,
    root: &Path,
    rule: &str,
    bucket: Bucket,
    local: bool,
    dry_run: bool,
) -> Answer {
    // A settings file reached through a link — `.claude` pointing out of the plane — is
    // refused in the check and the write alike, as `charter init` refuses it: a rule written
    // through it would land in somebody else's file.
    let gate = |path: &Path| {
        crate::contain::no_link_on_the_way(root, path)
            .err()
            .map(|e| {
                Answer::Malformed(format!(
                    "{} (reached through a link, which charter does not write through: {e})",
                    path.display()
                ))
            })
    };
    match harness {
        "claude-code" => {
            let path = root.join(if local {
                ".claude/settings.local.json"
            } else {
                settings::SETTINGS
            });
            if let Some(refused) = gate(&path) {
                return refused;
            }
            answer(
                settings::ensure_rule(&path, bucket.key(), rule, dry_run),
                &path,
            )
        }
        "opencode" => {
            if local {
                return Answer::Unsupported(
                    "opencode has no machine-local settings file, so a --local rule cannot \
                     reach it"
                        .to_owned(),
                );
            }
            // opencode's `permission.bash` holds command globs; any other tool's rule has no
            // place there.
            let Some(glob) = rule.strip_prefix("Bash(").and_then(|r| r.strip_suffix(')')) else {
                return Answer::Unsupported(format!(
                    "opencode's config holds command patterns, and {rule} is not one"
                ));
            };
            let path = root.join(settings::OPENCODE);
            if let Some(refused) = gate(&path) {
                return refused;
            }
            answer(
                settings::ensure_opencode_rule(root, glob, bucket.key(), dry_run),
                &path,
            )
        }
        _ => Answer::Unsupported(
            "Codex has no command-pattern permissions, so charter's own guard is what applies \
             there"
                .to_owned(),
        ),
    }
}

/// The harnesses a rule is written for, in charter's registry order.
pub const HARNESSES: [&str; 3] = ["claude-code", "opencode", "codex"];

/// `commands._guard_apply`: give `rule` to every harness that can hold it, or to none.
/// Returns each harness's answer, and whether nothing was written because one refused.
pub fn apply(
    root: &Path,
    rule: &str,
    bucket: Bucket,
    local: bool,
) -> (Vec<(&'static str, Answer)>, bool) {
    let checked: Vec<(&'static str, Answer)> = HARNESSES
        .iter()
        .map(|h| (*h, write(h, root, rule, bucket, local, true)))
        .collect();
    if checked
        .iter()
        .any(|(_, a)| matches!(a, Answer::Malformed(_)))
    {
        return (checked, true);
    }
    let committed = checked
        .into_iter()
        .map(|(h, a)| match a {
            Answer::Added(_) => (h, write(h, root, rule, bucket, local, false)),
            other => (h, other),
        })
        .collect();
    (committed, false)
}

/// What `charter guard ask|allow` prints for `rule`, and its exit code.
pub fn report(root: &Path, rule: &str, bucket: Bucket, local: bool) -> (String, u8) {
    let (answers, blocked) = apply(root, rule, bucket, local);
    let verb = match bucket {
        Bucket::Ask => "asking for",
        Bucket::Allow => "allowing",
    };
    let mut out = String::new();
    let mut code = 0u8;
    let mut wrote = false;
    if blocked {
        for (h, a) in &answers {
            if let Answer::Malformed(what) = a {
                out.push_str(&format!(
                    "\u{2717} {h}: {} is not valid — left untouched.\n",
                    crate::shown::one_line(what, 1024)
                ));
            }
        }
        let already: Vec<&str> = answers
            .iter()
            .filter(|(_, a)| matches!(a, Answer::Present(_)))
            .map(|(h, _)| *h)
            .collect();
        let pending: Vec<&str> = answers
            .iter()
            .filter(|(_, a)| matches!(a, Answer::Added(_)))
            .map(|(h, _)| *h)
            .collect();
        out.push_str("  Nothing was written.");
        if !already.is_empty() {
            out.push_str(&format!(
                " {rule} is already in force under {}, so the plane is uneven right now.",
                already.join(", ")
            ));
        }
        if !pending.is_empty() {
            out.push_str(&format!(" Still without it: {}.", pending.join(", ")));
        }
        out.push_str(" Fix that file by hand, then re-run; charter never repairs these files.\n");
        return (out, 1);
    }
    for (h, a) in &answers {
        match a {
            Answer::Added(path) => {
                wrote = true;
                out.push_str(&format!(
                    "\u{2713} {h}: {verb} {rule} \u{2192} {}\n",
                    crate::shown::one_line(&path.display().to_string(), 1024)
                ));
            }
            Answer::Present(_) => {
                wrote = true;
                out.push_str(&format!("\u{2713} {h}: already {verb} {rule}.\n"));
            }
            Answer::Unsupported(why) => out.push_str(&format!("  {h}: {why}.\n")),
            // Passed the check, and changed under the command before the write.
            Answer::Malformed(what) => {
                code = 1;
                out.push_str(&format!(
                    "\u{2717} {h}: {} is not valid — left untouched.\n",
                    crate::shown::one_line(what, 1024)
                ));
            }
            Answer::Unwritable(what) => {
                code = 1;
                out.push_str(&format!(
                    "\u{2717} {h}: could not write {} — left untouched. It passed the check \
                     before any file was written; re-run once it can be written.\n",
                    crate::shown::one_line(what, 1024)
                ));
            }
        }
    }
    if !wrote && code == 0 {
        out.push_str("! No harness took the rule.\n");
    }
    if wrote && code == 1 {
        out.push_str(
            "! The plane is uneven: the rule is in force under the harnesses marked \u{2713} and \
             not under the ones marked \u{2717}. Re-run once those files can be written.\n",
        );
    }
    if wrote {
        match (bucket, local) {
            (_, true) => out.push_str(
                "  Machine-local (.claude/settings.local.json, not committed) — this rule is \
                 yours, on this machine.\n",
            ),
            (Bucket::Ask, false) => out.push_str(
                "  These files are committed, so the rule applies to everyone on this repo \
                 (ADR 0014). A workspace carries it once `charter workspace reinit --all` \
                 rewrites its settings.\n",
            ),
            (Bucket::Allow, false) => out.push_str(
                "! COMMITTED — this stops the prompt for everyone on this repo, not just you. \
                 Use --local for a rule that is yours alone.\n",
            ),
        }
        if bucket == Bucket::Allow {
            out.push_str(
                "  It reaches a chat at the plane root only: a workspace's and a clone's \
                 generated settings carry ask and deny rules, never allow, so a chat started \
                 there is still asked.\n  charter's own guards are unaffected: an allow rule \
                 relaxes the harness's prompt, never the secret, credential, plane-root or \
                 release denials.\n",
            );
            if rules(&root.join(settings::SETTINGS), "ask").contains(&rule.to_owned())
                || rules(&root.join(".claude/settings.local.json"), "ask")
                    .contains(&rule.to_owned())
            {
                out.push_str(&format!(
                    "! {rule} is also an ask rule, and Claude Code weighs ask before allow: it \
                     still prompts.\n"
                ));
            }
        }
    }
    (out, code)
}

/// The string rules in `permissions.<bucket>` of the settings file at `path`, tolerating
/// every other shape — a reader reports what it can read and never fails over the rest.
pub fn rules(path: &Path, bucket: &str) -> Vec<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| crate::pyjson::loads_strict(&raw))
        .and_then(|doc| doc.get("permissions")?.get(bucket)?.as_array().cloned())
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `charter guard list`: the ask and allow rules in the plane's two Claude Code files,
/// grouped by file — the file a rule lives in is its reach — and whether the listing is whole
/// (`false` when a file could not be read, so a script does not take it as complete).
pub fn list(root: &Path) -> (String, bool) {
    let mut out = String::new();
    let mut whole = true;
    for (rel, reach) in [
        (settings::SETTINGS, "committed: everyone on this repo"),
        (".claude/settings.local.json", "this machine only"),
    ] {
        let path = root.join(rel);
        out.push_str(&format!("{rel} ({reach})\n"));
        if !path.exists() {
            out.push_str("  (no file)\n");
            continue;
        }
        if std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| crate::pyjson::loads_strict(&raw))
            .is_none_or(|doc| !doc.is_object())
        {
            out.push_str("  not a JSON object charter can read — its rules are not listed\n");
            whole = false;
            continue;
        }
        let mut any = false;
        for bucket in ["ask", "allow"] {
            for rule in rules(&path, bucket) {
                any = true;
                out.push_str(&format!(
                    "  {bucket:<5} {}\n",
                    crate::shown::one_line(&rule, 1024)
                ));
            }
        }
        if !any {
            out.push_str("  (no ask or allow rules)\n");
        }
    }
    (out, whole)
}

#[cfg(test)]
mod tests;
