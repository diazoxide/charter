//! The cross-repo **change**: one intent across N repos, recorded as intent only (ADR 0060).
//!
//! A change is `workspaces/<ws>/changes/<slug>.json`: why, which repos (its **members**), which
//! branch in each is this change's, which must land first, and which repos were left out and
//! why. Nothing git or the forge knows is stored — no state, no request number, no check result,
//! no "landed" flag — so nothing on disk can disagree with them.
//!
//! - [`Record`]: the closed six-key record, parsed and serialised canonically. Pure.
//! - [`store`]: where records live, gated by containment.
//! - [`cmd`]: the verbs `charter change` runs, speaking through a `Say` sink.
//!
//! Words, because "change" already means one pull request in parts of `forge`: a **change** is
//! the cross-repo object, a **member** one repo's part of it, and a member's pull or merge
//! request is a **request**.

pub mod cmd;
pub mod observe;
mod record;
pub mod store;

pub use record::{
    EXCLUSION_KEYS, Exclusion, KEYS, MEMBER_KEYS, Member, Record, RecordError, TEXT_LIMIT,
    branch_refusal, default_branch, name_ok,
};

#[cfg(test)]
mod tests;
