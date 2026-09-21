//! Where charter looks for a program a profile names by a bare word — and nowhere else.
//!
//! **charter-app#134.** macOS gives an app launched from Finder
//! `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, and nothing else: a GUI process is started by
//! `launchd`, which does not read a login shell. The operator's `claude` lives in
//! `~/.local/bin` — where its own installer puts it — so `Command::new("claude")` could not
//! spawn, [`crate::wiring`] answered `State::Unknown`, and a double-clicked charter refused
//! every chat on a built-in profile with *"an unknown is not a pass"*. The refusal was
//! right. The lookup was wrong: the harness was findable all along and the app never looked
//! where the operator's shell looks. Launched from a terminal it never reproduced, because
//! then the app inherits the shell's `PATH` — which is why CI, every scenario run and every
//! agent had passed.
//!
//! **Resolution and execution are separate questions, and this module answers only the
//! first.** What it hands back is an absolute path. Nothing here builds an environment, and
//! nothing here puts a directory on any child's `PATH`: a chat's environment is
//! [`crate::start::environment`]'s, a probe's is [`crate::wiring::environment`]'s, and git's
//! is `worktree::git`'s fixed one. Widening a search is not the same act as widening what a
//! program can then reach, and keeping them apart is what makes this change reviewable.
//!
//! **Why a fixed, audited list and not a login shell.** Asking `$SHELL -lc 'echo $PATH'`
//! once at startup was the obvious candidate and it is the wrong one:
//!
//! - it **executes the operator's dotfiles** inside charter's own process tree to answer a
//!   lookup, so a compromised `.zprofile` silently redirects every harness charter starts,
//!   and nothing in a diff or a review would show it;
//! - its answer depends on which rc files the flags reach. On macOS `zsh` a login
//!   non-interactive shell reads `.zprofile`/`.zlogin` but **not** `.zshrc`, which is where
//!   most people's `PATH` is actually set — so the safe spelling would not even have found
//!   this machine's `claude`, and the spelling that would (`-ilc`) runs an interactive
//!   startup that can prompt, print, or hang;
//! - it needs a deadline, a hung-shell story and a way to tell a shell's `PATH` from its
//!   noise, all for a question that has ten answers and no ambiguity.
//!
//! A fixed list is the pattern this repository already chose, for the same reason, in
//! `worktree::git::GIT_DIRS`: it is in the diff, a reviewer can read it, and nothing an
//! attacker writes can add to it. What it costs is completeness — see [`USER_BIN`] — and
//! that cost is paid by a refusal that says exactly where charter looked, so the operator
//! can declare an absolute path the way they already did for `codex`.
//!
//! **The inherited `PATH` is searched first, and that is deliberate.** It makes this change
//! strictly additive: every machine where a harness is found today finds the same binary
//! afterwards, because the first hit still wins and the process's own `PATH` is still what
//! is asked first. The fixed directories are a floor under a truncated `PATH`, not a
//! replacement for it. `crate::forge::find_cli` already searches in this order and says why;
//! `worktree::git` searches the other way round because a redirected `git` is handed
//! charter's credentials, which a harness never is.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Directories under the operator's home that a harness installs itself into, relative to
/// `$HOME`, in the order a shell's `PATH` usually carries them.
///
/// Every entry is a **stable** directory that some installer creates and then leaves alone:
///
/// - `.local/bin` — Claude Code's and Codex's own native installers, and `uv`/`pipx`. This
///   is the one charter-app#134 was actually about.
/// - `bin` — the oldest convention there is, and still what a distribution's default
///   `~/.profile` adds.
/// - `.opencode/bin` — opencode's installer. charter's registry knows the kind, so charter
///   knows where it lands. (charter-app v1 does not *start* opencode; it still lists it.)
/// - `.bun/bin`, `.volta/bin`, `.npm-global/bin` — the three JavaScript installers that make
///   one stable directory. All three harnesses are installable through them.
///
/// **What is deliberately NOT here**: `nvm`, `asdf`, `mise` and Nix. Their directories are
/// version- or shim-scoped (`~/.nvm/versions/node/v22.11.0/bin`, `~/.asdf/shims`,
/// `/nix/store/<hash>-…/bin`) and cannot be enumerated without either guessing a version or
/// reading the tool's own state — which is the login-shell question again, wearing a hat. A
/// machine like that declares the profile with an absolute path, which is what the refusal
/// from [`NotFound::said`] asks for and what this plane's `codex` profile already does.
pub const USER_BIN: [&str; 6] = [
    ".local/bin",
    "bin",
    ".opencode/bin",
    ".bun/bin",
    ".volta/bin",
    ".npm-global/bin",
];

