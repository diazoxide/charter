//! The rows about `charter.toml` itself: `charter.toml`, `schema`, `version lock`, and which
//! forges the plane declares (which names two rows further up).

use std::path::Path;

use super::{Config, Doctor, NOT_CHECKED_HINT, Row, SCHEMA, deferred, first_line};

/// The words `[harness] default` may name, in registration order — Python's
/// `instance.launchable_harnesses`.
const LAUNCHABLE: [&str; 3] = ["claude", "opencode", "codex"];

/// The directories a plane is expected to have — `instance.BASELINE_DIRS`.
const BASELINE_DIRS: [&str; 3] = ["personas", "inventory", "workspaces"];

/// Python's truthiness of a TOML value — the `x or default` every reader of the file uses.
pub(crate) fn truthy(v: &toml::Value) -> bool {
    match v {
        toml::Value::String(s) => !s.is_empty(),
        toml::Value::Integer(i) => *i != 0,
        toml::Value::Float(f) => *f != 0.0,
        toml::Value::Boolean(b) => *b,
        toml::Value::Datetime(_) => true,
        toml::Value::Array(a) => !a.is_empty(),
        toml::Value::Table(t) => !t.is_empty(),
    }
}

/// The Python type name of a TOML value, for the sentences Python's own errors are made of.
pub(super) fn py_type(v: &toml::Value) -> &'static str {
    match v {
        toml::Value::String(_) => "str",
        toml::Value::Integer(_) => "int",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "bool",
        toml::Value::Datetime(_) => "datetime.datetime",
        toml::Value::Array(_) => "list",
        toml::Value::Table(_) => "dict",
    }
}

/// The forge a `[[forge]]` block declares, or Python's error text for it.
///
/// [`crate::forge::Forge::build`] decides it — the same resolution `discover` and the
/// one-credential policy use, with the same two sentences. What this adds is the shapes a
/// hand-edited TOML file can put where a word goes: Python reaches `KINDS.get(kind)` with
/// whatever the file held, so a list or a table is an unhashable type before it is an unknown
/// kind, and a host that is not a string is refused by value.
fn block_forge(block: &toml::Table) -> Result<crate::forge::Forge, String> {
    let kind = match block.get("kind").filter(|v| truthy(v)) {
        None => crate::forge::DEFAULT_KIND.word().to_owned(),
        Some(toml::Value::String(k)) => k.clone(),
        // `KINDS.get(kind)` hashes the value first, and a list or a table cannot be hashed.
        Some(v @ (toml::Value::Array(_) | toml::Value::Table(_))) => {
            return Err(format!("unhashable type: '{}'", py_type(v)));
        }
        Some(v) => {
            return Err(format!(
                "unknown forge kind {} — known kinds: github, gitlab",
                toml_repr(v)
            ));
        }
    };
    match block.get("host").filter(|v| truthy(v)) {
        None => crate::forge::Forge::build(&kind, None),
        Some(toml::Value::String(host)) => crate::forge::Forge::build(&kind, Some(host)),
        // A host that is not text cannot be a hostname. The kind is still resolved first, as
        // Python's `_build` does, and the refusal is charter's one sentence for it with the
        // value quoted the way Python quotes it.
        Some(other) => crate::forge::Forge::build(&kind, None)
            .and(Err(crate::forge::not_a_host_repr(&toml_repr(other)))),
    }
}

/// A TOML value as Python `repr`s it: [`crate::pyrepr::repr_str`] for text, and
/// `profiles::py_repr` for the containers and scalars it already renders Python's way.
pub(super) fn toml_repr(value: &toml::Value) -> String {
    crate::profiles::py_repr(value)
}

