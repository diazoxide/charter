//! The policy compiled for Codex: charter's own Seatbelt profile around the whole harness
//! (#1123, ADR 0067 §2), the same wrap opencode runs in ([`super::seatbelt`]), with Codex's own
//! sandbox off inside it, and a Codex home of the project's own (D-88q).
//!
//! Measured against codex-cli 0.147.0 on macOS 26.2, with `sandbox-exec`, real `codex exec`
//! turns against a stand-in model server whose answers ran shell commands through Codex's own
//! tool, and the compiler's own line:
//!
//! - **Why not Codex's own sandbox.** Codex resolves the paths of its own profile again at every
//!   command, so a running chat, or another chat that can write above its folder, could move
//!   what the profile names, and no class held (ruling V87f, measured). Charter's profile is
//!   written once, as the kernel names each path when the chat starts.
//! - **Why Codex's own sandbox is off inside it** (ruling V21 4). Seatbelt applies one profile
//!   per process: under charter's, Codex's own `sandbox-exec` fails with `sandbox_apply:
//!   Operation not permitted` and every command fails (measured). So Codex is handed
//!   `--sandbox danger-full-access`, last among its flags, and charter's profile holds every
//!   class for Codex and every command it runs. Measured under the wrap: what Codex's own sandbox
//!   let through in earlier rounds is refused (a folder holding a protected name moved aside,
//!   changed and moved back; the temp folder or the chat's folder swapped for a link), and what
//!   its own sandbox refused is refused too (writing outside the chat's folder and temp folder,
//!   a direct connection past the proxy).
//! - **A Codex home of the project's own** (D-88q, tightening V73 and V73a). Codex keeps state
//!   that every one of its threads shares and a later resume hands the model: each thread's
//!   goals, its queue, the threads' table and the sessions a resume reads. A chat that could
//!   write the operator's own home could plant instructions in a thread the operator later
//!   resumes outside any sandbox (measured: a planted goal reached the model on a resume). So a
//!   sandboxed project's Codex chats run with a home of their own under charter's data home
//!   ([`super::Homes::codex_project`]), which only those chats, inside this wrap, ever use, and
//!   the operator's own home is neither read nor written by them (only Codex's installed
//!   program in it is read, to run it). Charter seeds the project's home before the wrap
//!   ([`prepare`]): the operator's login, copied when the project's is missing or older, and a
//!   config charter writes itself.
//! - **What a turn writes in that home**: its sessions (a folder for each day, which a resume
//!   reads), its threads' state, history, shell snapshots (which Codex sources before a
//!   thread's commands), writer locks, log, its installation id, and its login, where a refresh
//!   lands; and its memories, goals and queue ([`SHARED_STORES`], ruling D-88s), which Codex's
//!   interactive screen opens to write at every start and does not start without (measured).
//!   A turn was measured to stop without the locks and the id. Never written: its config (MCP
//!   servers, hooks and what it trusts), its skills, plugins and helper links. No link is made
//!   in it, and no folder but a day's sessions; a folder moved in under a date-shaped name is
//!   taken out, with any link, at the next start ([`prepare`]).
//! - **The residual** (D-88s). Every one of those stores is shared by the project's sandboxed
//!   chats: one of them can change what a later sandboxed chat of the same project loads (its
//!   memories, goals and queue, its sessions, shell snapshots and history), inside this wrap
//!   only. It never reaches an unsandboxed run's Codex state: the operator's own home and other
//!   projects' homes are neither read nor written. Per-chat isolation of the shared stores is
//!   #1150.
//! - **Trust.** The config charter writes marks the chat's folder and every folder above it
//!   untrusted, so Codex asks nothing it could not save and loads no project-local config,
//!   hooks or exec policies (measured: a persisted `untrusted` starts the TUI without asking).
//!   It also trusts exactly the hooks charter arms (D-88r, [`hook_trust`]), and nothing else.
//! - **What Codex still asks.** Its approval policy rejects what it would ask to run outside a
//!   sandbox, rather than asking, and its live web search, which fetches from the provider's
//!   side, is off. The features that may reach past the proxy stay off until measured.
//! - **The network.** Only charter's egress proxy, and the socket the chat's hooks report on.
//!   Codex's own requests go through the proxy (`HTTPS_PROXY`, measured), and the keychain's
//!   service is not reachable, so a keyring vault is held. Codex checks a host's certificate
//!   against the system's authorities in a file ([`ROOTS_ENV`]), since asking the keychain's
//!   service is refused.
//! - **The chat's own words** can still name a flag of Codex's own; [`loosened_by`] names each
//!   one that would widen what charter hands Codex, and the chat is refused.

