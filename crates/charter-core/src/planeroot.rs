//! Arms A3 and A3b: the plane root is one shared working tree — refuse a branch move in it
//! (#157), and refuse a `git reset` there that would destroy commits no remote has (#401).
//!
//! A port of `charter/hooks.py`'s `_plane_root_branch_reason`, `_plane_root_reset_reason` and
//! everything they close over that no earlier stage shipped: `_plane_root_git` (the walk both
//! share), `_git_target` (the directories one invocation acts on), `_checkout_opt_kind`,
//! `_checkout_operand_kind`, `_created_branch`, `_inline_aliases`, `_resolve_git_alias`,
//! `_unpushed_at_risk`, `doctor._plane_default_branch`, and their tables. The line is read by
//! [`crate::shellseg`], each segment's program and environment by [`crate::shellwrap`] (whose
//! `exported_env` answers "what did an earlier segment export", #496), and a repository's own
//! `core.worktree` by [`crate::gitconfig`].
//!
//! # Where this is called from
//!
//! [`crate::toolgate`] — `charter/hooks.py:pretooluse`'s eight refusals, in its order — and
//! through it `charter hook pretooluse` (M3.1 stage 6).
//!
//! Until that stage the switch was closed: `main.rs`'s `is_a_tool_hook` answered every word in
//! the `pretooluse`/`posttooluse` namespace with exit 2 — *block* — because a PARTIAL guard on
//! the switch turns fail-closed into allow-everything-except-the-arm-that-is-ported. It moved
//! once all eight arms were standing, and never earlier. The other eight words in that
//! namespace still block, for the same reason they always did.
//! # These guards ask git, and how they ask is part of the answer
//!
//! Four questions are put to a real git: does an operand resolve to a commit, does git track it
//! as a path, what is an alias's body, and how many commits would a reset take off the branch
//! that no upstream has. Python asks through `doctor._git_in`, which inherits the hook's whole
//! environment; this asks through [`crate::worktree::git::run_as_session`], the hardened runner
//! (a constructed environment, no hooks, no fsmonitor, git found in a fixed list of directories)
//! plus the variables that say where git's CONFIG is. The second half matters: an alias in a
//! relocated global config is exactly what the command about to run will expand, and a guard
//! whose git could not see it would stand aside while the root's branch moved.
//!
//! The deadline is Python's, five seconds (`doctor.CHECK_TIMEOUT`), and what a question that
//! could not be asked answers is Python's per call site — which is not one rule:
//!
//! * an operand that could not be classified is `unknown`, and that **keeps the refusal**;
//! * an alias that could not be read is "not an alias", which **stands aside** — charter has no
//!   reason yet to believe the command is a branch move;
//! * a reset whose unpushed count could not be read is **allowed** — the guard only speaks when
//!   it has measured that something would be lost, the same silence `doctor` keeps;
//! * a default branch that could not be read is "none", which withholds the remedy's carve-out.
//!
//! "Could not be asked" is also Python's: the process could not start, the deadline passed, an
//! argument held a NUL, or git wrote bytes that are not UTF-8 — Python decodes a child's output
//! strictly and raises. [`crate::worktree::git::RawRun`] keeps the bytes so that last case is
//! told apart from a U+FFFD git really wrote.
//!
//! # Where the Python raises and this answers
//!
//! The Python raises out of these guards on three inputs, and a raise out of `pretooluse` is an
//! ALLOW (charter#1166, and charter#1178 for these sites): a symlink loop among an invocation's
//! subjects on CPython 3.11/3.12, a NUL in `-C`/`--work-tree`/`--git-dir`, and a default branch
//! whose name is not UTF-8. This answers all three — the loop is resolved as CPython 3.14 does,
//! the NUL is a path that is not the root, and the undecodable default is no default — and the
//! differential keeps those inputs out of its alphabet, because it cannot arbitrate an input one
//! side has no answer for. Unit tests below pin what this does there.
//!
//! # Where this parts from the frozen Python, on purpose
//!
//! The Python oracle recognised git and the root by SPELLING and modelled every `cd` as
//! succeeding (charter#1176, charter#1177). Both were fail-opens, fixed here and recorded as
//! changed corpus rows (ADR 0046):
//!
//! * **#346** — `git` is recognised folded (`GIT` runs git on APFS and NTFS), inline aliases are
//!   keyed folded as git keys them, and a subject is compared to the root by identity and by the
//!   repository git would discover from it ([`TheRoot`]), not as a resolved string.
//! * **#345** — a `cd` only moves the later segments for certain when its failure would stop them
//!   (the shell's own `cd`, joined by `&&`); any other `cd` adds a directory the shell may be in
//!   and keeps the one it was in ([`Whereabouts`]).

use crate::gitconfig;
use crate::memstore::py_strip;
use crate::pypath::{is_abs, normpath, path_div, pure_path, realpath};
use crate::shellseg;
use crate::shellwrap::{self, GIT_VALUE_OPTS};
use crate::worktree::git as runner;

/// git subcommands that move HEAD from one branch to another — `_BRANCH_MOVERS`.
///
/// Deliberately short: the evidence in #157 is about SWITCHING. `reset` has its own guard with
/// its own subject and remedy, because "HEAD moved between branches" and "commits were destroyed"
/// are two findings and only one sentence can be the denial.
pub const BRANCH_MOVERS: [&str; 2] = ["checkout", "switch"];

/// Options that make a `checkout`/`switch` CREATE a branch — `_BRANCH_CREATOR_OPTS`. The operand
/// of one of these is a name to create, never a path to restore. `--orphan` was the round-one
/// bypass: `git checkout --orphan README` read as a restore of `README`.
pub const BRANCH_CREATOR_OPTS: [&str; 7] = [
    "-b",
    "-B",
    "-c",
    "-C",
    "--orphan",
    "--create",
    "--force-create",
];

/// Options that take HEAD off its branch — `_DETACH_OPTS`. `--detach` needs no operand at all.
pub const DETACH_OPTS: [&str; 2] = ["--detach", "-d"];

