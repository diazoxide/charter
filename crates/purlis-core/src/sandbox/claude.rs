//! The policy compiled for Claude Code: a `sandbox` object and `permissions.deny` rules, in
//! the `--settings` charter already hands each chat ([`crate::harness`]).
//!
//! Read out of Claude Code 2.1.285's settings schema, and measured live against it on macOS:
//!
//! - `sandbox` confines the commands its Bash tool runs. Its Read and Edit tools run in its
//!   own process, and a `permissions.deny` rule is what holds them, so every denied path is
//!   written twice: once for the sandbox and once for the tools.
//! - Its web tools run in its own process too, and are not held to the sandbox's allowed hosts
//!   (measured: a fetch of an unlisted host succeeded under `strictAllowlist`). No setting
//!   limits them to a list of hosts, and an allow cannot outrank a deny, so both are denied.
//! - `--settings` sits above project settings, and a project file is not honoured on the keys
//!   that loosen a sandbox.
//! - Where a class is held by a service no key in its settings can deny, and the service does not
//!   hold it either, the chat is refused ([`super::NotStarted::Uncompilable`]) rather than
//!   started without it. Its sandbox lets a command ask the keychain's service (its profile
//!   allows the lookup), so on macOS the vaults class rests on the keyring items' own access
//!   rule (ruling V90a): a command that reads one is refused or the person is asked.
//!
//! Measured live on 2.1.285 on macOS, a headless chat started with what [`settings`] writes: a
//! command could neither read nor write a denied directory, could write its own directory,
//! reached a listed host, and was refused an unlisted one; and with the web tools denied they
//! were not offered to the model at all, even when named in `--allowedTools`.
//!
//! **The later-code class** (ruling V73b). Measured on 2.1.288 on macOS, a headless turn against
//! a stand-in model server: Claude Code's own sandbox already kept a command from writing git's
//! config and hooks (in a nested clone too), `.mcp.json`, `.vscode`, `.idea`, `.claude`'s
//! settings, commands and agents, and shell startup files, and let it write `opencode.json`,
//! `.opencode`, `.codex`, `.envrc` and `charter.toml`. With each name added to `denyWrite` as
//! `**/<name>`, every one was refused at any depth, and an ordinary file was still written.
//! `.claude/skills`, `tui.json`, `tui.jsonc` and `.agents`, added later (#1057), are held by
//! the same `**/<name>` rule. The project's manifests are not (#1336): they are denied by path,
//! at the project root and in each folder from the chat's up to it ([`Settings::denying`]).
//! What it does not hold: a command moved a directory holding a `config` into a nested
//! clone's `.git` (measured), and no glob can deny the `.git` itself without denying what git
//! writes below it (#1065).
//!
//! **The hook socket** (ADR 0067 §2, #1328). A `purlis` command a chat runs is part of the chat
//! and asks the app over the socket in `$PURLIS_HOOK_SOCKET`; Claude Code's sandbox refuses
//! every unix socket it is not told to allow. The socket is known only where the chat opens, so
//! [`Settings::reporting_on`] adds it then, as the one path in `network.allowUnixSockets`.
//! Read out of Claude Code 2.1.291's own settings schema: `allowUnixSockets` is a list of
//! paths, *"macOS only … Ignored on Linux (seccomp cannot filter by path)"*, merged across
//! settings sources, and its macOS profile allows each as a `subpath` for connect **and bind**.
//! The bind is harmless only while the chat cannot replace the socket, so the socket's folder is
//! denied to writes beside it: under the project's state folder that is denied already, and the
//! fallback folder in the temp directory is not. `allowAllUnixSockets` is set `false`, never
//! left out, so a user's `true` cannot merge in. On Linux a sandboxed chat reaches no socket.
//!
//! **What a project's sandbox widens** (spec #1330, #1337). Read out of Claude Code 2.1.291's
//! own settings schema and its profile builder:
//! - `filesystem.allowWrite` is *"Additional paths to allow writing within the sandbox"*,
//!   absolute or `~`-expanded, a glob compiled to a regular expression. It carries the project's
//!   package caches ([`super::Widened::caches`]): each tool's folder and cargo's few files by
//!   path, and inside each of cargo's bare repositories by `<db>/*/**`, which never matches an
//!   entry itself. Each path by both names where a link stands in between; left out when there
//!   are none. Every `denyWrite` still wins inside them.
//! - Each folder purlis made is pinned: Claude Code denies `file-write-unlink` and
//!   `file-write-create` on every folder above a `denyWrite` entry, so a `.git` denied at each
//!   cache folder's top pins that folder, and a bare repository's `config` and `hooks` denied
//!   pin cargo's git database.
//! - `enableWeakerNetworkIsolation` is *"macOS only: Allow access to com.apple.trustd.agent in
//!   the sandbox. Needed for Go-based CLI tools (gh, gcloud, terraform, etc.) to verify TLS
//!   certificates"*; its profile adds exactly `(allow mach-lookup (global-name
//!   "com.apple.trustd.agent"))`. It is `true` only where the project's `certificate-checks` is
//!   (D-1337-7), and `false` otherwise, never left out, so a user's `true` does not merge in.

