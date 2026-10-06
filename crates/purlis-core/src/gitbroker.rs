//! A clone or a worktree a chat asks the app to make for it: **a brokered git action**
//! (ADR 0067 §2, #1335).
//!
//! A sandboxed chat may not write a clone's `.git/config`, its hooks, or the editor settings a
//! checkout carries (`.vscode/`, `.idea/`, `.claude/settings.json`), and `git clone` and
//! `git worktree add` write all of them. So `purlis clone` and `purlis worktree add`, run in a
//! chat the app started, hand the work to the app over the chat's hook socket
//! ([`crate::hookwire::Ask::Git`]), and the app runs **the same core functions** the terminal's
//! commands run — [`crate::repocmd::clone::clone`] and [`crate::piececmd::add`] — so git is
//! hardened the one way it always is ([`crate::worktree::git`]). The chat's own writes to those
//! files stay denied.
//!
//! What the app takes from the ask is checked against its own record of the chat, never the
//! ask's word:
//! - **the workspace** must be one the chat already writes its ordinary files in: the chat
//!   stands at the project root, or inside that workspace ([`bound`]);
//! - **the clone's host** must be one the chat's sandbox may reach, where the chat runs
//!   sandboxed ([`hosts`]);
//! - **the piece log** credits the chat and the persona the app recorded for it;
//! - **no configuration the chat could have shaped names a program** (D-1335-7): every git
//!   call reads no global or system file ([`git::Isolated`]), and a repository whose own
//!   config defines a driver, a helper or an include is refused before any git runs in it
//!   ([`runs_a_program`]).

use std::path::{Path, PathBuf};

use crate::hookwire::{Answer, GitAsk, GitWork};
use crate::pieces::Who;
use crate::repocmd::{self, Say};
use crate::worktree::git;

/// What the app knows of the chat that asks: its own record, never the ask's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asker {
    /// The app's number for the chat.
    pub chat: u32,
    /// Where the chat stands.
    pub cwd: Option<PathBuf>,
    /// The persona the app started it as.
    pub persona: Option<String>,
    /// Whether it runs a harness. A shell tab is the operator's own and is never sandboxed.
    pub harnessed: bool,
    /// Whether a person started this run of it without the sandbox (ruling V78 a).
    pub unsandboxed: bool,
    /// The machine store the app holds, whose device id names the piece log (FD-25), as the
    /// window's own pieces are named (`worktrees::window`); `None` on a machine that keeps none.
    pub config: Option<PathBuf>,
}

/// Runs `ask` for `asker` in the project at `root`, every git call reading `isolation`'s
/// configuration and nothing else, and answers every line it said with its exit status, or
/// why it was not run.
pub fn answer(
    root: &Path,
    asker: &Asker,
    ask: &GitAsk,
    isolation: &git::Isolated,
    now: chrono::DateTime<chrono::Utc>,
) -> Answer {
    git::isolated(isolation, || isolated_answer(root, asker, ask, now))
}

fn isolated_answer(
    root: &Path,
    asker: &Asker,
    ask: &GitAsk,
    now: chrono::DateTime<chrono::Utc>,
) -> Answer {
    let no = |why: String| Answer::No { why };
    if let Err(why) = bound(root, asker.cwd.as_deref(), &ask.workspace) {
        return no(why);
    }
    let hosts = match hosts(root, asker) {
        Ok(hosts) => hosts,
        Err(why) => return no(why),
    };
    let mut checked = Vec::new();
    for repo in touched(root, &ask.workspace, &ask.work) {
        match runs_a_program(&repo) {
            Ok(git_dir) => checked.push((repo, git_dir)),
            Err(why) => return no(why),
        }
    }
    let mut lines = Vec::new();
    let mut say = |line: Say| lines.push(line);
    let code = match &ask.work {
        GitWork::Clone { repos } => repocmd::clone::clone(
            &repocmd::clone::Request {
                root,
                ws: &ask.workspace,
                repos,
                now,
                author: &crate::wscmd::ensure::author(),
                hosts: hosts.as_deref(),
            },
            &mut say,
        ),
        GitWork::WorktreeAdd {
            repo,
            piece,
            branch,
        } => {
            let add = |say: &mut dyn FnMut(Say)| {
                crate::piececmd::add(
                    root,
                    &ask.workspace,
                    repo,
                    piece,
                    branch.as_deref(),
                    &who(asker),
                    now,
                    say,
                )
            };
            // The clone's git calls are given the git directory the check resolved, so the
            // worktree is cut from the repository that was checked (D-1335-9).
            match (git::isolation(), checked.first()) {
                (Some(held), Some((tree, git_dir))) => {
                    git::isolated(&held.pinned(tree, git_dir), || add(&mut say))
                }
                _ => add(&mut say),
            }
        }
    };
    Answer::Said {
        lines: fitted(lines, SAID_AT_MOST),
        code,
    }
}