/// Options a RESTORE accepts, and that cannot move HEAD — `_RESTORE_OPTS`.
///
/// An ALLOWLIST, and the direction is the point: "is every option here one I can show a restore
/// accepts?" keeps the guard shut for an option git adds next year, where "is it one of the four
/// I know move HEAD?" opened it for `--orphan`, `--track` and `--guess` the day it was written.
pub const RESTORE_OPTS: [&str; 19] = [
    "--ours",
    "--theirs",
    "--force",
    "--merge",
    "--patch",
    "--quiet",
    "--progress",
    "--conflict",
    "--overlay",
    "--ignore-skip-worktree-bits",
    "--pathspec-from-file",
    "--pathspec-file-nul",
    "--recurse-submodules",
    "--overwrite-ignore",
    "--ignore-other-worktrees",
    "--source",
    "--staged",
    "--worktree",
    "--ignore-unmerged",
];

/// The same list in git's short spelling, as LETTERS — `_RESTORE_SHORTS`. `-fq` is one token and
/// `-bREADME` is one token; `-2`/`-3` are `--ours`/`--theirs`.
pub const RESTORE_SHORTS: &str = "fmpq23";

/// How many trailing operands of a `git checkout <tree-ish> <paths…>` are resolved before the
/// guard stops and keeps refusing — `_MAX_CHECKOUT_OPERANDS`. Two local git questions each, on
/// the `PreToolUse` path, so the work an agent can ask for here is bounded.
pub const MAX_CHECKOUT_OPERANDS: usize = 32;

/// `git reset` modes that overwrite the WORKING TREE as they move HEAD — `_RESET_TREE_MODES`.
/// `--soft` and `--mixed` leave every byte on disk for the next `charter save` to re-land.
pub const RESET_TREE_MODES: [&str; 3] = ["--hard", "--merge", "--keep"];

/// The environment spellings of the two global options that NAME A REPOSITORY — `_GIT_DIR_ENV`.
/// `GIT_DIR=<plane>/.git git checkout feature` moves the root's HEAD from anywhere.
pub const GIT_DIR_ENV: [(&str, &str); 2] = [("GIT_DIR", "git_dir"), ("GIT_WORK_TREE", "work_tree")];

/// Subcommands taken to be git's own, so they are not asked about as aliases —
/// `_GIT_KNOWN_SUBCOMMANDS`.
///
/// **A cost list, never a safety list**: a name missing from it costs one `git config --get`.
/// Everything that MOVES HEAD is deliberately absent, so the resolution still runs for
/// `checkout` and `switch`.
pub const GIT_KNOWN_SUBCOMMANDS: [&str; 67] = [
    "add",
    "am",
    "annotate",
    "apply",
    "archive",
    "bisect",
    "blame",
    "branch",
    "bundle",
    "cat-file",
    "check-ignore",
    "cherry",
    "cherry-pick",
    "clean",
    "clone",
    "commit",
    "config",
    "describe",
    "diff",
    "difftool",
    "fetch",
    "for-each-ref",
    "format-patch",
    "fsck",
    "gc",
    "grep",
    "help",
    "init",
    "log",
    "ls-files",
    "ls-remote",
    "ls-tree",
    "merge",
    "merge-base",
    "mergetool",
    "mv",
    "notes",
    "pull",
    "push",
    "range-diff",
    "rebase",
    "reflog",
    "remote",
    "repack",
    "replace",
    "rerere",
    "reset",
    "restore",
    "rev-list",
    "rev-parse",
    "revert",
    "rm",
    "shortlog",
    "show",
    "show-ref",
    "sparse-checkout",
    "stash",
    "status",
    "submodule",
    "symbolic-ref",
    "tag",
    "update-index",
    "update-ref",
    "verify-commit",
    "version",
    "whatchanged",
    "worktree",
];

/// How many alias hops are followed — `_MAX_ALIAS_HOPS`. git resolves an alias to an alias
/// (`ck = co`, `co = checkout` switches branches), so one hop is not enough and unbounded is a
/// loop.
pub const MAX_ALIAS_HOPS: usize = 4;

/// What one `checkout`/`switch` option does — `_checkout_opt_kind`'s four answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptKind {
    Create,
    Detach,
    Restore,
    /// Anything this cannot place — treated as able to move HEAD, so the fail-closed default
    /// belongs to the option charter has never seen.
    Unknown,
}

impl OptKind {
    /// The Python's string for it.
    pub fn as_str(self) -> &'static str {
        match self {
            OptKind::Create => "create",
            OptKind::Detach => "detach",
            OptKind::Restore => "restore",
            OptKind::Unknown => "unknown",
        }
    }
}

/// Classify one option of a `git checkout`/`git switch` — `_checkout_opt_kind`.
///
/// git takes an option's value ATTACHED as readily as separated (`-bREADME`, `--orphan=README`)
/// and clusters short options (`-fq`), so this normalises to the option NAME before answering. A
/// short cluster is read left to right and stops at the first letter that decides: `-b` swallows
/// the rest of the token as the new branch's name. `--no-x` is normalised to `--x` and stays on
/// the same side of the fence.
pub fn checkout_opt_kind(tok: &str) -> OptKind {
    if tok.starts_with("--") {
        let mut name = tok.split('=').next().unwrap_or(tok).to_string();
        if let Some(rest) = name.strip_prefix("--no-") {
            name = format!("--{rest}");
        }
        if BRANCH_CREATOR_OPTS.contains(&name.as_str()) {
            return OptKind::Create;
        }
        if DETACH_OPTS.contains(&name.as_str()) {
            return OptKind::Detach;
        }
        return if RESTORE_OPTS.contains(&name.as_str()) {
            OptKind::Restore
        } else {
            OptKind::Unknown
        };
    }
    for ch in tok.chars().skip(1) {
        let letter = format!("-{ch}");
        if BRANCH_CREATOR_OPTS.contains(&letter.as_str()) {
            return OptKind::Create;
        }
        if DETACH_OPTS.contains(&letter.as_str()) {
            return OptKind::Detach;
        }
        if !RESTORE_SHORTS.contains(ch) {
            return OptKind::Unknown;
        }
    }
    OptKind::Restore
}

