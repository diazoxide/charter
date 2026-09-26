//! The persona tool gate: let a Bash command the ACTIVE persona declares run without a prompt —
//! a port of `charter/toolgate.py`.
//!
//! [`crate::toolgate`] took that module's name for the eight REFUSALS, because in this binary
//! the refusals were what a tool call met first. This is the other half, the one that module's
//! header lists as not ported: the last thing `hooks.py:pretooluse` does, after every refusal
//! has had its say, is ask this whether the persona's `tools:` covers the command — and if it
//! does, answer `allow` so the harness does not prompt.
//!
//! # It never denies
//!
//! The worst it can do is decline, and a declined command meets the harness's ordinary
//! permission prompt. So a bug here cannot block work, only fail to smooth it. Every rule below
//! is a way to DECLINE, and each one exists because an earlier round of charter smoothed
//! something it should not have:
//!
//! - **The shell must have nothing left to do** ([`shell_literal`]). A command holding a
//!   character the shell would rewrite — `$`, `~`, `*`, `{`, `;`, `|`, `>` — is not read at all,
//!   because every rule below reads a TOKEN and a token is only the word the program gets when
//!   the shell passes it through unchanged (charter#450).
//! - **Interpreters and wrappers never** ([`INTERPRETERS`]): `bash`, `python`, `env`, `sudo`,
//!   `xargs`, `find`, `make` run whatever their arguments say.
//! - **No argument may name another program** — an executable file reachable by a path.
//! - **Destructive subcommands still prompt** ([`dangerous`]): `kubectl delete`, `git clean`,
//!   `charter secret`.
//! - **Nothing may touch charter's control surface** — the state directory, the vault
//!   directory, a registered vault file, a persona definition — by any spelling, any link or
//!   any case-folded name the filesystem would open.
//! - **The program run is the program declared**: a bare name only when the persona ships no
//!   script of that name, a path only when it is that very file.
//!
//! # The ceiling is frozen at session start
//!
//! A persona's `tools:` is a line in a committed file the session itself can edit. Read live,
//! a chat could write `tools: bash` into its own persona and be waved through on the next turn
//! (charter#432). So `SessionStart` [`snapshot`]s every persona's tools, and the gate grants
//! only what is in BOTH the live file and the snapshot: an edit can narrow a grant mid-session
//! and never widen one.
//!
//! # What is not ported
//!
//! `os.path.expanduser` and `expandvars` on a candidate: unreachable, because
//! [`shell_literal`] has already refused every command holding a `~` or a `$`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use crate::hookstate::State;
use crate::{leakguard, personagrant, pypath};

/// `_LITERAL`: the characters a shell hands a program unchanged.
fn literal(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_-./:,=+@%".contains(c)
}

/// `_DANGEROUS`: subcommands of a declared binary that still prompt.
const DANGEROUS: [(&str, &[&str]); 6] = [
    (
        "kubectl",
        &[
            "delete",
            "drain",
            "cordon",
            "uncordon",
            "taint",
            "evict",
            "replace",
            "exec",
            "attach",
            "cp",
            "port-forward",
            "proxy",
            "run",
        ],
    ),
    ("glab", &["delete", "remove"]),
    ("git", &["clean"]),
    (
        "agentmail",
        &["send", "reply", "forward", "delete", "remove"],
    ),
    ("charter", &["secret", "vault", "change"]),
    ("edm", &["secret", "vault", "change"]),
];

/// `_INTERPRETERS`: programs that run whatever their arguments name.
pub const INTERPRETERS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "mksh",
    "csh",
    "tcsh",
    "ash",
    "busybox",
    "python",
    "python2",
    "python3",
    "pypy",
    "pypy3",
    "ipython",
    "node",
    "nodejs",
    "deno",
    "bun",
    "ts-node",
    "tsx",
    "perl",
    "ruby",
    "irb",
    "php",
    "lua",
    "luajit",
    "tclsh",
    "osascript",
    "groovy",
    "scala",
    "jshell",
    "java",
    "Rscript",
    "R",
    "julia",
    "elixir",
    "iex",
    "erl",
    "escript",
    "awk",
    "gawk",
    "mawk",
    "nawk",
    "sed",
    "expect",
    "env",
    "xargs",
    "nohup",
    "setsid",
    "sudo",
    "doas",
    "su",
    "nice",
    "ionice",
    "stdbuf",
    "script",
    "chroot",
    "unshare",
    "time",
    "timeout",
    "watch",
    "command",
    "builtin",
    "exec",
    "eval",
    "parallel",
    "find",
    "make",
    "npx",
    "pnpx",
    "bunx",
    "uvx",
    "uv",
    "pipx",
    "pip",
    "pip3",
    "poetry",
    "rye",
    "deno_run",
];

fn env_assign() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*=").expect("compiles"))
}

