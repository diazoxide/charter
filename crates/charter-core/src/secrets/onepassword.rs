//! The 1Password provider — `charter/secrets/onepassword.py`: charter reads and writes
//! 1Password items through the `op` CLI.
//!
//! **One item per vault**, whose custom fields are the secrets:
//!
//! ```text
//! charter vault 'devops', key 'AWS_ACCESS_KEY_ID'
//!     → 1Password item  charter-devops   (or `op-item` from the registry)
//!       tagged          charter, charter:devops
//!       field           AWS_ACCESS_KEY_ID, concealed
//! ```
//!
//! The `op` invocations, exactly as Python makes them (`--account <A>` appended to every one
//! when the vault pins an account):
//!
//! - read one value: `op read --no-newline op://<op-vault>/<op-item>/<key>`;
//! - the item: `op item get <op-item> --vault <op-vault> --format json` (`--reveal` added on the
//!   write path only);
//! - proving absence: `op item list --vault <op-vault> --format json`, and with
//!   `--tags charter:<vault>` for the legacy-layout check;
//! - writes: `op item create - --vault <op-vault>` and `op item edit <op-item> --vault
//!   <op-vault>`, the template on STDIN.
//!
//! **No secret ever reaches argv**, and no error carries `op`'s output: its stderr can echo the
//! assignment it was given, and on a read path its stdout IS the secret.

use serde_json::{Map, Value};

use super::registry::Vault;
use super::run::{self, Ran, RunError};
use super::{Ctx, VaultError};

/// Every item charter creates carries this tag.
const TAG: &str = "charter";

/// The category whose primary field is a concealed password.
const CATEGORY: &str = "PASSWORD";

/// Failures of `op` charter recognises, each a FIXED sentence — `_DIAGNOSES`. `op`'s text is
/// matched against and never interpolated.
const DIAGNOSES: [(&str, &str); 4] = [
    (
        "no accounts configured",
        "`op` has no account configured on this machine. Sign in (`op signin`), or set this \
         vault's service-account token, then retry.",
    ),
    (
        "you do not have permission",
        "1Password refused this as unauthorised. For a service-account token that usually means \
         it has no WRITE access to this vault — the same token reads perfectly well, which is \
         what makes the failure look like a charter bug.",
    ),
    (
        "rate-limited",
        "1Password rate-limited this client. Its contents are UNKNOWN — this is not an empty \
         vault. Wait and retry rather than re-provisioning secrets that are probably there.",
    ),
    (
        "provide the item category",
        "`op` did not parse the JSON template it was given on stdin — that template does declare \
         a category, so the flag op asks for is not the real problem. Do NOT add --category or \
         --title to satisfy it: op then creates the item with every custom field silently \
         dropped, storing nothing while reporting success (issue #78).",
    ),
];

fn diagnose(stderr: &str) -> Option<&'static str> {
    let low = stderr.to_lowercase();
    DIAGNOSES
        .iter()
        .find(|(needle, _)| low.contains(needle))
        .map(|(_, hint)| *hint)
}

/// `op_vault`: the 1Password vault the items live in. Not charter's vault name.
pub fn op_vault(vault: &Vault) -> Result<String, VaultError> {
    let v = super::config_str(&vault.config, "op-vault")
        .filter(|v| !v.is_empty())
        .or_else(|| super::config_str(&vault.config, "op_vault"))
        .map(|v| crate::memstore::py_strip(v).to_string())
        .unwrap_or_default();
    not_a_flag("op-vault", &vault.name, &v)?;
    if v.is_empty() {
        return Err(VaultError::new(format!(
            "vault '{}' has no 'op-vault' configured — which 1Password vault should its items \
             live in? Re-register it:\n  charter vault add {} --provider 1password --op-vault \
             <NAME>",
            vault.name, vault.name
        )));
    }
    Ok(v)
}

