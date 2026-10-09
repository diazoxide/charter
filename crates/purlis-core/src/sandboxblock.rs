//! **A sandbox block, sorted** (#1338, spec #1330): what a sandboxed chat's sandbox refused, read
//! from the harness's own report of it, reduced to an operation and the kind of path or host it
//! was about — and nothing else.
//!
//! # Only a sandboxed chat, and the sandbox's own evidence first
//!
//! A tool hook runs outside the chat's sandbox, after the tool came back. It reads anything only
//! in a chat the app started sandboxed (`crate::sandbox::chat_is_sandboxed`): a refusal in a chat
//! that opted out, or by macOS's privacy controls or system integrity protection, is not the
//! sandbox's, and is never shown as one. Then ([`detect`]):
//!
//! - **Claude Code's `<sandbox_violations>` block**, the primary signal: one Seatbelt line per
//!   refusal, `cargo(123) deny(1) file-write-create /path`. Only the last block, and only when it
//!   ends the text, which is where Claude Code appends it.
//! - **An egress proxy's refusal**: purlis's own ("purlis's sandbox does not allow host:443"),
//!   Claude Code's ("blocked by network allowlist"), or a client's word for the `403` a proxy
//!   answered a tunnel with; and Go's certificate check failing on the system service the
//!   sandbox keeps from it (`x509: OSStatus -26276`).
//! - **A program's own "Operation not permitted"** that names its path, where no violation block
//!   said more, and only for a path outside what the chat may write (its folder and the temporary
//!   folders): inside them, the sandbox did not refuse it.
//!
//! The last two are read from a command's **standard error alone** (`PostToolUse`'s
//! `tool_response.stderr`). A failed command's `error` holds its standard output as well, so on
//! that path only the violation block is read: a chat that prints a file or greps purlis's own
//! sources is not refused anything.
//!
//! **Each harness's adapter hands its result over in that shape** (#1353). opencode returns a
//! shell command's standard output and error as one text, with its exit status beside it. Its
//! shim (`crate::opencode`) passes a command that did not fail as one mixed text, so only a
//! violation block is read from it. A command that failed is passed as standard error marked
//! `mixed`, and all three kinds of evidence are read from it, standard output included, which is
//! wider than Claude Code's route: a failing command that prints a refusal's words raises a
//! block. So from a mixed stream nothing but a violation line names a target for a grant
//! ([`detect_with_targets`]): such a Notice names no host or path, and the person types it.
//! Codex arms no hook after a command, so a Codex chat's blocks are not read.
//!
//! # Whose operation it was
//!
//! **Per violation line, by the process it names**: a line whose process is `purlis` (or a name
//! it is installed under) is purlis's own, which is a purlis bug. Nothing else is: a line with no
//! process, a proxy's refusal, a program's own words. So `cargo build; purlis status` does not
//! make cargo's block purlis's. A chat can still print a line that names `purlis`, as it can
//! send the app any line its token admits; what that buys is a Notice on its own tab and one
//! more in the count ([`Throttle`] bounds it), and a Report that names only fixed words.
//!
//! # What is kept of it
//!
//! **An operation, a kind, and whether it was purlis's own** ([`Block`]). The path or host is
//! read here, in the hook, to sort it, and dropped: no path, argument, host name or output
//! leaves this module.
//!
//! The app keeps each block it hears in [`path`] for seven days ([`record`]), which is what
//! `purlis doctor` counts ([`counts`]), and the window shows it as a notice on the chat's tab.
//! A block of purlis's own is a purlis bug, and the notice offers a Report whose draft is made
//! from the block alone (`report::Draft::of_sandbox_block`). Nothing is sent without a press.

use std::path::{Component, Path, PathBuf};

/// What the sandbox refused to let a program do.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    /// Create, change, rename or remove a file or folder.
    Write,
    /// Read a file, or list a folder.
    Read,
    /// Open a connection: to a host, or to a local socket.
    Connect,
    /// Reach a system service by name (macOS's `mach-lookup`).
    Lookup,
    /// Start a program.
    Run,
    /// Something done to a file that the report does not say: a bare "Operation not permitted".
    File,
    /// Anything else the sandbox names.
    Other,
}

impl Operation {
    /// Every operation, in the order a count lists them.
    pub const ALL: [Operation; 7] = [
        Self::Write,
        Self::Read,
        Self::Connect,
        Self::Lookup,
        Self::Run,
        Self::File,
        Self::Other,
    ];