use std::path::{Path, PathBuf};

use super::seatbelt::{self, Own, quote, string, under};
use super::{Access, Class, Compiled, Denial, Os, Uncompilable, Unheld};
use crate::harness::Harness;

/// What a Codex chat is wrapped in, before the place it opens is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrap {
    /// Every path the chat is denied, with the keychain files and the operator's own Codex home
    /// among them.
    pub denied: Vec<Denial>,
    /// The hosts its proxy carries.
    pub hosts: Vec<String>,
    /// What its presets widen past the hosts: the package caches and the certificate check.
    pub widened: Box<super::Widened>,
    /// The folders a person let this chat write besides its own (#1342), each a root the
    /// later-code names are denied in, and each under every denial that follows it.
    pub granted: Vec<PathBuf>,
    /// The project's own Codex home (D-88q), of which only what a turn writes is writable.
    pub home: Option<PathBuf>,
    /// The operator's own Codex home, which the project's is seeded from and a chat never
    /// reaches.
    pub operator: Option<PathBuf>,
}

/// The folders in the project's Codex home a turn writes into, each everything under it and
/// never itself.
pub const HOME_DIRS: [&str; 5] = [
    "sessions",
    "archived_sessions",
    "shell_snapshots",
    "thread-writer-locks",
    "log",
];

/// The files in the project's Codex home a turn writes, besides its state databases: its
/// history, its installation id, and its login, which a refresh rewrites.
pub const HOME_FILES: [&str; 3] = ["history.jsonl", "installation_id", "auth.json"];

/// The state databases every thread of the project shares and a resume hands the model: written
/// by a chat in its own project's home only (ruling D-88s, amending D-88q), since Codex's
/// interactive screen does not start without writing them (measured). They are what one
/// sandboxed chat can leave for a later one of the same project, never for an unsandboxed run.
pub const SHARED_STORES: [&str; 3] = ["memories", "goals", "queue"];

/// The variable that names Codex's home.
pub const HOME_ENV: &str = "CODEX_HOME";

/// What in the operator's own Codex home a wrapped chat may still read: Codex's installed
/// program, which the chat runs.
pub const PROGRAMS: &str = "packages";

/// `compiled`, for Codex, or why it cannot be wrapped on this system. The Linux wrap is #1040.
pub fn wrap(compiled: &Compiled) -> Result<Wrap, Uncompilable> {
    if compiled.os != Os::MacOs {
        return Err(Uncompilable {
            harness: Harness::Codex,
            unheld: Unheld::Wrap(compiled.os),
        });
    }
    let mut denied = compiled.denied.paths.clone();
    denied.extend(seatbelt::keychains(compiled.homes.home.as_deref()));
    // D-88q: the operator's own home, where every thread the operator resumes keeps its state.
    if let Some(operator) = &compiled.homes.codex {
        denied.push(Denial {
            class: Class::LaterCode,
            path: operator.clone(),
            access: Access::ReadWrite,
            named: None,
        });
    }
    Ok(Wrap {
        denied,
        hosts: compiled.hosts.clone(),
        widened: Box::new(compiled.widened.clone()),
        granted: compiled.writable.clone(),
        home: compiled.homes.codex_project.clone(),
        operator: compiled.homes.codex.clone(),
    })
}

