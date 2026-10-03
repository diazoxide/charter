//! The sandbox a chat runs in (ADR 0067): one harness-agnostic policy, compiled per harness.
//!
//! [`Plane`] reads a plane's `charter.toml` once; [`Said`] is what it says about the sandbox;
//! [`Denied`] resolves the denial classes to this machine; [`Compiled`] is the neutral answer
//! every harness's compiler reads; and [`for_start`] is the one place a chat is sandboxed or
//! refused. Each harness has one compiler, chosen in one match ([`compiler`]).

use std::fmt;
use std::path::{Path, PathBuf};

use crate::harness::Harness;

/// The table in `charter.toml` that holds the policy.
pub const TABLE: &str = "sandbox";

/// The file the policy is read from. Only the committed file: turning the sandbox on is a
/// restriction a plane may carry (ADR 0035), and nothing else says anything about it.
pub const FILE: &str = "charter.toml";

/// A named set of hosts a sandboxed chat may reach (ADR 0067 §3). What each holds is
/// [`hosts`]; SD-4 and SD-31 add to the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    ModelProviders,
    Forge,
    Toolchains,
}

impl Preset {
    /// Every preset, in the order a sentence lists them.
    pub const ALL: [Preset; 3] = [Preset::ModelProviders, Preset::Forge, Preset::Toolchains];

    /// What a new plane starts with (ruling V21, 5).
    pub const DEFAULT: [Preset; 3] = Self::ALL;

    /// The word `charter.toml` names it by.
    pub fn word(self) -> &'static str {
        match self {
            Self::ModelProviders => "model-providers",
            Self::Forge => "forge",
            Self::Toolchains => "toolchains",
        }
    }

    fn of_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.word() == word)
    }

    /// Every preset's word, as a refusal lists them. The one place that list is written.
    fn listed() -> String {
        Self::ALL.map(Self::word).join(", ")
    }
}

/// A plane's sandbox policy, where it has turned the sandbox on.
///
/// Only egress is the plane's to choose. What a chat may write (its own directory, and the
/// harness's own temp directory) and what it is always denied ([`Class`]) are not in the file
/// at all, so no file can widen them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub egress: Vec<Preset>,
}

/// A plane's `charter.toml`, read and parsed once for everything the sandbox asks of it: the
/// policy, the refusals and the forge hosts.
#[derive(Debug, Clone, Default)]
pub struct Plane {
    top: Option<toml::Table>,
}

impl Plane {
    /// The plane at `root`. No file, or one that is not TOML, says nothing: the doctor's row
    /// already reports a file that cannot be read.
    pub fn read(root: &Path) -> Self {
        Self::of(std::fs::read_to_string(root.join(FILE)).ok().as_deref())
    }

    /// `text`, the whole of a `charter.toml`.
    pub fn of(text: Option<&str>) -> Self {
        Self {
            top: text.and_then(|text| text.parse::<toml::Table>().ok()),
        }
    }

    /// What it says about the sandbox.
    pub fn said(&self) -> Said {
        Said::of(self.top.as_ref())
    }

    /// The hosts of its `[[forge]]` blocks, without a port: a sandbox allows a host. A host that
    /// is not one ([`crate::forge::host_ok`]) is never added.
    fn forge_hosts(&self) -> Vec<String> {
        let Some(forges) = self
            .top
            .as_ref()
            .and_then(|top| top.get("forge"))
            .and_then(toml::Value::as_array)
        else {
            return Vec::new();
        };
        forges
            .iter()
            .filter_map(|forge| forge.get("host")?.as_str())
            .filter(|host| crate::forge::host_ok(host))
            .map(|host| host.split(':').next().unwrap_or(host).to_owned())
            .collect()
    }
}

