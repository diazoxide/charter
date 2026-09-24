//! The plane's vaults, as the window reaches them.

use charter_core::secrets::cmd;
use charter_core::secrets::keyring;
use charter_core::secrets::registry::{self, Vault};
use charter_core::secrets::{Ctx, Env, VaultError, env_overlay};

use crate::planes::{PlaneId, Planes};

/// Whether a vault can be read, and the provider's own sentence about it. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct VaultHealth {
    pub ok: bool,
    pub detail: String,
}

/// One registered vault, as the Vaults panel lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct VaultSummary {
    pub name: String,
    pub provider: String,
    pub count: Option<u32>,
    pub health: VaultHealth,
}

/// A value the window hands over to be stored, and the only way one enters.
#[derive(serde::Deserialize, specta::Type)]
#[serde(transparent)]
pub(crate) struct SecretValue(String);

impl From<&str> for SecretValue {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretValue(***)")
    }
}

/// A vault failure as the window receives it: the core's sentence, which never holds a value.
fn message_of(e: VaultError) -> String {
    e.message
}

/// A count as the wire carries it. A vault never holds four billion secrets, and a number
/// that could not say so would be the wrong one rather than a big one.
fn counted(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The provider's own health line, or — for a vault read through an identity variable that is
/// unset — the first line of the core's sentence saying so.
fn health(ctx: &Ctx, v: &Vault) -> VaultHealth {
    match env_overlay(ctx, v) {
        Err(e) => VaultHealth {
            ok: false,
            detail: e.message.lines().next().unwrap_or_default().to_owned(),
        },
        Ok(_) => {
            let (ok, detail) = cmd::health(ctx, v);
            VaultHealth { ok, detail }
        }
    }
}

/// How many secrets the vault holds, where charter can say so without a network: the keys
/// index for a keyring vault, the file for a plain-file or reference one. `None` for a
/// 1Password vault, whose count is `op`'s to give and is not worth a round trip per panel draw.
fn count(ctx: &Ctx, v: &Vault) -> Option<u32> {
    let n = match v.provider.as_str() {
        "keyring" => keyring::load_index(ctx, v).ok()?.keys.len(),
        "plain-file" | "reference" => cmd::keys(ctx, v).ok()?.len(),
        _ => return None,
    };
    Some(counted(n))
}

/// Every vault the plane registers, by name.
pub(crate) fn list(ctx: &Ctx) -> Result<Vec<VaultSummary>, String> {
    let doc = registry::load_registry(ctx).map_err(message_of)?;
    let mut names: Vec<String> = registry::vaults(&doc).keys().cloned().collect();
    names.sort();
    Ok(names
        .into_iter()
        .map(|name| match registry::vault_in(&doc, &name) {
            Ok(v) => VaultSummary {
                count: count(ctx, &v),
                health: health(ctx, &v),
                provider: v.provider,
                name,
            },
            Err(e) => VaultSummary {
                provider: String::new(),
                count: None,
                health: VaultHealth {
                    ok: false,
                    detail: e.message,
                },
                name,
            },
        })
        .collect())
}

/// One secret, as a vault's table shows it: its name, and what the keys index knows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct VaultSecret {
    pub key: String,
    pub size: Option<String>,
    pub updated: Option<String>,
}

/// One vault, opened.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct VaultContents {
    pub name: String,
    pub provider: String,
    pub count: u32,
    pub health: VaultHealth,
    pub secrets: Vec<VaultSecret>,
}

/// One vault's secrets, by name.
pub(crate) fn open(ctx: &Ctx, vault: &str) -> Result<VaultContents, String> {
    let v = cmd::provider(ctx, vault).map_err(message_of)?;
    let secrets: Vec<VaultSecret> = if v.provider == "keyring" {
        keyring::listed(ctx, &v)
            .map_err(message_of)?
            .into_iter()
            .map(|l| VaultSecret {
                key: l.key,
                size: Some(l.size).filter(|s| !s.is_empty()),
                updated: Some(l.updated).filter(|s| !s.is_empty()),
            })
            .collect()
    } else {
        cmd::keys(ctx, &v)
            .map_err(message_of)?
            .into_iter()
            .map(|key| VaultSecret {
                key,
                size: None,
                updated: None,
            })
            .collect()
    };
    Ok(VaultContents {
        count: counted(secrets.len()),
        health: health(ctx, &v),
        name: v.name,
        provider: v.provider,
        secrets,
    })
}