use std::path::Path;

use serde_json::{Value, json};

use super::{Access, Compiled, Uncompilable};
use crate::harness::Harness;

/// `path` as written, and as the kernel names it where that differs.
fn both_names(path: &Path) -> Vec<std::path::PathBuf> {
    let resolved = super::real(path);
    if resolved == path {
        vec![path.to_path_buf()]
    } else {
        vec![path.to_path_buf(), resolved]
    }
}

/// Claude Code's own tools that reach the network from its process rather than through its
/// sandbox, so the sandbox's allowed hosts do not hold them.
pub const WEB_TOOLS: [&str; 2] = ["WebFetch", "WebSearch"];

/// What a Claude Code chat's `--settings` carries for the sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The `sandbox` object.
    pub sandbox: Value,
    /// `permissions.deny` rules for its own tools.
    pub deny: Vec<String>,
}

/// `compiled`, for Claude Code — or the class it cannot hold on this machine.
///
/// The credential store is held by the store on macOS (ruling V90a) and refused elsewhere:
/// Claude Code's settings have no key that denies it (`Compiled::holds_every_service`).
///
/// No denial reaching here covers `/`, the home directory, the project or a folder above it:
/// [`super::for_start`] refuses that chat, and [`super::Applied::line`] refuses one whose own
/// folder a denial covers ([`super::covering`], #1327).
pub fn settings(compiled: &Compiled) -> Result<Settings, Uncompilable> {
    compiled.holds_every_service(Harness::ClaudeCode)?;
    let mut deny_read = Vec::new();
    let mut deny_write = Vec::new();
    let mut read_rules = Vec::new();
    let mut edit_rules = Vec::new();
    for (denial, path) in names_of(&compiled.denied.paths) {
        let path = path.display().to_string();
        let rooted = format!("/{path}");
        if denial.access == Access::ReadWrite {
            deny_read.push(path.clone());
            read_rules.push(format!("Read({rooted})"));
            read_rules.push(format!("Read({rooted}/**)"));
        }
        deny_write.push(path);
        edit_rules.push(format!("Edit({rooted})"));
        edit_rules.push(format!("Edit({rooted}/**)"));
    }
    // The later-code class (ruling V73b), by name at any depth: added to the names Claude
    // Code's own sandbox already keeps commands from writing, never in their place. A name
    // that holds only itself (`.git`) is left out: a glob here denies what is below it too,
    // and git writes there.
    for planted in super::PLANTED
        .iter()
        .filter(|planted| planted.reach == super::Reach::AndBelow)
    {
        let glob = format!("**/{}", planted.path);
        deny_write.push(glob.clone());
        edit_rules.push(format!("Edit({glob})"));
        edit_rules.push(format!("Edit({glob}/**)"));
    }
    // Under each granted folder (#1342), every later-code name again, by absolute path: a
    // relative `**/<name>` is read against the chat's folder, so it is not known to hold in a
    // folder outside it (D-1342-3). The folder is the kernel's spelling already (D-1342-12).
    for folder in &compiled.writable {
        for planted in super::PLANTED
            .iter()
            .filter(|planted| planted.reach == super::Reach::AndBelow)
        {
            let glob = format!("{}/**/{}", folder.display(), planted.path);
            edit_rules.push(format!("Edit(/{glob})"));
            edit_rules.push(format!("Edit(/{glob}/**)"));
            deny_write.push(glob);
        }
    }
    // The project's package caches (D-1337-6), each path as written and as the kernel names it.
    let mut allow_write: Vec<String> = Vec::new();
    if let Some(caches) = &compiled.widened.caches {
        let mut allow = |path: String| {
            if !allow_write.contains(&path) {
                allow_write.push(path);
            }
        };
        for path in caches.trees.iter().chain(&caches.files) {
            for name in both_names(path) {
                allow(name.display().to_string());
            }
        }
        // Inside each bare repository, never an entry itself: a glob is a regular expression
        // in Claude Code's profile, and `*/**` matches nothing at the first level.
        for bare in &caches.bare {
            for name in both_names(bare) {
                allow(format!("{}/*/**", name.display()));
            }
        }
        // Claude Code pins every folder above a denied path, so it is not removed, renamed or
        // made (measured in 2.1.291's profile builder: each ancestor of a `denyWrite` entry is
        // denied `file-write-unlink` and `file-write-create` as a literal). A repository's
        // `.git` at a tree's top is denied for that pin, and what a later git runs in each
        // bare repository (ADR 0067 §5) pins the database.
        let mut deny = |path: String| {
            deny_write.push(path.clone());
            edit_rules.push(format!("Edit(/{path})"));
            edit_rules.push(format!("Edit(/{path}/**)"));
        };
        for tree in &caches.trees {
            for name in both_names(tree) {
                deny(format!("{}/.git", name.display()));
            }
        }
        for bare in &caches.bare {
            for name in both_names(bare) {
                for run in super::caches::BARE_RUN {
                    deny(format!("{}/*/{run}", name.display()));
                }
            }
        }
    }
    // A person's grants (#1342), each exactly as judged: already the kernel's spelling
    // (D-1342-12), never resolved again here, so a folder swapped for a link since is not
    // followed to where it now points. Claude Code holds `denyWrite` over `allowWrite`, so a
    // class inside one still wins.
    for path in &compiled.writable {
        let path = path.display().to_string();
        if !allow_write.contains(&path) {
            allow_write.push(path);
        }
    }
    let mut filesystem = json!({
        "denyRead": deny_read,
        "denyWrite": deny_write,
    });
    if !allow_write.is_empty() {
        filesystem["allowWrite"] = json!(allow_write);
    }
    let deny = WEB_TOOLS
        .iter()
        .map(|tool| (*tool).to_owned())
        .chain(read_rules)
        .chain(edit_rules)
        .collect();
    let sandbox = json!({
            "enabled": true,
            "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "enableWeakerNetworkIsolation": compiled.widened.trust,
            "network": {
                "allowedDomains": compiled.hosts,
                "strictAllowlist": true,
                "allowAllUnixSockets": false,
            },
            "filesystem": filesystem,
    });
    Ok(Settings { sandbox, deny })
}

