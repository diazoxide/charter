//! The `charter` command line, in Rust.
//!
//! Only the plane commands M1.1 covers are here, and only as far as the PLANE goes. The
//! differential tests (`tests/differential/run.py`) prove that by running both
//! implementations against copies of one fixture plane and comparing the trees they leave.
//!
//! One thing this binary deliberately does NOT do yet, recorded in the harness rather than
//! left to be discovered:
//!
//! - **Not every command prints its confirmation.** charter says `✓ Vision set for 'alpha' →
//!   …` on stderr, and `vision` and `todo` here are still silent. The memory commands
//!   (`memory.rs`) are ported whole, their output included, and so is the one line of stdout
//!   `workspace current` and `persona current` print — the sentence explaining which rung
//!   decided is not.
//!
//! **`-w` and `--persona` are optional, and M2.9 is what made them so.** Every rung of both
//! resolution ladders lives in [`charter_core::active`]; this file only decides which flag
//! feeds each one. Until then `charter recall` with no flags — how a harness calls it at
//! session start — refused with exit **2**, and every `-w` was a clap usage error.
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

mod handoff;
mod memory;
mod statusline;
mod voice;

/// One line of a ported command, in the voice charter says it in.
///
/// [`charter_core::repocmd::Say`] carries the mark and the message as a value so a test can
/// read them back; this is the one place they become the coloured line `charter/util.py`
/// prints. `eprintln!("{line}")` would print the mark uncoloured, which is right in a pipe
/// and wrong in a terminal.
fn speak(line: charter_core::repocmd::Say) {
    use charter_core::repocmd::Say;
    match line {
        Say::Info(text) => voice::info(&text),
        Say::Done(text) => voice::ok(&text),
        Say::Warn(text) => voice::warn(&text),
        Say::Fail(text) => voice::err(&text),
        // `raise SystemExit(message)`, which prints the message as it is — on stderr, where
        // every other mark goes.
        Say::Plain(text) => eprintln!("{text}"),
        // The command's ANSWER, on stdout, for the script reading it.
        Say::Out(text) => println!("{text}"),
    }
}

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
        /// The workspace to clone into (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
        /// Pin the clock the manifest's `updated_at` is stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },

    /// Commit and push the control plane's own changes over its forge's HTTPS token.
    ///
    /// It stages EVERYTHING pending in the plane's tree, not only what you changed, and prints
    /// the directory breakdown of what it is about to commit before it commits it.
    Save {
        /// The commit message. Default: `charter save: N file(s)`.
        message: Option<String>,
        /// Sign the commit. Off by default, so a signer prompt can never hang an agent.
        #[arg(long)]
        sign: bool,
        /// Commit only; do not push.
        #[arg(long)]
        no_push: bool,
    },

    /// Golden rule 0: check — or `--apply` — token-only git auth on the plane and every clone.
    #[command(name = "git-policy")]
    GitPolicy {
        /// Write the policy. Without it, drift is reported and nothing is changed.
        #[arg(long)]
        apply: bool,
    },

    /// Fetch and fast-forward the clones in a workspace, skipping any that hold work.
    Sync {
        /// The workspace to sync (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
        /// Sync every workspace.
        #[arg(long, conflicts_with = "workspace")]
        all: bool,
    },

    /// The one memory gate: search/list across ALL bases (a workspace + a persona's own +
    /// shared), each hit labeled by source.
    Recall(memory::RecallArgs),

    /// Personas: their memory.
    #[command(subcommand)]
    Persona(memory::PersonaCommand),

    /// Preflight: check the plane, its workspaces, personas and profiles before working.
    ///
    /// Exits non-zero only on a blocker. Every check this charter does not run yet is still
    /// listed, as a warning saying it was not checked — never as a pass.
    Doctor {
        /// Emit machine-readable results.
        #[arg(long)]
        json: bool,
        /// Run as the SessionStart hook does: no harness-profile probe and no git call for
        /// one. Every other check runs.
        #[arg(long)]
        preflight: bool,
        /// Install what charter can install for this plane before reporting. Not in this
        /// charter yet: refused, so nobody reads the report as the state after a repair.
        #[arg(long)]
        fix: bool,
    },

    /// Refresh the forge state the CI column is drawn from: each clone's open PR/MR and the
    /// last pipeline on the branch it is actually on.
    ///
    /// **This is the process that holds the forge credential, and it draws nothing.** The
    /// panels read the file it writes and can never fetch, which is deliberate: a fetch on a
    /// render path puts a forge token in the process that draws the window.
    #[command(name = "gl-refresh")]
    GlRefresh {
        /// The workspace to refresh (default: the active one).
        ///
        /// Resolved through [`charter_core::active`] since M2.9, and resolved BEFORE
        /// `--detach` rather than after: the child is handed the name this process worked
        /// out, so a refresh cannot end up keyed to a different workspace than the one the
        /// operator was standing in.
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
        /// Return at once and refresh in a process that outlives this one.
        ///
        /// What a hook's `async` used to buy, done by charter — one harness skips async hooks
        /// outright.
        #[arg(long)]
        detach: bool,
        /// Pin the instant every entry is stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },

    /// Claude Code's footer, from the per-turn JSON on stdin.
    ///
    /// Inside the app it prints an empty line and still records the turn's token usage, which
    /// is the only place that record exists (ADR 0019). Everywhere else it draws the frame and
    /// the workspace's identity row, and says in the body which surfaces it does not draw yet.
    Statusline {
        /// Repaint in place until Ctrl-C, on a harness with no status bar of its own.
        ///
        /// Taken and answered with the same one line, because there is no render to repeat
        /// yet. Accepted rather than refused: a plane wired for `charter statusline --watch`
        /// must not meet a usage error from a `charter` that appeared first on PATH.
        #[arg(long)]
        watch: bool,
        /// Seconds between repaints with --watch.
        ///
        /// Taken and ignored, for the same reason `--watch` is: there is nothing to repaint
        /// yet. Refusing the flag would refuse a command line a plane already has.
        #[allow(dead_code)]
        #[arg(long, default_value = "10")]
        interval: f64,
        /// Pin the instant the footer's ages are measured from, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },

    /// What a version brought, and what this plane has not adopted.
    News(NewsCommand),

    /// Adopt what a newer charter brought. This command does NOT install one.
    ///
    /// charter's own `update` moves a Python package with `uv tool install`; this charter is a
    /// binary inside the app, and the app is what moves it. So `update` here is the half that
    /// is about this plane's content: what the versions it skipped brought, and what it has not
    /// taken up. `--to` and `--bump` are taken and refused by name rather than rejected as
    /// unknown flags, because an agent that typed one is owed the reason.
    Update {
        /// Install exactly this version. Refused: nothing here installs.
        #[arg(long)]
        to: Option<String>,
        /// Also move this plane's pin. Refused: the pin names a published charter-cp release.
        #[arg(long)]
        bump: bool,
    },

    /// Open a chat in a workspace you name, already working on a brief you pass as a quoted
    /// heredoc on stdin. Your harness asks before it runs.
    ///
    /// **This charter cannot open a chat** — it has no frame and no channel into the app —
    /// so every call reaches the frame refusal and is told the command to run in a new
    /// terminal. Every refusal in front of that one is ported and is the point: charter
    /// refuses every shape the permission prompt in front of this command cannot stand in
    /// front of (`charter_core::handoff`).
    Handoff {
        /// Where the chat opens — an existing workspace, or a new one with --create. Always
        /// named, this workspace included.
        workspace: String,
        /// Make the workspace first (LOCAL, never LIVE). Needs --vision.
        #[arg(long)]
        create: bool,
        /// What the new workspace is for, one line. A workspace with no vision is never
        /// proposed as a handoff target.
        #[arg(long)]
        vision: Option<String>,
        /// Pin the new chat's persona. Without it the chat gets whatever a new chat in that
        /// workspace gets.
        #[arg(long)]
        persona: Option<String>,
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

#[derive(Args, Clone)]
struct NewsCommand {
    /// Every entry, any version, whose probe says you have not adopted it yet.
    #[arg(long)]
    pending: bool,
    /// Report entries newer than this version.
    #[arg(long)]
    since: Option<String>,
    /// Stop at this version (default: the newest one this build ships an entry for).
    #[arg(long)]
    until: Option<String>,
    /// One version's entries, as the body of its release notes.
    #[arg(long = "for", value_name = "VERSION")]
    for_version: Option<String>,
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

/// Each line in the voice `charter/util.py` gives it, through the one module that owns those
/// four glyphs — so `news` and `init` cannot come out looking like two different programs.
fn say_lines(said: &[charter_core::scaffold::Say]) {
    use charter_core::scaffold::Say;
    for line in said {
        match line {
            Say::Info(text) => voice::info(text),
            Say::Ok(text) => voice::ok(text),
            Say::Warn(text) => voice::warn(text),
            Say::Err(text) => voice::err(text),
        }
    }
}

/// What `charter init` and `reinit` said, and the status they chose.
fn say(outcome: &charter_core::scaffold::Outcome) -> ExitCode {
    say_lines(&outcome.said);
    ExitCode::from(outcome.code)
}

/// stdout first, then stderr, then the status — the order the two streams are written in
/// matters only for a terminal, and this is the one charter writes in.
fn emit(report: &charter_core::news::Report) -> ExitCode {
    use std::io::Write;
    print!("{}", report.out);
    let _ = std::io::stdout().flush();
    say_lines(&report.said);
    ExitCode::from(report.code)
}

/// This charter's subcommands, as `news` needs them to decide what a `check:` may name.
///
/// Read off clap rather than written down, which is what `cli._subcommand_names` does with
/// argparse — and a list written down is a list the next command added to the CLI is missing
/// from. **Aliases are children too**: argparse's `choices` holds one key per alias, so `ws` is
/// a subcommand name there, and a tree without it would read `ws …` as a command this charter
/// does not have.
fn command_tree() -> charter_core::news::CommandTree {
    use charter_core::news::CommandTree;
    use clap::CommandFactory;

    fn walk(cmd: &clap::Command) -> Vec<CommandTree> {
        let mut out = Vec::new();
        for sub in cmd.get_subcommands() {
            let children = walk(sub);
            out.push(CommandTree {
                name: sub.get_name().to_owned(),
                children: children.clone(),
            });
            for alias in sub.get_all_aliases() {
                out.push(CommandTree {
                    name: alias.to_owned(),
                    children: children.clone(),
                });
            }
        }
        out
    }

    let root = Cli::command();
    CommandTree {
        name: root.get_name().to_owned(),
        children: walk(&root),
    }
}

/// The charter a news probe dispatches into: this process, running this binary's commands.
///
/// **In-process is what makes a dozen probes cheap enough to run on demand; it was never what
/// made them safe.** What makes a probe safe is `news::PROBEABLE` — a list of command paths a
/// human has confirmed read rather than act. Two of the four are wired here: `news`, whose own
/// probe is then refused for probing, and `doctor` (M2.4), which reads the plane and reports.
/// `persona lint` and `frame-probe` are commands this binary does not have, and an entry naming
/// one is refused with its own sentence before it reaches this function.
struct Probes {
    tree: charter_core::news::CommandTree,
}

impl charter_core::news::Dispatch for Probes {
    fn tree(&self) -> &charter_core::news::CommandTree {
        &self.tree
    }

    fn run(&self, tokens: &[String]) -> Option<i32> {
        // Only a path in `news::PROBEABLE` reaches here, and `charter` is implied rather than
        // written — so it is put back to parse the line as a command line.
        let argv: Vec<String> = std::iter::once("charter".to_owned())
            .chain(tokens.iter().cloned())
            .collect();
        // Python catches `SystemExit` around `parse_args` and reports no exit code; a parse
        // this binary refuses is the same nothing.
        let parsed = Cli::try_parse_from(&argv).ok()?;
        match parsed.command {
            Command::News(ref news) => Some(i32::from(news_report(news, self).code)),
            Command::Doctor { preflight, fix, .. } => {
                // **A probe reads; it does not act.** `PROBEABLE` lists the command PATH and
                // leaves the flags to the entry, which is charter's rule and is right — flags
                // are how a `check:` asks a narrower question. `--fix` is the one flag that
                // asks a different KIND of question, and this binary refuses it and exits 1;
                // read as an answer that would be `pending`, which is a chore invented out of
                // a probe that never ran. No exit code worth reading, so none is given.
                if fix {
                    return None;
                }
                let cwd = std::env::current_dir().ok()?;
                // The rows, not the table: this is the in-process equivalent of charter
                // redirecting a probe's stdout, and a probe that printed its report into
                // `news --pending`'s output would be its own kind of wrong.
                let rows = charter_core::doctor::Doctor::new(&cwd, preflight).run();
                Some(i32::from(charter_core::doctor::exit_code(&rows)))
            }
            // A listed command this binary does not have never gets this far — `news::tokens`
            // refuses an unregistered first token with its own sentence. A listed one it grows
            // later and does not wire in here would, and `None` is the honest answer for it:
            // "no exit code worth reading" rather than a guess.
            _ => None,
        }
    }
}

/// `charter news`, in the order `commands.cmd_news` asks its questions: the release gate first,
/// then the pending view, then the range.
fn news_report(
    cmd: &NewsCommand,
    d: &dyn charter_core::news::Dispatch,
) -> charter_core::news::Report {
    use charter_core::news;

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let place = charter_core::plane::place(&cwd);
    if let Some(version) = cmd.for_version.as_deref().filter(|v| !v.is_empty()) {
        return news::for_release(version);
    }
    if cmd.pending {
        return news::pending_report(d, place.is_plane);
    }
    news::range_report(
        cmd.since.as_deref().unwrap_or_default(),
        cmd.until.as_deref().unwrap_or_default(),
        d,
        place.is_plane,
    )
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
    /// Print the active workspace — the name alone.
    ///
    /// It takes no `-w`, because charter's own `workspace current` takes none: the top rung
    /// of the ladder is typed on the command it acts on, and a flag here would report a
    /// resolution that nothing performed.
    Current,
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
    /// Record one workspace memory (its own file, indexed) — the task journal. Omit the
    /// text to list the workspace's memories.
    Remember {
        text: Option<String>,
        /// Optional title (else derived from the first line).
        #[arg(long)]
        title: Option<String>,
        /// Don't reactively commit+push it now (LIVE workspaces; sync later).
        #[arg(long)]
        no_sync: bool,
        #[command(flatten)]
        common: Common,
    },
    /// Alias for `remember` — record a workspace memory (or list them).
    Note {
        message: Option<String>,
        /// Don't reactively commit+push it now (LIVE workspaces; sync later).
        #[arg(long)]
        no_sync: bool,
        #[command(flatten)]
        common: Common,
    },
    /// Search the workspace's memories (--query) or list them all.
    Recall {
        /// Keyword query; omit to list every memory chronologically.
        #[arg(short = 'q', long)]
        query: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Delete one workspace memory by slug or filename.
    Forget {
        /// Memory slug or filename (see `charter workspace recall`).
        slug: String,
        #[command(flatten)]
        common: Common,
    },
    /// Curate a workspace's memory: collapse exact duplicates and repair the index with
    /// --apply; propose the rest.
    Optimize {
        /// Workspace to optimize (default: every one).
        name: Option<String>,
        /// Every workspace (the default).
        #[arg(long)]
        all: bool,
        /// Apply the safe, reversible ops. Proposals always stay manual.
        #[arg(long)]
        apply: bool,
        /// Age at which a memory is proposed for review (default: 90).
        #[arg(
            long = "stale-days",
            default_value_t = 90,
            allow_negative_numbers = true
        )]
        stale_days: i64,
        /// Pin the clock, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
    /// Delete a workspace and its clones. Guards work that removing it would discard.
    ///
    /// Exit 2 is the guard: a refusal that protected work is not the same failure as a name
    /// that is not a workspace, and a script can tell them apart.
    #[command(alias = "rm")]
    Remove {
        name: String,
        /// Remove it even though a clone or a worktree holds work nothing else does.
        #[arg(long)]
        force: bool,
    },
    /// Share a workspace's manifest + memory (LIVE), or make it private again (`--off`).
    Live {
        name: String,
        /// Make it LOCAL: untrack what is committed, then re-ignore it.
        #[arg(long)]
        off: bool,
    },
    /// Select a workspace for this terminal and session, and lock the session to it.
    Use {
        name: String,
        /// Create it first. Refused by this charter — see the command's own refusal.
        #[arg(long)]
        create: bool,
        /// Switch even though this session is locked to another workspace.
        #[arg(long)]
        force: bool,
    },
    /// Release this session's workspace lock so a different one can be selected.
    Unlock,
    /// Show, set or clear the workspace a session lands on when nothing else has decided.
    Default {
        name: Option<String>,
        /// Remove the nomination.
        #[arg(long)]
        clear: bool,
    },
    /// Capture this workspace's repos and branches into its committed manifest.
    Snapshot {
        /// The workspace (default: the active one).
        name: Option<String>,
        /// What this workspace is for, recorded in the manifest.
        #[arg(long)]
        description: Option<String>,
        /// Record the branches as they stand, even though some would not restore.
        #[arg(long)]
        force: bool,
        /// Pin the clock `updated_at` is stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
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
    /// The workspace to act on (default: the active one).
    #[arg(short = 'w', long = "workspace")]
    workspace: Option<String>,
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

/// Where this invocation is standing: the plane, the directory, and who is asking.
///
/// Built ONCE per command rather than per rung. Every piece of it is read from the process —
/// the cwd, the environment, the session and pane ids — and a second read is a second answer:
/// the cwd rung and the pointer rungs deciding from different snapshots is how a command
/// comes to act on one workspace and report another.
pub struct Here {
    pub plane: Plane,
    cwd: std::path::PathBuf,
    ids: charter_core::active::Ids,
    workspace_env: Option<String>,
    persona_env: Option<String>,
}

/// The plane this invocation acts on, and nothing else about it.
///
/// Kept beside [`Here`] and called BY it, for the two commands that want the plane and never
/// the ladder: `gl-refresh` is handed its workspace by `main`, and `statusline` runs on every
/// paint, where building the two ids costs a syscall for an answer it does not read.
fn plane() -> Result<Plane, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    charter_core::plane::resolve(&cwd)
        .map(Plane::open)
        .map_err(|e| e.to_string())
}

impl Here {
    fn read() -> Result<Self, String> {
        let cwd = std::env::current_dir()
            .map_err(|e| format!("cannot read the current directory: {e}"))?;
        Ok(Self {
            plane: plane()?,
            cwd,
            ids: charter_core::active::Ids::from_env(),
            workspace_env: std::env::var(charter_core::active::WORKSPACE_ENV).ok(),
            persona_env: std::env::var(charter_core::active::PERSONA_ENV).ok(),
        })
    }

    /// The whole ladder, with `flag` on top of it.
    fn asking<'a>(
        &'a self,
        flag: Option<&'a str>,
        env: Option<&'a str>,
    ) -> charter_core::active::Asking<'a> {
        charter_core::active::Asking {
            root: self.plane.root(),
            cwd: &self.cwd,
            flag,
            ids: &self.ids,
            env,
        }
    }

    /// The workspace this invocation acts on. There is always one: the ladder ends on
    /// `[workspace] default`, and under that on the literal `default`.
    pub fn active_workspace(&self, flag: Option<&str>) -> String {
        charter_core::active::workspace(&self.asking(flag, self.workspace_env.as_deref())).name
    }

    /// The persona this invocation acts as, or `None` — a plane may have no front door, and
    /// charter inventing one would be it choosing an identity nobody asked for.
    pub fn active_persona(&self, flag: Option<&str>) -> Option<String> {
        charter_core::active::persona(&self.asking(flag, self.persona_env.as_deref())).name
    }

    /// The workspace this command acts on, refusing a name that cannot be one.
    ///
    /// The refusal names the workspace RESOLVED, not the flag: with `-w` absent the operator
    /// never typed a name, and quoting an empty one would describe nothing.
    fn workspace(&self, flag: Option<&str>) -> Result<charter_core::workspaces::Workspace, String> {
        self.plane
            .workspace(&self.active_workspace(flag))
            .map_err(|e| e.to_string())
    }
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

/// `charter gl-refresh` — ask each clone's own forge about the branch it is on, and write the
/// answers into the cache the panels read.
///
/// A port of `charter/commands.py:cmd_gl_refresh`. The work itself is
/// [`charter_core::glrefresh`], which is where the credential boundary is argued.
fn gl_refresh(ws: &str, detach: bool, now: Option<&str>) -> ExitCode {
    use charter_core::glrefresh;

    // Checked FIRST: the point is to return before any of the work below, in a process the
    // harness will not tear down with the turn.
    if detach {
        return match detach_self(ws, now) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("charter: {why}");
                ExitCode::FAILURE
            }
        };
    }
    let root = match plane() {
        Ok(plane) => plane.root().to_path_buf(),
        Err(why) => {
            eprintln!("charter: {why}");
            return ExitCode::FAILURE;
        }
    };
    let stamp = match instant(now) {
        Ok(stamp) => stamp,
        Err(why) => {
            eprintln!("charter: {why}");
            return ExitCode::FAILURE;
        }
    };
    // A workspace this plane does not have is REFUSED here, where charter answers "No repos in
    // workspace '<name>'." and exits 0. That is a declared divergence: a `-w` nobody can act on
    // reading as "there is nothing to do" is how a typo silently refreshes nothing for ever,
    // and this binary already takes that position everywhere else (`vision` refuses a
    // workspace it would otherwise have invented).
    let found = match glrefresh::trees(&root, ws) {
        Ok(found) => found,
        Err(why) => {
            eprintln!("charter: {why}");
            return ExitCode::FAILURE;
        }
    };
    // Said, never dropped — `repos::clones`'s own rule. A refused directory is one this
    // refresh will not fetch for, and the row it feeds will stay empty until somebody is told
    // why.
    for (name, why) in &found.refused {
        voice::warn(&format!("{name} is not refreshed — {why}"));
    }
    let trees = found.trees;
    if trees.is_empty() {
        voice::info(&format!("No repos in workspace '{ws}'."));
        return ExitCode::SUCCESS;
    }
    let cache = glrefresh::refresh(&root, &trees, stamp);
    voice::ok(&format!(
        "Refreshed forge state for {} tree(s) in '{ws}'.",
        trees.len()
    ));
    for tree in &trees {
        let entry = cache.get(&glrefresh::key_for(tree));
        let field = |name: &str| entry.and_then(|row| row.get(name));
        let mut bits: Vec<String> = Vec::new();
        // `if ent.get("change")`: a change of zero or none is no change to report.
        if let Some(change) = field("change")
            .and_then(serde_json::Value::as_u64)
            .filter(|n| *n > 0)
        {
            // An entry written before the forge protocol carried a sigil has none, and the
            // display default is GitLab's — which is what `cmd_gl_refresh` prints.
            let sigil = field("sigil")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("!");
            bits.push(format!("{sigil}{change}"));
        }
        if let Some(ci) = field("ci")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            bits.push(format!("pipeline:{ci}"));
        }
        if !bits.is_empty() {
            let name = tree
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            voice::info(&format!("  {name}: {}", bits.join(" · ")));
        }
    }
    ExitCode::SUCCESS
}

/// Re-run this binary's `gl-refresh` in a process that outlives this one.
/// `charter/util.py:detach_self`.
///
/// **Two differences from Python, both deliberate.**
///
/// Python re-launches `python -m charter` and drops `--workspace`, leaving the child to
/// resolve the active workspace for itself. This binary has no resolution ladder, so the
/// workspace is carried — and carrying it is the better half of that argument anyway:
/// `glstate.maybe_spawn` already passes `--workspace` explicitly because "a refresh keyed to a
/// different workspace than the row it is refreshing is the defect".
///
/// Python calls `setsid`; this sets the child's own process GROUP. A hook's process group is
/// what a harness tears down when the turn ends, so the group is what has to be left — and
/// `Command::process_group` is safe, where `setsid` would need a `pre_exec` closure and this
/// workspace forbids `unsafe`. What it does not buy is detachment from the controlling
/// terminal, which a background refresh writing to `/dev/null` never touches.
fn detach_self(ws: &str, now: Option<&str>) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let me = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    let mut child = Command::new(me);
    child.arg("gl-refresh").arg("-w").arg(ws);
    if let Some(now) = now {
        child.arg("--now").arg(now);
    }
    match child
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
    {
        Ok(_) => Ok(()),
        Err(why) => Err(format!("could not start a detached refresh: {why}")),
    }
}