/// The machine-wide directories, in the order an operator's `PATH` usually carries them.
///
/// Not `worktree::git::GIT_DIRS`, although it holds the same four: that list is ordered to
/// prefer the system's own `git` over anything a package manager put beside it, which is the
/// right answer for the binary charter hands credentials to and the wrong one here. No
/// harness ships in `/usr/bin`; every one of them arrives through Homebrew or an npm prefix,
/// so those come first and a shell's own order is reproduced.
pub const SYSTEM_BIN: [&str; 4] = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"];

/// A program charter was asked to run and could not find, carrying every directory it looked
/// in — which is the one fact charter-app#134's refusal was missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound {
    /// The bare word that was searched for.
    pub program: String,
    /// Every directory searched, in the order they were searched.
    pub looked: Vec<PathBuf>,
}

impl NotFound {
    /// The refusal, naming the program, the one fix, and then every directory charter looked
    /// in.
    ///
    /// **It names the directories, and that is the point.** The message this replaces named
    /// the config folder and the working directory — neither of which had anything to do with
    /// why the spawn failed — so an operator reading it had no way to tell "the harness is not
    /// installed" from "the harness is installed somewhere this process cannot see". That one
    /// missing fact is what made charter-app#134 take a code read to diagnose.
    ///
    /// **The fix comes before the list.** A caller holds this to a budget
    /// (`wiring::SAID_LIMIT` is 1024 characters) and a directory list built from a long `$HOME`
    /// can pass it on its own. Whatever is clipped has to be the least load-bearing thing in
    /// the sentence, and that is the tail of the search, not the action.
    ///
    /// Every value that came from outside charter — the program word, each directory — goes
    /// through [`crate::shown::readable`] unclipped, so a control byte is shown escaped and a
    /// path is shown whole.
    pub fn said(&self) -> String {
        let program = crate::shown::readable(&self.program, usize::MAX);
        let looked: Vec<String> = self
            .looked
            .iter()
            .map(|d| crate::shown::readable(&d.display().to_string(), usize::MAX))
            .collect();
        format!(
            "charter could not find a program called {program}. Put it on PATH, or name it by \
             its absolute path in charter.local.toml (command = \
             [\"/full/path/to/{program}\"]). It looked in: {}",
            looked.join(", ")
        )
    }
}

/// Is `path` a file this process could execute?
///
/// **Off unix this is `false`, and that is a refusal rather than a degradation.** There is no
/// executable bit on Windows: what makes a file runnable is its extension, against `PATHEXT`,
/// and every caller here joins a BARE name with no extension at all. Answering "it is a file"
/// would make any file on `PATH` a harness — the mistake `doctor::profiles::on_path` used to
/// make on its own. charter-app#100 is where Windows gets its answer, and putting the one
/// copy of this question here is what makes that a single-place change: `find` below, the
/// doctor's listing and `forge::find_cli` now ask it once, instead of three times in three
/// different ways.
#[cfg(unix)]
pub fn runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// See the unix arm: nothing is runnable here until charter-app#100 reads `PATHEXT`.
#[cfg(not(unix))]
pub fn runnable(_path: &Path) -> bool {
    false
}

