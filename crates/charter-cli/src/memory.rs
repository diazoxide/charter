//! The memory commands: `charter recall`, `charter workspace remember|note|recall|forget|
//! optimize` and `charter persona remember|recall` — `charter/commands.py:cmd_recall`,
//! `commands_workspace.py` and `commands_persona.py`, with their output byte for byte.
//!
//! **What an agent reads is the contract.** `charter recall` runs at session start and on
//! demand, and whatever consumes it parses these exact lines; the recorded scenarios
//! (`recall-*`, ADR 0046) hold them to Python's.
//!
//! Two things this binary does not do, and says so where a command meets them:
//!
//! - **Commit memory reactively.** A plane whose `[memory] share` is `commit` or `push` had
//!   the Python charter commit each memory as it was written. Here a memory travels with the
//!   plane's next save: the app's auto-save, or, outside the app, `charter save` (ADR 0051).
//! - **Follow a slug out of the store.** `workspace forget` takes one path segment. charter's
//!   own resolver accepts more than that; this does not, and that is deliberate.

use std::path::{Path, PathBuf};

use charter_core::memstore::{self, Unread};
use charter_core::recall::{self, Ask, Workspaces};
use charter_core::workspaces::Plane;
use clap::{Args, Subcommand};

use crate::voice;

/// A command's own exit code, as charter's `cmd_*` functions return it.
pub type Code = u8;

#[derive(Args)]
pub struct RecallArgs {
    /// Keyword query; omit to list recent memories across bases.
    query: Option<String>,
    /// Comma list of scopes to search (workspace,persona,shared,refs,ephemeral); default
    /// workspace,persona,shared,refs.
    #[arg(long)]
    scope: Option<String>,
    /// Also include the persona's session scratch.
    #[arg(long)]
    ephemeral: bool,
    /// Also print a line of each memory's body (the path is always shown).
    #[arg(long)]
    full: bool,
    /// The persona whose memory and refs are searched (default: the active one).
    #[arg(long)]
    persona: Option<String>,
    /// The workspace whose journal is searched (default: the active one).
    #[arg(short = 'w', long = "workspace", conflicts_with = "all_workspaces")]
    workspace: Option<String>,
    /// Search EVERY workspace's journal (persona + shared appear once).
    #[arg(long)]
    all_workspaces: bool,
    /// Only memories recorded on/after this: an age (14d, 2w, 3m) or a date (2026-07-01).
    #[arg(long, value_name = "WHEN")]
    since: Option<String>,
    /// Max results (0 = no cap).
    #[arg(long, default_value_t = 8, allow_negative_numbers = true)]
    limit: i64,
    /// Pin today's date, for tests only.
    #[arg(long, hide = true)]
    now: Option<String>,
}

