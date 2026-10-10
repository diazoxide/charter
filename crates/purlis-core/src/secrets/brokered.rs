//! A brokered `secret exec` (#1407, ADR 0067 §5 class 1): **a sandboxed chat never reads a
//! vault; the app resolves the secret and hands it to the command it runs.**
//!
//! Inside a sandboxed chat the app started, `purlis secret exec <vault> … -- <cmd>` reads
//! nothing. It sends one line over the chat's hook socket ([`Ask`], carrying the chat's token)
//! and prints what comes back ([`forwarded`]). The app then ([`serve`]):
//!
//! 1. **authorises** the vault against the persona the app recorded for the chat, never one the
//!    request names: a vault the registry tags with that persona, or one the person allowed for
//!    it on this machine ([`authorise`], D-1430-1). A chat on no persona gets no vault;
//! 2. **resolves** each value with the providers' own code, in the app's process and
//!    environment, never the chat's, so nothing the chat sets moves where a value is read from,
//!    and a provider's program is never one in the chat's folder or anywhere its sandbox lets
//!    it write ([`Ctx::chat`]);
//! 3. **runs** the command outside the chat's read rules but inside a sandbox built from what
//!    the chat's own sandbox was compiled to when it started ([`crate::sandbox::Confines`]): the
//!    same denials, its harness's own among them, the chat's folder and a temp directory of its
//!    own as the only places it writes, and the network only through an egress proxy carrying
//!    the chat's hosts. A `--file`/`--dotenv` credential is a 0600 file in the vaults directory,
//!    which every chat is denied, and the child's profile gives back a read of that one file
//!    alone. It is removed when the child ends. The child leads a process group of its own:
//!    everything it started that stayed in that group is killed when it ends, when the asker
//!    goes, and when the app lets go. A process that leaves the group (`setsid`, a daemon) is
//!    not;
//! 4. **streams** stdout and stderr back as they come, redacted across chunk boundaries
//!    ([`Redactor`]), then the exit status. A child's stdin is the asker's when that is not a
//!    terminal;
//! 5. **tells the asking chat's tab** each host the run's proxy refused because the chat's
//!    hosts do not list it, as the proxy refused it (`host:port`), as it happens ([`Told`]):
//!    the same sandbox block a chat's own command raises, so its Notice offers Allow. A client
//!    reports the proxy's `403` in its own words (kubectl's "Forbidden"), which name no host,
//!    so the chat could not raise it itself. The host comes from the proxy's record
//!    ([`crate::sandbox::egress::Refusals`]), never from what the command printed. Before the
//!    exit status, purlis says the same in its own words on stderr ([`refused_note`]), so a chat
//!    that sees only "Forbidden" can tell the person where to allow it, masked as every frame
//!    is. A refused host that carries one of the run's values is never named, told or offered:
//!    purlis says only that one was withheld ([`WITHHELD_NOTE`]). A grant reaches the next run
//!    once the chat is started again with it, which its Allow owes it.
//!
//! Every run is recorded as `secret-exec` with `brokered`, the chat and its persona; every
//! refusal is logged with the chat's number and the vault's name, never a value.
//!
//! Where no app takes the line (no socket, an app older than the line, or one with nothing that
//! answers it: [`crate::hookwire::NOTHING_ANSWERS`]) the command runs in this process as it
//! always has, **except for a vault whose values come from the keyring**, which is refused
//! instead ([`refused_in_this_chat`], #1638): read here, the Keychain would ask the person for
//! the chat. `secret get` and `cp` are refused it the same way, since the app runs only `exec`.
//!
//! **What this guarantees, and what it does not.** The command is the chat's to choose, so a
//! chat can obtain the value of ANY key of a vault tagged for its persona: `sh -c 'echo "$X" |
//! base64'`, or a copy written into its folder, hands it over, and masking catches only a value's
//! own literal bytes. Brokering keeps out of the chat what a chat could otherwise reach: the
//! vault's storage and its provider's session (the op config, the keychain, the plain file),
//! every vault the chat's persona may not use (D-1430-1), and the credential file itself, which only the
//! child may read. It is not a boundary between a chat and its own persona's secrets.

use std::ffi::OsString;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine as _;
use serde_json::Value;

use super::cmd::{self, Io, Say};
use super::exec::{self, Request, Stopped};
use super::{Ctx, Env, registry};
use crate::sandbox::{Confines, Denial};

/// The line a chat writes: which chat (checked against its token), and what it wants run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Ask {
    /// The app's number for the chat, from [`crate::hookwire::CHAT_ENV`].
    pub chat: u32,
    /// What is to be run. The field's name is what tells this line from every other kind.
    pub secret_exec: Wanted,
}

/// What a chat asks to have run: `secret exec`'s own arguments, where it runs, and the
/// environment the child starts from. **No persona, no place and no policy**: those are the
/// app's record of the chat whose token the line carries.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Wanted {
    pub vault: String,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub file: Vec<String>,
    #[serde(default)]
    pub dotenv: Vec<String>,
    /// The command, its program first.
    pub command: Vec<String>,
    /// The directory the asker runs in, which the child starts in when it is a directory.
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    /// The asker's environment, which the child's starts from.
    #[serde(default)]
    pub environment: Vec<(String, String)>,
    /// Whether the asker sends its stdin ([`Input`]). The child's is empty otherwise.
    #[serde(default)]
    pub stdin: bool,
}

/// What the app writes back, one JSON line each.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    /// The child is running: from now on the asker's stdin is wanted, when it sends one.
    Started,
    /// A line in purlis's voice, for stderr.
    Note(String),
    /// Redacted bytes of the child's stdout, base64.
    Stdout(String),
    /// Redacted bytes of the child's stderr, base64.
    Stderr(String),
    /// The child ended with this status. The last frame.
    Exit(i32),
    /// Nothing was run, with the status and the sentence. The last frame.
    Refused { why: String, code: i32 },
}

/// What the asker writes after its ask, once the child has [`Frame::Started`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    /// Bytes of the asker's stdin, base64.
    Stdin(String),
    /// The asker's stdin has ended.
    StdinClosed,
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(text: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .unwrap_or_default()
}

/// The most one frame may be, either way: a chunk of output or of stdin, base64, with room.
const A_FRAME_IS_AT_MOST: u64 = 256 * 1024;

/// The longest value of the asker's environment handed on to the child.
const A_VARIABLE_IS_AT_MOST: usize = 4 * 1024;

/// How long a child's pipes are read for after it has ended.
const PIPES_AFTER_EXIT: Duration = Duration::from_secs(2);

/// How much of a pipe is read at a time.
const A_CHUNK: usize = 16 * 1024;

// ----------------------------------------------------------------------------------------
// the asker: `purlis secret exec` inside a chat

/// The app's number for the chat this runs in, as `purlis session record` reads it: the hook
/// variable, else the session id the app sets to the same number.
fn chat_number(env: &dyn Fn(&str) -> Option<String>) -> Option<u32> {
    [crate::hookwire::CHAT_ENV, crate::active::SESSION_ID_ENV]
        .into_iter()
        .filter_map(env)
        .find_map(|n| n.trim().parse::<u32>().ok().filter(|n| *n > 0))
}

/// What would read a vault in a chat, for [`refused_in_this_chat`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    /// `secret exec`, which the app did not take.
    Exec,
    /// `secret get` or `cp`, which the app does not run for a chat.
    GetOrCp,
    /// Any other read of the keyring (`vault verify`, `secret rename`, a listing that runs the
    /// provider): the backstop where the keyring itself is read.
    Other,
}