/// Every directory a git invocation's GLOBAL options aim it at — `_git_target`.
///
/// Exactly: the cwd, always (after any `-C`s, each joined onto the last, as git does); the
/// `--work-tree`/`GIT_WORK_TREE` if one is named; the `--git-dir`/`GIT_DIR` and its `..` if one
/// is named (the `..` is appended rather than taken lexically, so the caller's `realpath`
/// collapses `.git/refs/..` the way the filesystem does); and the `core.worktree` the repository
/// this invocation is aimed at writes in its own config (#504).
///
/// **It only ever GROWS**: a subject list that got smaller as the command line got longer would
/// be a flag-shaped bypass by construction, so the cwd is kept even when `--work-tree` is named
/// (git still discovers the REFS from the cwd).
///
/// `pre` is `git_globals`' first half — git's own options ONLY — and `env` is the invocation's
/// environment, earlier exports first: an option on the line overrides the environment.
///
/// Every path is the string `str(Path(...))` would be.
pub fn git_target(cwd: &str, pre: &[String], env: &[String]) -> Vec<String> {
    let mut here = pure_path(if cwd.is_empty() { "." } else { cwd });
    let mut git_dir: Option<String> = None;
    let mut work_tree: Option<String> = None;
    for assign in env {
        let (name, val) = assign.split_once('=').unwrap_or((assign.as_str(), ""));
        match GIT_DIR_ENV
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, w)| *w)
        {
            Some("git_dir") => git_dir = Some(val.to_string()),
            Some("work_tree") => work_tree = Some(val.to_string()),
            _ => {}
        }
    }
    let mut i = 0usize;
    while i < pre.len() {
        let tok = pre[i].as_str();
        let (name, attached) = match tok.split_once('=') {
            Some((n, a)) => (n, Some(a)),
            None => (tok, None),
        };
        // Only the separated `-C`: git rejects the attached one outright.
        if tok == "-C" && i + 1 < pre.len() {
            let val = pre[i + 1].as_str();
            if !val.is_empty() {
                here = path_div(&here, val); // `-C ""` leaves the directory unchanged
            }
            i += 2;
            continue;
        }
        if name == "--git-dir" || name == "--work-tree" {
            let (val, step) = match attached {
                Some(a) => (a.to_string(), 1),
                None => (pre.get(i + 1).cloned().unwrap_or_default(), 2),
            };
            if !val.is_empty() {
                if name == "--git-dir" {
                    git_dir = Some(val);
                } else {
                    work_tree = Some(val);
                }
            }
            i += step;
            continue;
        }
        i += if GIT_VALUE_OPTS.contains(&tok) { 2 } else { 1 };
    }
    let at = |p: &str| path_div(&here, p);
    let mut out = vec![here.clone()];
    if let Some(wt) = &work_tree {
        out.push(at(wt));
    }
    let named = git_dir.as_deref().map(at);
    if let Some(gd) = &named {
        out.push(gd.clone());
        out.push(path_div(gd, ".."));
    }
    if let Some(configured) = gitconfig::configured_work_tree(&here, named.as_deref()) {
        out.push(configured);
    }
    out
}

/// `Path(p).resolve()` as a string — CPython's non-strict `realpath` of the `Path`'s string.
fn resolved(p: &str) -> String {
    pure_path(&realpath(&pure_path(p)))
}

/// One git invocation that acts on the plane root: its subcommand as WRITTEN, the arguments
/// after it, and git's own options before it (one of which can define the subcommand: `git -c
/// alias.co=checkout co feature`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootInvocation {
    pub sub: String,
    pub post: Vec<String>,
    pub pre: Vec<String>,
}

/// The plane root as the walk recognises it: by IDENTITY, never by spelling (#346).
///
/// A re-cased path on a case-insensitive filesystem and macOS's `/System/Volumes/Data` firmlink
/// both reach the root while [`resolved`] spells them differently, so a subject is compared by
/// `(st_dev, st_ino)`, falling back to the resolved string only where it cannot be stat-ed. And
/// git does not act on the directory it runs in but on the repository it DISCOVERS upward from
/// it, so a subject inside the root's own repository — `docs/`, or a directory the same command
/// is about to make — reaches the root too, while a repository of its own inside it (a workspace
/// clone) stops the ascent and does not.
struct TheRoot {
    path: String,
    id: Option<(u64, u64)>,
    /// The root's own git directory and its identity, when the root has a `.git` of its own.
    git_dir: Option<(String, Option<(u64, u64)>)>,
}

impl TheRoot {
    fn of(root: &str) -> Self {
        let own = std::fs::symlink_metadata(path_div(root, ".git")).is_ok();
        let git_dir = own
            .then(|| gitconfig::git_dir_at(root))
            .flatten()
            .map(|gd| {
                let gd = resolved(&gd);
                let id = identity(&gd);
                (gd, id)
            });
        TheRoot {
            path: root.to_string(),
            id: identity(root),
            git_dir,
        }
    }

    /// Whether git, pointed at the subject `t`, acts on the root's repository.
    fn is_reached_from(&self, t: &str) -> bool {
        let t = resolved(t);
        if same_dir(&t, &self.path, self.id) {
            return true;
        }
        let Some((root_gd, root_gd_id)) = &self.git_dir else {
            return false;
        };
        gitconfig::git_dir_at(&t).is_some_and(|gd| same_dir(&resolved(&gd), root_gd, *root_gd_id))
    }
}

/// Whether the resolved `path` and `other` (whose identity is `other_id`) are one directory: by
/// identity where both can be stat-ed, by the resolved string where either cannot.
fn same_dir(path: &str, other: &str, other_id: Option<(u64, u64)>) -> bool {
    match (identity(path), other_id) {
        (Some(a), Some(b)) => a == b,
        _ => path == other,
    }
}

/// `str(Path(dir) / rel)`, with an empty `dir` read as the current directory.
fn join_dir(dir: &str, rel: &str) -> String {
    path_div(&pure_path(if dir.is_empty() { "." } else { dir }), rel)
}

/// [`crate::pypath::file_identity`] of a path string.
fn identity(path: &str) -> Option<(u64, u64)> {
    crate::pypath::file_identity(std::path::Path::new(path))
}

/// Where the later segments of one command line may be running: every directory a `cd` so far
/// could have left the shell in, and whether one of them is a directory this cannot name
/// (`cd "$DIR"`, `cd -`, `popd`), which may be the root.
///
/// **It only ever grows while the shell might still be elsewhere**, as [`git_target`]'s list does
/// and for the same reason: a `cd` is only taken to REPLACE where the shell is when its failure
/// would stop what follows, and a `cd` that fails leaves the shell where it was (#345).
#[derive(Clone, Debug, Default)]
struct Whereabouts {
    dirs: Vec<String>,
    anywhere: bool,
}

