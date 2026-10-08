//! **A persona's own sandbox grants** (#1362; ADR 0067 §1 as amended): the hosts a persona's
//! chats reach, beside the project's and yours ([`super::hosts`]).
//!
//! **Where they are written.** In the project's committed `charter.toml`, under the persona's
//! name:
//!
//! ```toml
//! [sandbox.personas.devops]
//! hosts = ["10.100.39.145:6443", "*.internal.example"]
//! ```
//!
//! Not in the persona's own `persona.md`, and that is the point (D-1362-1): a chat may edit its
//! own persona's charter (spec #1330, amended 2026-10-06), so a grant written there would be a
//! grant a chat could write itself. `charter.toml` at the project root is a later-code name no
//! chat writes ([`super::PLANTED`]), a brokered write refuses any change under `[sandbox]`
//! ([`super::changes_a_sandbox_key`]). Each entry goes through [`Host::parse`], so it is refused
//! for everything a project's host is: a private address is one exact address with an optional
//! exact port, and no wildcard or range ever names addresses.
//!
//! **Allowed on each machine, bound to what was shown** (D-1362-7). A persona's hosts are
//! committed, so a teammate can change them; and they are the hosts that reach past the
//! presets into private networks. So, unlike the project's own hosts, which apply and are told
//! once, a persona's reach nothing on a machine until the person there allows them on the
//! arrival Notice ([`shown`]). The Allow is kept with the [`digest`] of the list the Notice
//! showed ([`super::local::allow_persona_hosts`]), and checked against the list as it is when
//! it is pressed ([`shown_now`]). A list that changes in any way after it was allowed grants
//! nothing until it is allowed again ([`in_force_here`]): never a part of it, never the old
//! one.
//!
//! **A chat running as that persona gets them** ([`super::Compiled::of`], at the
//! [`super::hosts::Level::Persona`] level); a chat on another persona does not, and a chat that
//! names none gets the default persona's, the one its briefing adopts
//! ([`crate::start::grants_persona`]). **Fixed at the start**: a chat's sandbox is compiled as it
//! starts, so a persona switched mid-chat changes its grants only at its next start.
//!
//! **A chat never widens what it reaches on its own say.** A **Resume** of a session record
//! holds the default persona's grants when the record's persona reaches past them (D-1362-6,
//! [`held_unless_within`]), since a record's `persona:` is something a chat can write, until
//! the person allows its own on its tab. A chat the person starts from the window holds its
//! own. A handoff used to be held the same way on the asking chat's grants (D-1362-5); since a
//! handoff became a dispatch (#1444) it is not, and a chat still held from before stays held,
//! and is refused a dispatch of either kind, until the person allows it.
//!
//! **A dispatch is not held** (#1437, spec #1434). A persona chat started by a dispatch that a
//! dispatch grant covers holds its own persona's grants from its first command
//! ([`crate::dispatchgrant::grants_for_a_dispatched_chat`]): the grant is the person's consent
//! to the pair, given before anything started, so nothing is left to allow on the new chat's
//! tab. Without a grant the dispatch does not start at all. **A handoff is a dispatch**
//! (#1444), decided and started the same way, so this holds for it too.
//!
//! **Policy** (#1343) can forbid persona grants: [`super::hosts::Locks`] is asked of every
//! persona host. The vault-backed tools a persona runs (`tools = { kubectl = … }`, brokered
//! `purlis secret exec`) are the next slice of #1362 and are not read yet.

use std::collections::BTreeMap;
use std::path::Path;

use super::hosts::{self, Host};
use super::policy::Locks;

/// The key in `[sandbox]` that holds each persona's grants.
pub const KEY: &str = "personas";

/// The keys one persona's table holds.
const KEYS: [&str; 1] = [hosts::KEY];

/// What one persona is granted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    /// The hosts its chats reach, beside the project's and yours.
    pub hosts: Vec<Host>,
}