/// `registry.declared_forges`' errors — one per block that did not resolve, or one for the
/// whole key when it is not something a loop can walk.
fn forge_errors(cfg: &toml::Table) -> Vec<String> {
    let Some(value) = cfg.get("forge").filter(|v| truthy(v)) else {
        return Vec::new();
    };
    // What Python's `for block in value` walks: an array's items, a table's KEYS, a string's
    // characters. Each of the last two is a block that is not a table.
    let blocks: Vec<Option<&toml::Table>> = match value {
        toml::Value::Array(items) => items.iter().map(toml::Value::as_table).collect(),
        toml::Value::Table(keys) => keys.iter().map(|_| None).collect(),
        toml::Value::String(s) => s.chars().map(|_| None).collect(),
        other => {
            return vec![format!(
                "charter.toml: '{}' object is not iterable",
                py_type(other)
            )];
        }
    };
    blocks
        .iter()
        .enumerate()
        .filter_map(|(i, block)| match block {
            None => Some(format!(
                "[[forge]] block {i}: [[forge]] block {i} is not a table"
            )),
            Some(table) => block_forge(table)
                .err()
                .map(|e| format!("[[forge]] block {i}: {e}")),
        })
        .collect()
}

/// The CLI of every forge the plane declares, de-duplicated by kind and host — or GitLab's
/// alone when it declares none, or when any block does not resolve (`declared_or_default`).
///
/// The `gh`/`glab` rows are named from this, so a GitHub-only plane is not told to install
/// `glab` (FINDING I3).
pub(super) fn forge_clis(d: &Doctor) -> Vec<String> {
    let fallback = vec!["glab".to_owned()];
    let Some(cfg) = d.config.table() else {
        return fallback;
    };
    let Some(value) = cfg.get("forge").filter(|v| truthy(v)) else {
        return fallback;
    };
    let toml::Value::Array(items) = value else {
        return fallback;
    };
    let mut seen: Vec<crate::forge::Forge> = Vec::new();
    for item in items {
        let Some(Ok(forge)) = item.as_table().map(block_forge) else {
            return fallback;
        };
        if !seen.contains(&forge) {
            seen.push(forge);
        }
    }
    if seen.is_empty() {
        return fallback;
    }
    seen.into_iter()
        .map(|forge| forge.kind.cli().to_owned())
        .collect()
}

/// `contain.plane_adjacent`: at or under the plane, or ONE sibling of it.
fn plane_adjacent(root: &Path, path: &Path) -> bool {
    let (Some(target), Some(base)) = (
        crate::contain::resolved(path),
        crate::contain::resolved(root),
    ) else {
        return false;
    };
    if target.starts_with(&base) {
        return true;
    }
    target.parent() == base.parent() && Some(target.as_path()) != base.parent()
}

/// Why a declared `[plane] worktrees` may not be used, or `None` — `contain.
/// plane_adjacent_refusal`.
fn worktrees_refusal(root: &Path, declared: &str) -> Option<String> {
    // The same `~` rule `plane::place` reads out of `$CHARTER_ROOT`: one spelling, because a
    // committed value means one thing wherever charter reads it.
    let p = crate::plane::expand_user(Path::new(declared));
    let p = if p.is_absolute() { p } else { root.join(p) };
    if plane_adjacent(root, &p) {
        return None;
    }
    let target = crate::contain::resolved(&p).unwrap_or(p);
    let field = |s: &str| super::one_line(s, super::PATH_DISPLAY_LIMIT);
    Some(format!(
        "'{}' resolves to '{}', which is neither inside the control plane ({}) nor beside it. \
         This is read from a committed file and directories get created there, so it may \
         name a place under the plane or a single sibling of it — '../charter.worktrees', \
         the documented shape — and nothing further afield",
        field(declared),
        field(&target.display().to_string()),
        field(&root.display().to_string()),
    ))
}

