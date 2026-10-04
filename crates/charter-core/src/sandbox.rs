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

/// The `[sandbox]` block that turns the sandbox on with the default egress, as charter writes it
/// into a new project's `charter.toml` and into an existing one whose operator took the offer
/// (ADR 0067 §1, ruling V21 1 and 5). It ends with a newline.
pub fn on_block() -> String {
    let egress: Vec<String> = Preset::DEFAULT
        .iter()
        .map(|preset| format!("\"{}\"", preset.word()))
        .collect();
    format!(
        "[{TABLE}]\nmode = \"on\"\negress = [{}]\n",
        egress.join(", ")
    )
}

/// A plane's `charter.toml`, read and parsed once for everything the sandbox asks of it: the
/// policy, the refusals and the forge hosts.
#[derive(Debug, Clone, Default)]
pub struct Plane {
    top: Option<toml::Table>,
    /// The file is there and is not TOML charter can read, so what it says about the sandbox
    /// is unknown ([`NotStarted::PlaneUnreadable`]).
    unreadable: bool,
}

impl Plane {
    /// The plane at `root`. No file says nothing. A file that cannot be read, or is not TOML,
    /// is [`Self::unreadable`]: it may say `[sandbox]`, so it never reads as saying nothing.
    pub fn read(root: &Path) -> Self {
        match read_plane_file(&root.join(FILE)) {
            Ok(text) => Self::of(text.as_deref()),
            Err(()) => Self {
                top: None,
                unreadable: true,
            },
        }
    }

    /// `text`, the whole of a `charter.toml`.
    pub fn of(text: Option<&str>) -> Self {
        let top = text.map(|text| text.parse::<toml::Table>().ok());
        Self {
            unreadable: matches!(top, Some(None)),
            top: top.flatten(),
        }
    }

    /// Whether the file is there and charter cannot read it as TOML.
    pub fn unreadable(&self) -> bool {
        self.unreadable
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

/// The most of a `charter.toml` charter reads when a chat starts. Far more than any plane's,
/// and a bound on what a start can be made to read.
pub const PLANE_FILE_MAX: usize = 1024 * 1024;

/// The `charter.toml` at `path`: `None` where there is none, its text where it is a regular
/// file of at most [`PLANE_FILE_MAX`] bytes of UTF-8, and `Err` for anything else.
///
/// **Not followed, and never blocked on.** A link, dangling or not, a directory, a FIFO, a
/// socket and a device are each refused: the name is checked without following it, then opened
/// without following it and without waiting (`O_NOFOLLOW | O_NONBLOCK`), and the open file is
/// checked again before it is read, so nothing swapped in between is read either.
pub(crate) fn read_plane_file(path: &Path) -> Result<Option<String>, ()> {
    use std::io::Read;
    match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
        Ok(meta) if !meta.file_type().is_file() => return Err(()),
        Ok(_) => {}
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| ())?;
    if !file.metadata().map_err(|_| ())?.file_type().is_file() {
        return Err(());
    }
    let mut bytes = Vec::new();
    let limit = u64::try_from(PLANE_FILE_MAX).unwrap_or(u64::MAX) + 1;
    file.take(limit).read_to_end(&mut bytes).map_err(|_| ())?;
    if bytes.len() > PLANE_FILE_MAX {
        return Err(());
    }
    String::from_utf8(bytes).map(Some).map_err(|_| ())
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
    /// What a program run later, outside any sandbox, loads code or settings from: git's
    /// config and hooks, shell startup files, and each harness's and editor's project config
    /// ([`PLANTED`], ruling V73b). Never written by a chat, wherever it may write.
    LaterCode,
}

impl Class {
    pub const ALL: [Class; 5] = [
        Class::Vaults,
        Class::Integrity,
        Class::HumanPowers,
        Class::RunnerInternals,
        Class::LaterCode,
    ];

    /// The word a refusal and the audit name the class by.
    pub fn word(self) -> &'static str {
        match self {
            Self::Vaults => "vaults",
            Self::Integrity => "integrity",
            Self::HumanPowers => "human-powers",
            Self::RunnerInternals => "runner-internals",
            Self::LaterCode => "later-code",
        }
    }
}

/// How much of a [`Planted`] name a chat is denied writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The name itself: it is never created, renamed, moved or removed. What is below it is
    /// written as usual. A directory moved into its place would bring its contents with it.
    Itself,
    /// The name and everything below it.
    AndBelow,
}

/// One name, at any depth below a directory a chat may write, that the [`Class::LaterCode`]
/// class denies writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Planted {
    /// The name, as `/`-separated parts, relative to the directory it is found in. A `**` part
    /// stands for any number of directories, as in a submodule's git directory.
    pub path: &'static str,
    pub reach: Reach,
}

impl Planted {
    const fn itself(path: &'static str) -> Self {
        Self {
            path,
            reach: Reach::Itself,
        }
    }

