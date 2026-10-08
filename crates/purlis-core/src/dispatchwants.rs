//! **What a persona wants** (#1502, spec #1483; V100-20, V100-21, V100-28): the personas a
//! persona's definition says it usually works with, and what the permission question says of
//! a persona before the person allows a dispatch to it.
//!
//! # The line grants nothing
//!
//! ```text
//! ---
//! name: steward
//! wants: [devops, qa]
//! ---
//! ```
//!
//! `wants` is read for one thing: which boxes the dispatch question offers under its answers
//! ("Also let steward dispatch to: devops, qa"), each unticked until the person ticks it.
//! **Nothing here is read by [`crate::dispatchgrant::covers`]**, by the rule for a chat nobody
//! is at, or by anything that starts a chat. The list of who may dispatch to whom stays where
//! it was: the project's file and this machine's record, which no sandboxed chat writes.
//!
//! That matters because **a chat may edit its own persona's file**. So everything this module
//! reads off that file is an offer the person may decline, and never more than an offer:
//!
//! - a name is offered only if it is a persona's name as purlis mints one, a persona of this
//!   project, not a draft, and not the persona itself ([`read`]); `*` is never offered, since
//!   "any persona" is Settings' alone;
//! - no more than [`MOST`] are offered, however long the line is;
//! - a pair already granted, refused by the person or locked by policy is not offered
//!   ([`also`]).
//!
//! `purlis persona lint`, and so the doctor's count of persona findings, says each thing in
//! the line that is ignored ([`Ignored::said`]).
//!
//! # What a persona works with
//!
//! [`Access`] is what the question says of a persona, in purlis's words and from records no
//! chat writes: **the vaults the registry tags for it** and the ones you let it use on this
//! machine (what [`crate::secrets::brokered::authorise`] opens for a chat, and never the
//! `vault:` line of the persona's own file), and **the hosts the project's file declares for
//! it** (`[sandbox.personas.<name>] hosts`). Only names of vaults and hosts: never a secret's
//! name or value, which this module never reads.
//!
//! # What was shown is what is answered
//!
//! [`Offer::stamp`] is a digest of everything the question said, the part a long list clips
//! included. The window sends it back with the answer, and an answer to a question that no
//! longer reads the same grants nothing: the person is shown it again.

use std::path::Path;

use crate::dispatchgrant::{Covers, InForce};
use crate::sandbox::policy::Locks;

/// The key of a persona's frontmatter.
pub const KEY: &str = "wants";

/// The most personas one question offers, however many the line names.
pub const MOST: usize = 6;

/// The most ignored names [`read`] says one by one; the rest are counted.
pub const MOST_SAID: usize = 12;

/// The most vaults, and the most hosts, [`Access::brief`] names before "and n more".
pub const MOST_NAMED: usize = 3;

/// One thing a `wants` line holds that is not offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ignored {
    /// Not a persona's name as purlis mints one, as written.
    NotAName(String),
    /// `*`: any persona is granted in Settings only.
    Any,
    /// The persona itself.
    Itself,
    /// A name no persona of this project has.
    Unknown(String),
    /// A persona that is still a draft: no chat is dispatched to it.
    Draft(String),
    /// How many entries came after the last one read.
    Beyond(usize),
}

impl Ignored {
    /// The sentence `purlis persona lint` says of it.
    pub fn said(&self) -> String {
        match self {
            Self::NotAName(written) => format!(
                "wants names '{}', which is not a persona's name, so it is not offered",
                crate::shown::short(written)
            ),
            Self::Any => "wants names `*`, which is ignored: any persona is granted in Settings \
                          only, and `wants` grants nothing"
                .to_owned(),
            Self::Itself => "wants names the persona itself, which is ignored: a chat dispatches \
                             to its own persona with no grant"
                .to_owned(),
            Self::Unknown(name) => format!(
                "wants names '{name}', which is not a persona of this project, so it is not \
                 offered"
            ),
            Self::Draft(name) => format!(
                "wants names '{name}', which is a draft, so it is not offered until its `draft: \
                 true` line is dropped"
            ),
            Self::Beyond(more) => format!(
                "wants names {more} more than the {MOST} purlis offers in one question, so they \
                 are not offered"
            ),
        }
    }
}

