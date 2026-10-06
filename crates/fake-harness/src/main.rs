//! `fake-harness`: stands in for Claude Code or Codex where a test or a benchmark needs a
//! harness that behaves the same way every run.
//!
//! It writes its output (a recorded corpus, or synthetic harness-shaped output), then an
//! optional sentinel line a benchmark can wait for, then runs its hooks in order, the way a
//! harness fires `Notification` and `Stop`. With `--interactive` it then answers each typed
//! line until `/quit`.
//!
//! With `--wait-for-input` it holds the output back until a line is typed, so a benchmark can
//! have the pane on screen and the right size before the first byte is written — otherwise
//! output that arrived before the pane was watching reaches it as a snapshot of the screen,
//! and a throughput number would be measuring the wrong thing.

mod acp;
mod synthetic;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

use clap::{ArgGroup, Parser};

#[derive(Parser)]
#[command(
    version,
    about = "A stand-in harness for scenario tests and benchmarks"
)]
#[command(group(ArgGroup::new("output").args(["corpus", "synthetic"])))]
struct Args {
    /// Replay this file's bytes exactly as recorded.
    #[arg(long)]
    corpus: Option<PathBuf>,

    /// Write this many bytes of synthetic harness-shaped output instead.
    #[arg(long)]
    synthetic: Option<usize>,

    /// Bytes per write.
    #[arg(long, default_value_t = 4096)]
    chunk: usize,

    /// Pause between writes, in milliseconds.
    #[arg(long, default_value_t = 0)]
    interval_ms: u64,

    /// Replay the output this many times.
    #[arg(long, default_value_t = 1)]
    loops: usize,

    /// Write nothing until a line is typed.
    #[arg(long)]
    wait_for_input: bool,

    /// A line written once the output is done, for a benchmark to wait for.
    #[arg(long)]
    sentinel: Option<String>,

    /// A shell command run after the output, in order; repeat for several.
    #[arg(long = "hook")]
    hooks: Vec<String>,

    /// After the hooks, answer each typed line until `/quit`.
    #[arg(long)]
    interactive: bool,

    /// With `--interactive`: put the terminal in raw mode first (`stty raw -echo`), as a real
    /// harness does at its prompt, echo each byte typed and answer a line on Enter — so a test
    /// can see text typed into it and tell whether it was sent (FM-9).
    #[arg(long, requires = "interactive")]
    raw: bool,

    /// The exit code once everything else has run.
    #[arg(long, default_value_t = 0)]
    exit_code: u8,

    /// Be a scripted ACP agent on stdio instead (`acp.rs`), and do nothing else.
    #[arg(long)]
    acp: bool,

    /// `acp`, as the last word: the same as `--acp`, the way opencode's own ACP mode is
    /// `opencode acp`, so a profile of kind opencode can run this as its program at level 3.
    #[arg(value_parser = ["acp"])]
    mode: Option<String>,

    /// With `--acp`: append each message the client sends to this file.
    #[arg(long, requires = "acp")]
    acp_record: Option<PathBuf>,

    /// With `--acp`: the protocol version to answer `initialize` with.
    #[arg(long, requires = "acp", default_value_t = 1)]
    acp_version: u64,

    /// With `--acp`: answer `session/new` that a login is needed.
    #[arg(long, requires = "acp")]
    acp_login: bool,

    /// Say whether this process has a controlling terminal, leave it as the host does
    /// (`purlis_core::noterminal::leave`), open a terminal pair, and say again; nothing else.
    #[arg(long)]
    leave_terminal: bool,

    /// With `--acp`: read every message and answer none.
    #[arg(long, requires = "acp")]
    acp_silent: bool,

    /// With `--acp`: first start a process outside this one's group that holds its stdout.
    #[arg(long, requires = "acp")]
    acp_escape: bool,

    /// Start this program and wait for it, and nothing else: a parent that, like tauri-cli's
    /// `dev`, ends on Ctrl-C without ending its child. Says the child's pid.
    #[arg(long, num_args = 1.., allow_hyphen_values = true, value_name = "PROGRAM")]
    parent_of: Vec<String>,