impl Whereabouts {
    fn at(dir: &str) -> Self {
        Whereabouts {
            dirs: vec![dir.to_string()],
            anywhere: false,
        }
    }

    fn merge(&mut self, other: Whereabouts) {
        for d in other.dirs {
            if !self.dirs.contains(&d) {
                self.dirs.push(d);
            }
        }
        self.anywhere |= other.anywhere;
    }
}

/// The shell's own directory-changing builtins, by the word the shell looks up: the lookup is
/// case-SENSITIVE (a builtin is found before `PATH`), so `CD` is not one of them — on a
/// case-insensitive filesystem it runs `/usr/bin/cd`, a program that cannot move the shell.
const CD_BUILTINS: [&str; 3] = ["cd", "pushd", "popd"];

/// One `cd`, `pushd` or `popd` in a segment.
struct CdCall {
    builtin: String,
    args: Vec<String>,
    /// CERTAINLY the shell's own builtin run in this shell: the bare word at the front, after
    /// assignments and an optional `builtin` or `command`. Anything else that names one (`if cd
    /// x`, `! cd x`, `time cd x`, `env cd x`, `/usr/bin/cd x`) may or may not move the shell, and
    /// a guard that cannot tell takes both directories.
    certain: bool,
}

/// The `cd`-shaped call in one segment, if it is one — see [`CdCall`].
fn cd_call(toks: &[String], prog: &str, args: &[String]) -> Option<CdCall> {
    let mut i = 0;
    while i < toks.len() && shellwrap::is_env_assignment(&toks[i]) {
        i += 1;
    }
    if matches!(toks.get(i).map(String::as_str), Some("builtin" | "command")) {
        i += 1;
    }
    if let Some(word) = toks.get(i)
        && CD_BUILTINS.contains(&word.as_str())
    {
        return Some(CdCall {
            builtin: word.clone(),
            args: toks[i + 1..].to_vec(),
            certain: true,
        });
    }
    let base = shellwrap::basename(prog);
    CD_BUILTINS.contains(&base).then(|| CdCall {
        builtin: base.to_string(),
        args: args.get(1..).unwrap_or(&[]).to_vec(),
        certain: false,
    })
}

/// What the whole command line does to the meaning of a `cd` in it — read once, from its text.
///
/// Each is a fail-closed reading of a substring: a line that merely mentions `HOME` loses the
/// `~` shortcut, which costs nothing but a refusal the denial's own remedy answers.
struct CdContext {
    /// The line may redefine `cd` — a function (`cd() …`, `function cd`), an `alias`, or an
    /// `enable -n` — so no `cd` in it is certainly the builtin.
    redefined: bool,
    /// The line may set `HOME`, so `~` and a bare `cd` go where this cannot tell.
    sets_home: bool,
    /// `CDPATH` is in play — set on the line, or in the environment the shell inherits — so a
    /// relative name may resolve somewhere else entirely.
    cdpath: bool,
}

impl CdContext {
    fn of(cmd: &str) -> Self {
        static REDEFINES: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let redefines = REDEFINES.get_or_init(|| {
            regex::Regex::new(
                r"(?:^|[^A-Za-z0-9_])(?:(?:cd|pushd|popd)\s*\(|function\s+(?:cd|pushd|popd)\b)|(?:^|[;&|({\n]|\bthen|\bdo)\s*(?:builtin\s+|command\s+)?(?:alias|enable)\b",
            )
            .expect("compiles")
        });
        CdContext {
            redefined: redefines.is_match(cmd),
            sets_home: cmd.contains("HOME"),
            cdpath: cmd.contains("CDPATH")
                || std::env::var_os("CDPATH").is_some_and(|v| !v.is_empty()),
        }
    }
}

/// Where one `cd`/`pushd`/`popd` sends the shell from `here`, read as the shell reads it.
///
/// A destination this cannot name is `anywhere`: `popd`, `cd -` (`$OLDPWD`), `pushd` with no
/// directory or a stack rotation, a word the shell will still expand (`$`, a glob, a brace), a
/// `~user`, a `~` when the line may set `HOME`, a second operand (zsh's `cd old new`), and — when
/// `CDPATH` is in play — a relative name `CDPATH` may resolve elsewhere. `pushd -n` moves nothing.
/// `~` and a bare `cd` are `$HOME`. A `..` is read both ways, because `cd` takes `a/link/..`
/// LOGICALLY (back in `a`) where [`resolved`] takes it physically; both readings are kept.
fn cd_destinations(
    builtin: &str,
    args: &[String],
    here: &Whereabouts,
    line: &CdContext,
) -> Whereabouts {
    let anywhere = || Whereabouts {
        dirs: Vec::new(),
        anywhere: true,
    };
    if builtin == "popd" {
        return anywhere();
    }
    let mut operands = args.iter().filter(|a| *a == "-" || !a.starts_with('-'));
    let dest = operands.next();
    if operands.next().is_some() {
        return anywhere(); // zsh: `cd old new` substitutes in the current path
    }
    let no_change = args
        .iter()
        .any(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('n'));
    if builtin == "pushd" && no_change {
        return here.clone(); // `pushd -n` edits the stack and stays put
    }
    let dest = match (builtin, dest) {
        (_, Some(d)) if d == "-" || d.starts_with('+') => return anywhere(),
        ("pushd", None) => return anywhere(),
        (_, None) => "~".to_string(),
        (_, Some(d)) if d.is_empty() => return here.clone(), // `cd ""` stays put
        (_, Some(d)) => d.clone(),
    };
    if dest.contains(['$', '*', '?', '[', '{', '`']) {
        return anywhere();
    }
    let dest = if dest == "~" || dest.starts_with("~/") {
        if line.sets_home {
            return anywhere();
        }
        match std::env::var("HOME") {
            Ok(home) if is_abs(&home) => format!("{home}{}", &dest[1..]),
            _ => return anywhere(),
        }
    } else if dest.starts_with('~') {
        return anywhere();
    } else {
        dest
    };
    if !is_abs(&dest) {
        let walks_cdpath =
            !(dest == "." || dest == ".." || dest.starts_with("./") || dest.starts_with("../"));
        if walks_cdpath && line.cdpath {
            return anywhere();
        }
    }
    let mut out = Whereabouts {
        dirs: Vec::new(),
        anywhere: here.anywhere && !is_abs(&dest),
    };
    let from: Vec<&str> = if is_abs(&dest) {
        vec![""]
    } else {
        here.dirs.iter().map(String::as_str).collect()
    };
    for h in from {
        let physical = if is_abs(&dest) {
            dest.clone()
        } else {
            join_dir(h, &dest)
        };
        let logical = normpath(&physical);
        out.merge(Whereabouts::at(&physical));
        out.merge(Whereabouts::at(&logical));
    }
    out
}

