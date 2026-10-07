//! Where a persona chat works: the asking chat's folder, another workspace, or a worktree of
//! its own on a branch of its own (#1453, spec #1434).
//!
//! A dispatch may say where with `--in`: `workspace:<name>` for another workspace of the
//! project, or `worktree` for a new worktree of the repo the asking chat works in. With
//! neither, the persona's own `dispatch-isolation: worktree` is its default, and without that
//! the chat works where the asking chat does.
//!
//! # What the ask chooses, and what it never does
//!
//! **A word, and nothing else.** The ask says `worktree`, or a workspace's name. It names no
//! folder, no repo and no branch: there is no field to name one in. Which repo a worktree is
//! cut from is the app's own record of where the asking chat stands ([`repo_of`]); what the
//! worktree's folder and branch are called is purlis's ([`piece_name`]), from the task's name
//! and the dispatch's own id, so two worktree tasks never share either; and a workspace is
//! looked up among the ones the project has, reached through no link ([`workspace_folder`]).
//!
//! # A worktree's life
//!
//! The app cuts it, never the chat, and by the brokered route
//! ([`crate::gitbroker::in_a_checked_clone`]): a sandboxed chat may not write a clone's
//! `.git`, and git run for it reads no configuration it could have shaped. The persona chat
//! starts in it and commits on its branch. **Nothing is ever merged for it.** The report names
//! the branch from the dispatch's record, which the app wrote.
//!
//! It goes one of two ways. The person discards its folder, from the window, after being
//! shown what goes with it ([`at_risk`], [`discard`]); its branch stays unless git finds it
//! merged, so no commit is lost by a discard (ADR 0072 §4: deleting a branch that holds work
//! is never purlis's act). Or purlis finds its branch merged and takes it away itself
//! ([`tidy`]): asked when its chat closes and when the project is opened, never on a timer,
//! and only by git's own safe removal, which leaves a folder holding anything uncommitted
//! exactly where it is.

use std::path::{Path, PathBuf};

use crate::chatpiece::{self, Cut, Naming};
use crate::dispatchrecord::{self, Record, Removed};
use crate::worktree::{self, git, name, standing};

/// `--in worktree`.
pub const WORKTREE: &str = "worktree";

/// What `--in` puts before a workspace's name.
pub const WORKSPACE: &str = "workspace:";

/// The persona key that makes a worktree its chats' default place to work.
pub const ISOLATION_KEY: &str = "dispatch-isolation";

/// What a dispatch asks for besides the asking chat's own folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Where {
    /// Another workspace of the project, by name.
    Workspace(String),
    /// A new worktree of the repo the asking chat works in.
    Worktree,
}

/// Why a persona chat is not started where it was asked to work. Each is a sentence the
/// asking chat reads ([`Refused::say`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// `--in` holds neither word.
    Word(String),
    /// What follows `workspace:` cannot name a workspace.
    WorkspaceName(String),
    /// The project has no such workspace.
    NoWorkspace(String),
    /// The workspace's folder is reached through a link, or lands outside the project.
    WorkspaceElsewhere(String),
    /// The asking chat does not stand in a repo, so there is nothing to cut a worktree of.
    NotInARepo,
    /// The worktree was not cut, and why, in the words of what refused it.
    Cut(String),
    /// A chat nobody is at asked to start a chat in another workspace, and no grant that
    /// already stands covers the pair (D-1453-16).
    NobodyToAsk {
        workspace: String,
        /// The asking chat's persona; none for a chat on no persona.
        asking: Option<String>,
        /// The persona asked for; none for a chat on no persona dispatching to none.
        target: Option<String>,
    },
}