/// One thing a plane's `[sandbox]` says that charter does not honour as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// `sandbox` is not a table.
    NotATable,
    /// A key the schema does not have.
    UnknownKey(String),
    /// `mode = "off"`, which a plane can never carry.
    ModeOff,
    /// A `mode` that is not `"on"` or `"off"`.
    ModeUnknown,
    /// `egress` that is not a list.
    EgressNotAList,
    /// A word in `egress` that is not a preset, as written.
    EgressUnknown(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let on = "so the sandbox is on";
        match self {
            Self::NotATable => write!(
                f,
                "{TABLE} in {FILE} is not a table — [{TABLE}] holds mode and egress; {on}"
            ),
            Self::UnknownKey(key) => write!(
                f,
                "{TABLE}.{key} in {FILE} is not a key charter reads — [{TABLE}] holds mode and \
                 egress"
            ),
            Self::ModeOff => write!(
                f,
                "{TABLE}.mode in {FILE} cannot be \"off\": a committed file may turn the sandbox \
                 on and never off — only a person turns it off, for one chat; {on}"
            ),
            Self::ModeUnknown => write!(
                f,
                "{TABLE}.mode in {FILE} is not \"on\" — the one value a plane may give it; {on}"
            ),
            Self::EgressNotAList => write!(
                f,
                "{TABLE}.egress in {FILE} is not a list of presets — one or more of: {}; the \
                 default is used",
                Preset::listed()
            ),
            Self::EgressUnknown(word) => write!(
                f,
                "{TABLE}.egress in {FILE} names {word}, which is not a preset — one of: {}",
                Preset::listed()
            ),
        }
    }
}

/// What a plane's `charter.toml` says about the sandbox: the policy where it is on, and each
/// thing it said that charter does not honour as written.
///
/// **Absent is not "off".** A plane that has no `[sandbox]`, or no `mode` in it, runs its chats
/// as it did before the sandbox existed, until the operator takes the one-time offer (ruling
/// V21, 1). **Anything else that is not `"on"` is read as `"on"`**, with a refusal: a committed
/// file can never loosen what a chat is confined to, and a typo must not do it by accident.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Said {
    pub policy: Option<Policy>,
    pub refused: Vec<Refusal>,
}

impl Said {
    /// What `top`, a parsed `charter.toml`, says.
    pub fn of(top: Option<&toml::Table>) -> Self {
        let Some(table) = top.and_then(|top| top.get(TABLE)) else {
            return Self::default();
        };
        let Some(table) = table.as_table() else {
            return Self {
                policy: Some(Policy {
                    egress: Preset::DEFAULT.to_vec(),
                }),
                refused: vec![Refusal::NotATable],
            };
        };
        let mut refused = Vec::new();
        let on = match table.get("mode") {
            None => false,
            Some(toml::Value::String(word)) if word == "on" => true,
            Some(toml::Value::String(word)) if word == "off" => {
                refused.push(Refusal::ModeOff);
                true
            }
            Some(_) => {
                refused.push(Refusal::ModeUnknown);
                true
            }
        };
        for key in table.keys() {
            if key != "mode" && key != "egress" {
                refused.push(Refusal::UnknownKey(key.clone()));
            }
        }
        let egress = match table.get("egress") {
            None => Preset::DEFAULT.to_vec(),
            Some(toml::Value::Array(words)) => {
                let mut egress = Vec::new();
                for word in words {
                    match word.as_str().and_then(Preset::of_word) {
                        Some(preset) if !egress.contains(&preset) => egress.push(preset),
                        Some(_) => {}
                        None => refused.push(Refusal::EgressUnknown(word.to_string())),
                    }
                }
                egress
            }
            Some(_) => {
                refused.push(Refusal::EgressNotAList);
                Preset::DEFAULT.to_vec()
            }
        };
        Self {
            policy: on.then_some(Policy { egress }),
            refused,
        }
    }
}

/// Everything in `text`'s `[sandbox]` that charter would not honour as written, as `file`
/// holds it — for the Project settings tab's save, which refuses to write it. Only the
/// committed file is read for it; `charter.local.toml` already refuses every table it does not
/// carry.
pub fn refusals(text: &str, file: &str) -> Vec<String> {
    if file != FILE {
        return Vec::new();
    }
    Plane::of(Some(text))
        .said()
        .refused
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// What a chat's sandbox always denies (ADR 0067 §5). Classes, not a list of paths: each is
/// turned into rules by [`Denied::of`], and no plane, persona or preset can remove one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    /// Chats never read a vault directly: every provider's storage and local session.
    Vaults,
    /// Charter's integrity state, which `charterd` alone writes.
    Integrity,
    /// What a person holds that a chat must not: the approvals and scopes behind them.
    HumanPowers,
    /// On a runner, `charterd`'s install files and the git internals it operates on.
    RunnerInternals,
}

impl Class {
    pub const ALL: [Class; 4] = [
        Class::Vaults,
        Class::Integrity,
        Class::HumanPowers,
        Class::RunnerInternals,
    ];

    /// The word a refusal and the audit name the class by.
    pub fn word(self) -> &'static str {
        match self {
            Self::Vaults => "vaults",
            Self::Integrity => "integrity",
            Self::HumanPowers => "human-powers",
            Self::RunnerInternals => "runner-internals",
        }
    }
}

