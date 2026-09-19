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

use std::time::Duration;

use charter_core::hookwire::{self, Report, SOCKET_ENV};
use charter_core::profiles::{self, ProfileSet, Source};
use charter_core::shown;
use charter_core::state::Event;
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
    /// Scaffold a fresh control plane here: charter.toml, baseline dirs, .gitignore.
    ///
    /// Additive and idempotent — never touches existing content. A path charter would write
    /// that is occupied by something it cannot safely touch, or that leads out of the plane,
    /// is named and left alone; everything else is still created, and the exit is 1.
    Init(InitCommand),
    /// Heal control-plane drift: create any missing baseline directory a newer charter
    /// expects. Idempotent and additive — existing content is never touched.
    Reinit,
    /// Workspaces: their vision, their memory, their todos.
    #[command(subcommand, alias = "ws")]
    Workspace(WorkspaceCommand),

    /// Harness profiles: which program a chat runs, and with what environment.
    #[command(subcommand)]
    Harness(HarnessCommand),

    /// Refresh inventory/repos.json from the plane's forges, then regenerate docs.
    Discover {
        /// Skip per-repo stack detection (faster).
        #[arg(long)]
        no_probe: bool,
        /// Do not regenerate docs afterward.
        #[arg(long)]
        no_docs: bool,
    },

    /// Clone repos on demand into a workspace, each on its own default branch.
    Clone {
        /// Repo name(s) or full path(s) from the inventory.
        repos: Vec<String>,
        /// The workspace to clone into.
        #[arg(short = 'w', long = "workspace")]
        workspace: String,
        /// Pin the clock the manifest's `updated_at` is stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },

    /// Fetch and fast-forward the clones in a workspace, skipping any that hold work.
    Sync {
        /// The workspace to sync.
        #[arg(short = 'w', long = "workspace", required_unless_present = "all")]
        workspace: Option<String>,
        /// Sync every workspace.
        #[arg(long, conflicts_with = "workspace")]
        all: bool,
    },

    /// Tell the app what a harness just did. Run by a harness's hooks, never by a person.
    ///
    /// It reads the harness's payload on stdin, says one thing on a socket the app owns, and
    /// gets out of the way. It decides nothing, refuses nothing and prints nothing: the
    /// guard that answers `pretooluse` is the Python charter's and is not this command.
    Hook {
        /// The harness's event, lowercased: `sessionstart`, `userpromptsubmit`,
        /// `notification`, `subagentstop`, `stop`, `sessionend`.
        name: String,

        /// The installed plugin's version. Taken and ignored.
        ///
        /// The charter plugin's `hooks/hooks.json` puts it on every one of its twelve hook
        /// commands (`charter hook sessionstart --plugin-version 0.62.1`). It means nothing
        /// to this binary — the skew check it feeds is the Python charter's — but refusing
        /// the flag would mean refusing every call the plugin actually makes.
        #[arg(long)]
        plugin_version: Option<String>,
    },
}

#[derive(Args)]
struct InitCommand {
    /// Forge this control plane tracks.
    #[arg(long, default_value = "gitlab", value_parser = charter_core::scaffold::FORGES)]
    forge: String,
    /// Group/org/user that owns the repos.
    #[arg(long)]
    owner: Option<String>,
    /// Self-hosted forge host (default: the forge's own public host).
    #[arg(long)]
    host: Option<String>,
    /// Also clone the git repo you are standing in into the first workspace.
    #[arg(long)]
    clone_this_repo: bool,
    /// Name of the generic front-door persona to scaffold and declare. Skipped if this
    /// plane already has personas.
    #[arg(long, value_name = "NAME", overrides_with = "no_front_door")]
    front_door: Option<String>,
    /// Scaffold no persona at all; the plane declares no front door.
    #[arg(long, overrides_with = "front_door")]
    no_front_door: bool,
}

