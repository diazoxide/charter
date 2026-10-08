//! The plane's vaults, as the window reaches them.

use purlis_core::secrets::cmd::{self, Io, Say};
use purlis_core::secrets::identity::{self, Held};
use purlis_core::secrets::keyring;
use purlis_core::secrets::onepassword;
use purlis_core::secrets::registry::{self, Vault};
use purlis_core::secrets::setup;
use purlis_core::secrets::vaultcmd;
use purlis_core::secrets::{Ctx, Env, VaultError, identity_missing};

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

/// Runs a brokered `secret exec` for one of `held`'s chats (#1407): the chat's persona and
/// folder are this app's record of the chat whose token the line carried, never the line's, and
/// the sandbox the command runs under is the one the chat's own was compiled to as it started.
/// A chat this app does not have open is refused.
///
/// A vault the chat's persona is not tagged for is refused by the core, and noted here first
/// for the Notice on the chat's tab (#1430, `crate::vaultroute`): from the same record of the
/// chat, never from the line.
pub(crate) fn run_brokered(
    held: &crate::planes::Held,
    plane: &PlaneId,
    refused: &(dyn Fn(crate::vaultroute::VaultRefused) + Send + Sync),
    ask: purlis_core::secrets::brokered::Ask,
    reader: Box<dyn std::io::BufRead + Send>,
    writer: Box<dyn std::io::Write + Send>,
) {
    use purlis_core::secrets::brokered;
    let Some(open) = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == ask.chat)
    else {
        tracing::warn!(
            "purlis: chat {} asked for `secret exec` and is not one this app has open",
            ask.chat
        );
        return brokered::refused(
            writer,
            format!("chat {} is not one this app has open", ask.chat),
        );
    };
    let asker = brokered::Asker {
        root: held.root().to_path_buf(),
        env: Env::from_process(),
        chat: ask.chat,
        persona: open.persona,
        folder: open.cwd,
        // Recorded as the chat started, so the run is held to what the chat itself is held to
        // and a policy changed since does not change it.
        confines: held.chats().confines_of(ask.chat),
    };
    crate::vaultroute::refused(
        plane,
        held.vault_refusals(),
        &asker,
        &ask.secret_exec.vault,
        refused,
    );
    brokered::serve(&asker, ask.secret_exec, reader, writer);
}

/// Answers `purlis vault list` for chat `chat` of `held` (#1430): every vault the project
/// registers, with its tag and whether the persona this app started the chat as may use it.
/// Read from the registry alone: no provider is asked, and no value or key name is answered.
pub(crate) fn list_for_chat(
    held: &crate::planes::Held,
    chat: u32,
) -> purlis_core::hookwire::Answer {
    use purlis_core::hookwire::Answer;
    let Some(open) = held
        .chats()
        .open_now()
        .into_iter()
        .find(|open| open.session == chat)
    else {
        return Answer::No {
            why: format!("chat {chat} is not one this app has open"),
        };
    };
    let ctx = Ctx::new(held.root(), Env::from_process());
    match purlis_core::secrets::brokered::listed(&ctx, open.persona.as_deref()) {
        Ok(vaults) => Answer::Vaults {
            persona: open.persona,
            vaults,
            allow_locked: purlis_core::sandbox::policy::Locks::of(held.root())
                .forbids_vault_grants(),
        },
        Err(why) => Answer::No { why },
    }
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
    /// The token is declared as kept in the keyring and read through no variable (#1527):
    /// `variable` is then the core's word for that, and no variable's name to show.
    pub kept: bool,
}

/// What kept a vault's contents from being read, as far as purlis can tell: what the tab says
/// about the token depends on it, since most failures are not the token's (#1526).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum UnreadFor {
    /// The identity variable is found nowhere: there is no token to read with.
    NoToken,
    /// The provider's program is missing, or the one pinned with the token is gone or changed.
    /// Storing the token again pins the program found now.
    Program,
    /// No network, a rate limit, a program that did not finish: try again. Nothing says the
    /// token is wrong.
    TryAgain,
    /// The provider refused the sign-in: the token is the likely cause.
    SignIn,
    /// Anything else. The token may or may not be the cause.
    Other,
}

/// Why a vault read through an identity variable could not be read: the core's own sentence,
/// which holds names and never a value, and what kind of failure it is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct Unread {
    pub why: String,
    pub kind: UnreadFor,
}

/// One vault, opened.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct VaultContents {
    pub name: String,
    pub provider: String,
    pub count: u32,
    pub health: VaultHealth,
    pub secrets: Vec<VaultSecret>,
    /// Why the vault's contents could not be read, for a vault read through an identity
    /// variable; `None` for a vault that was read. `secrets` is then empty and says nothing of
    /// what the vault holds. Answered rather than refused, because `identity` below is what the
    /// tab draws the way out from: the box that stores the token (#1526).
    pub refused: Option<Unread>,
    /// The OTHER vaults of the project read through one of this vault's identity variables
    /// whose token is nowhere, by name: what the tab points at, each a link to that vault's own
    /// tab, where its token is put in. A pointer and never a write (#1526, D-1526-7).
    pub identity_unset_elsewhere: Vec<String>,
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
            kept: identity::kept(&b.source),
            variable: b.source,
            held: match b.held {
                Held::Keyring => IdentityHeld::Keyring,
                Held::Environment => IdentityHeld::Environment,
                Held::Unset => IdentityHeld::Unset,
            },
        })
        .collect()
}

/// The vault's secrets, by name, as its table lists them.
fn secrets_of(ctx: &Ctx, v: &Vault) -> Result<Vec<VaultSecret>, VaultError> {
    if v.provider == "keyring" {
        return Ok(keyring::listed(ctx, v)?
            .into_iter()
            .map(|l| VaultSecret {
                key: l.key,
                size: Some(l.size).filter(|s| !s.is_empty()),
                updated: Some(l.updated).filter(|s| !s.is_empty()),
            })
            .collect());
    }
    Ok(cmd::keys(ctx, v)?
        .into_iter()
        .map(|key| VaultSecret {
            key,
            size: None,
            updated: None,
        })
        .collect())
}

/// What the tab of a vault whose identity variable is found nowhere says: the first line of the
/// core's refusal, which names the vault and the variable, and then the way out the tab itself
/// draws under it. The core's own second line is written for a terminal, and sends the reader to
/// this tab.
fn unset_in_the_tab(refusal: &VaultError) -> String {
    format!(
        "{} Paste the token into the box below: it goes straight into the Keychain.",
        refusal.message.lines().next().unwrap_or_default()
    )
}

