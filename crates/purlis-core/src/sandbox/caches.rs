//! The package caches of a project's sandboxed chats (spec #1330's coupled widening, #1337):
//! with the toolchains preset on, `cargo build` and `npm install` download into caches of the
//! project's own instead of stopping with "Operation not permitted".
//!
//! **A cache home of the project's own, never the person's caches** (D-1337-6). Every tool
//! uses a cached package without checking it again somewhere: cargo an extracted
//! `registry/src` and a cached `.crate`, Go an extracted module tree, Gradle its script and
//! plugin class caches, pnpm its store index. So a cache a chat writes is code a later build
//! runs, and a shared one would be code the person's own, unsandboxed builds run, in every
//! project (ADR 0067 §5). Each project's sandboxed chats get their own caches instead, in a
//! folder under purlis's data home ([`home_of`]), and each tool is pointed at it through the
//! chat's environment ([`TOOLS`]). Only that project's chats, all sandboxed, use them. The
//! person's own caches are read, never written, and no unsandboxed program is pointed at
//! these. The cost is disk, and one first download per project.
//!
//! **cargo's home is seeded, never shared.** `CARGO_HOME` moves cargo's config with it, so
//! purlis writes the new home's `config.toml` at every start from the person's own, keeping
//! only where crates come from: the `[registries]` indexes, `registry.default` and the
//! `[source]` replacements ([`cargo_config`]). Never a token, a credential provider, a build
//! setting or an alias, and never a `bin` folder. A private registry that needs a login is not
//! reached from a sandboxed chat.
//!
//! **What a chat writes there** ([`CacheHome`]): each tool's folder, whole; and of cargo's
//! home only its registry, its git checkouts, what is inside each bare repository of its git
//! database, and the lock and tracking files cargo keeps beside them. Never cargo's config,
//! credentials or `bin`, a bare repository's `config` or `hooks`, and never a folder purlis
//! made: each is pinned, so none is swapped for a link.

use std::path::{Path, PathBuf};

use super::{Denied, Machine};

/// One tool's folder in a project's cache home, and the variables that point the tool at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    pub dir: &'static str,
    pub vars: &'static [&'static str],
}

/// Every toolchain's folder, in the order a sentence lists them.
pub const TOOLS: [Tool; 10] = [
    Tool {
        dir: "cargo",
        vars: &["CARGO_HOME"],
    },
    Tool {
        dir: "npm",
        vars: &["npm_config_cache", "NPM_CONFIG_CACHE"],
    },
    Tool {
        dir: "pip",
        vars: &["PIP_CACHE_DIR"],
    },
    Tool {
        dir: "go-mod",
        vars: &["GOMODCACHE"],
    },
    Tool {
        dir: "go-build",
        vars: &["GOCACHE"],
    },
    Tool {
        dir: "gradle",
        vars: &["GRADLE_USER_HOME"],
    },
    Tool {
        dir: "yarn",
        vars: &["YARN_CACHE_FOLDER"],
    },
    Tool {
        dir: "yarn-berry",
        vars: &["YARN_GLOBAL_FOLDER"],
    },
    Tool {
        dir: "pnpm-store",
        // pnpm 10 reads `pnpm_config_`, pnpm 9 and older the `npm_config_` spelling. Spelled in
        // two halves: a commit scan from before #1364 read these names as an npm token.
        vars: &["pnpm_config_store_dir", concat!("npm", "_config_store_dir")],
    },
    Tool {
        dir: "pnpm-cache",
        vars: &["pnpm_config_cache_dir", concat!("npm", "_config_cache_dir")],
    },
];

/// What in cargo's home a chat writes whole: its registry (index, archives and sources) and its
/// git checkouts.
pub const CARGO_TREES: [&str; 2] = ["registry", "git/checkouts"];

/// cargo's git database: one bare repository per git dependency. A chat writes inside each one
/// cargo made, never an entry itself, so none is made, moved in or linked in.
pub const CARGO_GIT_DB: &str = "git/db";