/// Whether `workspace` is one a chat standing at `cwd` already writes its ordinary files in:
/// the chat stands at the project root, which writes every workspace, or inside `workspace`
/// itself (D-3). Anything else is refused with a sentence.
pub fn bound(root: &Path, cwd: Option<&Path>, workspace: &str) -> Result<(), String> {
    let Some(cwd) = cwd else {
        return Err(
            "the app has no record of where this chat stands, so it will not write a workspace \
             for it"
                .to_owned(),
        );
    };
    match crate::active::workspace_of_tree(root, cwd) {
        Some(own) if own == workspace => Ok(()),
        Some(own) => Err(format!(
            "this chat works in workspace '{own}', so the app will not write workspace \
             '{workspace}' for it. Run the command from a chat in '{workspace}', or at the \
             project root"
        )),
        None if same_dir(root, cwd) => Ok(()),
        None => Err(format!(
            "this chat stands outside the project's workspaces, so the app will not write \
             workspace '{workspace}' for it. Run the command from a chat in '{workspace}', or \
             at the project root"
        )),
    }
}

/// The hosts a clone made for `asker` may reach: its sandbox's egress where it runs sandboxed
/// (D-5), or `None`, no limit, where it does not — a shell tab, a chat a person started without
/// the sandbox, or a project that has not turned it on. A `charter.toml` that cannot be read
/// may turn it on, so it refuses rather than reading as no limit.
pub fn hosts(root: &Path, asker: &Asker) -> Result<Option<Vec<String>>, String> {
    if !asker.harnessed || asker.unsandboxed {
        return Ok(None);
    }
    let plane = crate::sandbox::Plane::read(root);
    if plane.unreadable() {
        return Err(crate::sandbox::NotStarted::PlaneUnreadable.to_string());
    }
    Ok(plane
        .said()
        .policy
        .map(|policy| crate::sandbox::hosts(&policy.egress, &plane)))
}

/// Who the piece log credits: the chat, by the app's number for it, and its persona (D-4).
pub fn who(asker: &Asker) -> Who {
    Who {
        session: Some(asker.chat.to_string()),
        persona: asker.persona.clone(),
        host: crate::dispatch::host(),
        log: crate::dispatch::log_name(asker.config.as_deref(), &crate::dispatch::host()),
    }
}

/// The most an answer's lines may take as JSON, under the 64 KiB an asker reads one answer in
/// (`hookwire::Asking::ask`), with room for the rest of the line.
pub const SAID_AT_MOST: usize = 56 * 1024;

/// The longest one line is kept, in characters, when the lines would not fit whole.
const A_LINE_AT_MOST: usize = 2000;

/// `lines`, cut to fit `budget` bytes of JSON: whole when they fit; otherwise each line held to
/// [`A_LINE_AT_MOST`] characters, then the last ones dropped for one warning that says how many.
/// A clone of many repos whose git said a great deal still answers, rather than an answer too
/// long for the asker to read reading as no answer at all.
pub fn fitted(lines: Vec<Say>, budget: usize) -> Vec<Say> {
    let size = |lines: &[Say]| serde_json::to_vec(lines).map_or(usize::MAX, |json| json.len());
    if size(&lines) <= budget {
        return lines;
    }
    let mut kept: Vec<Say> = lines.into_iter().map(shortened).collect();
    let mut cut = 0;
    let note = |cut: usize| {
        Say::Warn(format!(
            "{cut} more line(s) were cut here to fit the app's answer; the app's log has the run"
        ))
    };
    while !kept.is_empty() && size(&[kept.as_slice(), &[note(cut + 1)]].concat()) > budget {
        kept.pop();
        cut += 1;
    }
    if cut > 0 {
        kept.push(note(cut));
    }
    kept
}

fn shortened(line: Say) -> Say {
    let cut = |text: String| {
        if text.chars().count() <= A_LINE_AT_MOST {
            text
        } else {
            let mut short: String = text.chars().take(A_LINE_AT_MOST).collect();
            short.push_str(" …");
            short
        }
    };
    match line {
        Say::Info(text) => Say::Info(cut(text)),
        Say::Done(text) => Say::Done(cut(text)),
        Say::Warn(text) => Say::Warn(cut(text)),
        Say::Fail(text) => Say::Fail(cut(text)),
        Say::Plain(text) => Say::Plain(cut(text)),
        Say::Out(text) => Say::Out(cut(text)),
    }
}

/// The repositories the work runs git in that are there before it starts: for a worktree,
/// the clone it is cut from; for a clone, every clone of the workspace, which a clone wires
/// (and an asked repo that is already cloned). A clone made by this call is new, and git
/// writes its config.
fn touched(root: &Path, ws: &str, work: &GitWork) -> Vec<PathBuf> {
    match work {
        GitWork::WorktreeAdd { repo, .. } => {
            if crate::contain::repo_name_ok(repo) && crate::contain::workspace_name_ok(ws) {
                vec![root.join("workspaces").join(ws).join(repo)]
            } else {
                // `piececmd::add` refuses the name before git runs.
                Vec::new()
            }
        }
        GitWork::Clone { .. } => crate::repos::clones(root, ws)
            .map(|found| found.repos.into_iter().map(|repo| repo.path).collect())
            .unwrap_or_default(),
    }
}