/// Whether a program `v`'s provider runs cannot be run here, asked before the provider is: the
/// one pinned with a keyring-held token is gone or changed, or the lookup finds none. The
/// refusal the read itself gives then says which, and where it looked.
fn program_cannot_run(ctx: &Ctx, v: &Vault) -> bool {
    purlis_core::secrets::program::needed_by(ctx, v)
        .into_iter()
        .any(|program| {
            let pinned = match program {
                "op" => identity::pinned_op(ctx, v),
                _ => Ok(None),
            };
            match pinned {
                Err(_) => true,
                Ok(Some(_)) => false,
                Ok(None) => ctx.program(program).is_err(),
            }
        })
}

/// What kind of failure the core's sentence `why` is, for a read that ran the provider. Matched
/// on the core's own fixed sentences (`secrets::onepassword`'s diagnoses and `op_run`), never on
/// what a provider printed, which the core withholds. A sentence this does not know is
/// [`UnreadFor::Other`], which blames nothing.
fn kind_of(why: &str) -> UnreadFor {
    const TRY_AGAIN: [&str; 3] = [
        "rate-limited this client",
        "could not reach 1Password",
        "did not finish and was stopped",
    ];
    const SIGN_IN: [&str; 2] = ["refused this as unauthorised", "has no account configured"];
    if TRY_AGAIN.iter().any(|said| why.contains(said)) {
        UnreadFor::TryAgain
    } else if SIGN_IN.iter().any(|said| why.contains(said)) {
        UnreadFor::SignIn
    } else {
        UnreadFor::Other
    }
}

/// The vault's secrets, or why they could not be read. The provider is run at most once.
fn read(ctx: &Ctx, v: &Vault) -> Result<Vec<VaultSecret>, Unread> {
    // Asked before the provider is: a vault whose token is nowhere runs no program at all.
    if let Some(unset) = identity_missing(ctx, v) {
        return Err(Unread {
            why: unset_in_the_tab(&unset),
            kind: UnreadFor::NoToken,
        });
    }
    let program = program_cannot_run(ctx, v);
    secrets_of(ctx, v).map_err(|e| Unread {
        kind: if program {
            UnreadFor::Program
        } else {
            kind_of(&e.message)
        },
        why: e.message,
    })
}

/// One vault's secrets, by name.
///
/// **A vault read through an identity variable is answered even when its contents cannot be
/// read** (#1526): with [`VaultContents::refused`] saying why, no secrets, and the identity it
/// declares. Reading such a vault takes its token, so a refusal alone would hide the one thing
/// the tab needs to offer the way out. A vault that declares no identity is refused as before.
///
/// **An unread vault's health line is its refusal**, not a second run of the provider to hear
/// the same thing again.
pub(crate) fn open(ctx: &Ctx, vault: &str) -> Result<VaultContents, String> {
    let v = cmd::provider(ctx, vault).map_err(message_of)?;
    let identity = identity_of(ctx, &v);
    let (secrets, refused) = match read(ctx, &v) {
        Ok(secrets) => (secrets, None),
        // A 1Password vault with no token of its own too (#1527): its tab is where how it
        // signs in is changed, which a refusal alone would hide.
        Err(unread) if !identity.is_empty() || v.provider == "1password" => {
            (Vec::new(), Some(unread))
        }
        Err(unread) => return Err(unread.why),
    };
    let health = match &refused {
        Some(unread) => VaultHealth {
            ok: false,
            detail: unread.why.lines().next().unwrap_or_default().to_owned(),
        },
        None => health(ctx, &v),
    };
    Ok(VaultContents {
        count: counted(secrets.len()),
        health,
        refused,
        identity,
        identity_unset_elsewhere: identity::unset_alike(ctx, &v),
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
             secret everywhere purlis looks."
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
    /// Put ordinary text on it, which clipboard histories keep: a path a file row copied
    /// (FM-10), never a secret.
    fn set_text(&mut self, text: &str) -> Result<(), String>;
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

/// Put ordinary text on the clipboard, through the one handle a vault's Copy uses too (FM-10).
/// A clear still waiting for a vault's copy leaves it alone: that clear empties the clipboard
/// only while it still holds what the vault put there.
pub(crate) fn put_text(pasting: &std::sync::Mutex<Pasting>, text: &str) -> Result<(), String> {
    locked(pasting).board.set_text(text)
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

// ---------------------------------------------------------------------------------------
// The guided set-up of a 1Password vault's sign-in (#1527, `purlis_core::secrets::setup`).

/// How long a token given to a set-up is held for it before it must be given again.
const A_SETUP_IS_HELD_FOR: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Where a set-up is held for: the window that began it, by its label, and the project.
/// Only that window may use it or let go of it, and it is dropped when that window goes or
/// reloads ([`Setups::forget`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct At {
    window: String,
    plane: PlaneId,
}

impl At {
    pub(crate) fn new(window: &str, plane: PlaneId) -> Self {
        Self {
            window: window.to_owned(),
            plane,
        }
    }
}

/// One set-up in progress: what the person gave at its first step, held here so that the page
/// hands a token over **once** and never holds it between the test and the create.
struct Pending {
    number: u32,
    at: At,
    sign_in: setup::SignIn,
    account: Option<String>,
    since: std::time::Instant,
}

/// The set-up a window has open, if one has. One at a time: a new one replaces the last,
/// whose token is dropped with it.
///
/// **The token is held for [`A_SETUP_IS_HELD_FOR`] at most**, from the moment it was given:
/// a timer started then lets go of it ([`Setups::expire_after`]), whatever the window does.
/// Before that it goes with a finished create or change, a cancel, a new set-up, and the window
/// that began it closing or reloading ([`Setups::forget`]): a page that is gone sends no
/// cancel, so the app does not wait for one.
#[derive(Default)]
pub(crate) struct Setups {
    held: std::sync::Mutex<Option<Pending>>,
    made: std::sync::atomic::AtomicU32,
}

impl std::fmt::Debug for Setups {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Setups(***)")
    }
}

/// What a set-up that is no longer held is told.
const SETUP_GONE: &str = "This set-up is no longer held: it was left for a while, or another \
                          was started. Give the sign-in again.";

impl Setups {
    fn held(&self) -> std::sync::MutexGuard<'_, Option<Pending>> {
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Hold a new set-up for `at` in place of whatever was held, and answer its number.
    fn begin(&self, at: &At, sign_in: setup::SignIn, account: Option<String>) -> u32 {
        let number = self.made.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        *self.held() = Some(Pending {
            number,
            at: at.clone(),
            sign_in,
            account,
            since: std::time::Instant::now(),
        });
        number
    }

    /// What set-up `number` was given, asked by `at`, while it is still held for it.
    fn given(&self, at: &At, number: u32) -> Result<(setup::SignIn, Option<String>), String> {
        let mut held = self.held();
        if held
            .as_ref()
            .is_some_and(|p| p.since.elapsed() > A_SETUP_IS_HELD_FOR)
        {
            *held = None;
        }
        held.as_ref()
            .filter(|p| p.number == number && p.at == *at)
            .map(|p| (p.sign_in.clone(), p.account.clone()))
            .ok_or_else(|| SETUP_GONE.to_owned())
    }

    /// Let go of set-up `number`, and of its token with it, when `window` is the one it is
    /// held for.
    fn end(&self, window: &str, number: u32) {
        let mut held = self.held();
        if held
            .as_ref()
            .is_some_and(|p| p.number == number && p.at.window == window)
        {
            *held = None;
        }
    }

    /// Let go of whatever is held for `window`: it closed, or its page reloaded.
    pub(crate) fn forget(&self, window: &str) {
        let mut held = self.held();
        if held.as_ref().is_some_and(|p| p.at.window == window) {
            *held = None;
        }
    }

    /// Let go of set-up `number` once `after` has passed, if it is still the one held.
    fn expire_after(self: &std::sync::Arc<Self>, number: u32, after: std::time::Duration) {
        let setups = std::sync::Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(after).await;
            let mut held = setups.held();
            if held.as_ref().is_some_and(|p| p.number == number) {
                *held = None;
            }
        });
    }
}

