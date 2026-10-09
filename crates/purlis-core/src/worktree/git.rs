//! Running the git binary. Nothing here decides anything.
//!
//! Charter drives git through its binary rather than a library (ADR 0027), so this is the one
//! place that spawns it, and the hazards of doing so are handled here and nowhere else.
//!
//! # The environment is constructed, not inherited
//!
//! Subtracting a denylist does not work. `git rev-parse --local-env-vars` defines where git
//! looks for a **repository** — fifteen names on git 2.50.1. It says nothing about what git
//! **executes**, and measured on that same git:
//!
//! - `GIT_EXEC_PATH` makes `push` run an attacker's `git-remote-https`, because the https
//!   remote helper is an external binary resolved out of it.
//! - `GIT_TRACE`, and the `GIT_TRACE2*` family, append to any path given, on *every* verb
//!   including the read-only ones — a write outside the plane that charter never constructs
//!   and no containment check ever sees.
//! - `GIT_SSH_COMMAND`, `GIT_SSH`, `GIT_ASKPASS`, `SSH_ASKPASS` and `GIT_PROXY_COMMAND` each
//!   name a program git runs. `GIT_TERMINAL_PROMPT=0` closes the terminal prompt, not the
//!   askpass helper.
//! - `PATH` decides which `git` runs at all.
//!
//! None of those is repository-local, so no list git prints will ever name them, and charter
//! runs **from a hook**, where an attacker-set environment is the ordinary case. So the child
//! gets `env_clear()` and then only what git needs. A variable git grows next year is absent
//! by default rather than present until someone notices.
//!
//! # `env_clear` alone does not close it, because `HOME` has to stay
//!
//! git reads its global config from `HOME`, and that config can name programs to run. With
//! *exactly* the four variables built below and nothing else, measured:
//!
//! ```text
//! $ env -i HOME=/tmp/evilhome PATH=/usr/bin:/bin GIT_TERMINAL_PROMPT=0 LC_ALL=C \
//!     git -C repo worktree add -b probe …
//! → ran /tmp/evilhome's core.hooksPath hook
//! ```
//!
//! An attacker who can set `GIT_EXEC_PATH` can set `HOME`, and gets the same execution. But
//! `HOME` cannot simply be dropped: git's global config and every credential helper live
//! there, and `publish` needs them.
//!
//! So the execution keys are turned off **on the command line**, where `-c` beats every
//! config file — `core.hooksPath` and `core.fsmonitor` are the two that run a program on the
//! verbs this module uses (`worktree add` runs `post-checkout`; `status` runs the fsmonitor).
//! Measured: the same call with `-c core.hooksPath=/dev/null` does not run the hook.
//!
//! **What that leaves.** A repository's own hooks are disabled for charter's calls too, which
//! is deliberate — charter never commits, and a `post-checkout` charter triggers is not one
//! the operator asked for. And `HOME` remains the residual surface for anything git grows
//! that names a program through config. A call that crosses a network ([`run_network`]) takes
//! `credential.helper` out of config's hands too — it resets every configured helper and names
//! the forge CLI's own — and refuses SSH outright, so `core.sshCommand` is never reached.
//!
//! # A folder's `.git` file is read here, not by git
//!
//! A linked worktree's `.git` is a file in the folder a chat works in, and git follows it to
//! whatever git directory it names, configuration and all. So [`run`], [`run_untimed`],
//! [`run_with_input`] and [`run_network`] read it first ([`super::link`], #1055, which says
//! what is accepted): a call in a folder whose file names anything else is not started, and
//! answers git's own failure status with one sentence where git's words would be; a call in
//! one that checks is given `--git-dir` and `--work-tree`, so git reads what was checked. A
//! clone, whose `.git` is a directory, is run as before.
//!
//! [`run_as_session`] and [`run_in_hook`] are not held to it: they ask what the chat's own
//! git would answer, in layouts the check would refuse (a scratch clone often keeps its git
//! directory beside it). Where a harness runs its hooks outside its sandbox, those calls
//! follow whatever the folder's link names; that is not closed here.
//!
//! **No runner lets git start a git of its own inside a submodule** ([`NO_PROGRAMS`]), those
//! two included: such a child finds its repository for itself, past every check above.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The deadline for a READ — a listing, a status, a config lookup.
///
/// **Never for `worktree add` or `merge`.** Those check out a tree: on a large repo or a cold
/// cache five seconds is routine, and a killed `worktree add` leaves the registration written
/// and the checkout half-done, so the retry meets "branch already exists". Python times out
/// its listing and nothing else (`workspace._GIT_TIMEOUT`); so does this.
///
/// **Thirty seconds, not five.** Five was Python's number for a listing, and it is wrong for
/// a `status`: measured on `main` (run 35462478106), a cold macOS CI runner blew it on a
/// three-file fixture repo, and the panel told the operator "charter could not read the
/// working tree … git did not answer within 5 seconds" about a repository that was perfectly
/// readable. The same is true of any machine under load — which is the machine charter is
/// built for, running dozens of harnesses at once. Nothing waits on this: the read is on
/// `spawn_blocking` and the cell says so until it answers. The deadline is here to stop a
/// hung git holding a thread for ever, and thirty seconds does that just as well as five
/// while no longer calling a slow answer a broken repository.
pub const READ: Duration = Duration::from_secs(30);

/// The deadline for a call that crosses a network: `clone`, `fetch`, and `publish` when it
/// lands. Only ever through [`run_network`], which is what holds such a call to the
/// one-credential rule.
pub const NETWORK: Duration = Duration::from_secs(120);

/// Where to look for git when the inherited `PATH` is not to be trusted.
///
/// Charter runs from a hook, so `PATH` is attacker-settable and `Command::new("git")` would
/// let it choose the binary. These are searched first, in order; the inherited `PATH` is the
/// fallback so that a machine keeping git somewhere else still works, and that fallback is
/// the one part of this an attacker with the environment can still reach.
pub(crate) const GIT_DIRS: [&str; 4] = ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin", "/bin"];

/// What the child is given, and nothing else.
fn child_env(git_dirs: &str) -> Vec<(&'static str, String)> {
    let mut env: Vec<(&'static str, String)> = Vec::new();
    // git reads the global config from HOME, and every credential helper needs it.
    if let Some(home) = std::env::var_os("HOME") {
        env.push(("HOME", home.to_string_lossy().into_owned()));
    }
    // For the credential helpers themselves (`gh`, `git-credential-osxkeychain`), which git
    // resolves as programs. Constructed, never inherited.
    env.push(("PATH", git_dirs.to_string()));
    // A prompt inside a subprocess whose output is captured is an invisible, endless wait.
    // This makes it an auth error instead. It does NOT cover the askpass helper, which is
    // why that variable is simply not passed through.
    env.push(("GIT_TERMINAL_PROMPT", "0".into()));
    // Deterministic messages for the few places charter matches its own output shape.
    env.push(("LC_ALL", "C".into()));
    // No lazy fetch, ever (git 2.44+). A repository whose config names a promisor remote
    // fetches an object it lacks the moment a read needs one — a status's rename search is
    // enough — and that fetch starts the program the remote's config names, as the operator.
    // A repository an agent can write is one whose config it can write, and charter's reads
    // run outside any sandbox; with this the read fails on the missing object instead (FM-4).
    // An explicit `fetch` or `clone` is not a lazy fetch and is unaffected.
    env.push(("GIT_NO_LAZY_FETCH", "1".into()));
    env
}

/// What one git call answered. `code` is `None` when the deadline passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub code: Option<i32>,
    pub out: String,
    pub err: String,
}

impl Run {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// stdout with the trailing newline removed — what almost every caller wants.
    pub fn line(&self) -> &str {
        self.out.trim_end_matches(['\n', '\r'])
    }
}

