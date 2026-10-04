//! Which transport, and which credential, a forge call gets (ADR 0070 §4 and §5).
//!
//! [`route`] is the one place it is decided, from the account and the [`Caller`], and nothing
//! else decides it. [`Resolver`] applies it: one per forge instance, it turns each call's
//! `Caller` into the transport that call is sent through.
//!
//! **Only a human in the window, on an account charter holds a sign-in for, gets the native
//! transport and its token.** A chat never does, whatever its surface; nor does an MCP call
//! (the host serves MCP on a chat's connection only), a trigger (nobody is there to have asked)
//! or a `charter` command (a `charter` a chat runs is part of that chat, ADR 0070 ruling 5).
//! A failure on the route chosen is reported, never retried on the other (ruling 7): the retry
//! would run under a different identity.
//!
//! **A sign-in is the host's to hand over.** [`Resolver::signed_in`] takes a [`HostScope`],
//! which [`HostScope::claim`] refuses inside a chat's process tree. The `charter` binary never
//! claims one (`tests/a_forge_token_never_reaches_a_chat.rs` checks its source), so the
//! resolver a command builds holds no token to route to. This is the in-process half of the
//! rule; the out-of-process half is the chat's sandbox, which denies the keyring item the
//! token lives in (ADR 0067's first denial class), and `charterd`'s client scopes (FD-27).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::backend::{Account, Caller, Principal, Surface, Transports};
use super::budget::{Clock, Meter, Metered, SystemClock};
use super::cli::Cli;
use super::etag::{EtagDir, EtagStore, MemoryEtags};
use super::http::{ApiRoot, Http, TokenSource};
use super::transport::Transport;
use super::{ForgeError, Kind};

/// What charter knows about one account when it routes a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccountState {
    /// The account has a charter sign-in.
    pub signed_in: bool,
    /// That sign-in was imported from the CLI's login (FI2), so the CLI login is the human's
    /// now and chats lose it.
    pub imported_from_cli: bool,
    /// The operator chose the CLI transport for this account in settings.
    pub prefer_cli: bool,
}

/// Where a call goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// HTTPS with the account's sign-in token.
    Native,
    /// `gh api` / `glab api` with the CLI's own login.
    Cli,
    /// No forge credential at all, and why.
    Refused(String),
}

/// The route a call from `caller` takes, given what is known of its account.
pub fn route(account: AccountState, caller: &Caller) -> Route {
    if caller.is_a_chat() {
        if account.imported_from_cli {
            return Route::Refused(
                "this host's CLI login was imported into charter, so it is the human's now, \
                 and a chat has no forge credential of its own here yet"
                    .to_string(),
            );
        }
        return Route::Cli;
    }
    match (caller.principal(), caller.surface()) {
        (Principal::Human, Surface::Window) if account.signed_in && !account.prefer_cli => {
            Route::Native
        }
        (Principal::Human, Surface::Window | Surface::Command | Surface::Trigger) => Route::Cli,
        // `is_a_chat` answered every chat and every MCP call above.
        (Principal::Chat(_), _) | (_, Surface::Mcp) => Route::Cli,
    }
}

/// Proof that this process is the host, not a process a chat started: what a sign-in is
/// handed over with.
#[derive(Debug)]
pub struct HostScope(());

/// The variables the app sets in every chat's environment (`hookwire::CHAT_ENV`, and the
/// session id `reopen` names).
const CHAT_MARKS: [&str; 2] = [crate::hookwire::CHAT_ENV, "CHARTER_SESSION_ID"];

impl HostScope {
    /// The host's scope, or `None` inside a chat's process tree.
    pub fn claim() -> Option<HostScope> {
        HostScope::claim_in(&|name| std::env::var(name).ok())
    }

    /// The host's scope, unconditionally, for a test build's recorded runs. Only a build with
    /// the plane fence on has it, and only a test build turns that on (`Cargo.toml`).
    #[cfg(feature = "fenced")]
    #[doc(hidden)]
    pub fn for_a_test() -> HostScope {
        HostScope(())
    }

