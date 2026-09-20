//! `charter clone <repo>...`: clone on demand into a workspace. Python's `cmd_clone`.
//!
//! # The destination is a name a forge chose
//!
//! A clone lands at `workspaces/<ws>/<name>`, and `<name>` comes out of `inventory/repos.json`
//! — a tracked file, filled from a forge's API and editable by anybody with a commit. So the
//! exact path created is gated, not its parent: the name must be a repo name
//! ([`contain::repo_name_ok`] — no `..`, no separator, no leading `-` git could read as an
//! option, no leading `.`), the path must stay inside the workspace without passing through a
//! link ([`confine::within_workspace`]), and must land in the plane's data directories
//! ([`contain::writable`]). The path handed to git is the one checked, after `--`.
//!
//! # The URL is one charter built
//!
//! HTTPS from the record's `web_url`, or the record's `ssh_url` rewritten through its forge's
//! own SSH forms — never a string handed through (Python #335: `ext::sh -c …` is a transport
//! that runs a command). On top of Python's rule: a URL carrying a user or a token before
//! its host is refused without being repeated, and a host the plane does not manage is
//! refused, so a tracked file cannot send a clone somewhere the forge's credential helper
//! would then be asked about.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

use super::{Say, Sink, banner, submodules};
use crate::contain;
use crate::forge::{self, Forge, py_str};
use crate::guest::{self, Hidden, Status};
use crate::inventory;
use crate::manifest::Ownership;
use crate::pyrepr::repr_str;
use crate::repos;
use crate::workspaces::Plane;
use crate::worktree::confine::{self, Outside};
use crate::worktree::git;

/// How many clones run at once. Python's `CLONE_WORKERS`.
pub const WORKERS: usize = 8;

/// What `clone` needs besides the repo names.
pub struct Request<'a> {
    pub root: &'a Path,
    pub ws: &'a str,
    pub repos: &'a [String],
    /// When the manifest says it was last touched: `updated_at`.
    pub now: chrono::DateTime<chrono::Utc>,
    /// Who it says touched it: `updated_by`, `$USER` in Python.
    pub author: &'a str,
}

/// What happened to one repo.
enum Outcome {
    /// Not attempted, for the reason given.
    Refused(String),
    /// Something is already at the destination.
    Exists(PathBuf),
    /// Cloned, onto this branch.
    Cloned {
        dest: PathBuf,
        forge: Forge,
        branch: String,
    },
    /// git tried and failed, in its own words.
    Failed { forge: Forge, said: String },
}