/// The files cargo keeps at the top of its home while it downloads: its package cache locks and
/// its tracking database with SQLite's companions.
pub const CARGO_FILES: [&str; 6] = [
    ".package-cache",
    ".package-cache-mutate",
    ".global-cache",
    ".global-cache-wal",
    ".global-cache-shm",
    ".global-cache-journal",
];

/// What a bare repository holds that a later git runs: its config, which names programs
/// (`core.sshCommand`, `credential.helper`, `core.fsmonitor`), and its hooks.
pub const BARE_RUN: [&str; 2] = ["config", "hooks"];

/// A project's cache home, resolved for one chat: what it may write there, and what points its
/// tools at it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CacheHome {
    /// The project's cache home, which purlis makes and a chat never writes itself.
    pub root: PathBuf,
    /// Folders a chat writes whole. Each is made by purlis and pinned.
    pub trees: Vec<PathBuf>,
    /// Folders of bare repositories, of which a chat writes only what is inside each entry,
    /// never an entry, and never an entry's [`BARE_RUN`].
    pub bare: Vec<PathBuf>,
    /// Single files a chat writes.
    pub files: Vec<PathBuf>,
    /// The variables that point each tool at its folder.
    pub env: Vec<(String, String)>,
    /// The `config.toml` purlis writes in cargo's home ([`cargo_config`]).
    pub cargo_config: String,
}

impl CacheHome {
    /// Every folder purlis makes before the chat starts, outermost first.
    pub fn folders(&self) -> Vec<PathBuf> {
        let mut out = vec![self.root.clone()];
        let cargo = self.root.join("cargo");
        out.push(cargo.clone());
        out.push(cargo.join("git"));
        out.extend(self.trees.iter().cloned());
        out.extend(self.bare.iter().cloned());
        out.sort();
        out.dedup();
        out
    }

    /// What a chat may write, as folders and files: for a check that no program a chat starts
    /// on lies there.
    pub fn writable(&self) -> Vec<PathBuf> {
        let mut out = self.trees.clone();
        out.extend(self.bare.iter().cloned());
        out.extend(self.files.iter().cloned());
        out
    }

    /// Takes out every link where purlis keeps a folder or file of this home, without following
    /// it: the home is purlis's own, so a link there is one a chat planted to send a tool's
    /// writes elsewhere, and taking it out lets the chat start rather than dead-end. A link
    /// that cannot be taken out is left, and [`Self::prepare`] then refuses the chat.
    pub fn unlink_links(&self) {
        for path in self
            .folders()
            .iter()
            .chain(&self.files)
            .chain([&self.cargo_config_path()])
        {
            if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// Where purlis writes cargo's config in this home.
    pub fn cargo_config_path(&self) -> PathBuf {
        self.root.join("cargo").join("config.toml")
    }

    /// Makes every folder and writes cargo's config, outside the sandbox, before the chat
    /// starts: links are taken out first ([`Self::unlink_links`]), every folder is checked to
    /// be no link before and after it is made, and the config is written to a new file beside
    /// it, opened without following a link, then renamed into place. Or the one sentence,
    /// naming the path, why the chat does not start.
    pub fn prepare(&self) -> Result<(), String> {
        self.unlink_links();
        let linked = |when: &str| -> Result<(), String> {
            match self
                .folders()
                .iter()
                .chain(&self.files)
                .find(|path| is_linked(path))
            {
                Some(path) => Err(super::caches_linked(path, when)),
                None => Ok(()),
            }
        };
        linked("before")?;
        for folder in self.folders() {
            std::fs::create_dir_all(&folder).map_err(|err| unwritable(&folder, &err))?;
        }
        linked("after")?;
        let config = self.cargo_config_path();
        write_beside(&config, self.cargo_config.as_bytes()).map_err(|err| unwritable(&config, &err))
    }
}

/// Why a chat does not start when purlis cannot make or write `path` of its project's caches.
fn unwritable(path: &Path, err: &std::io::Error) -> String {
    format!(
        "this project runs every chat sandboxed, and purlis could not make {} for the project's \
         package caches ({err}), so nothing was started.",
        path.display()
    )
}

/// Writes `bytes` to `path` through a new file beside it, made without following a link and
/// renamed into place, so a link at `path` is replaced, never written through.
fn write_beside(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no folder"))?;
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("no name"))?;
    let temp = parent.join(format!(
        ".{}.purlis-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW).mode(0o644);
    }
    let written = options
        .open(&temp)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()));
    if let Err(err) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(err);
    }
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// Where [`home_of`] puts the cache home of the project at `root` on `machine`, whether or not
/// it is made or given: `cache-homes/<project key>` under purlis's data home. `None` where
/// there is no data home.
pub fn root_of(machine: &Machine, root: &Path) -> Option<PathBuf> {
    Some(
        super::Homes::charter_data(machine)?
            .join("cache-homes")
            .join(super::Homes::project_key(root)),
    )
}

