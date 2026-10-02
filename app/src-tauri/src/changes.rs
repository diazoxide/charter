//! A cross-repo change's Push and Land, as the changes view's buttons run them (#474, ADR 0060).
//!
//! **No second implementation.** Each button is two commands, and both call the core the
//! `charter change push` and `charter change land` verbs call:
//!
//! - the **question** ([`change_push_question`], [`change_land_question`]) reads what would
//!   happen and does none of it: `change::push::destinations` names each repo, branch and
//!   destination; `change::land::verify` runs every gate of a landing and names the request, the
//!   head its checks passed at, and whether it merges now or goes into the forge's queue;
//! - the **act** ([`change_push`], [`change_land`]) hands back exactly what the operator was
//!   shown, and the core refuses, before anything is pushed or merged, when that is no longer
//!   what would happen (`push_confirmed`, `land_verified`). Every gate, the head pinning and
//!   the pending landing are the core's, and so is every refusal's wording.
//!
//! **Landing is attended only, and this is the attended path.** These commands are reached
//! only through the window's IPC (ADR 0052), which only the window's own script can call. No
//! hook, chat or extension reaches them: a chat's way to land is the `charter` binary, which
//! `floorguard::PUBLISH_FORGE` refuses unattended, and nothing in the palette's catalogue or an
//! extension's actions names these commands.
//!
//! **Off the main thread, one at a time.** Each runs on a blocking thread (GL-1's
//! `spawn_blocking`), and an act of a change is refused while another act of the same change
//! is running ([`Busy`]), so a second click cannot start a second push or landing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use charter_core::change::land::{self, How, Through, Verified};
use charter_core::change::push::{self, Destination};
use charter_core::forge::pr::Repo;
use charter_core::forge::{ForgeBackend, Kind};
use charter_core::repocmd::Say;

use crate::planes::{PlaneId, Planes};

/// Which changes have a push or a landing running, by plane, workspace and change.
#[derive(Default)]
pub(crate) struct Busy(Arc<Mutex<BTreeSet<(PathBuf, String, String)>>>);

/// A running push or landing; the change is free again when it is dropped.
pub(crate) struct Running {
    of: Arc<Mutex<BTreeSet<(PathBuf, String, String)>>>,
    key: (PathBuf, String, String),
}

impl Drop for Running {
    fn drop(&mut self) {
        self.of
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.key);
    }
}

impl Busy {
    /// The change `slug` of `ws`, claimed for one act, or why not.
    pub(crate) fn claim(&self, root: &Path, ws: &str, slug: &str) -> Result<Running, String> {
        let key = (root.to_path_buf(), ws.to_owned(), slug.to_owned());
        let mut running = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if !running.insert(key.clone()) {
            return Err(format!(
                "charter is already pushing or landing {slug} in {ws}. Wait for it to finish."
            ));
        }
        Ok(Running {
            of: Arc::clone(&self.0),
            key,
        })
    }
}

/// Where a change's forges and remotes are reached: the real ones, or a test's.
pub(crate) struct Reach<'a> {
    pub backend_of: &'a dyn Fn(&Repo) -> Box<dyn ForgeBackend>,
    /// The URL git is handed for each destination printed (`push::push_with`'s route).
    pub route: &'a dyn Fn(&str) -> String,
    /// The machine the landing log names.
    pub host: &'a str,
}

/// One member's destination, as the question names it and the yes hands it back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub(crate) struct PushDestination {
    pub repo: String,
    pub branch: String,
    /// The commit the branch is at in its clone: what the push sends.
    pub head: String,
    /// Its first twelve characters, as the core's lines show a commit.
    pub head_short: String,
    /// The HTTPS URL the branch is pushed to.
    pub to: String,
    /// The branch a request opened for it goes into, when charter can tell.
    pub base: Option<String>,
    pub forge: ForgeKind,
    /// What its forge calls a request: `pull request` or `merge request`.
    pub request: String,
}