/// `charter.toml`: whether the plane's own file parsed, and the settings in it that are
/// silently ignored — each of which renders exactly as the key being absent, so this row is
/// the only place any of them is said.
pub(super) fn charter_toml(d: &Doctor) -> Row {
    const NAME: &str = "charter.toml";
    let cfg = match &d.config {
        Config::Malformed(why) => {
            return Row::fail(
                NAME,
                first_line(why),
                "Fix or remove charter.toml, then re-run. Falling back to empty \
                 group/exclude/workspace defaults until it does.",
            );
        }
        Config::Refused(why) => {
            return Row::fail(
                NAME,
                first_line(why),
                "charter refuses to operate on this plane at all — see the `schema` row. \
                 `charter update`, then re-run.",
            );
        }
        Config::Read(cfg) => cfg,
    };
    if let Some((summary, detail)) = worktrees_finding(&d.root, cfg)
        .or_else(|| forge_finding(cfg))
        // The profiles are read only when there is a default to look for, as they always were.
        .or_else(|| {
            refused_default(cfg)?;
            default_finding(cfg, &crate::profiles::current(&d.root))
        })
    {
        return Row::warn(NAME, summary, detail);
    }
    // `[[frame.component]]` is refused WHOLE when charter cannot draw it, and nothing but
    // this row says so. Whether an arrangement can be drawn is the tmux frame's question,
    // which this charter does not answer — so a plane that writes one is told this row did
    // not look at it, rather than told it parsed cleanly.
    let arranges = cfg
        .get("frame")
        .and_then(toml::Value::as_table)
        .is_some_and(|frame| frame.contains_key("component"));
    if arranges {
        return deferred::row(
            NAME,
            "this plane declares a [[frame.component]] arrangement, which arranges a tmux \
             frame this charter does not have, so nothing reads it",
        );
    }
    // After the arrangement, so a save finding never hides that it went unread.
    if let Some((summary, detail)) = save_finding(&d.root) {
        return Row::warn(NAME, summary, detail);
    }
    if !d.has_plane {
        return Row::warn(
            NAME,
            format!(
                "no control plane found (cwd: {})",
                super::fsx::path_field(&d.root)
            ),
            "`charter init` here, or cd into a plane, or set $CHARTER_ROOT. Every check below \
             is reporting on a plane that does not exist.",
        );
    }
    Row::ok(
        NAME,
        format!("parsed cleanly ({})", super::fsx::path_field(&d.root)),
    )
}

