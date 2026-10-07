//! **Policy** (#1343; ADR 0067 §1 and §4, ruling C9): an administrator's locks on what a
//! project's sandbox and the people working in it may widen. The strictest value wins: a lock
//! only ever takes away from what the project, you, a persona or a chat's grant would allow.
//!
//! # Where it is read from
//!
//! **This machine's policy file, [`MACHINE_FILE`]**, and nothing else yet. ADR 0067 §4 names
//! C9's other layers (an MDM profile, a Windows policy key, the organisation's policy on a
//! server) without saying where any of them lives, so they are not read (D-1343-1).
//!
//! **Only a file a chat cannot have written is read** ([`judge`]): a regular file, never a
//! link, owned by the system's administrator (root), in a folder owned by root, neither of
//! them writable by anyone else by its permission bits.
//!
//! **An access-control list is not read** (D-1423-5). On Linux one cannot hide from the bits:
//! the group bits of a file that carries a POSIX ACL are its mask, the most any named user or
//! group is given, so an ACL that grants write shows as group-writable and is refused. On macOS
//! an ACL is kept apart from the bits and is read only through `acl_get_link_np`, a C call
//! this crate cannot make without `unsafe`, so a file or folder whose ACL grants someone else
//! write passes. Only root can set one on a file root owns, so it is an administrator's
//! misconfiguration and never a chat's way in; `ls -le /etc/purlis` shows one. The folders
//! above the policy's own are not judged either: they are root's on every system purlis reads
//! a policy on, as for `sudoers`.
//!
//! Its folder is denied to every chat's writes as well
//! ([`super::Denied::of`], [`super::Class::HumanPowers`]). A file that is there and fails any
//! of that, a folder that is there and fails it with or without the file in it ([`judge_folder`],
//! D-1343-11), a file that does not parse, or one that says anything this version does not
//! know, is **refused, and refused closed** (D-1343-3): every lock but the presets is set,
//! because purlis cannot tell what the administrator meant, and the window says why and names
//! the file.
//!
//! # What it can lock
//!
//! ```json
//! {
//!   "owner": "Platform team <platform@example.com>",
//!   "sandbox": {
//!     "presets": ["model-providers", "forge"],
//!     "hosts": ["*.corp.example.com", "api.example.com"],
//!     "personal-hosts": false,
//!     "persona-hosts": false,
//!     "opt-out": false,
//!     "write-grants": false,
//!     "vault-grants": false
//!   },
//!   "dispatch": {
//!     "running-per-chat": 4,
//!     "depth": 2,
//!     "may-run-at-once": 3
//!   }
//! }
//! ```
//!
//! - `owner`: who set it, as Settings and a Notice name them. Absent, it is "this machine's
//!   administrator".
//! - `presets`: the Internet access presets a project may turn on; any other is off.
//! - `hosts`: the hosts a project (its own and its forges'), you, a persona or a chat's grant
//!   may add; any other is refused. A preset's fixed list is the `presets` lock's. A `*.domain` entry allows every name under it; an entry without a port allows
//!   every port.
//! - `personal-hosts`, `persona-hosts`: `false` forbids your own hosts on this machine (and a
//!   block's Allow for this chat or for you), and a persona's own hosts.
//! - `opt-out`: `false` forbids a person's opt-out, and so **requires the sandbox** (D-1423-1,
//!   the operator's ruling of 2026-10-07): no chat on this machine starts without the sandbox,
//!   from the new-chat picker or a block's Notice, in any project. A project with no `[sandbox]`,
//!   or one that has not turned it on, runs every chat sandboxed here as if it had, with the
//!   default presets ([`super::Plane::in_force`]); `presets` and `hosts` hold it like any other.
//!   On a system purlis has no sandbox backend for, no harness chat starts at all
//!   ([`super::NotStarted::RequiredWithoutBackend`]).
//! - `write-grants`: `false` forbids every folder a block's Allow or Settings would let a chat
//!   write.
//! - `vault-grants`: `false` forbids letting a persona's chats use a vault the vault registry
//!   does not tag for it (a refused vault's Allow, #1430), and takes away any such grant
//!   already made: only the registry's tags open a vault to a chat.
//!
//! - `dispatch`: a **ceiling** on each dispatch limit it names (#1440,
//!   [`crate::dispatchlimits`]): no project, workspace, persona or person's own setting gives
//!   more. `may-dispatch` and `may-run-at-once` cap every persona. 0 switches dispatch off on
//!   this machine. A file that is refused is read as 0 for every one.
//!
//! Absent keys lock nothing. `true` is the same as absent.