/// The instant a refresh stamps every entry with: `--now` as a local naive time, else the
/// wall clock. Seconds since the epoch, as Python's `time.time()` answers.
fn instant(now: Option<&str>) -> Result<f64, String> {
    let Some(text) = now else {
        return Ok(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs_f64())
            .unwrap_or(0.0));
    };
    let naive: chrono::NaiveDateTime = text
        .parse()
        .map_err(|e| format!("--now is not a local naive timestamp: {e}"))?;
    // A naive stamp is LOCAL time, as `--now` is everywhere in this binary.
    chrono::TimeZone::from_local_datetime(&chrono::Local, &naive)
        .single()
        .map(|local| local.timestamp() as f64)
        .ok_or_else(|| "--now names no single local instant".to_string())
}

/// `save` and `git-policy`, or `None` for any other command.
///
/// **These two resolve the plane the way Python's `config.ROOT` does**
/// ([`charter_core::plane::command_root`]): on the plane the vault, the personas and the
/// memory belong to, out of a linked worktree and outward through an enclosing plane's
/// `workspaces/`. `save`'s two refusals — you are standing in a worktree, you are standing in
/// a nested plane — only exist once that is the resolution, because they are about the caller
/// standing somewhere other than the tree being committed.
///
/// **The gap between this and what every other command does is now one step wide**, and it
/// was two: M2.9 gave `plane::find_root` the outward hop, so the read commands no longer stop
/// at the nearest marker and act on a clone's own plane. What is left to these two is the
/// worktree redirect.
fn plane_command(command: &Command) -> Option<ExitCode> {
    use charter_core::repocmd::Say;

    let mut say = |line: Say| eprintln!("{line}");
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("charter: cannot read the current directory: {e}");
            return Some(ExitCode::FAILURE);
        }
    };
    let root = match command {
        Command::Save { .. } | Command::GitPolicy { .. } => {
            match charter_core::plane::command_root(&cwd) {
                Ok(root) => root,
                Err(why) => {
                    eprintln!("charter: {why}");
                    return Some(ExitCode::FAILURE);
                }
            }
        }
        _ => return None,
    };
    let code = match command {
        Command::Save {
            message,
            sign,
            no_push,
        } => charter_core::planegit::save(
            &charter_core::planegit::Request {
                root: &root,
                message: message.as_deref(),
                sign: *sign,
                no_push: *no_push,
                cwd: &cwd,
            },
            &mut say,
        ),
        Command::GitPolicy { apply } => charter_core::gitpolicy::policy(&root, *apply, &mut say),
        _ => return None,
    };
    Some(ExitCode::from(code))
}

