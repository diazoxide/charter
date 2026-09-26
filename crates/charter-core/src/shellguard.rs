//! A plain shell tab, and what happens when a harness is started by hand inside one (SI-5).
//!
//! A shell tab is the operator's own `$SHELL` in a tab, with no harness and no profile. Typing
//! `claude` there starts a harness charter did not start: no profile, no persona, no state
//! hooks of its own, nothing to resume it by. It works, and it is invisible to everything
//! charter does for a chat. So a shell tab's `PATH` begins with a directory of charter's own
//! holding one small script per harness — a **shim** — and each shim runs
//! `charter shell-guard <harness> -- <args…>` in front of the real program. That command:
//!
//! 1. says one line on standard error: this harness runs outside charter's session tracking,
//!    and a chat tab is where it should run;
//! 2. tells the app over the hook socket ([`crate::hookwire::StartedByHand`]), which draws a
//!    banner on the tab with an "Open as chat" button;
//! 3. finds the real program on `PATH` with the shim directory left out, and runs it with the
//!    same arguments.
//!
//! **Nothing reads the harness's output.** The detection is which command the operator
//! started, which is the same kind of fact as a hook's event word: charter wrote the thing
//! that fires it. ADR 0018's rule is untouched.
//!
//! **It never stands in the way.** Every failure in steps 1 and 2 is dropped and step 3 runs
//! regardless; a shim whose `charter` has gone (an app moved or deleted since it wrote the
//! shim) runs the real program itself.
//!
//! **It cannot recurse.** The program is looked up with the shim directory left out, and it
//! is started with that directory taken off its `PATH` — so the harness, and every shell its
//! model's tools start, finds the real `claude` and never the shim.
//!
//! **Why a shell's own start files are involved at all.** A `PATH` handed to a shell is only
//! where it STARTS: an interactive shell reads the operator's `~/.zshrc` or `~/.bashrc`, and
//! the usual line there is `export PATH="$HOME/.local/bin:$PATH"` — which is exactly where
//! Claude Code and Codex install themselves. Measured on the operator's own machine on
//! 2026-09-26: `~/.zshrc` puts `~/.local/bin` and `~/.opencode/bin` in front, so a shim
//! directory that was only first in the environment would have been behind every harness it
//! exists to stand in front of. So for zsh and bash the shim directory is put first again
//! AFTER the operator's own files have run, the way VS Code's shell integration does it: zsh
//! is pointed at a `ZDOTDIR` of charter's whose files source the operator's own and then put
//! the shims first, and bash is started with an `--rcfile` that does the same. Any other shell
//! gets the `PATH` alone, and a start file that puts a harness's directory first wins there.
//! See ADR 0062.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

use crate::harness::Harness;
use crate::hookwire::{CHAT_ENV, SOCKET_ENV, StartedByHand};

/// The CLI word the shims run.
pub const COMMAND: &str = "shell-guard";

/// Every harness a shell tab has a shim for: every harness charter starts.
pub const SHIMMED: [Harness; 3] = [Harness::ClaudeCode, Harness::Codex, Harness::Opencode];

/// Where the operator's own `ZDOTDIR` is kept while zsh reads charter's, for charter's files
/// to hand it back. Unset when the operator had none, which means `$HOME`.
pub const USER_ZDOTDIR_ENV: &str = "CHARTER_USER_ZDOTDIR";

/// The line a harness started by hand says, on standard error, before it starts.
pub fn warning(harness: Harness) -> String {
    format!(
        "charter: this {} runs outside charter's session tracking; open it as a chat tab \
         instead",
        harness.name()
    )
}

/// The app-owned directory the shims and the shell start files live in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shims {
    root: PathBuf,
}

/// What a shell tab's shell is started with, beyond what any chat is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShellStart {
    /// Put in front of the shell's own arguments.
    pub args: Vec<String>,
    /// The chat's environment, with `PATH` and whatever the shell needs to find charter's
    /// start files.
    pub env: Vec<(String, String)>,
}

