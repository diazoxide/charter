//! The `charter` command line, in Rust.
//!
//! Only the plane commands M1.1 covers are here. Each one is the Python command of the same
//! name, and the differential tests (`tests/differential/run.py`) prove that by running both
//! against copies of one fixture plane and comparing the trees they leave.

use std::path::PathBuf;
use std::process::ExitCode;

use charter_core::workspaces::Plane;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "charter",
    version,
    about = "charter: run tons of harness sessions in parallel"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the plane root the current directory sits in.
    Root,
    /// Workspaces: their vision, their memory, their todos.
    #[command(subcommand, alias = "ws")]
    Workspace(WorkspaceCommand),
}

#[derive(Subcommand)]
enum WorkspaceCommand {
    /// List the plane's workspaces, one per line.
    List,
    /// Set a workspace's `## Vision`.
    Vision {
        text: String,
        #[command(flatten)]
        common: Common,
    },
    /// Record one durable fact in a workspace's journal.
    Remember {
        text: String,
        #[command(flatten)]
        common: Common,
    },
    /// Record a todo, list them, or close one with `done <slug>`.
    ///
    /// `done`/`forget` are read as verbs rather than as todo text, and told apart by the
    /// shape of the call rather than the word: one positional records, two close. charter's
    /// own parser does exactly this, and a real subcommand cannot.
    Todo {
        #[arg(num_args = 0..=2)]
        words: Vec<String>,
        #[command(flatten)]
        common: Common,
    },
}

#[derive(Args)]
struct Common {
    /// The workspace to act on.
    #[arg(short = 'w', long = "workspace")]
    workspace: String,
    /// Pin the clock a write stamps itself with, for tests only. charter's own
    /// `memstore.write` takes a `stamp=` for the same reason: ordering has to be testable
    /// across real time gaps, not just within one second.
    #[arg(long, hide = true)]
    now: Option<String>,
}

impl Common {
    fn stamp(&self) -> Result<chrono::NaiveDateTime, String> {
        match &self.now {
            Some(text) => text
                .parse()
                .map_err(|e| format!("--now is not a local naive timestamp: {e}")),
            None => Ok(chrono::Local::now().naive_local()),
        }
    }
}

fn plane() -> Result<Plane, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    // `$CHARTER_ROOT` wins over the walk up, as it does in Python charter: it is how a
    // caller pins the plane rather than inheriting whichever one the cwd sits in.
    if let Some(root) = std::env::var_os("CHARTER_ROOT") {
        return Ok(Plane::open(PathBuf::from(root)));
    }
    charter_core::plane::find_root(&cwd)
        .map(Plane::open)
        .map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    match Cli::parse().command {
        Command::Root => {
            println!("{}", plane()?.root().display());
        }
        Command::Workspace(WorkspaceCommand::List) => {
            for name in plane()?.workspaces().map_err(|e| e.to_string())? {
                println!("{name}");
            }
        }
        Command::Workspace(WorkspaceCommand::Vision { text, common }) => {
            plane()?
                .workspace(&common.workspace)
                .set_vision(&text)
                .map_err(|e| e.to_string())?;
        }
        Command::Workspace(WorkspaceCommand::Remember { text, common }) => {
            plane()?
                .workspace(&common.workspace)
                .remember(&text, common.stamp()?)
                .map_err(|e| e.to_string())?;
        }
        Command::Workspace(WorkspaceCommand::Todo { words, common }) => {
            let ws = plane()?.workspace(&common.workspace);
            let stamp = common.stamp()?;
            match words.as_slice() {
                [] => {
                    for todo in ws.todos().map_err(|e| e.to_string())? {
                        println!("{}  {}", todo.slug, todo.title);
                    }
                }
                [verb, slug] if verb == "done" => {
                    ws.close_todo(slug, stamp).map_err(|e| e.to_string())?;
                }
                [text] => {
                    ws.add_todo(text, stamp).map_err(|e| e.to_string())?;
                }
                _ => return Err("usage: charter ws todo [-w WS] [\"<text>\" | done <slug>]".into()),
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("charter: {message}");
            ExitCode::FAILURE
        }
    }
}