/// `discover`, `clone` and `sync`, or `None` for any other command.
fn repo_command(command: &Command) -> Option<ExitCode> {
    use charter_core::repocmd::{self, Say};

    let mut say = |line: Say| eprintln!("{line}");
    let here = match command {
        Command::Discover { .. } | Command::Clone { .. } | Command::Sync { .. } => {
            match Here::read() {
                Ok(here) => here,
                Err(why) => {
                    eprintln!("charter: {why}");
                    return Some(ExitCode::FAILURE);
                }
            }
        }
        _ => return None,
    };
    let root = here.plane.root().to_path_buf();
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
            // With no `-w`, the ladder: "default: the active one" is what charter's own
            // `--workspace` help has always promised for this command.
            let ws = here.active_workspace(workspace.as_deref());
            repocmd::clone::clone(
                &repocmd::clone::Request {
                    root: &root,
                    ws: &ws,
                    repos,
                    now,
                    author: &author,
                },
                &mut say,
            )
        }
        Command::Sync { workspace, all } => {
            // `--all` is the only thing that replaces the ladder here, and clap already
            // refuses it beside `-w`.
            let one = (!*all).then(|| here.active_workspace(workspace.as_deref()));
            let scope = match &one {
                Some(ws) => repocmd::sync::Scope::One(ws),
                None => repocmd::sync::Scope::All,
            };
            repocmd::sync::sync(&root, scope, &mut say)
        }
        _ => return None,
    };
    Some(ExitCode::from(code))
}