    const fn and_below(path: &'static str) -> Self {
        Self {
            path,
            reach: Reach::AndBelow,
        }
    }
}

/// What a program run later, outside any sandbox, loads code or settings from (ADR 0067 §5,
/// class 5, ruling V73b): git's config and hooks, in every clone and worktree; shell startup
/// files; the project config of every harness and editor; and `charter.toml`, which turns the
/// sandbox on. A chat writing one of these could have code run outside its sandbox the next
/// time a person, an editor or another chat opens the directory.
pub const PLANTED: [Planted; 38] = [
    // A `.git` moved into place brings its own config and hooks.
    Planted::itself(".git"),
    Planted::and_below(".git/config"),
    Planted::and_below(".git/config.worktree"),
    Planted::and_below(".git/hooks"),
    Planted::and_below(".git/worktrees"),
    // Each submodule's git directory, nested at any depth; and the directory that holds them,
    // so it is never moved into place whole.
    Planted::itself(".git/modules"),
    Planted::and_below(".git/modules/**/config"),
    Planted::and_below(".git/modules/**/config.worktree"),
    Planted::and_below(".git/modules/**/hooks"),
    // Hook managers' directories, which a `core.hooksPath` commonly names (ruling V73d).
    Planted::and_below(".husky"),
    Planted::and_below(".githooks"),
    Planted::itself(".claude"),
    Planted::and_below(".claude/settings.json"),
    Planted::and_below(".claude/settings.local.json"),
    Planted::and_below(".claude/commands"),
    Planted::and_below(".claude/agents"),
    // A skill runs its `!` commands when invoked and registers its frontmatter hooks (#1057).
    Planted::and_below(".claude/skills"),
    Planted::and_below(".mcp.json"),
    Planted::and_below("opencode.json"),
    Planted::and_below("opencode.jsonc"),
    Planted::and_below(".opencode"),
    // opencode's interface config, whose `plugin` key loads code (#1057).
    Planted::and_below("tui.json"),
    Planted::and_below("tui.jsonc"),
    Planted::and_below(".codex"),
    // The skills and plugins Codex reads from a project, which can start code outside any
    // sandbox (#1057).
    Planted::and_below(".agents"),
    Planted::and_below(".vscode"),
    Planted::and_below(".idea"),
    Planted::and_below(".envrc"),
    Planted::and_below(".profile"),
    Planted::and_below(".bashrc"),
    Planted::and_below(".bash_profile"),
    Planted::and_below(".bash_login"),
    Planted::and_below(".zshrc"),
    Planted::and_below(".zshenv"),
    Planted::and_below(".zprofile"),
    Planted::and_below(".zlogin"),
    // A chat at the plane root could otherwise take `[sandbox]` out of it.
    Planted::and_below(FILE),
    // What the machine adds to the plane, which charter reads at every later start: the
    // variables passed to chats, plugins and extensions.
    Planted::and_below(crate::profiles::LOCAL_FILE),
];

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

        // 5. Later code: what a protected config points at elsewhere, resolved now (V73d).
        let xdg_config = machine
            .env
            .get("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| machine.home.as_ref().map(|home| home.join(".config")));
        for path in planted::resolved(root, machine.home.as_deref(), xdg_config.as_deref()) {
            deny(Class::LaterCode, path, Access::Write);
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

/// The directories strictly between `root` and `path`, nearest `root` first: each one a chat
/// working in `root` could otherwise move away with `path` inside it, and replace with one of
/// its own, which no rule on `path` itself holds.
pub fn ancestors_within(path: &Path, root: &Path) -> Vec<PathBuf> {
    let Ok(below) = path.strip_prefix(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut at = root.to_path_buf();
    let parts: Vec<_> = below.components().collect();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        at.push(part);
        out.push(at.clone());
    }
    out
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
    /// Where a harness keeps its own files on this machine, for a compiler that has to let the
    /// whole harness write them ([`opencode`]).
    pub homes: Homes,
}

/// The directories a harness keeps its own files under, by the XDG base directory rules: each
/// variable where it is set and absolute, its default under the home directory otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Homes {
    pub home: Option<PathBuf>,
    /// `$XDG_DATA_HOME`, or `~/.local/share`.
    pub data: Option<PathBuf>,
    /// `$XDG_STATE_HOME`, or `~/.local/state`.
    pub state: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    pub config: Option<PathBuf>,
    /// `$XDG_CACHE_HOME`, or `~/.cache`.
    pub cache: Option<PathBuf>,
    /// The operator's own Codex home: `$CODEX_HOME`, or `~/.codex`. A wrapped chat neither reads
    /// nor writes it (D-88q).
    pub codex: Option<PathBuf>,
    /// The Codex home of this project's sandboxed chats, of their own (D-88q): under charter's
    /// data home ([`crate::datahome`]), one per project. Set by [`Compiled::of`], which knows the
    /// project.
    pub codex_project: Option<PathBuf>,
}

impl Homes {
    /// `machine`'s.
    pub fn of(machine: &Machine) -> Self {
        let xdg = |var: &str, default: &str| {
            machine
                .env
                .get(var)
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
                .or_else(|| machine.home.as_ref().map(|home| home.join(default)))
        };
        Self {
            home: machine.home.clone(),
            data: xdg("XDG_DATA_HOME", ".local/share"),
            state: xdg("XDG_STATE_HOME", ".local/state"),
            config: xdg("XDG_CONFIG_HOME", ".config"),
            cache: xdg("XDG_CACHE_HOME", ".cache"),
            codex: xdg("CODEX_HOME", ".codex"),
            codex_project: None,
        }
    }

    /// charter's data home on `machine` ([`crate::datahome`]'s ladder, read from `machine`): the
    /// variable, else `$XDG_DATA_HOME/charter`, else the system's data directory under its home.
    fn charter_data(machine: &Machine) -> Option<PathBuf> {
        let named = |name: &str| {
            machine
                .env
                .get(name)
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
        };
        named(crate::datahome::HOME_VAR)
            .or_else(|| named("XDG_DATA_HOME").map(|xdg| xdg.join("charter")))
            .or_else(|| {
                let home = machine.home.as_ref()?;
                Some(match machine.os {
                    Os::MacOs => home.join("Library/Application Support/charter"),
                    Os::Linux | Os::Windows | Os::Other => home.join(".local/share/charter"),
                })
            })
    }

    /// The Codex home of the project at `root`'s sandboxed chats on `machine` (D-88q): a folder
    /// named for the project as the kernel names it, under charter's data home.
    pub fn codex_project(machine: &Machine, root: &Path) -> Option<PathBuf> {
        use sha2::Digest;
        let real = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let digest = sha2::Sha256::digest(real.as_os_str().as_encoded_bytes());
        let key: String = digest
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Some(Self::charter_data(machine)?.join("codex-homes").join(key))
    }
}

impl Compiled {
    /// Whether `harness`'s compiler can hold every class a service holds here. No compiler can
    /// yet: the credential store is refused for each harness, because no harness's settings
    /// deny it and each one's sandbox was measured reaching it or not measured at all.
    fn holds_every_service(&self, harness: Harness) -> Result<(), Uncompilable> {
        match self.denied.services.first() {
            Some(service) => Err(Uncompilable {
                harness,
                unheld: Unheld::Service(*service),
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
            homes: Homes {
                codex_project: Homes::codex_project(machine, root),
                ..Homes::of(machine)
            },
        }
    }
}

/// What a harness's compiler cannot hold on this machine, so the chat does not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uncompilable {
    pub harness: Harness,
    pub unheld: Unheld,
}

/// What a compiler could not hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unheld {
    /// A class held by a service the harness's sandbox cannot deny.
    Service(Service),
    /// The harness has no sandbox of its own, and charter cannot wrap it on this system yet,
    /// so no class can be held.
    Wrap(Os),
}

impl Uncompilable {
    /// The one class that could not be held, where it was one class.
    pub fn class(self) -> Option<Class> {
        match self.unheld {
            Unheld::Service(service) => Some(service.class()),
            Unheld::Wrap(_) => None,
        }
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
    /// The plane it was compiled for, which the chat's folder must be inside.
    root: PathBuf,
}

impl Applied {
    /// The harness it was compiled for.
    pub fn harness(&self) -> Harness {
        self.harness
    }

    /// The plane it was compiled for.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folders this sandbox lets a chat write besides its own and the temp directories:
    /// opencode's own data, or what a Codex turn writes in Codex's home, under charter's wrap.
    pub fn writable(&self) -> Vec<PathBuf> {
        match &self.form {
            Form::Opencode(wrap) => wrap.data.iter().cloned().collect(),
            Form::Codex(wrap) => wrap.writable(),
            Form::ClaudeCode(_) => Vec::new(),
        }
    }

    /// What that harness is handed.
    pub fn form(&self) -> &Form {
        &self.form
    }

    /// The chat's whole line under this sandbox, from `words` — the program, the profile's
    /// `command`, then `armed`, the arguments that arm the harness, then `charters`, charter's
    /// own words — where it opens, `at`; or the one sentence saying why it may not start.
    ///
    /// **Fail closed** (ADR 0067). A flag of the harness's own, in the chat's own words, can
    /// outrank the sandbox it is handed, so such a chat is refused, naming where the flag is.
    /// A harness that carries its sandbox as flags has them last among the flags, where they
    /// win, and in front of the subcommand, session id and first message that end the line.
    /// A harness charter wraps runs as the wrap's program, with the whole of its own line
    /// after it, and is refused without the [`Confinement`] [`Self::confine`] started.
    pub fn line(&self, words: Words, at: &At<'_>) -> Result<Line, String> {
        // Ruling of 2026-10-03, every harness: a folder reached through a link is not the
        // folder the rules name, so no sandboxed chat starts in one.
        let Some(cwd) = at.cwd else {
            return Err(FOLDER_MISSING.to_owned());
        };
        if let Some(why) = folder_refusal(&self.root, cwd) {
            return Err(why.to_owned());
        }
        self.harness.adapter().sandboxed_line(&self.form, words, at)
    }

    /// What has to run for as long as a chat under this sandbox does, started now: charter's
    /// egress proxy and the chat's own temp directory, for a harness charter wraps ([`Form::
    /// Opencode`], [`Form::Codex`]); `None` for a harness whose own sandbox holds the policy.
    pub fn confine(&self) -> std::io::Result<Option<Confinement>> {
        match &self.form {
            Form::Opencode(wrap) => Confinement::start(wrap.hosts.clone()).map(Some),
            Form::Codex(wrap) => Confinement::start(wrap.hosts.clone()).map(Some),
            Form::ClaudeCode(_) => Ok(None),
        }
    }
}

/// A chat's words, before the sandbox decides its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    /// The program the chat runs: the harness, or the profile's wrapper of it.
    pub program: String,
    /// The rest of the profile's own command.
    pub command: Vec<String>,
    /// What arms the harness for this chat.
    pub armed: Vec<String>,
    /// Charter's own words, which end the line.
    pub charters: Vec<String>,
}

/// What only the place a session opens knows about it.
#[derive(Debug, Clone, Copy, Default)]
pub struct At<'a> {
    /// The directory the chat runs in.
    pub cwd: Option<&'a Path>,
    /// The socket its hooks report on, where the app listens on one.
    pub hook_socket: Option<&'a Path>,
    /// What [`Applied::confine`] started for it.
    pub confinement: Option<&'a Confinement>,
}

