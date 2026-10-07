//! Charter's own compiled sandbox on macOS: the Seatbelt profile a whole harness runs inside
//! (ADR 0067 §2), and the one a profile's program answers `--version` inside (D-88k).
//!
//! One profile for every harness charter wraps (opencode and Codex): what a chat may write, the
//! denial classes after it, and what each harness keeps for itself ([`Own`]). Seatbelt takes
//! the last rule that matches, so every denial comes after every grant.
//!
//! **One sandbox per process.** Seatbelt refuses a second profile inside one charter applied
//! (measured on macOS 26.2: `sandbox_apply: Operation not permitted` under any profile that
//! denies anything), so a harness whose own sandbox is applied through Seatbelt cannot keep it
//! on inside this one.

use std::path::{Path, PathBuf};

use super::{Access, Denial, PLANTED, Reach, real};

/// Why a profile could not be written: a path holding a control character, which a rule could
/// not state exactly.
pub const CONTROL: &str = "purlis cannot write a sandbox profile for a path holding a control \
                           character";

/// Why a profile could not be handed over: longer than [`MOST_PROFILE`].
pub const TOO_LARGE: &str = "purlis cannot hand the sandbox a profile this long: the project's \
                             configs name more paths than one chat's sandbox can hold";

/// What a person can do about a refusal `why` of [`profile`], where there is something.
/// `opt_out` is whether they may start the chat without the sandbox: where an administrator's
/// policy forbids it, the way out does not name it.
fn way_out(why: &str, opt_out: bool) -> Option<&'static str> {
    (why == TOO_LARGE).then_some(if opt_out {
        "Have the configs name fewer scripts, or start this chat without the sandbox from the \
         new-chat picker."
    } else {
        "Have the configs name fewer scripts."
    })
}

/// The refusal of a chat whose profile could not be written for `why`: after `lead`, what
/// happened, then the way out where there is one (#1418). `opt_out` is whether the person may
/// start the chat without the sandbox ([`super::At::no_opt_out`] says where policy forbids
/// it): where they may not, the refusal names no opt-out (#1423), and [`super::Applied::line`]
/// ends it with the policy and who set it.
pub fn not_started(lead: &str, why: &str, opt_out: bool) -> String {
    match way_out(why, opt_out) {
        Some(then) => format!("{lead} {why}, so nothing was started. {then}"),
        None => format!("{lead} {why}, so nothing was started."),
    }
}

/// The longest profile a wrap hands `sandbox-exec` (#1418). It goes in argv (`-p`), which
/// holds 1 MiB on macOS with the environment in it: one config at [`super::planted::MOST_NAMED`]
/// writes about 300 KB, so two fit, and a quarter is left for the environment and the
/// harness's own words. A longer one is refused by [`TOO_LARGE`], not left to fail the start
/// with `E2BIG`.
pub const MOST_PROFILE: usize = 768 * 1024;

/// What a harness charter wraps keeps for itself, beside the chat's own directory and temp
/// directory: each grant is only what a turn of that harness writes (rulings V73 and V73a).
#[derive(Debug, Clone, Default)]
pub struct Own {
    /// The harness's own directories. A chat whose directory holds one, or is inside one, is
    /// not wrapped: its directory's grant would be a grant on what a later harness loads.
    pub dirs: Vec<PathBuf>,
    /// Why such a chat is not wrapped, as one clause.
    pub overlap: &'static str,
    /// Filters, in the `file-write*` allow, for what a turn writes.
    pub allow: Vec<String>,
    /// Directories it writes into, which get the later-code names denied at any depth too.
    pub roots: Vec<PathBuf>,
    /// Whole rules, after every denial class.
    pub deny: Vec<String>,
    /// Whole rules last of all: a read given back inside a folder denied above.
    pub after: Vec<String>,
}