/// What `charter init` and `reinit` said, each line with the glyph `charter/util.py` gives it,
/// on stderr — coloured only when stderr is a terminal, which is Python's rule too.
fn say(outcome: &charter_core::scaffold::Outcome) -> ExitCode {
    use charter_core::scaffold::Say;
    use std::io::IsTerminal;
    let colour = std::io::stderr().is_terminal();
    for line in &outcome.said {
        let (code, glyph, text) = match line {
            Say::Info(t) => ("36", "•", t),
            Say::Ok(t) => ("32", "✓", t),
            Say::Warn(t) => ("33", "!", t),
            Say::Err(t) => ("31", "✗", t),
        };
        if colour {
            eprintln!("\x1b[{code}m{glyph}\x1b[0m {text}");
        } else {
            eprintln!("{glyph} {text}");
        }
    }
    ExitCode::from(outcome.code)
}

/// Where `init` and `reinit` act: `charter/root.py:find_root_or_cwd`.
fn place() -> Result<charter_core::plane::Place, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    Ok(charter_core::plane::place(&cwd))
}

#[derive(Subcommand)]
enum HarnessCommand {
    /// Every profile charter read, the file it came from, and why any was refused.
    List,
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

/// Whether a word this binary does not own is a TOOL hook, where refusing means blocking.
///
/// **The namespace, not a list of words, and not a blanket rule either.** Both of those were
/// tried and both were wrong, each in the other's direction:
///
/// - A list of the nine words in `charter/hooks.py:_HANDLERS` fails OPEN on everything not in
///   it. `charter hook pretooluse-notebook` exited 1, which a harness logs and ignores, so the
///   day charter adds a matcher the Rust binary on PATH would allow that tool class silently.
/// - Blocking every unknown word instead fails the other way: `charter hook stopp`, a typo in
///   a settings file charter itself wrote, blocked the session from ENDING — which is the
///   exact hazard this whole file was written around.
///
/// The word says which hook it is. In the `pretooluse`/`posttooluse` namespace there is a tool
/// call to protect and blocking is the safe answer, for every matcher charter has and every
/// one it adds. Outside it there is nothing to protect and blocking can only wedge a session.
/// A review found both halves of this.
fn is_a_tool_hook(name: &str) -> bool {
    name.starts_with("pretooluse") || name.starts_with("posttooluse")
}

/// What a harness reads as "block".
const BLOCK: u8 = 2;

/// How long the payload on stdin is waited for.
///
/// **Two seconds, and it used to be 25 milliseconds.** The spec allows the whole call 50 ms
/// and this binary was measured at 1.8, so 25 looked generous — but a review measured an
/// 8 MB `UserPromptSubmit` (a pasted log) at 47 ms, and a payload written in two pieces
/// missed 25 ms every time. Reaching the deadline is not free: charter then cannot establish
/// which conversation the report is of, and before a chat has adopted a process that costs
/// the whole report.
///
/// This exists only against a harness that opens the hook's stdin and never writes, which
/// would otherwise hang the turn for good. Two seconds is well under the plugin's own 5 s
/// hook timeout, so the harness's deadline is still the one that fires first, and no ordinary
/// payload can reach this one.
const PAYLOAD_DEADLINE: Duration = Duration::from_secs(2);

/// Reads the harness's payload, or gives up on it.
///
/// On its own thread, because a read from a pipe nobody is writing to cannot be interrupted.
/// The thread is left behind when the deadline passes: the process is about to exit, and
/// waiting for it is the very thing being avoided.
fn payload() -> String {
    use std::io::Read;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::stdin().read_to_string(&mut text);
        let _ = tx.send(text);
    });
    rx.recv_timeout(PAYLOAD_DEADLINE).unwrap_or_default()
}