/// A chat's line under its sandbox: the program that runs, its arguments, and what its
/// environment gains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// What runs beside a chat charter wraps, for as long as it lives: the loopback proxy its
/// traffic leaves through ([`egress`]) and a temp directory of its own. Both end when this is
/// dropped.
#[derive(Debug)]
pub struct Confinement {
    proxy: egress::Proxy,
    tmp: tempfile::TempDir,
}

impl Confinement {
    /// A proxy carrying `hosts`, and a new temp directory.
    pub fn start(hosts: Vec<String>) -> std::io::Result<Self> {
        Ok(Self {
            proxy: egress::Proxy::start(hosts)?,
            tmp: tempfile::Builder::new().prefix("charter-chat-").tempdir()?,
        })
    }

    /// The loopback port of its proxy.
    pub fn proxy_port(&self) -> u16 {
        self.proxy.port()
    }

    /// The URL its proxy is named by in the chat's environment.
    pub fn proxy_url(&self) -> String {
        self.proxy.url()
    }

    /// The chat's own temp directory.
    pub fn tmp(&self) -> &Path {
        self.tmp.path()
    }
}

/// Each harness's own form of the policy, one variant per compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form {
    ClaudeCode(claude::Settings),
    Codex(codex::Wrap),
    Opencode(opencode::Wrap),
}