/// `text` as an SBPL string, or [`CONTROL`].
pub fn string(text: &str) -> Result<String, &'static str> {
    if text.chars().any(char::is_control) {
        return Err(CONTROL);
    }
    Ok(format!(
        "\"{}\"",
        text.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// `path` as the kernel names it, as an SBPL string.
pub fn quote(path: &Path) -> Result<String, &'static str> {
    string(&real(path).display().to_string())
}

/// The names a rule on `path` is written under: as the kernel names it, and, where its last part
/// is a link, that link itself in the folder as the kernel names it. A rule on the target alone
/// would let the link be removed and a file made in its place (#1336).
pub fn held_names(path: &Path) -> Vec<PathBuf> {
    let resolved = real(path);
    let mut out = vec![resolved.clone()];
    if let (Some(folder), Some(name)) = (path.parent(), path.file_name()) {
        let itself = real(folder).join(name);
        if itself != resolved {
            out.push(itself);
        }
    }
    out
}

/// `text` with every regular-expression character escaped.
pub fn escaped(text: &str) -> String {
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
pub fn under(dir: &Path, below: &str) -> String {
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

/// The one lookup the certificate check needs (#1337): Go programs on macOS, `gh` among them,
/// verify a host's certificate by asking the `trustd` agent, and fail every TLS connection
/// without it. The same rule Claude Code's `enableWeakerNetworkIsolation` adds to its profile.
/// The agent fetches the addresses a certificate names, past the egress proxy, so it is off
/// unless the project turns `certificate-checks` on (D-1337-7).
pub const TRUST: &str = "(allow mach-lookup (global-name \"com.apple.trustd.agent\"))";

/// `own`, widened by what the project's sandbox widens ([`super::Widened`]): its package caches
/// (D-1337-6), each tool's folder written whole, inside each of cargo's bare repositories and
/// never an entry itself, and cargo's few files; each folder purlis made pinned as an entry, so
/// it is never removed, renamed or swapped for a link; the later-code names denied at any
/// depth of each, as in every folder a chat writes, and a bare repository's `config` and
/// `hooks` never written. And the certificate check, where the project turned it on (D-1337-7).
pub fn widen(own: &mut Own, widened: &super::Widened) -> Result<(), &'static str> {
    if let Some(caches) = &widened.caches {
        for tree in &caches.trees {
            own.allow.push(format!("(subpath {})", quote(tree)?));
            own.roots.push(tree.clone());
        }
        for bare in &caches.bare {
            own.allow
                .push(format!("(regex {})", string(&under(bare, "[^/]+/.+$"))?));
            own.roots.push(bare.clone());
            own.deny.push(format!(
                "(deny file-write* (regex {}))",
                string(&under(
                    bare,
                    &format!("[^/]+/({})(/.*)?$", super::caches::BARE_RUN.join("|"))
                ))?
            ));
        }
        for file in &caches.files {
            own.allow.push(format!("(literal {})", quote(file)?));
        }
        for folder in caches.folders() {
            own.deny
                .push(format!("(deny file-write* (literal {}))", quote(&folder)?));
        }
    }
    if widened.trust {
        own.after.push(TRUST.to_owned());
    }
    Ok(())
}

/// Lets a wrapped chat write each of `folders`, a person's grants (#1342): each in the
/// `file-write*` allow, and each a root the later-code names are denied at any depth in. Every
/// denial class follows them in the profile, so a class still wins inside one.
pub fn granted(own: &mut Own, folders: &[PathBuf]) -> Result<(), &'static str> {
    for folder in folders {
        // As judged, already the kernel's spelling (D-1342-12): never resolved again, so a
        // folder swapped for a link since is not followed to where it now points.
        own.allow.push(format!(
            "(subpath {})",
            string(&folder.display().to_string())?
        ));
        own.roots.push(folder.clone());
        // And every project manifest at any depth of it (D-T55-4): #1336 holds them by path
        // only in the chat's own folders, which a granted folder is not.
        let names = super::MANIFESTS
            .iter()
            .map(|name| escaped(name))
            .collect::<Vec<_>>()
            .join("|");
        let regex = format!(
            "^{}/(.*/)?({names})$",
            escaped(&folder.display().to_string())
        );
        own.deny
            .push(format!("(deny file-write* (regex {}))", string(&regex)?));
    }
    Ok(())
}

/// The keychain files of the home `home`, denied with the vaults class: no lookup of the
/// security service is allowed either, so a wrap holds a keyring vault ([`BASE`]).
pub fn keychains(home: Option<&Path>) -> Option<Denial> {
    home.map(|home| Denial {
        class: super::Class::Vaults,
        path: home.join("Library/Keychains"),
        access: Access::ReadWrite,
        named: None,
    })
}

/// The Seatbelt profile for a chat in `cwd`, with its own temp directory `tmp`, denied
/// `denied`, keeping `own` for its harness, reaching the network through the proxy on
/// `proxy_port` and reporting on `hook_socket`.
pub fn profile(
    denied: &[Denial],
    own: &Own,
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
    for dir in &own.dirs {
        let dir = real(dir);
        if dir.starts_with(&cwd_real) || cwd_real.starts_with(&dir) {
            return Err(own.overlap);
        }
    }

    // What the chat may write.
    let mut allow = vec![
        format!("(subpath {})", quote(cwd)?),
        format!("(subpath {})", quote(tmp)?),
    ];
    allow.extend(own.allow.iter().cloned());
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
    // Each under every name the kernel may give it: as it resolves, and across a macOS
    // firmlink (`/Users` and `/System/Volumes/Data/Users`), which resolving leaves as spelled
    // (#1356).
    let named = |path: &Path| -> Result<Vec<String>, &'static str> {
        super::kernel_names(path)
            .iter()
            .map(|name| string(&name.display().to_string()))
            .collect()
    };
    for denial in denied {
        let what = match denial.access {
            Access::ReadWrite => "file-read* file-write*",
            Access::Write => "file-write*",
        };
        // Held as itself and as its target (#1336), each also across a firmlink (#1356).
        let mut names: Vec<PathBuf> = Vec::new();
        for held in held_names(&denial.path) {
            for name in super::both_firmlink_names(held) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        for name in names {
            line(format!(
                "(deny {what} (subpath {}))",
                string(&name.display().to_string())?
            ));
        }
    }
    // Each directory between the chat's own and a denied path, as an entry only, so it is never
    // moved away with the denied path in it, nor replaced (measured: a plane-root chat could
    // otherwise move `.charter` aside and write `.charter/app` under its new name).
    let mut pinned = std::collections::BTreeSet::new();
    let mut pin = |folder: &Path, line: &mut dyn FnMut(String)| -> Result<(), &'static str> {
        let names = named(folder)?;
        if pinned.insert(names.first().cloned()) {
            for name in names {
                line(format!("(deny file-write* (literal {name}))"));
            }
        }
        Ok(())
    };
    // The chat's own directory and its temp directory too, as entries: moved whole into the
    // other, every path rule under the one moved would no longer match (measured).
    for root in [&cwd_real, &real(tmp)] {
        pin(root, &mut line)?;
    }
    // Between a name of the chat's directory and the same side's name of the denied path.
    let cwd_names = super::kernel_names(&cwd_real);
    for denial in denied {
        for path in super::kernel_names(&denial.path) {
            for cwd_name in &cwd_names {
                for ancestor in super::ancestors_within(&path, cwd_name) {
                    pin(&ancestor, &mut line)?;
                }
            }
        }
    }
    // On every directory the chat may write, so a protected name made in one cannot be moved
    // into another with its parent.
    let mut roots = vec![cwd.to_path_buf(), tmp.to_path_buf()];
    roots.extend(own.roots.iter().cloned());
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
    for rule in own.deny.iter().chain(&own.after) {
        line(rule.clone());
    }
    if out.len() > MOST_PROFILE {
        return Err(TOO_LARGE);
    }
    Ok(out)
}