impl Refused {
    /// The sentence the asking chat reads: what was refused, and what to do instead.
    pub fn say(&self) -> String {
        match self {
            Self::Word(word) => format!(
                "--in is `{WORKTREE}` or `{WORKSPACE}<name>`, not '{}'. Leave it out and the \
                 new chat works in this chat's folder.",
                crate::shown::short(word)
            ),
            Self::WorkspaceName(name) => format!(
                "'{}' cannot name a workspace, so no chat is started there. A workspace is \
                 named by its folder under `workspaces/`, and by nothing else.",
                crate::shown::short(name)
            ),
            Self::NoWorkspace(name) => format!(
                "this project has no workspace '{}'. List the workspaces with `purlis \
                 workspace list`, then dispatch into one of them.",
                crate::shown::short(name)
            ),
            Self::WorkspaceElsewhere(name) => format!(
                "workspace '{}' is reached through a link, or does not land inside this \
                 project, so no chat is started there.",
                crate::shown::short(name)
            ),
            Self::NotInARepo => "a worktree is cut from the repo the asking chat works in, and \
                                 this chat works in no repo's clone: this folder is not a git \
                                 repository purlis cuts worktrees of. Dispatch it from a chat \
                                 that works in a repo, or leave out `--in worktree` and the new \
                                 chat works in this chat's folder."
                .to_owned(),
            // What refused it may be git's own words, on several lines and with no full
            // stop: said here as one line that ends in one.
            Self::Cut(why) => format!(
                "purlis could not cut a worktree for this task: {}.",
                why.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim_end_matches('.')
            ),
            Self::NobodyToAsk {
                workspace,
                asking,
                target,
            } => {
                let workspace = crate::shown::short(workspace);
                let pair = match (asking, target) {
                    // Its own persona: no grant is ever kept for a persona's dispatch to
                    // itself, so there is none to point at and none to go and make.
                    (Some(asking), Some(target)) if asking == target => {
                        return format!(
                            "this chat runs with its harness's permission prompts off, so \
                             nobody is here to answer for it, and such a chat starts no chat \
                             as its own persona in another workspace ('{workspace}'): a \
                             chat's own persona needs no grant, so none can stand for it, \
                             and with nobody here that rule does not carry a chat into \
                             another workspace. The person can start it from this chat's \
                             tab. Leave out `--in` and the new chat works in this chat's \
                             folder."
                        );
                    }
                    (Some(asking), Some(target)) => format!(
                        "no grant that already stands lets {} chats dispatch to {}",
                        crate::shown::short(asking),
                        crate::shown::short(target)
                    ),
                    _ => "it runs as no persona, which no standing grant covers".to_owned(),
                };
                format!(
                    "this chat runs with its harness's permission prompts off, so nobody is \
                     here to answer for it, and such a chat starts a chat in another workspace \
                     ('{workspace}') only under a grant that already stands: {pair}. Only a \
                     person makes one, for themselves on this machine or for this project; {} \
                     lists the grants that stand. Until then, leave out `--in` and the new \
                     chat works in this chat's folder.",
                    crate::dispatchunattended::SETTINGS
                )
            }
        }
    }
}

impl Refused {
    /// The same refusal as the **person** reads it, where they chose the place in the window's
    /// "Ask <persona>…" dialog: of a branch of its own and its folder (ADR 0072 §4), and with
    /// no flag of a command in it.
    pub fn in_window(&self) -> String {
        match self {
            Self::Word(word) => format!(
                "'{}' is not a place a chat can be started in.",
                crate::shown::short(word)
            ),
            Self::WorkspaceName(name) => format!(
                "'{}' cannot name a workspace, so no chat was started there.",
                crate::shown::short(name)
            ),
            Self::NoWorkspace(name) => format!(
                "This project has no workspace '{}', so no chat was started there.",
                crate::shown::short(name)
            ),
            Self::WorkspaceElsewhere(name) => format!(
                "Workspace '{}' is reached through a link, or does not land inside this \
                 project, so no chat was started there.",
                crate::shown::short(name)
            ),
            Self::NotInARepo => "That chat works in no repo, so there is none to cut a branch \
                                 of its own from. Choose that chat's folder, or ask from a chat \
                                 that works in a repo."
                .to_owned(),
            Self::Cut(why) => format!(
                "purlis could not cut a branch for this task: {}.",
                why.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim_end_matches('.')
            ),
            // The person at a chat's tab is at it: this is never theirs to read.
            Self::NobodyToAsk { .. } => self.say(),
        }
    }
}

/// What `--in` asks for, or nothing where it was left out.
pub fn asked(raw: Option<&str>) -> Result<Option<Where>, Refused> {
    let Some(word) = raw.map(str::trim).filter(|word| !word.is_empty()) else {
        return Ok(None);
    };
    if word == WORKTREE {
        return Ok(Some(Where::Worktree));
    }
    let Some(named) = word.strip_prefix(WORKSPACE) else {
        return Err(Refused::Word(word.to_owned()));
    };
    if !crate::contain::workspace_name_ok(named) {
        return Err(Refused::WorkspaceName(named.to_owned()));
    }
    Ok(Some(Where::Workspace(named.to_owned())))
}

/// Whether the definition of `persona`, with its chain applied, makes a worktree its chats'
/// default place to work (`dispatch-isolation: worktree`).
pub fn isolates(root: &Path, persona: &str) -> bool {
    crate::personagrant::resolve(root, persona).is_some_and(|resolved| {
        resolved
            .get(ISOLATION_KEY)
            .is_some_and(|value| crate::memstore::py_strip(value) == WORKTREE)
    })
}

