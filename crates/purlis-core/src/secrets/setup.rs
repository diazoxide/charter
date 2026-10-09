//! Setting up how a 1Password vault signs in (#1527): asked, tested, and kept in the keyring.
//!
//! Until this module a vault read with a service-account token was set up by hand: a variable's
//! name in the registry, an export in a shell profile where every chat's shell could read it,
//! and later a move into the keyring from the vault's tab. Here the token is **given once, to
//! purlis itself**, tested, and stored, and no variable is involved at any point.
//!
//! # What it does
//!
//! - [`test`] signs in with what was given and reads the chosen 1Password vault's item
//!   **names**: `op item list`, never a value. Nothing is registered and nothing is stored. A
//!   failure is one of four [`Kind`]s and one of this module's own fixed sentences; what the
//!   provider's program printed is matched against and never repeated, since it can hold what
//!   it was given.
//! - [`create`] registers the vault **and** writes its keyring record in one step: the token's
//!   item first, then one write of this machine's registry half holding the vault and its
//!   record together. If any part fails, the item is deleted and both halves are put back as
//!   they were, so there is never a registered vault whose token is nowhere, and never a token
//!   nothing refers to.
//! - [`change`] is the same for a vault that exists: a vault bound to an environment variable
//!   becomes one whose token is kept in the keyring, without a restart.
//! - [`alike`] lists the OTHER vaults bound to the same identity, each with the settings a
//!   record would pin for it and a digest of them. A store marks only the vaults whose names it
//!   is handed **with the digest the person was shown**, and only while that digest still
//!   matches (ADR 0047's 2026-10-09 amendment): the person's tick on what they were shown makes
//!   a record, and a committed entry still cannot.
//!
//! # What a vault made here declares
//!
//! `"token": "keyring"` in its config ([`identity::TOKEN`], D-1527-1). It names no secret, no
//! variable and no keyring item, so it may be committed with `--share`. The record that points
//! at the item is in this machine's half only, pinned to the binding, the item the secrets are
//! kept in and the provider's program, exactly as a moved identity's is.
//!
//! # Who may call this
//!
//! The app's window (through commands no chat can invoke) and `purlis vault add --token-stdin`
//! at a terminal. **A chat is never the one supplying a vault's token**:
//! [`in_a_chat`] is asked before a byte of standard input is read.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use serde_json::{Map, Value};
use sha2::Digest as _;

use super::identity::{self, Held};
use super::registry::{self, Vault};
use super::run::{self, RunError};
use super::{Ctx, VaultError, keyring, onepassword};

/// How long one run of the provider's program may take during a set-up, so a dialog never
/// waits on a program that hangs.
const A_PROBE_TAKES_AT_MOST: Duration = Duration::from_secs(40);

/// The longest name or address this module passes to the provider's program.
const LONGEST: usize = 256;

/// The most names one listing answers with.
const MOST_LISTED: usize = 500;

/// How purlis signs in to 1Password for a vault.
#[derive(Clone, PartialEq, Eq)]
pub enum SignIn {
    /// A service-account token, kept in the keyring.
    Token(String),
    /// The 1Password app on this machine, through its command-line integration, with its own
    /// unlock. No credential is given to purlis.
    App,
}

impl std::fmt::Debug for SignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SignIn::Token(_) => "SignIn::Token(***)",
            SignIn::App => "SignIn::App",
        })
    }
}

/// What kept a test from passing, as far as purlis can tell. The same four the vault's tab
/// draws a failed read by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The provider's program is missing, or is only where a chat may write.
    Program,
    /// No network, a rate limit, a program that did not finish. Nothing says what was given is
    /// wrong.
    TryAgain,
    /// 1Password refused the sign-in.
    SignIn,
    /// Anything else, a vault 1Password does not have for this sign-in included.
    Other,
}

/// A test, or a listing, that did not pass: its kind and one of this module's sentences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub kind: Kind,
    pub why: String,
}

impl Failed {
    fn new(kind: Kind, why: impl Into<String>) -> Self {
        Self {
            kind,
            why: why.into(),
        }
    }
}

/// What a passed test saw. Names and counts, never a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tested {
    /// How many items the sign-in can see in the 1Password vault.
    pub items: usize,
    /// The item purlis keeps this vault's secrets in.
    pub item: String,
    /// Whether that item is there already. It is made at the first secret otherwise.
    pub item_there: bool,
}

/// Where a vault's items live, as a set-up was given it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Place {
    /// The 1Password vault.
    pub op_vault: String,
    /// The item; purlis's default for the vault when `None`.
    pub op_item: Option<String>,
    /// The account or sign-in address to pin, for this machine only.
    pub account: Option<String>,
}

const RATE_LIMITED: &str = "1Password rate-limited this sign-in. Nothing says what was given is \
                            wrong. Wait a little and test again.";
const NO_NETWORK: &str = "purlis could not reach 1Password: this machine has no network, or \
                          1Password did not answer. Nothing says what was given is wrong. Test \
                          again when the network is back.";
const DID_NOT_FINISH: &str = "The 1Password CLI (`op`) did not finish and was stopped. Nothing \
                              says what was given is wrong. Test again.";
const TOKEN_REFUSED: &str = "1Password refused the sign-in with this token. Check that it is the \
                             whole service-account token, that it has not been revoked or \
                             expired, and that it may read this vault.";
const APP_REFUSED: &str = "1Password refused the sign-in through its app. Check that the app is \
                           unlocked, that its command-line integration is turned on (Settings, \
                           Developer), and that the account or sign-in address is the right one.";
const NO_SUCH_VAULT: &str = "1Password answered, and has no vault of that name for this \
                             sign-in. Check the vault's name, and that this sign-in may read it.";
const NOT_READ: &str = "The 1Password CLI (`op`) answered with something purlis could not read. \
                        Check that it is a current version, and test again.";

/// What the provider's program said, matched to a kind and a sentence of purlis's own. `op`'s
/// text is matched against and never repeated. (The needles for a refused sign-in are from
/// 1Password's documented messages and are not verified against a live `op` in this repo: one
/// that does not match costs the wording, since the failure is then [`Kind::Other`].)
const REASONS: [(&str, Kind, Option<&str>); 15] = [
    ("rate-limited", Kind::TryAgain, Some(RATE_LIMITED)),
    ("too many requests", Kind::TryAgain, Some(RATE_LIMITED)),
    ("no such host", Kind::TryAgain, Some(NO_NETWORK)),
    ("network is unreachable", Kind::TryAgain, Some(NO_NETWORK)),
    ("dial tcp", Kind::TryAgain, Some(NO_NETWORK)),
    ("i/o timeout", Kind::TryAgain, Some(NO_NETWORK)),
    ("isn't a vault", Kind::Other, Some(NO_SUCH_VAULT)),
    ("no accounts configured", Kind::SignIn, None),
    ("you do not have permission", Kind::SignIn, None),
    ("decodesacredentials", Kind::SignIn, None),
    ("service account token", Kind::SignIn, None),
    ("unauthenticated", Kind::SignIn, None),
    ("unauthorized", Kind::SignIn, None),
    ("not currently signed in", Kind::SignIn, None),
    ("signin credentials", Kind::SignIn, None),
];