/// Whether `key` can name a secret in every provider: not empty, not only spaces, no control
/// character.
fn check_key(key: &str) -> Result<(), String> {
    if key.trim().is_empty() || key.chars().any(char::is_control) {
        return Err(
            "a secret needs a name that is not empty and holds no control character".into(),
        );
    }
    Ok(())
}

/// The vault, ready to be written: registered, and not a plaintext file git would commit.
fn writable(ctx: &Ctx, vault: &str) -> Result<Vault, String> {
    let v = cmd::provider(ctx, vault).map_err(message_of)?;
    cmd::plaintext_refusal(ctx, &v).map_err(message_of)?;
    Ok(v)
}

/// Whether the vault holds `key`, read from its keys (the index, for a keyring vault).
fn holds(ctx: &Ctx, v: &Vault, key: &str) -> Result<bool, String> {
    Ok(cmd::keys(ctx, v)
        .map_err(message_of)?
        .iter()
        .any(|k| k == key))
}

/// Which write the window asked for: a key that must be new, or one that must be held.
#[derive(Clone, Copy)]
enum Writing {
    Add,
    Edit,
}

/// Write `value` under `key`, refusing an add of a held key and an edit of a missing one.
fn write(
    ctx: &Ctx,
    vault: &str,
    key: &str,
    value: &SecretValue,
    writing: Writing,
) -> Result<VaultContents, String> {
    check_key(key)?;
    if value.0.is_empty() {
        return Err(format!(
            "refusing to store an empty value for '{key}' — it would read as a present, healthy \
             secret everywhere charter looks."
        ));
    }
    let v = writable(ctx, vault)?;
    match (writing, holds(ctx, &v, key)?) {
        (Writing::Add, true) => {
            return Err(format!(
                "vault '{vault}' already holds '{key}'. Edit its value instead of adding it again."
            ));
        }
        (Writing::Edit, false) => {
            return Err(format!("secret '{key}' not found in vault '{vault}'"));
        }
        _ => {}
    }
    cmd::set_value(ctx, &v, key, &value.0).map_err(message_of)?;
    open(ctx, vault)
}

/// Store a new secret under `key`. A key the vault already holds is refused.
pub(crate) fn add(
    ctx: &Ctx,
    vault: &str,
    key: &str,
    value: &SecretValue,
) -> Result<VaultContents, String> {
    write(ctx, vault, key, value, Writing::Add)
}

/// Replace the value of a key the vault holds. A key it does not hold is refused.
pub(crate) fn set(
    ctx: &Ctx,
    vault: &str,
    key: &str,
    value: &SecretValue,
) -> Result<VaultContents, String> {
    write(ctx, vault, key, value, Writing::Edit)
}

/// Move the secret under `from` to `to` ([`cmd::rename`]).
pub(crate) fn rename(
    ctx: &Ctx,
    vault: &str,
    from: &str,
    to: &str,
) -> Result<VaultContents, String> {
    check_key(to)?;
    let v = writable(ctx, vault)?;
    cmd::rename(ctx, &v, from, to).map_err(message_of)?;
    open(ctx, vault)
}

/// Delete the secret under `key`.
pub(crate) fn delete(ctx: &Ctx, vault: &str, key: &str) -> Result<VaultContents, String> {
    let v = cmd::provider(ctx, vault).map_err(message_of)?;
    cmd::delete(ctx, &v, key).map_err(message_of)?;
    open(ctx, vault)
}

// ---------------------------------------------------------------------------------------
// The commands. Each resolves its plane on the thread that asked and does the work on a
// blocking one: a 1Password vault's health and keys run `op`, which can take seconds.

/// The plane's vault context, with this process's environment.
fn ctx_of(planes: &Planes, plane: &PlaneId) -> Result<Ctx, String> {
    Ok(Ctx::new(planes.held(plane)?.root(), Env::from_process()))
}

/// `work` on a blocking thread, answered on this one.
async fn blocking<T: Send + 'static>(
    ctx: Ctx,
    work: impl FnOnce(&Ctx) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&ctx))
        .await
        .map_err(|err| format!("reading the vault did not finish: {err}"))?
}

