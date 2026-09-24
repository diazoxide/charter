//! The rows that ask git: `git`, `git identity`, `git auth`, `plane root` and `index lock`.
//!
//! Every call goes through [`crate::worktree::git`], the hardened runner — a cleared
//! environment and no hooks or fsmonitor — so a row that asks git a question cannot be made
//! to run a program by the plane it is asking about. That runner finds git in a fixed list of
//! directories before `PATH`, so the `git` row reports the git this binary RUNS, which is the
//! one worth reporting.

use std::path::{Path, PathBuf};

use super::{CHECK_TIMEOUT, Doctor, NOT_CHECKED_HINT, Row, first_line};
use crate::memstore::py_strip;
use crate::worktree::git::{self as runner, Run};

/// One git question under the check deadline — Python's `_git_in` — or why it went unasked:
/// git could not be started, or did not answer in time. Both are Python's `not checked (…)`.
pub(super) fn git_in(dir: &Path, args: &[&str]) -> Result<Run, String> {
    match runner::run(dir, args, CHECK_TIMEOUT) {
        Ok(run) if run.code.is_none() => Err(format!(
            "timed out after {}s: git -C {} {}",
            CHECK_TIMEOUT.as_secs(),
            dir.display(),
            args.join(" ")
        )),
        Ok(run) => Ok(run),
        Err(e) => Err(e.to_string()),
    }
}

/// `git`: is there one, and which.
///
/// Asked from `/` so the answer cannot depend on the directory it was asked in — `--version`
/// needs no repository, and a working directory deleted out from under the process must not
/// turn "git is installed" into "git is broken".
pub(super) fn git() -> Row {
    const NAME: &str = "git";
    match git_in(Path::new("/"), &["--version"]) {
        Ok(run) => Row::ok(NAME, first_line(&run.out)),
        Err(why) if why.starts_with("timed out") => Row::not_checked(NAME, why),
        Err(_) => Row::fail(
            NAME,
            "",
            "Install git: xcode-select --install (macOS) or brew install git.",
        ),
    }
}

/// `git identity`: can a commit be made at all. A plane's memory, notes and tallies are
/// committed from paths that swallow a failed commit, so without this nothing says they were
/// lost.
pub(super) fn identity(d: &Doctor) -> Row {
    const NAME: &str = "git identity";
    let ask = |key: &str| git_in(&d.cwd, &["config", "--get", key]).map(|run| first_line(&run.out));
    let (name, email) = match (ask("user.name"), ask("user.email")) {
        (Ok(name), Ok(email)) => (name, email),
        // Not "not set": charter could not ask, and an unset identity is a claim about git's
        // configuration that nothing here has read.
        (Err(why), _) | (_, Err(why)) => return Row::not_checked(NAME, why),
    };
    if !name.is_empty() && !email.is_empty() {
        return Row::ok(NAME, format!("{name} <{email}>"));
    }
    let missing: Vec<&str> = [("user.name", &name), ("user.email", &email)]
        .into_iter()
        .filter(|(_, v)| v.is_empty())
        .map(|(k, _)| k)
        .collect();
    Row::fail(
        NAME,
        format!("not set: {}", missing.join(", ")),
        "Run: git config --global user.email \"you@example.com\" && git config --global \
         user.name \"Your Name\"  — otherwise a commit (memory, workspace notes, dispatch \
         tallies) silently never happens.",
    )
}