    /// The word it is stored and counted under.
    pub fn word(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Read => "read",
            Self::Connect => "connect",
            Self::Lookup => "lookup",
            Self::Run => "run",
            Self::File => "file",
            Self::Other => "other",
        }
    }

    /// The operation `word` names, as [`Operation::word`] spells it.
    pub fn of_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.word() == word)
    }

    /// The operation a Seatbelt word names: `file-write-create`, `network-outbound`, ….
    fn of_seatbelt(word: &str) -> Self {
        if word.starts_with("file-write") {
            Self::Write
        } else if word.starts_with("file-read") {
            Self::Read
        } else if word.starts_with("network") {
            Self::Connect
        } else if word.starts_with("mach-lookup") {
            Self::Lookup
        } else if word.starts_with("process-exec") {
            Self::Run
        } else if word.starts_with("file-") {
            Self::File
        } else {
            Self::Other
        }
    }

    /// The operation as a sentence names it, before what it was done to.
    fn phrase(self) -> &'static str {
        match self {
            Self::Write => "a write to",
            Self::Read => "a read of",
            Self::Connect => "a connection to",
            Self::Lookup => "a lookup of",
            Self::Run => "starting a program in",
            Self::File => "a file operation in",
            Self::Other => "an operation on",
        }
    }
}

/// The kind of path or host a block was about. A closed set: nothing the chat wrote can
/// become one.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// The project's own files, outside the chat's folder: records, memory, `workspace.md`.
    ProjectFiles,
    /// purlis's state folder at the project root (`.purlis/` or `.charter/`).
    ProjectState,
    /// A file a later program loads code or settings from, which the sandbox protects
    /// wherever it is (`.git/config`, `.vscode`; `sandbox::PLANTED`), or a project manifest
    /// where it could change a chat's sandbox (`sandbox::manifests_held`).
    ProtectedFile,
    /// The chat's own folder.
    ChatFolder,
    /// A toolchain's package cache in the home folder (`~/.cargo`, `~/.npm`, `~/Library/Caches`).
    ToolchainCache,
    /// Elsewhere in the home folder.
    Home,
    /// A temporary folder.
    Temp,
    /// macOS's per-user temporary folder (`/var/folders/<a>/<b>/T`, what
    /// `getconf DARWIN_USER_TEMP_DIR` answers), which some tools ask the system for in place of
    /// `TMPDIR` (#1120): `mktemp` without a path, Foundation's `NSTemporaryDirectory`, and
    /// `xcrun`'s cache, which only warns. No sandboxed chat may write it (D-1342-11).
    SystemTemp,
    /// macOS's per-user cache folder (`/var/folders/<a>/<b>/C`, `DARWIN_USER_CACHE_DIR`), where
    /// clang and Swift keep their module cache. A wrapped chat's module cache is its own temp
    /// directory (`sandbox::seatbelt::TEMP_ENV`); a Claude Code chat's is not yet (#1416), so
    /// those builds fail there. No sandboxed chat may write it: it holds what other programs load.
    SystemCache,
    /// Anywhere else on the machine.
    System,
    /// An internet host the project does not allow.
    Host,
    /// A local socket.
    LocalSocket,
    /// The system's certificate check, which Go's TLS asks a system service for.
    CertificateCheck,
    /// Another system service.
    SystemService,
}

impl Kind {
    /// Every kind, in the order they are listed.
    pub const ALL: [Kind; 14] = [
        Self::ProjectFiles,
        Self::ProjectState,
        Self::ProtectedFile,
        Self::ChatFolder,
        Self::ToolchainCache,
        Self::Home,
        Self::Temp,
        Self::SystemTemp,
        Self::SystemCache,
        Self::System,
        Self::Host,
        Self::LocalSocket,
        Self::CertificateCheck,
        Self::SystemService,
    ];

    /// The word it is stored under.
    pub fn word(self) -> &'static str {
        match self {
            Self::ProjectFiles => "project-files",
            Self::ProjectState => "project-state",
            Self::ProtectedFile => "protected-file",
            Self::ChatFolder => "chat-folder",
            Self::ToolchainCache => "toolchain-cache",
            Self::Home => "home",
            Self::Temp => "temp",
            Self::SystemTemp => "system-temp",
            Self::SystemCache => "system-cache",
            Self::System => "system",
            Self::Host => "host",
            Self::LocalSocket => "local-socket",
            Self::CertificateCheck => "certificate-check",
            Self::SystemService => "system-service",
        }
    }

    /// The kind `word` names, as [`Kind::word`] spells it.
    pub fn of_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.word() == word)
    }

    /// The kind as a sentence names it.
    pub fn phrase(self) -> &'static str {
        match self {
            Self::ProjectFiles => "the project's own files",
            Self::ProjectState => "purlis's state for this project",
            Self::ProtectedFile => "a protected file later programs load, such as .git/config",
            Self::ChatFolder => "this chat's own folder",
            Self::ToolchainCache => "a toolchain's package cache",
            Self::Home => "your home folder",
            Self::Temp => "a temporary folder",
            Self::SystemTemp => {
                "macOS's per-user temporary folder, which mktemp and Swift programs use in place \
                 of this chat's own. mktemp -p \"$TMPDIR\" writes in this chat's own"
            }
            Self::SystemCache => {
                "macOS's per-user cache folder, where Swift and clang builds keep compiled \
                 modules. Swift and clang builds cannot write it in a sandboxed Claude Code chat yet"
            }
            Self::System => "a system folder",
            Self::Host => "an internet host this project does not allow",
            Self::LocalSocket => "a local socket",
            Self::CertificateCheck => "the system's certificate check",
            Self::SystemService => "a system service",
        }
    }
}

