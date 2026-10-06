//! `charter secret exec` — `commands_secrets.cmd_secret_exec`: run a command with secrets
//! injected as environment variables and/or temp files, then redact.
//!
//! The model builds the command out of variable NAMES, and charter resolves the values inside
//! its own process. What the child does with a value after that is the child's: redaction is a
//! literal search over the bytes charter captured, so a child that TRANSFORMS a value hands
//! back something redaction cannot recognise. Read `secret exec <vault> -- <cmd>` with the
//! same suspicion as `<cmd>` holding the credential directly, because that is what it is.
//!
//! Three modes:
//!
//! - **capture** (default): the child's stdout and stderr are captured, redacted, then printed;
//! - **`--stream`**: the child inherits stdio and charter waits, then deletes the temp files —
//!   for a long-running child whose credential must be a FILE. Nothing is captured, so nothing
//!   is redacted;
//! - **`--exec`**: charter REPLACES itself with the child. Nothing survives to delete a temp
//!   file, so `--file`/`--dotenv` are refused with it.
//!
//! **Temp files** are created 0600 in the system temp directory and registered for removal
//! BEFORE a byte is written. Every exit from the point the first is created — a return, an
//! error, and every terminating signal charter can catch — unwinds through one cleanup
//! ([`Cleanup`]). SIGKILL cannot be caught, and a fault (SIGSEGV, SIGABRT…) is deliberately
//! not intercepted; either leaves the 0600 file behind until it is removed or the machine
//! reboots. Removal is an unlink: no overwrite pass, which means nothing on a copy-on-write
//! filesystem.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use super::cmd::{self, Io, Say};
use super::registry;
use super::{Ctx, dotenv};

/// What `secret exec` was asked to do.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub vault: String,
    pub env: Vec<String>,
    pub file: Vec<String>,
    pub dotenv: Vec<String>,
    pub stream: bool,
    pub exec: bool,
    pub command: Vec<String>,
}