impl Settings {
    /// These settings for a chat whose hooks and `purlis` commands report on `socket`: as
    /// compiled, with that one socket, as the kernel names it, as the only path in
    /// `network.allowUnixSockets` (ADR 0067 §2: the chat may connect to that socket and
    /// nothing more), and its folder denied to writes, to the sandbox and to the Edit tool, so
    /// the bind Claude Code grants with it can never take the app's place. Without a socket it
    /// is the compiled settings, which allow none.
    pub fn reporting_on(&self, socket: Option<&Path>) -> Settings {
        let mut out = self.clone();
        let Some(socket) = socket else {
            return out;
        };
        let socket = super::real(socket);
        out.sandbox["network"]["allowUnixSockets"] = json!([socket.display().to_string()]);
        if let Some(folder) = socket.parent() {
            let folder = folder.display().to_string();
            if let Some(denied) = out.sandbox["filesystem"]["denyWrite"].as_array_mut() {
                denied.push(json!(folder));
            }
            out.deny.push(format!("Edit(/{folder})"));
            out.deny.push(format!("Edit(/{folder}/**)"));
        }
        out
    }

    /// These settings with `denied` denied too, after the compiled rules: each path as written
    /// and as the kernel names it where that differs, to the sandbox and to the Read and Edit
    /// tools, as [`settings`] writes the compiled ones. The manifests between the project root
    /// and the chat's folder, which only the place the chat opens knows (#1336).
    pub fn denying(&self, denied: &[super::Denial]) -> Settings {
        let mut out = self.clone();
        for (denial, path) in names_of(denied) {
            let path = path.display().to_string();
            let filesystem = &mut out.sandbox["filesystem"];
            if denial.access == Access::ReadWrite {
                if let Some(list) = filesystem["denyRead"].as_array_mut() {
                    list.push(json!(path));
                }
                out.deny.push(format!("Read(/{path})"));
                out.deny.push(format!("Read(/{path}/**)"));
            }
            if let Some(list) = filesystem["denyWrite"].as_array_mut() {
                list.push(json!(path));
            }
            out.deny.push(format!("Edit(/{path})"));
            out.deny.push(format!("Edit(/{path}/**)"));
        }
        out
    }
}

/// Each of `denied` under every name a rule on it is written by: as written, and as the kernel
/// names it where that differs, so a path through a link (a linked `~/.config`, `/tmp`, `/var`)
/// is denied by both names and a rule matches whichever name the sandbox or a tool compares
/// (FD-27); and, where its last part is a link, that link in its folder as the kernel names
/// it, so the link itself is held as well as its target ([`super::seatbelt::held_names`], #1336).
fn names_of(denied: &[super::Denial]) -> Vec<(&super::Denial, std::path::PathBuf)> {
    let mut out = Vec::new();
    for denial in denied {
        let mut names = vec![denial.path.clone()];
        for name in super::seatbelt::held_names(&denial.path) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        out.extend(names.into_iter().map(|name| (denial, name)));
    }
    out
}