/// What `[sandbox] personas` holds: each persona's grants, and each thing in it that is not
/// taken, as one sentence.
pub fn read(value: Option<&toml::Value>, file: &str) -> (BTreeMap<String, Grants>, Vec<String>) {
    let mut out = BTreeMap::new();
    let mut refused = Vec::new();
    let Some(value) = value else {
        return (out, refused);
    };
    let at = format!("{}.{KEY}", super::TABLE);
    let Some(table) = value.as_table() else {
        refused.push(format!(
            "{at} in {file} is not a table, so no persona is granted anything — write \
             [{at}.<persona>] with hosts"
        ));
        return (out, refused);
    };
    for (persona, grants) in table {
        let here = format!("{at}.{persona}");
        if !crate::personas::valid_name(persona) {
            refused.push(format!(
                "{here} in {file} is not a persona's name, so it grants nothing — a persona's \
                 name is lowercase letters, digits and hyphens"
            ));
            continue;
        }
        let Some(grants) = grants.as_table() else {
            refused.push(format!(
                "{here} in {file} is not a table, so it grants nothing — it holds hosts"
            ));
            continue;
        };
        for key in grants.keys() {
            if !KEYS.contains(&key.as_str()) {
                refused.push(format!(
                    "{here}.{key} in {file} is not a key purlis reads — a persona's sandbox \
                     holds hosts"
                ));
            }
        }
        let hosts = match hosts::read(grants.get(hosts::KEY)) {
            Ok(listed) => {
                refused.extend(listed.refused.into_iter().map(|(written, why)| {
                    format!(
                        "{here}.hosts in {file} names {written}, which no chat is let reach: {why}"
                    )
                }));
                listed.hosts
            }
            Err(hosts::NotAList) => {
                refused.push(format!(
                    "{here}.hosts in {file} is not a list of hosts, so none of {persona}'s is \
                     allowed — write hosts = [\"api.example.com\", \"10.0.0.5:6443\"]"
                ));
                Vec::new()
            }
        };
        out.insert(persona.clone(), Grants { hosts });
    }
    (out, refused)
}

/// **The grants a chat running as `persona` holds**: its hosts. Nothing for a chat on no
/// persona, or one the project grants nothing.
pub fn of<'a>(personas: &'a BTreeMap<String, Grants>, persona: Option<&str>) -> Option<&'a Grants> {
    personas.get(persona?)
}

/// **What a chat started as `target` by someone the person did not pick holds** (#1362,
/// D-1362-5 and D-1362-6): `None`, its own persona's grants, where `target`'s hosts are all among
/// those of `trusted` (it widens nothing), or where the project does not sandbox its chats;
/// otherwise the grants of `trusted`, until the person allows its own on its tab.
///
/// `trusted` is, for a **handoff**, the grants the asking chat itself runs with, read from the
/// app's own record of it ([`crate::start::runs_with`]) — so a chain of handoffs never climbs
/// past the first chat's grants — and never the request. For a **Resume** of a session record,
/// whose `persona:` a chat could have written, it is the default persona's. A chat the person
/// starts from the window holds its own and asks nothing.
pub fn held_unless_within(
    policy: Option<&super::Policy>,
    trusted: Option<&str>,
    target: Option<&str>,
) -> Option<crate::reopen::HeldGrants> {
    let personas = &policy?.personas;
    let wanted = of(personas, target).map_or(&[][..], |grants| grants.hosts.as_slice());
    let had = of(personas, trusted).map_or(&[][..], |grants| grants.hosts.as_slice());
    let within = wanted.iter().all(|host| had.contains(host));
    (!within).then(|| crate::reopen::HeldGrants {
        persona: trusted.map(str::to_owned),
    })
}

// ---- allowed on this machine, bound to what was shown (#1362, D-1362-7) ----------------------

/// **The digest an Allow of a persona's hosts is bound to**: SHA-256 over the hosts as the
/// sandbox spells them, sorted and once each, so order is no change and any other change is.
pub fn digest(hosts: &[Host]) -> String {
    use sha2::Digest;
    let mut spelled: Vec<String> = hosts.iter().map(ToString::to_string).collect();
    spelled.sort();
    spelled.dedup();
    let text = format!("purlis persona hosts 1\n{}", spelled.join("\n"));
    crate::extension::hex(&sha2::Sha256::digest(text.as_bytes()))
}

/// `grants`' hosts as a chat would reach them under `locks`: none a policy locks out (#1343),
/// so the Notice never asks for a host no chat would reach.
fn reached(grants: &Grants, locks: &Locks) -> Vec<Host> {
    grants
        .hosts
        .iter()
        .filter(|host| {
            locks
                .refuses(&hosts::Granted {
                    host: (*host).clone(),
                    level: hosts::Level::Persona,
                })
                .is_none()
        })
        .cloned()
        .collect()
}