/// A harness's compiler.
pub type Compiler = fn(&Compiled) -> Result<Form, Uncompilable>;

/// The compiler for `harness`, or none where charter has not written one yet: its adapter's
/// ([`crate::harness::HarnessAdapter::sandbox_compiler`]). A harness gains a sandbox by its
/// adapter gaining a compiler and a [`Form`] variant.
pub fn compiler(harness: Harness) -> Option<Compiler> {
    harness.adapter().sandbox_compiler()
}

/// Why no chat on `harness` starts sandboxed on `os`, whatever the project: charter has no
/// compiler for it, holds its sandbox back, or cannot wrap it on this system. `None` where one
/// can, as far as the harness and the system decide; a project can still refuse it for its own
/// reasons, such as a keyring vault. What a harness's card says about the sandbox.
pub fn never_on(harness: Harness, os: Os) -> Option<String> {
    let Some(compile) = compiler(harness) else {
        return Some("charter has no sandbox compiler for it yet".to_owned());
    };
    if let Some(issue) = harness.adapter().sandbox_held_back() {
        return Some(format!(
            "charter cannot keep its chats inside the sandbox yet (#{issue})"
        ));
    }
    let nothing = Compiled {
        denied: Denied::default(),
        hosts: Vec::new(),
        os,
        homes: Homes::default(),
    };
    match compile(&nothing) {
        Err(Uncompilable {
            unheld: Unheld::Wrap(_),
            ..
        }) => Some("charter can wrap it on macOS only, so far".to_owned()),
        _ => None,
    }
}

/// Why a sandboxed chat with no folder of its own is not started.
pub const FOLDER_MISSING: &str = "this plane runs every chat sandboxed, and the chat has no folder \
                                  of its own to be confined to, so nothing was started.";

/// Why a sandboxed chat in a linked folder is not started.
pub const FOLDER_LINKED: &str = "this plane runs every chat sandboxed, and the chat's folder, or a \
                                 folder between it and the plane, is a link or not a real folder, \
                                 so nothing was started.";