impl From<Destination> for PushDestination {
    fn from(d: Destination) -> Self {
        Self {
            head_short: push::short(&d.head),
            repo: d.repo,
            branch: d.branch,
            head: d.head,
            to: d.to,
            base: d.base,
            forge: d.kind.into(),
            request: d.kind.request_noun().to_owned(),
        }
    }
}

impl From<PushDestination> for Destination {
    /// What the operator confirmed, as the core compares it: whole, so a destination edited on
    /// its way back is a refusal, never a different push.
    fn from(d: PushDestination) -> Self {
        Self {
            kind: d.forge.into(),
            repo: d.repo,
            branch: d.branch,
            head: d.head,
            to: d.to,
            base: d.base,
        }
    }
}

/// What a Push would do, before it does any of it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PushQuestion {
    pub destinations: Vec<PushDestination>,
    /// Each member that would not be pushed, in the core's words.
    pub not_pushed: Vec<String>,
}

/// A forge, as the window is told it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ForgeKind {
    Github,
    Gitlab,
}

impl From<Kind> for ForgeKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::GitHub => Self::Github,
            Kind::GitLab => Self::Gitlab,
        }
    }
}

impl From<ForgeKind> for Kind {
    fn from(kind: ForgeKind) -> Self {
        match kind {
            ForgeKind::Github => Kind::GitHub,
            ForgeKind::Gitlab => Kind::GitLab,
        }
    }
}

/// How a verified landing goes (`land::Through`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LandThrough {
    /// Charter merges it now.
    Merge,
    /// Into the forge's merge queue or merge train.
    Queue,
    /// It has merged at a head charter started landing, and is only recorded.
    Record,
}

impl From<Through> for LandThrough {
    fn from(through: Through) -> Self {
        match through {
            Through::Merge => Self::Merge,
            Through::Queue => Self::Queue,
            Through::Record => Self::Record,
        }
    }
}

impl From<LandThrough> for Through {
    fn from(through: LandThrough) -> Self {
        match through {
            LandThrough::Merge => Through::Merge,
            LandThrough::Queue => Through::Queue,
            LandThrough::Record => Through::Record,
        }
    }
}

/// What a Land would do, before it does any of it: the request, the head its checks passed at,
/// and how it lands. Handed back whole by the yes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub(crate) struct LandQuestion {
    pub repo: String,
    pub number: u32,
    pub url: String,
    /// The full head commit.
    pub head: String,
    /// Its first twelve characters, as the core's lines show a commit.
    pub head_short: String,
    pub through: LandThrough,
    pub forge: ForgeKind,
    /// What the forge calls a request: `pull request` or `merge request`.
    pub request: String,
    /// `#` or `!`.
    pub sigil: String,
    /// What the forge calls its queue: `merge queue` or `merge train`.
    pub queue: String,
    /// How it lands, in the core's sentence (`Verified::how`).
    pub how: String,
    /// Whether squashing is charter's to ask for here (`Verified::squash_is_charters`): never
    /// offered where it would do nothing.
    pub squash: bool,
    /// What the gates said on the way, such as the checks that passed, in the core's words.
    pub said: Vec<String>,
}

impl LandQuestion {
    fn of(v: Verified, said: Vec<String>) -> Result<Self, String> {
        Ok(Self {
            number: u32::try_from(v.number)
                .map_err(|_| format!("request {} is not a number charter can show", v.number))?,
            head_short: v.head_short(),
            how: v.how(),
            squash: v.squash_is_charters(),
            through: v.through.into(),
            forge: v.kind.into(),
            request: v.kind.request_noun().to_owned(),
            sigil: v.kind.change_sigil().to_owned(),
            queue: v.kind.queue_noun().to_owned(),
            repo: v.repo,
            url: v.url,
            head: v.head,
            said,
        })
    }

    /// The landing the operator confirmed, as the core compares it. Anything the window could
    /// have changed is compared whole by `land_verified`, so a question edited on its way back
    /// is a refusal, never a different landing.
    fn verified(&self) -> Verified {
        Verified {
            repo: self.repo.clone(),
            number: u64::from(self.number),
            url: self.url.clone(),
            head: self.head.clone(),
            through: self.through.into(),
            kind: self.forge.into(),
        }
    }
}