fn versioned() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?:python|pypy|node|deno|bun|perl|ruby|php|lua|bash|sh|zsh|ksh|tclsh|julia|scala|pip|uv)[0-9]+(?:[._-][0-9]+)*$",
        )
        .expect("compiles")
    })
}

/// `_SELF_PATH_RE`. Asked of a LOWERCASED spelling, so the pattern itself is lowercase.
fn self_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\.(?:charter|edm)(?:/|$)|persona\.md|personas/\.default").expect("compiles")
    })
}

fn word() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Za-z0-9][A-Za-z0-9_-]*").expect("compiles"))
}

/// `_shell_literal`: every character is one the shell passes through unchanged — a literal,
/// a quote, a backslash, a blank, or any non-ASCII character.
pub fn shell_literal(command: &str) -> bool {
    command
        .chars()
        .all(|c| literal(c) || matches!(c, '\'' | '"' | '\\' | ' ' | '\t') || !c.is_ascii())
}

/// `_tokens`: the command's words, or `None` when it is empty, not literal, or will not split.
pub fn tokens(command: &str) -> Option<Vec<String>> {
    if command.is_empty() || !shell_literal(command) {
        return None;
    }
    let toks = crate::shellseg::posix_split(command).ok()?;
    (!toks.is_empty()).then_some(toks)
}

/// `_parse`: the program's basename, its arguments, and every token.
pub fn parse(command: &str) -> Option<(String, Vec<String>, Vec<String>)> {
    let toks = tokens(command)?;
    let i = toks.iter().take_while(|t| env_assign().is_match(t)).count();
    let prog = toks.get(i)?;
    let binary = pypath_basename(prog).to_string();
    let args = toks[i + 1..].to_vec();
    Some((binary, args, toks))
}

fn pypath_basename(path: &str) -> &str {
    crate::shellwrap::basename(path)
}

/// `_is_interpreter`.
pub fn is_interpreter(binary: &str) -> bool {
    INTERPRETERS.contains(&binary) || versioned().is_match(binary)
}

/// `_is_dangerous`: any word of any argument is one of the binary's destructive subcommands.
pub fn dangerous(binary: &str, args: &[String]) -> bool {
    let Some((_, bad)) = DANGEROUS.iter().find(|(b, _)| *b == binary) else {
        return false;
    };
    args.iter()
        .any(|tok| word().find_iter(tok).any(|w| bad.contains(&w.as_str())))
}

/// Whether `charter <args…>` runs an extension's command that charter cannot say only reads
/// (charter-app#342) — **a write, as far as a persona's grant is concerned**, so the operator's
/// prompt stays in front of it. `charter <id> <command>`, or a core-owned alias onto one, is a
/// write unless the installed extension's manifest declares that command `"writes": false`; an
/// extension that is not installed, a command it does not declare and a config home charter
/// cannot find all read as writes. A core command is not decided here ([`dangerous`] is).
pub fn writes_through_an_extension(binary: &str, args: &[String]) -> bool {
    if binary != "charter" {
        return false;
    }
    let Some((id, command)) = crate::extension::cli::extension_command(args) else {
        return false;
    };
    crate::machine::config_root_if_there()
        .is_none_or(|root| !crate::extension::cli::only_reads(&root, id, command))
}

/// `_path_candidates`: the token, and what is left after a leading `@` or after the first `=`,
/// repeatedly — `--file=@x` is three spellings of one path.
fn path_candidates(token: &str) -> Vec<String> {
    let mut out = vec![token.to_string()];
    let mut frontier = vec![token.to_string()];
    while let Some(t) = frontier.pop() {
        let at = t.strip_prefix('@').map(str::to_string).unwrap_or_default();
        let eq = t
            .split_once('=')
            .map(|(_, r)| r.to_string())
            .unwrap_or_default();
        for next in [at, eq] {
            if !next.is_empty() && !out.contains(&next) {
                out.push(next.clone());
                frontier.push(next);
            }
        }
    }
    out
}

/// `_norm`: `//` and `/./` collapsed, repeatedly, then lowercased.
fn norm(text: &str) -> String {
    static DOUBLE: OnceLock<Regex> = OnceLock::new();
    static DOT: OnceLock<Regex> = OnceLock::new();
    let double = DOUBLE.get_or_init(|| Regex::new(r"/{2,}").expect("compiles"));
    let dot = DOT.get_or_init(|| Regex::new(r"/(?:\./)+").expect("compiles"));
    let mut t = text.to_string();
    for _ in 0..4 {
        let n = dot
            .replace_all(&double.replace_all(&t, "/"), "/")
            .into_owned();
        if n == t {
            break;
        }
        t = n;
    }
    t.to_lowercase()
}

/// `(st_dev, st_ino)`, following links, or `None`.
fn ids(path: &Path) -> Option<(u64, u64)> {
    pypath::file_identity(path)
}