impl Shims {
    /// The shims under `root`, which the app owns.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory that goes first on a shell tab's `PATH`.
    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    /// The `ZDOTDIR` a shell tab's zsh reads first.
    pub fn zdotdir(&self) -> PathBuf {
        self.root.join("zsh")
    }

    /// The `--rcfile` a shell tab's bash reads instead of `~/.bashrc`, which it sources.
    pub fn bashrc(&self) -> PathBuf {
        self.root.join("bash").join("bashrc")
    }

    /// Writes every shim, running `charter`, and the shell start files. Called at every launch,
    /// so a shim always runs the `charter` of the app that is running.
    ///
    /// Each file is written beside itself and renamed into place, so a shell starting while
    /// the app writes reads the old file or the new one and never half of one — and a shim a
    /// shell is running at that moment keeps its own copy (the rename leaves its inode alone).
    #[cfg(unix)]
    pub fn write(&self, charter: &Path) -> io::Result<()> {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

        // The operator's alone, like everything else under the app's data directory: what is
        // in here runs in every shell tab.
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.root)?;
        std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))?;
        let bin = self.bin();
        for dir in [&bin, &self.zdotdir(), &self.root.join("bash")] {
            std::fs::create_dir_all(dir)?;
        }
        for harness in SHIMMED {
            put(
                &bin.join(harness.name()),
                &shim(charter, &bin, harness.name()),
                0o755,
            )?;
        }
        let zdotdir = self.zdotdir();
        for (name, text) in zsh_files(&zdotdir, &bin) {
            put(&zdotdir.join(name), &text, 0o644)?;
        }
        put(&self.bashrc(), &bashrc(&bin), 0o644)
    }

    /// Off unix there are no shims: a shell tab there is a plain shell, and a harness started
    /// in it is not warned about. The refusal says so rather than writing scripts no shell
    /// there would run.
    #[cfg(not(unix))]
    pub fn write(&self, _charter: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a shell tab's shims are POSIX shell scripts, and this platform has no answer yet",
        ))
    }

    /// How a shell tab's `shell` is started, given the environment a chat would get (`env`),
    /// the app's own `PATH` for when `env` names none, and the operator's `ZDOTDIR`.
    pub fn shell_start(
        &self,
        shell: &str,
        env: Vec<(String, String)>,
        inherited_path: Option<&OsStr>,
        zdotdir: Option<&OsStr>,
    ) -> ShellStart {
        let mut env = env;
        let mut args = Vec::new();
        let given = env
            .iter()
            .position(|(name, _)| name == PATH_ENV)
            .map(|at| env.remove(at).1);
        let base: OsString = match given {
            Some(path) => path.into(),
            None => inherited_path.map(OsStr::to_owned).unwrap_or_default(),
        };
        let dirs = std::iter::once(self.bin()).chain(std::env::split_paths(&base));
        match std::env::join_paths(dirs)
            .ok()
            .and_then(|joined| joined.into_string().ok())
        {
            Some(path) => env.push((PATH_ENV.to_owned(), path)),
            // A shim directory or an inherited entry that cannot ride in a chat's `String`
            // environment. The shell keeps the `PATH` it would have had — a shell tab with no
            // shims is a plain shell, which is what it was before there were any.
            None => {
                if let Some(path) = base.into_string().ok().filter(|p| !p.is_empty()) {
                    env.push((PATH_ENV.to_owned(), path));
                }
                env.sort();
                return ShellStart { args, env };
            }
        }
        match Path::new(shell).file_name().and_then(OsStr::to_str) {
            Some("zsh") => {
                env.retain(|(name, _)| name != ZDOTDIR_ENV && name != USER_ZDOTDIR_ENV);
                env.push((ZDOTDIR_ENV.to_owned(), self.zdotdir().display().to_string()));
                if let Some(theirs) = zdotdir.and_then(OsStr::to_str).filter(|d| !d.is_empty()) {
                    env.push((USER_ZDOTDIR_ENV.to_owned(), theirs.to_owned()));
                }
            }
            Some("bash") => {
                args.push("--rcfile".to_owned());
                args.push(self.bashrc().display().to_string());
            }
            _ => {}
        }
        env.sort();
        ShellStart { args, env }
    }
}

