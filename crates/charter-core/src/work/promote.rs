//! Promoting a todo to an issue (ADR 0088 §5, V40 c): `charter ws todo promote <slug> --repo
//! <repo>`, and the window's **Promote to issue**, which runs the same function.
//!
//! 1. The slug resolves as `ws todo done` resolves one.
//! 2. `--repo` names a repo of the workspace by its inventory name, and may be left out when the
//!    workspace has exactly one repo on a forge. The repo is read first ([`Repos::about`]): one
//!    that takes no issue from this account is refused before anything is sent, and the caller
//!    is told the repo and whether it is public before the issue is created.
//! 3. The issue is created through [`WorkItems::create`] as the signed-in human, titled with the
//!    todo's title, its text as the body, carrying FI6's layer-2 label unless the repo is public.
//! 4. The alias `todo key → issue key` (`promoted`) is appended to this device's log.
//! 5. The todo is closed as `ws todo done` closes one, journalled as
//!    `Promoted todo: <title> → <issue key>`.
//!
//! **Stopping half-way.** A crash after step 4 leaves the todo file with an alias: every reader
//! already treats it as the issue, and [`finish_closes`] closes it at the next `ws todo`. A crash
//! between steps 3 and 4 leaves an issue nobody linked, so the caller is told the issue's key the
//! moment it exists ([`Step::Created`]).

use std::path::Path;

use super::log::{self, Cause, Fold};
use super::{TrackerKey, WorkItem};
use crate::forge::backend::{About, Issues, NewWorkItem, RepoRecord, Visibility};
use crate::forge::{self, Caller, Forge, ForgeBackend};
use crate::workspaces::{Plane, Workspace};

/// The repo a todo is promoted into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Its inventory name, as `--repo` takes it.
    pub name: String,
    pub forge: Forge,
    /// The repo, as its inventory row reads ([`crate::inventory::read`]). Its
    /// `path_with_namespace` is its path on the forge: `owner/repo`, or GitLab's full namespace.
    pub repo: RepoRecord,
}

impl Target {
    /// Its path on the forge: `owner/repo`, or GitLab's full namespace.
    pub fn path(&self) -> &str {
        &self.repo.path_with_namespace
    }
}

/// What [`promote`] tells its caller as it goes.
#[derive(Debug)]
pub enum Step<'a> {
    /// About to send the todo's title and text to `target`, whose readers `about` names, with
    /// the workspace label the issue will carry, if any, as the forge spells it.
    Sending {
        target: &'a Target,
        about: About,
        label: Option<String>,
    },
    /// The issue exists. Said at once, so that an issue a later step fails to link is not lost.
    Created(&'a WorkItem),
}

/// What a promote did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Promoted {
    pub title: String,
    pub todo: TrackerKey,
    pub item: WorkItem,
    /// The journal entry the todo was closed with, as `ws todo done` writes one.
    pub journal: std::path::PathBuf,
}

