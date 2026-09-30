//! The sandbox a chat runs in (ADR 0067): one harness-agnostic policy, compiled per harness.

use std::path::{Path, PathBuf};

/// The table in `charter.toml` that holds the policy.
pub const TABLE: &str = "sandbox";

/// The file the policy is read from. Only the committed file: turning the sandbox on is a
/// restriction a plane may carry (ADR 0035), and nothing else says anything about it.
pub const FILE: &str = "charter.toml";

/// A named set of hosts a sandboxed chat may reach (ADR 0067 §3). What each holds is
/// [`Preset::hosts`]; SD-4 and SD-31 add to the set.
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

/// What a plane's `charter.toml` says about the sandbox: the policy where it turned it on, and
/// a sentence for each thing it said that charter does not honour.
///
/// **Absent is not "off".** A plane that says nothing runs its chats as it did before the
/// sandbox existed, until the operator takes the one-time offer (ruling V21, 1). A plane that
/// says `off` is refused, because a committed file can never loosen what a chat is confined to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Said {
    pub policy: Option<Policy>,
    pub refused: Vec<String>,
}

impl Said {
    /// What `text`, the whole of a `charter.toml`, says. No file, or one that is not TOML, says
    /// nothing: the doctor's row already reports a file that cannot be read.
    pub fn of(text: Option<&str>) -> Self {
        let Some(top) = text.and_then(|text| text.parse::<toml::Table>().ok()) else {
            return Self::default();
        };
        let Some(table) = top.get(TABLE) else {
            return Self::default();
        };
        let Some(table) = table.as_table() else {
            return Self {
                policy: None,
                refused: vec![format!(
                    "{TABLE} in {FILE} is not a table — [{TABLE}] holds mode and egress"
                )],
            };
        };
        let mut refused = Vec::new();
        for key in table.keys() {
            if key != "mode" && key != "egress" {
                refused.push(format!(
                    "{TABLE}.{key} in {FILE} is not a key charter reads — [{TABLE}] holds mode \
                     and egress"
                ));
            }
        }
        let on = match table.get("mode") {
            None => false,
            Some(toml::Value::String(word)) if word == "on" => true,
            Some(toml::Value::String(word)) if word == "off" => {
                refused.insert(
                    0,
                    format!(
                        "{TABLE}.mode in {FILE} cannot be \"off\": a committed file may turn \
                         the sandbox on and never off — only a person turns it off, for one chat"
                    ),
                );
                false
            }
            Some(_) => {
                refused.insert(
                    0,
                    format!(
                        "{TABLE}.mode in {FILE} is not \"on\" — the one value a plane may give it"
                    ),
                );
                false
            }
        };
        let egress = match table.get("egress") {
            None => Preset::DEFAULT.to_vec(),
            Some(toml::Value::Array(words)) => {
                let mut egress = Vec::new();
                for word in words {
                    match word.as_str().and_then(Preset::of_word) {
                        Some(preset) if !egress.contains(&preset) => egress.push(preset),
                        Some(_) => {}
                        None => refused.push(format!(
                            "{TABLE}.egress in {FILE} names {}, which is not a preset — one \
                             of: {}",
                            word,
                            Preset::ALL.map(Preset::word).join(", ")
                        )),
                    }
                }
                egress
            }
            Some(_) => {
                refused.push(format!(
                    "{TABLE}.egress in {FILE} is not a list of presets — one or more of: {}",
                    Preset::ALL.map(Preset::word).join(", ")
                ));
                Preset::DEFAULT.to_vec()
            }
        };
        Self {
            policy: on.then_some(Policy { egress }),
            refused,
        }
    }

    /// What the plane at `root` says.
    pub fn read(root: &Path) -> Self {
        Self::of(std::fs::read_to_string(root.join(FILE)).ok().as_deref())
    }
}

