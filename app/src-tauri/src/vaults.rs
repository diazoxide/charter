//! The plane's vaults, as the window reaches them.

use charter_core::secrets::cmd::{self, Io, Say};
use charter_core::secrets::identity::{self, Held};
use charter_core::secrets::keyring;
use charter_core::secrets::registry::{self, Vault};
use charter_core::secrets::vaultcmd;
use charter_core::secrets::{Ctx, Env, VaultError, identity_missing};

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

/// A value the window hands over to be stored, and the one a reveal hands back
/// ([`vault_secret_reveal`], the only command whose answer holds one).
#[derive(serde::Deserialize, serde::Serialize, specta::Type)]
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
    match identity_missing(ctx, v) {
        Some(e) => VaultHealth {
            ok: false,
            detail: e.message.lines().next().unwrap_or_default().to_owned(),
        },
        None => {
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

/// Where one of a vault's identity variables is read from now (#237).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub(crate) enum IdentityHeld {
    /// Moved into the keyring, which is read first.
    Keyring,
    /// In charter's environment, and not moved: the tab offers to move it.
    Environment,
    /// Nowhere: the vault cannot be read.
    Unset,
}

/// One identity variable a vault is read through — `$OP_TEAM_TOKEN` — and where it is. Its
/// NAME, never its value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct VaultIdentity {
    pub variable: String,
    pub held: IdentityHeld,
}

/// One vault, opened.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct VaultContents {
    pub name: String,
    pub provider: String,
    pub count: u32,
    pub health: VaultHealth,
    pub secrets: Vec<VaultSecret>,
    /// The identity variables it is read through; empty for a vault that declares none. Said
    /// from the registry's mark and the environment, never by reading the keyring.
    pub identity: Vec<VaultIdentity>,
    /// Identity variables charter's OWN process environment still carries. A same-user process
    /// can read another's environment block, so while this is non-empty a chat could read the
    /// token however it was moved — the tab warns to relaunch charter without the export
    /// (#271 review, U3). Names only.
    pub identity_in_app_env: Vec<String>,
}

/// Where each of the vault's identity variables is read from now.
fn identity_of(ctx: &Ctx, v: &Vault) -> Vec<VaultIdentity> {
    identity::held(ctx, v)
        .into_iter()
        .map(|b| VaultIdentity {
            variable: b.source,
            held: match b.held {
                Held::Keyring => IdentityHeld::Keyring,
                Held::Environment => IdentityHeld::Environment,
                Held::Unset => IdentityHeld::Unset,
            },
        })
        .collect()
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
        identity: identity_of(ctx, &v),
        identity_in_app_env: identity::app_env_holds_a_token(ctx, &v),
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

/// Where a value handed out by the window went, as the trace names it: a reveal's `to`.
#[derive(Clone, Copy)]
enum Handed {
    Window,
    Clipboard,
}

impl Handed {
    fn as_str(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Clipboard => "clipboard",
        }
    }
}

/// Read `key`'s value and record that it left: the event a terminal's revealing `charter secret
/// get` writes, in its shape (`vault`, `key_names`, `forced`), with `to` saying whether the
/// window showed it or put it on the clipboard. Recorded before it is handed back, as the CLI
/// records before it prints. A read that fails records nothing, because nothing left.
fn handed_out(ctx: &Ctx, vault: &str, key: &str, to: Handed) -> Result<SecretValue, String> {
    use serde_json::Value;
    let v = cmd::provider(ctx, vault).map_err(message_of)?;
    let value = cmd::get_value(ctx, &v, key).map_err(message_of)?;
    cmd::trace_secret_use(
        ctx,
        "secret-reveal",
        std::slice::from_ref(&value),
        &[
            ("vault", Value::String(vault.into())),
            ("key_names", Value::Array(vec![Value::String(key.into())])),
            ("forced", Value::Bool(false)),
            ("to", Value::String(to.as_str().into())),
        ],
    );
    Ok(SecretValue(value))
}

/// One secret's value, for the window to show: the one answer that holds a value. A keyring
/// vault reads the store, so the system may ask the operator first.
pub(crate) fn reveal(ctx: &Ctx, vault: &str, key: &str) -> Result<SecretValue, String> {
    handed_out(ctx, vault, key, Handed::Window)
}