fn dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

/// The control surface, as the filesystem and as text.
struct Surface {
    /// `_control_roots`: the state directory, the vault directory and every registered vault
    /// file, resolved.
    roots: Vec<String>,
    /// `_control_names`: the same, as lowercased normalised spellings, both as written and
    /// resolved.
    names: Vec<String>,
    root_ids: BTreeSet<(u64, u64)>,
    /// Every root and every directory above it.
    chain_ids: BTreeSet<(u64, u64)>,
}

impl Surface {
    fn of(plane: &Path, state: &State) -> Option<Self> {
        let written: Vec<String> = [state.dir().to_path_buf(), state.vaults()]
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .chain(registered_vault_files(plane)?)
            .collect();
        let roots: Vec<String> = written.iter().map(|p| pypath::realpath(p)).collect();
        let mut names = BTreeSet::new();
        for p in &written {
            for spelling in [p.clone(), pypath::realpath(p)] {
                let n = norm(&spelling);
                if !n.trim_matches('/').is_empty() {
                    names.insert(n);
                }
            }
        }
        let root_ids = roots.iter().filter_map(|r| ids(Path::new(r))).collect();
        let mut chain_ids = BTreeSet::new();
        for root in &roots {
            let mut cur = root.clone();
            for _ in 0..64 {
                if let Some(i) = ids(Path::new(&cur)) {
                    chain_ids.insert(i);
                }
                let parent = dirname(&cur);
                // Python also stops on an empty parent. That only saves a step: an empty parent
                // becomes `cur`, `ids("")` is nothing, and `dirname("")` is `""` again, which
                // stops here. The test could not change the set, so it is not spelled (#311).
                if parent == cur {
                    break;
                }
                cur = parent;
            }
        }
        Some(Self {
            roots,
            names: names.into_iter().collect(),
            root_ids,
            chain_ids,
        })
    }

    /// `_resolves_into`: does `cand`, resolved against `base`, land ON a control root, INSIDE
    /// one, or ABOVE one (a walk from there reaches it) — by inode, and by case-folded glob
    /// match of its components against a root's.
    fn resolves_into(&self, cand: &str, base: &str) -> bool {
        let joined = if pypath::is_abs(cand) {
            cand.to_string()
        } else {
            pypath::join(base, cand)
        };
        let p = pypath::realpath(&joined);
        let mut cur = p.clone();
        for _ in 0..64 {
            if let Some(i) = ids(Path::new(&cur)) {
                let hit = if cur == p {
                    self.chain_ids.contains(&i)
                } else {
                    self.root_ids.contains(&i)
                };
                if hit {
                    return true;
                }
            }
            let parent = dirname(&cur);
            // No `|| parent.is_empty()`, for the reason `Surface::of` gives.
            if parent == cur {
                break;
            }
            cur = parent;
        }
        let cparts: Vec<&str> = p.split('/').collect();
        self.roots.iter().any(|root| {
            let rparts: Vec<&str> = root.split('/').collect();
            cparts.len() >= rparts.len()
                && cparts
                    .iter()
                    .zip(&rparts)
                    .all(|(c, r)| pypath::fnmatch(&r.to_lowercase(), &c.to_lowercase()))
        })
    }

    /// `_touches_control_surface`.
    fn touched_by(&self, tokens: &[String], base: &str) -> bool {
        tokens.iter().any(|tok| {
            let low = norm(tok);
            if self.names.iter().any(|n| low.contains(n.as_str())) {
                return true;
            }
            path_candidates(tok).iter().any(|cand| {
                let text = norm(cand);
                leakguard::vault_path_matches(&text)
                    || self_path().is_match(&text)
                    || self.resolves_into(cand, base)
            })
        })
    }
}