/// `git auth`: golden rule 0 — does every repo in scope carry ITS forge's token-only policy.
/// Python's `check_ssh`.
///
/// **The same check `charter git-policy` runs, and only its read half.** [`gitpolicy::scan`]
/// and [`gitpolicy::check`] list directories and read each repo's `origin` and local config;
/// [`gitpolicy::apply`] is never called from here, so a doctor cannot change a clone's config.
/// The fix it names is `charter git-policy --apply`, which the operator runs.
///
/// [`gitpolicy::scan`]: crate::gitpolicy::scan
/// [`gitpolicy::check`]: crate::gitpolicy::check
/// [`gitpolicy::apply`]: crate::gitpolicy::apply
pub(super) fn git_auth(d: &Doctor) -> Row {
    use crate::gitpolicy::{self, UNMANAGED_FORGE};
    const NAME: &str = "git auth";
    let listing = d.root.join("workspaces");
    let (scope, unseen) = gitpolicy::scan(&d.root, &listing);
    let bad: Vec<(&PathBuf, Vec<String>)> = scope
        .iter()
        .map(|repo| (repo, gitpolicy::check(repo, &d.root)))
        .filter(|(_, drift)| !drift.is_empty())
        .collect();
    // A directory under `workspaces/` charter could not look into is named with what clears
    // it, never left out of the count without a word. `workspaces/` itself, when it could not
    // be listed, is named as itself and is then the only entry.
    let base = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let named = |p: &Path| {
        if p == listing {
            format!("{}/", base(p))
        } else {
            format!("{}/{}", p.parent().map(base).unwrap_or_default(), base(p))
        }
    };
    let cannot = if unseen.is_empty() {
        String::new()
    } else {
        let each: Vec<String> = unseen
            .iter()
            .map(|u| {
                format!(
                    "{} cannot be checked — {}",
                    named(&u.path),
                    super::fsx::uncheckable_fix(u.code, &u.path.display().to_string())
                )
            })
            .collect();
        format!("   {}.", each.join("; "))
    };
    if bad.is_empty() {
        if unseen.is_empty() {
            return Row::ok(
                NAME,
                format!(
                    "token-only across {} repo(s) (each forge's own HTTPS token; no SSH/signing)",
                    scope.len()
                ),
            );
        }
        let what = if unseen.len() == 1 && unseen[0].path == listing {
            named(&listing)
        } else {
            format!("{} director(ies) under workspaces/", unseen.len())
        };
        return Row::warn(
            NAME,
            format!(
                "token-only across {} repo(s); {what} cannot be checked",
                scope.len()
            ),
            cannot.trim_start(),
        );
    }
    // `--apply` deliberately does nothing for a repo on a forge charter cannot name, so a hint
    // sending that repo to it could never be acted on: the two cases are told apart.
    let unmanaged = bad
        .iter()
        .filter(|(_, drift)| drift.len() == 1 && drift[0] == UNMANAGED_FORGE)
        .count();
    let fixable = bad.len() - unmanaged;
    let names: Vec<String> = bad.iter().take(3).map(|(repo, _)| base(repo)).collect();
    let more = if bad.len() > 3 { " …" } else { "" };
    let hint = if fixable == 0 {
        format!(
            "{unmanaged} repo(s) have an unrecognised forge — `charter git-policy --apply` \
             deliberately no-ops for these (there's no policy to apply for a host it can't \
             identify). Declare the host under [[forge]] in charter.toml to bring them under \
             management, then re-run."
        )
    } else if unmanaged == 0 {
        "Apply the single-credential policy to every clone: charter git-policy --apply".into()
    } else {
        format!(
            "charter git-policy --apply fixes {fixable} drifted repo(s); {unmanaged} more have \
             an unrecognised forge and need a [[forge]] declaration in charter.toml first — \
             --apply alone won't touch those."
        )
    };
    Row::warn(
        NAME,
        format!(
            "{}/{} repo(s) not token-only: {}{more}",
            bad.len(),
            scope.len(),
            names.join(", ")
        ),
        hint + &cannot,
    )
}

/// What `plane root` found, before it is worded.
struct Standing {
    branch: Option<String>,
    default: Option<String>,
    dirty: usize,
    behind: u64,
    ahead: u64,
    upstream: String,
    stranded: Option<(String, String)>,
}

enum Probe {
    NotARepo,
    Inside(String),
    StatusFailed(i32),
    Read(Standing),
}