/// Where one change's member list is in a changes view, sent with its blocks
/// (`change::view::ChangeAt`): what Push and Land act on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct ChangeList {
    /// The index of its member list among the view's blocks.
    pub at: u32,
    pub change: String,
    pub members: Vec<ChangeMember>,
}

/// One member row of a [`ChangeList`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct ChangeMember {
    /// The row's key.
    pub key: String,
    pub repo: String,
}

impl From<charter_core::change::view::ChangeAt> for ChangeList {
    fn from(c: charter_core::change::view::ChangeAt) -> Self {
        Self {
            at: u32::try_from(c.at).unwrap_or(u32::MAX),
            change: c.change,
            members: c
                .members
                .into_iter()
                .map(|m| ChangeMember {
                    key: m.key,
                    repo: m.repo,
                })
                .collect(),
        }
    }
}

/// The lines a question said that were not refusals, and the refusals, apart.
fn parted(said: Vec<Say>) -> (Vec<String>, Vec<String>) {
    let mut fine = Vec::new();
    let mut refused = Vec::new();
    for line in said {
        match line {
            Say::Fail(text) | Say::Plain(text) => refused.push(text),
            other => fine.push(other.to_string()),
        }
    }
    (fine, refused)
}

/// A workspace name the core would read, or the refusal.
fn workspace_ok(ws: &str) -> Result<(), String> {
    if charter_core::contain::workspace_name_ok(ws) {
        Ok(())
    } else {
        Err(format!("{ws:?} is not a workspace name charter would read"))
    }
}

/// [`change_push_question`], without a runtime.
pub(crate) fn push_question_in(
    root: &Path,
    ws: &str,
    slug: &str,
    reach: &Reach,
) -> Result<PushQuestion, String> {
    workspace_ok(ws)?;
    let mut said = Vec::new();
    let found = push::destinations_with(root, ws, slug, reach.route, &mut |l| said.push(l));
    match found {
        Ok(found) => Ok(PushQuestion {
            destinations: found.into_iter().map(PushDestination::from).collect(),
            not_pushed: parted(said).1,
        }),
        Err(code) => Err(refusal(code, said)),
    }
}

/// A refused question's words: the core's, as `workspaces::ran` hands any refusal back.
fn refusal(code: u8, said: Vec<Say>) -> String {
    crate::workspaces::ran(code.max(1), said)
        .err()
        .unwrap_or_default()
}

/// [`change_push`], without a runtime.
pub(crate) fn push_in(
    root: &Path,
    ws: &str,
    slug: &str,
    confirmed: Vec<PushDestination>,
    reach: &Reach,
) -> Result<Vec<String>, String> {
    workspace_ok(ws)?;
    let confirmed: Vec<Destination> = confirmed.into_iter().map(Destination::from).collect();
    let mut said = Vec::new();
    let code = push::push_confirmed_with(
        root,
        ws,
        slug,
        &confirmed,
        reach.backend_of,
        reach.route,
        &mut |l| said.push(l),
    );
    crate::workspaces::ran(code, said)
}

/// [`change_land_question`], without a runtime.
pub(crate) fn land_question_in(
    root: &Path,
    ws: &str,
    slug: &str,
    repo: &str,
    reach: &Reach,
) -> Result<LandQuestion, String> {
    workspace_ok(ws)?;
    let mut said = Vec::new();
    let found = land::verify_with(
        root,
        ws,
        slug,
        repo,
        reach.backend_of,
        reach.host,
        chrono::Utc::now(),
        &mut |l| said.push(l),
    );
    match found {
        Ok(v) => LandQuestion::of(v, parted(said).0),
        Err(code) => Err(refusal(code, said)),
    }
}