impl Wrap {
    /// What the wrap lets a chat write in the project's Codex home, besides its state databases.
    pub fn writable(&self) -> Vec<PathBuf> {
        let Some(home) = &self.home else {
            return Vec::new();
        };
        HOME_DIRS
            .iter()
            .chain(HOME_FILES.iter())
            .map(|name| home.join(name))
            .collect()
    }
}

/// The project's Codex home, made ready before the wrap for a chat in `cwd` armed with
/// `armed`, which lets a chat make none of it, and only once the chat's line is built:
/// - its folders, empty, where missing, and the home itself readable by its owner alone;
/// - in its folders, every link and every folder that is not a day's sessions taken out, so
///   nothing a chat moved in under a date-shaped name stays;
/// - the operator's login, copied where the project's is missing, older, dated in the future,
///   or of another account ([`seed_login`]);
/// - its config, written by charter alone: `cwd` and every folder above it untrusted, and
///   trust for exactly the hooks charter armed ([`hook_trust`]).
///
/// What cannot be made is left to Codex, which then stops.
pub fn prepare(wrap: &Wrap, cwd: &Path, armed: &[String]) {
    let Some(home) = &wrap.home else { return };
    for dir in HOME_DIRS {
        let _ = std::fs::create_dir_all(home.join(dir));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700));
    }
    for dir in HOME_DIRS {
        let depth = if DAY_DIRS.contains(&dir) { 3 } else { 0 };
        tidy(&home.join(dir), depth);
    }
    if let Some(operator) = &wrap.operator {
        seed_login(&operator.join("auth.json"), &home.join("auth.json"));
    }
    write_config(&home.join("config.toml"), cwd, hook_trust(armed));
}

/// The folders of the project's Codex home that hold a folder for each day.
const DAY_DIRS: [&str; 2] = ["sessions", "archived_sessions"];

/// Below `dir`, without following a link: every link taken out, and every folder that is not
/// `folders` deep in a day's tree (`YYYY/MM/DD`), with all it holds; a regular file stays.
fn tidy(dir: &Path, folders: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let width = [4, 2, 2];
    let level = 3 - folders.min(3);
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let dated = folders > 0
            && name.to_str().is_some_and(|name| {
                name.len() == width[level] && name.bytes().all(|b| b.is_ascii_digit())
            });
        if kind.is_symlink() {
            let _ = std::fs::remove_file(&path);
        } else if kind.is_dir() {
            if dated {
                tidy(&path, folders - 1);
            } else {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

/// Who a Codex login at `path` signs in as: its API key, or the identity the claims of its
/// tokens name (review R2-F1), or `None` where it says neither or a token does not decode, which
/// reseeds the project's login (fail closed).
///
/// The claims are decoded, never verified: what counts is that the tokens are the same account's
/// as the operator's. A plain field beside them (`tokens.account_id`) is not asked, since a chat
/// that writes the project's login could copy the operator's into a file holding another
/// account's tokens.
fn account_of(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let login: serde_json::Value = serde_json::from_str(&text).ok()?;
    if let Some(key) = login
        .get("OPENAI_API_KEY")
        .and_then(serde_json::Value::as_str)
        .filter(|key| !key.is_empty())
    {
        return Some(format!("key:{key}"));
    }
    let tokens = login.get("tokens")?;
    let mut identity = vec![format!(
        "access:{}",
        claimed(tokens.get("access_token")?.as_str()?)?
    )];
    if let Some(id) = tokens.get("id_token").filter(|id| !id.is_null()) {
        identity.push(format!("id:{}", claimed(id.as_str()?)?));
    }
    Some(identity.join(" "))
}

/// The identity a token's claims name: its subject, email and every `*_account_id` or
/// `*_user_id` claim at its top level or in a namespaced object (OpenAI's tokens keep the
/// ChatGPT account and user under `https://api.openai.com/auth`), as one string; `None` where
/// the token is not a JWT whose claims decode or name no one.
fn claimed(token: &str) -> Option<String> {
    let mut parts = token.split('.');
    let (_, payload, _) = (parts.next()?, parts.next()?, parts.next()?);
    let claims: serde_json::Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    let claims = claims.as_object()?;
    let mut named: Vec<String> = Vec::new();
    let mut take = |scope: &str, map: &serde_json::Map<String, serde_json::Value>| {
        for (key, value) in map {
            let names = key == "sub"
                || key == "email"
                || key.ends_with("_account_id")
                || key.ends_with("_user_id");
            if let (true, Some(value)) = (names, value.as_str()) {
                named.push(format!("{scope}{key}={value}"));
            }
        }
    };
    take("", claims);
    for (scope, value) in claims {
        if let Some(inner) = value.as_object() {
            take(&format!("{scope}/"), inner);
        }
    }
    named.sort();
    (!named.is_empty()).then(|| named.join(","))
}

/// `text` decoded as unpadded base64url, as a JWT's parts are written, or `None`.
fn base64url(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32)
    };
    let text = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut held) = (0u32, 0u32);
    for c in text.bytes() {
        bits = (bits << 6) | value(c)?;
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push(u8::try_from((bits >> held) & 0xff).ok()?);
        }
    }
    Some(out)
}