/// `plane root`: is anyone working in the plane root? ADR 0008's signal, WARN and never
/// FAIL — a root being worked in is a smell that gets expensive later, not a broken plane.
///
/// Read from already-fetched refs and never from the network: this runs from a hook.
pub(super) fn plane_root(d: &Doctor) -> Row {
    const NAME: &str = "plane root";
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    let root = d.root.as_path();
    let standing = match probe(d) {
        Err(why) => return Row::not_checked(NAME, why),
        Ok(Probe::NotARepo) => return Row::ok(NAME, "not a git repository"),
        Ok(Probe::Inside(top)) => {
            return Row::ok(NAME, format!("not its own repository (inside {top})"));
        }
        // Read for its exit status as well as its output (#917): an empty answer from a
        // `git status` that failed is not a clean tree.
        Ok(Probe::StatusFailed(code)) => {
            return Row::warn(
                NAME,
                format!("not checked (git status exited {code})"),
                NOT_CHECKED_HINT,
            );
        }
        Ok(Probe::Read(standing)) => standing,
    };

    let mut findings: Vec<String> = Vec::new();
    let mut actions: Vec<String> = Vec::new();
    match (&standing.branch, &standing.default) {
        (None, default) => {
            findings.push("detached HEAD".to_owned());
            actions.push(format!(
                "Put the root back on a branch: git -C {} checkout {}.",
                root.display(),
                default
                    .as_deref()
                    .filter(|d| !d.is_empty())
                    .unwrap_or("<your default branch>")
            ));
        }
        (Some(branch), Some(default)) if !default.is_empty() && branch != default => {
            findings.push(format!("on {branch}, not {default}"));
            actions.push(format!(
                "Put the root back: git -C {} checkout {default}.",
                root.display()
            ));
        }
        _ => {}
    }
    if standing.dirty > 0 {
        findings.push(format!("{} uncommitted file(s)", standing.dirty));
        actions.push("Commit control-plane content with `charter save`.".to_owned());
    }
    if let Some((finding, action)) = standing.stranded {
        findings.push(finding);
        actions.push(action);
    }
    let upstream = if standing.upstream.is_empty() {
        "upstream"
    } else {
        standing.upstream.as_str()
    };
    // Counts, from a ref only as current as the last fetch — which is why they say so.
    let mut drift = String::new();
    if standing.behind > 0 {
        drift.push_str(&format!(
            ", {} behind {upstream} at last fetch",
            standing.behind
        ));
    }
    if standing.ahead > 0 {
        drift.push_str(&format!(
            ", {} ahead of {upstream} at last fetch",
            standing.ahead
        ));
    }
    if findings.is_empty() {
        let branch = standing.branch.unwrap_or_default();
        return Row::ok(NAME, format!("clean on {branch}{drift}"));
    }
    Row::warn(
        NAME,
        format!("{}{drift}", findings.join(", ")),
        format!(
            "{} Anything that is not control plane belongs in a workspace clone — charter \
             workspace create <task>, then charter clone <repo>; the plane root is one working \
             tree every session shares.",
            actions.join(" ")
        ),
    )
}

