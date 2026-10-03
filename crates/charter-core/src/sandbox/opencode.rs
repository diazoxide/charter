//! The policy compiled for opencode: a Seatbelt profile charter writes, which the whole harness
//! runs inside (ADR 0067 §2). opencode has no sandbox of its own to hand the policy to.
//!
//! The profile is charter's own, generated here in Rust. Charter does not ship
//! `sandbox-runtime`. Measured against opencode 1.18.33 on macOS, with `sandbox-exec`, real
//! `opencode run` turns and its TUI, and a stand-in model server whose answer ran one shell
//! command through opencode's own tool:
//!
//! - **Closed by default.** The profile starts from `(deny default)` and allows what a harness
//!   and the commands it runs need: running programs, reading files, the terminal, the user
//!   lookups, preferences and file events the system libraries ask for. Asking another process
//!   to act, such as `launchctl submit`, `open` or an Apple event, was refused (measured), so a
//!   command cannot start a program outside the wrap.
//! - **Writes** (rulings V73 and V73a, ADR 0067 §2 as amended). The chat's directory and its
//!   own temp directory, and of opencode's own files only what a turn writes (measured on a
//!   fresh data directory): its sessions database and what is in its log and storage
//!   directories. Writing elsewhere was refused (measured). opencode's own directories are made
//!   by charter before the wrap ([`prepare`]), and the chat may neither make, move nor replace
//!   one: moving the data directory out, changing it and moving it back was measured to work
//!   while the directory itself was writable. Its state (locks it makes with `mkdir`, its last
//!   model) goes to a state directory of the chat's own ([`STATE_ENV`]), never opencode's.
//!   A chat whose directory holds opencode's directories, or is inside one, is not wrapped.
//! - **Nothing a later opencode loads.** opencode's config and cache hold plugins and
//!   packages, and its data directory holds its credentials (whose entries can name a remote
//!   config that starts MCP servers and plugins) and snapshot repositories that git is run in.
//!   So the config and cache are read-only, except the one `.gitignore` a first run writes and
//!   stops without; the credentials files are never written; and no snapshot repository is
//!   written, made or moved in. A path-by-path deny of a snapshot's `config` and `hooks` was
//!   measured not to be enough: a directory holding a `config` could still be moved into place
//!   whole, and opencode stops a turn when it cannot make a snapshot's directory. So a wrapped
//!   chat is started with opencode's snapshots off, which wins over every other config
//!   (measured), and its turns run without them (#1075). No link and no directory is made in
//!   the data directory, so none is moved in with links already in it, and no link is made in
//!   place of the `.gitignore`.
//! - **Each denial class** comes after the writes, because Seatbelt takes the last rule that
//!   matches. So a denied directory inside the chat's own is still denied. Reading a denied
//!   file was refused (measured). The later-code class ([`super::PLANTED`]) is denied at any
//!   depth of every directory the chat may write, by name, so a nested clone's `.git/config`
//!   is held, and so is a `.git` built in the temp directory to be moved in with its parent.
//! - **The network.** Only charter's egress proxy, on its loopback port, and the socket the
//!   chat's hooks report on can be reached. opencode's own requests to its model provider go
//!   through the proxy (measured: a provider on a listed host answered through it), and so
//!   do `curl` and `git` (a listed host answered, an unlisted one was refused by the proxy). A
//!   direct connection was refused, a name could not be resolved, and the agent socket of
//!   `ssh-agent` was refused (measured). The proxy holds the egress presets
//!   ([`super::egress`]).
//! - **The credential store.** No lookup of the security service is allowed, and the
//!   keychain files are denied. Under the profile, `security find-generic-password` could not
//!   reach an item it read outside it (measured). So the wrap holds the vaults class where a
//!   plane has a keyring vault, which no harness's own sandbox can.
//! - **Paths** are written as the kernel names them. `/tmp` and `/var` are links into
//!   `/private`, and a rule on the link's name would match nothing.

use std::path::{Path, PathBuf};

use super::{Access, Compiled, Denial, Os, PLANTED, Reach, Uncompilable, Unheld};
use crate::harness::Harness;