/// What `reader` on `vault` is refused with in this process, or `None` where it may read the
/// vault itself (#1638): **a chat the app started sandboxed never reads the keyring through the
/// command.**
///
/// Only for a vault whose values come from the keyring ([`super::keyring::holds_values_of`]).
/// Read here, in the chat, each such read makes the Keychain ask the person (V90a), and an
/// "Always Allow" given to the command would let the chat read it silently from then on; the
/// Secret Service on Linux answers without asking at all. So the only way a sandboxed chat
/// reaches one is `secret exec` through the app: where the app does not take it, or for
/// anything else, nothing is read. Any other vault is left to its own provider and to the
/// chat's sandbox, as before. Resolving every chat's reads in the app is the rest of V16b
/// (#1180).
///
/// **A courtesy, not the boundary.** The mark it reads is the chat's environment, which the
/// chat can change; what holds the item is the store's own rule (V90a) and the sandbox. This
/// keeps purlis itself from raising a question on the chat's behalf, and says what to do.
pub fn refused_in_this_chat(ctx: &Ctx, vault: &registry::Vault, reader: Reader) -> Option<String> {
    let env = |name: &str| ctx.env.get(name);
    if !crate::sandbox::chat_is_sandboxed_in(&env) || !super::keyring::holds_values_of(ctx, vault) {
        return None;
    }
    let (why, next) = match reader {
        // The app runs no command for a chat on this system yet ([`NO_WRAP`]).
        _ if !cfg!(target_os = "macos") => (
            "purlis cannot yet run a command in a sandbox on this system, so the app runs none \
             for a chat here",
            "Run it in a terminal outside the chat.".to_owned(),
        ),
        Reader::Exec => (
            "the app that started this chat did not take this `secret exec`, so nothing was run",
            "Run it again while the app that started this chat is open.".to_owned(),
        ),
        Reader::GetOrCp | Reader::Other => (
            "the app runs only `secret exec` for a chat",
            format!(
                "Hand the value to a command with `purlis secret exec {} --env NAME=key -- \
                 <command>`: the app runs that for the chat.",
                vault.name
            ),
        ),
    };
    Some(format!(
        "vault '{}' is read from {}, which a sandboxed chat reaches only through the app, and \
         {why}. purlis does not read {} from inside a sandboxed chat.\n  {next}",
        vault.name,
        super::keyring::STORE_NAME,
        super::keyring::STORE_NAME,
    ))
}

/// Runs `req` through the app that started this chat, and answers its status, or `None` when
/// no app takes it and it is to run here.
///
/// Only inside a chat the app started sandboxed ([`crate::sandbox::chat_is_sandboxed_in`]): an
/// unsandboxed chat reads its vault as the person does. A chat that claims to be sandboxed when
/// it is not gets the stricter run, and one that hides it gets the vault's own refusal.
#[cfg(unix)]
pub fn forwarded(ctx: &Ctx, req: &Request, command: &[String], io: &mut dyn Io) -> Option<i32> {
    let env = |name: &str| ctx.env.get(name);
    if !crate::sandbox::chat_is_sandboxed_in(&env) {
        return None;
    }
    let socket = env(crate::hookwire::SOCKET_ENV).filter(|s| !s.is_empty())?;
    let chat = chat_number(&env)?;
    let token = crate::hookwire::ChatToken::read(&env);
    let stream = std::os::unix::net::UnixStream::connect(&socket).ok()?;
    converse(ctx, req, command, chat, token, stream, io)
}

/// [`forwarded`]'s exchange with the app, on a connection already made.
#[cfg(unix)]
fn converse(
    ctx: &Ctx,
    req: &Request,
    command: &[String],
    chat: u32,
    token: Option<crate::hookwire::ChatToken>,
    stream: std::os::unix::net::UnixStream,
    io: &mut dyn Io,
) -> Option<i32> {
    // The child gets the asker's stdin where it is not a terminal and the caller can hand it
    // on as a stream; an empty one otherwise.
    let mut stdin = if io.stdin_is_terminal() {
        None
    } else {
        io.stdin_stream()
    };
    let ask = Ask {
        chat,
        secret_exec: Wanted {
            vault: req.vault.clone(),
            env: req.env.clone(),
            file: req.file.clone(),
            dotenv: req.dotenv.clone(),
            command: command.to_vec(),
            cwd: std::env::current_dir().ok(),
            environment: ctx
                .env
                .vars()
                .iter()
                .filter_map(|(k, v)| Some((k.to_str()?.to_owned(), v.to_str()?.to_owned())))
                // The line has a cap (`hookwire`'s), and an environment is a few kilobytes: a
                // variable past this is not handed on, so the line is never cut.
                .filter(|(_, v)| v.len() <= A_VARIABLE_IS_AT_MOST)
                .collect(),
            stdin: stdin.is_some(),
        },
    };
    let line = crate::hookwire::line_with(token.as_ref(), &ask).ok()?;
    (&stream).write_all(&line).ok()?;
    let writer = stream.try_clone().ok()?;
    let mut reader = std::io::BufReader::new(stream);
    let mut started = false;
    loop {
        let mut said = String::new();
        let read = (&mut reader).take(A_FRAME_IS_AT_MOST).read_line(&mut said);
        let frame = match read {
            Ok(n) if n > 0 => serde_json::from_str::<Frame>(&said).ok(),
            _ => None,
        };
        let Some(frame) = frame else {
            // An app older than this line drops it unread: nothing ran, so it runs here.
            if !started {
                return None;
            }
            io.say(Say::Err(
                "the app stopped answering while the command ran, so how it ended is not known."
                    .into(),
            ));
            return Some(1);
        };
        match frame {
            Frame::Started => {
                started = true;
                if let Some(stdin) = stdin.take() {
                    pump(stdin, writer.try_clone().ok());
                }
            }
            Frame::Note(note) => io.say(Say::Info(note)),
            Frame::Stdout(bytes) => io.out(&unb64(&bytes)),
            Frame::Stderr(bytes) => io.err(&unb64(&bytes)),
            Frame::Exit(code) => return Some(code),
            Frame::Refused { why, .. } if why == crate::hookwire::NOTHING_ANSWERS && !started => {
                return None;
            }
            Frame::Refused { why, code } => {
                io.say(Say::Err(why));
                return Some(code);
            }
        }
    }
}

/// Where there is no unix socket there is no app to ask, and the command runs here.
#[cfg(not(unix))]
pub fn forwarded(_ctx: &Ctx, _req: &Request, _command: &[String], _io: &mut dyn Io) -> Option<i32> {
    None
}

/// Sends `stdin` to the app as it comes, on a thread of its own, and says when it ends.
#[cfg(unix)]
fn pump(mut stdin: Box<dyn Read + Send>, writer: Option<std::os::unix::net::UnixStream>) {
    let Some(mut writer) = writer else { return };
    let _ = std::thread::Builder::new()
        .name("purlis-secret-exec-stdin".into())
        .spawn(move || {
            let mut buf = vec![0u8; A_CHUNK];
            loop {
                let input = match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => Input::StdinClosed,
                    Ok(n) => Input::Stdin(b64(&buf[..n])),
                };
                let done = input == Input::StdinClosed;
                let Ok(mut line) = serde_json::to_vec(&input) else {
                    return;
                };
                line.push(b'\n');
                if writer.write_all(&line).is_err() || done {
                    return;
                }
            }
        });
}

// ----------------------------------------------------------------------------------------
// the app: authorise, resolve, run

/// The chat that asked, as the app recorded it: never as the line says.
#[derive(Debug, Clone)]
pub struct Asker {
    /// The project the chat runs in.
    pub root: PathBuf,
    /// The app's own environment, which every value is resolved in.
    pub env: Env,
    pub chat: u32,
    /// The persona the app started the chat as.
    pub persona: Option<String>,
    /// The chat's folder, which its sandbox lets it write.
    pub folder: Option<PathBuf>,
    /// What the chat's sandbox was compiled to when it started, or `None` for a chat the app
    /// did not start sandboxed.
    pub confines: Option<Confines>,
}