/// The workspace `name` asks for, **by the name its own folder has**, and that folder, for a
/// chat to start in: a name that can be one, a folder that is there, and no link on the way to
/// it ([`worktree::confine::workspace_dir`]).
///
/// **The name answered is the directory's own entry, never the one asked.** On a volume that
/// folds case, `BETA` opens the folder `beta`; the limits, the record, the badge and where the
/// chat is filed are all looked up by name, so each would miss `beta`'s under `BETA`. So the
/// entry under `workspaces/` is read, and its name is the one used from here on.
pub fn workspace_folder(root: &Path, name: &str) -> Result<(String, PathBuf), Refused> {
    use worktree::confine::Outside;
    let refused = |outside: Outside, name: &str| match outside {
        Outside::NotAWorkspaceName(_) => Refused::WorkspaceName(name.to_owned()),
        Outside::NoWorkspace { .. } => Refused::NoWorkspace(name.to_owned()),
        Outside::NotInPlane { .. }
        | Outside::ThroughALink { .. }
        | Outside::WalksUp { .. }
        | Outside::NotInWorkspace { .. } => {
            // A folder that is simply not there reads as a link nowhere to the walk; said as
            // what it is.
            if root
                .join("workspaces")
                .join(name)
                .symlink_metadata()
                .is_err()
            {
                Refused::NoWorkspace(name.to_owned())
            } else {
                Refused::WorkspaceElsewhere(name.to_owned())
            }
        }
    };
    let folder =
        worktree::confine::workspace_dir(root, name).map_err(|outside| refused(outside, name))?;
    let own = own_entry(root, name).ok_or_else(|| Refused::NoWorkspace(name.to_owned()))?;
    if own == name {
        return Ok((own, folder));
    }
    // Asked again of the entry's own name, which is the one every later read uses.
    worktree::confine::workspace_dir(root, &own)
        .map(|folder| (own.clone(), folder))
        .map_err(|outside| refused(outside, &own))
}

/// The entry under `workspaces/` that `name` opens: `name` itself where a folder is called
/// exactly that, else the one entry that differs from it only in the case of its letters, on a
/// volume that folds case. `None` where the directory cannot be read or holds neither.
fn own_entry(root: &Path, name: &str) -> Option<String> {
    let entries: Vec<String> = std::fs::read_dir(root.join("workspaces"))
        .ok()?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    if entries.iter().any(|entry| entry == name) {
        return Some(name.to_owned());
    }
    let mut folded = entries.into_iter().filter(|entry| {
        entry.eq_ignore_ascii_case(name) && crate::contain::workspace_name_ok(entry)
    });
    match (folded.next(), folded.next()) {
        (Some(one), None) => Some(one),
        _ => None,
    }
}

/// A repo's clone in a workspace: what a worktree is cut from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub workspace: String,
    pub repo: String,
}

/// The repo a chat standing at `cwd` works in: the clone it stands in or below, or the clone
/// the worktree it stands in was cut from. Path arithmetic and one existence check, as
/// [`chatpiece::clone_at`] is; a chat at the project's root, in a workspace's own folder or
/// outside the project works in none.
pub fn repo_of(root: &Path, cwd: Option<&Path>) -> Result<Repo, Refused> {
    let cwd = cwd.ok_or(Refused::NotInARepo)?;
    if let Some(found) = worktree::locate(root, cwd) {
        return Ok(Repo {
            workspace: found.workspace,
            repo: found.repo,
        })
        .and_then(|repo| is_a_clone(root, repo));
    }
    let here = std::fs::canonicalize(cwd).map_err(|_| Refused::NotInARepo)?;
    let workspaces =
        std::fs::canonicalize(root.join("workspaces")).map_err(|_| Refused::NotInARepo)?;
    let rest = here
        .strip_prefix(&workspaces)
        .map_err(|_| Refused::NotInARepo)?;
    let mut parts = rest.components().map(|part| part.as_os_str().to_str());
    let (Some(Some(ws)), Some(Some(repo))) = (parts.next(), parts.next()) else {
        return Err(Refused::NotInARepo);
    };
    is_a_clone(
        root,
        Repo {
            workspace: ws.to_owned(),
            repo: repo.to_owned(),
        },
    )
}

/// `repo` where its names can be a workspace's and a repo's and its clone holds a `.git`.
fn is_a_clone(root: &Path, repo: Repo) -> Result<Repo, Refused> {
    let named = crate::contain::workspace_name_ok(&repo.workspace)
        && crate::contain::repo_name_ok(&repo.repo)
        && repo.repo != worktree::DIR_NAME;
    let there = root
        .join("workspaces")
        .join(&repo.workspace)
        .join(&repo.repo)
        .join(".git")
        .symlink_metadata()
        .is_ok();
    if named && there {
        Ok(repo)
    } else {
        Err(Refused::NotInARepo)
    }
}

/// How many characters of a dispatch's id end its worktree's name: forty bits of the id's
/// random half, which no two dispatches of a project are expected to share.
const ID_TAIL: usize = 8;

/// What a task's name becomes where nothing of it can be part of a branch's name.
const UNNAMED: &str = "task";