/// The system clipboard, as far as a copy needs it.
pub(crate) trait Clipboard: Send {
    /// The text it holds, when it holds text.
    fn text(&mut self) -> Option<String>;
    /// Put a secret on it, marked for clipboard histories to leave out where the system has a
    /// way to say so.
    fn set_secret(&mut self, text: &str) -> Result<(), String>;
    /// Empty it.
    fn clear(&mut self) -> Result<(), String>;
}

/// How long a copied value may stay on the clipboard (#232, decision 3).
pub(crate) const CLEAR_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

/// The clipboard a vault's Copy writes to, and what it last wrote there.
pub(crate) struct Pasting {
    board: Box<dyn Clipboard>,
    /// A digest of the value last copied, so a clear can tell whether the clipboard still holds
    /// it without keeping the value itself.
    copied: Option<[u8; 32]>,
    /// Which copy that was. A clear scheduled for an earlier copy leaves a later one alone.
    copy: u64,
}

impl Pasting {
    pub(crate) fn new(board: Box<dyn Clipboard>) -> Self {
        Self {
            board,
            copied: None,
            copy: 0,
        }
    }

    /// Clear the clipboard if it still holds the value charter last copied — and, when `copy`
    /// is named, only if that is still the last copy — and answer whether it did. Whatever the
    /// operator copied since is theirs and is left alone.
    fn clear_copied(&mut self, copy: Option<u64>) -> Result<bool, String> {
        if copy.is_some_and(|n| n != self.copy) {
            return Ok(false);
        }
        let Some(waiting) = self.copied.take() else {
            return Ok(false);
        };
        if self.board.text().is_some_and(|now| digest(&now) == waiting) {
            self.board.clear()?;
            return Ok(true);
        }
        Ok(false)
    }
}

impl std::fmt::Debug for Pasting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let waiting = if self.copied.is_some() {
            "waiting"
        } else {
            "none"
        };
        write!(f, "Pasting(copy {}, {waiting})", self.copy)
    }
}

fn digest(text: &str) -> [u8; 32] {
    use sha2::Digest as _;
    sha2::Sha256::digest(text.as_bytes()).into()
}

/// The clipboard, locked, whatever a panic elsewhere left it holding.
fn locked(pasting: &std::sync::Mutex<Pasting>) -> std::sync::MutexGuard<'_, Pasting> {
    pasting
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Put `key`'s value on the clipboard, and answer which copy it was, for [`clear_later`]. The
/// value never reaches the window. It is read before the clipboard is locked, so a Keychain
/// prompt left waiting holds up no clear. A read that fails leaves the clipboard as it was.
pub(crate) fn copy(
    ctx: &Ctx,
    vault: &str,
    key: &str,
    pasting: &std::sync::Mutex<Pasting>,
) -> Result<u64, String> {
    let value = handed_out(ctx, vault, key, Handed::Clipboard)?;
    let mut pasting = locked(pasting);
    pasting.board.set_secret(&value.0)?;
    pasting.copied = Some(digest(&value.0));
    pasting.copy += 1;
    Ok(pasting.copy)
}

/// [`CLEAR_AFTER`] from now, clear the clipboard if copy `copy` is still the last one and the
/// clipboard still holds it. The core keeps this timer rather than the window, so a window that
/// closes or reloads within the minute leaves nothing behind, and a second window's copy is
/// never cleared on the first one's clock.
pub(crate) async fn clear_later(pasting: std::sync::Arc<std::sync::Mutex<Pasting>>, copy: u64) {
    tokio::time::sleep(CLEAR_AFTER).await;
    // Best-effort: the answer is the clipboard's sentence, never a value, and nobody waits on it.
    let _ = tokio::task::spawn_blocking(move || locked(&pasting).clear_copied(Some(copy))).await;
}

/// At quit, clear the clipboard if it still holds what a vault's Copy last put there: the clear
/// that was waiting will not run.
pub(crate) fn clear_now(pasting: &std::sync::Mutex<Pasting>) -> Result<bool, String> {
    locked(pasting).clear_copied(None)
}

/// Put a token the operator pasted into the keyring for the vault's identity
/// ([`identity::put_in_keyring`]), and answer with the vault as it now is. The token comes in
/// here and goes straight to the keyring: it never sits in the app's environment, so no chat can
/// read it from there. The preferred path (#271 review, U3).
pub(crate) fn put_identity(
    ctx: &Ctx,
    vault: &str,
    token: &SecretValue,
) -> Result<VaultContents, String> {
    let v = cmd::provider(ctx, vault).map_err(message_of)?;
    identity::put_in_keyring(ctx, &v, &token.0).map_err(message_of)?;
    open(ctx, vault)
}

