//! What a plane says that its tree cannot: a repository's branch, commit and config, which
//! live under a `.git` whose index and reflogs carry timestamps and inodes.
//!
//! Each reader is the differential's own `facts` function, question for question, so the
//! recorded text is what the same questions answer now. The differential asked them of BOTH
//! implementations and required the answers equal; that answer is what was recorded.

use std::path::Path;
use std::process::Command;

use regex::Regex;
use serde_json::Value;

/// The facts of kind `kind`, asked of the plane at `root`. `Err` is a scenario whose setup
/// made nothing to ask about, which the differential refused to call a pass.
pub fn read(kind: &str, args: &[Value], root: &Path) -> Result<String, String> {
    let arg = |i: usize| args[i].as_str().expect("a facts argument").to_owned();
    match kind {
        "plane" => Ok(plane(root)),
        "clone" => Ok(clone(root, &arg(0), &arg(1))),
        "checkout" => guest(root, &["svc"]),
        "clone-and-worktree" => guest(root, &["svc", "svc-wt"]),
        "piece" => guest(root, &["svc", ".worktrees/svc/p1"]),
        "unrecorded" => unrecorded(root),
        "restored-clone" => restored_clone(root),
        "glstate" => Ok(glstate(root)),
        "logged" => logged(root),
        "inflight" => Ok(inflight(root)),
        "logged+inflight" => Ok(format!("{}\n{}", logged(root)?, inflight(root))),
        other => panic!("no facts reader named {other:?}"),
    }
}

/// The two config keys `git init` writes on macOS and nowhere else — the filesystem's case and
/// Unicode behaviour, probed at init — so a repository made on one platform and read on the
/// other does not differ by them.
pub fn platform_neutral(text: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| {
            let l = line.trim_start();
            !(l.starts_with("core.ignorecase=") || l.starts_with("core.precomposeunicode="))
        })
        .collect()
}

/// `git -C at …`'s stdout, in the environment the differential asked in.
fn git(at: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn plane(root: &Path) -> String {
    let mut out = vec![
        format!("branch: {}", git(root, &["symbolic-ref", "-q", "HEAD"])),
        format!("tree: {}", git(root, &["rev-parse", "HEAD^{tree}"])),
        format!("subject: {}", git(root, &["log", "-1", "--format=%s"])),
        format!(
            "committed:\n{}",
            git(root, &["show", "--name-only", "--format=", "HEAD"])
        ),
        format!("status:\n{}", git(root, &["status", "--porcelain"])),
        format!("config:\n{}", git(root, &["config", "--local", "--list"])),
    ];
    let clone = root.join("workspaces/alpha/widget");
    if clone.join(".git").is_dir() {
        out.push(format!(
            "clone config:\n{}",
            git(&clone, &["config", "--local", "--list"])
        ));
    }
    let bare = root.parent().expect("a side").join("forge/acme/plane.git");
    if bare.is_dir() {
        let named = Regex::new(r"charter/[0-9a-f]{7,}$").expect("a pattern");
        let mut refs: Vec<String> = git(&bare, &["for-each-ref", "--format=%(refname)"])
            .split_whitespace()
            .map(|r| {
                format!(
                    "{} tree {} subject {}",
                    named.replace(r, "charter/<sha>"),
                    git(&bare, &["rev-parse", &format!("{r}^{{tree}}")]).trim(),
                    git(&bare, &["log", "-1", "--format=%s", r]).trim()
                )
            })
            .collect();
        refs.sort();
        out.push(format!("remote:\n{}", refs.join("\n")));
    }
    let side = Regex::new(r"/(?:python|rust)/").expect("a pattern");
    side.replace_all(&out.join("\n"), "/<side>/").into_owned()
}

fn clone(root: &Path, name: &str, ws: &str) -> String {
    let clone = root.join("workspaces").join(ws).join(name);
    if !clone.join(".git").is_dir() {
        return "no clone".into();
    }
    let exclude = clone.join(".git/info/exclude");
    format!(
        "head: {}commit: {}config:\n{}exclude:\n{}\nstatus:\n{}",
        git(&clone, &["symbolic-ref", "-q", "HEAD"]),
        git(&clone, &["rev-parse", "HEAD"]),
        git(&clone, &["config", "--local", "--list"]),
        std::fs::read_to_string(&exclude).unwrap_or_else(|_| "<none>".into()),
        git(&clone, &["status", "--porcelain"]),
    )
}

fn guest(root: &Path, rels: &[&str]) -> Result<String, String> {
    let mut out = Vec::new();
    for rel in rels {
        let tree = root.join("workspaces/beta").join(rel);
        if !tree.join(".git").exists() {
            return Err(format!(
                "there is no checkout at workspaces/beta/{rel}, so this scenario would \
                 compare nothing about it"
            ));
        }
        let common = git(
            &tree,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        );
        let common = common.trim();
        let text = if common.is_empty() {
            "<none>".to_owned()
        } else {
            std::fs::read_to_string(Path::new(common).join("info/exclude"))
                .unwrap_or_else(|_| "<none>".into())
        };
        let status = git(&tree, &["status", "--porcelain"]);
        out.push(format!("--- {rel}\nexclude:\n{text}\nstatus:\n{status}"));
    }
    Ok(out.join("\n"))
}

fn unrecorded(root: &Path) -> Result<String, String> {
    let mut notes: Vec<_> = std::fs::read_dir(root.join(".charter/unrecorded"))
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    notes.sort();
    if notes.is_empty() {
        return Err("no record-publish failure was kept".into());
    }
    let kept: Vec<String> = notes
        .iter()
        .map(|n| std::fs::read_to_string(n).expect("a note reads"))
        .collect();
    Ok(format!(
        "{}\nunrecorded:\n{}",
        guest(root, &["svc"])?,
        kept.join("\n")
    ))
}

fn restored_clone(root: &Path) -> Result<String, String> {
    let tree = root.join("workspaces/alpha/api");
    if !tree.join(".git").is_dir() {
        return Err("there is no clone at workspaces/alpha/api".into());
    }
    let answer = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&tree)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/nonexistent")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git runs");
        match out.status.code() {
            Some(0) => String::from_utf8_lossy(&out.stdout).trim().to_owned(),
            code => format!("<rc {}>", code.unwrap_or(-1)),
        }
    };
    Ok(format!(
        "head: {}\nbranch: {}\nstatus:\n{}\n",
        answer(&["rev-parse", "HEAD"]),
        answer(&["rev-parse", "--abbrev-ref", "HEAD"]),
        answer(&["status", "--porcelain"]),
    ))
}