/// One block: what was refused, the kind of thing it was refused on, and whether it was
/// purlis's own operation. Nothing more is kept of it, anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Block {
    pub operation: Operation,
    pub kind: Kind,
    /// Whether the process the sandbox refused was purlis itself: a purlis bug, not the chat's.
    #[serde(default)]
    pub ours: bool,
}

impl Block {
    /// What was blocked, as the notice says it: "a write to the project's own files".
    pub fn said(&self) -> String {
        format!("{} {}", self.operation.phrase(), self.kind.phrase())
    }
}

/// The folder the app started the chat in, in its environment (the app's `chats::open_it`):
/// what a block is sorted against as the chat's own folder. Not the payload's `cwd`, which moves
/// with the chat. Claude Code's own `CLAUDE_PROJECT_DIR` says the same where this is not set.
pub const CHAT_DIR_ENV: &str = "PURLIS_CHAT_DIR";

/// Where the chat that was blocked works: what a path is sorted against.
#[derive(Debug, Clone, Copy)]
pub struct Place<'a> {
    /// The project's root.
    pub root: &'a Path,
    /// The folder the app started the chat in ([`CHAT_DIR_ENV`]).
    pub chat: &'a Path,
    /// The folder the command ran in, which a relative path is read from.
    pub cwd: &'a Path,
    /// The home folder, when there is one.
    pub home: Option<&'a Path>,
}

/// The most blocks one tool result yields: a command that failed on a thousand files is one
/// pattern, not a thousand notices.
pub const AT_MOST_PER_RESULT: usize = 8;

/// The blocks in a tool hook's `payload`, each sorted against `place`, at most
/// [`AT_MOST_PER_RESULT`] and none twice. The caller asks only in a sandboxed chat.
///
/// A `PostToolUse` payload's `tool_response.stderr` is read for all three kinds of evidence; a
/// `PostToolUseFailure` payload's `error`, and a `tool_response` that is one string, hold the
/// command's standard output too, so only their violation block is read. The command itself is
/// never read.
pub fn detect(payload: &serde_json::Value, place: &Place<'_>) -> Vec<Block> {
    detect_with_targets(payload, place)
        .into_iter()
        .map(|(block, _)| block)
        .collect()
}

/// **What a grant could name**, beside each block [`detect`] finds (#1342): the host a refused
/// connection was to, where the report names one, and the whole path of a refused write
/// outside purlis's own state and the protected files. `None` for every other block, and for a
/// connection whose report names no host (Claude Code's proxy says none): the person types it.
///
/// A `tool_response` marked `mixed` holds what the command printed on both streams in its
/// `stderr` (opencode's, #1353): from it only a violation line names a target.
///
/// It goes to the app alone, on the block's line, for the Notice's Allow. It is never kept with
/// the block ([`record`] keeps a [`Block`]), counted, or put in a Report.
pub fn detect_with_targets(
    payload: &serde_json::Value,
    place: &Place<'_>,
) -> Vec<(Block, Option<String>)> {
    let mut found: Vec<(Block, Option<String>)> = Vec::new();
    match &payload["tool_response"] {
        serde_json::Value::String(mixed) => found.extend(violations(mixed, place)),
        response => {
            if let Some(stderr) = response["stderr"].as_str() {
                if response["mixed"].as_bool() == Some(true) {
                    // Standard error and output in one text (opencode's, #1353): what the
                    // command printed may name any host or path, so only a violation line,
                    // which is the sandbox's own, names a target.
                    let own = violations(stderr, place);
                    found.extend(on_stderr(stderr, place).into_iter().map(|(block, target)| {
                        let kept = target.is_some() && own.contains(&(block, target.clone()));
                        (block, if kept { target } else { None })
                    }));
                } else {
                    found.extend(on_stderr(stderr, place));
                }
            }
        }
    }
    if let Some(mixed) = payload["error"].as_str() {
        found.extend(violations(mixed, place));
    }
    let mut blocks: Vec<(Block, Option<String>)> = Vec::new();
    for (block, target) in found {
        if !blocks.iter().any(|(one, _)| *one == block) && blocks.len() < AT_MOST_PER_RESULT {
            blocks.push((block, target));
        }
    }
    blocks
}