use std::path::{Path, PathBuf};

use super::Preset;
use super::grant;
use super::hosts::{Granted, Host, Level};

/// **This machine's policy file**: in a system folder only an administrator writes.
pub const MACHINE_FILE: &str = "/etc/purlis/policy.json";

/// The most of a policy file read; a larger one is refused.
const MOST_BYTES: u64 = 64 * 1024;

/// The longest `owner` kept, in characters.
const MOST_OWNER_CHARS: usize = 120;

/// Who an administrator is called where the policy names nobody.
const NOBODY_NAMED: &str = "this machine's administrator";

/// Where a policy came from, and who set it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    file: PathBuf,
    /// The policy's `owner`, one line.
    owner: Option<String>,
    /// Why the file was refused, where it was: every lock is then set.
    refused: Option<String>,
}

/// **An administrator's locks** (ADR 0067 §1 and §4): asked of every preset, host, grant and
/// opt-out before it is allowed, and the strictest answer wins. [`Locks::none`] where there is
/// no policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locks {
    source: Option<Source>,
    /// The presets a project may turn on, where the policy fixes them.
    presets: Option<Vec<Preset>>,
    /// The hosts any level may add, where the policy fixes them.
    hosts: Option<Vec<Host>>,
    no_personal_hosts: bool,
    no_persona_hosts: bool,
    no_opt_out: bool,
    no_write_grants: bool,
    no_vault_grants: bool,
    /// The most each dispatch limit may be, where the policy says (#1440).
    dispatch: crate::dispatchlimits::Level,
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// A test's seam: the locks [`Locks::of`] answers, in place of this machine's file, which a
    /// test can neither write nor rely on. Only a test build has it (`test-support` is turned on
    /// from `[dev-dependencies]` alone), so no build anyone is given reads anything but the file.
    pub(crate) static TEST_LOCKS: std::cell::RefCell<Locks> =
        std::cell::RefCell::new(Locks::none());
}

/// **A test's locks** for this thread, in place of this machine's file: what [`Locks::of`]
/// answers until another call. Only in a test build.
#[cfg(any(test, feature = "test-support"))]
pub fn set_for_this_test(locks: Locks) {
    TEST_LOCKS.with(|held| *held.borrow_mut() = locks);
}

impl Locks {
    /// No locks: no policy.
    pub fn none() -> Self {
        Self::default()
    }

    /// **The locks in force for the project at `root` on this machine**: this machine's policy
    /// file's ([`MACHINE_FILE`]). No project setting moves where it is read from, and nothing
    /// in the environment does either: one a chat could set would be one a chat could point at
    /// a file of its own.
    pub fn of(_root: &Path) -> Self {
        #[cfg(any(test, feature = "test-support"))]
        {
            TEST_LOCKS.with(|locks| locks.borrow().clone())
        }
        #[cfg(not(any(test, feature = "test-support")))]
        {
            Self::read(Path::new(MACHINE_FILE))
        }
    }

    /// The policy file at `path`: none where there is no file, its locks where it is one purlis
    /// trusts and reads, and every lock set where it is there and refused.
    pub fn read(path: &Path) -> Self {
        match read_trusted(path) {
            Ok(None) => Self::none(),
            Ok(Some(text)) => Self::parse(&text, path),
            Err(why) => Self::refused(path, &why),
        }
    }

    /// The policy `text`, read from `file`: its locks, or every lock where it says anything
    /// this version does not take.
    pub fn parse(text: &str, file: &Path) -> Self {
        parsed(text, file).unwrap_or_else(|why| Self::refused(file, &why))
    }

    /// Every lock set, for a file refused for `why`: purlis cannot tell what it meant. The
    /// presets stay the project's (D-1343-3): fixing them to none would leave no chat a way to
    /// its model, and whoever could make the file untrusted is an administrator already.
    fn refused(file: &Path, why: &str) -> Self {
        Self {
            source: Some(Source {
                file: file.to_path_buf(),
                owner: None,
                refused: Some(why.to_owned()),
            }),
            presets: None,
            hosts: Some(Vec::new()),
            no_personal_hosts: true,
            no_persona_hosts: true,
            no_opt_out: true,
            no_write_grants: true,
            no_vault_grants: true,
            dispatch: crate::dispatchlimits::ceiling_when_refused(),
        }
    }

    /// Whether any policy is in force.
    pub fn any(&self) -> bool {
        self.source.is_some()
    }