/// Every temp file this call created, removed when it goes out of scope — on a return, on an
/// error, and on the way out after a caught signal.
#[derive(Default)]
struct Cleanup {
    paths: Vec<PathBuf>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// `_child_env`: this process's environment, minus every OTHER vault's declared identity
/// variables. The vault being read keeps its own names.
///
/// No fallback when the registry cannot be read: a fallback to the whole environment would
/// quietly restore every other vault's credential to the child. (The registry was read a moment
/// earlier to find this vault, so in practice this does not fail.)
fn child_env(ctx: &Ctx, vault: &str) -> Result<Vec<(OsString, OsString)>, super::VaultError> {
    child_env_of(ctx, vault, ctx.env.vars())
}

/// [`child_env`] of `vars`, an environment other than this process's: the one a brokered run
/// was handed by the chat that asked ([`super::brokered`]).
pub(crate) fn child_env_of(
    ctx: &Ctx,
    vault: &str,
    vars: &[(OsString, OsString)],
) -> Result<Vec<(OsString, OsString)>, super::VaultError> {
    let doc = registry::load_registry(ctx)?;
    let declared = registry::identity_vars(&doc);
    let own: Vec<String> = declared
        .iter()
        .find(|(n, _)| n == vault)
        .map(|(_, names)| names.clone())
        .unwrap_or_default();
    let strip: Vec<&String> = declared
        .iter()
        .flat_map(|(_, names)| names.iter())
        .filter(|n| !own.contains(n))
        .collect();
    Ok(vars
        .iter()
        .filter(|(k, _)| !strip.iter().any(|s| k.as_os_str() == s.as_str()))
        .cloned()
        .collect())
}

pub(crate) fn set_var(env: &mut Vec<(OsString, OsString)>, name: &str, value: impl Into<OsString>) {
    let value = value.into();
    match env.iter_mut().find(|(k, _)| k == name) {
        Some(slot) => slot.1 = value,
        None => env.push((OsString::from(name), value)),
    }
}

/// A temp file named `charter-secret-<random>`, 0600, registered for removal before `bytes` are
/// written into it. Never named after the vault or the key: a name is visible to every account
/// that can list the temp directory, and a key name is a user's word that could shape a path.
///
/// In the system temp directory, or in `within` where it is given: a brokered run keeps its
/// files where no chat can read them ([`super::brokered`]).
fn temp_file(
    cleanup: &mut Cleanup,
    within: Option<&std::path::Path>,
    bytes: &[u8],
) -> std::io::Result<PathBuf> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("charter-secret-").rand_bytes(12);
    let made = match within {
        Some(dir) => builder.tempfile_in(dir),
        None => builder.tempfile(),
    };
    let (mut file, path) = made?.keep().map_err(|e| e.error)?;
    cleanup.paths.push(path.clone());
    file.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

/// What the child is handed, resolved: its environment, every value to redact, the names
/// recorded, and the temp files made, removed when this is dropped.
pub(crate) struct Prepared {
    pub env: Vec<(OsString, OsString)>,
    pub secret_values: Vec<String>,
    pub key_names: Vec<String>,
    pub env_names: Vec<String>,
    /// The temp files, in the order they were made.
    pub files: Vec<PathBuf>,
    _cleanup: Cleanup,
}

/// Why [`prepare`] stopped: a terminating signal arrived (`128+N` is the status), or a refusal
/// with its status and its sentence.
pub(crate) enum Stopped {
    Signal(i32),
    Said(i32, String),
}

/// Resolves `req`'s `--env`, `--file` and `--dotenv` from `v` into `env`, making each temp file
/// in `within` (the system temp directory where it is `None`). `caught` is asked after every
/// resolution whether a terminating signal arrived, and if one did nothing more is resolved.
pub(crate) fn prepare(
    ctx: &Ctx,
    v: &registry::Vault,
    req: &Request,
    mut env: Vec<(OsString, OsString)>,
    within: Option<&std::path::Path>,
    caught: &dyn Fn() -> Option<i32>,
) -> Result<Prepared, Stopped> {
    let mut secret_values: Vec<String> = Vec::new();
    let mut key_names: Vec<String> = Vec::new();
    let mut env_names: Vec<String> = Vec::new();
    // Declared before the first temp file and dropped after the child is gone, on every path.
    let mut cleanup = Cleanup::default();

    // One value, and then the question every resolution has to be followed by: did a
    // terminating signal arrive meanwhile? If so nothing is started — the credential already
    // read goes no further than this process, and the files made so far are removed.
    let resolve = |key: &str| -> Result<String, Stopped> {
        let got = cmd::get_value(ctx, v, key);
        if let Some(sig) = caught() {
            return Err(Stopped::Signal(128 + sig));
        }
        got.map_err(|e| Stopped::Said(1, e.message))
    };

    for spec in &req.env {
        let (name, key) = match spec.split_once('=') {
            Some((n, k)) if !n.is_empty() => (n, k),
            _ => {
                return Err(Stopped::Said(
                    2,
                    format!("--env expects NAME=key, got '{spec}'"),
                ));
            }
        };
        let val = resolve(key)?;
        set_var(&mut env, name, val.clone());
        secret_values.push(val);
        key_names.push(key.to_string());
        env_names.push(name.to_string());
    }
    for spec in &req.file {
        let (name, key) = match spec.split_once('=') {
            Some((n, k)) if !n.is_empty() => (n, k),
            _ => {
                return Err(Stopped::Said(
                    2,
                    format!("--file expects ENVVAR=key, got '{spec}'"),
                ));
            }
        };
        let val = resolve(key)?;
        secret_values.push(val.clone());
        key_names.push(key.to_string());
        env_names.push(name.to_string());
        match temp_file(&mut cleanup, within, val.as_bytes()) {
            Ok(path) => set_var(&mut env, name, path.into_os_string()),
            Err(e) => {
                return Err(Stopped::Said(
                    1,
                    format!("cannot write the temp file for --file {name}: {e}"),
                ));
            }
        }
    }

    // --dotenv ENVVAR=NAME:key, repeatable; entries sharing an ENVVAR merge into one file, in
    // flag order, so a consumer wanting several secrets gets exactly one path.
    let mut grouped: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for spec in &req.dotenv {
        let parsed = spec.split_once('=').and_then(|(envvar, entry)| {
            let (name, key) = entry.split_once(':')?;
            (!envvar.is_empty() && !name.is_empty() && !key.is_empty())
                .then(|| (envvar.to_string(), name.to_string(), key.to_string()))
        });
        let Some((envvar, name, key)) = parsed else {
            return Err(Stopped::Said(
                2,
                format!("--dotenv expects ENVVAR=NAME:key, got '{spec}'"),
            ));
        };
        let slot = match grouped.iter().position(|(e, _)| *e == envvar) {
            Some(i) => i,
            None => {
                grouped.push((envvar.clone(), Vec::new()));
                grouped.len() - 1
            }
        };
        if grouped[slot].1.iter().any(|(n, _)| *n == name) {
            return Err(Stopped::Said(
                2,
                format!(
                    "--dotenv defines '{name}' twice for {envvar}; which value wins would be up \
                     to the reader of the file. Use one entry per name."
                ),
            ));
        }
        grouped[slot].1.push((name, key));
    }
    for (envvar, entries) in &grouped {
        let mut lines: Vec<String> = Vec::new();
        env_names.push(envvar.clone());
        for (name, key) in entries {
            let val = resolve(key)?;
            secret_values.push(val.clone());
            key_names.push(key.clone());
            let escaped = dotenv::escaped(&val);
            if escaped != val {
                secret_values.push(escaped);
            }
            match dotenv::line(name, &val) {
                Ok(line) => lines.push(line),
                Err(e) => return Err(Stopped::Said(2, e.0)),
            }
        }
        let body = format!("{}\n", lines.join("\n"));
        match temp_file(&mut cleanup, within, body.as_bytes()) {
            Ok(path) => set_var(&mut env, envvar, path.into_os_string()),
            Err(e) => {
                return Err(Stopped::Said(
                    1,
                    format!("cannot write the temp file for --dotenv {envvar}: {e}"),
                ));
            }
        }
    }
    Ok(Prepared {
        env,
        secret_values,
        files: cleanup.paths.clone(),
        key_names,
        env_names,
        _cleanup: cleanup,
    })
}

/// The one record of a run handed its credentials (`secret-exec`): the vault, the key and
/// variable names, the program and the mode, and `also` — never a value.
pub(crate) fn record(
    ctx: &Ctx,
    vault: &str,
    prepared: &Prepared,
    argv0: &str,
    mode: &str,
    also: &[(&str, Value)],
) {
    let mut keys_sorted = prepared.key_names.clone();
    keys_sorted.sort();
    keys_sorted.dedup();
    let mut envs_sorted = prepared.env_names.clone();
    envs_sorted.sort();
    envs_sorted.dedup();
    let mut fields: Vec<(&str, Value)> = vec![
        ("vault", Value::String(vault.to_owned())),
        (
            "key_names",
            Value::Array(keys_sorted.into_iter().map(Value::String).collect()),
        ),
        (
            "env_names",
            Value::Array(envs_sorted.into_iter().map(Value::String).collect()),
        ),
        ("argv0", Value::String(argv0.to_owned())),
        ("mode", Value::String(mode.into())),
    ];
    fields.extend(also.iter().cloned());
    cmd::trace_secret_use(ctx, "secret-exec", &prepared.secret_values, &fields);
}

/// The usage line a request with no command is refused with.
pub(crate) const NO_COMMAND: &str =
    "No command given. Usage: purlis secret exec <vault> --env NAME=key -- <command...>";

/// `cmd_secret_exec`.
pub fn exec(ctx: &Ctx, req: &Request, io: &mut dyn Io) -> i32 {
    let mut command = req.command.clone();
    if command.first().is_some_and(|c| c == "--") {
        command.remove(0);
    }
    // Inside a sandboxed chat the app runs it, and this process never reads the vault
    // (ADR 0067 §5 class 1, #1407). Only where no app takes it does it run here.
    if !command.is_empty()
        && !(req.exec && req.stream)
        && let Some(code) = super::brokered::forwarded(ctx, req, &command, io)
    {
        return code;
    }
    let v = match cmd::provider(ctx, &req.vault) {
        Ok(v) => v,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    if command.is_empty() {
        io.say(Say::Err(NO_COMMAND.into()));
        return 2;
    }
    if req.exec && req.stream {
        io.say(Say::Err(
            "--exec and --stream are two ways to run the same command; pick one. --stream is the \
             one that can clean up a --file credential."
                .into(),
        ));
        return 2;
    }
    if req.exec && (!req.file.is_empty() || !req.dotenv.is_empty()) {
        let flags: Vec<&str> = [
            ("--file", !req.file.is_empty()),
            ("--dotenv", !req.dotenv.is_empty()),
        ]
        .iter()
        .filter(|(_, on)| *on)
        .map(|(f, _)| *f)
        .collect();
        io.say(Say::Err(format!(
            "{} cannot be combined with --exec: exec replaces this process, so the temp file \
             would never be cleaned up. Use --stream instead — it forks, inherits stdio, and \
             deletes the file when the child exits.",
            flags.join(" and ")
        )));
        return 2;
    }

    let env = match child_env(ctx, &req.vault) {
        Ok(env) => env,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    let signals = Termination::install();
    let prepared = match prepare(ctx, &v, req, env, None, &|| signals.caught()) {
        Ok(prepared) => prepared,
        Err(Stopped::Signal(code)) => return code,
        Err(Stopped::Said(code, why)) => {
            io.say(Say::Err(why));
            return code;
        }
    };

    // The last point before anything is started or recorded as handed out.
    if let Some(sig) = signals.caught() {
        return 128 + sig;
    }
    cmd::say_held_note(ctx, &v, io);

    // ONE record, above the three ways the child is started: everything that runs a command
    // passes through here, and `--exec` never comes back to record anything after.
    let mode = if req.exec {
        "exec"
    } else if req.stream {
        "stream"
    } else {
        "capture"
    };
    record(ctx, &req.vault, &prepared, &command[0], mode, &[]);

    let mut child = Command::new(&command[0]);
    child
        .args(&command[1..])
        .env_clear()
        .envs(prepared.env.iter().map(|(k, v)| (k, v)));

    if req.exec {
        drop(signals);
        return replace_process(child, &command[0], io);
    }

    if req.stream {
        let spawned = match crate::forklock::spawn(&mut child) {
            Ok(c) => c,
            Err(e) => return not_started(&command[0], &e, io),
        };
        let code = supervise(spawned, &signals, CTRL_C_GRACE);
        drop(prepared);
        return code;
    }

    child.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut spawned = match crate::forklock::spawn(&mut child) {
        Ok(c) => c,
        Err(e) => return not_started(&command[0], &e, io),
    };
    let mut out_pipe = spawned.stdout.take().expect("stdout was piped");
    let mut err_pipe = spawned.stderr.take().expect("stderr was piped");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });
    let code = supervise(spawned, &signals, CTRL_C_GRACE);
    if code >= 128 && signals.caught().is_some() {
        // Died on a signal charter caught: the child is gone and nothing it said is printed.
        drop(prepared);
        return code;
    }
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    let out = super::redact(&out, &prepared.secret_values);
    let err = super::redact(&err, &prepared.secret_values);
    if !out.is_empty() {
        io.out(&out);
    }
    if !err.is_empty() {
        io.err(&err);
    }
    drop(prepared);
    code
}

/// A child that could not be started: `command not found` and 127 when there is no such
/// program, as Python's `FileNotFoundError` arm says; any other failure named, exit 1.
pub(crate) fn not_started(program: &str, e: &std::io::Error, io: &mut dyn Io) -> i32 {
    if e.kind() == std::io::ErrorKind::NotFound {
        io.say(Say::Err(format!("command not found: {program}")));
        127
    } else {
        io.say(Say::Err(format!("cannot run {program}: {e}")));
        1
    }
}

/// `--exec`: replace this process with the child, stdio untouched. Returns only when the
/// replacement failed — `command not found`, 127, for every such failure, as Python's does.
///
/// Windows cannot replace a process; there the child runs with stdio inherited and its status
/// is this one's, which is what `os.execvpe` does there.
///
/// One function with two platform arms rather than two functions of one name: cargo-mutants
/// names a mutant by its function, and the arm this platform does not compile would otherwise
/// carry the same three names as the one the tests catch, unkillable and unexcludable apart.
fn replace_process(mut child: Command, program: &str, io: &mut dyn Io) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = child.exec();
    }
    #[cfg(not(unix))]
    if let Ok(status) = crate::forklock::status(&mut child) {
        return super::run::exit_code(&status);
    }
    io.say(Say::Err(format!("command not found: {program}")));
    127
}

