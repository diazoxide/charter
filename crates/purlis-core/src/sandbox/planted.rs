//! Where the later-code class (ADR 0067 §5 class 5, rulings V73b and V73d) is found in a plane
//! when a chat starts. Names that are protected wherever they are ([`super::PLANTED`]) need no
//! search. What a protected config points at elsewhere does: the directory a `core.hooksPath`
//! names, and every script a harness's project config runs. Each is denied writing from the
//! chat's start, resolved then.
//!
//! The search is bounded ([`DEPTH`], [`LOOKED_AT`]) and follows no link. Something added after
//! the start, or beyond the bound, is not resolved: the config that would name it is itself
//! never written by the chat, so only a config a person writes later can name something new.

use std::path::{Path, PathBuf};

/// How many directories below the plane the search goes.
pub const DEPTH: usize = 4;

/// How many directory entries the search looks at in all.
pub const LOOKED_AT: usize = 20_000;

/// Directories the search never enters: a repository's own internals, and package trees.
const SKIPPED: [&str; 3] = [".git", "node_modules", "target"];

/// `root` and the directories below it, to [`DEPTH`], without links or [`SKIPPED`] ones.
pub fn directories(root: &Path) -> Vec<PathBuf> {
    let mut found = vec![root.to_path_buf()];
    let mut looked = 0;
    let mut level = vec![root.to_path_buf()];
    for _ in 0..DEPTH {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                looked += 1;
                if looked > LOOKED_AT {
                    return found;
                }
                let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
                let name = entry.file_name();
                if !is_dir || SKIPPED.iter().any(|skipped| name == *skipped) {
                    continue;
                }
                found.push(entry.path());
                next.push(entry.path());
            }
        }
        level = next;
    }
    found
}

/// The directories of `dirs` that hold a `.git`: each a clone or a worktree.
pub fn clones(dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.iter()
        .filter(|dir| dir.join(".git").exists())
        .cloned()
        .collect()
}

/// The git directory of the clone at `clone`, and the common one its config lives in: the
/// `.git` directory, or for a worktree the directory its `.git` file names and that one's
/// `commondir`.
pub fn git_dirs(clone: &Path) -> Vec<PathBuf> {
    let dot_git = clone.join(".git");
    if dot_git.is_dir() {
        return vec![dot_git];
    }
    let Ok(text) = std::fs::read_to_string(&dot_git) else {
        return Vec::new();
    };
    let Some(named) = text.lines().find_map(|line| line.strip_prefix("gitdir:")) else {
        return Vec::new();
    };
    let git_dir = clone.join(named.trim());
    let mut dirs = vec![git_dir.clone()];
    if let Ok(common) = std::fs::read_to_string(git_dir.join("commondir")) {
        dirs.push(git_dir.join(common.trim()));
    }
    dirs
}

/// The submodules' git directories below a clone's `git_dir` (`modules/…`, nested), each one
/// a directory holding a `HEAD`.
pub fn module_git_dirs(git_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut looked = 0;
    let mut level = vec![git_dir.join("modules")];
    while !level.is_empty() && looked < LOOKED_AT {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                looked += 1;
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                let path = entry.path();
                if path.join("HEAD").is_file() {
                    found.push(path.clone());
                    next.push(path.join("modules"));
                } else {
                    next.push(path);
                }
            }
        }
        level = next;
    }
    found
}

/// `core.hooksPath` in the git config `text`, as written.
pub fn hooks_path_in(text: &str) -> Option<String> {
    let mut in_core = false;
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let section = line.trim_start_matches('[').split([']', ' ', '"']).next();
            in_core = section.is_some_and(|it| it.eq_ignore_ascii_case("core"));
            continue;
        }
        if !in_core {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("hookspath") {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|it| it.strip_suffix('"'))
                .unwrap_or(value);
            found = Some(value.to_owned());
        }
    }
    found
}

/// `written`, a path a config names, made absolute against `base` with `~` as `home`.
fn absolute(written: &str, base: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if written.is_empty() || written.contains('$') {
        return None;
    }
    if let Some(rest) = written.strip_prefix("~/") {
        return home.map(|home| home.join(rest));
    }
    let path = Path::new(written);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    // `./` parts said as written, not as a sandbox matches a path.
    Some(
        joined
            .components()
            .filter(|part| !matches!(part, std::path::Component::CurDir))
            .collect(),
    )
}

/// One path resolved for the later-code class, with what named it: the config file and the
/// word in it, as written, so a refusal can say where to look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub path: PathBuf,
    /// The file that names it: a git config, or a harness's project config.
    pub file: PathBuf,
    /// The word in that file that names it.
    pub word: String,
}

/// `path` with each `..` taking off the part before it, as written: what a sandbox makes of
/// `repo/../..` where those folders need not exist to be asked about.
pub(crate) fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            part => out.push(part),
        }
    }
    out
}

/// The directories every `core.hooksPath` that git would use names: each clone's own, and the
/// user's global one, resolved against each clone. Those directories hold the hooks git runs.
pub fn hooks_paths(
    clones: &[PathBuf],
    home: Option<&Path>,
    xdg_config: Option<&Path>,
) -> Vec<Resolved> {
    let read = |file: PathBuf| {
        let text = std::fs::read_to_string(&file).ok()?;
        Some((file, hooks_path_in(&text)?))
    };
    let global: Vec<(PathBuf, String)> = [
        home.map(|home| home.join(".gitconfig")),
        xdg_config.map(|config| config.join("git/config")),
    ]
    .into_iter()
    .flatten()
    .filter_map(read)
    .collect();
    let mut found = Vec::new();
    for clone in clones {
        let own = git_dirs(clone)
            .into_iter()
            .filter_map(|dir| read(dir.join("config")));
        for (file, written) in own.chain(global.iter().cloned()) {
            if let Some(path) = absolute(&written, clone, home) {
                found.push(Resolved {
                    path,
                    file,
                    word: written,
                });
            }
        }
    }
    found
}

/// The project config files whose commands a harness runs, relative to the directory they
/// configure (ruling V73d).
pub const COMMAND_CONFIGS: [&str; 6] = [
    ".claude/settings.json",
    ".claude/settings.local.json",
    ".mcp.json",
    "opencode.json",
    "opencode.jsonc",
    ".codex/config.toml",
];

/// The keys whose values are a command, or a command's words, or a plugin, in those files.
const COMMAND_KEYS: [&str; 4] = ["command", "args", "plugin", "plugins"];

/// What a [`COMMAND_KEYS`] key holds: a line a shell reads, or words handed over as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Line(String),
    Words(Vec<String>),
}