/// Why a sandboxed chat outside its plane is not started.
pub const FOLDER_OUTSIDE: &str = "this plane runs every chat sandboxed, and the chat's folder is \
                                  not inside the plane, so nothing was started.";

/// Why a chat in the folder `cwd` of the plane at `root` may not start sandboxed, or `None`
/// (ruling of 2026-10-03): every folder from the plane's own, as the kernel names it, down to
/// the chat's must be a real directory and not a link, so a folder another chat swapped for a
/// link never takes the rules of the folder it replaced to somewhere else.
pub fn folder_refusal(root: &Path, cwd: &Path) -> Option<&'static str> {
    let Ok(real_root) = root.canonicalize() else {
        return Some(FOLDER_LINKED);
    };
    // Where the folder meets the plane: the outermost of its ancestors that is the plane as the
    // kernel names it, so a link above the plane (`/tmp`, a linked parent) is allowed on either
    // side, and one inside it is walked below and refused.
    let Some(meets) = cwd
        .ancestors()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find(|dir| dir.canonicalize().is_ok_and(|real| real == real_root))
    else {
        return Some(FOLDER_OUTSIDE);
    };
    let Ok(below) = cwd.strip_prefix(meets) else {
        return Some(FOLDER_OUTSIDE);
    };
    let below = below.to_path_buf();
    let mut at = real_root.clone();
    let check = |at: &Path| match std::fs::symlink_metadata(at) {
        Ok(meta) => meta.file_type().is_dir() && !meta.file_type().is_symlink(),
        Err(_) => false,
    };
    if !check(&at) {
        return Some(FOLDER_LINKED);
    }
    for part in below.components() {
        match part {
            std::path::Component::Normal(name) => at.push(name),
            std::path::Component::CurDir => continue,
            _ => return Some(FOLDER_LINKED),
        }
        if !check(&at) {
            return Some(FOLDER_LINKED);
        }
    }
    // The walk and the kernel agree on where the folder is.
    match cwd.canonicalize() {
        Ok(real) if real == at => None,
        _ => Some(FOLDER_LINKED),
    }
}

/// Whether a `charter.toml` above `start` is there and is not a regular file — a link, a
/// FIFO, a socket, a device or a directory — which the plane's own walk does not take for a
/// plane at all. A sandboxed start refuses it ([`NotStarted::PlaneUnreadable`]) rather than
/// reading "no plane" and starting the chat unsandboxed.
pub fn marker_unreadable(start: &Path) -> bool {
    let here = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    here.ancestors().any(|dir| {
        std::fs::symlink_metadata(dir.join(FILE)).is_ok_and(|meta| !meta.file_type().is_file())
    })
}

/// The indefinite article a sentence puts before `word`.
fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    }
}