/// The profile a program answers `--version` inside, before a sandboxed chat starts on it
/// (D-88k): no network at all, and nothing written but `tmp`, the probe's own temp directory,
/// and `/dev/null`. Stricter than any chat's, so a program that loads a file a chat wrote
/// gets nothing out of the probe.
pub fn probe_profile(tmp: &Path) -> Result<String, &'static str> {
    Ok(format!(
        "{BASE}(allow file-write* (subpath {}) (literal \"/dev/null\"))\n",
        quote(tmp)?
    ))
}

/// What every wrapped chat is allowed, whatever its plane: each line was needed by a harness
/// or by a command it ran (measured), and nothing here reaches the network or writes a file.
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

/// What tools read in place of `TMPDIR`, each pointed into a wrapped chat's own temp directory
/// (#1120, measured on macOS 26.2): `TMP` and `TEMP`, and `xcrun`'s cache and clang's module
/// cache by the name below it, which otherwise go to the system's per-user temp and cache
/// folders (`/var/folders/…/T` and `…/C`). No chat may write those, so clang and Swift fail
/// on the module cache without this. Fresh each start: no chat loads a module another built.
pub const TEMP_ENV: [(&str, &str); 4] = [
    ("TMP", ""),
    ("TEMP", ""),
    ("xcrun_db", "xcrun_db"),
    ("CLANG_MODULE_CACHE_PATH", "clang-module-cache"),
];

/// What a wrapped chat's environment gains: its traffic pointed at the proxy at `proxy`, the
/// way past it emptied, and its own temp directory `tmp`, under every name in [`TEMP_ENV`] too.
/// `mktemp` and Foundation's `NSTemporaryDirectory` read no variable at all, and are refused:
/// a block sorted as `sandboxblock::Kind::SystemTemp`, whose notice gives `mktemp`'s way round.
pub fn env(proxy: &str, tmp: &Path) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = PROXY_ENV
        .iter()
        .map(|key| ((*key).to_owned(), proxy.to_owned()))
        .collect();
    env.extend(
        NO_PROXY_ENV
            .iter()
            .map(|key| ((*key).to_owned(), String::new())),
    );
    env.push(("TMPDIR".to_owned(), tmp.display().to_string()));
    env.extend(TEMP_ENV.iter().map(|(key, below)| {
        let at = if below.is_empty() {
            tmp.to_path_buf()
        } else {
            tmp.join(below)
        };
        ((*key).to_owned(), at.display().to_string())
    }));
    env
}