/// Does `program` name a place rather than a program — `./x`, `bin/x`, `/usr/bin/x`?
///
/// `shutil.which`'s own question, and charter's answer is the same as Python's: a word with a
/// separator in it is a path the operator wrote down, and charter neither searches for it nor
/// second-guesses it. It is handed to the spawn exactly as declared, so a profile naming a
/// path that is missing or not executable fails where it always did, with the operating
/// system's own error, rather than with a refusal from here about a search that never
/// happened.
fn is_a_path(program: &str) -> bool {
    Path::new(program)
        .parent()
        .is_some_and(|dir| !dir.as_os_str().is_empty())
}

/// Every directory charter searches for a bare program name, in order and without repeats.
///
/// Pure, so the list can be tested without touching the process's own environment — which
/// `unsafe_code = "forbid"` puts out of reach anyway. [`search_dirs`] is the one caller that
/// reads the environment.
///
/// A relative entry in `PATH` is dropped. It resolves against the working directory, which
/// for a chat is a repository a chat can write, and "the harness is whatever `./claude` is in
/// the directory you happen to be in" is not a lookup charter performs.
pub fn search_dirs_from(path: Option<&OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |dir: PathBuf| {
        if dir.is_absolute() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    if let Some(path) = path {
        for dir in std::env::split_paths(path) {
            push(dir);
        }
    }
    if let Some(home) = home {
        for rel in USER_BIN {
            push(home.join(rel));
        }
    }
    for dir in SYSTEM_BIN {
        push(PathBuf::from(dir));
    }
    dirs
}

/// [`search_dirs_from`] against this process: its `PATH` first, then the fixed list.
pub fn search_dirs() -> Vec<PathBuf> {
    search_dirs_from(
        std::env::var_os("PATH").as_deref(),
        crate::profiles::home().as_deref(),
    )
}

/// `program` as an absolute path, found in `dirs`.
pub fn find(program: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(program))
        .find(|candidate| runnable(candidate))
}

/// `shutil.which`, widened: is `program` something this process could run?
///
/// A word with a separator is asked of the filesystem directly; a bare word is searched for.
pub fn on_path(program: &str) -> bool {
    if is_a_path(program) {
        return runnable(Path::new(program));
    }
    find(program, &search_dirs()).is_some()
}

/// `program`, resolved to an absolute path when it is a bare word, or handed back unchanged
/// when it already names a place.
///
/// **Resolved in the parent, where the search is charter's own.** The absolute path is what
/// the spawn is then given, so nothing downstream depends on how a child's `PATH` happens to
/// be built — which is the same reasoning `worktree::git::git_binary` writes down, and the
/// reason an app launched from Finder can start a chat at all.
///
/// **Off unix a bare word is handed back unresolved**, which leaves Windows exactly where it
/// was. [`runnable`] answers `false` there until charter-app#100 reads `PATHEXT`, so a search
/// would find nothing and this would refuse every built-in profile — where today the spawn
/// itself resolves `claude` to `claude.exe` and works. A port that has never been compiled for
/// a platform must not start refusing on it: #100 is where the search learns about extensions,
/// and until then the operating system keeps the lookup it already does.
pub fn resolve(program: &str) -> Result<String, NotFound> {
    if is_a_path(program) || cfg!(not(unix)) {
        return Ok(program.to_owned());
    }
    let dirs = search_dirs();
    match find(program, &dirs) {
        Some(found) => Ok(found.display().to_string()),
        None => Err(NotFound {
            program: program.to_owned(),
            looked: dirs,
        }),
    }
}