/// Everything in `text`'s `[sandbox]` that charter would not honour, as `file` holds it — for
/// the Project settings tab's save, which refuses to write it. Only the committed file is read
/// for it; `charter.local.toml` already refuses every table it does not carry.
pub fn refusals(text: &str, file: &str) -> Vec<String> {
    if file != FILE {
        return Vec::new();
    }
    Said::of(Some(text)).refused
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

    /// What it is, in a refusal.
    pub fn said(self) -> &'static str {
        match self {
            Self::CredentialStore => "the operating system's credential store (a keyring vault)",
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
/// **Only what exists today is resolved.** The audit directory, the device key and the hook
/// spool (class 2), the client scopes behind the terminal, fleet-MCP and approvals (class 3)
/// and a runner's install (class 4) are not on disk yet; each is added here in the change that
/// puts it there, and its class's test names it.
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

        // 1. Vaults. The state directory's vault files (plain-file defaults, a keyring vault's
        // index) and the key the fingerprints are keyed with, then every plain-file vault's own
        // file wherever it is configured, then each resolver CLI's local session.
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
        // The registry names where each vault's secrets are read from: readable, never
        // rewritten, or a chat could point a vault at a file of its own.
        deny(Class::Vaults, ctx.shared_registry(), Access::Write);
        deny(Class::Vaults, ctx.local_registry(), Access::Write);

        // 2. Integrity. The app's record holds the command lines the next launch runs, beside
        // the hook socket.
        deny(Class::Integrity, root.join(".charter/app"), Access::Write);

        // 3. Human powers. The machine store holds the approvals a person gave: which planes
        // may run which commands without asking.
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

/// The hosts `presets` let a chat reach, for a plane whose `charter.toml` is `plane`: each
/// preset's own, and for [`Preset::Forge`] the self-managed hosts the plane's `[[forge]]` blocks
/// name. A host that is not one ([`crate::forge::host_ok`]) is never added.
///
/// A first cut, and SD-4 owns what each preset holds; what a preset does not list is refused
/// rather than let through.
pub fn hosts(presets: &[Preset], plane: Option<&str>) -> Vec<String> {
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
            for host in forge_hosts(plane) {
                add(&host);
            }
        }
    }
    out
}

/// The hosts of `plane`'s `[[forge]]` blocks, without a port: a sandbox allows a host.
fn forge_hosts(plane: Option<&str>) -> Vec<String> {
    let Some(top) = plane.and_then(|text| text.parse::<toml::Table>().ok()) else {
        return Vec::new();
    };
    let Some(forges) = top.get("forge").and_then(toml::Value::as_array) else {
        return Vec::new();
    };
    forges
        .iter()
        .filter_map(|forge| forge.get("host")?.as_str())
        .filter(|host| crate::forge::host_ok(host))
        .map(|host| host.split(':').next().unwrap_or(host).to_owned())
        .collect()
}

/// The policy for one chat, resolved to this machine and ready for a harness's compiler: the
/// neutral answer every adapter reads, and the only one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled {
    pub denied: Denied,
    pub hosts: Vec<String>,
    pub os: Os,
}

impl Compiled {
    /// `policy`, for a chat in the plane at `root`, on `machine`.
    pub fn of(policy: &Policy, root: &Path, machine: &Machine) -> Self {
        let plane = std::fs::read_to_string(root.join(FILE)).ok();
        Self {
            denied: Denied::of(root, machine),
            hosts: hosts(&policy.egress, plane.as_deref()),
            os: machine.os,
        }
    }
}

/// Why a harness cannot hold a class on this machine, so the chat does not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uncompilable {
    pub class: Class,
    pub why: String,
}

/// The sandbox a chat starts under, compiled for its harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    ClaudeCode(claude::Settings),
}

/// What a chat of `harness` in the plane at `root` starts under on `machine`: `None` where the
/// plane has not turned the sandbox on, the compiled sandbox where it has — or the one sentence
/// saying why the chat does not start. `has` answers whether a program the backend needs is
/// installed ([`backend::installed`]).
///
/// **Fail closed** (ADR 0067 §1). A harness charter does not compile the policy for, a machine
/// the policy cannot be applied on, and a class the harness cannot hold here each refuse the
/// chat; none of them starts it unsandboxed.
pub fn for_start(
    harness: crate::harness::Harness,
    root: &Path,
    machine: &Machine,
    has: &dyn Fn(&str) -> bool,
) -> Result<Option<Applied>, String> {
    use crate::harness::Harness;
    let Some(policy) = Said::read(root).policy else {
        return Ok(None);
    };
    let lead = "this plane runs every chat sandboxed";
    if harness != Harness::ClaudeCode {
        return Err(format!(
            "{lead}, and charter cannot sandbox a {} chat yet, so it was not started. Start \
             this chat on a Claude Code profile.",
            harness.title()
        ));
    }
    if let Some(missing) = backend::missing(machine.os, has) {
        return Err(format!(
            "{lead}, and this machine cannot apply the sandbox: {missing}. Nothing was started."
        ));
    }
    let compiled = Compiled::of(&policy, root, machine);
    claude::settings(&compiled)
        .map(|settings| Some(Applied::ClaudeCode(settings)))
        .map_err(|refused| {
            format!(
                "{lead}, and {}, which the {} class needs denied. Nothing was started.",
                refused.why,
                refused.class.word()
            )
        })
}

pub mod backend;
pub mod claude;

#[cfg(test)]
mod tests;