/// What a denied path is denied for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Neither read nor written.
    ReadWrite,
    /// Read, but never written.
    Write,
}

/// One path a chat is denied, and the class it is denied for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    pub class: Class,
    pub path: PathBuf,
    pub access: Access,
}

/// A class that is held by a service rather than a path: the compiler of each harness either
/// denies the service or cannot, and a harness that cannot is refused (ADR 0067 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// The operating system's credential store, where a `keyring` vault keeps its secrets
    /// (ADR 0047). Reached through a system service, never through a file a rule can name.
    CredentialStore,
}

impl Service {
    /// The class it is denied for.
    pub fn class(self) -> Class {
        match self {
            Self::CredentialStore => Class::Vaults,
        }
    }
}

/// The operating system a chat runs on, which decides the backend and what it can express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Linux,
    Windows,
    Other,
}

impl Os {
    /// This machine's.
    pub fn this() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

/// What of this machine the denial classes are resolved against.
#[derive(Debug, Clone)]
pub struct Machine {
    /// The environment charter runs in: `$CHARTER_HOME`, `$CHARTER_CONFIG_HOME`,
    /// `$XDG_CONFIG_HOME` and `$OP_CONFIG_DIR` move what is denied with them.
    pub env: crate::secrets::Env,
    pub home: Option<PathBuf>,
    pub os: Os,
}

impl Machine {
    /// This machine, as charter's own process sees it.
    pub fn this() -> Self {
        Self {
            env: crate::secrets::Env::from_process(),
            home: dirs::home_dir(),
            os: Os::this(),
        }
    }
}

/// Everything a chat in one plane is denied, resolved to this machine.
///
/// Each class is resolved to the state charter keeps for it on this machine, and a change that
/// adds state to a class adds it here, with its class's test naming it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Denied {
    pub paths: Vec<Denial>,
    pub services: Vec<Service>,
}

impl Denied {
    /// The denials for a chat in the plane at `root`, on `machine`.
    pub fn of(root: &Path, machine: &Machine) -> Self {
        let ctx = crate::secrets::Ctx::new(root, machine.env.clone());
        let mut paths = Vec::new();
        let mut deny = |class, path: PathBuf, access| {
            paths.push(Denial {
                class,
                path,
                access,
            })
        };

        // 1. Vaults: every provider's storage and local session, wherever it is configured.
        deny(Class::Vaults, ctx.vaults_dir(), Access::ReadWrite);
        deny(
            Class::Vaults,
            crate::secrets::fingerprint::key_path(&ctx),
            Access::ReadWrite,
        );
        let registry = crate::secrets::registry::load_registry(&ctx);
        let mut keyring = registry.is_err();
        if let Ok(doc) = &registry {
            for (name, _) in crate::secrets::registry::vaults(doc) {
                let Ok(vault) = crate::secrets::registry::vault_in(doc, &name) else {
                    continue;
                };
                match vault.provider.as_str() {
                    "plain-file" => {
                        if let Ok(file) = crate::secrets::plain_file::file_path(&ctx, &vault) {
                            deny(Class::Vaults, file, Access::ReadWrite);
                        }
                    }
                    "keyring" => keyring = true,
                    _ => {}
                }
            }
        }
        if let Some(home) = &machine.home {
            let op = machine
                .env
                .get("OP_CONFIG_DIR")
                .filter(|dir| !dir.is_empty())
                .map_or_else(|| home.join(".config/op"), PathBuf::from);
            deny(Class::Vaults, op, Access::ReadWrite);
            deny(Class::Vaults, home.join(".op"), Access::ReadWrite);
            deny(Class::Vaults, home.join(".vault-token"), Access::ReadWrite);
        }
        // The registry that names the vaults: readable, never rewritten.
        deny(Class::Vaults, ctx.shared_registry(), Access::Write);
        deny(Class::Vaults, ctx.local_registry(), Access::Write);

        // 2. Integrity: charter's own records, which only charter writes.
        deny(Class::Integrity, root.join(".charter/app"), Access::Write);
        // Every chat's hook spool and the keys that check it, neither read nor written: the
        // hooks that write a spool run outside the sandbox their tools run in (ADR 0068 §6 as
        // amended by V63), so nothing inside it needs them.
        deny(
            Class::Integrity,
            crate::hookwire::spool::dir_for(&root.join(".charter/app/hooks.sock")),
            Access::ReadWrite,
        );

        // 3. Human powers: the approvals a person gave on this machine.
        let config_root = crate::machine::rooted(
            machine.env.get(crate::machine::HOME_VAR).map(Into::into),
            machine.env.get("XDG_CONFIG_HOME").map(Into::into),
            machine.home.clone(),
        );
        if let Some(config_root) = config_root {
            deny(
                Class::HumanPowers,
                crate::machine::dir(&config_root),
                Access::Write,
            );
            // What the human's forge sign-in fetched: the native transport's ETag store holds
            // raw answers, private repos' among them, so a chat neither reads nor writes it
            // (ADR 0070 §3 and §4).
            deny(
                Class::HumanPowers,
                crate::forge::etag::root(&config_root),
                Access::ReadWrite,
            );
        }

        Self {
            paths,
            services: if keyring {
                vec![Service::CredentialStore]
            } else {
                Vec::new()
            },
        }
    }
}

