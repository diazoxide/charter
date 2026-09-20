//! Whether charter's guard actually runs in the chat a profile would start — and wiring it
//! where charter can do that alone.
//!
//! A port of `charter/wiring.py` and the parts of `charter/plugincache.py` it leans on,
//! decided by [ADR 0022] and narrowed by its 2026-09-15 amendment.
//!
//! **Why this exists.** Measured on claude 2.1.268 and 2.1.276, codex-cli 0.147.0 and
//! opencode 1.18.23, in throwaway folders with no login: *a harness pointed at another
//! config folder loads none of charter's wiring.* Claude Code lists no charter plugin — not
//! even "enabled but not installed" — even in a directory whose `.claude/settings.json`
//! enables it, because the `charter` marketplace is known only to the default config folder.
//! A chat in that state looks guarded and is not, and that is the same failure whichever
//! profile started it, built-ins included.
//!
//! **Wiring is detected by ASKING the harness under the profile's own environment**, never
//! inferred from which variables the profile sets: one account is reachable through
//! variables that do and do not move the plugin. Claude Code's
//! `CLAUDE_SECURESTORAGE_CONFIG_DIR` moves the login alone, by its own binary's code.
//!
//! **Measured here, 2026-09-18, on claude 2.1.276**, because this port had to agree with a
//! real binary and not only with the Python:
//!
//! - an empty `CLAUDE_CONFIG_DIR` answers `[]` to `plugin list --json` in 211 ms, and writes
//!   `.claude.json` into that folder as it goes;
//! - `plugin marketplace add diazoxide/charter` took 20.2 s (a git clone, paced by the
//!   network) and `plugin install charter@charter --scope project` 0.8 s;
//! - a wired folder answers one row carrying `id`, `version`, `scope`, `enabled`,
//!   `installPath`, `installedAt`, `lastUpdated` and `projectPath`;
//! - and **`enabled` is resolved by the binary at the probe's own working directory**: the
//!   same row read `true` asked from the project it was installed for and `false` asked from
//!   a directory beside it. `projectPath` comes back resolved (`/private/tmp/…`) while
//!   `installPath` does not, which is why coverage compares resolved paths.
//!
//! Nothing here parses harness output to decide a session's STATE (ADR 0018). It reads one
//! `--json` inventory of what is installed, which is configuration, not conversation.
//!
//! [ADR 0022]: https://github.com/diazoxide/charter/blob/main/docs/adr/0022-a-harness-profile-belongs-to-one-machine.md

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::profiles::{self, Profile, Source};
use crate::shown;

/// The one plugin id charter recognises as its own.
pub const PLUGIN_ID: &str = "charter@charter";

/// What `claude plugin marketplace add` is given — the GitHub `<owner>/<repo>` charter is
/// published from, which is how a marketplace comes to be called `charter` at all.
pub const MARKETPLACE_SOURCE: &str = "diazoxide/charter";

/// The scope charter installs its own plugin at, and **not** `user`.
///
/// A plane pins the plugin's version rather than the binary's, so two planes on one laptop
/// can sit on different charters without fighting. A machine-global install collapses that,
/// and also puts charter's hooks into repositories nobody pointed charter at.
pub const INSTALL_SCOPE: &str = "project";

/// The scopes charter will install at, as a closed set: the value reaches an argv.
const SCOPES: [&str; 3] = ["user", "project", "local"];

/// Most specific first. Claude Code resolves `enabled` itself, so this order decides nothing
/// today; it is here for the day that stops being true, and it is the order that fails
/// CLOSED between install records — a local record that reads disabled outranks a user one
/// that reads enabled.
const SCOPE_ORDER: [&str; 3] = ["local", "project", "user"];

/// How long a probe may take before its answer is an unknown.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// How long an install step may take. The marketplace add is a git clone over the network,
/// measured at 7–20 s, so this is generous on purpose.
const INSTALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// What a refusal repeats of a detail or a fix: larger than a display budget, because both
/// name PATHS and a clipped path is one the reader cannot act on.
const SAID_LIMIT: usize = 1024;

/// What charter found when it asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// charter's guard runs in a chat started on this profile.
    Wired,
    /// It does not, and charter looked and is sure.
    Unwired,
    /// charter could not look. **Not a pass**, and never installed over.
    Unknown,
}

/// What was asked, what it answered, and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wiring {
    pub state: State,
    /// What was asked and what it answered — escaped, never clipped.
    pub detail: String,
    /// The command that would fix it; empty when wired.
    pub fix: String,
}

/// What a launch is told: whether it may go on, and what charter installed on the way.
///
/// Two fields, because "it may start" and "software went into a folder" are both things the
/// operator is owed a sentence about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    /// Why the profile may not start, or empty when it may.
    pub refusal: String,
    /// The one line a launch says when it wired the profile; empty otherwise.
    pub wired: String,
}

impl Answer {
    pub fn may_start(&self) -> bool {
        self.refusal.is_empty()
    }
}

/// A value escaped for a terminal and not clipped.
fn whole(value: &str) -> String {
    shown::readable(value, usize::MAX)
}