/// The repo `repo` names among workspace `ws`'s repos, or the one repo it has on a forge.
pub fn target(root: &Path, ws: &str, repo: Option<&str>) -> Result<Target, String> {
    let declared = crate::repos::declared(&Plane::open(root), ws);
    let cfg = forge::load_config(root)?;
    let doc = crate::inventory::load(root, &forge::group_of(&cfg, 0))?;
    let inventory = crate::inventory::repos(root, &doc, &[]);
    let on_a_forge: Vec<(&String, RepoRecord)> = declared
        .iter()
        .filter_map(|name| {
            let row = crate::inventory::find(&inventory, name)?;
            crate::inventory::read(row).map(|record| (name, record))
        })
        .collect();
    let (name, record) = match repo {
        Some(wanted) => {
            if !declared.iter().any(|name| name == wanted) {
                return Err(format!(
                    "'{wanted}' is not a repo of workspace '{ws}'. Its repos: {}",
                    names(&declared)
                ));
            }
            on_a_forge
                .into_iter()
                .find(|(name, _)| *name == wanted)
                .ok_or_else(|| {
                    format!(
                        "'{wanted}' is not in the project's inventory as a repo on a forge, so \
                         there is nowhere to open an issue. Run `charter discover` first"
                    )
                })?
        }
        None => {
            let mut on_a_forge = on_a_forge.into_iter();
            match (on_a_forge.next(), on_a_forge.next()) {
                (Some(one), None) => one,
                (None, _) => {
                    return Err(format!(
                        "workspace '{ws}' has no repo on a forge to open an issue in"
                    ));
                }
                (Some(first), Some(second)) => {
                    let listed: Vec<String> = [first, second]
                        .into_iter()
                        .chain(on_a_forge)
                        .map(|(n, _)| n.clone())
                        .collect();
                    return Err(format!(
                        "workspace '{ws}' has more than one repo on a forge; name one with \
                         --repo: {}",
                        names(&listed)
                    ));
                }
            }
        }
    };
    if record.path_with_namespace.is_empty() {
        return Err(format!("the inventory names no forge path for '{name}'"));
    }
    let page = record.web_url.as_str();
    let forge = if forge::host_of(page).is_empty() {
        // At the kind's default host, as `Forge::for_record` builds one from the row.
        Forge::default_of(record.forge)
    } else {
        forge::resolve_host(page, root).ok_or_else(|| {
            format!(
                "'{name}' is at {page}, on a host this project does not declare. Add it as a \
                 [[forge]] in charter.toml"
            )
        })?
    };
    Ok(Target {
        name: name.clone(),
        forge,
        repo: record,
    })
}

fn names(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// Promote todo `slug` of `ws` to an issue in `target`, through `backend` as `caller`, and write
/// the alias to device `device`'s log. `now` is the local clock's, as `ws todo` stamps a todo.
///
/// **Everything that can refuse is asked before the issue exists**: the todo, whether it was
/// promoted already, whether an alias from it could close a cycle, whether this device's log can
/// be written at all (its id, the workspace's name, containment), and the repo itself. An issue
/// is the one step that cannot be undone, so nothing that could have refused it comes after.
#[allow(clippy::too_many_arguments)]
pub fn promote(
    ws: &Workspace,
    slug: &str,
    target: &Target,
    backend: &dyn ForgeBackend,
    caller: &Caller,
    device: &str,
    now: chrono::NaiveDateTime,
    tell: &mut dyn FnMut(Step<'_>),
) -> Result<Promoted, String> {
    let root = ws.plane_root();
    let dir = ws.dir().join("todos");
    let path = crate::memstore::resolve(root, &dir, slug).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("no todo '{slug}' in workspace '{}'.", ws.name())
        } else {
            e.to_string()
        }
    })?;
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let open = ws.todos().map_err(|e| e.to_string())?;
    let todo = open
        .into_iter()
        .find(|t| t.slug == stem)
        .ok_or_else(|| format!("no todo '{slug}' in workspace '{}'.", ws.name()))?;
    let todo_key = TrackerKey::todo(ws.name(), &stem)?;
    let folded = log::fold(root);
    if folded.is_aliased(&todo_key) {
        return Err(format!(
            "'{}' was already promoted to {}; it is closed at the next `charter ws todo`",
            todo.title,
            folded.resolve(&todo_key)
        ));
    }
    if folded.is_reached(&todo_key) {
        return Err(format!(
            "another key already resolves to {todo_key}, so an alias from it would close a \
             cycle; nothing was sent. `charter doctor` shows the work links"
        ));
    }
    log::check_writable(root, ws.name(), device)
        .map_err(|e| format!("the work link log cannot be written, so nothing was sent: {e}"))?;
    let host = super::key::normal_host(&target.forge.host)?;

    let about = backend
        .about(caller, &target.repo)
        .map_err(|e| e.to_string())?;
    let refusal = match about.issues {
        Issues::Open => None,
        Issues::Off => Some("has its issues turned off"),
        Issues::NoRight => Some("takes no issue from this account"),
        Issues::Archived => Some("is archived"),
    };
    if let Some(why) = refusal {
        return Err(format!(
            "{} {why}, so nothing was sent and the todo stays a todo",
            target.path()
        ));
    }
    // FI6: the layer-2 label is on for a private repo only (D-FW5a: `internal` is read as
    // public, since everyone on the instance reads it).
    let labelled = about.visibility == Visibility::Private;
    tell(Step::Sending {
        target,
        about,
        label: labelled
            .then(|| crate::forge::backend::workspace_label(target.forge.kind, ws.name())),
    });
    let new = NewWorkItem {
        title: todo.title.clone(),
        body: body_of(&todo.title, &todo.body),
        workspace_label: labelled.then(|| ws.name().to_string()),
    };
    let item = backend
        .create(caller, target.path(), &new)
        .map_err(|e| e.to_string())?;
    tell(Step::Created(&item));
    if item.key.host() != Some(host.as_str()) {
        return Err(format!(
            "{} was opened at {}, but charter asked {host}, so its key is not trusted and no \
             alias was written. The todo is still open",
            item.key, item.url
        ));
    }

    log::append_alias_over(
        &folded,
        root,
        ws.name(),
        device,
        utc_of(now),
        todo_key.clone(),
        item.key.clone(),
        Cause::Promoted,
    )
    .map_err(|e| {
        format!(
            "{} was opened, but its alias could not be written: {e}. The todo is still open",
            item.key
        )
    })?;
    let journal = close(ws, &stem, &todo.title, &item.key, now).map_err(|e| {
        format!(
            "{} was opened and linked, but the todo could not be closed: {e}. The next `charter \
             ws todo` closes it",
            item.key
        )
    })?;
    Ok(Promoted {
        title: todo.title,
        todo: todo_key,
        item,
        journal,
    })
}