/// A configured value that `op` would read as a flag is refused, never passed: the registry is
/// hand-editable and its committed half arrives by `git pull`.
pub fn not_a_flag(what: &str, vault: &str, value: &str) -> Result<(), VaultError> {
    if value.starts_with('-') {
        return Err(VaultError::new(format!(
            "vault '{vault}' has an {what} that starts with '-', which `op` would read as an \
             option; charter will not pass it. Re-register the vault with a real name."
        )));
    }
    Ok(())
}

/// `op_item`: the one item whose fields are this vault's secrets; `charter-<vault>` by default.
pub fn op_item(vault: &Vault) -> Result<String, VaultError> {
    let item = op_item_unchecked(vault);
    not_a_flag("op-item", &vault.name, &item)?;
    Ok(item)
}

fn op_item_unchecked(vault: &Vault) -> String {
    let v = super::config_str(&vault.config, "op-item")
        .filter(|v| !v.is_empty())
        .or_else(|| super::config_str(&vault.config, "op_item"))
        .map(|v| crate::memstore::py_strip(v).to_string())
        .unwrap_or_default();
    if v.is_empty() {
        format!("{TAG}-{}", vault.name)
    } else {
        v
    }
}

fn account(vault: &Vault) -> Option<String> {
    super::config_str(&vault.config, "account")
        .map(|a| crate::memstore::py_strip(a).to_string())
        .filter(|a| !a.is_empty())
}

/// Every configured value that lands in `op`'s argv, checked before any `op` is run.
fn checked(vault: &Vault) -> Result<(), VaultError> {
    op_vault(vault)?;
    op_item(vault)?;
    if let Some(a) = account(vault) {
        not_a_flag("account", &vault.name, &a)?;
    }
    Ok(())
}

/// `_argv`: `op <args…>`, with the account pin appended when the vault has one.
fn op_argv(vault: &Vault, args: &[&str]) -> Vec<String> {
    let mut argv: Vec<String> = std::iter::once("op")
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect();
    if let Some(a) = account(vault) {
        argv.push("--account".into());
        argv.push(a);
    }
    argv
}

/// `_run`: `op` must be on PATH, and runs under this vault's declared identity.
fn op_run(
    ctx: &Ctx,
    vault: &Vault,
    argv: &[String],
    stdin: Option<&str>,
) -> Result<Ran, VaultError> {
    checked(vault)?;
    // A keyring-held identity runs exactly the `op` pinned when its token was stored — never one
    // the caller's PATH resolves, which a chat controls (#271 review, U1). A vault whose identity
    // is not in the keyring keeps resolving `op` from PATH: no keyring item is at stake.
    let mut argv = argv.to_vec();
    match super::identity::pinned_op(ctx, vault)? {
        Some(path) => argv[0] = path.display().to_string(),
        None => {
            if ctx.which("op").is_none() {
                return Err(VaultError::new(
                    "the 1Password CLI ('op') is not on PATH. Install it and sign in \
                     (https://developer.1password.com/docs/cli/), then retry.",
                ));
            }
        }
    }
    let overlay = super::env_overlay(ctx, vault)?;
    run::run(&ctx.env, &argv, stdin, &overlay, None).map_err(|e| match e {
        RunError::Timeout => VaultError::new("`op` did not finish and was stopped."),
        RunError::Interrupted(_) => VaultError::interrupted(),
        RunError::Spawn(e) => VaultError::new(format!("`op` could not be started: {e}")),
    })
}

/// `_fail`: what failed, and a cause only where one was actually recognised.
fn fail(vault: &Vault, what: &str, ran: &Ran, write: bool) -> VaultError {
    let note = format!(
        "{what} failed (op exit {}){}.",
        ran.code,
        super::identity_note(vault)
    );
    if let Some(hint) = diagnose(&ran.stderr) {
        return VaultError::new(format!("{note} {hint}"));
    }
    let mut causes: Vec<&str> = if write {
        vec![
            "the token can read this vault but can not write to it — a service-account token \
             fails here while every read succeeds",
            "another writer changed or removed the item mid-operation",
        ]
    } else {
        vec![
            "the vault or item does not exist under this identity",
            "this vault's identity variable is unset, so `op` read it as somebody else",
        ]
    };
    causes.push("`op` is not signed in, or its session has expired");
    let listed: String = causes.iter().map(|c| format!("    - {c}\n")).collect();
    VaultError::new(format!(
        "{note} charter did not recognise this failure. Causes, roughly in order of how often \
         they are the real one:\n{listed}  Run the same `op` command yourself to see what it said \
         — charter withholds op's output because it can contain the secret."
    ))
}