/// The hosts `presets` let a chat reach in `plane`: each preset's own, and for
/// [`Preset::Forge`] the hosts of the plane's `[[forge]]` blocks — the forges its logins are
/// checked against (ADR 0055).
///
/// A first cut, and SD-4 owns what each preset holds; what a preset does not list is refused
/// rather than let through.
pub fn hosts(presets: &[Preset], plane: &Plane) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |host: &str| {
        if !out.iter().any(|it| it == host) {
            out.push(host.to_owned());
        }
    };
    for preset in presets {
        let listed: &[&str] = match preset {
            Preset::ModelProviders => &[
                "api.anthropic.com",
                "claude.ai",
                "platform.claude.com",
                "api.openai.com",
                "auth.openai.com",
                "chatgpt.com",
                "opencode.ai",
                "models.dev",
                "openrouter.ai",
            ],
            Preset::Forge => &[
                "github.com",
                "api.github.com",
                "codeload.github.com",
                "*.githubusercontent.com",
                "ghcr.io",
                "gitlab.com",
                "registry.gitlab.com",
            ],
            Preset::Toolchains => &[
                "registry.npmjs.org",
                "registry.yarnpkg.com",
                "pypi.org",
                "files.pythonhosted.org",
                "crates.io",
                "index.crates.io",
                "static.crates.io",
                "static.rust-lang.org",
                "proxy.golang.org",
                "sum.golang.org",
                "rubygems.org",
                "repo.maven.apache.org",
                "repo1.maven.org",
            ],
        };
        for host in listed {
            add(host);
        }
        if *preset == Preset::Forge {
            for host in plane.forge_hosts() {
                add(&host);
            }
        }
    }
    out
}

/// The policy for one chat, resolved to this machine and ready for a harness's compiler: the
/// neutral answer every compiler reads, and the only one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled {
    pub denied: Denied,
    pub hosts: Vec<String>,
    pub os: Os,
}

impl Compiled {
    /// Whether `harness`'s compiler can hold every class a service holds here. No compiler can
    /// yet: the credential store is refused for each harness, because no harness's settings
    /// deny it and each one's sandbox was measured reaching it or not measured at all.
    fn holds_every_service(&self, harness: Harness) -> Result<(), Uncompilable> {
        match self.denied.services.first() {
            Some(service) => Err(Uncompilable {
                harness,
                service: *service,
            }),
            None => Ok(()),
        }
    }

    /// `policy`, for a chat in `plane` at `root`, on `machine`.
    pub fn of(policy: &Policy, plane: &Plane, root: &Path, machine: &Machine) -> Self {
        Self {
            denied: Denied::of(root, machine),
            hosts: hosts(&policy.egress, plane),
            os: machine.os,
        }
    }
}

/// A class a harness's compiler cannot hold on this machine, so the chat does not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uncompilable {
    pub harness: Harness,
    pub service: Service,
}

impl Uncompilable {
    pub fn class(self) -> Class {
        self.service.class()
    }
}

/// The sandbox a chat starts under, compiled for its harness by that harness's compiler.
///
/// Made only by [`for_start`], for the harness it was asked about, and it says which
/// ([`Self::harness`]): the one place a session opens refuses one handed to a chat of another
/// harness, so a form compiled for one harness never reaches another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    harness: Harness,
    form: Form,
}

impl Applied {
    /// The harness it was compiled for.
    pub fn harness(&self) -> Harness {
        self.harness
    }

    /// What that harness is handed.
    pub fn form(&self) -> &Form {
        &self.form
    }