/// A [`Wiring`] field bounded for a sentence.
///
/// Its own clip rather than escaping again: the field is escaped already, and a second pass
/// would double every backslash the first one wrote.
pub(crate) fn said(text: &str) -> String {
    if text.len() <= SAID_LIMIT {
        text.to_owned()
    } else {
        let mut cut = SAID_LIMIT;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}...", &text[..cut])
    }
}

/// The one sentence every "wire it with charter" answer ends in.
fn install_fix(p: &Profile) -> String {
    format!("charter harness install {}", whole(&p.name))
}

/// The environment `p`'s command is exec'd with — **and the one a probe runs under**.
///
/// One function, so a probe can never ask about a session nobody is about to start. The
/// profile's own variables win over the process's, and charter's own are set last because a
/// profile may not set them at all (its declaration would be refused for trying).
pub fn environment(p: &Profile, root: &Path) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = std::env::vars().collect();
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    env.extend(profiles::expanded_env(p, &home));
    env.insert("CHARTER_ROOT".to_owned(), root.display().to_string());
    // The registry's name for the kind, never the profile's name: hooks compare this to
    // `claude-code` for session ids, resume and the working spinner, and a value of
    // `claude-work` would make each of them quietly answer "not Claude Code".
    env.insert("CHARTER_HARNESS".to_owned(), p.harness.clone());
    env.insert("CHARTER_HARNESS_PROFILE".to_owned(), p.name.clone());
    env
}

/// `argv` as an operator would paste it: `cd` first when the directory matters, then the
/// profile's variables, then the words — each shell-quoted, then escaped.
///
/// Quoted because a home with a space in it is a fix that cannot be pasted otherwise;
/// escaped after the quoting and not before, because the escape is for the terminal the line
/// is shown on and the quoting is for the shell it is pasted into. The variables as the
/// launch EXPANDS them: a quoted `~` is a directory named `~`.
fn typed(p: &Profile, argv: &[String], cwd: Option<&Path>) -> String {
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    let mut pieces = Vec::new();
    if let Some(cwd) = cwd {
        pieces.push("cd".to_owned());
        pieces.push(quote(&cwd.display().to_string()));
        pieces.push("&&".to_owned());
    }
    for (name, value) in profiles::expanded_env(p, &home) {
        pieces.push(format!("{name}={}", quote(&value)));
    }
    pieces.extend(argv.iter().map(|word| quote(word)));
    whole(&pieces.join(" "))
}

/// `shlex.quote`, as `profiles` spells it.
fn quote(word: &str) -> String {
    const SAFE: &str = "%+,-./:=@_";
    if word.is_empty() {
        return "''".to_owned();
    }
    if word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || SAFE.contains(c))
    {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', "'\"'\"'"))
}

/// Why this app will not start `p` at all — `None` when it may.
///
/// **Parsing is not launching.** [`crate::profiles`] reads all three kinds, `opencode`
/// included, because the Python charter accepts one and both implementations read one plane
/// until M4: refusing it at the PARSE would give an operator two different answers about
/// their own file. What this app will not do is start one, and that is the spec's decision
/// (decision 6, "Harnesses in v1: Claude Code and Codex. opencode follows").
///
/// Said here rather than left to fall out of a wiring check that happens not to exist. A
/// missing probe is the wrong sentence — it reads as "charter could not look" when the truth
/// is "this app does not do that yet" — and it would stop being a refusal at all the day
/// opencode's wiring is ported, silently starting chats no decision ever allowed.
///
/// Decided from the DECLARED `kind`, which is a known value
/// ([`crate::harness::Harness::of_kind`]), never from the program name, which is not.
fn not_startable(p: &Profile) -> Option<String> {
    if crate::harness::Harness::of_kind(&p.kind).is_some() {
        return None;
    }
    Some(format!(
        "profile '{}' runs {}, which this app does not start — charter-app v1 starts Claude \
         Code and Codex, and opencode follows. The Python charter on this same plane still \
         starts it: charter {}.",
        shown::short(&p.name),
        shown::short(&p.kind),
        whole(&p.name)
    ))
}

/// Why charter may not run `p`'s command to ask it anything — `None` when it may.
///
/// **The same two checks a launch makes before its own, in the same order**: a probe IS a
/// run of the profile's command. A gate each caller has to remember is a gate one of them
/// will not, and the thing it lets through is a command out of a file a chat can write.
///
/// The ignore check first: an approved profile in a file git would commit is still a
/// declaration charter has refused, and a record saying the operator once approved it says
/// nothing about the file it now sits in. Built-ins skip both — their command is charter's
/// own, out of the registry.
fn not_asked(p: &Profile, root: &Path) -> Option<(String, String)> {
    if p.source == Source::BuiltIn {
        return None;
    }
    let name = shown::short(&p.name);
    let check = profiles::ignore_check(root);
    if !check.passes() {
        return Some((
            format!(
                "profile '{name}' is declared in a file charter has refused ({}) — nothing \
                 was started.",
                check.reason
            ),
            check.fix,
        ));
    }
    crate::profiletrust::approval_needed(root, p).map(|state| {
        (
            format!(
                "profile '{name}' is {}, and nobody has approved it — nothing was started. \
                 Approve it once so charter can run it.",
                state.as_str()
            ),
            format!("charter harness install {}", whole(&p.name)),
        )
    })
}

