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
    Ok(ctx
        .env
        .vars()
        .iter()
        .filter(|(k, _)| !strip.iter().any(|s| k.as_os_str() == s.as_str()))
        .cloned()
        .collect())
}

fn set_var(env: &mut Vec<(OsString, OsString)>, name: &str, value: impl Into<OsString>) {
    let value = value.into();
    match env.iter_mut().find(|(k, _)| k == name) {
        Some(slot) => slot.1 = value,
        None => env.push((OsString::from(name), value)),
    }
}

/// A temp file named `charter-secret-<random>`, 0600, registered for removal before `bytes` are
/// written into it. Never named after the vault or the key: a name is visible to every account
/// that can list the temp directory, and a key name is a user's word that could shape a path.
fn temp_file(cleanup: &mut Cleanup, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let (mut file, path) = tempfile::Builder::new()
        .prefix("charter-secret-")
        .rand_bytes(12)
        .tempfile()?
        .keep()
        .map_err(|e| e.error)?;
    cleanup.paths.push(path.clone());
    file.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

/// `cmd_secret_exec`.
pub fn exec(ctx: &Ctx, req: &Request, io: &mut dyn Io) -> i32 {
    let v = match cmd::provider(ctx, &req.vault) {
        Ok(v) => v,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    let mut command = req.command.clone();
    if command.first().is_some_and(|c| c == "--") {
        command.remove(0);
    }
    if command.is_empty() {
        io.say(Say::Err(
            "No command given. Usage: charter secret exec <vault> --env NAME=key -- <command...>"
                .into(),
        ));
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

    let mut env = match child_env(ctx, &req.vault) {
        Ok(env) => env,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    let mut secret_values: Vec<String> = Vec::new();
    let mut key_names: Vec<String> = Vec::new();
    let mut env_names: Vec<String> = Vec::new();
    // Declared before the first temp file and dropped after the child is gone, on every path.
    let mut cleanup = Cleanup::default();
    let signals = Termination::install();

    // One value, and then the question every resolution has to be followed by: did a
    // terminating signal arrive meanwhile? If so nothing is started — the credential already
    // read goes no further than this process, and the files made so far are removed.
    macro_rules! resolve {
        ($key:expr) => {{
            let got = cmd::get_value(ctx, &v, $key);
            if let Some(sig) = signals.caught() {
                return 128 + sig;
            }
            match got {
                Ok(val) => val,
                Err(e) => {
                    io.say(Say::Err(e.message));
                    return 1;
                }
            }
        }};
    }

    for spec in &req.env {
        let (name, key) = match spec.split_once('=') {
            Some((n, k)) if !n.is_empty() => (n, k),
            _ => {
                io.say(Say::Err(format!("--env expects NAME=key, got '{spec}'")));
                return 2;
            }
        };
        let val = resolve!(key);
        set_var(&mut env, name, val.clone());
        secret_values.push(val);
        key_names.push(key.to_string());
        env_names.push(name.to_string());
    }
    for spec in &req.file {
        let (name, key) = match spec.split_once('=') {
            Some((n, k)) if !n.is_empty() => (n, k),
            _ => {
                io.say(Say::Err(format!("--file expects ENVVAR=key, got '{spec}'")));
                return 2;
            }
        };
        let val = resolve!(key);
        secret_values.push(val.clone());
        key_names.push(key.to_string());
        env_names.push(name.to_string());
        match temp_file(&mut cleanup, val.as_bytes()) {
            Ok(path) => set_var(&mut env, name, path.into_os_string()),
            Err(e) => {
                io.say(Say::Err(format!(
                    "cannot write the temp file for --file {name}: {e}"
                )));
                return 1;
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
            io.say(Say::Err(format!(
                "--dotenv expects ENVVAR=NAME:key, got '{spec}'"
            )));
            return 2;
        };
        let slot = match grouped.iter().position(|(e, _)| *e == envvar) {
            Some(i) => i,
            None => {
                grouped.push((envvar.clone(), Vec::new()));
                grouped.len() - 1
            }
        };
        if grouped[slot].1.iter().any(|(n, _)| *n == name) {
            io.say(Say::Err(format!(
                "--dotenv defines '{name}' twice for {envvar}; which value wins would be up to the \
                 reader of the file. Use one entry per name."
            )));
            return 2;
        }
        grouped[slot].1.push((name, key));
    }
    for (envvar, entries) in &grouped {
        let mut lines: Vec<String> = Vec::new();
        env_names.push(envvar.clone());
        for (name, key) in entries {
            let val = resolve!(key);
            secret_values.push(val.clone());
            key_names.push(key.clone());
            let escaped = dotenv::escaped(&val);
            if escaped != val {
                secret_values.push(escaped);
            }
            match dotenv::line(name, &val) {
                Ok(line) => lines.push(line),
                Err(e) => {
                    io.say(Say::Err(e.0));
                    return 2;
                }
            }
        }
        let body = format!("{}\n", lines.join("\n"));
        match temp_file(&mut cleanup, body.as_bytes()) {
            Ok(path) => set_var(&mut env, envvar, path.into_os_string()),
            Err(e) => {
                io.say(Say::Err(format!(
                    "cannot write the temp file for --dotenv {envvar}: {e}"
                )));
                return 1;
            }
        }
    }

    // The last point before anything is started or recorded as handed out.
    if let Some(sig) = signals.caught() {
        return 128 + sig;
    }

    // ONE record, above the three ways the child is started: everything that runs a command
    // passes through here, and `--exec` never comes back to record anything after.
    let mut keys_sorted = key_names.clone();
    keys_sorted.sort();
    keys_sorted.dedup();
    let mut envs_sorted = env_names.clone();
    envs_sorted.sort();
    envs_sorted.dedup();
    let mode = if req.exec {
        "exec"
    } else if req.stream {
        "stream"
    } else {
        "capture"
    };
    cmd::trace_secret_use(
        ctx,
        "secret-exec",
        &secret_values,
        &[
            ("vault", Value::String(req.vault.clone())),
            (
                "key_names",
                Value::Array(keys_sorted.into_iter().map(Value::String).collect()),
            ),
            (
                "env_names",
                Value::Array(envs_sorted.into_iter().map(Value::String).collect()),
            ),
            ("argv0", Value::String(command[0].clone())),
            ("mode", Value::String(mode.into())),
        ],
    );

    let mut child = Command::new(&command[0]);
    child
        .args(&command[1..])
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)));

    if req.exec {
        drop(signals);
        return replace_process(child, &command[0], io);
    }

    if req.stream {
        let spawned = match crate::forklock::spawn(&mut child) {
            Ok(c) => c,
            Err(e) => return not_started(&command[0], &e, io),
        };
        let code = supervise(spawned, &signals);
        drop(cleanup);
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
    let code = supervise(spawned, &signals);
    if code >= 128 && signals.caught().is_some() {
        // Died on a signal charter caught: the child is gone and nothing it said is printed.
        drop(cleanup);
        return code;
    }
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    let out = super::redact(&out, &secret_values);
    let err = super::redact(&err, &secret_values);
    if !out.is_empty() {
        io.out(&out);
    }
    if !err.is_empty() {
        io.err(&err);
    }
    drop(cleanup);
    code
}

/// A child that could not be started: `command not found` and 127 when there is no such
/// program, as Python's `FileNotFoundError` arm says; any other failure named, exit 1.
fn not_started(program: &str, e: &std::io::Error, io: &mut dyn Io) -> i32 {
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
#[cfg(unix)]
fn replace_process(mut child: Command, program: &str, io: &mut dyn Io) -> i32 {
    use std::os::unix::process::CommandExt;
    let _ = child.exec();
    io.say(Say::Err(format!("command not found: {program}")));
    127
}

/// Windows cannot replace a process; the child runs with stdio inherited and its status is
/// this one's, which is what `os.execvpe` does there.
#[cfg(not(unix))]
fn replace_process(mut child: Command, program: &str, io: &mut dyn Io) -> i32 {
    match crate::forklock::status(&mut child) {
        Ok(status) => super::run::exit_code(&status),
        Err(_) => {
            io.say(Say::Err(format!("command not found: {program}")));
            127
        }
    }
}

/// Wait for `child`, and if a terminating signal arrives first, kill it and return `128+N`.
///
/// A SIGINT is given a quarter of a second first — the child got the same Ctrl-C from the
/// terminal, and Python's `subprocess` waits that long before killing it.
fn supervise(mut child: std::process::Child, signals: &Termination) -> i32 {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return exit_status(&status),
            Ok(None) => {}
            Err(_) => return 1,
        }
        if let Some(sig) = signals.caught() {
            if sig == Termination::SIGINT {
                let deadline = std::time::Instant::now() + Duration::from_millis(250);
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
fn exit_status(status: &std::process::ExitStatus) -> i32 {
    let code = super::run::exit_code(status);
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
