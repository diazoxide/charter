//! Every name the charter → purlis rename moves, in one place (#1253, RN-1).
//!
//! Each [`Name`] knows three things:
//!
//! * [`Name::write`] — the purlis name. It is the only name anything writes.
//! * [`Name::reads`] — the old names still read during the compatibility window (until 1.0,
//!   V93l). Removing the window is deleting these, and nothing else.
//! * [`Name::history`] — names recognised forever, because they sit somewhere nothing will
//!   rewrite: commit trailers, PR bodies already on a forge, a pre-charter `.edm-structure`.
//!
//! **Precedence (V93e).** The purlis name wins. An old name is read only when the purlis name
//! is absent; when both exist the old one is a *leftover*, which the doctor reports
//! ([`crate::doctor`]'s `renamed-leftover` row). [`Name::pick`] is that rule, written once, over
//! any notion of "exists" — a file, a folder, an environment variable, a keychain item.
//!
//! This is the EXPAND step of an expand–contract rename: callers move onto these entries one
//! area at a time, and the old literals they replace stay correct meanwhile because
//! `names_tests.rs` pins every entry's old name to the literal the code still uses.

use std::path::{Path, PathBuf};

/// What sort of thing a name names. Callers never branch on it; it is how the doctor, the
/// guards and the tests enumerate [`ALL`] by family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A file charter writes or reads by name.
    File,
    /// A folder: the state folder, the config, data and log homes.
    Folder,
    /// Text that marks a block or a file as charter's own, recognised by the text itself.
    Marker,
    /// The start of a keychain service name.
    KeychainPrefix,
    /// The start of an environment variable's name.
    EnvPrefix,
    /// The start of a branch charter creates on a forge.
    BranchPrefix,
    /// A commit-message trailer key.
    Trailer,
    /// A harness plugin's name, id, marketplace, skill namespace or tool prefix.
    PluginId,
    /// The app's bundle identifier (and so its log folder, lock and D-Bus name).
    BundleId,
    /// The name the product's own program goes by.
    Binary,
}

/// One renamed name: what to write, what to keep reading, and what history keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Name {
    /// A stable handle for messages and the doctor (`plane-manifest`, `state-dir`, …).
    pub id: &'static str,
    pub kind: Kind,
    /// The purlis name. Nothing writes any other.
    pub write: &'static str,
    /// Old names read during the window, newest first. Read only when [`Self::write`] is absent.
    pub reads: &'static [&'static str],
    /// Names recognised forever, after [`Self::reads`], newest first.
    pub history: &'static [&'static str],
}

/// Which of a name's spellings was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// The purlis name is there (and wins, whatever else is).
    Purlis,
    /// Only an old name is there.
    Old,
    /// Nothing is there: the answer is the purlis name, to write.
    Neither,
}

/// The answer to "which spelling of this name do I use?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked<T> {
    /// The spelling to read; the purlis spelling when [`Found::Neither`].
    pub name: T,
    pub found: Found,
    /// Other spellings that exist beside the one read: state split between two names, which the
    /// doctor flags. Always older than `name`, which is the newest spelling present.
    pub leftovers: Vec<T>,
}

/// A [`Picked`] path in a directory.
pub type At = Picked<PathBuf>;

impl At {
    /// The path to read, or to write when nothing was found.
    pub fn path(&self) -> &Path {
        &self.name
    }
}