/// Why `repo`'s own configuration names a program git could run for this call, or nothing.
///
/// Read as git resolves it — following an include, and a `commondir` to wherever it points —
/// under the isolation this thread holds, so only the repository's own scopes are listed
/// (`local`, `worktree`). Refused: a content filter (`filter.*`, which a committed
/// `.gitattributes` turns on at checkout and at status), a diff or merge driver
/// (`diff.*.textconv`, `diff.*.command`, `diff.external`, `merge.*.driver`), a transport
/// program (`core.sshCommand`, `core.gitProxy`, `remote.*.uploadpack`, `remote.*.receivepack`,
/// `core.alternateRefsCommand`), any `include` or `includeIf`, and a `credential` helper. A
/// configuration git cannot read is refused too: it may say any of these.
///
/// The one helper let through is the one purlis's own git policy writes into every clone it
/// makes (`credential.helper = !gh auth git-credential`, or `glab`'s; [`crate::gitpolicy`]),
/// which no git call made here reaches anyway: none of them but a clone crosses a network, and
/// a clone's call names its own helper.
///
/// **Which repository is checked is git's answer, not the path's** (D-1335-8): git is asked,
/// under the same isolation, which git directory it uses at `repo`, and anything but
/// `<repo>/.git` is refused, as is a `repo` with no `.git` at all. A bare repository a chat made
/// in its workspace, or a directory with none that discovery would take past, is never the one
/// the action runs in. Every repository checked is a clone (a worktree is cut from its clone),
/// so a linked worktree's git directory is never the answer. Answers the git directory it
/// checked, for the action to be pinned to.
pub fn runs_a_program(repo: &Path) -> Result<PathBuf, String> {
    let shown = repo.display();
    let not_a_clone = || {
        format!(
            "{shown} is not a clone with its own `.git`, so the app will not run git there for the \
             chat"
        )
    };
    let own = std::fs::canonicalize(repo.join(".git")).map_err(|_| not_a_clone())?;
    let resolved = git::run(repo, &["rev-parse", "--absolute-git-dir"], git::READ)
        .map_err(|e| e.to_string())?;
    let git_dir = std::fs::canonicalize(resolved.line()).map_err(|_| not_a_clone())?;
    if !resolved.ok() || git_dir != own {
        return Err(not_a_clone());
    }
    let listed = git::isolated(
        &git::isolation().unwrap_or_default().pinned(repo, &git_dir),
        || {
            git::run(
                repo,
                &["config", "--list", "--show-origin", "--show-scope", "-z"],
                git::READ,
            )
        },
    )
    .map_err(|e| e.to_string())?;
    if !listed.ok() {
        return Err(format!(
            "git could not read the configuration of {shown}, so the app will not run git there \
             for the chat: {}",
            listed.err.trim()
        ));
    }
    let mut fields = listed.out.split('\0');
    while let (Some(scope), Some(origin), Some(entry)) =
        (fields.next(), fields.next(), fields.next())
    {
        if scope == "command" {
            continue;
        }
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        if names_a_program(key) && !is_the_policys_helper(key, value) {
            return Err(format!(
                "{shown} sets `{key}` ({origin}), which names a program git would run outside \
                 the chat's sandbox, so the app will not run git there for the chat. Run the \
                 command in your own terminal, or remove the key"
            ));
        }
    }
    Ok(git_dir)
}

/// Whether config key `key` (as git lists it: section and name lowercased) names a program.
fn names_a_program(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    let (section, rest) = key.split_once('.').unwrap_or((key.as_str(), ""));
    let name = rest.rsplit('.').next().unwrap_or(rest);
    let sub = rest.contains('.');
    match section {
        "filter" | "include" | "includeif" => true,
        "diff" => key == "diff.external" || (sub && matches!(name, "textconv" | "command")),
        "merge" => sub && name == "driver",
        "core" => matches!(name, "sshcommand" | "gitproxy" | "alternaterefscommand"),
        "remote" => sub && matches!(name, "uploadpack" | "receivepack"),
        "credential" => name == "helper",
        _ => false,
    }
}

/// Whether `key = value` is the credential helper purlis's git policy writes into a clone.
fn is_the_policys_helper(key: &str, value: &str) -> bool {
    key.eq_ignore_ascii_case("credential.helper")
        && crate::forge::KINDS
            .iter()
            .any(|kind| value == format!("!{} auth git-credential", kind.cli()))
}

/// Whether two paths name one directory, links resolved.
fn same_dir(a: &Path, b: &Path) -> bool {
    let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    real(a) == real(b)
}

#[cfg(test)]
mod tests;