/// **The hosts a chat running as `persona` holds on this machine**, in the project at `root`:
/// its persona's hosts under `locks`, all of them where the person here allowed exactly this
/// list ([`digest`]), and none otherwise. Nothing for a chat on no persona, or on one the
/// project grants nothing. A record that cannot be read allows nothing.
pub fn in_force_here(
    root: &Path,
    personas: &BTreeMap<String, Grants>,
    persona: Option<&str>,
    locks: &Locks,
) -> Vec<Host> {
    let (Some(name), Some(grants)) = (persona, of(personas, persona)) else {
        return Vec::new();
    };
    let hosts = reached(grants, locks);
    if hosts.is_empty() || !allowed_here(root, name, &hosts) {
        return Vec::new();
    }
    hosts
}

/// Whether the person allowed exactly `hosts` for `persona` in the project at `root`.
fn allowed_here(root: &Path, persona: &str, hosts: &[Host]) -> bool {
    let now = digest(hosts);
    super::local::allowed_persona_hosts(root)
        .iter()
        .any(|(allowed, was)| allowed == persona && *was == now)
}

/// One persona's hosts as the arrival Notice and Settings show them on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub persona: String,
    /// Its hosts as the project's file lists them.
    pub listed: Vec<Host>,
    /// Those a chat would reach, the ones an Allow allows: none a policy locks out. Empty
    /// where a policy forbids persona hosts, and then nothing is asked.
    pub hosts: Vec<Host>,
    /// What an Allow of exactly this list is bound to ([`digest`]).
    pub digest: String,
    /// Whether the person allowed exactly this list here.
    pub allowed: bool,
}

/// **Each persona's hosts in the project at `root`**, as this machine would grant them, and
/// whether the person here allowed them: the arrival Notice asks for each one not allowed that
/// has a host a chat would reach (none where a policy forbids persona hosts, #1343). None where
/// the project's chats are not sandboxed, and none for a persona that lists no host.
pub fn shown(root: &Path) -> Vec<Shown> {
    shown_in(root, &super::Plane::read(root), &Locks::of(root))
}

/// [`shown`], for the project at `root` read as `plane`, under `locks`.
pub fn shown_in(root: &Path, plane: &super::Plane, locks: &Locks) -> Vec<Shown> {
    let Some(policy) = plane.in_force(locks) else {
        return Vec::new();
    };
    policy
        .personas
        .iter()
        .filter(|(_, grants)| !grants.hosts.is_empty())
        .map(|(persona, grants)| {
            let hosts = reached(grants, locks);
            Shown {
                persona: persona.clone(),
                listed: grants.hosts.clone(),
                digest: digest(&hosts),
                allowed: !hosts.is_empty() && allowed_here(root, persona, &hosts),
                hosts,
            }
        })
        .collect()
}

/// **What an Allow of `persona`'s hosts allows**, where `digest` is the digest of the list as
/// it stands now: the list the window showed is the list kept. Refused, with a sentence, where
/// the persona has no hosts here or they changed after they were shown.
pub fn shown_now(root: &Path, persona: &str, digest: &str) -> Result<Shown, String> {
    pick(shown(root), persona, digest)
}

/// [`shown_now`] among `shown`, each persona's hosts as they stand now.
pub fn pick(shown: Vec<Shown>, persona: &str, digest: &str) -> Result<Shown, String> {
    let now = shown
        .into_iter()
        .find(|one| one.persona == persona && !one.hosts.is_empty())
        .ok_or_else(|| {
            format!(
                "purlis allowed nothing: chats as {persona} have no hosts of their own in this                  project now."
            )
        })?;
    if now.digest != digest {
        return Err(format!(
            "purlis allowed nothing: {persona}'s hosts changed after they were shown. Look at              them again."
        ));
    }
    Ok(now)
}

/// Tests' stand-in for a person who allowed every persona's hosts in `policy`, as listed, at
/// `root`.
#[cfg(test)]
pub(crate) fn allow_every_as_listed(root: &Path, policy: &super::Policy) {
    for (persona, grants) in &policy.personas {
        super::local::allow_persona_hosts(root, persona, &digest(&grants.hosts)).expect("kept");
    }
}