/// The harnesses charter can sandbox on this machine other than `but`, as a sentence names
/// them: one a chat could be started on instead. A harness held back or one this system cannot
/// wrap is never offered.
fn sandboxed_harnesses_but(but: Option<Harness>) -> String {
    let titles: Vec<&str> = Harness::ALL
        .into_iter()
        .filter(|harness| never_on(*harness, Os::this()).is_none() && Some(*harness) != but)
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
    /// The profile's program, as written or as the kernel names it, lives where the chat can
    /// write, so a sandboxed chat could have changed it (ruling V87g).
    ProgramWritable(PathBuf),
    /// The profile's program is a relative path, which would be found against a folder the
    /// chat can write.
    ProgramRelative,
    /// A word of the profile's command, or the value of a `--flag=value` word, names a file
    /// where the chat can write: a program outside handed it would run what a chat wrote.
    WordWritable(String),
    /// A word of the profile's command is longer than [`program::WORD_MAX`], which the check
    /// does not read (D-88k).
    WordTooLong,
    /// The profile's program does not answer as the harness its sandbox was compiled for, so
    /// the sandbox may bind nothing (ruling V87g).
    NotTheHarness(Harness),
    /// The profile's program did not answer `--version` in time ([`program::PATIENCE`]): a
    /// first run of a program the system has not seen can be slow, so the person is asked to
    /// start the chat again rather than told it is not the harness.
    ProbeTimedOut(Harness),
    /// Charter has a compiler for the harness, and holds it back: its own sandbox cannot be
    /// kept to what the compiler says while it runs, until the issue named lands.
    HeldBack(Harness, u32),
    /// The plane's `charter.toml` cannot be read, so whether it turns the sandbox on is
    /// unknown, and the chat is not started rather than started unsandboxed.
    PlaneUnreadable,
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
                sandboxed_harnesses_but(Some(*harness))
            ),
            Self::ProgramWritable(path) => write!(
                f,
                "{lead}, and the program lives where this chat can write: {}, so it was not \
                 started sandboxed. Keep the program outside the plane and outside what a chat \
                 may write.",
                path.display()
            ),
            Self::ProgramRelative => write!(
                f,
                "{lead}, and this profile's program is a relative path, which would be found in \
                 a folder the chat can write, so it was not started sandboxed. Name the program \
                 by its full path."
            ),
            Self::WordTooLong => write!(
                f,
                "{lead}, and a word of this profile's command is longer than 4 KiB, which charter \
                 does not check, so it was not started sandboxed. Keep what it says in a file \
                 outside the plane and name that file instead."
            ),
            Self::WordWritable(word) => write!(
                f,
                "{lead}, and this profile's command names {word}, which lies where this chat \
                 can write, so it was not started sandboxed. Keep every file the command names \
                 outside the plane and outside what a chat may write."
            ),
            Self::ProbeTimedOut(harness) => write!(
                f,
                "{lead}, and this profile's program did not answer whether it is {} within {} \
                 seconds, so it was not started. A first run of a program can be slow: start \
                 the chat again.",
                harness.title(),
                program::PATIENCE.as_secs()
            ),
            Self::NotTheHarness(harness) => write!(
                f,
                "{lead}, and this profile's program does not answer as {}, whose sandbox it was \
                 given, so it was not started sandboxed.",
                harness.title()
            ),
            Self::HeldBack(harness, issue) => write!(
                f,
                "charter cannot keep {} {} chat inside its sandbox yet (#{issue}), so in this \
                 project a new one starts only without the sandbox, from the new-chat picker.",
                article(harness.title()),
                harness.title()
            ),
            Self::PlaneUnreadable => write!(
                f,
                "{FILE} in this plane cannot be read as TOML, so charter cannot tell whether it \
                 runs chats sandboxed, and nothing was started. Fix {FILE} and start the chat \
                 again."
            ),
            Self::NoBackend(missing) => write!(
                f,
                "{lead}, and this machine cannot apply the sandbox: {missing}. Nothing was \
                 started."
            ),
            Self::Uncompilable(it) => match it.unheld {
                // The Linux wrap is #1040.
                Unheld::Wrap(os) => write!(
                    f,
                    "{lead}, and charter runs {} inside a sandbox of its own, which it can apply \
                     on macOS but not yet on {}, so it was not started. Start this chat on a {} \
                     profile.",
                    it.harness.title(),
                    match os {
                        Os::Linux => "Linux (#1040)",
                        Os::Windows => "Windows",
                        Os::MacOs | Os::Other => "this system",
                    },
                    sandboxed_harnesses_but(Some(it.harness))
                ),
                Unheld::Service(Service::CredentialStore) => write!(
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
    // One spelling of the plane for every harness: the kernel's.
    let real_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let root = real_root.as_path();
    let plane = Plane::read(root);
    // Fail closed: a file that may say `[sandbox]` and cannot be read never reads as "not set".
    if plane.unreadable() {
        return Err(NotStarted::PlaneUnreadable);
    }
    let Some(policy) = plane.said().policy else {
        return Ok(None);
    };
    let Some(compile) = compiler(harness) else {
        return Err(NotStarted::NoCompiler(harness));
    };
    // Ruling V87f: the one place a harness with a compiler is held back.
    if let Some(issue) = harness.adapter().sandbox_held_back() {
        return Err(NotStarted::HeldBack(harness, issue));
    }
    if let Some(missing) = backend::missing(machine.os, has) {
        return Err(NotStarted::NoBackend(missing));
    }
    let form =
        compile(&Compiled::of(&policy, &plane, root, machine)).map_err(NotStarted::Uncompilable)?;
    Ok(Some(Applied {
        harness,
        form,
        root: root.to_path_buf(),
    }))
}

/// [`for_start`] without asking this machine for a backend, for a test of a compiler alone.
#[cfg(test)]
pub(crate) fn compiled_anyway(
    harness: Harness,
    root: &Path,
    machine: &Machine,
) -> Result<Applied, NotStarted> {
    let plane = Plane::read(root);
    let policy = plane
        .said()
        .policy
        .expect("a plane that turned the sandbox on");
    let compile = compiler(harness).ok_or(NotStarted::NoCompiler(harness))?;
    let form =
        compile(&Compiled::of(&policy, &plane, root, machine)).map_err(NotStarted::Uncompilable)?;
    Ok(Applied {
        harness,
        form,
        root: root.to_path_buf(),
    })
}

/// A person's choice, in the window, to start one chat without the sandbox (ADR 0067 §7,
/// ruling V78 a): the new-chat picker's "Start without the sandbox".
///
/// **Only the window makes one** — the app's start command, on the human `local-ui` scope.
/// No CLI word, no file, no profile and no chat can: [`crate::start::Start::without_sandbox`]
/// is `None` everywhere else, and nothing is recorded that a later start reads as one, so it is
/// never inherited by a new chat, a resumed chat, a relaunch, a workspace or the project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptOut {
    /// What the person typed as the reason, if anything: an operator note for the audit.
    pub reason: Option<String>,
}

impl OptOut {
    /// The longest reason kept, in characters.
    pub const MOST_REASON_CHARS: usize = 200;