/// The operator's login at `from` copied to `to`, where `to` is missing, older, dated in the
/// future, or of another account than `from` (a chat may write the project's login, which a
/// refresh rewrites, and so could put another account there and date it ahead). Neither is
/// followed if it is a link: a regular file is copied over a regular file, or nothing is done.
fn seed_login(from: &Path, to: &Path) {
    let regular = |path: &Path| {
        std::fs::symlink_metadata(path)
            .ok()
            .filter(std::fs::Metadata::is_file)
    };
    let Some(source) = regular(from) else { return };
    let stale = match std::fs::symlink_metadata(to) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
        Ok(target) if target.is_file() => {
            let now = std::time::SystemTime::now();
            match (source.modified(), target.modified()) {
                (Ok(source), Ok(target)) => {
                    target < source
                        || target > now
                        || match (account_of(from), account_of(to)) {
                            (Some(ours), Some(theirs)) => ours != theirs,
                            _ => true,
                        }
                }
                _ => true,
            }
        }
        // A link, or anything else, in its place: replaced by a regular file.
        _ => true,
    };
    if !stale {
        return;
    }
    let Ok(text) = std::fs::read(from) else {
        return;
    };
    replace(to, &text, 0o600);
}

/// `bytes` put at `path` whole: written to a temp file of this start's own, made fresh and
/// never through a link, then renamed over `path`, which replaces the entry rather than writing
/// through it. Two starts at once each write their own, and the last rename wins.
fn replace(path: &Path, bytes: &[u8], mode: u32) {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static MADE: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let Some(name) = path.file_name() else { return };
    let temp = path.with_file_name(format!(
        "{}.purlis-{}-{}-{nanos}",
        name.to_string_lossy(),
        std::process::id(),
        MADE.fetch_add(1, Ordering::SeqCst)
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let Ok(mut file) = options.open(&temp) else {
        return;
    };
    if file.write_all(bytes).is_ok() && file.sync_all().is_ok() {
        let _ = std::fs::rename(&temp, path);
    } else {
        let _ = std::fs::remove_file(&temp);
    }
}

/// The source Codex names the hooks given with `-c` by, in a trust record's key.
const SESSION_FLAGS: &str = "/<session-flags>/config.toml";

/// Codex's records of trust for exactly the hooks in `armed`, charter's own `-c hooks.<Event>=…`
/// words, keyed and hashed as codex-cli 0.147.0 keys and hashes them (its `hooks` discovery):
/// `<source>:<event>:<group>:<hook>`, and `sha256:` of the compact, key-sorted JSON of the
/// event, the group's matcher where the event takes one, and the one hook with its timeout
/// normalised. A record for another hook, or a hook changed since, does not match, so nothing
/// else is trusted. Measured: a record charter computed matched the one Codex's own "Trust
/// all" wrote.
pub fn hook_trust(armed: &[String]) -> toml::Table {
    let mut state = toml::Table::new();
    let mut words = armed.iter();
    while let Some(word) = words.next() {
        if word != "-c" {
            continue;
        }
        let Some(pair) = words.next() else { break };
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let Some(event) = key.strip_prefix("hooks.").and_then(event_label) else {
            continue;
        };
        let Ok(parsed) = format!("v = {value}").parse::<toml::Table>() else {
            continue;
        };
        let Some(groups) = parsed.get("v").and_then(toml::Value::as_array) else {
            continue;
        };
        for (group_at, group) in groups.iter().enumerate() {
            let matcher = group
                .get("matcher")
                .and_then(toml::Value::as_str)
                .filter(|_| !matches!(event, "stop" | "user_prompt_submit"));
            let hooks = group.get("hooks").and_then(toml::Value::as_array);
            for (hook_at, hook) in hooks.into_iter().flatten().enumerate() {
                let Some(hash) = hook_hash(event, matcher, hook) else {
                    continue;
                };
                let mut record = toml::Table::new();
                record.insert("trusted_hash".to_owned(), toml::Value::from(hash));
                state.insert(
                    format!("{SESSION_FLAGS}:{event}:{group_at}:{hook_at}"),
                    toml::Value::Table(record),
                );
            }
        }
    }
    state
}

/// Codex's key label for the event a `-c hooks.<Event>` names.
fn event_label(event: &str) -> Option<&'static str> {
    Some(match event {
        "PreToolUse" => "pre_tool_use",
        "PermissionRequest" => "permission_request",
        "PostToolUse" => "post_tool_use",
        "PreCompact" => "pre_compact",
        "PostCompact" => "post_compact",
        "SessionStart" => "session_start",
        "SessionEnd" => "session_end",
        "UserPromptSubmit" => "user_prompt_submit",
        "SubagentStart" => "subagent_start",
        "SubagentStop" => "subagent_stop",
        "Stop" => "stop",
        _ => return None,
    })
}

/// The hash Codex keeps a command hook's trust under, or `None` for a hook that is not one
/// charter arms (only `command` hooks, without async, Windows commands or status lines).
fn hook_hash(event: &str, matcher: Option<&str>, hook: &toml::Value) -> Option<String> {
    use sha2::Digest;
    let hook = hook.as_table()?;
    if hook.get("type").and_then(toml::Value::as_str) != Some("command")
        || hook
            .keys()
            .any(|key| !["type", "command", "timeout"].contains(&key.as_str()))
    {
        return None;
    }
    let command = hook.get("command")?.as_str()?;
    let timeout = hook
        .get("timeout")
        .map(|timeout| timeout.as_integer().and_then(|it| u64::try_from(it).ok()))
        .unwrap_or(None);
    let timeout = if event == "session_end" {
        timeout.unwrap_or(1).clamp(1, 3)
    } else {
        timeout.unwrap_or(600).max(1)
    };
    // Inserted in key order, which is Codex's canonical form, whether or not the map keeps its
    // keys sorted itself.
    let mut group = serde_json::Map::new();
    group.insert("event_name".to_owned(), serde_json::Value::from(event));
    group.insert(
        "hooks".to_owned(),
        serde_json::json!([{
            "async": false,
            "command": command,
            "timeout": timeout,
            "type": "command",
        }]),
    );
    if let Some(matcher) = matcher {
        group.insert("matcher".to_owned(), serde_json::Value::from(matcher));
    }
    let text = serde_json::to_vec(&serde_json::Value::Object(group)).ok()?;
    let hex: String = sha2::Sha256::digest(&text)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Some(format!("sha256:{hex}"))
}

/// The config charter writes into the project's Codex home: the folders it marked before, as
/// charter wrote them, with `cwd` and every folder above it marked untrusted, and `trust` as
/// the hooks' trust records, replacing any before.
fn write_config(path: &Path, cwd: &Path, trust: toml::Table) {
    let mut projects = std::fs::symlink_metadata(path)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .and_then(|_| std::fs::read_to_string(path).ok())
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|mut top| top.remove("projects"))
        .and_then(|projects| match projects {
            toml::Value::Table(table) => Some(table),
            _ => None,
        })
        .unwrap_or_default();
    let real = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    for dir in real.ancestors().filter(|dir| dir.parent().is_some()) {
        let mut entry = toml::Table::new();
        entry.insert("trust_level".to_owned(), toml::Value::from("untrusted"));
        projects.insert(dir.display().to_string(), toml::Value::Table(entry));
    }
    let mut top = toml::Table::new();
    top.insert("projects".to_owned(), toml::Value::Table(projects));
    if !trust.is_empty() {
        let mut hooks = toml::Table::new();
        hooks.insert("state".to_owned(), toml::Value::Table(trust));
        top.insert("hooks".to_owned(), toml::Value::Table(hooks));
    }
    let text = format!(
        "# Written by purlis for this project's sandboxed Codex chats; rewritten at every \
         start.\n{top}"
    );
    replace(path, text.as_bytes(), 0o600);
}