/// The variable a shell searches for every bare word it runs.
const PATH_ENV: &str = "PATH";

/// Where zsh reads its start files from.
const ZDOTDIR_ENV: &str = "ZDOTDIR";

/// `text` at `path`, with `mode`, replacing whatever was there in one step.
#[cfg(unix)]
fn put(path: &Path, text: &str, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let beside = path.with_extension(format!("charter-{}", std::process::id()));
    std::fs::write(&beside, text)?;
    std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(mode))?;
    std::fs::rename(&beside, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&beside);
    })
}

/// `text` as one POSIX shell word, whatever is in it.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The shim for `word`: `charter shell-guard` in front of the real program, or the real
/// program alone when that `charter` has gone.
fn shim(charter: &Path, bin: &Path, word: &str) -> String {
    let charter = quoted(&charter.display().to_string());
    let shims = quoted(&bin.display().to_string());
    format!(
        r#"#!/bin/sh
# Written by charter at every launch, for its shell tabs (ADR 0062). A `{word}` started in a
# shell tab runs through this: charter says it runs outside session tracking, tells the app,
# and runs the real `{word}` with this directory taken off PATH. Edits here are overwritten.
charter={charter}
shims={shims}
if [ -x "$charter" ]; then
  exec "$charter" {command} --shims "$shims" {word} -- "$@"
fi
# The app that wrote this has gone: run the real one, without this directory on PATH. Once
# only, so a PATH that names this directory another way cannot bring it back here for ever.
if [ -n "$CHARTER_SHIM_PASSED" ]; then
  echo "charter: no {word} on PATH outside charter's shell-tab shims" >&2
  exit 127
fi
CHARTER_SHIM_PASSED=1
export CHARTER_SHIM_PASSED
PATH=$(printf '%s\n' "$PATH" | tr ':' '\n' | grep -vxF "$shims" | paste -s -d : -)
export PATH
exec {word} "$@"
"#,
        command = COMMAND,
    )
}