/// Ask `p`'s harness, under `p`'s environment, whether charter's guard runs there.
///
/// `cwd` is the directory the chat would start in — Claude Code resolves `enabled` there,
/// and an install record is bound to the directory it was installed from, so this is not a
/// detail that can be defaulted.
///
/// Never cached: only one of a cache and a fresh probe may start a chat, and it is this one.
pub fn detect(p: &Profile, cwd: &Path, root: &Path) -> Wiring {
    if let Some((why, fix)) = not_asked(p, root) {
        return Wiring {
            state: State::Unknown,
            detail: why,
            fix,
        };
    }
    match p.kind.as_str() {
        "claude" => claude(p, cwd, root),
        "codex" => codex(p, root),
        // A kind this app does not start says so, rather than reporting a probe it was
        // never going to run. A kind charter has neither is still an UNKNOWN and therefore
        // a refusal, not a pass: the day a kind joins without a check, this says so.
        other => Wiring {
            state: State::Unknown,
            detail: not_startable(p)
                .unwrap_or_else(|| format!("charter has no wiring check for {}", whole(other))),
            fix: install_fix(p),
        },
    }
}

/// What a launch of `p` would install for the answer `w` — or `None` for an answer a launch
/// would not install over.
///
/// Three things have to hold, and each is a case that stays refused without it: the answer
/// is a **definite** unwired; the kind's wire can finish on its own (Codex's cannot — trust
/// is a person's, inside a session); and the fix charter named IS the install, because for a
/// plugin that is installed and disabled the install answers "present" and changes nothing,
/// so that row keeps the sentence with the fix that does.
pub fn would_install(p: &Profile, w: &Wiring) -> bool {
    p.kind == "claude" && w.state == State::Unwired && w.fix == install_fix(p)
}

/// `w` said as a refusal of `p` — empty when it is wired.
pub fn sentence(p: &Profile, w: &Wiring) -> String {
    let name = shown::short(&p.name);
    match w.state {
        State::Wired => String::new(),
        State::Unwired => format!(
            "profile '{name}' is not wired — {}, so a chat on it would run without charter's \
             guard. Wire it: {}",
            said(&w.detail),
            said(&w.fix)
        ),
        State::Unknown => format!(
            "charter could not ask {} whether profile '{name}' is wired ({}), and an unknown \
             is not a pass — nothing was started.",
            shown::short(&p.kind),
            said(&w.detail)
        ),
    }
}

/// Claude Code: `claude plugin list --json`, run as `p` would run `claude`.
///
/// Two facts out of the same answer, resolved differently (measured, see the module):
/// the ENTRIES are install records, listed whatever the working directory and each bound to
/// the directory it was installed from; `enabled` is the EFFECTIVE value for the plugin id
/// resolved at the probe's own working directory.
///
/// So the probe is given `cwd`, and charter reads no settings file of its own: a second
/// reader would answer for a different merge than the binary's, and a wrong "unwired"
/// refuses a chat that would have been guarded.
///
/// **An install covering the PLANE covers a chat in that plane's workspace.** `charter init`
/// installs at project scope for the plane root while a chat stands in `workspaces/<ws>/` —
/// a different `projectPath`. It is safe to count because of the fact above and only because
/// of it: the `enabled` on that record is what the binary resolved at `cwd`, so a plane
/// install the chat's own directory disables reads as disabled here.
fn claude(p: &Profile, cwd: &Path, root: &Path) -> Wiring {
    let env = environment(p, root);
    let folder = config_home(&env);
    let where_ = format!(
        "{} for {}",
        whole(&folder),
        whole(&cwd.display().to_string())
    );
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    let mut argv = profiles::expanded_command(p, &home);
    argv.extend(["plugin".to_owned(), "list".to_owned(), "--json".to_owned()]);

    let Some(text) = run(&argv, cwd, &env, PROBE_TIMEOUT) else {
        return Wiring {
            state: State::Unknown,
            detail: format!("claude plugin list --json could not be read in {where_}"),
            fix: install_fix(p),
        };
    };
    // "could not read" and "`--json` answered something that is not a list of rows" are the
    // same answer — nothing is known — and neither of them is "there is nothing installed".
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(&text)
    else {
        return Wiring {
            state: State::Unknown,
            detail: format!("claude plugin list --json could not be read in {where_}"),
            fix: install_fix(p),
        };
    };

    let mut ours: Vec<&serde_json::Map<String, serde_json::Value>> = Vec::new();
    for row in &rows {
        let Some(entry) = row.as_object() else {
            continue;
        };
        // Equality against a constant, which is strictly stronger than any pattern: what
        // survives it IS the constant. This is the only thing standing between charter and
        // a plugin that is not charter's to touch.
        if entry.get("id").and_then(serde_json::Value::as_str) != Some(PLUGIN_ID) {
            continue;
        }
        let scope = entry.get("scope").and_then(serde_json::Value::as_str);
        if !scope.is_some_and(|s| SCOPES.contains(&s)) {
            continue;
        }
        if covers(entry, cwd) || covers(entry, root) {
            ours.push(entry);
        }
    }
    if ours.is_empty() {
        return Wiring {
            state: State::Unwired,
            detail: format!("{PLUGIN_ID} is not installed in {where_}"),
            fix: install_fix(p),
        };
    }
    let best = ours
        .iter()
        .min_by_key(|entry| {
            entry
                .get("scope")
                .and_then(serde_json::Value::as_str)
                .and_then(|s| SCOPE_ORDER.iter().position(|o| *o == s))
                .unwrap_or(SCOPE_ORDER.len())
        })
        .expect("a non-empty list has a minimum");
    let scope = whole(
        best.get("scope")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default(),
    );
    if best.get("enabled") == Some(&serde_json::Value::Bool(true)) {
        return Wiring {
            state: State::Wired,
            detail: format!("claude plugin list: {PLUGIN_ID} enabled at {scope} scope in {where_}"),
            fix: String::new(),
        };
    }
    // NOT `charter harness install`: the install answers "present" for an install that
    // exists, so pointing back at it would print a fix that changes nothing and loops.
    //
    // `--scope local`, run FROM the chat's directory, because that is the enable measured to
    // undo every disable that reaches it: a `false` in the directory's own
    // `.claude/settings.local.json`, in its `.claude/settings.json`, and, in a git plane, in
    // the plane root's. `--scope project` and `--scope user` each exited 1 over a local
    // disable and changed nothing.
    let mut enable = profiles::expanded_command(p, &home);
    enable.extend([
        "plugin".to_owned(),
        "enable".to_owned(),
        PLUGIN_ID.to_owned(),
        "--scope".to_owned(),
        "local".to_owned(),
    ]);
    Wiring {
        state: State::Unwired,
        detail: format!(
            "{PLUGIN_ID} is installed at {scope} scope and reads as disabled in {where_}"
        ),
        fix: typed(p, &enable, Some(cwd)),
    }
}