fn glstate(root: &Path) -> String {
    let cache = root.join(".charter/cache/glstate.json");
    let Ok(mut text) = std::fs::read_to_string(&cache) else {
        return "no cache".into();
    };
    let mut spellings = vec![root.to_string_lossy().into_owned()];
    if let Ok(real) = root.canonicalize() {
        spellings.push(real.to_string_lossy().into_owned());
    }
    spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for spelling in spellings {
        text = text.replace(&spelling, "<plane>");
    }
    text
}

fn logged(root: &Path) -> Result<String, String> {
    let mut rows = Vec::new();
    for sub in ["_dispatch", "_skills"] {
        let dir = root.join("personas").join(sub);
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<_> = read
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .collect();
        files.sort();
        for f in files {
            let name = f
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            let month = name.split('.').next().unwrap_or_default().to_owned();
            for line in std::fs::read_to_string(&f).expect("a log reads").lines() {
                rows.push(format!("{sub}/{month}: {line}"));
            }
        }
    }
    if rows.is_empty() {
        return Err(format!("nothing was logged under {}", root.display()));
    }
    Ok(rows.join("\n"))
}

fn inflight(root: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let dir = root.join(".charter/dispatch-inflight");
    let Ok(read) = std::fs::read_dir(&dir) else {
        return "(no directory)".into();
    };
    let mut rows: Vec<String> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|f| {
            let rec: Value = serde_json::from_str(&std::fs::read_to_string(&f).expect("reads"))
                .expect("a record is JSON");
            let mode = std::fs::metadata(&f).expect("stat").permissions().mode() & 0o777;
            format!(
                "{} {} {} 0o{mode:o}",
                py(&rec["agent"]),
                py(&rec["kind"]),
                py(&rec["ts"])
            )
        })
        .collect();
    rows.sort();
    rows.join("\n")
}

/// A JSON value the way Python's f-string printed it.
fn py(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        other => other.to_string(),
    }
}