#[derive(Subcommand)]
pub enum PersonaCommand {
    /// Internal, answered and ignored: the Python plugin's SessionStart prune of ended
    /// sessions' ephemeral scratch. Not ported, because its rule — a session idle for six hours
    /// has ended — is wrong for a chat the app keeps open for days, and a prune that deletes a
    /// live chat's scratch cannot be undone. Leaving it costs a few small files.
    #[command(name = "_gc", hide = true)]
    Gc {
        /// Accepted and ignored: the Python charter re-runs itself in the background.
        #[arg(long)]
        detach: bool,
    },
    /// Print the active persona — the name alone, or `(none)`.
    ///
    /// Not a memory command, and it lives here because this is the enum `charter persona`
    /// dispatches on rather than because it belongs to memory. It takes no `--persona`, as
    /// charter's own `persona current` takes none: the top rung of the ladder is typed on
    /// the command it acts on, and a flag here would report a resolution nothing performed.
    Current,
    /// Clear the active persona: this session's, this pane's and the plane-wide selection.
    ///
    /// The work is [`charter_core::personaverbs::select::clear`].
    Clear,
    /// Create a persona → committed personas/<name>/persona.md, as a draft.
    ///
    /// The work is [`charter_core::personaverbs::define::create`].
    Create {
        name: String,
        /// Human role, e.g. "DevOps Engineer" (default: the name, title-cased).
        #[arg(long)]
        role: Option<String>,
        /// REQUIRED (unless --extends): when the steward should route work here, e.g. "CI/CD
        /// pipelines, k8s deploys". Becomes the persona's routing line in its dispatchable
        /// description.
        #[arg(long, value_name = "WHEN")]
        delegate_when: Option<String>,
        /// Vault this persona uses (default: the persona name; `none` for no credentials).
        #[arg(long)]
        vault: Option<String>,
        /// Inherit another persona's charter + tools; this one adds its own on top.
        #[arg(long, value_name = "PARENT")]
        extends: Option<String>,
        /// Also register its vault now, as `charter vault add <vault> --persona <name>`.
        #[arg(long)]
        with_vault: bool,
        /// Make it the active persona.
        #[arg(long = "use")]
        select: bool,
        /// Overwrite an existing definition.
        #[arg(long)]
        force: bool,
    },
    /// Print a persona's metadata and charter.
    ///
    /// The work is [`charter_core::personaverbs::show`].
    Show { name: String },
    /// Delete a persona definition (commit the deletion).
    ///
    /// The work is [`charter_core::personaverbs::define::remove`].
    Remove {
        name: String,
        /// Remove even if another persona extends/uses it (leaves a dangling ref).
        #[arg(long)]
        force: bool,
    },
    /// Config eval: dangling uses:, missing role/vault/delegate-when, stale agents.
    ///
    /// The work is [`charter_core::personaverbs::lint`].
    Lint {
        /// Only this persona (default: all).
        name: Option<String>,
        /// Report only findings mentioning KEY, and exit non-zero solely on those.
        #[arg(long, value_name = "KEY")]
        only: Option<String>,
    },
    /// Write a memory (persistent by default; --ephemeral for scratch).
    Remember {
        /// `[NAME] TEXT` — the persona, then the fact. One word is the FACT, and the persona
        /// is the active one, which is how charter's own `name nargs="?"` reads it.
        #[arg(num_args = 1..=2, required = true)]
        words: Vec<String>,
        /// Short title (default: first line of the text).
        #[arg(long)]
        title: Option<String>,
        /// Write to the cross-persona _shared namespace.
        #[arg(long)]
        shared: bool,
        /// Session scratch, deleted after the session.
        #[arg(long)]
        ephemeral: bool,
        /// Don't reactively commit+push it now (record locally; sync later).
        #[arg(long)]
        no_sync: bool,
        /// Pin the clock, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
    /// Show, set or clear the plane's declared front door — `charter.toml`'s
    /// `[persona] default`.
    ///
    /// Not a memory command either, and here for the same reason `current` is: this is the
    /// enum `charter persona` dispatches on. The work is [`charter_core::personacmd`].
    Default {
        /// The persona to declare. Omit to print what is declared.
        name: Option<String>,
        /// Undeclare it — from `charter.toml` AND from the legacy `personas/.default`.
        #[arg(long)]
        clear: bool,
    },
    /// List personas; mark active; show role + vault status.
    ///
    /// The work is [`charter_core::personaverbs::list`].
    List,
    /// Set the active persona (writes .charter/active-persona).
    ///
    /// For this session and this pane — the plane-wide file only when the process has
    /// neither id. The work is [`charter_core::personaverbs::select`].
    Use {
        name: String,
        /// Pin the trace's clock, for tests only.
        #[arg(long, hide = true)]
        now: Option<String>,
    },
    /// Generate a Claude Code sub-agent (.claude/agents/<name>.md) per persona.
    ///
    /// The work is [`charter_core::personaverbs::agents`].
    #[command(name = "sync-agents")]
    SyncAgents {
        /// Only sync this persona (default: all).
        #[arg(long)]
        persona: Option<String>,
        /// Ask, per server, whether the MCP command the personas' mcp.json files name may
        /// receive the persona's vault value. What is recorded is a digest of the line
        /// printed above the question, so ANY change that changes that line — including the
        /// persona's vault, an env value, or a key charter does not read — lapses the
        /// approval and asks again.
        #[arg(long)]
        approve_mcp: bool,
        /// With --approve-mcp: approve every credentialed server without asking. Required
        /// off a terminal, where nobody can be asked.
        #[arg(long)]
        yes: bool,
        /// With --approve-mcp: print the servers it would ask about and record nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Roster health from committed memory: usage (count/recency) + quality proxy
    /// (verification / dedup ratios); flags prune candidates.
    ///
    /// The work is [`charter_core::personaverbs::stats`].
    Stats {
        /// Only this persona (default: all + _shared).
        name: Option<String>,
        /// Window for the RECENT column (`stats::RECENT_DAYS` by default).
        #[arg(
            long,
            default_value_t = charter_core::personaverbs::stats::RECENT_DAYS,
            allow_negative_numbers = true
        )]
        recent_days: i64,
    },
    /// Read/write the ACTIVE persona's vault (values stay out of the model).
    ///
    /// Not a memory command; here because this is the enum `charter persona` dispatches on.
    #[command(subcommand)]
    Secret(crate::secret::PersonaSecretCommand),
    /// Show a persona's memory, or --query to search it.
    Recall {
        /// The persona (default: the active one).
        name: Option<String>,
        /// Keyword-search memories (ranked) instead of listing all.
        #[arg(short = 'q', long)]
        query: Option<String>,
        /// Log lines to show / max search hits (default: 8).
        #[arg(long, default_value_t = 8, allow_negative_numbers = true)]
        log: i64,
    },
}

/// The clock a write stamps itself with: `--now` in a test, the local clock otherwise.
pub fn stamp(now: Option<&str>) -> Result<chrono::NaiveDateTime, String> {
    match now {
        Some(text) => text
            .parse()
            .map_err(|e| format!("--now is not a local naive timestamp: {e}")),
        None => Ok(chrono::Local::now().naive_local()),
    }
}

fn session() -> String {
    charter_core::trace::bucket(&|name| std::env::var(name).ok())
}