/// Every git question `plane root` asks, in Python's order. Any one that could not be asked
/// costs the whole row — a partial reading of the root is not a reading of it.
fn probe(d: &Doctor) -> Result<Probe, String> {
    let root = d.root.as_path();
    let top = git_in(root, &["rev-parse", "--show-toplevel"])?;
    let toplevel = py_strip(&top.out).to_owned();
    if !top.ok() || toplevel.is_empty() {
        return Ok(Probe::NotARepo);
    }
    // Resolved on both sides: macOS hands out temp and home paths through symlinks, and git
    // answers with the physical path.
    if super::canonical(Path::new(&toplevel)) != super::canonical(root) {
        return Ok(Probe::Inside(toplevel));
    }
    let head = git_in(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let branch = head.ok().then(|| py_strip(&head.out).to_owned());
    let default = default_branch(root)?;
    // `--untracked-files=no`: memory defaults to local, so every plane a few days old
    // carries untracked files, and counting them would put this row permanently in yellow.
    let status = git_in(root, &["status", "--porcelain", "--untracked-files=no"])?;
    if !status.ok() {
        return Ok(Probe::StatusFailed(status.code.unwrap_or(-1)));
    }
    let dirty = status
        .out
        .lines()
        .filter(|l| !py_strip(l).is_empty())
        .count();
    let count = |args: &[&str]| -> Result<u64, String> {
        let run = git_in(root, args)?;
        Ok(if run.ok() {
            py_strip(&run.out).parse().unwrap_or(0)
        } else {
            0
        })
    };
    let behind = count(&["rev-list", "--count", "HEAD..@{upstream}"])?;
    let ahead = count(&["rev-list", "--count", "@{upstream}..HEAD"])?;
    let up = git_in(root, &["rev-parse", "--abbrev-ref", "@{upstream}"])?;
    let upstream = if up.ok() {
        py_strip(&up.out).to_owned()
    } else {
        String::new()
    };
    let stranded = stranded_push(d)?;
    Ok(Probe::Read(Standing {
        branch,
        default,
        dirty,
        behind,
        ahead,
        upstream,
        stranded,
    }))
}

/// The root's default branch: the remote's own answer first, then a local `main` or
/// `master`, and nothing when neither answers — naming a default charter has not discovered
/// would warn at every session of a plane whose branch is simply called something else.
fn default_branch(root: &Path) -> Result<Option<String>, String> {
    let head = git_in(
        root,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )?;
    let remote_head = py_strip(&head.out);
    if head.ok() && !remote_head.is_empty() {
        return Ok(Some(
            remote_head
                .strip_prefix("origin/")
                .unwrap_or(remote_head)
                .to_owned(),
        ));
    }
    for guess in ["main", "master"] {
        let refname = format!("refs/heads/{guess}");
        if git_in(
            root,
            &["rev-parse", "--verify", "--quiet", refname.as_str()],
        )?
        .ok()
        {
            return Ok(Some(guess.to_owned()));
        }
    }
    Ok(None)
}

/// Where charter keeps the plane's machine-local state — Python's `config.STATE_DIR`, and the
/// crate's one answer to it (`plane::state_dir`), because `save` writes the record this module
/// reads.
pub(super) fn state_dir(root: &Path) -> PathBuf {
    crate::plane::state_dir(root)
}

/// The bound on the one state file this module reads, as `contain.MAX_BYTES` bounds plane
/// data: nothing charter writes there comes near it. One number, shared with the record
/// `reopen` keeps and with the writer of this one.
const RECORD_LIMIT: u64 = crate::reopen::MAX_BYTES;

/// A small file of charter's own machine-local state, or `None` when it cannot be had —
/// `planegit.push_record`, which answers `None` for a missing, unreadable or malformed file
/// alike, because a defect in charter's own file is not a reason to take a preflight down.
///
/// Read only when it is a regular file under the bound: a FIFO there would hang the
/// preflight for ever. It is read through a link, as Python reads it — this is not plane
/// data a commit can carry (`.charter/` is ignored, or `$CHARTER_HOME` is elsewhere), and a
/// record this binary refused to follow would drop a finding Python reports.
fn read_state(path: &Path) -> Option<String> {
    use std::io::Read;
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > RECORD_LIMIT {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(RECORD_LIMIT)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// `planegit.unlanded` through `doctor._stranded_push`: a memory commit whose push did not
/// reach `origin`, as a finding and its action — or nothing.
///
/// **The record is checked, not believed.** Once the commit it names is an ancestor of the
/// tracked upstream the condition is over, whatever the file still says.
fn stranded_push(d: &Doctor) -> Result<Option<(String, String)>, String> {
    let path = state_dir(&d.root).join("plane-push.json");
    let Some(text) = read_state(&path) else {
        return Ok(None);
    };
    let Ok(serde_json::Value::Object(rec)) = serde_json::from_str::<serde_json::Value>(&text)
    else {
        return Ok(None);
    };
    if !rec.get("outcome").is_some_and(truthy) {
        return Ok(None);
    }
    let head = rec
        .get("head")
        .filter(|v| truthy(v))
        .map(py_str)
        .unwrap_or_default();
    // `--is-ancestor` exits 0 for yes and anything else for no or "could not tell"; only a
    // clean yes means the commit landed.
    if !head.is_empty()
        && git_in(
            &d.root,
            &["merge-base", "--is-ancestor", head.as_str(), "@{upstream}"],
        )?
        .ok()
    {
        return Ok(None);
    }
    let landed = rec.get("landed").filter(|v| truthy(v)).map(py_str);
    let url = rec.get("url").filter(|v| truthy(v)).map(py_str);
    let branch = rec
        .get("branch")
        .filter(|v| truthy(v))
        .map(py_str)
        .unwrap_or_else(|| "main".to_owned());
    if rec.get("outcome").and_then(serde_json::Value::as_str) == Some("branched")
        && let Some(landed) = landed
    {
        let open_it = match url {
            Some(url) => format!("Open it: {url}"),
            None => "Open a pull request for it.".to_owned(),
        };
        return Ok(Some((
            format!("a memory commit went to '{landed}', not {branch}"),
            format!(
                "'{branch}' requires a pull request, so charter pushed {landed} instead. \
                 {open_it}"
            ),
        )));
    }
    Ok(Some((
        "a memory commit was committed but never pushed".to_owned(),
        format!(
            "Push it with `charter save` before anything runs `git reset --hard \
             origin/{branch}` in {}, which would delete it silently. What the remote said is \
             in {}.",
            d.root.display(),
            path.display()
        ),
    )))
}

/// Python's truthiness of a JSON value.
fn truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
}

/// Python's `str()` of a JSON value, as an f-string interpolates it —
/// [`crate::pyrepr::str_json`], so this row and the rest of the binary quote one value one
/// way.
fn py_str(v: &serde_json::Value) -> String {
    crate::pyrepr::str_json(v)
}

/// How long ago, coarsely — `gitstate.age_phrase`, and the crate's one copy of it.
///
/// `save` prints the same phrase about the same lock (`crate::gitstate`), and two spellings of
/// "how old is this" is how one command calls a lock stale and another calls it live.
pub(super) fn age_phrase(seconds: f64) -> String {
    crate::gitstate::age_phrase(seconds)
}

/// A lock that never grew and has sat there this long is a crash, not contention — the one
/// threshold, so `doctor` and `save` cannot disagree about which locks are corpses.
const STALE_AFTER: f64 = crate::gitstate::STALE_AFTER;

/// The git directory behind `root` — `gitstate.git_dir_of`: the filesystem first, which knows
/// both a clone's `.git` directory and a worktree's `.git` file, and git only when the
/// filesystem cannot say (a plane that is a subdirectory of some larger repository).
///
/// `Err` when git had to be asked and could not answer. Python folds that into "no git
/// directory", and then prints "none held on the plane's index" over a lock nobody looked
/// for — the one place this port says "not checked" where Python says OK.
fn git_dir_of(root: &Path) -> Result<Option<PathBuf>, String> {
    let dot = root.join(".git");
    if dot.is_dir() {
        return Ok(Some(super::canonical(&dot)));
    }
    let named = std::fs::metadata(&dot)
        .ok()
        .filter(|m| m.is_file() && m.len() <= RECORD_LIMIT)
        .and_then(|_| std::fs::read_to_string(&dot).ok())
        .and_then(|text| {
            py_strip(&text)
                .strip_prefix("gitdir:")
                .map(|rest| PathBuf::from(py_strip(rest)))
        });
    if let Some(p) = named {
        let p = if p.is_absolute() { p } else { root.join(p) };
        return Ok(Some(super::canonical(&p)));
    }
    let run = git_in(root, &["rev-parse", "--git-dir"])?;
    let out = py_strip(&run.out);
    if !run.ok() || out.is_empty() {
        return Ok(None);
    }
    Ok(Some(root.join(out)))
}

/// `index lock`: a `.git/index.lock` left in the plane's own repository, noticed before a save
/// runs into it (#917). charter names it and never removes it.
pub(super) fn index_lock(d: &Doctor) -> Row {
    const NAME: &str = "index lock";
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    let lock = match git_dir_of(&d.root) {
        Err(why) => return Row::not_checked(NAME, why),
        Ok(None) => return Row::ok(NAME, "none held on the plane's index"),
        Ok(Some(dir)) => dir.join("index.lock"),
    };
    let Ok(meta) = std::fs::metadata(&lock) else {
        return Row::ok(NAME, "none held on the plane's index");
    };
    let age = meta
        .modified()
        .ok()
        .map(|m| match std::time::SystemTime::now().duration_since(m) {
            Ok(ago) => ago.as_secs_f64(),
            Err(ahead) => -ahead.duration().as_secs_f64(),
        })
        .unwrap_or(0.0);
    let size = meta.len();
    let path = super::canonical(&lock);
    let crashed = size == 0 && age >= STALE_AFTER;
    if !crashed {
        return Row::ok(
            NAME,
            format!(
                "held now — {size} byte(s), {} old; a git is probably writing",
                age_phrase(age)
            ),
        );
    }
    Row::warn(
        NAME,
        format!(
            "{} — {size} byte(s), {} old",
            path.display(),
            age_phrase(age)
        ),
        format!(
            "A git process crashed here and left the index locked; every `charter save` and \
             `git add` in this plane will refuse until it is gone. Check nothing holds it (ps \
             -eo pid,lstart,command | grep '[g]it'), then remove it yourself: rm -f {}  — \
             charter never removes a lock.",
            path.display()
        ),
    )
}