    /// Who set the policy: its `owner`, or this machine's administrator.
    pub fn owner(&self) -> String {
        self.source
            .as_ref()
            .and_then(|source| source.owner.clone())
            .unwrap_or_else(|| NOBODY_NAMED.to_owned())
    }

    /// The file the policy was read from.
    pub fn file(&self) -> Option<&Path> {
        self.source.as_ref().map(|source| source.file.as_path())
    }

    /// Why the policy file was refused, where it was.
    pub fn refused_because(&self) -> Option<&str> {
        self.source
            .as_ref()
            .and_then(|source| source.refused.as_deref())
    }

    /// `lead`, then who set the policy and in which file; or, for a refused file, why it is
    /// refused and that everything it could lock is locked.
    fn led_by(&self, lead: &str) -> String {
        let file = self.file().map_or_else(
            || MACHINE_FILE.to_owned(),
            |file| file.display().to_string(),
        );
        match self.refused_because() {
            Some(why) => format!(
                "{lead}: this machine's policy file, {file}, is refused ({why}), so everything \
                 it could lock is locked until {NOBODY_NAMED} fixes it."
            ),
            None => format!("{lead}, set by {} in {file}.", self.owner()),
        }
    }

    /// **"Locked by policy", and who set it**: the words every locked value is shown with.
    pub fn locked_by(&self) -> String {
        self.led_by("Locked by policy")
    }

    /// **"On, required by policy", and who set it**: what Settings says of the mode where
    /// policy requires the sandbox ([`Self::forbids_opt_out`]); none where it does not.
    pub fn required_by(&self) -> Option<String> {
        self.forbids_opt_out()
            .then(|| self.led_by("On, required by policy"))
    }

    /// The presets of `asked` the policy lets a project turn on, in `asked`'s order.
    pub fn presets(&self, asked: &[Preset]) -> Vec<Preset> {
        asked
            .iter()
            .copied()
            .filter(|preset| self.allows_preset(*preset))
            .collect()
    }

    /// Whether the policy lets a project turn `preset` on.
    pub fn allows_preset(&self, preset: Preset) -> bool {
        self.presets
            .as_ref()
            .is_none_or(|allowed| allowed.contains(&preset))
    }

    /// Whether the policy fixes the presets.
    pub fn fixes_presets(&self) -> bool {
        self.presets.is_some()
    }

    /// The hosts the policy lets any level add, where it fixes them.
    pub fn allowed_hosts(&self) -> Option<&[Host]> {
        self.hosts.as_deref()
    }

    /// Whether policy forbids your own hosts on this machine.
    pub fn forbids_personal_hosts(&self) -> bool {
        self.no_personal_hosts
    }

    /// Whether policy forbids a persona's own hosts.
    pub fn forbids_persona_hosts(&self) -> bool {
        self.no_persona_hosts
    }

    /// Whether policy forbids starting a chat without the sandbox, **and so requires the
    /// sandbox** (D-1423-1): in every project on this machine, whatever the project's own file
    /// says ([`super::Plane::in_force`]).
    pub fn forbids_opt_out(&self) -> bool {
        self.no_opt_out
    }

    /// **The most each dispatch limit may be** (#1440): a ceiling over the project's, a
    /// workspace's, a persona's and your own. A limit it does not name is not capped.
    pub fn dispatch_ceiling(&self) -> &crate::dispatchlimits::Level {
        &self.dispatch
    }

    /// Whether policy forbids every folder a grant would let a chat write.
    pub fn forbids_write_grants(&self) -> bool {
        self.no_write_grants
    }

    /// **Why `granted` is locked out**, if it is: a level policy forbids, or a host it does not
    /// allow.
    pub fn refuses(&self, granted: &Granted) -> Option<String> {
        let host = &granted.host;
        let why = match granted.level {
            Level::Persona if self.no_persona_hosts => Some(format!(
                "{host} is a persona's host, and policy forbids a persona's own hosts."
            )),
            Level::You if self.no_personal_hosts => Some(format!(
                "{host} is a host of yours, and policy forbids hosts of your own."
            )),
            Level::Project | Level::You | Level::Persona => None,
        }
        .or_else(|| {
            self.hosts
                .as_ref()
                .filter(|allowed| !allowed.iter().any(|one| one.covers(host)))
                .map(|_| format!("{host} is not a host policy allows."))
        })?;
        Some(format!("{why} {}", self.locked_by()))
    }