/// The issue's body: the todo's text, without the title line it starts with, since the title is
/// the issue's own.
fn body_of(title: &str, text: &str) -> String {
    let mut lines = text.lines();
    match lines.next() {
        Some(first) if first.trim() == title.trim() => lines
            .skip_while(|l| l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => text.to_string(),
    }
}

/// A stamp on the local clock, as `ws todo` takes one, as the UTC instant a log line holds. A
/// local time a clock change makes ambiguous or skips takes the earlier reading, else now.
fn utc_of(local: chrono::NaiveDateTime) -> chrono::DateTime<chrono::Utc> {
    local
        .and_local_timezone(chrono::Local)
        .earliest()
        .map_or_else(chrono::Utc::now, |t| t.with_timezone(&chrono::Utc))
}

/// The journal line a promoted todo is closed with.
pub fn journal_line(title: &str, to: &TrackerKey) -> String {
    format!("Promoted todo: {title} → {to}")
}

/// Close todo `stem` as `ws todo done` does, the journal saying where it went.
fn close(
    ws: &Workspace,
    stem: &str,
    title: &str,
    to: &TrackerKey,
    now: chrono::NaiveDateTime,
) -> std::io::Result<std::path::PathBuf> {
    // The journal entry first, while the todo is still there to name.
    let journal = ws.remember(&journal_line(title, to), now)?;
    ws.forget_todo(stem)?;
    Ok(journal)
}

/// Close every open todo of `ws` whose key already has an alias: a promote that stopped after
/// writing its alias (ADR 0088 §5). Answers each closed todo's title and the key it resolves to.
pub fn finish_closes(
    ws: &Workspace,
    folded: &Fold,
    now: chrono::NaiveDateTime,
) -> std::io::Result<Vec<(String, TrackerKey)>> {
    let mut closed = Vec::new();
    for todo in ws.todos()? {
        let Ok(key) = TrackerKey::todo(ws.name(), &todo.slug) else {
            continue;
        };
        if !folded.is_aliased(&key) {
            continue;
        }
        let to = folded.resolve(&key);
        close(ws, &todo.slug, &todo.title, &to, now)?;
        closed.push((todo.title, to));
    }
    Ok(closed)
}

#[cfg(test)]
mod tests;