/// What an opencode chat is wrapped in, before the place it opens is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrap {
    /// Every path the chat is denied, with the keychain files among them.
    pub denied: Vec<Denial>,
    /// The hosts its proxy carries.
    pub hosts: Vec<String>,
    /// opencode's data directory, of which only what a turn writes is writable.
    pub data: Option<PathBuf>,
    /// opencode's state directory, which a wrapped chat never writes: it is started with a
    /// state directory of its own, in its temp directory ([`STATE_ENV`]).
    pub state: Option<PathBuf>,
    /// opencode's config directory, of which only its `.gitignore` is writable.
    pub config: Option<PathBuf>,
    /// opencode's cache directory, which is read-only.
    pub cache: Option<PathBuf>,
}

/// What a turn writes in opencode's data directory, below it, each and everything under it.
/// Not its snapshots: a sandboxed chat makes none ([`crate::opencode::session_config`]).
pub const DATA_WRITTEN: [&str; 2] = ["log", "storage"];

/// opencode's credentials files in its data directory, never written by a chat.
pub const CREDENTIALS: [&str; 2] = ["auth.json", "mcp-auth.json"];

/// `compiled`, for opencode, or why it cannot be wrapped on this system.
pub fn wrap(compiled: &Compiled) -> Result<Wrap, Uncompilable> {
    if compiled.os != Os::MacOs {
        return Err(Uncompilable {
            harness: Harness::Opencode,
            unheld: Unheld::Wrap(compiled.os),
        });
    }
    let mut denied = compiled.denied.paths.clone();
    if let Some(home) = &compiled.homes.home {
        denied.push(Denial {
            class: super::Class::Vaults,
            path: home.join("Library/Keychains"),
            access: Access::ReadWrite,
        });
    }
    let own = |dir: &Option<PathBuf>| dir.as_ref().map(|dir| dir.join("opencode"));
    Ok(Wrap {
        denied,
        hosts: compiled.hosts.clone(),
        data: own(&compiled.homes.data),
        state: own(&compiled.homes.state),
        config: own(&compiled.homes.config),
        cache: own(&compiled.homes.cache),
    })
}

/// The directories opencode makes on a first run, made before the wrap: the wrap lets a chat
/// make none of them, nor move one, since a directory made could be one moved into place with
/// what a later opencode loads already in it (measured: a home without them stops opencode at
/// its first `mkdir`). Empty, and only where missing. One that cannot be made is left to
/// opencode, which then stops: the wrap is not widened for it.
pub fn prepare(wrap: &Wrap) {
    let dirs = [
        wrap.data.clone(),
        wrap.data.as_ref().map(|data| data.join("repos")),
        wrap.data.as_ref().map(|data| data.join("log")),
        wrap.data.as_ref().map(|data| data.join("storage")),
        wrap.config.clone(),
        wrap.cache.clone(),
        wrap.cache.as_ref().map(|cache| cache.join("bin")),
    ];
    for dir in dirs.into_iter().flatten() {
        let _ = std::fs::create_dir_all(dir);
    }
}

/// The variable that gives a wrapped chat a state directory of its own, under its temp
/// directory: opencode's own is never written, so nothing a later opencode reads from it (its
/// locks among them) can be planted there.
pub const STATE_ENV: &str = "XDG_STATE_HOME";

/// Why a chat is not wrapped: its directory holds one of opencode's own, or is inside one, so
/// its directory's grant would be a grant on what a later opencode loads.
pub const OVERLAP: &str = "charter cannot wrap an opencode chat whose directory holds opencode's \
                           own files, or is inside them";

/// Why a profile could not be written: a path holding a control character, which a rule could
/// not state exactly.
pub const CONTROL: &str = "charter cannot write a sandbox profile for a path holding a control \
                           character";