    /// **Why a grant of `what` at `level` is locked out**, if it is: a host as
    /// [`Self::refuses`] judges it at the level it would be kept at, and a folder where policy
    /// forbids write grants.
    pub fn refuses_grant(&self, what: &grant::What, level: grant::Level) -> Option<String> {
        match what {
            grant::What::Host(host) => self.refuses(&Granted {
                host: host.clone(),
                level: level.hosts_level(),
            }),
            grant::What::Write(_) => self.write_grants_refused(),
        }
    }

    /// Why no folder may be granted, where policy forbids it.
    pub fn write_grants_refused(&self) -> Option<String> {
        self.no_write_grants.then(|| {
            format!(
                "Policy forbids allowing a chat to write a folder. {}",
                self.locked_by()
            )
        })
    }

    /// Whether policy forbids letting a persona's chats use a vault it is not tagged for.
    pub fn forbids_vault_grants(&self) -> bool {
        self.no_vault_grants
    }

    /// Why no persona may be allowed a vault the registry does not tag for it, where policy
    /// forbids it (#1430).
    pub fn vault_grants_refused(&self) -> Option<String> {
        self.no_vault_grants.then(|| {
            format!(
                "Policy forbids allowing a persona a vault it is not tagged for. {}",
                self.locked_by()
            )
        })
    }

    /// Why no chat may start without the sandbox, where policy forbids it.
    pub fn opt_out_refused(&self) -> Option<String> {
        self.no_opt_out.then(|| {
            format!(
                "Policy forbids starting a chat without the sandbox. {}",
                self.locked_by()
            )
        })
    }
}

/// The keys `sandbox` may hold, each a lock.
const KEYS: [&str; 7] = [
    "presets",
    "hosts",
    "personal-hosts",
    "persona-hosts",
    "opt-out",
    "write-grants",
    "vault-grants",
];