/// The target a grant could name for `block`, refused on `named` (a host, or a path read
/// against `place`), where one may be granted at all.
fn target_of(block: &Block, named: &str, place: &Place<'_>) -> Option<String> {
    let named = named.trim();
    if named.is_empty() {
        return None;
    }
    match (block.operation, block.kind) {
        (Operation::Connect, Kind::Host) => Some(named.to_owned()),
        (
            Operation::Write | Operation::File,
            Kind::ProjectFiles | Kind::Home | Kind::ToolchainCache | Kind::System,
        ) => Some(lexical(&place.cwd.join(named)).display().to_string()),
        _ => None,
    }
}

/// What a command's standard error says: its violation block, else a program's own refusals of
/// paths outside what the chat may write; and any proxy's or certificate check's refusal.
fn on_stderr(text: &str, place: &Place<'_>) -> Vec<(Block, Option<String>)> {
    let mut out = violations(text, place);
    // Claude Code's own lines say exactly what was refused; a program's words for the same
    // refusal would only count it again, less well.
    if out.is_empty() {
        let lines: Vec<&str> = text.lines().collect();
        // purlis's own words for a write the chat's sandbox refused it (#1421): purlis's own
        // block, which no grant names, as a violation line naming `purlis` is.
        out.extend(
            lines
                .iter()
                .filter_map(|line| purlis_refusal(line, place))
                .map(|kind| {
                    (
                        Block {
                            operation: Operation::Write,
                            kind,
                            ours: true,
                        },
                        None,
                    )
                }),
        );
        out.extend(
            (0..lines.len())
                .filter(|at| purlis_refusal(lines[*at], place).is_none())
                .filter_map(|at| refused_path(&lines, at, place))
                .filter(|(_, kind, _)| !matches!(kind, Kind::ChatFolder | Kind::Temp))
                // A read there that macOS's privacy controls refuse (`du`, `find`, `ls`) says
                // the same words: only a write is the sandbox's.
                .filter(|(operation, kind, _)| {
                    *operation == Operation::Write
                        || !matches!(kind, Kind::SystemTemp | Kind::SystemCache)
                })
                .map(|(operation, kind, path)| {
                    let block = Block {
                        operation,
                        kind,
                        ours: false,
                    };
                    let target = target_of(&block, &path, place);
                    (block, target)
                }),
        );
    }
    out.extend(
        text.lines()
            .filter_map(network_refusal)
            .map(|(operation, kind, host)| {
                let block = Block {
                    operation,
                    kind,
                    ours: false,
                };
                let target = host.and_then(|host| target_of(&block, &host, place));
                (block, target)
            }),
    );
    out
}

/// The lines of the `<sandbox_violations>` block Claude Code appended: the last one, and only
/// when nothing but space follows it.
fn violations(text: &str, place: &Place<'_>) -> Vec<(Block, Option<String>)> {
    const OPEN: &str = "<sandbox_violations>";
    const CLOSE: &str = "</sandbox_violations>";
    let Some(at) = text.rfind(OPEN) else {
        return Vec::new();
    };
    let inside = &text[at + OPEN.len()..];
    let Some(end) = inside.find(CLOSE) else {
        return Vec::new();
    };
    if !inside[end + CLOSE.len()..].trim().is_empty() {
        return Vec::new();
    }
    inside[..end]
        .lines()
        .filter_map(|line| violation(line, place))
        .collect()
}

/// One Seatbelt line, `cargo(123) deny(1) file-write-create /opt/x`, sorted, and purlis's own
/// only when the process it names is purlis.
fn violation(line: &str, place: &Place<'_>) -> Option<(Block, Option<String>)> {
    let at = line.find("deny(")?;
    let ours = process_of(&line[..at]).is_some_and(crate::cliname::is_recognised);
    let after = &line[at..];
    let after = &after[after.find(')')? + 1..];
    let mut words = after.trim().splitn(2, char::is_whitespace);
    let word = words.next().filter(|word| !word.is_empty())?;
    let target = words.next().unwrap_or("").trim().trim_matches('"');
    let operation = Operation::of_seatbelt(word);
    let kind = match operation {
        Operation::Lookup if target.contains("trustd") => Kind::CertificateCheck,
        Operation::Lookup => Kind::SystemService,
        Operation::Connect if target.starts_with('/') => Kind::LocalSocket,
        Operation::Connect => Kind::Host,
        _ if target.is_empty() => Kind::System,
        _ => kind_of(Path::new(target), place),
    };
    let block = Block {
        operation,
        kind,
        ours,
    };
    // purlis's own operation is a purlis bug, never something to grant.
    let target = (!ours).then(|| target_of(&block, target, place)).flatten();
    Some((block, target))
}