/// Move the vault's identity token from charter's environment into the keyring
/// ([`identity::move_to_keyring`]), and answer with the vault as it now is. The token goes from
/// this process's environment to the keyring and nowhere else: not the window, not an error. The
/// app's process keeps its environment block, so [`VaultContents::identity_in_app_env`] then warns
/// the operator to relaunch (#271 review, U3).
pub(crate) fn move_identity(ctx: &Ctx, vault: &str) -> Result<VaultContents, String> {
    let v = cmd::provider(ctx, vault).map_err(message_of)?;
    identity::move_to_keyring(ctx, &v).map_err(message_of)?;
    open(ctx, vault)
}

/// What a new vault is kept in when the window does not say: the system's own credential
/// store (#232, decision 1).
const DEFAULT_PROVIDER: &str = "keyring";

/// `charter vault add`'s refusal, kept rather than printed, so it reaches the window as the
/// sentences a terminal would have shown: the error, and the lines of advice that follow it. What
/// it says on success is the CLI's own report and is dropped — the window answers with the vault.
/// It is asked for names only, so there is no value here to keep, and it answers every question
/// about a terminal with "no".
#[derive(Default)]
struct Kept {
    lines: Vec<String>,
}

impl Io for Kept {
    fn say(&mut self, line: Say) {
        if let Say::Err(text) | Say::Info(text) = line {
            self.lines.push(text);
        }
    }
    fn out(&mut self, _bytes: &[u8]) {}
    fn err(&mut self, _bytes: &[u8]) {}
    fn stdout_is_terminal(&self) -> bool {
        false
    }
    fn stdin_is_terminal(&self) -> bool {
        false
    }
    fn read_stdin(&mut self) -> String {
        String::new()
    }
    fn read_hidden(&mut self, _prompt: &str) -> String {
        String::new()
    }
}

/// Register a new, local vault called `name`, kept by `provider` (the keyring when `None`), and
/// answer with it opened. The path `charter vault add` takes, so the window and a terminal refuse
/// the same names, the same taken name and the same unignored plaintext file, in the same words.
/// A 1Password vault needs `op_vault`, the 1Password vault its items go in.
pub(crate) fn create(
    ctx: &Ctx,
    name: &str,
    provider: Option<&str>,
    op_vault: Option<&str>,
) -> Result<VaultContents, String> {
    let req = vaultcmd::AddRequest {
        name: name.to_owned(),
        provider: provider.unwrap_or(DEFAULT_PROVIDER).to_owned(),
        op_vault: op_vault.map(str::to_owned),
        ..Default::default()
    };
    let mut kept = Kept::default();
    if vaultcmd::add(ctx, &req, &mut kept) != 0 {
        return Err(kept.lines.join("\n"));
    }
    open(ctx, name)
}

/// What deleting a vault took away: its provider, and the secrets deleted from the keyring by
/// name. Never a value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct VaultRemoved {
    pub name: String,
    pub provider: String,
    /// Deleted from the system keyring: empty for every provider but `keyring`, whose file or
    /// 1Password item is left where it is.
    pub destroyed: Vec<String>,
}

/// Delete the vault `name` ([`vaultcmd::destroy`]): a keyring vault's secrets are deleted from
/// the keyring one by one, then the vault is unregistered. An entry the keyring will not delete
/// stops it with the vault still registered.
pub(crate) fn remove(ctx: &Ctx, name: &str) -> Result<VaultRemoved, String> {
    let gone = vaultcmd::destroy(ctx, name).map_err(message_of)?;
    Ok(VaultRemoved {
        name: name.to_owned(),
        provider: gone.provider,
        destroyed: gone.destroyed,
    })
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

/// Make a new vault on this plane, kept by `provider` — the keyring when `null` — and answer
/// with it opened. No value crosses.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_create(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
    provider: Option<String>,
    op_vault: Option<String>,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        create(ctx, &vault, provider.as_deref(), op_vault.as_deref())
    })
    .await
}

/// Delete a vault and, for a keyring vault, every secret it holds ([`remove`]). No value
/// crosses.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_remove(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
) -> Result<VaultRemoved, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| remove(ctx, &vault)).await
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

/// One secret's value, to show in the window for a while ([`reveal`]).
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_secret_reveal(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
    key: String,
) -> Result<SecretValue, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        reveal(ctx, &vault, &key)
    })
    .await
}