/// The window `label` is gone or its page reloaded: the set-up held for it goes too.
pub(crate) fn window_gone<R: tauri::Runtime>(app: &impl tauri::Manager<R>, label: &str) {
    if let Some(setups) = app.try_state::<std::sync::Arc<Setups>>() {
        setups.forget(label);
    }
}

/// A test or a listing that did not pass: its kind, as the vault's tab knows kinds, and the
/// core's own sentence. Never what the provider's program printed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SetupFailed {
    pub kind: UnreadFor,
    pub why: String,
}

impl From<setup::Failed> for SetupFailed {
    fn from(failed: setup::Failed) -> Self {
        Self {
            kind: match failed.kind {
                setup::Kind::Program => UnreadFor::Program,
                setup::Kind::TryAgain => UnreadFor::TryAgain,
                setup::Kind::SignIn => UnreadFor::SignIn,
                setup::Kind::Other => UnreadFor::Other,
            },
            why: failed.why,
        }
    }
}

/// One account the 1Password app on this machine is signed in to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SetupAccount {
    pub address: String,
    pub email: String,
    /// What is pinned when it is chosen.
    pub pin: String,
}

/// The accounts the 1Password app lists, or why they could not be listed (the address is then
/// typed).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SetupAccounts {
    pub accounts: Vec<SetupAccount>,
    pub failed: Option<SetupFailed>,
}

/// One other vault bound to the same identity, as the person is shown it before they tick it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SetupAlike {
    pub name: String,
    pub op_vault: String,
    pub op_item: String,
    pub account: Option<String>,
    /// The persona it is tagged for.
    pub persona: Option<String>,
    /// Which half of the registry names it: `local`, `shared` or `both`.
    pub half: String,
    pub held: IdentityHeld,
    /// Whether its box starts ticked. Never for a vault the committed half names.
    pub ticked: bool,
    /// What the store is handed back with the name.
    pub digest: String,
}

/// One vault the person ticked, with the digest they were shown for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
pub(crate) struct SetupTick {
    pub name: String,
    pub digest: String,
}

/// The vault a set-up makes: its name, the 1Password vault its items live in, and the item
/// (purlis's default for the vault when null).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
pub(crate) struct SetupNew {
    pub vault: String,
    pub op_vault: String,
    pub op_item: Option<String>,
}

/// A set-up begun: its number, the 1Password vaults the sign-in can see (or why they could not
/// be listed, and the name is typed), and the other vaults the token may be used for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SetupBegun {
    pub setup: u32,
    pub op_vaults: Vec<String>,
    pub listing: Option<SetupFailed>,
    pub alike: Vec<SetupAlike>,
}

/// What a test saw: names and counts, never a value. Or why it did not pass.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SetupTested {
    pub items: u32,
    pub item: String,
    pub item_there: bool,
    pub failed: Option<SetupFailed>,
}

/// A ticked vault that was not given the token.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SetupSkipped {
    pub name: String,
    /// `changed`: its settings are not the ones shown. `gone`: it is not registered, or not
    /// bound to this identity, any more. `failed`: the keyring or the registry refused.
    pub why: String,
    /// The core's sentence, for `failed`.
    pub said: Option<String>,
}

/// A vault made, or its sign-in changed: the vault as it now is, and what became of the ticks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct SetupDone {
    pub contents: VaultContents,
    pub marked: Vec<String>,
    pub skipped: Vec<SetupSkipped>,
}

fn alike_shown(all: Vec<setup::Alike>) -> Vec<SetupAlike> {
    all.into_iter()
        .map(|a| SetupAlike {
            name: a.name,
            op_vault: a.op_vault,
            op_item: a.op_item,
            account: a.account,
            persona: a.persona,
            half: a.half,
            held: match a.held {
                Held::Keyring => IdentityHeld::Keyring,
                Held::Environment => IdentityHeld::Environment,
                Held::Unset => IdentityHeld::Unset,
            },
            ticked: a.ticked,
            digest: a.digest,
        })
        .collect()
}

fn ticks_of(also: Vec<SetupTick>) -> Vec<setup::Tick> {
    also.into_iter()
        .map(|t| setup::Tick {
            name: t.name,
            digest: t.digest,
        })
        .collect()
}

fn done(ctx: &Ctx, vault: &str, marked: setup::Marked) -> Result<SetupDone, String> {
    Ok(SetupDone {
        contents: open(ctx, vault)?,
        marked: marked.marked,
        skipped: marked
            .skipped
            .into_iter()
            .map(|(name, why)| {
                let (why, said) = match why {
                    setup::NotMarked::Changed => ("changed", None),
                    setup::NotMarked::Gone => ("gone", None),
                    setup::NotMarked::Failed(said) => ("failed", Some(said)),
                };
                SetupSkipped {
                    name,
                    why: why.to_owned(),
                    said,
                }
            })
            .collect(),
    })
}

/// The accounts the 1Password app on this machine lists ([`setup::accounts`]).
pub(crate) fn setup_accounts(ctx: &Ctx) -> SetupAccounts {
    match setup::accounts(ctx) {
        Ok(accounts) => SetupAccounts {
            accounts: accounts
                .into_iter()
                .map(|a| SetupAccount {
                    address: a.address,
                    email: a.email,
                    pin: a.pin,
                })
                .collect(),
            failed: None,
        },
        Err(failed) => SetupAccounts {
            accounts: Vec::new(),
            failed: Some(failed.into()),
        },
    }
}