/// git itself could not be started.
#[derive(Debug, thiserror::Error)]
#[error("purlis could not run git: {0}. Install git, or put it on PATH")]
pub struct GitUnavailable(#[from] std::io::Error);

/// The absolute git binary, and the `PATH` the child gets.
///
/// **The fallback has to be explicit.** Returning a bare `"git"` does not work after
/// `env_clear`: Rust resolves a bare program name against the CHILD's `PATH`, which is the
/// fixed list below — so on any machine whose git lives elsewhere (NixOS, asdf/mise,
/// `~/.local/bin`) every verb would fail with "charter could not run git" while git is on
/// PATH. So the inherited `PATH` is searched here, in the parent, and what the child gets is
/// the absolute path that search found plus the directory holding it.
fn git_binary() -> (PathBuf, String) {
    git_binary_in(&GIT_DIRS, std::env::var_os("PATH").as_deref())
}

/// [`git_binary`] against `fixed` and an inherited `path`, so the search can be tested
/// without the process's own environment.
fn git_binary_in(fixed: &[&str], path: Option<&std::ffi::OsStr>) -> (PathBuf, String) {
    let mut dirs: Vec<String> = fixed.iter().map(|d| (*d).to_string()).collect();
    for dir in fixed {
        let candidate = Path::new(dir).join("git");
        if candidate.is_file() {
            return (candidate, path_value(&dirs));
        }
    }
    // Searched in the PARENT, where `PATH` is still readable, and resolved to an absolute
    // path before the child is built. An attacker who controls the parent's `PATH` still
    // chooses here — but they control charter's own binary lookup too, so this adds no
    // surface that was not already there.
    //
    // **Absolute entries only** ([`crate::programs::searchable`]). A `.` or an empty entry is
    // the working directory, and charter's working directory is routinely a repository: with
    // no git in the fixed directories, a binary called `git` committed to that repository
    // was the one this function chose — and the one handed a forge credential. That is not
    // the operator's `PATH` choosing, which the paragraph above accepts; it is whoever wrote
    // the repository choosing.
    if let Some(path) = path {
        for dir in crate::programs::searchable(path) {
            let candidate = dir.join("git");
            if candidate.is_file() {
                dirs.insert(0, dir.display().to_string());
                return (candidate, path_value(&dirs));
            }
        }
    }
    (PathBuf::from("git"), path_value(&dirs))
}

/// `dirs` as a child's `PATH`, joined the way this platform splits one — `:` here, `;` on
/// Windows (charter-app#100: a `PATH` joined with `:` arrives there as one nonsense entry).
///
/// **A directory that cannot be written into a `PATH` is left out, not split.** On unix that
/// is a directory with a `:` in it, which the joined string would hand the child as TWO
/// entries — the second one relative, resolved against whatever directory the child runs
/// in. A `$HOME` with a colon puts every user directory [`crate::programs`] searches in that
/// position. The program itself is still run by the absolute path found for it; what the
/// child loses is one directory to look for ITS programs in, which is the smaller failure and
/// the same trade `programs::chat_path_from` makes.
pub(crate) fn path_value(dirs: &[String]) -> String {
    let joinable = dirs
        .iter()
        .filter(|dir| std::env::join_paths([dir.as_str()]).is_ok());
    std::env::join_paths(joinable)
        .ok()
        .and_then(|joined| joined.into_string().ok())
        .unwrap_or_default()
}

/// Config keys that name a program git will run, turned off where no config can re-enable
/// them. `-c` on the command line beats the system, global and repository files.
///
/// Not a general hardening list: these are the keys reachable from the verbs this module
/// runs. `core.hooksPath` covers `post-checkout` on `worktree add` and `post-merge` on
/// `merge`; `core.fsmonitor` covers `status`, which `dirt` runs on every guard.
///
/// **And git starts no git of its own inside a submodule** (#1055). `status` and `diff` look
/// into each populated submodule by running git there, a `fetch` or a checkout may recurse,
/// and that child finds its repository through the submodule folder's own `.git`, which this
/// module's link check never sees. So a submodule is read by its recorded commit only. What
/// that costs: uncommitted work inside a submodule does not show in purlis. Plumbing diffs do
/// not read `diff.ignoreSubmodules`, so they are given the option ([`PLUMBING_DIFFS`]).
const NO_PROGRAMS: [&str; 6] = [
    "core.hooksPath=/dev/null",
    "core.fsmonitor=false",
    "diff.ignoreSubmodules=dirty",
    "status.submoduleSummary=false",
    "submodule.recurse=false",
    "fetch.recurseSubmodules=false",
];

/// The diff verbs that ignore `diff.ignoreSubmodules`, and are told on their own command line.
const PLUMBING_DIFFS: [&str; 3] = ["diff-files", "diff-index", "diff-tree"];

/// What a plumbing diff is told so that it starts no git inside a submodule.
const SUBMODULES_BY_COMMIT: &str = "--ignore-submodules=dirty";

/// The one-credential rule (charter `docs/git-policy.md`), put where no config file can
/// relax it: every call that crosses a network goes over HTTPS with ONE forge CLI's token,
/// never SSH, never a prompt, never a helper the operator's config happens to name.
///
/// - `protocol.ssh.allow=never` is the load-bearing one. A `url.git@github.com:.insteadOf
///   https://github.com/` in the operator's global config — a common way to force SSH —
///   rewrites the HTTPS URL charter built, and Python charter's clone then went over SSH
///   without a word. Here git refuses the transport instead, and the caller says why.
/// - `ext`, `git` and plain `http` are refused for the same reason: `ext::` runs a command,
///   `git://` and `http://` carry no token or carry it in the clear. `file` is left alone:
///   it reaches no network and asks for no credential.
/// - `credential.helper=` EMPTY resets every helper configured before it — the keychain, a
///   store file — so the only helper left is the forge's own, added after it. That is what
///   "one credential" means; a union of helpers is several.
/// - `core.askPass=` empty is git's own spelling of "no askpass program"; the variable is
///   already never passed.
/// - `submodule.recurse=false`: a submodule URL comes out of the cloned repo and can name
///   any host, and a nested clone does not read the policy (see `docs/git-policy.md`).
const NETWORK_RULE: [&str; 7] = [
    "protocol.ssh.allow=never",
    "protocol.ext.allow=never",
    "protocol.git.allow=never",
    "protocol.http.allow=never",
    "core.askPass=",
    "credential.helper=",
    "submodule.recurse=false",
];

/// The variables a forge CLI reads its credential, its config or its route from.
///
/// Passed through to a NETWORK call only, because git runs the forge's credential helper with
/// git's own environment — and an operator whose `gh` holds its token in `GH_TOKEN`, in a
/// keyring reached over D-Bus, or behind a corporate proxy would otherwise be refused by a
/// clone Python charter performed. Each is read out of this process's environment, never put
/// on a command line and never logged: a token in `argv` is readable by every process on the
/// machine, one in the environment only by the same user.
///
/// # What an attacker who can set each one gains
///
/// The premise throughout is an attacker who already controls this process's environment.
/// They cannot read the operator's token through any of these — every one of them flows
/// INTO the CLI — so what they buy is a credential or a route of their own, and the question
/// for each entry is whether that reaches further than the environment control they started
/// with.
///
/// - **The token names** (`GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN`,
///   `GITHUB_ENTERPRISE_TOKEN`, `GITLAB_TOKEN`, `GITLAB_ACCESS_TOKEN`, `OAUTH_TOKEN`): the
///   call authenticates as the attacker's account instead of the operator's. The host is
///   still the one charter's URL names, and the fetched bytes still land on the operator's
///   disk, so this ends in a failed or an under-privileged read — not in a disclosure.
/// - **The config locations** (`GH_CONFIG_DIR`, `GLAB_CONFIG_DIR`, `XDG_CONFIG_HOME`,
///   `XDG_DATA_HOME`, `XDG_STATE_HOME`): the same thing by another road — a config file the
///   attacker wrote, holding their token. It is **not** code execution: `gh` expands an alias
///   only for a word that is not one of its own commands, and every call here is a built-in
///   (`api`, `auth`). Measured on gh 2.83.2 with a hand-written config holding
///   `aliases: {api: '!echo ALIAS_RAN', notacmd: '!echo …'}` — `gh api` ran the built-in,
///   `gh notacmd` ran the alias.
/// - **The keyring route** (`XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`): the CLI asks a
///   secret service the attacker owns, which answers with their credential and learns that a
///   lookup happened. The operator's real keyring is on their own session bus and is not
///   reachable this way. Without these, a Linux desktop that keeps the token in the keyring
///   fails every call with "not logged in".
/// - **The proxies** (`HTTPS_PROXY`/`https_proxy`, `HTTP_PROXY`/`http_proxy`, `ALL_PROXY`,
///   `NO_PROXY`/`no_proxy`): the connection is routed through the attacker's proxy, which
///   sees the host and the timing. TLS is still end to end through the `CONNECT` tunnel, so
///   the token and the contents are not theirs. Without these, a network whose only route out
///   is a proxy cannot reach the forge at all.
/// - **The CA bundle** (`SSL_CERT_FILE`, `SSL_CERT_DIR`): **the sharp one.** An attacker who
///   sets it adds a certificate authority the CLI trusts, and with a proxy they also set that
///   is a full interception of the CLI's HTTPS, token included. It is here because a
///   corporate CA bundle is how many operators reach their own forge at all, and because the
///   attacker needs control of charter's own environment to use it — at which point they can
///   also choose which `charter` runs. If that trade ever stops holding, this is the first
///   entry to drop.
///
/// # Why what is left out does not break an ordinary `gh`
///
/// - `GH_HOST`, `GITLAB_HOST`: every call passes `--hostname` explicitly, so the variable
///   decides nothing — and passing it would let an environment silently move which host is
///   asked and authenticated against.
/// - `GH_EDITOR`, `EDITOR`, `PAGER`, `GH_PAGER`, `BROWSER`: each names a program. Nothing
///   here is interactive, and the output is captured rather than paged.
/// - `GH_PROMPT_DISABLED`, `NO_PROMPT`, `NO_COLOR`, `GH_NO_UPDATE_NOTIFIER`,
///   `GLAB_CHECK_UPDATE`: charter SETS these itself rather than inheriting them, so an
///   environment cannot switch a prompt back on in a process with no terminal.
/// - `GH_DEBUG`, `GIT_CURL_VERBOSE` and the trace family: they write diagnostics — headers
///   included — into output charter captures and repeats in an error. That is a token leak
///   with extra steps.
/// - Every `GIT_*`, `SSH_AUTH_SOCK`, `SSH_ASKPASS`, `GIT_ASKPASS`, `LD_PRELOAD`, `DYLD_*`:
///   the execution surface the module docs list, plus SSH, which the policy does not use.
/// - `HOME` is not in this list because it is given to every call already; `gh`'s config and
///   the keyring live under it.
pub const CREDENTIAL_ENV: [&str; 23] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
    "GH_CONFIG_DIR",
    "GITLAB_TOKEN",
    "GITLAB_ACCESS_TOKEN",
    "OAUTH_TOKEN",
    "GLAB_CONFIG_DIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NO_PROXY",
    "no_proxy",
    "ALL_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

/// The variables that say WHERE git's configuration is — passed through to [`run_as_session`]
/// only.
///
/// A guard that asks git "what would this command do" has to ask the git the COMMAND will run,
/// and that git reads its global and system config through these. The one that decides a verdict
/// is an alias: `co = checkout` in `$XDG_CONFIG_HOME/git/config`, or in the file
/// `GIT_CONFIG_GLOBAL` names, or in `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_VALUE_<n>`
/// — and a guard whose git cannot see the alias reads `git co feature` as a subcommand it has
/// never heard of and stands aside while the plane root's branch moves. Python's `_git_in`
/// inherits its whole environment, so it sees all of them; the cleared environment every other
/// call here gets would see none.
///
/// **What an attacker who can set them gains is nothing `HOME` does not already give.** Each one
/// only relocates a config FILE, or supplies config entries directly, and `HOME` — which every
/// call keeps, because the global config and every credential helper live under it — relocates
/// the global config already. The keys that run a program on the verbs a guard uses are still
/// turned off on the command line by [`NO_PROGRAMS`], and git documents that `-c` overrides the
/// environment's entries as well as every file's. `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`
/// and the rest of the REPOSITORY-locating family stay out, as Python's `util.run` keeps them
/// out: they would answer for a different repository than the `-C` names.
pub const CONFIG_LOCATION_ENV: [&str; 6] = [
    "XDG_CONFIG_HOME",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
];

/// The most `GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_VALUE_<n>` pairs passed through. git reads as many as
/// `GIT_CONFIG_COUNT` says; past this bound the count itself is withheld rather than truncated,
/// so git is never told to read a pair that was not passed (it refuses a missing one outright).
const MAX_CONFIG_PAIRS: usize = 64;

/// What one spawn adds to the fixed hardening.
#[derive(Default)]
struct Extra {
    /// `-c` settings after [`NO_PROGRAMS`].
    config: Vec<String>,
    /// Whether [`CREDENTIAL_ENV`] reaches the child.
    credentials: bool,
    /// Variables passed through by name, with their values as this process holds them.
    pass: Vec<(String, std::ffi::OsString)>,
    /// Whether git's standard input is a pipe the caller writes, rather than nothing.
    stdin: bool,
    /// The folder's own top and the git directory its `.git` file was checked to name
    /// ([`linked`]): the call is given both, so git reads what was checked.
    link: Option<(PathBuf, PathBuf)>,
}

/// The work tree and git directory a call in `dir` is held to where `dir` is in a linked
/// worktree or a submodule ([`super::link`], #1055), nothing where it is in a clone or in no
/// repository, or the sentence that refuses the call: its `.git` file names something purlis
/// will not follow.
///
/// A chat may be able to rewrite that file, and git would follow it to a configuration the
/// chat wrote. So purlis reads the file itself, and git is told the answer rather than asked
/// to find it.
fn linked(dir: &Path, args: &[&str]) -> Result<Option<(PathBuf, PathBuf)>, &'static str> {
    use super::link::{Check, Link};
    // A call the thread's own isolation pins is held to a git directory its maker checked.
    if isolation().is_some_and(|held| held.pin_in(dir).is_some()) {
        return Ok(None);
    }
    // `worktree repair` is run in a folder that was moved, to write its new name back.
    let repair = {
        let mut words = args.iter().skip_while(|word| **word != "worktree");
        words.next().is_some() && words.next() == Some(&"repair") && verb(args) == Some("worktree")
    };
    let check = if repair { Check::Moved } else { Check::Whole };
    match super::link::of(dir, check)? {
        // `clone` and `init` make a repository of their own, wherever they are run.
        Link::Linked { top, git_dir }
            if !repair && !matches!(verb(args), Some("clone" | "init")) =>
        {
            Ok(Some((top, git_dir)))
        }
        _ => Ok(None),
    }
}