    /// [`HostScope::claim`], with the environment asked through `env`.
    fn claim_in(env: &dyn Fn(&str) -> Option<String>) -> Option<HostScope> {
        let in_a_chat = CHAT_MARKS
            .iter()
            .any(|name| env(name).is_some_and(|v| !v.is_empty()));
        (!in_a_chat).then_some(HostScope(()))
    }
}

/// A sign-in charter holds for one account.
#[derive(Clone)]
pub struct SignIn {
    pub tokens: Arc<dyn TokenSource>,
    pub imported_from_cli: bool,
}

/// Where the native transport's ETag stores live.
enum Etags {
    Memory(Mutex<HashMap<Account, Arc<MemoryEtags>>>),
    Machine(PathBuf),
}

/// Turns each call's [`Caller`] into its transport, by [`route`], for one forge instance.
pub struct Resolver {
    kind: Kind,
    host: String,
    root: ApiRoot,
    signins: HashMap<Account, SignIn>,
    prefer_cli: Vec<Account>,
    cli: Arc<dyn Transport>,
    etags: Etags,
    clock: Arc<dyn Clock>,
    budgets: Option<PathBuf>,
    meters: Mutex<HashMap<Account, Arc<Meter>>>,
}

impl Resolver {
    /// A resolver for `kind` on `host` with no sign-ins: every call routes to the CLI until
    /// one is handed over.
    pub fn new(kind: Kind, host: &str) -> Resolver {
        Resolver {
            kind,
            host: host.to_string(),
            root: ApiRoot::of(kind, host),
            signins: HashMap::new(),
            prefer_cli: Vec::new(),
            cli: Arc::new(Cli::default()),
            etags: Etags::Memory(Mutex::new(HashMap::new())),
            clock: Arc::new(SystemClock),
            budgets: None,
            meters: Mutex::new(HashMap::new()),
        }
    }

    /// Send native requests to `root` rather than the host's own API (a recorded forge). Only a
    /// test build has it.
    #[cfg(any(test, feature = "fenced"))]
    pub fn at_root(mut self, root: ApiRoot) -> Resolver {
        self.root = root;
        self
    }

    /// `account` has a charter sign-in, handed over by the host.
    pub fn signed_in(mut self, _host: &HostScope, account: Account, signin: SignIn) -> Resolver {
        self.signins.insert(account, signin);
        self
    }

    /// The operator chose the CLI transport for `account`.
    pub fn preferring_cli(mut self, account: Account) -> Resolver {
        self.prefer_cli.push(account);
        self
    }

    /// Send CLI-routed calls through `cli` rather than the forge's own CLI (a test's stand-in).
    pub fn cli_over(mut self, cli: Arc<dyn Transport>) -> Resolver {
        self.cli = cli;
        self
    }

    /// Keep the ETag stores in the machine tier under `config_root` (ADR 0069).
    pub fn etags_in(mut self, config_root: &Path) -> Resolver {
        self.etags = Etags::Machine(config_root.to_path_buf());
        self
    }

    /// Keep each account's request budget in the machine tier under `config_root`, where
    /// `charter doctor` reads it (FW-4).
    pub fn budgets_in(mut self, config_root: &Path) -> Resolver {
        self.budgets = Some(config_root.to_path_buf());
        self
    }

    /// Tell the time by `clock`: a test's virtual clock.
    pub fn clock(mut self, clock: Arc<dyn Clock>) -> Resolver {
        self.clock = clock;
        self
    }

    /// `account`'s meter: one per account for this resolver's life, counting every call made
    /// as it on either route (FW-4).
    pub fn meter(&self, account: &Account) -> Arc<Meter> {
        let mut meters = self.meters.lock().unwrap_or_else(|e| e.into_inner());
        meters
            .entry(account.clone())
            .or_insert_with(|| {
                Arc::new(match &self.budgets {
                    Some(root) => Meter::kept_in(root, account, self.clock.clone()),
                    None => Meter::new(account, self.clock.clone()),
                })
            })
            .clone()
    }