/// Begin a set-up with what the person gave: a token (`Some`), or the 1Password app with the
/// account they chose. The token is cleaned and held by `setups`; the answer lists the
/// 1Password vaults that sign-in can see and, for a token, the other vaults it may be used
/// for: those bound as `vault` is when the set-up is for a vault that exists, those that keep a
/// token when it is for a new one. No value is answered, and nothing is written.
pub(crate) fn setup_begin(
    ctx: &Ctx,
    setups: &Setups,
    at: &At,
    token: Option<&SecretValue>,
    account: Option<&str>,
    vault: Option<&str>,
) -> Result<SetupBegun, String> {
    let account = setup::clean_account(account).map_err(message_of)?;
    let sign_in = match token {
        Some(token) => setup::SignIn::Token(setup::clean_token(&token.0).map_err(message_of)?),
        None => setup::SignIn::App,
    };
    let (op_vaults, listing) = match setup::op_vaults(ctx, &sign_in, account.as_deref()) {
        Ok(names) => (names, None),
        Err(failed) => (Vec::new(), Some(failed.into())),
    };
    let alike = match (&sign_in, vault) {
        (setup::SignIn::App, _) => Vec::new(),
        (_, Some(vault)) => setup::alike_of(ctx, vault),
        (_, None) => setup::alike(ctx, &setup::kept_sources(), ""),
    };
    Ok(SetupBegun {
        setup: setups.begin(at, sign_in, account),
        op_vaults,
        listing,
        alike: alike_shown(alike),
    })
}

/// Test set-up `number` against the 1Password vault chosen, or against the one the registered
/// vault `name` keeps its items in when none is named ([`setup::test`]): item names only,
/// nothing registered, nothing stored.
pub(crate) fn setup_test(
    ctx: &Ctx,
    setups: &Setups,
    at: &At,
    number: u32,
    name: &str,
    op_vault: Option<&str>,
    op_item: Option<&str>,
) -> Result<SetupTested, String> {
    let (sign_in, account) = setups.given(at, number)?;
    let place = match op_vault {
        Some(op_vault) => setup::Place {
            op_vault: op_vault.to_owned(),
            op_item: op_item.map(str::to_owned),
            account,
        },
        // A vault that exists is tested where its items already live.
        None => {
            let v = cmd::provider(ctx, name).map_err(message_of)?;
            setup::Place {
                op_vault: onepassword::op_vault(&v).map_err(message_of)?,
                op_item: Some(onepassword::op_item(&v).map_err(message_of)?),
                account,
            }
        }
    };
    Ok(match setup::test(ctx, name, &sign_in, &place) {
        Ok(tested) => SetupTested {
            items: counted(tested.items),
            item: tested.item,
            item_there: tested.item_there,
            failed: None,
        },
        Err(failed) => SetupTested {
            items: 0,
            item: String::new(),
            item_there: false,
            failed: Some(failed.into()),
        },
    })
}

/// Make the vault set-up `number` is for and write its keyring record, in one step
/// ([`setup::create`]). The set-up is let go of once the vault is made, and kept when it was
/// refused, so a name that was taken can be changed without the token being given again.
pub(crate) fn setup_create(
    ctx: &Ctx,
    setups: &Setups,
    at: &At,
    number: u32,
    new: &SetupNew,
    also: Vec<SetupTick>,
) -> Result<SetupDone, String> {
    let (sign_in, account) = setups.given(at, number)?;
    let marked = setup::create(
        ctx,
        &setup::Request {
            name: new.vault.clone(),
            place: setup::Place {
                op_vault: new.op_vault.clone(),
                op_item: new.op_item.clone(),
                account,
            },
            persona: None,
            sign_in,
            share: false,
            force: false,
            also: ticks_of(also),
        },
    )
    .map_err(message_of)?;
    setups.end(&at.window, number);
    done(ctx, &new.vault, marked)
}

/// Change how the vault `vault` signs in to what set-up `number` was given ([`setup::change`]).
pub(crate) fn setup_change(
    ctx: &Ctx,
    setups: &Setups,
    at: &At,
    number: u32,
    vault: &str,
    also: Vec<SetupTick>,
) -> Result<SetupDone, String> {
    let (sign_in, account) = setups.given(at, number)?;
    let marked = setup::change(ctx, vault, &sign_in, account.as_deref(), &ticks_of(also))
        .map_err(message_of)?;
    setups.end(&at.window, number);
    done(ctx, vault, marked)
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

/// The accounts the 1Password app on this machine lists, for a vault that signs in through it
/// ([`setup_accounts`]). No credential is involved and no value crosses.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_setup_accounts(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<SetupAccounts, String> {
    blocking(ctx_of(&planes, &plane)?, move |ctx| Ok(setup_accounts(ctx))).await
}

/// Begin setting up how a 1Password vault signs in ([`setup_begin`]): with a service-account
/// token, which comes in here once and is held by the app until the set-up ends, or through
/// the 1Password app (`token` null) with the account chosen. `vault` names the vault whose
/// sign-in is being changed, and is null for a new one. Nothing is written, and the answer
/// holds names only.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_setup_begin(
    planes: tauri::State<'_, Planes>,
    setups: tauri::State<'_, std::sync::Arc<Setups>>,
    window: tauri::Window,
    plane: PlaneId,
    token: Option<SecretValue>,
    account: Option<String>,
    vault: Option<String>,
) -> Result<SetupBegun, String> {
    let setups = std::sync::Arc::clone(&setups);
    let of = At::new(window.label(), plane.clone());
    let timed = std::sync::Arc::clone(&setups);
    let begun = blocking(ctx_of(&planes, &plane)?, move |ctx| {
        setup_begin(
            ctx,
            &setups,
            &of,
            token.as_ref(),
            account.as_deref(),
            vault.as_deref(),
        )
    })
    .await?;
    // The token is let go of when the limit is reached, whatever the window does.
    timed.expire_after(begun.setup, A_SETUP_IS_HELD_FOR);
    Ok(begun)
}

/// Test a set-up before anything is registered ([`setup_test`]): purlis signs in with what was
/// given and reads the chosen 1Password vault's item names. `op_vault` is null for a vault that
/// exists, which is tested where its items already live. Never a value.
#[tauri::command]
#[specta::specta]
// The Tauri state, the asking window and the project come with every command; what is tested
// is the rest.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn vault_setup_test(
    planes: tauri::State<'_, Planes>,
    setups: tauri::State<'_, std::sync::Arc<Setups>>,
    window: tauri::Window,
    plane: PlaneId,
    setup: u32,
    vault: String,
    op_vault: Option<String>,
    op_item: Option<String>,
) -> Result<SetupTested, String> {
    let setups = std::sync::Arc::clone(&setups);
    let of = At::new(window.label(), plane.clone());
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        setup_test(
            ctx,
            &setups,
            &of,
            setup,
            &vault,
            op_vault.as_deref(),
            op_item.as_deref(),
        )
    })
    .await
}