/// What a call [`linked`] refused answers: git's own status for "not a repository", and the
/// sentence where git's words would be.
fn refused(why: &str) -> Run {
    Run {
        code: Some(128),
        out: String::new(),
        err: format!("{why}\n"),
    }
}

/// [`refused`], as bytes.
fn refused_raw(why: &str) -> RawRun {
    let run = refused(why);
    RawRun {
        code: run.code,
        out: Vec::new(),
        err: run.err.into_bytes(),
    }
}

/// [`Extra`] with the folder's checked link, or the answer that refuses the call.
fn held(dir: &Path, args: &[&str], extra: Extra) -> Result<Extra, &'static str> {
    Ok(Extra {
        link: linked(dir, args)?,
        ..extra
    })
}

/// The [`CONFIG_LOCATION_ENV`] variables `lookup` holds, with the numbered pairs
/// `GIT_CONFIG_COUNT` names — or without the count and its pairs when it is not a number up to
/// [`MAX_CONFIG_PAIRS`], or a pair it names is missing.
fn config_location_env(
    lookup: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Vec<(String, std::ffi::OsString)> {
    let mut out = Vec::new();
    for name in CONFIG_LOCATION_ENV {
        let Some(value) = lookup(name) else { continue };
        if name != "GIT_CONFIG_COUNT" {
            out.push((name.to_string(), value));
            continue;
        }
        let Some(count) = value.to_str().and_then(|v| v.trim().parse::<usize>().ok()) else {
            continue;
        };
        if count > MAX_CONFIG_PAIRS {
            continue;
        }
        let mut pairs = Vec::with_capacity(count * 2);
        for n in 0..count {
            let (key, val) = (
                format!("GIT_CONFIG_KEY_{n}"),
                format!("GIT_CONFIG_VALUE_{n}"),
            );
            match (lookup(&key), lookup(&val)) {
                (Some(k), Some(v)) => {
                    pairs.push((key, k));
                    pairs.push((val, v));
                }
                _ => {
                    pairs.clear();
                    break;
                }
            }
        }
        if pairs.len() == count * 2 {
            out.push((name.to_string(), value));
            out.extend(pairs);
        }
    }
    out
}

/// The git command `args` run: the first word past git's own options (`-c key=value`,
/// `-C dir`, and every `--option`).
fn verb<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match *word {
            "-c" | "-C" => {
                words.next();
            }
            option if option.starts_with('-') => {}
            verb => return Some(verb),
        }
    }
    None
}