/// The workspace verbs that act on a workspace as a whole, or `None` for any other command.
///
/// Answered here rather than in [`run`] for the reason the repo commands are: each says
/// several lines as it goes and chooses its own exit status — `remove`'s guard is exit **2**,
/// which is neither a success nor the failure a bad name gets.
fn workspace_command(command: &Command) -> Option<ExitCode> {
    use charter_core::wscmd;

    let verb = match command {
        Command::Workspace(verb) => verb,
        _ => return None,
    };
    // Only the verbs below; everything else stays with `run`.
    if !matches!(
        verb,
        WorkspaceCommand::Remove { .. }
            | WorkspaceCommand::Live { .. }
            | WorkspaceCommand::Use { .. }
            | WorkspaceCommand::Unlock
            | WorkspaceCommand::Default { .. }
            | WorkspaceCommand::Snapshot { .. }
    ) {
        return None;
    }
    let here = match Here::read() {
        Ok(here) => here,
        Err(why) => {
            eprintln!("charter: {why}");
            return Some(ExitCode::FAILURE);
        }
    };
    let root = here.plane.root().to_path_buf();
    let mut sink = speak;
    let say: &mut dyn FnMut(charter_core::repocmd::Say) = &mut sink;
    let code = match verb {
        WorkspaceCommand::Remove { name, force } => {
            let code = wscmd::remove::remove(&root, name, *force, say);
            // The active workspace followed the removal: a pointer naming a workspace that is
            // gone resolves to it on every later command, and `workspaces/<gone>` is then
            // created again by the first write. Python resets it the same way and for the
            // same reason; the rung it tests for is spelled here as the two pointer rungs,
            // which are `session`/`active-file` in charter's own vocabulary.
            if code == 0 {
                reset_active_after_removal(&here, name, say);
            }
            code
        }
        WorkspaceCommand::Live { name, off } => wscmd::live::live(&root, name, *off, say),
        WorkspaceCommand::Use {
            name,
            create,
            force,
        } => wscmd::select::use_workspace(&root, name, &here.ids, *create, *force, say),
        WorkspaceCommand::Unlock => wscmd::select::unlock_command(&root, &here.ids, say),
        WorkspaceCommand::Default { name, clear } => {
            wscmd::select::default_command(&root, name.as_deref(), *clear, say)
        }
        WorkspaceCommand::Snapshot {
            name,
            description,
            force,
            now,
        } => {
            let now = match now {
                Some(text) => match text.parse::<chrono::NaiveDateTime>() {
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
            wscmd::snapshot::snapshot(
                &wscmd::snapshot::Request {
                    root: &root,
                    ws: &here.active_workspace(name.as_deref()),
                    description: description.as_deref(),
                    force: *force,
                    now,
                },
                say,
            )
        }
        _ => unreachable!("filtered above"),
    };
    Some(ExitCode::from(code))
}

/// Point this session back at the always-present workspace after the one it was on was
/// removed — `cmd_workspace_remove`'s closing branch.
///
/// Only when a POINTER is what named it. A `-w`, a `$CHARTER_WORKSPACE` or the tree the
/// caller is standing in are the operator's own and are not charter's to rewrite; a pointer
/// charter wrote is, and one naming a directory that no longer exists is how the next write
/// re-creates the workspace that was just deleted.
fn reset_active_after_removal(
    here: &Here,
    removed: &str,
    say: &mut dyn FnMut(charter_core::repocmd::Say),
) {
    use charter_core::active::WorkspaceRung;
    use charter_core::repocmd::Say;

    let active = charter_core::active::workspace(&here.asking(None, here.workspace_env.as_deref()));
    if active.name != removed
        || !matches!(
            active.rung,
            WorkspaceRung::SessionPointer | WorkspaceRung::TerminalPointer
        )
    {
        return;
    }
    let fallback = charter_core::active::plane_default_workspace(here.plane.root());
    // `force`, because the session is locked to the workspace that just went away and that
    // lock can refuse nothing useful now.
    charter_core::wscmd::select::set_active(here.plane.root(), &fallback, &here.ids, true);
    say(Say::Info(format!(
        "Active workspace reset to '{fallback}'."
    )));
}

fn run(command: Command) -> Result<u8, String> {
    let here = Here::read()?;
    match command {
        // Answered in `main`, before this: it is the one command whose exit code is not a
        // plain success or failure, and clap must never be allowed to exit 2 in front of it.
        Command::Hook { .. }
        | Command::Init(_)
        | Command::Reinit
        | Command::News(_)
        | Command::Update { .. }
        | Command::Discover { .. }
        | Command::Clone { .. }
        | Command::Sync { .. }
        | Command::Doctor { .. }
        | Command::GlRefresh { .. }
        | Command::Statusline { .. }
        | Command::Save { .. }
        | Command::Handoff { .. }
        | Command::Workspace(WorkspaceCommand::Remove { .. })
        | Command::Workspace(WorkspaceCommand::Live { .. })
        | Command::Workspace(WorkspaceCommand::Use { .. })
        | Command::Workspace(WorkspaceCommand::Unlock)
        | Command::Workspace(WorkspaceCommand::Default { .. })
        | Command::Workspace(WorkspaceCommand::Snapshot { .. })
        | Command::GitPolicy { .. } => {
            unreachable!("answered before run")
        }
        Command::Root => {
            println!("{}", here.plane.root().display());
        }
        Command::Workspace(WorkspaceCommand::Current) => {
            println!("{}", here.active_workspace(None));
        }
        Command::Harness(HarnessCommand::List) => {
            let root = here.plane.root().to_path_buf();
            // The git check runs HERE because a person typed this command; it never runs on
            // a config read, so no hook pays a git call per tool call.
            let check = profiles::ignore_check(&root);
            let set = profiles::with_ignore_check(profiles::current(&root), &check);
            eprint!("{}", harness_listing(&set, &check));
        }
        Command::Workspace(WorkspaceCommand::List) => {
            let mut names = here.plane.workspaces().map_err(|e| e.to_string())?;
            // The workspace `resolve` TERMINATES on is always listable, whether or not its
            // directory is there: a plane where nobody selected anything resolves to a name
            // this listing did not contain, and the table then marked no row at all
            // (charter#745). It is `config.DEFAULT_WORKSPACE` — `[workspace] default`, and
            // only the literal `default` when the plane declares none.
            let always = charter_core::active::plane_default_workspace(here.plane.root());
            if !names.contains(&always) {
                names.push(always);
                names.sort();
            }
            for name in names {
                println!("{name}");
            }
        }
        Command::Workspace(WorkspaceCommand::Vision { text, common }) => {
            let ws = here.workspace(common.workspace.as_deref())?;
            // A workspace charter does not have is not one this scaffolds: `vision` shows or
            // replaces, and inventing the directory is what made a bad `-w` silent.
            if !ws.dir().is_dir() {
                return Err(format!("no workspace '{}'", ws.name()));
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
        Command::Recall(args) => return memory::recall(&here, args),
        Command::Persona(command) => return memory::persona(&here, command),
        Command::Workspace(WorkspaceCommand::Remember {
            text,
            title,
            no_sync,
            common,
        }) => {
            return memory::workspace_remember(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref()),
                text.as_deref(),
                title.as_deref(),
                no_sync,
                common.now.as_deref(),
            );
        }
        Command::Workspace(WorkspaceCommand::Note {
            message,
            no_sync,
            common,
        }) => {
            return memory::workspace_remember(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref()),
                message.as_deref(),
                None,
                no_sync,
                common.now.as_deref(),
            );
        }
        Command::Workspace(WorkspaceCommand::Recall { query, common }) => {
            return memory::workspace_recall(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref()),
                query.as_deref(),
            );
        }
        Command::Workspace(WorkspaceCommand::Forget { slug, common }) => {
            return memory::workspace_forget(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref()),
                &slug,
            );
        }
        Command::Workspace(WorkspaceCommand::Optimize {
            name,
            // `--all` is the default and changes nothing, as in charter; taken so a script
            // that passes it is not refused.
            all: _all,
            apply,
            stale_days,
            now,
        }) => {
            return memory::workspace_optimize(
                &here.plane,
                name.as_deref(),
                apply,
                stale_days,
                now.as_deref(),
            );
        }
        Command::Workspace(WorkspaceCommand::Todo { words, common }) => {
            let ws = here.workspace(common.workspace.as_deref())?;
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
                    charter_core::memstore::forget(
                        here.plane.root(),
                        &ws.dir().join("todos"),
                        slug,
                    )
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
                        here.plane.root(),
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
    Ok(0)
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
    // `init`, `reinit` and `doctor` each say several lines of their own and choose their own
    // exit status — and for `doctor` the status IS the verdict, where a blocker is not an
    // error message.
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
        Command::Doctor {
            json,
            preflight,
            fix,
        } => return doctor(*json, *preflight, *fix),
        // A background refresh and a footer: neither is a plane write, and both choose their
        // own exit status as their Python counterparts do.
        Command::GlRefresh {
            workspace,
            detach,
            now,
        } => {
            let here = match Here::read() {
                Ok(here) => here,
                Err(why) => {
                    eprintln!("charter: {why}");
                    return ExitCode::FAILURE;
                }
            };
            return gl_refresh(
                &here.active_workspace(workspace.as_deref()),
                *detach,
                now.as_deref(),
            );
        }
        Command::Statusline { watch, now, .. } => {
            // Nothing writes a payload to a `--watch` render, and reading stdin there would
            // sit on the deadline for no reason.
            let payload = if *watch { String::new() } else { payload() };
            // Resolved BEFORE the render, and a bad value is refused rather than quietly
            // replaced by the wall clock: the flag exists so a differential can pin the ages
            // on the row, and a pin that silently did not take would make the comparison
            // green for the wrong reason.
            let when = match instant(now.as_deref()) {
                Ok(secs) => chrono::DateTime::from_timestamp(secs as i64, 0)
                    .unwrap_or_else(chrono::Utc::now),
                Err(why) => {
                    eprintln!("charter: {why}");
                    return ExitCode::FAILURE;
                }
            };
            // No plane means nothing is recorded: see `statusline::run`, which declares that
            // divergence from Python and why it is the right way round.
            let here = plane().ok();
            statusline::run(
                here.as_ref().map(|plane| plane.root()),
                &payload,
                &statusline::Ambient::here(),
                when,
            );
            return ExitCode::SUCCESS;
        }
        // `news` and `update` say several lines of their own on both streams and choose their
        // own exit status, exactly as `init` does.
        Command::News(news) => {
            let probes = Probes {
                tree: command_tree(),
            };
            return emit(&news_report(news, &probes));
        }
        Command::Update { to, bump } => {
            let probes = Probes {
                tree: command_tree(),
            };
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let place = charter_core::plane::place(&cwd);
            let args = charter_core::adopt::UpdateArgs {
                to: to.clone().unwrap_or_default(),
                bump: *bump,
            };
            let root = place.is_plane.then_some(place.root.as_path());
            return emit(&charter_core::adopt::update_report(root, &args, &probes));
        }
        _ => {}
    }
    // The repo commands speak line by line as they go — a clone is slow, and the line that
    // says which repo is being fetched is worth nothing once it has been — and choose their
    // own exit status, as their Python counterparts do.
    if let Some(code) = repo_command(&cli.command) {
        return code;
    }
    // `save` and `git-policy` likewise: several lines each, and an exit status of their own.
    if let Some(code) = plane_command(&cli.command) {
        return code;
    }
    // The workspace verbs that act on a workspace as a whole — `remove`'s guard exits 2.
    if let Some(code) = workspace_command(&cli.command) {
        return code;
    }
    // `handoff` says one refusal and exits 1; it never returns 0 in this charter.
    if let Command::Handoff {
        workspace,
        create,
        vision,
        persona,
    } = &cli.command
    {
        let here = match Here::read() {
            Ok(here) => here,
            Err(why) => {
                eprintln!("charter: {why}");
                return ExitCode::FAILURE;
            }
        };
        return handoff::handoff(
            &here,
            &handoff::Args {
                workspace: workspace.clone(),
                create: *create,
                vision: vision.clone(),
                persona: persona.clone(),
            },
        );
    }
    match run(cli.command) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("charter: {message}");
            ExitCode::FAILURE
        }
    }
}

