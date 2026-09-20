//! `session root` and `session layer`: which directory answered for this session's settings,
//! and what of charter's layer a session started here would find.
//!
//! Both are facts, never verdicts — OK even when the answer is "none of it". A chat rooted in
//! a workspace is the designed workflow, and a row that warns on the normal case is a row
//! operators learn to skip. Whatever is actually missing warns on its own row.
//!
//! **The discovery rules are the harnesses' own, as measured and declared in
//! `charter/harness/*.py`** (`Harness.layer`, `layer_note`, `trust_gate`) — copied here as
//! data, one entry per registered harness, because both rows narrate one set of rules and
//! two sources for it is how #879 happened.

use std::path::{Path, PathBuf};

use super::{Doctor, Row, canonical, short_path};

/// One artefact of charter's in-repo layer and the rule a harness finds it by —
/// `harness.base.LayerPart`.
struct Part {
    what: &'static str,
    /// Relative to the directory asked about; any one satisfies the part.
    paths: &'static [&'static str],
    /// Walk up to the git root, or read the directory alone.
    walks: bool,
    /// Printed only when the part does NOT reach: the measured rule, not a remedy.
    why: &'static str,
    /// When set, the file must be a JSON object carrying one of these top-level keys.
    keys: &'static [&'static str],
}

/// A registered harness, as far as these two rows need one.
struct Harness {
    name: &'static str,
    layer: &'static [Part],
    /// Where the layer comes from, for a harness charter declares no in-repo parts for.
    note: &'static str,
    /// What the harness gates on a directory being trusted, where that is measured.
    trust_gate: &'static str,
}

/// Claude Code's layer, measured on 2.1.259 (the settings rule re-measured on 2.1.267).
static CLAUDE_LAYER: [Part; 2] = [
    Part {
        what: "settings",
        paths: &[".claude/settings.json", ".claude/settings.local.json"],
        walks: false,
        why: "project settings are read from the session's OWN directory and the host does \
              not walk up, so nothing above it is in force",
        keys: &["enabledPlugins", "env"],
    },
    Part {
        what: "skills+agents",
        paths: &[".claude/skills", ".claude/agents"],
        walks: true,
        why: "skills and agents DO walk up, but the walk stops at the git root — anything \
              charter wrote above that boundary is out of reach, while CLAUDE.md walks up and \
              is NOT git-bounded, which is why a session like this reads as half-configured \
              rather than empty",
        keys: &[],
    },
];

/// Every registered harness, in registration order — `harness.registry.KINDS`.
static HARNESSES: [Harness; 3] = [
    Harness {
        name: "claude-code",
        layer: &CLAUDE_LAYER,
        note: "",
        trust_gate: "hooks",
    },
    Harness {
        name: "opencode",
        layer: &[],
        note: "charter has not measured opencode's discovery rules, so this row cannot say \
               what a session here would find — the layer arrives from `~/.config/opencode/` \
               and the plugin, which every directory on this machine reads. opencode DOES \
               read an in-repo `opencode.json` at the repository root and `.opencode/agent/` \
               from the project (measured, 1.18.23); charter mirrors the plane's \
               `.opencode/agent/` into a workspace's checkouts and leaves `opencode.json` \
               alone — that file is where `charter guard` keeps this plane's own permission \
               grants",
        trust_gate: "",
    },
    Harness {
        name: "codex",
        layer: &[],
        note: "charter has not measured Codex's discovery rules, so this row cannot say what a \
               session here would find — the layer arrives from `~/.codex/config.toml` and the \
               plugin, and a project `.codex/config.toml` is ignored. Codex DOES read an \
               in-repo `.codex/skills/` (measured, 0.147.0); charter mirrors the plane's copy \
               of that into a workspace's checkouts",
        trust_gate: "",
    },
];

/// Continuation lines of a row's detail.
const MORE: &str = "\n        \u{21b3} ";