/// **The name of a worktree task's folder and of its branch**: the task's name as a branch
/// name can carry it ([`name::slug`]), then the end of the dispatch's own id. `id` is the
/// dispatch record's ULID ([`dispatchrecord::mint`]).
///
/// purlis's own, whole: nothing a chat sends is read as a path or a branch here. A task's name
/// is a chat's word, and the slug keeps of it only letters, digits, `.`, `_` and `-`, so a
/// name holding `/` or `..` names a folder beside its siblings and nowhere else. The id's tail
/// is what makes two tasks called the same two folders and two branches.
pub fn piece_name(task: &str, id: &str) -> String {
    let tail = id_tail(id);
    let said = name::slug(task).unwrap_or_else(|| UNNAMED.to_owned());
    let named = format!("{said}-{tail}");
    // A task called `aux.rs fix` slugs to a name whose stem before its first dot is a device
    // on Windows, which the cut refuses. The task is not refused for what it was called: its
    // folder is named for the dispatch alone.
    if crate::contain::mintable(&named).is_ok() {
        named
    } else {
        format!("{UNNAMED}-{tail}")
    }
}

/// The end of a dispatch's id as its worktree's name carries it: its last [`ID_TAIL`]
/// characters, lowercased.
fn id_tail(id: &str) -> String {
    id.chars()
        .rev()
        .take(ID_TAIL)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Where a persona chat is to work, resolved against the project: every question that needs
/// no git, asked before anything is decided or cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ground {
    /// The asking chat's own folder.
    Asker {
        /// Set where the persona's own default was a worktree and the asking chat works in no
        /// repo: the default gives way, and the asking chat is told.
        fell_back: bool,
    },
    /// Another workspace's folder.
    Workspace { name: String, folder: PathBuf },
    /// A new worktree of this repo.
    Worktree(Repo),
}

impl Ground {
    /// The workspace the chat will work in, where that is not simply the asking chat's.
    pub fn workspace(&self) -> Option<&str> {
        match self {
            Self::Asker { .. } => None,
            Self::Workspace { name, .. } => Some(name),
            Self::Worktree(repo) => Some(&repo.workspace),
        }
    }
}

/// Where the dispatch `asked` for, to `persona`, from a chat standing at `cwd`, comes to.
///
/// **What the dispatch names wins; the persona's default only fills its silence.** A worktree
/// the dispatch asked for and that cannot be cut is a refusal. A worktree that is only the
/// persona's default, for an asking chat that works in no repo, gives way to the asking chat's
/// folder: a default that refused every dispatch from a chat outside a repo would be a persona
/// nobody at the project's root could reach.
pub fn ground(
    root: &Path,
    asked: Option<Where>,
    persona: Option<&str>,
    cwd: Option<&Path>,
) -> Result<Ground, Refused> {
    match asked {
        Some(Where::Workspace(asked)) => {
            let (name, folder) = workspace_folder(root, &asked)?;
            Ok(Ground::Workspace { name, folder })
        }
        Some(Where::Worktree) => repo_of(root, cwd).map(Ground::Worktree),
        None if persona.is_some_and(|persona| isolates(root, persona)) => {
            Ok(match repo_of(root, cwd) {
                Ok(repo) => Ground::Worktree(repo),
                Err(_) => Ground::Asker { fell_back: true },
            })
        }
        None => Ok(Ground::Asker { fell_back: false }),
    }
}

/// What the asking chat is told where the persona's default worktree gave way.
pub fn fell_back_note(persona: &str) -> String {
    format!(
        "persona '{}' works in a worktree of its own by default, and this chat works in no \
         repo to cut one from, so the new chat works in this chat's folder",
        crate::shown::short(persona)
    )
}

/// Cuts the worktree of the task named `task`, dispatch `id`, off `repo`'s clone: purlis's
/// own name for the folder and the branch ([`piece_name`]), used exactly or refused, by the
/// brokered route.
///
/// **Never the next free name.** A folder or a branch already there under that name is a
/// refusal ([`worktree::Refusal::BranchTaken`], or git's own for the folder): the name holds
/// this dispatch's id, so something already carrying it is not a coincidence to step around.
pub fn cut(
    root: &Path,
    repo: &Repo,
    task: &str,
    id: &str,
    isolation: &git::Isolated,
) -> Result<Cut, Refused> {
    let piece = piece_name(task, id);
    // Asked before git is: `git worktree add` into a folder that is there and empty succeeds,
    // and a folder purlis did not make is not one it starts a chat in.
    let taken = worktree::path_for(root, &repo.workspace, &repo.repo, &piece)
        .map_err(|refusal| Refused::Cut(refusal.in_window()))?;
    if taken.symlink_metadata().is_ok() {
        return Err(Refused::Cut(format!(
            "something is already at the folder '{piece}' would take in {}, and purlis did \
             not put it there. Dispatch the task again: its worktree is named for the \
             dispatch, so the next one has another name.",
            repo.repo
        )));
    }
    crate::gitbroker::in_a_checked_clone(root, &repo.workspace, &repo.repo, isolation, || {
        chatpiece::cut(root, &repo.workspace, &repo.repo, &Naming::Exactly(piece))
    })
    .map_err(Refused::Cut)?
    .map_err(|refusal| Refused::Cut(refusal.in_window()))
}