/// The variable that hands Codex the certificate authorities it checks a host against, read
/// from a file rather than asked of the keychain's service, which the wrap does not let a chat
/// reach: without it, every HTTPS request Codex made under the wrap failed, and with it a listed
/// host answered through the proxy and an unlisted one was refused (measured). Authorities a
/// person added to the keychain are not among them.
pub const ROOTS_ENV: &str = "SSL_CERT_FILE";

/// The system's own certificate authorities, as macOS keeps them in a file.
pub const ROOTS: &str = "/etc/ssl/cert.pem";

/// Why a chat is not wrapped: its directory holds Codex's home, or is inside it.
pub const OVERLAP: &str = "purlis cannot wrap a Codex chat whose directory holds Codex's own \
                           files, or is inside them";

/// The Seatbelt profile for a chat in `cwd`, with its own temp directory `tmp`, reaching the
/// network through its own proxy's `proxy_ports` and reporting on `hook_socket`.
pub fn profile(
    wrap: &Wrap,
    cwd: &Path,
    tmp: &Path,
    proxy_ports: &[u16],
    hook_socket: Option<&Path>,
) -> Result<String, &'static str> {
    seatbelt::profile(
        &wrap.denied,
        &own(wrap)?,
        cwd,
        tmp,
        proxy_ports,
        hook_socket,
    )
}