/// `get`: one field via `op read` — one call, and only this secret.
pub fn get(ctx: &Ctx, vault: &Vault, key: &str) -> Result<String, VaultError> {
    let v = op_vault(vault)?;
    let item = op_item(vault)?;
    let uri = format!("op://{v}/{item}/{key}");
    let ran = op_run(
        ctx,
        vault,
        &op_argv(vault, &["read", "--no-newline", &uri]),
        None,
    )?;
    if ran.code != 0 {
        return Err(VaultError::not_found(format!(
            "no secret '{key}' in vault '{}' (field '{key}' of 1Password item '{item}' in '{v}')",
            vault.name
        )));
    }
    let value = ran.stdout;
    Ok(value.strip_suffix('\n').map(str::to_owned).unwrap_or(value))
}

/// `keys`: the item's field names. Empty when the item PROVABLY does not exist; an error when
/// charter could not tell (#322).
pub fn keys(ctx: &Ctx, vault: &Vault) -> Result<Vec<String>, VaultError> {
    let doc = document(ctx, vault, false)?;
    let mut keys: Vec<String> = fields_of(vault, doc.as_ref())?
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    keys.sort();
    Ok(keys)
}

/// `_list_items`: item titles in the op-vault, tagged as charter's unless `tagged` is false.
fn list_items(ctx: &Ctx, vault: &Vault, tagged: bool) -> Result<Vec<String>, VaultError> {
    let v = op_vault(vault)?;
    let tag = format!("{TAG}:{}", vault.name);
    let mut args = vec!["item", "list", "--vault", v.as_str()];
    if tagged {
        args.push("--tags");
        args.push(tag.as_str());
    }
    args.extend(["--format", "json"]);
    let ran = op_run(ctx, vault, &op_argv(vault, &args), None)?;
    if ran.code != 0 {
        return Err(fail(
            vault,
            &format!("listing vault '{}'", vault.name),
            &ran,
            false,
        ));
    }
    let text = if ran.stdout.is_empty() {
        "[]"
    } else {
        ran.stdout.as_str()
    };
    let data: Value = serde_json::from_str(text).map_err(|e| {
        VaultError::new(format!(
            "could not parse `op item list` output: {}",
            super::registry::py_json_error(&e)
        ))
    })?;
    Ok(data
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .map(|i| i.get("title").map(super::py_str).unwrap_or_default())
        .collect())
}

/// `_document`: the item as `op item get` returns it, or `None` when it PROVABLY is not there.
/// An error when charter could not tell — never `None` for a failed read.
pub fn document(
    ctx: &Ctx,
    vault: &Vault,
    reveal: bool,
) -> Result<Option<Map<String, Value>>, VaultError> {
    let v = op_vault(vault)?;
    let item = op_item(vault)?;
    let mut args = vec![
        "item",
        "get",
        item.as_str(),
        "--vault",
        v.as_str(),
        "--format",
        "json",
    ];
    if reveal {
        args.push("--reveal");
    }
    let ran = op_run(ctx, vault, &op_argv(vault, &args), None)?;
    if ran.code != 0 {
        if list_items(ctx, vault, false)?.contains(&item) {
            return Err(fail(
                vault,
                &format!("reading vault '{}'", vault.name),
                &ran,
                false,
            ));
        }
        return Ok(None);
    }
    let text = if ran.stdout.is_empty() {
        "{}"
    } else {
        ran.stdout.as_str()
    };
    let doc: Value = serde_json::from_str(text).map_err(|e| {
        VaultError::new(format!(
            "could not parse 1Password item '{item}': {}",
            super::registry::py_json_error(&e)
        ))
    })?;
    match doc {
        Value::Object(map) => Ok(Some(map)),
        _ => Err(VaultError::new(format!(
            "could not parse 1Password item '{item}': `op item get` succeeded but did not return \
             a JSON object"
        ))),
    }
}

