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
//! **Resolution and execution are separate questions.** [`resolve`] answers the first: what
//! it hands back is an absolute path, and nothing a child inherits is involved. [`chat_path`]
//! is the one place this module answers the second, for exactly one kind of child — the
//! program a CHAT runs — and it says why at length (charter-app#136). A forge CLI's
//! environment is still `forge::cli_env`'s and git's is still `worktree::git`'s fixed one:
//! widening what a chat can reach is not a reason to widen what charter's own subprocesses
//! can.
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
    /// **The fix comes before the list.** A caller holds this to a budget (a refusal is
    /// clipped to a length a person reads) and a directory list built from a long `$HOME` can
    /// pass it on its own. Whatever is clipped has to be the least load-bearing thing in
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

/// See the unix arm. [`file_names`] already reads `PATHEXT`, so [`find`] asks this about
/// `git.EXE` rather than `git`; answering `true` for such a file is what charter-app#100 has
/// left, and it waits for a Windows machine to prove it on — it is the line at which `gh`
/// lands in a credential helper, and nothing here can check what happens after that.
#[cfg(not(unix))]
pub fn runnable(_path: &Path) -> bool {
    false
}

/// `PATHEXT` when the variable is unset or empty: Python's `shutil._WIN_DEFAULT_PATHEXT`,
/// so an unset variable means what it means to the oracle.
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD;.VBS;.JS;.WS;.MSC";

/// The names `program` may have on disk, in the order they are tried.
///
/// `pathext` is `None` where the platform has no such thing, and then the name is exactly the
/// one asked for. Given a `PATHEXT` it is Windows's rule (charter-app#100): a name that
/// already ends in one of the listed extensions, compared without case, is tried as it is and
/// nothing else; any other name is tried with each extension appended, in `PATHEXT`'s order.
///
/// **Never bare, and that is stricter than Python on purpose.** `shutil.which` in 3.12 tries
/// the bare name last, and in 3.11 an empty `PATHEXT` entry matched every name and so tried
/// it bare first. A bare name with no listed extension is exactly the file [`runnable`]
/// refuses to call a program off unix — any file on `PATH` would do — so offering it as a
/// candidate would reopen the hole that arm is there to close. An unset or empty `PATHEXT`,
/// and empty entries in one, are read the way 3.12 reads them: its default, empties dropped.
///
/// **Split on `;`, never the host's separator.** `PATHEXT` is a Windows variable whose
/// separator is fixed; `std::env::split_paths` would split it on `:` here, and a test on this
/// machine would then pass for the wrong reason.
///
/// Pure, so the Windows rule is checked on every platform charter's tests run on — which is
/// the half of charter-app#100 that can be checked anywhere at all.
pub fn file_names(program: &str, pathext: Option<&OsStr>) -> Vec<String> {
    let Some(pathext) = pathext else {
        return vec![program.to_owned()];
    };
    let listed = pathext.to_string_lossy();
    let listed = if listed.is_empty() {
        DEFAULT_PATHEXT
    } else {
        &listed
    };
    let extensions: Vec<&str> = listed.split(';').filter(|ext| !ext.is_empty()).collect();
    let carries_one = Path::new(program).extension().is_some_and(|have| {
        let have = format!(".{}", have.to_string_lossy());
        extensions.iter().any(|ext| ext.eq_ignore_ascii_case(&have))
    });
    if carries_one {
        return vec![program.to_owned()];
    }
    extensions
        .iter()
        .map(|ext| format!("{program}{ext}"))
        .collect()
}

/// This process's `PATHEXT` for [`file_names`]: none on unix, where a name is only itself.
#[cfg(unix)]
fn host_pathext() -> Option<std::ffi::OsString> {
    None
}

/// This process's `PATHEXT` for [`file_names`], unset read as empty so the default applies.
#[cfg(not(unix))]
fn host_pathext() -> Option<std::ffi::OsString> {
    Some(std::env::var_os("PATHEXT").unwrap_or_default())
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

/// The entries of a `PATH` value that charter will search, in order: the absolute ones.
///
/// **One copy of this rule, for every lookup that reads an inherited `PATH`.** A relative
/// entry — `.`, an empty one, `bin` — resolves against the working directory, which for a
/// chat or a hook is a repository someone else can write, so "the program is whatever
/// `./git` is in the directory you happen to be in" is not a lookup charter performs.
/// [`search_dirs_from`] and `worktree::git`'s fallback both ask this; they used to hold a
/// copy each, and the copy in `worktree::git` was the one missing the rule.
pub fn searchable(path: &OsStr) -> impl Iterator<Item = PathBuf> + '_ {
    std::env::split_paths(path).filter(|dir| dir.is_absolute())
}