/// Does this install of charter's plugin apply to a session rooted at `project`?
///
/// `user` scope is machine-wide and covers everything. `project` and `local` are bound to
/// the directory they were installed from, so an install belonging to somebody else's
/// checkout is **not** an answer for this plane — reading it as one is how "already
/// installed" gets printed over a plane with no plugin at all.
fn covers(entry: &serde_json::Map<String, serde_json::Value>, project: &Path) -> bool {
    if entry.get("scope").and_then(serde_json::Value::as_str) == Some("user") {
        return true;
    }
    let Some(theirs) = entry.get("projectPath").and_then(serde_json::Value::as_str) else {
        return false;
    };
    // Resolved, which is what makes `/var/…` and `/private/var/…` the same answer on macOS
    // — not a test-only concern: a plane under `/tmp` is a symlinked path on that machine.
    match (
        std::fs::canonicalize(theirs),
        std::fs::canonicalize(project),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The folder Claude Code keeps its user-level state in, resolved the way the binary
/// resolves it: `CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude")`.
///
/// `??` and not `||` — an EMPTY value is kept, and names the working directory. Not
/// `~/.claude` by assumption: the variable moves it, most often for a second account, and a
/// checker reading the default folder answers for a session that is not running.
fn config_home(env: &BTreeMap<String, String>) -> String {
    match env.get("CLAUDE_CONFIG_DIR") {
        Some(dir) => dir.clone(),
        None => profiles::home()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".claude")
            .display()
            .to_string(),
    }
}

/// Run `argv` and hand back its stdout, or `None` for every way that can fail: a program
/// that is not there, a non-zero exit, a timeout, output that is not text.
///
/// One answer for all of them on purpose. Every one means charter could not look, and the
/// caller must not be able to tell them apart into a pass.
///
/// **The pipes are drained while the program runs, not after it exits**, and that is not a
/// tidy-up: an earlier version polled for exit and read afterwards, so a harness that wrote
/// more than a pipe holds blocked on its own write, never exited, and the launch sat on the
/// full timeout before refusing a chat that was perfectly fine. A probe found it. A plugin
/// list is exactly the sort of output that grows.
fn run(
    argv: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
    timeout: std::time::Duration,
) -> Option<String> {
    let (program, rest) = argv.split_first()?;
    let mut child = std::process::Command::new(program)
        .args(rest)
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    // A reader per pipe, so neither can fill and stop the program. Both end when the
    // program closes its end, which a killed program also does.
    let drain = |pipe: Option<std::process::ChildStdout>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                use std::io::Read;
                let _ = pipe.read_to_string(&mut text);
            }
            text
        })
    };
    let out = drain(child.stdout.take());
    let errs = {
        let pipe = child.stderr.take();
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                use std::io::Read;
                let _ = pipe.read_to_string(&mut text);
            }
            text
        })
    };
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                // The readers end with the pipes the kill closed; nothing is left running.
                let _ = out.join();
                let _ = errs.join();
                return None;
            }
            Err(_) => return None,
        }
    };
    let _ = errs.join();
    let read = out.join().ok()?;
    status.success().then_some(read)
}