/// `charter hook <name>` — always succeeds, whatever went wrong.
///
/// **Never exit 2.** A harness reads 2 as "block": on `Stop` it makes the harness carry on
/// rather than end. Nothing charter draws is worth that, so every failure here is a silent 0
/// and the state the app draws is simply the last one it was told.
fn hook(name: &str, plugin_version: Option<&str>) -> ExitCode {
    let Some(event) = Event::parse(name) else {
        let tool = is_a_tool_hook(name);
        eprintln!(
            "charter: `{name}` is not one of this binary's events (sessionstart, \
             userpromptsubmit, notification, subagentstop, stop, sessionend){}. If a plugin \
             meant this, the `charter` it wants is the Python one — check which is first on \
             PATH.",
            if tool {
                ", and it names a tool hook, so the tool call is refused rather than allowed \
                 by a program that checked nothing"
            } else {
                ""
            }
        );
        // Blocking only where there is a tool call to protect. Everywhere else a refusal that
        // a harness reads as "block" would wedge the session instead of guarding anything.
        return if tool {
            ExitCode::from(BLOCK)
        } else {
            ExitCode::FAILURE
        };
    };
    // `--plugin-version` is written by the charter plugin's `hooks.json` and never by the
    // app, which invokes this binary by absolute path. So its presence says this process was
    // started by the Python plugin, and this binary answers far less than the Python charter
    // does for the same word — no persona charter, no memory, no tool-gate ceiling frozen
    // (charter#432). It still answers, because blocking `sessionstart` would be worse than
    // a session without its context; but it says so where a person will actually see it.
    //
    // `systemMessage` and not stderr: a zero-exit hook's stderr goes to a debug log nobody
    // reads. `charter/hooks.py` learned that the same way.
    //
    // Once per session, not once per hook. The plugin wires `sessionstart`,
    // `userpromptsubmit` and `stop`, so an ungated notice reaches the operator on every prompt
    // and every turn end — `charter/hooks.py:_queue_plugin_notices` carries "the gate that
    // keeps them to sessionstart" for exactly this reason.
    if plugin_version.is_some() && event == Event::SessionStart {
        println!(
            "{}",
            serde_json::json!({
                "systemMessage": format!(
                    "charter: the `charter` on PATH is the desktop app's binary, which \
                     answers `hook {name}` with session state only — no persona charter, no \
                     memory, and no tool-gate snapshot. The Python charter should be \
                     answering this."
                )
            })
        );
    }
    let Some(socket) = std::env::var_os(SOCKET_ENV) else {
        // No app started this session — the operator's own harness in a terminal, with the
        // hooks pointed here. There is nothing to tell.
        return ExitCode::SUCCESS;
    };
    if let Some(report) = Report::read(event, &payload(), &|name| std::env::var(name).ok())
        && let Err(why) = hookwire::send(std::path::Path::new(&socket), &report)
    {
        // The app may have quit while this session was still running, which is the ordinary
        // way for this to fail and is not the harness's business — hence the exit 0 below.
        //
        // But it is said, because a report that never arrives is otherwise invisible
        // everywhere: the chat simply stops changing, and there is nothing anywhere to look
        // at. A zero-exit hook's stderr goes to the harness's debug log, which costs the
        // operator nothing and is exactly where somebody debugging this would look.
        eprintln!(
            "charter: the app did not take this {} ({why})",
            event.word()
        );
    }
    ExitCode::SUCCESS
}