/// Every vault the plane registers: name, provider, secret count and health. Never a value.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_list(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<VaultSummary>, String> {
    blocking(ctx_of(&planes, &plane)?, list).await
}

/// One vault's secrets: names, size bands and when each was written. Never a value.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_open(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| open(ctx, &vault)).await
}

/// One vault read again, after something outside the window may have changed it — a
/// `charter secret set` in a terminal. The same reading as `vault_open`, from the keys index for
/// a keyring vault, so a refresh never makes the Keychain ask anything. Never a value.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_refresh(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| open(ctx, &vault)).await
}

/// Store a new secret. The value comes in here and goes nowhere but the vault.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_secret_add(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
    key: String,
    value: SecretValue,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        add(ctx, &vault, &key, &value)
    })
    .await
}

/// Replace a held secret's value. The value comes in here and goes nowhere but the vault.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_secret_set(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
    key: String,
    value: SecretValue,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        set(ctx, &vault, &key, &value)
    })
    .await
}

/// Move a secret to a new name. No value crosses.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_secret_rename(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
    from: String,
    to: String,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        rename(ctx, &vault, &from, &to)
    })
    .await
}

/// Delete a secret. No value crosses.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_secret_delete(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
    key: String,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        delete(ctx, &vault, &key)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use charter_core::secrets::registry;

    /// A plane with a keyring vault `ops` and a plain-file vault `files`, both registered
    /// locally. A test build keeps the keyring in `<state>/keyring-stub.json`.
    fn plane() -> (tempfile::TempDir, Ctx) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Ctx::new(dir.path(), Env::of(&[]));
        registry::add_vault(
            &ctx,
            "ops",
            "keyring",
            Default::default(),
            None,
            false,
            false,
        )
        .unwrap();
        register_file_vault(&ctx, "files", "plain-file");
        (dir, ctx)
    }

    /// Register `name` as a vault kept in a file under the state directory.
    fn register_file_vault(ctx: &Ctx, name: &str, provider: &str) {
        let file = ctx.vaults_dir().join(format!("{name}.json"));
        let mut config = serde_json::Map::new();
        config.insert(
            "file".into(),
            serde_json::Value::String(file.to_string_lossy().into_owned()),
        );
        registry::add_vault(ctx, name, provider, config, None, false, false).unwrap();
    }

    #[test]
    fn the_list_names_each_vault_with_its_provider_and_how_many_secrets_it_holds() {
        let (_dir, ctx) = plane();
        add(&ctx, "ops", "A", &SecretValue::from("list-value-a-3e1")).unwrap();
        add(&ctx, "ops", "B", &SecretValue::from("list-value-b-7f2")).unwrap();

        let listed = list(&ctx).unwrap();

        let shown: Vec<(&str, &str, Option<u32>, bool)> = listed
            .iter()
            .map(|v| (v.name.as_str(), v.provider.as_str(), v.count, v.health.ok))
            .collect();
        assert_eq!(
            shown,
            [
                ("files", "plain-file", Some(0), true),
                ("ops", "keyring", Some(2), true)
            ]
        );
    }

    #[test]
    fn listing_or_opening_a_keyring_vault_reads_the_index_and_never_the_store() {
        let (dir, ctx) = plane();
        add(
            &ctx,
            "ops",
            "API_TOKEN",
            &SecretValue::from("open-value-0123456789"),
        )
        .unwrap();
        // A store that cannot be read: an open that asked it anything would fail.
        std::fs::write(dir.path().join(".charter/keyring-stub.json"), "not json").unwrap();

        let opened = open(&ctx, "ops").unwrap();
        let listed = list(&ctx).unwrap();

        assert_eq!(
            (opened.name.as_str(), opened.provider.as_str()),
            ("ops", "keyring")
        );
        assert_eq!(opened.count, 1);
        assert!(opened.health.ok, "{:?}", opened.health);
        let [secret] = opened.secrets.as_slice() else {
            panic!("{:?}", opened.secrets)
        };
        assert_eq!(secret.key, "API_TOKEN");
        assert_eq!(secret.size.as_deref(), Some("16–31 bytes"));
        assert!(
            secret.updated.as_deref().is_some_and(|t| t.ends_with('Z')),
            "{secret:?}"
        );
        let ops = listed.iter().find(|v| v.name == "ops").unwrap();
        assert_eq!((ops.count, ops.health.ok), (Some(1), true), "{ops:?}");
    }

    #[test]
    fn a_vault_charter_does_not_track_sizes_for_lists_names_alone() {
        let (_dir, ctx) = plane();
        add(
            &ctx,
            "files",
            "DB_URL",
            &SecretValue::from("postgres://open-fixture"),
        )
        .unwrap();

        let opened = open(&ctx, "files").unwrap();

        assert_eq!(opened.count, 1);
        assert_eq!(
            opened.secrets,
            [VaultSecret {
                key: "DB_URL".into(),
                size: None,
                updated: None
            }]
        );
    }

    #[test]
    fn opening_a_vault_the_plane_does_not_register_says_so() {
        let (_dir, ctx) = plane();
        let err = open(&ctx, "nope").unwrap_err();
        assert!(err.contains("nope"), "{err}");
    }
    fn keys(opened: &VaultContents) -> Vec<&str> {
        opened.secrets.iter().map(|s| s.key.as_str()).collect()
    }

    fn value(ctx: &Ctx, vault: &str, key: &str) -> String {
        let v = cmd::provider(ctx, vault).unwrap();
        cmd::get_value(ctx, &v, key).unwrap()
    }

    #[test]
    fn adding_a_secret_answers_with_the_vault_as_it_now_is() {
        let (_dir, ctx) = plane();
        let opened = add(&ctx, "ops", "API_TOKEN", &SecretValue::from("add-value-5a")).unwrap();
        assert_eq!(keys(&opened), ["API_TOKEN"]);
        assert_eq!(value(&ctx, "ops", "API_TOKEN"), "add-value-5a");
    }

    #[test]
    fn adding_a_key_the_vault_already_holds_is_refused_and_the_value_stays() {
        let (_dir, ctx) = plane();
        add(
            &ctx,
            "ops",
            "API_TOKEN",
            &SecretValue::from("first-value-11"),
        )
        .unwrap();

        let err = add(
            &ctx,
            "ops",
            "API_TOKEN",
            &SecretValue::from("second-value-22"),
        )
        .unwrap_err();

        assert!(err.contains("API_TOKEN"), "{err}");
        assert_eq!(value(&ctx, "ops", "API_TOKEN"), "first-value-11");
    }

    #[test]
    fn an_empty_value_or_a_key_that_is_not_a_name_is_refused_before_anything_is_written() {
        let (dir, ctx) = plane();
        for (key, val) in [("EMPTY", ""), ("", "v-1"), ("  ", "v-2"), ("A\nB", "v-3")] {
            assert!(
                add(&ctx, "ops", key, &SecretValue::from(val)).is_err(),
                "{key:?}"
            );
        }
        assert!(!dir.path().join(".charter/keyring-stub.json").exists());
        assert_eq!(open(&ctx, "ops").unwrap().count, 0);
    }

    #[test]
    fn setting_a_value_replaces_the_one_a_held_key_had() {
        let (_dir, ctx) = plane();
        add(&ctx, "files", "DB_URL", &SecretValue::from("old-value-33")).unwrap();

        let opened = set(&ctx, "files", "DB_URL", &SecretValue::from("new-value-44")).unwrap();

        assert_eq!(keys(&opened), ["DB_URL"]);
        assert_eq!(value(&ctx, "files", "DB_URL"), "new-value-44");
    }

    #[test]
    fn setting_a_key_the_vault_does_not_hold_is_refused_rather_than_added() {
        let (_dir, ctx) = plane();
        assert!(set(&ctx, "ops", "NEVER_ADDED", &SecretValue::from("v-55")).is_err());
        assert_eq!(open(&ctx, "ops").unwrap().count, 0);
    }

    #[test]
    fn renaming_and_deleting_answer_with_the_vault_as_it_now_is() {
        let (_dir, ctx) = plane();
        add(&ctx, "ops", "OLD", &SecretValue::from("moving-value-66")).unwrap();
        add(&ctx, "ops", "GONE", &SecretValue::from("deleted-value-77")).unwrap();

        assert_eq!(
            keys(&rename(&ctx, "ops", "OLD", "NEW").unwrap()),
            ["GONE", "NEW"]
        );
        assert_eq!(value(&ctx, "ops", "NEW"), "moving-value-66");
        assert_eq!(keys(&delete(&ctx, "ops", "GONE").unwrap()), ["NEW"]);
    }
    /// Every value the no-value test writes, one per provider and one per write.
    const FIXTURES: [&str; 6] = [
        "fixture-keyring-5f0c9a2e71d4",
        "fixture-keyring-edited-8b3e0f",
        "fixture-plain-2c7d51e9a0b6",
        "fixture-plain-edited-4e19a7",
        "op://Fixture Vault/fixture-item/fixture-field",
        "fixture-refused-duplicate-93aa",
    ];

    /// Whatever crossed to the window, as the window receives it.
    fn wire<T: serde::Serialize>(said: &Result<T, String>) -> String {
        match said {
            Ok(it) => serde_json::to_string(it).unwrap(),
            Err(why) => serde_json::to_string(why).unwrap(),
        }
    }

    #[test]
    fn no_list_open_refresh_or_write_ever_answers_with_a_value() {
        let (dir, ctx) = plane();
        register_file_vault(&ctx, "refs", "reference");
        let v = |s: &str| SecretValue::from(s);

        let mut answers = vec![
            wire(&add(&ctx, "ops", "API_TOKEN", &v(FIXTURES[0]))),
            wire(&set(&ctx, "ops", "API_TOKEN", &v(FIXTURES[1]))),
            wire(&add(&ctx, "files", "DB_URL", &v(FIXTURES[2]))),
            wire(&set(&ctx, "files", "DB_URL", &v(FIXTURES[3]))),
            wire(&add(&ctx, "refs", "DEPLOY", &v(FIXTURES[4]))),
            // Refused writes answer too, and a refusal is the classic place a value leaks.
            wire(&add(&ctx, "ops", "API_TOKEN", &v(FIXTURES[5]))),
            wire(&set(&ctx, "files", "MISSING", &v(FIXTURES[5]))),
            wire(&set(&ctx, "nope", "X", &v(FIXTURES[5]))),
            wire(&rename(&ctx, "ops", "API_TOKEN", "TOKEN")),
            wire(&rename(&ctx, "files", "DB_URL", "DATABASE_URL")),
            wire(&rename(&ctx, "refs", "DEPLOY", "DEPLOY_REF")),
            wire(&list(&ctx)),
        ];
        for vault in ["ops", "files", "refs"] {
            answers.push(wire(&open(&ctx, vault)));
            answers.push(wire(&open(&ctx, vault))); // what `vault_refresh` answers
        }
        answers.push(wire(&delete(&ctx, "ops", "TOKEN")));
        answers.push(wire(&delete(&ctx, "files", "DATABASE_URL")));
        answers.push(wire(&delete(&ctx, "refs", "DEPLOY_REF")));

        // The writes happened, so the absence below is not an absence of values to leak.
        assert!(
            std::fs::read_to_string(dir.path().join(".charter/vaults/files.json"))
                .is_ok_and(|t| !t.contains("fixture-plain")),
            "the plain-file vault was never written"
        );
        for answer in &answers {
            for fixture in FIXTURES {
                assert!(!answer.contains(fixture), "{fixture} in {answer}");
            }
        }
    }

    #[test]
    fn a_value_from_the_window_arrives_as_a_string_and_never_prints_in_debug() {
        let value: SecretValue = serde_json::from_str("\"debug-fixture-61c2\"").unwrap();
        assert_eq!(format!("{value:?}"), "SecretValue(***)");
    }

    #[test]
    fn a_plain_file_vault_that_git_would_commit_is_not_written() {
        let (dir, ctx) = plane();
        charter_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(["init", "-q"]),
        )
        .unwrap();
        let mut file = serde_json::Map::new();
        file.insert(
            "file".into(),
            serde_json::Value::String("secrets.json".into()),
        );
        registry::add_vault(&ctx, "tracked", "plain-file", file, None, false, false).unwrap();

        let err = add(&ctx, "tracked", "K", &SecretValue::from("tracked-value-1")).unwrap_err();

        assert!(err.contains("NOT gitignored"), "{err}");
        assert!(!dir.path().join("secrets.json").exists());
    }
}
