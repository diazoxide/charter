//! The `charter` command line, in Rust.
//!
//! Only the plane commands M1.1 covers are here, and only as far as the PLANE goes. The
//! recorded scenarios (`tests/fixtures/recorded/`, ADR 0046) prove that: each runs this binary
//! against a copy of a fixture plane and compares the tree it leaves with what the Python
//! charter left.
//!
//! One thing this binary deliberately does NOT do yet, recorded in the harness rather than
//! left to be discovered:
//!
//! - **Not every command prints its confirmation.** charter says `✓ Vision set for 'alpha' →
//!   …` on stderr, and `vision` here is still silent (`todo` speaks since M8.5). The memory commands
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

use charter_core::extension::events::Event as ExtensionEvent;
use charter_core::hookwire::{self, Report, SOCKET_ENV};
use charter_core::profiles::{self, ProfileSet, Source};
use charter_core::shown;
use charter_core::state::Event;
use charter_core::workspaces::Plane;
use clap::{Args, Parser, Subcommand};

mod extcmd;
mod extensions;
mod guard;
mod handoff;
mod hooks;
mod memory;
mod piece;
mod report;
mod secret;
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

    /// Pieces: worktrees of a workspace's clones — cut, declared done or abandoned, removed.
    #[command(subcommand, alias = "wt")]
    Worktree(piece::WorktreeCommand),

    /// Harness profiles: which program a chat runs, and with what environment.
    #[command(subcommand)]
    Harness(HarnessCommand),

    /// charter's plugin for a `claude` or `codex` started outside the app: its hooks, the Bash
    /// guard and its skills. The app arms its own chats; this is for the others.
    #[command(subcommand)]
    Plugin(PluginCommand),

    /// Force-prompt and stop-prompting rules for this plane, written in each harness's own
    /// syntax into the file each one reads — Claude Code, opencode and Codex, not only the one
    /// you are running. charter keeps no list of its own (ADR 0014). Bare, it lists them.
    Guard {
        #[command(subcommand)]
        verb: Option<GuardCommand>,
    },

    /// The browser lane: charter's plugin ships the credential bridge (the `browser` skill),
    /// Playwright ships the page-driving surface.
    #[command(subcommand)]
    Browser(BrowserCommand),

    /// Add what the plane's forges list to inventory/repos.json, then regenerate docs.
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

    /// Show the plane's workspaces and the cloned repos in the active one.
    ///
    /// The command an operator types when something has already gone wrong, so its output is
    /// a contract: which plane answered, which rung chose the workspace, and for every clone
    /// a branch and one of `clean`, `dirty` or `unknown` — never `clean` for a tree charter
    /// could not read.
    Status {
        /// The workspace to detail (default: the active one).
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
        /// Detail every workspace.
        #[arg(long)]
        all: bool,
    },

    /// Regenerate this plane's `docs/topology.md` — and the README's persona roster block.
    ///
    /// Bare `charter docs` generates, as it did long before it grew subcommands: Makefiles in
    /// the wild call it that way, and making the group require a subcommand would refuse a
    /// command line planes already have.
    ///
    /// `docs list` and `docs show` read charter's OWN documentation and are not this binary's
    /// — one command describes charter, the other describes your repos.
    Docs {
        #[command(subcommand)]
        what: Option<DocsCommand>,
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
        /// First bring in what the remote has, as the app does: fetch the target branch and
        /// fast-forward a clean tree. Refuses, and saves nothing, when the tree has conflicts.
        #[arg(long)]
        pull: bool,
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

    /// Read/write secrets in a vault; values stay out of the model.
    #[command(subcommand)]
    Secret(secret::SecretCommand),

    /// Manage secret vaults (provider + config + persona).
    #[command(subcommand)]
    Vault(secret::VaultCommand),

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
        /// Install charter's plugin for chats started outside the app first — what
        /// `charter plugin install` does, each change printed on stderr — then report. It
        /// writes this machine's harness settings only, never a file in the plane.
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

    /// What each version of the app brought: its CHANGELOG.md, newest first.
    News(NewsCommand),

    /// Which channel the app updates from. This command does NOT install anything.
    ///
    /// charter's own `update` moves a Python package with `uv tool install`; this charter is a
    /// binary inside the app, and the app is what moves it. So `update` here says so, names the
    /// channel (`--channel` moves it), and points at `charter news` for what a version brought.
    /// `--to` and `--bump` are taken and refused by name rather than rejected as unknown flags,
    /// because an agent that typed one is owed the reason.
    Update {
        /// Install exactly this version. Refused: nothing here installs.
        #[arg(long)]
        to: Option<String>,
        /// Also move this plane's pin. Refused: the pin names a published charter-cp release.
        #[arg(long)]
        bump: bool,
        /// Put the app on this machine on a release channel: `stable` (the default) or `dev`.
        #[arg(long, value_name = "stable|dev")]
        channel: Option<String>,
    },

    /// Which charter this is, what this control plane pins, and whether they agree.
    ///
    /// Not charter's three rows, and ADR 0030 is why: two of them — the installed wheel and
    /// the newest one on PyPI — have no subject for a binary that ships inside the app. What
    /// this prints instead is the app's own version and the pin itself (ADR 0045). The EXIT
    /// STATUS is charter's: 0 with no pin, 0 when the pin is met, 1 on drift.
    Version {
        #[command(subcommand)]
        what: Option<VersionCommand>,
    },

    /// File a bug or a feature request on charter's own tracker, under your own `gh` login.
    /// Shows the draft first and sends nothing without your yes.
    #[command(subcommand)]
    Report(report::ReportCommand),

    /// Open a chat in a workspace you name, already working on a brief you pass as a quoted
    /// heredoc on stdin. Your harness asks before it runs.
    ///
    /// **The app opens the chat.** Run from a chat the charter app started, the new chat
    /// opens there as a tab in the workspace you name. With no app running, nothing is
    /// opened and charter says to open the app. Every refusal in front of that is the
    /// point: charter refuses every shape the permission prompt in front of this command
    /// cannot stand in front of (`charter_core::handoff`).
    ///
    /// `charter handoff report "<summary>"`, from a chat a `--report` handoff opened, sends
    /// its one report back to the chat that opened it.
    Handoff {
        /// Where the chat opens — an existing workspace, or a new one with --create. Always
        /// named, this workspace included. `report`, followed by a summary, is a report back.
        workspace: String,
        /// With `report` as the first word: the report, a few lines on what was done.
        summary: Option<String>,
        /// A short name for the task, which the new chat is called instead of its default
        /// (`drop account-console-commons`). At most 64 characters.
        #[arg(long)]
        name: Option<String>,
        /// Ask the new chat to report back when it is done, with `charter handoff report`.
        /// The report reaches this chat as context on its next turn.
        #[arg(long)]
        report: bool,
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
        /// Pin the clock the stamp, the todo and the dispatch row are written at, for tests
        /// only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },

    /// Answer a harness hook. Run by a harness's hooks, never by a person.
    ///
    /// It reads the harness's payload on stdin and answers the way that hook is answered.
    ///
    /// `sessionstart` briefs the session — the persona it was started as, that persona's
    /// memory, the workspace gate, the workspace's todos, the plane's other workspaces — as
    /// `additionalContext`, and freezes the persona tool gate's ceiling.
    ///
    /// `userpromptsubmit` keeps the session's heartbeat and adds, as `additionalContext`, the
    /// commitment gate — a prompt asking for work with a real fork in it is told to scout and
    /// ask before building; never on a lookup, never unattended, then quiet for three prompts —
    /// and any report a chat this one handed work to has sent back.
    ///
    /// `pretooluse` is the Bash guard and the persona tool gate, `pretooluse-read` the vault
    /// guard on Read/Grep, `pretooluse-edit` the state-directory guard on Write/Edit, and
    /// `pretooluse-dispatch` the ask before a code-writing persona is sent out beside a running
    /// agent.
    ///
    /// `posttooluse`, `-skill`, `-dispatch` and `-message` keep the memory nudges, the secret
    /// warning on a written memory, and the dispatch and skill logs. Every event word also tells
    /// the app, over its socket, what the chat is doing.
    ///
    /// Exit 2 — "block" — only when a denial it decided could not be printed. `--list` prints
    /// every word it answers, with the event and tool matcher each is wired to; `--json` makes
    /// that the registry a plugin's `hooks.json` is generated from.
    Hook {
        /// The hook word: `sessionstart`, `pretooluse`, `pretooluse-read`, … (`--list`).
        #[arg(required_unless_present = "list")]
        name: Option<String>,

        /// Print every hook this binary answers, and exit.
        #[arg(long)]
        list: bool,

        /// With `--list`: the registry as JSON — `{"schema": 1, "handlers": [...]}`.
        #[arg(long, requires = "list")]
        json: bool,

        /// The retired Python charter's plugin version. Taken and ignored, and removed with that
        /// plugin.
        ///
        /// The retired plugin puts it on every one of its hook commands (`charter hook
        /// sessionstart --plugin-version 0.62.1`); refusing the flag would mean refusing every
        /// call such a plugin still makes. charter's own plugin never passes it.
        #[arg(long)]
        plugin_version: Option<String>,

        /// Pin the clock, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
}

#[derive(Args, Clone)]
struct NewsCommand {
    /// One version's section (`0.3.0`, or `Unreleased`), as its release notes print it.
    #[arg(long = "for", value_name = "VERSION")]
    for_version: Option<String>,
    /// Retired with the range view (#352). Taken and refused by name, with what to run instead.
    #[arg(long, hide = true)]
    pending: bool,
    /// Retired with the range view (#352).
    #[arg(long, hide = true)]
    since: Option<String>,
    /// Retired with the range view (#352).
    #[arg(long, hide = true)]
    until: Option<String>,
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
    #[arg(long, conflicts_with = "adopt")]
    clone_this_repo: bool,
    /// Adopt an existing repository as this plane's first clone: the plane is made HERE and
    /// that repo is cloned into the first workspace, with nothing written into the repo
    /// itself. ADR 0035's default, where the directory this runs in is the "beside it".
    #[arg(long, value_name = "REPO")]
    adopt: Option<std::path::PathBuf>,
    /// The instant the first workspace's manifest records. Testing only; the wall clock
    /// otherwise.
    #[arg(long, hide = true)]
    now: Option<String>,
    /// Make the git repo you are standing in BE the control plane: write charter.toml,
    /// personas/, inventory/, workspaces/ and charter's rules into that repo's own tracked
    /// .gitignore. Without it, `init` at the top of a repo writes nothing and says how to put
    /// the plane in a directory of its own (ADR 0035).
    #[arg(long)]
    plane_is_this_repo: bool,
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

/// Where `init` and `reinit` act: `charter/root.py:find_root_or_cwd`.
fn place() -> Result<charter_core::plane::Place, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    Ok(charter_core::plane::place(&cwd))
}

#[derive(Subcommand)]
enum BrowserCommand {
    /// Generate Playwright's driving-surface skill into this plane's .claude/skills/, from the
    /// tool that owns it (charter vendors none of it — Apache-2.0, and it ships far more
    /// often than charter does).
    Install {
        /// @playwright/cli version, exactly (default: the one charter is known to work with).
        #[arg(long)]
        version: Option<String>,
    },
}

#[derive(Subcommand)]
enum GuardCommand {
    /// Always prompt before this command runs.
    Ask {
        /// e.g. 'terraform apply *' — wrapped as Bash(...) unless it already names a tool.
        pattern: String,
        /// Write this machine's own file (`.claude/settings.local.json`, not committed)
        /// instead of the plane's committed settings: the rule is yours alone.
        #[arg(long)]
        local: bool,
    },
    /// Stop the harness prompting for a command pattern (writes the harness's own allow rule).
    /// It reaches a chat at the plane root only.
    Allow {
        /// e.g. 'git status *'. A bare command is wrapped as a Bash rule.
        pattern: String,
        /// Write this machine's own file instead of the plane's committed settings.
        #[arg(long)]
        local: bool,
    },
    /// Always prompt before a handoff runs: the rule `charter init` writes, put back.
    Handoff,
    /// Show this plane's ask and allow rules, by the file each lives in.
    List,
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Register charter's plugin with each harness on this machine, so a chat started in a
    /// terminal runs charter's hooks and guard. Prints each change; running it again changes
    /// nothing that is already so.
    Install {
        /// Only this harness (`claude`, `codex` or `opencode`); repeat for more. Default: each
        /// one whose config folder exists.
        #[arg(long, value_parser = charter_core::plugin_install::HARNESSES)]
        harness: Vec<String>,
        /// Print what would change, and write nothing.
        #[arg(long)]
        dry_run: bool,
        /// The plugin folder to install from, instead of the one the app ships beside this
        /// binary.
        #[arg(long, hide = true)]
        plugin_from: Option<std::path::PathBuf>,
    },
    /// Take back what `install` wrote, and nothing else.
    Uninstall {
        /// Only this harness (`claude`, `codex` or `opencode`); repeat for more.
        #[arg(long, value_parser = charter_core::plugin_install::HARNESSES)]
        harness: Vec<String>,
        /// Print what would change, and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum HarnessCommand {
    /// Every profile charter read, the file it came from, and why any was refused.
    List,
}

/// The two `version` verbs that move a PUBLISHED `charter-cp` release.
///
/// Registered rather than left to clap so that each gets a sentence instead of a usage error
/// — M2.21's rule, applied to the verbs beside it. Their flags are declared as charter
/// declares them, because a script that passes `--cli` or `--push` must meet the refusal
/// rather than the parser.
#[derive(Subcommand)]
enum VersionCommand {
    /// Move THIS plane to the version it pins.
    Sync {
        /// Conform the machine-global `charter` binary instead.
        #[arg(long)]
        cli: bool,
    },
    /// Move the pin: install + verify the target, then write `charter.toml`.
    Bump {
        /// Version to pin (default: the latest published).
        #[arg(long)]
        to: Option<String>,
        /// Also commit + push the lock.
        #[arg(long)]
        push: bool,
    },
}

#[derive(Subcommand)]
enum DocsCommand {
    /// Regenerate `docs/topology.md` from the inventory, and the README's roster block.
    Generate,
    /// List charter's own documentation topics.
    List,
    /// Print one of charter's own documentation pages — served by the install that
    /// implements it, so it cannot be a version behind the CLI reading it.
    Show {
        /// e.g. secrets, personas, git-policy (see `docs list`).
        topic: String,
    },
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
    /// Rename a workspace: its directory, its clones' worktrees, and every record that names it.
    ///
    /// Refused while a chat is running in it, and when the new name is taken or is not one a
    /// workspace can have. A rename that was interrupted is finished by running it again.
    #[command(alias = "mv")]
    Rename {
        /// The workspace's name now.
        old: String,
        /// The name it is to have.
        new: String,
    },
    /// Share a workspace's manifest + memory (LIVE), or make it private again (`--off`).
    Live {
        name: String,
        /// Make it LOCAL: untrack what is committed, then re-ignore it.
        #[arg(long)]
        off: bool,
    },
    /// Create a workspace: its directory, its baseline files and charter's harness layer.
    Create {
        name: String,
        /// Repos to clone into it immediately, by inventory name. POSITIONAL, as charter's
        /// own `workspace create <name> [repos...]` takes them — a `--repos` of this port's
        /// own would be a command line that works against one charter and not the other.
        repos: Vec<String>,
        /// What this workspace is for, recorded in its `workspace.md` charter.
        #[arg(long, alias = "about")]
        vision: Option<String>,
        /// Share its charter, manifest and memory from birth.
        #[arg(long)]
        live: bool,
        /// Select it for this session once it exists.
        #[arg(long = "use")]
        use_it: bool,
        /// Select it even though this session is locked to another workspace.
        #[arg(long)]
        force: bool,
        /// Pin the clock the manifest's `updated_at` is stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
    /// Rebuild a workspace from its manifest — clone repos + checkout branches.
    Restore {
        /// The workspace to rebuild.
        name: String,
        /// Don't clone now; clone each repo when you enter it.
        #[arg(long = "on-demand")]
        on_demand: bool,
        /// Pin the clock a membership record is stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
    /// Fork a workspace: a new one pre-loaded with its charter, memory, todos and manifest.
    ///
    /// The clones are NOT copied — they are reconstructible. `--restore` clones them straight
    /// away, which since M2.26 is the whole restore: `charter clone` per missing repo, then
    /// the recorded branch checked out and pulled.
    #[command(alias = "duplicate")]
    Fork {
        /// The workspace to fork from.
        src: String,
        /// The fork's name.
        new: String,
        /// Also clone the inherited repos now.
        #[arg(long)]
        restore: bool,
        /// Make the fork LIVE (default LOCAL).
        #[arg(long)]
        live: bool,
        /// Pin the clock the fork's manifest and note are stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
    /// Bring a workspace's structure and charter's layer up to what this version writes.
    Reinit {
        /// The workspace (default: the active one).
        name: Option<String>,
        /// Every workspace this plane has. Given with a name, this wins and the name is
        /// ignored — charter's own parser refuses neither, and a port that refused one would
        /// be a command line that works against one charter and not the other.
        #[arg(long)]
        all: bool,
        /// Pin the clock a backfilled manifest is stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
    /// Select a workspace for this terminal and session, and lock the session to it.
    Use {
        name: String,
        /// Create it first, scaffolded exactly as `workspace create` scaffolds.
        #[arg(long)]
        create: bool,
        /// Switch even though this session is locked to another workspace.
        #[arg(long)]
        force: bool,
        /// Pin the clock a scaffolded manifest is stamped with, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
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
    /// Internal, answered and ignored: the Python plugin's SessionStart seed of a session's
    /// workspace pointer from its terminal pane. A chat charter-app starts has no pane to seed
    /// from — its workspace is the pointer keyed on the chat, which `charter ws use` writes.
    #[command(name = "_reconcile", hide = true)]
    Reconcile,
    /// Internal, answered and ignored: the Python plugin's turn-end commit of a LIVE
    /// workspace's memory. It commits nothing under the default `share = "local"`; under
    /// `commit`/`push` charter-app leaves the commit to `charter save`, which commits the plane
    /// as one decision instead of racing the operator's own git on every turn end.
    #[command(name = "_autosave", hide = true)]
    Autosave,
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

/// The Bash guard's word — answered by [`guard::pretooluse`], ahead of every other tool hook
/// because its refusals are the ones M3.1 was built for.
const GUARDED_TOOL_HOOK: &str = "pretooluse";

/// Whether a word this binary does NOT answer is a TOOL hook, where refusing means blocking.
///
/// **The namespace, not a list of words, and not a blanket rule either.** Both of those were
/// tried and both were wrong, each in the other's direction:
///
/// - A list of the words in `charter/hooks.py:_HANDLERS` fails OPEN on everything not in it.
///   `charter hook pretooluse-notebook` exited 1, which a harness logs and ignores, so the day
///   charter adds a matcher the Rust binary on PATH would allow that tool class silently.
/// - Blocking every unknown word instead fails the other way: `charter hook stopp`, a typo in
///   a settings file charter itself wrote, blocked the session from ENDING — which is the
///   exact hazard this whole file was written around.
///
/// The word says which hook it is. In the `pretooluse`/`posttooluse` namespace there is a tool
/// call to protect and blocking is the safe answer, for every matcher charter has and every
/// one it adds. Outside it there is nothing to protect and blocking can only wedge a session.
///
/// **Every word charter has today is answered before this is asked** — the registry
/// ([`charter_core::hookreg::HANDLERS`]) and its no-ops. What is left here is only the word
/// nobody has invented yet, and the rule for it is unchanged because its argument is: a
/// program that has checked nothing may not say `allow`.
fn is_a_tool_hook(name: &str) -> bool {
    is_a_pretooluse_hook(name) || name.starts_with("posttooluse")
}

/// Whether the word is a `PreToolUse` hook: one that decides whether a tool call runs, so a
/// crash in it must refuse the call ([`guard::refuse_on_a_crash`]). The namespace, as
/// [`is_a_tool_hook`] reads it, and for its reason — a matcher charter adds tomorrow is covered
/// today. `posttooluse` is not: the tool has already run, and there is nothing left to refuse.
fn is_a_pretooluse_hook(name: &str) -> bool {
    name.starts_with("pretooluse")
}

/// Whether this command line is `charter hook <a PreToolUse word>`, read off the raw argv
/// because it is asked before clap has parsed anything — a crash in the parse is a crash too.
/// Any argument after `hook` counts, so a flag written before the word does not hide it.
fn is_a_pretooluse_call(argv: &[std::ffi::OsString]) -> bool {
    argv.get(1).is_some_and(|word| word == "hook")
        && argv
            .iter()
            .skip(2)
            .any(|arg| arg.to_str().is_some_and(is_a_pretooluse_hook))
}

/// What a harness reads as "block".
const BLOCK: u8 = 2;

/// Set in a debug build, `charter` panics before it has read its command line (#349): the
/// test suite's way to watch what a crash answers. Compiled out of a release build.
#[cfg(debug_assertions)]
const PANIC_ON_PURPOSE_ENV: &str = "CHARTER_TEST_HOOK_PANICS";

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
/// **Never exit 2, except where the whole point is to.** A harness reads 2 as "block": on
/// `Stop` it makes the harness carry on rather than end. Nothing charter draws is worth that,
/// so every failure on a REPORTING hook is a silent 0 and the state the app draws is simply the
/// last one it was told. The three exceptions are all about a tool call: a denial a guard
/// decided and could not print ([`guard::deny`]), a word in the tool-hook namespace this
/// binary does not answer at all ([`is_a_tool_hook`]), and a `PreToolUse` hook that crashed
/// ([`guard::refuse_on_a_crash`]).
fn hook(name: &str, now: Option<&str>) -> ExitCode {
    // FIRST, in front of `Event::parse`, because none of these is one of the app's reporting
    // events: a tool call carries no chat state worth a `Report`, and a guard that also spoke
    // on the app's socket would be two jobs on one exit status.
    if name == GUARDED_TOOL_HOOK {
        return guard::pretooluse(&payload(), now);
    }
    if let Some(answered) = hooks::tool(name, now) {
        return answered;
    }
    if charter_core::hookreg::NO_OPS.contains(&name) {
        return ExitCode::SUCCESS;
    }
    let Some(event) = Event::parse(name) else {
        let tool = is_a_tool_hook(name);
        // The word comes out of a settings file a chat can write, and this sentence goes to
        // a terminal and into the harness's own log. Contained like every other value
        // charter quotes back.
        let name = charter_core::shown::readable(name, charter_core::shown::DISPLAY_LIMIT);
        eprintln!(
            "charter: `{name}` is not a hook this binary answers (`charter hook --list` names \
             them){}.",
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
    let socket = std::env::var_os(SOCKET_ENV);
    // The payload is read once, and only when something reads it: `sessionstart` always does,
    // `userpromptsubmit` for the heartbeat, and every event when the app is listening.
    let wanted = socket.is_some() || matches!(event, Event::SessionStart | Event::UserPromptSubmit);
    let text = if wanted { payload() } else { String::new() };
    // The session's own work comes BEFORE the report: a briefing the harness never receives
    // because the app's socket was slow would be the worse of the two to lose.
    match event {
        Event::SessionStart => hooks::sessionstart(&text, now),
        Event::UserPromptSubmit => hooks::userpromptsubmit(&text, now),
        _ => {}
    }
    let Some(socket) = socket else {
        // No app started this session — the operator's own harness in a terminal, with the
        // hooks pointed here. There is nothing to tell, and this is the ONE path on which
        // anything decides to refresh the forge cache (charter-app#89).
        if event == Event::SessionStart {
            refresh_the_forge_cache();
        }
        return ExitCode::SUCCESS;
    };
    if let Some(report) = Report::read(event, &text, &|name| std::env::var(name).ok())
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

/// The forge cache's trigger on a plane with **no app open** — charter-app#89, which is the
/// half of #69 that was deferred.
///
/// # What Python actually does, since the issue says otherwise
///
/// #89 says this is "the `charter hook` trigger Python has". It is not: `charter/hooks.py`
/// never calls `glstate.maybe_spawn`, and `sessionstart()` does not go near it. Python's one
/// trigger is `charter/statusline.py:_render`, which calls `glstate.read_for` and then
/// `glstate.maybe_spawn` on the footer's own render path — every turn that draws the footer
/// kicks a refresh. So this is not a port of a call site. It is the POLICY ported (that is
/// `glstate.rs`, #69) put on a path Python does not use for it, and the reason is that the
/// path Python does use is not available here:
///
/// - **Inside the app** `charter statusline` draws nothing and returns early (ADR 0019), and
///   the app has its own trigger already — `panels::repo_states`, focusing a workspace.
/// - **Outside it**, `statusLine` is armed only for chats the app starts and only where the
///   operator fills that line with nothing (`harness::claude_code_status_line`,
///   `footerclaim`). A plane with no app open has no `charter statusline` running at all —
///   charter #895 deleted the one that used to be wired — while the plugin's hooks DO run.
///
/// Which leaves this: the hook is the only thing charter runs on a plane nobody has an app
/// open on, so it is the only place the trigger can go.
///
/// # Why only `SessionStart`, and only with no app behind it
///
/// **Once a session, not once a turn.** The brakes make a repeat trigger cheap — past the
/// cooldown is two file reads — but "cheap" multiplied by every prompt and every turn end is
/// the shape #69's brakes exist to prevent, on a path with a budget. `SessionStart` is the one
/// event that fires once, and with `REFRESH_TTL` at 300 s a per-turn trigger buys no freshness
/// a per-session one does not.
///
/// **And only where the app is not.** `$CHARTER_HOOK_SOCKET` in the environment means this
/// chat was started by the app, which already decides when a refresh runs. Two deciders on one
/// plane is not unsafe — the lock is what makes that safe — but it is two policies, and the
/// issue is about the plane that has none.
///
/// # What it costs
///
/// Everything here is filesystem-only: resolving the plane, reading the workspace ladder,
/// listing the workspace's clones and their worktree directories (`glrefresh::trees` is
/// `read_dir` and path arithmetic — no git spawn), then `glstate::decide`'s two file reads.
/// The one expensive act, the fork, happens at most once per `SPAWN_COOLDOWN` and is a spawn
/// and never a wait. Measured on this machine over 300 runs each, `charter hook sessionstart`
/// against a plane whose cache is fresh: **3.68 ms before, 3.66 ms after**, against a
/// `charter --version` floor of 3.63 ms — the added work does not clear the noise of starting
/// the process at all.
///
/// # Best-effort, and silent about it
///
/// Nothing here is worth a word on a reporting hook: no plane, no readable workspace, no
/// `current_exe`, a fork that failed — each simply means no refresh, and the column already
/// says "nothing has fetched this checkout" rather than reading as green (#69). Python's own
/// is a bare `except Exception: return` for the same reason.
fn refresh_the_forge_cache() {
    use charter_core::{glrefresh, glstate};

    // **This very executable**, never a `charter` found on `$PATH`. That is
    // `charter/util.py:self_relaunch_argv`'s `-P` (charter #390) carried over: the child must
    // be the same charter as the parent, and a `PATH` lookup from inside a plane can find an
    // older install, the Python charter, or a `charter` in the checkout the chat is standing
    // in. `glstate::spawn`'s own doc gives the app's half of the same rule.
    let Ok(binary) = std::env::current_exe() else {
        return;
    };
    // Outside a plane there is nothing to refresh and nowhere to cache it —
    // `charter/glstate.py:maybe_spawn`'s `if not config.HAS_CONTROL_PLANE: return`, which is
    // the brake `glstate::decide` leaves to its callers because every other one is handed a
    // root that was already found (charter #527).
    let Ok(here) = Here::read() else {
        return;
    };
    let workspace = here.active_workspace(None);
    let root = here.plane.root();
    // The same list the panel draws and the same list the child will fetch for, because a
    // staleness question asked about other trees is a question about nothing.
    let Ok(targets) = glrefresh::trees(root, &workspace) else {
        return;
    };
    // The answer is dropped on purpose: `Declined` is the brakes working, and `NotStarted` is
    // a fork that did not happen, which the next session start retries because a spawn that
    // failed does not arm the cooldown.
    let _ = glstate::maybe_spawn(root, &workspace, &targets.trees, &binary);
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
    use std::process::{Command, Stdio};

    let me = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    let mut child = Command::new(me);
    child.arg("gl-refresh").arg("-w").arg(ws);
    if let Some(now) = now {
        child.arg("--now").arg(now);
    }
    child
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    in_a_group_of_its_own(&mut child);
    match child.spawn() {
        Ok(_) => Ok(()),
        Err(why) => Err(format!("could not start a detached refresh: {why}")),
    }
}

/// Puts the child in a process group of its own, which is what a harness's teardown at the
/// end of a turn does NOT reach.
#[cfg(unix)]
fn in_a_group_of_its_own(child: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    child.process_group(0);
}

/// Windows has no process group in this sense, and what a harness tears down there is not a
/// group — so this is **not** the same guarantee under another name.
///
/// `CREATE_NEW_PROCESS_GROUP` is the nearest thing: it takes the child out of the parent's
/// Ctrl+C/Ctrl+Break group. Whether that is enough to outlive a Claude Code turn on Windows
/// is unmeasured, because no harness has ever run there — charter-app#100 is where that is
/// asked, with a test. Left here rather than omitted because a refresh in the parent's group
/// is strictly worse than one outside it, and neither is yet known to be right.
#[cfg(windows)]
fn in_a_group_of_its_own(child: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    /// `CREATE_NEW_PROCESS_GROUP`, from `winbase.h`. Spelled out rather than pulled from a
    /// crate for one constant that has not changed since Windows NT.
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    child.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

/// Anywhere else, the refresh simply runs in this process's group.
#[cfg(not(any(unix, windows)))]
fn in_a_group_of_its_own(_child: &mut std::process::Command) {}

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
/// **They resolve the plane exactly as every other command does**
/// ([`charter_core::plane::resolve`]): on the plane the vault, the personas and the memory
/// belong to, out of a linked worktree and outward through an enclosing plane's `workspaces/`.
/// `save`'s two refusals — you are standing in a worktree, you are standing in a nested plane
/// — only exist once that is the resolution, because they are about the caller standing
/// somewhere other than the tree being committed.
///
/// **There is no gap left to describe, and there were two.** M2.9 gave `plane::find_root` the
/// outward hop, so the read commands stopped acting on a clone's own plane; M2.16 gave it the
/// worktree redirect and deleted the second resolver these two used to ask, because a step
/// only `save` takes is a step on which `charter save` and the command beside it name
/// different planes in the same directory. Python resolves once, through
/// `charter/root.py:find_root`, for every command it has.
///
/// What is still special here is only WHEN the plane is resolved: before the ladder, because
/// these two want the plane and never the workspace or the persona.
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
            match charter_core::plane::resolve(&cwd) {
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
            pull,
        } => {
            // A pull that failed, or met conflicts, stops the save: it would stage the markers.
            if *pull && charter_core::planegit::pull(&root, &mut say) != 0 {
                1
            } else {
                charter_core::planegit::save(
                    &charter_core::planegit::Request {
                        root: &root,
                        message: message.as_deref(),
                        sign: *sign,
                        no_push: *no_push,
                        cwd: &cwd,
                    },
                    &mut say,
                )
            }
        }
        Command::GitPolicy { apply } => charter_core::gitpolicy::policy(&root, *apply, &mut say),
        _ => return None,
    };
    if code == 0 && matches!(command, Command::Save { .. }) {
        extensions::tell(&root, &ExtensionEvent::PlaneSaved);
    }
    Some(ExitCode::from(code))
}

/// `docs list` and `docs show`, or `None` for any other command.
///
/// **Kept apart from the `docs` that generates, and before any plane is resolved.** One
/// command describes charter and the other describes your repos; they share a noun and
/// nothing else. `charter/cli.py` hangs all three off one parser and routes them to three
/// functions, of which only `cmd_docs` reads `config.ROOT` — so `charter docs list` answers
/// outside a plane, and a Rust binary that resolved a plane first would refuse there.
///
/// **M2.21, and the alternative was an honest refusal.** These two were clap usage errors
/// (exit 2, the parser's own wording) for verbs the tool being replaced has: a Makefile
/// calling `charter docs list` with a Rust `charter` first on `$PATH` met one. The port costs
/// a vendored directory and a lookup, because `news` had already built the road — so it is
/// the port, not a sentence apologising for the gap.
fn docs_command(command: &Command) -> Option<ExitCode> {
    use charter_core::docsrc;

    let mut say = speak;
    let code = match command {
        Command::Docs {
            what: Some(DocsCommand::List),
        } => docsrc::listing(&mut say),
        Command::Docs {
            what: Some(DocsCommand::Show { topic }),
        } => docsrc::show(topic, &mut say),
        _ => return None,
    };
    Some(ExitCode::from(code))
}

/// `discover`, `clone`, `sync`, `status` and `docs`, or `None` for any other command.
fn repo_command(command: &Command) -> Option<ExitCode> {
    use charter_core::repocmd::{self, Say};

    let mut say = |line: Say| eprintln!("{line}");
    let here = match command {
        Command::Discover { .. }
        | Command::Clone { .. }
        | Command::Sync { .. }
        | Command::Status { .. }
        // `list` and `show` are NOT here: they read charter's own documentation, which this
        // binary carries, and asking `Here::read()` first would make them refuse outside a
        // plane where charter answers. [`docs_command`] takes them before this runs.
        | Command::Docs {
            what: None | Some(DocsCommand::Generate),
        } => match Here::read() {
            Ok(here) => here,
            Err(why) => {
                eprintln!("charter: {why}");
                return Some(ExitCode::FAILURE);
            }
        },
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
        Command::Status { workspace, all } => {
            // Resolved ONCE, and the sentence naming the rung comes off the same answer: a
            // header that named the workspace from one reading and the reason from another
            // would explain it by naming a rung that did not decide it.
            let asking = here.asking(workspace.as_deref(), here.workspace_env.as_deref());
            let chosen = charter_core::active::workspace(&asking);
            let via = charter_core::active::workspace_source(&here.ids, chosen.rung);
            let mut out = |line: String| println!("{line}");
            repocmd::status::status(
                &repocmd::status::Request {
                    root: &root,
                    cwd: &here.cwd,
                    active: &chosen.name,
                    via: &via,
                    all: *all,
                },
                &mut out,
                &mut say,
            )
        }
        // Bare `charter docs` and `charter docs generate` are the same command.
        Command::Docs {
            what: None | Some(DocsCommand::Generate),
        } => {
            let cfg = match charter_core::forge::load_config(&root) {
                Ok(cfg) => cfg,
                Err(why) => {
                    eprintln!("{}", Say::Plain(why));
                    return Some(ExitCode::FAILURE);
                }
            };
            repocmd::docs::docs(&root, &charter_core::forge::group_of(&cfg, 0), &mut say)
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
            | WorkspaceCommand::Rename { .. }
            | WorkspaceCommand::Live { .. }
            | WorkspaceCommand::Use { .. }
            | WorkspaceCommand::Unlock
            | WorkspaceCommand::Default { .. }
            | WorkspaceCommand::Snapshot { .. }
            | WorkspaceCommand::Create { .. }
            | WorkspaceCommand::Fork { .. }
            | WorkspaceCommand::Restore { .. }
            | WorkspaceCommand::Reinit { .. }
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
    // What the extensions that hear it are told once the verb is done and has said everything
    // it says (charter-app#343).
    let mut heard: Vec<ExtensionEvent> = Vec::new();
    let code = match verb {
        WorkspaceCommand::Remove { name, force } => {
            // The list it refused on is the window's (charter-app#182): a terminal has already
            // been given every sentence in it, by the refusal itself.
            let code = wscmd::remove::remove(&root, name, *force, say).code;
            // The active workspace followed the removal: a pointer naming a workspace that is
            // gone resolves to it on every later command, and `workspaces/<gone>` is then
            // created again by the first write. Python resets it the same way and for the
            // same reason; the rung it tests for is spelled here as the two pointer rungs,
            // which are `session`/`active-file` in charter's own vocabulary.
            if code == 0 {
                reset_active_after_removal(&here, name, say);
                heard.push(ExtensionEvent::WorkspaceRemoved {
                    workspace: name.clone(),
                });
            }
            code
        }
        WorkspaceCommand::Rename { old, new } => {
            // The chats the app has open in it, under either name so a rename finished after
            // a crash is guarded too. A terminal cannot see into the app, so it reads the
            // record the app keeps of what it has open, while an app is listening.
            let running = wscmd::rename::open_in_app(&root, &[old.as_str(), new.as_str()]);
            let config_root = charter_core::machine::config_root();
            wscmd::rename::rename(
                &wscmd::rename::Request {
                    root: &root,
                    old,
                    new,
                    running: &running,
                    config_root: config_root.as_deref(),
                },
                say,
            )
        }
        WorkspaceCommand::Live { name, off } => wscmd::live::live(&root, name, *off, say),
        WorkspaceCommand::Use {
            name,
            create,
            force,
            now,
        } => {
            let Some(now) = pinned(now) else {
                return Some(ExitCode::FAILURE);
            };
            // `--create` makes one that is not there, which is a workspace being created too.
            let there_before = Plane::open(&root)
                .workspace(name)
                .is_ok_and(|ws| ws.dir().exists());
            let code =
                wscmd::select::use_workspace(&root, name, &here.ids, *create, *force, now, say);
            if code == 0 {
                if !there_before {
                    heard.push(ExtensionEvent::WorkspaceCreated {
                        workspace: name.clone(),
                    });
                }
                heard.push(ExtensionEvent::WorkspaceFocused {
                    workspace: name.clone(),
                });
            }
            code
        }
        WorkspaceCommand::Create {
            name,
            vision,
            live,
            use_it,
            force,
            repos,
            now,
        } => {
            let Some(now) = pinned(now) else {
                return Some(ExitCode::FAILURE);
            };
            let code = wscmd::create::create(
                &wscmd::create::Request {
                    root: &root,
                    name,
                    vision: vision.as_deref(),
                    live: *live,
                    use_it: *use_it,
                    force: *force,
                    repos,
                    now,
                    ids: &here.ids,
                },
                say,
            );
            if code == 0 {
                heard.push(ExtensionEvent::WorkspaceCreated {
                    workspace: name.clone(),
                });
            }
            code
        }
        WorkspaceCommand::Restore {
            name,
            on_demand,
            now,
        } => {
            let Some(now) = pinned(now) else {
                return Some(ExitCode::FAILURE);
            };
            wscmd::restore::restore(
                &wscmd::restore::Request {
                    root: &root,
                    ws: name,
                    on_demand: *on_demand,
                    now,
                },
                say,
            )
        }
        WorkspaceCommand::Fork {
            src,
            new,
            restore,
            live,
            now,
        } => {
            let Some(now) = pinned(now) else {
                return Some(ExitCode::FAILURE);
            };
            // A fork that could not read every piece still exists and exits 1 (charter#1084),
            // so whether one was made is asked of the disk: there before, it is not this one.
            let made_here = || {
                charter_core::workspaces::Plane::open(&root)
                    .workspace(new)
                    .is_ok_and(|ws| ws.dir().exists())
            };
            let there_before = made_here();
            let extension_folders = extensions::carried();
            let code = wscmd::fork::fork(
                &wscmd::fork::Request {
                    root: &root,
                    src,
                    new,
                    live: *live,
                    restore: *restore,
                    now,
                    extension_folders: &extension_folders,
                },
                say,
            );
            if !there_before && made_here() {
                heard.push(ExtensionEvent::WorkspaceForked {
                    workspace: new.clone(),
                    from: src.clone(),
                });
            }
            code
        }
        WorkspaceCommand::Reinit { name, all, now } => {
            let Some(now) = pinned(now) else {
                return Some(ExitCode::FAILURE);
            };
            // With no `--all` and no name, the ladder: "default: the active one", which is
            // what charter's own `reinit` resolves.
            let one = (!*all).then(|| here.active_workspace(name.as_deref()));
            let scope = match &one {
                Some(ws) => wscmd::reinit::Scope::One(ws),
                None => wscmd::reinit::Scope::All,
            };
            wscmd::reinit::reinit(&root, scope, now, say)
        }
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
    for event in &heard {
        extensions::tell(&root, event);
    }
    Some(ExitCode::from(code))
}

/// `--now` as an instant, or the wall clock when it was not given.
///
/// A naive stamp is LOCAL time, as `--now` is everywhere in this binary. `None` back means the
/// value was not an instant and the caller has already had the reason printed.
fn pinned(now: &Option<String>) -> Option<chrono::DateTime<chrono::Utc>> {
    let Some(text) = now else {
        return Some(chrono::Utc::now());
    };
    let naive: chrono::NaiveDateTime = match text.parse() {
        Ok(naive) => naive,
        Err(e) => {
            eprintln!("charter: --now is not a local naive timestamp: {e}");
            return None;
        }
    };
    match chrono::TimeZone::from_local_datetime(&chrono::Local, &naive).single() {
        Some(local) => Some(local.with_timezone(&chrono::Utc)),
        None => {
            eprintln!("charter: --now names no single local instant");
            None
        }
    }
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
        | Command::Version { .. }
        | Command::Discover { .. }
        | Command::Clone { .. }
        | Command::Sync { .. }
        | Command::Status { .. }
        | Command::Docs { .. }
        | Command::Doctor { .. }
        | Command::GlRefresh { .. }
        | Command::Statusline { .. }
        | Command::Save { .. }
        | Command::Handoff { .. }
        | Command::Report(_)
        | Command::Workspace(WorkspaceCommand::Remove { .. })
        | Command::Workspace(WorkspaceCommand::Rename { .. })
        | Command::Workspace(WorkspaceCommand::Live { .. })
        | Command::Workspace(WorkspaceCommand::Use { .. })
        | Command::Workspace(WorkspaceCommand::Unlock)
        | Command::Workspace(WorkspaceCommand::Default { .. })
        | Command::Workspace(WorkspaceCommand::Snapshot { .. })
        | Command::Workspace(WorkspaceCommand::Create { .. })
        | Command::Workspace(WorkspaceCommand::Fork { .. })
        | Command::Workspace(WorkspaceCommand::Restore { .. })
        | Command::Workspace(WorkspaceCommand::Reinit { .. })
        | Command::Workspace(WorkspaceCommand::Reconcile)
        | Command::Workspace(WorkspaceCommand::Autosave)
        | Command::GitPolicy { .. }
        | Command::Secret(_)
        | Command::Plugin(_)
        | Command::Vault(_) => {
            unreachable!("answered before run")
        }
        Command::Root => {
            println!("{}", here.plane.root().display());
        }
        Command::Workspace(WorkspaceCommand::Current) => {
            println!("{}", here.active_workspace(None));
        }
        Command::Guard { verb } => {
            use charter_core::guardcmd::{self, Bucket};
            let root = here.plane.root().to_path_buf();
            let (pattern, bucket, local) = match &verb {
                None | Some(GuardCommand::List) => {
                    let (listing, whole) = guardcmd::list(&root);
                    print!("{listing}");
                    return Ok(u8::from(!whole));
                }
                Some(GuardCommand::Ask { pattern, local }) => {
                    (pattern.as_str(), Bucket::Ask, *local)
                }
                Some(GuardCommand::Allow { pattern, local }) => {
                    (pattern.as_str(), Bucket::Allow, *local)
                }
                Some(GuardCommand::Handoff) => (guardcmd::HANDOFF_PATTERN, Bucket::Ask, false),
            };
            let rule = match guardcmd::as_rule(pattern) {
                Ok(rule) => rule,
                Err(why) => {
                    eprintln!("charter: {why} Example: charter guard ask 'terraform apply *'");
                    return Ok(2);
                }
            };
            let (said, code) = guardcmd::report(&root, &rule, bucket, local);
            print!("{said}");
            return Ok(code);
        }
        Command::Browser(BrowserCommand::Install { version }) => {
            let path = std::env::var_os("PATH");
            let mut sink = speak;
            return Ok(charter_core::browser::install(
                here.plane.root(),
                version.as_deref(),
                charter_core::browser::npx_on(path.as_deref()),
                &mut charter_core::browser::Npx,
                &mut sink,
            ));
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
        Command::Worktree(command) => return piece::run(&here, command),
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
            return Ok(todo(&here.plane, &ws, &words, stamp));
        }
    }
    Ok(0)
}

/// `charter ws todo` — record one todo, list them, or close one with `done`/`forget <slug>`,
/// saying what it did as `commands_workspace.cmd_workspace_todo` says it (M8.5: the port
/// did the writes and said nothing, so an agent recording a todo had no confirmation and
/// one closing a mistyped slug could not tell it had closed nothing).
fn todo(
    plane: &Plane,
    ws: &charter_core::workspaces::Workspace,
    words: &[String],
    stamp: chrono::NaiveDateTime,
) -> u8 {
    let root = plane.root();
    let name = ws.name();
    let dir = ws.dir().join("todos");
    let see = format!("charter ws todo --workspace {name}");
    match words {
        [] => {
            let (open, unread) = charter_core::memstore::read_entries(root, &dir);
            voice::unread(root, &unread);
            if open.is_empty() {
                voice::info(&format!(
                    "No open todos in '{name}'. Record one: charter ws todo \"<what>\""
                ));
                return 0;
            }
            // Oldest first, with its age: what surfaces is what is being avoided.
            let today = stamp.date();
            for todo in open {
                let file = todo.path.file_name().unwrap_or_default().to_string_lossy();
                let stem = todo.path.file_stem().unwrap_or_default().to_string_lossy();
                let age = charter_core::memstore::memory_date(&todo.text, &file)
                    .map_or(0, |d| (today - d).num_days());
                println!(
                    "  {stem}  {age}d  {}",
                    charter_core::personas::one_line(&todo.title)
                );
            }
            0
        }
        [verb, slug] if verb == "done" || verb == "forget" => {
            let slug = charter_core::memstore::py_strip(slug);
            if !charter_core::contain::segment_ok(slug) {
                voice::err(&charter_core::repocmd::clone::not_a_segment(slug));
                voice::info(&format!("  List the real ones: {see}"));
                return 1;
            }
            let Some(path) = charter_core::memstore::resolve(root, &dir, slug) else {
                voice::err(&format!("no todo '{slug}' in workspace '{name}'."));
                voice::info(&format!("  List the real ones: {see}"));
                return 1;
            };
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let title = ws
                .todos()
                .ok()
                .and_then(|open| open.into_iter().find(|t| t.slug == stem))
                .map_or_else(|| stem.clone(), |t| t.title);
            if verb == "done" {
                // The journal entry first, while the todo is still there to name: it is the
                // only evidence that survives the close.
                let code = match memory::workspace_remember(
                    plane,
                    name,
                    Some(&format!("Closed todo: {title}")),
                    None,
                    false,
                    Some(&stamp.format("%Y-%m-%dT%H:%M:%S").to_string()),
                ) {
                    Ok(code) => code,
                    Err(e) => {
                        voice::err(&e);
                        1
                    }
                };
                if code != 0 {
                    return code;
                }
            }
            if let Err(e) = charter_core::memstore::forget(root, &dir, slug) {
                voice::err(&e.to_string());
                return 1;
            }
            if verb == "done" {
                voice::ok(&format!(
                    "Closed '{title}' in '{name}' — the journal has the trace."
                ));
            } else {
                voice::ok(&format!(
                    "Dropped '{title}' from '{name}' — abandoned, so nothing was journalled."
                ));
            }
            0
        }
        // A lone verb is NOT todo text. charter refuses it, and the reason is that
        // `todo done` with a forgotten slug would otherwise record a todo called "done" and
        // leave the one it meant to close open.
        [verb] if verb == "done" || verb == "forget" => {
            voice::err(&format!(
                "`todo {verb}` needs the slug of the todo to close."
            ));
            voice::info(&format!("  The slug is the first column: {see}"));
            let capital: String = verb
                .chars()
                .take(1)
                .flat_map(char::to_uppercase)
                .chain(verb.chars().skip(1))
                .collect();
            voice::info(&format!(
                "  To record a todo actually called \"{verb}\", capitalise it or add a word: \
                 charter ws todo \"{capital} …\""
            ));
            1
        }
        [text] => {
            // Duplicate INTENT is worse than duplicate memory: closing one of a near-identical
            // pair leaves its twin looking outstanding, so the list starts lying about what is
            // left. Warn and skip rather than merge. CONTAINED, as `commands_workspace.py`
            // contains it: `dup` is the `# ` heading of a file on disk, and since `charter
            // handoff` a stored title can be a model's prose.
            if let Some(dup) = charter_core::memstore::duplicate_of(root, &dir, text) {
                voice::err(&format!(
                    "already on the list: {}",
                    charter_core::personas::one_line(&dup)
                ));
                voice::info(&format!("  See it: {see}"));
                return 1;
            }
            match ws.add_todo(text, stamp) {
                Ok(path) => {
                    voice::ok(&format!(
                        "Todo recorded in '{name}' → workspaces/{name}/todos/{}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    if !plane.is_live(name) {
                        voice::info(&format!(
                            "  '{name}' is LOCAL (private) — todos stay on disk, not committed."
                        ));
                    }
                    0
                }
                Err(e) => {
                    voice::err(&e.to_string());
                    1
                }
            }
        }
        _ => {
            voice::err("usage: charter ws todo [-w WS] [\"<text>\" | done <slug>]");
            1
        }
    }
}

/// This machine, as `charter plugin` and `charter doctor --fix` act on it: this binary by its
/// resolved path, and the plugin from `from` or else the one the app ships beside it.
fn plugin_machine(
    from: Option<&std::path::Path>,
) -> Result<charter_core::plugin_install::Machine, String> {
    use charter_core::plugin_install as install;
    let binary = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| format!("cannot tell where this charter is, so no hook could name it: {e}"))?;
    let bundle = match from {
        Some(dir) => Some(
            dir.canonicalize()
                .map_err(|e| format!("--plugin-from {}: {e}", dir.display()))?,
        ),
        None => install::bundle_beside(&binary),
    };
    install::Machine::from_env(binary, bundle)
}

/// `charter plugin install|uninstall`: needs no plane, and acts on this machine's harnesses.
fn plugin(verb: &PluginCommand) -> ExitCode {
    use charter_core::plugin_install::{self as install, Verb};
    let (verb, harness, dry_run, from) = match verb {
        PluginCommand::Install {
            harness,
            dry_run,
            plugin_from,
        } => (Verb::Install, harness, *dry_run, plugin_from.clone()),
        PluginCommand::Uninstall { harness, dry_run } => (Verb::Uninstall, harness, *dry_run, None),
    };
    let machine = match plugin_machine(from.as_deref()) {
        Ok(machine) => machine,
        Err(why) => {
            eprintln!("charter: {why}");
            return ExitCode::FAILURE;
        }
    };
    let outcomes = install::run(&machine, verb, harness, dry_run);
    print!("{}", install::render(&outcomes, dry_run));
    if install::failed(&outcomes) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    // FIRST, before anything that could panic, argv and clap included: a guard that crashed
    // must refuse the tool call, not allow it (#349).
    if is_a_pretooluse_call(&std::env::args_os().collect::<Vec<_>>()) {
        guard::refuse_on_a_crash();
    }
    #[cfg(debug_assertions)]
    if std::env::var_os(PANIC_ON_PURPOSE_ENV).is_some() {
        panic!("{PANIC_ON_PURPOSE_ENV} is set, so charter crashes");
    }
    // **`Cli::parse` exits 2 on a bad command line, and 2 is the one code a harness reads as
    // "block".** A hook that exited 2 by accident would make a session unable to end, so this
    // binary answers for its own argv before clap can, whatever the command turns out to be.
    // `secret exec`'s `-- <command…>` is the child's, flags included, and is peeled off before
    // the parser can read any of it as charter's — `cli._split_exec_command`.
    let (argv, graft) = secret::split_exec(std::env::args_os().collect());
    // An extension's command, or a core-owned alias onto one (charter-app#342) — asked only
    // about a first word clap does not answer itself, so a core word is always the core's.
    let mut parser = {
        use clap::CommandFactory;
        Cli::command()
    };
    parser.build();
    if let Some(code) = extcmd::intercept(&argv, |word| parser.find_subcommand(word).is_some()) {
        return code;
    }
    // The first word, when clap does not answer it: what the line after clap's own error names.
    // clap reports a bad word under a core command with the same error kind, and that one is
    // not about extensions.
    let unknown_first = argv
        .get(1)
        .map(|word| word.to_string_lossy().into_owned())
        .filter(|word| parser.find_subcommand(word).is_none());
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            if err.kind() == clap::error::ErrorKind::InvalidSubcommand
                && let Some(word) = &unknown_first
            {
                extcmd::note_after_an_unknown_word(word);
            }
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
        list,
        json,
        plugin_version: _,
        now,
    } = &cli.command
    {
        if *list {
            return hooks::list(*json);
        }
        return hook(name.as_deref().unwrap_or_default(), now.as_deref());
    }
    // Needs no plane: a chat anywhere can report a charter bug.
    if let Command::Report(command) = &cli.command {
        return report::run(command);
    }
    // The three internal words the Python charter's plugin wires beside its hooks. Answered —
    // exit 0, nothing printed, nothing read — so a plugin that still names them can never fail
    // a session start or a turn end on them; see `hookreg` for why each is not ported.
    if matches!(
        &cli.command,
        Command::Workspace(WorkspaceCommand::Reconcile | WorkspaceCommand::Autosave)
            | Command::Persona(memory::PersonaCommand::Gc { .. })
    ) {
        return ExitCode::SUCCESS;
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
                plane_is_this_repo: init.plane_is_this_repo,
                adopt: init.adopt.clone(),
                // Refused rather than quietly replaced by the wall clock: the flag exists so
                // the differential can pin the first workspace's manifest, and a pin that
                // silently did not take would make that comparison green for the wrong reason.
                now: match instant(init.now.as_deref()) {
                    Ok(secs) => chrono::DateTime::from_timestamp(secs as i64, 0),
                    Err(why) => {
                        eprintln!("charter: {why}");
                        return ExitCode::FAILURE;
                    }
                },
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
        Command::Plugin(verb) => return plugin(verb),
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
        // `charter version`, and it needs no plane: Python builds `config.ROOT` from
        // `find_root_or_cwd`, so the command answers outside one and simply has no pin to
        // report. What it answers, and why it is not Python's three rows, is ADR 0030 as
        // amended by ADR 0045.
        Command::Version { what } => {
            use charter_core::adopt;
            return emit(&match what {
                Some(VersionCommand::Sync { .. }) => adopt::version_move_refusal("sync"),
                Some(VersionCommand::Bump { .. }) => adopt::version_move_refusal("bump"),
                None => {
                    let cwd =
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                    let place = charter_core::plane::place(&cwd);
                    adopt::version_report(place.is_plane.then_some(place.root.as_path()))
                }
            });
        }
        // `news` and `update` say several lines of their own on both streams and choose their
        // own exit status, exactly as `init` does.
        Command::News(news) => {
            return emit(&charter_core::news::report(&charter_core::news::Args {
                for_version: news.for_version.clone(),
                since: news.since.is_some(),
                until: news.until.is_some(),
                pending: news.pending,
            }));
        }
        Command::Update { to, bump, channel } => {
            let config_root = charter_core::machine::config_root();
            if let Some(word) = channel {
                return emit(&charter_core::adopt::set_channel_report(
                    config_root.as_deref(),
                    word,
                ));
            }
            let args = charter_core::adopt::UpdateArgs {
                to: to.clone().unwrap_or_default(),
                bump: *bump,
            };
            return emit(&charter_core::adopt::update_report_with_channel(
                config_root.as_deref(),
                &args,
            ));
        }
        _ => {}
    }
    // The repo commands speak line by line as they go — a clone is slow, and the line that
    // says which repo is being fetched is worth nothing once it has been — and choose their
    // own exit status, as their Python counterparts do.
    // Before the repo commands, because `docs list` and `docs show` need no plane and those
    // resolve one first.
    if let Some(code) = docs_command(&cli.command) {
        return code;
    }
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
        summary,
        name,
        report,
        create,
        vision,
        persona,
        now,
    } = &cli.command
    {
        let here = match Here::read() {
            Ok(here) => here,
            Err(why) => {
                eprintln!("charter: {why}");
                return ExitCode::FAILURE;
            }
        };
        let code = handoff::handoff(
            &here,
            &handoff::Args {
                workspace: workspace.clone(),
                create: *create,
                vision: vision.clone(),
                persona: persona.clone(),
                name: name.clone(),
                report: *report,
                summary: summary.clone(),
                now: now.clone(),
            },
        );
        if code == ExitCode::SUCCESS {
            extensions::tell(
                here.plane.root(),
                &ExtensionEvent::HandoffCreated {
                    workspace: workspace.clone(),
                },
            );
        }
        return code;
    }
    // The secrets commands: each chooses its own exit status (2 for a refusal, the child's for
    // `exec`), and `exec` takes the command the argv split set aside.
    let command = match cli.command {
        Command::Secret(c) => return with_here(|here| secret::secret(here, c, graft)),
        Command::Vault(c) => return with_here(|here| secret::vault(here, c)),
        Command::Persona(memory::PersonaCommand::Secret(c)) => {
            return with_here(|here| secret::persona_secret(here, c, graft));
        }
        other => other,
    };
    match run(command) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("charter: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Run `f` on this invocation's [`Here`], or say why there is none.
fn with_here(f: impl FnOnce(&Here) -> u8) -> ExitCode {
    match Here::read() {
        Ok(here) => ExitCode::from(f(&here)),
        Err(why) => {
            eprintln!("charter: {why}");
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

    // The one repair: charter's plugin, for the chats started outside the app (#373). Its
    // steps go to stderr so a `--json` reader still gets JSON alone.
    let mut fix_failed = false;
    if fix {
        match plugin_machine(None) {
            Ok(machine) => {
                use charter_core::plugin_install as install;
                let outcomes = install::run(&machine, install::Verb::Install, &[], false);
                eprint!("{}", install::render(&outcomes, false));
                fix_failed = install::failed(&outcomes);
            }
            Err(why) => {
                eprintln!("charter: --fix installed nothing: {why}");
                fix_failed = true;
            }
        }
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
    ExitCode::from(charter_core::doctor::exit_code(&rows).max(u8::from(fix_failed)))
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
mod core_word_tests {
    use super::*;
    use clap::CommandFactory;
    use std::collections::BTreeSet;

    /// Every word this binary's parser answers as its first argument: each command's name,
    /// every other name clap takes for it, and clap's own `help` — read off the parser, so a
    /// command added to it is in this set the day it is added.
    fn words_the_parser_answers() -> BTreeSet<String> {
        let mut root = Cli::command();
        root.build();
        root.get_subcommands()
            .flat_map(|sub| {
                std::iter::once(sub.get_name().to_owned())
                    .chain(sub.get_all_aliases().map(str::to_owned))
            })
            .collect()
    }

    #[test]
    fn every_word_charter_answers_is_one_no_extension_may_take() {
        // **charter-app#342: an extension can never take over a core word.** The refusal lives
        // in the core, which the app calls to approve an extension and which cannot read this
        // parser — so this is where the list it refuses by is held to the parser, both ways.
        // A new core command that is not in `CORE_WORDS` fails here, in the change adding it.
        let parser = words_the_parser_answers();
        let refused: BTreeSet<String> = charter_core::extension::cli::CORE_WORDS
            .iter()
            .map(|&word| word.to_owned())
            .collect();
        let unprotected: Vec<&String> = parser.difference(&refused).collect();
        assert!(
            unprotected.is_empty(),
            "`charter {unprotected:?}` is a core command an extension could still take as its \
             id — add it to charter_core::extension::cli::CORE_WORDS"
        );
        let stale: Vec<&String> = refused.difference(&parser).collect();
        assert!(
            stale.is_empty(),
            "CORE_WORDS refuses {stale:?}, which is no longer a word charter answers"
        );
    }
}
