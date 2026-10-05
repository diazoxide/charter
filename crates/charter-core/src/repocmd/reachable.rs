//! The repos the operator's own forge login reaches, for the workspace repo picker (ADR 0055).
//!
//! **Asked live and never saved as a list.** Each operator's login reaches different repos,
//! and `inventory/repos.json` is tracked, so a listing written there is one operator's view
//! handed to everyone. The picker asks this each time it opens; what the operator then picks
//! is [`take`]n into the inventory, added beside what is there and never replacing it.

use std::path::Path;

use serde_json::Value;

use crate::forge;
use crate::inventory;

/// What the operator's logins reach, and what could not be asked.
#[derive(Debug, Default)]
pub struct Reachable {
    /// Inventory records, sorted by name. `stack` is `"unknown"`: probing every repo an
    /// operator can reach to draw a list is a call per repo, and the stack is descriptive.
    pub repos: Vec<Value>,
    /// One per forge that did not answer — not logged in, or the listing failed — in the
    /// forge CLI's own words. The rest of the forges' repos are still listed.
    pub trouble: Vec<Trouble>,
}

/// A forge that did not answer: why, and the login that would cure it where one would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trouble {
    /// The forge CLI's sentence, or charter's where it could not ask.
    pub said: String,
    /// `<cli> auth login --hostname <host>`, for a forge whose CLI answered that it is not
    /// logged in to that host (NO-8). `None` where logging in would not help: no CLI, a
    /// deadline, or a listing that failed. The host passed [`forge::host_ok`] when the project's
    /// forges were read, so the line holds nothing a shell would read as more than words.
    pub login: Option<String>,
}

impl Trouble {
    /// A failure no login would cure.
    fn only(said: impl Into<String>) -> Trouble {
        Trouble {
            said: said.into(),
            login: None,
        }
    }
}

/// What [`reachable`] says of `forge` when its login check refused with `why`.
fn trouble(forge: &forge::Forge, why: &forge::ForgeError) -> Trouble {
    // Only a refused credential, or the CLI's own "not authenticated" sentence (which charter
    // cannot classify): a rate limit, a missing right or a missing owner is not cured by
    // logging in again, and no CLI or a deadline is not either.
    let login = matches!(
        why.failure(),
        forge::Failure::Auth | forge::Failure::Unrecognised
    )
    .then(|| format!("{} auth login --hostname {}", forge.kind.cli(), forge.host));
    Trouble {
        said: why.to_string(),
        login,
    }
}

/// Every repo the plane's forges let this operator reach, under the owners it declares and
/// past its excludes.
///
/// `Err` only for a plane whose forges cannot be read at all; a forge that did not answer is
/// [`Reachable::trouble`], so one logged-out host does not hide another's repos.
pub fn reachable(root: &Path) -> Result<Reachable, String> {
    let cfg = forge::load_config(root)?;
    let mut out = Reachable::default();
    let mut batches = Vec::new();
    for (forge, owner, exclude) in forge::to_query(&cfg)? {
        if let Err(why) = forge.check_auth() {
            out.trouble.push(trouble(&forge, &why));
            continue;
        }
        let reached = forge
            .backend()
            .reachable(&forge::Caller::window(), &forge::Owner::new(owner));
        match reached {
            Ok(repos) => batches.push(
                repos
                    .iter()
                    .filter(|p| !exclude.contains(&p.name))
                    .map(|p| inventory::record(p, "unknown"))
                    .collect::<Vec<_>>(),
            ),
            Err(why) => out.trouble.push(Trouble::only(why.to_string())),
        }
    }
    match inventory::merge(&batches) {
        Ok(repos) => out.repos = repos,
        Err(why) => out.trouble.push(Trouble::only(why)),
    }
    Ok(out)
}

/// Add the named repos to the inventory, so `clone` can find them — the ones it does not
/// already list, asked of the forge as this operator.
///
/// A repo the inventory already lists keeps its record: that one may carry a stack `discover`
/// probed, where this one would write `"unknown"` over it. A name nobody can reach is left
/// for `clone` to refuse in its own words.
pub fn take(root: &Path, names: &[String]) -> Result<(), String> {
    let cfg = forge::load_config(root).unwrap_or_default();
    let group = forge::group_of(&cfg, 0);
    let listed = inventory::listed(&inventory::load(root, &group)?);
    let wanted: Vec<&String> = names
        .iter()
        .filter(|n| inventory::find(&listed, n).is_none())
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let reached = reachable(root)?;
    let new: Vec<Value> = wanted
        .iter()
        .filter_map(|n| inventory::find(&reached.repos, n).cloned())
        .collect();
    if new.is_empty() {
        return Ok(());
    }
    inventory::add(root, &group, &new).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::{Forge, ForgeError};

    #[test]
    fn a_forge_that_is_not_logged_in_offers_its_clis_login_for_its_host() {
        // NO-8 (#1233): the repo picker's refusal gets the login the CLI's sentence names.
        let github = Forge::build("github", Some("ghe.example.com")).expect("a forge");
        let gitlab = Forge::build("gitlab", None).expect("a forge");
        let out = ForgeError::new("gh is not authenticated for ghe.example.com".into());

        assert_eq!(
            trouble(&github, &out),
            Trouble {
                said: "gh is not authenticated for ghe.example.com".into(),
                login: Some("gh auth login --hostname ghe.example.com".into()),
            }
        );
        assert_eq!(
            trouble(&gitlab, &out).login.as_deref(),
            Some("glab auth login --hostname gitlab.com")
        );
    }

    #[test]
    fn a_forge_charter_could_not_ask_offers_no_login() {
        // No CLI, or a deadline: logging in would not help, so nothing is offered.
        let github = Forge::build("github", None).expect("a forge");
        let missing = ForgeError::transport("charter could not find gh on PATH");

        assert_eq!(trouble(&github, &missing).login, None);
    }

    #[test]
    fn a_refusal_a_login_would_not_cure_offers_no_login() {
        use crate::forge::Failure;
        let github = Forge::build("github", None).expect("a forge");
        for failure in [
            Failure::RateLimited { reset: None },
            Failure::Forbidden,
            Failure::NotFound,
            Failure::Conflict,
        ] {
            let why = ForgeError::of(failure.clone(), "refused");
            assert_eq!(trouble(&github, &why).login, None, "{failure:?}");
        }
        let refused = ForgeError::of(Failure::Auth, "bad credentials");
        assert!(trouble(&github, &refused).login.is_some());
    }
}
