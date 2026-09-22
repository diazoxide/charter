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
#[error("charter could not run git: {0}. Install git, or put it on PATH")]
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
    let mut dirs: Vec<String> = GIT_DIRS.iter().map(|d| (*d).to_string()).collect();
    for dir in GIT_DIRS {
        let candidate = Path::new(dir).join("git");
        if candidate.is_file() {
            return (candidate, dirs.join(":"));
        }
    }
    // Searched in the PARENT, where `PATH` is still readable, and resolved to an absolute
    // path before the child is built. An attacker who controls the parent's `PATH` still
    // chooses here — but they control charter's own binary lookup too, so this adds no
    // surface that was not already there.
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("git");
            if candidate.is_file() {
                dirs.insert(0, dir.display().to_string());
                return (candidate, dirs.join(":"));
            }
        }
    }
    (PathBuf::from("git"), dirs.join(":"))
}

/// Config keys that name a program git will run, turned off where no config can re-enable
/// them. `-c` on the command line beats the system, global and repository files.
///
/// Not a general hardening list: these are the keys reachable from the verbs this module
/// runs. `core.hooksPath` covers `post-checkout` on `worktree add` and `post-merge` on
/// `merge`; `core.fsmonitor` covers `status`, which `dirt` runs on every guard.
const NO_PROGRAMS: [&str; 2] = ["core.hooksPath=/dev/null", "core.fsmonitor=false"];

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

fn spawn(dir: &Path, args: &[&str]) -> Result<Child, GitUnavailable> {
    spawn_with(dir, args, &Extra::default())
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
    cmd.arg("-C").arg(dir).args(args);
    cmd.env_clear();
    for (k, v) in child_env(&dirs) {
        cmd.env(k, v);
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
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(crate::forklock::spawn(&mut cmd)?)
}

/// Run `git -C <dir> <args>` with no deadline — for a call that checks out a tree.
pub fn run_untimed(dir: &Path, args: &[&str]) -> Result<Run, GitUnavailable> {
    let child = spawn(dir, args)?;
    // `wait_with_output` reads both pipes as it waits, so it cannot deadlock on them.
    let out = child.wait_with_output()?;
    Ok(Run {
        code: out.status.code(),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Run `git -C <dir> <args>` with a deadline.
///
/// **The pipes are drained on their own threads** ([`wait`]). Waiting on the child while its
/// output sits in an undrained pipe deadlocks the moment git writes past the buffer — about
/// 64 KiB, which `status --porcelain` in a large dirty clone passes easily — and the symptom
/// is a timeout that looks like a slow machine.
pub fn run(dir: &Path, args: &[&str], timeout: Duration) -> Result<Run, GitUnavailable> {
    Ok(wait(spawn(dir, args)?, timeout)?)
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
    Ok(wait_raw(spawn_with(dir, args, &extra)?, timeout)?)
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
    Ok(wait(spawn_with(dir, args, &extra)?, NETWORK)?)
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
                // Kill the child THIS call spawned, by its own handle. Never by name: other
                // chats on this machine run git too.
                let _ = child.kill();
                let _ = child.wait();
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
        run(dir.path(), &["init", "-q", "-b", "main", "."], READ).unwrap();
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