/// `text` as an SBPL string, or [`CONTROL`].
fn string(text: &str) -> Result<String, &'static str> {
    if text.chars().any(char::is_control) {
        return Err(CONTROL);
    }
    Ok(format!(
        "\"{}\"",
        text.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// `path` as the kernel names it, as an SBPL string.
fn quote(path: &Path) -> Result<String, &'static str> {
    string(&real(path).display().to_string())
}

/// `text` with every regular-expression character escaped.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\.^$|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A regular expression for `below` and its contents under `dir`, both as the kernel names them.
fn under(dir: &Path, below: &str) -> String {
    format!("^{}/{below}", escaped(&real(dir).display().to_string()))
}

/// The rules that keep every [`PLANTED`] name, at any depth below `root`, from being written.
pub fn planted_rules(root: &Path) -> Result<Vec<String>, &'static str> {
    let root = escaped(&real(root).display().to_string());
    PLANTED
        .iter()
        .map(|planted| {
            let tail = match planted.reach {
                Reach::Itself => "$",
                Reach::AndBelow => "(/.*)?$",
            };
            let mut body = String::new();
            let parts: Vec<&str> = planted.path.split('/').collect();
            for (at, part) in parts.iter().enumerate() {
                if *part == "**" {
                    body.push_str("(.*/)?");
                    continue;
                }
                body.push_str(&escaped(part));
                if at + 1 < parts.len() {
                    body.push('/');
                }
            }
            let regex = format!("^{root}/(.*/)?{body}{tail}");
            Ok(format!("(deny file-write* (regex {}))", string(&regex)?))
        })
        .collect()
}

/// The Seatbelt profile for a chat in `cwd`, with its own temp directory `tmp`, reaching the
/// network through the proxy on `proxy_port` and reporting on `hook_socket`.
pub fn profile(
    wrap: &Wrap,
    cwd: &Path,
    tmp: &Path,
    proxy_port: u16,
    hook_socket: Option<&Path>,
) -> Result<String, &'static str> {
    let mut out = String::from(BASE);
    let mut line = |text: String| {
        out.push_str(&text);
        out.push('\n');
    };
    line(format!(
        "(allow network-outbound (remote ip \"localhost:{proxy_port}\"))"
    ));
    if let Some(socket) = hook_socket {
        line(format!(
            "(allow network-outbound (remote unix-socket (path-literal {})))",
            quote(socket)?
        ));
    }

    let cwd_real = real(cwd);
    for own in [&wrap.data, &wrap.state, &wrap.config, &wrap.cache]
        .into_iter()
        .flatten()
    {
        let own = real(own);
        if own.starts_with(&cwd_real) || cwd_real.starts_with(&own) {
            return Err(OVERLAP);
        }
    }

    // What the chat may write.
    let mut allow = vec![
        format!("(subpath {})", quote(cwd)?),
        format!("(subpath {})", quote(tmp)?),
    ];
    if let Some(data) = &wrap.data {
        allow.push(format!(
            "(regex {})",
            string(&under(data, "opencode\\.db(-wal|-shm|-journal)?$"))?
        ));
        // What is in each, never the directory itself, which charter made ([`prepare`]): so
        // none is moved out, replaced or moved in whole.
        for written in DATA_WRITTEN {
            allow.push(format!(
                "(regex {})",
                string(&under(data, &format!("{written}/.+")))?
            ));
        }
    }
    if let Some(config) = &wrap.config {
        allow.push(format!("(literal {})", quote(&config.join(".gitignore"))?));
    }
    line("(allow file-write*".to_owned());
    for rule in allow {
        line(format!("  {rule}"));
    }
    out.push_str(DEVICES);
    out.push_str(")\n");

    // What it may never write, after what it may: Seatbelt takes the last rule that matches.
    let mut line = |text: String| {
        out.push_str(&text);
        out.push('\n');
    };
    for denial in &wrap.denied {
        let what = match denial.access {
            Access::ReadWrite => "file-read* file-write*",
            Access::Write => "file-write*",
        };
        line(format!("(deny {what} (subpath {}))", quote(&denial.path)?));
    }
    // Each directory between the chat's own and a denied path, as an entry only, so it is never
    // moved away with the denied path in it, nor replaced (measured: a plane-root chat could
    // otherwise move `.charter` aside and write `.charter/app` under its new name).
    let mut pinned = std::collections::BTreeSet::new();
    // The chat's own directory and its temp directory too, as entries: moved whole into the
    // other, every path rule under the one moved would no longer match (measured).
    for root in [&cwd_real, &real(tmp)] {
        if pinned.insert(root.clone()) {
            line(format!("(deny file-write* (literal {}))", quote(root)?));
        }
    }
    for denial in &wrap.denied {
        for ancestor in super::ancestors_within(&real(&denial.path), &cwd_real) {
            if pinned.insert(ancestor.clone()) {
                line(format!(
                    "(deny file-write* (literal {}))",
                    quote(&ancestor)?
                ));
            }
        }
    }
    // On every directory the chat may write, so a protected name made in one cannot be moved
    // into another with its parent.
    let mut roots = vec![cwd.to_path_buf(), tmp.to_path_buf()];
    if let Some(data) = &wrap.data {
        roots.extend(DATA_WRITTEN.iter().map(|written| data.join(written)));
    }
    for root in &roots {
        for rule in planted_rules(root)? {
            line(rule);
        }
        // No directory made under `.git/modules`, so a submodule's git directory, config and
        // hooks inside, is never moved in whole.
        line(format!(
            "(deny file-write-create (require-all (vnode-type DIRECTORY) (regex {})))",
            string(&format!(
                "^{}/(.*/)?\\.git/modules/.+",
                escaped(&real(root).display().to_string())
            ))?
        ));
    }
    if let Some(data) = &wrap.data {
        for file in CREDENTIALS {
            line(format!(
                "(deny file-write* (literal {}))",
                quote(&data.join(file))?
            ));
        }
        // Every snapshot repository, its config and hooks among them, which git reads and runs
        // when opencode later runs it there: none is written, moved in or made.
        line(format!(
            "(deny file-write* (subpath {}))",
            quote(&data.join("snapshot"))?
        ));
    }
    // No link where a later opencode writes, which it would write through, and no directory,
    // which could be one moved in with links already in it.
    if let Some(data) = &wrap.data {
        for kind in ["SYMLINK", "DIRECTORY"] {
            line(format!(
                "(deny file-write-create (require-all (vnode-type {kind}) (subpath {})))",
                quote(data)?
            ));
        }
    }
    if let Some(config) = &wrap.config {
        line(format!(
            "(deny file-write-create (require-all (vnode-type SYMLINK) (literal {})))",
            quote(&config.join(".gitignore"))?
        ));
    }
    Ok(out)
}

