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
//! * **#1323** — a directory git is sent to (`-C`, `--git-dir`, `--work-tree`, `GIT_DIR`,
//!   `GIT_WORK_TREE`, a wrapper's chdir flag) is read the way the shell hands it over: `~` is
//!   `$HOME`, and a word the guard cannot name (`$VAR`, a glob, `~user`) may be the root
//!   ([`invocation_readings`]), refused with a sentence of its own. The Python joined the word
//!   to the cwd as written. And a subject that IS the root's git directory reaches the root,
//!   wherever that directory lives.

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
    /// It reaches the root only through a directory the guard cannot name before the shell runs
    /// (`cd "$X"`, `git -C "$X"`, a glob, `cd -`): it MAY act on the root, so it is refused, and
    /// the denial says that rather than claiming it does (#1323).
    pub unnamed: bool,
    /// Its options held a substitution, and which word after it git runs could not be worked
    /// out — git could not be asked about its aliases, or there were more alias words than the
    /// line's budget: it is refused (#1354, as #438 rules for an operand git could not be asked
    /// about).
    pub unread: bool,
    /// The environment it may run with as the line sets it — earlier bare assignments and
    /// exports, then its own assignments — which can define aliases (#1358).
    pub env: Vec<String>,
    /// Its own assignments alone: what reaches git whatever an earlier segment's fate (`unset`,
    /// `env -u`, a subshell, `false &&`). An alias the two disagree on is unread ([`LineConfig`]),
    /// so a line value can only add refusals.
    pub own_env: Vec<String>,
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
        // The root's git directory itself, wherever it lives: `--git-dir=<it>` moves the root's
        // HEAD as surely as standing in the root does.
        if same_dir(&t, root_gd, *root_gd_id) {
            return true;
        }
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
    /// The line quotes or escapes a `~` (`'~`, `"~`, `\~`, or a quote right after one), so a
    /// separated `~` word the reader hands over unquoted may be one the shell leaves alone: a
    /// directory named `~`, not `$HOME` (#1323). Read from the text, before anything runs, so a
    /// `~` the same line creates is covered too.
    quotes_tilde: bool,
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
            // `$'…'` too: ANSI-C quoting spells a `~` as `$'\x7e'` or `$'\176'`, which the
            // reader decodes and the raw text never shows.
            quotes_tilde: ["'~", "\"~", "\\~", "~'", "~\"", "~\\", "$'"]
                .iter()
                .any(|quoted| cmd.contains(quoted)),
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
    if dest == "~" || dest.starts_with("~/") {
        if line.sets_home {
            return anywhere();
        }
        let Some(home) = std::env::var("HOME").ok().filter(|h| is_abs(h)) else {
            return anywhere();
        };
        let mut out = cd_to(format!("{home}{}", &dest[1..]), here, line);
        // A quoted or escaped `~` stays a directory named `~`, and the reader has taken the
        // quoting off: on a line that shows one, both are where the shell may go (#1323).
        if line.quotes_tilde {
            out.merge(cd_to(dest, here, line));
        }
        return out;
    }
    if dest.starts_with('~') {
        return anywhere();
    }
    cd_to(dest, here, line)
}