/// `_registered_vault_files(resolve=False)`: each vault's configured `file`, as a path —
/// `vaults.json` at the root merged under `.charter/vaults.json`. `None` when the registry holds
/// something charter cannot read as a registry, so the gate declines rather than guess which
/// files are vaults — and when a half is a link, or reached through one (#440): what it points
/// at would otherwise decide what the gate protects.
fn registered_vault_files(plane: &Path) -> Option<Vec<String>> {
    /// `Err` for a half that is not JSON; `Ok(None)` for one charter refuses to read.
    fn half(
        trust: &Path,
        path: &Path,
    ) -> Result<Option<serde_json::Map<String, serde_json::Value>>, ()> {
        let text = match crate::contain::read_text_no_link(trust, path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Some(serde_json::Map::new()));
            }
            // A link, a FIFO, a file charter may not read: nothing it can vouch for.
            Err(_) => return Ok(None),
        };
        let doc: serde_json::Value = serde_json::from_str(&text).map_err(|_| ())?;
        Ok(Some(
            doc.get("vaults")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .filter(|(_, e)| e.is_object())
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default(),
        ))
    }
    // A registry that is not JSON is `VaultError` in charter, caught by `_registered_vault_files`
    // as "no registered files" — the state and vault directories are still the surface.
    let state = State::of(plane);
    let corrupt_is_empty =
        |read: Result<Option<_>, ()>| read.unwrap_or_else(|()| Some(Default::default()));
    let shared = corrupt_is_empty(half(plane, &plane.join("vaults.json")))?;
    let local = corrupt_is_empty(half(state.trust(), &state.dir().join("vaults.json")))?;
    let mut files: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (name, entry) in shared.iter().chain(local.iter()) {
        match entry.get("config") {
            None | Some(serde_json::Value::Null) => {
                files.entry(name.clone()).or_insert(None);
            }
            Some(serde_json::Value::Object(config)) => {
                if let Some(file) = config.get("file") {
                    // A `file` that is not a string is skipped by charter; a later half with
                    // one still overrides it.
                    let file = file.as_str().filter(|f| !f.is_empty()).map(str::to_owned);
                    files.insert(name.clone(), file);
                } else {
                    files.entry(name.clone()).or_insert(None);
                }
            }
            Some(_) => return None,
        }
    }
    Some(
        files
            .into_values()
            .flatten()
            .map(|f| {
                // `vault_file_path`: `Path(f).expanduser()`, then the plane under a relative one.
                let f = expanduser(&f);
                if pypath::is_abs(&f) {
                    f
                } else {
                    plane.join(f).to_string_lossy().into_owned()
                }
            })
            .collect(),
    )
}

/// `Path.expanduser` for the one spelling a registry holds: `~` or `~/…` under `$HOME`.
fn expanduser(path: &str) -> String {
    expand_home(path, std::env::var("HOME").ok().as_deref())
}

/// [`expanduser`] with `$HOME` handed in, so the rule is asked without touching the process's
/// environment. An empty `$HOME` is no home, and the path stays as written.
fn expand_home(path: &str, home: Option<&str>) -> String {
    let home = home.filter(|h| !h.is_empty());
    if path == "~" {
        return home.unwrap_or(path).to_string();
    }
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => format!("{}/{rest}", home.trim_end_matches('/')),
        _ => path.to_string(),
    }
}

/// `_argv_names_another_program`: an argument that is a path to an executable file.
fn names_another_program(tokens: &[String], base: &str) -> bool {
    let i = tokens
        .iter()
        .take_while(|t| env_assign().is_match(t))
        .count();
    tokens
        .iter()
        .enumerate()
        .filter(|(at, _)| *at != i)
        .any(|(_, tok)| {
            path_candidates(tok).iter().any(|cand| {
                cand.contains('/') && {
                    let joined = if pypath::is_abs(cand) {
                        cand.clone()
                    } else {
                        pypath::join(base, cand)
                    };
                    personagrant::is_executable_file(Path::new(&pypath::realpath(&joined)))
                }
            })
        })
}

/// `shutil.which(binary)` over `$PATH`.
fn which(binary: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    path.split(':').find_map(|dir| {
        let candidate = if dir.is_empty() {
            PathBuf::from(binary)
        } else {
            Path::new(dir).join(binary)
        };
        personagrant::is_executable_file(&candidate).then_some(candidate)
    })
}

/// `_runs_the_declared_program`: a bare name runs the persona's program only when the persona
/// ships no script of that name; a path runs it only when it is that very file.
fn runs_the_declared_program(
    plane: &Path,
    name: &str,
    tokens: &[String],
    binary: &str,
    base: &str,
) -> bool {
    let Some(prog) = tokens.first() else {
        return false;
    };
    if env_assign().is_match(prog) {
        return false;
    }
    let owned = personagrant::bin_scripts(plane, name).remove(binary);
    if pypath_basename(prog) == prog {
        return owned.is_none();
    }
    let Some(reference) = owned.or_else(|| which(binary)) else {
        return false;
    };
    let joined = if pypath::is_abs(prog) {
        prog.clone()
    } else {
        pypath::join(base, prog)
    };
    let here = ids(Path::new(&pypath::realpath(&joined)));
    here.is_some() && here == ids(&reference)
}

/// Where the per-session ceiling lives — `<sessions>/<sid>.tools`.
pub fn ceiling_file(state: &State, sid: &str) -> PathBuf {
    state.sessions().join(format!("{sid}.tools"))
}

/// The marker that says a ceiling was TAKEN for this session — `<sessions>/<sid>.gate`.
pub fn marker_file(state: &State, sid: &str) -> PathBuf {
    state.sessions().join(format!("{sid}.gate"))
}