impl Name {
    /// Every spelling, in precedence order: the purlis name, the window's, then history's.
    pub fn spellings(&self) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(self.write)
            .chain(self.reads.iter().copied())
            .chain(self.history.iter().copied())
    }

    /// Whether `text` is any spelling this name has ever had.
    pub fn recognises(&self, text: &str) -> bool {
        self.spellings().any(|s| s == text)
    }

    /// The precedence rule (V93e) over any notion of "exists".
    pub fn pick(&self, exists: impl Fn(&str) -> bool) -> Picked<String> {
        self.pick_with("", exists)
    }

    /// [`Self::pick`] for a prefix: each spelling followed by `rest` (`ENV_PREFIX` + `ROOT`).
    pub fn pick_with(&self, rest: &str, exists: impl Fn(&str) -> bool) -> Picked<String> {
        let mut there = self
            .spellings()
            .map(|s| format!("{s}{rest}"))
            .filter(|full| exists(full));
        let fresh = format!("{}{rest}", self.write);
        match there.next() {
            None => Picked {
                name: fresh,
                found: Found::Neither,
                leftovers: Vec::new(),
            },
            Some(first) if first == fresh => Picked {
                name: first,
                found: Found::Purlis,
                leftovers: there.collect(),
            },
            Some(first) => Picked {
                name: first,
                found: Found::Old,
                leftovers: there.collect(),
            },
        }
    }

    /// The spelling of this name to use in `dir`, judged by `exists` on the joined path.
    pub fn in_dir(&self, dir: &Path, exists: impl Fn(&Path) -> bool) -> At {
        let picked = self.pick(|name| exists(&dir.join(name)));
        Picked {
            name: dir.join(picked.name),
            found: picked.found,
            leftovers: picked.leftovers.into_iter().map(|n| dir.join(n)).collect(),
        }
    }

    /// This name as a regular FILE in `dir`. `is_file`, so a folder of that name is no marker.
    pub fn file_in(&self, dir: &Path) -> At {
        self.in_dir(dir, Path::is_file)
    }

    /// This name as a FOLDER in `dir`.
    pub fn dir_in(&self, dir: &Path) -> At {
        self.in_dir(dir, Path::is_dir)
    }

    /// For a prefix: what follows whichever spelling `text` starts with, or `None`.
    pub fn strip<'t>(&self, text: &'t str) -> Option<&'t str> {
        self.spellings()
            .filter_map(|s| text.strip_prefix(s))
            // The longest spelling that matches, so a spelling that is a prefix of another
            // never takes its text.
            .min_by_key(|rest| rest.len())
    }
}

const fn name(
    id: &'static str,
    kind: Kind,
    write: &'static str,
    reads: &'static [&'static str],
    history: &'static [&'static str],
) -> Name {
    Name {
        id,
        kind,
        write,
        reads,
        history,
    }
}

// ---- files ---------------------------------------------------------------------------- //

/// The file that marks a directory as a plane root.
pub const PLANE_MANIFEST: Name = name(
    "plane-manifest",
    Kind::File,
    "purlis.toml",
    &["charter.toml"],
    &[],
);
/// The machine-local settings file beside it.
pub const LOCAL_SETTINGS: Name = name(
    "local-settings",
    Kind::File,
    "purlis.local.toml",
    &["charter.local.toml"],
    &[],
);
/// The committed scan allowlist.
pub const SCAN_ALLOW: Name = name(
    "scan-allow",
    Kind::File,
    ".purlis-scan-allow.toml",
    &[".charter-scan-allow.toml"],
    &[],
);
/// A workspace's structure-version stamp; `.edm-structure` predates charter.
pub const STRUCTURE_STAMP: Name = name(
    "structure-stamp",
    Kind::File,
    ".purlis-structure",
    &[".charter-structure"],
    &[".edm-structure"],
);
/// The generated-layer sidecar that records which files are the product's own.
pub const GENERATED_SIDECAR: Name = name(
    "generated-sidecar",
    Kind::File,
    ".purlis-generated",
    &[".charter-generated"],
    &[],
);
/// The start of a generated file's temporary name while it is written.
pub const GENERATED_TEMP_PREFIX: Name = name(
    "generated-temp-prefix",
    Kind::File,
    ".purlis-generated.",
    &[".charter-generated."],
    &[],
);
/// An extension's manifest file.
pub const EXTENSION_MANIFEST: Name = name(
    "extension-manifest",
    Kind::File,
    "purlis-extension.json",
    &["charter-extension.json"],
    &[],
);
/// The opencode plugin shim's file name.
pub const OPENCODE_SHIM: Name = name(
    "opencode-shim",
    Kind::File,
    "purlis.ts",
    &["charter.ts"],
    &[],
);

// ---- folders -------------------------------------------------------------------------- //

/// The plane's local state folder.
pub const STATE_DIR: Name = name("state-dir", Kind::Folder, ".purlis", &[".charter"], &[]);
/// The config home's folder name (`~/.config/<this>`).
pub const CONFIG_HOME: Name = name("config-home", Kind::Folder, "purlis", &["charter"], &[]);
/// The data home's folder name (`$XDG_DATA_HOME/<this>`).
pub const DATA_HOME: Name = name("data-home", Kind::Folder, "purlis", &["charter"], &[]);

// ---- markers -------------------------------------------------------------------------- //