/// Every script the project config files in `dir` name for a harness to run: each word of a
/// command, its arguments or a plugin entry that reads as a path ([`reads_as_a_path`]), made
/// absolute against `dir`.
///
/// **Fail closed** for what is a path (ADR 0067 §5, the later-code class). A word is left out
/// only where it cannot name a script a harness runs: jq's program text, which runs nothing; an
/// operator; where a command's output goes; an option; a URL; a word made only of `/`; and a
/// folder that holds the config, which could not be replaced without the config going with it
/// (#1327, D-1327-8), though a runner pointed at one has the files it loads there named
/// ([`ENTRY_FILES`], D-1327-9). What a shell is handed to run (`-c`), what it reads its commands from (`<`, `<<<`,
/// `<(…)`) and the program text of `awk` and `sed`, which can run commands, are read as
/// commands in turn (D-1327-7).
///
/// A config that names more than purlis follows ([`MOST_MOVES`], [`MOST_NAMED`]) is [`Unread`]:
/// the chat is refused rather than started with what was read so far.
pub fn scripts_named(
    project: &Path,
    dir: &Path,
    home: Option<&Path>,
    writable: &[PathBuf],
) -> Result<Vec<Resolved>, Unread> {
    let mut found = Vec::new();
    for name in COMMAND_CONFIGS {
        if let Ok(text) = std::fs::read_to_string(dir.join(name)) {
            found.extend(scripts_in(project, dir, name, &text, home, writable)?);
        }
    }
    Ok(found)
}

/// A config purlis stopped reading, and the word it stopped at.
pub type Unread = super::Named;

/// The most folder moves (`cd`, `pushd`) followed in one config.
pub const MOST_MOVES: usize = 64;

/// The most paths named from one config.
pub const MOST_NAMED: usize = 1024;

/// The files a runner loads when it is pointed at a folder (D-1327-9): denied in that folder,
/// not below it, where the folder itself is never denied (D-1327-8).
pub const ENTRY_FILES: [&str; 11] = [
    "package.json",
    "index.js",
    "index.mjs",
    "index.cjs",
    "index.ts",
    "main.go",
    "go.mod",
    "__main__.py",
    "main.py",
    "index.php",
    "main.rb",
];

/// Programs that run what a folder holds when pointed at one ([`ENTRY_FILES`]); `go` and `uv`
/// with `run`.
const RUNNERS: [&str; 10] = [
    "node", "deno", "bun", "tsx", "ts-node", "npx", "python", "python3", "ruby", "php",
];

/// [`scripts_named`] for one of the [`COMMAND_CONFIGS`], `name`, in `dir`, that holds `text`,
/// in the project at `project`.
pub fn scripts_in(
    project: &Path,
    dir: &Path,
    name: &str,
    text: &str,
    home: Option<&Path>,
    writable: &[PathBuf],
) -> Result<Vec<Resolved>, Unread> {
    let value: Option<serde_json::Value> = if name.ends_with(".toml") {
        text.parse::<toml::Table>()
            .ok()
            .and_then(|table| serde_json::to_value(table).ok())
    } else {
        serde_json::from_str(&without_comments(text)).ok()
    };
    let mut commands = Vec::new();
    if let Some(value) = value {
        commands_in(&value, false, &mut commands);
    }
    let mut scan = Scan {
        project,
        dir,
        home,
        writable,
        file: dir.join(name),
        dir_text: dir.display().to_string(),
        found: Vec::new(),
        seen: std::collections::HashSet::new(),
        moves: 0,
        unread: None,
    };
    for command in commands {
        let mut bases = Bases::at(dir);
        scan.moves = 0;
        match command {
            Command::Line(line) => scan.line(&line, &mut bases, NESTING),
            Command::Words(words) => scan.segment(&words, &mut bases, NESTING),
        }
    }
    match scan.unread {
        Some(unread) => Err(unread),
        None => Ok(scan.found),
    }
}

/// The folders one command's relative words are found from: every folder it went to, from
/// the one its config is in on, so a script after the shell went back to an earlier folder
/// (`cd -`, `popd`, a closed subshell) is still found where it is. A move is taken from the
/// folder the shell is in, so a chain of them costs one step each, and [`MOST_MOVES`] bounds
/// how many there are.
struct Bases {
    visited: Vec<PathBuf>,
    /// The folder the shell is in now, the one `cd -` goes back to, and `pushd`'s stack.
    now: PathBuf,
    before: PathBuf,
    stack: Vec<PathBuf>,
    /// What each open subshell will go back to.
    scopes: Vec<(PathBuf, PathBuf, Vec<PathBuf>)>,
}

impl Bases {
    fn at(start: &Path) -> Self {
        Self {
            visited: vec![start.to_path_buf()],
            now: start.to_path_buf(),
            before: start.to_path_buf(),
            stack: Vec::new(),
            scopes: Vec::new(),
        }
    }

    /// The shell is now in `folder`.
    fn go(&mut self, folder: PathBuf) {
        self.before = std::mem::replace(&mut self.now, folder);
        if !self.visited.contains(&self.now) {
            self.visited.push(self.now.clone());
        }
    }
}

/// How deep a command handed to another as text (`bash -c '…'`, a quoted word) is read.
const NESTING: usize = 4;

/// Programs whose program text runs nothing, so it names no script: their first operand,
/// unless a `-f` names the file the program is read from (D-1327-7).
const INERT_TEXT: [&str; 2] = ["jq", "gojq"];

/// Shells, whose `-c` operand is commands.
const SHELLS: [&str; 7] = ["sh", "bash", "zsh", "dash", "ksh", "mksh", "fish"];

/// Where [`scripts_in`] gathers what one config names.
struct Scan<'a> {
    /// The project the config is in, whose folders a value may name ([`Scan::values`]).
    project: &'a Path,
    dir: &'a Path,
    home: Option<&'a Path>,
    /// The folders outside the project and the home folder a chat could still write: those
    /// you list as ones chats may be granted, and the project's cache home (D-T56-1).
    writable: &'a [PathBuf],
    file: PathBuf,
    dir_text: String,
    found: Vec<Resolved>,
    seen: std::collections::HashSet<PathBuf>,
    /// The folder moves followed so far in the command being read.
    moves: usize,
    /// Where reading stopped, past [`MOST_MOVES`] or [`MOST_NAMED`].
    unread: Option<Unread>,
}

