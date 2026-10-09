//! The `vault registry` row: whether both halves of the vault registry can be read, and whether
//! every vault in them is one purlis can use (OB-8, #994).
//!
//! **Read, never opened.** The row reads `vaults.json` and this machine's half the way every
//! vault command does ([`registry::load_shared`], [`registry::load_local`]) and asks nothing of
//! a provider: no vault is opened, no program runs, no keyring is asked, and no value is in
//! hand. Whether a vault's credentials are there is the `vaults` row's question, which needs
//! the provider asked and is still deferred.
//!
//! A warning for a half that cannot be read, because every `purlis secret` and `purlis vault`
//! command refuses while it cannot; and for an entry no command can use: one that is not an
//! object, one whose name purlis refuses, or one whose provider purlis does not implement.
//! Names only, through [`crate::personas::one_line`]: the committed half arrives by `git pull`.

use serde_json::{Map, Value};

use super::{Doctor, Row};
use crate::secrets::registry;
use crate::secrets::{Ctx, Env};

/// The row's name, as the Python charter printed it.
pub(super) const NAME: &str = "vault registry";

/// The row for the project `d` answers for.
pub(super) fn vault_registry(d: &Doctor) -> Row {
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    row_for(&Ctx::new(&d.root, Env::from_process()))
}

/// The row for the project `ctx` names.
pub(super) fn row_for(ctx: &Ctx) -> Row {
    let halves =
        registry::load_shared(ctx).and_then(|shared| Ok((shared, registry::load_local(ctx)?)));
    let (shared, local) = match halves {
        Ok(halves) => halves,
        Err(e) => {
            return Row::warn(
                NAME,
                super::one_line(&e.message, super::PATH_DISPLAY_LIMIT),
                "Every `purlis secret` and `purlis vault` command refuses while the registry \
                 cannot be read. Fix the file named above, or move it aside and register the \
                 vaults again with `purlis vault add`.",
            );
        }
    };
    let mut unusable: Vec<String> = Vec::new();
    for half in [&shared, &local] {
        unusable.extend(not_vaults(half));
    }
    let merged = registry::merged(&shared, &local);
    let mut usable: Vec<String> = Vec::new();
    for (name, _) in registry::vaults(&merged) {
        match registry::vault_in(&merged, &name) {
            Ok(vault) => usable.push(format!("'{}' ({})", shown(&name), vault.provider)),
            Err(e) => unusable.push(format!(
                "'{}' ({})",
                shown(&name),
                super::one_line(&e.message, super::DISPLAY_LIMIT)
            )),
        }
    }
    usable.sort();
    unusable.sort();
    unusable.dedup();
    let registered = if usable.is_empty() {
        "no vaults registered".to_owned()
    } else {
        format!(
            "{} vault(s) registered: {}",
            usable.len(),
            usable.join(", ")
        )
    };
    if unusable.is_empty() {
        return Row::ok(NAME, registered);
    }
    Row::warn(
        NAME,
        format!(
            "{} no command can use: {}; {registered}",
            if unusable.len() == 1 {
                "1 entry".to_owned()
            } else {
                format!("{} entries", unusable.len())
            },
            unusable.join("; ")
        ),
        "A vault the registry cannot use reads no secret. Fix its entry in vaults.json or this \
         machine's half, or remove it with `purlis vault remove <name>`.",
    )
}

/// The entries of one half that are not vaults at all: not an object, or under a name purlis
/// refuses. [`registry::usable_vaults`] drops both silently; this names them.
fn not_vaults(half: &Map<String, Value>) -> Vec<String> {
    let Some(entries) = half.get("vaults") else {
        return Vec::new();
    };
    let Some(entries) = entries.as_object() else {
        return vec!["'vaults' (not an object of vaults)".to_owned()];
    };
    entries
        .iter()
        .filter_map(|(name, entry)| {
            if !registry::name_ok(name) {
                Some(format!(
                    "'{}' (not a vault name purlis accepts)",
                    shown(name)
                ))
            } else if !entry.is_object() {
                Some(format!("'{}' (not a vault entry)", shown(name)))
            } else {
                None
            }
        })
        .collect()
}

/// A name from a registry, on one line and short.
fn shown(name: &str) -> String {
    crate::personas::one_line(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::Status;
    use crate::secrets::cmd::tests::Plane;
    use serde_json::json;

    #[test]
    fn a_project_with_no_vaults_says_so_on_a_green_row() {
        let plane = Plane::new(&[]);

        let row = row_for(&plane.ctx);

        assert_eq!(row.name, NAME);
        assert_eq!(row.status, Status::Ok);
        assert_eq!(row.detail, "no vaults registered");
    }

    #[test]
    fn each_registered_vault_is_named_with_its_provider_and_never_a_value() {
        let plane = Plane::new(&[]);
        plane.plain("home", json!({"K": "never-printed-994a"}));
        plane.register("home", "plain-file", json!({}), Some("steward"));
        plane.register("box", "keyring", json!({}), None);

        let row = row_for(&plane.ctx);

        assert_eq!(row.status, Status::Ok, "{row:?}");
        assert_eq!(
            row.detail,
            "2 vault(s) registered: 'box' (keyring), 'home' (plain-file)"
        );
        assert!(!row.detail.contains("never-printed-994a"));
        assert!(!row.deferred());
    }

    #[test]
    fn a_vault_on_a_provider_purlis_does_not_implement_is_a_warning_naming_it() {
        let plane = Plane::new(&[]);
        plane.register("good", "keyring", json!({}), None);
        plane.register("odd", "carrier-pigeon", json!({}), None);

        let row = row_for(&plane.ctx);

        assert_eq!(row.status, Status::Warn, "{row:?}");
        assert!(
            row.detail.starts_with(
                "1 entry no command can use: 'odd' (vault 'odd' uses unknown provider \
                 'carrier-pigeon'); 1 vault(s) registered: 'good' (keyring)"
            ),
            "{}",
            row.detail
        );
        assert!(
            row.hint.contains("purlis vault remove <name>"),
            "{}",
            row.hint
        );
    }

    #[test]
    fn an_entry_that_is_not_a_vault_is_named_rather_than_dropped() {
        let plane = Plane::new(&[]);
        std::fs::write(
            plane.ctx.shared_registry(),
            json!({"vaults": {"s": "a string", "-flag": {"provider": "keyring"}}}).to_string(),
        )
        .unwrap();

        let row = row_for(&plane.ctx);

        assert_eq!(row.status, Status::Warn, "{row:?}");
        assert!(
            row.detail.contains("'s' (not a vault entry)"),
            "{}",
            row.detail
        );
        assert!(
            row.detail
                .contains("'-flag' (not a vault name purlis accepts)"),
            "{}",
            row.detail
        );
        assert!(
            row.detail.ends_with("; no vaults registered"),
            "{}",
            row.detail
        );
    }

    #[test]
    fn a_registry_that_is_not_json_is_a_warning_naming_the_file() {
        let plane = Plane::new(&[]);
        std::fs::write(plane.ctx.shared_registry(), "{ not json").unwrap();

        let row = row_for(&plane.ctx);

        assert_eq!(row.status, Status::Warn, "{row:?}");
        assert!(
            row.detail.contains("vaults.json is corrupt"),
            "{}",
            row.detail
        );
        assert!(row.hint.contains("purlis vault add"), "{}", row.hint);
    }
}