/// What a wrapped Codex keeps for itself.
fn own(wrap: &Wrap) -> Result<Own, &'static str> {
    let mut own = Own {
        overlap: OVERLAP,
        ..Own::default()
    };
    seatbelt::widen(&mut own, &wrap.widened)?;
    seatbelt::granted(&mut own, &wrap.granted)?;
    own.dirs.extend(wrap.operator.iter().cloned());
    // Codex's installed program, in the operator's home, is read to be run, never written.
    if let Some(operator) = &wrap.operator {
        own.after.push(format!(
            "(allow file-read* (subpath {}))",
            quote(&operator.join(PROGRAMS))?
        ));
    }
    let Some(home) = &wrap.home else {
        return Ok(own);
    };
    own.dirs.push(home.clone());
    // No other project's Codex home is read: their sessions are their own. The folder of homes
    // itself is looked up, never listed (measured: without its metadata no path below it
    // resolves).
    if let Some(homes) = home.parent() {
        own.deny
            .push(format!("(deny file-read* (subpath {}))", quote(homes)?));
        own.after.push(format!(
            "(allow file-read-metadata (literal {}))",
            quote(homes)?
        ));
        own.after
            .push(format!("(allow file-read* (subpath {}))", quote(home)?));
    }
    own.allow.push(format!(
        "(regex {})",
        string(&under(
            home,
            "[a-z_]+_[0-9]+\\.sqlite(-wal|-shm|-journal)?$"
        ))?
    ));
    own.allow.push(format!(
        "(regex {})",
        string(&under(home, &format!("({})/.+", HOME_DIRS.join("|"))))?
    ));
    for file in HOME_FILES {
        own.allow
            .push(format!("(literal {})", quote(&home.join(file))?));
    }
    own.roots.extend(HOME_DIRS.iter().map(|dir| home.join(dir)));
    // No link made where a later Codex writes, which it would write through, and no folder
    // made but a day's sessions. A folder can still be moved in under a date-shaped name with
    // links or anything else in it (a rename is a create here): [`prepare`] takes those out
    // before the next chat starts, and nothing outside the wrap reads this home.
    own.deny.push(format!(
        "(deny file-write-create (require-all (vnode-type SYMLINK) (subpath {})))",
        quote(home)?
    ));
    own.deny.push(format!(
        "(deny file-write-create (require-all (vnode-type DIRECTORY) (subpath {}) (require-not \
         (regex {}))))",
        quote(home)?,
        string(&under(
            home,
            "(sessions|archived_sessions)/[0-9][0-9][0-9][0-9](/[0-9][0-9](/[0-9][0-9])?)?$"
        ))?
    ));
    Ok(own)
}