    /// The chat's whole line under this sandbox — the profile's `command`, then `armed`, the
    /// arguments that arm the harness, then `charters`, charter's own words — or the one
    /// sentence saying why it may not start.
    ///
    /// **Fail closed** (ADR 0067). A flag of the harness's own, in the chat's own words, can
    /// outrank the sandbox it is handed, so such a chat is refused, naming where the flag is.
    /// A harness that carries its sandbox as flags has them last among the flags, where they
    /// win, and in front of the subcommand, session id and first message that end the line.
    pub fn line(
        &self,
        command: Vec<String>,
        armed: Vec<String>,
        charters: Vec<String>,
    ) -> Result<Vec<String>, String> {
        self.harness
            .adapter()
            .sandboxed_line(&self.form, command, armed, charters)
    }
}

/// Each harness's own form of the policy, one variant per compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form {
    ClaudeCode(claude::Settings),
    Codex(codex::Flags),
}

/// A harness's compiler.
pub type Compiler = fn(&Compiled) -> Result<Form, Uncompilable>;

/// The compiler for `harness`, or none where charter has not written one yet: its adapter's
/// ([`crate::harness::HarnessAdapter::sandbox_compiler`]). A harness gains a sandbox by its
/// adapter gaining a compiler and a [`Form`] variant.
pub fn compiler(harness: Harness) -> Option<Compiler> {
    harness.adapter().sandbox_compiler()
}

/// The indefinite article a sentence puts before `word`.
fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    }
}

/// The harnesses charter can sandbox, as a sentence names them: `Claude Code or Codex`.
fn sandboxed_harnesses() -> String {
    let titles: Vec<&str> = Harness::ALL
        .into_iter()
        .filter(|harness| compiler(*harness).is_some())
        .map(Harness::title)
        .collect();
    match titles.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Why a chat did not start in a sandboxed plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotStarted {
    /// Charter has no compiler for the harness yet.
    NoCompiler(Harness),
    /// This machine cannot apply a sandbox.
    NoBackend(backend::Missing),
    /// The harness's compiler cannot hold a class here.
    Uncompilable(Uncompilable),
}

impl fmt::Display for NotStarted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lead = "this plane runs every chat sandboxed";
        match self {
            Self::NoCompiler(harness) => write!(
                f,
                "{lead}, and charter cannot sandbox {} {} chat yet, so it was not started. \
                 Start this chat on a {} profile.",
                article(harness.title()),
                harness.title(),
                sandboxed_harnesses()
            ),
            Self::NoBackend(missing) => write!(
                f,
                "{lead}, and this machine cannot apply the sandbox: {missing}. Nothing was \
                 started."
            ),
            Self::Uncompilable(it) => match it.service {
                Service::CredentialStore => write!(
                    f,
                    "{lead}, and {} cannot keep a chat away from the operating system's \
                     credential store, where this plane's keyring vaults are kept, so the \
                     vaults class cannot be held. Until charter can wrap the harness or \
                     resolve secrets for it, a sandboxed plane with a keyring vault starts no \
                     {} chat. Nothing was started.",
                    it.harness.title(),
                    it.harness.title()
                ),
            },
        }
    }
}

/// What a chat of `harness` in the plane at `root` starts under on `machine`: `None` where the
/// plane has not turned the sandbox on, the compiled sandbox where it has — or why the chat
/// does not start. `has` answers whether a program the backend needs is installed
/// ([`backend::installed`]).
///
/// **Fail closed** (ADR 0067 §1). A harness charter has no compiler for, a machine the policy
/// cannot be applied on, and a class the harness cannot hold here each refuse the chat; none
/// of them starts it unsandboxed.
pub fn for_start(
    harness: Harness,
    root: &Path,
    machine: &Machine,
    has: &dyn Fn(&str) -> bool,
) -> Result<Option<Applied>, NotStarted> {
    let plane = Plane::read(root);
    let Some(policy) = plane.said().policy else {
        return Ok(None);
    };
    let Some(compile) = compiler(harness) else {
        return Err(NotStarted::NoCompiler(harness));
    };
    if let Some(missing) = backend::missing(machine.os, has) {
        return Err(NotStarted::NoBackend(missing));
    }
    let form =
        compile(&Compiled::of(&policy, &plane, root, machine)).map_err(NotStarted::Uncompilable)?;
    Ok(Some(Applied { harness, form }))
}

pub mod backend;
pub mod claude;
pub mod codex;

#[cfg(test)]
mod tests;