/// The key a generated file's front matter records ownership under.
pub const GENERATED_KEY: Name = name(
    "generated-key",
    Kind::Marker,
    "purlis_generated",
    &["charter_generated"],
    &[],
);
/// The plane `.gitattributes` merge-rules block, first line.
pub const MERGE_RULES_BEGIN: Name = name(
    "merge-rules-begin",
    Kind::Marker,
    "# >>> purlis merge rules (managed by purlis) >>>",
    &["# >>> charter merge rules (managed by charter) >>>"],
    &[],
);
/// …and its last line.
pub const MERGE_RULES_END: Name = name(
    "merge-rules-end",
    Kind::Marker,
    "# <<< purlis merge rules <<<",
    &["# <<< charter merge rules <<<"],
    &[],
);
/// A guest clone's `info/exclude` block, first line.
pub const EXCLUDE_BEGIN: Name = name(
    "exclude-begin",
    Kind::Marker,
    "# >>> purlis (generated layer — `purlis workspace reinit`) >>>",
    &["# >>> charter (generated layer — `charter workspace reinit`) >>>"],
    &[],
);
/// …and its last line.
pub const EXCLUDE_END: Name = name(
    "exclude-end",
    Kind::Marker,
    "# <<< purlis <<<",
    &["# <<< charter <<<"],
    &[],
);
/// The plane `.gitignore` live-workspaces block, first line.
pub const LIVE_BEGIN: Name = name(
    "live-begin",
    Kind::Marker,
    "# >>> purlis live workspaces (managed by `purlis workspace live`) >>>",
    &["# >>> charter live workspaces (managed by `charter workspace live`) >>>"],
    &[],
);
/// …and its last line.
pub const LIVE_END: Name = name(
    "live-end",
    Kind::Marker,
    "# <<< purlis live workspaces <<<",
    &["# <<< charter live workspaces <<<"],
    &[],
);
/// The generated personas block in the plane's docs, first line.
pub const PERSONAS_BEGIN: Name = name(
    "personas-begin",
    Kind::Marker,
    "<!-- BEGIN personas — GENERATED by `purlis docs`; do not edit by hand. -->",
    &["<!-- BEGIN personas — GENERATED by `charter docs`; do not edit by hand. -->"],
    &[],
);
/// The marker `persona sync-agents` stamps on the agent files it writes.
pub const SYNC_AGENTS_MARKER: Name = name(
    "sync-agents-marker",
    Kind::Marker,
    "GENERATED by `purlis persona sync-agents`",
    &["GENERATED by `charter persona sync-agents`"],
    &[],
);
/// The first line of the opencode shim.
pub const OPENCODE_MARK: Name = name(
    "opencode-mark",
    Kind::Marker,
    "// purlis's opencode plugin, generated by purlis. Do not edit.",
    &["// charter's opencode plugin, generated by charter. Do not edit."],
    &["// charter-version: "],
);
/// The change block in a pull request's body, first line. Bodies already on a forge keep the
/// old one, so it is history.
pub const CHANGE_BLOCK_BEGIN: Name = name(
    "change-block-begin",
    Kind::Marker,
    "<!-- BEGIN purlis change — GENERATED by `purlis change push`; do not edit by hand. -->",
    &[],
    &["<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->"],
);
/// …and its last line.
pub const CHANGE_BLOCK_END: Name = name(
    "change-block-end",
    Kind::Marker,
    "<!-- END purlis change -->",
    &[],
    &["<!-- END charter change -->"],
);
/// The marker a save pull request carries.
pub const SAVE_PR_MARKER: Name = name(
    "save-pr-marker",
    Kind::Marker,
    "<!-- purlis-save -->",
    &[],
    &["<!-- charter-save -->"],
);

// ---- keychain ------------------------------------------------------------------------- //

/// The start of every vault's keychain service name.
pub const KEYCHAIN_PREFIX: Name = name(
    "keychain-prefix",
    Kind::KeychainPrefix,
    "purlis/",
    &["charter/"],
    &[],
);
/// The service base moved identity items live under.
pub const KEYCHAIN_IDENTITY_PREFIX: Name = name(
    "keychain-identity-prefix",
    Kind::KeychainPrefix,
    "purlis/@identity",
    &["charter/@identity"],
    &[],
);
/// The tag charter puts on 1Password items it creates.
pub const ONEPASSWORD_TAG: Name = name(
    "onepassword-tag",
    Kind::KeychainPrefix,
    "purlis",
    &["charter"],
    &[],
);

// ---- environment ---------------------------------------------------------------------- //

/// The start of every product environment variable (`PURLIS_ROOT`; `CHARTER_ROOT` as fallback).
pub const ENV_PREFIX: Name = name("env-prefix", Kind::EnvPrefix, "PURLIS_", &["CHARTER_"], &[]);

// ---- forge and history ---------------------------------------------------------------- //