/// `charter doctor`: every check, as a table or as `--json`, and the verdict as the exit.
///
/// **`--fix` is refused, not ignored.** Python's installs the Claude Code plugin before it
/// reports, so the report reads as the state after the repair; this charter installs nothing
/// yet, and a report printed under that flag would be read as one.
fn doctor(json: bool, preflight: bool, fix: bool) -> ExitCode {
    use std::io::IsTerminal;

    if fix {
        eprintln!(
            "charter: `doctor --fix` installs the Claude Code plugin for this plane, which this \
             charter does not do yet — nothing was installed and nothing was checked. The \
             Python charter's `charter doctor --fix` does it; `charter doctor` reports without \
             it."
        );
        return ExitCode::FAILURE;
    }
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("charter: cannot read the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    let rows = charter_core::doctor::Doctor::new(&cwd, preflight).run();
    if json {
        print!("{}", charter_core::doctor::json(&rows));
    } else {
        print!(
            "{}",
            charter_core::doctor::table(&rows, std::io::stdout().is_terminal())
        );
    }
    ExitCode::from(charter_core::doctor::exit_code(&rows))
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

#[cfg(test)]
mod probe_tests {
    use super::*;
    use clap::CommandFactory;

    /// The clap `Command` at a subcommand path, or `None` when this binary has no such command.
    fn command_at(path: &[&str]) -> Option<clap::Command> {
        let mut at = Cli::command();
        for name in path {
            at = at.find_subcommand(name)?.clone();
        }
        Some(at)
    }

    #[test]
    fn no_command_a_probe_may_name_takes_a_pass_through_argv() {
        // **charter #317, and the half of it a parser can answer.** `secret exec` takes the rest
        // of the line as a pass-through argv, so a `check:` naming it reached any binary on the
        // machine with a vault's credential in the child's environment — on every plane that
        // upgraded, from a SessionStart hook. `news::PROBEABLE` is a list a human keeps because
        // the other half ("does it write to the disk?") cannot be read off a parser; this half
        // can be, and asking it of the list rather than at runtime makes it a proof.
        //
        // In `main.rs` rather than under `tests/`, because a binary's parser is not importable
        // from an integration test and a copy of the enum over there would be a test of the copy.
        let mut found = Vec::new();
        for path in charter_core::news::PROBEABLE {
            // Only the paths this binary registers. The two it does not have are refused before
            // a probe reaches them, and a list that shrank to match this CLI would stop being
            // the rule an entry's AUTHOR is held to.
            let Some(cmd) = command_at(path) else {
                continue;
            };
            found.push(path.join(" "));
            for arg in cmd.get_positionals() {
                assert!(
                    !arg.get_num_args().is_some_and(|n| n.max_values() > 1),
                    "`charter {}` takes `{}` as an open-ended argv, so a `check:` naming it \
                     would hand an entry's own words to whatever it runs — take it off \
                     news::PROBEABLE, or take the positional off the command",
                    path.join(" "),
                    arg.get_id()
                );
            }
        }
        assert_eq!(
            found,
            vec!["doctor".to_owned(), "news".to_owned()],
            "the probeable commands this binary has changed; the notes in `news`'s module \
             docstring that say which two they are have to change with it"
        );
    }

    #[test]
    fn the_tree_a_probe_is_checked_against_is_this_binarys_own() {
        // Read off clap rather than written down. A command added to the CLI is in it the same
        // day; a name that is only an ALIAS is in it too, because argparse's `choices` holds one
        // key per alias, so `ws` IS a subcommand name in charter and a tree without it would
        // read `check: ws …` as a command this charter does not have.
        let tree = command_tree();
        let names: Vec<&str> = tree.children.iter().map(|c| c.name.as_str()).collect();
        for expected in ["news", "update", "doctor", "workspace", "ws"] {
            assert!(
                names.contains(&expected),
                "{expected} is missing from {names:?}"
            );
        }
        let alias = tree
            .children
            .iter()
            .find(|c| c.name == "ws")
            .expect("the alias is a child");
        assert!(
            alias.children.iter().any(|c| c.name == "todo"),
            "an alias carries the same subcommands as the name it stands for"
        );
    }
}
