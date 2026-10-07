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
/// Named as messages name it; the file read is [`crate::names::manifest`].
pub const FILE: &str = crate::names::PLANE_MANIFEST.reads[0];

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

    /// The name the window shows it by (`CONTEXT.md`, **Internet access**, #1340).
    pub fn title(self) -> &'static str {
        match self {
            Self::ModelProviders => "AI providers",
            Self::Forge => "Code hosting",
            Self::Toolchains => "Package registries",
        }
    }

    /// The hosts it lists itself, before a project's own forges ([`hosts`]): the one place a
    /// preset's hosts are written.
    pub fn own_hosts(self) -> &'static [&'static str] {
        match self {
            Self::ModelProviders => &[
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
            Self::Forge => &[
                "github.com",
                "api.github.com",
                "codeload.github.com",
                "*.githubusercontent.com",
                "ghcr.io",
                "gitlab.com",
                "registry.gitlab.com",
            ],
            Self::Toolchains => &[
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
/// Only Internet access is the plane's to choose: the presets, and the project's own hosts
/// ([`hosts::Host`], #1341). What a chat may write (its own directory, and the harness's own temp
/// directory) and what it is always denied ([`Class`]) are not in the file at all, so no file
/// can widen them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub egress: Vec<Preset>,
    /// The project's own hosts: `[sandbox] hosts` in the committed file.
    pub hosts: Vec<hosts::Host>,
    /// `certificate-checks`: whether a chat may ask the system's certificate check, which Go
    /// programs such as `gh` need to verify a host (D-1337-7). Off unless the file says `true`.
    pub certificate_checks: bool,
    /// Each persona's own grants: `[sandbox.personas.<name>]` in the committed file (#1362).
    pub personas: std::collections::BTreeMap<String, persona::Grants>,
}

impl Policy {
    /// The sandbox turned on and nothing else said: the default presets ([`Preset::DEFAULT`]),
    /// no host of the project's own, no certificate checks, no persona's grants.
    pub fn defaults() -> Self {
        Self {
            egress: Preset::DEFAULT.to_vec(),
            hosts: Vec::new(),
            certificate_checks: false,
            personas: std::collections::BTreeMap::new(),
        }
    }
}

/// The key that lets a chat ask the system's certificate check.
pub const CERTIFICATE_CHECKS: &str = "certificate-checks";

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
    /// There is no file at all, so what the project says about the sandbox is unknown too
    /// ([`NotStarted::PlaneMissing`]): a project's own manifest that has gone never reads as
    /// "the sandbox is off" (D-1410e).
    missing: bool,
}

impl Plane {
    /// The plane at `root`. No file says nothing. A file that cannot be read, or is not TOML,
    /// is [`Self::unreadable`]: it may say `[sandbox]`, so it never reads as saying nothing.
    pub fn read(root: &Path) -> Self {
        match read_plane_file(&crate::names::manifest(root)) {
            Ok(text) => Self::of(text.as_deref()),
            Err(()) => Self {
                top: None,
                unreadable: true,
                missing: false,
            },
        }
    }

    /// `text`, the whole of a `charter.toml`, or `None` where there is no file.
    pub fn of(text: Option<&str>) -> Self {
        let top = text.map(|text| text.parse::<toml::Table>().ok());
        Self {
            unreadable: matches!(top, Some(None)),
            missing: top.is_none(),
            top: top.flatten(),
        }
    }

    /// Whether the file is there and charter cannot read it as TOML.
    pub fn unreadable(&self) -> bool {
        self.unreadable
    }

    /// Whether there is no file at all.
    pub fn missing(&self) -> bool {
        self.missing
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
            .map(|host| host.split(':').next().unwrap_or(host))
            // A host as a project's own hosts are taken (#1341): never this machine, a
            // link-local or metadata address, or a name ending in a number.
            .filter_map(|host| hosts::Host::parse(host).ok())
            .map(|host| host.to_string())
            .collect()
    }

    /// **The sandbox policy in force for chats here under `locks`**: the project's own where it
    /// turned the sandbox on, and none where it has not — unless an administrator's policy
    /// requires the sandbox ([`policy::Locks::forbids_opt_out`], D-1423-1). Then a project
    /// with no `[sandbox]`, or one that has not turned it on, runs every chat sandboxed as if
    /// it had: what its `[sandbox]` says with the mode on, so the default presets where it
    /// names none ([`Policy::defaults`]). `locks` hold that policy as they hold any other when
    /// it is compiled ([`Compiled::granted`]): the presets it fixes, the hosts it allows.
    ///
    /// The one place "do chats here run sandboxed" is answered, for a start, Settings and every
    /// Notice.
    pub fn in_force(&self, locks: &policy::Locks) -> Option<Policy> {
        self.said().in_force(locks)
    }

    /// The hosts that reach chats here because this file says so, under `locks`: its own
    /// `[sandbox] hosts`, then, while the `forge` preset is on, its `[[forge]]` hosts, then each
    /// persona's own as `<host> for <persona> chats` (#1362), so the Notice says which chats a
    /// host reaches — what the one-time Notice of a change names (#1341).
    ///
    /// **Only what a chat reaches** (#1423): a host an administrator's policy locks out, a
    /// forge's while policy turns the `forge` preset off, and a persona's where policy forbids
    /// a persona's own, is not named, as [`Compiled::granted`] grants none of them.
    pub fn granted_hosts(&self, locks: &policy::Locks) -> Vec<String> {
        let Some(policy) = self.in_force(locks) else {
            return Vec::new();
        };
        let reaches = |host: &hosts::Host, level: hosts::Level| {
            locks
                .refuses(&hosts::Granted {
                    host: host.clone(),
                    level,
                })
                .is_none()
        };
        let mut out: Vec<String> = policy
            .hosts
            .iter()
            .filter(|host| reaches(host, hosts::Level::Project))
            .map(ToString::to_string)
            .collect();
        if locks.presets(&policy.egress).contains(&Preset::Forge) {
            for host in self.forge_hosts() {
                let reached = hosts::Host::parse(&host)
                    .is_ok_and(|host| reaches(&host, hosts::Level::Project));
                if reached && !out.contains(&host) {
                    out.push(host);
                }
            }
        }
        for (persona, grants) in &policy.personas {
            out.extend(
                grants
                    .hosts
                    .iter()
                    .filter(|host| reaches(host, hosts::Level::Persona))
                    .map(|host| format!("{host} for {persona} chats")),
            );
        }
        out
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
    /// `hosts` that is not a list.
    HostsNotAList,
    /// An entry of `hosts` that is not a host ([`hosts::Host::parse`]): as written, and why.
    Host(String, String),
    /// `certificate-checks` that is not `true` or `false`.
    CertificateChecksNotABool,
    /// Something in `personas` that grants nothing ([`persona::read`]), as one sentence.
    Persona(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let on = "so the sandbox is on";
        match self {
            Self::NotATable => write!(
                f,
                "{TABLE} in {FILE} is not a table — [{TABLE}] holds mode, egress, hosts and \
                 {CERTIFICATE_CHECKS}; {on}"
            ),
            Self::UnknownKey(key) => write!(
                f,
                "{TABLE}.{key} in {FILE} is not a key purlis reads — [{TABLE}] holds mode, \
                 egress, hosts and {CERTIFICATE_CHECKS}"
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
            Self::HostsNotAList => write!(
                f,
                "{TABLE}.hosts in {FILE} is not a list of hosts, so no host of the project's is \
                 allowed — write hosts = [\"api.example.com\", \"10.0.0.5:6443\"]"
            ),
            Self::Host(written, why) => write!(
                f,
                "{TABLE}.hosts in {FILE} names {written}, which no chat is let reach: {why}"
            ),
            Self::CertificateChecksNotABool => write!(
                f,
                "{TABLE}.{CERTIFICATE_CHECKS} in {FILE} is not true or false; certificate checks \
                 stay off"
            ),
            Self::Persona(said) => f.write_str(said),
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
    /// What its `[sandbox]` says where it has **not** turned the sandbox on (a table with no
    /// `mode`): what its chats run under where an administrator's policy requires the sandbox
    /// ([`Plane::in_force`], D-1423-1). `None` where the sandbox is on, and where there is no
    /// `[sandbox]` at all.
    pub unset: Option<Policy>,
}

impl Said {
    /// The sandbox policy in force under `locks` ([`Plane::in_force`]): its own where it turned
    /// the sandbox on, or what it says with the mode on where policy requires the sandbox.
    pub fn in_force(self, locks: &policy::Locks) -> Option<Policy> {
        self.policy.or_else(|| {
            locks
                .forbids_opt_out()
                .then(|| self.unset.unwrap_or_else(Policy::defaults))
        })
    }

    /// What `top`, a parsed `charter.toml`, says.
    pub fn of(top: Option<&toml::Table>) -> Self {
        let Some(table) = top.and_then(|top| top.get(TABLE)) else {
            return Self::default();
        };
        let Some(table) = table.as_table() else {
            return Self {
                policy: Some(Policy::defaults()),
                refused: vec![Refusal::NotATable],
                unset: None,
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
            if key != "mode"
                && key != "egress"
                && key != hosts::KEY
                && key != CERTIFICATE_CHECKS
                && key != persona::KEY
            {
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
        let hosts = match hosts::read(table.get(hosts::KEY)) {
            Ok(hosts::Listed {
                hosts,
                refused: not,
            }) => {
                refused.extend(
                    not.into_iter()
                        .map(|(written, why)| Refusal::Host(written, why)),
                );
                hosts
            }
            Err(hosts::NotAList) => {
                refused.push(Refusal::HostsNotAList);
                Vec::new()
            }
        };
        let certificate_checks = match table.get(CERTIFICATE_CHECKS) {
            None => false,
            Some(toml::Value::Boolean(on)) => *on,
            Some(_) => {
                refused.push(Refusal::CertificateChecksNotABool);
                false
            }
        };
        let (personas, not) = persona::read(table.get(persona::KEY), FILE);
        refused.extend(not.into_iter().map(Refusal::Persona));
        let written = Policy {
            egress,
            hosts,
            certificate_checks,
            personas,
        };
        let (policy, unset) = if on {
            (Some(written), None)
        } else {
            (None, Some(written))
        };
        Self {
            policy,
            refused,
            unset,
        }
    }
}

/// Everything in `text`'s `[sandbox]` that charter would not honour as written, as `file`
/// holds it — for the Settings tab's save, which refuses to write it. The committed file's
/// whole table, and this machine's `charter.local.toml`, which holds the person's own hosts and
/// nothing else of the sandbox ([`hosts`], #1341).
pub fn refusals(text: &str, file: &str) -> Vec<String> {
    if file == crate::profiles::LOCAL_FILE {
        return local_refusals(text);
    }
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
    /// ([`PLANTED`], ruling V73b), never written by a chat wherever it may write; and the
    /// project's manifests where they could change a chat's sandbox ([`manifests_held`]).
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
/// files; and the project config of every harness and editor. A chat writing one of these could
/// have code run outside its sandbox the next time a person, an editor or another chat opens
/// the directory. The project's manifests are held only where they could change a chat's
/// sandbox ([`MANIFESTS`]).
pub const PLANTED: [Planted; 42] = [
    // A `.git` moved into place brings its own config and hooks.
    Planted::itself(".git"),
    Planted::and_below(".git/config"),
    Planted::and_below(".git/config.worktree"),
    Planted::and_below(".git/hooks"),
    Planted::and_below(".git/worktrees"),
    // What moves git's config, objects or attributes somewhere else, so a chat cannot point a
    // repository the app runs git in at a config of its own (#1335, D-1335-7). A linked
    // worktree's pointer files live under `.git/worktrees`, above, and its `.git` file is
    // `.git` itself.
    Planted::and_below(".git/commondir"),
    Planted::and_below(".git/objects/info/alternates"),
    Planted::and_below(".git/info/attributes"),
    // Each submodule's git directory, nested at any depth; and the directory that holds them,
    // so it is never moved into place whole.
    Planted::itself(".git/modules"),
    Planted::and_below(".git/modules/**/config"),
    Planted::and_below(".git/modules/**/config.worktree"),
    Planted::and_below(".git/modules/**/hooks"),
    Planted::and_below(".git/modules/**/commondir"),
    Planted::and_below(".git/modules/**/objects/info/alternates"),
    Planted::and_below(".git/modules/**/info/attributes"),
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
];

/// The project's manifests, under both names (RN-2a): `purlis.toml`, which turns the sandbox
/// on, and `purlis.local.toml`, what the machine adds to the project and purlis reads at every
/// later start (the variables passed to chats, plugins and extensions).
///
/// Not [`PLANTED`]: a manifest is held only where it could change a chat's sandbox
/// ([`manifests_held`]), and elsewhere, in a clone's fixtures or a temp folder, it is an
/// ordinary file (ADR 0067 §5 as amended on 2026-10-06).
pub const MANIFESTS: [&str; 4] = [
    crate::names::PLANE_MANIFEST.write,
    crate::names::PLANE_MANIFEST.reads[0],
    crate::names::LOCAL_SETTINGS.write,
    crate::names::LOCAL_SETTINGS.reads[0],
];

/// The manifests a chat of the project at `root`, working in `cwd`, is denied writing (ADR 0067
/// §5 as amended on 2026-10-06): every [`MANIFESTS`] name at the project root and in each
/// folder from the chat's own up to the root, both ends included. Those are the files purlis
/// reads when it starts a chat there. With no `cwd`, or one outside the project, only the
/// root's.
///
/// Each as the folder is spelled and as the kernel names the folder, the name itself never
/// resolved: a manifest that is a link (a `purlis.local.toml` kept in a dotfiles repository) is
/// held by its own path, so it is neither removed nor replaced, as well as by its target.
///
/// **A manifest lower down changes no chat's sandbox.** A chat's project is the root its sandbox
/// was compiled for ([`Applied::root`]), never the nearest manifest to its folder, and every
/// reader of the project's settings reads them at that root. A marker the walk finds inside
/// another project's `workspaces/` re-roots to that project (`plane::outermost`), so a manifest
/// a chat writes in a clone's fixtures names no project of its own.
pub fn manifests_held(root: &Path, cwd: Option<&Path>) -> Vec<Denial> {
    let real_root = real(root);
    let mut folders = vec![root.to_path_buf()];
    if let Some(cwd) = cwd {
        let mut chain = Vec::new();
        for folder in cwd.ancestors() {
            chain.push(folder.to_path_buf());
            if real(folder) == real_root {
                folders.extend(chain);
                break;
            }
        }
    }
    let mut out: Vec<Denial> = Vec::new();
    for folder in folders {
        for spelled in [real(&folder), folder] {
            for name in MANIFESTS {
                let denial = Denial {
                    class: Class::LaterCode,
                    path: spelled.join(name),
                    access: Access::Write,
                    named: None,
                };
                if !out.contains(&denial) {
                    out.push(denial);
                }
            }
        }
    }
    out
}

/// Whether this process runs in a chat the app started under a sandbox
/// ([`crate::hookwire::SANDBOXED_ENV`] is `1`): a tool hook of that chat inherits it.
pub fn chat_is_sandboxed() -> bool {
    chat_is_sandboxed_in(&crate::envvar::var)
}

/// [`chat_is_sandboxed`], asking `env`.
pub fn chat_is_sandboxed_in(env: &dyn Fn(&str) -> Option<String>) -> bool {
    env(crate::hookwire::SANDBOXED_ENV).as_deref() == Some("1")
}

/// The harness [`crate::hookwire::HARNESS_ENV`] names: the profile's registry name
/// (`claude-code`, `codex`, `opencode`; [`crate::profiles::KINDS`]), which is what a chat's start
/// puts there, and not the kind word a profile is written with.
fn harness_of_registry(registry: &str) -> Option<Harness> {
    crate::profiles::KINDS
        .iter()
        .find(|kind| kind.registry == registry)
        .and_then(|kind| Harness::of_kind(kind.word))
}

/// What started this process: a command the chat ran, or the harness itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// A command the chat ran, in its shell or tool: what a chat's sandbox confines first.
    ByTheChat,
    /// The harness itself, for one of its hooks or as its MCP server. Inside the chat's sandbox
    /// only where the harness's adapter says its sandbox holds what it starts
    /// ([`crate::harness::adapter::HarnessAdapter::sandbox_holds_what_it_starts`]).
    ByTheHarness,
}

/// Set once, at the start of a hook or the MCP server ([`started_by_the_harness`]).
static STARTED_BY_THE_HARNESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Says that this process is one the harness itself started, a hook or its MCP server: the
/// entry of `purlis hook` and `purlis mcp` calls it before anything is written (#1421).
pub fn started_by_the_harness() {
    STARTED_BY_THE_HARNESS.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// [`Started`], for this process.
pub fn started() -> Started {
    if STARTED_BY_THE_HARNESS.load(std::sync::atomic::Ordering::Relaxed) {
        Started::ByTheHarness
    } else {
        Started::ByTheChat
    }
}

/// Whether a write this process makes is held to its chat's sandbox, so a refusal of it may be
/// told as the sandbox's (#1345, #1421). [`chat_is_sandboxed`] alone says only that the chat
/// has one: a hook inherits that from its chat, and Claude Code's sandbox confines the commands
/// its Bash tool runs and not its hooks, so a hook there that meets `EPERM` met something else.
pub fn writes_are_sandboxed() -> bool {
    writes_are_sandboxed_in(&crate::envvar::var, started())
}

/// [`writes_are_sandboxed`], asking `env`, for a process `started` so. A harness this binary
/// does not know, or none named, is not taken to hold what it starts: a refusal is then told
/// without blaming a sandbox, which is the side that is never untrue.
pub fn writes_are_sandboxed_in(env: &dyn Fn(&str) -> Option<String>, started: Started) -> bool {
    if !chat_is_sandboxed_in(env) {
        return false;
    }
    match started {
        Started::ByTheChat => true,
        Started::ByTheHarness => env(crate::hookwire::HARNESS_ENV)
            .and_then(|registry| harness_of_registry(&registry))
            .is_some_and(|harness| harness.adapter().sandbox_holds_what_it_starts()),
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
    /// The file and the word in it that named the path, for one a config named
    /// ([`Class::LaterCode`]); `None` for one charter names itself.
    pub named: Option<Named>,
}

/// Where a config named a denied path: the file, and the word in it as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub file: PathBuf,
    pub word: String,
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
    /// Whether purlis has a sandbox backend on this system at all. Where it has none (Windows),
    /// a chat in a project that turns the sandbox on starts unsandboxed and says why
    /// ([`By::NoBackend`]), so nothing about a sandbox is checked for it.
    pub fn has_backend(self) -> bool {
        self != Self::Windows
    }

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
    /// A config whose commands purlis stopped reading ([`planted::Unread`]), so what they run
    /// is not known and the chat is refused ([`NotStarted::Unread`]).
    pub unread: Option<Named>,
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
                named: None,
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
        // Under EVERY spelling of the state folder (RN-2a): a chat that could write
        // `.purlis/app` would be writing records charter may read next.
        for state in crate::names::STATE_DIR.spellings() {
            let app = root.join(state).join("app");
            deny(Class::Integrity, app.clone(), Access::Write);
            // Every chat's hook spool and the keys that check it, neither read nor written: the
            // hooks that write a spool run outside the sandbox their tools run in (ADR 0068 §6
            // as amended by V63), so nothing inside it needs them.
            deny(
                Class::Integrity,
                crate::hookwire::spool::dir_for(&app.join("hooks.sock")),
                Access::ReadWrite,
            );
            // Every dispatch's record, neither read nor written (#1452, D-1452-11): a brief or a
            // report written for one persona is not for a chat running as another. A chat gets
            // its own dispatch's report on the delivery path and its own list from the app's
            // answer, never from the file.
            deny(
                Class::Integrity,
                app.join(crate::dispatchrecord::DIR_NAME),
                Access::ReadWrite,
            );
        }
        // …and a state folder the project does not have as a folder is not the chat's to make
        // (D-RN2a-7): which folder holds charter's state is decided by which are there, so making
        // one is a move of that state. One that is there is left to the rows above.
        for state in crate::names::STATE_DIR.spellings() {
            let folder = root.join(state);
            // Not a directory of its own: absent, a file, or a link (which a chat could point
            // anywhere) — none is the chat's to turn into a state folder.
            if !std::fs::symlink_metadata(&folder).is_ok_and(|meta| meta.is_dir()) {
                deny(Class::Integrity, folder, Access::Write);
            }
        }

        // 3. Human powers: the approvals a person gave on this machine.
        let config_root = crate::machine::rooted(
            machine.env.get(crate::machine::HOME_VAR).map(Into::into),
            machine.env.get("XDG_CONFIG_HOME").map(Into::into),
            machine.home.clone(),
        );
        // Under EVERY spelling of the config home, there or not (RN-5): which one is the home
        // is decided by which is there, so a chat that could make `purlis/` beside `charter/`
        // would be writing the approvals charter reads next.
        if let Some(config_root) = config_root {
            for home in crate::machine::dirs_spelled(&config_root) {
                deny(Class::HumanPowers, home.clone(), Access::Write);
                // The human client scopes' credentials (FD-27, V16a): a chat neither reads nor
                // writes anything there, so it holds no person's credential. This does not stop
                // a connect to `charterd.sock` beside them, which a sandbox judges as network:
                // each compiler allows no unix socket but the hook socket for that. `charterd`
                // also refuses those scopes to a chat's processes. Every spelling of the folder.
                for daemon in crate::names::DAEMON_DIR.spellings() {
                    deny(Class::HumanPowers, home.join(daemon), Access::ReadWrite);
                }
                // What the human's forge sign-in fetched: the native transport's ETag store
                // holds raw answers, private repos' among them, so a chat neither reads nor
                // writes it (ADR 0070 §3 and §4).
                deny(
                    Class::HumanPowers,
                    home.join(crate::forge::etag::DIR),
                    Access::ReadWrite,
                );
            }
        }

        // …and this machine's policy (#1343): what an administrator locks is never a chat's to
        // write, wherever the system lets it be written.
        deny(
            Class::HumanPowers,
            real(&policy::machine_folder()),
            Access::Write,
        );

        // 5. Later code: the project root's manifests, under every name (#1336); those between
        // the root and the chat's folder are added where the folder is known
        // ([`Applied::form_in`]).
        paths.extend(manifests_held(root, None));
        // …and what a protected config points at elsewhere, resolved now (V73d).
        let xdg_config = machine
            .env
            .get("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| machine.home.as_ref().map(|home| home.join(".config")));
        // Where else a chat could write, so a value split off a later letter there is still
        // named (D-T56-1): every folder you list as one chats may be granted, which with the
        // project holds every grant (D-1342-10), and the project's cache home (#1337), whether
        // or not this chat is given it. Counting too many only names more, which fails closed.
        let writable: Vec<PathBuf> = local::grantable_folders(root)
            .into_iter()
            .chain(caches::root_of(machine, root))
            .collect();
        let (resolved, unread) = match planted::resolved(
            root,
            machine.home.as_deref(),
            xdg_config.as_deref(),
            &writable,
        ) {
            Ok(resolved) => (resolved, None),
            Err(unread) => (Vec::new(), Some(unread)),
        };
        for it in resolved {
            paths.push(Denial {
                class: Class::LaterCode,
                path: it.path,
                access: Access::Write,
                named: Some(Named {
                    file: it.file,
                    word: it.word,
                }),
            });
        }

        Self {
            paths,
            services: if keyring {
                vec![Service::CredentialStore]
            } else {
                Vec::new()
            },
            unread,
        }
    }
}

/// `path` as the kernel names it: its longest part that exists, with its links resolved, and
/// the rest as written. A sandbox matches the path the kernel resolved, so a rule written on a
/// link's name (`/tmp`, `/var`, a linked `~/.config`) would match nothing.
pub(crate) fn real(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    loop {
        if let Ok(found) = existing.canonicalize() {
            let mut out = found;
            out.extend(rest.into_iter().rev());
            return out;
        }
        match (existing.file_name().map(ToOwned::to_owned), existing.pop()) {
            (Some(name), true) => rest.push(name),
            _ => return path.to_path_buf(),
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
///
/// **A forge's host is the project's choice, not the preset's** (D-1343-10): it is held to
/// `locks` as a host of the project's own is, so a policy's `hosts` reaches it. A preset's fixed
/// list is the `presets` lock's, which the caller asks first.
pub fn hosts(presets: &[Preset], plane: &Plane, locks: &policy::Locks) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |host: &str| {
        if !out.iter().any(|it| it == host) {
            out.push(host.to_owned());
        }
    };
    for preset in presets {
        let listed = preset.own_hosts();
        for host in listed {
            add(host);
        }
        if *preset == Preset::Forge {
            for host in plane.forge_hosts() {
                let held = hosts::Host::parse(&host).map_or(true, |host| {
                    locks
                        .refuses(&hosts::Granted {
                            host,
                            level: hosts::Level::Project,
                        })
                        .is_some()
                });
                if !held {
                    add(&host);
                }
            }
        }
    }
    out
}

/// What every chat of a project reaches and writes on this machine besides its presets, its own
/// folder and its temp folder (#1340): counted as [`Compiled::granted`] grants them, for the
/// sentence Settings › Sandbox opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Besides {
    /// The project's own hosts it is granted: each it takes, none a policy locks out, none an
    /// address of this machine.
    pub project_hosts: usize,
    /// Your own hosts on this machine it is granted besides those: confirmed here, and only
    /// while the local file is one git ignores ([`hosts::personal`]).
    pub your_hosts: usize,
    /// The folders you let every chat of the project write on this machine that still judge
    /// as grantable ([`local::granted_writes`], [`grant::still_grantable`]).
    pub folders: usize,
}

/// [`Besides`] for the project at `root`, read as `plane`, on `machine`: nothing where no
/// sandbox is in force ([`Plane::in_force`]).
pub fn besides(root: &Path, plane: &Plane, machine: &Machine) -> Besides {
    let locks = policy::Locks::of(root);
    let Some(policy) = plane.in_force(&locks) else {
        return Besides::default();
    };
    Besides {
        folders: granted_folders(root, machine, &[], &locks).len(),
        ..counted_hosts(&granted_hosts(&policy, root, &[], &[], &locks))
    }
}

/// The hosts every level grants a chat in the project at `root`: the project's, this
/// machine's with `chat`'s own, then `persona`'s, each held to `locks` and none an address of
/// this machine. The one place a chat's start and Settings' count read them ([`besides`]).
fn granted_hosts(
    policy: &Policy,
    root: &Path,
    persona: &[hosts::Host],
    chat: &[hosts::Host],
    locks: &policy::Locks,
) -> Vec<hosts::Granted> {
    let mut personal = hosts::personal(root);
    for host in chat {
        if !personal.contains(host) {
            personal.push(host.clone());
        }
    }
    hosts::off_this_machine(
        hosts::in_force(&policy.hosts, &personal, persona, locks),
        &hosts::own_addresses(),
    )
}

/// The folders a chat in the project at `root` may write besides its own: `chat`'s and the
/// ones you let every chat write here, each judged again ([`grant::still_grantable`]); none
/// where `locks` forbid write grants (#1343). The one place a chat's start and Settings' count
/// read them ([`besides`]).
fn granted_folders(
    root: &Path,
    machine: &Machine,
    chat: &[PathBuf],
    locks: &policy::Locks,
) -> Vec<PathBuf> {
    if locks.forbids_write_grants() {
        return Vec::new();
    }
    let mut writes = chat.to_vec();
    writes.extend(local::granted_writes(root));
    grant::still_grantable(&writes, &grant::Ground::of(root, root, machine).place())
}

/// The project's hosts and yours among `granted`.
fn counted_hosts(granted: &[hosts::Granted]) -> Besides {
    let at = |level: hosts::Level| granted.iter().filter(|one| one.level == level).count();
    Besides {
        project_hosts: at(hosts::Level::Project),
        your_hosts: at(hosts::Level::You),
        folders: 0,
    }
}

/// The policy for one chat, resolved to this machine and ready for a harness's compiler: the
/// neutral answer every compiler reads, and the only one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled {
    pub denied: Denied,
    pub hosts: Vec<String>,
    /// The folders a person let this chat write besides its own and the temp folders (#1342):
    /// its own grants and yours ([`grant`]), each judged again as it is compiled. Every compiler
    /// puts its denials after them, so a class still wins inside one.
    pub writable: Vec<PathBuf>,
    pub os: Os,
    /// Where a harness keeps its own files on this machine, for a compiler that has to let the
    /// whole harness write them ([`opencode`]).
    pub homes: Homes,
    /// What the presets widen past the hosts.
    pub widened: Widened,
}

/// What a project's sandbox widens past the hosts it lets a chat reach (spec #1330's coupled
/// widenings, #1337).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Widened {
    /// The project's own package caches, with [`Preset::Toolchains`] only (D-1337-6): a cache
    /// home of its sandboxed chats', never the person's caches ([`caches`]).
    pub caches: Option<caches::CacheHome>,
    /// Whether a chat may ask the system's certificate check, with `certificate-checks` only
    /// (D-1337-7): on macOS, Go programs such as `gh` verify a host's certificate by asking
    /// the `trustd` service, and without it every one of them fails TLS. Off by default: the
    /// service fetches the addresses a certificate names, past the egress proxy.
    pub trust: bool,
}

impl Widened {
    /// What `policy` widens for a chat in the project at `root` on `machine`, denied `denied`.
    pub fn of(policy: &Policy, machine: &Machine, root: &Path, denied: &Denied) -> Self {
        Self {
            caches: policy
                .egress
                .contains(&Preset::Toolchains)
                .then(|| caches::home_of(machine, root, denied))
                .flatten(),
            trust: policy.certificate_checks,
        }
    }
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
    pub(crate) fn charter_data(machine: &Machine) -> Option<PathBuf> {
        let named = |name: &str| {
            machine
                .env
                .get(name)
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
        };
        named(crate::datahome::HOME_VAR)
            .or_else(|| named("XDG_DATA_HOME").map(|xdg| crate::names::DATA_HOME.folder_at(&xdg)))
            .or_else(|| {
                let home = machine.home.as_ref()?;
                let base = match machine.os {
                    Os::MacOs => home.join("Library/Application Support"),
                    Os::Linux | Os::Windows | Os::Other => home.join(".local/share"),
                };
                // The folder rename-local moved it to, once it has (RN-5).
                Some(crate::names::DATA_HOME.folder_at(&base))
            })
    }

    /// The Codex home of the project at `root`'s sandboxed chats on `machine` (D-88q): a folder
    /// named for the project as the kernel names it, under charter's data home.
    pub fn codex_project(machine: &Machine, root: &Path) -> Option<PathBuf> {
        Some(
            Self::charter_data(machine)?
                .join("codex-homes")
                .join(Self::project_key(root)),
        )
    }

    /// The name of the project at `root`'s folders under purlis's data home: a digest of the
    /// project as the kernel names it.
    pub(crate) fn project_key(root: &Path) -> String {
        use sha2::Digest;
        let real = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let digest = sha2::Sha256::digest(real.as_os_str().as_encoded_bytes());
        digest
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

impl Compiled {
    /// Whether every class a service holds here is held for `harness`'s chat on this system.
    ///
    /// **The credential store is held by the store itself on macOS** (ruling V90a): charter
    /// writes every keyring item so that only charter's own app reads it without the person's
    /// confirmation (`secrets::keyring::OsStore`), whatever the chat's sandbox lets it
    /// reach. Charter's own wrap denies the service as well (V90b, [`seatbelt`]). Elsewhere the
    /// store keeps no rule of its own (the Secret Service answers any process of the session),
    /// and no harness's sandbox has been measured keeping a chat off it, so the chat is refused
    /// with the ways out (V90c).
    fn holds_every_service(&self, harness: Harness) -> Result<(), Uncompilable> {
        let unheld = self.denied.services.iter().find(|service| match service {
            Service::CredentialStore => self.os != Os::MacOs,
        });
        match unheld {
            Some(service) => Err(Uncompilable {
                harness,
                unheld: Unheld::Service(*service, self.os),
            }),
            None => Ok(()),
        }
    }

    /// `policy`, for a chat in `plane` at `root` running as `persona`, on `machine`: its
    /// presets' hosts, then the hosts in force at every level ([`hosts::in_force`]): the
    /// project's, this machine's, then the persona's (#1362). A chat on another persona, or on
    /// none, gets no persona's hosts.
    pub fn of(
        policy: &Policy,
        plane: &Plane,
        root: &Path,
        machine: &Machine,
        persona: Option<&str>,
    ) -> Self {
        Self::granted(
            policy,
            plane,
            root,
            machine,
            persona,
            &grant::Grants::default(),
        )
    }

    /// [`Self::of`], with what a person let this one chat do besides (#1342): `chat`'s hosts,
    /// after the project's and this machine's and held to the same locks; and the folders it and
    /// you may write, each judged again here ([`grant::still_grantable`]), so a grant a denial
    /// class has since come to cover grants nothing.
    pub fn granted(
        policy: &Policy,
        plane: &Plane,
        root: &Path,
        machine: &Machine,
        persona: Option<&str>,
        chat: &grant::Grants,
    ) -> Self {
        // An administrator's policy (#1343): the strictest value wins, so a preset it does not
        // allow is off here, whatever the project's file says.
        let locks = policy::Locks::of(root);
        let policy = &Policy {
            egress: locks.presets(&policy.egress),
            ..policy.clone()
        };
        let mut reached = hosts(&policy.egress, plane, &locks);
        let personas = persona::of(&policy.personas, persona)
            .map(|grants| grants.hosts.as_slice())
            .unwrap_or_default();
        for one in granted_hosts(policy, root, personas, &chat.hosts, &locks) {
            let spelled = one.host.to_string();
            if !reached.contains(&spelled) {
                reached.push(spelled);
            }
        }
        let denied = Denied::of(root, machine);
        let writable = granted_folders(root, machine, &chat.writes, &locks);
        Self {
            widened: Widened::of(policy, machine, root, &denied),
            denied,
            hosts: reached,
            writable,
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
    /// A class held by a service that neither the store nor the harness's sandbox holds on
    /// this system.
    Service(Service, Os),
    /// The harness has no sandbox of its own, and charter cannot wrap it on this system yet,
    /// so no class can be held.
    Wrap(Os),
}

impl Uncompilable {
    /// The one class that could not be held, where it was one class.
    pub fn class(self) -> Option<Class> {
        match self.unheld {
            Unheld::Service(service, _) => Some(service.class()),
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
    /// Boxed: a wrap carries its grants and widenings, and [`Decided`] carries this beside a
    /// variant of a few words.
    form: Box<Form>,
    /// The plane it was compiled for, which the chat's folder must be inside.
    root: PathBuf,
    /// The paths it denies, which must not cover the chat's folder ([`covering`]).
    denied: Vec<Denial>,
    /// The project's package caches it lets a chat write ([`Widened::caches`]).
    caches: Option<Box<caches::CacheHome>>,
    /// What a command run on the chat's behalf is held to. Boxed: it is a list of paths, and
    /// [`Decided`] carries this beside a variant of a few words.
    confines: Box<Confines>,
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

    /// What a command purlis runs for this chat is held to ([`Confines`]).
    pub fn confines(&self) -> &Confines {
        &self.confines
    }

    /// The folders this sandbox lets a chat write besides its own and the temp directories:
    /// opencode's own data, or what a Codex turn writes in Codex's home, under charter's wrap;
    /// and, for every harness, what the project's package caches let a chat write.
    pub fn writable(&self) -> Vec<PathBuf> {
        let mut out = match &*self.form {
            Form::Opencode(wrap) => wrap.data.iter().cloned().collect(),
            Form::Codex(wrap) => wrap.writable(),
            Form::ClaudeCode(_) => Vec::new(),
        };
        out.extend(self.caches.iter().flat_map(|caches| caches.writable()));
        out
    }

    /// The form as compiled for the project, before the manifests of a chat's own folders are
    /// added: what a chat is handed is [`Self::form_in`] its folder.
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
        // Every refusal here is said under the policy in force (#1423): where it forbids the
        // opt-out, none names it, and each ends with the policy and who set it.
        let locks = policy::Locks::of(&self.root);
        let locked = locks.opt_out_refused();
        let Some(cwd) = at.cwd else {
            return Err(FolderRefusal::Missing.said(&locks));
        };
        if let Some(why) = folder_refusal(&self.root, cwd) {
            return Err(why.said(&locks));
        }
        // #1327: never a chat whose own folder, or one above it, it may not write — by what
        // was compiled, or by a manifest of its folders that is a link to one of them (#1336).
        let ground: Vec<PathBuf> = cwd.ancestors().map(Path::to_path_buf).collect();
        let chain = self.chain(Some(cwd));
        if let Some(refused) = covering(&self.denied, &ground).or_else(|| covering(&chain, &ground))
        {
            return Err(refused.placed_in(&self.root).said(&locks));
        }
        // An adapter's own refusal, and the caches' below, ends with the policy too.
        let under = |why: String| match &locked {
            Some(policy) => format!("{why} {policy}"),
            None => why,
        };
        let at = &At {
            no_opt_out: locked.is_some(),
            ..*at
        };
        let mut line = self
            .harness
            .adapter()
            .sandboxed_line(&self.with(chain), words, at)
            .map_err(under)?;
        // The project's package caches (D-1337-6): made here, outside the sandbox, with any link
        // a chat planted in them taken out; refused, naming the path, only where one stays.
        if let Some(caches) = &self.caches {
            caches.prepare().map_err(under)?;
            line.env
                .retain(|(key, _)| !caches.env.iter().any(|(set, _)| set == key));
            line.env.extend(caches.env.iter().cloned());
        }
        Ok(line)
    }

    /// What that harness is handed for a chat in `cwd`: [`Self::form`], with the manifests in
    /// each folder from `cwd` up to the project root denied too ([`manifests_held`], #1336).
    /// The one form a chat is armed with and opened under, so both name the same folders.
    pub fn form_in(&self, cwd: Option<&Path>) -> Form {
        self.with(self.chain(cwd))
    }

    /// The manifests a chat in `cwd` is denied that the compiled form does not already deny.
    fn chain(&self, cwd: Option<&Path>) -> Vec<Denial> {
        manifests_held(&self.root, cwd)
            .into_iter()
            .filter(|denial| !self.denied.contains(denial))
            .collect()
    }

    /// The compiled form with `more` denied too.
    fn with(&self, more: Vec<Denial>) -> Form {
        let mut form = (*self.form).clone();
        match &mut form {
            Form::ClaudeCode(settings) => *settings = settings.denying(&more),
            Form::Codex(codex::Wrap { denied, .. })
            | Form::Opencode(opencode::Wrap { denied, .. }) => denied.extend(more),
        }
        form
    }

    /// What has to run for as long as a chat under this sandbox does, started now: charter's
    /// egress proxy and the chat's own temp directory, for a harness charter wraps ([`Form::
    /// Opencode`], [`Form::Codex`]); `None` for a harness whose own sandbox holds the policy.
    pub fn confine(&self) -> std::io::Result<Option<Confinement>> {
        match &*self.form {
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
    /// Whether an administrator's policy forbids starting a chat without the sandbox
    /// ([`policy::Locks::forbids_opt_out`]): a refusal an adapter writes then names no opt-out
    /// (#1423). [`Applied::line`] sets it from the policy in force, and ends every refusal with
    /// the policy and who set it; a caller leaves it `false`.
    pub no_opt_out: bool,
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

impl Form {
    /// Every path a wrap charter applies denies, with what its compiler added to the neutral
    /// list ([`Compiled::denied`]); `None` for a harness whose own sandbox holds the policy.
    pub fn denied(&self) -> Option<&[Denial]> {
        match self {
            Self::Codex(wrap) => Some(&wrap.denied),
            Self::Opencode(wrap) => Some(&wrap.denied),
            Self::ClaudeCode(_) => None,
        }
    }
}

/// `compiled`, by `compile`, for a chat in the plane at `root` on `machine`, with every path
/// it denies, or why it does not start: [`covering`]'s refusal among the reasons, held to the
/// list a wrap applies as well as the neutral one.
fn compile_checked(
    compile: Compiler,
    compiled: &Compiled,
    root: &Path,
    machine: &Machine,
) -> Result<(Form, Vec<Denial>), NotStarted> {
    if let Some(unread) = &compiled.denied.unread {
        return Err(NotStarted::Unread(unread.clone()));
    }
    let ground = ground(root, machine.home.as_deref());
    if let Some(refused) = covering(&compiled.denied.paths, &ground) {
        return Err(refused.placed_in(root));
    }
    let form = compile(compiled).map_err(NotStarted::Uncompilable)?;
    let denied = form
        .denied()
        .map_or_else(|| compiled.denied.paths.clone(), <[Denial]>::to_vec);
    if let Some(refused) = covering(&denied, &ground) {
        return Err(refused.placed_in(root));
    }
    Ok((form, denied))
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
        return Some("purlis has no sandbox compiler for it yet".to_owned());
    };
    if let Some(issue) = harness.adapter().sandbox_held_back() {
        return Some(format!(
            "purlis cannot keep its chats inside the sandbox yet (#{issue})"
        ));
    }
    let nothing = Compiled {
        denied: Denied::default(),
        hosts: Vec::new(),
        writable: Vec::new(),
        os,
        homes: Homes::default(),
        widened: Widened::default(),
    };
    match compile(&nothing) {
        Err(Uncompilable {
            unheld: Unheld::Wrap(_),
            ..
        }) => Some("purlis can wrap it on macOS only, so far".to_owned()),
        _ => None,
    }
}

/// Why a sandboxed chat is not started when `path`, of its project's package caches, is a link
/// purlis could not take out, as found `when` it made the folders.
pub fn caches_linked(path: &Path, when: &str) -> String {
    format!(
        "this project runs every chat sandboxed, and {} of the project's package caches is a \
         link purlis could not take out ({when} making them), so nothing was started. Remove \
         that link and start the chat again.",
        path.display()
    )
}

/// Why a sandboxed chat is not started in the folder it was given (ruling of 2026-10-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRefusal {
    /// The chat has no folder of its own.
    Missing,
    /// Its folder, or one between it and the project, is a link or not a real folder.
    Linked,
    /// Its folder is not inside the project.
    Outside,
}

impl FolderRefusal {
    /// What happened and what to change, with no full stop: [`Self::said`] ends it.
    fn happened(self) -> &'static str {
        match self {
            Self::Missing => {
                "this project runs every chat sandboxed, and the chat has no folder of its own \
                 to be confined to, so nothing was started. Forget this chat and start a new one \
                 in a folder of the project"
            }
            Self::Linked => {
                "this project runs every chat sandboxed, and the chat's folder, or a folder \
                 between it and the project, is a link or not a real folder, so nothing was \
                 started. Move the chat's folder into the project as a real folder, or forget \
                 this chat and start a new one"
            }
            Self::Outside => {
                "this project runs every chat sandboxed, and the chat's folder is not inside \
                 the project, so nothing was started. Move the chat's folder into the project, \
                 or forget this chat and start a new one"
            }
        }
    }

    /// **The refusal as the person reads it under `locks`** (#1423): it ends with the person's
    /// own way out, "Start without the sandbox", or, where an administrator's policy forbids
    /// that, with the policy and who set it instead. Both are built from the same first part,
    /// so no rewording can leave the opt-out named under a lock.
    pub fn said(self, locks: &policy::Locks) -> String {
        let happened = self.happened();
        match locks.opt_out_refused() {
            None => {
                format!("{happened}; Start without the sandbox is yours to pick when you start it.")
            }
            Some(policy) => format!("{happened}. {policy}"),
        }
    }
}

/// Why a chat in the folder `cwd` of the plane at `root` may not start sandboxed, or `None`
/// (ruling of 2026-10-03): every folder from the plane's own, as the kernel names it, down to
/// the chat's must be a real directory and not a link, so a folder another chat swapped for a
/// link never takes the rules of the folder it replaced to somewhere else.
pub fn folder_refusal(root: &Path, cwd: &Path) -> Option<FolderRefusal> {
    let Ok(real_root) = root.canonicalize() else {
        return Some(FolderRefusal::Linked);
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
        return Some(FolderRefusal::Outside);
    };
    let Ok(below) = cwd.strip_prefix(meets) else {
        return Some(FolderRefusal::Outside);
    };
    let below = below.to_path_buf();
    let mut at = real_root.clone();
    let check = |at: &Path| match std::fs::symlink_metadata(at) {
        Ok(meta) => meta.file_type().is_dir() && !meta.file_type().is_symlink(),
        Err(_) => false,
    };
    if !check(&at) {
        return Some(FolderRefusal::Linked);
    }
    for part in below.components() {
        match part {
            std::path::Component::Normal(name) => at.push(name),
            std::path::Component::CurDir => continue,
            _ => return Some(FolderRefusal::Linked),
        }
        if !check(&at) {
            return Some(FolderRefusal::Linked);
        }
    }
    // The walk and the kernel agree on where the folder is.
    match cwd.canonicalize() {
        Ok(real) if real == at => None,
        _ => Some(FolderRefusal::Linked),
    }
}

/// The ground a chat in the plane at `root` stands on, whatever its folder: `/` and every
/// folder down to the plane's own, and the home directory.
fn ground(root: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    root.ancestors()
        .map(Path::to_path_buf)
        .chain(home.map(Path::to_path_buf))
        .collect()
}

/// The refusal for the first of `denied` that covers a folder of `ground` — is it, or is above
/// it — by any name either has ([`spellings`]), compared case-folded where the volume the folder
/// is on folds case ([`folds_case`]); `None` where none does.
///
/// **Fail closed** (#1327). A rule that covers the ground a chat stands on (`/`, the home
/// directory, the project, or the chat's own folder or one above it) leaves it read-only
/// everywhere it works. That is never what a class means, so the chat is refused, naming what
/// named the path, rather than started read-only or started with the rule left out.
pub fn covering(denied: &[Denial], ground: &[PathBuf]) -> Option<NotStarted> {
    let ground: Vec<(PathBuf, bool)> = ground
        .iter()
        .flat_map(|folder| {
            let folds = folds_case(folder);
            spellings(folder).into_iter().map(move |name| (name, folds))
        })
        .collect();
    denied.iter().find_map(|denial| {
        let covers = spellings(&denial.path).iter().any(|name| {
            ground.iter().any(|(folder, folds)| {
                folder.starts_with(name) || (*folds && folded(folder).starts_with(folded(name)))
            })
        });
        covers.then(|| NotStarted::CoversItsGround {
            path: denial.path.clone(),
            class: denial.class,
            named: denial.named.clone(),
            within: None,
        })
    })
}

/// Each spelling of `path` a sandbox or a tool may compare: as written, with `..` taken off,
/// as the kernel names either, and each of those on the data volume's other side
/// ([`firmlink_twin`]).
fn spellings(path: &Path) -> Vec<PathBuf> {
    let lexical = planted::lexical(path);
    let mut out = vec![path.to_path_buf(), real(path), real(&lexical), lexical];
    let twins: Vec<PathBuf> = out.iter().filter_map(|it| firmlink_twin(it)).collect();
    out.extend(twins);
    out
}

/// `path` with every part lower-cased: what a volume that folds case compares.
fn folded(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

/// Whether the volume `path` is on folds case (#1356): whether the deepest folder of it that
/// exists, with a letter in its name, is found again under that name with its case swapped.
/// A path that does not exist yet and differs from one that does only in case is that folder
/// there, which resolving it ([`real`]) cannot tell.
///
/// Asked of `path` as the kernel names it, and of the swapped name itself, not what it links
/// to (#1418): a link on the way could put the folder on another volume, and a link beside it
/// whose name differs only in case would answer as the folder.
fn folds_case(path: &Path) -> bool {
    let path = real(path);
    path.ancestors()
        .filter(|folder| folder.exists())
        .find_map(|folder| {
            let name = folder.file_name()?.to_str()?;
            let swapped: String = name
                .chars()
                .flat_map(|c| {
                    if c.is_lowercase() {
                        c.to_uppercase().collect::<Vec<_>>()
                    } else {
                        c.to_lowercase().collect()
                    }
                })
                .collect();
            (swapped != name).then(|| (folder, folder.with_file_name(swapped)))
        })
        .is_some_and(|(folder, swapped)| {
            let id = crate::pypath::file_identity(folder);
            id.is_some() && id == entry_identity(&swapped)
        })
}

/// `(st_dev, st_ino)` of the entry `path` itself, a link not followed.
fn entry_identity(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((meta.dev(), meta.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}

/// Where macOS keeps what its firmlinks reach: `/Users` is also `/System/Volumes/Data/Users`.
const DATA_VOLUME: &str = "/System/Volumes/Data";

/// The firmlinks macOS 26.2 lists in `/usr/share/firmlinks`, for a machine whose list cannot
/// be read.
const KNOWN_FIRMLINKS: [&str; 18] = [
    "/AppleInternal",
    "/Applications",
    "/Library",
    "/System/Library/Caches",
    "/System/Library/Assets",
    "/System/Library/PreinstalledAssets",
    "/System/Library/AssetsV2",
    "/System/Library/PreinstalledAssetsV2",
    "/System/Library/CoreServices/CoreTypes.bundle/Contents/Library",
    "/System/Library/Speech",
    "/Users",
    "/Volumes",
    "/cores",
    "/opt",
    "/private",
    "/usr/local",
    "/usr/libexec/cups",
    "/usr/share/snmp",
];

/// The folders this machine reaches by a firmlink: none but on macOS, where they are those
/// `/usr/share/firmlinks` lists and those [`KNOWN_FIRMLINKS`] holds, together.
///
/// **Fail closed** (#1356). A list that cannot be read, or reads empty, is logged and leaves
/// the known ones, never none: without them a rule or a ground loses its other name.
fn firmlinks() -> &'static [PathBuf] {
    static FIRMLINKS: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    FIRMLINKS.get_or_init(|| {
        if !cfg!(target_os = "macos") {
            return Vec::new();
        }
        let text = std::fs::read_to_string("/usr/share/firmlinks").ok();
        if text.as_deref().is_none_or(|text| text.trim().is_empty()) {
            tracing::warn!(
                "the system's firmlink list could not be read; the sandbox uses the one purlis knows"
            );
        }
        firmlinks_from(text.as_deref())
    })
}

/// The firmlinks `text`, a `/usr/share/firmlinks`, lists, with [`KNOWN_FIRMLINKS`]; the known
/// ones alone where there is no text.
pub(crate) fn firmlinks_from(text: Option<&str>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = KNOWN_FIRMLINKS.iter().map(PathBuf::from).collect();
    let listed = text
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split('\t').next())
        .map(PathBuf::from)
        .filter(|it| it.is_absolute() && it.parent().is_some());
    for link in listed {
        if !out.contains(&link) {
            out.push(link);
        }
    }
    out
}

/// `path` as the kernel names it ([`real`]), and under its other name across a firmlink
/// ([`firmlink_twin`]) where it has one: each a rule written for the kernel is held to.
///
/// The name off the data volume comes first, so either spelling gives one order.
pub(crate) fn kernel_names(path: &Path) -> Vec<PathBuf> {
    both_firmlink_names(real(path))
}

/// `path` as it is spelled, and under its other name across a firmlink ([`firmlink_twin`])
/// where it has one, the name off the data volume first.
pub(crate) fn both_firmlink_names(path: PathBuf) -> Vec<PathBuf> {
    match firmlink_twin(&path) {
        Some(twin) if path.starts_with(DATA_VOLUME) => vec![twin, path],
        Some(twin) => vec![path, twin],
        None => vec![path],
    }
}

/// `path`'s other name across a firmlink (#1356): `/Users/x` for `/System/Volumes/Data/Users/x`
/// and the other way, or `None` where no firmlink reaches it. Resolving either name
/// ([`real`]) leaves it as it is spelled, so a rule or a ground held to one would miss the
/// other.
pub(crate) fn firmlink_twin(path: &Path) -> Option<PathBuf> {
    let data = Path::new(DATA_VOLUME);
    let linked = |it: &Path| firmlinks().iter().any(|link| it.starts_with(link));
    match path.strip_prefix(data) {
        Ok(below) => {
            let plain = Path::new("/").join(below);
            linked(&plain).then_some(plain)
        }
        Err(_) => linked(path)
            .then(|| path.strip_prefix("/").ok().map(|below| data.join(below)))
            .flatten(),
    }
}

/// Where a config that named a refused path sits in the project: its repo, as a path from
/// the project, and its workspace (#1356).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Within {
    pub repo: Option<PathBuf>,
    pub workspace: Option<String>,
}

impl Within {
    /// Where `file` sits in the project at `root`: the nearest folder above it, below the
    /// project, that holds a `.git`, and its workspace; `None` where it is in neither.
    fn of(file: &Path, root: &Path) -> Option<Self> {
        let dir = file.parent()?;
        let repo = dir
            .ancestors()
            .take_while(|folder| *folder != root && folder.starts_with(root))
            .find(|folder| folder.join(".git").exists())
            .and_then(|folder| folder.strip_prefix(root).ok())
            .map(Path::to_path_buf);
        let workspace = crate::active::workspace_of_tree(root, dir);
        (repo.is_some() || workspace.is_some()).then_some(Self { repo, workspace })
    }
}

impl fmt::Display for Within {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.repo, &self.workspace) {
            (Some(repo), Some(workspace)) => {
                write!(f, "in the repo {} of workspace {workspace}", repo.display())
            }
            (Some(repo), None) => write!(f, "in the repo {}", repo.display()),
            (None, Some(workspace)) => write!(f, "in workspace {workspace}"),
            (None, None) => Ok(()),
        }
    }
}

/// Whether a plane manifest above `start`, under either name (`charter.toml` or
/// `purlis.toml`), is there and is not a regular file — a link, a FIFO, a socket, a device or a
/// directory — which the plane's own walk does not take for a plane at all. A sandboxed start
/// refuses it ([`NotStarted::PlaneUnreadable`]) rather than reading "no plane" and starting the
/// chat unsandboxed.
pub fn marker_unreadable(start: &Path) -> bool {
    let here = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    here.ancestors().any(|dir| {
        crate::names::PLANE_MANIFEST.spellings().any(|name| {
            std::fs::symlink_metadata(dir.join(name)).is_ok_and(|meta| !meta.file_type().is_file())
        })
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
    /// The open project's own manifest is not there, so whether it runs chats sandboxed is
    /// unknown, and the chat is not started rather than started unsandboxed (D-1410e).
    PlaneMissing,
    /// A path the sandbox would deny covers the ground the chat stands on ([`covering`]), so
    /// it would start unable to write where it works (#1327).
    CoversItsGround {
        path: PathBuf,
        class: Class,
        named: Option<Named>,
        /// Where the config that named it sits, in a repo or a workspace.
        within: Option<Within>,
    },
    /// A config's commands change folder or name scripts more often than purlis follows
    /// ([`planted::MOST_MOVES`], [`planted::MOST_NAMED`]), so what they run is not known.
    Unread(Named),
    /// A person asked to start the chat without the sandbox, and an administrator's policy
    /// forbids it (#1343): why, naming the policy and who set it.
    OptOutLocked(String),
    /// An administrator's policy requires the sandbox, and purlis has no sandbox backend on
    /// this system (D-1423-1): the chat is not started, and never started unconfined.
    /// [`Self::said`] names the policy and who set it.
    RequiredWithoutBackend(Os),
}

impl NotStarted {
    /// This refusal with where the config that named its path sits in the project at `root`
    /// ([`Within`]), for one [`covering`] gave.
    fn placed_in(self, root: &Path) -> Self {
        match self {
            Self::CoversItsGround {
                path,
                class,
                named,
                within: None,
            } => {
                let within = named
                    .as_ref()
                    .and_then(|named| Within::of(&named.file, root));
                Self::CoversItsGround {
                    path,
                    class,
                    named,
                    within,
                }
            }
            other => other,
        }
    }
}

impl fmt::Display for NotStarted {
    /// The refusal where a person may still start the chat without the sandbox. Where an
    /// administrator's policy forbids that, say it with [`NotStarted::said`] instead.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.sentence(true))
    }
}

impl NotStarted {
    /// **The refusal as the person reads it under `locks`** (#1423): where an administrator's
    /// policy forbids starting a chat without the sandbox, no sentence sends the person to
    /// that opt-out, and each ends with the policy and who set it instead
    /// ([`policy::Locks::opt_out_refused`]). Where it does not, the refusal as it always read.
    pub fn said(&self, locks: &policy::Locks) -> String {
        match (self, locks.opt_out_refused()) {
            // It names the policy itself.
            (Self::OptOutLocked(_), _) | (_, None) => self.to_string(),
            (_, Some(policy)) => format!("{} {policy}", self.sentence(false)),
        }
    }

    /// **The refusal beside a line that already names the policy** ([`Ahead::Refused`], which
    /// the new-chat picker draws over what locks the opt-out): [`Self::said`] without the
    /// policy's own sentence.
    fn said_beside(&self, locks: &policy::Locks) -> String {
        self.sentence(!locks.forbids_opt_out())
    }

    /// The refusal in one or more sentences. `opt_out` is whether a person may start this chat
    /// without the sandbox: where they may not, no way out names it.
    fn sentence(&self, opt_out: bool) -> String {
        let lead = "this project runs every chat sandboxed";
        match self {
            Self::NoCompiler(harness) => format!(
                "{lead}, and purlis cannot sandbox {} {} chat yet, so it was not started. \
                 Start this chat on a {} profile.",
                article(harness.title()),
                harness.title(),
                sandboxed_harnesses_but(Some(*harness))
            ),
            Self::ProgramWritable(path) => format!(
                "{lead}, and the program lives where this chat can write: {}, so it was not \
                 started sandboxed. Keep the program outside the plane and outside what a chat \
                 may write.",
                path.display()
            ),
            Self::ProgramRelative => format!(
                "{lead}, and this profile's program is a relative path, which would be found in \
                 a folder the chat can write, so it was not started sandboxed. Name the program \
                 by its full path."
            ),
            Self::WordTooLong => format!(
                "{lead}, and a word of this profile's command is longer than 4 KiB, which purlis \
                 does not check, so it was not started sandboxed. Keep what it says in a file \
                 outside the plane and name that file instead."
            ),
            Self::WordWritable(word) => format!(
                "{lead}, and this profile's command names {word}, which lies where this chat \
                 can write, so it was not started sandboxed. Keep every file the command names \
                 outside the plane and outside what a chat may write."
            ),
            Self::ProbeTimedOut(harness) => format!(
                "{lead}, and this profile's program did not answer whether it is {} within {} \
                 seconds, so it was not started. A first run of a program can be slow: start \
                 the chat again.",
                harness.title(),
                program::PATIENCE.as_secs()
            ),
            Self::NotTheHarness(harness) => format!(
                "{lead}, and this profile's program does not answer as {}, whose sandbox it was \
                 given, so it was not started sandboxed.",
                harness.title()
            ),
            Self::HeldBack(harness, issue) => format!(
                "purlis cannot keep {} {} chat inside its sandbox yet (#{issue}), so in this \
                 project {}",
                article(harness.title()),
                harness.title(),
                if opt_out {
                    "a new one starts only without the sandbox, from the new-chat picker."
                } else {
                    "none starts."
                }
            ),
            Self::PlaneUnreadable => format!(
                "{FILE} in this plane cannot be read as TOML, so purlis cannot tell whether it \
                 runs chats sandboxed, and nothing was started. Fix {FILE} and start the chat \
                 again."
            ),
            Self::PlaneMissing => format!(
                "This project's {FILE} is missing, so purlis cannot tell whether it runs chats \
                 sandboxed, and nothing was started. Restore {FILE}, or reopen the project."
            ),
            Self::CoversItsGround {
                path,
                named: Some(named),
                within,
                ..
            } => format!(
                "{lead}, and `{}` in {}{} reads as a script purlis keeps this chat from \
                 changing, which would leave it unable to write {}, so nothing was started. \
                 Change that word in {}{}",
                named.word,
                named.file.display(),
                within
                    .as_ref()
                    .map_or_else(String::new, |within| format!(" ({within})")),
                path.display(),
                named.file.display(),
                if opt_out {
                    ", or start this chat without the sandbox from the new-chat picker."
                } else {
                    "."
                }
            ),
            Self::CoversItsGround {
                path,
                class,
                named: None,
                ..
            } => format!(
                "{lead}, and its {} rules would keep the chat from writing {}, so nothing was \
                 started.{}",
                class.word(),
                path.display(),
                if opt_out {
                    " Start this chat without the sandbox from the new-chat picker."
                } else {
                    ""
                }
            ),
            Self::Unread(named) => format!(
                "{lead}, and a command in {} changes folder or names scripts more often than \
                 purlis follows, from `{}` on, so purlis cannot tell what it runs, and nothing \
                 was started. Move that command into a script of its own{}",
                named.file.display(),
                named.word,
                if opt_out {
                    ", or start this chat without the sandbox from the new-chat picker."
                } else {
                    "."
                }
            ),
            Self::OptOutLocked(why) => format!("{why} Nothing was started."),
            // The Windows backend is M46, #565.
            Self::RequiredWithoutBackend(os) => format!(
                "purlis has no sandbox backend on {} yet, and policy requires the sandbox for \
                 every chat on this machine, so nothing was started.",
                match os {
                    Os::Windows => "Windows",
                    Os::MacOs | Os::Linux | Os::Other => "this system",
                }
            ),
            Self::NoBackend(missing) => format!(
                "{lead}, and this machine cannot apply the sandbox: {missing}. Nothing was \
                 started."
            ),
            Self::Uncompilable(it) => match it.unheld {
                // The Linux wrap is #1040.
                Unheld::Wrap(os) => format!(
                    "{lead}, and purlis runs {} inside a sandbox of its own, which it can apply \
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
                // Ruling V90c: never a dead end. The new-chat picker offers the opt-out beside
                // it; a resumed or relaunched chat has no opt-out, so moving the secrets is its
                // way on.
                Unheld::Service(Service::CredentialStore, os) => format!(
                    "this project runs every chat sandboxed, and {} purlis cannot keep {} {} \
                     chat away from the system keyring, where this project's keyring vaults \
                     keep their secrets, so nothing was started. {} a plain-file or 1Password \
                     vault, which the sandbox can keep from a chat.{}",
                    match os {
                        Os::Linux => "on Linux",
                        Os::MacOs | Os::Windows | Os::Other => "on this system",
                    },
                    article(it.harness.title()),
                    it.harness.title(),
                    if opt_out {
                        "Start this chat without the sandbox from the new-chat picker, or move \
                         those secrets to"
                    } else {
                        "Move those secrets to"
                    },
                    if opt_out {
                        " For a resumed or relaunched chat, moving them is the way on."
                    } else {
                        ""
                    }
                ),
            },
        }
    }
}

/// **Whether writing `after` over `before` changes a sandbox key** — the rule every brokered
/// write keeps (ADR 0067 §1 and §2 as amended; #1333): a write a chat asks `purlisd` to make is
/// refused when it would. `before` is the file as it stands (`None`: not there). Any change to
/// `[sandbox]` counts, in either settings file: its mode, its presets, its hosts. A text that is
/// not TOML counts too, since what it says about the sandbox is unknown — and so does an absent
/// file made where none was, if it holds `[sandbox]`.
pub fn changes_a_sandbox_key(before: Option<&str>, after: &str) -> bool {
    let table = |text: &str| -> Result<Option<toml::Value>, ()> {
        let top = text.parse::<toml::Table>().map_err(|_| ())?;
        Ok(top.get(TABLE).cloned())
    };
    let was = match before.map(table) {
        None => Ok(None),
        Some(was) => was,
    };
    match (was, table(after)) {
        (Ok(was), Ok(now)) => was != now,
        _ => true,
    }
}

/// What this machine's `charter.local.toml` says in `[sandbox]` that purlis does not take: any
/// key but `hosts`, and each entry of it that is not a host.
fn local_refusals(text: &str) -> Vec<String> {
    let local = crate::profiles::LOCAL_FILE;
    let Ok(top) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(table) = top.get(TABLE) else {
        return Vec::new();
    };
    let Some(table) = table.as_table() else {
        return vec![format!(
            "{TABLE} in {local} is not a table — this machine's [{TABLE}] holds hosts, and only \
             hosts"
        )];
    };
    let mut out: Vec<String> = table
        .keys()
        .filter(|key| *key != hosts::KEY)
        .map(|key| {
            format!(
                "{TABLE}.{key} in {local} is not read — this machine's [{TABLE}] holds hosts, and \
                 only hosts. Whether chats run sandboxed, and the presets, are the project's, in \
                 {FILE}."
            )
        })
        .collect();
    match hosts::read(table.get(hosts::KEY)) {
        Ok(hosts::Listed { refused, .. }) => {
            out.extend(refused.into_iter().map(|(written, why)| {
                format!(
                    "{TABLE}.hosts in {local} names {written}, which no chat is let reach: {why}"
                )
            }))
        }
        Err(hosts::NotAList) => out.push(format!(
            "{TABLE}.hosts in {local} is not a list of hosts, so none of yours is allowed — \
             write hosts = [\"api.example.com\", \"10.0.0.5:6443\"]"
        )),
    }
    out
}

/// What a chat of `harness` in the plane at `root` starts under on `machine`: `None` where no
/// sandbox is in force (the plane has not turned it on, and no policy requires it:
/// [`Plane::in_force`]), the compiled sandbox where one is — or why the chat does not start.
/// `has` answers whether a program the backend needs is installed ([`backend::installed`]).
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
    for_start_granted(harness, root, machine, has, None, &grant::Grants::default())
}

/// [`for_start`] for a chat running as `persona`, which adds the persona's own grants (#1362).
pub fn for_start_as(
    harness: Harness,
    root: &Path,
    machine: &Machine,
    has: &dyn Fn(&str) -> bool,
    persona: Option<&str>,
) -> Result<Option<Applied>, NotStarted> {
    for_start_granted(
        harness,
        root,
        machine,
        has,
        persona,
        &grant::Grants::default(),
    )
}

/// [`for_start_as`], for a chat a person let do `chat` besides (#1342): compiled in by
/// [`Compiled::granted`].
pub fn for_start_granted(
    harness: Harness,
    root: &Path,
    machine: &Machine,
    has: &dyn Fn(&str) -> bool,
    persona: Option<&str>,
    chat: &grant::Grants,
) -> Result<Option<Applied>, NotStarted> {
    // One spelling of the plane for every harness: the kernel's.
    let real_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let root = real_root.as_path();
    let plane = Plane::read(root);
    // Fail closed: a file that may say `[sandbox]` and cannot be read never reads as "not set".
    if plane.unreadable() {
        return Err(NotStarted::PlaneUnreadable);
    }
    // The project's own, or the one an administrator's policy requires (D-1423-1).
    let Some(policy) = plane.in_force(&policy::Locks::of(root)) else {
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
    let compiled = Compiled::granted(&policy, &plane, root, machine, persona, chat);
    applied_of(harness, compile, &compiled, root, machine).map(Some)
}

/// `compiled`, compiled by `compile` for a chat of `harness` in the plane at `root` on
/// `machine`, with what a command run for that chat is held to ([`Confines`]).
pub(crate) fn applied_of(
    harness: Harness,
    compile: Compiler,
    compiled: &Compiled,
    root: &Path,
    machine: &Machine,
) -> Result<Applied, NotStarted> {
    let (form, denied) = compile_checked(compile, compiled, root, machine)?;
    let mut held = denied.clone();
    if let Some(keychains) = seatbelt::keychains(machine.home.as_deref())
        && !held.contains(&keychains)
    {
        held.push(keychains);
    }
    Ok(Applied {
        harness,
        form: Box::new(form),
        root: root.to_path_buf(),
        denied,
        caches: compiled.widened.caches.clone().map(Box::new),
        confines: Box::new(Confines {
            denied: held,
            hosts: compiled.hosts.clone(),
            widened: compiled.widened.clone(),
            writable: compiled.writable.clone(),
        }),
    })
}

/// What a command purlis runs on a sandboxed chat's behalf is held to (#1407): exactly what the
/// chat's own sandbox was compiled to deny, as its harness's compiler listed it (a Codex chat's
/// own Codex home among them, D-88q) with the credential store's files, and the hosts it may
/// reach. Recorded when the chat starts, so a change to the project's policy afterwards does
/// not change what a run for that chat may do. What the chat's sandbox widens (#1337) is
/// recorded too, so the run gets the chat's certificate check and cache grants.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Confines {
    pub denied: Vec<Denial>,
    pub hosts: Vec<String>,
    /// What the chat's sandbox widens (#1337): its cache grants and certificate check, so a
    /// run for the chat gets them too.
    pub widened: Widened,
    /// The folders a person let the chat write besides its own ([`Compiled::writable`], #1342):
    /// its own grants and the every-chat ones. Recorded so a start compiled later can be told
    /// from this one by them (#1428); a run for the chat is not widened by it.
    pub writable: Vec<PathBuf>,
}

/// `policy` for a chat of `harness` in `plane` at `root`, with no `charter.toml` written and no
/// backend asked: for a test of what a compiler makes of a policy.
#[cfg(test)]
pub(crate) fn applied_for(
    harness: Harness,
    policy: &Policy,
    plane: &Plane,
    root: &Path,
    machine: &Machine,
) -> Result<Applied, NotStarted> {
    let compile = compiler(harness).expect("a harness with a compiler");
    let compiled = Compiled::of(policy, plane, root, machine, None);
    applied_of(harness, compile, &compiled, root, machine)
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
    let compiled = Compiled::of(&policy, &plane, root, machine, None);
    applied_of(harness, compile, &compiled, root, machine)
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
                "This chat runs without the sandbox: purlis has no sandbox backend on {} yet, \
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
                "actor": "purlis (no backend on this OS)",
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
/// is a person's choice to start it without the sandbox: `None` where no sandbox is in force
/// (the project has not turned it on and no policy requires it, [`Plane::in_force`]: there is
/// nothing to lift, so nothing to audit), else sandboxed or unsandboxed — or why the chat does
/// not start.
///
/// **Fail closed, with two exits that are each audited** (ADR 0067 §1 and §7):
/// - a person's opt-out, which starts the chat without the sandbox even where it could not be
///   applied — the opt-out inside the refusal;
/// - Windows, where no backend exists yet and every chat starts at the opt-out with charter as
///   the actor (ruling V21 3). Any other system with no backend still refuses.
///
/// **An administrator's policy that forbids the opt-out closes both** (ADR 0067 §4 as amended,
/// D-1423-1): the opt-out is refused ([`NotStarted::OptOutLocked`]), and so is a start on a
/// system with no backend ([`NotStarted::RequiredWithoutBackend`]). Neither starts unconfined.
pub fn decide(
    harness: Harness,
    root: &Path,
    machine: &Machine,
    has: &dyn Fn(&str) -> bool,
    opt_out: Option<&OptOut>,
    persona: Option<&str>,
) -> Result<Option<Decided>, NotStarted> {
    decide_granted(
        harness,
        root,
        machine,
        has,
        opt_out,
        persona,
        &grant::Grants::default(),
    )
}

/// [`decide`], for a chat a person let do `chat` besides (#1342).
pub fn decide_granted(
    harness: Harness,
    root: &Path,
    machine: &Machine,
    has: &dyn Fn(&str) -> bool,
    opt_out: Option<&OptOut>,
    persona: Option<&str>,
    chat: &grant::Grants,
) -> Result<Option<Decided>, NotStarted> {
    // A file that cannot be read may say `[sandbox]`, so it never reads as "not set", and nor
    // does one that has gone (D-1410e): each falls through to a refusal below, which the
    // opt-out sits inside.
    let plane = Plane::read(root);
    let locks = policy::Locks::of(root);
    if !plane.unreadable() && !plane.missing() && plane.in_force(&locks).is_none() {
        return Ok(None);
    }
    if let Some(opt_out) = opt_out {
        // An administrator's policy may forbid it (#1343, ADR 0067 §7): refused, saying who.
        if let Some(why) = locks.opt_out_refused() {
            return Err(NotStarted::OptOutLocked(why));
        }
        return Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::Person,
            reason: opt_out.kept_reason(),
        })));
    }
    if !machine.os.has_backend() {
        // A policy that requires the sandbox is never met by starting without one (D-1423-1,
        // which replaces D-1343-9): refused, and never started unconfined.
        if locks.forbids_opt_out() {
            return Err(NotStarted::RequiredWithoutBackend(machine.os));
        }
        return Ok(Some(Decided::Unsandboxed(Lifted {
            by: By::NoBackend(Os::Windows),
            reason: None,
        })));
    }
    if plane.missing() {
        return Err(NotStarted::PlaneMissing);
    }
    for_start_granted(harness, root, machine, has, persona, chat)
        .map(|applied| applied.map(Decided::Sandboxed))
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
    // Under a policy that forbids the opt-out no sentence sends the person to it (#1423); the
    // picker says what locks it on a line of its own.
    let locks = policy::Locks::of(root);
    let refused = |refused: NotStarted| Ahead::Refused {
        why: refused.said_beside(&locks),
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
            | NotStarted::CoversItsGround { .. }
            | NotStarted::Unread(_)
            | NotStarted::PlaneUnreadable
            | NotStarted::PlaneMissing
            | NotStarted::OptOutLocked(_)
            | NotStarted::RequiredWithoutBackend(_) => None,
        },
    };
    match decide(harness, root, machine, has, None, None) {
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
pub mod caches;
pub mod claude;
pub mod codex;
pub mod egress;
pub mod grant;
pub mod hosts;
pub mod local;
pub mod opencode;
pub mod persona;
pub mod planted;
pub mod policy;
pub mod program;
pub mod seatbelt;

#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod egress_tests;
#[cfg(test)]
mod hosts_tests;
#[cfg(test)]
mod opencode_tests;
#[cfg(test)]
mod persona_tests;
#[cfg(test)]
mod tests;