/// What happens after a memory is written, in charter's words — `commit_memory_reactive`.
///
/// Nothing commits a memory on its own: it travels with the plane's next save, by
/// `[plane] mode` (ADR 0051). A plane that names no mode, or `off`, says nothing; any other
/// says when the memory will leave this machine, rather than leaving the operator to find it
/// uncommitted later.
fn reactive(plane: &Plane) {
    use charter_core::planesave::Mode;
    let mode = charter_core::planesave::Settings::read(plane.root())
        .plane
        .mode
        .value;
    if let Some(mode @ (Mode::Commit | Mode::Push | Mode::Pr | Mode::PrMerge)) = mode {
        voice::info(&format!(
            "This plane's [plane] mode is {}: the memory goes with the plane's next save — \
             `charter save`.",
            mode.as_str()
        ));
    }
}

// ---------------------------------------------------------------------------------------
// charter recall

/// `charter recall` — the one memory gate, across every base in scope.
pub fn recall(here: &crate::Here, args: RecallArgs) -> Result<Code, String> {
    let plane = &here.plane;
    let root = plane.root();
    // Before anything is searched: a `--persona` that names nothing was searched as if it
    // did, and answered "no memories" over the persona that holds them (#1055, #1059).
    let persona = args.persona.filter(|p| !p.is_empty());
    if let Some(name) = &persona
        && let Some(refused) = charter_core::personas::name_refusal(root, name)
    {
        voice::err(&refused);
        return Ok(1);
    }
    let mut scopes: Vec<String> = recall::DEFAULT_SCOPES.map(str::to_string).to_vec();
    if let Some(scope) = args.scope.as_deref().filter(|s| !s.is_empty()) {
        scopes = scope
            .split(',')
            .map(memstore::py_strip)
            .filter(|s| recall::SCOPES.contains(s))
            .map(str::to_string)
            .collect();
        if scopes.is_empty() {
            voice::err(&format!(
                "invalid --scope; choose from {}",
                recall::SCOPES.join(", ")
            ));
            return Ok(1);
        }
    }
    if args.ephemeral && !scopes.iter().any(|s| s == "ephemeral") {
        scopes.push("ephemeral".to_string());
    }
    let today = stamp(args.now.as_deref())?.date();
    let mut since = None;
    if let Some(value) = args.since.as_deref().filter(|s| !s.is_empty()) {
        match recall::parse_since(value, today) {
            Ok(day) => since = Some(day),
            Err(message) => {
                voice::err(&message);
                return Ok(2);
            }
        }
    }

    let has = |scope: &str| scopes.iter().any(|s| s == scope);
    // **The ladder, not a refusal.** `charter recall` with no flags is how a harness calls
    // it at session start; before M2.9 this exited 2 with "does not resolve the active
    // workspace", so the one command a session begins with could not be called at all.
    let workspaces = if args.all_workspaces {
        Some(Workspaces::All)
    } else if has("workspace") {
        // A name is checked before it becomes a path. charter joins `-w` straight on and
        // reads whatever that lands on inside the plane's data; a name that is a path is not
        // one this reads through. The resolved name has already passed the same check on the
        // way out of the ladder, so this can only ever fire for a flag somebody typed.
        let name = here.active_workspace(args.workspace.as_deref().filter(|w| !w.is_empty()));
        if let Err(e) = plane.workspace(&name) {
            voice::err(&e.to_string());
            return Ok(1);
        }
        Some(Workspaces::One(name))
    } else {
        // `--scope persona` names no workspace at all, and charter's `sources` resolves one
        // only for the scope that uses it. Resolving here anyway would let a `-w` the search
        // will never touch refuse the command.
        None
    };
    // A persona is allowed to be absent, where a workspace is not: a plane may genuinely
    // have no front door, and charter then SKIPS those scopes rather than refusing the
    // search (`recall.sources`: "a scope with no owner"). The flag is still refused when it
    // names nothing — that is the check above, and it is about the flag, not the rung.
    let persona = persona.or_else(|| here.active_persona(None));

    if args.all_workspaces {
        say_unread_workspaces(root);
    }
    let limit = args.limit;
    let ask = Ask {
        scopes: scopes.clone(),
        workspaces,
        persona,
        session: session(),
        query: args.query.clone(),
        // One more than is shown, so "8 of ?" can say whether anything was cut.
        limit: if limit != 0 { limit + 1 } else { 0 },
        since,
    };
    let got = recall::recall(root, &ask);
    voice::unread(root, &got.unread);
    let skipped = if got.unread.is_empty() {
        String::new()
    } else {
        format!(
            "; {} base(s) not searched — charter could not read them",
            got.unread.len()
        )
    };
    let code = if got.unread.is_empty() { 0 } else { 1 };
    let truncated = limit != 0 && (got.hits.len() as i64) > limit;
    let results = if limit != 0 {
        recall::py_head(got.hits, limit)
    } else {
        got.hits
    };
    let place = if args.all_workspaces {
        "every workspace".to_string()
    } else {
        scopes.join(", ")
    };
    let query = args.query.as_deref().filter(|q| !q.is_empty());

    if results.is_empty() {
        if let Some(q) = query
            && memstore::terms(q).is_empty()
        {
            let dropped = memstore::dropped_terms(q);
            let named = if dropped.is_empty() {
                charter_core::pyrepr::repr_str(q)
            } else {
                dropped
                    .iter()
                    .map(|t| charter_core::pyrepr::repr_str(t))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let verb = if dropped.len() == 1 { "is" } else { "are" };
            voice::info(&format!(
                "Nothing searchable in {} — {named} {verb} too short or too common to rank. \
                 Try a distinctive word.",
                charter_core::pyrepr::repr_str(q)
            ));
            return Ok(code);
        }
        let what = match query {
            Some(q) => format!("match {}", charter_core::pyrepr::repr_str(q)),
            None => "yet".to_string(),
        };
        let when = since
            .map(|d| format!(" recorded since {}", d.format("%Y-%m-%d")))
            .unwrap_or_default();
        voice::info(&format!(
            "No memories {what} across {place}{when}{skipped}."
        ));
        if got.undated > 0 {
            voice::info(&format!(
                "{} undated memory(ies) skipped — no recorded date to compare.",
                got.undated
            ));
        }
        return Ok(code);
    }

    let dates: Vec<String> = results
        .iter()
        .map(|h| {
            h.date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "—".to_string())
        })
        .collect();
    let dw = voice::column(dates.iter().map(String::as_str));
    let lw = voice::column(results.iter().map(|h| h.label.as_str()));
    let hang = " ".repeat(2 + dw);
    let mut out = String::new();
    for (hit, date) in results.iter().zip(&dates) {
        let tag = if hit.score > 0 {
            format!("  ({})", hit.score)
        } else {
            String::new()
        };
        let line = format!(
            "  {}{}{}{tag}",
            voice::pad(date, dw),
            voice::pad(&hit.label, lw),
            hit.title
        );
        out.push_str(voice::py_rstrip(&line));
        out.push('\n');
        out.push_str(&format!("{hang}{}\n", voice::rel(root, &hit.path)));
        if args.full {
            let snip = body_snippet(&hit.path, true);
            if !snip.is_empty() {
                out.push_str(&format!("{hang}{snip}\n"));
            }
        }
    }
    print!("{out}");
    voice::info(&format!(
        "{} memory(ies) across {place}{skipped}.{}",
        results.len(),
        if args.full {
            ""
        } else {
            "  Pass --full for a line of each body."
        }
    ));
    if truncated {
        voice::info(&format!(
            "Showing {} — pass --limit 0 for all.",
            results.len()
        ));
    }
    if got.undated > 0 {
        voice::info(&format!(
            "{} undated memory(ies) skipped by --since — no recorded date.",
            got.undated
        ));
    }
    if got.undated_refs > 0 {
        voice::info(&format!(
            "{} ref doc(s) not searched — `--since` filters by recorded date and refs carry \
             none. Drop --since to include them.",
            got.undated_refs
        ));
    }
    Ok(code)
}