/// Takes back a worktree whose chat did not start, by the route it was cut
/// ([`chatpiece::undo`]): git's safe removal, so anything that did land in it stays.
pub fn take_back(
    root: &Path,
    cut: &Cut,
    isolation: &git::Isolated,
) -> Result<chatpiece::Undone, String> {
    let (ws, repo, piece) = (&cut.workspace, &cut.repo, &cut.piece);
    crate::gitbroker::in_a_checked_folder(root, ws, repo, piece, isolation, || {
        chatpiece::undo(root, cut)
    })
    .map_err(|not_run| NotDone::from(not_run).in_window(repo))?
    .map_err(|refusal| refusal.in_window())
}

/// Whether a chat a dispatch starts in the project at `root` starts sandboxed: the project's
/// sandbox is in force under this machine's policy. A persona chat never carries an opt-out
/// ([`crate::dispatchgrant::grants_for_a_dispatched_chat`]), so the project's answer is its.
pub fn starts_sandboxed(root: &Path) -> bool {
    crate::sandbox::Plane::read(root)
        .in_force(&crate::sandbox::policy::Locks::of(root))
        .is_some()
}

/// The command a sandboxed chat commits with in a worktree, where its own `git commit` is
/// refused (#1055): the app commits for it.
pub const COMMITS_WITH: &str = "purlis worktree commit";

/// What the persona chat is told under its stamp: where it works and that nothing merges.
///
/// **Where it starts `sandboxed` it is told how it commits** (#1453 review, M3; #1055): a
/// worktree's git data is in its repo's `.git`, outside the folder a sandboxed chat may write,
/// so its own `git commit` is refused and the app commits for it. Telling it only to commit
/// would send it into a refusal it cannot act on.
pub fn told_the_chat(repo: &str, piece: &str, sandboxed: bool) -> String {
    if sandboxed {
        return format!(
            "you work in a worktree of {repo} that purlis cut for this task, on the branch \
             `{piece}`, which is yours alone. This chat is sandboxed, and a worktree's git \
             data is outside the folder it may write, so `git add` and `git commit` are \
             refused here: commit your work with `{COMMITS_WITH} -m \"<message>\" --all` (or \
             paths in place of `--all`; a new file must be named). It only commits. Nothing \
             is merged for you: your report names the branch, and merging it is the asking \
             chat's or the person's decision"
        );
    }
    format!(
        "you work in a worktree of {repo} that purlis cut for this task, on the branch \
         `{piece}`, which is yours alone. Commit your work on it. Nothing is merged for you: \
         your report names the branch, and merging it is the asking chat's or the person's \
         decision"
    )
}

/// What a persona chat started in another workspace than its asker's is told under its stamp
/// (D-1453-28). The stamp says who asked and where **that** chat works; this says where
/// **this** one does, so neither workspace is taken for the other. `asker` is the asking
/// chat's workspace, or none for one at the project's root.
pub fn told_of_its_workspace(name: &str, asker: Option<&str>) -> String {
    let name = crate::shown::short(name);
    let asked_from = match asker {
        Some(asker) => format!("workspace '{}'", crate::shown::short(asker)),
        None => "the project's root".to_owned(),
    };
    format!(
        "you work in workspace '{name}': its folder, its todos, its memory and its session \
         records are the ones you use. The chat that asked works in {asked_from}, and your \
         report goes to it there"
    )
}

/// What the asking chat is told of where the new chat works, after "It works". `sandboxed` is
/// whether the new chat starts sandboxed, which decides what a worktree task can leave behind
/// ([`told_the_chat`]).
pub fn said_to_the_asker(ground: &Ground, cut: Option<&Cut>, sandboxed: bool) -> String {
    match (ground, cut) {
        (_, Some(cut)) => {
            let from = match &cut.base {
                worktree::Base::Branch(base) => base.clone(),
                worktree::Base::Detached(sha) => format!("commit {}", &sha[..sha.len().min(12)]),
            };
            let leaves = if sandboxed {
                format!(
                    "It is sandboxed, so it commits there with `{COMMITS_WITH}`, which the \
                     app runs for it. Nothing is merged for it: its report names the branch, \
                     and merging is yours or the person's decision"
                )
            } else {
                "Nothing is merged for it: its report names the branch, and merging is yours \
                 or the person's decision"
                    .to_owned()
            };
            format!(
                "in a worktree of its own, on the branch `{}` in {}, cut from {from}. {leaves}",
                cut.branch, cut.repo
            )
        }
        (Ground::Workspace { name, .. }, None) => format!(
            "in workspace '{name}', with that workspace's todos, memory and session records"
        ),
        (Ground::Asker { .. } | Ground::Worktree(_), None) => "in this chat's folder".to_owned(),
    }
}