/// Freeze every persona's effective tools for session `sid` — `toolgate.snapshot`. Returns
/// what was frozen; empty when nothing could be stored, which later reads as "no grant".
pub fn snapshot(plane: &Path, sid: &str) -> BTreeMap<String, Vec<String>> {
    let data: BTreeMap<String, Vec<String>> = personagrant::list_personas(plane)
        .into_iter()
        .map(|n| {
            let tools = personagrant::effective_tools(plane, &n)
                .into_iter()
                .collect();
            (n, tools)
        })
        .collect();
    let state = State::of(plane);
    if state.touch(&marker_file(&state, sid)).is_err() {
        return BTreeMap::new();
    }
    let json = serde_json::to_value(&data).unwrap_or_default();
    let text = crate::pyjson::dumps_sorted(&json);
    if state
        .replace(&ceiling_file(&state, sid), text.as_bytes())
        .is_err()
    {
        return BTreeMap::new();
    }
    data
}

/// The tools session `sid` froze for `name` — `toolgate.frozen_tools`. `None` when there is no
/// session to key a ceiling on; an empty set when the ceiling is unreadable or was taken and is
/// gone, because a grant charter cannot confirm is no grant.
pub fn frozen_tools(plane: &Path, name: &str, sid: Option<&str>) -> Option<BTreeSet<String>> {
    let sid = sid?;
    let state = State::of(plane);
    // Read as it is written, never through a link (#440).
    let data: serde_json::Value = match state.read(&ceiling_file(&state, sid)) {
        Ok(bytes) => match String::from_utf8(bytes)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            Some(doc) => doc,
            None => return Some(BTreeSet::new()),
        },
        // A ceiling that is there but refused — a link, a FIFO — is one charter cannot
        // confirm, and a grant charter cannot confirm is no grant.
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Some(BTreeSet::new()),
        Err(_) => {
            // `_ceiling_was_taken`: a marker that exists, or cannot even be asked about, means
            // the ceiling was taken and has since gone — which is not permission to take it
            // again now, after the session has had turns in which to edit a persona.
            let marker = marker_file(&state, sid);
            let taken = match std::fs::symlink_metadata(&marker) {
                Ok(_) => true,
                Err(e) => e.kind() != std::io::ErrorKind::NotFound,
            };
            if taken {
                return Some(BTreeSet::new());
            }
            serde_json::to_value(snapshot(plane, sid)).unwrap_or_default()
        }
    };
    let tools = data
        .get(name)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Some(tools)
}

/// Who is asking, and where the command would run.
pub struct Ask<'a> {
    pub plane: &'a Path,
    /// The active persona, already resolved by the ladder.
    pub persona: Option<&'a str>,
    /// `session.current(payload id)`.
    pub session: Option<&'a str>,
    pub command: &'a str,
    /// The payload's `cwd`, or the process's when the payload has none.
    pub cwd: &'a str,
}

/// `toolgate.decide`: `(persona, binary)` when the command may run without a prompt.
pub fn decide(ask: &Ask) -> Option<(String, String)> {
    let name = ask.persona.filter(|n| !n.is_empty())?;
    let mut tools = personagrant::effective_tools(ask.plane, name);
    if let Some(frozen) = frozen_tools(ask.plane, name, ask.session) {
        tools = tools.intersection(&frozen).cloned().collect();
    }
    if tools.is_empty() {
        return None;
    }
    let (binary, args, tokens) = parse(ask.command)?;
    if !tools.contains(&binary) || is_interpreter(&binary) {
        return None;
    }
    if names_another_program(&tokens, ask.cwd)
        || dangerous(&binary, &args)
        || writes_through_an_extension(&binary, &args)
    {
        return None;
    }
    let surface = Surface::of(ask.plane, &State::of(ask.plane))?;
    if surface.touched_by(&tokens, ask.cwd) {
        return None;
    }
    if !runs_the_declared_program(ask.plane, name, &tokens, &binary, ask.cwd) {
        return None;
    }
    Some((name.to_string(), binary))
}

