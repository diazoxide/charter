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
//! **Blocked is derived on each read.** A member's blocker counts as landed here when the
//! forge reports its request merged. `charter change land` (#472) adds the other half of
//! "landed" — the landing log, and the default branch still containing the logged commit.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, Utc};

use super::record::Record;
use crate::forge::checks::{self, Checks};
use crate::forge::pr::{self, Repo, Request, State};

/// One member, as the forge answered for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub repo: String,
    pub branch: String,
    /// `Ok(None)`: no request from this branch. `Err`: charter could not ask, and why.
    pub request: Result<Option<Request>, String>,
    /// The checks at the request's head, read for an open request only.
    pub checks: Option<Checks>,
    /// Its blockers that have not merged, by this reading. On a merged member, it went in ahead
    /// of them.
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
    let mut members: Vec<Observed> = record
        .members
        .iter()
        .map(|m| {
            let request = crate::repos::clone_at(plane, ws, &m.repo)
                .ok_or_else(|| format!("no clone of {} in this workspace", m.repo))
                .and_then(|clone| Repo::of_clone(plane, &clone.path))
                .and_then(|repo| {
                    let found = pr::by_head(&repo, &m.branch)?;
                    Ok((repo, found))
                });
            let (request, checks) = match request {
                Ok((repo, Some(req))) => {
                    let checks = (req.state == State::Open)
                        .then(|| checks::at(&repo, &req.head, req.number));
                    (Ok(Some(req)), checks)
                }
                Ok((_, None)) => (Ok(None), None),
                Err(why) => (Err(why), None),
            };
            Observed {
                repo: m.repo.clone(),
                branch: m.branch.clone(),
                request,
                checks,
                waiting_on: Vec::new(),
            }
        })
        .collect();
    let landed: BTreeSet<String> = members
        .iter()
        .filter(|m| m.merged())
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