impl Scan<'_> {
    /// A line a shell reads, with `bases` the folders its relative words are found from.
    fn line(&mut self, line: &str, bases: &mut Bases, depth: usize) {
        for segment in shell_segments(line) {
            match segment {
                Segment::Words(words) => self.segment(&words, bases, depth),
                Segment::Open => bases.scopes.push((
                    bases.now.clone(),
                    bases.before.clone(),
                    bases.stack.clone(),
                )),
                Segment::Close => {
                    if let Some((now, before, stack)) = bases.scopes.pop() {
                        bases.now = now;
                        bases.before = before;
                        bases.stack = stack;
                    }
                }
            }
        }
    }

    /// One command's words.
    fn segment(&mut self, segment: &[String], bases: &mut Bases, depth: usize) {
        if self.unread.is_some() {
            return;
        }
        let read = read_segment(segment);
        for one in read.moves {
            self.moves += 1;
            if self.moves > MOST_MOVES {
                self.stop(&one.word);
                return;
            }
            match one.kind {
                MoveKind::To | MoveKind::Push => {
                    let written = self.substituted(&one.word);
                    if let Some(path) = absolute(&written, &bases.now, self.home) {
                        if one.kind == MoveKind::Push {
                            bases.stack.push(bases.now.clone());
                        }
                        bases.go(path);
                    }
                }
                MoveKind::Back => {
                    let before = bases.before.clone();
                    bases.go(before);
                }
                MoveKind::Pop => {
                    if let Some(folder) = bases.stack.pop() {
                        bases.go(folder);
                    }
                }
            }
        }
        for word in read.words {
            self.word(&word, bases, depth, read.runner);
        }
        if let Some(depth) = depth.checked_sub(1) {
            for text in read.commands {
                self.line(&text, bases, depth);
            }
        }
    }

    /// One word: named where it reads as a path, and read again as a line where a shell would
    /// split it, as a quoted operand or an argument handed over whole may be.
    /// One word of a command whose program is a runner (`runner`) or not.
    fn word(&mut self, word: &str, bases: &mut Bases, depth: usize, runner: bool) {
        if self.unread.is_some() {
            return;
        }
        for written in self.values(word) {
            self.named_as(&written, word, bases, runner);
        }
        let split_again = word.contains(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '|' | ';' | '&' | '<' | '>' | '(' | ')' | '"' | '\'' | '`'
                )
        });
        if split_again && let Some(depth) = depth.checked_sub(1) {
            self.line(word, bases, depth);
        }
    }

    /// `written`, one value of `word`, named where it reads as a path.
    fn named_as(&mut self, written: &str, word: &str, bases: &Bases, runner: bool) {
        let folder_word = runner && matches!(written, "." | "..");
        if reads_as_a_path(written) || folder_word {
            for base in bases.visited.clone() {
                let Some(path) = absolute(written, &base, self.home) else {
                    continue;
                };
                // A folder that holds the config is no script: replaced, it would take the
                // config naming it along (D-1327-8). What a runner loads from it is (D-1327-9).
                if self.dir.starts_with(lexical(&path)) {
                    if runner {
                        for entry in ENTRY_FILES {
                            self.name(path.join(entry), word);
                        }
                    }
                    continue;
                }
                // As written and with `..` taken off, which a sandbox matches when the folder
                // before the `..` is missing.
                let plain = lexical(&path);
                self.name(path, word);
                self.name(plain, word);
            }
        }
    }

    /// `path`, named by `word`, once; past [`MOST_NAMED`] the config is unread.
    fn name(&mut self, path: PathBuf, word: &str) {
        if !self.seen.insert(path.clone()) {
            return;
        }
        if self.found.len() >= MOST_NAMED {
            self.stop(word);
            return;
        }
        self.found.push(Resolved {
            path,
            file: self.file.clone(),
            word: word.to_owned(),
        });
    }

    /// Stops reading at `word`.
    fn stop(&mut self, word: &str) {
        self.unread.get_or_insert_with(|| Unread {
            file: self.file.clone(),
            word: word.to_owned(),
        });
    }

    /// `word` as the harness would see it: a `file://` plugin as its path, and the project
    /// folder's variable as the folder.
    fn substituted(&self, word: &str) -> String {
        word.trim_start_matches("file://")
            .replace("${CLAUDE_PROJECT_DIR}", &self.dir_text)
            .replace("$CLAUDE_PROJECT_DIR", &self.dir_text)
    }

    /// What `word` may hand its program as a file ([`Self::substituted`]): an option's value
    /// after `=` (`--flag=value`) or after a JVM agent's colon, or each value it may have
    /// written against a short option ([`attached_values`]), as a separated value would be;
    /// any other word as it is (#1356).
    ///
    /// A value taken out of a word that starts with `~` is named as written too, below the
    /// folder the command runs in: a shell expands `~` only at a word's start, and bash after
    /// an option's `=` as well, so the program may open either (D-1356-8).
    fn values(&self, word: &str) -> Vec<String> {
        let written = self.substituted(word);
        let taken: Vec<String> = if let Some(value) = COLON_OPTIONS
            .iter()
            .find_map(|option| written.strip_prefix(option))
        {
            // A JVM agent's file, and its options after `=`, which can name files.
            value.splitn(2, '=').map(str::to_owned).collect()
        } else if let Some((flag, value)) = written.split_once('=')
            && flag.starts_with('-')
        {
            vec![value.to_owned()]
        } else {
            attached_values(&written)
                .into_iter()
                .filter(|(value, first)| *first || !value.starts_with('/') || self.holds(value))
                .map(|(value, _)| value)
                .collect()
        };
        if taken.is_empty() {
            return vec![written];
        }
        let mut out = Vec::new();
        for value in taken {
            if value.starts_with('~') {
                out.push(format!("./{value}"));
            }
            out.push(value);
        }
        out
    }

    /// Whether the absolute `path` is where a chat could write it: in the project, the home
    /// folder, a folder you list as one chats may be granted, or the project's cache home
    /// (D-1356-7, D-T56-1). Counting too many only names more files, which fails closed.
    fn holds(&self, path: &str) -> bool {
        let path = lexical(Path::new(path));
        path.starts_with(self.project)
            || self.home.is_some_and(|home| path.starts_with(home))
            || self.writable.iter().any(|dir| path.starts_with(dir))
    }
}

/// Options whose value is written after a colon: a JVM's agents (`-javaagent:<jar>[=<options>]`).
const COLON_OPTIONS: [&str; 3] = ["-javaagent:", "-agentpath:", "-agentlib:"];

/// The values a cluster of short options may have written against one of them, `-L./lib` or
/// `-bf./x.awk`: what follows each of its leading letters, where that holds a `/`, and whether
/// it follows the first letter.
///
/// **Fail closed** (#1356, D-1356-1). Which letter takes a value is the program's to say, so
/// every split is read. A relative split that is no value names a file below the config's
/// folder that nobody runs, and denies a chat nothing it needs. An absolute split after a
/// later letter could name a real folder elsewhere (`-Ivendor/tmp` names `/tmp`), so
/// [`Scan::values`] keeps one only where a chat could write it, in the project, the home
/// folder, a folder chats may be granted or the project's cache home (D-1356-7, D-T56-1).
fn attached_values(word: &str) -> Vec<(String, bool)> {
    let Some(short) = word
        .strip_prefix('-')
        .filter(|short| !short.starts_with('-'))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (at, letter) in short.char_indices() {
        if !letter.is_ascii_alphanumeric() {
            break;
        }
        let value = &short[at + letter.len_utf8()..];
        if !value.contains('/') {
            break;
        }
        out.push((value.to_owned(), at == 0));
    }
    out
}

/// Whether `word` is a cluster of short options whose leading letters hold `letter`, with or
/// without a value written against the last of them (`-rf`, `-rf./x.jq`).
fn short_leads_with(word: &str, letter: char) -> bool {
    word.strip_prefix('-')
        .filter(|short| !short.starts_with('-'))
        .is_some_and(|short| {
            short
                .chars()
                .take_while(char::is_ascii_alphabetic)
                .any(|it| it == letter)
        })
}

/// One command's words, sorted by what they are to a script search.
#[derive(Debug, Default)]
struct Read {
    /// Words that may name a script.
    words: Vec<String>,
    /// Text handed to a shell to run (`-c`), read as commands.
    commands: Vec<String>,
    /// Where the commands after this one run (`cd`, `pushd`, `popd`).
    moves: Vec<Move>,
    /// Whether the program runs what a folder holds when pointed at one ([`RUNNERS`]).
    runner: bool,
}

