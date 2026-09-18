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
fn said(text: &str) -> String {
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
        // Codex's three marks are a file read, and its hook trust is granted only inside a
        // Codex session — so it is never wired by a launch. Ported in its own pass.
        "codex" => Wiring {
            state: State::Unknown,
            detail: format!(
                "charter has no wiring check for {} in this app yet",
                whole(&p.kind)
            ),
            fix: install_fix(p),
        },
        // A kind charter has no wiring check for is an UNKNOWN and therefore a refusal, not
        // a pass: the day a kind joins without one, this is the answer that says so.
        other => Wiring {
            state: State::Unknown,
            detail: format!("charter has no wiring check for {}", whole(other)),
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
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => return None,
        }
    }
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
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
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive) {
            Ok(()) => Self(Some(file)),
            Err(_) => Self(None),
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(file) = self.0.take() {
            let _ = rustix::fs::flock(&file, rustix::fs::FlockOperation::Unlock);
        }
    }
}