/// `discover`, `clone` and `sync`, or `None` for any other command.
fn repo_command(command: &Command) -> Option<ExitCode> {
    use charter_core::repocmd::{self, Say};

    let mut say = |line: Say| eprintln!("{line}");
    let root = match command {
        Command::Discover { .. } | Command::Clone { .. } | Command::Sync { .. } => match plane() {
            Ok(plane) => plane.root().to_path_buf(),
            Err(why) => {
                eprintln!("charter: {why}");
                return Some(ExitCode::FAILURE);
            }
        },
        _ => return None,
    };
    let code = match command {
        Command::Discover { no_probe, no_docs } => repocmd::discover::discover(
            &root,
            repocmd::discover::Options {
                no_probe: *no_probe,
                no_docs: *no_docs,
            },
            &mut say,
        ),
        Command::Clone {
            repos,
            workspace,
            now,
        } => {
            let now = match now {
                Some(text) => match text.parse::<chrono::NaiveDateTime>() {
                    // A naive stamp is LOCAL time, as `--now` is everywhere in this binary.
                    Ok(naive) => {
                        match chrono::TimeZone::from_local_datetime(&chrono::Local, &naive).single()
                        {
                            Some(local) => local.with_timezone(&chrono::Utc),
                            None => {
                                eprintln!("charter: --now names no single local instant");
                                return Some(ExitCode::FAILURE);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("charter: --now is not a local naive timestamp: {e}");
                        return Some(ExitCode::FAILURE);
                    }
                },
                None => chrono::Utc::now(),
            };
            // Who a manifest says last touched it. Python's `_author`: `$USER`, and a word
            // that says nobody knows rather than an empty field.
            let author = std::env::var("USER")
                .ok()
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| "unknown".to_string());
            repocmd::clone::clone(
                &repocmd::clone::Request {
                    root: &root,
                    ws: workspace,
                    repos,
                    now,
                    author: &author,
                },
                &mut say,
            )
        }
        Command::Sync { workspace, all } => {
            let scope = match (workspace, all) {
                (_, true) => repocmd::sync::Scope::All,
                (Some(ws), false) => repocmd::sync::Scope::One(ws),
                (None, false) => unreachable!("clap requires one of the two"),
            };
            repocmd::sync::sync(&root, scope, &mut say)
        }
        _ => return None,
    };
    Some(ExitCode::from(code))
}

fn run(command: Command) -> Result<(), String> {
    match command {
        // Answered in `main`, before this: it is the one command whose exit code is not a
        // plain success or failure, and clap must never be allowed to exit 2 in front of it.
        Command::Hook { .. }
        | Command::Init(_)
        | Command::Reinit
        | Command::Discover { .. }
        | Command::Clone { .. }
        | Command::Sync { .. } => {
            unreachable!("answered before run")
        }
        Command::Root => {
            println!("{}", plane()?.root().display());
        }
        Command::Harness(HarnessCommand::List) => {
            let root = plane()?.root().to_path_buf();
            // The git check runs HERE because a person typed this command; it never runs on
            // a config read, so no hook pays a git call per tool call.
            let check = profiles::ignore_check(&root);
            let set = profiles::with_ignore_check(profiles::current(&root), &check);
            eprint!("{}", harness_listing(&set, &check));
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
                    charter_core::memstore::forget(plane()?.root(), &ws.dir().join("todos"), slug)
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
                    if let Some(dup) = charter_core::memstore::duplicate_of(
                        plane()?.root(),
                        &ws.dir().join("todos"),
                        text,
                    ) {
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
    // **`Cli::parse` exits 2 on a bad command line, and 2 is the one code a harness reads as
    // "block".** A hook that exited 2 by accident would make a session unable to end, so this
    // binary answers for its own argv before clap can, whatever the command turns out to be.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return match err.exit_code() {
                0 => ExitCode::SUCCESS,
                _ => ExitCode::FAILURE,
            };
        }
    };
    // Before `run`, because its exit code is not a plain success or failure: a tool hook this
    // binary does not answer must BLOCK rather than be read as "allow".
    if let Command::Hook {
        name,
        plugin_version,
    } = &cli.command
    {
        return hook(name, plugin_version.as_deref());
    }
    // `init` and `reinit` say several lines of their own and choose their own exit status.
    match &cli.command {
        Command::Init(init) => {
            let args = charter_core::scaffold::InitArgs {
                forge: init.forge.clone(),
                owner: init.owner.clone().unwrap_or_default(),
                host: init.host.clone(),
                clone_this_repo: init.clone_this_repo,
                front_door: if init.no_front_door {
                    None
                } else {
                    Some(
                        init.front_door
                            .clone()
                            .unwrap_or_else(|| "steward".to_owned()),
                    )
                },
            };
            return match place() {
                Ok(place) => say(&charter_core::scaffold::init(&place, &args)),
                Err(message) => {
                    eprintln!("charter: {message}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::Reinit => {
            return match place() {
                Ok(place) => say(&charter_core::scaffold::reinit(&place)),
                Err(message) => {
                    eprintln!("charter: {message}");
                    ExitCode::FAILURE
                }
            };
        }
        _ => {}
    }
    // The repo commands speak line by line as they go — a clone is slow, and the line that
    // says which repo is being fetched is worth nothing once it has been — and choose their
    // own exit status, as their Python counterparts do.
    if let Some(code) = repo_command(&cli.command) {
        return code;
    }
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("charter: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The profile listing, as `charter harness list` prints it on stderr.
///
/// Built-ins first in registry order — a declared replacement keeps its kind's place — then
/// declared profiles by name. **Every width is measured from the cells about to be printed**
/// rather than guessed: a fixed `{:<28}` pads a short value and does nothing at all to a
/// long one, which pushes that row's remaining columns somewhere no other row's land and
/// stops the table being a table.
///
/// Every cell that came out of the file has already been contained by the time it arrives
/// here (`profiles::display`, `shown::readable`): a command is text a chat can write, and a
/// carriage return in one would otherwise redraw this line.
fn harness_listing(set: &ProfileSet, check: &profiles::IgnoreCheck) -> String {
    let order: Vec<String> = profiles::builtins().into_iter().map(|p| p.name).collect();
    let mut rows: Vec<&charter_core::profiles::Profile> = set.profiles().iter().collect();
    rows.sort_by_key(|p| {
        let place = order.iter().position(|name| *name == p.name);
        (place.is_none(), place.unwrap_or(0), p.name.clone())
    });

    let heads = ["NAME", "KIND", "COMMAND"];
    let body: Vec<[String; 3]> = rows
        .iter()
        .map(|p| [shown::short(&p.name), p.kind.clone(), profiles::display(p)])
        .collect();
    // Each column is its header, its widest cell, and the gap to the next one — counted
    // inside the width so a caller pads once rather than padding and then adding spaces.
    // Every cell is printable ASCII by now, so a character is a column.
    let widths: Vec<usize> = heads
        .iter()
        .enumerate()
        .map(|(i, head)| {
            body.iter()
                .map(|row| row[i].chars().count())
                .chain(std::iter::once(head.chars().count()))
                .max()
                .unwrap_or(0)
                + 2
        })
        .collect();

    let line = |mark: &str, cells: [&str; 3], last: &str| {
        let mut out = mark.to_owned();
        for (cell, width) in cells.iter().zip(&widths) {
            out.push_str(cell);
            out.extend(std::iter::repeat_n(
                ' ',
                width.saturating_sub(cell.chars().count()),
            ));
        }
        out.push_str(last);
        format!("{}\n", out.trim_end())
    };

    let mut out = line("  ", heads, "FROM");
    for (p, cells) in rows.iter().zip(&body) {
        // The row the selector starts on, marked. It launches nothing by itself.
        let mark = if set.default.as_deref() == Some(p.name.as_str()) {
            "* "
        } else {
            "  "
        };
        let source = match p.source {
            Source::BuiltIn => Source::BuiltIn.as_str(),
            Source::Local => Source::Local.as_str(),
        };
        out.push_str(&line(mark, [&cells[0], &cells[1], &cells[2]], source));
    }
    if !set.refused.is_empty() {
        out.push_str("refused:\n");
        for refused in &set.refused {
            // A whole-file refusal has no profile name, so it is named by its FILE rather
            // than printing a line that starts with a bare colon.
            let who = if refused.name.is_empty() {
                &refused.source
            } else {
                &refused.name
            };
            out.push_str(&format!("  {who}: {}\n", refused.reason));
        }
    }
    if !check.fix.is_empty() {
        out.push_str(&format!(
            "! to use the profiles in {}: {}\n",
            profiles::LOCAL_FILE,
            check.fix
        ));
    }
    out
}