/// `registry.current`: `$CHARTER_HARNESS` verbatim — the harness is the authority on its
/// own identity, and a name charter has not met is information — or Claude Code's own
/// evidence, `$CLAUDE_PLUGIN_ROOT`.
fn current() -> Option<String> {
    let named = std::env::var("CHARTER_HARNESS")
        .ok()
        .filter(|v| !v.is_empty());
    named.or_else(|| {
        std::env::var_os("CLAUDE_PLUGIN_ROOT")
            .filter(|v| !v.is_empty())
            .map(|_| "claude-code".to_owned())
    })
}

/// The harnesses a row answers for: the running one when something names one this charter
/// knows, every registered one otherwise — and none for a runtime charter has no record of,
/// which must not be reported under somebody else's rules.
fn answering(current: Option<&str>) -> Result<Vec<&'static Harness>, String> {
    match current {
        Some(name) => match HARNESSES.iter().find(|h| h.name == name) {
            Some(h) => Ok(vec![h]),
            None => Err(name.to_owned()),
        },
        None => Ok(HARNESSES.iter().collect()),
    }
}

/// `a`, `a and b`, `a, b and c`.
fn named(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The Claude Code config folder in use: `$CLAUDE_CONFIG_DIR`, kept even when empty (Claude
/// Code reads `??`, not `||`), else `~/.claude`.
fn claude_config_home() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if dir.is_empty() => PathBuf::from("."),
        Some(dir) => PathBuf::from(dir),
        None => crate::profiles::home().unwrap_or_default().join(".claude"),
    }
}

/// The directory this session is rooted in — the process's working directory, which the
/// host resolved project settings against. Not the plane: project settings do not walk up.
fn here(d: &Doctor) -> PathBuf {
    canonical(&d.cwd)
}

/// `session root`: which directory answered for the settings rows, and whether it is the
/// plane (#851).
pub(super) fn session_root(d: &Doctor) -> Row {
    const NAME: &str = "session root";
    let here = here(d);
    if !d.has_plane {
        return Row::ok(NAME, format!("{} — no control plane found", here.display()));
    }
    if here == d.root {
        return Row::ok(NAME, format!("{} — the plane", here.display()));
    }
    let (cwd_only, walking): (Vec<&str>, Vec<&str>) = match answering(current().as_deref()) {
        Err(_) => (Vec::new(), Vec::new()),
        Ok(harnesses) => {
            let parts: Vec<&Part> = harnesses.iter().flat_map(|h| h.layer.iter()).collect();
            let mut cwd_only: Vec<&str> = Vec::new();
            let mut walking: Vec<&str> = Vec::new();
            for p in parts {
                let into = if p.walks { &mut walking } else { &mut cwd_only };
                if !into.contains(&p.what) {
                    into.push(p.what);
                }
            }
            (cwd_only, walking)
        }
    };
    let shown = here.display();
    let mut lines = vec![format!("{shown} — not the plane ({})", d.root.display())];
    let user_folder = format!("{}/", short_path(&claude_config_home()));
    if cwd_only.is_empty() {
        lines.push(format!(
            "the rows below read {shown}/.claude/ and {user_folder}, not the plane's"
        ));
    } else {
        lines.push(format!(
            "the host reads {} from the session's own directory and does not walk up, so the \
             plane's .claude/ is not in force for them — the rows below read {shown}/.claude/ \
             and {user_folder}",
            named(&cwd_only)
        ));
    }
    if !walking.is_empty() {
        lines.push(format!(
            "{} DO walk up, as far as the git root — which of them reach here is the `session \
             layer` row below, not this one",
            named(&walking)
        ));
    }
    lines.push(format!(
        "the plane is still this session's identity: personas, the vault, memory and \
         workspaces resolve to {} from anywhere inside it",
        d.root.display()
    ));
    Row::ok(NAME, lines.join(MORE))
}