/// Make the vault `place` names, with its token in the keyring and its record, in one step
/// ([`setup_create`]). `also` is the other vaults the person ticked, each with the digest they
/// were shown; only those still matching are given the token. No value crosses.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_setup_create(
    planes: tauri::State<'_, Planes>,
    setups: tauri::State<'_, std::sync::Arc<Setups>>,
    window: tauri::Window,
    plane: PlaneId,
    setup: u32,
    place: SetupNew,
    also: Vec<SetupTick>,
) -> Result<SetupDone, String> {
    let setups = std::sync::Arc::clone(&setups);
    let of = At::new(window.label(), plane.clone());
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        setup_create(ctx, &setups, &of, setup, &place, also)
    })
    .await
}

/// Change how a registered vault signs in, to what a set-up was given ([`setup_change`]): a
/// vault bound to an environment variable comes to keep its token in the keyring, with no
/// restart. No value crosses.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_setup_change(
    planes: tauri::State<'_, Planes>,
    setups: tauri::State<'_, std::sync::Arc<Setups>>,
    window: tauri::Window,
    plane: PlaneId,
    setup: u32,
    vault: String,
    also: Vec<SetupTick>,
) -> Result<SetupDone, String> {
    let setups = std::sync::Arc::clone(&setups);
    let of = At::new(window.label(), plane.clone());
    blocking(ctx_of(&planes, &plane)?, move |ctx| {
        setup_change(ctx, &setups, &of, setup, &vault, also)
    })
    .await
}