/// What a persona's `wants` line comes to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wants {
    /// The personas a question may offer, in the order written, each once.
    pub personas: Vec<String>,
    /// What the line holds that is not offered.
    pub ignored: Vec<Ignored>,
}

/// **`value`, the `wants` line of persona `me`, read**: where `known` says which names are
/// personas of the project and `draft` which of those are drafts. Pure.
///
/// A list as every list of a persona's is written: the outer brackets optional, entries split
/// at commas. A name written twice is offered once. Reading stops at [`MOST`] offered, or at
/// [`MOST_SAID`] ignored, and what is left is counted.
pub fn read(
    value: &str,
    me: &str,
    known: &dyn Fn(&str) -> bool,
    draft: &dyn Fn(&str) -> bool,
) -> Wants {
    let mut out = Wants::default();
    let mut entries = value
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(crate::memstore::py_strip)
        .filter(|entry| !entry.is_empty());
    for entry in entries.by_ref() {
        if out.personas.len() == MOST || out.ignored.len() == MOST_SAID {
            out.ignored.push(Ignored::Beyond(1 + entries.count()));
            break;
        }
        let bare = entry.trim_matches(|c| c == '"' || c == '\'');
        let ignored = if bare == crate::dispatchgrant::ANY {
            Ignored::Any
        } else if !crate::personas::valid_name(entry) {
            Ignored::NotAName(entry.to_owned())
        } else if entry == me {
            Ignored::Itself
        } else if !known(entry) {
            Ignored::Unknown(entry.to_owned())
        } else if draft(entry) {
            Ignored::Draft(entry.to_owned())
        } else {
            if !out.personas.iter().any(|one| one == entry) {
                out.personas.push(entry.to_owned());
            }
            continue;
        };
        out.ignored.push(ignored);
    }
    out
}

/// **What `persona` of the project at `root` wants**, as its definition reads now: its own
/// `wants` line, or the nearest one up its `extends` chain. Nothing for a name that is no
/// persona's, or one whose definition does not load.
pub fn of(root: &Path, persona: &str) -> Wants {
    let Some(resolved) = crate::personaverbs::resolve(root, persona) else {
        return Wants::default();
    };
    let Some(value) = resolved.get(KEY) else {
        return Wants::default();
    };
    let known = crate::workspaces::Plane::open(root.to_path_buf())
        .personas()
        .unwrap_or_default();
    read(
        value,
        persona,
        &|name| known.iter().any(|one| one == name),
        &|name| crate::personaverbs::is_draft(root, name),
    )
}

/// **The wanted personas a question from a chat running as `asking`, about `target`, may
/// offer beside it**: each one [`of`] offers that is not the target and that nothing answers
/// for yet, so not one a grant covers, the person said never to, or a policy locks. None for a
/// chat on no persona.
pub fn also(
    root: &Path,
    asking: Option<&str>,
    target: &str,
    grants: &InForce,
    locks: &Locks,
) -> Vec<String> {
    let Some(asking) = asking else {
        return Vec::new();
    };
    of(root, asking)
        .personas
        .into_iter()
        .filter(|wanted| wanted != target)
        .filter(|wanted| {
            crate::dispatchgrant::covers(Some(asking), wanted, grants, locks) == Covers::NeedsGrant
        })
        .collect()
}

/// The vaults a persona's chats are handed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vaults {
    /// Their names, sorted; none where there is none.
    Named(Vec<String>),
    /// The registry did not read, so which is not known: never said as "no vault".
    Unreadable,
}

/// The hosts a persona's chats reach beside the project's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hosts {
    /// As the project's file declares them for the persona, less what policy locks out.
    Named(Vec<String>),
    /// The project's chats run in no sandbox, so they reach any host.
    Any,
}

/// **What one persona works with**, as the dispatch question says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub persona: String,
    pub vaults: Vaults,
    pub hosts: Hosts,
}