/// What every wrapped chat is allowed, whatever its plane: each line was needed by opencode or
/// by a command it ran (measured), and nothing here reaches the network or writes a file.
const BASE: &str = "(version 1)
(deny default)
(allow process-exec)
(allow process-fork)
(allow signal (target same-sandbox))
(allow process-info* (target same-sandbox))
(allow sysctl-read)
(allow file-read*)
(allow file-ioctl (literal \"/dev/ptmx\") (literal \"/dev/tty\") (regex #\"^/dev/ttys[0-9]+$\"))
(allow pseudo-tty)
(allow ipc-posix-shm-read* (ipc-posix-name-prefix \"apple.cfprefs.\"))
(allow ipc-posix-sem)
(allow user-preference-read)
(allow mach-lookup
  (global-name \"com.apple.system.opendirectoryd.libinfo\")
  (global-name \"com.apple.cfprefsd.daemon\")
  (global-name \"com.apple.cfprefsd.agent\")
  (global-name \"com.apple.logd\")
  (global-name \"com.apple.FSEvents\"))
(allow system-socket)
";

/// The devices a terminal program writes, inside the `file-write*` allow.
const DEVICES: &str = "  (literal \"/dev/null\")
  (literal \"/dev/tty\")
  (literal \"/dev/ptmx\")
  (regex #\"^/dev/ttys[0-9]+$\")";

/// `path` as the kernel names it: its longest part that exists, with its links resolved, and
/// the rest as written.
fn real(path: &Path) -> PathBuf {
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

/// The variables that point a wrapped chat's traffic at charter's egress proxy. Both cases,
/// because programs disagree on which they read.
pub const PROXY_ENV: [&str; 6] = [
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "ALL_PROXY",
    "https_proxy",
    "http_proxy",
    "all_proxy",
];

/// The variables that would send a host past the proxy, emptied. The profile refuses such a
/// connection anyway, and this way it is refused by the proxy, which says why.
pub const NO_PROXY_ENV: [&str; 2] = ["NO_PROXY", "no_proxy"];