/// Why nothing is run where purlis has no sandbox to run it in (the Linux wrap is #1040). The
/// chat's `purlis secret exec` prints it, so it names no issue (#1422).
pub const NO_WRAP: &str = "purlis cannot yet run a command in a sandbox on this system, so it \
                           runs none for a sandboxed chat here: run `purlis secret exec` in a \
                           terminal outside the chat";

/// A vault a chat asked for that is registered and is not one its persona may use (#1430):
/// what the Notice on the chat's tab names. Every field is the app's own record or the
/// registry's, never the line's but the vault's name, which is one the registry holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotTagged {
    pub vault: String,
    /// The persona the app started the chat as.
    pub persona: String,
    /// The persona the registry tags the vault for, where it tags one.
    pub tagged_for: Option<String>,
}

/// Whether you let `persona`'s chats use `vault` on this machine (#1430): a grant in
/// [`crate::sandbox::local`], which no chat writes, and which an administrator's policy can
/// take away ([`crate::sandbox::policy::Locks::forbids_vault_grants`]). Read on every call, so
/// an Allow or a Revoke reaches the next run with no restart.
fn allowed_here(ctx: &Ctx, persona: &str, vault: &str) -> bool {
    !crate::sandbox::policy::Locks::of(&ctx.root).forbids_vault_grants()
        && crate::sandbox::local::granted_vaults(&ctx.root)
            .iter()
            .any(|one| one.vault == vault && one.persona == persona)
}

