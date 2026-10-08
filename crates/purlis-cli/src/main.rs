//! The `charter` command line, in Rust.
//!
//! Only the plane commands M1.1 covers are here, and only as far as the PLANE goes. The
//! recorded scenarios (`tests/fixtures/recorded/`, ADR 0046) prove that: each runs this binary
//! against a copy of a fixture plane and compares the tree it leaves with what the Python
//! charter left.
//!
//! One thing this binary does NOT do yet, recorded in the harness rather than left to be
//! discovered, and planned in #999:
//!
//! - **Not every command prints its confirmation.** charter says `✓ Vision set for 'alpha' →
//!   …` on stderr, and `vision` here is still silent (`todo` speaks since M8.5). The memory commands
//!   (`memory.rs`) are ported whole, their output included, and so is the one line of stdout
//!   `workspace current` and `persona current` print — the sentence explaining which rung
//!   decided is not.
//!
//! **`-w` and `--persona` are optional, and M2.9 is what made them so.** Every rung of both
//! resolution ladders lives in [`purlis_core::active`]; this file only decides which flag
//! feeds each one. Until then `charter recall` with no flags — how a harness calls it at
//! session start — refused with exit **2**, and every `-w` was a clap usage error.
//!
//! `root` and the hidden `--now` have no Python counterpart at all: `--now` is the test seam
//! charter itself has as `memstore.write(stamp=…)`.

use std::process::ExitCode;

use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use purlis_core::extension::events::Event as ExtensionEvent;
use purlis_core::hookwire::{self, Report, SOCKET_ENV};
use purlis_core::profiles::{self, ProfileSet};
use purlis_core::shown;
use purlis_core::state::Event;
use purlis_core::workspaces::Plane;

mod brokered;
mod change;
mod curation;
mod dispatch;
mod extcmd;
mod extensions;
mod gitask;
mod githook;
mod guard;
mod handoff;
mod hooks;
mod mcp;
mod memory;
mod permission;
mod piece;
mod report;
mod scan;
mod secret;
mod session;
mod shellguard;
mod statusline;
mod voice;
mod whereworking;