/// The process a Seatbelt line names before its `deny(`: `cargo` of `Sandbox: cargo(123) `.
/// None when the words there are not `name(pid)`.
fn process_of(before: &str) -> Option<&str> {
    let word = before.split_whitespace().next_back()?;
    let (name, pid) = word.strip_suffix(')')?.rsplit_once('(')?;
    (!name.is_empty() && !pid.is_empty() && pid.chars().all(|c| c.is_ascii_digit())).then_some(name)
}

/// The text inside the last pair of quotes (`'`, `` ` `` or `"`) in `field`: a destination comes
/// after its source (`cannot copy 'a' to 'b'`).
fn last_quoted(field: &str) -> Option<&str> {
    for quote in ['\'', '`', '"'] {
        let parts: Vec<&str> = field.split(quote).collect();
        if parts.len() >= 3 {
            return parts.iter().skip(1).step_by(2).copied().next_back();
        }
    }
    None
}

/// Whether `said` names a write: the verb or a stem of one.
fn writes(program: &str, said: &str) -> bool {
    let said = said.to_ascii_lowercase();
    [
        "touch", "mkdir", "cp", "mv", "ln", "rm", "tee", "install", "rmdir", "mktemp",
    ]
    .contains(&program)
        || [
            "lock", "creat", "writ", "renam", "remov", "delet", "copy", "mkdir", "mkstemp",
            "mkdtemp",
        ]
        .iter()
        .any(|stem| said.contains(stem))
}

/// The kind of path a line of purlis's own refusal wording names, where `line` is one: the
/// sandboxed [`crate::rewrite::refused_write`] ("this chat's sandbox refused writing <path>
/// (Operation not permitted). …"), or a sentence carrying [`crate::rewrite::os_words`]'s clause
/// ("… <path> (Operation not permitted: this chat's sandbox refused it) …"). In the second the
/// path is the caller's, read as the last absolute path before the clause (a relative one there
/// is the project's, not the command's folder's); a line that names none is the project's
/// files, which is what purlis writes.
fn purlis_refusal(line: &str, place: &Place<'_>) -> Option<Kind> {
    use crate::rewrite::{NOT_PERMITTED, SANDBOX_REFUSED_IT, SANDBOX_REFUSED_WRITING};
    if let Some(at) = line.find(SANDBOX_REFUSED_WRITING) {
        let rest = &line[at + SANDBOX_REFUSED_WRITING.len()..];
        let path = rest
            .split(NOT_PERMITTED)
            .next()
            .filter(|_| rest.contains(NOT_PERMITTED))?;
        return Some(kind_of(Path::new(path), place));
    }
    let at = line.find(&format!("Operation not permitted: {SANDBOX_REFUSED_IT}"))?;
    let named = line[..at]
        .split_whitespace()
        .rev()
        .map(|word| word.trim_matches(|c: char| "'\"`(),:".contains(c)))
        .find(|word| word.starts_with('/'));
    Some(named.map_or(Kind::ProjectFiles, |path| kind_of(Path::new(path), place)))
}

