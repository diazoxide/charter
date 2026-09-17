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

    /// The exit code once everything else has run.
    #[arg(long, default_value_t = 0)]
    exit_code: u8,
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