/// A change of the folder later commands run in.
#[derive(Debug)]
struct Move {
    kind: MoveKind,
    /// The word that made it: the folder, or `-` or `popd`.
    word: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MoveKind {
    /// `cd <folder>`.
    To,
    /// `pushd <folder>`.
    Push,
    /// `cd -`, to the folder before.
    Back,
    /// `popd`.
    Pop,
}

/// Whether `word` is a cluster of short options (`-rf`) that holds `letter`; one with a value
/// written against it (`-L./lib`) is not.
fn short_holds(word: &str, letter: char) -> bool {
    word.strip_prefix('-').is_some_and(|short| {
        !short.is_empty()
            && short.chars().all(|c| c.is_ascii_alphabetic())
            && short.contains(letter)
    })
}

/// `segment`'s words, sorted ([`Read`]).
fn read_segment(segment: &[String]) -> Read {
    let mut read = Read::default();
    // Variables set for the command come first, and are words like any other.
    let assigning = |word: &String| {
        word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        })
    };
    let at = segment.iter().take_while(|word| assigning(word)).count();
    read.words.extend(segment[..at].iter().cloned());
    let Some(program) = segment.get(at) else {
        return read;
    };
    let rest = &segment[at + 1..];
    let name = program.rsplit('/').next().unwrap_or(program);
    read.words.push(program.clone());
    read.runner = RUNNERS.contains(&name)
        || (matches!(name, "go" | "uv") && rest.first().is_some_and(|word| word == "run"));
    let mut words = rest.iter();
    if name == "popd" {
        read.moves.push(Move {
            kind: MoveKind::Pop,
            word: program.clone(),
        });
        return read;
    }
    if matches!(name, "cd" | "pushd") {
        for word in words {
            if word == "-" && name == "cd" {
                read.moves.push(Move {
                    kind: MoveKind::Back,
                    word: word.clone(),
                });
                break;
            }
            if word.starts_with('-') {
                continue;
            }
            read.moves.push(Move {
                kind: if name == "cd" {
                    MoveKind::To
                } else {
                    MoveKind::Push
                },
                word: word.clone(),
            });
            break;
        }
        return read;
    }
    if name == "git" {
        while let Some(word) = words.next() {
            if word == "-C" {
                // The folder git runs in, which no later command does.
                words.next();
            } else {
                read.words.push(word.clone());
            }
        }
        return read;
    }
    if SHELLS.contains(&name) {
        while let Some(word) = words.next() {
            if short_holds(word, 'c') {
                read.commands.extend(words.next().cloned());
            } else {
                read.words.push(word.clone());
            }
        }
        return read;
    }
    if INERT_TEXT.contains(&name) {
        let from_file = rest.iter().any(|word| {
            word == "--from-file" || word.starts_with("--from-file=") || short_leads_with(word, 'f')
        });
        let mut text_given = from_file;
        while let Some(word) = words.next() {
            match word.as_str() {
                "--arg" | "--argjson" | "--slurpfile" | "--rawfile" => {
                    read.words.extend(words.by_ref().take(2).cloned());
                }
                "--from-file" | "--indent" | "-L" => read.words.extend(words.next().cloned()),
                _ if short_holds(word, 'f') || short_holds(word, 'L') => {
                    read.words.push(word.clone());
                    read.words.extend(words.next().cloned());
                }
                _ if word.starts_with('-') => read.words.push(word.clone()),
                // jq's program, which runs nothing.
                _ if !text_given => text_given = true,
                _ => read.words.push(word.clone()),
            }
        }
        return read;
    }
    read.words.extend(rest.iter().cloned());
    read
}

/// Whether `word`, as a shell left it, reads as a path a harness could run: it holds a `/`, and
/// is not only `/`s, a URL, an option or a device. A space is a path's only where no part of it
/// between `/`s starts or ends with one, as `.a // .b` does and `tools/my hook.sh` does not.
///
/// Any other word is kept, odd characters and all: one that is not a path names a file nobody
/// runs, which costs nothing, while one left out could be a script a chat may then write.
fn reads_as_a_path(word: &str) -> bool {
    word.contains('/')
        && !word.chars().all(|c| c == '/')
        && !word.contains("://")
        && !word.starts_with('-')
        && !word.starts_with("/dev/")
        && word.split('/').all(|part| {
            part.trim() == part && !(part.is_empty() && word.contains(char::is_whitespace))
        })
}

/// `line` as a POSIX shell splits it: each simple command's words, with quotes taken off, and
/// without its operators (`|`, `&&`, `;`, …) or its redirections and their targets.
fn shell_segments(line: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut redirected = false;
    let mut chars = line.chars().peekable();
    let finish =
        |word: &mut String, in_word: &mut bool, redirected: &mut bool, words: &mut Vec<String>| {
            if *in_word {
                if *redirected {
                    *redirected = false;
                } else {
                    words.push(std::mem::take(word));
                }
            }
            word.clear();
            *in_word = false;
        };
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    word.push(c);
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(next) = chars.next() {
                                word.push(next);
                            }
                        }
                        _ => word.push(c),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            '>' | '<' => {
                // A file descriptor's number before the operator is part of it.
                if in_word && word.chars().all(|c| c.is_ascii_digit()) {
                    word.clear();
                    in_word = false;
                }
                finish(&mut word, &mut in_word, &mut redirected, &mut words);
                if chars.peek() == Some(&'(') {
                    // `<(…)` and `>(…)`: commands, read as such.
                    continue;
                }
                let mut operator = c.to_string();
                while let Some(next) = chars.next_if(|c| matches!(c, '>' | '<' | '&' | '|' | '-')) {
                    operator.push(next);
                }
                // Where output goes, a descriptor, and a here-document's delimiter name nothing
                // that runs. What a command reads (`<`, `<<<`, `<>`) may be what a shell runs.
                redirected =
                    c == '>' || operator.contains('&') || matches!(operator.as_str(), "<<" | "<<-");
            }
            '|' | ';' | '&' | '\n' | '(' | ')' => {
                finish(&mut word, &mut in_word, &mut redirected, &mut words);
                redirected = false;
                if c == '&' && chars.peek() == Some(&'>') {
                    continue;
                }
                if !words.is_empty() {
                    segments.push(Segment::Words(std::mem::take(&mut words)));
                }
                match c {
                    '(' => segments.push(Segment::Open),
                    ')' => segments.push(Segment::Close),
                    _ => {}
                }
            }
            c if c.is_whitespace() => {
                finish(&mut word, &mut in_word, &mut redirected, &mut words);
            }
            _ => {
                in_word = true;
                word.push(c);
            }
        }
    }
    finish(&mut word, &mut in_word, &mut redirected, &mut words);
    if !words.is_empty() {
        segments.push(Segment::Words(words));
    }
    segments
}

/// One piece of a line a shell reads: a simple command's words, or where a subshell opens or
/// closes, after which the folder is again the one it was before.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Words(Vec<String>),
    Open,
    Close,
}

