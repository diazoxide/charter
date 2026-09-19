//! `inventory/repos.json`: every repo the plane's forges expose, whether or not it is cloned.
//!
//! A port of `charter/inventory.py`. The file is **tracked**, so it is two things at once: the
//! durable record `discover` writes, and an input anybody with a commit can hand this plane.
//! Every name in it is re-checked where it is joined onto a path (`repocmd::clone`), never
//! trusted because `discover` checked it on the way in — a hand-edited or PR-modified
//! inventory never passed through `discover` at all.
//!
//! Records are kept as JSON values rather than a struct, and that is the format rule rather
//! than a shortcut: the file is Python's, its key order is part of it, and a field a newer
//! charter adds must survive a rewrite by this one.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::forge::{self, Forge, Kind, py_str, truthy};
use crate::pyrepr::repr_str;
use crate::worktree::git;

/// Marks a record charter derived rather than discovered: the plane's own repo. Never saved.
pub const PLANE_SOURCE: &str = "plane";

/// Where the inventory lives under a plane root.
pub fn path(root: &Path) -> PathBuf {
    root.join("inventory").join("repos.json")
}

/// A coarse role inferred from the repo's name. Python's `classify_kind`.
pub fn classify_kind(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.ends_with("-workspace") {
        "workspace"
    } else if n.ends_with("-docs") {
        "docs"
    } else if n.ends_with("-frontend") || n.contains("-ui-") || n.ends_with("-ui") {
        "frontend"
    } else if n.ends_with("-service") || n.ends_with("-services") || n.ends_with("-engine") {
        "service"
    } else if n.ends_with("-api") || n.contains("gateway") {
        "api"
    } else if n.ends_with("-core") {
        "core"
    } else {
        "app"
    }
}

/// The primary build stack, from a repo's top-level file names. Python's `classify_stack`.
pub fn classify_stack(files: &[String]) -> &'static str {
    let has = |name: &str| files.iter().any(|f| f == name);
    let any = |names: &[&str]| names.iter().any(|n| has(n));
    if has("nx.json") {
        "nx"
    } else if has("pom.xml") || has(".mvn") {
        "java-maven"
    } else if any(&[
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ]) {
        "java-gradle"
    } else if has("go.mod") {
        "go"
    } else if has("Cargo.toml") {
        "rust"
    } else if any(&["pyproject.toml", "requirements.txt", "Pipfile", "setup.py"]) {
        "python"
    } else if has("pnpm-workspace.yaml") {
        "node-monorepo"
    } else if has("package.json") {
        "node"
    } else if has("composer.json") {
        "php"
    } else if has("Gemfile") {
        "ruby"
    } else if any(&["Chart.yaml", "helmfile.yaml"]) {
        "helm"
    } else if files.iter().any(|f| f.ends_with(".tf")) {
        "terraform"
    } else if has("Dockerfile") {
        "docker"
    } else {
        "unknown"
    }
}