/// Every git invocation in `cmd` that acts on the PLANE ROOT — `_plane_root_git`, the walk both
/// guards share, so the two of them share one pair of eyes.
///
/// `root` is the plane root as `Path(config.ROOT).resolve()` makes it; [`plane_root`] makes it.
///
/// * **Fails OPEN on a command that would not tokenize**, in one place: the re-segmented guess
///   is made without quoting, and a phantom `git checkout` out of a broken quote must not stop a
///   turn. The leak and one-credential guards, which may not miss, keep scanning it.
/// * **A `cd` earlier in the SAME command moves where later segments run** (#183) — but only a
///   `cd` whose failure stops them is taken to have moved them for certain: the shell's own `cd`,
///   joined to what follows by `&&`, and only until that `&&` chain ends. Every other `cd` — one
///   followed by `;`, `||`, `&` or a newline, one in a pipeline or a subshell, one run as a
///   program — ADDS its destination and keeps the directory the shell was in (#345).
/// * **A wrapper's own chdir flag** (`env -C`, `sudo --chdir`) moves git as a `cd` would.
/// * **Any** subject being the root is enough ([`git_target`]), from any directory the shell may
///   be in.
pub fn plane_root_git(cmd: &str, cwd: &str, root: &str) -> Vec<RootInvocation> {
    let Some(segments) = shellseg::joined_argv(cmd) else {
        return Vec::new();
    };
    let argvs: Vec<Vec<String>> = segments.iter().map(|s| s.argv.clone()).collect();
    let carried = shellwrap::exported_env(&argvs);
    // Stat-ed on the first git invocation, so a line with none pays nothing for it.
    let the_root = std::cell::OnceCell::new();
    let mut out = Vec::new();
    let mut here = Whereabouts::at(cwd);
    // Where the shell stays if a `cd … &&` failed: back in play once the `&&` chain ends.
    let mut left_behind = Whereabouts::default();
    let context = CdContext::of(cmd);
    for (seg, before) in segments.iter().zip(carried.iter()) {
        let call = shellwrap::split_env_chdir(&seg.argv);
        let (prog, env, args, chdir) = (call.prog, call.env, call.argv, call.chdir);
        let mut inherited = before.clone();
        inherited.extend(env);
        if let Some(cd) = cd_call(&seg.argv, &prog, &args) {
            let moved = cd_destinations(&cd.builtin, &cd.args, &here, &context);
            let piped = matches!(seg.before.as_deref(), Some("|" | "|&"));
            if cd.certain && !context.redefined && !piped && seg.after.as_deref() == Some("&&") {
                left_behind.merge(std::mem::replace(&mut here, moved));
            } else {
                here.merge(moved);
            }
        } else if shellwrap::base_lower(&prog) == "git" {
            // Folded: on APFS and NTFS `GIT` runs git (#346).
            let (pre, rest) = shellwrap::git_globals(&args);
            if let Some((sub, post)) = rest.split_first() {
                // `before + env`, in that order: an assignment on THIS invocation overrides an
                // export.
                let on_root = here.anywhere
                    || here.dirs.iter().any(|dir| {
                        let dir = if chdir.is_empty() {
                            dir.clone()
                        } else {
                            join_dir(dir, &chdir) // `env -C`, `sudo --chdir`
                        };
                        git_target(&dir, &pre, &inherited).iter().any(|t| {
                            the_root
                                .get_or_init(|| TheRoot::of(root))
                                .is_reached_from(t)
                        })
                    });
                if on_root {
                    out.push(RootInvocation {
                        sub: sub.clone(),
                        post: post.to_vec(),
                        pre,
                    });
                }
            }
        }
        if !matches!(seg.after.as_deref(), Some("&&" | "|" | "|&")) {
            here.merge(std::mem::take(&mut left_behind));
        }
    }
    out
}

/// `Path(config.ROOT).resolve()` — the string every subject is compared to.
pub fn plane_root(root: &str) -> String {
    resolved(root)
}

/// One git question could not be put: the process did not start, the deadline passed, an
/// argument held a NUL, or git wrote bytes that are not UTF-8 — Python's `ProcTimeout`,
/// `OSError` and `ValueError`.
#[derive(Debug)]
struct Unasked;

/// What one git question answered, as Python's `CompletedProcess` would hold it.
struct Answer {
    ok: bool,
    out: String,
}

/// One read-only git question about `root` — `doctor._git_in`, through the session-aware runner.
fn git_in(root: &str, args: &[&str]) -> Result<Answer, Unasked> {
    let raw = runner::run_as_session(
        std::path::Path::new(root),
        args,
        crate::doctor::CHECK_TIMEOUT,
    )
    .map_err(|_| Unasked)?;
    let code = raw.code.ok_or(Unasked)?;
    // `text=True` decodes BOTH streams strictly; either failing is the `ValueError`.
    let out = String::from_utf8(raw.out).map_err(|_| Unasked)?;
    String::from_utf8(raw.err).map_err(|_| Unasked)?;
    // ...and translates newlines universally.
    let out = out.replace("\r\n", "\n").replace('\r', "\n");
    Ok(Answer { ok: code == 0, out })
}

/// What `git checkout <op>` would make of an operand — `_checkout_operand_kind`'s five answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperandKind {
    Rev,
    Path,
    Both,
    Neither,
    /// git could not be asked — kept denied: a guard that opened because it failed to ask is the
    /// fail-open shape #438 is about.
    Unknown,
}

