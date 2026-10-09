//! What the forge says about each member right now: its request, whether it merged, and the
//! checks at its head commit (ADR 0060 §6; #469).
//!
//! **A reading, taken and thrown away.** Nothing here writes to the record or anywhere else:
//! the record holds intent only, and a request number or a check result stored beside it would
//! be a second clock that disagrees with the forge. Every read is fresh, and a surface shows
//! when it was taken.
//!
//! **Where each member is, is local.** The record names repos and never places; the forge and
//! the repository path come from the member's own clone's `origin` ([`Repo::of_clone`]), which
//! the operator put there by hand.
//!
//! **Blocked is derived on each read.** A member's blocker counts as landed here exactly when
//! `purlis change land`'s own gate would count it ([`super::land::verdict`], #877): the forge
//! reports its request merged, and purlis's records say purlis landed it, in the landing log
//! with the clone's default branch still holding the logged commit, or as a landing purlis
//! started. A request merged outside purlis is merged, and not landed.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, Utc};

use super::land::Verdict;
use super::record::Record;
use super::{landing, pending};
use crate::forge::Caller;
use crate::forge::checks::Checks;
use crate::forge::pr::{Repo, Request, State};

/// One member, as the forge answered for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub repo: String,
    pub branch: String,
    /// `Ok(None)`: no request from this branch. `Err`: charter could not ask, and why.
    pub request: Result<Option<Request>, String>,
    /// The checks at the request's head, read for an open request only.
    pub checks: Option<Checks>,
    /// `Ok` when it has landed, by the land gate's definition ([`super::land::verdict`]);
    /// else why not, in the gate's words.
    pub landed: Result<(), String>,
    /// Its blockers that have not landed, by this reading. On a merged member, it merged before
    /// they landed.
    pub waiting_on: Vec<String>,
}

/// Every member of one change, read at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub at: DateTime<Utc>,
    pub members: Vec<Observed>,
}

impl Observed {
    /// Whether the forge reports this member's request merged.
    pub fn merged(&self) -> bool {
        matches!(
            self.request,
            Ok(Some(Request {
                state: State::Merged { .. },
                ..
            }))
        )
    }
}

impl Observation {
    /// How many members the forge reports merged, and how many there are.
    pub fn landed(&self) -> (usize, usize) {
        let merged = self.members.iter().filter(|m| m.merged()).count();
        (merged, self.members.len())
    }
}

/// Ask the forge about every member of `record`, one member's failure costing only itself.
pub fn observe(plane: &Path, ws: &str, record: &Record, now: DateTime<Utc>) -> Observation {
    let log = landing::landings(plane, ws, &record.change);
    let pending = pending::pendings(plane, ws, &record.change);
    let mut members: Vec<Observed> = record
        .members
        .iter()
        .map(|m| {
            let request = crate::repos::clone_at(plane, ws, &m.repo)
                .ok_or_else(|| format!("no clone of {} in this workspace", m.repo))
                .and_then(|clone| {
                    let repo = Repo::of_clone(plane, &clone.path)?;
                    let found = repo
                        .backend()
                        .by_head(&Caller::command(), &repo.path, &m.branch)
                        .map_err(|why| why.to_string())?;
                    Ok((clone.path, repo, found))
                });
            let (request, checks, landed) = match request {
                Ok((clone, repo, Some(req))) => {
                    let checks = (req.state == State::Open).then(|| {
                        repo.backend().checks_at(
                            &Caller::command(),
                            &repo.path,
                            &req.head,
                            req.number,
                        )
                    });
                    // Read only: a landing purlis started and found merged counts, as the
                    // gate counts it, and is logged by `land`, never here.
                    let landed =
                        match super::land::verdict(plane, &log, &pending, &m.repo, &clone, &req) {
                            Verdict::Landed | Verdict::ToRecord(_) => Ok(()),
                            Verdict::NotLanded(why) => Err(why),
                        };
                    (Ok(Some(req)), checks, landed)
                }
                Ok((_, _, None)) => (Ok(None), None, Err("it has no request".to_string())),
                Err(why) => (Err(why.clone()), None, Err(why)),
            };
            Observed {
                repo: m.repo.clone(),
                branch: m.branch.clone(),
                request,
                checks,
                landed,
                waiting_on: Vec::new(),
            }
        })
        .collect();
    let landed: BTreeSet<String> = members
        .iter()
        .filter(|m| m.landed.is_ok())
        .map(|m| m.repo.clone())
        .collect();
    let blocked = record.blocked(&landed);
    // For every member, a merged one included: a member that went in while its blocker had
    // not is exactly what a reader must see.
    for m in &mut members {
        m.waiting_on = blocked.get(&m.repo).cloned().unwrap_or_default();
    }
    Observation { at: now, members }
}

/// Where a member's request stands, as a row says it: `open`, `merged as <sha7>`, or `REJECTED`
/// — the spec's word for a request closed unmerged, whose dependents cannot land.
pub fn standing(state: &State) -> String {
    match state {
        State::Open => "open".to_string(),
        State::Merged { commit: Some(c) } => format!(
            "merged as {}",
            crate::shown::line(c).chars().take(7).collect::<String>()
        ),
        State::Merged { commit: None } => "merged".to_string(),
        State::Closed => "REJECTED".to_string(),
    }
}