/// [`change_land`], without a runtime.
pub(crate) fn land_in(
    root: &Path,
    ws: &str,
    slug: &str,
    confirmed: &LandQuestion,
    squash: bool,
    reach: &Reach,
) -> Result<Vec<String>, String> {
    workspace_ok(ws)?;
    // Squash is asked for only where it is charter's to ask: the question offered it nowhere
    // else, and a box the window could not have shown changes nothing.
    let how = if squash && confirmed.squash {
        How::Squash
    } else {
        How::Merge
    };
    let confirmed = confirmed.verified();
    let mut said = Vec::new();
    let code = land::land_verified_with(
        root,
        ws,
        slug,
        &confirmed,
        how,
        reach.backend_of,
        reach.host,
        chrono::Utc::now(),
        &mut |l| said.push(l),
    );
    crate::workspaces::ran(code, said)
}

/// The real forges and remotes, logged under this device's id from the store at `config`
/// (FD-25).
fn real<T>(config: Option<&Path>, then: impl FnOnce(&Reach) -> T) -> T {
    let host = charter_core::dispatch::log_name(config, &charter_core::dispatch::host());
    then(&Reach {
        backend_of: &|repo: &Repo| repo.backend(),
        route: &|https: &str| https.to_owned(),
        host: &host,
    })
}

/// What Push would do to `change`: each member's repo, branch and destination, and each member
/// that would not be pushed. Pushes nothing.
///
/// On a blocking thread: it asks git in every member's clone.
#[tauri::command]
#[specta::specta]
pub(crate) async fn change_push_question(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    change: String,
) -> Result<PushQuestion, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let config = planes.config().map(Path::to_path_buf);
    tauri::async_runtime::spawn_blocking(move || {
        real(config.as_deref(), |reach| {
            push_question_in(&root, &workspace, &change, reach)
        })
    })
    .await
    .map_err(|err| format!("reading where the push goes did not finish: {err}"))?
}

/// Push `change` as the operator confirmed it: `charter change push`, refused when what it would
/// push is no longer `confirmed`.
///
/// On a blocking thread: it pushes, and opens or updates each member's request.
#[tauri::command]
#[specta::specta]
pub(crate) async fn change_push(
    planes: tauri::State<'_, Planes>,
    busy: tauri::State<'_, Busy>,
    plane: PlaneId,
    workspace: String,
    change: String,
    confirmed: Vec<PushDestination>,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let config = planes.config().map(Path::to_path_buf);
    let running = busy.claim(&root, &workspace, &change)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _running = running;
        real(config.as_deref(), |reach| {
            push_in(&root, &workspace, &change, confirmed, reach)
        })
    })
    .await
    .map_err(|err| format!("the push did not finish: {err}"))?
}

/// What Land would do to `repo` of `change`: every gate taken, and the request, the head its
/// checks passed at and how it lands named, or the gate's refusal. Merges nothing.
///
/// On a blocking thread: it asks the forge.
#[tauri::command]
#[specta::specta]
pub(crate) async fn change_land_question(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    workspace: String,
    change: String,
    repo: String,
) -> Result<LandQuestion, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let config = planes.config().map(Path::to_path_buf);
    tauri::async_runtime::spawn_blocking(move || {
        real(config.as_deref(), |reach| {
            land_question_in(&root, &workspace, &change, &repo, reach)
        })
    })
    .await
    .map_err(|err| format!("checking the landing did not finish: {err}"))?
}

/// Land the landing the operator confirmed: `charter change land`, refused when the request, its
/// head or how it lands is no longer `confirmed`.
///
/// On a blocking thread: it asks the forge to merge.
#[tauri::command]
#[specta::specta]
pub(crate) async fn change_land(
    planes: tauri::State<'_, Planes>,
    busy: tauri::State<'_, Busy>,
    plane: PlaneId,
    workspace: String,
    change: String,
    confirmed: LandQuestion,
    squash: bool,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let config = planes.config().map(Path::to_path_buf);
    let running = busy.claim(&root, &workspace, &change)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _running = running;
        real(config.as_deref(), |reach| {
            land_in(&root, &workspace, &change, &confirmed, squash, reach)
        })
    })
    .await
    .map_err(|err| format!("the landing did not finish: {err}"))?
}

#[cfg(test)]
mod tests;