/// A program's own "Operation not permitted" at line `at` of `lines`, naming its path, sorted.
fn refused_path(lines: &[&str], at: usize, place: &Place<'_>) -> Option<(Operation, Kind, String)> {
    let line = lines[at].trim_end();
    // Node: `EPERM: operation not permitted, mkdir '/opt/x'`.
    if let Some(start) = line.find("EPERM: operation not permitted, ") {
        let rest = &line[start + "EPERM: operation not permitted, ".len()..];
        let (call, quoted) = rest.split_once(' ')?;
        let path = quoted.trim().strip_prefix('\'')?.split('\'').next()?;
        let operation = match call {
            "connect" => return Some((Operation::Connect, Kind::LocalSocket, String::new())),
            "mkdir" | "rmdir" | "rename" | "unlink" | "write" | "copyfile" | "symlink" | "link"
            | "utime" | "chmod" => Operation::Write,
            "scandir" | "read" => Operation::Read,
            _ => Operation::File,
        };
        return Some((operation, kind_of(Path::new(path), place), path.to_owned()));
    }
    // Python: `PermissionError: [Errno 1] Operation not permitted: '/opt/x'`.
    if let Some(start) = line.find("[Errno 1] Operation not permitted: '") {
        let path = line[start + "[Errno 1] Operation not permitted: '".len()..]
            .split('\'')
            .next()?;
        return Some((
            Operation::File,
            kind_of(Path::new(path), place),
            path.to_owned(),
        ));
    }
    let lower = line.to_ascii_lowercase();
    // Rust's io error on a line of its own, its path on a line before (cargo's `Caused by:`):
    // `failed to create directory `/x``, then `Operation not permitted (os error 1)`.
    if lower.trim() == "operation not permitted (os error 1)" {
        let named = lines[..at]
            .iter()
            .rev()
            .take(4)
            .find_map(|before| last_quoted(before).map(|path| (*before, path)))?;
        let operation = if writes("", named.0) {
            Operation::Write
        } else {
            Operation::File
        };
        return Some((
            operation,
            kind_of(Path::new(named.1), place),
            named.1.to_owned(),
        ));
    }
    // A program's own, in any case: `touch: /opt/x: Operation not permitted`,
    // `mkdir: cannot create directory '/opt/x': Operation not permitted`,
    // `fatal: could not create work tree dir 'x': Operation not permitted`,
    // `open /x/y: operation not permitted` (Go), `… `/x`: Operation not permitted (os error 1)`.
    // The line has to END there, so a line quoting one (a test's source, a log) is not one.
    let cut = [
        ": operation not permitted (os error 1)",
        ": operation not permitted",
    ]
    .iter()
    .find_map(|suffix| lower.strip_suffix(suffix).map(str::len))?;
    let before = &line[..cut];
    let (program, said) = match before.split_once(": ") {
        Some((program, said)) if is_program(program) => (program, said),
        _ => ("", before),
    };
    let field = said.rsplit(": ").next().unwrap_or(said);
    let path = match last_quoted(field) {
        Some(quoted) => quoted,
        None => {
            let word = field.rsplit(' ').next().unwrap_or(field);
            if !(word.contains('/') || word.starts_with('.')) {
                return None;
            }
            word
        }
    };
    if path.is_empty() {
        return None;
    }
    let verb = program.rsplit('/').next().unwrap_or(program);
    let operation = if writes(verb, said) {
        Operation::Write
    } else {
        Operation::File
    };
    Some((operation, kind_of(Path::new(path), place), path.to_owned()))
}

/// Whether `word` reads as the program a line begins with: `touch`, `git`, `/usr/bin/cp`, `fatal`.
fn is_program(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/' | '+'))
}

/// A refusal of a connection or of the certificate check, from whichever program said it.
fn network_refusal(line: &str) -> Option<(Operation, Kind, Option<String>)> {
    const REFUSED: [&str; 4] = [
        // purlis's own egress proxy (`sandbox::egress`).
        "purlis's sandbox does not allow ",
        // Claude Code's proxy, in its body and its header.
        "blocked by network allowlist",
        "blocked-by-allowlist",
        // curl's word for the 403 a proxy answered its tunnel with.
        "CONNECT tunnel failed, response 403",
    ];
    if REFUSED.iter().any(|said| line.contains(said)) {
        // purlis's own proxy names the host and port it refused: `… does not allow h:443: …`.
        let host = line
            .split_once(REFUSED[0])
            .and_then(|(_, rest)| rest.split(": ").next())
            .filter(|host| !host.is_empty() && !host.contains(char::is_whitespace))
            .map(str::to_owned);
        return Some((Operation::Connect, Kind::Host, host));
    }
    (line.contains("x509") && line.contains("-26276")).then_some((
        Operation::Lookup,
        Kind::CertificateCheck,
        None,
    ))
}

/// The kind of `path`, read against `place` and never kept.
pub fn kind_of(path: &Path, place: &Place<'_>) -> Kind {
    let path = lexical(&place.cwd.join(path));
    if is_planted(&path) {
        return Kind::ProtectedFile;
    }
    let within = |dir: &Path| {
        let dir = lexical(dir);
        path.starts_with(&dir) || unprivate(&path).starts_with(unprivate(&dir))
    };
    // A manifest where the sandbox holds one: at the root, or in a folder from the chat's up to
    // it (#1336).
    if crate::sandbox::manifests_held(place.root, Some(place.chat))
        .iter()
        .any(|held| unprivate(&path) == unprivate(&lexical(&held.path)))
    {
        return Kind::ProtectedFile;
    }
    // Under either spelling of the state folder: the one the project has may not be the one
    // a chat was refused writing.
    if crate::names::STATE_DIR
        .spellings()
        .any(|name| within(&place.root.join(name)))
    {
        return Kind::ProjectState;
    }
    if within(place.chat) {
        return Kind::ChatFolder;
    }
    if within(place.root) {
        return Kind::ProjectFiles;
    }
    let temp = std::env::temp_dir();
    // Before the temp folders: a hook whose own `TMPDIR` is the per-user folder itself would
    // otherwise take a tool's write there for the chat's own.
    if let Some(parts) = per_user_folder(&unprivate(&path))
        && !(within(&temp)
            && per_user_folder(&unprivate(&lexical(&temp))).is_some_and(|at| at.len() > 1))
    {
        return if parts[0] == "C" {
            Kind::SystemCache
        } else {
            Kind::SystemTemp
        };
    }
    if [
        "/tmp",
        "/private/tmp",
        "/var/folders",
        "/private/var/folders",
    ]
    .iter()
    .any(|dir| within(Path::new(dir)))
        || within(&temp)
    {
        return Kind::Temp;
    }
    if let Some(home) = place.home.filter(|home| within(home)) {
        let inside = unprivate(&path)
            .strip_prefix(unprivate(&lexical(home)))
            .map(Path::to_path_buf)
            .or_else(|_| path.strip_prefix(lexical(home)).map(Path::to_path_buf))
            .unwrap_or_default();
        let first = inside.components().next();
        let caches = [
            ".cargo",
            ".rustup",
            ".npm",
            ".pnpm-store",
            ".yarn",
            ".bun",
            ".deno",
            ".cache",
            ".gradle",
            ".m2",
            "go",
        ];
        let cached = first.is_some_and(|first| caches.iter().any(|c| first.as_os_str() == *c))
            || inside.starts_with("Library/Caches");
        return if cached {
            Kind::ToolchainCache
        } else {
            Kind::Home
        };
    }
    Kind::System
}