    /// With `--leave-terminal`: say this process's pid, and stay a while once left.
    #[arg(long, requires = "leave_terminal")]
    linger: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::from(args.exit_code),
        Err(message) => {
            eprintln!("fake-harness: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), String> {
    if let Some((program, rest)) = args.parent_of.split_first() {
        let mut child = Command::new(program)
            .args(rest)
            .spawn()
            .map_err(|err| format!("cannot start {program}: {err}"))?;
        println!("child pid {}", child.id());
        child
            .wait()
            .map_err(|err| format!("cannot wait for {program}: {err}"))?;
        return Ok(());
    }
    if args.leave_terminal {
        return leave_terminal(args.linger);
    }
    if args.acp || args.mode.is_some() {
        return acp::serve(&acp::Script {
            record: args.acp_record.clone(),
            version: args.acp_version,
            login: args.acp_login,
            silent: args.acp_silent,
            escape: args.acp_escape,
        });
    }
    let output = match (&args.corpus, args.synthetic) {
        (Some(path), _) => {
            std::fs::read(path).map_err(|err| format!("cannot read {}: {err}", path.display()))?
        }
        (None, Some(len)) => synthetic::generate(len),
        (None, None) => Vec::new(),
    };

    let mut stdout = io::stdout().lock();
    let write_err = |err: io::Error| format!("cannot write output: {err}");
    if args.wait_for_input {
        let mut line = String::new();
        io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|err| format!("cannot read input: {err}"))?;
    }
    for _ in 0..args.loops {
        for chunk in output.chunks(args.chunk.max(1)) {
            stdout.write_all(chunk).map_err(write_err)?;
            stdout.flush().map_err(write_err)?;
            if args.interval_ms > 0 {
                thread::sleep(Duration::from_millis(args.interval_ms));
            }
        }
    }
    if let Some(sentinel) = &args.sentinel {
        write!(stdout, "\r\n{sentinel}\r\n")
            .and_then(|()| stdout.flush())
            .map_err(write_err)?;
    }

    for hook in &args.hooks {
        let status = Command::new("/bin/sh")
            .args(["-c", hook])
            .status()
            .map_err(|err| format!("cannot run hook {hook:?}: {err}"))?;
        if !status.success() {
            return Err(format!("hook {hook:?} failed: {status}"));
        }
    }

    if args.interactive && args.raw {
        return answer_raw(&mut stdout);
    }
    if args.interactive {
        for line in io::stdin().lock().lines() {
            let line = line.map_err(|err| format!("cannot read input: {err}"))?;
            if line.trim() == "/quit" {
                break;
            }
            writeln!(stdout, "you said: {line}")
                .and_then(|()| stdout.flush())
                .map_err(write_err)?;
        }
    }
    Ok(())
}

/// `--interactive --raw`: the terminal raw, each byte echoed as it is typed, and a line answered
/// only when Enter (a carriage return) arrives — what "sent" means to a harness at its prompt.
fn answer_raw(stdout: &mut impl Write) -> Result<(), String> {
    use std::io::Read;
    let status = Command::new("stty")
        .args(["raw", "-echo"])
        .stdin(std::process::Stdio::inherit())
        .status()
        .map_err(|err| format!("cannot run stty: {err}"))?;
    if !status.success() {
        return Err(format!("stty raw -echo failed: {status}"));
    }
    let write_err = |err: io::Error| format!("cannot write output: {err}");
    let mut line = Vec::new();
    for byte in io::stdin().lock().bytes() {
        let byte = byte.map_err(|err| format!("cannot read input: {err}"))?;
        if byte == b'\r' || byte == b'\n' {
            let said = String::from_utf8_lossy(&line).into_owned();
            if said.trim() == "/quit" {
                break;
            }
            write!(stdout, "\r\nyou said: {said}\r\n").map_err(write_err)?;
            line.clear();
        } else {
            line.push(byte);
            stdout.write_all(&[byte]).map_err(write_err)?;
        }
        stdout.flush().map_err(write_err)?;
    }
    Ok(())
}

/// `--leave-terminal`: what `/dev/tty` answers before and after leaving, and after opening a
/// terminal pair the way the host does.
fn leave_terminal(linger: bool) -> Result<(), String> {
    use purlis_core::noterminal::{self, Left};
    let said = |when: &str| {
        let has = if noterminal::has_one() { "yes" } else { "no" };
        println!("terminal {when}: {has}");
    };
    said("before");
    if linger {
        println!("started as pid {}", std::process::id());
    }
    // A level-3 chat refuses to start while the host still has its terminal.
    let refused = purlis_core::acp::Chat::start(
        purlis_core::acp::Launch {
            chat: "probe".to_owned(),
            argv: vec!["true".to_owned()],
            cwd: std::env::temp_dir(),
            env: Vec::new(),
            charter_mcp: None,
            patience: std::time::Duration::from_secs(5),
        },
        std::sync::Arc::new(purlis_core::harness::asks::Asks::new()),
    )
    .err();
    if noterminal::has_one() {
        let refused = refused == Some(purlis_core::acp::NotStarted::Terminal);
        println!(
            "acp with a terminal refused: {}",
            if refused { "yes" } else { "no" }
        );
    }
    match noterminal::leave().map_err(|err| format!("cannot leave the terminal: {err}"))? {
        Left::Relaunched(code) => std::process::exit(code),
        Left::NoneToLeave | Left::NewSession => {}
    }
    said("after");
    let pair = portable_pty::native_pty_system()
        .openpty(portable_pty::PtySize::default())
        .map_err(|err| format!("cannot open a terminal: {err}"))?;
    said("after a pair");
    drop(pair);
    if linger {
        println!("left as pid {}", std::process::id());
        std::thread::sleep(Duration::from_secs(60));
    }
    Ok(())
}