/// Why a run of `op` that exited `code` did not pass.
fn reason(sign_in: &SignIn, code: i32, stderr: &str) -> Failed {
    let low = stderr.to_lowercase();
    match REASONS.iter().find(|(needle, _, _)| low.contains(needle)) {
        Some((_, kind, Some(why))) => Failed::new(*kind, *why),
        Some((_, kind, None)) => Failed::new(
            *kind,
            match sign_in {
                SignIn::Token(_) => TOKEN_REFUSED,
                SignIn::App => APP_REFUSED,
            },
        ),
        None => Failed::new(
            Kind::Other,
            format!(
                "The test did not pass (op exit {code}), and purlis did not recognise why. \
                 Check the vault's name and the sign-in. purlis withholds what `op` printed, \
                 because it can hold what it was given."
            ),
        ),
    }
}

/// A [`VaultError`] as a failed test of kind [`Kind::Other`]: one of purlis's own refusals,
/// which holds names and never a value.
fn refused(e: VaultError) -> Failed {
    Failed::new(Kind::Other, e.message)
}

/// The token as it is stored: what was given with the whitespace around it gone, since a
/// pasted token arrives with a newline as often as not.
///
/// Refused when nothing is left, and when what is left holds anything but printable ASCII
/// with no space: a token is one plain word, and a paste with a space, a line break or an
/// invisible character inside it is two things or a damaged one. The refusal says so and never
/// repeats the text.
pub fn clean_token(given: &str) -> Result<String, VaultError> {
    let token = given.trim();
    if token.is_empty() {
        return Err(VaultError::new(
            "no token was given. purlis stores nothing for an empty one: the vault would look \
             set up and every read would fail as if it were not.",
        ));
    }
    if !token.chars().all(|c| c.is_ascii_graphic()) {
        return Err(VaultError::new(
            "what was given holds a space, a line break or a character no token has, and a \
             service-account token is one plain word. Nothing was stored. Copy the token alone \
             and give it again.",
        ));
    }
    Ok(token.to_owned())
}

/// The account or sign-in address as it is pinned, or `None` for none: `my.1password.com`, a
/// regional address, a company's own, an account's id or its shorthand.
///
/// **It reaches the provider program's arguments**, so it is held to an alphabet: a letter or a
/// digit, then letters, digits, `.`, `_` and `-`. A pasted `https://` and a trailing `/` are
/// taken off first. Anything else is refused, a leading `-` first of all.
pub fn clean_account(given: Option<&str>) -> Result<Option<String>, VaultError> {
    let Some(given) = given.map(str::trim).filter(|a| !a.is_empty()) else {
        return Ok(None);
    };
    let address = given
        .strip_prefix("https://")
        .unwrap_or(given)
        .trim_end_matches('/');
    let mut chars = address.chars();
    let ok = address.len() <= LONGEST
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        return Err(VaultError::new(
            "that is not an account or a sign-in address purlis will pass to `op`: letters, \
             digits, '.', '_' and '-', starting with a letter or a digit, as in \
             my.1password.com or acme.1password.eu.",
        ));
    }
    Ok(Some(address.to_owned()))
}

/// Whether `value` may be passed to the provider's program as a vault's or an item's name: not
/// empty, not read as an option, no control character, not longer than [`LONGEST`].
fn passable(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value.len() <= LONGEST
        && !value.chars().any(char::is_control)
}

/// The refusal for a 1Password vault's or item's name that is not [`passable`].
fn name_refusal(what: &str) -> VaultError {
    VaultError::new(format!(
        "purlis will not pass that {what} to `op`: it is empty, starts with '-', holds a \
         control character, or is longer than {LONGEST} characters."
    ))
}

/// `place` with every part checked and trimmed.
fn clean_place(place: &Place) -> Result<Place, VaultError> {
    let op_vault = place.op_vault.trim().to_owned();
    if !passable(&op_vault) {
        return Err(name_refusal("1Password vault's name"));
    }
    let op_item = place
        .op_item
        .as_deref()
        .map(str::trim)
        .filter(|i| !i.is_empty())
        .map(str::to_owned);
    if op_item.as_deref().is_some_and(|item| !passable(item)) {
        return Err(name_refusal("item's name"));
    }
    Ok(Place {
        op_vault,
        op_item,
        account: clean_account(place.account.as_deref())?,
    })
}

/// Run `op <args…>` signed in as `sign_in`, with `account` pinned when there is one: the
/// program the one lookup finds ([`Ctx::program`], never one where a chat may write), bounded,
/// with the token in the child's environment only and never in its arguments.
fn op(
    ctx: &Ctx,
    sign_in: &SignIn,
    account: Option<&str>,
    args: &[&str],
) -> Result<run::Ran, Failed> {
    let found = ctx.program("op").map_err(|why| {
        Failed::new(
            Kind::Program,
            ctx.not_run(
                "the 1Password CLI ('op')",
                "Install it (https://developer.1password.com/docs/cli/), then test again.",
                &why,
            ),
        )
    })?;
    let mut argv: Vec<String> = std::iter::once(found.path.display().to_string())
        .chain(args.iter().map(|a| (*a).to_owned()))
        .collect();
    if let Some(account) = account {
        argv.push("--account".into());
        argv.push(account.to_owned());
    }
    let overlay: Vec<(String, String)> = match sign_in {
        SignIn::Token(token) => vec![(identity::TOKEN_TARGET.to_owned(), token.clone())],
        SignIn::App => Vec::new(),
    };
    run::run(&ctx.env, &argv, None, &overlay, Some(A_PROBE_TAKES_AT_MOST)).map_err(|e| match e {
        RunError::Timeout => Failed::new(Kind::TryAgain, DID_NOT_FINISH),
        RunError::Interrupted(_) => Failed::new(Kind::TryAgain, DID_NOT_FINISH),
        // The reason is the system's, about the program's file: no provider output is in it.
        RunError::Spawn(e) => Failed::new(
            Kind::Program,
            format!("The 1Password CLI (`op`) could not be started: {e}"),
        ),
    })
}