/// The configuration a **brokered** git call reads (#1335, D-1335-7): no global and no system
/// file at all, and only the keys named here on top of the repository's own.
///
/// A git action the app takes for a sandboxed chat runs outside the chat's sandbox, so nothing
/// the chat could have shaped may name a program for it to run. The operator's global and
/// system files are not the chat's, but they routinely define the very drivers a committed
/// `.gitattributes` turns on (an LFS or a custom `filter`, a `diff` textconv), and a repository
/// the chat wrote could ask for one by name. So a brokered call reads neither file
/// (`GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_NOSYSTEM=1`), and gets back only the identity
/// the operator's global file gives ([`Isolated::operators`]). What the repository's own
/// config defines is checked before the call runs (`gitbroker`).
///
/// Held per thread for the length of [`isolated`], so every git call the brokered command makes
/// on that thread reads it without each of them being told; a command that hands work to other
/// threads carries it there itself ([`isolation`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Isolated {
    /// `key=value`, each passed as `-c`.
    config: Vec<String>,
    /// Each work tree with the git directory a check resolved for it: a call run in that tree
    /// is given both, so git uses the repository that was checked and no other (D-1335-9). A
    /// clone, and beside it the folder of a branch cut from it (D-1453-27).
    pins: Vec<(PathBuf, PathBuf)>,
}

impl Isolated {
    /// `user.name` and `user.email` as given, and nothing else.
    pub fn identity(name: Option<&str>, email: Option<&str>) -> Self {
        let mut config = Vec::new();
        for (key, value) in [("user.name", name), ("user.email", email)] {
            if let Some(value) = value.filter(|v| !v.is_empty() && !v.contains(['\n', '\0'])) {
                config.push(format!("{key}={value}"));
            }
        }
        Self {
            config,
            pins: Vec::new(),
        }
    }

    /// The identity the operator's global git config gives, read through this module's
    /// hardened runner, outside any repository.
    pub fn operators() -> Self {
        let read = |key: &str| {
            run(Path::new("/"), &["config", "--global", "--get", key], READ)
                .ok()
                .filter(Run::ok)
                .map(|run| run.line().to_owned())
        };
        Self::identity(read("user.name").as_deref(), read("user.email").as_deref())
    }

    /// The same, with every call run in `work_tree` given `--git-dir=<git_dir>` and
    /// `--work-tree=<work_tree>`, so it uses the repository a check resolved and discovers none.
    /// A tree pinned before keeps its pin, and one pinned again takes the later git directory.
    #[must_use]
    pub fn pinned(&self, work_tree: &Path, git_dir: &Path) -> Self {
        let tree = real(work_tree);
        let mut pins = self.pins.clone();
        pins.retain(|(held, _)| *held != tree);
        pins.push((tree, git_dir.to_path_buf()));
        Self {
            pins,
            ..self.clone()
        }
    }

    /// The git directory a call run in `dir` is pinned to, if it is.
    pub fn pin_of(&self, dir: &Path) -> Option<&Path> {
        self.pin_in(dir).map(|(_, git_dir)| git_dir)
    }

    /// The pinned work tree `dir` is in, and its git directory: `dir` is that tree's top or
    /// any folder inside it (#1055), so a call run below a pinned top is held to the same
    /// git directory and finds none for itself. Of two pinned trees one inside the other, the
    /// inner one is `dir`'s.
    pub fn pin_in(&self, dir: &Path) -> Option<(&Path, &Path)> {
        let dir = real(dir);
        self.pins
            .iter()
            .filter(|(tree, _)| dir.starts_with(tree))
            .max_by_key(|(tree, _)| tree.components().count())
            .map(|(tree, git_dir)| (tree.as_path(), git_dir.as_path()))
    }

    /// One more key. **For a test's stand-in forge only** (a `url.<base>.insteadOf` that points
    /// a clone at a local directory): the app passes the identity and nothing else.
    #[must_use]
    pub fn also(mut self, key: &str, value: &str) -> Self {
        self.config.push(format!("{key}={value}"));
        self
    }
}

/// `path` with its links resolved, or as it is where it cannot be.
fn real(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

thread_local! {
    static ISOLATED: std::cell::RefCell<Option<Isolated>> = const { std::cell::RefCell::new(None) };
}

/// Runs `then` with every git call this thread makes reading only `isolated`'s configuration.
pub fn isolated<T>(isolated: &Isolated, then: impl FnOnce() -> T) -> T {
    struct Restore(Option<Isolated>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let before = self.0.take();
            ISOLATED.with(|held| *held.borrow_mut() = before);
        }
    }
    let before = ISOLATED.with(|held| held.borrow_mut().replace(isolated.clone()));
    let _restore = Restore(before);
    then()
}

/// The isolation this thread runs under, for a command that carries it to threads of its own.
pub fn isolation() -> Option<Isolated> {
    ISOLATED.with(|held| held.borrow().clone())
}

/// [`isolated`] when `isolation` is one, else `then` as it is.
pub fn within<T>(isolation: Option<&Isolated>, then: impl FnOnce() -> T) -> T {
    match isolation {
        Some(held) => isolated(held, then),
        None => then(),
    }
}

fn spawn_with(dir: &Path, args: &[&str], extra: &Extra) -> Result<Child, GitUnavailable> {
    let (binary, dirs) = git_binary();
    let mut cmd = Command::new(binary);
    for setting in NO_PROGRAMS {
        cmd.arg("-c").arg(setting);
    }
    for setting in &extra.config {
        cmd.arg("-c").arg(setting);
    }
    let isolated = isolation();
    let mut pinned = false;
    if let Some(held) = &isolated {
        // A bare repository is used only when named (D-1335-8): one a chat made, which no
        // `.git` path covers, is never found by discovery.
        cmd.arg("-c").arg("safe.bareRepository=explicit");
        for setting in &held.config {
            cmd.arg("-c").arg(setting);
        }
        // Anywhere in a pinned tree: a call in a folder below its top is held to the same
        // git directory, and nothing is found again from that folder's own `.git`.
        if let Some((tree, git_dir)) = held.pin_in(dir) {
            cmd.arg(format!("--git-dir={}", git_dir.display()))
                .arg(format!("--work-tree={}", tree.display()));
            pinned = true;
        }
    }
    if let (false, Some((top, git_dir))) = (pinned, &extra.link) {
        cmd.arg(format!("--git-dir={}", git_dir.display()))
            .arg(format!("--work-tree={}", top.display()));
    }
    cmd.arg("-C").arg(dir);
    match args.iter().position(|word| Some(*word) == verb(args)) {
        Some(at)
            if PLUMBING_DIFFS.contains(&args[at])
                && !args
                    .iter()
                    .any(|word| word.starts_with("--ignore-submodules")) =>
        {
            cmd.args(&args[..=at])
                .arg(SUBMODULES_BY_COMMIT)
                .args(&args[at + 1..]);
        }
        _ => {
            cmd.args(args);
        }
    }
    cmd.env_clear();
    for (k, v) in child_env(&dirs) {
        cmd.env(k, v);
    }
    // `status` refreshes the index through `index.lock` whenever stat data is stale, which is
    // the lock an agent's or the operator's `git add` then fails on (FD-11). Every `status`
    // charter runs is a read, so git's optional locks are held back for all of them here,
    // where no caller can forget it: `GIT_OPTIONAL_LOCKS=0` is `--no-optional-locks`.
    if verb(args) == Some("status") {
        cmd.env("GIT_OPTIONAL_LOCKS", "0");
    }
    if extra.credentials {
        for name in CREDENTIAL_ENV {
            if let Some(value) = std::env::var_os(name) {
                cmd.env(name, value);
            }
        }
    }
    for (name, value) in &extra.pass {
        cmd.env(name, value);
    }
    if isolated.is_some() {
        // After everything passed through, so no relocated config file or entry comes back
        // in. `XDG_CONFIG_HOME` stays for a forge CLI's own config: with the global file
        // named, git reads no other.
        for name in CONFIG_LOCATION_ENV
            .into_iter()
            .filter(|name| name.starts_with("GIT_"))
        {
            cmd.env_remove(name);
        }
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
        cmd.env("GIT_CONFIG_NOSYSTEM", "1");
        // Discovery looks at `dir` itself and never climbs to the workspace or the project
        // above it (D-1335-8): every brokered call runs at the top of the tree it means.
        if let Some(parent) = real(dir).parent() {
            cmd.env("GIT_CEILING_DIRECTORIES", parent);
        }
    }
    cmd.stdin(if extra.stdin {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    Ok(crate::forklock::spawn(&mut cmd)?)
}

/// `core.untrackedCache=true`, for the `git add` of a save (FD-11): git remembers which
/// directories held no untracked files by their modification time, so a status in a large tree
/// stops reading every directory. The save's `add` writes the index anyway, so it is what adds
/// the cache; git's own `feature.manyFiles` turns it on the same way.
///
/// **Not for charter's reads.** Measured on git 2.50 at 300,000 files: a read-only status
/// (`--no-optional-locks`) uses a cache the index holds whether or not it is handed this `-c`,
/// never writes one back — so the cache is only as fresh as the last status that could write
/// it, the operator's own — and under `--untracked-files=all` git does not consult it at all.
/// The branch reader (gitoxide) never reads it either. What keeps an idle repo from costing a
/// status is the working-tree watch (`standings`), not this cache.
pub const UNTRACKED_CACHE: &str = "core.untrackedCache=true";

/// [`UNTRACKED_CACHE`] for `dir`, unless the operator's config already says what to do about
/// the untracked cache (`true`, `false` or `keep`): a `-c` on the command line beats every
/// config file, so it is passed only where none speaks.
pub fn untracked_cache(dir: &Path) -> Option<&'static str> {
    let asked = run(dir, &["config", "--get", "core.untrackedCache"], READ).ok()?;
    // `git config --get` exits 1 when the key is set nowhere.
    (asked.code == Some(1)).then_some(UNTRACKED_CACHE)
}

/// `args`, led by `-c` [`UNTRACKED_CACHE`] when [`untracked_cache`] says the operator's config
/// leaves it to charter: what a save's `add` runs (FD-11).
pub fn with_untracked_cache<'a>(dir: &Path, args: &[&'a str]) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = untracked_cache(dir)
        .map(|cache| vec!["-c", cache])
        .unwrap_or_default();
    out.extend_from_slice(args);
    out
}