/// Put a token the operator pasted into the keyring for a vault's identity ([`put_identity`]).
/// The token comes in here and goes straight to the keyring — never the app's environment, never
/// the window, never an error. The preferred path (#271 review, U3).
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_identity_put(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
    token: SecretValue,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        put_identity(ctx, &vault, &token)
    })
    .await
}

/// Move a vault's identity token from the app's OWN environment into the keyring
/// ([`move_identity`]). No value crosses to the window. Kept beside the paste path for an app
/// launched from a shell that exports the token; the answer's `identity_in_app_env` then warns to
/// relaunch, because the app's process still carries the export (#271 review, U3).
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_identity_move(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    vault: String,
) -> Result<VaultContents, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        move_identity(ctx, &vault)
    })
    .await
}

/// The system clipboard, opened on first use and kept for the app's life: on X11 what a process
/// puts on the clipboard is served by that process's handle, and goes when the handle does.
#[derive(Default)]
struct Arboard(Option<arboard::Clipboard>);

impl Arboard {
    fn open(&mut self) -> Result<&mut arboard::Clipboard, String> {
        match &mut self.0 {
            Some(open) => Ok(open),
            closed => Ok(closed.insert(
                arboard::Clipboard::new()
                    .map_err(|e| format!("the clipboard could not be opened: {e}"))?,
            )),
        }
    }
}

impl Clipboard for Arboard {
    fn text(&mut self) -> Option<String> {
        self.open().ok()?.get_text().ok()
    }
    fn set_secret(&mut self, text: &str) -> Result<(), String> {
        let set = self.open()?.set();
        #[cfg(target_os = "macos")]
        let set = arboard::SetExtApple::exclude_from_history(set);
        #[cfg(windows)]
        let set = arboard::SetExtWindows::exclude_from_history(set);
        #[cfg(all(unix, not(target_os = "macos")))]
        let set = arboard::SetExtLinux::exclude_from_history(set);
        set.text(text)
            .map_err(|e| format!("the clipboard did not take the copy: {e}"))
    }
    fn clear(&mut self) -> Result<(), String> {
        self.open()?
            .clear()
            .map_err(|e| format!("the clipboard could not be cleared: {e}"))
    }
}

/// The system clipboard as the app manages it, shared with the clear each copy schedules.
pub(crate) struct SystemClipboard(std::sync::Arc<std::sync::Mutex<Pasting>>);

impl Default for SystemClipboard {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(Pasting::new(
            Box::new(Arboard::default()),
        ))))
    }
}

impl SystemClipboard {
    /// [`clear_now`], at the app's exit. A failure is not worth refusing to exit over.
    pub(crate) fn clear_at_exit(&self) {
        let _ = clear_now(&self.0);
    }
}

