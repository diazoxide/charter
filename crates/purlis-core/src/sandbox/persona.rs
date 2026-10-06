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
//! ([`super::changes_a_sandbox_key`]), and every teammate is told once when a persona's hosts
//! change ([`super::local::hosts_changed`]), exactly as for the project's own hosts. Each entry
//! goes through [`Host::parse`], so it is refused for everything a project's host is.
//!
//! **A chat running as that persona gets them** ([`super::Compiled::of`], at the
//! [`super::hosts::Level::Persona`] level); a chat on another persona does not, and a chat that
//! names none gets the default persona's, the one its briefing adopts
//! ([`crate::start::grants_persona`]). **Fixed at the start**: a chat's sandbox is compiled as it
//! starts, so a persona switched mid-chat changes its grants only at its next start.
//!
//! **A handoff never launders them** (D-1362-5). A chat that hands its brief to a persona whose
//! hosts reach past its own's opens a chat that holds the *asking* chat's grants, read from the
//! app's own record of that chat and never from the request ([`held_unless_within`]), until the
//! person allows its own on its tab — and the asking chat's grants are the ones it runs with, so
//! a second handoff from a held chat stays held. A **Resume** of a session record holds the
//! default persona's grants when the record's persona reaches past them (D-1362-6), since a
//! record's `persona:` is something a chat can write. A handoff to a persona whose hosts the
//! asking chat already reaches, and a chat the person starts from the window, hold their own.
//!
//! **Policy** (#1343) can forbid persona grants: [`super::hosts::Locks`] is asked of every
//! persona host. The vault-backed tools a persona runs (`tools = { kubectl = … }`, brokered
//! `purlis secret exec`) are the next slice of #1362 and are not read yet.

use std::collections::BTreeMap;

use super::hosts::{self, Host};

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