/// zsh's start files, as charter's `ZDOTDIR` holds them: each runs the operator's own, from
/// the `ZDOTDIR` they had, and `.zshrc` — the last one an interactive shell reads before its
/// first prompt — hands that `ZDOTDIR` back and puts the shims first again.
fn zsh_files(ours: &Path, bin: &Path) -> Vec<(&'static str, String)> {
    let ours = quoted(&ours.display().to_string());
    let bin = quoted(&bin.display().to_string());
    let head = "# Written by charter at every launch, for its shell tabs (ADR 0062). zsh reads \
                this directory\n# first; each file runs the operator's own. Edits here are \
                overwritten.\n";
    vec![
        (
            ".zshenv",
            format!(
                r#"{head}ZDOTDIR="${{{user}:-$HOME}}"
[[ -r "$ZDOTDIR/.zshenv" ]] && builtin source "$ZDOTDIR/.zshenv"
# The operator's .zshenv may move ZDOTDIR; where it points now is where the rest of theirs are.
{user}="$ZDOTDIR"
ZDOTDIR={ours}
"#,
                user = USER_ZDOTDIR_ENV,
            ),
        ),
        (
            ".zprofile",
            format!(
                r#"{head}ZDOTDIR="${{{user}:-$HOME}}"
[[ -r "$ZDOTDIR/.zprofile" ]] && builtin source "$ZDOTDIR/.zprofile"
ZDOTDIR={ours}
"#,
                user = USER_ZDOTDIR_ENV,
            ),
        ),
        (
            ".zshrc",
            format!(
                r#"{head}ZDOTDIR="${{{user}:-$HOME}}"
unset {user}
[[ "$ZDOTDIR" == "$HOME" ]] && unset ZDOTDIR
[[ -r "${{ZDOTDIR:-$HOME}}/.zshrc" ]] && builtin source "${{ZDOTDIR:-$HOME}}/.zshrc"
# After the operator's own, which commonly put ~/.local/bin in front: the shims go first.
path=({bin} ${{path:#{bin}}})
"#,
                user = USER_ZDOTDIR_ENV,
            ),
        ),
    ]
}

/// bash's `--rcfile`: the operator's `~/.bashrc`, then the shims first again.
fn bashrc(bin: &Path) -> String {
    let bin = quoted(&bin.display().to_string());
    format!(
        r#"# Written by charter at every launch, for its shell tabs (ADR 0062). bash reads this in
# place of ~/.bashrc, which it runs first. Edits here are overwritten.
[ -r "$HOME/.bashrc" ] && . "$HOME/.bashrc"
# After the operator's own, which commonly put ~/.local/bin in front: the shims go first.
PATH={bin}:"$PATH"
"#
    )
}

/// `path` with `dir` taken out of it wherever it appears — compared as spelled and as
/// resolved, so a link to the directory is the directory.
pub fn without(path: &OsStr, dir: &Path) -> OsString {
    let kept: Vec<PathBuf> = std::env::split_paths(path)
        .filter(|entry| !same_dir(entry, dir))
        .collect();
    // Every entry came out of a `PATH`, so it goes back into one.
    std::env::join_paths(kept).unwrap_or_default()
}

/// Whether `one` and `other` are the same directory, as spelled or once resolved.
fn same_dir(one: &Path, other: &Path) -> bool {
    one == other
        || matches!(
            (one.canonicalize(), other.canonicalize()),
            (Ok(one), Ok(other)) if one == other
        )
}

/// The real `program`: the first runnable one on `path` that is not in `shims`.
///
/// A candidate that resolves INTO the shim directory is passed over too — a link in
/// `~/.local/bin` pointing at a shim is still the shim.
pub fn real_program(program: &str, path: &OsStr, shims: &Path) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|dir| dir.is_absolute() && !same_dir(dir, shims))
        .filter_map(|dir| crate::programs::find(program, &[dir]))
        .find(|found| {
            !found
                .canonicalize()
                .ok()
                .and_then(|real| real.parent().map(Path::to_path_buf))
                .is_some_and(|home| same_dir(&home, shims))
        })
}

/// What `charter shell-guard` does, worked out and not yet done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The line to say on standard error, when the word is a harness charter knows.
    pub warning: Option<String>,
    /// The notice to send, and the socket to send it to, when this shell is a chat's.
    pub tell: Option<(PathBuf, StartedByHand)>,
    /// The program to run, where there is one outside the shims.
    pub program: Option<PathBuf>,
    /// The `PATH` it runs under: the shell's, without the shims.
    pub path: OsString,
}