/// The start of every branch the product creates. An open `charter/…` branch carries on until
/// it merges, and old ones stay in remotes, so the old prefix is history.
pub const BRANCH_PREFIX: Name = name(
    "branch-prefix",
    Kind::BranchPrefix,
    "purlis/",
    &[],
    &["charter/"],
);
/// The trailer naming the chat that made a commit.
pub const TRAILER_CHAT: Name = name(
    "trailer-chat",
    Kind::Trailer,
    "Purlis-Chat",
    &[],
    &["Charter-Chat"],
);
/// The trailer naming the persona that made a commit.
pub const TRAILER_PERSONA: Name = name(
    "trailer-persona",
    Kind::Trailer,
    "Purlis-Persona",
    &[],
    &["Charter-Persona"],
);
/// The trailer naming the change a landing commit belongs to.
pub const TRAILER_CHANGE: Name = name(
    "trailer-change",
    Kind::Trailer,
    "Purlis-Change",
    &[],
    &["Charter-Change"],
);

// ---- harness plugin ------------------------------------------------------------------- //

/// The plugin's name, which is also its skill namespace and its MCP server's name.
pub const PLUGIN_NAME: Name = name("plugin-name", Kind::PluginId, "purlis", &["charter"], &[]);
/// The id a `--plugin-dir` load gets. The old ids are pinned off (ADR 0056's FORMERLY), never
/// read as this plugin, so they are history.
pub const PLUGIN_LOADED_AS: Name = name(
    "plugin-loaded-as",
    Kind::PluginId,
    "purlis@inline",
    &[],
    &["charter@inline", "charter-app@inline"],
);
/// The marketplace the installed copy is registered under.
pub const PLUGIN_MARKETPLACE: Name = name(
    "plugin-marketplace",
    Kind::PluginId,
    "purlis-app",
    &[],
    &["charter-app"],
);
/// The id the installed copy gets.
pub const PLUGIN_INSTALLED_AS: Name = name(
    "plugin-installed-as",
    Kind::PluginId,
    "purlis@purlis-app",
    &[],
    &["charter@charter-app"],
);
/// How skills are namespaced in a harness (`purlis:handoff`).
pub const SKILL_NAMESPACE: Name = name(
    "skill-namespace",
    Kind::PluginId,
    "purlis:",
    &["charter:"],
    &[],
);
/// The start of the MCP server's tool ids.
pub const MCP_TOOL_PREFIX: Name = name(
    "mcp-tool-prefix",
    Kind::PluginId,
    "mcp__purlis__",
    &["mcp__charter__"],
    &[],
);

// ---- the app and the program ---------------------------------------------------------- //

/// The app's bundle identifier.
pub const BUNDLE_ID: Name = name(
    "bundle-id",
    Kind::BundleId,
    "dev.purlis.app",
    &["dev.charter.app"],
    &[],
);
/// The product's own program. `charter` is an alias during the window; `edm` predates charter
/// and is recognised forever, as the guards always have.
pub const BINARY: Name = name("binary", Kind::Binary, "purlis", &["charter"], &["edm"]);

/// Every entry, for enumeration by the doctor, the guards and the tests.
pub const ALL: &[Name] = &[
    PLANE_MANIFEST,
    LOCAL_SETTINGS,
    SCAN_ALLOW,
    STRUCTURE_STAMP,
    GENERATED_SIDECAR,
    GENERATED_TEMP_PREFIX,
    EXTENSION_MANIFEST,
    OPENCODE_SHIM,
    STATE_DIR,
    CONFIG_HOME,
    DATA_HOME,
    GENERATED_KEY,
    MERGE_RULES_BEGIN,
    MERGE_RULES_END,
    EXCLUDE_BEGIN,
    EXCLUDE_END,
    LIVE_BEGIN,
    LIVE_END,
    PERSONAS_BEGIN,
    SYNC_AGENTS_MARKER,
    OPENCODE_MARK,
    CHANGE_BLOCK_BEGIN,
    CHANGE_BLOCK_END,
    SAVE_PR_MARKER,
    KEYCHAIN_PREFIX,
    KEYCHAIN_IDENTITY_PREFIX,
    ONEPASSWORD_TAG,
    ENV_PREFIX,
    BRANCH_PREFIX,
    TRAILER_CHAT,
    TRAILER_PERSONA,
    TRAILER_CHANGE,
    PLUGIN_NAME,
    PLUGIN_LOADED_AS,
    PLUGIN_MARKETPLACE,
    PLUGIN_INSTALLED_AS,
    SKILL_NAMESPACE,
    MCP_TOOL_PREFIX,
    BUNDLE_ID,
    BINARY,
];

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