    /// The reason as the audit keeps it: one line, its whitespace runs made one space, clipped
    /// to [`Self::MOST_REASON_CHARS`] with its ellipsis, and none when it is blank.
    fn kept_reason(&self) -> Option<String> {
        let words: Vec<&str> = self.reason.as_deref()?.split_whitespace().collect();
        (!words.is_empty())
            .then(|| crate::shown::one_line(&words.join(" "), Self::MOST_REASON_CHARS - 1))
    }
}

/// Who started a chat in a sandboxed project without the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// A person, in the window, for this chat ([`OptOut`]).
    Person,
    /// Charter itself, because this operating system has no sandbox backend yet (Windows,
    /// M46, #565; ruling V21 3). The audit names charter as the actor, never the operator (ruling V78 b).
    NoBackend(Os),
}

/// A chat in a sandboxed project that starts without the sandbox: who chose it and why — what
/// its `trust.sandbox.off` event records ([`crate::eventlog::Recorder::trust`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lifted {
    pub by: By,
    /// The person's reason, one line, where one was typed.
    pub reason: Option<String>,
}

impl Lifted {
    /// What is lifted: every class (ADR 0067 §5). An unsandboxed chat holds none of them.
    pub const CLASSES: [Class; 5] = Class::ALL;

    /// What the chat's tab says as it starts, so the opt-out is visible for its whole life.
    pub fn notice(&self) -> String {
        match self.by {
            By::Person => "This chat runs without the sandbox: you turned it off for this chat \
                           only. A new or resumed chat does not inherit it."
                .to_owned(),
            // The Windows backend is M46, #565.
            By::NoBackend(os) => format!(
                "This chat runs without the sandbox: charter has no sandbox backend on {} yet, \
                 so every chat here starts without it until one exists.",
                match os {
                    Os::Windows => "Windows",
                    Os::MacOs | Os::Linux | Os::Other => "this system",
                }
            ),
        }
    }
}

/// A change to one chat's sandbox, as its trust event records it (ADR 0067 §7; ADR 0075 §4's
/// `trust.sandbox.off` and `trust.sandbox.on`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The chat starts without the sandbox.
    Off(Lifted),
    /// A chat whose last run was unsandboxed starts this run sandboxed: an opt-out lasts one
    /// run and is never inherited, so it is charter that puts it back on.
    On,
}

impl Change {
    /// The event's kind.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Off(_) => "trust.sandbox.off",
            Self::On => "trust.sandbox.on",
        }
    }

    /// The event's body: who, the chat's harness and persona, and the classes lifted or put
    /// back. Which chat, which run, when and on which machine are the envelope's (ADR 0066).
    ///
    /// **The person is named by their scope, never by a login** (ADR 0075 §3): the actor is
    /// `operator` on `local-ui`, and the audit's pseudonym replaces it when AU-18 lands.
    pub fn body(&self, harness: Option<Harness>, persona: Option<&str>) -> serde_json::Value {
        let classes: Vec<&str> = Lifted::CLASSES.iter().map(|class| class.word()).collect();
        let harness = harness.map(Harness::name);
        match self {
            Self::Off(Lifted {
                by: By::Person,
                reason,
            }) => serde_json::json!({
                "actor_kind": "human",
                "actor": "operator",
                "scope": "local-ui",
                "harness": harness,
                "persona": persona,
                "reason": reason,
                "lifted": classes,
            }),
            Self::Off(Lifted {
                by: By::NoBackend(os),
                reason,
            }) => serde_json::json!({
                "actor_kind": "host",
                "actor": "charter (no backend on this OS)",
                "os": match os {
                    Os::Windows => "windows",
                    Os::MacOs => "macos",
                    Os::Linux => "linux",
                    Os::Other => "other",
                },
                "harness": harness,
                "persona": persona,
                "reason": reason,
                "lifted": classes,
            }),
            Self::On => serde_json::json!({
                "actor_kind": "host",
                "actor": "charter",
                "harness": harness,
                "persona": persona,
                "restored": classes,
            }),
        }
    }
}

/// What a chat in a sandboxed project starts under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// The sandbox, compiled for its harness.
    Sandboxed(Applied),
    /// No sandbox, and why ([`Lifted`]): audited as `trust.sandbox.off`.
    Unsandboxed(Lifted),
}