/// One step of an install, and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// `added`, `installed`, `present` — or `failed`.
    pub status: String,
    pub detail: String,
}

/// The statuses that are charter reporting what it DID. Everything else is a fault a launch
/// refuses on. Listed this way round so a new status fails loud rather than quiet.
const WROTE: [&str; 2] = ["added", "installed"];
const DID: [&str; 3] = ["added", "installed", "present"];

/// Wire `p` if charter can, then answer: why `p` may not start, or that it may — and what
/// was installed on the way.
///
/// **The one home for every launch path**: the picker's start, a reopen, a handoff. One
/// function, so no path wires and another refuses over the same folder.
///
/// Always a fresh probe, and the gate first: a profile charter may not run a command for is
/// told why, and nothing is asked or written for it. Then:
///
/// - **Wired** — nothing to say.
/// - **Unknown** — refused, and **never installed over**: "charter could not look" is not
///   "charter looked and the guard is absent", and installing over an unknown state is how
///   a second copy appears.
/// - **Unwired** — the kind's own wire runs, one launch at a time per profile, and the
///   folder is asked again. Wired now: the launch goes on with one line about what went
///   where. Still not: a refusal naming what the install said.
///
/// A start therefore pays two probes, and the install happens at the first one that sees
/// unwired so the second finds the folder wired.
pub fn wired_or_refusal(p: &Profile, cwd: &Path, root: &Path) -> Answer {
    // First, before any gate that would ask or run anything: a profile that can never start
    // is not worth approving, probing or installing for, and the sentence an operator wants
    // is the one about v1 rather than one about consent or a missing probe.
    if let Some(why) = not_startable(p) {
        return Answer {
            refusal: why,
            wired: String::new(),
        };
    }
    if let Some((why, _fix)) = not_asked(p, root) {
        return Answer {
            refusal: why,
            wired: String::new(),
        };
    }
    let w = detect(p, cwd, root);
    if w.state != State::Unwired {
        return Answer {
            refusal: sentence(p, &w),
            wired: String::new(),
        };
    }
    if !would_install(p, &w) {
        // A fix that is not the install: the install would answer "present" and change
        // nothing, so the refusal keeps the fix that does.
        return Answer {
            refusal: sentence(p, &w),
            wired: String::new(),
        };
    }
    // One launch at a time per profile: two launches of one unwired profile at once would
    // otherwise both clone the marketplace into the same folder.
    let did = {
        let _held = Lock::on(root, p);
        install(p, root)
    };
    let again = detect(p, cwd, root);
    if again.state == State::Wired {
        let what = format!(
            "{PLUGIN_ID} into {}",
            whole(&config_home(&environment(p, root)))
        );
        let name = shown::short(&p.name);
        // A launch whose own install found the work already done says THAT, because the
        // second of two serialised launches installed nothing.
        let line = if did.iter().any(|s| WROTE.contains(&s.status.as_str())) {
            format!("wired '{name}' — installed {what}")
        } else {
            format!("wired '{name}' — {what} was already in place")
        };
        return Answer {
            refusal: String::new(),
            wired: line,
        };
    }
    let faults: Vec<String> = did
        .iter()
        .filter(|s| !DID.contains(&s.status.as_str()))
        .map(|s| format!("{}: {}", s.status, s.detail))
        .collect();
    if !faults.is_empty() {
        return Answer {
            refusal: format!(
                "profile '{}' is not wired, and charter could not wire it — {}. Run it by \
                 hand to see the whole of what it said: {}",
                shown::short(&p.name),
                said(&faults.join("; ")),
                said(&install_fix(p))
            ),
            wired: String::new(),
        };
    }
    Answer {
        refusal: sentence(p, &again),
        wired: String::new(),
    }
}

/// Wire `p`'s own config folder: the two commands that put charter's plugin on a machine,
/// in order.
///
/// **The order is the mechanism** — installing from a marketplace that has not been added
/// fails. `-y` on the install because charter runs it without a terminal; `claude` requires
/// it when stdout is not a TTY and would otherwise refuse rather than prompt.
pub fn install(p: &Profile, root: &Path) -> Vec<Step> {
    if p.kind != "claude" {
        return vec![Step {
            status: "refused".to_owned(),
            detail: format!("charter cannot wire {} on its own", whole(&p.kind)),
        }];
    }
    let env = environment(p, root);
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    let base = profiles::expanded_command(p, &home);
    let steps: [(&str, Vec<String>); 2] = [
        (
            "added",
            [
                base.clone(),
                vec![
                    "plugin".to_owned(),
                    "marketplace".to_owned(),
                    "add".to_owned(),
                    MARKETPLACE_SOURCE.to_owned(),
                ],
            ]
            .concat(),
        ),
        (
            "installed",
            [
                base,
                vec![
                    "plugin".to_owned(),
                    "install".to_owned(),
                    PLUGIN_ID.to_owned(),
                    "--scope".to_owned(),
                    INSTALL_SCOPE.to_owned(),
                    "-y".to_owned(),
                ],
            ]
            .concat(),
        ),
    ];
    let mut done = Vec::new();
    for (status, argv) in steps {
        // The plugin goes into the folder this profile names and no other, for the chat
        // this profile is about to start — so the install runs from the PLANE, which is the
        // project `charter init` installs for.
        match run(&argv, root, &env, INSTALL_TIMEOUT) {
            Some(out) => done.push(Step {
                status: status.to_owned(),
                detail: whole(out.trim()),
            }),
            None => {
                done.push(Step {
                    status: "failed".to_owned(),
                    detail: whole(&typed(p, &argv, Some(root))),
                });
                // The order is the mechanism: an install after a marketplace that was not
                // added cannot succeed, and running it would only add a second fault.
                break;
            }
        }
    }
    done
}