impl OperandKind {
    /// The Python's string for it.
    pub fn as_str(self) -> &'static str {
        match self {
            OperandKind::Rev => "rev",
            OperandKind::Path => "path",
            OperandKind::Both => "both",
            OperandKind::Neither => "neither",
            OperandKind::Unknown => "unknown",
        }
    }
}

/// What `git checkout <op>` would make of `op` in `root` — `_checkout_operand_kind` — **asked of
/// git**, never inferred from the spelling.
///
/// `rev-parse --verify --quiet <op>^{commit}` (does it resolve to a commit?) and `ls-files
/// --error-unmatch -- <op>` (does git TRACK it?). Only `Path` — git cannot read it as a commit
/// and can read it as a tracked path — lets a command through; `Both` is refused (git breaks the
/// tie for the ref), `Neither` is refused (a remote-only branch is DWIM-created), and `Unknown` is
/// refused. `-` is `@{-1}`, a ref, and is never handed to git, where it would read as an option.
pub fn checkout_operand_kind(root: &str, op: &str) -> OperandKind {
    if op == "-" {
        return OperandKind::Rev;
    }
    let spec = format!("{op}^{{commit}}");
    let rev = match git_in(root, &["rev-parse", "--verify", "--quiet", &spec]) {
        Ok(a) => a.ok,
        Err(_) => return OperandKind::Unknown,
    };
    let path = match git_in(root, &["ls-files", "--error-unmatch", "--", op]) {
        Ok(a) => a.ok,
        Err(_) => return OperandKind::Unknown,
    };
    match (rev, path) {
        (true, true) => OperandKind::Both,
        (true, false) => OperandKind::Rev,
        (false, true) => OperandKind::Path,
        (false, false) => OperandKind::Neither,
    }
}

/// Aliases defined ON THE COMMAND LINE: `git -c alias.co=checkout co feature` —
/// `_inline_aliases`. Only the separated `-c <name>=<value>`: git rejects the attached form.
/// Later definitions win, as a dict's later assignment does. The NAME is kept LOWER-CASED, git's
/// own rule for the last component of a config key, so `-c alias.CO=checkout co` is found (#346).
pub fn inline_aliases(pre: &[String]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for j in 0..pre.len().saturating_sub(1) {
        if pre[j] != "-c" {
            continue;
        }
        let Some((name, body)) = pre[j + 1].split_once('=') else {
            continue;
        };
        if name.to_lowercase().starts_with("alias.") {
            let alias: String = name
                .chars()
                .skip("alias.".len())
                .collect::<String>()
                .to_lowercase();
            match out.iter_mut().find(|(k, _)| *k == alias) {
                Some(row) => row.1 = body.to_string(),
                None => out.push((alias, body.to_string())),
            }
        }
    }
    out
}

/// Follow git ALIASES to the subcommand that will really run — `_resolve_git_alias`.
///
/// Asked of git, for this repository, and only for a subcommand not already taken to be git's
/// own. The caller's arguments are appended to the expansion, exactly as git appends them, so
/// `sw = switch -c` plus `git sw neu` is judged as `switch -c neu`. A `!git …` body is read as
/// the git command it is; any other `!` body is a shell charter does not read, and stands aside.
/// An alias git would not answer for is "not an alias" — out of scope, not allowed.
pub fn resolve_git_alias(
    root: &str,
    sub: &str,
    post: &[String],
    pre: &[String],
) -> (String, Vec<String>) {
    let inline = inline_aliases(pre);
    let mut sub = sub.to_string();
    let mut post = post.to_vec();
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..MAX_ALIAS_HOPS {
        if BRANCH_MOVERS.contains(&sub.as_str())
            || GIT_KNOWN_SUBCOMMANDS.contains(&sub.as_str())
            || seen.contains(&sub)
        {
            return (sub, post);
        }
        seen.push(sub.clone());
        let folded = sub.to_lowercase();
        let body = match inline.iter().find(|(k, _)| *k == folded) {
            Some((_, b)) => b.clone(),
            None => {
                let key = format!("alias.{sub}");
                match git_in(root, &["config", "--get", &key]) {
                    Ok(a) if a.ok => py_strip(&a.out).to_string(),
                    Ok(_) => String::new(),
                    Err(_) => return (sub, post),
                }
            }
        };
        if body.is_empty() {
            return (sub, post);
        }
        let shell = body.starts_with('!');
        let text = if shell { &body[1..] } else { &body[..] };
        // A `!` alias is run by `sh -c`, so it is read as the shell reads it.
        let split = if shell {
            shellseg::shell_split(text)
        } else {
            shellseg::posix_split(text)
        };
        let Ok(mut toks) = split else {
            return (sub, post);
        };
        if shell {
            if toks.first().map(|t| shellwrap::base_lower(t)).as_deref() != Some("git") {
                return (sub, post);
            }
            toks.remove(0);
        }
        let Some((first, tail)) = toks.split_first() else {
            return (sub, post);
        };
        let mut next = tail.to_vec();
        next.extend(post);
        sub = first.clone();
        post = next;
    }
    (sub, post)
}

