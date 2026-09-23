//! Running a vendor's CLI (`op`, `vault`) the way `charter/util.py:run` does for a vault.
//!
//! - **The environment is an OVERLAY** on this process's: the child still needs `PATH` and
//!   `HOME`, and a vault's identity (`OP_SERVICE_ACCOUNT_TOKEN`) is added for this one call
//!   and never set on charter itself — a mutated environment would outlive the call and apply
//!   to the next vault, which is the identity confusion the binding exists to prevent.
//! - **stdin is the only way a value reaches the CLI.** Without input the child gets
//!   `/dev/null`, never charter's own stdin: a CLI that reads it gets EOF instead of blocking
//!   on a descriptor nobody will write to (#324).
//! - **Output is captured, never inherited**, and callers never interpolate it into a message.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::Env;

/// What a CLI said and how it exited.
#[derive(Debug, Clone, Default)]
pub struct Ran {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Why a CLI could not be run to completion.
#[derive(Debug)]
pub enum RunError {
    /// It ran past its timeout and was killed — `util.ProcTimeout`.
    Timeout,
    /// It could not be started at all.
    Spawn(std::io::Error),
}

/// `shutil.which(name, path=PATH)`: the first executable regular file called `name` on
/// `path`. A name holding a `/` is checked as it stands.
pub fn which(name: &str, path: Option<&str>) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        let p = PathBuf::from(name);
        return executable(&p).then_some(p);
    }
    let path = path?;
    for dir in path.split(':') {
        let dir = if dir.is_empty() { "." } else { dir };
        let candidate = Path::new(dir).join(name);
        if executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        p.is_file() && rustix::fs::access(p, rustix::fs::Access::EXEC_OK).is_ok()
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Run `argv` with `overlay` added to `env`, `input` on stdin (else `/dev/null`), capturing
/// both streams, bounded by `timeout` when there is one.
pub fn run(
    env: &Env,
    argv: &[String],
    input: Option<&str>,
    overlay: &[(String, String)],
    timeout: Option<Duration>,
) -> Result<Ran, RunError> {
    let program =
        which(&argv[0], env.get("PATH").as_deref()).unwrap_or_else(|| PathBuf::from(&argv[0]));
    let mut command = Command::new(program);
    command.args(&argv[1..]).env_clear();
    for (k, v) in env.vars() {
        command.env(k, v);
    }
    for (k, v) in overlay {
        command.env(k, v);
    }
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = crate::forklock::spawn(&mut command).map_err(RunError::Spawn)?;
    let writer = input.map(|text| {
        let mut pipe = child.stdin.take().expect("stdin was piped");
        let text = text.to_owned();
        std::thread::spawn(move || {
            let _ = pipe.write_all(text.as_bytes());
        })
    });
    let mut out_pipe = child.stdout.take().expect("stdout was piped");
    let mut err_pipe = child.stderr.take().expect("stderr was piped");
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
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(RunError::Spawn(e)),
        }
        if timeout.is_some_and(|t| started.elapsed() >= t) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if let Some(w) = writer {
        let _ = w.join();
    }
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Ok(Ran {
        code: exit_code(&status),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

/// `returncode` as Python reports it: the exit status, or `-N` for a death by signal `N`.
pub fn exit_code(status: &std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return -sig;
        }
    }
    1
}