/// What a chat of `harness` in the project at `root` starts under on `machine`, where `opt_out`
/// is a person's choice to start it without the sandbox: `None` where the project has not
/// turned the sandbox on (there is nothing to lift, so nothing to audit), else sandboxed or
/// unsandboxed — or why the chat does not start.
///
/// **Fail closed, with two exits that are each audited** (ADR 0067 §1 and §7):
/// - a person's opt-out, which starts the chat without the sandbox even where it could not be
///   applied — the opt-out inside the refusal;
/// - Windows, where no backend exists yet and every chat starts at the opt-out with charter as
///   the actor (ruling V21 3). Any other system with no backend still refuses.
pub fn decide(
    harness: Harness,
    root: &Path,
    machine: &Machine,
    has: &dyn Fn(&str) -> bool,
    opt_out: Option<&OptOut>,
) -> Result<Option<Decided>, NotStarted> {
    // A file that cannot be read may say `[sandbox]`, so it never reads as "not set": it falls
    // through to the refusal below, which the opt-out sits inside.
    let plane = Plane::read(root);
    if !plane.unreadable() && plane.said().policy.is_none() {
        return Ok(None);
    }
    if let Some(opt_out) = opt_out {
        return Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: opt_out.kept_reason(),
        })));
    }
    if machine.os == Os::Windows {
        return Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        })));
    }
    for_start(harness, root, machine, has).map(|applied| applied.map(Decided::Sandboxed))
}

/// What the new-chat picker says about the sandbox for a chat of one harness, before anything
/// starts (ruling V78 a): the reason is on screen beside "Start without the sandbox".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ahead {
    /// The project has not turned the sandbox on: the picker says nothing.
    Off,
    /// The chat starts sandboxed, unless the person opts out.
    Sandboxed,
    /// The chat starts without the sandbox whatever is picked (Windows), and why.
    Unsandboxed(Lifted),
    /// The sandbox cannot be applied, so the chat starts only without it: the refusal, and
    /// SD-30's install command where installing something would fix it (ruling V78 c).
    Refused {
        why: String,
        install: Option<String>,
    },
}

/// [`Ahead`] for a chat of `harness` in the project at `root` on `machine`, with `has` saying
/// what is installed and `os_release` the text of `/etc/os-release` (empty off Linux).
///
/// Every refusal the start would give is shown here first, beside "Start without the sandbox":
/// a harness charter holds back (Codex, ruling V87f), and — where the chat would be sandboxed —
/// what `check` says of the program under the sandbox compiled for it (ruling V87g): the start's
/// own [`program::checked`], asked by [`crate::start::sandbox_ahead`] with the start's words,
/// folder and environment. A profile nobody approved is never run, so its caller passes a
/// `check` that asks nothing.
pub fn ahead(
    harness: Harness,
    root: &Path,
    machine: &Machine,
    has: &dyn Fn(&str) -> bool,
    os_release: &str,
    check: &dyn Fn(&Applied) -> Result<(), NotStarted>,
) -> Ahead {
    let refused = |refused: NotStarted| Ahead::Refused {
        why: refused.to_string(),
        install: match &refused {
            NotStarted::NoBackend(missing) => backend::install_command(missing, os_release),
            NotStarted::NoCompiler(_)
            | NotStarted::HeldBack(..)
            | NotStarted::ProgramWritable(_)
            | NotStarted::ProgramRelative
            | NotStarted::WordWritable(_)
            | NotStarted::WordTooLong
            | NotStarted::NotTheHarness(_)
            | NotStarted::ProbeTimedOut(_)
            | NotStarted::Uncompilable(_)
            | NotStarted::PlaneUnreadable => None,
        },
    };
    match decide(harness, root, machine, has, None) {
        Ok(None) => Ahead::Off,
        Ok(Some(Decided::Sandboxed(applied))) => match check(&applied) {
            Ok(()) => Ahead::Sandboxed,
            Err(not) => refused(not),
        },
        Ok(Some(Decided::Unsandboxed(lifted))) => Ahead::Unsandboxed(lifted),
        Err(not) => refused(not),
    }
}

/// What one start of a chat means for the sandbox's audit and its count: the trust event to
/// write, if any, and how to count the chat, if it is new.
///
/// - `lifted`: it starts without the sandbox in a project that has it on — `trust.sandbox.off`.
/// - `sandboxed`: it starts under the sandbox; where its last run was unsandboxed
///   (`was_unsandboxed`), the sandbox is back on — `trust.sandbox.on`.
/// - `new_chat`: a chat that was not open before, counted once towards the opt-out rate. A
///   relaunch or a start in a chat's place is another run of the same chat, never counted.
pub fn at_start(
    lifted: Option<&Lifted>,
    sandboxed: bool,
    was_unsandboxed: bool,
    new_chat: bool,
) -> (Option<Change>, Option<local::Started>) {
    let counted = |started| new_chat.then_some(started);
    match lifted {
        Some(lifted) => (
            Some(Change::Off(lifted.clone())),
            counted(match lifted.by {
                By::Person => local::Started::OptedOut,
                By::NoBackend(_) => local::Started::NoBackend,
            }),
        ),
        None if sandboxed => (
            was_unsandboxed.then_some(Change::On),
            counted(local::Started::Sandboxed),
        ),
        None => (None, None),
    }
}

pub mod backend;
pub mod claude;
pub mod codex;
pub mod egress;
pub mod local;
pub mod opencode;
pub mod planted;
pub mod program;
pub mod seatbelt;

#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod egress_tests;
#[cfg(test)]
mod opencode_tests;
#[cfg(test)]
mod tests;