/// `run`, counted in the test build's [`tally`]: one git process in `dir` from before it is
/// spawned until it has been waited for. In the shipped build it is `run` itself.
fn counted<T>(dir: &Path, args: &[&str], run: impl FnOnce() -> T) -> T {
    #[cfg(test)]
    let _running = tally::Running::start(dir, args);
    #[cfg(not(test))]
    let _ = (dir, args);
    run()
}

/// How many git processes the runner ran in one directory in this test process, and the most
/// that ran there at once (FD-11): what shows the pollers share one standing instead of each
/// spawning its own. Test builds only; the shipped runner counts nothing.
#[cfg(test)]
pub(crate) mod tally {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, Mutex, PoisonError};

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Tally {
        pub spawned: u64,
        pub most_at_once: u64,
    }

    /// Each directory's tally, and how many git processes run there now.
    static TALLIES: LazyLock<Mutex<HashMap<PathBuf, (Tally, u64)>>> =
        LazyLock::new(Default::default);

    /// Every argument list the runner was handed for each directory, in order.
    static ASKED: LazyLock<Mutex<HashMap<PathBuf, Vec<Vec<String>>>>> =
        LazyLock::new(Default::default);

    /// The argument lists the runner ran git with in `dir`, oldest first.
    pub fn asked(dir: &Path) -> Vec<Vec<String>> {
        ASKED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(dir)
            .cloned()
            .unwrap_or_default()
    }

    /// [`Tally`] for `dir`, as it was handed to the runner.
    pub fn of(dir: &Path) -> Tally {
        TALLIES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(dir)
            .map(|(tally, _)| *tally)
            .unwrap_or_default()
    }

    pub(super) struct Running(PathBuf);

    impl Running {
        pub(super) fn start(dir: &Path, args: &[&str]) -> Self {
            ASKED
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(dir.to_path_buf())
                .or_default()
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            let mut all = TALLIES.lock().unwrap_or_else(PoisonError::into_inner);
            let (tally, now) = all.entry(dir.to_path_buf()).or_default();
            *now += 1;
            tally.spawned += 1;
            tally.most_at_once = tally.most_at_once.max(*now);
            Self(dir.to_path_buf())
        }
    }

    impl Drop for Running {
        fn drop(&mut self) {
            let mut all = TALLIES.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some((_, now)) = all.get_mut(&self.0) {
                *now = now.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
pub(crate) use tally::of as tally;

/// Run `git -C <dir> <args>` with no deadline — for a call that checks out a tree.
pub fn run_untimed(dir: &Path, args: &[&str]) -> Result<Run, GitUnavailable> {
    let extra = match held(dir, args, Extra::default()) {
        Ok(extra) => extra,
        Err(why) => return Ok(refused(why)),
    };
    counted(dir, args, || {
        let child = spawn_with(dir, args, &extra)?;
        // `wait_with_output` reads both pipes as it waits, so it cannot deadlock on them.
        let out = child.wait_with_output()?;
        Ok(Run {
            code: out.status.code(),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    })
}

/// Run `git -C <dir> <args>` with a deadline.
///
/// **The pipes are drained on their own threads** ([`wait`]). Waiting on the child while its
/// output sits in an undrained pipe deadlocks the moment git writes past the buffer — about
/// 64 KiB, which `status --porcelain` in a large dirty clone passes easily — and the symptom
/// is a timeout that looks like a slow machine.
pub fn run(dir: &Path, args: &[&str], timeout: Duration) -> Result<Run, GitUnavailable> {
    let extra = match held(dir, args, Extra::default()) {
        Ok(extra) => extra,
        Err(why) => return Ok(refused(why)),
    };
    counted(dir, args, || {
        Ok(wait(spawn_with(dir, args, &extra)?, timeout)?)
    })
}

/// [`run`]'s hardening for a git the caller feeds and reads itself, a request at a time
/// (`cat-file --batch`): the child, with its three streams piped, or the sentence that refuses
/// the call. **The caller owns the deadline**: it stops the child with [`stop`] and waits for
/// it.
pub(crate) fn spawn_fed(
    dir: &Path,
    args: &[&str],
) -> Result<Result<Child, &'static str>, GitUnavailable> {
    let extra = Extra {
        stdin: true,
        ..Extra::default()
    };
    match held(dir, args, extra) {
        Ok(extra) => Ok(Ok(spawn_with(dir, args, &extra)?)),
        Err(why) => Ok(Err(why)),
    }
}

/// What one git call answered, as the BYTES it wrote. `code` is `None` when the deadline passed.
///
/// For a caller that must tell "git wrote something that is not UTF-8" from "git wrote U+FFFD",
/// which [`Run`]'s lossy strings cannot: Python decodes a child's output strictly and RAISES on
/// the first, so a port that decoded lossily would answer where its oracle had refused to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRun {
    pub code: Option<i32>,
    pub out: Vec<u8>,
    pub err: Vec<u8>,
}

/// Run `git -C <dir> <args>` with a deadline, as the SESSION's git would read its config.
///
/// The same hardening as [`run`] — a constructed environment, no hooks, no fsmonitor — plus
/// [`CONFIG_LOCATION_ENV`], so an alias or a setting the operator keeps in a relocated global
/// config is one this call sees too. For the plane-root guards, which ask git what a command the
/// session is about to run will do; nothing that ACTS goes through here.
pub fn run_as_session(
    dir: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<RawRun, GitUnavailable> {
    let extra = Extra {
        pass: config_location_env(|name| std::env::var_os(name)),
        ..Extra::default()
    };
    counted(dir, args, || {
        Ok(wait_raw(spawn_with(dir, args, &extra)?, timeout)?)
    })
}

/// [`run`], with `input` written to git's standard input, as BYTES back: for a `--stdin` verb,
/// so paths reach git as data — never as arguments a name could turn into an option or a
/// pathspec's magic — and `-z` answers can be split exactly.
///
/// The input is written on its own thread, so git answering before it has read all of it never
/// deadlocks on a full pipe.
pub fn run_with_input(
    dir: &Path,
    args: &[&str],
    input: Vec<u8>,
    timeout: Duration,
) -> Result<RawRun, GitUnavailable> {
    let extra = Extra {
        stdin: true,
        ..Extra::default()
    };
    let extra = match held(dir, args, extra) {
        Ok(extra) => extra,
        Err(why) => return Ok(refused_raw(why)),
    };
    counted(dir, args, || {
        let mut child = spawn_with(dir, args, &extra)?;
        if let Some(mut stdin) = child.stdin.take() {
            std::thread::spawn(move || {
                use std::io::Write as _;
                // A git that stopped reading has answered or failed; its answer says which.
                let _ = stdin.write_all(&input);
            });
        }
        Ok(wait_raw(child, timeout)?)
    })
}

/// The variables git hands a hook to say WHICH repository and which index it is working on.
///
/// `git commit -a`, `git commit <paths>` and `git commit --only` stage into a temporary index
/// and name it in `GIT_INDEX_FILE`; a linked worktree or `git --git-dir` names the repository in
/// `GIT_DIR`. A hook that asked git about "the staged diff" without them would be asked about
/// a different index than the one being committed.
pub const HOOK_REPOSITORY_ENV: [&str; 6] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

/// Run `git -C <dir> <args>` with a deadline, from inside a git hook: [`run`]'s hardening plus
/// [`HOOK_REPOSITORY_ENV`] and [`CONFIG_LOCATION_ENV`], so it reads the index and the config
/// the command that ran the hook reads. Read-only callers only.
pub fn run_in_hook(dir: &Path, args: &[&str], timeout: Duration) -> Result<RawRun, GitUnavailable> {
    let mut pass = config_location_env(|name| std::env::var_os(name));
    pass.extend(
        HOOK_REPOSITORY_ENV
            .iter()
            .filter_map(|name| std::env::var_os(name).map(|value| ((*name).to_owned(), value))),
    );
    let extra = Extra {
        pass,
        ..Extra::default()
    };
    counted(dir, args, || {
        Ok(wait_raw(spawn_with(dir, args, &extra)?, timeout)?)
    })
}

/// Run `git -C <dir> <args>` across a network, under the one-credential rule.
///
/// `helper` is the credential helper the forge's own CLI provides — `!'/abs/gh' auth
/// git-credential` — and is the ONLY helper git will consult: [`NETWORK_RULE`] resets every
/// other one first. `None` gives git no credential at all, which is what a host charter does
/// not manage gets: a token for one forge is never offered to another.
///
/// The deadline is [`NETWORK`]. A `clone` killed at the deadline leaves a partial directory,
/// which the caller reports rather than retrying into.
pub fn run_network(dir: &Path, helper: Option<&str>, args: &[&str]) -> Result<Run, GitUnavailable> {
    let mut config: Vec<String> = NETWORK_RULE.iter().map(|s| (*s).to_string()).collect();
    if let Some(helper) = helper {
        config.push(format!("credential.helper={helper}"));
    }
    let extra = Extra {
        config,
        credentials: true,
        ..Extra::default()
    };
    let extra = match held(dir, args, extra) {
        Ok(extra) => extra,
        Err(why) => return Ok(refused(why)),
    };
    counted(dir, args, || {
        Ok(wait(spawn_with(dir, args, &extra)?, NETWORK)?)
    })
}

/// Wait for `child` with a deadline, draining both pipes as it runs.
///
/// Shared with the forge CLI runner (`crate::forge`), which has the same two hazards: an
/// undrained pipe that deadlocks the wait, and a child that never answers.
pub(crate) fn wait(child: Child, timeout: Duration) -> std::io::Result<Run> {
    let raw = wait_raw(child, timeout)?;
    Ok(Run {
        code: raw.code,
        out: String::from_utf8_lossy(&raw.out).into_owned(),
        err: String::from_utf8_lossy(&raw.err).into_owned(),
    })
}

/// How long a child asked to stop has to do it before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(2);

/// Stop the child THIS call spawned, by its own pid — never by name: other chats on this
/// machine run git too.
///
/// **Asked first, killed only if it will not go** (charter-app#267). A git killed outright
/// cannot clean up: it leaves `index.lock` behind, and every git after it in that repository
/// refuses to start — the `rebase --abort` after a rebase ran out of time failed exactly so,
/// and left the plane mid-rebase. `SIGTERM` is the signal git's own lockfile cleanup is wired
/// to, so a git asked to stop removes its locks and exits; `SIGKILL` follows for one that has
/// not within [`STOP_GRACE`].
pub(crate) fn stop(child: &mut Child) {
    let asked = rustix::process::Pid::from_raw(child.id().cast_signed())
        .map(|pid| rustix::process::kill_process(pid, rustix::process::Signal::TERM).is_ok());
    if asked == Some(true) {
        let grace = Instant::now() + STOP_GRACE;
        while Instant::now() < grace {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// [`wait`], keeping the bytes.
fn wait_raw(mut child: Child, timeout: Duration) -> std::io::Result<RawRun> {
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (out_tx, out_rx) = std::sync::mpsc::channel();
    let (err_tx, err_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        let _ = out_tx.send(buf);
    });
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        let _ = err_tx.send(buf);
    });

    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait()? {
            Some(status) => break status.code(),
            None if Instant::now() >= deadline => {
                stop(&mut child);
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };

    // A child that exited is waited for in full. One that was KILLED may have left its own
    // children holding the pipes — `clone` runs `git-remote-https`, which runs the credential
    // helper — and a read that waits for every writer would then outlive the deadline it
    // was killed for. So a killed call gets a short grace for what was already written, and
    // the readers are left behind rather than waited on.
    let collect = |rx: std::sync::mpsc::Receiver<Vec<u8>>| match code {
        Some(_) => rx.recv().unwrap_or_default(),
        None => rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default(),
    };
    let out = collect(out_rx);
    let err = collect(err_rx);
    Ok(RawRun { code, out, err })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_brokered_call_reads_no_global_or_system_config_and_only_the_identity_it_is_given() {
        let dir = tempfile::tempdir().expect("a directory");
        let listed = |dir: &Path| {
            run(dir, &["config", "--list", "--show-scope"], READ)
                .expect("git runs")
                .out
        };
        let held = Isolated::identity(Some("Op Erator"), Some("op@e.invalid"));
        let seen = isolated(&held, || listed(dir.path()));
        assert!(
            seen.lines()
                .all(|line| !line.starts_with("global") && !line.starts_with("system")),
            "{seen}"
        );
        assert!(seen.contains("command\tuser.name=Op Erator"), "{seen}");
        assert!(seen.contains("command\tuser.email=op@e.invalid"), "{seen}");
        assert_eq!(isolation(), None, "the isolation outlived its call");
        assert!(
            !listed(dir.path()).contains("user.name=Op Erator"),
            "an ordinary call read the brokered identity"
        );
    }

    #[test]
    fn an_identity_that_would_break_a_line_is_left_out() {
        assert_eq!(
            Isolated::identity(Some("a\nb"), Some("")),
            Isolated::default()
        );
    }

    fn lookup_in<'a>(
        vars: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> Option<std::ffi::OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| std::ffi::OsString::from(*v))
        }
    }

    #[test]
    fn a_session_git_sees_the_config_the_session_relocated_and_nothing_else() {
        let vars = [
            ("XDG_CONFIG_HOME", "/x"),
            ("GIT_CONFIG_GLOBAL", "/g"),
            ("GIT_DIR", "/elsewhere/.git"),
            ("GIT_EXEC_PATH", "/evil"),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "alias.co"),
            ("GIT_CONFIG_VALUE_0", "checkout"),
        ];
        let got: Vec<String> = config_location_env(lookup_in(&vars))
            .into_iter()
            .map(|(k, v)| format!("{k}={}", v.to_string_lossy()))
            .collect();
        assert_eq!(
            got,
            [
                "XDG_CONFIG_HOME=/x",
                "GIT_CONFIG_GLOBAL=/g",
                "GIT_CONFIG_COUNT=1",
                "GIT_CONFIG_KEY_0=alias.co",
                "GIT_CONFIG_VALUE_0=checkout",
            ]
        );
    }

    #[test]
    fn a_config_count_naming_a_pair_that_is_not_there_is_withheld_whole() {
        let vars = [
            ("GIT_CONFIG_COUNT", "2"),
            ("GIT_CONFIG_KEY_0", "a.b"),
            ("GIT_CONFIG_VALUE_0", "c"),
        ];
        assert!(config_location_env(lookup_in(&vars)).is_empty());
        let vars = [("GIT_CONFIG_COUNT", "65")];
        assert!(config_location_env(lookup_in(&vars)).is_empty());
        let vars = [("GIT_CONFIG_COUNT", "x")];
        assert!(config_location_env(lookup_in(&vars)).is_empty());
    }

    #[test]
    fn a_session_git_keeps_bytes_that_are_not_utf8() {
        let dir = repo();
        let config = dir.path().join(".git").join("config");
        let mut text = std::fs::read(&config).unwrap();
        text.extend_from_slice(b"[alias]\n\tco = checkout \xff\n");
        std::fs::write(&config, text).unwrap();
        let raw = run_as_session(dir.path(), &["config", "--get", "alias.co"], READ).unwrap();
        assert_eq!(raw.code, Some(0));
        assert_eq!(raw.out, b"checkout \xff\n");
    }

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::testgit::run(dir.path(), &["init", "-q", "-b", "main", "."]);
        dir
    }

    /// The marker that tells a re-executed copy of this test binary it is the child.
    const CHILD: &str = "CHARTER_TEST_HOSTILE_CHILD";

    #[test]
    fn a_hostile_environment_does_not_reach_git_through_the_runner() {
        // `env_clear` is the load-bearing line, and asserting on `child_env` alone cannot see
        // it — a mutation that deleted `env_clear` passed every test in this module. So the
        // hostile environment is set on a CHILD PROCESS: this test re-executes the test
        // binary with it, which needs no `unsafe` (the workspace forbids it) and is sound
        // against the concurrent `getenv` that `std::env::set_var` is not.
        if std::env::var_os(CHILD).is_some() {
            // The child. Its environment is hostile in every way the parent could make it,
            // including HOME — which is where git reads a config that names programs, and is
            // the one variable the runner cannot simply drop.
            let dir = repo();
            std::fs::write(dir.path().join("f"), "x").unwrap();
            run(dir.path(), &["add", "f"], READ).unwrap();
            run(
                dir.path(),
                &[
                    "-c",
                    "user.email=t@e.invalid",
                    "-c",
                    "user.name=t",
                    "commit",
                    "-q",
                    "-m",
                    "one",
                ],
                READ,
            )
            .unwrap();

            let ran = std::env::var_os("CHARTER_TEST_HOOK_MARKER").expect("the marker path");
            let ran = std::path::PathBuf::from(ran);
            let wt = dir.path().join("wt");
            let cut = run_untimed(
                dir.path(),
                &["worktree", "add", "-b", "probe", &wt.display().to_string()],
            )
            .unwrap();

            assert!(cut.ok(), "the call still worked: {cut:?}");
            assert!(
                !ran.exists(),
                "a hook named by the hostile HOME's gitconfig ran"
            );
            // And the fsmonitor, which `status` would run on every dirt check.
            let _ = run(dir.path(), &["status", "--porcelain"], READ).unwrap();
            assert!(!ran.exists(), "the fsmonitor named by that config ran");
            return;
        }

        // The parent. It builds a hostile HOME with a gitconfig that names programs, and a
        // hostile environment, then re-executes this test inside both.
        let home = tempfile::tempdir().expect("a home");
        let hooks = home.path().join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let marker = home.path().join("RAN");
        let script = format!("#!/bin/sh\ntouch {}\n", marker.display());
        // Through `stand_in::program`: these are run the moment they are written, and a
        // program this process wrote through its own descriptor can lose to `ETXTBSY`
        // (charter-app#81).
        for hook in ["post-checkout", "query-watchman"] {
            stand_in::program(&hooks, hook, &script);
        }
        std::fs::write(
            home.path().join(".gitconfig"),
            format!(
                "[core]\n\thooksPath = {}\n\tfsmonitor = {}\n",
                hooks.display(),
                hooks.join("query-watchman").display()
            ),
        )
        .unwrap();

        let trace = std::env::temp_dir().join(format!(
            "charter-hostile-trace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let me = std::env::current_exe().expect("the test binary");
        let out = crate::forklock::output(
            Command::new(me)
                .args([
                    "--exact",
                    "worktree::git::tests::a_hostile_environment_does_not_reach_git_through_the_runner",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("CHARTER_TEST_HOOK_MARKER", &marker)
                .env("HOME", home.path())
                .env("GIT_CONFIG_COUNT", "1")
                .env("GIT_CONFIG_KEY_0", "core.hooksPath")
                .env("GIT_CONFIG_VALUE_0", &hooks)
                .env("GIT_EXEC_PATH", "/tmp/charter-test-evil-exec")
                .env("GIT_TRACE", &trace),
        )
        .expect("the test binary re-runs");

        assert!(
            out.status.success(),
            "the hostile environment reached git:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !trace.exists(),
            "GIT_TRACE reached git and it wrote {} outside the plane",
            trace.display()
        );
    }

    #[test]
    fn every_variable_git_calls_repository_local_is_absent_from_the_child() {
        // Held to GIT's own answer rather than to a list someone maintains. This is the check
        // on the allowlist, not the mechanism: the mechanism is `env_clear`.
        let dir = repo();
        let printed = run(dir.path(), &["rev-parse", "--local-env-vars"], READ).unwrap();
        assert!(!printed.out.trim().is_empty(), "git named its variables");

        for name in printed.out.split_whitespace() {
            let seen = run(dir.path(), &["config", "--get", "nothing.here"], READ);
            assert!(seen.is_ok(), "{name}");
        }
        // The real assertion: the child's whole environment, read back through git itself.
        let env = child_env("/usr/bin");
        for name in printed.out.split_whitespace() {
            assert!(
                !env.iter().any(|(k, _)| *k == name),
                "{name} is repository-local and must not be given to the child"
            );
        }
    }

    /// A `git` planted where only a relative `PATH` entry reaches it, with no git in the
    /// fixed directories — the one arrangement in which the fallback search decides.
    ///
    /// **Reached through a relative path to a temporary directory, not through `.`.** `.`
    /// and an empty entry are both the working directory, and a test cannot move the
    /// process's working directory without moving it under every other test running beside
    /// it. A relative entry is a relative entry to the search — it is resolved against the
    /// same working directory — so this plants where one reaches and puts `.` and an empty
    /// entry in front of it as well.
    #[cfg(unix)]
    #[test]
    fn a_git_that_only_a_relative_path_entry_reaches_is_never_the_one_charter_runs() {
        let fixed = tempfile::tempdir().unwrap();
        let planted = tempfile::tempdir().unwrap();
        stand_in::program(planted.path(), "git", "#!/bin/sh\nexit 0\n");
        let here = std::env::current_dir().unwrap().canonicalize().unwrap();
        let up = "../".repeat(here.components().count() - 1);
        let relative = format!(
            "{up}{}",
            planted.path().display().to_string().trim_start_matches('/')
        );
        assert!(
            Path::new(&relative).join("git").is_file(),
            "the planted git is reachable through {relative}"
        );

        let fixed_dir = fixed.path().display().to_string();
        let path = format!(".::{relative}");
        let (binary, child_path) =
            git_binary_in(&[fixed_dir.as_str()], Some(std::ffi::OsStr::new(&path)));
        assert_eq!(
            binary,
            PathBuf::from("git"),
            "a relative PATH entry chose the binary charter hands credentials to"
        );
        for dir in std::env::split_paths(&child_path) {
            assert!(dir.is_absolute(), "{} in {child_path}", dir.display());
        }
    }

    #[test]
    fn a_childs_path_splits_back_into_exactly_the_directories_it_was_built_from() {
        let dirs: Vec<String> = GIT_DIRS.iter().map(|d| (*d).to_string()).collect();
        let back: Vec<String> = std::env::split_paths(&path_value(&dirs))
            .map(|d| d.display().to_string())
            .collect();
        assert_eq!(
            back, dirs,
            "joined with this platform's separator, not a literal `:`"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_with_the_separator_in_it_is_left_out_rather_than_split_in_two() {
        let dirs = vec!["/home/a:b/.local/bin".to_string(), "/usr/bin".to_string()];
        assert_eq!(
            path_value(&dirs),
            "/usr/bin",
            "joined, it would reach the child as `/home/a` and a RELATIVE `b/.local/bin`"
        );
    }

    #[test]
    fn nothing_that_names_a_program_git_runs_is_given_to_the_child() {
        // The execution surface, which `--local-env-vars` does not cover.
        let env = child_env("/usr/bin");
        for name in [
            "GIT_EXEC_PATH",
            "GIT_TRACE",
            "GIT_TRACE2",
            "GIT_TRACE2_EVENT",
            "GIT_SSH",
            "GIT_SSH_COMMAND",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
            "GIT_PROXY_COMMAND",
            "GIT_EXTERNAL_DIFF",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG",
            "GIT_DIR",
        ] {
            assert!(
                !env.iter().any(|(k, _)| *k == name),
                "{name} must not be given to the child"
            );
        }
    }

    #[test]
    fn a_status_through_the_runner_never_takes_the_index_lock_even_without_the_flag() {
        // FD-11 (#651): `status` is the one read that writes the index — its refresh, through
        // `index.lock` — so the runner holds back git's optional locks for every `status`,
        // and a new reader that forgets `--no-optional-locks` cannot start a lock fight.
        let dir = repo();
        let index = dir.path().join(".git/index");
        for n in 0..10 {
            std::fs::write(dir.path().join(format!("{n}.txt")), "same\n").unwrap();
        }
        assert!(crate::testgit::run(dir.path(), &["add", "-A"]).ok());
        let was = std::fs::read(&index).unwrap();
        // Past a coarse clock, the same bytes again: stat data the index no longer matches.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        for n in 0..10 {
            std::fs::write(dir.path().join(format!("{n}.txt")), "same\n").unwrap();
        }

        let seen = run(
            dir.path(),
            &["-c", "core.quotePath=false", "status", "--porcelain"],
            READ,
        )
        .unwrap();

        assert!(seen.ok(), "{seen:?}");
        assert_eq!(
            std::fs::read(&index).unwrap(),
            was,
            "the index was rewritten"
        );
        // The control: git's own `status`, not through the runner and allowed its optional
        // lock, does rewrite it.
        let plain = crate::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(["status", "--porcelain"]),
        )
        .unwrap();
        assert!(plain.status.success(), "{plain:?}");
        assert_ne!(
            std::fs::read(&index).unwrap(),
            was,
            "the control saw no rewrite"
        );
    }

    #[test]
    fn the_verb_is_found_past_the_options_before_it() {
        assert_eq!(verb(&["status", "--porcelain"]), Some("status"));
        assert_eq!(
            verb(&["-c", "a=b", "--no-optional-locks", "status"]),
            Some("status")
        );
        assert_eq!(verb(&["-C", "status", "log"]), Some("log"));
        assert_eq!(verb(&["--git-dir=x", "add", "status"]), Some("add"));
        assert_eq!(verb(&["--no-pager"]), None);
    }

    #[test]
    fn output_larger_than_a_pipe_buffer_does_not_deadlock_the_deadline() {
        // ~64 KiB is where an undrained pipe blocks the writer, and a wait that cannot
        // complete becomes a timeout that looks like a slow machine.
        let dir = repo();
        // Long names on purpose: `status --porcelain` writes one line per file, and the
        // point is to pass the ~64 KiB where an undrained pipe blocks the writer.
        let long = "n".repeat(60);
        for i in 0..2000 {
            std::fs::write(dir.path().join(format!("{long}{i}")), "x").unwrap();
        }

        let seen = run(dir.path(), &["status", "--porcelain"], READ).unwrap();

        assert_eq!(seen.code, Some(0), "it finished rather than timing out");
        assert!(
            seen.out.len() > 64 * 1024,
            "and it really was past the buffer: {} bytes",
            seen.out.len()
        );
    }

    #[test]
    fn a_failed_call_carries_its_code_and_its_stderr_with_nobody_reading_the_english() {
        let dir = tempfile::tempdir().unwrap();
        let answer = run(dir.path(), &["rev-parse", "--git-dir"], READ).unwrap();
        assert_eq!(answer.code, Some(128));
        assert!(!answer.err.is_empty());
    }

    #[test]
    fn a_network_call_refuses_ssh_even_when_config_rewrites_https_to_it() {
        // The operator's own `url.<ssh>.insteadOf <https>` is the everyday way a clone of an
        // HTTPS URL ends up on SSH. Local config stands in for the global file here: `-c`
        // on the command line beats both.
        let dir = repo();
        run(
            dir.path(),
            &[
                "config",
                "url.ssh://git@127.0.0.1:9/.insteadOf",
                "https://example.invalid/",
            ],
            READ,
        )
        .unwrap();

        let fetched = run_network(
            dir.path(),
            None,
            &["fetch", "https://example.invalid/acme/x.git"],
        )
        .unwrap();

        assert!(!fetched.ok(), "{fetched:?}");
        assert!(
            fetched.err.contains("transport 'ssh' not allowed"),
            "git refused the transport, and said so: {:?}",
            fetched.err
        );
    }

    #[test]
    fn a_network_call_asks_only_the_helper_it_was_given() {
        // One credential: a helper the repo's (or the operator's) config names — a keychain,
        // a store file, another forge's CLI — is never consulted. git passes `-c` settings on
        // to a git it runs, so a `credential fill` run from inside the call sees exactly the
        // helper list the call itself would use.
        let dir = repo();
        let theirs = dir.path().join("theirs-asked");
        let mine = dir.path().join("mine-asked");
        run(
            dir.path(),
            &[
                "config",
                "credential.helper",
                &format!("!echo asked >> '{}'", theirs.display()),
            ],
            READ,
        )
        .unwrap();
        let fill = "alias.fill=!printf 'protocol=https\\nhost=example.invalid\\n\\n' | git credential fill";

        run_network(
            dir.path(),
            Some(&format!("!echo asked >> '{}'", mine.display())),
            &["-c", fill, "fill"],
        )
        .unwrap();

        assert!(mine.exists(), "the helper the call was given was asked");
        assert!(!theirs.exists(), "the helper config named was never asked");
    }

    /// The marker that tells a re-executed copy of this test binary it is the credential
    /// child.
    const CREDENTIAL_CHILD: &str = "CHARTER_TEST_CREDENTIAL_CHILD";

    #[test]
    fn a_network_call_hands_the_forge_cli_its_credential_and_no_other_call_does() {
        // git runs the credential helper with git's own environment, so a `gh` that keeps its
        // token in `GH_TOKEN` gets nothing unless the NETWORK call passes it. Every other call
        // must not: a read has no use for a token. Re-executed, because setting a variable in
        // this process is `unsafe` and would leak into every other test.
        if std::env::var_os(CREDENTIAL_CHILD).is_some() {
            let dir = repo();
            let seen = |name: &str| dir.path().join(name);
            let dump = |file: &std::path::Path| format!("alias.dump=!env > '{}'", file.display());
            let network = seen("network.env");
            let read = seen("read.env");
            run_network(dir.path(), None, &["-c", &dump(&network), "dump"]).unwrap();
            run(dir.path(), &["-c", &dump(&read), "dump"], READ).unwrap();

            let network = std::fs::read_to_string(network).expect("the alias ran");
            let read = std::fs::read_to_string(read).expect("the alias ran");
            assert!(network.contains("GH_TOKEN=tok-under-test"), "{network}");
            assert!(!read.contains("tok-under-test"), "{read}");
            assert!(
                !network.contains("CHARTER_TEST_NOT_A_CREDENTIAL"),
                "{network}"
            );
            return;
        }
        let me = std::env::current_exe().expect("the test binary");
        let out = crate::forklock::output(
            Command::new(me)
                .args([
                    "--exact",
                    "worktree::git::tests::a_network_call_hands_the_forge_cli_its_credential_and_no_other_call_does",
                    "--nocapture",
                ])
                .env(CREDENTIAL_CHILD, "1")
                .env("GH_TOKEN", "tok-under-test")
                .env("CHARTER_TEST_NOT_A_CREDENTIAL", "1"),
        )
        .expect("the test binary re-runs");
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn a_killed_call_returns_at_its_deadline_even_when_a_grandchild_holds_the_pipes() {
        // `clone` runs `git-remote-https`, which runs the credential helper: killing git does
        // not kill them, and a reader that waits for every writer would outlive the deadline.
        let dir = repo();
        let started = Instant::now();
        let answer = run(
            dir.path(),
            &["-c", "alias.hang=!sleep 30 & sleep 30", "hang"],
            Duration::from_millis(300),
        )
        .unwrap();

        assert_eq!(answer.code, None, "the deadline passed");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "it returned near its deadline, not when the grandchild let go: {:?}",
            started.elapsed()
        );
    }
}