/// The objects of the JSON array `op` printed.
fn listed(stdout: &str) -> Result<Vec<Map<String, Value>>, Failed> {
    let text = if stdout.trim().is_empty() {
        "[]"
    } else {
        stdout
    };
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Array(rows)) => Ok(rows
            .into_iter()
            .filter_map(|row| match row {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .collect()),
        _ => Err(Failed::new(Kind::Other, NOT_READ)),
    }
}

/// A name the provider listed, as the page and a terminal may show it and a later run may pass
/// it: on one line, and [`passable`]. Anything else is left out of the listing.
fn showable(row: &Map<String, Value>, field: &str) -> Option<String> {
    let name = row.get(field)?.as_str()?;
    (passable(name) && crate::personas::one_line(name) == name).then(|| name.to_owned())
}

/// **Test a sign-in before anything is registered.** Signs in with `sign_in` and reads the item
/// names of `place`'s 1Password vault. Writes nothing, stores nothing, and runs `op item list`
/// only: no value is read. `name` is the purlis vault being set up, for its default item.
pub fn test(ctx: &Ctx, name: &str, sign_in: &SignIn, place: &Place) -> Result<Tested, Failed> {
    let sign_in = cleaned(sign_in).map_err(refused)?;
    let place = clean_place(place).map_err(refused)?;
    let ran = op(
        ctx,
        &sign_in,
        place.account.as_deref(),
        &[
            "item",
            "list",
            "--vault",
            place.op_vault.as_str(),
            "--format",
            "json",
        ],
    )?;
    if ran.code != 0 {
        return Err(reason(&sign_in, ran.code, &ran.stderr));
    }
    let rows = listed(&ran.stdout)?;
    let item = place
        .op_item
        .clone()
        .unwrap_or_else(|| format!("charter-{name}"));
    Ok(Tested {
        items: rows.len(),
        item_there: rows
            .iter()
            .any(|row| row.get("title").and_then(Value::as_str) == Some(item.as_str())),
        item,
    })
}

/// A listing's failure in a listing's words: a failure of a kind purlis knows is said as that
/// kind (a refused sign-in, no network), and one it does not know says the listing failed,
/// never that a test did. A sign-in that may not list its vaults lands here, and the name is
/// then typed.
fn listing_failed(failed: Failed, code: i32) -> Failed {
    if failed.kind != Kind::Other || failed.why == NO_SUCH_VAULT {
        return failed;
    }
    Failed::new(
        Kind::Other,
        format!(
            "purlis could not list the 1Password vaults this sign-in can see (op exit {code}), \
             and did not recognise why. Some sign-ins may read a vault and not list them: type \
             the vault's name, and the test will say whether it can be read. purlis withholds \
             what `op` printed, because it can hold what it was given."
        ),
    )
}

/// The 1Password vaults `sign_in` can see, by name, sorted: what the set-up offers to choose
/// from. A sign-in that may not list its vaults fails here and the name is typed instead.
pub fn op_vaults(
    ctx: &Ctx,
    sign_in: &SignIn,
    account: Option<&str>,
) -> Result<Vec<String>, Failed> {
    let sign_in = cleaned(sign_in).map_err(refused)?;
    let account = clean_account(account).map_err(refused)?;
    let ran = op(
        ctx,
        &sign_in,
        account.as_deref(),
        &["vault", "list", "--format", "json"],
    )?;
    if ran.code != 0 {
        return Err(listing_failed(
            reason(&sign_in, ran.code, &ran.stderr),
            ran.code,
        ));
    }
    let mut names: Vec<String> = listed(&ran.stdout)?
        .iter()
        .filter_map(|row| showable(row, "name"))
        .collect();
    names.sort();
    names.dedup();
    names.truncate(MOST_LISTED);
    Ok(names)
}

/// One account the 1Password app on this machine is signed in to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// Its sign-in address: `my.1password.com`, a regional one, a company's own.
    pub address: String,
    /// Whose it is, as 1Password names them. Shown, never stored.
    pub email: String,
    /// What is pinned when it is chosen: the address, or the user's id where two accounts
    /// share an address.
    pub pin: String,
}

/// The accounts `op` lists on this machine (`op account list`), for the sign-in through the
/// 1Password app. No credential is involved: the app answers for itself.
pub fn accounts(ctx: &Ctx) -> Result<Vec<Account>, Failed> {
    let ran = op(
        ctx,
        &SignIn::App,
        None,
        &["account", "list", "--format", "json"],
    )?;
    if ran.code != 0 {
        return Err(reason(&SignIn::App, ran.code, &ran.stderr));
    }
    let rows = listed(&ran.stdout)?;
    let mut found: Vec<(String, String, Option<String>)> = rows
        .iter()
        .filter_map(|row| {
            let address = clean_account(Some(&showable(row, "url")?)).ok()??;
            let email = showable(row, "email").unwrap_or_default();
            let user = showable(row, "user_uuid").and_then(|id| clean_account(Some(&id)).ok()?);
            Some((address, email, user))
        })
        .collect();
    found.truncate(MOST_LISTED);
    let shared = |address: &str| found.iter().filter(|(a, _, _)| a == address).count() > 1;
    Ok(found
        .iter()
        .map(|(address, email, user)| Account {
            pin: match user {
                Some(user) if shared(address) => user.clone(),
                _ => address.clone(),
            },
            address: address.clone(),
            email: email.clone(),
        })
        .collect())
}

/// `sign_in` with its token cleaned ([`clean_token`]).
fn cleaned(sign_in: &SignIn) -> Result<SignIn, VaultError> {
    Ok(match sign_in {
        SignIn::Token(token) => SignIn::Token(clean_token(token)?),
        SignIn::App => SignIn::App,
    })
}

// ---------------------------------------------------------------------------------------
// The other vaults one token may be used for.

/// One other vault bound to the same identity, as the person is shown it before they tick it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alike {
    pub name: String,
    /// The settings a record would pin for it.
    pub op_vault: String,
    pub op_item: String,
    pub account: Option<String>,
    /// The persona it is tagged for: who may read through the record.
    pub persona: Option<String>,
    /// Which half of the registry names it: `local`, `shared` or `both`.
    pub half: String,
    /// Where its token is now.
    pub held: Held,
    /// Whether its box starts ticked: only a vault this machine's half alone names, bound to a
    /// variable, with no token yet. One the committed half names never starts ticked.
    pub ticked: bool,
    /// A digest of everything above that a record pins: what a store is handed back, and
    /// compares against the vault as it is then.
    pub digest: String,
}

/// One vault the person ticked, with the digest they were shown for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tick {
    pub name: String,
    pub digest: String,
}

/// Why a ticked vault was not given the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotMarked {
    /// Its settings are not the ones the person was shown: something changed them in between.
    Changed,
    /// It is not registered any more, or is no longer bound to this identity.
    Gone,
    /// The keyring or the registry refused; the sentence is the core's.
    Failed(String),
}