/// What the guard for `word` does, in the environment `env` and the directory `cwd`.
pub fn plan(
    word: &str,
    shims: &Path,
    env: &dyn Fn(&str) -> Option<OsString>,
    cwd: Option<PathBuf>,
) -> Plan {
    let harness = Harness::of_kind(word);
    let path = env(PATH_ENV).unwrap_or_default();
    let tell = harness.and_then(|harness| {
        let socket = PathBuf::from(env(SOCKET_ENV)?);
        let chat: u32 = env(CHAT_ENV)?.to_str()?.parse().ok().filter(|n| *n > 0)?;
        Some((
            socket,
            StartedByHand {
                chat,
                started_by_hand: harness.name().to_owned(),
                cwd,
            },
        ))
    });
    Plan {
        warning: harness.map(warning),
        tell,
        program: real_program(word, &path, shims),
        path: without(&path, shims),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn program_in(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn joined(dirs: &[&Path]) -> OsString {
        std::env::join_paths(dirs).unwrap()
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let pairs: Vec<(String, OsString)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect();
        move |want| {
            pairs
                .iter()
                .find(|(k, _)| k == want)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn the_real_harness_is_found_past_the_shim_that_stands_in_front_of_it() {
        let dir = tempfile::tempdir().unwrap();
        let shims = dir.path().join("shims");
        program_in(&shims, "claude");
        let real = program_in(&dir.path().join("real"), "claude");

        let found = real_program(
            "claude",
            &joined(&[&shims, &dir.path().join("real")]),
            &shims,
        );

        assert_eq!(found, Some(real));
    }

    #[test]
    fn a_link_to_the_shim_directory_is_the_shim_directory() {
        // A `PATH` can name one directory two ways — `/var` and `/private/var` on macOS — and a
        // guard that compared spellings would find itself under the other one and run for ever.
        let dir = tempfile::tempdir().unwrap();
        let shims = dir.path().join("shims");
        program_in(&shims, "claude");
        let linked = dir.path().join("linked");
        std::os::unix::fs::symlink(&shims, &linked).unwrap();
        let real = program_in(&dir.path().join("real"), "claude");

        let found = real_program(
            "claude",
            &joined(&[&linked, &shims, &dir.path().join("real")]),
            &shims,
        );

        assert_eq!(found, Some(real));
    }

    #[test]
    fn a_harness_that_is_only_a_shim_is_not_found_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let shims = dir.path().join("shims");
        program_in(&shims, "claude");

        assert_eq!(real_program("claude", &joined(&[&shims]), &shims), None);
    }

    #[test]
    fn the_path_a_harness_runs_under_has_no_shims_on_it() {
        let dir = tempfile::tempdir().unwrap();
        let shims = dir.path().join("shims");
        std::fs::create_dir_all(&shims).unwrap();
        let path = joined(&[&shims, Path::new("/usr/bin"), &shims, Path::new("/bin")]);

        assert_eq!(
            without(&path, &shims),
            joined(&[Path::new("/usr/bin"), Path::new("/bin")])
        );
    }

    #[test]
    fn a_harness_started_in_a_chats_shell_warns_and_tells_the_app_and_runs_the_real_one() {
        let dir = tempfile::tempdir().unwrap();
        let shims = dir.path().join("shims");
        program_in(&shims, "codex");
        let real = program_in(&dir.path().join("real"), "codex");
        let path = joined(&[&shims, &dir.path().join("real")]);
        let env = env_of(&[
            ("PATH", path.to_str().unwrap()),
            (SOCKET_ENV, "/tmp/app/hooks.sock"),
            (CHAT_ENV, "9"),
        ]);

        let plan = plan("codex", &shims, &env, Some(PathBuf::from("/work/alpha")));

        assert_eq!(plan.warning, Some(warning(Harness::Codex)));
        assert_eq!(
            plan.tell,
            Some((
                PathBuf::from("/tmp/app/hooks.sock"),
                StartedByHand {
                    chat: 9,
                    started_by_hand: "codex".to_owned(),
                    cwd: Some(PathBuf::from("/work/alpha")),
                }
            ))
        );
        assert_eq!(plan.program, Some(real));
        assert_eq!(plan.path, joined(&[&dir.path().join("real")]));
    }

    #[test]
    fn a_harness_started_where_no_app_is_listening_still_warns_and_still_runs() {
        let dir = tempfile::tempdir().unwrap();
        let shims = dir.path().join("shims");
        let real = program_in(&dir.path().join("real"), "claude");
        let path = joined(&[&shims, &dir.path().join("real")]);
        let env = env_of(&[("PATH", path.to_str().unwrap())]);

        let plan = plan("claude", &shims, &env, None);

        assert_eq!(plan.warning, Some(warning(Harness::ClaudeCode)));
        assert_eq!(plan.tell, None);
        assert_eq!(plan.program, Some(real));
    }

    #[test]
    fn a_chat_number_that_is_not_one_tells_nobody() {
        let dir = tempfile::tempdir().unwrap();
        let shims = dir.path().join("shims");
        let env = env_of(&[
            ("PATH", "/usr/bin"),
            (SOCKET_ENV, "/tmp/app/hooks.sock"),
            (CHAT_ENV, "nine"),
        ]);

        assert_eq!(plan("claude", &shims, &env, None).tell, None);
    }

    #[test]
    fn the_warning_names_the_harness_and_says_where_it_belongs() {
        let said = warning(Harness::Opencode);

        assert!(said.contains("opencode"), "{said}");
        assert!(
            said.contains("outside charter's session tracking"),
            "{said}"
        );
        assert!(said.contains("chat tab"), "{said}");
    }

    #[test]
    fn a_shell_tab_finds_the_shims_before_anything_else_on_its_path() {
        let shims = Shims::at("/app/shims");
        let env = vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())];

        let start = shims.shell_start("/bin/sh", env, None, None);

        assert_eq!(
            start.env,
            vec![("PATH".to_owned(), "/app/shims/bin:/usr/bin:/bin".to_owned())]
        );
        assert!(start.args.is_empty());
    }

    #[test]
    fn a_shell_tab_given_no_path_puts_the_shims_before_the_one_it_inherits() {
        let shims = Shims::at("/app/shims");

        let start = shims.shell_start("/bin/sh", Vec::new(), Some(OsStr::new("/usr/bin")), None);

        assert_eq!(
            start.env,
            vec![("PATH".to_owned(), "/app/shims/bin:/usr/bin".to_owned())]
        );
    }

    #[test]
    fn a_zsh_shell_tab_reads_charters_start_files_which_hand_back_the_operators() {
        let shims = Shims::at("/app/shims");
        let env = vec![("PATH".to_owned(), "/usr/bin".to_owned())];

        let start = shims.shell_start("/bin/zsh", env, None, Some(OsStr::new("/home/me/.zsh")));

        assert!(start.args.is_empty());
        assert!(
            start
                .env
                .contains(&("ZDOTDIR".to_owned(), "/app/shims/zsh".to_owned()))
        );
        assert!(
            start
                .env
                .contains(&(USER_ZDOTDIR_ENV.to_owned(), "/home/me/.zsh".to_owned()))
        );
    }

    #[test]
    fn a_zsh_shell_tab_whose_operator_has_no_zdotdir_carries_none_to_hand_back() {
        let shims = Shims::at("/app/shims");

        let start = shims.shell_start("zsh", Vec::new(), Some(OsStr::new("/usr/bin")), None);

        assert!(start.env.iter().all(|(name, _)| name != USER_ZDOTDIR_ENV));
        assert!(
            start
                .env
                .contains(&("ZDOTDIR".to_owned(), "/app/shims/zsh".to_owned()))
        );
    }

    #[test]
    fn a_bash_shell_tab_reads_charters_rcfile_which_sources_the_operators() {
        let shims = Shims::at("/app/shims");

        let start = shims.shell_start("/usr/bin/bash", Vec::new(), Some(OsStr::new("/bin")), None);

        assert_eq!(
            start.args,
            vec!["--rcfile".to_owned(), "/app/shims/bash/bashrc".to_owned()]
        );
    }

    #[test]
    fn every_shim_and_start_file_is_written_and_every_shim_runs() {
        let dir = tempfile::tempdir().unwrap();
        let shims = Shims::at(dir.path().join("with space"));

        shims
            .write(Path::new(
                "/Applications/charter.app/Contents/MacOS/charter",
            ))
            .unwrap();

        for harness in SHIMMED {
            let shim = shims.bin().join(harness.name());
            let mode = std::fs::metadata(&shim).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "{} is not runnable", shim.display());
            let text = std::fs::read_to_string(&shim).unwrap();
            assert!(text.contains(COMMAND), "{text}");
            assert!(
                text.contains(&format!(" {} -- \"$@\"", harness.name())),
                "{text}"
            );
        }
        for file in [".zshenv", ".zprofile", ".zshrc"] {
            assert!(shims.zdotdir().join(file).is_file(), "no {file}");
        }
        assert!(shims.bashrc().is_file());
    }

    #[test]
    fn writing_the_shims_again_replaces_them_with_the_running_apps_charter() {
        let dir = tempfile::tempdir().unwrap();
        let shims = Shims::at(dir.path());

        shims.write(Path::new("/old/charter")).unwrap();
        shims.write(Path::new("/new/charter")).unwrap();

        let text = std::fs::read_to_string(shims.bin().join("claude")).unwrap();
        assert!(text.contains("/new/charter"), "{text}");
        assert!(!text.contains("/old/charter"), "{text}");
    }

    /// A stand-in for a program: a script that writes the words it was run with, one per
    /// line, to `out`. Run through `/bin/sh` by the shim, never exec'd by the test itself.
    fn recorder(dir: &Path, name: &str, out: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\" > '{}'\n",
                out.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn a_shim_hands_its_harness_and_every_argument_to_charter_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("said");
        let charter = recorder(&dir.path().join("app"), "charter", &out);
        let shims = Shims::at(dir.path().join("shims"));
        shims.write(&charter).unwrap();

        let ran = crate::forklock::status(
            std::process::Command::new("/bin/sh")
                .arg(shims.bin().join("claude"))
                .args(["-p", "two words", "--", "it's"]),
        )
        .unwrap();

        assert!(ran.success());
        let said = std::fs::read_to_string(&out).unwrap();
        let bin = shims.bin().display().to_string();
        assert_eq!(
            said.lines().collect::<Vec<_>>(),
            vec![
                charter.to_str().unwrap(),
                COMMAND,
                "--shims",
                bin.as_str(),
                "claude",
                "--",
                "-p",
                "two words",
                "--",
                "it's",
            ]
        );
    }

    #[test]
    fn a_shim_whose_charter_has_gone_runs_the_real_harness_itself() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("said");
        let real = recorder(&dir.path().join("real"), "codex", &out);
        let shims = Shims::at(dir.path().join("shims"));
        shims
            .write(&dir.path().join("gone").join("charter"))
            .unwrap();
        let path = joined(&[
            &shims.bin(),
            &dir.path().join("real"),
            Path::new("/usr/bin"),
            Path::new("/bin"),
        ]);

        let ran = crate::forklock::status(
            std::process::Command::new("/bin/sh")
                .arg(shims.bin().join("codex"))
                .args(["exec", "a b"])
                .env("PATH", &path),
        )
        .unwrap();

        assert!(ran.success());
        let said = std::fs::read_to_string(&out).unwrap();
        assert_eq!(
            said.lines().collect::<Vec<_>>(),
            vec![real.to_str().unwrap(), "exec", "a b"]
        );
    }

    /// Whether this machine has `shell`, for the tests that start a real one. CI's Linux
    /// runner has bash and may not have zsh; the operator's Mac has both.
    fn have(shell: &str) -> bool {
        crate::forklock::status(std::process::Command::new(shell).args(["-c", "exit 0"]))
            .is_ok_and(|status| status.success())
    }

    /// `PATH` as a real interactive `shell` tab sees it once every start file has run, with a
    /// `HOME` whose start file puts `~/.local/bin` first — the operator's own `~/.zshrc`.
    fn path_seen_by(shell: &str, start_file: &str) -> Option<String> {
        if !have(shell) {
            eprintln!("no {shell} on this machine; the test that starts one is not run here");
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(start_file),
            "export PATH=\"$HOME/.local/bin:$PATH\"\n",
        )
        .unwrap();
        let shims = Shims::at(dir.path().join("app shims"));
        shims.write(Path::new("/nowhere/charter")).unwrap();
        let start = shims.shell_start(
            shell,
            vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())],
            None,
            None,
        );
        let out = crate::forklock::output(
            std::process::Command::new(shell)
                .args(&start.args)
                .args(["-i", "-c", "printf '%s' \"$PATH\""])
                .env_clear()
                .env("HOME", &home)
                .env("TERM", "dumb")
                .envs(start.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .stdin(std::process::Stdio::null()),
        )
        .unwrap();
        let path = String::from_utf8(out.stdout).unwrap();
        let first = path.split(':').next().unwrap_or_default().to_owned();
        assert_eq!(first, shims.bin().display().to_string(), "PATH was {path}");
        assert!(
            path.contains(&format!("{}/.local/bin", home.display())),
            "the operator's own start file did not run: PATH was {path}"
        );
        Some(path)
    }

    #[test]
    fn a_zsh_shell_tab_puts_the_shims_first_after_the_operators_zshrc_has_run() {
        path_seen_by("zsh", ".zshrc");
    }

    #[test]
    fn a_bash_shell_tab_puts_the_shims_first_after_the_operators_bashrc_has_run() {
        path_seen_by("bash", ".bashrc");
    }
}
