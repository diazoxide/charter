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

use super::seatbelt::{self, Own, quote, string, under};
use super::{Compiled, Denial, Os, Uncompilable, Unheld};
use crate::harness::Harness;

/// What an opencode chat is wrapped in, before the place it opens is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrap {
    /// Every path the chat is denied, with the keychain files among them.
    pub denied: Vec<Denial>,
    /// The hosts its proxy carries.
    pub hosts: Vec<String>,
    /// What its presets widen past the hosts: the package caches and the certificate check.
    pub widened: Box<super::Widened>,
    /// The folders a person let this chat write besides its own (#1342), each a root the
    /// later-code names are denied in, and each under every denial that follows it.
    pub granted: Vec<PathBuf>,
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
    denied.extend(seatbelt::keychains(compiled.homes.home.as_deref()));
    let own = |dir: &Option<PathBuf>| dir.as_ref().map(|dir| dir.join("opencode"));
    Ok(Wrap {
        denied,
        hosts: compiled.hosts.clone(),
        widened: Box::new(compiled.widened.clone()),
        granted: compiled.writable.clone(),
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
pub const OVERLAP: &str = "purlis cannot wrap an opencode chat whose directory holds opencode's \
                           own files, or is inside them";

/// The Seatbelt profile for a chat in `cwd`, with its own temp directory `tmp`, reaching the
/// network through its own proxy's `proxy_ports` and reporting on `hook_socket`.
pub fn profile(
    wrap: &Wrap,
    cwd: &Path,
    tmp: &Path,
    proxy_ports: &[u16],
    hook_socket: Option<&Path>,
) -> Result<String, &'static str> {
    seatbelt::profile(
        &wrap.denied,
        &own(wrap)?,
        cwd,
        tmp,
        proxy_ports,
        hook_socket,
    )
}

/// What a wrapped opencode keeps for itself.
fn own(wrap: &Wrap) -> Result<Own, &'static str> {
    let mut own = Own {
        dirs: [&wrap.data, &wrap.state, &wrap.config, &wrap.cache]
            .into_iter()
            .flatten()
            .cloned()
            .collect(),
        overlap: OVERLAP,
        ..Own::default()
    };
    seatbelt::widen(&mut own, &wrap.widened)?;
    seatbelt::granted(&mut own, &wrap.granted)?;
    if let Some(data) = &wrap.data {
        own.allow.push(format!(
            "(regex {})",
            string(&under(data, "opencode\\.db(-wal|-shm|-journal)?$"))?
        ));
        // What is in each, never the directory itself, which charter made ([`prepare`]): so
        // none is moved out, replaced or moved in whole.
        for written in DATA_WRITTEN {
            own.allow.push(format!(
                "(regex {})",
                string(&under(data, &format!("{written}/.+")))?
            ));
        }
        own.roots
            .extend(DATA_WRITTEN.iter().map(|written| data.join(written)));
    }
    if let Some(config) = &wrap.config {
        own.allow
            .push(format!("(literal {})", quote(&config.join(".gitignore"))?));
    }
    if let Some(data) = &wrap.data {
        for file in CREDENTIALS {
            own.deny.push(format!(
                "(deny file-write* (literal {}))",
                quote(&data.join(file))?
            ));
        }
        // Every snapshot repository, its config and hooks among them, which git reads and runs
        // when opencode later runs it there: none is written, moved in or made.
        own.deny.push(format!(
            "(deny file-write* (subpath {}))",
            quote(&data.join("snapshot"))?
        ));
        // No link where a later opencode writes, which it would write through, and no
        // directory, which could be one moved in with links already in it.
        for kind in ["SYMLINK", "DIRECTORY"] {
            own.deny.push(format!(
                "(deny file-write-create (require-all (vnode-type {kind}) (subpath {})))",
                quote(data)?
            ));
        }
    }
    if let Some(config) = &wrap.config {
        own.deny.push(format!(
            "(deny file-write-create (require-all (vnode-type SYMLINK) (literal {})))",
            quote(&config.join(".gitignore"))?
        ));
    }
    Ok(own)
}