/// **The vaults a chat started as `persona` is handed**, by name and sorted: the ones the
/// registry tags for it and the ones you let it use on this machine, which is what
/// [`authorise`] opens. Never the `vault:` line of the persona's own file. An error where the
/// registry does not read, so "which is not known" is never said as "none". Names only: no
/// vault is opened, and nothing inside one is read.
pub fn vaults_of(ctx: &Ctx, persona: &str) -> Result<Vec<String>, String> {
    let doc = registry::load_registry(ctx).map_err(|e| e.message)?;
    let mut out = registry::vaults_for_persona(&doc, persona);
    if !crate::sandbox::policy::Locks::of(&ctx.root).forbids_vault_grants() {
        for one in crate::sandbox::local::granted_vaults(&ctx.root) {
            let held = registry::vaults(&doc).contains_key(&one.vault);
            if one.persona == persona && held && !out.contains(&one.vault) {
                out.push(one.vault);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `vault` as [`NotTagged`] for a chat started as `persona`, or `None` where there is nothing
/// to allow: the chat runs as no persona, the vault is not registered, or the persona may use
/// it already.
pub fn not_tagged(ctx: &Ctx, persona: Option<&str>, vault: &str) -> Option<NotTagged> {
    let persona = persona.filter(|p| !p.is_empty())?;
    let doc = registry::load_registry(ctx).ok()?;
    let tagged_for = registry::vault_in(&doc, vault).ok()?.persona;
    if tagged_for.as_deref() == Some(persona) || allowed_here(ctx, persona, vault) {
        return None;
    }
    Some(NotTagged {
        vault: vault.to_owned(),
        persona: persona.to_owned(),
        tagged_for: tagged_for.filter(|p| !p.is_empty()),
    })
}

/// What every refusal of a vault says of the chat's own persona (ADR 0090 as amended, #1435).
pub const FIXED: &str = "A chat's persona is fixed for its life, so nothing run in this chat \
                         changes which vaults it may use.";

/// The persona `not`'s vault is tagged for, where a chat can be dispatched to it: one the
/// project defines. The registry's tag is any text its shared, committed half holds, so a tag
/// that names no persona offers no chat, and the name is one line wherever it is shown.
pub fn dispatchable(ctx: &Ctx, not: &NotTagged) -> Option<String> {
    let theirs = not.tagged_for.as_deref()?;
    crate::personas::name_refusal(&ctx.root, theirs)
        .is_none()
        .then(|| crate::personas::one_line(theirs))
}

/// **The ways forward a refused vault names** (#1430), for the chat that reads the refusal: the
/// operator's Allow on the chat's tab, where policy leaves it open, and a dispatch to the
/// persona the vault is tagged for, where the project defines that persona
/// ([`dispatchable`]). Never a change of the chat's own persona, which is fixed for its life.
pub fn routes(ctx: &Ctx, not: &NotTagged) -> String {
    let dispatch = dispatchable(ctx, not).map(|theirs| {
        format!(
            "to have '{theirs}', the persona the vault is tagged for, do the work, dispatch to \
             it: `purlis dispatch --to {theirs}`"
        )
    });
    let said = match (
        crate::sandbox::policy::Locks::of(&ctx.root).vault_grants_refused(),
        dispatch,
    ) {
        (None, Some(dispatch)) => format!("Two ways forward: {ALLOW}; or, {dispatch}."),
        (None, None) => format!("The way forward: {ALLOW}."),
        (Some(locked), Some(dispatch)) => format!("{locked} The way forward: {dispatch}."),
        (Some(locked), None) => locked,
    };
    format!("{said} {FIXED}")
}

/// The first way forward: the operator's Allow, which the next run reads.
const ALLOW: &str = "ask the operator to press Allow in the notice on this chat's tab, then run \
                     the command again (no restart is needed)";

/// Whether `persona` may use `vault`: the vault registry tags it with that persona
/// (`purlis vault add <vault> --persona <persona>`), or you allowed it for that persona on this
/// machine ([`allowed_here`], D-1430-1). A chat on no persona uses no vault.
///
/// **The registry and your own grant, and nothing a chat writes.** D-1407-1 made the registry
/// the only thing that opens a vault to a chat; D-1430-1 widens it by one thing, the person's
/// own grant on this machine. A persona's own `vault:` line is what `purlis persona secret`
/// reads first, but it lives in the persona's file, which a chat can write (a chat at the
/// project root writes `personas/`, and a chat edits its own persona's charter), so it cannot
/// be what lets a chat at a vault: a chat would grant itself any vault by naming it there. The
/// registry is denied to every chat's writes (ADR 0067 §5 class 1), and so is the file your
/// grant is kept in (the integrity class).
///
/// **A vault the registry does not hold is refused in the registry's own words**, with no way
/// forward named: there is nothing to allow and nobody to dispatch to, and no Notice is raised
/// for it ([`not_tagged`]).
pub fn authorise(ctx: &Ctx, persona: Option<&str>, vault: &str) -> Result<(), String> {
    let Some(persona) = persona.filter(|p| !p.is_empty()) else {
        return Err(format!(
            "this chat runs as no persona, so purlis hands it no vault. Start the chat as the \
             persona that vault '{vault}' is tagged with."
        ));
    };
    let doc = registry::load_registry(ctx).map_err(|e| e.message)?;
    let registered = registry::vault_in(&doc, vault).map_err(|e| e.message)?;
    let tagged = registry::vaults_for_persona(&doc, persona);
    if tagged.iter().any(|t| t == vault) || allowed_here(ctx, persona, vault) {
        return Ok(());
    }
    let not = NotTagged {
        vault: vault.to_owned(),
        persona: persona.to_owned(),
        tagged_for: registered.persona.filter(|p| !p.is_empty()),
    };
    let routes = routes(ctx, &not);
    if cmd::persona_vault(ctx, persona).is_ok_and(|declared| declared == vault) {
        return Err(format!(
            "persona '{persona}' names vault '{vault}' in its own file, and a chat can edit that \
             file, so purlis does not open the vault for a chat on that alone. {routes}"
        ));
    }
    let theirs = if tagged.is_empty() {
        "none is tagged with it".to_owned()
    } else {
        format!(
            "it may use {}",
            tagged
                .iter()
                .map(|a| format!("'{a}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Err(format!(
        "vault '{vault}' is not one persona '{persona}' may use, so purlis did not open it \
         ({theirs}). {routes}"
    ))
}

/// One vault as `purlis vault list` shows it to a sandboxed chat (#1430): its name, the
/// provider's id, the persona the registry tags it for, and whether the chat's persona may use
/// it. **No value, no key name, and nothing of a provider's session.**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Listed {
    pub name: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// Whether the chat that asked may use it, as [`authorise`] would answer.
    pub usable: bool,
}

/// Every vault the project registers, for a chat started as `persona`: read from the registry
/// alone, in the app, so no provider is asked and no session is opened. The registry, the
/// policy and this machine's grants are each read once.
pub fn listed(ctx: &Ctx, persona: Option<&str>) -> Result<Vec<Listed>, String> {
    let doc = registry::load_registry(ctx).map_err(|e| e.message)?;
    let persona = persona.filter(|p| !p.is_empty());
    let granted = if crate::sandbox::policy::Locks::of(&ctx.root).forbids_vault_grants() {
        Vec::new()
    } else {
        crate::sandbox::local::granted_vaults(&ctx.root)
    };
    let mut names: Vec<String> = registry::vaults(&doc).keys().cloned().collect();
    names.sort();
    Ok(names
        .into_iter()
        .filter_map(|name| {
            let vault = registry::vault_in(&doc, &name).ok()?;
            let tag = vault.persona.filter(|p| !p.is_empty());
            let usable = persona.is_some_and(|persona| {
                tag.as_deref() == Some(persona)
                    || granted
                        .iter()
                        .any(|one| one.vault == name && one.persona == persona)
            });
            Some(Listed {
                usable,
                provider: vault.provider,
                persona: tag,
                name,
            })
        })
        .collect())
}

/// What the app answers `purlis vault list` from a chat: the persona it started the chat as,
/// every vault, and whether an administrator's policy forbids the person's Allow here, so the
/// listing names only a way forward that is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub persona: Option<String>,
    pub vaults: Vec<Listed>,
    pub allow_locked: bool,
}

/// How long `purlis vault list` waits for the app's answer.
#[cfg(unix)]
const A_LISTING_TAKES_AT_MOST: Duration = Duration::from_secs(5);

/// The app's answer to `purlis vault list` from a sandboxed chat it started (#1430), or `None`
/// where no app answers it (no socket, an app older than the ask) and the listing is made here
/// as it always was.
#[cfg(unix)]
pub fn listing_from_the_app(ctx: &Ctx) -> Option<Result<Listing, String>> {
    use crate::hookwire::{Answer, Ask, Asking, ChatToken};
    let env = |name: &str| ctx.env.get(name);
    if !crate::sandbox::chat_is_sandboxed_in(&env) {
        return None;
    }
    let socket = env(crate::hookwire::SOCKET_ENV).filter(|s| !s.is_empty())?;
    let chat = chat_number(&env)?;
    let mut asking = Asking::on(Path::new(&socket), ChatToken::read(&env)).ok()?;
    match asking
        .ask(&Ask::Vaults { chat }, A_LISTING_TAKES_AT_MOST)
        .ok()?
    {
        Answer::Vaults {
            persona,
            vaults,
            allow_locked,
        } => Some(Ok(Listing {
            persona,
            vaults,
            allow_locked,
        })),
        Answer::No { why } => Some(Err(why)),
        _ => None,
    }
}

/// Where there is no unix socket there is no app to ask.
#[cfg(not(unix))]
pub fn listing_from_the_app(_ctx: &Ctx) -> Option<Result<Listing, String>> {
    None
}

/// How a run is wrapped: in a Seatbelt profile built from the chat's policy, or, for a test of
/// everything around it on a machine that cannot apply a second sandbox, not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wrap {
    Seatbelt,
    #[cfg(test)]
    Unwrapped,
    /// Unwrapped, with the proxy refusing this `host:port` as the run starts.
    #[cfg(test)]
    Refusing(&'static str),
}

/// What is told each sandbox block of a brokered run, for the asking chat's Notice: a host the
/// run's proxy refused, as [`crate::hookwire::SandboxBlocked`] carries a chat's own.
pub type Told = Arc<dyn Fn(crate::hookwire::SandboxBlocked) + Send + Sync + 'static>;

/// Answers `wanted` for `asker` on the connection `reader`/`writer`: every frame written there,
/// the last an [`Frame::Exit`] or a [`Frame::Refused`]. Returns once the child has ended.
/// `told` hears each host the run's proxy refused, as the asking chat's block.
pub fn serve(
    asker: &Asker,
    wanted: Wanted,
    reader: Box<dyn BufRead + Send>,
    writer: Box<dyn Write + Send>,
    told: Told,
) {
    serve_in(asker, wanted, reader, writer, Wrap::Seatbelt, told);
}

/// [`serve`], telling `reached` each connection the run made (#1667): through its proxy, and
/// through a tunnel to a database host its values point at ([`crate::sandbox::tunnel`]). The
/// network record keeps them under the asking chat. A host that carries one of the run's values
/// is counted and never named.
pub fn serve_recording(
    asker: &Asker,
    wanted: Wanted,
    reader: Box<dyn BufRead + Send>,
    writer: Box<dyn Write + Send>,
    told: Told,
    reached: crate::sandbox::egress::Reached,
) {
    serve_recorded(
        asker,
        wanted,
        reader,
        writer,
        Wrap::Seatbelt,
        told,
        Some(reached),
    );
}

/// [`serve`] in `wrap`, telling nothing of what its proxy refused.
#[cfg(test)]
pub(crate) fn serve_wrapped(
    asker: &Asker,
    wanted: Wanted,
    reader: Box<dyn BufRead + Send>,
    writer: Box<dyn Write + Send>,
    wrap: Wrap,
) {
    serve_in(asker, wanted, reader, writer, wrap, Arc::new(|_| {}));
}

/// The block a host the run's proxy refused is, for chat `chat`'s Notice: a connection to a
/// host, the chat's own, naming `target` (`host:port`) whole.
fn refused_block(chat: u32, target: String) -> crate::hookwire::SandboxBlocked {
    use crate::sandboxblock::{Block, Kind, Operation};
    crate::hookwire::SandboxBlocked {
        chat,
        sandbox_blocked: Block {
            operation: Operation::Connect,
            kind: Kind::Host,
            ours: false,
        },
        harness: None,
        target: Some(target),
    }
}

/// What purlis says on the command's stderr of a host its proxy refused (`host:port`): that the
/// sandbox refused it, and where the person allows it. Not in the words a chat's own block hook
/// reads (`crate::sandboxblock`), so one refusal raises one Notice, from the proxy's record.
pub fn refused_note(target: &str) -> String {
    format!(
        "purlis's sandbox refused this command a connection to {target}, which this chat's \
         hosts do not list. It is the sandbox, not the server. The person can allow it on this \
         chat's tab (Allow for this chat, or Always allow); the chat is then started again \
         with it, and the command can be run again."
    )
}

/// Whether `target`, a host the run's proxy refused, carries any of the run's `values`, as the
/// mask would find it or in another case (a host is the same host in any case).
fn carries_a_value(target: &str, values: &[String]) -> bool {
    let target = target.to_lowercase();
    values
        .iter()
        .filter(|value| !value.is_empty())
        .any(|value| target.contains(&value.to_lowercase()))
}

/// What purlis says instead of naming a refused host that carries one of the run's values:
/// nothing of the host, and no Notice offers it.
pub const WITHHELD_NOTE: &str = "purlis's sandbox refused this command a connection to a host \
                                 that carries a value from the vault, so purlis does not name \
                                 it and offers no way to allow it.";

/// Writes `frame` on `writer` as one line.
fn send(writer: &mut dyn Write, frame: &Frame) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(frame).map_err(std::io::Error::other)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()
}

/// Refuses the ask on `writer` with `why`: nothing was run.
pub fn refused(mut writer: Box<dyn Write + Send>, why: String) {
    let _ = send(&mut *writer, &Frame::Refused { why, code: 1 });
}

/// What an app with nothing that runs a brokered command answers, so the asker runs it itself.
pub fn not_answered(mut writer: Box<dyn Write + Send>) {
    let _ = send(
        &mut *writer,
        &Frame::Refused {
            why: crate::hookwire::NOTHING_ANSWERS.to_owned(),
            code: 1,
        },
    );
}

/// The variables of the asker's environment a child never starts with: the chat's hook channel
/// and its token (the child cannot reach the socket, and needs neither), and the ones the wrap
/// sets itself. Each under its old prefix too ([`not_handed_on`]).
const NOT_HANDED_ON: &[&str] = &[crate::hookwire::SOCKET_ENV, crate::hookwire::TOKEN_ENV];

/// Whether `key` is one of [`NOT_HANDED_ON`], under the purlis prefix or an old one the window
/// still reads ([`crate::names::ENV_PREFIX`]).
fn not_handed_on(key: &str) -> bool {
    let prefix = &crate::names::ENV_PREFIX;
    NOT_HANDED_ON.iter().any(|name| {
        *name == key
            || name.strip_prefix(prefix.write).is_some_and(|rest| {
                prefix
                    .reads
                    .iter()
                    .any(|old| key.strip_prefix(old) == Some(rest))
            })
    })
}

pub(crate) fn serve_in(
    asker: &Asker,
    wanted: Wanted,
    reader: Box<dyn BufRead + Send>,
    writer: Box<dyn Write + Send>,
    wrap: Wrap,
    told: Told,
) {
    serve_recorded(asker, wanted, reader, writer, wrap, told, None);
}

pub(crate) fn serve_recorded(
    asker: &Asker,
    wanted: Wanted,
    reader: Box<dyn BufRead + Send>,
    mut writer: Box<dyn Write + Send>,
    wrap: Wrap,
    told: Told,
    reached: Option<crate::sandbox::egress::Reached>,
) {
    let refuse = |writer: &mut dyn Write, code: i32, why: String| {
        tracing::warn!(
            "purlis: chat {} was refused `secret exec` on vault '{}': {why}",
            asker.chat,
            wanted.vault
        );
        let _ = send(writer, &Frame::Refused { why, code });
    };
    let ctx = Ctx::new(&asker.root, asker.env.clone());
    let mut command = wanted.command.clone();
    if command.first().is_some_and(|c| c == "--") {
        command.remove(0);
    }
    if command.is_empty() {
        return refuse(&mut *writer, 2, exec::NO_COMMAND.to_owned());
    }
    let Some(confines) = &asker.confines else {
        // A chat the app did not start sandboxed reads its vault as the person does: the line
        // claimed a sandbox the chat does not have, and the command runs where it was asked.
        tracing::info!(
            "purlis: chat {} asked for a brokered `secret exec` and was not started sandboxed, \
             so it runs the command itself",
            asker.chat
        );
        return not_answered(writer);
    };
    // What this chat may write is the app's record of it, and no provider's program is run
    // from there (#1516).
    let ctx = ctx.chat(confines, asker.folder.as_deref());
    if let Err(why) = authorise(&ctx, asker.persona.as_deref(), &wanted.vault) {
        return refuse(&mut *writer, 1, why);
    }
    // At most a few at once per chat: each holds a proxy, a temp directory and a process group.
    let Some(_slot) = Slot::take(&asker.root, asker.chat) else {
        return refuse(
            &mut *writer,
            1,
            format!(
                "this chat already has {RUNS_AT_ONCE} commands running through purlis with a \
                 vault; wait for one to end, then run this again"
            ),
        );
    };
    if wrap == Wrap::Seatbelt && !cfg!(target_os = "macos") {
        return refuse(&mut *writer, 1, NO_WRAP.to_owned());
    }
    let Some(folder) = asker.folder.clone().filter(|f| f.is_dir()) else {
        return refuse(
            &mut *writer,
            1,
            "this chat has no folder purlis knows of, so there is no sandbox to run the command \
             in"
            .to_owned(),
        );
    };
    let v = match cmd::provider(&ctx, &wanted.vault) {
        Ok(v) => v,
        Err(e) => return refuse(&mut *writer, 1, e.message),
    };
    // What runs beside the child for as long as it does: the egress proxy carrying the chat's
    // hosts, telling the chat's tab each host it refuses, and a temp directory of its own.
    // A host that carries one of the run's values is never told: no Notice names it and no
    // grant is made from it. The values are known once the run is prepared, which is after its
    // proxy starts; until then nothing is told.
    let values: Arc<std::sync::OnceLock<Vec<String>>> = Arc::default();
    let refusals = crate::sandbox::egress::Refusals::telling({
        let chat = asker.chat;
        let values = Arc::clone(&values);
        Arc::new(move |host: &str, port: u16| {
            let target = crate::sandbox::egress::host_and_port(host, port);
            if values
                .get()
                .is_some_and(|values| !carries_a_value(&target, values))
            {
                told(refused_block(chat, target));
            }
        })
    });
    // Each connection the run makes, told under the chat; a host that carries one of the
    // run's values counted with no name, as the record never keeps a value.
    let reached = reached.map(|reached| -> crate::sandbox::egress::Reached {
        let values = Arc::clone(&values);
        Arc::new(move |target: Option<&str>, by: &'static str, times: u64| {
            let named = target.filter(|target| {
                values
                    .get()
                    .is_some_and(|values| !carries_a_value(target, values))
            });
            reached(named, by, times);
        })
    });
    let beside = match Beside::start(wrap, confines, refusals.clone(), reached.clone()) {
        Ok(beside) => beside,
        Err(e) => {
            return refuse(
                &mut *writer,
                1,
                format!("purlis could not start the sandbox the command runs in ({e})"),
            );
        }
    };
    // The asker's environment, less the hook channel, then the wrap's own (its proxy and its
    // temp directory), then every other vault's identity variables taken out.
    let mut base: Vec<(OsString, OsString)> = wanted
        .environment
        .iter()
        .filter(|(k, _)| !not_handed_on(k))
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect();
    for (k, v) in crate::sandbox::seatbelt::env(&beside.proxy_url(), beside.tmp()) {
        exec::set_var(&mut base, &k, v);
    }
    // ssh, and git over ssh, through the run's own SOCKS port (#1667): the chat's route names
    // the chat's port, which the run's sandbox does not reach.
    if let Some(route) = beside.ssh_route() {
        for (k, v) in route.env() {
            exec::set_var(&mut base, &k, v);
        }
        let path = base
            .iter()
            .rev()
            .find(|(k, _)| k == "PATH")
            .and_then(|(_, v)| v.to_str())
            .unwrap_or("")
            .to_owned();
        exec::set_var(&mut base, "PATH", route.on_path(&path));
    }
    let base = match exec::child_env_of(&ctx, &wanted.vault, &base) {
        Ok(base) => base,
        Err(e) => return refuse(&mut *writer, 1, e.message),
    };
    let within = match private_dir(&ctx.vaults_dir()) {
        Ok(within) => within,
        Err(e) => {
            return refuse(
                &mut *writer,
                1,
                format!("purlis could not make the folder a credential file is kept in ({e})"),
            );
        }
    };
    let req = Request {
        vault: wanted.vault.clone(),
        env: wanted.env.clone(),
        file: wanted.file.clone(),
        dotenv: wanted.dotenv.clone(),
        stream: false,
        exec: false,
        command: command.clone(),
    };
    let prepared = match exec::prepare(&ctx, &v, &req, base, Some(&within), &|| None) {
        Ok(prepared) => prepared,
        Err(Stopped::Said(code, why)) => return refuse(&mut *writer, code, why),
        Err(Stopped::Signal(code)) => return refuse(&mut *writer, code, "stopped".to_owned()),
    };
    let _ = values.set(prepared.secret_values.clone());
    let mut prepared = prepared;
    // A value that points at a host the chat may reach goes through a tunnel to exactly it, and
    // one the chat may not reach is refused as the proxy refuses one (#1667).
    let (tunnels, notes) = match tunnelled(&mut prepared, &req.env, confines, &refusals, reached) {
        Ok(opened) => opened,
        Err(e) => {
            return refuse(
                &mut *writer,
                1,
                format!("purlis could not open the tunnel the command reaches its host by ({e})"),
            );
        }
    };
    #[cfg(test)]
    if let Wrap::Refusing(target) = wrap {
        let (host, port) = target.rsplit_once(':').expect("host:port");
        refusals.heard(host, port.parse().expect("a port"));
    }
    let program = match on_path(&command[0], &prepared.env) {
        Some(program) => program,
        None => {
            return refuse(
                &mut *writer,
                127,
                format!("command not found: {}", command[0]),
            );
        }
    };
    let mut child = match wrap {
        Wrap::Seatbelt => {
            let mut ports = beside.proxy_ports();
            ports.extend(tunnels.iter().map(crate::sandbox::tunnel::Tunnel::port));
            let profile = match profile_on(confines, &prepared.files, &folder, beside.tmp(), &ports)
            {
                Ok(profile) => profile,
                Err(why) => return refuse(&mut *writer, 1, why.to_owned()),
            };
            let mut child = std::process::Command::new(crate::sandbox::backend::SANDBOX_EXEC);
            child.arg("-p").arg(profile).arg("--").arg(&program);
            child
        }
        #[cfg(test)]
        Wrap::Unwrapped | Wrap::Refusing(_) => std::process::Command::new(&program),
    };
    let start_in = wanted
        .cwd
        .clone()
        .filter(|cwd| cwd.is_absolute() && cwd.is_dir())
        .unwrap_or_else(|| folder.clone());
    // A process group of its own, so everything it starts is ended with it.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut child, 0);
    child
        .args(&command[1..])
        .current_dir(start_in)
        .env_clear()
        .envs(prepared.env.iter().map(|(k, v)| (k, v)))
        .stdin(if wanted.stdin {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Recorded under the chat's own session, as the record of a run in the chat would be.
    let chat = asker.chat.to_string();
    let ctx_traced = Ctx {
        root: ctx.root.clone(),
        state: ctx.state.clone(),
        env: Env::of(&[(crate::active::SESSION_ID_ENV, &chat)]),
        chat_writes: Vec::new(),
        kept: super::identity::Kept::default(),
    };
    exec::record(
        &ctx_traced,
        &wanted.vault,
        &prepared,
        &command[0],
        "brokered",
        &[
            ("chat", Value::from(asker.chat)),
            (
                "persona",
                Value::String(asker.persona.clone().unwrap_or_default()),
            ),
        ],
    );
    tracing::info!(
        "purlis: chat {} (persona {}) ran `{}` with vault '{}' through the app",
        asker.chat,
        asker.persona.as_deref().unwrap_or("none"),
        command[0],
        wanted.vault
    );

    let mut spawned = match crate::forklock::spawn(&mut child) {
        Ok(spawned) => spawned,
        Err(e) => return refuse(&mut *writer, 1, format!("cannot run {}: {e}", command[0])),
    };
    let group = Live::hold(spawned.id());
    if cmd_said_note(&ctx, &v, &mut *writer).is_err()
        || notes
            .iter()
            .any(|note| send(&mut *writer, &Frame::Note(note.clone())).is_err())
        || send(&mut *writer, &Frame::Started).is_err()
    {
        group.kill();
        let _ = spawned.wait();
        return;
    }
    let code = run(
        spawned,
        &group,
        reader,
        &mut *writer,
        &prepared.secret_values,
    );
    let (withheld, named): (Vec<String>, Vec<String>) = refusals
        .refused()
        .into_iter()
        .partition(|target| carries_a_value(target, &prepared.secret_values));
    for target in named {
        let note = super::redact_str(&refused_note(&target), &prepared.secret_values);
        let _ = send(&mut *writer, &Frame::Note(note));
    }
    if !withheld.is_empty() {
        let _ = send(&mut *writer, &Frame::Note(WITHHELD_NOTE.to_owned()));
    }
    let _ = send(&mut *writer, &Frame::Exit(code));
    drop(tunnels);
    drop(prepared);
    drop(beside);
}

/// Points each `--env` value of `prepared` that names a host and port at a tunnel to it, where
/// the chat may reach that exact host and port ([`crate::sandbox::tunnel::route`]): the value is
/// swapped in the child's environment and masked as the value is. A host the chat may not reach
/// is told to `refusals`, as the proxy tells one, so the chat's Notice offers Allow; one on this
/// machine is said in a note naming the variable alone. Answers the tunnels, which live as long
/// as the run, and the notes.
fn tunnelled(
    prepared: &mut exec::Prepared,
    specs: &[String],
    confines: &Confines,
    refusals: &crate::sandbox::egress::Refusals,
    reached: Option<crate::sandbox::egress::Reached>,
) -> std::io::Result<(Vec<crate::sandbox::tunnel::Tunnel>, Vec<String>)> {
    use crate::sandbox::tunnel::{Route, TUNNELS_AT_MOST, Tunnel, route};
    let reach = confines.decides();
    let own = crate::sandbox::hosts::own_addresses();
    let mut tunnels: Vec<Tunnel> = Vec::new();
    let mut notes = Vec::new();
    for name in specs
        .iter()
        .filter_map(|spec| spec.split_once('=').map(|(n, _)| n))
    {
        let Some(value) = prepared
            .env
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        // libpq's own pair (#1708): a host alone in PGHOST, its port beside it in PGPORT.
        let pair = libpq_pair(name, &value, &prepared.env);
        let routed = pair.as_ref().map_or(value, |pair| pair.target.clone());
        match route(&routed, &reach, &own) {
            Route::Through(pointed, decision) => {
                let at = match tunnels.iter().find(|t| t.target() == pointed.target()) {
                    Some(open) => open.port(),
                    None if tunnels.len() >= TUNNELS_AT_MOST => {
                        notes.push(format!(
                            "purlis opens at most {TUNNELS_AT_MOST} tunnels for one command, so \
                             {name} was handed as it is."
                        ));
                        continue;
                    }
                    None => {
                        let tunnel =
                            Tunnel::open(&pointed, &decision, reach.clone(), reached.clone())?;
                        let port = tunnel.port();
                        tunnels.push(tunnel);
                        port
                    }
                };
                if let Some(pair) = &pair {
                    // Nothing secret is added: loopback and the tunnel's port.
                    let place = if pair.by_name { PGHOSTADDR } else { PGHOST };
                    exec::set_var(&mut prepared.env, place, crate::sandbox::tunnel::LOOPBACK);
                    exec::set_var(&mut prepared.env, PGPORT, at.to_string());
                } else {
                    let swapped = pointed.at(at);
                    exec::set_var(&mut prepared.env, name, swapped.clone());
                    prepared.secret_values.push(swapped);
                }
                tracing::info!(
                    "purlis: {name} of a brokered `secret exec` reaches its host through a \
                     tunnel on port {at}"
                );
            }
            Route::Refused(pointed) => refusals.heard(pointed.host(), pointed.port()),
            Route::Local(_) => notes.push(format!(
                "{name} points at this machine, a link-local address or a cloud metadata \
                 service, which purlis's sandbox never reaches, so it was handed as it is."
            )),
            Route::Untouched => {}
        }
    }
    Ok((tunnels, notes))
}

/// libpq's variables for where it connects: the host, the address it connects to instead of
/// looking the host up, the port, and the certificate check.
const PGHOST: &str = "PGHOST";
const PGHOSTADDR: &str = "PGHOSTADDR";
const PGPORT: &str = "PGPORT";
const PGSSLMODE: &str = "PGSSLMODE";

/// A host handed alone in libpq's `PGHOST`, with its port beside it (#1708).
struct LibpqPair {
    /// `host:port`, as a value pointed at one place spells it.
    target: String,
    /// The certificate is checked by name (`PGSSLMODE=verify-full`): `PGHOST` stays, and libpq
    /// connects to `PGHOSTADDR` instead.
    by_name: bool,
}

/// **`PGHOST` as one place** (#1708): where `name` is libpq's `PGHOST` and `value` a host alone,
/// the host with its port from `PGPORT` in `env` (libpq's 5432 where none is set). None for any
/// other variable, a value that already names its port, a port that is not one, or where
/// `PGHOSTADDR` is set already, which could be read two ways.
fn libpq_pair(name: &str, value: &str, env: &[(OsString, OsString)]) -> Option<LibpqPair> {
    let var = |wanted: &str| {
        env.iter()
            .rev()
            .find(|(k, _)| k == wanted)
            .map(|(_, v)| v.to_str().map(str::to_owned))
    };
    if name != PGHOST
        || crate::sandbox::tunnel::pointed(value).is_some()
        || var(PGHOSTADDR).is_some()
    {
        return None;
    }
    let port = match var(PGPORT) {
        None => "5432".to_owned(),
        Some(Some(port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => port,
        Some(_) => return None,
    };
    let host = if value.contains(':') {
        format!("[{value}]")
    } else {
        value.to_owned()
    };
    Some(LibpqPair {
        target: format!("{host}:{port}"),
        by_name: var(PGSSLMODE).flatten().as_deref() == Some("verify-full"),
    })
}

/// What runs beside a brokered child: the chat's egress proxy and a temp directory of its own,
/// or, for a test that runs it unwrapped, a temp directory and a proxy nothing listens on.
enum Beside {
    Confined(crate::sandbox::Confinement),
    #[cfg(test)]
    Pretend(tempfile::TempDir),
}

impl Beside {
    fn start(
        wrap: Wrap,
        confines: &Confines,
        refusals: crate::sandbox::egress::Refusals,
        reached: Option<crate::sandbox::egress::Reached>,
    ) -> std::io::Result<Self> {
        match wrap {
            Wrap::Seatbelt => {
                crate::sandbox::Confinement::serving(crate::sandbox::egress::Serving {
                    refusals,
                    reached,
                    ..crate::sandbox::egress::Serving::of(confines.decides())
                })
                .map(Self::Confined)
            }
            #[cfg(test)]
            Wrap::Unwrapped => tempfile::tempdir().map(Self::Pretend),
            #[cfg(test)]
            Wrap::Refusing(_) => {
                drop(refusals);
                tempfile::tempdir().map(Self::Pretend)
            }
        }
    }

    fn tmp(&self) -> &Path {
        match self {
            Self::Confined(confinement) => confinement.tmp(),
            #[cfg(test)]
            Self::Pretend(tmp) => tmp.path(),
        }
    }

    /// Its proxy's ports, HTTP then SOCKS5: what the child may connect to besides its tunnels.
    fn proxy_ports(&self) -> Vec<u16> {
        match self {
            Self::Confined(confinement) => confinement.proxy_ports().to_vec(),
            #[cfg(test)]
            Self::Pretend(_) => vec![9],
        }
    }

    /// Its ssh route through its own SOCKS port (#1667), where it has one.
    fn ssh_route(&self) -> Option<&crate::sandbox::tunnel::SshRoute> {
        match self {
            Self::Confined(confinement) => confinement.ssh_route(),
            #[cfg(test)]
            Self::Pretend(_) => None,
        }
    }

    fn proxy_url(&self) -> String {
        match self {
            Self::Confined(confinement) => confinement.proxy_url(),
            #[cfg(test)]
            Self::Pretend(_) => "http://127.0.0.1:9".to_owned(),
        }
    }
}

/// The vault's one note, where it is due, as a frame.
fn cmd_said_note(ctx: &Ctx, v: &registry::Vault, writer: &mut dyn Write) -> std::io::Result<()> {
    struct Notes(Vec<String>);
    impl Io for Notes {
        fn say(&mut self, line: Say) {
            let (Say::Info(s) | Say::Ok(s) | Say::Warn(s) | Say::Err(s)) = line;
            self.0.push(s);
        }
        fn out(&mut self, _: &[u8]) {}
        fn err(&mut self, _: &[u8]) {}
        fn stdout_is_terminal(&self) -> bool {
            false
        }
        fn stdin_is_terminal(&self) -> bool {
            false
        }
        fn read_stdin(&mut self) -> String {
            String::new()
        }
        fn read_hidden(&mut self, _: &str) -> String {
            String::new()
        }
    }
    let mut notes = Notes(Vec::new());
    cmd::say_held_note(ctx, v, &mut notes);
    for note in notes.0 {
        send(writer, &Frame::Note(note))?;
    }
    Ok(())
}

/// The folder under the vaults directory a brokered run keeps its credential files in.
pub const EXEC_DIR: &str = "exec";

/// The `exec` folder under `vaults` (the project's vaults directory), made `0700` if it is not
/// there. Refused unless `vaults` is a directory and no link, and `exec` is, as the kernel names
/// it, exactly `exec` inside it: a link anywhere on the way could put a credential file where a
/// chat reads.
fn private_dir(vaults: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(vaults)?;
    let not_a_folder = |what: &Path| {
        std::io::Error::other(format!("{} is not a folder of its own", what.display()))
    };
    if !std::fs::symlink_metadata(vaults)?.is_dir() {
        return Err(not_a_folder(vaults));
    }
    let dir = vaults.join(EXEC_DIR);
    match std::fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    if !std::fs::symlink_metadata(&dir)?.is_dir()
        || dir.canonicalize()? != vaults.canonicalize()?.join(EXEC_DIR)
    {
        return Err(not_a_folder(&dir));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

/// Removes every credential file a brokered run left in the project at `root`: one the app was
/// stopped before it could remove (a crash, a kill). Run when a project opens, when no run of
/// its can be live. Only the files [`super::exec`] names, and nothing behind a link.
pub fn sweep(ctx: &Ctx) {
    let dir = ctx.vaults_dir().join(EXEC_DIR);
    let ours = std::fs::symlink_metadata(ctx.vaults_dir()).is_ok_and(|m| m.is_dir())
        && std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir());
    if !ours {
        return;
    }
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let left = entry
            .file_name()
            .to_string_lossy()
            .starts_with("charter-secret-")
            && entry.file_type().is_ok_and(|t| t.is_file());
        if left {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The most brokered runs one chat may have at once.
pub const RUNS_AT_ONCE: usize = 4;

/// One chat's place among its brokered runs, given back when dropped.
struct Slot {
    key: (PathBuf, u32),
}

static SLOTS: std::sync::Mutex<Vec<(PathBuf, u32)>> = std::sync::Mutex::new(Vec::new());

impl Slot {
    /// A place for chat `chat` of the project at `root`, or `None` when it has
    /// [`RUNS_AT_ONCE`] already.
    fn take(root: &Path, chat: u32) -> Option<Self> {
        let key = (root.to_path_buf(), chat);
        let mut held = SLOTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if held.iter().filter(|k| **k == key).count() >= RUNS_AT_ONCE {
            return None;
        }
        held.push(key.clone());
        Some(Self { key })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut held = SLOTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(at) = held.iter().position(|k| *k == self.key) {
            held.remove(at);
        }
    }
}

/// A brokered child's process group while it may be running, which [`stop_every_run`] kills.
struct Live {
    group: u32,
}

static LIVE: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

impl Live {
    fn hold(group: u32) -> Self {
        LIVE.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(group);
        Self { group }
    }

    /// Kills the whole group: the child and everything it started that stayed in it.
    fn kill(&self) {
        kill_group(self.group);
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        // Not killed here: the group was killed as the child ended (`run`), and a number killed
        // long after could by then be another group's.
        let mut live = LIVE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(at) = live.iter().position(|g| *g == self.group) {
            live.remove(at);
        }
    }
}

fn kill_group(group: u32) {
    #[cfg(unix)]
    if let Some(pid) = i32::try_from(group)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    #[cfg(not(unix))]
    let _ = group;
}

/// Kills every brokered run this process has going, as the app lets go of its projects: each
/// run's process group, so everything it started that stayed in that group.
pub fn stop_every_run() {
    let live: Vec<u32> = LIVE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for group in live {
        kill_group(group);
    }
}

/// `program` as a path: itself when it names one, else the first executable of that name on
/// the child's own `PATH`.
fn on_path(program: &str, env: &[(OsString, OsString)]) -> Option<PathBuf> {
    if program.contains('/') {
        return Some(PathBuf::from(program));
    }
    let path = env.iter().rev().find(|(k, _)| k == "PATH")?.1.clone();
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// The child's Seatbelt profile: the chat's own ([`crate::sandbox::seatbelt::profile`]) for
/// its folder, a temp directory of the child's own, and `ports` on loopback (its proxy's two,
/// carrying the chat's hosts, and each tunnel's, #1667), with a read of each credential file in
/// `files` given back after every denial, and nothing else of the vaults class.
pub(crate) fn profile_on(
    confines: &Confines,
    files: &[PathBuf],
    folder: &Path,
    tmp: &Path,
    ports: &[u16],
) -> Result<String, &'static str> {
    use crate::sandbox::seatbelt;
    let denied: &[Denial] = &confines.denied;
    // The chat's own widenings (#1337), so the run gets its certificate check and its cache
    // grants.
    let mut own = seatbelt::Own::default();
    seatbelt::widen(&mut own, &confines.widened)?;
    for file in files {
        own.after.push(format!(
            "(allow file-read* (literal {}))",
            seatbelt::quote(file)?
        ));
    }
    seatbelt::profile(denied, &own, folder, tmp, ports, None)
}

/// What [`run`] hears from the child's pipes and the asker.
enum Heard {
    Out(Vec<u8>),
    Err(Vec<u8>),
    OutDone,
    ErrDone,
}

/// Streams the child's output to `writer` as it comes, redacted, feeding it the asker's stdin,
/// and answers its status. A connection that goes away kills the child: nobody is left to see
/// what it does.
fn run(
    mut child: std::process::Child,
    group: &Live,
    reader: Box<dyn BufRead + Send>,
    writer: &mut dyn Write,
    secrets: &[String],
) -> i32 {
    let (tx, rx) = std::sync::mpsc::channel::<Heard>();
    let gone = Arc::new(AtomicBool::new(false));
    for (pipe, out) in [
        (
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
            true,
        ),
        (
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
            false,
        ),
    ] {
        let tx = tx.clone();
        let Some(mut pipe) = pipe else {
            let _ = tx.send(if out { Heard::OutDone } else { Heard::ErrDone });
            continue;
        };
        std::thread::spawn(move || {
            let mut buf = vec![0u8; A_CHUNK];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(if out { Heard::OutDone } else { Heard::ErrDone });
                        return;
                    }
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        let _ = tx.send(if out {
                            Heard::Out(chunk)
                        } else {
                            Heard::Err(chunk)
                        });
                    }
                }
            }
        });
    }
    drop(tx);
    listen(reader, child.stdin.take(), Arc::clone(&gone));

    let mut out = Redactor::new(secrets);
    let mut err = Redactor::new(secrets);
    let (mut out_done, mut err_done) = (false, false);
    let mut status: Option<i32> = None;
    // Once the child has ended, how long its pipes are read for: whatever held them was in its
    // group and is killed with it, so they close at once, and this bounds one that does not.
    let mut until: Option<std::time::Instant> = None;
    loop {
        if gone.load(Ordering::SeqCst) && status.is_none() {
            group.kill();
        }
        if status.is_none()
            && let Ok(Some(ended)) = child.try_wait()
        {
            status = Some(exec::exit_status(&ended));
            group.kill();
            until = Some(std::time::Instant::now() + PIPES_AFTER_EXIT);
        }
        let past = until.is_some_and(|until| std::time::Instant::now() > until);
        if (out_done && err_done) || past {
            if let Some(code) = status {
                if past {
                    let _ = frame_of(&out.finish(), Frame::Stdout, writer);
                    let _ = frame_of(&err.finish(), Frame::Stderr, writer);
                }
                return code;
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        let sent = match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Heard::Out(chunk)) => frame_of(&out.push(&chunk), Frame::Stdout, writer),
            Ok(Heard::Err(chunk)) => frame_of(&err.push(&chunk), Frame::Stderr, writer),
            Ok(Heard::OutDone) => {
                out_done = true;
                frame_of(&out.finish(), Frame::Stdout, writer)
            }
            Ok(Heard::ErrDone) => {
                err_done = true;
                frame_of(&err.finish(), Frame::Stderr, writer)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                out_done = true;
                err_done = true;
                Ok(())
            }
        };
        if sent.is_err() {
            gone.store(true, Ordering::SeqCst);
        }
    }
}

fn frame_of(
    bytes: &[u8],
    kind: fn(String) -> Frame,
    writer: &mut dyn Write,
) -> std::io::Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    send(writer, &kind(b64(bytes)))
}

/// Reads the asker's [`Input`] on a thread of its own, into `stdin` where the child has one,
/// and marks the asker `gone` when its connection ends.
fn listen(
    mut reader: Box<dyn BufRead + Send>,
    mut stdin: Option<std::process::ChildStdin>,
    gone: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        loop {
            let mut line = String::new();
            match (&mut reader).take(A_FRAME_IS_AT_MOST).read_line(&mut line) {
                Ok(0) | Err(_) => {
                    gone.store(true, Ordering::SeqCst);
                    return;
                }
                Ok(_) => {}
            }
            match serde_json::from_str::<Input>(&line) {
                Ok(Input::Stdin(bytes)) => {
                    if let Some(pipe) = stdin.as_mut()
                        && pipe.write_all(&unb64(&bytes)).is_err()
                    {
                        stdin = None;
                    }
                }
                // The child's stdin ends; the connection stays, and its end is the asker's.
                Ok(Input::StdinClosed) => stdin = None,
                Err(_) => {
                    gone.store(true, Ordering::SeqCst);
                    return;
                }
            }
        }
    });
}

/// [`super::redact`] over a stream: a value split across two chunks is still masked.
///
/// It holds back the bytes that could still be the start of a value, at most one byte short of
/// the longest, and never emits part of an occurrence that a later chunk could complete.
pub struct Redactor {
    secrets: Vec<String>,
    longest: usize,
    held: Vec<u8>,
}

impl Redactor {
    pub fn new(secrets: &[String]) -> Self {
        let secrets: Vec<String> = secrets.iter().filter(|s| !s.is_empty()).cloned().collect();
        let longest = secrets.iter().map(String::len).max().unwrap_or(0);
        Self {
            secrets,
            longest,
            held: Vec::new(),
        }
    }

    /// `chunk` added; what is safe to show now, redacted.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.held.extend_from_slice(chunk);
        let mut cut = self
            .held
            .len()
            .saturating_sub(self.longest.saturating_sub(1));
        // No occurrence is cut in two: one that begins before the cut and ends after it is
        // held back whole, and so is any that then straddles the new cut.
        loop {
            let mut moved = false;
            for secret in &self.secrets {
                let needle = secret.as_bytes();
                let first = cut.saturating_sub(needle.len() - 1);
                let straddling = (first..cut)
                    .find(|&at| at + needle.len() > cut && self.held[at..].starts_with(needle));
                if let Some(at) = straddling {
                    cut = at;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        let shown = super::redact(&self.held[..cut], &self.secrets);
        self.held.drain(..cut);
        shown
    }

    /// Everything still held, redacted: the stream has ended.
    pub fn finish(&mut self) -> Vec<u8> {
        let shown = super::redact(&self.held, &self.secrets);
        self.held.clear();
        shown
    }
}

#[cfg(all(test, unix))]
#[path = "brokered_tests.rs"]
mod tests;