/// The git repository `dir` stands in — its own worktree's root for a linked worktree — or
/// `None` when it is in none, or git could not say in time.
fn git_root_of(dir: &Path) -> Option<PathBuf> {
    let run = super::git::git_in(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    let top = crate::memstore::py_strip(&run.out);
    if !run.ok() || top.is_empty() {
        return None;
    }
    Some(canonical(Path::new(top)))
}

/// The directories a lookup from `here` covers, stopping at `bound` inclusive — only `here`
/// when there is no bound, or `here` is not under it. Under-claiming is the direction that
/// cannot mislead.
fn search_dirs(here: &Path, bound: Option<&Path>) -> Vec<PathBuf> {
    let Some(bound) = bound.filter(|b| *b != here && here.starts_with(b)) else {
        return vec![here.to_path_buf()];
    };
    let mut out = vec![here.to_path_buf()];
    for parent in here.ancestors().skip(1) {
        out.push(parent.to_path_buf());
        if parent == bound {
            break;
        }
    }
    out
}

/// A JSON object at `p`, or nothing — absent, unreadable and "not an object" are one answer:
/// the host resolves nothing from any of them.
fn json_object(p: &Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    let meta = std::fs::metadata(p).ok()?;
    if !meta.is_file() || meta.len() > 1_048_576 {
        return None;
    }
    match serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()? {
        serde_json::Value::Object(doc) => Some(doc),
        _ => None,
    }
}

/// `base.part_reaches`: would a session started in `here` find `part`?
fn reaches(part: &Part, here: &Path, bound: Option<&Path>) -> bool {
    let dirs = search_dirs(here, if part.walks { bound } else { None });
    dirs.iter().any(|dir| {
        part.paths.iter().any(|rel| {
            let p = dir.join(rel);
            if part.keys.is_empty() {
                p.exists()
            } else {
                json_object(&p).is_some_and(|doc| part.keys.iter().any(|k| doc.contains_key(*k)))
            }
        })
    })
}

/// `session layer`: can a session started HERE see charter's layer (#869, #859)? What each
/// harness would find, by its own measured rule — and, where a harness gates its hooks on
/// trust, that a directory with a git root of its own carries its own acceptance.
pub(super) fn session_layer(d: &Doctor) -> Row {
    const NAME: &str = "session layer";
    let here = here(d);
    if !d.has_plane {
        return Row::ok(NAME, format!("{} — no control plane found", here.display()));
    }
    let current = current();
    let harnesses = match answering(current.as_deref()) {
        Ok(harnesses) => harnesses,
        Err(name) => {
            return Row::ok(
                NAME,
                format!(
                    "{} — charter has no record of how {name} finds an in-repo layer, so this \
                     row has nothing to say about it",
                    here.display()
                ),
            );
        }
    };
    let bound = git_root_of(&here);
    let plane_bound = git_root_of(&d.root);
    // Its OWN git root: `workspaces/<ws>/` is a plain directory inside the plane's
    // repository and rides the plane's acceptance.
    let own_repo = bound.is_some() && bound != plane_bound;
    let mut lines = vec![format!(
        "{} — what a session started here would find in the repo",
        here.display()
    )];
    let mut gates: Vec<&str> = Vec::new();
    for h in &harnesses {
        if h.layer.is_empty() {
            let note = if h.note.is_empty() {
                "charter has not measured how this harness finds an in-repo layer"
            } else {
                h.note
            };
            lines.push(format!("{}: {note}", h.name));
            continue;
        }
        if !h.trust_gate.is_empty() && !gates.contains(&h.trust_gate) {
            gates.push(h.trust_gate);
        }
        let bits: Vec<String> = h
            .layer
            .iter()
            .map(|part| {
                if reaches(part, &here, bound.as_deref()) {
                    format!("{} \u{2713}", part.what)
                } else {
                    format!("{} \u{2717} — {}", part.what, part.why)
                }
            })
            .collect();
        lines.push(format!("{}: {}", h.name, bits.join("; ")));
    }
    if own_repo
        && !gates.is_empty()
        && let Some(bound) = &bound
    {
        lines.push(format!(
            "trust: {} is a git root of its own, so it carries its own trust acceptance — \
             until that is given, {} do not run here whatever any settings file declares. \
             `guard seen` cannot answer it: that state lives under {}, so it is per PLANE and \
             a sighting there says nothing about this directory",
            bound.display(),
            gates.join(" / "),
            super::git::state_dir(&d.root).display()
        ));
    }
    Row::ok(NAME, lines.join(MORE))
}