/// What `path` names below macOS's per-user temporary or cache folder,
/// `/var/folders/<a>/<b>/T` or `…/C` (what `getconf DARWIN_USER_TEMP_DIR` and
/// `DARWIN_USER_CACHE_DIR` answer), as its parts from that folder on: `["T", "tmp.x"]`. `None`
/// anywhere else. Read from the path's shape alone, so the hook asks the system nothing.
fn per_user_folder(path: &Path) -> Option<Vec<&std::ffi::OsStr>> {
    let mut parts = path.components();
    if parts.next() != Some(Component::RootDir) {
        return None;
    }
    let names: Vec<&std::ffi::OsStr> = parts
        .map(|part| match part {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<Option<_>>()?;
    match names.as_slice() {
        [var, folders, _, _, which, ..]
            if *var == "var" && *folders == "folders" && (*which == "T" || *which == "C") =>
        {
            Some(names[4..].to_vec())
        }
        _ => None,
    }
}

/// Whether `path` is, or is under, a name the sandbox protects wherever it is
/// ([`crate::sandbox::PLANTED`]).
///
/// A lock file git writes beside one (`.git/config.lock`) is that file: git is refused there
/// first.
pub(crate) fn is_planted(path: &Path) -> bool {
    let mut parts: Vec<&str> = path
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    if let Some(last) = parts.last_mut() {
        *last = last.strip_suffix(".lock").unwrap_or(last);
    }
    crate::sandbox::PLANTED.iter().any(|planted| {
        if planted.path.contains("**") {
            return false;
        }
        let wanted: Vec<&str> = planted.path.split('/').collect();
        parts.windows(wanted.len()).enumerate().any(|(at, here)| {
            here == wanted.as_slice()
                && (planted.reach == crate::sandbox::Reach::AndBelow
                    || at + wanted.len() == parts.len())
        })
    })
}

/// `path` with `.` and `..` taken out, without asking the file system.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// macOS's `/private/var/…` and `/var/…` are one folder: the sandbox reports one, a project
/// may be named by the other.
fn unprivate(path: &Path) -> PathBuf {
    match path.strip_prefix("/private") {
        Ok(rest)
            if ["var", "tmp", "etc"]
                .iter()
                .any(|top| rest.starts_with(top)) =>
        {
            Path::new("/").join(rest)
        }
        _ => path.to_path_buf(),
    }
}

// ---- how often the app takes one -------------------------------------------------------------

/// How long one chat's same block is heard once, and the window [`Throttle::PER_CHAT`] counts in.
pub const THROTTLE_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

/// **What the app lets through, per chat** (#1338): the same block once a minute, and at most
/// [`Throttle::PER_CHAT`] blocks a minute in all. A chat holds its own token, so it can send
/// the app as many block lines as it likes; each one kept rewrites the store, and this keeps
/// that to a handful a minute and the real blocks in it from being pushed out.
///
/// **The same block is the same block on the same target** ([`Throttle::lets_on`]): a host
/// refused a minute after another is its own block, with its own Notice to allow it, or the
/// second would have no Allow for a minute. Hosts compare without case.
#[derive(Debug, Default)]
pub struct Throttle {
    heard: std::collections::HashMap<u32, Vec<(Block, Option<String>, std::time::Instant)>>,
}

impl Throttle {
    /// The most blocks one chat is heard in a [`THROTTLE_WINDOW`].
    pub const PER_CHAT: usize = 10;

    /// Whether chat `chat`'s `block`, naming no target, heard at `now`, is let through;
    /// remembered if it is.
    pub fn lets(&mut self, chat: u32, block: &Block, now: std::time::Instant) -> bool {
        self.lets_on(chat, block, None, now)
    }

    /// Whether chat `chat`'s `block` on `target` (the host or path a grant would name, where
    /// the block names one), heard at `now`, is let through; remembered if it is.
    pub fn lets_on(
        &mut self,
        chat: u32,
        block: &Block,
        target: Option<&str>,
        now: std::time::Instant,
    ) -> bool {
        let heard = self.heard.entry(chat).or_default();
        heard.retain(|(_, _, at)| now.saturating_duration_since(*at) < THROTTLE_WINDOW);
        let same = |one: &Block, on: &Option<String>| {
            one == block
                && match (on.as_deref(), target) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                    _ => false,
                }
        };
        if heard.len() >= Self::PER_CHAT || heard.iter().any(|(one, on, _)| same(one, on)) {
            return false;
        }
        heard.push((*block, target.map(str::to_owned), now));
        true
    }
}

// ---- what the app keeps ----------------------------------------------------------------------

/// The file, relative to the project's state folder ([`path`]).
pub const IN_STATE: &str = "app/sandbox-blocks.json";

/// How long a block is kept, and the window `purlis doctor` counts over: seven days.
pub const KEPT_FOR_SECS: u64 = 7 * 24 * 60 * 60;

/// The most blocks the file holds; the oldest go first.
pub const AT_MOST_KEPT: usize = 1000;

/// The file in the project at `root`.
pub fn path(root: &Path) -> PathBuf {
    crate::names::state(root).join(IN_STATE)
}

/// One block as the file keeps it: when the app heard it, in seconds since 1970, and the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    pub at: u64,
    #[serde(flatten)]
    pub block: Block,
}