/// Every directory charter searches for a bare program name, in order and without repeats.
///
/// Pure, so the list can be tested without touching the process's own environment — which
/// `unsafe_code = "forbid"` puts out of reach anyway. [`search_dirs`] is the one caller that
/// reads the environment.
///
/// A relative entry in `PATH` is dropped, by [`searchable`], which says why.
pub fn search_dirs_from(path: Option<&OsStr>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |dir: PathBuf| {
        if dir.is_absolute() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    if let Some(path) = path {
        for dir in searchable(path) {
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

/// The `PATH` a chat's program is started with: the directory of the `charter` the app ships,
/// then exactly the directories [`search_dirs_from`] searched to find the harness — or none,
/// where the answer cannot be written as a `PATH` without losing something inherited.
///
/// **charter-app#136.** A chat used to inherit the app's own `PATH`, and a Finder-launched
/// `.app` gets `/usr/bin:/bin:/usr/sbin:/sbin`. The hooks charter arms on a chat were never
/// affected — the bundled plugin's hooks and the status line name the bundled binary by its
/// absolute path, so a hostile `PATH` cannot redirect them either. What broke was everything
/// that names a program by its bare word from INSIDE the chat: a plane's own
/// `.claude/settings.json` says `charter hook …`, because it is a file that travels between
/// machines and cannot carry one machine's path, and the model's own Bash tool could not find
/// `node`, `gh` or `uv` and concluded the machine lacked them.
///
/// **Why this list and nothing wider.** It is the list that already chose which harness runs,
/// so a chat searches exactly where charter searched on its behalf and nowhere else. It is in
/// the diff and nothing an attacker writes can extend it, and it asks no login shell — the
/// module docs say why that is ruled out, and it is ruled out here for the same reasons. The
/// user directories in it are ones the operator's own shell already puts on `PATH`: anything
/// that can write `~/.local/bin` can write `~/.zshrc`, so no boundary is crossed that a
/// terminal-launched chat did not already cross.
///
/// **Why the app's own `charter` comes FIRST.** Nothing the app ships runs on a Python
/// fallback (charter ADR 0025): a chat is the app's, so the `charter` it reaches by the bare
/// word — the hand-off command the `handoff` skill runs, `charter workspace list`, a plane's
/// own hook — is the one built and shipped with this app, whatever else the machine has
/// installed. An operator with the Python charter in `~/.local/bin` used to get THAT one in
/// every chat, because this directory went last: a hand-off then went through a program with
/// no channel into the app and printed a command for a terminal instead of opening the chat
/// (charter-app#204), and a plane hook was answered by whichever charter happened to be
/// installed.
///
/// What made last the right answer before no longer holds. It was there because the Python
/// charter's plugin wired nine tool-hook words by the bare word, and this binary blocks every
/// one it has not ported — first, it would have refused every `Read` in every chat. An app
/// chat no longer loads that plugin (`crate::plugin::SUPERSEDED` is turned off for the
/// session), and the bundled plugin wires only words this binary answers.
///
/// **Otherwise the inherited `PATH` in its own order, then the fixed list**, so the change is
/// strictly additive for every other program: a terminal-launched chat finds every program it
/// found before, the same one, and a Finder-launched one gains a floor.
///
/// **What is dropped.** A relative or empty entry: it resolves against the chat's working
/// directory, which is a repository a chat can write, and "the program is whatever `./git`
/// is in this checkout" is not a lookup charter hands a chat. A directory named twice keeps
/// its first place.
///
/// **When it answers `None`**, the chat inherits the app's `PATH` exactly as it did before —
/// an inherited entry that is not UTF-8 cannot ride in the `String` environment a chat is
/// given, and silently dropping one of the operator's own directories would be a worse
/// failure than not adding charter's. A fixed-list or `charter` directory that cannot be
/// written into a `PATH` at all (a `$HOME` with the separator in it) is left out on its own.
pub fn chat_path_from(
    path: Option<&OsStr>,
    home: Option<&Path>,
    charter: Option<&Path>,
) -> Option<String> {
    let mut dirs = search_dirs_from(path, home);
    if let Some(dir) = charter.and_then(Path::parent)
        && dir.is_absolute()
    {
        dirs.retain(|have| have != dir);
        dirs.insert(0, dir.to_path_buf());
    }
    let inherited: Vec<PathBuf> = path
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    let mut kept: Vec<&str> = Vec::new();
    for dir in &dirs {
        match dir.to_str() {
            Some(text) if std::env::join_paths([dir]).is_ok() => kept.push(text),
            // Not ours to drop. The chat keeps the PATH it would have had.
            _ if inherited.contains(dir) => return None,
            _ => {}
        }
    }
    std::env::join_paths(kept).ok()?.into_string().ok()
}

/// [`chat_path_from`] against this process, with the app's own `charter` at `charter`.
pub fn chat_path(charter: Option<&Path>) -> Option<String> {
    chat_path_from(
        std::env::var_os("PATH").as_deref(),
        crate::profiles::home().as_deref(),
        charter,
    )
}

/// `program` as an absolute path, found in `dirs`: every name [`file_names`] gives it in the
/// first directory, then the next directory — the order a Windows shell and `shutil.which`
/// search in. On unix that is the one name, so nothing here changed there.
pub fn find(program: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let names = file_names(program, host_pathext().as_deref());
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
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
/// was. [`runnable`] answers `false` there until charter-app#100's Windows half lands — the
/// search knows the extensions now ([`file_names`]) but not yet which file is a program — so
/// a search would find nothing and this would refuse every built-in profile, where today the
/// spawn itself resolves `claude` to `claude.exe` and works. A port that has never been run on
/// a platform must not start refusing on it, and until it has, the operating system keeps the
/// lookup it already does.
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

    /// charter-app#136's launch, as a chat's `PATH` came out of it.
    const FINDER: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

    #[cfg(unix)]
    #[test]
    fn a_finder_launched_chat_searches_where_charter_searched_and_then_charters_own_directory() {
        let home = PathBuf::from("/home/op");
        let charter = PathBuf::from("/Applications/charter.app/Contents/MacOS/charter");
        let path =
            chat_path_from(Some(OsStr::new(FINDER)), Some(&home), Some(&charter)).expect("a PATH");
        assert_eq!(
            path,
            [
                FINDER,
                "/home/op/.local/bin:/home/op/bin:/home/op/.opencode/bin:/home/op/.bun/bin",
                "/home/op/.volta/bin:/home/op/.npm-global/bin",
                "/opt/homebrew/bin:/usr/local/bin",
                "/Applications/charter.app/Contents/MacOS",
            ]
            .join(":"),
            "the inherited four, the list that found the harness, then the app's charter"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_terminal_launched_chat_keeps_every_directory_it_inherited_first_and_in_order() {
        // Strictly additive: whatever a terminal-launched chat found before, it finds the same
        // one now. `/usr/local/bin` first here is the operator's order, and it stays first.
        let path = chat_path_from(
            Some(OsStr::new("/usr/local/bin:/nix/me/bin:/usr/bin")),
            Some(Path::new("/home/op")),
            None,
        )
        .expect("a PATH");
        assert!(
            path.starts_with("/usr/local/bin:/nix/me/bin:/usr/bin:/home/op/.local/bin:"),
            "{path}"
        );
        assert_eq!(
            path.matches("/usr/local/bin").count(),
            1,
            "named twice: {path}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_apps_own_charter_is_found_before_an_installed_one() {
        // Nothing the app ships runs on a Python fallback (ADR 0025). A chat that reaches
        // `charter` by the bare word — the hand-off the skill runs, a plane's own hook — gets
        // the binary this app was built with, even where the operator also installed the
        // Python one. Asked the way `/bin/sh` asks: first hit wins.
        let home = tempfile::tempdir().expect("a home");
        let app = tempfile::tempdir().expect("an app bundle");
        let installed = home.path().join(".local/bin/charter");
        let bundled = app.path().join("charter");
        for binary in [&installed, &bundled] {
            std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
            std::fs::write(binary, "#!/bin/sh\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = chat_path_from(Some(OsStr::new(FINDER)), Some(home.path()), Some(&bundled))
            .expect("a PATH");
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert_eq!(find("charter", &dirs), Some(bundled));
        assert_eq!(dirs[0], app.path(), "{path}");
    }

    #[cfg(unix)]
    #[test]
    fn charters_directory_already_on_the_path_moves_to_the_front_and_is_named_once() {
        let path = chat_path_from(
            Some(OsStr::new("/usr/bin:/opt/charter")),
            None,
            Some(Path::new("/opt/charter/charter")),
        )
        .expect("a PATH");
        assert_eq!(
            path,
            "/opt/charter:/usr/bin:/opt/homebrew/bin:/usr/local/bin:/bin"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_relative_or_empty_entry_is_not_handed_to_a_chat() {
        // Resolved against the chat's working directory — a checkout a chat can write.
        let path = chat_path_from(Some(OsStr::new(".::bin:/abs")), None, None).expect("a PATH");
        assert_eq!(path, "/abs:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin");
    }

    #[cfg(unix)]
    #[test]
    fn an_inherited_directory_that_is_not_utf8_leaves_the_chat_its_inherited_path() {
        // The chat's environment is `String`s. Dropping one of the operator's own directories
        // to add charter's would be a worse failure than not adding charter's.
        use std::os::unix::ffi::OsStrExt;
        let path = OsStr::from_bytes(b"/usr/bin:/op/\xff/bin");
        assert_eq!(chat_path_from(Some(path), None, None), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_home_that_cannot_be_written_into_a_path_costs_only_its_own_directories() {
        let path = chat_path_from(
            Some(OsStr::new("/usr/bin")),
            Some(Path::new("/home/a:b")),
            Some(Path::new("/weird:dir/charter")),
        )
        .expect("a PATH");
        assert_eq!(path, "/usr/bin:/opt/homebrew/bin:/usr/local/bin:/bin");
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

    // charter-app#100: the names a program may have on disk. Windows's rule, asserted here
    // because it is a rule about strings and this is where the tests run.

    #[test]
    fn with_no_pathext_a_program_is_looked_for_by_exactly_the_name_it_was_given() {
        assert_eq!(file_names("claude", None), vec!["claude"]);
        assert_eq!(file_names("claude.exe", None), vec!["claude.exe"]);
    }

    #[test]
    fn with_pathext_a_bare_name_is_tried_with_each_extension_in_order_and_never_bare() {
        let names = file_names("git", Some(OsStr::new(".COM;.EXE;.CMD")));
        assert_eq!(names, vec!["git.COM", "git.EXE", "git.CMD"]);
        assert!(
            !names.iter().any(|n| n == "git"),
            "a file with no extension is any file on PATH, not a program: {names:?}"
        );
    }

    #[test]
    fn a_name_that_already_ends_in_a_listed_extension_is_tried_as_it_is_whatever_its_case() {
        assert_eq!(
            file_names("git.exe", Some(OsStr::new(".COM;.EXE"))),
            vec!["git.exe"]
        );
        assert_eq!(
            file_names("npx.Cmd", Some(OsStr::new(".EXE;.CMD"))),
            vec!["npx.Cmd"]
        );
    }

    #[test]
    fn an_extension_pathext_does_not_list_is_part_of_the_name() {
        assert_eq!(
            file_names("node.1", Some(OsStr::new(".EXE"))),
            vec!["node.1.EXE"]
        );
    }

    #[test]
    fn pathext_is_split_on_semicolons_whatever_this_host_separates_paths_with() {
        // `split_paths` would split on `:` on this machine and give ONE extension here.
        assert_eq!(
            file_names("gh", Some(OsStr::new(";.EXE;;.BAT;"))),
            vec!["gh.EXE", "gh.BAT"],
            "empty entries dropped, as Python 3.12 drops them — in 3.11 one matched every name"
        );
    }

    #[test]
    fn an_empty_pathext_means_the_default_python_uses() {
        let names = file_names("claude", Some(OsStr::new("")));
        assert_eq!(names.len(), DEFAULT_PATHEXT.split(';').count());
        assert_eq!(
            names[..4],
            ["claude.COM", "claude.EXE", "claude.BAT", "claude.CMD"]
        );
    }
}