    fn state(&self, account: Option<&Account>) -> AccountState {
        let signin = account.and_then(|a| self.signins.get(a));
        AccountState {
            signed_in: signin.is_some(),
            imported_from_cli: signin.is_some_and(|s| s.imported_from_cli),
            prefer_cli: account.is_some_and(|a| self.prefer_cli.contains(a)),
        }
    }

    /// The store for `account`'s native answers. It is keyed by the account the native
    /// transport sends as, and only the native transport uses it, so an answer a human's token
    /// fetched is never handed to a CLI call, which may be logged in as someone else.
    fn etags(&self, account: &Account) -> Arc<dyn EtagStore> {
        match &self.etags {
            Etags::Machine(root) => Arc::new(EtagDir::for_native(root, account)),
            Etags::Memory(stores) => {
                let mut stores = stores.lock().unwrap_or_else(|e| e.into_inner());
                stores.entry(account.clone()).or_default().clone()
            }
        }
    }
}

impl Transports for Resolver {
    fn for_caller(&self, caller: &Caller) -> Result<Arc<dyn Transport>, ForgeError> {
        if let Some(account) = caller.account()
            && (account.kind != self.kind || !account.host.eq_ignore_ascii_case(&self.host))
        {
            return Err(ForgeError::transport(format!(
                "the account {} is on {} {}, not this forge",
                account.login,
                account.kind.word(),
                account.host
            )));
        }
        let transport: Arc<dyn Transport> = match route(self.state(caller.account()), caller) {
            Route::Native => {
                // `route` answers Native only for an account with a sign-in.
                let account = caller.account().ok_or_else(|| {
                    ForgeError::transport("a native call names no account".to_string())
                })?;
                let signin = self.signins.get(account).ok_or_else(|| {
                    ForgeError::transport(format!("{} has no charter sign-in", account.login))
                })?;
                let native = Http::new(self.kind, self.root.clone(), signin.tokens.clone())?
                    .with_etags(self.etags(account));
                Arc::new(native)
            }
            Route::Cli => self.cli.clone(),
            Route::Refused(why) => return Err(ForgeError::of(super::Failure::Forbidden, why)),
        };
        // Every call this resolves for an account is admitted and counted by its meter,
        // whichever route it took. Only this builds the native transport outside tests
        // (`tests/only_the_resolver_builds_the_native_transport.rs`), so nothing sends with a
        // sign-in token unmetered. A call that names no account is the CLI's own login, not a
        // charter account, and has no budget here.
        Ok(match caller.account() {
            Some(account) => Arc::new(Metered::new(
                transport,
                self.meter(account),
                super::budget::admission(caller),
            )),
            None => transport,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_state() -> Vec<AccountState> {
        let mut all = Vec::new();
        for signed_in in [false, true] {
            for imported_from_cli in [false, true] {
                for prefer_cli in [false, true] {
                    all.push(AccountState {
                        signed_in,
                        imported_from_cli,
                        prefer_cli,
                    });
                }
            }
        }
        all
    }

    /// Every caller the constructors can make, on every surface they reach.
    fn every_caller() -> Vec<Caller> {
        let mut all = vec![Caller::window(), Caller::command(), Caller::trigger()];
        all.push(Caller::chat("c1"));
        all.push(Caller::mcp("c1"));
        let background: Vec<Caller> = all.iter().cloned().map(Caller::background).collect();
        all.extend(background);
        for surface in Surface::all() {
            assert!(
                all.iter().any(|c| c.surface() == surface),
                "no caller on {surface:?}"
            );
        }
        all
    }

    #[test]
    fn only_a_human_in_the_window_on_a_signed_in_account_resolves_to_the_sign_in_token() {
        for state in every_state() {
            for caller in every_caller() {
                let native = route(state, &caller) == Route::Native;
                let allowed = caller.surface() == Surface::Window
                    && *caller.principal() == Principal::Human
                    && state.signed_in
                    && !state.prefer_cli;
                assert_eq!(native, allowed, "{state:?} {caller:?}");
            }
        }
    }

    #[test]
    fn a_resolver_sends_each_forge_to_its_own_api() {
        assert_eq!(
            Resolver::new(Kind::GitLab, "gitlab.com").root,
            ApiRoot::gitlab("gitlab.com")
        );
        assert_eq!(
            Resolver::new(Kind::GitLab, "git.example.com").root,
            ApiRoot::gitlab("git.example.com")
        );
        assert_eq!(
            Resolver::new(Kind::GitHub, "github.com").root,
            ApiRoot::github("github.com")
        );
    }

    /// A CLI route that answers every request.
    struct Answers;
    impl Transport for Answers {
        fn send(
            &self,
            _forge: &crate::forge::Forge,
            _call: &crate::forge::transport::Call,
        ) -> Result<crate::forge::transport::Reply, crate::forge::transport::NoAnswer> {
            Ok(crate::forge::transport::Reply::of(
                0,
                "{}".into(),
                String::new(),
            ))
        }
        fn check_auth(&self, _forge: &crate::forge::Forge) -> Result<(), ForgeError> {
            Ok(())
        }
    }

    #[test]
    fn a_chat_naming_an_account_is_held_back_once_its_hour_is_spent_and_a_person_is_not() {
        use crate::forge::transport::{Call, NoAnswer};
        let account = Account {
            kind: Kind::GitHub,
            host: "github.com".into(),
            login: "octocat".into(),
        };
        let resolver = Resolver::new(Kind::GitHub, "github.com").cli_over(Arc::new(Answers));
        let meter = resolver.meter(&account);
        for _ in 0..crate::forge::budget::HOURLY_ALLOWANCE {
            meter.record(None);
        }
        let forge = crate::forge::Forge::default_of(Kind::GitHub);
        let call = Call::get("user", crate::forge::LIST_TIMEOUT);
        let send = |caller: Caller| {
            resolver
                .for_caller(&caller.as_account(account.clone()))
                .unwrap()
                .send(&forge, &call)
        };
        for chat in [Caller::chat("c1"), Caller::mcp("c1")] {
            assert!(
                matches!(send(chat), Err(NoAnswer::HeldBack { .. })),
                "a chat is never a person waiting"
            );
        }
        assert!(
            send(Caller::command()).is_ok(),
            "a person waiting goes through"
        );
        assert_eq!(meter.usage().held_back, 2);
    }

    #[test]
    fn a_trigger_never_resolves_to_the_sign_in_token() {
        let signed_in = AccountState {
            signed_in: true,
            ..AccountState::default()
        };
        assert_eq!(route(signed_in, &Caller::trigger()), Route::Cli);
    }

    #[test]
    fn a_chat_loses_an_imported_cli_login_and_keeps_one_that_was_not() {
        let imported = AccountState {
            signed_in: true,
            imported_from_cli: true,
            prefer_cli: false,
        };
        for chat in [Caller::chat("c1"), Caller::mcp("c1")] {
            assert!(matches!(route(imported, &chat), Route::Refused(_)));
            assert_eq!(
                route(
                    AccountState {
                        imported_from_cli: false,
                        ..imported
                    },
                    &chat
                ),
                Route::Cli
            );
        }
    }

    #[test]
    fn the_host_scope_is_refused_inside_a_chats_process_tree() {
        assert!(HostScope::claim_in(&|_| None).is_some());
        assert!(HostScope::claim_in(&|n| (n == "CHARTER_CHAT").then(|| "7".to_string())).is_none());
        assert!(
            HostScope::claim_in(&|n| (n == "CHARTER_SESSION_ID").then(|| "s".to_string()))
                .is_none()
        );
        assert!(
            HostScope::claim_in(&|n| (n == "CHARTER_CHAT").then(String::new)).is_some(),
            "an exported-but-blank variable is unset"
        );
    }
}