/// The commands under a [`COMMAND_KEYS`] key anywhere in `value`: a string as a line a shell
/// reads, a list of strings as words handed over as they are.
fn commands_in(value: &serde_json::Value, under: bool, into: &mut Vec<Command>) {
    match value {
        serde_json::Value::String(text) if under => into.push(Command::Line(text.clone())),
        serde_json::Value::Array(items)
            if under && !items.is_empty() && items.iter().all(serde_json::Value::is_string) =>
        {
            into.push(Command::Words(
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect(),
            ));
        }
        serde_json::Value::Array(items) => {
            for item in items {
                commands_in(item, under, into);
            }
        }
        serde_json::Value::Object(map) => {
            // An MCP server's program and its arguments are one command: a shell's `-c` is
            // among the arguments.
            if let (Some(serde_json::Value::String(program)), Some(serde_json::Value::Array(args))) =
                (map.get("command"), map.get("args"))
                && args.iter().all(serde_json::Value::is_string)
            {
                let mut words = vec![program.clone()];
                words.extend(
                    args.iter()
                        .filter_map(|arg| arg.as_str().map(str::to_owned)),
                );
                into.push(Command::Words(words));
                for (key, item) in map {
                    if key != "command" && key != "args" {
                        commands_in(item, under || COMMAND_KEYS.contains(&key.as_str()), into);
                    }
                }
                return;
            }
            for (key, item) in map {
                commands_in(item, under || COMMAND_KEYS.contains(&key.as_str()), into);
            }
        }
        _ => {}
    }
}