/// Python's `str.strip()`: its whitespace is Unicode's, plus the four information
/// separators U+001C..U+001F that Rust's `trim` leaves in place.
pub fn py_strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// The inventory document, or the empty skeleton when there is none. Python's `load`.
///
/// Gated as the exact file it reads: a committed `inventory -> /elsewhere` link would
/// otherwise have `clone` take its URLs from a file outside the plane.
pub fn load(root: &Path, group: &str) -> Result<Value, String> {
    let at = path(root);
    if std::fs::symlink_metadata(&at).is_err() {
        return Ok(serde_json::json!({"group": group, "count": 0, "repos": []}));
    }
    in_plane(root, &at)?;
    let text = std::fs::read_to_string(&at)
        .map_err(|e| format!("could not read {}: {e}", at.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is not valid JSON: {e}", at.display()))
}

/// Refuse a plane file that resolves out of the plane.
///
/// `inventory/` and `docs/` are not data directories (`contain::writable` answers for
/// `personas/`, `workspaces/` and `persona-state` only), but they are the plane's own tracked
/// files, and a committed `inventory -> /elsewhere` would send this read or write wherever
/// it points. Python gates neither; this one is resolved at both ends, link by link.
pub fn in_plane(root: &Path, at: &Path) -> Result<(), String> {
    if crate::contain::within_plane(root, at) {
        Ok(())
    } else {
        Err(format!(
            "'{}' resolves outside the plane at '{}' — a committed symlink redirects it, and \
             charter will not follow one out",
            at.display(),
            root.display()
        ))
    }
}

/// The records in a document, as Python's `doc.get("repos", [])` reads them.
pub fn listed(doc: &Value) -> Vec<Value> {
    doc.get("repos")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn text<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The plane repo — the forge repo the plane root is a checkout of — or `None` whenever
/// charter cannot answer honestly. Python's `plane_repo`.
pub fn plane_repo(root: &Path) -> Option<Value> {
    let origin = git::run(root, &["remote", "get-url", "origin"], git::READ).ok()?;
    let origin = origin.line().trim().to_string();
    if origin.is_empty() {
        return None;
    }
    let forge = forge::resolve_host(&origin, root)?;
    let path = namespace(&origin)?;
    let name = path.rsplit_once('/').map(|(_, n)| n.to_string())?;
    let https = origin.starts_with("https://");
    let mut record = Map::new();
    record.insert("name".into(), Value::String(name));
    record.insert(
        "default_branch".into(),
        Value::String(root_default_branch(root)),
    );
    record.insert("path_with_namespace".into(), Value::String(path));
    record.insert("forge".into(), Value::String(forge.kind.word().into()));
    record.insert(
        "web_url".into(),
        Value::String(if https {
            origin.strip_suffix(".git").unwrap_or(&origin).to_string()
        } else {
            String::new()
        }),
    );
    record.insert(
        "ssh_url".into(),
        Value::String(if https { String::new() } else { origin.clone() }),
    );
    record.insert("description".into(), Value::String(String::new()));
    record.insert("topics".into(), Value::Array(Vec::new()));
    record.insert("source".into(), Value::String(PLANE_SOURCE.into()));
    Some(Value::Object(record))
}

/// `<owner>/<name>` out of a remote URL, by Python's two substitutions.
fn namespace(origin: &str) -> Option<String> {
    let mut path = origin;
    // `^[a-z+]+://[^/]+/`
    if let Some((scheme, rest)) = origin.split_once("://")
        && !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_lowercase() || c == '+')
        && let Some((host, after)) = rest.split_once('/')
        && !host.is_empty()
    {
        path = after;
    }
    // `^[^@]+@[^:]+:`
    if let Some((user, rest)) = path.split_once('@')
        && !user.is_empty()
        && let Some((host, after)) = rest.split_once(':')
        && !host.is_empty()
    {
        path = after;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    (!path.is_empty() && path.contains('/')).then(|| path.to_string())
}

/// The plane root's default branch as best charter can tell, or `""`.
fn root_default_branch(root: &Path) -> String {
    let ask = |args: &[&str]| {
        git::run(root, args, git::READ)
            .ok()
            .filter(|r| r.ok())
            .map(|r| r.line().trim().to_string())
            .filter(|s| !s.is_empty())
    };
    if let Some(head) = ask(&[
        "symbolic-ref",
        "--quiet",
        "--short",
        "refs/remotes/origin/HEAD",
    ]) {
        return match head.strip_prefix("origin/") {
            Some(branch) => branch.to_string(),
            None => head,
        };
    }
    ask(&["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default()
}

/// Every repo this plane can clone: the inventory, plus the plane repo unless the
/// inventory already has it or the plane excludes it. Python's `repos`.
pub fn repos(root: &Path, doc: &Value, exclude: &[String]) -> Vec<Value> {
    let listed = listed(doc);
    let Some(own) = plane_repo(root) else {
        return listed;
    };
    let name = text(&own, "name").to_string();
    let pwn = text(&own, "path_with_namespace").to_string();
    if exclude.iter().any(|e| *e == name || *e == pwn) {
        return listed;
    }
    if listed
        .iter()
        .any(|r| r.get("path_with_namespace").and_then(Value::as_str) == Some(pwn.as_str()))
    {
        return listed;
    }
    let mut out = vec![own];
    out.extend(listed);
    out
}

/// One forge project as the record `discover` writes. Python's `commands._build_repo`, in its
/// key order.
pub fn record(forge: &Forge, project: &Value, stack: &str) -> Value {
    let get = |key: &str| project.get(key).cloned().unwrap_or(Value::Null);
    let name = get("name");
    let default_branch = match project.get("default_branch") {
        // Python writes `or "main"`, and the file keeps that: it is the plane format. What
        // `clone` checks out is the remote's own HEAD, never this field.
        Some(v) if truthy(v) => v.clone(),
        _ => Value::String("main".into()),
    };
    let description = match project.get("description") {
        Some(Value::String(s)) => py_strip(s).to_string(),
        Some(v) if truthy(v) => py_str(v),
        _ => String::new(),
    };
    let topics = match project.get("topics") {
        Some(v) if truthy(v) => v.clone(),
        _ => Value::Array(Vec::new()),
    };
    let stamp = match project.get("forge") {
        Some(v) if truthy(v) => v.clone(),
        _ => Value::String(forge.kind.word().into()),
    };
    let mut out = Map::new();
    out.insert("name".into(), name.clone());
    out.insert("path_with_namespace".into(), get("path_with_namespace"));
    out.insert("ssh_url".into(), get("ssh_url"));
    out.insert("default_branch".into(), default_branch);
    out.insert(
        "kind".into(),
        Value::String(classify_kind(name.as_str().unwrap_or_default()).into()),
    );
    out.insert("stack".into(), Value::String(stack.into()));
    out.insert("description".into(), Value::String(description));
    out.insert("topics".into(), topics);
    out.insert(
        "web_url".into(),
        project
            .get("web_url")
            .cloned()
            .unwrap_or(Value::String(String::new())),
    );
    out.insert("forge".into(), stamp);
    Value::Object(out)
}

/// Python's `repr()` of a JSON value, for a sentence that quotes one back.
fn repr(value: &Value) -> String {
    match value {
        Value::String(s) => repr_str(s),
        other => py_str(other),
    }
}

/// Every forge's list as one inventory, keyed by bare name, a genuine collision refused
/// rather than resolved by guessing. Python's `merge`.
///
/// A name that cannot be one directory entry is dropped here, per entry — and that does not
/// make the check at the clone's join redundant: this file is tracked, and a hand-edited one
/// never passes through here.
pub fn merge(batches: &[Vec<Value>]) -> Result<Vec<Value>, String> {
    let mut by_identity: Vec<((Value, Value), Value)> = Vec::new();
    let mut owner_of_name: Vec<(String, (Value, Value))> = Vec::new();
    for batch in batches {
        for r in batch {
            let Some(name) = r.get("name").and_then(Value::as_str) else {
                continue;
            };
            if !crate::contain::segment_ok(name) {
                continue;
            }
            let forge_of = r.get("forge").cloned().unwrap_or(Value::Null);
            let pwn = r.get("path_with_namespace").cloned().unwrap_or(Value::Null);
            let identity = (forge_of.clone(), pwn.clone());
            if let Some((_, prev_identity)) = owner_of_name.iter().find(|(n, _)| n == name)
                && *prev_identity != identity
            {
                let prev = &by_identity
                    .iter()
                    .find(|(id, _)| id == prev_identity)
                    .expect("an owner always has a record")
                    .1;
                let prev_forge = prev.get("forge").cloned().unwrap_or(Value::Null);
                let prev_pwn = prev
                    .get("path_with_namespace")
                    .cloned()
                    .unwrap_or(Value::Null);
                if prev_forge == forge_of {
                    return Err(format!(
                        "{} exposes two different repos both named {}: {} and {}. There's no \
                         bare-name qualifier that can tell two same-forge repos apart — exclude \
                         one via that `[[forge]]` block's `exclude` in charter.toml.",
                        py_str(&forge_of),
                        repr_str(name),
                        repr(&prev_pwn),
                        repr(&pwn)
                    ));
                }
                return Err(format!(
                    "both {} ({}) and {} ({}) expose a repo named {}. Qualify it — e.g. \
                     `{}:{}` — or exclude one in charter.toml.",
                    py_str(&prev_forge),
                    repr(&prev_pwn),
                    py_str(&forge_of),
                    repr(&pwn),
                    repr_str(name),
                    py_str(&forge_of),
                    name
                ));
            }
            match owner_of_name.iter_mut().find(|(n, _)| n == name) {
                Some(slot) => slot.1 = identity.clone(),
                None => owner_of_name.push((name.to_string(), identity.clone())),
            }
            match by_identity.iter_mut().find(|(id, _)| *id == identity) {
                Some(slot) => slot.1 = r.clone(),
                None => by_identity.push((identity, r.clone())),
            }
        }
    }
    let mut out: Vec<Value> = by_identity.into_iter().map(|(_, r)| r).collect();
    out.sort_by(|a, b| text(a, "name").cmp(text(b, "name")));
    Ok(out)
}

/// A repo by bare name, by `path_with_namespace`, or by `<forge>:<name>`. Python's `find`.
pub fn find<'a>(repos: &'a [Value], wanted: &str) -> Option<&'a Value> {
    if let Some((kind, bare)) = wanted.split_once(':')
        && let Some(kind) = Kind::parse(kind)
    {
        return repos.iter().find(|r| {
            text(r, "forge") == kind.word()
                && (text(r, "name") == bare || text(r, "path_with_namespace") == bare)
        });
    }
    repos.iter().find(|r| {
        r.get("name").and_then(Value::as_str) == Some(wanted)
            || r.get("path_with_namespace").and_then(Value::as_str) == Some(wanted)
    })
}

/// Write the inventory, sorted, with no timestamp: the file is tracked, and a volatile field
/// would churn history on every run. A derived record is never written. Python's `save`.
pub fn save(root: &Path, group: &str, records: &[Value]) -> Result<Value, String> {
    let mut kept: Vec<Value> = records
        .iter()
        .filter(|r| r.get("source").and_then(Value::as_str) != Some(PLANE_SOURCE))
        .cloned()
        .collect();
    kept.sort_by(|a, b| text(a, "name").cmp(text(b, "name")));
    let mut doc = Map::new();
    doc.insert("group".into(), Value::String(group.into()));
    doc.insert("count".into(), Value::from(kept.len()));
    doc.insert(
        "note".into(),
        Value::String(format!(
            "Source of truth for repos in the {group} group. Regenerate with `charter \
             discover`; do not hand-edit."
        )),
    );
    doc.insert("repos".into(), Value::Array(kept));
    let doc = Value::Object(doc);
    let at = path(root);
    in_plane(root, &at)?;
    if let Some(dir) = at.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    std::fs::write(&at, crate::pyjson::dumps_indent2_unicode(&doc))
        .map_err(|e| format!("could not write {}: {e}", at.display()))?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rec(name: &str, forge: &str, pwn: &str) -> Value {
        json!({"name": name, "forge": forge, "path_with_namespace": pwn})
    }

    #[test]
    fn the_same_repo_seen_twice_is_one_record() {
        let merged = merge(&[
            vec![rec("api", "gitlab", "acme/api")],
            vec![rec("api", "gitlab", "acme/api")],
        ])
        .unwrap();
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn two_repos_one_bare_name_are_refused_rather_than_guessed_between() {
        let same = merge(&[vec![
            rec("api", "gitlab", "acme/team-a/api"),
            rec("api", "gitlab", "acme/team-b/api"),
        ]])
        .unwrap_err();
        assert_eq!(
            same,
            "gitlab exposes two different repos both named 'api': 'acme/team-a/api' and \
             'acme/team-b/api'. There's no bare-name qualifier that can tell two same-forge \
             repos apart — exclude one via that `[[forge]]` block's `exclude` in charter.toml."
        );
        let cross = merge(&[
            vec![rec("api", "gitlab", "acme/api")],
            vec![rec("api", "github", "acme/api")],
        ])
        .unwrap_err();
        assert_eq!(
            cross,
            "both gitlab ('acme/api') and github ('acme/api') expose a repo named 'api'. \
             Qualify it — e.g. `github:api` — or exclude one in charter.toml."
        );
    }

    #[test]
    fn a_name_that_is_a_path_never_becomes_an_identity() {
        let merged = merge(&[vec![
            rec("../escape", "github", "acme/x"),
            rec("a/b", "github", "acme/y"),
            rec("ok", "github", "acme/ok"),
        ]])
        .unwrap();
        let names: Vec<&str> = merged.iter().map(|r| text(r, "name")).collect();
        assert_eq!(names, ["ok"]);
    }

    #[test]
    fn find_takes_a_bare_name_a_full_path_or_a_forge_qualified_name() {
        let repos = vec![rec("api", "gitlab", "acme/api"), rec("web", "github", "o/web")];
        assert_eq!(text(find(&repos, "api").unwrap(), "forge"), "gitlab");
        assert_eq!(text(find(&repos, "o/web").unwrap(), "name"), "web");
        assert_eq!(text(find(&repos, "github:web").unwrap(), "name"), "web");
        assert!(find(&repos, "gitlab:web").is_none());
        assert!(find(&repos, "nope").is_none());
    }

    #[test]
    fn stacks_and_kinds_are_read_the_way_python_reads_them() {
        let files = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(classify_stack(&files(&["Cargo.toml", "package.json"])), "rust");
        assert_eq!(classify_stack(&files(&["main.tf"])), "terraform");
        assert_eq!(classify_stack(&files(&[])), "unknown");
        assert_eq!(classify_kind("api-gateway"), "api");
        assert_eq!(classify_kind("Billing-Service"), "service");
        assert_eq!(classify_kind("widget"), "app");
    }

    #[test]
    fn a_remote_names_its_namespace_in_either_url_form() {
        assert_eq!(
            namespace("https://github.com/acme/widget.git").as_deref(),
            Some("acme/widget")
        );
        assert_eq!(
            namespace("git@gitlab.com:acme/sub/widget.git").as_deref(),
            Some("acme/sub/widget")
        );
        assert_eq!(namespace("https://github.com/widget").as_deref(), None);
    }

    #[test]
    fn a_description_is_stripped_the_way_python_strips_it() {
        assert_eq!(py_strip("\u{1f} padded \u{a0}\n"), "padded");
    }

    #[test]
    fn a_derived_record_is_never_saved_and_the_file_keeps_its_characters() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "").unwrap();
        let mut own = rec("plane", "github", "o/plane");
        own["source"] = json!(PLANE_SOURCE);
        let doc = save(
            &root,
            "acme",
            &[json!({"name": "b", "description": "ü — x"}), own, json!({"name": "a"})],
        )
        .unwrap();
        assert_eq!(doc["count"], json!(2));
        let written = std::fs::read_to_string(path(&root)).unwrap();
        assert!(written.contains("\"ü — x\""), "{written}");
        assert!(written.find("\"a\"").unwrap() < written.find("\"b\"").unwrap());
        assert!(!written.contains("plane\""), "{written}");
    }
}
