//! The `charter` command line, in Rust.
//!
//! Only the plane commands M1.1 covers are here, and only as far as the PLANE goes. The
//! differential tests (`tests/differential/run.py`) prove that by running both
//! implementations against copies of one fixture plane and comparing the trees they leave.
//!
//! Two things this binary deliberately does NOT do yet, both recorded in the harness rather
//! than left to be discovered:
//!
//! - **`-w` is required.** Python resolves a workspace through nine rungs — `-w`,
//!   `$CHARTER_WORKSPACE`, the working directory, the session and terminal pointers, the
//!   frame, `workspaces/.default`, `[workspace] default`, then the literal `default`
//!   (`charter/workspace.py:589` `chosen`). None of that is ported, so omitting `-w` is a
//!   usage error rather than a guess at the wrong workspace.
//! - **No command prints its confirmation.** charter says `✓ Vision set for 'alpha' → …` on
//!   stderr; this is silent. That is the command presentation layer, and porting it is M2's
//!   "the `charter` binary answers these commands".
//!
//! `root` and the hidden `--now` have no Python counterpart at all: `--now` is the test seam
//! charter itself has as `memstore.write(stamp=…)`.

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
    /// Show or set a workspace's `## Vision`.
    ///
    /// With text, replace it; without, print it. Empty text is the SHOWING form, as it is
    /// in Python charter — `if text:` there, so `vision ""` prints rather than erasing a
    /// committed, hand-edited file.
    Vision {
        text: Option<String>,
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

/// The workspace every plane starts on, whether or not its directory exists.
const DEFAULT_WORKSPACE: &str = "default";

fn plane() -> Result<Plane, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    charter_core::plane::resolve(&cwd)
        .map(Plane::open)
        .map_err(|e| e.to_string())
}

/// The workspace a command names, refusing a name that cannot be one.
fn workspace(common: &Common) -> Result<charter_core::workspaces::Workspace, String> {
    plane()?
        .workspace(&common.workspace)
        .map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    match Cli::parse().command {
        Command::Root => {
            println!("{}", plane()?.root().display());
        }
        Command::Workspace(WorkspaceCommand::List) => {
            let mut names = plane()?.workspaces().map_err(|e| e.to_string())?;
            // `default` is always listable, whether or not the directory is there: it is the
            // one every plane starts on, and charter adds it the same way.
            if !names.iter().any(|n| n == DEFAULT_WORKSPACE) {
                names.push(DEFAULT_WORKSPACE.to_string());
                names.sort();
            }
            for name in names {
                println!("{name}");
            }
        }
        Command::Workspace(WorkspaceCommand::Vision { text, common }) => {
            let ws = workspace(&common)?;
            // A workspace charter does not have is not one this scaffolds: `vision` shows or
            // replaces, and inventing the directory is what made a bad `-w` silent.
            if !ws.dir().is_dir() {
                return Err(format!("no workspace '{}'", common.workspace));
            }
            match text.as_deref().filter(|t| !t.is_empty()) {
                Some(text) => ws.set_vision(text).map_err(|e| e.to_string())?,
                None => {
                    let vision = ws.vision();
                    if !vision.is_empty() {
                        println!("{vision}");
                    }
                }
            }
        }
        Command::Workspace(WorkspaceCommand::Remember { text, common }) => {
            workspace(&common)?
                .remember(&text, common.stamp()?)
                .map_err(|e| e.to_string())?;
        }
        Command::Workspace(WorkspaceCommand::Todo { words, common }) => {
            let ws = workspace(&common)?;
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
                // `forget` abandons a todo silently — no journal entry, unlike `done`.
                [verb, slug] if verb == "forget" => {
                    charter_core::memstore::forget(&ws.dir().join("todos"), slug)
                        .map_err(|e| e.to_string())?;
                }
                // A lone verb is NOT todo text. charter refuses it, and the reason is that
                // `todo done` with a forgotten slug would otherwise record a todo called
                // "done" and leave the one it meant to close open.
                [verb] if verb == "done" || verb == "forget" => {
                    return Err(format!(
                        "`todo {verb}` needs the slug of the todo to close."
                    ));
                }
                [text] => {
                    // Duplicate INTENT is worse than duplicate memory: closing one of a
                    // near-identical pair leaves its twin looking outstanding, so the list
                    // starts lying about what is left. Warn and skip rather than merge.
                    if let Some(dup) =
                        charter_core::memstore::duplicate_of(&ws.dir().join("todos"), text)
                    {
                        return Err(format!("already on the list: {dup}"));
                    }
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