/// `_field_name`: a field's label, falling back to its id (#75).
fn field_name(f: &Map<String, Value>) -> String {
    let pick = |k: &str| {
        f.get(k)
            .filter(|v| !matches!(v, Value::Null) && v.as_str() != Some(""))
            .map(super::py_str)
    };
    pick("label").or_else(|| pick("id")).unwrap_or_default()
}

/// `_fields_of`: a document's secrets, in its field order. A field with no `value` key is not
/// a secret; two fields sharing a label is an error, never a silent winner.
pub fn fields_of(
    vault: &Vault,
    doc: Option<&Map<String, Value>>,
) -> Result<Vec<(String, Value)>, VaultError> {
    let mut out: Vec<(String, Value)> = Vec::new();
    let fields = doc
        .and_then(|d| d.get("fields"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for f in fields.iter().filter_map(Value::as_object) {
        let name = field_name(f);
        let Some(value) = f.get("value") else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        if out.iter().any(|(n, _)| *n == name) {
            return Err(VaultError::new(format!(
                "1Password item '{}' has more than one field labelled '{name}', so charter cannot \
                 tell which secret that key means. Rename one in 1Password, then retry.",
                op_item_unchecked(vault)
            )));
        }
        out.push((name, value.clone()));
    }
    Ok(out)
}

/// `_ids_of`: a document's field name → 1Password's id, for every field that has an id.
fn ids_of(doc: Option<&Map<String, Value>>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let fields = doc
        .and_then(|d| d.get("fields"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for f in fields.iter().filter_map(Value::as_object) {
        let name = field_name(f);
        let id = f
            .get("id")
            .filter(|v| !matches!(v, Value::Null) && v.as_str() != Some(""));
        if let (false, Some(id)) = (name.is_empty(), id) {
            out.retain(|(n, _)| *n != name);
            out.push((name, super::py_str(id)));
        }
    }
    out
}

/// `_write`: replace the item with exactly `fields`; the template travels on stdin.
fn write(
    ctx: &Ctx,
    vault: &Vault,
    fields: &[(String, Value)],
    ids: &[(String, String)],
    creating: bool,
) -> Result<(), VaultError> {
    let v = op_vault(vault)?;
    let item = op_item(vault)?;
    let mut sorted: Vec<&(String, Value)> = fields.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let rendered: Vec<Value> = sorted
        .iter()
        .map(|(k, val)| {
            let id = ids
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, id)| id.clone())
                .unwrap_or_else(|| k.clone());
            let mut f = Map::new();
            f.insert("id".into(), Value::String(id));
            f.insert("label".into(), Value::String(k.clone()));
            f.insert("type".into(), Value::String("CONCEALED".into()));
            f.insert("value".into(), val.clone());
            Value::Object(f)
        })
        .collect();
    let mut template = Map::new();
    template.insert("title".into(), Value::String(item.clone()));
    template.insert("category".into(), Value::String(CATEGORY.into()));
    template.insert(
        "tags".into(),
        Value::Array(vec![
            Value::String(TAG.into()),
            Value::String(format!("{TAG}:{}", vault.name)),
        ]),
    );
    template.insert("fields".into(), Value::Array(rendered));
    let template = crate::pyjson::dumps(&Value::Object(template), None, ", ", ": ");
    let (argv, what) = if creating {
        (
            op_argv(vault, &["item", "create", "-", "--vault", &v]),
            format!("creating 1Password item '{item}'"),
        )
    } else {
        (
            op_argv(vault, &["item", "edit", &item, "--vault", &v]),
            format!("updating 1Password item '{item}'"),
        )
    };
    let ran = op_run(ctx, vault, &argv, Some(&template))?;
    if ran.code != 0 {
        return Err(fail(vault, &what, &ran, true));
    }
    Ok(())
}

/// `set`: create or replace one field, siblings untouched — a read-modify-write with
/// `--reveal`, verified by reading the field back.
pub fn set(ctx: &Ctx, vault: &Vault, key: &str, value: &str) -> Result<(), VaultError> {
    let doc = document(ctx, vault, true)?;
    let mut fields = fields_of(vault, doc.as_ref())?;
    match fields.iter_mut().find(|(n, _)| n == key) {
        Some(slot) => slot.1 = Value::String(value.to_string()),
        None => fields.push((key.to_string(), Value::String(value.to_string()))),
    }
    write(ctx, vault, &fields, &ids_of(doc.as_ref()), doc.is_none())?;
    let v = op_vault(vault)?;
    let item = op_item(vault)?;
    match get(ctx, vault, key) {
        Ok(got) if got == value => Ok(()),
        Ok(_) => Err(VaultError::new(format!(
            "wrote '{key}' to 1Password item '{item}' but reading it back did not return what was \
             written. Another writer may have replaced the item between the read and the write — \
             1Password keeps item history, so check the item's previous versions before retrying."
        ))),
        Err(e) => {
            let why = if e.kind == super::Kind::NotFound {
                "`op read` exited non-zero, which it does both for a field that is not there and \
                 for every way a read can fail, so charter cannot tell which happened"
                    .to_string()
            } else {
                e.message
            };
            Err(VaultError::new(format!(
                "wrote '{key}' to 1Password item '{item}' in '{v}' but could not read it back, so \
                 charter cannot confirm what the item now holds — {why}. The likeliest cause is TWO \
                 items titled '{item}' in '{v}': another writer creating it between charter proving \
                 it absent and charter's own `op item create` leaves the title ambiguous, and a \
                 field read by title then fails. `op item list --vault {v}` shows whether the title \
                 appears twice; a rate limit or an expired `op` session looks the same from here. Do \
                 not treat this as the secret being absent — 1Password keeps item history, so check \
                 the item before re-provisioning anything."
            )))
        }
    }
}

/// `delete`: remove one field. The item survives — it is the vault, not the secret.
pub fn delete(ctx: &Ctx, vault: &Vault, key: &str) -> Result<(), VaultError> {
    let doc = document(ctx, vault, true)?;
    let mut fields = fields_of(vault, doc.as_ref())?;
    let before = fields.len();
    fields.retain(|(n, _)| n != key);
    if fields.len() == before {
        return Err(VaultError::not_found(format!(
            "no secret '{key}' in vault '{}'",
            vault.name
        )));
    }
    write(ctx, vault, &fields, &ids_of(doc.as_ref()), false)
}

/// `health`: whether the item is reachable and how many fields it holds. Never a value.
pub fn health(ctx: &Ctx, vault: &Vault) -> (bool, String) {
    if ctx.which("op").is_none() {
        return (false, "op CLI not on PATH".into());
    }
    let item = match op_item(vault) {
        Ok(item) => item,
        Err(e) => return (false, e.message),
    };
    let n = match keys(ctx, vault) {
        Ok(k) => k.len(),
        Err(e) => {
            return (
                false,
                e.message.split('.').next().unwrap_or_default().to_string(),
            );
        }
    };
    if n == 0 {
        let prefix = format!("{TAG}-{}-", vault.name);
        let legacy = match list_items(ctx, vault, true) {
            Ok(items) => items.into_iter().filter(|t| t.starts_with(&prefix)).count(),
            Err(e) => {
                return (
                    false,
                    e.message.split('.').next().unwrap_or_default().to_string(),
                );
            }
        };
        if legacy > 0 {
            return (
                false,
                format!(
                    "{legacy} item(s) from the old one-item-per-key layout are not readable as this \
                     vault — charter now keeps a vault in a single item ('{item}'). Their \
                     credentials still exist in 1Password; re-register them with `charter secret \
                     set`, then delete the old items."
                ),
            );
        }
        return (true, format!("no secrets yet in item '{item}'"));
    }
    (true, format!("{n} secret(s) in 1Password item '{item}'"))
}

#[cfg(test)]
#[path = "onepassword_tests.rs"]
mod tests;