/// `text` as a policy read from `file`, or why it is refused.
fn parsed(text: &str, file: &Path) -> Result<Locks, String> {
    let top: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "it is not JSON".to_owned())?;
    let top = top
        .as_object()
        .ok_or_else(|| "it is not a JSON object".to_owned())?;
    if let Some(key) = top
        .keys()
        .find(|key| !["owner", "sandbox", "dispatch"].contains(&key.as_str()))
    {
        return Err(format!(
            "it says \"{}\", which purlis does not know",
            clip(key)
        ));
    }
    let owner = match top.get("owner") {
        None => None,
        Some(serde_json::Value::String(owner)) => {
            Some(crate::shown::one_line(owner.trim(), MOST_OWNER_CHARS)).filter(|o| !o.is_empty())
        }
        Some(_) => return Err("its \"owner\" is not text".to_owned()),
    };
    let empty = serde_json::Map::new();
    let sandbox = match top.get("sandbox") {
        None => &empty,
        Some(serde_json::Value::Object(sandbox)) => sandbox,
        Some(_) => return Err("its \"sandbox\" is not an object".to_owned()),
    };
    let dispatch = match top.get("dispatch") {
        None => crate::dispatchlimits::Level::unset(),
        Some(serde_json::Value::Object(dispatch)) => crate::dispatchlimits::ceiling(dispatch)?,
        Some(_) => return Err("its \"dispatch\" is not an object".to_owned()),
    };
    if let Some(key) = sandbox.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!(
            "its sandbox says \"{}\", which purlis does not know",
            clip(key)
        ));
    }
    let words = |key: &str| -> Result<Option<Vec<String>>, String> {
        match sandbox.get(key) {
            None => Ok(None),
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| format!("its \"{key}\" holds something that is not text"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(format!("its \"{key}\" is not a list")),
        }
    };
    let forbids = |key: &str| -> Result<bool, String> {
        match sandbox.get(key) {
            None => Ok(false),
            Some(serde_json::Value::Bool(allowed)) => Ok(!allowed),
            Some(_) => Err(format!("its \"{key}\" is not true or false")),
        }
    };
    let presets = words("presets")?
        .map(|words| {
            words
                .iter()
                .map(|word| {
                    Preset::of_word(word).ok_or_else(|| {
                        format!(
                            "its presets name \"{}\", which is not one of {}",
                            clip(word),
                            Preset::listed()
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let hosts = words("hosts")?
        .map(|words| {
            words
                .iter()
                .map(|word| {
                    Host::parse(word)
                        .map_err(|why| format!("its hosts name \"{}\": {why}", clip(word)))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    Ok(Locks {
        source: Some(Source {
            file: file.to_path_buf(),
            owner,
            refused: None,
        }),
        presets,
        hosts,
        no_personal_hosts: forbids("personal-hosts")?,
        no_persona_hosts: forbids("persona-hosts")?,
        no_opt_out: forbids("opt-out")?,
        no_write_grants: forbids("write-grants")?,
        no_vault_grants: forbids("vault-grants")?,
        dispatch,
    })
}

/// A word of the file as a refusal quotes it: one line, short.
fn clip(word: &str) -> String {
    crate::shown::one_line(word, 40)
}

/// What [`judge`] is told of one file or folder, without following a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    /// Whether it is a link.
    pub link: bool,
    /// Whether it is a regular file (for the file) or a directory (for its folder).
    pub expected_type: bool,
    /// Its owner's user id.
    pub uid: u32,
    /// Its permission bits.
    pub mode: u32,
}

/// **Whether a policy file, as `file` and its folder `folder` are seen, is one purlis trusts**,
/// or why not: neither a link, each the type it should be, each owned by root (user id 0), and
/// neither writable by its group or anyone else. So no one but an administrator could have
/// written it, a chat included.
pub fn judge(file: Seen, folder: Seen) -> Result<(), String> {
    judge_folder(folder)?;
    if file.link {
        return Err("it is a link".to_owned());
    }
    if !file.expected_type {
        return Err("it is not a regular file".to_owned());
    }
    if file.uid != 0 {
        return Err("it is not owned by the system's administrator".to_owned());
    }
    if file.mode & 0o022 != 0 {
        return Err("it can be written by others than the administrator".to_owned());
    }
    Ok(())
}

/// **Whether the policy's folder, as seen, is one purlis trusts**, or why not: not a link, a
/// folder, owned by root and writable by no one else. Asked whenever the folder is there, file
/// or no file: whoever could write an untrusted folder could take the file out of it, and a
/// policy taken out that way is never read as no policy (D-1343-11).
pub fn judge_folder(folder: Seen) -> Result<(), String> {
    if folder.link {
        return Err("its folder is a link".to_owned());
    }
    if !folder.expected_type {
        return Err("its folder is not a folder".to_owned());
    }
    if folder.uid != 0 {
        return Err("its folder is not owned by the system's administrator".to_owned());
    }
    if folder.mode & 0o022 != 0 {
        return Err("its folder can be written by others than the administrator".to_owned());
    }
    Ok(())
}

/// The text of the policy file at `path`, `None` where there is none, or why it is refused.
/// Neither the file nor its folder is followed through a link, the file is opened without
/// following one, and what was opened is judged again before it is read.
#[cfg(unix)]
fn read_trusted(path: &Path) -> Result<Option<String>, String> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let seen = |meta: &std::fs::Metadata, dir: bool| Seen {
        link: meta.file_type().is_symlink(),
        expected_type: if dir {
            meta.file_type().is_dir()
        } else {
            meta.file_type().is_file()
        },
        uid: meta.uid(),
        mode: meta.mode(),
    };
    // The folder first, whenever it is there: no folder is no policy, and an untrusted one is
    // refused whether or not the file is in it (D-1343-11).
    let dir = path.parent().ok_or_else(|| "it has no folder".to_owned())?;
    let folder = match std::fs::symlink_metadata(dir) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("its folder cannot be read: {err}")),
        Ok(meta) => meta,
    };
    judge_folder(seen(&folder, true))?;
    let file = match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("it cannot be read: {err}")),
        Ok(meta) => meta,
    };
    judge(seen(&file, false), seen(&folder, true))?;
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|err| format!("it cannot be opened: {err}"))?;
    let now = opened
        .metadata()
        .map_err(|err| format!("it cannot be read: {err}"))?;
    if now.ino() != file.ino() || now.dev() != file.dev() {
        return Err("it changed while it was read".to_owned());
    }
    judge(seen(&now, false), seen(&folder, true))?;
    let mut bytes = Vec::new();
    opened
        .take(MOST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("it cannot be read: {err}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MOST_BYTES {
        return Err("it is larger than purlis reads".to_owned());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "it is not UTF-8 text".to_owned())
}

/// No policy file is read off Unix yet: a Windows policy key is one of C9's layers no ADR
/// places (D-1343-1).
#[cfg(not(unix))]
fn read_trusted(_path: &Path) -> Result<Option<String>, String> {
    Ok(None)
}

/// The folder [`MACHINE_FILE`] is in: every chat is denied writing it.
pub fn machine_folder() -> PathBuf {
    Path::new(MACHINE_FILE)
        .parent()
        .map_or_else(|| PathBuf::from(MACHINE_FILE), Path::to_path_buf)
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