/// The file's shape. Each block is read on its own, as `reopen`'s `lenient` reads a field: one
/// that does not read as a [`Kept`], such as a kind a newer build added, is kept as it was by
/// [`record`] and let go of by its `at` like any other, and [`counts`] passes over it.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct OnDisk {
    #[serde(default)]
    blocks: Vec<serde_json::Value>,
}

/// When `entry` was heard, if it says.
fn heard_at(entry: &serde_json::Value) -> Option<u64> {
    entry["at"].as_u64()
}

/// Keeps `block`, heard at `at` (seconds since 1970), in the project at `root`, and lets go of
/// every block older than [`KEPT_FOR_SECS`]. Written by the app, under purlis's lock on the
/// directory; a sandboxed chat cannot write it (the integrity class's `.purlis/app/`).
pub fn record(root: &Path, block: &Block, at: u64) -> std::io::Result<()> {
    let path = path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::rewrite::update(root, &path, |now| {
        // A file that is not this shape reads as empty, and the count starts again.
        let mut held: OnDisk = now
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default();
        held.blocks.retain(|entry| {
            heard_at(entry).is_some_and(|then| then.saturating_add(KEPT_FOR_SECS) > at)
        });
        held.blocks
            .push(serde_json::to_value(Kept { at, block: *block }).map_err(std::io::Error::other)?);
        let over = held.blocks.len().saturating_sub(AT_MOST_KEPT);
        held.blocks.drain(..over);
        serde_json::to_string_pretty(&held)
            .map(|text| Some(format!("{text}\n")))
            .map_err(std::io::Error::other)
    })
    .map(|_| ())
}

/// The blocks of one operation over the last seven days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    pub operation: Operation,
    pub blocks: u64,
    /// Of them, purlis's own operations: purlis bugs.
    pub ours: u64,
}

/// The blocks kept in the project at `root` in the seven days before `now`, per operation, in
/// [`Operation::ALL`]'s order, and only the operations that had any.
pub fn counts(root: &Path, now: u64) -> Vec<Count> {
    let held: OnDisk = std::fs::read_to_string(path(root))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let read: Vec<Kept> = held
        .blocks
        .into_iter()
        .filter_map(|entry| serde_json::from_value(entry).ok())
        .collect();
    let recent: Vec<&Kept> = read
        .iter()
        .filter(|kept| kept.at <= now && kept.at.saturating_add(KEPT_FOR_SECS) > now)
        .collect();
    Operation::ALL
        .into_iter()
        .filter_map(|operation| {
            let mine: Vec<&&Kept> = recent
                .iter()
                .filter(|kept| kept.block.operation == operation)
                .collect();
            (!mine.is_empty()).then(|| Count {
                operation,
                blocks: mine.len() as u64,
                ours: mine.iter().filter(|kept| kept.block.ours).count() as u64,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "sandboxblock_tests.rs"]
mod tests;