/// `text` without `//` line comments, which `opencode.jsonc` may hold. A `//` inside a string,
/// as in a URL, is kept.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut in_string = false;
        let mut escaped = false;
        let mut cut = line.len();
        let bytes = line.as_bytes();
        for (at, byte) in bytes.iter().enumerate() {
            match byte {
                _ if escaped => escaped = false,
                b'\\' if in_string => escaped = true,
                b'"' => in_string = !in_string,
                b'/' if !in_string && bytes.get(at + 1) == Some(&b'/') => {
                    cut = at;
                    break;
                }
                _ => {}
            }
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// Everything resolved for the plane at `root` at a chat's start: what `core.hooksPath` names
/// and every script a harness's project config runs, in the plane and the clones in it.
/// `writable` is where a chat could write outside the project and the home folder
/// ([`Scan::holds`]).
pub fn resolved(
    root: &Path,
    home: Option<&Path>,
    xdg_config: Option<&Path>,
    writable: &[PathBuf],
) -> Result<Vec<Resolved>, Unread> {
    let dirs = directories(root);
    let clones = clones(&dirs);
    let mut found = hooks_paths(&clones, home, xdg_config);
    for dir in &dirs {
        found.extend(scripts_named(root, dir, home, writable)?);
    }
    let mut seen = std::collections::HashSet::new();
    found.retain(|it| seen.insert(it.path.clone()));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hooks_path_is_read_from_the_core_section_in_any_case_and_quoting() {
        assert_eq!(
            hooks_path_in(
                "[user]\n\thooksPath = no\n[core]\n\tbare = false\n\tHooksPath = \"tools/hooks\"\n"
            ),
            Some("tools/hooks".to_owned())
        );
        assert_eq!(hooks_path_in("[core]\n\tbare = false\n"), None);
        assert_eq!(
            hooks_path_in("[core \"x\"]\nhookspath=h\n"),
            Some("h".to_owned())
        );
    }

    #[test]
    fn every_hooks_path_git_would_use_is_resolved_against_its_clone() {
        let plane = tempfile::tempdir().expect("a plane");
        let clone = plane.path().join("ws/repo");
        std::fs::create_dir_all(clone.join(".git")).expect("a clone");
        std::fs::write(
            clone.join(".git/config"),
            "[core]\n\thooksPath = scripts/hooks\n",
        )
        .expect("its config");
        let home = tempfile::tempdir().expect("a home");
        std::fs::write(
            home.path().join(".gitconfig"),
            "[core]\n\thooksPath = ~/my-hooks\n",
        )
        .expect("a global config");
        let found: Vec<PathBuf> = resolved(plane.path(), Some(home.path()), None, &[])
            .expect("read")
            .into_iter()
            .map(|it| it.path)
            .collect();
        assert!(found.contains(&clone.join("scripts/hooks")), "{found:?}");
        assert!(found.contains(&home.path().join("my-hooks")), "{found:?}");
    }

    #[test]
    fn a_worktree_s_hooks_path_is_read_from_its_common_git_directory() {
        let plane = tempfile::tempdir().expect("a plane");
        let main = plane.path().join("main");
        std::fs::create_dir_all(main.join(".git/worktrees/w")).expect("a main clone");
        std::fs::write(main.join(".git/config"), "[core]\nhooksPath = /abs/hooks\n")
            .expect("its config");
        std::fs::write(main.join(".git/worktrees/w/commondir"), "../..\n").expect("commondir");
        let worktree = plane.path().join("w");
        std::fs::create_dir_all(&worktree).expect("a worktree");
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/w").display()),
        )
        .expect("its .git file");
        let found = hooks_paths(&[worktree], None, None);
        assert_eq!(
            found,
            [Resolved {
                path: PathBuf::from("/abs/hooks"),
                file: main.join(".git/worktrees/w/../../config"),
                word: "/abs/hooks".to_owned(),
            }]
        );
    }

    #[test]
    fn every_script_a_harness_s_project_config_runs_is_named() {
        let dir = tempfile::tempdir().expect("a project");
        let at = |rel: &str| dir.path().join(rel);
        std::fs::create_dir_all(at(".claude")).expect(".claude");
        std::fs::write(
            at(".claude/settings.json"),
            r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/stop.sh --x"}]}]},
                "statusLine": {"command": "./bin/status.sh"}}"#,
        )
        .expect("settings");
        std::fs::write(
            at(".mcp.json"),
            r#"{"mcpServers": {"x": {"command": "node", "args": ["tools/server.js", "--port=3"]}}}"#,
        )
        .expect(".mcp.json");
        std::fs::write(
            at("opencode.jsonc"),
            "{\n // a comment\n \"plugin\": [\"file://./plugins/p.ts\", \"some-npm-plugin\"],\n \"mcp\": {\"y\": {\"command\": [\"bun\", \"run\", \"mcp/y.ts\"], \"url\": \"https://x.test/a\"}}\n}",
        )
        .expect("opencode.jsonc");
        std::fs::create_dir_all(at(".codex")).expect(".codex");
        std::fs::write(
            at(".codex/config.toml"),
            "[mcp_servers.z]\ncommand = \"/opt/z/run\"\nargs = [\"-c\", \"scripts/z.py\"]\n",
        )
        .expect("config.toml");
        let found: Vec<PathBuf> = scripts_named(dir.path(), dir.path(), None, &[])
            .expect("read")
            .into_iter()
            .map(|it| it.path)
            .collect();
        for want in [
            at(".claude/hooks/stop.sh"),
            at("bin/status.sh"),
            at("tools/server.js"),
            at("plugins/p.ts"),
            at("mcp/y.ts"),
            PathBuf::from("/opt/z/run"),
            at("scripts/z.py"),
        ] {
            assert!(found.contains(&want), "{want:?} not in {found:?}");
        }
        assert!(
            !found.iter().any(|it| it.ends_with("some-npm-plugin")),
            "{found:?}"
        );
    }

    /// What `scripts_named` finds for a project whose `.claude/settings.json` hook runs
    /// `command`.
    fn named_by_hook(command: &str) -> (PathBuf, Vec<PathBuf>) {
        let dir = PathBuf::from("/plane/ws/repo");
        let settings = serde_json::json!({"hooks": {"PostToolUse": [{"hooks": [
            {"type": "command", "command": command}
        ]}]}});
        let found = scripts_in(
            Path::new("/plane"),
            &dir,
            ".claude/settings.json",
            &settings.to_string(),
            None,
            &[],
        )
        .expect("read")
        .into_iter()
        .map(|named| named.path)
        .collect();
        (dir, found)
    }

    /// What `scripts_in` names for the `.mcp.json` server `server`, as JSON.
    fn named_by_server(server: &str) -> Vec<PathBuf> {
        let text = format!(r#"{{"mcpServers": {{"x": {server}}}}}"#);
        scripts_in(
            Path::new("/plane"),
            Path::new("/plane/ws/repo"),
            ".mcp.json",
            &text,
            None,
            &[],
        )
        .expect("read")
        .into_iter()
        .map(|named| named.path)
        .collect()
    }

    /// `found` with each path once, in order.
    fn once(found: Vec<PathBuf>) -> Vec<PathBuf> {
        let mut seen = std::collections::BTreeSet::new();
        found
            .into_iter()
            .filter(|it| seen.insert(it.clone()))
            .collect()
    }

    #[test]
    fn jq_s_alternative_operator_is_not_a_script_path() {
        // #1327: seen on a real project, whose every chat it made read-only.
        let (_dir, found) = named_by_hook(
            "jq -r '.tool_response.filePath // .tool_input.file_path' | xargs -r prettier --write",
        );
        assert_eq!(found, Vec::<PathBuf>::new());
        let (_dir, found) = named_by_hook("jq -r .a // .b");
        assert_eq!(found, Vec::<PathBuf>::new());
        let (_dir, found) = named_by_hook("bash -c \"jq -r '.a // .b' | xargs ls\"");
        assert_eq!(found, Vec::<PathBuf>::new());
    }

    #[test]
    fn jq_program_text_operators_and_output_redirections_are_not_script_paths() {
        for command in [
            "awk -F/ '{print $NF}' x",
            "jq '.a / 2'",
            "jq -L ./jqlib --arg n v '.a // .b'",
            "awk '{ print a \" // \" b }'",
            "grep -q x 2>/dev/null || true",
            "cat > /dev/null",
            "echo hi >/tmp/out",
            "echo hi >> ./log/out.txt 2>&1",
            "test -f x && exit 0",
        ] {
            let (_dir, found) = named_by_hook(command);
            let found: Vec<_> = found
                .into_iter()
                .filter(|it| !it.ends_with("jqlib"))
                .collect();
            assert_eq!(found, Vec::<PathBuf>::new(), "{command}");
        }
    }

    #[test]
    fn the_folder_a_command_works_in_is_not_a_script_path() {
        for command in [
            "cd \"$CLAUDE_PROJECT_DIR\" && npm run lint",
            "pushd \"$CLAUDE_PROJECT_DIR\"",
            "git -C \"$CLAUDE_PROJECT_DIR\" diff",
            // A folder that holds the config is never one a harness runs: were it replaced, the
            // config naming it would go with it.
            "prettier --write ./",
            "eslint \"$CLAUDE_PROJECT_DIR\"",
            "ls ../..",
        ] {
            let (_dir, found) = named_by_hook(command);
            assert_eq!(found, Vec::<PathBuf>::new(), "{command}");
        }
        // A script after a `cd` is named where it is found from either folder.
        let (dir, found) = named_by_hook("cd ./tools && ./fmt.sh");
        assert_eq!(found, [dir.join("fmt.sh"), dir.join("tools/fmt.sh")]);
    }

    #[test]
    fn a_real_script_path_in_a_hook_is_still_named() {
        // ADR 0067 §5, the later-code class: a script a hook runs is never the chat's to write.
        for (command, want) in [
            ("./scripts/x.sh", "scripts/x.sh"),
            (
                "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/x.sh --flag",
                ".claude/hooks/x.sh",
            ),
            ("bash 'tools/my hook.sh'", "tools/my hook.sh"),
            ("jq -r '.a' | ./bin/fmt.sh", "bin/fmt.sh"),
            ("sed -f ./scripts/fix.sed x", "scripts/fix.sed"),
            ("node tools/server.js 2>/dev/null", "tools/server.js"),
            ("jq -rf ./filters/x.jq", "filters/x.jq"),
            ("jq --from-file ./filters/y.jq", "filters/y.jq"),
            ("awk -f ./tools/sum.awk data", "tools/sum.awk"),
            ("'./scripts/run [1].sh'", "scripts/run [1].sh"),
            ("./scripts/a&b.sh", "scripts/a"),
            // What a shell reads its commands from is a script, however it is handed over.
            ("bash < ./scripts/x.sh", "scripts/x.sh"),
            (
                "sh < \"$CLAUDE_PROJECT_DIR\"/.claude/hooks/y.sh",
                ".claude/hooks/y.sh",
            ),
            ("python3 - < tools/hook.py", "tools/hook.py"),
            ("bash <<< ./scripts/here.sh", "scripts/here.sh"),
            ("bash <(./scripts/gen.sh)", "scripts/gen.sh"),
            ("diff <(./scripts/a.sh) - > /dev/null", "scripts/a.sh"),
            // An option's value that names a file the program loads.
            ("gawk -E ./tools/x.awk", "tools/x.awk"),
            ("gawk -i ./tools/lib.awk '{ print }'", "tools/lib.awk"),
            ("gawk -l ./tools/ext '{ print }'", "tools/ext"),
            // awk and sed run commands from their program text (D-1327-7).
            ("awk '{ system(\"./scripts/a.sh\") }'", "scripts/a.sh"),
            (
                "awk 'BEGIN { \"./scripts/b.sh --x\" | getline v }'",
                "scripts/b.sh",
            ),
            ("awk '{ print | \"./scripts/c.sh\" }' f", "scripts/c.sh"),
            ("sed -n 'e ./scripts/d.sh' f", "scripts/d.sh"),
        ] {
            let (dir, found) = named_by_hook(command);
            assert!(found.contains(&dir.join(want)), "{command}: {found:?}");
        }
        let (_dir, found) = named_by_hook("/opt/hooks/check && /usr/local/bin/lint.sh");
        assert_eq!(
            found,
            [
                PathBuf::from("/opt/hooks/check"),
                PathBuf::from("/usr/local/bin/lint.sh")
            ]
        );
    }

    #[test]
    fn what_a_shell_is_handed_with_c_is_read_as_commands() {
        let dir = Path::new("/plane/ws/repo");
        assert_eq!(
            once(named_by_server(
                r#"{"command": "bash", "args": ["-c", "node ./tools/server.js"]}"#
            )),
            [dir.join("tools/server.js")]
        );
        assert_eq!(
            once(named_by_server(
                r#"{"command": "sh", "args": ["-lc", "./a.sh && ./b.sh"]}"#
            )),
            [dir.join("a.sh"), dir.join("b.sh")]
        );
        let opencode = scripts_in(
            Path::new("/plane"),
            dir,
            "opencode.json",
            r#"{"mcp": {"y": {"command": ["bash", "-c", "./scripts/oc.sh --x"]}}}"#,
            None,
            &[],
        )
        .expect("read");
        assert_eq!(
            once(opencode.into_iter().map(|it| it.path).collect()),
            [dir.join("scripts/oc.sh")]
        );
        let (_, found) = named_by_hook("bash -c \"./scripts/a.sh && ./scripts/b.sh\"");
        assert_eq!(
            once(found),
            [dir.join("scripts/a.sh"), dir.join("scripts/b.sh")]
        );
        let (_, found) = named_by_hook("zsh -ic './scripts/z.sh'");
        assert_eq!(once(found), [dir.join("scripts/z.sh")]);
    }

    #[test]
    fn each_script_is_named_with_the_file_and_the_word_that_name_it() {
        let dir = Path::new("/plane/ws/repo");
        let named = scripts_in(
            Path::new("/plane"),
            dir,
            ".mcp.json",
            r#"{"mcpServers": {"x": {"command": "node", "args": ["--inspect", "tools/my server.js"]}}}"#,
            None, &[]
        )
        .expect("read");
        let want = Resolved {
            path: dir.join("tools/my server.js"),
            file: dir.join(".mcp.json"),
            word: "tools/my server.js".to_owned(),
        };
        assert!(named.contains(&want), "{named:?}");
    }

    #[test]
    fn a_long_chain_of_folder_moves_is_read_at_once_and_names_a_bounded_set() {
        let dir = Path::new("/plane/ws/repo");
        let moves: Vec<String> = (0..30).map(|at| format!("cd ./d{at}")).collect();
        let command = format!("{} && ./x.sh", moves.join(" && "));
        let started = std::time::Instant::now();
        let (_, found) = named_by_hook(&command);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(100),
            "{:?}",
            started.elapsed()
        );
        let deepest: PathBuf = (0..30).fold(dir.to_path_buf(), |at, n| at.join(format!("d{n}")));
        // Found from every folder the command went through: one path each, no more.
        assert_eq!(found.len(), 31, "{found:?}");
        assert!(found.contains(&deepest.join("x.sh")), "{found:?}");
    }

    #[test]
    fn a_script_after_the_shell_returns_to_an_earlier_folder_is_named_where_it_is() {
        let dir = Path::new("/plane/ws/repo");
        for (command, want) in [
            ("cd sub && (cd tools && make) && ./run.sh", "sub/run.sh"),
            ("(cd a && cd b); cd c; ./x.sh", "c/x.sh"),
            ("cd a && cd b && cd - && ./x.sh", "a/x.sh"),
            ("pushd a && pushd b && popd && ./x.sh", "a/x.sh"),
            ("cd a && cd ../b && ./x.sh", "b/x.sh"),
        ] {
            let (_, found) = named_by_hook(command);
            assert!(found.contains(&dir.join(want)), "{command}: {found:?}");
        }
    }

    #[test]
    fn folder_moves_are_counted_per_command() {
        let dir = Path::new("/plane/ws/repo");
        let hooks: Vec<serde_json::Value> = (0..100)
            .map(|at| {
                serde_json::json!({"type": "command",
                    "command": format!("cd \"$CLAUDE_PROJECT_DIR\" && ./scripts/h{at}.sh")})
            })
            .collect();
        let settings = serde_json::json!({"hooks": {"Stop": [{"hooks": hooks}]}});
        let found = scripts_in(
            Path::new("/plane"),
            dir,
            ".claude/settings.json",
            &settings.to_string(),
            None,
            &[],
        )
        .expect("every hook read");
        assert_eq!(found.len(), 100);
        assert!(found.iter().any(|it| it.path == dir.join("scripts/h99.sh")));
    }

    #[test]
    fn past_the_most_folder_moves_a_config_is_unread_rather_than_half_read() {
        let dir = Path::new("/plane/ws/repo");
        let moves: Vec<String> = (0..=MOST_MOVES).map(|at| format!("cd ./d{at}")).collect();
        let settings = serde_json::json!({"hooks": {"Stop": [{"hooks": [
            {"type": "command", "command": format!("{} && ./x.sh", moves.join(" && "))}
        ]}]}});
        assert_eq!(
            scripts_in(
                Path::new("/plane"),
                dir,
                ".claude/settings.json",
                &settings.to_string(),
                None,
                &[]
            ),
            Err(Unread {
                file: dir.join(".claude/settings.json"),
                word: format!("./d{MOST_MOVES}"),
            })
        );
    }

    #[test]
    fn a_runner_pointed_at_the_config_s_folder_has_that_folder_s_entry_files_named() {
        // D-1327-9: the folder itself is never denied (D-1327-8), what a runner loads from it is.
        for command in [
            "node ./",
            "node .",
            "deno run .",
            "bun \"$CLAUDE_PROJECT_DIR\"",
            "tsx .",
            "ts-node ./",
            "npx .",
            "go run .",
            "python .",
            "python3 \"$CLAUDE_PROJECT_DIR\"",
            "uv run .",
            "ruby .",
            "php ./",
        ] {
            let (dir, found) = named_by_hook(command);
            let want: Vec<PathBuf> = ENTRY_FILES.iter().map(|name| dir.join(name)).collect();
            assert_eq!(
                found.iter().map(|it| lexical(it)).collect::<Vec<_>>(),
                want,
                "{command}"
            );
        }
        let (dir, found) = named_by_hook("node ../..");
        assert!(found.contains(&dir.join("../../package.json")), "{found:?}");
        // Anything else pointed at the folder names nothing.
        let (_, found) = named_by_hook("prettier --write .");
        assert_eq!(found, Vec::<PathBuf>::new());
    }

    #[test]
    fn an_option_s_value_written_against_it_is_read_as_its_value() {
        let (dir, found) = named_by_hook("jq -L./jqlib '.a // .b'");
        assert_eq!(found, [dir.join("jqlib")]);
        let (dir, found) = named_by_hook("awk -f./tools/sum.awk data");
        assert_eq!(found, [dir.join("tools/sum.awk")]);
    }

    /// What a hook running `command` in `/plane/ws/repo` names, for an operator whose home is
    /// `/home/op`.
    fn named_at_home(command: &str) -> Vec<PathBuf> {
        named_writing(command, &[])
    }

    /// [`named_at_home`] for a chat that could also write `writable`.
    fn named_writing(command: &str, writable: &[PathBuf]) -> Vec<PathBuf> {
        let settings = serde_json::json!({"hooks": {"PostToolUse": [{"hooks": [
            {"type": "command", "command": command}
        ]}]}});
        scripts_in(
            Path::new("/plane"),
            Path::new("/plane/ws/repo"),
            ".claude/settings.json",
            &settings.to_string(),
            Some(Path::new("/home/op")),
            writable,
        )
        .expect("read")
        .into_iter()
        .map(|named| named.path)
        .collect()
    }

    #[test]
    fn an_absolute_split_of_a_cluster_is_named_in_a_folder_chats_may_be_granted() {
        // D-T56-1: a folder you list as one chats may be granted, outside the project and the
        // home folder, is one a chat could write, so a later letter's value there is named.
        let granted = [PathBuf::from("/opt/granted")];
        let command = "cc -xI/opt/granted/inc/x.h a.c";
        assert!(!named_at_home(command).contains(&PathBuf::from("/opt/granted/inc/x.h")));
        let found = named_writing(command, &granted);
        assert!(
            found.contains(&PathBuf::from("/opt/granted/inc/x.h")),
            "{found:?}"
        );
        // Only below it: a folder beside it is still no value.
        let found = named_writing("cc -xI/opt/grantedx/y a.c", &granted);
        assert!(
            !found.contains(&PathBuf::from("/opt/grantedx/y")),
            "{found:?}"
        );
    }

    #[test]
    fn an_absolute_split_of_a_cluster_is_named_in_the_project_s_cache_home() {
        // D-T56-1: the project's cache home (#1337) can sit outside the home folder, where
        // `XDG_DATA_HOME` moves purlis's data home; a chat writes it, so a value there is named.
        let cache = PathBuf::from("/srv/data/purlis/cache-homes/0123abcd");
        let command = "gawk -bf/srv/data/purlis/cache-homes/0123abcd/npm/x.awk data";
        let want = cache.join("npm/x.awk");
        assert!(!named_at_home(command).contains(&want));
        let found = named_writing(command, std::slice::from_ref(&cache));
        assert!(found.contains(&want), "{found:?}");
    }

    #[test]
    fn an_absolute_split_of_a_cluster_is_named_where_a_chat_could_write_it() {
        // #1356, D-1356-7: an absolute value after any letter, in the project or the home
        // folder, is named; one after a later letter elsewhere names no `/tmp` for `-Ivendor/tmp`.
        for (command, want) in [
            ("cc -I/opt/include/x.h -c a.c", "/opt/include/x.h"),
            ("gawk -bf/plane/tools/x.awk data", "/plane/tools/x.awk"),
            ("sed -nf/plane/ws/repo/fix.sed x", "/plane/ws/repo/fix.sed"),
            ("jq -rf/plane/filters/x.jq ./in.json", "/plane/filters/x.jq"),
            ("cc -xI/plane/inc/x.h a.c", "/plane/inc/x.h"),
            ("cc -xI/home/op/inc/x.h a.c", "/home/op/inc/x.h"),
        ] {
            let found = named_at_home(command);
            assert!(found.contains(&PathBuf::from(want)), "{command}: {found:?}");
        }
        for (command, not) in [
            ("cc -Ivendor/tmp -c a.c", "/tmp"),
            ("cc -Isrc/usr/local x.c", "/usr/local"),
            ("cc -xI/opt/elsewhere x.c", "/opt/elsewhere"),
        ] {
            let found = named_at_home(command);
            assert!(!found.contains(&PathBuf::from(not)), "{command}: {found:?}");
        }
    }

    #[test]
    fn a_tilde_inside_a_word_is_named_as_written_and_as_the_home_folder() {
        // #1356, D-1356-8: a shell expands `~` only at a word's start (and after `=` in bash),
        // so a value taken out of a word may be either.
        for (command, tail) in [
            ("cc -I~/lib/x.h a.c", "lib/x.h"),
            ("cc --include=~/lib/x.h a.c", "lib/x.h"),
            ("java -javaagent:~/tools/a.jar -jar x", "tools/a.jar"),
            ("awk -bf~/tools/x.awk data", "tools/x.awk"),
        ] {
            let found = named_at_home(command);
            for want in [
                PathBuf::from("/plane/ws/repo/~").join(tail),
                PathBuf::from("/home/op").join(tail),
            ] {
                assert!(
                    found.contains(&want),
                    "{command}: {want:?} not in {found:?}"
                );
            }
        }
    }

    #[test]
    fn a_java_agent_s_file_written_after_a_colon_is_named() {
        // #1356 review: `-javaagent:<jar>[=options]`, `-agentpath:<lib>[=options]`, `-agentlib:`.
        for (command, want) in [
            (
                "java -javaagent:./tools/agent.jar -jar app.jar",
                "tools/agent.jar",
            ),
            (
                "java -javaagent:./tools/agent.jar=./conf/a.yml -jar x",
                "tools/agent.jar",
            ),
            (
                "java -javaagent:./tools/agent.jar=./conf/a.yml -jar x",
                "conf/a.yml",
            ),
            (
                "java -agentpath:./lib/prof.so=depth=3 -jar x",
                "lib/prof.so",
            ),
            ("java -agentlib:./lib/hprof=cpu=times -jar x", "lib/hprof"),
        ] {
            let (dir, found) = named_by_hook(command);
            assert!(found.contains(&dir.join(want)), "{command}: {found:?}");
        }
    }

    #[test]
    fn an_option_s_value_attached_to_it_names_what_it_names_given_apart() {
        // #1356: written against its option, alone or after other short options, or after `=`,
        // a value is the operand it is when it is a word of its own.
        for (apart, attached) in [
            (
                "gawk -b -f ./tools/sum.awk data",
                "gawk -bf./tools/sum.awk data",
            ),
            (
                "sed -n -f ./scripts/fix.sed x",
                "sed -nf./scripts/fix.sed x",
            ),
            (
                "jq -f ./filters/x.jq ./data/in.json",
                "jq -f./filters/x.jq ./data/in.json",
            ),
            (
                "jq -r -f ./filters/x.jq ./data/in.json",
                "jq -rf./filters/x.jq ./data/in.json",
            ),
            (
                "jq --from-file ./filters/x.jq ./data/in.json",
                "jq --from-file=./filters/x.jq ./data/in.json",
            ),
            (
                "node --require ./tools/hook.js",
                "node --require=./tools/hook.js",
            ),
        ] {
            let (_, want) = named_by_hook(apart);
            let (_, found) = named_by_hook(attached);
            assert!(!want.is_empty(), "{apart}");
            for path in &want {
                assert!(
                    found.contains(path),
                    "{attached}: {path:?} not in {found:?}"
                );
            }
        }
    }

    #[test]
    fn a_submodule_s_git_directory_is_found_at_any_depth() {
        let clone = tempfile::tempdir().expect("a clone");
        let git = clone.path().join(".git");
        for dir in ["modules/a", "modules/a/modules/b", "modules/group/c"] {
            std::fs::create_dir_all(git.join(dir)).expect("a module");
            std::fs::write(git.join(dir).join("HEAD"), "ref: x\n").expect("HEAD");
        }
        let mut found = module_git_dirs(&git);
        found.sort();
        assert_eq!(
            found,
            [
                git.join("modules/a"),
                git.join("modules/a/modules/b"),
                git.join("modules/group/c")
            ]
        );
    }
}