/// **Why a chat nobody is at may not start a chat in another workspace** (D-1453-16), or
/// `None` where it may: a grant that already stands, the person's on this machine or the
/// project's acknowledged one, names the pair. A grant made for one chat does not count, and
/// neither does the rule that a chat's own persona needs none: with nobody to see it, a chat
/// confined to one workspace is not let into another on that rule alone.
///
/// **So its own persona never crosses unattended.** No grant is kept for a persona's dispatch
/// to itself ([`crate::dispatchgrant::Pair::new`]), so none can stand for that pair, and a
/// pair of one persona is refused here whatever `grants` holds. The person starts such a chat
/// from the asking chat's tab.
pub fn nobody_to_ask(
    asking: Option<&str>,
    target: Option<&str>,
    grants: &crate::dispatchgrant::InForce,
    workspace: &str,
) -> Option<Refused> {
    use crate::sandbox::grant::Level;
    let stands = target.is_some_and(|target| {
        asking != Some(target)
            && matches!(
                grants.level_of(asking, target),
                Some(Level::You | Level::Project)
            )
    });
    (!stands).then(|| Refused::NobodyToAsk {
        workspace: workspace.to_owned(),
        asking: asking.map(str::to_owned),
        target: target.map(str::to_owned),
    })
}

// ---------------------------------------------------------------------------------------
// a worktree's end
// ---------------------------------------------------------------------------------------

/// A dispatch's worktree, as its record names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub workspace: String,
    pub repo: String,
    pub piece: String,
    /// The branch purlis cut it on.
    pub branch: Option<String>,
}

impl Tree {
    /// The worktree `record` names, where it names one whose names can be a workspace's, a
    /// repo's and a piece's: a record is a file on the disk, and its names are about to be
    /// joined into a path.
    ///
    /// **And only the worktree purlis cut for that dispatch.** Its folder's name ends with the
    /// end of the record's own id and its branch is that name ([`piece_name`]), so a record
    /// that names another branch folder of the clone, a writing chat's or one cut by hand, is
    /// no tree purlis looks at or offers to discard.
    pub fn of(record: &Record) -> Option<Self> {
        let tree = record.place.worktree.as_ref()?;
        let workspace = record.place.workspace.as_deref()?;
        let named = crate::contain::workspace_name_ok(workspace)
            && crate::contain::repo_name_ok(&tree.repo)
            && name::piece_name_ok(&tree.piece)
            && tree.piece.ends_with(&format!("-{}", id_tail(&record.id)))
            && tree.branch.as_deref() == Some(tree.piece.as_str());
        named.then(|| Self {
            workspace: workspace.to_owned(),
            repo: tree.repo.clone(),
            piece: tree.piece.clone(),
            branch: tree.branch.clone(),
        })
    }

    /// Its folder in the project at `root`.
    pub fn folder(&self, root: &Path) -> Option<PathBuf> {
        worktree::path_for(root, &self.workspace, &self.repo, &self.piece).ok()
    }

    /// Whether its folder is still there. A read of the folder, and no git.
    pub fn there(&self, root: &Path) -> bool {
        self.folder(root)
            .is_some_and(|path| path.symlink_metadata().is_ok())
    }

    /// Whether a chat standing at `cwd` stands in it.
    pub fn holds(&self, root: &Path, cwd: &Path) -> bool {
        self.folder(root).is_some_and(|folder| {
            let real = |path: &Path| std::fs::canonicalize(path).unwrap_or(path.to_path_buf());
            real(cwd).starts_with(real(&folder))
        })
    }
}

/// How a dispatch's worktree stands, for the row that lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Its folder is there and nothing has merged it: listed, with Discard.
    Kept,
    /// purlis found its branch merged and took it away.
    Merged,
    /// purlis found its branch merged and took its folder away; git kept the branch.
    MergedBranchKept,
    /// The person discarded its folder.
    Discarded,
    /// Its folder is gone, and not by either of those: removed from the explorer or by hand.
    Gone,
}

impl Standing {
    /// The word a row and a list say.
    pub fn word(self) -> &'static str {
        match self {
            Self::Kept => "kept",
            Self::Merged => "merged",
            Self::MergedBranchKept => "merged-branch-kept",
            Self::Discarded => "discarded",
            Self::Gone => "gone",
        }
    }
}

impl Standing {
    /// What a chat's list of its tasks says of the branch (`purlis dispatch list`).
    pub fn said(self) -> &'static str {
        match self {
            Self::Kept => "its worktree is kept",
            Self::Merged => "merged, and its worktree removed",
            Self::MergedBranchKept => "merged, its worktree removed and the branch kept",
            Self::Discarded => "its worktree was discarded",
            Self::Gone => "its worktree was removed",
        }
    }
}

/// How the worktree of `record` stands, or `None` for a dispatch that had none.
pub fn standing(root: &Path, record: &Record) -> Option<Standing> {
    let recorded = record.place.worktree.as_ref()?;
    Some(match recorded.removed {
        Some(Removed::Merged) => Standing::Merged,
        Some(Removed::MergedBranchKept) => Standing::MergedBranchKept,
        Some(Removed::Discarded) => Standing::Discarded,
        None if Tree::of(record).is_some_and(|tree| tree.there(root)) => Standing::Kept,
        None => Standing::Gone,
    })
}