/// The flags that go last among a wrapped Codex's flags: its own sandbox off, since it cannot
/// be applied inside charter's (ruling V21 4); what it would ask rejected rather than asked; no
/// web search; and the features unmeasured against the wrap off.
pub fn flags() -> Vec<String> {
    let approval = toml::toml! {
        [granular]
        sandbox_approval = false
        request_permissions = false
        skill_approval = false
        rules = true
        mcp_elicitations = true
    };
    let mut args = vec!["--sandbox".to_owned(), "danger-full-access".to_owned()];
    for feature in UNMEASURED_FEATURES {
        args.push("--disable".to_owned());
        args.push(feature.to_owned());
    }
    let mut set = |key: &str, value: toml::Value| {
        args.push("-c".to_owned());
        args.push(format!("{key}={value}"));
    };
    set("approval_policy", toml::Value::Table(approval));
    set("approvals_reviewer", toml::Value::from("user"));
    set("web_search", toml::Value::from("disabled"));
    args
}

/// Features that are stable in 0.147.0 and may reach the network from Codex's own process,
/// past the proxy: unmeasured against the sandbox, so off in a sandboxed chat until they are.
/// `--disable` fails closed as `--enable` does: a Codex that does not know a feature refuses
/// to start, and one that has since removed it still starts (measured, with a feature 0.147.0
/// lists as removed).
const UNMEASURED_FEATURES: [&str; 3] = ["browser_use", "computer_use", "in_app_browser"];

/// Flags of Codex's own that would change what charter hands it, each measured on codex-cli
/// 0.147.0 or read from its source: `-s` of any value and the bypass outrank the flags charter
/// puts last, `--add-dir` and `--cd` move where it works, `--approve-for-me` routes what is
/// asked to a reviewing model, and `--search` turns the web search back on. `--enable` and
/// `--disable` because what a feature does under the wrap is unmeasured, one feature at a time.
/// The hidden aliases are Codex's own (`--yolo`, `--not-so-yolo`). Charter's wrap binds Codex
/// whatever its flags say; a chat naming one is still refused, so what Codex is handed is
/// always charter's.
const LOOSENING: [&str; 12] = [
    "-s",
    "--sandbox",
    "--dangerously-bypass-approvals-and-sandbox",
    "--yolo",
    "--add-dir",
    "-C",
    "--cd",
    "--approve-for-me",
    "--not-so-yolo",
    "--search",
    "--enable",
    "--disable",
];

/// The approval flags: they outrank the policy charter hands Codex, so any value but `never`,
/// which asks nothing and rejects what it would have asked, loosens it.
const APPROVAL: [&str; 2] = ["-a", "--ask-for-approval"];

/// The config flags.
const CONFIG: [&str; 2] = ["-c", "--config"];

/// The `-c` keys a chat's own words may set: the model and how it is shown, which no sandbox
/// reads. A key is its first dotted part. Every other key is refused, because the sandbox is
/// spread over many (`sandbox_mode`, `sandbox_workspace_write`, `profile`,
/// `default_permissions`, `permissions`, `features`, `tools`, the approval keys, `web_search`)
/// and a hook or an MCP server runs outside it.
const CONFIG_ALLOWED: [&str; 12] = [
    "model",
    "model_provider",
    "model_providers",
    "model_reasoning_effort",
    "model_reasoning_summary",
    "model_verbosity",
    "model_context_window",
    "model_auto_compact_token_limit",
    "plan_mode_reasoning_effort",
    "service_tier",
    "personality",
    "tui",
];