/// A lock held for the length of one profile's install.
///
/// Keyed by the profile, under the plane's own state directory. A launch that cannot take
/// the lock still installs: the lock makes two concurrent installs of one profile
/// serialise, and the second then finds the first one's work in place and says so. It is
/// not a correctness barrier — `claude plugin install` is itself safe to run twice, measured
/// — so a lock charter cannot create is not a reason to refuse a chat.
struct Lock(Option<std::fs::File>);

impl Lock {
    fn on(root: &Path, p: &Profile) -> Self {
        let dir = root.join(".charter").join("locks");
        if std::fs::create_dir_all(&dir).is_err() {
            return Self(None);
        }
        // Keyed by a digest, so a profile name is never a path segment charter writes.
        let digest = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(p.name.as_bytes());
            format!("{:x}", hasher.finalize())
        };
        let path = dir.join(format!("harness-wiring-{}.lock", &digest[..16]));
        let Ok(file) = std::fs::File::create(&path) else {
            return Self(None);
        };
        #[cfg(unix)]
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive) {
            Ok(()) => Self(Some(file)),
            Err(_) => Self(None),
        }
        // No `flock` off unix, and this takes the path the doc comment above already
        // describes: an install that could not take the lock still installs, because the
        // lock only makes two concurrent installs of one profile serialise and
        // `claude plugin install` is itself safe to run twice. It is NOT a guard being
        // dropped — nothing here decides whether anything may happen. Windows has
        // `LockFileEx`, and charter-app#100 is where it goes.
        #[cfg(not(unix))]
        {
            drop(file);
            Self(None)
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(file) = self.0.take() {
            #[cfg(unix)]
            let _ = rustix::fs::flock(&file, rustix::fs::FlockOperation::Unlock);
            // Nothing was locked off unix, so nothing is unlocked; the close is the whole
            // of it, and dropping the file is what does that.
            drop(file);
        }
    }
}

/// Where Codex keeps an installed plugin, under its home.
const CODEX_PLUGIN_CACHE: &str = "plugins/cache";

/// The key prefix Codex writes its hook-trust ledger under, per plugin.
const CODEX_TRUST_PREFIX: &str = "charter@charter:hooks/hooks.json:";

/// The handler of charter's GUARD — the hook on `Bash` that refuses a command.
///
/// **A trusted hook is not a trusted guard**: Codex asks about each hook separately, and
/// neither an approved SessionStart nor an approved dispatch hook says anything about
/// whether THIS one runs before a shell command. Named by handler and not by position,
/// because Codex keys trust by `<event>:<group>:<hook>` within the installed plugin's own
/// `hooks/hooks.json`, and a position is a fact about one version of that file.
const CODEX_GUARD_HANDLER: &str = "pretooluse";

/// How a Codex plugin is installed. Printed with `CODEX_HOME=` in front of it, never run:
/// it installs software into an account folder, and running the command IS the consent.
const CODEX_COMMANDS: [[&str; 5]; 2] = [
    [
        "codex",
        "plugin",
        "marketplace",
        "add",
        "https://github.com/diazoxide/charter",
    ],
    ["codex", "plugin", "add", PLUGIN_ID, ""],
];

/// The step no command can take: Codex asks a person, in a session, to trust each hook.
const CODEX_APPROVE: &str = "start codex once and approve charter's hooks when it asks";

/// Codex's config file: `$CODEX_HOME/config.toml`, else `~/.codex/config.toml`.
///
/// There is no project-level config — a `.codex/config.toml` planted in a project is
/// ignored. **The home is the one charter can SEE**: one a wrapper script exports on its way
/// to `codex` is invisible here.
fn codex_config(env: &BTreeMap<String, String>) -> PathBuf {
    match env.get("CODEX_HOME").filter(|home| !home.is_empty()) {
        Some(home) => PathBuf::from(home),
        None => profiles::home()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".codex"),
    }
    .join("config.toml")
}