/// The first line of a memory's body worth showing — `commands._body_snippet` (with
/// `frontmatter`) or `commands_persona._snippet` (without). Headings and `_…_` lines are
/// skipped because none of them tell one memory from another; cut at 90 with `…`.
fn body_snippet(path: &Path, frontmatter: bool) -> String {
    let Some(text) = memstore::read_text(path) else {
        return String::new();
    };
    let mut in_front = false;
    for (i, raw) in charter_core::mdsection::split_lines(&text)
        .into_iter()
        .enumerate()
    {
        let line = memstore::py_strip(raw);
        if frontmatter {
            if i == 0 && line == "---" {
                in_front = true;
                continue;
            }
            if in_front {
                in_front = line != "---";
                continue;
            }
        }
        if !line.is_empty() && !line.starts_with('#') && !line.starts_with('_') {
            let cut: String = line.chars().take(90).collect();
            return if line.chars().count() > 90 {
                format!("{cut}…")
            } else {
                cut
            };
        }
    }
    String::new()
}

/// Name each workspace `--all-workspaces` could not look at — `workspace.read_workspaces_aloud`.
fn say_unread_workspaces(root: &Path) {
    let Ok((_, unread)) = recall::read_workspaces(root) else {
        return;
    };
    for (path, code) in unread {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let shown = path.to_string_lossy();
        voice::err(&format!(
            "workspace '{name}' cannot be checked — charter changes nothing it cannot see; {}.",
            voice::uncheckable_fix(code, &shown, &shown)
        ));
    }
}

// ---------------------------------------------------------------------------------------
// charter workspace remember | note | recall | forget | optimize