/// `argv` with its program word resolved. An empty `argv` is handed back as it came: a
/// profile with no command is one validation already refused, and this is not the place that
/// says so again.
pub fn resolve_argv(argv: &[String]) -> Result<Vec<String>, NotFound> {
    let mut argv = argv.to_vec();
    if let Some(program) = argv.first_mut() {
        *program = resolve(program)?;
    }
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_processs_own_path_comes_first_and_the_fixed_directories_follow_it() {
        let home = PathBuf::from("/home/op");
        let dirs = search_dirs_from(Some(OsStr::new("/first:/second")), Some(&home));
        assert_eq!(dirs[0], PathBuf::from("/first"));
        assert_eq!(dirs[1], PathBuf::from("/second"));
        assert!(
            dirs.contains(&home.join(".local/bin")),
            "the directory charter-app#134 was about: {dirs:?}"
        );
        assert_eq!(*dirs.last().unwrap(), PathBuf::from("/bin"));
    }

    #[test]
    fn a_finder_launch_still_gets_the_user_directories() {
        let home = PathBuf::from("/home/op");
        let dirs = search_dirs_from(
            Some(OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")),
            Some(&home),
        );
        for rel in USER_BIN {
            assert!(
                dirs.contains(&home.join(rel)),
                "{rel} missing from {dirs:?}"
            );
        }
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
    }

    #[test]
    fn a_directory_named_twice_is_searched_once_and_keeps_its_first_place() {
        let home = PathBuf::from("/home/op");
        let dirs = search_dirs_from(Some(OsStr::new("/bin:/usr/bin:/bin")), Some(&home));
        assert_eq!(dirs.iter().filter(|d| *d == Path::new("/bin")).count(), 1);
        assert_eq!(dirs[0], PathBuf::from("/bin"));
        assert_eq!(dirs[1], PathBuf::from("/usr/bin"));
    }

    #[test]
    fn a_relative_path_entry_is_not_a_directory_charter_searches() {
        let dirs = search_dirs_from(Some(OsStr::new(".:relative:/abs")), None);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/abs"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin")
            ]
        );
    }

    #[test]
    fn with_no_home_the_fixed_system_directories_are_still_searched() {
        let dirs = search_dirs_from(None, None);
        assert_eq!(dirs, SYSTEM_BIN.map(PathBuf::from).to_vec());
    }

    #[test]
    fn a_word_with_a_separator_is_a_place_and_is_never_searched_for() {
        assert!(is_a_path("/usr/bin/claude"));
        assert!(is_a_path("./claude"));
        assert!(is_a_path("bin/claude"));
        assert!(!is_a_path("claude"));
        assert_eq!(resolve("/nowhere/claude").unwrap(), "/nowhere/claude");
    }

    #[test]
    fn a_program_that_is_in_none_of_the_directories_says_which_ones_they_were() {
        let dir = tempfile::tempdir().unwrap();
        let looked = search_dirs_from(None, Some(dir.path()));
        let said = NotFound {
            program: "claude".to_owned(),
            looked: looked.clone(),
        }
        .said();
        assert!(
            said.contains("charter could not find a program called claude"),
            "{said}"
        );
        for dir in &looked {
            assert!(said.contains(&dir.display().to_string()), "{said}");
        }
        assert!(
            said.contains("command = [\"/full/path/to/claude\"]"),
            "{said}"
        );
    }

    #[test]
    fn a_program_only_in_a_user_directory_is_found_when_path_does_not_hold_it() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        stand_in::program(&home.join(".local/bin"), "claude", "#!/bin/sh\nexit 0\n");
        let dirs = search_dirs_from(Some(OsStr::new("/usr/bin:/bin")), Some(home));
        assert_eq!(
            find("claude", &dirs),
            Some(home.join(".local/bin").join("claude")),
            "the harness the operator's shell finds, found without the operator's PATH"
        );
    }

    #[test]
    fn a_file_that_is_not_executable_is_not_a_program() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::write(dir.path().join("bin/claude"), "not a program\n").unwrap();
        let dirs = search_dirs_from(None, Some(dir.path()));
        assert_eq!(find("claude", &dirs), None);
    }

    #[test]
    fn the_first_directory_holding_it_wins() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        stand_in::program(&first, "claude", "#!/bin/sh\nexit 0\n");
        stand_in::program(&second, "claude", "#!/bin/sh\nexit 0\n");
        let dirs = vec![first.clone(), second];
        assert_eq!(find("claude", &dirs), Some(first.join("claude")));
    }
}