/// `names`, the first [`MOST_NAMED`] of them, then how many more.
fn listed(names: &[String]) -> String {
    let named = names[..names.len().min(MOST_NAMED)].join(", ");
    match names.len().saturating_sub(MOST_NAMED) {
        0 => named,
        more => format!("{named} and {more} more"),
    }
}

impl Access {
    /// What `persona` works with, where `ctx` reads the vault registry, `plane` is the
    /// project's file and `locks` the administrator's policy.
    pub fn of(
        ctx: &crate::secrets::Ctx,
        plane: &crate::sandbox::Plane,
        locks: &Locks,
        persona: &str,
    ) -> Self {
        let vaults = match crate::secrets::brokered::vaults_of(ctx, persona) {
            Ok(names) => Vaults::Named(names),
            Err(_) => Vaults::Unreadable,
        };
        let hosts = match plane.in_force(locks) {
            None => Hosts::Any,
            Some(policy) => Hosts::Named(
                crate::sandbox::persona::of(&policy.personas, Some(persona))
                    .map(|grants| grants.hosts.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .filter(|host| {
                        locks
                            .refuses(&crate::sandbox::hosts::Granted {
                                host: (*host).clone(),
                                level: crate::sandbox::hosts::Level::Persona,
                            })
                            .is_none()
                    })
                    .map(ToString::to_string)
                    .collect(),
            ),
        };
        Self {
            persona: persona.to_owned(),
            vaults,
            hosts,
        }
    }

    /// [`Self::of`], read from the project at `root` as it is now, under `locks`.
    pub fn at(root: &Path, locks: &Locks, persona: &str) -> Self {
        let ctx = crate::personaverbs::vault_ctx(root, &crate::plane::state_dir(root));
        Self::of(&ctx, &crate::sandbox::Plane::read(root), locks, persona)
    }

    /// The access alone, clipped: `vault devops; hosts a, b and 2 more`.
    pub fn brief(&self) -> String {
        let vaults = match &self.vaults {
            Vaults::Unreadable => "vaults purlis could not read".to_owned(),
            Vaults::Named(names) => match names.as_slice() {
                [] => "no vault".to_owned(),
                [one] => format!("vault {one}"),
                _ => format!("vaults {}", listed(names)),
            },
        };
        let hosts = match &self.hosts {
            Hosts::Any => "any host, since this project's sandbox is off".to_owned(),
            Hosts::Named(names) if names.is_empty() => "no hosts beyond the project's".to_owned(),
            Hosts::Named(names) => format!("hosts {}", listed(names)),
        };
        format!("{vaults}; {hosts}")
    }

    /// The sentence the question says of the persona it asks about.
    pub fn said(&self) -> String {
        format!(
            "{} works with its own access: {}.",
            self.persona,
            self.brief()
        )
    }

    /// Everything of it, unclipped, for [`Offer::stamp`].
    fn whole(&self) -> String {
        let vaults = match &self.vaults {
            Vaults::Unreadable => "?".to_owned(),
            Vaults::Named(names) => names.join(","),
        };
        let hosts = match &self.hosts {
            Hosts::Any => "*".to_owned(),
            Hosts::Named(names) => names.join(","),
        };
        format!("{}\u{1f}{vaults}\u{1f}{hosts}", self.persona)
    }
}

/// **Everything one dispatch question says beyond who asks and the brief**: what the persona
/// asked about works with, and each wanted persona offered beside it with what that one works
/// with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub target: Access,
    pub also: Vec<Access>,
}

impl Offer {
    /// **A digest of all of it**, what a long list clips included: what the window sends back
    /// with an answer, so an answer is to the question as it still reads.
    pub fn stamp(&self) -> String {
        use sha2::Digest;
        let mut all = self.target.whole();
        for one in &self.also {
            all.push('\u{1e}');
            all.push_str(&one.whole());
        }
        sha2::Sha256::digest(all.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

#[cfg(test)]
#[path = "dispatchwants_tests.rs"]
mod tests;