/// Codex: three marks in `$CODEX_HOME/config.toml`, and it needs all three.
///
/// The plugin declares the hooks, the policy line is the only thing that can tell a Codex
/// shell which harness it is, and **a hook Codex has not trusted is inert** — so a plugin
/// nobody approved is installed and does nothing, which reads exactly like wired to anything
/// that stops at the plugin table.
///
/// The trust rule is the measured one and deliberately weak: `trusted_hash` cannot be
/// recomputed from the plugin's `hooks.json`, and Codex writes an entry per hook LAZILY, as
/// each first fires — a wired home on this machine held 12 of the plugin's 18 keys
/// (2026-09-18), so "an entry for every key" would call it unwired. What charter can honestly
/// say is that charter's guard hook was approved in this home at least once.
fn codex(p: &Profile, root: &Path) -> Wiring {
    let env = environment(p, root);
    let path = codex_config(&env);
    let where_ = whole(&path.display().to_string());
    let fix = install_fix(p);

    let doc = match std::fs::read_to_string(&path) {
        Ok(text) => match text.parse::<toml::Table>() {
            Ok(doc) => doc,
            Err(e) => {
                return Wiring {
                    state: State::Unknown,
                    detail: format!("{where_} could not be read ({})", whole(&e.to_string())),
                    fix,
                };
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(e) => {
            return Wiring {
                state: State::Unknown,
                detail: format!("{where_} could not be read ({})", whole(&e.to_string())),
                fix,
            };
        }
    };
    let home = path.parent().unwrap_or(Path::new("."));
    let guards = match codex_guard_keys(home) {
        Ok(keys) => keys,
        Err(why) => {
            return Wiring {
                state: State::Unknown,
                detail: why,
                fix,
            };
        }
    };
    // Every level is checked for being a table before it is read, because each is a line a
    // chat can write: `plugins."charter@charter" = true` parses. Read without the check that
    // is a crash in a launch; read with it, an unknown that says which key.
    let plugin = match table(&doc, &["plugins", PLUGIN_ID]) {
        Ok(found) => found,
        Err(key) => return codex_shape(&where_, &key, fix),
    };
    let policy = match table(&doc, &["shell_environment_policy", "set"]) {
        Ok(found) => found,
        Err(key) => return codex_shape(&where_, &key, fix),
    };
    let ledger = match table(&doc, &["hooks", "state"]) {
        Ok(found) => found,
        Err(key) => return codex_shape(&where_, &key, fix),
    };

    let mut trusted = Vec::new();
    for (key, entry) in &ledger {
        // Only the guard's own entries are read: another hook's entry, in any shape, says
        // nothing about the guard.
        if !guards.contains(key) {
            continue;
        }
        let Some(entry) = entry.as_table() else {
            return codex_shape(&where_, &format!("hooks.state.\"{}\"", whole(key)), fix);
        };
        if entry
            .get("trusted_hash")
            .and_then(toml::Value::as_str)
            .is_some_and(|hash| !hash.is_empty())
        {
            trusted.push(key.clone());
        }
    }

    let named = policy.get("CHARTER_HARNESS").and_then(toml::Value::as_str) == Some("codex");
    let mut missing = Vec::new();
    if plugin.get("enabled") != Some(&toml::Value::Boolean(true)) {
        missing.push(format!("{PLUGIN_ID} is not an enabled plugin"));
    }
    if !named {
        missing.push("shell_environment_policy.set has no CHARTER_HARNESS = \"codex\"".to_owned());
    }
    if guards.is_empty() {
        missing.push(format!(
            "no copy of {PLUGIN_ID} under {} places charter's guard hook, so there is no \
             guard to trust",
            whole(&home.join(CODEX_PLUGIN_CACHE).display().to_string())
        ));
    } else if trusted.is_empty() {
        missing.push(
            "no guard hook of charter's is trusted — approve them in a codex session".to_owned(),
        );
    }
    if missing.is_empty() {
        return Wiring {
            state: State::Wired,
            detail: format!(
                "{where_}: plugin enabled, harness named, {} trusted guard hook(s)",
                trusted.len()
            ),
            fix: String::new(),
        };
    }
    let fix = if named {
        // Charter's own half is already written, so what is left is Codex's own commands and
        // a trust prompt only a person can answer. Naming `charter harness install` would
        // name the command that has already done everything it can.
        let mut steps = codex_steps(home);
        let last = steps.pop().unwrap_or_default();
        format!("{}; then {last}", steps.join("; "))
    } else if doc.contains_key("shell_environment_policy") {
        // An install answers `present` for ANY `[shell_environment_policy]` table, so
        // pointing back at it would print a fix that changes nothing.
        format!(
            "{where_} already has a [shell_environment_policy] table without charter's line, \
             and charter does not edit TOML it did not write — nothing was changed. Add this \
             line inside that table:\n  set = {{ CHARTER_HARNESS = \"codex\" }}\nor, if the \
             table already has a `set`, add CHARTER_HARNESS = \"codex\" to it."
        )
    } else {
        fix
    };
    Wiring {
        state: State::Unwired,
        detail: format!("{where_}: {}", missing.join("; ")),
        fix,
    }
}

fn codex_shape(where_: &str, key: &str, fix: String) -> Wiring {
    Wiring {
        state: State::Unknown,
        detail: format!(
            "{where_} holds {key} as something other than a table, which Codex never writes"
        ),
        fix,
    }
}

/// Walk `path` through `doc`, requiring a table at each step. `Err` names the dotted key
/// whose shape Codex never writes.
fn table(doc: &toml::Table, path: &[&str]) -> Result<toml::Table, String> {
    let mut here = doc.clone();
    let mut seen: Vec<String> = Vec::new();
    for key in path {
        seen.push(if key.contains('@') {
            format!("\"{key}\"")
        } else {
            (*key).to_owned()
        });
        match here.get(*key) {
            None => return Ok(toml::Table::new()),
            Some(toml::Value::Table(found)) => here = found.clone(),
            Some(_) => return Err(seen.join(".")),
        }
    }
    Ok(here)
}

/// The trust-ledger keys that are charter's guard in `home`, or why charter could not read
/// where the guard is.
///
/// Read out of the INSTALLED plugin's `hooks/hooks.json`, because that file is what Codex
/// numbers its keys against: charter's own `hooks.json` has moved groups between releases,
/// so a hard-coded index is right for one version and silently names the dispatch hook in
/// another.
///
/// **Every cached copy must agree**, and a key only one of them calls the guard is not one:
/// Codex can keep an older copy beside the current one and charter cannot tell which of them
/// it numbered the ledger by, so the rule is the one that fails closed. No copy at all is an
/// empty set — nothing to trust. A copy charter cannot read is an unknown, because it may be
/// the one that places the guard.
fn codex_guard_keys(home: &Path) -> Result<std::collections::BTreeSet<String>, String> {
    let root = home
        .join(CODEX_PLUGIN_CACHE)
        .join("charter")
        .join("charter");
    let versions = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(std::collections::BTreeSet::new());
        }
        Err(e) => {
            return Err(format!(
                "{} could not be read ({})",
                whole(&root.display().to_string()),
                whole(&e.to_string())
            ));
        }
    };
    let mut found: Vec<std::collections::BTreeSet<String>> = Vec::new();
    for version in versions.flatten() {
        if !version.path().is_dir() {
            continue;
        }
        let file = version.path().join("hooks").join("hooks.json");
        match std::fs::read_to_string(&file) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(doc) => found.push(guard_positions(&doc)),
                Err(e) => {
                    return Err(format!(
                        "{} could not be read ({})",
                        whole(&file.display().to_string()),
                        whole(&e.to_string())
                    ));
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                found.push(std::collections::BTreeSet::new());
            }
            Err(e) => {
                return Err(format!(
                    "{} could not be read ({})",
                    whole(&file.display().to_string()),
                    whole(&e.to_string())
                ));
            }
        }
    }
    Ok(found
        .into_iter()
        .reduce(|a, b| a.intersection(&b).cloned().collect())
        .unwrap_or_default())
}