/// Where a `cd` to the named `dest` sends the shell from `here` — [`cd_destinations`] once the
/// word is read: `CDPATH`, and both readings of a `..`.
fn cd_to(dest: String, here: &Whereabouts, line: &CdContext) -> Whereabouts {
    if !is_abs(&dest) {
        let walks_cdpath =
            !(dest == "." || dest == ".." || dest.starts_with("./") || dest.starts_with("../"));
        if walks_cdpath && line.cdpath {
            return Whereabouts {
                dirs: Vec::new(),
                anywhere: true,
            };
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

/// The most `~` words one invocation's directories are read both ways for. Each doubles the
/// readings; past this many the invocation is one the guard cannot name.
const MAX_TILDE_WORDS: usize = 4;

/// How the shell hands git one directory word: the readings it may become, or `None` when the
/// guard cannot name it (#1323).
///
/// `None` is a word the shell expands later — `$`, a command substitution, a glob, a brace — or a
/// `~user`, or a `~` when the line may set `HOME` or `HOME` is not absolute. These are the
/// destinations [`cd_destinations`] cannot name either. A `~` or `~/…` is `$HOME`, as the shell
/// expands it unquoted in a word of its own. The reader has taken the quoting off, though, so a
/// `'~'` or `\~` arrives looking the same and stays a directory named `~`: `literal` keeps that
/// reading as well, and the caller sets it wherever the shell may leave the `~` alone.
fn directory_readings(word: &str, literal: bool, line: &CdContext) -> Option<Vec<String>> {
    if word.contains(['$', '`', '*', '?', '[', '{']) {
        return None;
    }
    if !word.starts_with('~') {
        return Some(vec![word.to_string()]);
    }
    if !(word == "~" || word.starts_with("~/")) || line.sets_home {
        return None;
    }
    let home = std::env::var("HOME").ok().filter(|h| is_abs(h))?;
    let expanded = format!("{home}{}", &word[1..]);
    Some(if literal {
        vec![word.to_string(), expanded]
    } else {
        vec![expanded]
    })
}

/// One reading of an invocation's directory words: git's own options, its environment, and a
/// wrapper's chdir flag, as [`git_target`] and the walk take them.
type Reading = (Vec<String>, Vec<String>, String);

/// Every way one invocation's directory words may reach git — `(pre, env, chdir)` with each word
/// replaced by one of its [`directory_readings`] — or `None` when one of them cannot be named.
///
/// The words are git's own `-C`, `--git-dir` and `--work-tree` (separated or attached), the
/// `GIT_DIR`/`GIT_WORK_TREE` it inherits, and a wrapper's chdir flag. Nothing else in `pre` or
/// `env` is read: a `$` in `-c x.y=$V` names no directory.
fn invocation_readings(
    pre: &[String],
    env: &[String],
    chdir: &str,
    chdir_separated: bool,
    line: &CdContext,
) -> Option<Vec<Reading>> {
    // A separated word's `~` stays literal only when quoted or escaped, which the reader cannot
    // see in the word but the line's text shows ([`CdContext::quotes_tilde`]). An attached `--git-dir=~`, an
    // assignment's value (zsh leaves `GIT_DIR=~/x` alone after `env`) and an attached wrapper
    // chdir flag (`env --chdir=~` and `env -C~` leave it alone everywhere) keep both always.
    // Every word that names a directory: where it is, and its readings.
    enum At {
        Pre(usize, &'static str),
        Env(usize, &'static str),
        Chdir,
    }
    let mut words: Vec<(At, Vec<String>)> = Vec::new();
    let mut i = 0usize;
    while i < pre.len() {
        let tok = pre[i].as_str();
        if matches!(tok, "-C" | "--git-dir" | "--work-tree") {
            if let Some(val) = pre.get(i + 1) {
                let literal = line.quotes_tilde;
                words.push((At::Pre(i + 1, ""), directory_readings(val, literal, line)?));
            }
            i += 2;
            continue;
        }
        for prefix in ["--git-dir=", "--work-tree="] {
            if let Some(val) = tok.strip_prefix(prefix) {
                words.push((At::Pre(i, prefix), directory_readings(val, true, line)?));
            }
        }
        i += if GIT_VALUE_OPTS.contains(&tok) { 2 } else { 1 };
    }
    for (j, assign) in env.iter().enumerate() {
        for prefix in ["GIT_DIR=", "GIT_WORK_TREE="] {
            if let Some(val) = assign.strip_prefix(prefix) {
                words.push((At::Env(j, prefix), directory_readings(val, true, line)?));
            }
        }
    }
    if !chdir.is_empty() {
        let literal = !chdir_separated || line.quotes_tilde;
        words.push((At::Chdir, directory_readings(chdir, literal, line)?));
    }
    if words.iter().filter(|(_, r)| r.len() > 1).count() > MAX_TILDE_WORDS {
        return None;
    }
    let mut out = vec![(pre.to_vec(), env.to_vec(), chdir.to_string())];
    for (at, readings) in &words {
        let mut next = Vec::new();
        for (p, e, c) in &out {
            for reading in readings {
                let (mut p, mut e, mut c) = (p.clone(), e.clone(), c.clone());
                match at {
                    At::Pre(k, prefix) => p[*k] = format!("{prefix}{reading}"),
                    At::Env(k, prefix) => e[*k] = format!("{prefix}{reading}"),
                    At::Chdir => c = reading.clone(),
                }
                next.push((p, e, c));
            }
        }
        out = next;
    }
    Some(out)
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
///   be in, and under every reading of the directory words git is sent to; a word the guard
///   cannot name puts the invocation on the root (#1323).
pub fn plane_root_git(cmd: &str, cwd: &str, root: &str) -> Vec<RootInvocation> {
    let Some(segments) = shellseg::joined_argv(cmd) else {
        return Vec::new();
    };
    let argvs: Vec<Vec<String>> = segments.iter().map(|s| s.argv.clone()).collect();
    let carried = shellwrap::exported_env_shared(&argvs);
    // Stat-ed on the first git invocation, so a line with none pays nothing for it.
    let the_root = std::cell::OnceCell::new();
    let mut out = Vec::new();
    let mut here = Whereabouts::at(cwd);
    // Where the shell stays if a `cd … &&` failed: back in play once the `&&` chain ends.
    let mut left_behind = Whereabouts::default();
    let context = CdContext::of(cmd);
    // The root's configured aliases, read once for the whole line and only if it needs them.
    let aliases = AliasBook::of(root);
    // Every assignment a bare assignment segment (`A=1;`) has made so far: each distinct one
    // once, in order, so a line repeating one stays short and every value a variable was given
    // is still there to be read (D-1358f).
    let mut assigned: Vec<String> = Vec::new();
    let mut assigned_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, seg) in segments.iter().enumerate() {
        let before = carried.before(i);
        let call = shellwrap::split_env_chdir(&seg.argv);
        let (prog, env, args, chdir) = (call.prog, call.env, call.argv, call.chdir);
        // The segment's own assignments: the only line-set configuration git surely sees (#1358).
        let own_env = env;
        // `before + env`, built only for a segment that may be git: copying every earlier
        // export into every segment made a line of exports cost its square.
        let inherited = || {
            let mut out = before.to_vec();
            out.extend(own_env.iter().cloned());
            out
        };
        // What may set git's configuration for this segment: every bare assignment the line
        // made before it, exported or not — HOME and the session's own GIT_CONFIG_* are already
        // exported, so a bare `HOME=/x;` reaches git — then the exports and the segment's own.
        // Built only for a segment that may be git, so a long line of assignments costs what it
        // is long.
        let config_env = || {
            let mut out: Vec<String> = assigned.clone();
            out.extend(inherited());
            out
        };
        if let Some(cd) = cd_call(&seg.argv, &prog, &args) {
            let moved = cd_destinations(&cd.builtin, &cd.args, &here, &context);
            let piped = matches!(seg.before.as_deref(), Some("|" | "|&"));
            if cd.certain && !context.redefined && !piped && seg.after.as_deref() == Some("&&") {
                left_behind.merge(std::mem::replace(&mut here, moved));
            } else {
                here.merge(moved);
            }
        } else if prog.contains('$') {
            // The program is a word only the shell can name (`$G checkout -b x` after `G=git`,
            // `$(which git) …`): it may be git, sent anywhere (#1354). Answered for when a
            // branch move or reset follows it.
            if let Some((sub, post)) = literal_mover_after(args.get(1..).unwrap_or(&[])) {
                out.push(RootInvocation {
                    sub,
                    post,
                    pre: Vec::new(),
                    unnamed: true,
                    unread: false,
                    env: config_env(),
                    own_env: own_env.clone(),
                });
            }
        } else if let Some(at) = spliced_before_program(&seg.argv, &prog) {
            // The program itself is lost in a substitution's words (#1354): `GIT_DIR=$(…) git …`
            // and `env -C $(…) git …` read as running the substitution's first word. A git
            // among what follows may be the one that runs, sent where the guard cannot name.
            let (pre, rest) = shellwrap::git_globals(&seg.argv[at..]);
            let env = config_env();
            if let Some((sub, post, unread)) =
                subcommand_after_splice(&pre, &rest, &env, &own_env, &aliases)
            {
                out.push(RootInvocation {
                    sub,
                    post,
                    pre,
                    unnamed: true,
                    unread,
                    env,
                    own_env: own_env.clone(),
                });
            }
        } else if shellwrap::base_lower(&prog) == "git" {
            // Folded: on APFS and NTFS `GIT` runs git (#346).
            let (pre, rest) = shellwrap::git_globals(&args);
            // A substitution among git's own options (#1354): its words, and every option after
            // it, are spread through what follows, so neither the directory nor the subcommand
            // can be read from their places.
            let spliced = pre.iter().any(|t| t.ends_with('$'));
            let env = config_env();
            let words = if spliced {
                subcommand_after_splice(&pre, &rest, &env, &own_env, &aliases)
            } else {
                rest.split_first()
                    .map(|(sub, post)| (sub.clone(), post.to_vec(), false))
            };
            if let Some((sub, post, unread)) = words {
                // `before + env`, in that order: an assignment on THIS invocation overrides an
                // export.
                // `env -C ~/x` hands the shell a word of its own; `env --chdir=~/x` does not.
                let chdir_separated = seg
                    .argv
                    .windows(2)
                    .any(|w| matches!(w[0].as_str(), "-C" | "-D" | "--chdir") && w[1] == chdir);
                let inherited = inherited();
                let readings =
                    invocation_readings(&pre, &inherited, &chdir, chdir_separated, &context);
                let named = readings.as_ref().is_some_and(|readings| {
                    readings.iter().any(|(pre, inherited, chdir)| {
                        here.dirs.iter().any(|dir| {
                            let dir = if chdir.is_empty() {
                                dir.clone()
                            } else {
                                join_dir(dir, chdir) // `env -C`, `sudo --chdir`
                            };
                            git_target(&dir, pre, inherited).iter().any(|t| {
                                the_root
                                    .get_or_init(|| TheRoot::of(root))
                                    .is_reached_from(t)
                            })
                        })
                    })
                });
                // Fail closed: a directory the guard cannot name may be the root (#1323).
                let unnamed = spliced || (!named && (here.anywhere || readings.is_none()));
                if named || unnamed {
                    out.push(RootInvocation {
                        sub,
                        post,
                        pre,
                        unnamed,
                        unread,
                        env,
                        own_env: own_env.clone(),
                    });
                }
            }
        }
        if !matches!(seg.after.as_deref(), Some("&&" | "|" | "|&")) {
            here.merge(std::mem::take(&mut left_behind));
        }
        if !seg.argv.is_empty() && seg.argv.iter().all(|t| shellwrap::is_env_assignment(t)) {
            for assign in &seg.argv {
                if assigned_seen.insert(assign.clone()) {
                    assigned.push(assign.clone());
                }
            }
        }
    }
    out
}

/// Where a git invocation may begin in a segment whose PROGRAM was taken from a substitution's
/// words, or `None`.
///
/// The reader spreads an unquoted `$(…)` or backtick substitution through the argv it stands in:
/// the word it began in ends in `$`, then come its own words, then the rest (#1354). When that
/// word stands before the program — an assignment, a wrapper's option — the program read is the
/// substitution's first word; the first `git` after it is answered for instead.
fn spliced_before_program(argv: &[String], prog: &str) -> Option<usize> {
    let mark = argv.iter().position(|t| t.ends_with('$'))?;
    let prog_at = argv.iter().position(|t| t == prog)?;
    if mark >= prog_at || shellwrap::base_lower(prog) == "git" {
        return None;
    }
    argv.iter()
        .skip(mark + 1)
        .position(|t| shellwrap::base_lower(t) == "git")
        .map(|i| mark + 1 + i)
}

/// The subcommand of a git invocation whose options held a substitution, its arguments, and
/// whether it could not be read (#1354).
///
/// Which word git really sees first cannot be read, so the guard looks for the words it judges:
/// the first literal branch mover or `reset` among the words that follow; else the first word
/// named as an alias — inline (`-c alias.co=checkout`, before the word) or in the root's config
/// — that resolves to one, taken as what it resolves to; else nothing, since no word there is
/// one the guards judge. Where git could not be asked about its aliases, or more than
/// [`MAX_CHECKOUT_OPERANDS`] different aliases would have to be followed, the answer is UNREAD, and
/// refused. Every alias is read from the line's one [`AliasBook`], so the cost stays flat.
fn subcommand_after_splice(
    pre: &[String],
    rest: &[String],
    env: &[String],
    own_env: &[String],
    aliases: &AliasBook<'_>,
) -> Option<(String, Vec<String>, bool)> {
    if let Some((sub, post)) = literal_mover_after(rest) {
        return Some((sub, post, false));
    }
    let first = || {
        rest.split_first()
            .map(|(sub, post)| (sub.clone(), post.to_vec()))
    };
    let moves = |sub: &str| BRANCH_MOVERS.contains(&sub) || sub == "reset";
    // The aliases the line itself defines, kept as the walk goes: a `-c alias.x=…` or
    // `--config-env` pair joins them once both its words are behind (built once rather than
    // per word), over what the environment defines (#1358).
    let mut inline = LineConfig::of(pre, env, own_env);
    let mut prev: Option<&String> = pre.last();
    // What an alias is now: the line's body first, then the config's.
    let body_now = |inline: &LineConfig, name: &str| -> Result<String, AliasEnd> {
        let folded = name.to_lowercase();
        match inline.body(&folded) {
            Some(found) => found,
            None => aliases.body(&folded).ok_or(AliasEnd::Unasked),
        }
    };
    // Every alias chain already followed to no branch move, as the bodies it read: a word
    // whose chain reads the same bodies now answers the same again, so `st st st …` costs one
    // following, and redefining any alias on the chain makes it a new one.
    let mut cleared: Vec<Vec<(String, String)>> = Vec::new();
    for (k, word) in rest.iter().enumerate() {
        let folded = word.to_lowercase();
        let named = inline.names(&folded, word) || {
            let Some(book) = aliases.bodies() else {
                let (sub, post) = first().unwrap_or_default();
                return Some((sub, post, true));
            };
            book.iter().any(|(name, _)| *name == folded)
        };
        let known = || {
            cleared.iter().any(|chain| {
                chain.first().is_some_and(|(n, _)| *n == folded)
                    && chain
                        .iter()
                        .all(|(n, b)| body_now(&inline, n).as_ref() == Ok(b))
            })
        };
        if named && !known() {
            if cleared.len() >= MAX_CHECKOUT_OPERANDS {
                // Past the budget the rest is unread, never allowed.
                let (sub, post) = first().unwrap_or_default();
                return Some((sub, post, true));
            }
            let mut chain: Vec<(String, String)> = Vec::new();
            let (sub, post, end) = expand_alias(word, &rest[k + 1..], &mut |name| {
                let body = body_now(&inline, name);
                if let Ok(b) = &body {
                    chain.push((name.to_lowercase(), b.clone()));
                }
                body
            });
            if end != AliasEnd::Resolved {
                return Some((sub, post, true));
            }
            if moves(&sub) {
                return Some((sub, post, false));
            }
            cleared.push(chain);
        }
        inline.read_option(prev.map(String::as_str), word, own_env, env);
        prev = Some(word);
    }
    // No word after the substitution is one the guards judge, or an alias of one.
    None
}

/// The root's configured aliases for one command line: every `alias.*` read with ONE git
/// question, the first time a line needs one, so a line that names an alias a thousand times
/// asks git once (#1354). `None` inside is a question that could not be put.
struct AliasBook<'a> {
    root: &'a str,
    bodies: std::cell::OnceCell<Option<Vec<(String, String)>>>,
}

impl<'a> AliasBook<'a> {
    fn of(root: &'a str) -> Self {
        AliasBook {
            root,
            bodies: std::cell::OnceCell::new(),
        }
    }

    /// Every alias as `(name, body)`, names folded as git keys them, or `None` when git could
    /// not be asked.
    fn bodies(&self) -> Option<&[(String, String)]> {
        self.bodies
            .get_or_init(|| configured_aliases(self.root))
            .as_deref()
    }

    /// The body `git config --get alias.<name>` would answer — the last one set, `""` for none
    /// — or `None` when git could not be asked.
    fn body(&self, name: &str) -> Option<String> {
        let folded = name.to_lowercase();
        let book = self.bodies()?;
        Some(
            book.iter()
                .rev()
                .find(|(k, _)| *k == folded)
                .map(|(_, b)| py_strip(b).to_string())
                .unwrap_or_default(),
        )
    }
}

/// Every `alias.*` in the root's config, in order, as `(folded name, body)`: `git config -z
/// --get-regexp`, whose NUL-separated records keep a body's own newlines. `None` when git could
/// not be asked; a config with no alias is an empty list.
fn configured_aliases(root: &str) -> Option<Vec<(String, String)>> {
    let a = git_in(root, &["config", "-z", "--get-regexp", r"^alias\."]).ok()?;
    if !a.ok {
        return Some(Vec::new()); // exit 1: no key matched
    }
    Some(
        a.out
            .split('\0')
            .filter_map(|record| {
                let (key, body) = record.split_once('\n').unwrap_or((record, ""));
                let name = key.strip_prefix("alias.")?;
                Some((name.to_lowercase(), body.to_string()))
            })
            .collect(),
    )
}

/// The first literal branch mover or `reset` in `words`, with the words after it.
fn literal_mover_after(words: &[String]) -> Option<(String, Vec<String>)> {
    let at = words
        .iter()
        .position(|t| BRANCH_MOVERS.contains(&t.as_str()) || t == "reset")?;
    Some((words[at].clone(), words[at + 1..].to_vec()))
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
    let (sub, post, _) = resolve_git_alias_to_its_end(root, sub, post, pre, &[], &[]);
    (sub, post)
}

/// [`resolve_git_alias`], and how the walk ended: a chain still on an alias when
/// [`MAX_ALIAS_HOPS`] ran out is [`AliasEnd::TooDeep`], which the branch guard refuses — the
/// end it cannot see may move HEAD.
///
/// `env` is the invocation's environment as the line sets it ([`RootInvocation::env`]) and
/// `own_env` its own assignments ([`RootInvocation::own_env`]): an alias they define, or one a
/// source they name could define, is read from them first (#1358, [`LineConfig`]).
pub fn resolve_git_alias_to_its_end(
    root: &str,
    sub: &str,
    post: &[String],
    pre: &[String],
    env: &[String],
    own_env: &[String],
) -> (String, Vec<String>, AliasEnd) {
    let line = LineConfig::of(pre, env, own_env);
    expand_alias(sub, post, &mut |name| {
        if let Some(found) = line.body(&name.to_lowercase()) {
            return found;
        }
        let key = format!("alias.{name}");
        match git_in(root, &["config", "--get", &key]) {
            Ok(a) if a.ok => Ok(py_strip(&a.out).to_string()),
            Ok(_) => Ok(String::new()),
            Err(_) => Err(AliasEnd::Unasked),
        }
    })
}

/// The git configuration one invocation takes from its own command line and environment, as
/// far as aliases go (#1358): `-c`, `--config-env`, `GIT_CONFIG_COUNT` with its numbered pairs,
/// and `GIT_CONFIG_PARAMETERS`, each as the line sets it — the config FILES are git's to read,
/// and the guard asks git about them.
///
/// A source the guard does not read makes the line OPAQUE: any alias may be defined there. That
/// is a config file the line points git at (`GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`,
/// `GIT_CONFIG`, `HOME`, `XDG_CONFIG_HOME`, an `include.path` or `includeIf.*.path`) — never
/// opened, because the same line may write it before git reads it — and a key, count or list
/// only the shell can name. An alias whose VALUE only the shell can name is unread on its own.
///
/// **Only the invocation's own assignments and git's own options surely reach git** (D-1358f).
/// A value an earlier segment sets may not — `unset`, `env -u`, `env -i`, a subshell, `false &&`
/// — and any SUBSET of them may be the one that does, so no single reading is safe to trust. The
/// guard reads the line twice, with every value and with the invocation's own alone, and an
/// alias the two readings disagree on — defined, redefined or shadowed by a value that may not
/// reach git — is UNREAD, as is every alias when only the fuller reading is opaque. A line value
/// can therefore only add refusals.
#[derive(Clone, Debug, Default)]
struct LineConfig {
    /// Folded alias name → body, or `None` for a body only the shell can name, as the
    /// invocation's own assignments and git's options set them. Later sources were inserted
    /// later, so the map holds the one git would use.
    aliases: std::collections::HashMap<String, Option<String>>,
    opaque: bool,
    /// Aliases an earlier segment's value defines or shadows: unread.
    unsure: std::collections::HashSet<String>,
    /// An earlier segment's value may define ANY alias: every alias is unread.
    unsure_all: bool,
    /// Aliases git's own options on the command define: the last word, whatever came before.
    argv: std::collections::HashSet<String>,
}

/// The environment variables that point git at a config file of their own choosing.
const CONFIG_FILE_ENV: [&str; 5] = [
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG",
    "HOME",
    "XDG_CONFIG_HOME",
];

/// A value the line sets for an environment variable: the last assignment, `Some(None)` when
/// only the shell can name it, `None` when the line does not set it.
fn line_value(env: &[String], name: &str) -> Option<Option<String>> {
    env.iter().rev().find_map(|assign| {
        let (n, v) = assign.split_once('=')?;
        (n == name).then(|| (!shell_fills(v)).then(|| v.to_string()))
    })
}

/// Whether the shell, not the guard, decides what this word is: a parameter, a substitution.
fn shell_fills(word: &str) -> bool {
    word.contains(['$', '`'])
}

impl LineConfig {
    /// The configuration of an invocation whose environment may be `env` — everything the line
    /// set — and is surely `own_env`, with git's own options `pre`.
    fn of(pre: &[String], env: &[String], own_env: &[String]) -> Self {
        let mut own = Self::read(pre, own_env, env);
        let before: Vec<&String> = env.iter().filter(|a| !own_env.contains(a)).collect();
        own.mark_unsure(&before, env, own_env);
        own
    }

    /// Every alias a value from an earlier segment may define or shadow, read leniently: EVERY
    /// value each variable is given, not the last, because any subset of them may be the one
    /// that reaches git (D-1358f). A config file, an include, or a key, count or list only the
    /// shell can name may define any alias.
    fn mark_unsure(&mut self, before: &[&String], env: &[String], own_env: &[String]) {
        let mut all = false;
        let mut names: Vec<String> = Vec::new();
        let mut key = |key: &str, all: &mut bool| {
            if shell_fills(key) {
                *all = true;
                return;
            }
            let folded = key.to_lowercase();
            if folded == "include.path"
                || (folded.starts_with("includeif.") && folded.ends_with(".path"))
            {
                *all = true;
            } else if let Some(name) = folded.strip_prefix("alias.") {
                names.push(name.to_string());
            }
        };
        let own_sets = |prefix: &str| {
            own_env.iter().any(|a| {
                a.split_once('=')
                    .is_some_and(|(n, _)| n.starts_with(prefix))
            })
        };
        for assign in before {
            let Some((name, value)) = assign.split_once('=') else {
                continue;
            };
            if CONFIG_FILE_ENV.contains(&name) {
                all = true;
            } else if name == "GIT_CONFIG_PARAMETERS" {
                match (shell_fills(value), shellseg::posix_split(value)) {
                    (false, Ok(words)) => {
                        for word in words {
                            key(
                                word.split_once('=').map_or(word.as_str(), |(k, _)| k),
                                &mut all,
                            );
                        }
                    }
                    _ => all = true,
                }
            } else if name == "GIT_CONFIG_COUNT" {
                // A count the shell fills in, or one that turns the invocation's own pairs on
                // or off, may make any of them git's.
                if shell_fills(value)
                    || value.trim().parse::<usize>().is_err()
                    || (!own_sets("GIT_CONFIG_COUNT")
                        && (own_sets("GIT_CONFIG_KEY_") || own_sets("GIT_CONFIG_VALUE_")))
                {
                    all = true;
                }
            } else if name.starts_with("GIT_CONFIG_KEY_") {
                key(value, &mut all);
            } else if let Some(n) = name.strip_prefix("GIT_CONFIG_VALUE_") {
                // The value is an alias's body if any key at its index names one.
                let key_name = format!("GIT_CONFIG_KEY_{n}");
                let mut keys: Vec<String> = env
                    .iter()
                    .filter_map(|a| a.split_once('='))
                    .filter(|(k, _)| *k == key_name)
                    .map(|(_, v)| v.to_string())
                    .collect();
                keys.extend(std::env::var(&key_name).ok());
                if keys.is_empty() {
                    all = true;
                }
                for k in keys {
                    key(&k, &mut all);
                }
            }
        }
        for name in names {
            if !self.argv.contains(&name) {
                self.unsure.insert(name);
            }
        }
        self.unsure_all |= all;
    }

    /// The configuration `env` sets, with git's own options `pre`; `full` is the whole line's
    /// environment, which a `--config-env` variable is looked for in too.
    fn read(pre: &[String], env: &[String], full: &[String]) -> Self {
        let mut line = LineConfig::default();
        let process = |name: &str| std::env::var(name).ok();
        // The value an invocation sees: the line's, else the one this session started with.
        let value = |name: &str| match line_value(env, name) {
            Some(v) => v,
            None => process(name),
        };
        let sets = |prefix: &str| {
            env.iter().any(|a| {
                a.split_once('=')
                    .is_some_and(|(n, _)| n.starts_with(prefix))
            })
        };
        if CONFIG_FILE_ENV
            .iter()
            .any(|name| line_value(env, name).is_some())
        {
            line.opaque = true;
        }
        // The numbered pairs, read where the line touches them: the session's own are git's to
        // read, and the guard's git reads them.
        if sets("GIT_CONFIG_COUNT") || sets("GIT_CONFIG_KEY_") || sets("GIT_CONFIG_VALUE_") {
            match value("GIT_CONFIG_COUNT").map(|c| c.trim().parse::<usize>()) {
                Some(Ok(count)) if count <= MAX_CHECKOUT_OPERANDS * 4 => {
                    for n in 0..count {
                        match value(&format!("GIT_CONFIG_KEY_{n}")) {
                            Some(key) => {
                                // A value set nowhere the guard can see is unread, not empty.
                                let body = match line_value(env, &format!("GIT_CONFIG_VALUE_{n}")) {
                                    Some(v) => v,
                                    None => process(&format!("GIT_CONFIG_VALUE_{n}")),
                                };
                                line.entry(&key, body);
                            }
                            None => line.opaque = true,
                        }
                    }
                }
                Some(Ok(_)) | Some(Err(_)) => line.opaque = true,
                None => {
                    // A count only the shell can name, or pairs with no count to read them by.
                    if line_value(env, "GIT_CONFIG_COUNT").is_some() {
                        line.opaque = true;
                    }
                }
            }
        }
        match line_value(env, "GIT_CONFIG_PARAMETERS") {
            Some(Some(list)) => match shellseg::posix_split(&list) {
                Ok(words) => {
                    for word in words {
                        let (key, body) = match word.split_once('=') {
                            Some((k, v)) => (k.to_string(), v.to_string()),
                            None => (word.clone(), String::new()),
                        };
                        line.entry(&key, Some(body));
                    }
                }
                Err(_) => line.opaque = true,
            },
            Some(None) => line.opaque = true,
            None => {}
        }
        let mut prev: Option<&str> = None;
        for word in pre {
            line.read_option(prev, word, env, full);
            prev = Some(word);
        }
        line
    }

    /// Take in one of git's own options, given the word before it: `-c key=value`,
    /// `--config-env key=VAR` and `--config-env=key=VAR`.
    ///
    /// These are git's own words, the last say whatever the environment holds, so an alias they
    /// define is no longer unsure — unless its `--config-env` variable comes from an earlier
    /// segment, which may not reach git.
    fn read_option(&mut self, prev: Option<&str>, word: &str, env: &[String], full: &[String]) {
        let config_env = |this: &mut Self, spec: &str| {
            let Some((key, var)) = spec.split_once('=') else {
                return;
            };
            // git reads VAR from the environment it is given; a VAR the invocation does not set
            // is the session's, unless an earlier segment sets it — then it may not reach git —
            // and one the session does not hold either is one the guard cannot see.
            let body = match line_value(env, var) {
                Some(v) => v,
                None if line_value(full, var).is_some() => None,
                None => std::env::var(var).ok(),
            };
            this.option_entry(key, body);
        };
        match prev {
            Some("-c") => {
                let (key, body) = match word.split_once('=') {
                    Some((k, v)) => (k, Some(v.to_string())),
                    None => (word, Some(String::new())),
                };
                let body = body.filter(|b| !shell_fills(b));
                self.option_entry(key, body);
            }
            Some("--config-env") => config_env(self, word),
            _ => {
                if let Some(spec) = word.strip_prefix("--config-env=") {
                    config_env(self, spec);
                }
            }
        }
    }

    /// An entry git's own options make: the last word on that alias.
    fn option_entry(&mut self, key: &str, body: Option<String>) {
        self.entry(key, body);
        if let Some(name) = key.to_lowercase().strip_prefix("alias.") {
            self.argv.insert(name.to_string());
            self.unsure.remove(name);
        }
    }

    /// One configuration entry: an alias, a file to include, or nothing the guard reads.
    fn entry(&mut self, key: &str, body: Option<String>) {
        if shell_fills(key) {
            self.opaque = true;
            return;
        }
        let folded = key.to_lowercase();
        if folded == "include.path"
            || (folded.starts_with("includeif.") && folded.ends_with(".path"))
        {
            self.opaque = true;
        } else if let Some(name) = folded.strip_prefix("alias.") {
            self.aliases.insert(name.to_string(), body);
        }
    }

    /// The body the line gives the alias `folded`: `Some(Ok(body))`, `Some(Err(Unread))` when
    /// only the shell or an unread file can say, `None` when the config files are git's to ask.
    fn body(&self, folded: &str) -> Option<Result<String, AliasEnd>> {
        if !self.argv.contains(folded) && (self.unsure_all || self.unsure.contains(folded)) {
            return Some(Err(AliasEnd::Unread));
        }
        match self.aliases.get(folded) {
            Some(Some(body)) => Some(Ok(body.clone())),
            Some(None) => Some(Err(AliasEnd::Unread)),
            None if self.opaque => Some(Err(AliasEnd::Unread)),
            None => None,
        }
    }

    /// Whether `word` may be an alias the line defines: one it names, or, on an opaque line, any
    /// word that is not git's own.
    fn names(&self, folded: &str, word: &str) -> bool {
        self.aliases.contains_key(folded)
            || self.unsure.contains(folded)
            || ((self.opaque || self.unsure_all)
                && !BRANCH_MOVERS.contains(&word)
                && !GIT_KNOWN_SUBCOMMANDS.contains(&word))
    }
}

/// How following an alias ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasEnd {
    /// At a word that is no alias, a loop, or a body charter does not read.
    Resolved,
    /// A body git could not be asked for.
    Unasked,
    /// Still on an alias when [`MAX_ALIAS_HOPS`] ran out.
    TooDeep,
    /// Defined, or possibly defined, where the guard does not read: a config file the command
    /// names, or a value only the shell can name (#1358).
    Unread,
}

/// [`resolve_git_alias`]'s hops, with each alias's body asked of `body`: `Ok(body)` (`""` for
/// none), or the end a body that could not be read makes — [`AliasEnd::Unasked`] or
/// [`AliasEnd::Unread`] — which stops the walk where it stands.
fn expand_alias(
    sub: &str,
    post: &[String],
    body: &mut dyn FnMut(&str) -> Result<String, AliasEnd>,
) -> (String, Vec<String>, AliasEnd) {
    let mut sub = sub.to_string();
    let mut post = post.to_vec();
    let mut seen: Vec<String> = Vec::new();
    let settled = |sub: &str, seen: &[String]| {
        BRANCH_MOVERS.contains(&sub)
            || GIT_KNOWN_SUBCOMMANDS.contains(&sub)
            || seen.iter().any(|s| s == sub)
    };
    for _ in 0..MAX_ALIAS_HOPS {
        if settled(&sub, &seen) {
            return (sub, post, AliasEnd::Resolved);
        }
        seen.push(sub.clone());
        let text = match body(&sub) {
            Ok(text) => text,
            Err(end) => return (sub, post, end),
        };
        let Some((first, tail)) = alias_words(&text) else {
            return (sub, post, AliasEnd::Resolved);
        };
        let mut next = tail;
        next.extend(post);
        sub = first;
        post = next;
    }
    // The hops ran out: still on an alias is a chain whose end is out of sight.
    let end = if settled(&sub, &seen) {
        AliasEnd::Resolved
    } else {
        match body(&sub) {
            Err(end) => end,
            Ok(text) if alias_words(&text).is_some() => AliasEnd::TooDeep,
            Ok(_) => AliasEnd::Resolved,
        }
    };
    (sub, post, end)
}

/// The git words an alias body expands to — its first word and the rest — or `None` for an
/// empty body, one that does not split, or a `!` body that runs something other than git.
fn alias_words(body: &str) -> Option<(String, Vec<String>)> {
    if body.is_empty() {
        return None;
    }
    let shell = body.starts_with('!');
    let text = if shell { &body[1..] } else { body };
    // A `!` alias is run by `sh -c`, so it is read as the shell reads it.
    let split = if shell {
        shellseg::shell_split(text)
    } else {
        shellseg::posix_split(text)
    };
    let mut toks = split.ok()?;
    if shell {
        if toks.first().map(|t| shellwrap::base_lower(t)).as_deref() != Some("git") {
            return None;
        }
        toks.remove(0);
    }
    let (first, tail) = toks.split_first()?;
    Some((first.clone(), tail.to_vec()))
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
        if inv.unread {
            return Some(format!(
                "cannot tell what this `git` command does: a command substitution among its \
                 options hides which repository it acts on, and purlis could not work out which \
                 of the words after it git would run as its command — a guard that opened \
                 because it could not tell is no guard. Spell the path out and purlis checks \
                 it: `git -C <path>` and `cd <path> && git …` are both read. {}",
                root_tail(default_branch(&root).as_deref())
            ));
        }
        let (mut sub, mut post) = (inv.sub.clone(), inv.post.clone());
        if !BRANCH_MOVERS.contains(&sub.as_str()) {
            // `co = checkout` moves the root's HEAD exactly as far (#461, round two).
            let end;
            (sub, post, end) =
                resolve_git_alias_to_its_end(&root, &sub, &post, &inv.pre, &inv.env, &inv.own_env);
            if end == AliasEnd::Unread {
                return Some(format!(
                    "cannot tell what `git {}` does: it may be an alias, and this command sets \
                     git configuration purlis does not read — a config file it names, a value \
                     only the shell fills in, or one an earlier command on the line sets, which \
                     may not reach git — so where the alias leads is out of sight. \
                     Run the command the alias stands for, or leave that setting off. {}",
                    inv.sub,
                    root_tail(default_branch(&root).as_deref())
                ));
            }
            if end == AliasEnd::TooDeep {
                return Some(format!(
                    "cannot tell what `git {}` does in the PLANE ROOT: it is an alias that leads \
                     to another alias more than {MAX_ALIAS_HOPS} times, past where purlis \
                     follows them, and the end of a chain it cannot see may move HEAD. Run the \
                     command it stands for instead. {}",
                    inv.sub,
                    root_tail(default_branch(&root).as_deref())
                ));
            }
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
        let opening = if inv.unnamed {
            format!(
                "cannot tell which repository this `git {sub}` acts on: a directory it is sent \
                 to is named only when the shell runs it (a variable, a command substitution, a \
                 glob, `~user`, `cd -`), so it may be the PLANE ROOT. Spell the path out and \
                 purlis checks it: `git -C <path>` and `cd <path> && git …` are both read. "
            )
        } else {
            match kind {
                OperandKind::Both => format!(
                    "cannot tell what `git checkout {want0}` does in the PLANE ROOT — it is \
                 AMBIGUOUS: '{want0}' is both a tracked path here and a name git \
                 resolves to a commit, so it could be a file restore or a ref move, and \
                 git breaks that tie in favour of the REF — this would switch the root. \
                 Say which you meant and it runs: `git restore {want0}` (or \
                 `git checkout -- {want0}`) restores the file, and purlis allows that \
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
                    "would move HEAD in the PLANE ROOT — purlis could not ask git whether \
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
                         not recognise the option '{unplaced}', and an option purlis cannot place \
                         is one that may move HEAD — `git checkout --orphan <file>` reads exactly \
                         like a restore and creates a branch. Only a form purlis can show is a \
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
            }
        };
        return Some(format!("{opening}{}", root_tail(default.as_deref())));
    }
    None
}

/// What every branch-guard denial ends with: why the root is guarded, where branch work goes,
/// and the remedy that stays runnable — naming the default branch when there is one.
fn root_tail(default: Option<&str>) -> String {
    let back = match default.filter(|d| !d.is_empty()) {
        Some(d) => format!(
            "`git checkout {d}` — putting the root back on its default branch — is always \
             allowed."
        ),
        None => "Putting the root back on its default branch is always allowed.".to_string(),
    };
    format!(
        "The plane root is one working tree every session \
         shares — two agents here silently clobber each other's branches, and the \
         symptom looks like an unrelated bug. Branch work belongs in a workspace \
         clone: `purlis workspace create <task>`, then `purlis clone <repo>`. \
         {back}"
    )
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
        let tree_mode =
            |post: &[String]| post.iter().any(|a| RESET_TREE_MODES.contains(&a.as_str()));
        let mut post = inv.post.clone();
        if inv.sub != "reset" {
            // Any reading that is a working-tree reset is the one measured (#1358).
            let (s, p, _) = resolve_git_alias_to_its_end(
                &root,
                &inv.sub,
                &inv.post,
                &inv.pre,
                &inv.env,
                &inv.own_env,
            );
            if s != "reset" {
                continue;
            }
            post = p;
        }
        if !tree_mode(&post) {
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
        if inv.unnamed {
            return Some(format!(
                "cannot tell which repository this `git reset` acts on: a directory it is sent \
                 to is named only when the shell runs it (a variable, a command substitution, a \
                 glob, `~user`, `cd -`), so it may be the PLANE ROOT — and there it would delete \
                 {n} {commits} that {are} not on {upstream}, overwriting the working tree. Spell \
                 the path out and purlis checks it: `git -C <path>` and `cd <path> && git …` are \
                 both read."
            ));
        }
        return Some(format!(
            "would delete {n} {commits} from the PLANE ROOT that {are} \
             not on {upstream}, and this reset overwrites the working tree — their content \
             leaves the disk with the reflog as the only copy. An unpushed commit here is \
             usually a memory commit whose push a protected branch refused, which is how \
             eleven of them were lost. See exactly what would go: \
             `git -C {root} log --oneline '@{{upstream}}..HEAD'`. Keep it: `purlis save` \
             pushes it, and this reset stops being refused the moment it lands."
        ));
    }
    None
}

#[cfg(test)]
mod tests;