/// The cache home of the project at `root` on `machine`'s sandboxed chats: a folder named for
/// the project as the kernel names it, under purlis's data home. `None` where there is no data
/// home, or where the folder is not one a chat may be given ([`granted`]).
pub fn home_of(machine: &Machine, root: &Path, denied: &Denied) -> Option<CacheHome> {
    let base = root_of(machine, root)?;
    let cargo = base.join("cargo");
    let mut home = CacheHome {
        root: base.clone(),
        trees: TOOLS
            .iter()
            .filter(|tool| tool.dir != "cargo")
            .map(|tool| base.join(tool.dir))
            .collect(),
        bare: vec![cargo.join(CARGO_GIT_DB)],
        files: CARGO_FILES.iter().map(|file| cargo.join(file)).collect(),
        env: Vec::new(),
        cargo_config: cargo_config(&person_cargo_config(machine)),
    };
    home.trees
        .splice(0..0, CARGO_TREES.iter().map(|tree| cargo.join(tree)));
    for tool in TOOLS {
        for var in tool.vars {
            home.env
                .push(((*var).to_owned(), base.join(tool.dir).display().to_string()));
        }
    }
    // A link purlis's own home holds is one a chat planted: taken out, not followed, so the
    // grants below name the folders purlis made and not where a link points.
    home.unlink_links();
    granted(&home, machine, root, denied).then_some(home)
}