/// Every `<prefix><event>:<group>:<hook>` in a plugin `hooks.json` whose command runs the
/// guard handler. Codex spells the event in snake case (`PreToolUse` → `pre_tool_use`,
/// measured on its own ledger). Anything not in the file's documented shape places no guard.
fn guard_positions(doc: &serde_json::Value) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let Some(events) = doc.get("hooks").and_then(serde_json::Value::as_object) else {
        return out;
    };
    for (event, groups) in events {
        let snake = snake_case(event);
        let Some(groups) = groups.as_array() else {
            continue;
        };
        for (g, group) in groups.iter().enumerate() {
            let Some(entries) = group.get("hooks").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for (h, hook) in entries.iter().enumerate() {
                let Some(command) = hook.get("command").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if hook_handlers(command)
                    .iter()
                    .any(|w| w == CODEX_GUARD_HANDLER)
                {
                    out.insert(format!("{CODEX_TRUST_PREFIX}{snake}:{g}:{h}"));
                }
            }
        }
    }
    out
}

/// `PreToolUse` → `pre_tool_use`.
fn snake_case(event: &str) -> String {
    let mut out = String::with_capacity(event.len() + 4);
    for (i, c) in event.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// Every `<name>` in `charter hook <name>` inside a command string. Only that spelling
/// counts: a manifest also runs other charter commands, and those place no guard.
fn hook_handlers(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let words: Vec<&str> = command.split_whitespace().collect();
    for window in words.windows(3) {
        if window[0].ends_with("charter")
            && window[1] == "hook"
            && !window[2].is_empty()
            && window[2]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            out.push(window[2].to_owned());
        }
    }
    out
}

/// Codex's own install steps with `CODEX_HOME=` in front of each, then the approval only a
/// person can give. A list, so no caller has to split a sentence back into steps — a home
/// whose name holds the separator would come apart in the wrong place.
fn codex_steps(home: &Path) -> Vec<String> {
    let prefix = format!("CODEX_HOME={}", quote(&home.display().to_string()));
    let mut steps: Vec<String> = CODEX_COMMANDS
        .iter()
        .map(|argv| {
            let words: Vec<String> = argv
                .iter()
                .filter(|w| !w.is_empty())
                .map(|w| quote(w))
                .collect();
            whole(&format!("{prefix} {}", words.join(" ")))
        })
        .collect();
    steps.push(CODEX_APPROVE.to_owned());
    steps
}