/// What became of the ticked vaults, and which variables the vault was read through before
/// that nothing reads now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marked {
    /// Given the token, each under its own item and record.
    pub marked: Vec<String>,
    /// Ticked and not given it, with why.
    pub skipped: Vec<(String, NotMarked)>,
    /// The variables the vault was read through before a conversion that no registered vault
    /// declares any more, sorted (#1542): only names a shell can export. The person's shell
    /// profile may still export one of them, and every shell started from it, and every
    /// program started from such a shell, then carries the token: what is said after a
    /// conversion asks for the line to be removed.
    pub no_longer_read: Vec<String>,
    /// Whether every project this machine opened was checked too, so that no vault of any of
    /// them reads [`Self::no_longer_read`] (#1542 review, M2). Where one could not be, the
    /// sentence speaks for this project alone.
    pub checked_every_project: bool,
}

/// Whether `name` is one a POSIX shell can export: a letter or `_`, then letters, digits and
/// `_`. Anything else is never named, since it is not in a startup file's `export` line, and a
/// committed name is never printed to a terminal raw (#1542 review, A).
fn exportable(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The sources the vaults of `ctx`'s project are read through, or `None` where its registry
/// cannot be read.
fn sources_in(ctx: &Ctx) -> Option<BTreeSet<String>> {
    let doc = registry::load_registry(ctx).ok()?;
    Some(
        registry::vaults(&doc)
            .keys()
            .filter_map(|name| registry::vault_in(&doc, name).ok())
            .flat_map(|vault| sources_of(&vault))
            .collect(),
    )
}

/// `marked`, with the variables in `was` that no vault reads now ([`Marked::no_longer_read`]).
///
/// An export in a shell profile is the whole machine's, so a variable is named only when no
/// vault of this project **and of every other project this machine opened** reads it; one any
/// of them still reads is never named, since removing its export would break that vault. A
/// registry of this project that cannot be read names none. Where another project's cannot
/// (or the machine store cannot), the names stand for this project and say so
/// ([`Marked::checked_every_project`]).
fn with_unread(ctx: &Ctx, was: &BTreeSet<String>, marked: Marked) -> Marked {
    let Some(mut still) = sources_in(ctx) else {
        return marked;
    };
    #[cfg(unix)]
    let home = account_home();
    #[cfg(not(unix))]
    let home: Option<std::path::PathBuf> = None;
    let mut everywhere = true;
    match projects_asked(ctx, home.as_deref()) {
        Ok(projects) => {
            for project in projects.iter().filter(|p| **p != ctx.root) {
                match sources_in(&Ctx::new(project, ctx.env.clone())) {
                    Some(theirs) => still.extend(theirs),
                    None => everywhere = false,
                }
            }
        }
        Err(_) => everywhere = false,
    }
    Marked {
        no_longer_read: was
            .iter()
            .filter(|source| {
                !identity::kept(source) && exportable(source) && !still.contains(*source)
            })
            .cloned()
            .collect(),
        checked_every_project: everywhere,
        ..marked
    }
}

/// What is said of the variables a conversion left no vault reading ([`Marked::no_longer_read`]):
/// remove their export. `None` where there are none.
pub fn remove_the_export(marked: &Marked) -> Option<String> {
    let listed = match marked.no_longer_read.as_slice() {
        [] => return None,
        names => names
            .iter()
            .filter(|n| exportable(n))
            .map(|n| format!("${n}"))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let whose = if marked.checked_every_project {
        format!("No vault of any project this machine opened reads {listed} any more.")
    } else {
        format!(
            "No vault of this project reads {listed} any more; purlis could not check every \
             other project this machine opened, so make sure none of them needs it."
        )
    };
    Some(format!(
        "{whose} If your shell's startup files export it, remove that line: until then every \
         shell started from them, and every program started from such a shell, still carries \
         the token."
    ))
}

/// The distinct sources `vault` is read through.
fn sources_of(vault: &Vault) -> BTreeSet<String> {
    identity::bindings(vault)
        .into_iter()
        .map(|(_, source)| source)
        .collect()
}

/// `vault` as the person is shown it, with the digest of what a record would pin.
fn shown(ctx: &Ctx, vault: &Vault, half: String) -> Alike {
    let op_vault = onepassword::op_vault(vault).unwrap_or_default();
    let op_item = onepassword::op_item(vault).unwrap_or_default();
    let account = super::config_str(&vault.config, "account")
        .map(|a| a.trim().to_owned())
        .filter(|a| !a.is_empty());
    let bindings: BTreeMap<String, String> = identity::bindings(vault).into_iter().collect();
    let held = identity::held(ctx, vault)
        .into_iter()
        .map(|b| b.held)
        .next()
        .unwrap_or(Held::Unset);
    let pinned = serde_json::json!({
        "name": vault.name,
        "provider": vault.provider,
        "bindings": bindings,
        "op_vault": op_vault,
        "op_item": op_item,
        "account": account,
        "persona": vault.persona,
        "half": half,
        // Shown as where its token is now ("has no token yet"): a token stored in the vault's
        // own tab after it was shown is not replaced on a tick given before.
        "held": format!("{held:?}"),
    });
    let digest = sha2::Sha256::digest(pinned.to_string().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let bound_to_a_variable = bindings.values().all(|source| !identity::kept(source));
    Alike {
        ticked: half == "local" && held == Held::Unset && bound_to_a_variable,
        name: vault.name.clone(),
        op_vault,
        op_item,
        account,
        persona: vault.persona.clone(),
        half,
        held,
        digest,
    }
}

/// The OTHER 1Password vaults of the project read through exactly `sources`, by name: what a
/// set-up lists under the token it was given. `except` is the vault being set up. **A listing
/// only: nothing is written and no keyring is read.** A registry that cannot be read lists
/// none.
pub fn alike(ctx: &Ctx, sources: &BTreeSet<String>, except: &str) -> Vec<Alike> {
    alike_read(ctx, sources, except)
        .map(|(_, listed)| listed)
        .unwrap_or_default()
}

/// [`alike`], and the merged registry it was made from: ONE read of both halves, so a store
/// pins each record to the very settings its digest was taken over. `None` where either half
/// cannot be read.
///
/// **Alike is the whole binding, not only its source** (#1527 review): the other vault must be
/// read through exactly one binding, the token's variable from that one source, so a vault
/// that binds the same variable to another target is never offered.
fn alike_read(
    ctx: &Ctx,
    sources: &BTreeSet<String>,
    except: &str,
) -> Option<(Map<String, Value>, Vec<Alike>)> {
    let [source] = sources.iter().collect::<Vec<_>>()[..] else {
        return None;
    };
    let shared = registry::load_shared(ctx).ok()?;
    let local = registry::load_local(ctx).ok()?;
    let doc = registry::merged(&shared, &local);
    let want = [(identity::TOKEN_TARGET.to_owned(), source.clone())];
    let mut names: Vec<String> = registry::vaults(&doc).keys().cloned().collect();
    names.sort();
    let listed = names
        .into_iter()
        .filter(|name| name != except)
        .filter_map(|name| registry::vault_in(&doc, &name).ok())
        .filter(|other| other.provider == "1password" && identity::bindings(other) == want)
        .map(|other| {
            let half = registry::scope_in(&shared, &local, &other.name);
            shown(ctx, &other, half)
        })
        .collect();
    Some((doc, listed))
}

/// [`alike`] for a vault that is registered: the others read through what `name` is read
/// through now.
pub fn alike_of(ctx: &Ctx, name: &str) -> Vec<Alike> {
    registry::vault(ctx, name)
        .map(|vault| alike(ctx, &sources_of(&vault), name))
        .unwrap_or_default()
}

/// The sources a vault being made by [`create`] will be read through: the kept token's alone.
pub fn kept_sources() -> BTreeSet<String> {
    BTreeSet::from([identity::KEPT_SOURCE.to_owned()])
}

/// Give `token` to each vault in `ticks` that is still alike and **still as the person was
/// shown it**, each under its own keyring item and its own record. A vault that is not named
/// is never touched, whichever half of the registry names it; one whose digest no longer
/// matches is skipped and said.
fn mark(
    ctx: &Ctx,
    token: &str,
    sources: &BTreeSet<String>,
    except: &str,
    ticks: &[Tick],
) -> Marked {
    let mut out = Marked::default();
    if ticks.is_empty() {
        return out;
    }
    let Some((doc, now)) = alike_read(ctx, sources, except) else {
        out.skipped = ticks
            .iter()
            .map(|t| (t.name.clone(), NotMarked::Gone))
            .collect();
        return out;
    };
    for tick in ticks {
        if out.marked.contains(&tick.name) || out.skipped.iter().any(|(n, _)| *n == tick.name) {
            continue;
        }
        let Some(seen) = now.iter().find(|a| a.name == tick.name) else {
            out.skipped.push((tick.name.clone(), NotMarked::Gone));
            continue;
        };
        if seen.digest != tick.digest {
            out.skipped.push((tick.name.clone(), NotMarked::Changed));
            continue;
        }
        let kept = registry::vault_in(&doc, &tick.name).and_then(|vault| {
            let source = identity::sole_source(&vault)?;
            identity::keep(ctx, &vault, &[(source, token.to_owned())])
        });
        match kept {
            Ok(_) => out.marked.push(tick.name.clone()),
            Err(e) => out
                .skipped
                .push((tick.name.clone(), NotMarked::Failed(e.message))),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// Create and change.

/// A vault to make.
#[derive(Debug, Clone)]
pub struct Request {
    pub name: String,
    pub place: Place,
    pub persona: Option<String>,
    pub sign_in: SignIn,
    /// Record the vault in the committed half too. The record, the account and the token stay
    /// on this machine.
    pub share: bool,
    /// Replace a registration of this name.
    pub force: bool,
    /// The other vaults the person ticked for this token.
    pub also: Vec<Tick>,
}

/// Both halves of the registry as they were, to put back if a set-up fails part way.
struct Halves {
    local: (bool, Map<String, Value>),
    shared: (bool, Map<String, Value>),
}

impl Halves {
    fn now(ctx: &Ctx) -> Result<Self, VaultError> {
        let there = |path: std::path::PathBuf| std::fs::symlink_metadata(path).is_ok();
        Ok(Self {
            local: (there(ctx.local_registry()), registry::load_local(ctx)?),
            shared: (there(ctx.shared_registry()), registry::load_shared(ctx)?),
        })
    }

    /// Put both halves back; a half that was not there is removed again. Answers whether both
    /// could be.
    fn put_back(&self, ctx: &Ctx) -> bool {
        let local = if self.local.0 {
            registry::save_local(ctx, &self.local.1).is_ok()
        } else {
            gone(&ctx.local_registry())
        };
        let shared = if self.shared.0 {
            registry::load_shared(ctx).is_ok_and(|now| now == self.shared.1)
                || registry::save_shared(ctx, &self.shared.1).is_ok()
        } else {
            gone(&ctx.shared_registry())
        };
        local && shared
    }
}

/// Remove the file at `path`, and answer whether nothing is there now.
fn gone(path: &std::path::Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

/// A set-up that failed after it had started writing: undo it, and say so if the undo failed.
fn undone(ctx: &Ctx, before: &Halves, stored: &[(String, String)], why: VaultError) -> VaultError {
    let registry = before.put_back(ctx);
    let items = identity::forget(ctx, stored) == 0;
    if registry && items {
        return why;
    }
    VaultError::new(format!(
        "{} And purlis could not undo all of what it had written: {}. Run `purlis vault list` \
         and `purlis doctor` to see what is there.",
        why.message,
        match (registry, items) {
            (false, false) => format!(
                "the registry could not be put back, and a token it had just stored could not \
                 be removed from {}",
                keyring::STORE_NAME
            ),
            (false, true) => "the registry could not be put back".to_owned(),
            _ => format!(
                "a token it had just stored could not be removed from {}",
                keyring::STORE_NAME
            ),
        }
    ))
}

/// The refusal for a vault whose effective settings are not the ones the set-up was given: the
/// committed half says something about it that the person was not shown.
fn not_as_given(name: &str) -> VaultError {
    VaultError::new(format!(
        "the committed registry (vaults.json) says something about vault '{name}' that this \
         set-up did not show, so nothing was registered and no token was stored. Look at what \
         vaults.json says of '{name}', then set it up again."
    ))
}

/// **Register a 1Password vault and write its keyring record in one step.**
///
/// For [`SignIn::Token`]: the token's item is stored, then the registry is written once with
/// the vault, its `"token": "keyring"` declaration and its pinned record together. The vault is
/// then read back, and unless its effective settings are exactly the ones given (so the
/// committed half adds nothing the person was not shown) and its record is honoured, all of it
/// is undone. **Both halves or neither**: any failure deletes the item and puts both registry
/// halves back as they were.
///
/// For [`SignIn::App`]: the vault is registered with its account pin and no credential.
///
/// Then the ticked vaults are marked ([`Tick`]). A vault of this name is refused unless
/// `force`; with it the registration is replaced and the items of its old record are deleted
/// once the new one is in place.
pub fn create(ctx: &Ctx, req: &Request) -> Result<Marked, VaultError> {
    if !registry::name_ok(&req.name) {
        return Err(VaultError::new(registry::name_refusal(&req.name)));
    }
    if let Some(p) = req.persona.as_deref().filter(|p| !p.is_empty())
        && let Some(refused) = crate::personas::name_refusal(&ctx.root, p)
    {
        return Err(VaultError::new(refused));
    }
    let persona = req.persona.as_deref().filter(|p| !p.is_empty());
    let place = clean_place(&req.place)?;
    let sign_in = cleaned(&req.sign_in)?;
    let before = Halves::now(ctx)?;
    let was = registry::vault(ctx, &req.name).ok();
    if was.is_some() && !req.force {
        return Err(VaultError::new(format!(
            "vault '{}' is already registered. purlis will not replace it: the registration \
             is the only pointer to that vault's secrets. Choose another name, or change how \
             it signs in from its own tab.",
            req.name
        )));
    }
    let replaced: Vec<(String, String)> = was
        .as_ref()
        .map(|old| identity::items_recorded(ctx, old))
        .unwrap_or_default();
    let was_read_through = was.as_ref().map(sources_of).unwrap_or_default();

    let mut cfg = Map::new();
    cfg.insert("op-vault".into(), Value::String(place.op_vault.clone()));
    if let Some(item) = &place.op_item {
        cfg.insert("op-item".into(), Value::String(item.clone()));
    }
    if let Some(account) = &place.account {
        cfg.insert("account".into(), Value::String(account.clone()));
    }
    let SignIn::Token(token) = &sign_in else {
        registry::add_vault(
            ctx,
            &req.name,
            "1password",
            cfg,
            persona,
            req.force,
            req.share,
        )?;
        identity::forget(ctx, &replaced);
        return Ok(with_unread(ctx, &was_read_through, Marked::default()));
    };

    cfg.insert(
        identity::TOKEN.into(),
        Value::String(identity::TOKEN_IN_KEYRING.into()),
    );
    let planned = Vault {
        name: req.name.clone(),
        provider: "1password".into(),
        persona: persona.map(str::to_owned),
        config: cfg.clone(),
    };
    let (op_cmd, op_team) = identity::resolve_op_now(ctx);
    let identity::Stashed { ids, items: stored } =
        identity::stash(ctx, &[(identity::KEPT_SOURCE.to_owned(), token.clone())])?;
    cfg.insert(
        identity::MARK.into(),
        Value::Object(identity::record_of(&planned, &ids, &op_cmd, &op_team)),
    );
    let made = registry::add_vault(
        ctx,
        &req.name,
        "1password",
        cfg,
        persona,
        req.force,
        req.share,
    )
    .and_then(|()| registry::vault(ctx, &req.name))
    .and_then(|made| {
        // What the registry now says of it, both halves merged, is what will be read. It must
        // be what was given and nothing more, and the record must be honoured for it.
        let same = sources_of(&made) == kept_sources()
            && identity::bindings(&made).len() == 1
            && identity::in_keyring(ctx, &made)
            && made.provider == planned.provider
            && made.persona == planned.persona;
        if same {
            Ok(made)
        } else {
            Err(not_as_given(&req.name))
        }
    });
    let made = match made {
        Ok(made) => made,
        Err(why) => return Err(undone(ctx, &before, &stored, why)),
    };
    identity::traced(ctx, &made);
    let left = identity::forget(ctx, &replaced);
    if left > 0 {
        tracing::warn!(
            "purlis: {left} replaced identity item(s) of vault '{}' could not be deleted from {}",
            crate::personas::one_line(&req.name),
            keyring::STORE_NAME
        );
    }
    let marked = mark(ctx, token, &kept_sources(), &req.name, &req.also);
    Ok(with_unread(ctx, &was_read_through, marked))
}

/// The refusal for a change the committed half stands in the way of.
fn committed_says(name: &str, what: &str) -> VaultError {
    VaultError::new(format!(
        "the committed registry (vaults.json) {what} for vault '{name}', and this machine's \
         half cannot take that away. Change vaults.json, or register the vault again with \
         `purlis vault add {name} --provider 1password --force`."
    ))
}

/// The `config` of `name` in one half, if that half names it.
fn config_in<'a>(half: &'a Map<String, Value>, name: &str) -> Option<&'a Map<String, Value>> {
    half.get("vaults")?.get(name)?.get("config")?.as_object()
}

/// **Change how the registered vault `name` signs in**, from its tab.
///
/// To [`SignIn::Token`]: the vault comes to declare `"token": "keyring"` in this machine's
/// half, in the same write as its record, so a vault that was bound to an environment variable
/// is read from the keyring from the next read on, with no restart and no export. A vault that
/// already declares it has its token replaced. `account` is not changed.
///
/// To [`SignIn::App`]: the declaration, the binding of the token's variable and the record are
/// taken out of this machine's half, the record's items are deleted, and `account` is pinned
/// (or the pin removed for `None`). Refused where the committed half is what declares the
/// token: this machine's half cannot take a committed field away.
///
/// The ticked vaults are those listed for the identity the vault had BEFORE the change
/// ([`alike_of`]).
pub fn change(
    ctx: &Ctx,
    name: &str,
    sign_in: &SignIn,
    account: Option<&str>,
    also: &[Tick],
) -> Result<Marked, VaultError> {
    let vault = registry::vault(ctx, name)?;
    if vault.provider != "1password" {
        return Err(VaultError::new(format!(
            "vault '{name}' is not a 1Password vault, so there is no sign-in to change."
        )));
    }
    let sign_in = cleaned(sign_in)?;
    let before = Halves::now(ctx)?;
    let replaced = identity::items_recorded(ctx, &vault);
    let was = sources_of(&vault);
    let token_only = identity::bindings(&vault)
        .iter()
        .all(|(target, _)| target == identity::TOKEN_TARGET);

    let SignIn::Token(token) = &sign_in else {
        let account = clean_account(account)?;
        let shared = registry::load_shared(ctx)?;
        if let Some(committed) = config_in(&shared, name) {
            if committed.get(identity::TOKEN).is_some_and(|t| !t.is_null()) {
                return Err(committed_says(name, "declares a token"));
            }
            if committed
                .get("env")
                .and_then(Value::as_object)
                .is_some_and(|env| env.contains_key(identity::TOKEN_TARGET))
            {
                return Err(committed_says(name, "binds the token's variable"));
            }
        }
        let mut local = registry::load_local(ctx)?;
        let entry = identity::object_at(identity::object_at(&mut local, "vaults"), name);
        let config = identity::object_at(entry, "config");
        config.shift_remove(identity::TOKEN);
        config.shift_remove(identity::MARK);
        let env_left = config
            .get_mut("env")
            .and_then(Value::as_object_mut)
            .map(|env| {
                env.shift_remove(identity::TOKEN_TARGET);
                env.len()
            });
        if env_left == Some(0) {
            config.shift_remove("env");
        }
        match account {
            Some(account) => config.insert("account".into(), Value::String(account)),
            None => config.shift_remove("account"),
        };
        registry::save_local(ctx, &local)?;
        identity::forget(ctx, &replaced);
        return Ok(with_unread(ctx, &was, Marked::default()));
    };

    if !token_only || was.len() > 1 {
        return Err(VaultError::new(format!(
            "vault '{name}' is read through more than the one token's variable, and this \
             set-up stores one token. Register it again with `purlis vault add {name} \
             --provider 1password --force`, then set it up from its tab."
        )));
    }
    // Declared in this machine's half, with the record, in one write.
    let mut planned = vault.clone();
    planned.config.insert(
        identity::TOKEN.into(),
        Value::String(identity::TOKEN_IN_KEYRING.into()),
    );
    let (op_cmd, op_team) = identity::resolve_op_now(ctx);
    let identity::Stashed { ids, items: stored } =
        identity::stash(ctx, &[(identity::KEPT_SOURCE.to_owned(), token.clone())])?;
    let record = identity::record_of(&planned, &ids, &op_cmd, &op_team);
    let written = registry::load_local(ctx)
        .and_then(|mut local| {
            let entry = identity::object_at(identity::object_at(&mut local, "vaults"), name);
            let config = identity::object_at(entry, "config");
            config.insert(
                identity::TOKEN.into(),
                Value::String(identity::TOKEN_IN_KEYRING.into()),
            );
            // The variable's binding is replaced by the record, where this half holds it. One
            // the committed half holds stays there and is not honoured beside the declaration.
            let env_left = config
                .get_mut("env")
                .and_then(Value::as_object_mut)
                .map(|env| {
                    env.shift_remove(identity::TOKEN_TARGET);
                    env.len()
                });
            if env_left == Some(0) {
                config.shift_remove("env");
            }
            config.insert(identity::MARK.into(), Value::Object(record));
            registry::save_local(ctx, &local)
        })
        .and_then(|()| registry::vault(ctx, name))
        .and_then(|now| {
            if sources_of(&now) == kept_sources() && identity::in_keyring(ctx, &now) {
                Ok(now)
            } else {
                Err(not_as_given(name))
            }
        });
    let now = match written {
        Ok(now) => now,
        Err(why) => return Err(undone(ctx, &before, &stored, why)),
    };
    identity::traced(ctx, &now);
    identity::forget(ctx, &replaced);
    // The others are those bound as this vault WAS: to its variable, or to a kept token.
    let basis = if was.is_empty() {
        kept_sources()
    } else {
        was.clone()
    };
    let marked = mark(ctx, token, &basis, name, also);
    Ok(with_unread(ctx, &was, marked))
}

/// Whether this process runs inside a chat (or a shell) the app started, as far as purlis can
/// tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Where {
    /// A terminal of the person's own.
    Outside,
    /// Inside a chat, or a shell the app started.
    Inside,
    /// purlis could not tell, and says why. Taken as inside: a doubt never lets a token in.
    Unsure(String),
}

/// Whether this process runs inside a chat (or a shell) the app started.
///
/// Two ways of knowing, either of which is enough:
///
/// - **what the app sets in the environment of everything it starts**: the chat's number, its
///   session's number, and the mark of its sandbox;
/// - **where this process runs**: the record of open chats of this project, and of every
///   project this machine opened ([`projects_asked`]), names each one's program, and a process that IS one, is in one's session (every chat's program leads its
///   own, so a process it started stays in it after its parent has gone), or runs below one is
///   inside that chat whatever its environment says (`purlis_same_user::inside_a_chat`).
///
/// **Every doubt is [`Where::Unsure`]**: a record that cannot be read or is not one this purlis
/// knows, a parent or a session that cannot be read, an ancestry that loops.
///
/// **A chat never supplies a vault's token**, so `vault add --token-stdin` asks this before it
/// reads standard input. It is a refusal that tells an agent where the token is given instead,
/// and not the boundary: what holds a sandboxed chat off a vault's token is that the registry
/// and the keyring are not its to write.
pub fn in_a_chat(ctx: &Ctx) -> Where {
    #[cfg(unix)]
    {
        in_a_chat_with(
            ctx,
            std::process::id(),
            purlis_same_user::Parents::of,
            purlis_same_user::session_of,
            account_home().as_deref(),
        )
    }
    #[cfg(not(unix))]
    {
        in_a_chat_with(
            ctx,
            std::process::id(),
            |_| None,
            |_| Err(std::io::Error::other("no sessions here")),
            None,
        )
    }
}

/// This account's home as the user database records it, whatever `$HOME` says, for the machine
/// store under it. A fenced (test) build reads none outside its fence: the real account's store
/// names the person's real projects, which a test never reads.
#[cfg(unix)]
fn account_home() -> Option<std::path::PathBuf> {
    let home = purlis_same_user::account_home()?;
    #[cfg(feature = "fenced")]
    if !crate::fence::inside(&home, &crate::fence::fence()) {
        return None;
    }
    Some(home)
}

/// [`in_a_chat`] for process `me`, with the kernel's answers and this account's home handed in.
pub(crate) fn in_a_chat_with(
    ctx: &Ctx,
    me: u32,
    parent: impl Fn(u32) -> Option<u32>,
    session: impl Fn(u32) -> std::io::Result<u32>,
    account_home: Option<&std::path::Path>,
) -> Where {
    let env = |name: &str| ctx.env.get(name);
    let set = |name: &str| env(name).is_some_and(|v| !v.trim().is_empty());
    if set(crate::hookwire::CHAT_ENV)
        || set(crate::active::SESSION_ID_ENV)
        || crate::sandbox::chat_is_sandboxed_in(&env)
    {
        return Where::Inside;
    }
    let projects = match projects_asked(ctx, account_home) {
        Ok(projects) => projects,
        Err(why) => return Where::Unsure(why),
    };
    let mut programs: Vec<u32> = Vec::new();
    for project in &projects {
        // Only the chats' processes, read leniently: a record of another version still names
        // the programs it knew (#1542 review, D).
        let pids = match crate::reopen::chat_pids(project) {
            Ok(pids) => pids,
            Err(e) => {
                let record =
                    crate::personas::one_line(&crate::reopen::path(project).display().to_string());
                let whose = if *project == ctx.root {
                    "the project's record of its open chats".to_owned()
                } else {
                    format!(
                        "the record of open chats of {}, a project this machine opened",
                        crate::personas::one_line(&project.display().to_string())
                    )
                };
                let forget = if *project == ctx.root {
                    ""
                } else {
                    ", or forget that project under Settings, This machine"
                };
                return Where::Unsure(format!(
                    "purlis could not read {whose} ({}). To clear the doubt, delete {record} \
                     (the app writes it again){forget}, then run this command again",
                    e.kind()
                ));
            }
        };
        // 0 and 1 are the kernel's and `init`'s, above every process: a record naming either
        // vouches for nothing.
        programs.extend(pids.into_iter().filter(|pid| *pid > 1));
    }
    if programs.is_empty() {
        return Where::Outside;
    }
    #[cfg(unix)]
    let answer = purlis_same_user::inside_a_chat(me, &programs, parent, session);
    #[cfg(not(unix))]
    let answer: std::io::Result<Option<u32>> = {
        let _ = (me, parent, session);
        Err(std::io::Error::other("no process ancestry here"))
    };
    match answer {
        Ok(None) => Where::Outside,
        Ok(Some(_)) => Where::Inside,
        Err(_) => Where::Unsure(
            "purlis could not read where this process runs: a parent or its session could \
             not be read"
                .to_owned(),
        ),
    }
}

/// The projects whose records of open chats [`in_a_chat`] reads: this one, and every project
/// a machine store remembers opening or had open in a window (#1542), so a chat of another
/// project that names this one is still found below its program. A store that cannot be read,
/// or is not one this purlis knows, is a doubt and answers why; a machine with no store, or no
/// home to find one in, remembers nothing.
///
/// **Two stores are read, and their projects joined**: the one `ctx`'s environment finds, as
/// every reader of the config home finds it, and the default one under `account_home`, this
/// account's home as the user database records it. A chat that points the environment at an
/// empty store of its own still has the person's projects read (#1542 review, B).
pub(crate) fn projects_asked(
    ctx: &Ctx,
    account_home: Option<&std::path::Path>,
) -> Result<Vec<std::path::PathBuf>, String> {
    let mut projects = vec![ctx.root.clone()];
    #[cfg(not(unix))]
    let _ = account_home;
    #[cfg(unix)]
    let stores: Vec<std::path::PathBuf> = {
        let mut stores: Vec<std::path::PathBuf> = crate::machine::rooted(
            ctx.env.get(crate::machine::HOME_VAR).map(Into::into),
            ctx.env.get("XDG_CONFIG_HOME").map(Into::into),
            ctx.env
                .get("HOME")
                .filter(|h| !h.is_empty())
                .map(std::path::PathBuf::from),
        )
        .into_iter()
        .chain(
            account_home
                .and_then(|home| crate::machine::rooted(None, None, Some(home.to_path_buf()))),
        )
        .collect();
        let mut seen = std::collections::BTreeSet::new();
        stores.retain(|root| {
            seen.insert(std::fs::canonicalize(root).unwrap_or_else(|_| root.clone()))
        });
        stores
    };
    #[cfg(not(unix))]
    let stores: Vec<std::path::PathBuf> = Vec::new();
    for config in stores {
        let loaded = crate::machine::read(&config);
        let doubt = loaded.unreadable.clone().or_else(|| {
            loaded.dropped.iter().find_map(|dropped| match dropped {
                crate::machine::Dropped::TheStore(why) => Some(why.clone()),
                _ => None,
            })
        });
        if let Some(why) = doubt {
            return Err(format!(
                "purlis could not read which projects this machine opened: its machine store {}",
                crate::personas::one_line(&why)
            ));
        }
        projects.extend(
            loaded
                .store
                .recents
                .into_iter()
                .map(|recent| recent.plane)
                .chain(loaded.store.windows.into_iter().flat_map(|w| w.planes)),
        );
    }
    // A remembered project that is no longer there holds no chats to ask about.
    let mut seen = std::collections::BTreeSet::new();
    projects.retain(|project| {
        (*project == ctx.root || project.is_dir())
            && seen.insert(std::fs::canonicalize(project).unwrap_or_else(|_| project.clone()))
    });
    Ok(projects)
}

/// The command that gives a 1Password vault this machine's registry half declares its token
/// from a terminal, as a sentence prints it: it works as printed and keeps every other setting
/// of the vault (`vault add --token-stdin` on such a vault is [`change`]).
pub fn token_again(vault: &str) -> String {
    format!("purlis vault add {vault} --provider 1password --token-stdin")
}

/// Whether this machine's registry half declares vault `name`: registers it with a provider,
/// as `vault add` writes it. A local entry that only pins an account over a committed vault
/// does not: the committed half still says what that vault is.
///
/// **A record for a vault only the committed half declares is made in the app's window
/// alone**, where the settings it will pin are shown (ADR 0047's 2026-10-09 amendment), never
/// by `vault add --token-stdin`. A half that cannot be read declares nothing.
pub fn declared_here(ctx: &Ctx, name: &str) -> bool {
    registry::load_local(ctx).is_ok_and(|local| {
        registry::usable_vaults(&local)
            .get(name)
            .and_then(|entry| entry.get("provider"))
            .and_then(Value::as_str)
            .is_some_and(|p| !p.is_empty())
    })
}

/// [`token_again`] where it works, for a vault this machine's half declares; `None` for one
/// only the committed half declares, whose token is given in its tab in the app.
pub fn token_again_for(ctx: &Ctx, vault: &str) -> Option<String> {
    declared_here(ctx, vault).then(|| token_again(vault))
}

/// What is said for a vault only the committed half declares, where a terminal would give it
/// its token: the app's tab, which shows the settings the record will pin.
pub const COMMITTED_ONLY: &str = "Its record is made in its tab in the app, which shows the \
     settings the committed vaults.json gives it before anything is stored.";

/// What is said when purlis could not tell whether it runs inside a chat: the reason, with how
/// to clear the doubt where there is a way, then where the token is given instead. Never "a
/// terminal of your own": the person may already be in one.
pub fn could_not_tell(why: &str) -> String {
    format!(
        "purlis cannot tell whether this runs inside a chat: {why}. A vault's token is never \
         given from a chat, so purlis read nothing from standard input and stored nothing. \
         Give it in New vault in the app, or in the vault's own tab, instead."
    )
}

/// What is said when a vault's token is offered from inside a chat.
pub const NOT_FROM_A_CHAT: &str = "a vault's token is never given from inside a chat, or from \
     a shell the app started, so purlis read nothing from standard input and stored nothing. \
     Ask the person to give it themselves: New vault in the app (or the vault's own tab), or \
     this command in a terminal of their own, outside the app.";

#[cfg(test)]
#[cfg(unix)]
#[path = "setup_tests.rs"]
mod tests;