/// The text of the person's own cargo config: `$CARGO_HOME`'s, or `~/.cargo`'s, under either
/// name cargo reads. Empty where there is none.
fn person_cargo_config(machine: &Machine) -> String {
    let Some(home) = machine
        .env
        .get("CARGO_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| machine.home.as_ref().map(|home| home.join(".cargo")))
    else {
        return String::new();
    };
    ["config.toml", "config"]
        .iter()
        .find_map(|name| super::read_plane_file(&home.join(name)).ok().flatten())
        .unwrap_or_default()
}

/// The keys of a `[source]` entry that say where crates come from, and nothing else.
const SOURCE_KEYS: [&str; 8] = [
    "replace-with",
    "registry",
    "local-registry",
    "directory",
    "git",
    "branch",
    "tag",
    "rev",
];

/// The `config.toml` of a project's cargo home, from `person`, the person's own: where crates
/// come from and nothing else (D-1337-8). Each `[registries]` entry's `index` and `protocol`,
/// `registry.default`, and each `[source]` entry's [`SOURCE_KEYS`]. Never a token, a credential
/// provider, a build setting, an alias or a runner, which name programs or hold a secret. A
/// config purlis cannot read as TOML gives nothing.
pub fn cargo_config(person: &str) -> String {
    let mut out = toml::Table::new();
    let top = person.parse::<toml::Table>().unwrap_or_default();
    let keep = |table: Option<&toml::Value>, keys: &[&str]| -> toml::Table {
        let mut kept = toml::Table::new();
        let Some(entries) = table.and_then(toml::Value::as_table) else {
            return kept;
        };
        for (name, entry) in entries {
            let Some(entry) = entry.as_table() else {
                continue;
            };
            let fields: toml::Table = entry
                .iter()
                .filter(|(key, value)| keys.contains(&key.as_str()) && value.is_str())
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            if !fields.is_empty() {
                kept.insert(name.clone(), toml::Value::Table(fields));
            }
        }
        kept
    };
    let registries = keep(top.get("registries"), &["index", "protocol"]);
    if !registries.is_empty() {
        out.insert("registries".to_owned(), toml::Value::Table(registries));
    }
    if let Some(default) = top
        .get("registry")
        .and_then(|registry| registry.get("default"))
        .filter(|value| value.is_str())
    {
        let mut registry = toml::Table::new();
        registry.insert("default".to_owned(), default.clone());
        out.insert("registry".to_owned(), toml::Value::Table(registry));
    }
    let source = keep(top.get("source"), &SOURCE_KEYS);
    if !source.is_empty() {
        out.insert("source".to_owned(), toml::Value::Table(source));
    }
    format!(
        "# Written by purlis for this project's sandboxed chats at every start: where crates come \
         from, copied from your own cargo config. Nothing else is read from here.\n{out}"
    )
}

/// The folders a later program is found in, which no cache home may be or hold: every folder a
/// chat's `PATH` searches ([`crate::programs::search_dirs_from`]), and each tool's own bin
/// folder on `PATH` or not: cargo's home and its `bin`, Go's `bin`, and pnpm's home.
pub fn never_covered(machine: &Machine) -> Vec<PathBuf> {
    let absolute = |name: &str| {
        machine
            .env
            .get(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let home = |rel: &str| machine.home.as_ref().map(|home| home.join(rel));
    let path = machine.env.get("PATH");
    let mut out = crate::programs::search_dirs_from(
        path.as_deref().map(std::ffi::OsStr::new),
        machine.home.as_deref(),
    );
    if let Some(cargo) = absolute("CARGO_HOME").or_else(|| home(".cargo")) {
        out.push(cargo.join("bin"));
        out.push(cargo);
    }
    out.extend(absolute("GOBIN"));
    let gopath = machine
        .env
        .get("GOPATH")
        .and_then(|gopath| std::env::split_paths(&gopath).next())
        .filter(|first| first.is_absolute())
        .or_else(|| home("go"));
    out.extend(gopath.map(|gopath| gopath.join("bin")));
    out.extend(absolute("PNPM_HOME").or_else(|| home("Library/pnpm")));
    out
}

/// Whether `home` may be given to a chat: every folder of it plain, none a link, and its root
/// holding neither the person's home, the project, a folder in [`never_covered`] nor a denied
/// path, nor inside a denied path.
fn granted(home: &CacheHome, machine: &Machine, root: &Path, denied: &Denied) -> bool {
    use std::path::Component;
    let base = &home.root;
    if !base.is_absolute()
        || base
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    if home
        .folders()
        .iter()
        .chain(&home.files)
        .any(|path| is_linked(path))
    {
        return false;
    }
    let names = |path: &Path| {
        let real = super::real(path);
        if real == path {
            vec![path.to_path_buf()]
        } else {
            vec![path.to_path_buf(), real]
        }
    };
    let bases = names(base);
    let holds = |other: &Path| {
        names(other)
            .iter()
            .any(|other| bases.iter().any(|base| other.starts_with(base)))
    };
    let inside = |other: &Path| {
        names(other)
            .iter()
            .any(|other| bases.iter().any(|base| base.starts_with(other)))
    };
    if machine.home.as_deref().is_some_and(holds) || holds(root) || inside(root) {
        return false;
    }
    if never_covered(machine)
        .iter()
        .any(|dir| holds(dir) || inside(dir))
    {
        return false;
    }
    !denied
        .paths
        .iter()
        .any(|denial| holds(&denial.path) || inside(&denial.path))
}

/// Whether `path` is there and is a link, or is reached through one its own parent does not
/// name: a chat that swapped a cache folder for a link would have every write into it land
/// wherever the link points.
pub fn is_linked(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => super::real(path) != super::real(parent).join(name),
        _ => false,
    }
}