/// Every setting in `cfg` that charter reads as absent, each as the `charter.toml` row's
/// words — summary, then what to do — in the order the row would name them. `profiles` is
/// this machine's profiles, which a `[harness] default` may name.
///
/// The row reports the first; a writer refuses on any of them (charter-app#252), so the
/// Project settings tab refuses exactly what the doctor would warn is being ignored, in the
/// same sentences.
pub(crate) fn findings(
    root: &Path,
    cfg: &toml::Table,
    profiles: &crate::profiles::ProfileSet,
) -> Vec<(String, String)> {
    [
        worktrees_finding(root, cfg),
        forge_finding(cfg),
        default_finding(cfg, profiles),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// A `[plane] worktrees` charter ignores because it points outside the plane.
fn worktrees_finding(root: &Path, cfg: &toml::Table) -> Option<(String, String)> {
    let worktrees = cfg
        .get("plane")
        .and_then(toml::Value::as_table)
        .and_then(|plane| plane.get("worktrees"))
        .and_then(toml::Value::as_str)
        .map(crate::memstore::py_strip)
        .filter(|v| !v.is_empty())?;
    let why = worktrees_refusal(root, worktrees)?;
    Some((
        "[plane] worktrees points outside the plane and is being ignored".to_owned(),
        format!(
            "{why}. Worktrees are in the default layout (workspaces/<ws>/.worktrees/) \
             until the key is fixed or removed; $CHARTER_WORKTREES sets a per-machine \
             root without editing the file."
        ),
    ))
}

/// How far the plane's saves go, when the answer is worth a word (charter-app#292, ADR 0051):
/// the deprecated `[memory] share` standing in for `[plane] mode`, or a mode that opens a pull
/// request on a plane whose origin no forge adapter can open one on. Neither is a refusal: the
/// first still reads, and the second is the Saving view's **blocked**, said ahead of time.
fn save_finding(root: &Path) -> Option<(String, String)> {
    use crate::planesave::{Source, refusals};
    use crate::settings::{Which, layer_text};
    let shared = layer_text(root, Which::Shared)
        .text()
        .unwrap_or_default()
        .to_owned();
    // This machine's file as every reader takes it: none at all when git would carry it, which
    // the profiles row already names.
    let local = layer_text(root, Which::Local)
        .text()
        .unwrap_or_default()
        .to_owned();
    // A key or value the settings tab would refuse is one nothing reads: say so first, in
    // either file, since profiles no longer refuses `[plane]` and `[repos]` in the local one.
    if let Some(why) = refusals(&shared, false, Which::Shared.file())
        .into_iter()
        .chain(refusals(&local, true, Which::Local.file()))
        .next()
    {
        return Some((
            why,
            "Fix or remove it: until then charter reads the next file down, or the default."
                .to_owned(),
        ));
    }
    let plane = crate::planesave::Settings::read(root).plane;
    // Only when charter.toml's own `[plane] mode` overrides it: a mode from this machine's
    // local file leaves `share` the one every other clone reads, so removing it would change
    // theirs.
    if !plane.from_share
        && plane.mode.source == Source::Shared
        && crate::planesave::share_is_set(&shared)
    {
        return Some((
            "[memory] share is deprecated, and [plane] mode overrides it, so nothing reads it"
                .to_owned(),
            "Remove share from [memory] in charter.toml.".to_owned(),
        ));
    }
    // This machine's local mode overrides it here, and every other clone still reads it.
    let everywhere = crate::planesave::Settings::from_text(Some(&shared), None).plane;
    if plane.mode.source == Source::Local
        && everywhere.from_share
        && let Some(theirs) = everywhere.mode.value
    {
        let word = theirs.as_str();
        return Some((
            format!(
                "[memory] share is deprecated, and is still read as [plane] mode = \"{word}\" \
                 by every clone without this machine's charter.local.toml"
            ),
            format!(
                "Write mode = \"{word}\" under [plane] in charter.toml and remove share from \
                 [memory] — [plane] mode says how far every save goes, not only a memory's."
            ),
        ));
    }
    let mode = plane.mode.value?;
    if plane.from_share {
        let word = mode.as_str();
        return Some((
            format!("[memory] share is deprecated, and is being read as [plane] mode = \"{word}\""),
            format!(
                "Write mode = \"{word}\" under [plane] in charter.toml and remove share from \
                 [memory] — [plane] mode says how far every save goes, not only a memory's."
            ),
        ));
    }
    if mode.opens_a_pr() && crate::planegit::origin_https(root).is_none() {
        return Some((
            format!(
                "[plane] mode = \"{}\" opens a pull request, and this plane's origin is not a \
                 GitHub or GitLab forge charter knows",
                mode.as_str()
            ),
            "Saves stop at a local commit, and the plane shows as blocked, until origin is on a \
             forge a [[forge]] block declares, or mode is commit or push."
                .to_owned(),
        ));
    }
    None
}

/// The `[[forge]]` blocks that do not resolve, as one finding.
fn forge_finding(cfg: &toml::Table) -> Option<(String, String)> {
    let errors = forge_errors(cfg);
    if errors.is_empty() {
        return None;
    }
    let mut shown = errors
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if errors.len() > 3 {
        shown.push_str(" …");
    }
    Some((
        format!("{} [[forge]] block(s) failed to resolve", errors.len()),
        format!(
            "{shown} — those hosts are NOT covered by the one-credential guard or \
             git-policy until fixed (other declared/default hosts still are)."
        ),
    ))
}

/// A `[harness] default` that names neither a launchable harness nor a profile in `profiles`.
fn default_finding(
    cfg: &toml::Table,
    profiles: &crate::profiles::ProfileSet,
) -> Option<(String, String)> {
    let refused = refused_default(cfg)?;
    if profiles.get(&refused).is_some() {
        return None;
    }
    Some((
        format!("[harness] default = \"{refused}\" is not a harness charter can launch"),
        format!(
            "The app's new-chat picker marks no row for it and opens on the first one that \
             can run — which is also what a plane that declares no default gets, so the \
             key currently reads as absent. Name one of: {}, or any profile \
             charter.local.toml declares.",
            LAUNCHABLE.join(", ")
        ),
    ))
}

/// A `[harness] default` naming no launchable harness, as the sentence will quote it —
/// `instance.harness_of`'s `refused`.
fn refused_default(cfg: &toml::Table) -> Option<String> {
    let value = cfg.get("harness")?.as_table()?.get("default")?;
    if let toml::Value::String(s) = value
        && LAUNCHABLE.contains(&s.as_str())
    {
        return None;
    }
    Some(crate::shown::short(&crate::profiles::py_str(value)))
}

/// `schema`: the plane format this charter can place, and the baseline directories it
/// expects. FAIL when the format is refused: every other command stops outright, and a row
/// scoring a hard stop as a warning would disagree with the tool it reports on.
pub(super) fn schema(d: &Doctor) -> Row {
    const NAME: &str = "schema";
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    if let Config::Refused(why) = &d.config {
        return Row::fail(
            NAME,
            first_line(why),
            "charter refuses to operate on a plane whose format version it cannot place, \
             rather than guess at a layout it has been told it does not understand. `charter \
             update` is the way out; every other command declines until it runs.",
        );
    }
    let found: Vec<String> = BASELINE_DIRS
        .iter()
        .filter_map(|dir| {
            let p = d.root.join(dir);
            if p.is_dir() {
                None
            } else if p.exists() {
                Some(format!(
                    "{dir}/ is occupied by a file, not a directory — reinit will refuse to \
                     touch it"
                ))
            } else {
                Some(format!("missing directory: {dir}/"))
            }
        })
        .collect();
    if found.is_empty() {
        return Row::ok(NAME, format!("up to date (schema {SCHEMA})"));
    }
    Row::warn(
        NAME,
        format!("{} issue(s): {}", found.len(), found.join("; ")),
        "Run: charter reinit  (creates what's missing; never touches existing content).",
    )
}

/// `version lock`: `[charter] version`, opt-in. A plane that pins nothing reports OK.
///
/// **Reported up to the pin, and no further.** The comparison is `adopt::pin_verdict`'s (ADR
/// 0045), and `charter version` is the surface that makes it; this row still names the pin and
/// sends the reader there rather than comparing on its own. Moving it onto the verdict changes
/// the row the differential compares byte for byte with the Python charter's, which is its own
/// change (ADR 0030's follow-up, still open).
pub(super) fn version_lock(d: &Doctor) -> Row {
    const NAME: &str = "version lock";
    let cfg = match &d.config {
        Config::Read(cfg) => cfg,
        Config::Malformed(why) | Config::Refused(why) => return Row::not_checked(NAME, why),
    };
    let locked = match cfg.get("charter").filter(|v| truthy(v)) {
        None => None,
        Some(toml::Value::Table(section)) => section
            .get("version")
            .and_then(toml::Value::as_str)
            .map(crate::memstore::py_strip)
            .filter(|v| !v.is_empty()),
        // `(cfg.get("charter") or {}).get(...)` on a value that is not a table.
        Some(other) => {
            return Row::warn(
                NAME,
                format!(
                    "not checked ('{}' object has no attribute 'get')",
                    py_type(other)
                ),
                NOT_CHECKED_HINT,
            );
        }
    };
    match locked {
        None => Row::ok(NAME, "not pinned"),
        Some(pin) => deferred::row(
            NAME,
            &format!(
                "pinned {}; this row does not compare it — `charter version` does",
                super::one_line(pin, super::DISPLAY_LIMIT)
            ),
        ),
    }
}