/// `workspace remember` / `note` — record one journal entry, or with no text list them.
pub fn workspace_remember(
    plane: &Plane,
    name: &str,
    text: Option<&str>,
    title: Option<&str>,
    no_sync: bool,
    now: Option<&str>,
) -> Result<Code, String> {
    let Some(text) = text.filter(|t| !t.is_empty()) else {
        return workspace_recall(plane, name, None);
    };
    let ws = plane.workspace(name).map_err(|e| e.to_string())?;
    let path = match ws.remember_titled(text, title, stamp(now)?) {
        Ok(path) => path,
        Err(e) => {
            voice::err(&e.to_string());
            return Ok(1);
        }
    };
    voice::ok(&format!(
        "Remembered in '{name}' → workspaces/{name}/memory/{}",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    if !plane.is_live(name) {
        voice::info(&format!(
            "  '{name}' is LOCAL (private) — memory stays on disk, not committed. Make it \
             shareable: charter workspace live {name}"
        ));
    } else if no_sync {
        voice::info("  (--no-sync) recorded locally; share it later with: charter save");
    } else {
        reactive(plane);
    }
    Ok(0)
}

/// `workspace recall` — search the journal, or list it in filename order.
pub fn workspace_recall(plane: &Plane, name: &str, query: Option<&str>) -> Result<Code, String> {
    let root = plane.root();
    let ws = plane.workspace(name).map_err(|e| e.to_string())?;
    let dir = ws.dir().join("memory");
    let query = query.filter(|q| !q.is_empty());
    let mut unread: Unread = Vec::new();
    let results: Vec<(PathBuf, String, usize)> = match query {
        Some(q) => memstore::search(root, &[dir], q, 8, &mut unread)
            .into_iter()
            .map(|(f, score)| (f.path, f.title, score))
            .collect(),
        None => memstore::gather(root, &[dir], &mut unread)
            .into_iter()
            .map(|f| (f.path, f.title, 0))
            .collect(),
    };
    voice::unread(root, &unread);
    if !unread.is_empty() {
        return Ok(1);
    }
    if results.is_empty() {
        match query {
            Some(q) => voice::info(&format!("No memories in '{name}' match '{q}'.")),
            None => voice::info(&format!(
                "workspace '{name}' has no memories yet. Add one: charter workspace remember \
                 \"<text>\""
            )),
        }
        return Ok(0);
    }
    let mut out = String::new();
    for (path, title, score) in &results {
        let tag = if *score > 0 {
            format!("  ({score})")
        } else {
            String::new()
        };
        out.push_str(&format!(
            "  • {title}{tag}  [{}]\n",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    print!("{out}");
    voice::info(&format!(
        "{} memory(ies) in workspaces/{name}/memory/ — read one: cat workspaces/{name}/memory/<file>",
        results.len()
    ));
    Ok(0)
}

/// `workspace forget` — delete one journal entry by slug or filename, and its index line.
pub fn workspace_forget(plane: &Plane, name: &str, slug: &str) -> Result<Code, String> {
    let ws = plane.workspace(name).map_err(|e| e.to_string())?;
    match memstore::forget(plane.root(), &ws.dir().join("memory"), slug) {
        Ok(()) => {
            voice::ok(&format!("Forgot '{slug}' from workspace '{name}'."));
            reactive(plane);
            Ok(0)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            voice::err(&format!(
                "no memory '{slug}' in workspace '{name}' (list them: charter workspace recall)."
            ));
            Ok(1)
        }
        // A slug is one file in the store. `../<elsewhere>` is a path, and the store's own
        // resolver is never handed one: this is where M1.1's round 3 found a slug deleting
        // a file outside the plane.
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {
            voice::err(&format!(
                "'{}' is not the slug of one memory — name a file in \
                 workspaces/{name}/memory/, not a path.",
                charter_core::shown::short(slug)
            ));
            Ok(1)
        }
        Err(e) => {
            voice::err(&e.to_string());
            Ok(1)
        }
    }
}

/// `workspace optimize` — curate one journal, or every one: the safe ops with `--apply`,
/// the rest as proposals. Read-only unless `--apply`, and a read-only run names what
/// `--apply` would do.
pub fn workspace_optimize(
    plane: &Plane,
    name: Option<&str>,
    apply: bool,
    stale_days: i64,
    now: Option<&str>,
) -> Result<Code, String> {
    let root = plane.root();
    let today = stamp(now)?.date();
    let names: Vec<String> = match name {
        Some(name) => {
            let exists = plane.workspace(name).is_ok_and(|ws| ws.dir().exists());
            if !exists {
                voice::err(&format!("no workspace '{name}'"));
                return Ok(1);
            }
            vec![name.to_string()]
        }
        None => {
            say_unread_workspaces(root);
            recall::read_workspaces(root)
                .map(|(names, _)| names)
                .map_err(|e| e.to_string())?
        }
    };
    if names.is_empty() {
        voice::info("No workspaces to optimize.");
        return Ok(0);
    }
    let mut actions_total = 0;
    let mut unread: Unread = Vec::new();
    for n in &names {
        let dir = root.join("workspaces").join(n).join("memory");
        if !dir.exists() {
            continue;
        }
        let rep = match charter_core::curate::report(root, &dir, stale_days, 0.5, today) {
            Ok(rep) => rep,
            Err(missed) => {
                voice::unread(root, &missed);
                unread.extend(missed);
                continue;
            }
        };
        if rep.total == 0 {
            continue;
        }
        println!(
            "\n◆ {n}  ({} memories · {} exact-dup group(s) · {} near-dup pair(s) · {} stale)",
            rep.total,
            rep.exact_dups.len(),
            rep.near_dups.len(),
            rep.stale.len()
        );
        let rep = if apply {
            let actions = match charter_core::curate::apply_safe(root, &dir, today) {
                Ok(actions) => actions,
                Err(missed) => {
                    voice::unread(root, &missed);
                    unread.extend(missed);
                    continue;
                }
            };
            for action in &actions {
                voice::ok(&format!("  auto: {action}"));
            }
            actions_total += actions.len();
            if !actions.is_empty() {
                reactive(plane);
            }
            match charter_core::curate::report(root, &dir, stale_days, 0.5, today) {
                Ok(rep) => rep,
                Err(missed) => {
                    voice::unread(root, &missed);
                    unread.extend(missed);
                    continue;
                }
            }
        } else {
            let pending = charter_core::curate::pending_auto(&rep);
            if !pending.is_empty() {
                println!("  would auto-apply (re-run with --apply):");
                for p in pending {
                    println!("    + {p}");
                }
            }
            rep
        };
        let proposals = charter_core::curate::proposals(&rep);
        if !proposals.is_empty() {
            println!("  proposals (not auto-applied — decide these yourself):");
            for p in proposals {
                println!("    ? {p}");
            }
        } else if apply {
            voice::info("  clean — nothing to propose.");
        }
    }
    if !apply {
        voice::info(
            "\nRead-only. Re-run with --apply to auto-apply the safe/reversible ops (exact-dup \
             collapse + index repair); proposals always stay manual.",
        );
    } else if actions_total == 0 {
        voice::info("\nNo safe ops to apply — the journal is already tidy.");
    }
    Ok(if unread.is_empty() { 0 } else { 1 })
}

// ---------------------------------------------------------------------------------------
// charter persona remember | recall

/// Refuse a persona this plane does not define, in charter's words; `true` to go on.
fn persona_ok(root: &Path, name: &str) -> bool {
    match charter_core::personas::name_refusal(root, name) {
        Some(refused) => {
            voice::err(&refused);
            false
        }
        None => true,
    }
}

/// `charter persona remember|recall`, with the persona resolved when it is not named.
///
/// **A silent exit 1 when nothing resolves, which is charter's own answer**:
/// `cmd_persona_remember` and `cmd_persona_recall` both open with
/// `if not name or not _require(name): return 1`, and the first half of that prints nothing.
/// Measured against the oracle on a plane with no front door. It reads like a defect and it
/// is charter's, so it is ported rather than improved here.
pub fn persona(here: &crate::Here, command: PersonaCommand) -> Result<Code, String> {
    let plane = &here.plane;
    match command {
        // Answered in `main` before a plane is even looked for.
        PersonaCommand::Gc { .. } | PersonaCommand::Secret(_) => {
            unreachable!("answered before run")
        }
        PersonaCommand::Current => {
            // `(none)` is the word charter prints, and it is NOT what it prints for a rung
            // that named a persona this plane does not have: THAT name is printed, because a
            // rung decided and is hiding every rung below it. A script reading this has
            // always been handed the name the ladder resolved.
            let found = here.active_persona(None);
            println!("{}", found.as_deref().unwrap_or("(none)"));
            Ok(0)
        }
        PersonaCommand::Clear => {
            let asking = here.asking(None, here.persona_env.as_deref());
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::select::clear(
                &asking, &mut sink,
            ))
        }
        PersonaCommand::Create {
            name,
            role,
            delegate_when,
            vault,
            extends,
            with_vault,
            select,
            force,
        } => {
            let root = plane.root();
            let state = charter_core::personaverbs::state_dir(root);
            let ask = charter_core::personaverbs::define::Create {
                name: &name,
                role: role.as_deref(),
                delegate_when: delegate_when.as_deref(),
                vault: vault.as_deref(),
                extends: extends.as_deref(),
                select: select.then_some(charter_core::personaverbs::define::Selecting {
                    ids: &here.ids,
                    env_persona: here.persona_env.as_deref(),
                }),
                force,
            };
            // A registration that fails says so in the vault command's own words, and the
            // persona stays made — Python's `create --with-vault` warns and exits 0 too.
            let mut register = |vault: &str| {
                crate::secret::add_persona_vault(here, vault, &name);
            };
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::define::create(
                root,
                &state,
                &ask,
                with_vault.then_some(&mut register as _),
                &mut sink,
            ))
        }
        PersonaCommand::Show { name } => {
            let root = plane.root();
            let state = charter_core::personaverbs::state_dir(root);
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::show::show(
                root,
                &state,
                &name,
                &session(),
                &mut sink,
            ))
        }
        PersonaCommand::Remove { name, force } => {
            let selection =
                charter_core::active::persona(&here.asking(None, here.persona_env.as_deref()));
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::define::remove(
                plane.root(),
                &name,
                force,
                &selection,
                &mut sink,
            ))
        }
        PersonaCommand::Lint { name, only } => {
            let root = plane.root();
            let state = charter_core::personaverbs::state_dir(root);
            let linter = charter_core::personaverbs::lint::Linter::new(root, &state);
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::lint::lint_command(
                &linter,
                name.as_deref(),
                only.as_deref(),
                &mut sink,
            ))
        }
        PersonaCommand::Default { name, clear } => {
            let mut sink = crate::speak;
            Ok(charter_core::personacmd::default_command(
                plane.root(),
                name.as_deref(),
                clear,
                &mut sink,
            ))
        }
        PersonaCommand::Remember {
            words,
            title,
            shared,
            ephemeral,
            no_sync,
            now,
        } => {
            // `[NAME] TEXT`, told apart by the SHAPE of the call: one word is the fact
            // and the persona is the active one, which is `name nargs="?"` in charter's own
            // parser. `num_args` has already refused nought and three.
            let (named, text) = match words.as_slice() {
                [text] => (None, text),
                [name, text] => (Some(name.as_str()), text),
                _ => unreachable!("clap takes one or two"),
            };
            let Some(name) = here.active_persona(named) else {
                return Ok(1);
            };
            persona_remember(
                plane,
                &name,
                text,
                title.as_deref(),
                shared,
                ephemeral,
                no_sync,
                stamp(now.as_deref())?,
            )
        }
        PersonaCommand::List => {
            let root = plane.root();
            let selection =
                charter_core::active::persona(&here.asking(None, here.persona_env.as_deref()));
            let state = charter_core::personaverbs::state_dir(root);
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::list::list(
                root, &state, &selection, &mut sink,
            ))
        }
        PersonaCommand::Use { name, now } => {
            let asking = charter_core::personaverbs::select::Asking {
                ids: &here.ids,
                env_persona: here.persona_env.as_deref(),
                bucket: &session(),
                now: stamp(now.as_deref())?,
            };
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::select::use_persona(
                plane.root(),
                &name,
                &asking,
                &mut sink,
            ))
        }
        PersonaCommand::SyncAgents {
            persona,
            approve_mcp,
            yes,
            dry_run,
        } => {
            let options = charter_core::personaverbs::agents::Options {
                persona: persona.as_deref(),
                approve_mcp,
                yes,
                dry_run,
            };
            let mut confirm = charter_core::personaverbs::agents::confirm_on_terminal();
            let ask = confirm
                .as_mut()
                .map(|f| f.as_mut() as charter_core::personaverbs::agents::Ask);
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::agents::sync_agents(
                plane.root(),
                &here.cwd,
                &options,
                ask,
                &mut sink,
            ))
        }
        PersonaCommand::Stats { name, recent_days } => {
            let mut sink = crate::speak;
            Ok(charter_core::personaverbs::stats::stats(
                plane.root(),
                name.as_deref(),
                recent_days,
                chrono::Local::now().date_naive(),
                &mut sink,
            ))
        }
        PersonaCommand::Recall { name, query, log } => {
            let Some(name) = here.active_persona(name.as_deref().filter(|n| !n.is_empty())) else {
                return Ok(1);
            };
            persona_recall(plane, &name, query.as_deref(), log)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn persona_remember(
    plane: &Plane,
    name: &str,
    text: &str,
    title: Option<&str>,
    shared: bool,
    ephemeral: bool,
    no_sync: bool,
    stamp: chrono::NaiveDateTime,
) -> Result<Code, String> {
    let root = plane.root();
    if !persona_ok(root, name) {
        return Ok(1);
    }
    let text = memstore::py_strip(text);
    if text.is_empty() {
        voice::err("empty memory");
        return Ok(1);
    }
    let title = charter_core::personas::memory_title(text, title);
    let owner = if shared {
        charter_core::contain::SHARED_PERSONA
    } else {
        name
    };
    let session = session();
    let dir = if ephemeral {
        recall::ephemeral_dir(root, &session, owner)
    } else {
        root.join("personas").join(owner).join("memory")
    };
    let kind = if ephemeral { "ephemeral" } else { "persistent" };
    let path = match memstore::write(
        root,
        &dir,
        text,
        Some(title.as_str()).filter(|t| !t.is_empty()),
        false,
        kind,
        !ephemeral,
        stamp,
    ) {
        Ok(path) => path,
        Err(e) => {
            voice::err(&e.to_string());
            return Ok(1);
        }
    };
    charter_core::trace::record(
        root,
        &session,
        "memory",
        &[
            ("persona", name),
            ("scope", if shared { "shared" } else { "own" }),
            ("kind", kind),
            ("title", title.as_str()),
        ],
        stamp,
    );
    let place = format!("{}{kind}", if shared { "shared " } else { "" });
    voice::ok(&format!(
        "Remembered ({place}) → {}",
        voice::rel(root, &path)
    ));
    if ephemeral {
        return Ok(0);
    }
    if no_sync {
        voice::info("  (--no-sync) recorded locally; share it later with: charter save");
        return Ok(0);
    }
    reactive(plane);
    Ok(0)
}

fn persona_recall(
    plane: &Plane,
    name: &str,
    query: Option<&str>,
    log: i64,
) -> Result<Code, String> {
    let root = plane.root();
    if !persona_ok(root, name) {
        return Ok(1);
    }
    let own = root.join("personas").join(name).join("memory");
    let shared = root
        .join("personas")
        .join(charter_core::contain::SHARED_PERSONA)
        .join("memory");
    let mut unread: Unread = Vec::new();

    if let Some(q) = query.filter(|q| !q.is_empty()) {
        let limit = if log != 0 { log } else { 8 };
        let found = memstore::search(root, &[own, shared], q, usize::MAX, &mut unread);
        let hits = recall::py_head(found, limit);
        voice::unread(root, &unread);
        let code = if unread.is_empty() { 0 } else { 1 };
        if hits.is_empty() {
            if unread.is_empty() {
                voice::info(&format!("no memory of '{q}' for '{name}'."));
            } else {
                voice::info(&format!(
                    "nothing matching '{q}' in the memory charter could read for '{name}'."
                ));
            }
            return Ok(code);
        }
        let mut out = format!("── memory matching '{q}' ({})\n", hits.len());
        for (found, score) in &hits {
            out.push_str(&format!(
                "  [{score:>3}] {}\n        {}\n        {}\n",
                found.title,
                voice::rel(root, &found.path),
                body_snippet(&found.path, false)
            ));
        }
        print!("{out}");
        return Ok(code);
    }

    let mut out = String::new();
    let mut printed = false;
    for (dir, label) in [
        (&own, name.to_string()),
        (&shared, "_shared (all personas)".to_string()),
    ] {
        let (mems, missed) = memstore::read_files(root, dir);
        unread.extend(missed);
        if mems.is_empty() {
            continue;
        }
        printed = true;
        out.push_str(&format!(
            "── persistent memory · {label} ({}) [{}/]\n",
            mems.len(),
            voice::rel(root, dir)
        ));
        out.push_str(&index_text(root, dir));
        out.push_str("\n\n");
    }
    let session = session();
    let mut scratch = Vec::new();
    for owner in [name, charter_core::contain::SHARED_PERSONA] {
        let (found, missed) =
            memstore::read_files(root, &recall::ephemeral_dir(root, &session, owner));
        scratch.extend(found);
        unread.extend(missed);
    }
    if !scratch.is_empty() {
        printed = true;
        out.push_str(&format!(
            "── ephemeral scratch · this session ({})\n",
            scratch.len()
        ));
        for path in &scratch {
            out.push_str(&format!(
                "- {}\n",
                path.file_stem().unwrap_or_default().to_string_lossy()
            ));
        }
        out.push('\n');
    }
    let acts = charter_core::trace::for_persona(root, &session, name, log);
    if !acts.is_empty() {
        printed = true;
        out.push_str(&format!(
            "── recent activity · this session ({})\n",
            acts.len()
        ));
        for act in &acts {
            let extra: Vec<String> = act
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "ts" | "event" | "persona"))
                .map(|(k, v)| format!("{k}={}", py_str(v)))
                .collect();
            let ts = act.get("ts").map(py_str).unwrap_or_default();
            let event = match act.get("event") {
                Some(serde_json::Value::Number(n)) => {
                    let n = n.to_string();
                    format!("{n:>10}")
                }
                Some(v) => format!("{:<10}", py_str(v)),
                None => format!("{:<10}", ""),
            };
            out.push_str(&format!("  {ts}  {event} {}\n", extra.join("  ")));
        }
    }
    print!("{out}");
    voice::unread(root, &unread);
    if !printed && unread.is_empty() {
        voice::info(&format!(
            "persona '{name}' has no memories yet. Add one: charter persona remember {name} \"<fact>\""
        ));
    }
    Ok(if unread.is_empty() { 0 } else { 1 })
}