/// The name a branch-creating `checkout`/`switch` would create, for the DENIAL TEXT —
/// `_created_branch`. The name can be inside the option token (`-bREADME`, `--orphan=README`),
/// where the operand list is empty.
pub fn created_branch(opts: &[String], classes: &[OptKind], wants: &[String]) -> Option<String> {
    let first_want = || wants.first().cloned();
    let Some(first) = opts
        .iter()
        .zip(classes)
        .find(|(_, c)| **c == OptKind::Create)
        .map(|(o, _)| o)
    else {
        return first_want();
    };
    if first.starts_with("--") {
        if let Some((_, value)) = first.split_once('=') {
            return if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
    } else {
        let chars: Vec<char> = first.chars().collect();
        for (i, ch) in chars.iter().enumerate().skip(1) {
            if BRANCH_CREATOR_OPTS.contains(&format!("-{ch}").as_str()) {
                let attached: String = chars[i + 1..].iter().collect();
                return if attached.is_empty() {
                    first_want()
                } else {
                    Some(attached)
                };
            }
        }
    }
    first_want()
}

/// This repository's default branch, or `None` — `doctor._plane_default_branch`: the remote's
/// own `origin/HEAD` first, then a local `main` or `master`. `Err` is a question that could not
/// be asked, which the caller treats as no default.
fn plane_default_branch(root: &str) -> Result<Option<String>, Unasked> {
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
    if head.ok && !remote_head.is_empty() {
        // `--short` renders it as `origin/main`, not `main`.
        return Ok(Some(match remote_head.strip_prefix("origin/") {
            Some(rest) => rest.to_string(),
            None => remote_head.to_string(),
        }));
    }
    for guess in ["main", "master"] {
        let refname = format!("refs/heads/{guess}");
        if git_in(root, &["rev-parse", "--verify", "--quiet", &refname])?.ok {
            return Ok(Some(guess.to_string()));
        }
    }
    Ok(None)
}

/// [`plane_default_branch`] as the branch guard reads it: a question that could not be asked is
/// no default. Python catches `ProcTimeout`/`OSError` there and RAISES on a name that is not
/// UTF-8 (charter#1178); both are no default here, which withholds the remedy's carve-out — the
/// refusing direction.
pub fn default_branch(root: &str) -> Option<String> {
    plane_default_branch(root).unwrap_or(None)
}

/// Deny a git command that would move the PLANE ROOT between branches (#157) —
/// `_plane_root_branch_reason`. `root` is the plane root as charter's config holds it.
///
/// Four things keep it a guard rather than a cage: only branch moves; only the root; the
/// documented remedy (`git checkout <default>`, leaving HEAD ATTACHED to the default branch)
/// stays runnable; and a file restore is not a branch move (#461) — resolved by asking git, and
/// only an affirmative "a tracked path and not a revision", with every option one charter can
/// place as restore-only, opens the gate.
pub fn plane_root_branch_reason(cmd: &str, cwd: &str, root: &str) -> Option<String> {
    let root = plane_root(root);
    for inv in plane_root_git(cmd, cwd, &root) {
        let (mut sub, mut post) = (inv.sub.clone(), inv.post.clone());
        if !BRANCH_MOVERS.contains(&sub.as_str()) {
            // `co = checkout` moves the root's HEAD exactly as far (#461, round two).
            (sub, post) = resolve_git_alias(&root, &sub, &post, &inv.pre);
            if !BRANCH_MOVERS.contains(&sub.as_str()) {
                continue;
            }
        }
        // Every option classified before anything reads an operand: what an operand MEANS
        // depends on the options beside it.
        let opts: Vec<String> = post
            .iter()
            .filter(|a| a.starts_with('-') && *a != "-" && *a != "--")
            .cloned()
            .collect();
        let classes: Vec<OptKind> = opts.iter().map(|o| checkout_opt_kind(o)).collect();
        let creating = classes.contains(&OptKind::Create);
        let detaching = classes.contains(&OptKind::Detach);
        let restore_shaped = classes.iter().all(|c| *c == OptKind::Restore);
        let unplaced = opts
            .iter()
            .zip(&classes)
            .find(|(_, c)| **c == OptKind::Unknown)
            .map(|(o, _)| o.clone());

        // What FOLLOWS `--` makes a path form, not its presence; and `git switch` has no path
        // half for a `--` to introduce, so only `checkout` reads it this way.
        let separator = post.iter().position(|a| a == "--");
        if let (true, Some(cut)) = (sub == "checkout", separator) {
            if cut + 1 < post.len() {
                if restore_shaped {
                    continue; // paths follow: a restore, HEAD does not move
                }
            } else {
                post.truncate(cut); // a trailing bare `--` still switches
            }
        }
        // A bare `-` is a REF (the previous branch), not a flag.
        let wants: Vec<String> = post
            .iter()
            .filter(|a| *a == "-" || !a.starts_with('-'))
            .cloned()
            .collect();
        if wants.is_empty() && !(creating || detaching) && restore_shaped {
            continue; // bare `git checkout` moves nothing
        }

        let default = default_branch(&root);
        // The documented remedy stays runnable — and what makes a command the remedy is that it
        // leaves HEAD ATTACHED to the default branch, not that the name is beside it.
        if restore_shaped
            && default
                .as_deref()
                .is_some_and(|d| wants.first().map(String::as_str) == Some(d))
        {
            continue;
        }

        let mut kind = OperandKind::Rev;
        if let (true, Some(first)) = (sub == "checkout" && restore_shaped, wants.first()) {
            kind = checkout_operand_kind(&root, first);
            let restore = if wants.len() == 1 {
                kind == OperandKind::Path
            } else {
                // Every trailing operand RESOLVED, not assumed a path: the tokeniser flattens
                // `git checkout $(echo feature)` into several tokens.
                let rest = &wants[1..];
                rest.len() <= MAX_CHECKOUT_OPERANDS
                    && rest
                        .iter()
                        .all(|w| checkout_operand_kind(&root, w) == OperandKind::Path)
            };
            if restore {
                continue; // `git restore <path>`, spelled the old way
            }
        }

        let want0 = wants.first().cloned().unwrap_or_default();
        let opening = match kind {
            OperandKind::Both => format!(
                "cannot tell what `git checkout {want0}` does in the PLANE ROOT — it is \
                 AMBIGUOUS: '{want0}' is both a tracked path here and a name git \
                 resolves to a commit, so it could be a file restore or a ref move, and \
                 git breaks that tie in favour of the REF — this would switch the root. \
                 Say which you meant and it runs: `git restore {want0}` (or \
                 `git checkout -- {want0}`) restores the file, and charter allows that \
                 here in either spelling. "
            ),
            OperandKind::Neither => format!(
                "would move HEAD in the PLANE ROOT: '{want0}' is not a path this tree \
                 tracks, so `git checkout` reads it as a revision — and a branch of that \
                 name on a remote is checked out here as a new local branch. (Meant the \
                 file? `git restore {want0}` is allowed, and would tell you git has \
                 never heard of that path either.) "
            ),
            OperandKind::Unknown => format!(
                "would move HEAD in the PLANE ROOT — charter could not ask git whether \
                 '{want0}' is a path or a revision here, and a guard that opened \
                 because it failed to ask is no guard. "
            ),
            _ => match &unplaced {
                Some(unplaced) if !creating && !detaching => {
                    let hint = if wants.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "(A restore needs no options here: `git restore {want0}` and \
                             `git checkout -- {want0}` are both allowed.) "
                        )
                    };
                    format!(
                        "cannot read this `git {sub}` as a file restore in the PLANE ROOT: it does \
                         not recognise the option '{unplaced}', and an option charter cannot place \
                         is one that may move HEAD — `git checkout --orphan <file>` reads exactly \
                         like a restore and creates a branch. Only a form charter can show is a \
                         restore opens that gate. {hint}"
                    )
                }
                _ => {
                    let created = if creating {
                        created_branch(&opts, &classes, &wants).filter(|c| !c.is_empty())
                    } else {
                        None
                    };
                    let moving = if let Some(created) = created {
                        format!("create '{created}'")
                    } else if creating {
                        "create a branch".to_string()
                    } else if detaching && !wants.is_empty() {
                        format!("detach HEAD at '{want0}'")
                    } else if detaching {
                        "detach HEAD".to_string()
                    } else if !wants.is_empty() {
                        format!("switch to '{want0}'")
                    } else {
                        "switch branches".to_string()
                    };
                    format!("would {moving} in the PLANE ROOT. ")
                }
            },
        };
        let back = match default.as_deref().filter(|d| !d.is_empty()) {
            Some(d) => format!(
                "`git checkout {d}` — putting the root back on its default branch — is always \
                 allowed."
            ),
            None => "Putting the root back on its default branch is always allowed.".to_string(),
        };
        return Some(format!(
            "{opening}The plane root is one working tree every session \
             shares — two agents here silently clobber each other's branches, and the \
             symptom looks like an unrelated bug. Branch work belongs in a workspace \
             clone: `charter workspace create <task>`, then `charter clone <repo>`. \
             {back}"
        ));
    }
    None
}