/// Run `clone`. Returns the exit status.
pub fn clone(request: &Request, say: Sink) -> u8 {
    let root = request.root;
    let cfg = forge::load_config(root).unwrap_or_default();
    let group = forge::group_of(&cfg, 0);
    let doc = match inventory::load(root, &group) {
        Ok(doc) => doc,
        Err(why) => {
            say(Say::Fail(why));
            return 1;
        }
    };
    let all = inventory::repos(root, &doc, &forge::exclude_of(&cfg, 0));
    if all.is_empty() {
        say(Say::Fail(
            "Nothing to clone: this plane has no inventory, and its own repo could not be \
             derived (no origin, or a forge it does not declare)."
                .into(),
        ));
        say(Say::Info("  Build one: charter discover".into()));
        return 1;
    }

    let mut targets: Vec<Value> = Vec::new();
    for name in request.repos {
        match inventory::find(&all, name) {
            Some(record) => targets.push(record.clone()),
            None => say(Say::Warn(format!(
                "Unknown repo (not in inventory): {name} — check the name (`charter status`) or \
                 `charter discover` if it's new to the group."
            ))),
        }
    }
    if targets.is_empty() {
        say(Say::Fail(
            "No matching repos. Give one or more repo names/paths from the inventory (see \
             them: `charter status`; refresh from GitLab: `charter discover`)."
                .into(),
        ));
        return 1;
    }

    let ws = request.ws;
    banner(ws, say);
    let ws_dir = match workspace_dir(root, ws) {
        Ok(dir) => dir,
        Err(why) => {
            say(Say::Fail(why));
            return 1;
        }
    };

    // The checkouts already in the workspace get the layer FIRST, and silently: Python's
    // `ensure` scaffolds the workspace before cloning, and the scaffold wires what is there
    // without a word. The announcement after the clone is then only about what this command
    // brought in — a clone somebody made by hand last week is not news.
    wire(root, ws, &mut |_| {});

    if targets.len() > 1 {
        say(Say::Info(format!(
            "Cloning {} repo(s) into '{ws}', {} at a time …",
            targets.len(),
            WORKERS.min(targets.len())
        )));
    }
    let outcomes = clone_all(root, ws, &ws_dir, &targets);

    // Printed here, from one thread, in the order the repos were asked for: eight workers
    // printing as they finish interleave into something nobody can scan for which failed.
    let mut failures = 0;
    let mut members: Vec<String> = Vec::new();
    for (record, outcome) in targets.iter().zip(outcomes) {
        let name = py_str(record.get("name").unwrap_or(&Value::Null));
        let dest = match outcome {
            Outcome::Refused(why) => {
                failures += 1;
                let quoted = match record.get("name") {
                    Some(Value::String(s)) => repr_str(s),
                    Some(other) => py_str(other),
                    None => "None".into(),
                };
                say(Say::Fail(format!("{quoted}: not cloned — {why}.")));
                continue;
            }
            Outcome::Exists(dest) => {
                say(Say::Info(format!("{name}: already cloned in '{ws}'")));
                dest
            }
            Outcome::Cloned {
                dest,
                forge,
                branch,
            } => {
                let shown = if branch.is_empty() {
                    name.clone()
                } else {
                    format!("{name} ({branch})")
                };
                let rel = dest.strip_prefix(root).unwrap_or(&dest);
                say(Say::Done(format!(
                    "{name} → {} ({shown} via {}, HTTPS)",
                    rel.display(),
                    forge.kind.cli()
                )));
                dest
            }
            Outcome::Failed { forge, said } => {
                failures += 1;
                let cli = forge.kind.cli();
                say(Say::Fail(format!(
                    "{name}: clone failed — no access, network, or {cli} isn't authed (`{cli} \
                     auth status`). Skipping.\n{said}"
                )));
                continue;
            }
        };
        // Reported BEFORE the docs hint: an empty submodule directory is what breaks the next
        // command, and the clone itself still succeeded.
        submodules::report(root, &dest, &name, None, None, say);
        hint_docs(&dest, &name, say);
        if let Some(dir) = dest.file_name() {
            members.push(dir.to_string_lossy().into_owned());
        }
    }
    wire(root, ws, say);
    record_membership(request, &members, say);
    u8::from(failures > 0)
}

/// The workspace directory, which must already exist.
///
/// Python's `ensure` creates and scaffolds a workspace that is not there. The Rust charter
/// does not scaffold one yet, and a half-made workspace is one Python's `reinit` then flags
/// on every turn — so it refuses, and says what to do.
fn workspace_dir(root: &Path, ws: &str) -> Result<PathBuf, String> {
    if !contain::workspace_name_ok(ws) {
        return Err(format!(
            "invalid workspace name '{ws}' (use letters, digits, '.', '_', '-'; must not start \
             with a dot)"
        ));
    }
    match confine::workspace_dir(root, ws) {
        Ok(dir) => Ok(dir),
        Err(Outside::NoWorkspace { .. }) => Err(format!(
            "no workspace '{ws}' — this charter clones into a workspace that exists, and does \
             not create one yet. Create it first (`charter workspace create {ws}`)."
        )),
        Err(other) => Err(other.to_string()),
    }
}

/// Clone every target, [`WORKERS`] at a time, answering in the order asked.
fn clone_all(root: &Path, ws: &str, ws_dir: &Path, targets: &[Value]) -> Vec<Outcome> {
    let slots: Mutex<Vec<Option<Outcome>>> = Mutex::new((0..targets.len()).map(|_| None).collect());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(targets.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(record) = targets.get(i) else {
                        break;
                    };
                    let outcome = clone_one(root, ws, ws_dir, record);
                    if let Ok(mut slots) = slots.lock() {
                        slots[i] = Some(outcome);
                    }
                }
            });
        }
    });
    slots
        .into_inner()
        .unwrap_or_default()
        .into_iter()
        .map(|o| o.unwrap_or_else(|| Outcome::Refused("charter lost track of this clone".into())))
        .collect()
}

/// Python's `contain.refusal`: the sentence a name that is a path is refused with.
fn not_a_segment(name: &str) -> String {
    format!(
        "'{}' is not a name — it is a path. This is read from a committed file and joined onto \
         a directory, so it may name one entry there and nothing else: no '/', no '\\', no '.' \
         or '..', and nothing absolute",
        crate::shown::escaped(name)
    )
}