/// Why git was not run in a worktree's repo, or what it refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotDone {
    /// The brokered route will not run git in the repo at all: its own git settings name a
    /// program ([`crate::gitbroker::runs_a_program`]). The broker's sentence, for the log.
    Repo(String),
    /// git ran and refused, in the window's words.
    Git(String),
}

impl From<crate::gitbroker::NotRun> for NotDone {
    fn from(not_run: crate::gitbroker::NotRun) -> Self {
        match not_run {
            crate::gitbroker::NotRun::Repo(why) => Self::Repo(why),
            crate::gitbroker::NotRun::Folder(not_its) => Self::Git(not_its.in_window()),
        }
    }
}

impl NotDone {
    /// The sentence the person reads in the window, of the branch's folder in `repo`. The
    /// broker's own sentence names a command to run and a path, which is for a chat at a
    /// command line: the window says what stands in the way and the way out it has.
    pub fn in_window(&self, repo: &str) -> String {
        match self {
            Self::Repo(_) => format!(
                "purlis will not run git in {repo} for this: the repo's own git settings name \
                 a program, which git would run outside any sandbox. Remove the folder from \
                 its branch's row in the explorer instead, or take that setting out of the repo."
            ),
            Self::Git(why) => why.clone(),
        }
    }
}

/// `risk` without the paths that are purlis's own in `folder`: the layer it writes into a
/// chat's folder and hides through the repo's exclude file ([`crate::guest::hidden_by_purlis`]).
/// What is left under `ignored` is the task's.
fn without_purlis_own(folder: &Path, mut risk: standing::AtRisk) -> standing::AtRisk {
    let hidden = crate::guest::hidden_by_purlis(folder);
    if let Some(ignored) = risk.ignored.as_mut() {
        ignored.retain(|path| !crate::guest::is_purlis_own(&hidden, path));
    }
    risk
}

/// What discarding `tree`'s folder would take with it, read under the brokered route's rules
/// and nothing removed: every uncommitted file by its own path, and every ignored path that is
/// not purlis's own layer. `Ok(None)` for a folder that is not there.
pub fn at_risk(
    root: &Path,
    tree: &Tree,
    isolation: &git::Isolated,
) -> Result<Option<standing::AtRisk>, NotDone> {
    let (ws, repo, piece) = (&tree.workspace, &tree.repo, &tree.piece);
    // What is purlis's own there is read through the folder too, so inside the same check.
    crate::gitbroker::in_a_checked_folder(root, ws, repo, piece, isolation, || {
        standing::at_risk(root, ws, repo, piece).map(|read| {
            read.map(|risk| match tree.folder(root) {
                Some(folder) => without_purlis_own(&folder, risk),
                None => risk,
            })
        })
    })?
    .map_err(|refusal| NotDone::Git(refusal.in_window()))
}

/// What a discard did to the branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discarded {
    /// git found the branch merged, and it went with its folder.
    BranchGone,
    /// The branch stays: it holds a commit git does not find merged, or the folder was on none.
    BranchKept,
}

/// **Discards `tree`'s folder, whatever is in it.** The person's own act, from the window,
/// after [`at_risk`] put what goes with it in front of them: this is the forced removal of a
/// folder, and nothing else in purlis calls it.
///
/// **The branch purlis cut loses no commit by it.** That branch is deleted only where git
/// finds it merged ([`standing::drop_if_merged`]); one that holds work stays, an ordinary
/// branch of the repo. Commits made in the folder on no branch at all are the one thing a
/// discard does lose, and [`at_risk`] names them.
pub fn discard(root: &Path, tree: &Tree, isolation: &git::Isolated) -> Result<Discarded, NotDone> {
    let (ws, repo, piece) = (&tree.workspace, &tree.repo, &tree.piece);
    crate::gitbroker::in_a_checked_folder(root, ws, repo, piece, isolation, || {
        worktree::remove(root, &tree.workspace, &tree.repo, &tree.piece, true, false).map(|_| {
            let gone = tree.branch.as_deref().is_some_and(|branch| {
                standing::drop_if_merged(root, &tree.workspace, &tree.repo, branch)
            });
            if gone {
                Discarded::BranchGone
            } else {
                Discarded::BranchKept
            }
        })
    })?
    .map_err(|refusal| NotDone::Git(refusal.in_window()))
}

/// What a look at a finished worktree did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tidied {
    /// Its branch had landed where it was cut from, and its folder and branch are gone.
    Removed,
    /// Its branch had landed and its folder is gone; git would not delete the branch (the
    /// clone is not on the branch it landed in), so the branch stays.
    FolderRemoved,
    /// It stays: not merged, not readable, or holding something that is not committed.
    Kept,
}