/// A store's index as `persona recall` prints it: the whole file, stripped, or
/// `(no index)`.
///
/// **Gated where charter does not gate it.** charter reads the index by name, so a committed
/// `memory/MEMORY.md -> /elsewhere` is printed to whoever asked — the one read in this
/// command that `memstore.files` never covered. An index charter's gate refuses reads here
/// as having none.
fn index_text(root: &Path, dir: &Path) -> String {
    let index = dir.join(memstore::INDEX);
    if !memstore::readable_file(root, &index) {
        return "(no index)".to_string();
    }
    memstore::read_text(&index)
        .map(|t| memstore::py_strip(&t).to_string())
        .unwrap_or_else(|| "(no index)".to_string())
}

/// `str(value)` for a JSON value, as Python prints one it read with `json.loads` —
/// [`charter_core::pyrepr::str_json`].
///
/// **This crate had the second copy of the pair.** `charter-core`'s `inventory` had the other,
/// and the two parted on a number: this one wrote `n.to_string()`, which under
/// `arbitrary_precision` is the LITERAL the file held, so `1E5` was quoted back as `1E5` where
/// Python — which reads it as a float and prints its `repr` — writes `100000.0`.
fn py_str(value: &serde_json::Value) -> String {
    charter_core::pyrepr::str_json(value)
}