/// Codex 0.147.0's flags that take a value in the next word, top level and `resume` alike, so
/// that value is never read as a flag or as a first message.
const TAKES_A_VALUE: [&str; 20] = [
    "-c",
    "--config",
    "--enable",
    "--disable",
    "--remote",
    "--remote-auth-token-env",
    "-i",
    "--image",
    "-m",
    "--model",
    "--local-provider",
    "-p",
    "--profile",
    "-s",
    "--sandbox",
    "-C",
    "--cd",
    "--add-dir",
    "-a",
    "--ask-for-approval",
];

/// One word of a Codex command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Word<'a> {
    /// A flag, and its value where it takes one.
    Flag(&'a str, Option<&'a str>),
    /// The value of the flag before it.
    Value,
    /// A subcommand, a session id, or a first message.
    Positional,
}

/// `words` as Codex reads them.
///
/// A word that holds whitespace before any `=` is a message, never a flag: no flag of Codex's
/// is spelled with a space, and charter's first message is one word, whatever it starts with.
/// After `--`, every word is positional.
fn read(words: &[String]) -> Vec<Word<'_>> {
    let mut out = Vec::with_capacity(words.len());
    let mut rest = words.iter().map(String::as_str);
    let mut positional_only = false;
    while let Some(word) = rest.next() {
        if positional_only || !is_flag(word) {
            out.push(Word::Positional);
            continue;
        }
        if word == "--" {
            positional_only = true;
            out.push(Word::Positional);
            continue;
        }
        let (flag, attached) = split(word);
        if attached.is_none() && TAKES_A_VALUE.contains(&flag) {
            let value = rest.next();
            out.push(Word::Flag(flag, value));
            if value.is_some() {
                out.push(Word::Value);
            }
        } else {
            out.push(Word::Flag(flag, attached));
        }
    }
    out
}

pub(crate) fn is_flag(word: &str) -> bool {
    let head = word.split('=').next().unwrap_or(word);
    word.starts_with('-') && word.len() > 1 && !head.contains(char::is_whitespace)
}

/// `word` as a flag and the value attached to it: `--sandbox=x` and `-sx` are `-s`/`--sandbox`
/// with `x`.
pub(crate) fn split(word: &str) -> (&str, Option<&str>) {
    if word.starts_with("--") {
        return match word.split_once('=') {
            Some((flag, value)) => (flag, Some(value)),
            None => (word, None),
        };
    }
    if word.len() > 2 && word.is_char_boundary(2) {
        return (&word[..2], Some(&word[2..]));
    }
    (word, None)
}

/// The flag in `words`, one source of a Codex chat's words, that would drop or widen the
/// sandbox charter hands it — as it would be named to the operator, in backticks — or `None`.
pub fn loosened_by(words: &[String]) -> Option<String> {
    read(words).into_iter().find_map(|word| {
        let Word::Flag(flag, value) = word else {
            return None;
        };
        if CONFIG.contains(&flag) {
            let key = value?.split('=').next()?.trim();
            let root = key.split('.').next().unwrap_or(key);
            return (!CONFIG_ALLOWED.contains(&root)).then(|| format!("`-c {key}`"));
        }
        if APPROVAL.contains(&flag) {
            return (value != Some("never")).then(|| format!("`{flag}`"));
        }
        LOOSENING.contains(&flag).then(|| format!("`{flag}`"))
    })
}

/// How many of `words`, from the end, are positional: a subcommand, its session id and a first
/// message, which charter's flags must stand in front of.
pub(crate) fn positional_tail(words: &[String]) -> usize {
    read(words)
        .iter()
        .rev()
        .take_while(|word| **word == Word::Positional)
        .count()
}