/// One line of a ported command, in the voice purlis says it in.
///
/// [`purlis_core::repocmd::Say`] carries the mark and the message as a value so a test can
/// read them back; this is the one place they become the coloured line `charter/util.py`
/// prints. `eprintln!("{line}")` would print the mark uncoloured, which is right in a pipe
/// and wrong in a terminal.
fn speak(line: purlis_core::repocmd::Say) {
    use purlis_core::repocmd::Say;
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
    name = "purlis",
    version = purlis_core::adopt::VERSION_LINE,
    about = "purlis: run tons of harness sessions in parallel"
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
    /// Additive and idempotent — never touches existing content. A path purlis would write
    /// that is occupied by something it cannot safely touch, or that leads out of the plane,
    /// is named and left alone; everything else is still created, and the exit is 1.
    Init(InitCommand),
    /// Heal control-plane drift: create any missing baseline directory a newer purlis
    /// expects. Idempotent and additive — existing content is never touched.
    Reinit,
    /// Workspaces: their vision, their memory, their todos.
    #[command(subcommand, alias = "ws")]
    Workspace(WorkspaceCommand),

    /// Pieces: worktrees of a workspace's clones — cut, declared done or abandoned, removed.
    #[command(subcommand, alias = "wt")]
    Worktree(piece::WorktreeCommand),

    /// A cross-repo change: one piece of work across several of a workspace's repos — why,
    /// which repos, which branch in each, and which must land first
    /// (workspaces/<ws>/changes/<slug>.json).
    #[command(subcommand)]
    Change(change::ChangeCommand),

    /// Curation actions: what a workspace, a persona or the plane is offered — purlis's own
    /// and each persona's (personas/<name>/curation/<id>.md) — each a chat opened with its
    /// prompt typed and never sent.
    #[command(subcommand)]
    Curation(curation::CurationCommand),

    /// Session records: what a chat leaves behind when it closes through Smart close — a
    /// summary of its session in its workspace's sessions/ (or the plane's, at the plane
    /// root), never the transcript.
    #[command(subcommand)]
    Session(session::SessionCommand),

    /// Harness profiles: which program a chat runs, and with what environment.
    #[command(subcommand)]
    Harness(HarnessCommand),

    /// purlis's plugin for a `claude` or `codex` started outside the app: its hooks, the Bash
    /// guard and its skills. The app arms its own chats; this is for the others.
    #[command(subcommand)]
    Plugin(PluginCommand),

    /// Force-prompt and stop-prompting rules for this plane, written in each harness's own
    /// syntax into the file each one reads — Claude Code, opencode and Codex, not only the one
    /// you are running. purlis keeps no list of its own (ADR 0014). Bare, it lists them.
    Guard {
        #[command(subcommand)]
        verb: Option<GuardCommand>,
    },

    /// The browser lane: purlis's plugin ships the credential bridge (the `browser` skill),
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
    /// a branch and one of `clean`, `dirty` or `unknown` — never `clean` for a tree purlis
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
    /// Bare `purlis docs` generates, as it did long before it grew subcommands: Makefiles in
    /// the wild call it that way, and making the group require a subcommand would refuse a
    /// command line planes already have.
    ///
    /// `docs list` and `docs show` read purlis's OWN documentation and are not this binary's
    /// — one command describes purlis, the other describes your repos.
    Docs {
        #[command(subcommand)]
        what: Option<DocsCommand>,
    },

    /// Commit and push the control plane's own changes over its forge's HTTPS token.
    ///
    /// It stages EVERYTHING pending in the plane's tree, not only what you changed, and prints
    /// the directory breakdown of what it is about to commit before it commits it.
    Save {
        /// The commit message. Default: `purlis save: N file(s)`.
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
    /// Exits non-zero only on a blocker. Every check this purlis does not run yet is still
    /// listed, as a warning saying it was not checked — never as a pass.
    Doctor {
        /// Emit machine-readable results.
        #[arg(long)]
        json: bool,
        /// Run as the SessionStart hook does: no harness-profile probe and no git call for
        /// one. Every other check runs.
        #[arg(long)]
        preflight: bool,
        /// Repair first, then report. With no ID: install purlis's plugin for chats started
        /// outside the app (the `plugin-install` fix, what `purlis plugin install` does),
        /// apply the local fixes the doctor's rows offer (`reinit`, `local-ignore`,
        /// `memory-optimize`), and add the plane's default ask rule for `charter report --yes`
        /// when it is missing (what `purlis guard ask` does). `discover` goes over the network,
        /// and `rename-plane` commits the project's files under purlis's names, so each runs
        /// only by name. With an ID (the `fix` a row carries in `--json`): apply that fix
        /// alone. What each changed, or why it was refused, is printed on stderr.
        #[arg(
            long,
            value_name = "ID",
            num_args = 0..=1,
            value_parser = clap::builder::PossibleValuesParser::new(
                purlis_core::doctor::fix::FixId::ALL.map(purlis_core::doctor::fix::FixId::id)
            ),
        )]
        fix: Option<Option<String>>,
        /// The name `--fix git-identity` sets as git's global `user.name`.
        #[arg(long, value_name = "NAME", requires = "fix")]
        name: Option<String>,
        /// The email `--fix git-identity` sets as git's global `user.email`.
        #[arg(long, value_name = "EMAIL", requires = "fix")]
        email: Option<String>,
    },

    /// Move this machine's local state to the purlis names, or put it back with --undo.
    ///
    /// The config home and the session host's folder in it, the data home, the app's log
    /// folder, and in this project and each one this machine remembers, `charter.local.toml`
    /// and `.charter/`. Git is made to ignore the new names through the repository's own
    /// `info/exclude`; no committed file changes. Every move is journalled, and a move that
    /// fails leaves the old name where it was. Nothing moves while a running purlis has a
    /// project open. The app does this at launch; `--undo` stops that until this is run again.
    Migrate {
        /// Put back every move since the last undo, newest first.
        #[arg(long)]
        undo: bool,
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
        /// Resolved through [`purlis_core::active`] since M2.9, and resolved BEFORE
        /// `--detach` rather than after: the child is handed the name this process worked
        /// out, so a refresh cannot end up keyed to a different workspace than the one the
        /// operator was standing in.
        #[arg(short = 'w', long = "workspace")]
        workspace: Option<String>,
        /// Return at once and refresh in a process that outlives this one.
        ///
        /// What a hook's `async` used to buy, done by purlis — one harness skips async hooks
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
        /// Refused in this version: statusline does not repaint yet.
        ///
        /// Drawing one frame and exiting 0 read as a watch that stopped by itself. The flag is
        /// still parsed, so a plane wired for `purlis statusline --watch` meets purlis's
        /// reason, not a usage error.
        #[arg(long)]
        watch: bool,
        /// Refused with --watch: there are no repaints to space out yet.
        ///
        /// Parsed for the same reason `--watch` is, and never used.
        #[allow(dead_code)]
        #[arg(long, default_value = "10")]
        interval: f64,
        /// Pin the instant the footer's ages are measured from, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },

    /// The kill switch: stop every chat and shell purlis started, in every project and window
    /// the app holds, and start no chat until you re-arm it in the app's title bar.
    ///
    /// It leaves one line in the kill switch's journal. There is no command to re-arm: an agent
    /// can run any command, and letting agents start again is the operator's to decide
    /// (ADR 0071). Agents purlis did not start, headless ones among them, are not reached.
    Stop {
        /// Every agent. Required: it is the only scope there is.
        #[arg(long, required = true)]
        all: bool,
    },

    /// What each version of the app brought: its CHANGELOG.md, newest first.
    News(NewsCommand),

    /// Which channel the app updates from. This command does NOT install anything.
    ///
    /// charter's own `update` moves a Python package with `uv tool install`; this purlis is a
    /// binary inside the app, and the app is what moves it. So `update` here says so, names the
    /// channel (`--channel` moves it), and points at `purlis news` for what a version brought.
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

    /// Which purlis this is, what this control plane pins, and whether they agree.
    ///
    /// Not purlis's three rows, and ADR 0030 is why: two of them — the installed wheel and
    /// the newest one on PyPI — have no subject for a binary that ships inside the app. What
    /// this prints instead is the app's own version and the pin itself (ADR 0045). The EXIT
    /// STATUS is purlis's: 0 with no pin, 0 when the pin is met, 1 on drift.
    Version {
        #[command(subcommand)]
        what: Option<VersionCommand>,
    },

    /// File a bug or a feature request on purlis's own tracker, under your own `gh` login.
    /// Shows the draft first and sends nothing without your yes.
    #[command(subcommand)]
    Report(report::ReportCommand),

    /// What a shell tab's shims run in front of a harness started by hand there (ADR 0062):
    /// says it runs outside purlis's session tracking, tells the app, and runs the real one.
    ///
    /// Hidden: nobody types it. The shims purlis writes at every launch are its one caller.
    #[command(name = "shell-guard", hide = true)]
    ShellGuard {
        /// The shim directory, left out of the search and off the harness's `PATH`.
        #[arg(long, value_name = "DIR")]
        shims: std::path::PathBuf,
        /// The harness the operator typed: `claude`, `codex` or `opencode`.
        harness: String,
        /// Its arguments, exactly as typed, after `--`.
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<std::ffi::OsString>,
    },

    /// purlis's MCP server, on stdin and stdout: the tools a chat is offered for its
    /// workspace's todos, memory, session records and changes, and a question for the operator
    /// (HP-7).
    ///
    /// Hidden: nobody types it. Each harness starts it for a chat the app armed.
    #[command(name = "mcp", hide = true)]
    Mcp,

    /// Scan what is staged here as a chat's commit would be scanned: secrets and personal data
    /// in the lines it adds, and what the allowlist lets through. Commits nothing.
    Scan {
        /// For each finding, name its rule and the `.charter-scan-allow.toml` entry that would
        /// let it through, for the operator to review and commit.
        #[arg(long)]
        explain: bool,
    },

    /// purlis's part of a chat's git hooks (SQ-16): for `pre-commit` and `pre-merge-commit`, scans what the commit
    /// adds for secrets and personal data, and refuses it on a finding; for `commit-msg`,
    /// stamps the message with the chat's provenance trailers (GL-8).
    ///
    /// Hidden: nobody types it. The hooks the app writes at every launch are its one caller,
    /// and they run the repository's own hook after it.
    #[command(name = "git-hook", hide = true)]
    GitHook {
        /// The hook git is running: `pre-commit`, `pre-merge-commit` or `commit-msg`.
        name: String,
        /// git's own arguments to the hook: `commit-msg`'s message file.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Dispatch a task to a persona: a chat of its own in the app, started on a brief you pass
    /// as a quoted heredoc on stdin, which reports back to this chat.
    ///
    /// The new chat runs as the persona you name with --to, or as this chat's own persona,
    /// which needs no grant. It works in this chat's folder, with the sandbox, hosts and vaults
    /// the project gives that persona, and is listed under this chat in the explorer. Its
    /// report reaches this chat as context on its next turn. A persona other than this chat's
    /// own needs a dispatch grant, which the person gives: where there is none the
    /// person is asked on this chat's tab, and the task starts when they allow it.
    ///
    /// It starts on the persona's own harness profile where its definition names one, else on
    /// this chat's; --profile names another of the project's profiles.
    ///
    /// --in says where else it works: `--in workspace:<name>` starts it in another workspace
    /// of the project, and `--in worktree` gives it a new worktree of the repo this chat works
    /// in, on a new branch purlis names. Nothing is merged for it. Where the project has no
    /// sandbox, a worktree task commits on that branch and its report names it. Where the new
    /// chat is sandboxed its own `git commit` is refused in a worktree, and it commits with
    /// `purlis worktree commit`, which the app runs for it.
    ///
    /// `purlis dispatch report --outcome done "<text>"`, from a chat a dispatch started, sends
    /// its one report back.
    ///
    /// The chat that asked can wait for that report (`--wait`, or `purlis dispatch wait
    /// <chat>` later), list its tasks (`purlis dispatch list`) and cancel one (`purlis dispatch
    /// cancel <chat>`). A report nobody waits for reaches the chat as context when it lands.
    ///
    /// While a task works, the chat that asked can send it a follow-up (`purlis dispatch tell
    /// <chat> "<text>"`), and the task can send a progress note (`purlis dispatch note
    /// "<text>"`) or ask a question and wait for the answer (`purlis dispatch ask
    /// "<question>"`, answered with `purlis dispatch answer <chat> "<text>"`).
    #[command(args_conflicts_with_subcommands = true)]
    Dispatch {
        /// The persona the new chat runs as (default: this chat's own).
        #[arg(long)]
        to: Option<String>,
        /// A short name for the task, which the new chat is called and listed under
        /// (`check the queue`). At most 64 characters.
        #[arg(long)]
        name: Option<String>,
        /// One of the project's harness profiles to start the new chat on (default: the
        /// persona's own, else this chat's).
        #[arg(long)]
        profile: Option<String>,
        /// Wait for the task's report and print it as this command's result, instead of
        /// carrying on. A wait that runs out says the task is still running.
        #[arg(long)]
        wait: bool,
        /// With --wait, how long to wait, in seconds (default 100, at most 540).
        #[arg(long, requires = "wait")]
        timeout: Option<u32>,
        /// Where the new chat works: `workspace:<name>` for another workspace, or `worktree`
        /// for a new worktree on its own branch (default: this chat's folder, or a worktree
        /// where the persona's definition says `dispatch-isolation: worktree`).
        #[arg(long = "in", value_name = "WHERE")]
        place: Option<String>,
        #[command(subcommand)]
        command: Option<dispatch::DispatchCommand>,
    },

    /// Open a chat in a workspace you name, already working on a brief you pass as a quoted
    /// heredoc on stdin. A handoff is a dispatch: to this chat's own persona it opens at
    /// once, and to another purlis asks you once for the pair, on this chat's tab.
    ///
    /// Work this chat needs an answer from, for its own persona or another, is `purlis
    /// dispatch` (with `--in workspace:<name>` when it must run elsewhere). A handoff is
    /// fire-and-forget: the person's work moves to a chat they will read themselves.
    ///
    /// **The app opens the chat.** Run from a chat the purlis app started, the new chat
    /// opens there as a tab in the workspace you name. With no app running, nothing is
    /// opened and purlis says to open the app. In front of that purlis refuses a handoff
    /// from a helper sub-agent, and one whose brief it cannot read as it is written
    /// (`purlis_core::handoff`).
    ///
    /// `charter handoff report "<summary>"` sends the one report of a chat an older handoff
    /// opened owing one. A chat started as a task reports with `purlis dispatch report`.
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
        /// Kept for chats that learned it: the work is dispatched as a task of this chat, as
        /// `purlis dispatch --in workspace:<name>` does it, and reports back. Use `purlis
        /// dispatch` instead.
        #[arg(long)]
        report: bool,
        /// Make the workspace first (LOCAL, never LIVE). Needs --vision.
        #[arg(long)]
        create: bool,
        /// What the new workspace is for, one line. A workspace with no vision is never
        /// proposed as a handoff target.
        #[arg(long)]
        vision: Option<String>,
        /// The persona the new chat runs as. Without it, this chat's own. Another persona
        /// needs a dispatch grant, which purlis asks you for the first time. To have another
        /// persona do work this chat needs an answer from, use `purlis dispatch --to <persona>`.
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
    /// `pretooluse-dispatch` the refusal of a sub-agent call named for a persona, which names
    /// the dispatch route.
    ///
    /// `posttooluse` and `-skill` keep the memory nudges, the secret warning on a written
    /// memory, and the skill log; `-dispatch` and `-message` are answered and do nothing since
    /// a persona stopped being a sub-agent. Every event word also tells the app, over its
    /// socket, what the chat is doing.
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
        /// The retired plugin puts it on every one of its hook commands (`purlis hook
        /// sessionstart --plugin-version 0.62.1`); refusing the flag would mean refusing every
        /// call such a plugin still makes. purlis's own plugin never passes it.
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
    /// Forge this project's repos are on. Default: read from the origin of the repo the
    /// project is made for (`--adopt`, `--clone-this-repo`, `--plane-is-this-repo`) when it is
    /// on github.com or gitlab.com; otherwise `init` asks for it and writes nothing.
    #[arg(long, value_parser = purlis_core::scaffold::FORGES)]
    forge: Option<String>,
    /// Group/org/user that owns the repos (default: read from the same origin).
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
    /// personas/, inventory/, workspaces/ and purlis's rules into that repo's own tracked
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
fn say_lines(said: &[purlis_core::scaffold::Say]) {
    use purlis_core::scaffold::Say;
    for line in said {
        match line {
            Say::Info(text) => voice::info(text),
            Say::Ok(text) => voice::ok(text),
            Say::Warn(text) => voice::warn(text),
            Say::Err(text) => voice::err(text),
        }
    }
}

/// What `purlis init` and `reinit` said, and the status they chose.
fn say(outcome: &purlis_core::scaffold::Outcome) -> ExitCode {
    say_lines(&outcome.said);
    ExitCode::from(outcome.code)
}

/// `purlis stop --all`: throws the kill switch the app's title bar throws (OV-1, ADR 0071).
///
/// It needs no plane and no app. The app acts on the switch within seconds when it is running,
/// and a launch that finds it thrown starts nothing, so the stop holds either way. A stop that
/// could not be written is a failure, said as one: no app would hear it.
fn stop_every_agent() -> ExitCode {
    use purlis_core::halt;
    let Some(config) = purlis_core::machine::config_root() else {
        voice::err("Agents were NOT stopped: there is no config home to keep the stop in.");
        return ExitCode::FAILURE;
    };
    match halt::stop(&config, halt::Actor::Cli, halt::now()) {
        Ok(()) => {
            voice::ok(
                "Stopping every chat and shell purlis started, in every project and window: \
                 the app ends them within seconds.",
            );
            voice::info(
                "Nothing starts again until you re-arm it from the purlis app's title bar.",
            );
            ExitCode::SUCCESS
        }
        Err(not_kept) if not_kept.journaled => {
            voice::warn(&format!(
                "The stop is kept, but not as it should be: {not_kept}. A running app still \
                 hears it; re-arm from its title bar."
            ));
            ExitCode::FAILURE
        }
        Err(not_kept) => {
            voice::err(&format!("Agents were NOT stopped: {not_kept}."));
            voice::info("Use Stop all on the purlis app's title bar instead.");
            ExitCode::FAILURE
        }
    }
}

/// stdout first, then stderr, then the status — the order the two streams are written in
/// matters only for a terminal, and this is the one purlis writes in.
fn emit(report: &purlis_core::news::Report) -> ExitCode {
    use std::io::Write;
    print!("{}", report.out);
    let _ = std::io::stdout().flush();
    say_lines(&report.said);
    ExitCode::from(report.code)
}

/// What a command may do on a project that is read-only to this purlis (FR-24).
enum ReadOnly {
    /// It reports on the project or moves purlis itself forward, and says what is wrong itself.
    Runs,
    /// It only reads, so it runs, after one line saying the project is read-only (V5: a
    /// read-only project "opens read-only: it reads").
    RunsAndSays,
    /// It could write the project.
    Refused,
}

/// An explicit list of the commands that run on a read-only project. Everything else is
/// refused, so a new command is refused there until somebody shows it only reads.
fn read_only_standing(command: &Command) -> ReadOnly {
    match command {
        Command::Doctor { fix: None, .. }
        // It moves this machine's folders, and asks each project whether it may move its
        // state itself (`renamelocal`), so a read-only project here refuses nothing else.
        | Command::Migrate { .. }
        | Command::Update { .. }
        | Command::Version { .. }
        | Command::News(_)
        | Command::Root => ReadOnly::Runs,
        // Each one is here because a test runs it on a lived-in read-only project, sees it
        // print what it read, and finds every file unchanged
        // (`tests/a_project_charter_cannot_write_is_read_only.rs`, #833).
        Command::Status { .. }
        | Command::Recall(_)
        | Command::Statusline { .. }
        | Command::Workspace(
            WorkspaceCommand::List | WorkspaceCommand::Current | WorkspaceCommand::Recall { .. },
        )
        | Command::Persona(memory::PersonaCommand::List)
        | Command::Change(change::ChangeCommand::List { .. })
        | Command::Session(session::SessionCommand::List { .. })
        | Command::Harness(HarnessCommand::List | HarnessCommand::Show { .. })
        | Command::Guard {
            verb: None | Some(GuardCommand::List),
        } => ReadOnly::RunsAndSays,
        _ => ReadOnly::Refused,
    }
}

/// Why the project this command stands in is read-only to this purlis, or `None` (FR-24).
fn read_only_project() -> Option<String> {
    let place = place().ok()?;
    if !place.is_plane {
        return None;
    }
    match purlis_core::compat::read(&place.root) {
        purlis_core::compat::Compat::Writable => None,
        purlis_core::compat::Compat::ReadOnly(why) => Some(why.to_string()),
    }
}

/// Where `init` and `reinit` act: `charter/root.py:find_root_or_cwd`.
fn place() -> Result<purlis_core::plane::Place, String> {
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    Ok(purlis_core::plane::place(&cwd))
}

#[derive(Subcommand)]
enum BrowserCommand {
    /// Generate Playwright's driving-surface skill into this plane's .claude/skills/, from the
    /// tool that owns it (purlis vendors none of it — Apache-2.0, and it ships far more
    /// often than purlis does).
    Install {
        /// @playwright/cli version, exactly (default: the one purlis is known to work with).
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
    /// Retired: a handoff's consent is the dispatch grant, so this writes nothing and says
    /// what to run to have your harness ask as well.
    Handoff,
    /// Always prompt before `charter report … --yes` files an issue: the rule `purlis init`
    /// writes, put back (ADR 0059).
    Report,
    /// Show this plane's ask and allow rules, by the file each lives in.
    List,
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Register purlis's plugin with each harness on this machine, so a chat started in a
    /// terminal runs purlis's hooks and guard. Prints each change; running it again changes
    /// nothing that is already so.
    Install {
        /// Only this harness (`claude`, `codex` or `opencode`); repeat for more. Default: each
        /// one whose config folder exists.
        #[arg(long, value_parser = purlis_core::plugin_install::HARNESSES)]
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
        #[arg(long, value_parser = purlis_core::plugin_install::HARNESSES)]
        harness: Vec<String>,
        /// Print what would change, and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum HarnessCommand {
    /// Every profile purlis read, the file it came from, and why any was refused.
    List,
    /// A harness's declaration as purlis read it: a built-in, or the project's
    /// `harnesses/<name>.toml` (ADR 0073).
    Show {
        /// The harness's declared name, as a profile's `kind` names it.
        name: String,
    },
}

/// The two `version` verbs that move a PUBLISHED `charter-cp` release.
///
/// Registered rather than left to clap so that each gets a sentence instead of a usage error
/// — M2.21's rule, applied to the verbs beside it. Their flags are declared as purlis
/// declares them, because a script that passes `--cli` or `--push` must meet the refusal
/// rather than the parser.
#[derive(Subcommand)]
enum VersionCommand {
    /// Move THIS plane to the version it pins.
    Sync {
        /// Conform the machine-global `purlis` binary instead.
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
    /// List purlis's own documentation topics.
    List,
    /// Print one of purlis's own documentation pages — served by the install that
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
    /// It takes no `-w`, because purlis's own `workspace current` takes none: the top rung
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
        /// Memory slug or filename (see `purlis workspace recall`).
        slug: String,
        #[command(flatten)]
        common: Common,
    },
    /// Rewrite one workspace memory in place: same filename, same stamp, index line
    /// retitled. What is not given is kept.
    Edit {
        /// Memory slug or filename (see `purlis workspace recall`).
        slug: String,
        /// The new body; `-` reads it from standard input (default: keep the body).
        #[arg(required_unless_present = "title")]
        text: Option<String>,
        /// The new title (default: keep the title).
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Move one workspace memory into memory/archive/ and drop its index line (undo:
    /// unarchive).
    Archive {
        /// Memory slug or filename (see `purlis workspace recall`).
        slug: String,
        #[command(flatten)]
        common: Common,
    },
    /// Move one workspace memory to another scope: another workspace's journal, a persona,
    /// or shared. Its title and stamp go with it, and nothing is copied. Persona and shared
    /// memory are published with the project.
    #[command(name = "move-memory")]
    MoveMemory {
        /// Memory slug or filename (see `purlis workspace recall`).
        slug: String,
        #[command(flatten)]
        to: memory::MoveTo,
        #[command(flatten)]
        common: Common,
    },
    /// Move an archived workspace memory back into the journal and re-index it.
    Unarchive {
        /// The memory's slug or filename in memory/archive/.
        slug: String,
        /// Restore it under this filename instead (for one archiving had to number).
        #[arg(long = "as", value_name = "SLUG")]
        restore_as: Option<String>,
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
    /// Create a workspace: its directory, its baseline files and purlis's harness layer.
    Create {
        name: String,
        /// Repos to clone into it immediately, by inventory name. POSITIONAL, as purlis's
        /// own `workspace create <name> [repos...]` takes them — a `--repos` of this port's
        /// own would be a command line that works against one purlis and not the other.
        repos: Vec<String>,
        /// What this workspace is for, recorded in its `workspace.md` purlis.
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
    /// away, which since M2.26 is the whole restore: `purlis clone` per missing repo, then
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
    /// Bring a workspace's structure and purlis's layer up to what this version writes.
    Reinit {
        /// The workspace (default: the active one).
        name: Option<String>,
        /// Every workspace this plane has. Given with a name, this wins and the name is
        /// ignored — purlis's own parser refuses neither, and a port that refused one would
        /// be a command line that works against one purlis and not the other.
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
    /// Record a todo, list them, close one with `done <slug>`, or promote one to an issue with
    /// `promote <slug> --repo <repo>`.
    ///
    /// `done`/`forget`/`promote` are read as verbs rather than as todo text, and told apart by
    /// the shape of the call rather than the word: one positional records, two act on a todo.
    /// purlis's own parser does exactly this, and a real subcommand cannot.
    Todo {
        #[arg(num_args = 0..=2)]
        words: Vec<String>,
        /// With `promote`: the workspace's repo to open the issue in, by its inventory name.
        /// May be left out when the workspace has one repo on a forge.
        #[arg(long)]
        repo: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Internal, answered and ignored: the Python plugin's SessionStart seed of a session's
    /// workspace pointer from its terminal pane. A chat charter-app starts has no pane to seed
    /// from — its workspace is the pointer keyed on the chat, which `purlis ws use` writes.
    #[command(name = "_reconcile", hide = true)]
    Reconcile,
    /// Internal, answered and ignored: the Python plugin's turn-end commit of a LIVE
    /// workspace's memory. It commits nothing under the default `share = "local"`; under
    /// `commit`/`push` charter-app leaves the commit to `purlis save`, which commits the plane
    /// as one decision instead of racing the operator's own git on every turn end.
    #[command(name = "_autosave", hide = true)]
    Autosave,
}

#[derive(Args)]
struct Common {
    /// The workspace to act on (default: the active one).
    #[arg(short = 'w', long = "workspace")]
    workspace: Option<String>,
    /// Pin the clock a write stamps itself with, for tests only. purlis's own
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

/// Says `why` as purlis's refusal and fails — for a command answered outside [`run`], whose
/// `Err` is printed the same way.
fn refused(why: &str) -> ExitCode {
    eprintln!("purlis: {why}");
    ExitCode::FAILURE
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
    ids: purlis_core::active::Ids,
    workspace_env: Option<String>,
    /// `$CHARTER_PLANE_ROOT_SESSION`: the app started this chat at the plane root (SI-1).
    plane_root_env: Option<String>,
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
    purlis_core::plane::resolve(&cwd)
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
            ids: purlis_core::active::Ids::from_env(),
            workspace_env: purlis_core::envvar::var(purlis_core::active::WORKSPACE_ENV),
            plane_root_env: purlis_core::envvar::var(purlis_core::active::PLANE_ROOT_ENV),
            persona_env: purlis_core::envvar::var(purlis_core::active::PERSONA_ENV),
        })
    }

    /// The whole ladder, with `flag` on top of it.
    fn asking<'a>(
        &'a self,
        flag: Option<&'a str>,
        env: Option<&'a str>,
    ) -> purlis_core::active::Asking<'a> {
        purlis_core::active::Asking {
            root: self.plane.root(),
            cwd: &self.cwd,
            flag,
            ids: &self.ids,
            env,
        }
    }

    /// Whether this invocation is at the plane root, and why — `None` when `flag`, the
    /// environment, the tree or a pointer puts it in a workspace
    /// ([`purlis_core::active::plane_root`]).
    fn plane_root(&self, flag: Option<&str>) -> Option<purlis_core::active::PlaneRoot> {
        purlis_core::active::plane_root(
            &self.asking(flag, self.workspace_env.as_deref()),
            self.plane_root_env.as_deref(),
        )
    }

    /// Whether the app started this chat at the plane root and nothing names a workspace
    /// instead — the one kind of root session `workspace use` does not move.
    fn launched_at_plane_root(&self) -> bool {
        self.plane_root(None) == Some(purlis_core::active::PlaneRoot::Launched)
    }

    /// The workspace this invocation acts on — or, at the plane root, the sentence saying to
    /// name one with `-w` (SI-1). The plane root is a chat the app started there, or a session
    /// standing anywhere in the plane outside every workspace that has chosen none (SI-1b).
    /// Everywhere else there is always one: the ladder ends on `[workspace] default`, and
    /// under that on the literal `default`.
    pub fn active_workspace(&self, flag: Option<&str>) -> Result<String, String> {
        if let Some(by) = self.plane_root(flag) {
            return Err(purlis_core::active::plane_root_refusal(
                self.plane.root(),
                by,
            ));
        }
        Ok(purlis_core::active::workspace(&self.asking(flag, self.workspace_env.as_deref())).name)
    }

    /// Where this invocation works: the plane root, or the workspace it acts on — for a
    /// command that records where something came from (`handoff`'s stamp and todo).
    pub fn place(&self, flag: Option<&str>) -> purlis_core::active::Place {
        match self.active_workspace(flag) {
            Ok(ws) => purlis_core::active::Place::Workspace(ws),
            Err(_) => purlis_core::active::Place::PlaneRoot,
        }
    }

    /// The workspace this invocation is in, or `None` in a plane-root chat — for a command
    /// that reads a workspace when there is one and runs without one (`recall`, an extension's
    /// project choices, the background refresh).
    pub fn workspace_if_any(&self, flag: Option<&str>) -> Option<String> {
        self.active_workspace(flag).ok()
    }

    /// The persona this invocation acts as, or `None` — a plane may have no front door, and
    /// purlis inventing one would be it choosing an identity nobody asked for.
    pub fn active_persona(&self, flag: Option<&str>) -> Option<String> {
        purlis_core::active::persona(&self.asking(flag, self.persona_env.as_deref())).name
    }

    /// The workspace this command acts on, refusing a name that cannot be one.
    ///
    /// The refusal names the workspace RESOLVED, not the flag: with `-w` absent the operator
    /// never typed a name, and quoting an empty one would describe nothing.
    fn workspace(&self, flag: Option<&str>) -> Result<purlis_core::workspaces::Workspace, String> {
        self.plane
            .workspace(&self.active_workspace(flag)?)
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
///   `purlis hook pretooluse-notebook` exited 1, which a harness logs and ignores, so the day
///   purlis adds a matcher the Rust binary on PATH would allow that tool class silently.
/// - Blocking every unknown word instead fails the other way: `purlis hook stopp`, a typo in
///   a settings file purlis itself wrote, blocked the session from ENDING — which is the
///   exact hazard this whole file was written around.
///
/// The word says which hook it is. In the `pretooluse`/`posttooluse` namespace there is a tool
/// call to protect and blocking is the safe answer, for every matcher purlis has and every
/// one it adds. Outside it there is nothing to protect and blocking can only wedge a session.
///
/// **Every word purlis has today is answered before this is asked** — the registry
/// ([`purlis_core::hookreg::HANDLERS`]) and its no-ops. What is left here is only the word
/// nobody has invented yet, and the rule for it is unchanged because its argument is: a
/// program that has checked nothing may not say `allow`.
fn is_a_tool_hook(name: &str) -> bool {
    is_a_pretooluse_hook(name) || name.starts_with("posttooluse")
}

/// Whether the word is a `PreToolUse` hook: one that decides whether a tool call runs, so a
/// crash in it must refuse the call ([`guard::refuse_on_a_crash`]). The namespace, as
/// [`is_a_tool_hook`] reads it, and for its reason — a matcher purlis adds tomorrow is covered
/// today. `posttooluse` is not: the tool has already run, and there is nothing left to refuse.
fn is_a_pretooluse_hook(name: &str) -> bool {
    name.starts_with("pretooluse")
}

/// Whether this command line is `purlis hook <a PreToolUse word>`, read off the raw argv
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

/// Set in a debug build, `purlis` panics before it has read its command line (#349): the
/// test suite's way to watch what a crash answers. Compiled out of a release build.
#[cfg(debug_assertions)]
const PANIC_ON_PURPOSE_ENV: &str = "CHARTER_TEST_HOOK_PANICS";

/// Set in a debug build, `purlis-core` raises one warning before the command line is read: the
/// test suite's way to watch where a core diagnostic goes under the CLI (#647). Compiled out of
/// a release build.
#[cfg(debug_assertions)]
const CORE_WARNS_ON_PURPOSE_ENV: &str = "CHARTER_TEST_CORE_WARNS";

/// How long the payload on stdin is waited for.
///
/// **Two seconds, and it used to be 25 milliseconds.** The spec allows the whole call 50 ms
/// and this binary was measured at 1.8, so 25 looked generous — but a review measured an
/// 8 MB `UserPromptSubmit` (a pasted log) at 47 ms, and a payload written in two pieces
/// missed 25 ms every time. Reaching the deadline is not free: purlis then cannot establish
/// which conversation the report is of, and before a chat has adopted a process that costs
/// the whole report.
///
/// This exists only against a harness that opens the hook's stdin and never writes, which
/// would otherwise hang the turn for good. Two seconds is well under the plugin's own 5 s
/// hook timeout, so the harness's deadline is still the one that fires first, and no ordinary
/// payload can reach this one.
///
/// A reporting hook only. A `PreToolUse` hook reads its payload inside its budget, and one that
/// does not arrive in time is a refusal, never an empty payload (#928).
const PAYLOAD_DEADLINE: Duration = Duration::from_secs(2);

/// Reads the harness's payload, or gives up on it as empty.
fn payload() -> String {
    payload_within(PAYLOAD_DEADLINE).unwrap_or_default()
}

/// Reads the harness's payload, or `None` when it has not all arrived within `limit`.
///
/// On its own thread, because a read from a pipe nobody is writing to cannot be interrupted.
/// The thread is left behind when the deadline passes: the process is about to exit, and
/// waiting for it is the very thing being avoided.
fn payload_within(limit: Duration) -> Option<String> {
    use std::io::Read;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::stdin().read_to_string(&mut text);
        let _ = tx.send(text);
    });
    rx.recv_timeout(limit).ok()
}

/// The rule a refused tool hook word this binary does not answer is recorded under.
const UNKNOWN_HOOK_RULE: &str = "unknown-hook";

/// The rule a tool call refused because its guard crashed is recorded under.
pub(crate) const GUARD_CRASHED_RULE: &str = "guard-crashed";

/// The rule a tool call refused because its guard did not answer in time is recorded under.
const GUARD_UNANSWERED_RULE: &str = "guard-unanswered";

/// One [`hookwire::ToolCall`] to the host, when a host is listening: the tool, the hash of its
/// arguments, the decision and how long the hook took. Never the arguments themselves.
///
/// **Before the harness is answered** (FD-30): the host says it has recorded the line, or the
/// line is in the chat's spool, durably, when this returns ([`hookwire::deliver_tool`]). A
/// line that reached neither is said only on stderr, as a report that did not arrive is, and
/// the answer is the same either way.
#[cfg(unix)]
fn tell_the_host_about_the_tool_call(
    word: &str,
    payload: &str,
    answered: &hooks::Answered,
    took: std::time::Duration,
) {
    use std::io::Write as _;
    let Some(socket) = purlis_core::envvar::var_os(SOCKET_ENV) else {
        return;
    };
    let Some(chat) =
        purlis_core::envvar::var(hookwire::CHAT_ENV).and_then(|chat| chat.parse().ok())
    else {
        // Never `eprintln!` here: this runs from a crashed guard's panic hook too, and a
        // print to a closed stderr panics there, which aborts instead of refusing (exit 2).
        let _ = writeln!(
            std::io::stderr(),
            "purlis: this {word} names no chat (`${}` is not a chat number), so the app was \
             not told",
            hookwire::CHAT_ENV
        );
        return;
    };
    let data: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
    let text = |field: &str| data[field].as_str().map(str::to_owned);
    let call = hookwire::ToolCall {
        chat,
        tool_hook: word.to_owned(),
        tool: text("tool_name"),
        call: text("tool_use_id"),
        args: data.get("tool_input").map(purlis_core::eventlog::args_hash),
        decision: answered.decision,
        rule: answered.rule.clone(),
        hook_ms: u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
        agent: hookwire::sub_agent(data["agent_id"].as_str(), &purlis_core::envvar::var),
        at_ms: std::time::SystemTime::now()
            .checked_sub(took)
            .and_then(|began| began.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|since| u64::try_from(since.as_millis()).ok())
            .unwrap_or_default(),
    };
    let token = hookwire::ChatToken::from_env();
    if let Err(why) = hookwire::deliver_tool(std::path::Path::new(&socket), token.as_ref(), &call) {
        let _ = writeln!(std::io::stderr(), "{} {word} ({why})", hookwire::NOT_TAKEN);
    }
    // The file a file tool touched, for the tree's live marker (FM-6): on a line of its own,
    // sent once and never spooled, so no record holds it (D-86a). Not for a call the guard
    // refused, which touched nothing. A marker the app missed is not worth a word on stderr.
    if call.decision != hookwire::Decision::Deny
        && let Some(touching) = purlis_core::touching::of_tool(&data)
    {
        let _ = hookwire::touch(
            std::path::Path::new(&socket),
            token.as_ref(),
            &hookwire::Touching { chat, touching },
        );
    }
    // What the chat is doing, for the one line under its name (#1493): a kind from a fixed list
    // and at most one short name, on a line of its own, sent once and never spooled, so no
    // record holds it. Not for a call the guard refused, which did not run.
    if call.decision != hookwire::Decision::Deny
        && let Some(doing) = purlis_core::doing::of_hook(word, &data)
    {
        let _ = hookwire::tell_doing(
            std::path::Path::new(&socket),
            token.as_ref(),
            &hookwire::Doing {
                chat,
                doing,
                agent: call.agent.clone(),
            },
        );
    }
}

/// Each sandbox block a block word found in the call's result, to the host, when one is
/// listening (#1338): an operation and a kind, and whether it was purlis's own — never the path,
/// the command or what it printed. Sent once each and never spooled
/// ([`hookwire::tell_blocked`]); a block the app did not take is said only on stderr.
#[cfg(unix)]
fn tell_the_host_about_blocks(word: &str, payload: &str) {
    use std::io::Write as _;
    // The word first: every other tool hook passes through here, and none of them is read.
    if !hooks::is_a_block_word(word) {
        return;
    }
    let Some(socket) = purlis_core::envvar::var_os(SOCKET_ENV) else {
        return;
    };
    let Some(chat) =
        purlis_core::envvar::var(hookwire::CHAT_ENV).and_then(|chat| chat.parse().ok())
    else {
        return;
    };
    let data: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
    let token = hookwire::ChatToken::from_env();
    let harness = purlis_core::envvar::var(hookwire::HARNESS_ENV).filter(|it| !it.is_empty());
    for (block, target) in hooks::blocks(&data, &purlis_core::envvar::var) {
        let blocked = hookwire::SandboxBlocked {
            chat,
            sandbox_blocked: block,
            harness: harness.clone(),
            target,
        };
        if let Err(why) =
            hookwire::tell_blocked(std::path::Path::new(&socket), token.as_ref(), &blocked)
        {
            let _ = writeln!(std::io::stderr(), "{} {word} ({why})", hookwire::NOT_TAKEN);
        }
    }
}

#[cfg(not(unix))]
fn tell_the_host_about_blocks(_word: &str, _payload: &str) {}

/// What a crashed `PreToolUse` guard, or one that did not answer in time, tells the host from its
/// panic hook: the call was refused, and why. The payload is not read again in a process that is going down.
fn tell_the_host_about_a_crash(word: &str, unanswered: bool) {
    let rule = if unanswered {
        GUARD_UNANSWERED_RULE
    } else {
        GUARD_CRASHED_RULE
    };
    let answered =
        hooks::Answered::exit(ExitCode::from(BLOCK), hookwire::Decision::Deny, Some(rule));
    tell_the_host_about_the_tool_call(word, "", &answered, Duration::ZERO);
}

#[cfg(not(unix))]
fn tell_the_host_about_the_tool_call(
    _word: &str,
    _payload: &str,
    _answered: &hooks::Answered,
    _took: std::time::Duration,
) {
}

/// The rule a `PreToolUse` call refused for a disagreeing variable is recorded under.
const DISAGREEING_NAMES_RULE: &str = "env-names-disagree";

/// A hook run where a variable that chooses the project disagrees with its old name
/// (D-RN2d-8): it acts on no project, and says why on standard error.
///
/// - **A `PreToolUse` hook blocks** (exit 2, the sentence as the reason the harness shows): the
///   guard would otherwise judge the call against a project it guessed, and a refused write is
///   the safe side. The host hears the refusal, as it hears every tool call.
/// - **Every other hook stands aside** (exit 0, nothing on standard output): it only reports,
///   briefs or records, and exiting 2 from one would leave a session unable to end.
fn hook_on_disagreement(name: &str, disagree: &purlis_core::envvar::Disagreement) -> ExitCode {
    eprintln!("purlis: {disagree}");
    if !is_a_pretooluse_hook(name) {
        return ExitCode::SUCCESS;
    }
    let answered = hooks::Answered::exit(
        ExitCode::from(BLOCK),
        hookwire::Decision::Deny,
        Some(DISAGREEING_NAMES_RULE),
    );
    tell_the_host_about_the_tool_call(name, &payload(), &answered, Duration::ZERO);
    answered.print()
}

/// `purlis hook <name>` — always succeeds, whatever went wrong.
///
/// **Never exit 2, except where the whole point is to.** A harness reads 2 as "block": on
/// `Stop` it makes the harness carry on rather than end. Nothing purlis draws is worth that,
/// so every failure on a REPORTING hook is a silent 0 and the state the app draws is simply the
/// last one it was told. The three exceptions are all about a tool call: a denial a guard
/// decided and could not print ([`guard::deny`]), a word in the tool-hook namespace this
/// binary does not answer at all ([`is_a_tool_hook`]), and a `PreToolUse` hook that crashed
/// ([`guard::refuse_on_a_crash`]).
fn hook(name: &str, now: Option<&str>) -> ExitCode {
    // The harness started this process, not the chat: a refusal it meets is the chat's
    // sandbox's only where the harness's sandbox holds its hooks (#1421).
    purlis_core::sandbox::started_by_the_harness();
    // FIRST, in front of `Event::parse`, because none of these is one of the app's reporting
    // events: a tool call carries no chat state worth a `Report`.
    //
    // **The host hears every tool call before the harness is answered** (FD-9, FD-30): the
    // verdict is decided, then one line on the hook channel says what it is, and only once the
    // host has recorded it, or the line is in the chat's spool, is the verdict printed. The
    // exit status is the verdict's whatever the send does, and the send gives up on the host
    // after a bounded wait (`hookwire::deliver_tool`), so a slow or frozen app costs a guard's
    // answer that wait at most, never the answer.
    // The harness's permission prompt (HP-6): held open until the operator answers it in the
    // window, or the harness's own prompt decides. Not a tool hook: it decides nothing itself.
    if name == purlis_core::harness::hooked::WORD {
        return permission::permissionrequest(&payload());
    }
    let guarded = name == GUARDED_TOOL_HOOK;
    let handler = hooks::handler(name);
    if guarded || handler.is_some() {
        let began = std::time::Instant::now();
        let decide = |text: &str| match handler {
            Some(handler) if !guarded => hooks::run(handler, text, now),
            _ => guard::pretooluse(text, now),
        };
        // A `PreToolUse` hook decides inside one budget, payload and all, on a deep stack, and
        // a decision not made in time is a refusal (#1355, #928). The budget is what keeps the
        // harness's own timeout, which may run the tool, from ever being reached.
        let (text, answered) = if is_a_pretooluse_hook(name) {
            let budget = guard::Budget::start();
            let Some(text) = payload_within(budget.left()) else {
                guard::out_of_time()
            };
            let answered = guard::decided_within(&budget, || decide(&text));
            (text, answered)
        } else {
            let text = payload();
            let answered = decide(&text);
            (text, answered)
        };
        tell_the_host_about_the_tool_call(name, &text, &answered, began.elapsed());
        tell_the_host_about_blocks(name, &text);
        return answered.print();
    }
    if purlis_core::hookreg::NO_OPS.contains(&name) {
        // Answered with nothing, as ever, and still one call the host hears. The payload is
        // read only when a host is listening, so a terminal's own harness pays nothing for it.
        let began = std::time::Instant::now();
        if purlis_core::envvar::var_os(SOCKET_ENV).is_some() {
            let text = payload();
            let answered = hooks::Answered::exit(ExitCode::SUCCESS, hookwire::Decision::None, None);
            tell_the_host_about_the_tool_call(name, &text, &answered, began.elapsed());
        }
        return ExitCode::SUCCESS;
    }
    let Some(event) = Event::parse(name) else {
        let tool = is_a_tool_hook(name);
        // The word comes out of a settings file a chat can write, and this sentence goes to
        // a terminal and into the harness's own log. Contained like every other value
        // charter quotes back.
        let name = purlis_core::shown::readable(name, purlis_core::shown::DISPLAY_LIMIT);
        eprintln!(
            "purlis: `{name}` is not a hook this binary answers (`purlis hook --list` names \
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
        if !tool {
            return ExitCode::FAILURE;
        }
        // A refused tool call is still one the host hears.
        if purlis_core::envvar::var_os(SOCKET_ENV).is_some() {
            let answered = hooks::Answered::exit(
                ExitCode::from(BLOCK),
                hookwire::Decision::Deny,
                Some(UNKNOWN_HOOK_RULE),
            );
            tell_the_host_about_the_tool_call(name.as_ref(), &payload(), &answered, Duration::ZERO);
        }
        return ExitCode::from(BLOCK);
    };
    let socket = purlis_core::envvar::var_os(SOCKET_ENV);
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
    let report = Report::read(event, &text, &purlis_core::envvar::var);
    if report.is_none() {
        // A host is listening and this hook names no chat it started: `$CHARTER_CHAT` is
        // missing or not a number. There is no chat to put the event under, so none is sent,
        // and that is said rather than left for nobody to notice.
        eprintln!(
            "purlis: this {} names no chat (`${}` is not a chat number), so the app was not told",
            event.word(),
            hookwire::CHAT_ENV
        );
    }
    if let Some(report) = &report
        && let Err(why) = hookwire::deliver_report(
            std::path::Path::new(&socket),
            hookwire::ChatToken::from_env().as_ref(),
            report,
        )
    {
        // The app may have quit while this session was still running, which is the ordinary
        // way for this to fail and is not the harness's business — hence the exit 0 below.
        //
        // But it is said, because a report that never arrives is otherwise invisible
        // everywhere: the chat simply stops changing, and there is nothing anywhere to look
        // at. A zero-exit hook's stderr goes to the harness's debug log, which costs the
        // operator nothing and is exactly where somebody debugging this would look.
        eprintln!("{} {} ({why})", hookwire::NOT_TAKEN, event.word());
    }
    // After the report, so the app has heard the turn end before it hears the record: a
    // saved-record line the harness's sandbox kept from reaching the app (#517).
    if event == Event::Stop
        && let Some(report) = &report
    {
        session::pass_on_at_stop(std::path::Path::new(&socket), report);
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
/// - **Inside the app** `purlis statusline` draws nothing and returns early (ADR 0019), and
///   the app has its own trigger already — `panels::repo_states`, focusing a workspace.
/// - **Outside it**, `statusLine` is armed only for chats the app starts and only where the
///   operator fills that line with nothing (`harness::claude_code_status_line`,
///   `footerclaim`). A plane with no app open has no `purlis statusline` running at all —
///   purlis #895 deleted the one that used to be wired — while the plugin's hooks DO run.
///
/// Which leaves this: the hook is the only thing purlis runs on a plane nobody has an app
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
/// and never a wait. Measured on this machine over 300 runs each, `purlis hook sessionstart`
/// against a plane whose cache is fresh: **3.68 ms before, 3.66 ms after**, against a
/// `purlis --version` floor of 3.63 ms — the added work does not clear the noise of starting
/// the process at all.
///
/// # Best-effort, and silent about it
///
/// Nothing here is worth a word on a reporting hook: no plane, no readable workspace, no
/// `current_exe`, a fork that failed — each simply means no refresh, and the column already
/// says "nothing has fetched this checkout" rather than reading as green (#69). Python's own
/// is a bare `except Exception: return` for the same reason.
fn refresh_the_forge_cache() {
    use purlis_core::{glrefresh, glstate};

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
    // A plane-root chat is in no workspace, so there are no clones of one to refresh.
    let Some(workspace) = here.workspace_if_any(None) else {
        return;
    };
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

/// `purlis gl-refresh` — ask each clone's own forge about the branch it is on, and write the
/// answers into the cache the panels read.
///
/// A port of `charter/commands.py:cmd_gl_refresh`. The work itself is
/// [`purlis_core::glrefresh`], which is where the credential boundary is argued.
fn gl_refresh(ws: &str, detach: bool, now: Option<&str>) -> ExitCode {
    use purlis_core::glrefresh;

    // Checked FIRST: the point is to return before any of the work below, in a process the
    // harness will not tear down with the turn.
    if detach {
        return match detach_self(ws, now) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("purlis: {why}");
                ExitCode::FAILURE
            }
        };
    }
    let root = match plane() {
        Ok(plane) => plane.root().to_path_buf(),
        Err(why) => {
            eprintln!("purlis: {why}");
            return ExitCode::FAILURE;
        }
    };
    let stamp = match instant(now) {
        Ok(stamp) => stamp,
        Err(why) => {
            eprintln!("purlis: {why}");
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
            eprintln!("purlis: {why}");
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
/// ([`purlis_core::plane::resolve`]): on the plane the vault, the personas and the memory
/// belong to, out of a linked worktree and outward through an enclosing plane's `workspaces/`.
/// `save`'s two refusals — you are standing in a worktree, you are standing in a nested plane
/// — only exist once that is the resolution, because they are about the caller standing
/// somewhere other than the tree being committed.
///
/// **There is no gap left to describe, and there were two.** M2.9 gave `plane::find_root` the
/// outward hop, so the read commands stopped acting on a clone's own plane; M2.16 gave it the
/// worktree redirect and deleted the second resolver these two used to ask, because a step
/// only `save` takes is a step on which `purlis save` and the command beside it name
/// different planes in the same directory. Python resolves once, through
/// `charter/root.py:find_root`, for every command it has.
///
/// What is still special here is only WHEN the plane is resolved: before the ladder, because
/// these two want the plane and never the workspace or the persona.
fn plane_command(command: &Command) -> Option<ExitCode> {
    use purlis_core::repocmd::Say;

    let mut say = |line: Say| eprintln!("{line}");
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("purlis: cannot read the current directory: {e}");
            return Some(ExitCode::FAILURE);
        }
    };
    let root = match command {
        Command::Save { .. } | Command::GitPolicy { .. } => {
            match purlis_core::plane::resolve(&cwd) {
                Ok(root) => root,
                Err(why) => {
                    eprintln!("purlis: {why}");
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
            if *pull && purlis_core::planegit::pull(&root, &mut say) != 0 {
                1
            } else {
                // A chat's `charter save` is an agent run's, and carries its trailers (GL-8).
                let provenance =
                    purlis_core::provenance::Provenance::in_chat(&root, &purlis_core::envvar::var);
                purlis_core::planegit::save(
                    &purlis_core::planegit::Request {
                        root: &root,
                        message: message.as_deref(),
                        sign: *sign,
                        no_push: *no_push,
                        cwd: &cwd,
                        provenance: provenance.as_ref(),
                    },
                    &mut say,
                )
            }
        }
        Command::GitPolicy { apply } => purlis_core::gitpolicy::policy(&root, *apply, &mut say),
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
/// command describes purlis and the other describes your repos; they share a noun and
/// nothing else. `charter/cli.py` hangs all three off one parser and routes them to three
/// functions, of which only `cmd_docs` reads `config.ROOT` — so `purlis docs list` answers
/// outside a plane, and a Rust binary that resolved a plane first would refuse there.
///
/// **M2.21, and the alternative was an honest refusal.** These two were clap usage errors
/// (exit 2, the parser's own wording) for verbs the tool being replaced has: a Makefile
/// calling `purlis docs list` with a Rust `purlis` first on `$PATH` met one. The port costs
/// a vendored directory and a lookup, because `news` had already built the road — so it is
/// the port, not a sentence apologising for the gap.
fn docs_command(command: &Command) -> Option<ExitCode> {
    use purlis_core::docsrc;

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
    use purlis_core::repocmd::{self, Say};

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
                eprintln!("purlis: {why}");
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
            let on_a_test_clock = now.is_some();
            let now = match now {
                Some(text) => match text.parse::<chrono::NaiveDateTime>() {
                    // A naive stamp is LOCAL time, as `--now` is everywhere in this binary.
                    Ok(naive) => {
                        match chrono::TimeZone::from_local_datetime(&chrono::Local, &naive).single()
                        {
                            Some(local) => local.with_timezone(&chrono::Utc),
                            None => {
                                eprintln!("purlis: --now names no single local instant");
                                return Some(ExitCode::FAILURE);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("purlis: --now is not a local naive timestamp: {e}");
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
            let ws = match here.active_workspace(workspace.as_deref()) {
                Ok(ws) => ws,
                Err(why) => return Some(refused(&why)),
            };
            // In a chat the app started, the app clones (#1335): a sandboxed chat may not
            // write the clone's `.git/config`, hooks or editor settings. `--now` is a test's
            // clock, which only this process keeps.
            let asked = if on_a_test_clock {
                None
            } else {
                gitask::told(
                    gitask::forwarded(
                        &ws,
                        purlis_core::hookwire::GitWork::Clone {
                            repos: repos.clone(),
                        },
                        gitask::a_clone_takes_at_most(repos.len()),
                    ),
                    &mut say,
                )
            };
            if let Some(code) = asked {
                return Some(ExitCode::from(code));
            }
            repocmd::clone::clone(
                &repocmd::clone::Request {
                    root: &root,
                    ws: &ws,
                    repos,
                    now,
                    author: &author,
                    hosts: None,
                },
                &mut say,
            )
        }
        Command::Sync { workspace, all } => {
            // `--all` is the only thing that replaces the ladder here, and clap already
            // refuses it beside `-w`.
            let one = match (!*all).then(|| here.active_workspace(workspace.as_deref())) {
                Some(Err(why)) => return Some(refused(&why)),
                Some(Ok(ws)) => Some(ws),
                None => None,
            };
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
            let chosen = purlis_core::active::workspace(&asking);
            let via = purlis_core::active::workspace_source(&here.ids, chosen.rung);
            // A plane-root chat is in no workspace: every workspace is reported, and none is
            // marked as the one it is in (SI-1). `(none)` can be no workspace's name.
            let at_root = here.plane_root(workspace.as_deref()).is_some();
            let (active, via) = if at_root {
                ("(none)", "the plane root".to_string())
            } else {
                (chosen.name.as_str(), via)
            };
            let mut out = |line: String| println!("{line}");
            repocmd::status::status(
                &repocmd::status::Request {
                    root: &root,
                    cwd: &here.cwd,
                    active,
                    via: &via,
                    all: *all || at_root,
                },
                &mut out,
                &mut say,
            )
        }
        // Bare `charter docs` and `charter docs generate` are the same command.
        Command::Docs {
            what: None | Some(DocsCommand::Generate),
        } => {
            let cfg = match purlis_core::forge::load_config(&root) {
                Ok(cfg) => cfg,
                Err(why) => {
                    eprintln!("{}", Say::Plain(why));
                    return Some(ExitCode::FAILURE);
                }
            };
            repocmd::docs::docs(&root, &purlis_core::forge::group_of(&cfg, 0), &mut say)
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
    use purlis_core::wscmd;

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
            eprintln!("purlis: {why}");
            return Some(ExitCode::FAILURE);
        }
    };
    let root = here.plane.root().to_path_buf();
    let mut sink = speak;
    let say: &mut dyn FnMut(purlis_core::repocmd::Say) = &mut sink;
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
            let config_root = purlis_core::machine::config_root();
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
            if here.launched_at_plane_root() {
                return Some(refused(&format!(
                    "this chat was started at the plane root and stays there, so it is not \
                     moved into '{name}'. Name the workspace on each command with -w {name}, or \
                     start a chat in it."
                )));
            }
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
            if *use_it && here.launched_at_plane_root() {
                return Some(refused(&format!(
                    "this chat was started at the plane root and stays there, so nothing was \
                     created: --use would move it into '{name}'. Create it without --use, and \
                     name it with -w {name} from here."
                )));
            }
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
                purlis_core::workspaces::Plane::open(&root)
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
            let one = match (!*all).then(|| here.active_workspace(name.as_deref())) {
                Some(Err(why)) => return Some(refused(&why)),
                Some(Ok(ws)) => Some(ws),
                None => None,
            };
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
                                eprintln!("purlis: --now names no single local instant");
                                return Some(ExitCode::FAILURE);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("purlis: --now is not a local naive timestamp: {e}");
                        return Some(ExitCode::FAILURE);
                    }
                },
                None => chrono::Utc::now(),
            };
            wscmd::snapshot::snapshot(
                &wscmd::snapshot::Request {
                    root: &root,
                    ws: &match here.active_workspace(name.as_deref()) {
                        Ok(ws) => ws,
                        Err(why) => return Some(refused(&why)),
                    },
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
            eprintln!("purlis: --now is not a local naive timestamp: {e}");
            return None;
        }
    };
    match chrono::TimeZone::from_local_datetime(&chrono::Local, &naive).single() {
        Some(local) => Some(local.with_timezone(&chrono::Utc)),
        None => {
            eprintln!("purlis: --now names no single local instant");
            None
        }
    }
}

/// Point this session back at the always-present workspace after the one it was on was
/// removed — `cmd_workspace_remove`'s closing branch.
///
/// Only when a POINTER is what named it. A `-w`, a `$CHARTER_WORKSPACE` or the tree the
/// caller is standing in are the operator's own and are not purlis's to rewrite; a pointer
/// purlis wrote is, and one naming a directory that no longer exists is how the next write
/// re-creates the workspace that was just deleted.
fn reset_active_after_removal(
    here: &Here,
    removed: &str,
    say: &mut dyn FnMut(purlis_core::repocmd::Say),
) {
    use purlis_core::active::WorkspaceRung;
    use purlis_core::repocmd::Say;

    let active = purlis_core::active::workspace(&here.asking(None, here.workspace_env.as_deref()));
    if active.name != removed
        || !matches!(
            active.rung,
            WorkspaceRung::SessionPointer | WorkspaceRung::TerminalPointer
        )
    {
        return;
    }
    let fallback = purlis_core::active::plane_default_workspace(here.plane.root());
    // `force`, because the session is locked to the workspace that just went away and that
    // lock can refuse nothing useful now.
    purlis_core::wscmd::select::set_active(here.plane.root(), &fallback, &here.ids, true);
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
        | Command::Stop { .. }
        | Command::Update { .. }
        | Command::Version { .. }
        | Command::Discover { .. }
        | Command::Clone { .. }
        | Command::Sync { .. }
        | Command::Status { .. }
        | Command::Docs { .. }
        | Command::Doctor { .. }
        | Command::Migrate { .. }
        | Command::GlRefresh { .. }
        | Command::Statusline { .. }
        | Command::Save { .. }
        | Command::Handoff { .. }
        | Command::Dispatch { .. }
        | Command::Report(_)
        | Command::ShellGuard { .. }
        | Command::Mcp
        | Command::GitHook { .. }
        | Command::Scan { .. }
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
            println!("{}", here.active_workspace(None)?);
        }
        Command::Guard { verb } => {
            use purlis_core::guardcmd::{self, Bucket};
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
                Some(GuardCommand::Handoff) => {
                    eprintln!("purlis: {}", guardcmd::HANDOFF_RETIRED);
                    return Ok(2);
                }
                Some(GuardCommand::Report) => (guardcmd::REPORT_PATTERN, Bucket::Ask, false),
            };
            let rule = match guardcmd::as_rule(pattern) {
                Ok(rule) => rule,
                Err(why) => {
                    eprintln!("purlis: {why} Example: purlis guard ask 'terraform apply *'");
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
            return Ok(purlis_core::browser::install(
                here.plane.root(),
                version.as_deref(),
                purlis_core::browser::npx_on(path.as_deref()),
                &mut purlis_core::browser::Npx,
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
        Command::Harness(HarnessCommand::Show { name }) => {
            return Ok(harness_show(here.plane.root(), &name));
        }
        Command::Workspace(WorkspaceCommand::List) => {
            let mut names = here.plane.workspaces().map_err(|e| e.to_string())?;
            // The workspace `resolve` TERMINATES on is always listable, whether or not its
            // directory is there: a plane where nobody selected anything resolves to a name
            // this listing did not contain, and the table then marked no row at all
            // (charter#745). It is `config.DEFAULT_WORKSPACE` — `[workspace] default`, and
            // only the literal `default` when the plane declares none.
            let always = purlis_core::active::plane_default_workspace(here.plane.root());
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
        Command::Change(command) => return change::run(&here, command),
        Command::Curation(command) => return curation::run(here.plane.root(), command),
        Command::Session(command) => return session::run(&here, command),
        Command::Workspace(WorkspaceCommand::Remember {
            text,
            title,
            no_sync,
            common,
        }) => {
            return memory::workspace_remember_typed(
                &here,
                common.workspace.as_deref(),
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
            return memory::workspace_remember_typed(
                &here,
                common.workspace.as_deref(),
                message.as_deref(),
                None,
                no_sync,
                common.now.as_deref(),
            );
        }
        Command::Workspace(WorkspaceCommand::Recall { query, common }) => {
            return memory::workspace_recall(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref())?,
                query.as_deref(),
            );
        }
        Command::Workspace(WorkspaceCommand::Forget { slug, common }) => {
            return memory::workspace_forget(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref())?,
                &slug,
            );
        }
        Command::Workspace(WorkspaceCommand::Edit {
            slug,
            text,
            title,
            common,
        }) => {
            let text = memory::body_arg(text)?;
            return memory::workspace_edit(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref())?,
                &slug,
                title.as_deref(),
                text.as_deref(),
            );
        }
        Command::Workspace(WorkspaceCommand::Archive { slug, common }) => {
            return memory::workspace_archive(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref())?,
                &slug,
            );
        }
        Command::Workspace(WorkspaceCommand::MoveMemory { slug, to, common }) => {
            let name = here.active_workspace(common.workspace.as_deref())?;
            return memory::move_memory(
                &here.plane,
                &purlis_core::memscope::Scope::Workspace(name),
                &slug,
                &to.scope(),
                None,
            );
        }
        Command::Workspace(WorkspaceCommand::Unarchive {
            slug,
            restore_as,
            common,
        }) => {
            return memory::workspace_unarchive(
                &here.plane,
                &here.active_workspace(common.workspace.as_deref())?,
                &slug,
                restore_as.as_deref(),
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
        Command::Workspace(WorkspaceCommand::Todo {
            words,
            repo,
            common,
        }) => {
            // In a chat the app started, a todo for the chat's own workspace is the app's to
            // write (#1333). Closing, listing and promoting are not brokered yet.
            if let [text] = words.as_slice()
                && !["done", "forget", "promote"].contains(&text.as_str())
                && repo.is_none()
                && common.workspace.is_none()
                && common.now.is_none()
            {
                let write = purlis_core::brokered::Write::Todo { text: text.clone() };
                if let Some(code) =
                    memory::said_forwarded(brokered::forwarded(write), |to, path| {
                        said_todo_recorded(&here.plane, to, path);
                    })
                {
                    return Ok(code);
                }
            }
            let ws = here.workspace(common.workspace.as_deref())?;
            let stamp = common.stamp()?;
            return Ok(todo(&here.plane, &ws, &words, repo.as_deref(), stamp));
        }
    }
    Ok(0)
}

/// `purlis ws todo` — record one todo, list them, or close one with `done`/`forget <slug>`,
/// saying what it did as `commands_workspace.cmd_workspace_todo` says it (M8.5: the port
/// did the writes and said nothing, so an agent recording a todo had no confirmation and
/// one closing a mistyped slug could not tell it had closed nothing).
fn todo(
    plane: &Plane,
    ws: &purlis_core::workspaces::Workspace,
    words: &[String],
    repo: Option<&str>,
    stamp: chrono::NaiveDateTime,
) -> u8 {
    let root = plane.root();
    let name = ws.name();
    let dir = ws.dir().join("todos");
    let see = format!("purlis ws todo --workspace {name}");
    let promoting = words.first().is_some_and(|verb| verb == "promote") && words.len() == 2;
    if repo.is_some() && !promoting {
        voice::err("--repo is taken only with `charter ws todo promote <slug>`.");
        return 1;
    }
    finish_promotes(plane, ws, stamp);
    match words {
        [verb, slug] if verb == "promote" => promote_todo(plane, ws, slug, repo, stamp),
        [] => {
            let (open, unread) = purlis_core::memstore::read_entries(root, &dir);
            voice::unread(root, &unread);
            if open.is_empty() {
                voice::info(&format!(
                    "No open todos in '{name}'. Record one: purlis ws todo \"<what>\""
                ));
                return 0;
            }
            // Oldest first, with its age: what surfaces is what is being avoided.
            let today = stamp.date();
            for todo in open {
                let file = todo.path.file_name().unwrap_or_default().to_string_lossy();
                let stem = todo.path.file_stem().unwrap_or_default().to_string_lossy();
                let age = purlis_core::memstore::memory_date(&todo.text, &file)
                    .map_or(0, |d| (today - d).num_days());
                println!(
                    "  {stem}  {age}d  {}",
                    purlis_core::personas::one_line(&todo.title)
                );
            }
            0
        }
        [verb, slug] if verb == "done" || verb == "forget" => {
            let slug = purlis_core::memstore::py_strip(slug);
            if !purlis_core::contain::segment_ok(slug) {
                voice::err(&purlis_core::repocmd::clone::not_a_segment(slug));
                voice::info(&format!("  List the real ones: {see}"));
                return 1;
            }
            let path = match purlis_core::memstore::resolve(root, &dir, slug) {
                Ok(path) => path,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        voice::err(&format!("no todo '{slug}' in workspace '{name}'."));
                    } else {
                        voice::err(&e.to_string());
                    }
                    voice::info(&format!("  List the real ones: {see}"));
                    return 1;
                }
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
            if let Err(e) = ws.forget_todo(slug) {
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
        [verb] if verb == "promote" => {
            voice::err("`todo promote` needs the slug of the todo to promote.");
            voice::info(&format!("  The slug is the first column: {see}"));
            voice::info(
                "  To record a todo actually called \"promote\", capitalise it or add a word: \
                 purlis ws todo \"Promote …\"",
            );
            1
        }
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
                 purlis ws todo \"{capital} …\""
            ));
            1
        }
        [text] => {
            // The rule is the core's (`Workspace::record_todo`), so the window's Todos panel
            // refuses the same todos in the same words.
            match ws.record_todo(text, stamp) {
                Ok(path) => {
                    said_todo_recorded(plane, name, &path.to_string_lossy());
                    0
                }
                Err(refused @ purlis_core::workspaces::RecordRefused::AlreadyListed(_)) => {
                    voice::err(&refused.to_string());
                    voice::info(&format!("  See it: {see}"));
                    1
                }
                Err(refused) => {
                    voice::err(&refused.to_string());
                    1
                }
            }
        }
        _ => {
            voice::err(
                "usage: purlis ws todo [-w WS] [\"<text>\" | done <slug> | forget <slug> | \
                 promote <slug> [--repo <repo>]]",
            );
            1
        }
    }
}

/// What `ws todo "<text>"` says once the todo is recorded in workspace `name`, at `path`.
fn said_todo_recorded(plane: &Plane, name: &str, path: &str) {
    voice::ok(&format!(
        "Todo recorded in '{name}' → workspaces/{name}/todos/{}",
        std::path::Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    ));
    if !plane.is_live(name) {
        voice::info(&format!(
            "  '{name}' is LOCAL (private) — todos stay on disk, not committed."
        ));
    }
}

/// Finish closing every todo of `ws` a promote already aliased to an issue, when a crash cut the
/// promote short after its alias (ADR 0088 §5). Best effort: a log or a store purlis cannot
/// read closes nothing here, and `ws todo` goes on as it would have.
fn finish_promotes(
    plane: &Plane,
    ws: &purlis_core::workspaces::Workspace,
    stamp: chrono::NaiveDateTime,
) {
    use purlis_core::work::{log, promote};
    if !log::dir_for(plane.root(), ws.name()).exists() {
        return;
    }
    if let Ok(closed) = promote::finish_closes(ws, &log::fold(plane.root()), stamp) {
        for (title, to) in closed {
            voice::ok(&format!(
                "Closed '{title}' in '{}' — it was promoted to {to}.",
                ws.name()
            ));
        }
    }
}

/// `charter ws todo promote <slug> [--repo <repo>]` (ADR 0088 §5): name the repo and whether it
/// is public, open the issue as the operator, alias the todo to it, and close the todo.
fn promote_todo(
    plane: &Plane,
    ws: &purlis_core::workspaces::Workspace,
    slug: &str,
    repo: Option<&str>,
    stamp: chrono::NaiveDateTime,
) -> u8 {
    use purlis_core::work::promote::{self, Step};
    let root = plane.root();
    let slug = purlis_core::memstore::py_strip(slug);
    if !purlis_core::contain::segment_ok(slug) {
        voice::err(&purlis_core::repocmd::clone::not_a_segment(slug));
        return 1;
    }
    let target = match promote::target(root, ws.name(), repo) {
        Ok(target) => target,
        Err(why) => {
            voice::err(&why);
            return 1;
        }
    };
    let device = match purlis_core::machine::this_device_id() {
        Ok(device) => device,
        Err(why) => {
            voice::err(&format!(
                "this device has no id, so the work link log has no file to write: {why}"
            ));
            return 1;
        }
    };
    let backend = target.forge.backend();
    let mut tell = |step: Step<'_>| match step {
        Step::Sending {
            target,
            about,
            label,
        } => {
            let label = match label {
                Some(label) => format!(", labelled {label}"),
                None => ", with no workspace label".to_string(),
            };
            voice::info(&format!(
                "Opening an issue in {} ({} at {}, {}) with the todo's title and text{label}.",
                target.name,
                target.path(),
                target.forge.host,
                about.visibility.readers()
            ));
        }
        Step::Created(item) => voice::info(&format!("  Opened {} — {}", item.key, item.url)),
    };
    match promote::promote(
        ws,
        slug,
        &target,
        backend.as_ref(),
        &purlis_core::forge::Caller::command(),
        &device,
        stamp,
        &mut tell,
    ) {
        Ok(done) => {
            // The close is `ws todo done`'s, so it says what `done` says about the journal.
            memory::said_remembered(plane, ws.name(), &done.journal, false);
            voice::ok(&format!(
                "Promoted '{}' in '{}' to {} — the todo is closed, and its work links now reach \
                 the issue.",
                done.title,
                ws.name(),
                done.item.key
            ));
            0
        }
        Err(why) => {
            voice::err(&why);
            1
        }
    }
}

/// This machine, as `purlis plugin` and `purlis doctor --fix` act on it: this binary by its
/// resolved path, and the plugin from `from` or else the one the app ships beside it.
fn plugin_machine(
    from: Option<&std::path::Path>,
) -> Result<purlis_core::plugin_install::Machine, String> {
    // The core's, so `charter plugin install` and the doctor's `plugin-install` fix build one
    // machine the one way.
    purlis_core::doctor::fix::this_machine(from)
}

/// `purlis plugin install|uninstall`: needs no plane, and acts on this machine's harnesses.
/// `migrate [--undo]`: rename-local on this machine, with the project the current directory is
/// in among the projects it moves (RN-5). What it did goes to stdout, one line a step; a
/// refusal, which moved nothing, to stderr. Exit 1 when anything was refused or failed.
fn migrate(undo: bool) -> ExitCode {
    use purlis_core::renamelocal::{self, Local, Seams};
    let here: Vec<std::path::PathBuf> = std::env::current_dir()
        .ok()
        .and_then(|cwd| purlis_core::plane::find_root(&cwd).ok())
        .into_iter()
        .collect();
    let Some(mut local) = Local::of_this_machine(&here) else {
        eprintln!(
            "purlis: cannot tell where this machine's config home is, so there is nowhere to \
             journal a move; nothing was moved"
        );
        return ExitCode::FAILURE;
    };
    // The harness plugin moves with the rest, installed from this charter and the plugin
    // shipped beside it, as `plugin install` installs it (RN-8). Without a plugin beside it,
    // Claude Code's registration is still pointed at the copy where it moved (D-RN8-13).
    if let Ok(machine) = purlis_core::doctor::fix::this_machine(None) {
        local.plugin = Some(machine);
    }
    let moved = if undo {
        renamelocal::undo(&local, &Seams::real())
    } else {
        renamelocal::run(&local, &Seams::real())
    };
    if let Some(why) = &moved.refused {
        eprintln!("purlis: {why}");
        return ExitCode::FAILURE;
    }
    for line in &moved.said {
        println!("{line}");
    }
    if moved.complete {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn plugin(verb: &PluginCommand) -> ExitCode {
    use purlis_core::plugin_install::{self as install, Verb};
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
            eprintln!("purlis: {why}");
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
        guard::refuse_on_a_crash(tell_the_host_about_a_crash);
    }
    // A warning the core raises is said on this terminal rather than dropped (#647). Output a
    // command prints on purpose never goes through it.
    purlis_core::applog::install_for_a_terminal();
    #[cfg(debug_assertions)]
    if std::env::var_os(PANIC_ON_PURPOSE_ENV).is_some() {
        panic!("{PANIC_ON_PURPOSE_ENV} is set, so purlis crashes");
    }
    #[cfg(debug_assertions)]
    if std::env::var_os(CORE_WARNS_ON_PURPOSE_ENV).is_some() {
        purlis_core::applog::warn_on_purpose();
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
    // An extension's command runs before clap and before the read-only gate below, so it is
    // gated here: any first word that is not a core command or a flag goes to an extension,
    // and an extension's command can write the project (FR-24).
    let words: Vec<String> = argv
        .iter()
        .skip(1)
        .map(|word| word.to_string_lossy().into_owned())
        .collect();
    if purlis_core::extension::cli::extension_command(&words).is_some() {
        // An extension's command can act on the project too (D-RN2d-8).
        if let Some(disagree) = purlis_core::envvar::disagreement() {
            return refused(&disagree.to_string());
        }
        if let Some(why) = read_only_project() {
            return refused(&why);
        }
    }
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
    // **A variable that chooses what to act on, named twice with two values, is refused**
    // (D-RN2d-8), never read under one of its names: inside a chat both names carry the same
    // value, so this is a `CHARTER_ROOT=…` typed in front of a command, which would otherwise
    // act on the chat's own project. Every command that acts on a project refuses; a hook
    // decides per kind ([`hook_on_disagreement`]); the commands below need no project, and a
    // guarded program that is `charter` refuses on its own.
    if let Some(disagree) = purlis_core::envvar::disagreement() {
        match &cli.command {
            Command::Hook {
                name, list: false, ..
            } => return hook_on_disagreement(name.as_deref().unwrap_or_default(), &disagree),
            // `report`, the shell guard, git's hooks and `scan` need no project: a commit in a
            // repository that is not one is never blocked over the project's variables.
            Command::Hook { list: true, .. }
            | Command::Report(_)
            | Command::ShellGuard { .. }
            | Command::GitHook { .. }
            | Command::Scan { .. } => {}
            _ => return refused(&disagree.to_string()),
        }
    }
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
    // Needs no plane either, and must never stand in the way of the harness it guards.
    if let Command::ShellGuard {
        shims,
        harness,
        args,
    } = &cli.command
    {
        return shellguard::run(shims, harness, args);
    }
    // Needs no plane: it runs in whatever repository a chat commits to.
    if let Command::GitHook { name, args } = &cli.command {
        return githook::run(name, args);
    }
    // Needs no plane either: it scans the repository it stands in.
    if let Command::Scan { explain } = &cli.command {
        return scan::run(*explain);
    }
    // Needs no project: the app that started the chat answers it, from its own record (#1450).
    if let Command::Persona(memory::PersonaCommand::Where { now }) = &cli.command {
        return whereworking::run(now.as_deref());
    }
    // The three internal words the Python charter's plugin wires beside its hooks. Answered —
    // exit 0, nothing printed, nothing read — so a plugin that still names them can never fail
    // a session start or a turn end on them; `hookreg` says why each is left out on purpose.
    if matches!(
        &cli.command,
        Command::Workspace(WorkspaceCommand::Reconcile | WorkspaceCommand::Autosave)
            | Command::Persona(memory::PersonaCommand::Gc { .. })
    ) {
        return ExitCode::SUCCESS;
    }
    // `persona sync-agents` is retired with the persona sub-agent (#1451). The word is taken
    // and refused by name, with what replaced it, wherever it is run from.
    if matches!(
        &cli.command,
        Command::Persona(memory::PersonaCommand::SyncAgents { .. })
    ) {
        voice::err(purlis_core::personaverbs::retired::SYNC_AGENTS);
        return ExitCode::FAILURE;
    }
    // A project this charter cannot write is read-only to it (FR-24, V37a,
    // `docs/plane-format.md` § Compatibility across charter versions): a format it does not
    // understand, a feature it lacks, or a `charter.toml` it cannot read. Every command that
    // could write it is refused, naming why; the ones that only say what is wrong or move
    // charter itself forward still run.
    //
    // `init` and `reinit` facing a `charter.toml` that is a link are left to their own
    // containment gate, which refuses a link out of the project with the words that name it and
    // writes nothing; reading through the link here would answer a file that is not the
    // project's.
    let own_gate = matches!(&cli.command, Command::Init(_) | Command::Reinit)
        && place().is_ok_and(|p| purlis_core::scaffold::manifest_escapes(&p.root));
    if !own_gate && let Some(why) = read_only_project() {
        match read_only_standing(&cli.command) {
            ReadOnly::Runs => {}
            ReadOnly::RunsAndSays => eprintln!("purlis: {why}"),
            ReadOnly::Refused => return refused(&why),
        }
    }
    // After the gate: on a project this charter cannot write, the server is refused whole.
    if let Command::Mcp = &cli.command {
        // Started by the harness, as a hook is (#1421).
        purlis_core::sandbox::started_by_the_harness();
        // A chat holds this server for its whole life: the config home stays where it is until
        // it ends (`renamelocal::busy::LOCK`, D-RN5-11).
        let _holds = purlis_core::machine::config_root_if_there()
            .and_then(|root| purlis_core::renamelocal::busy::hold_shared(&root));
        return match mcp::serve() {
            Ok(code) => ExitCode::from(code),
            Err(why) => {
                voice::err(&why);
                ExitCode::FAILURE
            }
        };
    }
    // `init`, `reinit` and `doctor` each say several lines of their own and choose their own
    // exit status — and for `doctor` the status IS the verdict, where a blocker is not an
    // error message.
    match &cli.command {
        Command::Init(init) => {
            let args = purlis_core::scaffold::InitArgs {
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
                        eprintln!("purlis: {why}");
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
                Ok(place) => say(&purlis_core::scaffold::init(&place, &args)),
                Err(message) => {
                    eprintln!("purlis: {message}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::Reinit => {
            return match place() {
                Ok(place) => say(&purlis_core::scaffold::reinit(&place)),
                Err(message) => {
                    eprintln!("purlis: {message}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::Doctor {
            json,
            preflight,
            fix,
            name,
            email,
        } => {
            // FX-3: either flag is the git identity's input; the core says what is missing.
            let identity = (name.is_some() || email.is_some()).then(|| {
                (
                    name.as_deref().unwrap_or_default(),
                    email.as_deref().unwrap_or_default(),
                )
            });
            return doctor(
                *json,
                *preflight,
                fix.as_ref().map(Option::as_deref),
                identity,
            );
        }
        Command::Migrate { undo } => return migrate(*undo),
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
                    eprintln!("purlis: {why}");
                    return ExitCode::FAILURE;
                }
            };
            let workspace = match here.active_workspace(workspace.as_deref()) {
                Ok(ws) => ws,
                Err(why) => return refused(&why),
            };
            return gl_refresh(&workspace, *detach, now.as_deref());
        }
        Command::Statusline { watch, now, .. } => {
            // Before stdin is read or a plane is looked for: nothing is drawn, nothing is
            // recorded, and the answer does not depend on where it is asked (HY-11).
            // Whether `--watch` repaints or is retired: #997.
            if *watch {
                return refused(
                    "`statusline --watch` is not in this version yet; statusline does not repaint, \
                     and one frame would pass for a watch that stopped, so run \
                     `purlis statusline` once per turn from the harness's status-line \
                     command instead",
                );
            }
            let payload = payload();
            // Resolved BEFORE the render, and a bad value is refused rather than quietly
            // replaced by the wall clock: the flag exists so a differential can pin the ages
            // on the row, and a pin that silently did not take would make the comparison
            // green for the wrong reason.
            let when = match instant(now.as_deref()) {
                Ok(secs) => chrono::DateTime::from_timestamp(secs as i64, 0)
                    .unwrap_or_else(chrono::Utc::now),
                Err(why) => {
                    eprintln!("purlis: {why}");
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
            use purlis_core::adopt;
            return emit(&match what {
                Some(VersionCommand::Sync { .. }) => adopt::version_move_refusal("sync"),
                Some(VersionCommand::Bump { .. }) => adopt::version_move_refusal("bump"),
                None => {
                    let cwd =
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                    let place = purlis_core::plane::place(&cwd);
                    adopt::version_report(place.is_plane.then_some(place.root.as_path()))
                }
            });
        }
        Command::Stop { .. } => return stop_every_agent(),
        // `news` and `update` say several lines of their own on both streams and choose their
        // own exit status, exactly as `init` does.
        Command::News(news) => {
            return emit(&purlis_core::news::report(&purlis_core::news::Args {
                for_version: news.for_version.clone(),
                since: news.since.is_some(),
                until: news.until.is_some(),
                pending: news.pending,
            }));
        }
        Command::Update { to, bump, channel } => {
            let config_root = purlis_core::machine::config_root();
            if let Some(word) = channel {
                return emit(&purlis_core::adopt::set_channel_report(
                    config_root.as_deref(),
                    word,
                ));
            }
            let args = purlis_core::adopt::UpdateArgs {
                to: to.clone().unwrap_or_default(),
                bump: *bump,
            };
            return emit(&purlis_core::adopt::update_report_with_channel(
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
    // `dispatch` says what it started, or that it is held for the person, and exits 0; or says
    // one refusal and exits 1.
    if let Command::Dispatch {
        to,
        name,
        profile,
        wait,
        timeout,
        place,
        command,
    } = &cli.command
    {
        if let Some(command) = command {
            return dispatch::run(command);
        }
        let here = match Here::read() {
            Ok(here) => here,
            Err(why) => {
                eprintln!("purlis: {why}");
                return ExitCode::FAILURE;
            }
        };
        let waits = wait.then(|| timeout.unwrap_or(purlis_core::dispatched::WAITS_BY_DEFAULT));
        return dispatch::dispatch(
            &here,
            to.as_deref(),
            name.as_deref(),
            &dispatch::Options {
                profile: profile.as_deref(),
                place: place.as_deref(),
            },
            waits,
        );
    }
    // `handoff` answers for itself: opened or held for the person (0), or one refusal (1).
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
                eprintln!("purlis: {why}");
                return ExitCode::FAILURE;
            }
        };
        let args = handoff::Args {
            workspace: workspace.clone(),
            create: *create,
            vision: vision.clone(),
            persona: persona.clone(),
            name: name.clone(),
            report: *report,
            summary: summary.clone(),
            now: now.clone(),
        };
        let code = handoff::handoff(&here, &args);
        if code == ExitCode::SUCCESS && args.creates_a_handoff() {
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
            eprintln!("purlis: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Run `f` on this invocation's [`Here`], or say why there is none.
fn with_here(f: impl FnOnce(&Here) -> u8) -> ExitCode {
    match Here::read() {
        Ok(here) => ExitCode::from(f(&here)),
        Err(why) => {
            eprintln!("purlis: {why}");
            ExitCode::FAILURE
        }
    }
}

/// `purlis doctor`: every check, as a table or as `--json`, and the verdict as the exit.
///
/// `--fix` repairs before it reports, so the report reads as the state after the repair, as
/// Python's did when it installed the Claude Code plugin first. Bare `--fix` applies purlis's
/// plugin install (#373) and every fix the rows offer that is not `by_name_only` (the fix
/// registry, FX-1 and FX-2), then the plane's ask rule for `charter report --yes` (ADR 0059);
/// `--fix <id>` applies that one fix alone, `discover` included.
///
/// `fix` is `None` without `--fix`, `Some(None)` for a bare `--fix`, and `Some(Some(id))` for
/// one fix.
///
/// `identity` is `--name` and `--email`, the input `git-identity` takes (FX-3); a fix that
/// takes input and was given none is refused by the registry, which says what to give.
fn doctor(
    json: bool,
    preflight: bool,
    fix: Option<Option<&str>>,
    identity: Option<(&str, &str)>,
) -> ExitCode {
    use std::io::IsTerminal;

    // `--name`/`--email` with `--fix <id>` for another fix would be dropped without a word.
    if identity.is_some()
        && let Some(Some(id)) = fix
        && id != purlis_core::doctor::fix::FixId::GitIdentity.id()
    {
        eprintln!("purlis doctor: --name and --email go with --fix git-identity, not --fix {id}");
        return ExitCode::from(2);
    }

    let mut fix_failed = false;
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("purlis: cannot read the current directory: {e}");
            return ExitCode::FAILURE;
        }
    };
    // The registry's fixes (FX-1, FX-2): the one asked for, or every one a row offers. Bare
    // `--fix` always installs charter's plugin for the chats started outside the app (#373),
    // offered or not, as it always has: through the registry's `plugin-install`, the one code
    // path, so a row that offers it too does not install it twice. Each fix's lines go to
    // stderr so a `--json` reader still gets JSON alone.
    if let Some(asked) = fix {
        use purlis_core::doctor::fix::{self as registry, FixId};
        let doctor = purlis_core::doctor::Doctor::new(&cwd, preflight);
        let mut ids: Vec<FixId> = match asked {
            // clap took only an id the registry has.
            Some(id) => FixId::parse(id).into_iter().collect(),
            // Bare: the local, additive fixes only. A fix that goes over the network runs
            // only when it is named (`FixId::by_name_only`, D-FX2-9).
            None => {
                let mut ids = doctor.fixes();
                ids.push(FixId::PluginInstall);
                ids.retain(|id| !id.by_name_only());
                ids.sort();
                ids.dedup();
                ids
            }
        };
        // `--name`/`--email` with a bare `--fix` are the identity's input even when the row
        // offers no fix: the core then says the identity is already set, and the flags are
        // never dropped without a word (FX-3).
        if identity.is_some() && !ids.contains(&FixId::GitIdentity) {
            ids.push(FixId::GitIdentity);
            ids.sort();
        }
        for id in ids {
            let lines = match (id, identity) {
                (FixId::GitIdentity, Some((name, email))) => {
                    match registry::identity::apply(doctor.root(), name, email) {
                        Ok(fixed) => {
                            fix_failed |= !fixed.complete();
                            fixed.lines()
                        }
                        Err(invalid) => {
                            fix_failed = true;
                            invalid
                                .lines()
                                .into_iter()
                                .map(|why| format!("✗ refused: {why}"))
                                .collect()
                        }
                    }
                }
                _ => {
                    let fixed = registry::apply(doctor.root(), id);
                    fix_failed |= !fixed.complete();
                    fixed.lines()
                }
            };
            eprintln!("fix {id}:");
            for line in lines {
                eprintln!("  {line}");
            }
        }
    }
    // The plane repair: the ask rule a report is filed behind (ADR 0059, amended 2026-09-26).
    if fix == Some(None)
        && let Some((said, code)) = purlis_core::doctor::fix_report_rule(&cwd)
    {
        eprint!("{said}");
        fix_failed |= code != 0;
    }
    let rows = purlis_core::doctor::Doctor::new(&cwd, preflight).run();
    if json {
        print!("{}", purlis_core::doctor::json(&rows));
    } else {
        print!(
            "{}",
            purlis_core::doctor::table(&rows, std::io::stdout().is_terminal())
        );
    }
    ExitCode::from(purlis_core::doctor::exit_code(&rows).max(u8::from(fix_failed)))
}

/// `purlis harness show <name>`: the declaration on stdout under one comment line naming
/// where it came from and its digest, or the one sentence on stderr saying why there is none.
///
/// Each line through [`shown::readable`]: the file is one a chat can write, and a control
/// byte in it would otherwise drive the reader's terminal. **So what it prints is not always
/// the digested bytes**: anything outside printable ASCII (a tab, an accented letter in a
/// comment) is printed escaped, and the digest in the first line is of the file as it is on
/// disk.
fn harness_show(root: &std::path::Path, name: &str) -> u8 {
    use purlis_core::harness_declaration as decl;
    let declared = decl::read(root);
    let Some(found) = declared.get(name) else {
        let file = format!("{}/{}.toml", decl::DIR, shown::short(name));
        if let Some(refused) = declared.refused.iter().find(|r| r.file == file) {
            eprintln!("purlis: {}", refused.reason);
            return 1;
        }
        let names: Vec<&str> = declared.declared.iter().map(|d| d.name.as_str()).collect();
        eprintln!(
            "purlis: no harness '{}' is declared — the harnesses are {}. A project declares \
             one in {}/<name>.toml.",
            shown::short(name),
            names.join(", "),
            decl::DIR
        );
        return 1;
    };
    let lines: Vec<String> = found
        .text
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                shown::readable(line, usize::MAX)
            }
        })
        .collect();
    println!("# {}: {}, {}", found.name, found.file, found.digest);
    if lines
        .iter()
        .zip(found.text.lines())
        .any(|(shown, line)| shown != line)
    {
        println!(
            "# shown escaped: what is not printable ASCII is written as an escape, so the text \
             below is not the bytes the digest is of"
        );
    }
    for line in lines {
        println!("{line}");
    }
    0
}

/// The profile listing, as `purlis harness list` prints it on stderr.
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
    let mut rows: Vec<&purlis_core::profiles::Profile> = set.profiles().iter().collect();
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
        let source = p.source.as_str();
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
        let refused: BTreeSet<String> = purlis_core::extension::cli::CORE_WORDS
            .iter()
            .map(|&word| word.to_owned())
            .collect();
        let unprotected: Vec<&String> = parser.difference(&refused).collect();
        assert!(
            unprotected.is_empty(),
            "`charter {unprotected:?}` is a core command an extension could still take as its \
             id — add it to purlis_core::extension::cli::CORE_WORDS"
        );
        let stale: Vec<&String> = refused.difference(&parser).collect();
        assert!(
            stale.is_empty(),
            "CORE_WORDS refuses {stale:?}, which is no longer a word charter answers"
        );
    }
}