/// `(commits destroyed, upstream ref)` if resetting `root` to `target` would take commits off the
/// branch that exist NOWHERE ELSE — `_unpushed_at_risk`.
///
/// One question: `git rev-list --count HEAD --not <target> @{upstream} --`. `git reset --hard
/// HEAD` counts 0; a synced root counts 0; a path operand fails the whole call (the `--` marks
/// everything before it as revisions, and `@{upstream}` cannot be a path). `None` on any
/// non-zero exit — also the honest answer for a root with no tracking branch — and on any
/// question that could not be asked. Never panics.
pub fn unpushed_at_risk(root: &str, target: &str) -> Option<(u64, String)> {
    let r = git_in(
        root,
        &[
            "rev-list",
            "--count",
            "HEAD",
            "--not",
            target,
            "@{upstream}",
            "--",
        ],
    )
    .ok()?;
    if !r.ok {
        return None;
    }
    // `int(r.stdout.strip() or 0)` — a count git did not write is `ValueError`, i.e. `None`.
    let text = py_strip(&r.out);
    let n: i128 = if text.is_empty() { 0 } else { py_int(text)? };
    if n <= 0 {
        return None;
    }
    let up = git_in(root, &["rev-parse", "--abbrev-ref", "@{upstream}"]).ok()?;
    let name = if up.ok {
        py_strip(&up.out).to_string()
    } else {
        String::new()
    };
    let name = if name.is_empty() {
        "its upstream".to_string()
    } else {
        name
    };
    Some((u64::try_from(n).unwrap_or(u64::MAX), name))
}

/// `int(text)` for the digits `git rev-list --count` writes: an optional sign, then ASCII digits
/// with `_` allowed BETWEEN them, as Python's `int` reads a string. Anything else is `None` — the
/// `ValueError` the caller turns into "nothing at risk".
fn py_int(text: &str) -> Option<i128> {
    let (neg, digits) = match text.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    let n: i128 = digits.replace('_', "").parse().ok()?;
    Some(if neg { -n } else { n })
}

/// Deny a `git reset` that would DESTROY unpushed commits in the plane root (#401) —
/// `_plane_root_reset_reason`. `root` is the plane root as charter's config holds it.
///
/// Only the modes that overwrite the tree ([`RESET_TREE_MODES`]); only a reset that measurably
/// drops something unpublished ([`unpushed_at_risk`] is the whole condition); and the denial
/// clears itself the moment `charter save` pushes the commits. It follows ALIASES as the branch
/// guard does (#467), so its hot-path filter is `git`, never `reset`: `wipe = reset --hard`
/// destroys commits without the word. The filter folds and reads through quoting
/// ([`shellwrap::may_name`]), because the walk behind it does: `GIT` and `g''it` are git.
pub fn plane_root_reset_reason(cmd: &str, cwd: &str, root: &str) -> Option<String> {
    if !shellwrap::may_name(cmd, "git") {
        return None;
    }
    let root = plane_root(root);
    for inv in plane_root_git(cmd, cwd, &root) {
        let (mut sub, mut post) = (inv.sub.clone(), inv.post.clone());
        if sub != "reset" {
            (sub, post) = resolve_git_alias(&root, &sub, &post, &inv.pre);
            if sub != "reset" {
                continue;
            }
        }
        if !post.iter().any(|a| RESET_TREE_MODES.contains(&a.as_str())) {
            continue;
        }
        // What FOLLOWS `--` makes it a path form (`git reset <ref> -- <paths>` moves no HEAD).
        if let Some(cut) = post.iter().position(|a| a == "--") {
            if cut + 1 < post.len() {
                continue;
            }
            post.truncate(cut);
        }
        let Some(target) = post.iter().find(|a| !a.starts_with('-')) else {
            continue; // `git reset --hard` with no ref moves HEAD nowhere: no commit dies
        };
        let Some((n, upstream)) = unpushed_at_risk(&root, target) else {
            continue;
        };
        let (commits, are) = if n == 1 {
            ("commit", "is")
        } else {
            ("commits", "are")
        };
        return Some(format!(
            "would delete {n} {commits} from the PLANE ROOT that {are} \
             not on {upstream}, and this reset overwrites the working tree — their content \
             leaves the disk with the reflog as the only copy. An unpushed commit here is \
             usually a memory commit whose push a protected branch refused, which is how \
             eleven of them were lost. See exactly what would go: \
             `git -C {root} log --oneline '@{{upstream}}..HEAD'`. Keep it: `charter save` \
             pushes it, and this reset stops being refused the moment it lands."
        ));
    }
    None
}

#[cfg(test)]
mod tests;