/// The one destination a record may be cloned to, gated as itself.
pub fn destination(root: &Path, ws: &str, ws_dir: &Path, name: &str) -> Result<PathBuf, String> {
    if !contain::segment_ok(name) {
        return Err(not_a_segment(name));
    }
    if !contain::repo_name_ok(name) {
        return Err(format!(
            "'{}' is not a name charter will take for a repo: a clone is named by letters, \
             digits, '.', '_' and '-', starting with a letter or a digit, so that no clone can \
             be read as a git option or hidden as a dot-directory",
            crate::shown::escaped(name)
        ));
    }
    let dest =
        confine::within_workspace(root, ws, &ws_dir.join(name)).map_err(|e| e.to_string())?;
    contain::writable(root, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// The HTTPS URL a record is cloned from, or why there is none. Python's `_https_url`, and
/// then the two refusals it lacks.
pub fn https_url(record: &Value, root: &Path) -> Result<String, String> {
    let text = |key: &str| {
        record
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let web = text("web_url");
    let web = web.trim_end_matches('/');
    let url = if web.starts_with("https://") {
        format!("{web}.git")
    } else {
        let ssh = text("ssh_url");
        let (base, forms) = Forge::for_record(record).insteadof();
        match forms.iter().find(|p| ssh.starts_with(p.as_str())) {
            Some(prefix) => format!("{base}{}", &ssh[prefix.len()..]),
            None => {
                return Err(
                    "its inventory record carries no HTTPS clone URL, and the `ssh_url` \
                            it does carry is not a form this forge recognises — charter will \
                            not hand git a string it did not build"
                        .into(),
                );
            }
        }
    };
    let after_scheme = &url["https://".len()..];
    let authority = after_scheme.split('/').next().unwrap_or_default();
    // Not repeated back: what sits before an `@` is a user, a token or both, and a refusal is
    // a line in a terminal and a transcript.
    if authority.contains('@') {
        return Err(
            "its URL carries a user or a credential before the host, and charter will not put \
             one on a command line or into a clone's config — the forge's own CLI holds the \
             credential"
                .into(),
        );
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("its URL holds whitespace or a control character".into());
    }
    let host = forge::host_of(&url);
    if !forge::known(root).contains_key(&host) {
        return Err(format!(
            "its URL names host '{}', which is neither a forge's default host nor one this \
             plane declares in charter.toml — a tracked inventory could otherwise send a clone \
             anywhere",
            crate::shown::escaped(&host)
        ));
    }
    Ok(url)
}

/// Clone ONE repo. Prints nothing: [`clone`] renders every outcome afterwards, in order.
fn clone_one(root: &Path, ws: &str, ws_dir: &Path, record: &Value) -> Outcome {
    let name = record
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let dest = match destination(root, ws, ws_dir, name) {
        Ok(dest) => dest,
        Err(why) => return Outcome::Refused(why),
    };
    match std::fs::symlink_metadata(&dest) {
        Ok(found) if found.file_type().is_symlink() => {
            return Outcome::Refused(format!(
                "workspaces/{ws}/{name} is a symlink, and charter will not clone through one"
            ));
        }
        Ok(_) => return Outcome::Exists(dest),
        Err(_) => {}
    }
    let forge = Forge::for_record(record);
    let url = match https_url(record, root) {
        Ok(url) => url,
        Err(why) => return Outcome::Refused(why),
    };
    let dest_arg = dest.display().to_string();
    // No `--branch`: the remote's HEAD is its real default branch, where the inventory's
    // field may be one `discover` assumed, or one the forge has since moved off.
    let cloned = git::run_network(
        ws_dir,
        Some(&forge::helper_for(&forge)),
        &["clone", "--", &url, &dest_arg],
    );
    let run = match cloned {
        Ok(run) => run,
        Err(why) => {
            return Outcome::Failed {
                forge,
                said: why.to_string(),
            };
        }
    };
    if !run.ok() {
        return Outcome::Failed {
            said: failure(&run, &dest),
            forge,
        };
    }
    crate::gitpolicy::apply(&dest, root);
    let branch = git::run(
        &dest,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        git::READ,
    )
    .ok()
    .filter(|r| r.ok())
    .map(|r| r.line().to_string())
    .unwrap_or_default();
    Outcome::Cloned {
        dest,
        forge,
        branch,
    }
}

/// What a failed clone says: git's own words, and why when the why is charter's rule.
fn failure(run: &git::Run, dest: &Path) -> String {
    let said = run.err.trim().to_string();
    match run.code {
        None => format!(
            "git did not finish within {} seconds and was stopped; whatever it left at {} is \
             yours to inspect or remove — charter does not delete it",
            git::NETWORK.as_secs(),
            dest.display()
        ),
        Some(_) if said.contains("transport 'ssh' not allowed") => format!(
            "{said}\ngit was told to use SSH for this URL — a `url.<base>.insteadOf` in your \
             git config rewrites it — and charter clones over HTTPS with the forge CLI's token \
             only (docs/git-policy.md)"
        ),
        Some(_) => said,
    }
}

/// `  ↳ <name> ships its own <file>` for the first of the three a clone has.
fn hint_docs(dest: &Path, name: &str, say: Sink) {
    for file in ["CLAUDE.md", "AGENTS.md", "README.md"] {
        if dest.join(file).exists() {
            say(Say::Info(format!(
                "  ↳ {name} ships its own {file} — read it before working there."
            )));
            return;
        }
    }
}

/// Give every clone in the workspace the plane's layer, and say so once per clone that got
/// something. Python's `_wire_clones` for the clones — a workspace's linked worktrees get
/// theirs where they are cut.
fn wire(root: &Path, ws: &str, say: Sink) {
    let Ok(found) = repos::clones(root, ws) else {
        return;
    };
    for repo in found.repos {
        let wired = guest::wire(root, &repo.path);
        let label = &repo.name;
        if let Hidden::Blocked(..) = wired.hidden {
            say(Say::Warn(format!("{label}: {}", wired.refusal(&repo.path))));
            continue;
        }
        if wired.rows.iter().any(|r| r.status == Status::Blocked) {
            say(Say::Warn(format!("{label}: {}", wired.refusal(&repo.path))));
            continue;
        }
        let wrote = wired
            .rows
            .iter()
            .filter(|r| matches!(r.status, Status::Created | Status::Refreshed))
            .count();
        if wrote > 0 {
            // The files, and the exclude block that hides them: Python counts both.
            say(Say::Info(format!(
                "{label}: charter's layer written ({} file(s)) and hidden in that repo's \
                 .git/info/exclude — `git status` there is unaffected, and nothing charter wrote \
                 can be committed.",
                wrote + 1
            )));
        }
    }
}

/// Record what is now in the workspace in its committed manifest. Python's
/// `_record_membership` and `record_members`: additive, sorted, and never a branch.
fn record_membership(request: &Request, members: &[String], say: Sink) {
    if members.is_empty() {
        return;
    }
    let Ok(ws) = Plane::open(request.root).workspace(request.ws) else {
        return;
    };
    let now = request.now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
    let (_, owner) = ws.manifest();
    if owner == Ownership::Absent {
        let rows: Vec<Value> = repos::clones(request.root, request.ws)
            .map(|c| c.repos)
            .unwrap_or_default()
            .into_iter()
            .map(|r| serde_json::json!({"name": r.name}))
            .collect();
        let fresh = serde_json::json!({
            "name": request.ws,
            "description": "",
            "repos": rows,
            "updated_at": now,
            "updated_by": request.author,
        });
        let _ = ws.write_manifest(&fresh);
    }
    let (doc, owner) = ws.manifest();
    match owner {
        Ownership::Charter => {}
        Ownership::Operator => {
            say(Say::Warn(format!(
                "workspaces/{0}/workspace.json was not written by charter — left untouched, so \
                 it does not record what was just cloned. `charter workspace snapshot {0}` \
                 rewrites it deliberately.",
                request.ws
            )));
            return;
        }
        Ownership::Absent => return,
    }
    let Some(mut doc) = doc else {
        return;
    };
    let rows: Vec<Value> = doc
        .get("repos")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter(|r| r.is_object() && r.get("name").is_some_and(forge::truthy))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let have: Vec<String> = rows
        .iter()
        .map(|r| py_str(r.get("name").unwrap_or(&Value::Null)))
        .collect();
    let mut add: Vec<String> = Vec::new();
    for name in members {
        if !name.is_empty() && !have.contains(name) && !add.contains(name) {
            add.push(name.clone());
        }
    }
    if add.is_empty() {
        return;
    }
    let mut all = rows;
    all.extend(add.into_iter().map(|n| serde_json::json!({"name": n})));
    all.sort_by_key(|r| py_str(r.get("name").unwrap_or(&Value::Null)));
    if let Some(map) = doc.as_object_mut() {
        map.insert("repos".into(), Value::Array(all));
        map.insert("updated_at".into(), Value::String(now));
        map.insert(
            "updated_by".into(),
            Value::String(request.author.to_string()),
        );
    }
    // A manifest charter could not write is Python's "blocked", which it does not announce.
    let _ = ws.write_manifest(&doc);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plane() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(
            root.join("charter.toml"),
            "[[forge]]\nkind = \"github\"\nowner = \"acme\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
        (dir, root)
    }

    #[test]
    fn a_name_that_is_a_path_or_an_option_or_hidden_gets_no_destination() {
        let (_dir, root) = plane();
        let ws = root.join("workspaces/alpha");
        for hostile in [
            "..",
            ".",
            "../escape",
            "a/b",
            "a\\b",
            "/etc",
            "-rf",
            "--upload-pack=x",
            ".github",
            "",
            "a\0b",
        ] {
            let refused = destination(&root, "alpha", &ws, hostile);
            assert!(refused.is_err(), "{hostile:?} was given {refused:?}");
        }
        assert_eq!(
            destination(&root, "alpha", &ws, "widget").unwrap(),
            ws.join("widget")
        );
    }

    #[test]
    fn a_link_at_the_destination_or_at_the_workspace_is_not_cloned_through() {
        let (_dir, root) = plane();
        let outside = tempfile::tempdir().unwrap();
        let ws = root.join("workspaces/alpha");
        // A link at the destination that lands INSIDE the plane — another workspace's clone —
        // is the case only `within_workspace` sees: the data-directory gate is satisfied by
        // where it lands, and a clone would be written into the other workspace.
        std::fs::create_dir_all(root.join("workspaces/beta/stolen")).unwrap();
        std::os::unix::fs::symlink(root.join("workspaces/beta/stolen"), ws.join("widget")).unwrap();
        assert!(destination(&root, "alpha", &ws, "widget").is_err());
        // And one that leaves the plane.
        std::os::unix::fs::symlink(outside.path(), ws.join("gadget")).unwrap();
        assert!(destination(&root, "alpha", &ws, "gadget").is_err());

        std::fs::create_dir_all(root.join("workspaces")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("workspaces/evil")).unwrap();
        let evil = root.join("workspaces/evil");
        assert!(destination(&root, "evil", &evil, "widget").is_err());
    }

    #[test]
    fn the_url_is_https_from_the_record_or_the_forges_own_ssh_rewrite() {
        let (_dir, root) = plane();
        assert_eq!(
            https_url(
                &json!({"web_url": "https://github.com/acme/widget/"}),
                &root
            )
            .unwrap(),
            "https://github.com/acme/widget.git"
        );
        assert_eq!(
            https_url(
                &json!({"ssh_url": "git@github.com:acme/widget.git", "forge": "github"}),
                &root
            )
            .unwrap(),
            "https://github.com/acme/widget.git"
        );
    }

    #[test]
    fn a_url_charter_did_not_build_is_refused() {
        let (_dir, root) = plane();
        for record in [
            json!({"ssh_url": "ext::sh -c 'touch /tmp/pwned'"}),
            json!({"ssh_url": "--upload-pack=touch /tmp/pwned"}),
            json!({"ssh_url": "/etc/passwd"}),
            json!({"web_url": "http://github.com/acme/widget"}),
            json!({"web_url": "https://evil.example/acme/widget"}),
            json!({"web_url": "https://github.com/acme/wid get"}),
        ] {
            assert!(https_url(&record, &root).is_err(), "{record}");
        }
    }

    #[test]
    fn a_credential_in_the_url_is_refused_and_never_repeated() {
        let (_dir, root) = plane();
        let refused = https_url(
            &json!({"web_url": "https://x-access-token:ghp_SECRET123@github.com/acme/widget"}),
            &root,
        )
        .unwrap_err();
        assert!(!refused.contains("ghp_SECRET123"), "{refused}");
        assert!(!refused.contains("x-access-token"), "{refused}");
    }
}