/// How long a child is given to finish after a Ctrl-C before it is killed: a quarter of a
/// second, as Python's `subprocess` gives it.
const CTRL_C_GRACE: Duration = Duration::from_millis(250);

/// Wait for `child`, and if a terminating signal arrives first, kill it and return `128+N`.
///
/// A SIGINT is given `grace` first — the child got the same Ctrl-C from the terminal. Always
/// [`CTRL_C_GRACE`] in the product; a parameter so that a test of the grace is not failed by a
/// loaded machine taking longer than a quarter second to run a shell's last line (#465).
fn supervise(mut child: std::process::Child, signals: &Termination, grace: Duration) -> i32 {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return exit_status(&status),
            Ok(None) => {}
            Err(_) => return 1,
        }
        if let Some(sig) = signals.caught() {
            if sig == Termination::SIGINT {
                let deadline = std::time::Instant::now() + grace;
                // `<` and `<=` differ only at the one instant equal to the deadline, which a
                // 10 ms poll cannot land on (`.cargo/mutants.toml` excludes that mutant).
                while std::time::Instant::now() < deadline {
                    if let Ok(Some(_)) = child.try_wait() {
                        return 128 + sig;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            return 128 + sig;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The exit status charter passes through: the child's own, or — for a child killed by
/// signal `N` — what Python's `sys.exit(-N)` leaves, `256 - N`.
pub(crate) fn exit_status(status: &std::process::ExitStatus) -> i32 {
    let code = super::run::exit_code(status);
    // `code <= 0` would be the same function — it differs only at 0, where `(256 + 0) & 0xff`
    // is 0 as well, and an exit code is never above 255 — so `.cargo/mutants.toml` excludes it.
    if code < 0 { (256 + code) & 0xff } else { code }
}

/// The terminating signals charter takes over while a temp file may exist, so that a death
/// by one of them runs the cleanup — `_exit_on_termination`.
///
/// Left alone, as Python leaves them: SIGKILL and SIGSTOP (cannot be caught), the ones whose
/// default is ignore or stop (job control keeps working), SIGPIPE, and the faults — a handler
/// on a process whose state is already wrong can turn a crash into a hang.
struct Termination {
    #[cfg(unix)]
    flag: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(unix)]
    ids: Vec<signal_hook::SigId>,
}

impl Termination {
    #[cfg(unix)]
    const SIGINT: i32 = signal_hook::consts::SIGINT;
    #[cfg(not(unix))]
    const SIGINT: i32 = 2;

    #[cfg(unix)]
    fn signals() -> Vec<i32> {
        use signal_hook::consts::*;
        #[allow(unused_mut)]
        let mut out = vec![
            SIGINT, SIGHUP, SIGQUIT, SIGTERM, SIGALRM, SIGUSR1, SIGUSR2, SIGXCPU, SIGXFSZ,
            SIGVTALRM, SIGPROF,
        ];
        #[cfg(any(target_os = "linux", target_os = "android"))]
        out.extend([SIGIO, libc::SIGPWR, libc::SIGSTKFLT]);
        out
    }

    fn install() -> Self {
        #[cfg(unix)]
        {
            let flag = super::run::interrupt_flag();
            flag.store(0, std::sync::atomic::Ordering::SeqCst);
            let mut ids = Vec::new();
            for sig in Self::signals() {
                let f = std::sync::Arc::clone(&flag);
                let value = sig as usize;
                if let Ok(id) = signal_hook::flag::register_usize(sig, f, value) {
                    ids.push(id);
                }
            }
            Self { flag, ids }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }

    /// The signal that arrived, if one did.
    fn caught(&self) -> Option<i32> {
        #[cfg(unix)]
        {
            match self.flag.load(std::sync::atomic::Ordering::SeqCst) {
                0 => None,
                n => Some(n as i32),
            }
        }
        #[cfg(not(unix))]
        {
            None
        }
    }
}

impl Drop for Termination {
    fn drop(&mut self) {
        #[cfg(unix)]
        for id in self.ids.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