/// Put one secret's value on the clipboard ([`copy`]) and clear it a minute later
/// ([`clear_later`]). The answer is nothing: the value never comes back to the window.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_secret_copy(
    planes: tauri::State<'_, Planes>,
    clipboard: tauri::State<'_, SystemClipboard>,
    plane: PlaneId,
    vault: String,
    key: String,
) -> Result<(), String> {
    let pasting = std::sync::Arc::clone(&clipboard.0);
    let held = std::sync::Arc::clone(&pasting);
    let copied = blocking(ctx_of(&planes, &plane)?, move |ctx| {
        copy(ctx, &vault, &key, &held)
    })
    .await?;
    tauri::async_runtime::spawn(clear_later(pasting, copied));
    Ok(())
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
    fn no_list_open_refresh_write_or_copy_ever_answers_with_a_value() {
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
        // A copy puts a value on the clipboard and answers the window with nothing (the
        // command's `()`); what crosses back is only a refusal. Only `reveal` answers with a value,
        // and it is not asked here.
        let pasting = Pasteboard::default().pasting();
        for (vault, key) in [
            ("ops", "TOKEN"),
            ("files", "DATABASE_URL"),
            ("refs", "DEPLOY_REF"),
            ("ops", "MISSING"),
        ] {
            answers.push(wire(&copy(&ctx, vault, key, &pasting).map(|_| ())));
        }
        answers.push(format!("{:?}", pasting.lock().unwrap()));
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

    /// Every line the plane's trace holds, for a test run outside any session.
    fn traced(ctx: &Ctx) -> Vec<serde_json::Value> {
        let file = charter_core::trace::file(&ctx.root, charter_core::trace::NO_SESSION);
        std::fs::read_to_string(file)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn a_reveal_answers_with_that_one_value_and_is_recorded_as_secret_get_reveal_is() {
        let (_dir, ctx) = plane();
        add(
            &ctx,
            "ops",
            "API_TOKEN",
            &SecretValue::from("reveal-value-0c41"),
        )
        .unwrap();
        add(
            &ctx,
            "ops",
            "OTHER",
            &SecretValue::from("neighbour-value-7d"),
        )
        .unwrap();

        let shown = reveal(&ctx, "ops", "API_TOKEN");

        assert_eq!(wire(&shown), "\"reveal-value-0c41\"");
        let lines = traced(&ctx);
        let [line] = lines.as_slice() else {
            panic!("{lines:?}")
        };
        assert_eq!(line["event"], "secret-reveal");
        assert_eq!(line["vault"], "ops");
        assert_eq!(line["key_names"], serde_json::json!(["API_TOKEN"]));
        assert_eq!(line["to"], "window");
        assert!(!line.to_string().contains("reveal-value"), "{line}");
    }

    #[test]
    fn a_reveal_of_a_key_the_vault_does_not_hold_is_refused_and_records_nothing() {
        let (_dir, ctx) = plane();
        assert!(reveal(&ctx, "ops", "NEVER_ADDED").is_err());
        assert!(traced(&ctx).is_empty());
    }

    /// A clipboard held in memory, so no test touches the operator's. Cloned, it is the same
    /// clipboard: one clone goes to the core, and the test reads and writes through the other.
    #[derive(Clone, Default)]
    struct Pasteboard(std::sync::Arc<std::sync::Mutex<Option<String>>>);

    impl Pasteboard {
        fn holding(text: &str) -> Self {
            let board = Self::default();
            board.put(text);
            board
        }
        fn now(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
        /// What the operator copies, from anywhere.
        fn put(&self, text: &str) {
            *self.0.lock().unwrap() = Some(text.to_owned());
        }
        /// The core's view of it.
        fn pasting(&self) -> std::sync::Arc<std::sync::Mutex<Pasting>> {
            std::sync::Arc::new(std::sync::Mutex::new(Pasting::new(Box::new(self.clone()))))
        }
    }

    impl Clipboard for Pasteboard {
        fn text(&mut self) -> Option<String> {
            self.now()
        }
        fn set_secret(&mut self, text: &str) -> Result<(), String> {
            self.put(text);
            Ok(())
        }
        fn clear(&mut self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn a_copy_puts_the_value_on_the_clipboard_answers_with_nothing_and_is_recorded() {
        let (_dir, ctx) = plane();
        add(
            &ctx,
            "files",
            "DB_URL",
            &SecretValue::from("copied-value-2e9b"),
        )
        .unwrap();
        let board = Pasteboard::default();
        let pasting = board.pasting();

        copy(&ctx, "files", "DB_URL", &pasting).unwrap();

        assert_eq!(board.now().as_deref(), Some("copied-value-2e9b"));
        let lines = traced(&ctx);
        let [line] = lines.as_slice() else {
            panic!("{lines:?}")
        };
        assert_eq!(line["event"], "secret-reveal");
        assert_eq!(line["vault"], "files");
        assert_eq!(line["key_names"], serde_json::json!(["DB_URL"]));
        assert_eq!(line["forced"], false);
        assert_eq!(line["to"], "clipboard");
        assert!(!line.to_string().contains("copied-value"), "{line}");
        let kept = format!("{:?}", pasting.lock().unwrap());
        assert!(!kept.contains("copied-value"), "{kept}");
    }

    #[test]
    fn a_copy_of_a_key_the_vault_does_not_hold_leaves_the_clipboard_alone() {
        let (_dir, ctx) = plane();
        let board = Pasteboard::holding("the operator's own");
        let pasting = board.pasting();

        assert!(copy(&ctx, "ops", "NEVER_ADDED", &pasting).is_err());

        assert_eq!(board.now().as_deref(), Some("the operator's own"));
        assert_eq!(clear_now(&pasting), Ok(false));
        assert_eq!(board.now().as_deref(), Some("the operator's own"));
        assert!(traced(&ctx).is_empty());
    }

    /// A plane whose keyring vault `ops` holds `A` and `B`, and a clipboard for it.
    fn copying() -> (
        tempfile::TempDir,
        Ctx,
        Pasteboard,
        std::sync::Arc<std::sync::Mutex<Pasting>>,
    ) {
        let (dir, ctx) = plane();
        add(&ctx, "ops", "A", &SecretValue::from("copied-value-a-51")).unwrap();
        add(&ctx, "ops", "B", &SecretValue::from("copied-value-b-73")).unwrap();
        let board = Pasteboard::default();
        let pasting = board.pasting();
        (dir, ctx, board, pasting)
    }

    /// Move the paused clock on by `secs`, and nothing more: the clock is only ever moved here.
    async fn after(secs: u64) {
        tokio::time::advance(std::time::Duration::from_secs(secs)).await;
    }

    /// Whether `clear` has finished by now, given real time for its blocking half. Busy, so the
    /// paused clock never jumps ahead on its own while this waits.
    async fn done(clear: &tokio::task::JoinHandle<()>) -> bool {
        for _ in 0..500 {
            if clear.is_finished() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            tokio::task::yield_now().await;
        }
        false
    }

    /// Copy `key` as the command does: the copy, then its clear, started — so its minute runs
    /// from now.
    async fn copy_as_the_window_asks(
        ctx: &Ctx,
        key: &str,
        pasting: &std::sync::Arc<std::sync::Mutex<Pasting>>,
    ) -> tokio::task::JoinHandle<()> {
        let n = copy(ctx, "ops", key, pasting).unwrap();
        let clear = tokio::spawn(clear_later(std::sync::Arc::clone(pasting), n));
        tokio::task::yield_now().await;
        clear
    }

    #[tokio::test(start_paused = true)]
    async fn the_clipboard_is_cleared_a_minute_after_a_copy() {
        let (_dir, ctx, board, pasting) = copying();

        let clear = copy_as_the_window_asks(&ctx, "A", &pasting).await;
        after(59).await;
        assert!(!done(&clear).await);
        assert_eq!(board.now().as_deref(), Some("copied-value-a-51"));
        after(1).await;

        assert!(done(&clear).await);
        assert_eq!(board.now(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn what_the_operator_copied_since_is_left_on_the_clipboard() {
        let (_dir, ctx, board, pasting) = copying();

        let clear = copy_as_the_window_asks(&ctx, "A", &pasting).await;
        after(10).await;
        board.put("something the operator copied since");
        after(50).await;

        assert!(done(&clear).await);
        assert_eq!(
            board.now().as_deref(),
            Some("something the operator copied since")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_later_copy_is_cleared_on_its_own_minute_and_not_the_earlier_ones() {
        let (_dir, ctx, board, pasting) = copying();

        let first = copy_as_the_window_asks(&ctx, "A", &pasting).await;
        after(30).await;
        let second = copy_as_the_window_asks(&ctx, "B", &pasting).await;
        after(30).await;
        assert!(done(&first).await);
        assert_eq!(board.now().as_deref(), Some("copied-value-b-73"));
        after(30).await;

        assert!(done(&second).await);
        assert_eq!(board.now(), None);
    }

    #[test]
    fn at_exit_the_clipboard_is_cleared_only_while_it_still_holds_what_charter_copied() {
        let (_dir, ctx, board, pasting) = copying();

        copy(&ctx, "ops", "A", &pasting).unwrap();
        assert_eq!(clear_now(&pasting), Ok(true));
        assert_eq!(board.now(), None);

        copy(&ctx, "ops", "A", &pasting).unwrap();
        board.put("something the operator copied since");
        assert_eq!(clear_now(&pasting), Ok(false));
        assert_eq!(
            board.now().as_deref(),
            Some("something the operator copied since")
        );

        // Nothing charter copied is waiting, so nothing is cleared, even the same text.
        board.put("copied-value-a-51");
        assert_eq!(clear_now(&pasting), Ok(false));
        assert_eq!(board.now().as_deref(), Some("copied-value-a-51"));
    }

    // --- a 1Password identity, moved into the keyring (#237) ---------------------------- //

    /// A fabricated service-account token.
    const TOKEN: &str = "ops_fixture-window-move-6c02da";

    /// A plane with a 1Password vault `team` read through `$OP_TEAM_TOKEN`, an `op` on `PATH`
    /// that lists one field, and a process environment of `carrying`.
    fn team(carrying: &[(&str, &str)]) -> (tempfile::TempDir, Ctx) {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let op = bin.join("op");
        std::fs::write(
            &op,
            "#!/bin/sh\n[ \"$1 $2\" = 'item get' ] && { printf '%s' '{\"fields\":[{\"label\":\"DEPLOY\",\"value\":\"x\"}]}'; exit 0; }\nexit 1\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&op, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:/usr/bin:/bin", bin.display());
        let mut vars = vec![("PATH", path.as_str())];
        vars.extend_from_slice(carrying);
        let ctx = Ctx::new(dir.path(), Env::of(&vars));
        let mut config = serde_json::Map::new();
        config.insert("op-vault".into(), serde_json::json!("Fixture"));
        config.insert(
            "env".into(),
            serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}),
        );
        registry::add_vault(&ctx, "team", "1password", config, None, false, false).unwrap();
        (dir, ctx)
    }

    fn identity_of(opened: &VaultContents) -> Vec<(&str, IdentityHeld)> {
        opened
            .identity
            .iter()
            .map(|i| (i.variable.as_str(), i.held))
            .collect()
    }

    #[test]
    fn a_vault_read_through_a_token_in_the_environment_says_so_and_names_the_variable() {
        let (_dir, ctx) = team(&[("OP_TEAM_TOKEN", TOKEN)]);

        let opened = open(&ctx, "team").unwrap();

        assert_eq!(
            identity_of(&opened),
            [("OP_TEAM_TOKEN", IdentityHeld::Environment)]
        );
        assert!(open(&ctx, "team").is_ok_and(|o| o.health.ok), "{opened:?}");
    }

    #[test]
    fn moving_the_token_answers_with_the_vault_reading_it_from_the_keyring() {
        let (dir, ctx) = team(&[("OP_TEAM_TOKEN", TOKEN)]);

        let moved = move_identity(&ctx, "team").unwrap();

        assert_eq!(
            identity_of(&moved),
            [("OP_TEAM_TOKEN", IdentityHeld::Keyring)]
        );
        // A chat's `charter`, which has no `$OP_TEAM_TOKEN`, now finds it.
        let path = format!("{}:/usr/bin:/bin", dir.path().join("bin").display());
        let bare = Ctx::new(dir.path(), Env::of(&[("PATH", path.as_str())]));
        let reopened = open(&bare, "team").unwrap();
        assert_eq!(
            identity_of(&reopened),
            [("OP_TEAM_TOKEN", IdentityHeld::Keyring)]
        );
        assert!(reopened.health.ok, "{reopened:?}");
    }

    #[test]
    fn a_pasted_token_is_put_in_the_keyring_and_never_seen_in_the_app_environment() {
        // #271 review U3. The token comes in through the command, not the app's environment, so
        // `identity_in_app_env` stays empty: no chat can read it from charter's process.
        let (dir, ctx) = team(&[]);

        let put = put_identity(&ctx, "team", &SecretValue::from("ops_fixture-pasted-3f")).unwrap();

        assert_eq!(
            identity_of(&put),
            [("OP_TEAM_TOKEN", IdentityHeld::Keyring)]
        );
        assert!(put.identity_in_app_env.is_empty(), "{put:?}");
        let path = format!("{}:/usr/bin:/bin", dir.path().join("bin").display());
        let bare = Ctx::new(dir.path(), Env::of(&[("PATH", path.as_str())]));
        assert_eq!(
            identity_of(&open(&bare, "team").unwrap()),
            [("OP_TEAM_TOKEN", IdentityHeld::Keyring)]
        );
    }

    #[test]
    fn a_move_from_the_app_environment_warns_that_the_export_is_still_there() {
        // #271 review U3. The move leaves the export in the app's process, so the answer names it
        // in `identity_in_app_env` for the tab to warn about.
        let (_dir, ctx) = team(&[("OP_TEAM_TOKEN", TOKEN)]);

        let moved = move_identity(&ctx, "team").unwrap();

        assert_eq!(moved.identity_in_app_env, ["OP_TEAM_TOKEN"]);
    }

    #[test]
    fn a_vault_whose_token_is_nowhere_says_so_and_a_move_is_refused() {
        let (_dir, ctx) = team(&[]);

        // Its keys are `op`'s to list, and `op` is not run under an identity nobody declared.
        let opened = open(&ctx, "team").unwrap_err();
        assert!(
            opened.contains("$OP_TEAM_TOKEN, which is unset"),
            "{opened}"
        );
        let err = move_identity(&ctx, "team").unwrap_err();
        assert!(err.contains("$OP_TEAM_TOKEN"), "{err}");
    }

    #[test]
    fn a_vault_with_no_identity_lists_none_and_has_nothing_to_move() {
        let (_dir, ctx) = plane();
        assert!(open(&ctx, "ops").unwrap().identity.is_empty());
        assert!(move_identity(&ctx, "ops").is_err());
    }

    #[test]
    fn no_answer_or_refusal_of_a_move_carries_the_token() {
        let (dir, ctx) = team(&[("OP_TEAM_TOKEN", TOKEN)]);
        let mut answers = vec![wire(&open(&ctx, "team")), wire(&list(&ctx))];
        answers.push(wire(&move_identity(&ctx, "team")));
        answers.push(wire(&move_identity(&ctx, "nope")));
        answers.push(wire(&open(&ctx, "team")));
        answers.push(wire(&list(&ctx)));
        // A keyring that cannot be written: the refusal comes after the token was read.
        std::fs::write(dir.path().join(".charter/keyring-stub.json"), "not json").unwrap();
        answers.push(wire(&move_identity(&ctx, "team")));
        answers.push(wire(&open(&ctx, "team")));
        answers.extend(traced(&ctx).iter().map(ToString::to_string));

        // The move happened, so the absence below is not an absence of a token to leak.
        assert!(answers[2].contains("\"keyring\""), "{}", answers[2]);
        for answer in &answers {
            assert!(!answer.contains(TOKEN), "{answer}");
        }
    }

    #[test]
    fn a_value_from_the_window_arrives_as_a_string_and_never_prints_in_debug() {
        let value: SecretValue = serde_json::from_str("\"debug-fixture-61c2\"").unwrap();
        assert_eq!(format!("{value:?}"), "SecretValue(***)");
    }

    #[test]
    fn a_new_vault_is_a_keyring_vault_unless_another_provider_is_asked_for() {
        let (_dir, ctx) = plane();

        let made = create(&ctx, "fresh", None, None).unwrap();
        let file = create(&ctx, "papers", Some("plain-file"), None).unwrap();

        assert_eq!(
            (made.name.as_str(), made.provider.as_str(), made.count),
            ("fresh", "keyring", 0)
        );
        assert_eq!(file.provider, "plain-file");
        let names: Vec<String> = list(&ctx).unwrap().into_iter().map(|v| v.name).collect();
        assert_eq!(names, ["files", "fresh", "ops", "papers"]);
    }

    #[test]
    fn a_vault_name_charter_would_not_accept_or_one_already_registered_is_refused() {
        let (_dir, ctx) = plane();

        let bad = create(&ctx, "../escape", None, None).unwrap_err();
        let taken = create(&ctx, "ops", Some("plain-file"), None).unwrap_err();
        let unknown = create(&ctx, "odd", Some("carrier-pigeon"), None).unwrap_err();

        assert!(bad.contains("not a vault name"), "{bad}");
        assert!(taken.contains("already registered"), "{taken}");
        assert!(unknown.contains("unknown provider"), "{unknown}");
        assert_eq!(open(&ctx, "ops").unwrap().provider, "keyring");
    }

    #[test]
    fn a_1password_vault_needs_the_1password_vault_it_keeps_its_items_in() {
        let (_dir, ctx) = plane();
        let err = create(&ctx, "team", Some("1password"), None).unwrap_err();
        assert!(err.contains("1Password vault"), "{err}");
        assert!(open(&ctx, "team").is_err());
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

    #[test]
    fn deleting_a_keyring_vault_destroys_its_secrets_and_answers_with_their_names_only() {
        let (dir, ctx) = plane();
        add(&ctx, "ops", "B", &SecretValue::from("removed-value-b-7f2")).unwrap();
        add(&ctx, "ops", "A", &SecretValue::from("removed-value-a-3e1")).unwrap();

        let gone = remove(&ctx, "ops").unwrap();

        assert_eq!(
            gone,
            VaultRemoved {
                name: "ops".into(),
                provider: "keyring".into(),
                destroyed: vec!["A".into(), "B".into()],
            }
        );
        let answer = wire(&Ok::<_, String>(gone));
        assert!(!answer.contains("removed-value"), "{answer}");
        let stub = std::fs::read_to_string(dir.path().join(".charter/keyring-stub.json"))
            .unwrap_or_default();
        assert!(
            !stub.contains("removed-value"),
            "the keyring still holds a value"
        );
        assert_eq!(
            list(&ctx)
                .unwrap()
                .iter()
                .map(|v| v.name.as_str())
                .collect::<Vec<_>>(),
            ["files"]
        );
    }

    #[test]
    fn deleting_a_vault_the_plane_does_not_register_is_refused() {
        let (_dir, ctx) = plane();
        let err = remove(&ctx, "nope").unwrap_err();
        assert!(err.contains("nope"), "{err}");
        assert_eq!(list(&ctx).unwrap().len(), 2);
    }
}