/// **Takes `tree` away where its branch is merged and its folder holds nothing else**, and
/// only then:
///
/// - every commit of the branch purlis cut is in the branch it was cut from
///   ([`standing::landed`]); and
/// - the folder holds no uncommitted path **and no ignored one that is not purlis's own
///   layer** (D-1453-10 as amended in review, M4). git's safe removal does not count ignored
///   files and deletes them, and what a task leaves there may be all it produced: a results
///   folder, a database file, a `.env` it wrote to run tests.
///
/// Anything else leaves it exactly as it is, listed with Discard, where the person is shown
/// what is in it. A branch nothing was committed on has landed by the first rule, so a task
/// that left nothing at all leaves no folder behind once its chat is closed.
pub fn tidy(root: &Path, tree: &Tree, isolation: &git::Isolated) -> Tidied {
    let (Some(branch), Some(folder)) = (tree.branch.as_deref(), tree.folder(root)) else {
        return Tidied::Kept;
    };
    let (ws, repo, piece) = (&tree.workspace, &tree.repo, &tree.piece);
    crate::gitbroker::in_a_checked_folder(root, ws, repo, piece, isolation, || {
        if !matches!(
            standing::landed(root, ws, repo, piece, branch),
            standing::Landed::Yes { .. }
        ) {
            return Tidied::Kept;
        }
        let Ok(Some(risk)) = standing::at_risk(root, ws, repo, piece) else {
            return Tidied::Kept;
        };
        let risk = without_purlis_own(&folder, risk);
        let clean = |paths: &Option<Vec<String>>| paths.as_ref().is_some_and(Vec::is_empty);
        if !clean(&risk.changes) || !clean(&risk.ignored) {
            return Tidied::Kept;
        }
        match worktree::remove(root, ws, repo, piece, false, true) {
            Ok(removed) if removed.branch_deleted => Tidied::Removed,
            Ok(_) => Tidied::FolderRemoved,
            Err(_) => Tidied::Kept,
        }
    })
    .unwrap_or(Tidied::Kept)
}

/// [`tidy`] for the dispatch `record`, and its record marked where the worktree went. Only for
/// a dispatch that has ended: a running chat's folder is never looked at.
pub fn tidy_recorded(root: &Path, record: &Record, isolation: &git::Isolated) -> Tidied {
    if record.running() || standing(root, record) != Some(Standing::Kept) {
        return Tidied::Kept;
    }
    let Some(tree) = Tree::of(record) else {
        return Tidied::Kept;
    };
    let tidied = tidy(root, &tree, isolation);
    let how = match tidied {
        Tidied::Removed => Some(Removed::Merged),
        Tidied::FolderRemoved => Some(Removed::MergedBranchKept),
        Tidied::Kept => None,
    };
    if let Some(how) = how {
        let _ = dispatchrecord::worktree_removed(root, &record.id, how);
    }
    tidied
}

/// [`tidy_recorded`] for every ended dispatch of the project whose worktree `in_use` does not
/// answer for: what the app runs when it opens a project. How many went.
pub fn tidy_all(root: &Path, in_use: impl Fn(&Record) -> bool, isolation: &git::Isolated) -> usize {
    dispatchrecord::list(root)
        .iter()
        .filter(|record| record.place.worktree.is_some() && !in_use(record))
        .filter(|record| tidy_recorded(root, record, isolation) != Tidied::Kept)
        .count()
}

/// The chats a project's reopen record brings back, as it stood **when the project was
/// opened**: what [`tidy_at_open`] holds a worktree in use by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AtOpen {
    live: Vec<dispatchrecord::Live>,
    stand_at: Vec<PathBuf>,
}

/// The chats the reopen record of the project at `root` brings back, read now. `None` for a
/// record that cannot be read, which says nothing about which chats are gone: nothing is
/// looked at then.
///
/// **Read once, where the open reads it** ([`dispatchrecord::settle_on_open`]), and handed to
/// the look, which runs later on a thread of its own: by then the open has begun rewriting the
/// record, and a look that read it afresh could find a chat gone that was only being started.
pub fn at_open(root: &Path) -> Option<AtOpen> {
    let reopen = crate::reopen::read_strictly(root).ok()?;
    Some(AtOpen {
        live: reopen
            .as_ref()
            .map(dispatchrecord::Live::of)
            .unwrap_or_default(),
        stand_at: reopen
            .iter()
            .flat_map(|record| record.chats.iter().filter_map(|chat| chat.cwd.clone()))
            .collect(),
    })
}

/// [`tidy_all`] as the app runs it once a project is opened, against the chats `at_open`
/// brought back. A worktree is in use where one of them is its dispatch's persona chat, as
/// [`dispatchrecord::settle_on_open`] reads it, or stands in its folder.
pub fn tidy_at_open(root: &Path, at_open: &AtOpen, isolation: &git::Isolated) -> usize {
    tidy_all(
        root,
        |record| {
            at_open.live.iter().any(|chat| chat.is(&record.worker.chat))
                || Tree::of(record)
                    .is_some_and(|tree| at_open.stand_at.iter().any(|cwd| tree.holds(root, cwd)))
        },
        isolation,
    )
}

#[cfg(test)]
#[path = "dispatchplace_tests.rs"]
mod tests;