/// The JSON a `PreToolUse` hook prints for an allow — `hooks.py:pretooluse`'s, written the way
/// `json.dumps` writes it so the two implementations print the same bytes.
pub fn allowed(name: &str, binary: &str) -> String {
    crate::pyjson::dumps(
        &serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason":
                    format!("persona '{name}' declares '{binary}' in its tools"),
            }
        }),
        None,
        ", ",
        ": ",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane whose persona `ops` declares `tools`, standing at its root.
    fn plane(tools: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "").unwrap();
        let p = root.join("personas/ops");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(
            p.join("persona.md"),
            format!("---\nrole: Ops\ntools: {tools}\n---\n"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".charter/vaults")).unwrap();
        (dir, root)
    }

    fn ask(root: &Path, command: &str) -> Option<(String, String)> {
        let cwd = root.to_string_lossy().into_owned();
        decide(&Ask {
            plane: root,
            persona: Some("ops"),
            session: None,
            command,
            cwd: &cwd,
        })
    }

    #[test]
    fn a_declared_program_is_allowed() {
        let (_d, root) = plane("gh, kubectl");
        assert_eq!(
            ask(&root, "gh pr list --state open"),
            Some(("ops".into(), "gh".into()))
        );
        assert_eq!(
            allowed("ops", "gh"),
            r#"{"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow", "permissionDecisionReason": "persona 'ops' declares 'gh' in its tools"}}"#
        );
    }

    #[test]
    fn an_undeclared_program_is_not() {
        let (_d, root) = plane("gh");
        assert_eq!(ask(&root, "glab mr list"), None);
    }

    #[test]
    fn no_persona_no_grant() {
        let (_d, root) = plane("gh");
        let cwd = root.to_string_lossy().into_owned();
        let none = decide(&Ask {
            plane: &root,
            persona: None,
            session: None,
            command: "gh pr list",
            cwd: &cwd,
        });
        assert_eq!(none, None);
    }

    #[test]
    fn anything_the_shell_would_rewrite_declines() {
        let (_d, root) = plane("gh");
        for command in [
            "gh pr list | tee x",
            "gh pr list; rm -rf x",
            "gh pr view $PR",
            "gh pr list > out",
            "gh api ~/x",
            "gh pr list *",
            "gh pr list && true",
            "gh `x`",
        ] {
            assert_eq!(ask(&root, command), None, "{command}");
        }
    }

    #[test]
    fn an_interpreter_is_never_smoothed_even_when_declared() {
        let (_d, root) = plane("bash, python3.12, env");
        assert_eq!(ask(&root, "bash -c ls"), None);
        assert_eq!(ask(&root, "python3.12 x.py"), None);
        assert_eq!(ask(&root, "env gh pr list"), None);
    }

    #[test]
    fn a_destructive_subcommand_still_prompts() {
        let (_d, root) = plane("kubectl, git, charter");
        assert_eq!(ask(&root, "kubectl delete pod x"), None);
        assert_eq!(ask(&root, "git clean -fdx"), None);
        assert_eq!(ask(&root, "charter secret get x"), None);
        assert!(ask(&root, "kubectl get pods").is_some());
    }

    #[test]
    fn the_control_surface_declines_in_every_spelling() {
        let (_d, root) = plane("cat, ls, gh");
        for command in [
            "cat .charter/vaults/db.json",
            "cat .CHARTER//vaults/db.json",
            "ls .charter",
            "cat personas/ops/persona.md",
            "gh api --input=@.charter/active-persona",
            // `.` is ABOVE the state directory, so a walk from it reaches the vaults.
            "ls .",
        ] {
            assert_eq!(ask(&root, command), None, "{command}");
        }
    }

    #[test]
    fn an_env_assignment_in_front_declines() {
        let (_d, root) = plane("gh");
        assert_eq!(ask(&root, "GH_HOST=x gh pr list"), None);
    }

    #[cfg(unix)]
    #[test]
    fn an_argument_naming_an_executable_declines() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, root) = plane("gh");
        let script = root.join("run.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(ask(&root, "gh extension exec ./run.sh"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_persona_script_runs_by_its_path_and_never_by_its_bare_name() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, root) = plane("deploy");
        let bin = root.join("personas/ops/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("deploy"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(bin.join("deploy"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        // The bare word finds whatever `deploy` is first on PATH, which is not this file.
        assert_eq!(ask(&root, "deploy prod"), None);
        assert_eq!(
            ask(&root, "personas/ops/bin/deploy prod"),
            Some(("ops".into(), "deploy".into()))
        );
    }

    #[test]
    fn the_session_ceiling_narrows_and_never_widens() {
        let (_d, root) = plane("gh");
        let frozen = snapshot(&root, "s1");
        assert_eq!(frozen["ops"], vec!["gh".to_string()]);
        let cwd = root.to_string_lossy().into_owned();
        let asking = |command: &'static str| {
            decide(&Ask {
                plane: &root,
                persona: Some("ops"),
                session: Some("s1"),
                command,
                cwd: &cwd,
            })
        };
        // The session writes itself a wider grant; the ceiling holds.
        std::fs::write(
            root.join("personas/ops/persona.md"),
            "---\nrole: Ops\ntools: gh, glab\n---\n",
        )
        .unwrap();
        assert_eq!(asking("glab mr list"), None);
        assert!(asking("gh pr list").is_some());
        // A ceiling that was taken and is gone grants nothing, and is not re-taken.
        std::fs::remove_file(root.join(".charter/sessions/s1.tools")).unwrap();
        assert_eq!(asking("gh pr list"), None);
        assert!(!root.join(".charter/sessions/s1.tools").exists());
    }

    /// #440: a ceiling that is a link is never read. Whatever it points at could grant any
    /// tool; a grant charter cannot confirm is no grant, so nothing runs without a prompt.
    #[cfg(unix)]
    #[test]
    fn a_session_ceiling_that_is_a_link_grants_nothing() {
        let (_d, root) = plane("gh");
        let elsewhere = tempfile::tempdir().unwrap();
        let theirs = elsewhere.path().join("wide.tools");
        std::fs::write(&theirs, r#"{"ops": ["gh", "rm"]}"#).unwrap();
        std::fs::create_dir_all(root.join(".charter/sessions")).unwrap();
        std::os::unix::fs::symlink(&theirs, root.join(".charter/sessions/s1.tools")).unwrap();

        assert_eq!(
            frozen_tools(&root, "ops", Some("s1")),
            Some(BTreeSet::new()),
            "the linked ceiling was read as a grant"
        );
        assert_eq!(
            std::fs::read_to_string(&theirs).unwrap(),
            r#"{"ops": ["gh", "rm"]}"#
        );
    }

    #[test]
    fn a_session_with_no_ceiling_yet_takes_one_on_first_ask() {
        let (_d, root) = plane("gh");
        let cwd = root.to_string_lossy().into_owned();
        let got = decide(&Ask {
            plane: &root,
            persona: Some("ops"),
            session: Some("fresh"),
            command: "gh pr list",
            cwd: &cwd,
        });
        assert!(got.is_some());
        assert!(root.join(".charter/sessions/fresh.tools").is_file());
        assert!(root.join(".charter/sessions/fresh.gate").is_file());
    }

    #[test]
    fn a_registered_vault_file_is_on_the_surface() {
        let (_d, root) = plane("cat");
        std::fs::write(
            root.join("vaults.json"),
            r#"{"vaults": {"db": {"config": {"file": "secrets/db.enc"}}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("secrets")).unwrap();
        std::fs::write(root.join("secrets/db.enc"), "x").unwrap();
        assert_eq!(ask(&root, "cat secrets/db.enc"), None);
        assert_eq!(ask(&root, "cat SECRETS/DB.ENC"), None);
        // Named inside a longer word, where no path is peeled out of it: the spelling itself is
        // on the surface, so it still declines.
        let inside = format!("cat x:{}/secrets/db.enc", root.display());
        assert_eq!(ask(&root, &inside), None, "{inside}");
        // The registry puts that one file on the surface and nothing else in the plane.
        std::fs::write(root.join("notes.txt"), "x").unwrap();
        assert_eq!(
            ask(&root, "cat notes.txt"),
            Some(("ops".into(), "cat".into()))
        );
    }

    /// #440: a registry half that is a link is never read as the list of vault files. What
    /// it points at would decide what the gate protects, so the gate declines instead.
    #[cfg(unix)]
    #[test]
    fn a_registry_half_that_is_a_link_makes_the_gate_decline() {
        let (_d, root) = plane("cat");
        std::fs::write(root.join("notes.txt"), "x").unwrap();
        assert!(ask(&root, "cat notes.txt").is_some(), "the honest baseline");
        let elsewhere = tempfile::tempdir().unwrap();
        let theirs = elsewhere.path().join("theirs.json");
        std::fs::write(&theirs, r#"{"vaults": {}}"#).unwrap();
        for half in ["vaults.json", ".charter/vaults.json"] {
            std::os::unix::fs::symlink(&theirs, root.join(half)).unwrap();
            assert_eq!(
                ask(&root, "cat notes.txt"),
                None,
                "{half} was read through its link"
            );
            std::fs::remove_file(root.join(half)).unwrap();
        }
    }

    #[test]
    fn a_home_relative_vault_file_is_expanded_under_home_and_only_there() {
        let home = Some("/home/op");
        assert_eq!(expand_home("~/v/db.enc", home), "/home/op/v/db.enc");
        assert_eq!(
            expand_home("~/v/db.enc", Some("/home/op/")),
            "/home/op/v/db.enc"
        );
        assert_eq!(expand_home("~", home), "/home/op");
        // No home, or an empty one: the path stays as written.
        assert_eq!(expand_home("~/v/db.enc", None), "~/v/db.enc");
        assert_eq!(expand_home("~/v/db.enc", Some("")), "~/v/db.enc");
        assert_eq!(expand_home("~", Some("")), "~");
        assert_eq!(expand_home("~", None), "~");
        // Another user's home is not this rule's, nor is a `~` that is not the first character.
        assert_eq!(expand_home("~root/x", home), "~root/x");
        assert_eq!(expand_home("v/~/x", home), "v/~/x");
    }

    #[test]
    fn the_program_is_the_first_word_after_the_assignments() {
        let words = |xs: &[&str]| xs.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse("gh pr list"),
            Some((
                "gh".into(),
                words(&["pr", "list"]),
                words(&["gh", "pr", "list"])
            ))
        );
        assert_eq!(
            parse("A=1 B=2 /usr/bin/gh pr"),
            Some((
                "gh".into(),
                words(&["pr"]),
                words(&["A=1", "B=2", "/usr/bin/gh", "pr"])
            ))
        );
        assert_eq!(parse("A=1"), None);
        assert_eq!(parse(""), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_path_to_the_program_path_finds_is_that_program() {
        // A persona that ships no script of the name: the reference is whatever `$PATH` finds,
        // and a path naming that very file runs the declared program.
        let (_d, root) = plane("ls");
        let found = which("ls").expect("ls is on PATH on every unix charter's tests run on");
        let spelled = found.to_string_lossy().into_owned();
        assert_eq!(
            ask(&root, &spelled),
            Some(("ops".into(), "ls".into())),
            "{spelled}"
        );
        assert_eq!(which("charter-no-such-program-311"), None);
    }

    #[test]
    fn a_path_argument_that_is_not_an_executable_is_just_an_argument() {
        let (_d, root) = plane("gh");
        assert_eq!(
            ask(&root, "gh api repos/o/r/pulls"),
            Some(("ops".into(), "gh".into()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_executable_named_without_a_slash_is_not_a_program_the_argument_names() {
        // `_argv_names_another_program` asks only about a word with a `/` in it: a bare word is
        // looked up on PATH by whatever runs it, not in the directory the command stands in.
        use std::os::unix::fs::PermissionsExt;
        let (_d, root) = plane("gh");
        let script = root.join("run.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            ask(&root, "gh extension exec run.sh"),
            Some(("ops".into(), "gh".into()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_path_to_another_file_of_the_same_name_is_not_the_declared_program() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, root) = plane("deploy");
        for dir in ["personas/ops/bin", "elsewhere"] {
            let at = root.join(dir);
            std::fs::create_dir_all(&at).unwrap();
            std::fs::write(at.join("deploy"), "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(at.join("deploy"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        assert_eq!(ask(&root, "elsewhere/deploy prod"), None);
        assert_eq!(ask(&root, "./elsewhere/deploy prod"), None);
        assert_eq!(
            ask(&root, "./personas/ops/bin/deploy prod"),
            Some(("ops".into(), "deploy".into()))
        );
    }

    #[test]
    fn norm_collapses_separator_noise_and_case() {
        assert_eq!(norm("A//B/./C/././D"), "a/b/c/d");
    }

    #[test]
    fn path_candidates_peel_at_and_equals() {
        assert_eq!(
            path_candidates("--in=@x=y"),
            vec!["--in=@x=y", "@x=y", "x=y", "y"]
        );
    }

    /// Set on the re-run child: what [`writes_through_an_extension`] should answer there, for
    /// the extension record its `CHARTER_CONFIG_HOME` holds.
    const EXPECTED: &str = "PERSONAGATE_EXTENSION_RECORD";

    /// The child half of the test below: nothing unless it was re-run with [`EXPECTED`] set.
    #[test]
    fn an_extension_command_answers_from_the_config_home_the_child_was_given() {
        let Some(expected) = std::env::var_os(EXPECTED) else {
            return;
        };
        let words = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<_>>();
        let installed = expected == "installed";
        assert_eq!(
            writes_through_an_extension("charter", &words("probe list")),
            !installed,
            "a command the installed manifest says only reads, with the record {expected:?}"
        );
        for line in ["probe push", "probe", "other list"] {
            assert!(
                writes_through_an_extension("charter", &words(line)),
                "`charter {line}` read as one that only reads"
            );
        }
        assert!(!writes_through_an_extension("gh", &words("probe list")));
    }

    #[test]
    fn an_extension_command_is_a_write_unless_its_installed_manifest_says_it_only_reads() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let ext = top.join("probe");
        std::fs::create_dir_all(ext.join("bin")).unwrap();
        std::fs::write(ext.join("bin/probe"), "#!/bin/sh\n").unwrap();
        std::fs::write(
            ext.join(crate::extension::MANIFEST),
            r#"{"version": 2, "id": "probe", "name": "Probe", "capabilities": ["cli"],
                "contributes": {"runs": "bin/probe",
                                "cli": [{"name": "list", "title": "List", "writes": false}]}}"#,
        )
        .unwrap();
        let config = top.join("config");
        crate::extension::install(&config, &crate::extension::BuiltIn::none(), &ext).unwrap();
        let child = [
            "personagate::tests::an_extension_command_answers_from_the_config_home_the_child_was_given",
        ];

        crate::testrun::rerun(
            &child,
            &[
                (crate::machine::HOME_VAR, config.as_os_str()),
                (EXPECTED, "installed".as_ref()),
            ],
        );
        // No config home there at all: charter cannot say it only reads.
        crate::testrun::rerun(
            &child,
            &[
                (crate::machine::HOME_VAR, top.join("nowhere").as_os_str()),
                (EXPECTED, "missing".as_ref()),
            ],
        );
    }
}