/// Let go of a set-up this window began, and of the token it was given: the dialog was
/// cancelled or closed.
#[tauri::command]
#[specta::specta]
pub(crate) async fn vault_setup_cancel(
    setups: tauri::State<'_, std::sync::Arc<Setups>>,
    window: tauri::Window,
    setup: u32,
) -> Result<(), String> {
    setups.end(window.label(), setup);
    Ok(())
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
    fn set_text(&mut self, text: &str) -> Result<(), String> {
        self.open()?
            .set_text(text)
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

    /// [`put_text`], on the system clipboard.
    pub(crate) fn put_text(&self, text: &str) -> Result<(), String> {
        put_text(&self.0, text)
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
    use purlis_core::secrets::registry;

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
        let file = purlis_core::trace::file(&ctx.root, purlis_core::trace::NO_SESSION);
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
        fn set_text(&mut self, text: &str) -> Result<(), String> {
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
    async fn a_path_copied_after_a_secret_is_left_on_the_clipboard_by_the_secrets_clear() {
        let (_dir, ctx, board, pasting) = copying();

        let clear = copy_as_the_window_asks(&ctx, "A", &pasting).await;
        after(10).await;
        put_text(&pasting, "/work/svc/src/lib.rs").unwrap();
        after(50).await;

        assert!(done(&clear).await);
        assert_eq!(board.now().as_deref(), Some("/work/svc/src/lib.rs"));
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
        // The project is a folder of its own beside it: no provider's program is run from
        // inside a project (#1516).
        std::fs::create_dir_all(dir.path().join("plane")).unwrap();
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
        let ctx = Ctx::new(&dir.path().join("plane"), Env::of(&vars));
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
        let bare = Ctx::new(
            &dir.path().join("plane"),
            Env::of(&[("PATH", path.as_str())]),
        );
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
        let bare = Ctx::new(
            &dir.path().join("plane"),
            Env::of(&[("PATH", path.as_str())]),
        );
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
        let opened = open(&ctx, "team").unwrap();
        let refused = opened.refused.as_ref().map_or("", |r| r.why.as_str());
        assert!(
            refused.contains("$OP_TEAM_TOKEN, which is unset"),
            "{opened:?}"
        );
        let err = move_identity(&ctx, "team").unwrap_err();
        assert!(err.contains("$OP_TEAM_TOKEN"), "{err}");
    }

    // --- #1526: the identity is answered when the contents cannot be read ------------------ //

    /// Replace the project's stand-in `op` with one that runs `script`, and counts its runs in
    /// the file answered.
    fn op_now(dir: &tempfile::TempDir, script: &str) -> std::path::PathBuf {
        let ran = dir.path().join("op-ran");
        std::fs::write(
            dir.path().join("bin/op"),
            format!("#!/bin/sh\necho ran >> '{}'\n{script}\n", ran.display()),
        )
        .unwrap();
        ran
    }

    /// A second 1Password vault `name`, read through `$OP_TEAM_TOKEN` as `team` is.
    fn alike(ctx: &Ctx, name: &str) {
        let mut config = serde_json::Map::new();
        config.insert("op-vault".into(), serde_json::json!("Edge"));
        config.insert(
            "env".into(),
            serde_json::json!({"OP_SERVICE_ACCOUNT_TOKEN": "OP_TEAM_TOKEN"}),
        );
        registry::add_vault(ctx, name, "1password", config, None, false, false).unwrap();
    }

    #[test]
    fn a_vault_whose_token_is_nowhere_still_answers_the_identity_it_declares() {
        let (dir, ctx) = team(&[]);
        // An `op` that would betray being run at all.
        let ran = op_now(&dir, "exit 1");

        let opened = open(&ctx, "team").unwrap();

        assert_eq!(
            identity_of(&opened),
            [("OP_TEAM_TOKEN", IdentityHeld::Unset)]
        );
        assert_eq!(
            opened.refused,
            Some(Unread {
                why: "vault 'team' is read through $OP_TEAM_TOKEN, which is unset. purlis will \
                      not fall back to an ambient $OP_SERVICE_ACCOUNT_TOKEN: that would read \
                      this vault under an identity it does not declare, and the failure would \
                      look like a missing secret rather than a wrong credential. Paste the \
                      token into the box below: it goes straight into the Keychain."
                    .into(),
                kind: UnreadFor::NoToken,
            })
        );
        assert!(opened.secrets.is_empty() && opened.count == 0, "{opened:?}");
        assert!(!opened.health.ok, "{opened:?}");
        assert!(opened.identity_in_app_env.is_empty(), "{opened:?}");
        assert_eq!(
            (opened.name.as_str(), opened.provider.as_str()),
            ("team", "1password")
        );
        assert!(!ran.exists(), "op ran under an identity nobody declared");
    }

    #[test]
    fn a_token_pasted_for_a_vault_whose_token_was_nowhere_answers_with_its_secrets() {
        let (_dir, ctx) = team(&[]);
        assert!(open(&ctx, "team").unwrap().refused.is_some());

        let put = put_identity(&ctx, "team", &SecretValue::from(TOKEN)).unwrap();

        assert_eq!(put.refused, None, "{put:?}");
        assert_eq!(keys(&put), ["DEPLOY"]);
        assert_eq!(
            identity_of(&put),
            [("OP_TEAM_TOKEN", IdentityHeld::Keyring)]
        );
        assert!(put.identity_in_app_env.is_empty(), "{put:?}");
    }

    #[test]
    fn a_store_gives_no_other_vault_the_token_and_names_those_that_still_have_none() {
        // D-1526-7: a token is stored per vault, from its own tab. The answer only points.
        let (_dir, ctx) = team(&[]);
        alike(&ctx, "edge");

        let put = put_identity(&ctx, "team", &SecretValue::from(TOKEN)).unwrap();

        assert_eq!(put.identity_unset_elsewhere, ["edge"]);
        let edge = open(&ctx, "edge").unwrap();
        assert_eq!(
            edge.refused.as_ref().map(|r| r.kind),
            Some(UnreadFor::NoToken)
        );
        assert_eq!(identity_of(&edge), [("OP_TEAM_TOKEN", IdentityHeld::Unset)]);
        // team's token is kept, so edge has no other vault to point at.
        assert!(edge.identity_unset_elsewhere.is_empty(), "{edge:?}");

        // Put in from its own tab, it reads, and team has nothing left to point at.
        let edge = put_identity(&ctx, "edge", &SecretValue::from(TOKEN)).unwrap();
        assert_eq!(edge.refused, None, "{edge:?}");
        assert!(
            open(&ctx, "team")
                .unwrap()
                .identity_unset_elsewhere
                .is_empty()
        );
    }

    /// `team`, its token kept, opened against an `op` that now runs `script`: what kept it from
    /// being read, and how many times `op` was run to find out.
    fn unread_with(script: &str) -> (VaultContents, usize) {
        let (dir, ctx) = team(&[]);
        put_identity(&ctx, "team", &SecretValue::from(TOKEN)).unwrap();
        let ran = op_now(&dir, script);
        let opened = open(&ctx, "team").unwrap();
        let runs = std::fs::read_to_string(ran)
            .unwrap_or_default()
            .lines()
            .count();
        (opened, runs)
    }

    #[test]
    fn a_stored_token_that_does_not_read_the_vault_still_answers_the_identity() {
        // The tab that stored the token must be able to replace it, so this is answered with
        // why and the identity, not refused.
        let (opened, _) = unread_with("echo '[ERROR] something else' >&2; exit 1");

        assert_eq!(
            opened.refused.as_ref().map(|r| r.kind),
            Some(UnreadFor::Other),
            "{opened:?}"
        );
        assert!(opened.secrets.is_empty(), "{opened:?}");
        assert_eq!(
            identity_of(&opened),
            [("OP_TEAM_TOKEN", IdentityHeld::Keyring)]
        );
    }

    #[test]
    fn a_failure_that_is_not_the_tokens_is_not_called_the_tokens() {
        for (stderr, kind) in [
            ("[ERROR] 429: rate-limited", UnreadFor::TryAgain),
            (
                "[ERROR] dial tcp: lookup my.1password.com: no such host",
                UnreadFor::TryAgain,
            ),
            (
                "[ERROR] You do not have permission to do this",
                UnreadFor::SignIn,
            ),
            ("[ERROR] no accounts configured", UnreadFor::SignIn),
        ] {
            let (opened, _) = unread_with(&format!("echo '{stderr}' >&2; exit 1"));
            assert_eq!(
                opened.refused.as_ref().map(|r| r.kind),
                Some(kind),
                "{stderr}: {opened:?}"
            );
        }
    }

    #[test]
    fn a_pinned_program_that_is_gone_is_said_to_be_the_program() {
        let (dir, ctx) = team(&[]);
        put_identity(&ctx, "team", &SecretValue::from(TOKEN)).unwrap();
        std::fs::remove_file(dir.path().join("bin/op")).unwrap();

        let opened = open(&ctx, "team").unwrap();

        let unread = opened.refused.expect("unread");
        assert_eq!(unread.kind, UnreadFor::Program);
        assert!(unread.why.contains("pinned"), "{}", unread.why);
    }

    #[test]
    fn an_unreadable_vault_is_opened_without_asking_the_provider_for_its_health_too() {
        // The read asks for the item and, when that fails, whether the item is there: two runs.
        // The health line is the refusal, not a third run to hear it again.
        let (opened, runs) = unread_with("exit 1");
        assert_eq!(runs, 2, "{opened:?}");
        let unread = opened.refused.expect("unread");
        assert_eq!(
            opened.health,
            VaultHealth {
                ok: false,
                detail: unread.why.lines().next().unwrap().to_owned()
            }
        );
    }

    #[test]
    fn no_answer_about_a_vault_that_cannot_be_read_carries_the_token() {
        // A provider that prints the token it was handed, on both streams, failing and as a
        // body that is not what was asked for: if any of it reached an answer, this would see.
        const ECHO: &str = "printf '%s\n' \"$OP_SERVICE_ACCOUNT_TOKEN\"; \
                            printf '%s\n' \"$OP_SERVICE_ACCOUNT_TOKEN\" >&2";
        let (dir, ctx) = team(&[]);
        let mut answers = vec![wire(&open(&ctx, "team")), wire(&list(&ctx))];
        answers.push(wire(&put_identity(&ctx, "team", &SecretValue::from(TOKEN))));
        let ran = op_now(&dir, &format!("{ECHO}; exit 1"));
        answers.push(wire(&open(&ctx, "team")));
        answers.push(wire(&list(&ctx)));
        answers.push(wire(&put_identity(&ctx, "team", &SecretValue::from(TOKEN))));
        assert!(
            ran.exists(),
            "the provider was never run, so it echoed nothing"
        );
        op_now(&dir, &format!("{ECHO}; exit 0"));
        answers.push(wire(&open(&ctx, "team")));
        answers.push(wire(&list(&ctx)));
        // A keyring that cannot be written: the refusal comes with the token in hand.
        std::fs::write(ctx.state.join(keyring::STUB_FILE), "not json").unwrap();
        answers.push(wire(&put_identity(&ctx, "team", &SecretValue::from(TOKEN))));
        answers.push(wire(&open(&ctx, "team")));
        answers.extend(traced(&ctx).iter().map(ToString::to_string));

        assert!(answers[0].contains("\"no-token\""), "{}", answers[0]);
        assert!(answers[2].contains("\"keyring\""), "{}", answers[2]);
        for answer in &answers {
            assert!(!answer.contains(TOKEN), "{answer}");
        }
    }

    // --- #1527: the guided set-up ------------------------------------------------------- //

    /// Made-up tokens: the stand-in signs in with any that holds `works`.
    const GIVEN: &str = "fixture-word-that-works-1527-app";
    const WRONG: &str = "fixture-word-refused-1527-app";

    /// A stand-in `op` that signs in with a token holding `works`, and otherwise fails
    /// printing the token it was handed on both streams.
    const SIGNS_IN: &str = "case \"$OP_SERVICE_ACCOUNT_TOKEN\" in *works*) ;; *) \
        printf '%s\n' \"$OP_SERVICE_ACCOUNT_TOKEN\"; \
        printf '%s\n' \"$OP_SERVICE_ACCOUNT_TOKEN\" >&2; exit 1;; esac\n\
        case \"$1 $2\" in\n\
        'vault list') printf '%s' '[{\"name\":\"Engineering\"}]';;\n\
        'item list') printf '%s' '[{\"title\":\"other\"}]';;\n\
        'item get') printf '%s' '{\"fields\":[{\"label\":\"DEPLOY\",\"value\":\"x\"}]}';;\n\
        esac";

    /// The main window, on the project `ctx` is for.
    fn id_of(ctx: &Ctx) -> At {
        At::new("main", PlaneId::for_tests(&ctx.root))
    }

    /// A new vault `fresh`, its items in the 1Password vault `Engineering`.
    fn fresh() -> SetupNew {
        SetupNew {
            vault: "fresh".into(),
            op_vault: "Engineering".into(),
            op_item: None,
        }
    }

    #[test]
    fn a_set_up_is_given_its_token_once_tested_and_made_and_no_answer_holds_the_token() {
        let (dir, ctx) = team(&[]);
        op_now(&dir, SIGNS_IN);
        let (setups, plane) = (Setups::default(), id_of(&ctx));
        let mut answers = Vec::new();

        let begun = setup_begin(
            &ctx,
            &setups,
            &plane,
            Some(&SecretValue::from(format!("{GIVEN}\n").as_str())),
            None,
            None,
        );
        answers.push(wire(&begun));
        let begun = begun.unwrap();
        assert_eq!(begun.op_vaults, ["Engineering"]);
        assert_eq!(begun.listing, None);

        let tested = setup_test(
            &ctx,
            &setups,
            &plane,
            begun.setup,
            "fresh",
            Some("Engineering"),
            None,
        );
        answers.push(wire(&tested));
        let tested = tested.unwrap();
        assert_eq!(
            (tested.items, tested.item.as_str(), tested.item_there),
            (1, "charter-fresh", false)
        );
        assert_eq!(tested.failed, None);
        // Nothing is registered by a test.
        assert!(open(&ctx, "fresh").is_err());

        let made = setup_create(&ctx, &setups, &plane, begun.setup, &fresh(), Vec::new());
        answers.push(wire(&made));
        let made = made.unwrap();
        assert_eq!(made.contents.refused, None, "{made:?}");
        assert_eq!(keys(&made.contents), ["DEPLOY"]);
        assert_eq!(made.contents.identity.len(), 1);
        assert!(made.contents.identity[0].kept);
        assert_eq!(made.contents.identity[0].held, IdentityHeld::Keyring);
        // The app lets go of the token once the vault is made.
        assert!(setups.given(&plane, begun.setup).is_err());
        assert!(format!("{setups:?}").len() < 20);

        answers.push(wire(&list(&ctx)));
        answers.extend(traced(&ctx).iter().map(ToString::to_string));
        answers.push(std::fs::read_to_string(ctx.local_registry()).unwrap());
        for answer in &answers {
            assert!(!answer.contains(GIVEN), "{answer}");
        }
    }

    #[test]
    fn a_token_that_does_not_sign_in_is_said_by_kind_and_nothing_is_made_by_the_test() {
        let (dir, ctx) = team(&[]);
        op_now(&dir, SIGNS_IN);
        let (setups, plane) = (Setups::default(), id_of(&ctx));

        let begun = setup_begin(
            &ctx,
            &setups,
            &plane,
            Some(&SecretValue::from(WRONG)),
            None,
            None,
        );
        let tested = setup_test(
            &ctx,
            &setups,
            &plane,
            begun.as_ref().unwrap().setup,
            "fresh",
            Some("Engineering"),
            None,
        );

        // The listing failed, so the name is typed; the test says why by kind.
        assert!(begun.as_ref().unwrap().op_vaults.is_empty());
        assert_eq!(
            begun.as_ref().unwrap().listing.as_ref().map(|f| f.kind),
            Some(UnreadFor::Other)
        );
        assert_eq!(
            tested.as_ref().unwrap().failed.as_ref().map(|f| f.kind),
            Some(UnreadFor::Other)
        );
        assert!(open(&ctx, "fresh").is_err());
        assert!(!ctx.state.join(keyring::STUB_FILE).exists());
        for answer in [wire(&begun), wire(&tested)] {
            assert!(!answer.contains(WRONG), "{answer}");
        }
    }

    #[test]
    fn a_set_up_is_held_for_its_own_project_and_until_another_is_begun_or_it_is_cancelled() {
        let (dir, ctx) = team(&[]);
        op_now(&dir, SIGNS_IN);
        let (setups, plane) = (Setups::default(), id_of(&ctx));
        let first = setup_begin(
            &ctx,
            &setups,
            &plane,
            Some(&SecretValue::from(GIVEN)),
            None,
            None,
        )
        .unwrap()
        .setup;
        let elsewhere = At::new("main", PlaneId::for_tests(dir.path()));

        assert!(setups.given(&elsewhere, first).is_err());
        assert!(setups.given(&plane, first).is_ok());
        let second = setup_begin(&ctx, &setups, &plane, None, Some("acme.1password.eu"), None)
            .unwrap()
            .setup;
        assert_eq!(setups.given(&plane, first).unwrap_err(), SETUP_GONE);
        setups.end("main", second);
        assert!(setups.given(&plane, second).is_err());
        // A create with nothing held makes nothing.
        let refused = setup_create(&ctx, &setups, &plane, second, &fresh(), vec![]);
        assert_eq!(refused.unwrap_err(), SETUP_GONE);
        assert!(open(&ctx, "fresh").is_err());
    }

    #[test]
    fn a_set_up_is_held_for_the_window_that_began_it_alone() {
        let (dir, ctx) = team(&[]);
        op_now(&dir, SIGNS_IN);
        let (setups, main) = (Setups::default(), id_of(&ctx));
        let split = At::new("window-2", main.plane.clone());
        let number = setup_begin(
            &ctx,
            &setups,
            &main,
            Some(&SecretValue::from(GIVEN)),
            None,
            None,
        )
        .unwrap()
        .setup;

        // Another window of the same project cannot use it, nor let go of it.
        assert_eq!(setups.given(&split, number).unwrap_err(), SETUP_GONE);
        assert!(setup_change(&ctx, &setups, &split, number, "team", vec![]).is_err());
        setups.end("window-2", number);
        assert!(setups.given(&main, number).is_ok());
        // The window that began it going, or its page reloading, lets go of the token.
        setups.forget("window-2");
        assert!(setups.given(&main, number).is_ok());
        setups.forget("main");
        assert_eq!(setups.given(&main, number).unwrap_err(), SETUP_GONE);
        assert!(
            open(&ctx, "team").unwrap().refused.is_some(),
            "nothing was stored"
        );
    }

    #[test]
    fn a_held_token_is_let_go_of_when_its_time_is_up_with_no_call_from_the_window() {
        let (dir, ctx) = team(&[]);
        op_now(&dir, SIGNS_IN);
        let setups = std::sync::Arc::new(Setups::default());
        let main = id_of(&ctx);
        let number = setup_begin(
            &ctx,
            &setups,
            &main,
            Some(&SecretValue::from(GIVEN)),
            None,
            None,
        )
        .unwrap()
        .setup;

        setups.expire_after(number, std::time::Duration::from_millis(50));
        assert!(setups.given(&main, number).is_ok());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        // Asked of the holder itself, never through `given`, whose own check would hide a
        // timer that never ran.
        while setups.held().is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "the timer never let go"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // A later set-up is not let go of by an earlier one's timer.
        let later = setup_begin(
            &ctx,
            &setups,
            &main,
            Some(&SecretValue::from(GIVEN)),
            None,
            None,
        )
        .unwrap()
        .setup;
        setups.expire_after(number, std::time::Duration::from_millis(1));
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(setups.given(&main, later).is_ok());
    }

    #[test]
    fn a_window_that_is_gone_or_reloaded_lets_go_of_the_set_up_it_began() {
        // What the app's `Destroyed` and page-load hooks call (`lib.rs`), on an app that holds
        // the set-ups as the real one does. Tauri's mock runtime sends no window events, so
        // the hooks themselves are wired in `lib.rs` and not driven here.
        use tauri::Manager as _;
        let (dir, ctx) = team(&[]);
        op_now(&dir, SIGNS_IN);
        let setups = std::sync::Arc::new(Setups::default());
        let held = At::new("held", id_of(&ctx).plane);
        let number = setup_begin(
            &ctx,
            &setups,
            &held,
            Some(&SecretValue::from(GIVEN)),
            None,
            None,
        )
        .unwrap()
        .setup;
        let app = tauri::test::mock_builder()
            .manage(std::sync::Arc::clone(&setups))
            .build(tauri_context!(test = true))
            .expect("a mock app");

        window_gone(&app, "window-2");
        assert!(
            app.state::<std::sync::Arc<Setups>>()
                .given(&held, number)
                .is_ok()
        );
        window_gone(&app, "held");
        assert!(setups.held().is_none(), "the window's token was kept");
        // An app that holds no set-ups at all is not troubled by a window going.
        let bare = tauri::test::mock_builder()
            .build(tauri_context!(test = true))
            .expect("a mock app");
        window_gone(&bare, "main");
    }

    #[test]
    fn a_vault_bound_to_a_variable_is_converted_from_its_tab_and_read_at_once() {
        let (dir, ctx) = team(&[]);
        op_now(&dir, SIGNS_IN);
        alike(&ctx, "edge");
        let (setups, plane) = (Setups::default(), id_of(&ctx));
        assert!(open(&ctx, "team").unwrap().refused.is_some());

        let begun = setup_begin(
            &ctx,
            &setups,
            &plane,
            Some(&SecretValue::from(GIVEN)),
            None,
            Some("team"),
        )
        .unwrap();
        // The other vault bound to the same variable is offered, with what would be pinned.
        assert_eq!(begun.alike.len(), 1);
        assert_eq!(
            (
                begun.alike[0].name.as_str(),
                begun.alike[0].op_vault.as_str()
            ),
            ("edge", "Edge")
        );
        assert!(
            begun.alike[0].ticked,
            "this machine's own, with no token yet"
        );
        // Tested where the vault's items already live: no 1Password vault is named.
        let tested = setup_test(&ctx, &setups, &plane, begun.setup, "team", None, None).unwrap();
        assert_eq!(tested.failed, None);
        assert_eq!(tested.item, "charter-team");

        // Stored with no tick: `edge` is left alone.
        let done = setup_change(&ctx, &setups, &plane, begun.setup, "team", vec![]).unwrap();

        assert_eq!(done.contents.refused, None, "{done:?}");
        assert_eq!(keys(&done.contents), ["DEPLOY"]);
        assert!(done.contents.identity[0].kept);
        assert!(done.marked.is_empty() && done.skipped.is_empty());
        assert!(open(&ctx, "edge").unwrap().refused.is_some());
    }

    #[test]
    fn a_tick_whose_vault_changed_since_it_was_shown_is_answered_as_skipped() {
        let (dir, ctx) = team(&[]);
        op_now(&dir, SIGNS_IN);
        alike(&ctx, "edge");
        let (setups, plane) = (Setups::default(), id_of(&ctx));
        let begun = setup_begin(
            &ctx,
            &setups,
            &plane,
            Some(&SecretValue::from(GIVEN)),
            None,
            Some("team"),
        )
        .unwrap();
        let tick = SetupTick {
            name: "edge".into(),
            digest: "not-what-was-shown".into(),
        };

        let done = setup_change(&ctx, &setups, &plane, begun.setup, "team", vec![tick]).unwrap();

        assert!(done.marked.is_empty());
        assert_eq!(
            done.skipped,
            [SetupSkipped {
                name: "edge".into(),
                why: "changed".into(),
                said: None
            }]
        );
    }

    #[test]
    fn a_1password_vault_with_no_token_of_its_own_that_cannot_be_read_still_opens() {
        // Its tab is where how it signs in is changed, so a refusal alone would hide the way out.
        let (dir, ctx) = team(&[]);
        op_now(&dir, "exit 1");
        let mut config = serde_json::Map::new();
        config.insert("op-vault".into(), serde_json::json!("Mine"));
        registry::add_vault(&ctx, "mine", "1password", config, None, false, false).unwrap();

        let opened = open(&ctx, "mine").unwrap();

        assert!(opened.identity.is_empty());
        assert!(opened.refused.is_some());
        assert!(opened.secrets.is_empty());
    }

    #[test]
    fn a_vault_with_no_identity_lists_none_and_has_nothing_to_move() {
        let (_dir, ctx) = plane();
        assert!(open(&ctx, "ops").unwrap().identity.is_empty());
        assert!(move_identity(&ctx, "ops").is_err());
    }

    #[test]
    fn no_answer_or_refusal_of_a_move_carries_the_token() {
        let (_dir, ctx) = team(&[("OP_TEAM_TOKEN", TOKEN)]);
        let mut answers = vec![wire(&open(&ctx, "team")), wire(&list(&ctx))];
        answers.push(wire(&move_identity(&ctx, "team")));
        answers.push(wire(&move_identity(&ctx, "nope")));
        answers.push(wire(&open(&ctx, "team")));
        answers.push(wire(&list(&ctx)));
        // A keyring that cannot be written: the refusal comes after the token was read.
        // In the project's own state folder, which is where the move wrote it.
        std::fs::write(ctx.state.join(keyring::STUB_FILE), "not json").unwrap();
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
        purlis_core::forklock::output(
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
