//! Who already fills Claude Code's status line here — and therefore whether charter may.
//!
//! # The measurement this module exists for
//!
//! A `statusLine` handed to Claude Code through `--settings` **is executed and rendered**, and
//! it **shadows** every other one: measured on Claude Code 2.1.280 (macOS, a real pty, a fenced
//! `HOME`) as a four-cell matrix, one launch per cell. With a `statusLine` in
//! `~/.claude/settings.json` AND one on the flag, only the flag's ran — the file's proof file
//! was never created, and nothing warned. Claude Code's own settings reference says why:
//! settings merge key by key, the highest level that sets a key supplies it whole, and
//! `statusLine` is on none of the merge-exception lists. The flag sits above user, project and
//! project-local settings, so it is simply the last writer.
//!
//! So arming charter's own footer command unconditionally would stop the operator's own
//! `statusLine` from running, for every chat the app starts, silently. That is a change to how
//! their machine behaves, made without asking.
//!
//! # The rule, which the operator gave on 2026-09-22
//!
//! **charter arms its `statusLine` only where nothing else fills the line. Where something
//! does, charter arms nothing and leaves their configuration alone** — and the cost, a chat
//! with no `ctx`/`cache` gauge, is reported by `doctor` rather than left to look like
//! breakage. His reason: silently replacing somebody's configuration is the worse failure, and
//! `doctor` exists so that a missing feature can explain itself.
//!
//! **And it fails toward leaving it alone.** A settings file that is there and cannot be read
//! or parsed answers [`Claim::Unknown`], which arms nothing: an unreadable file is not evidence
//! that the key is free. Neither is a file that turns every non-managed status line off
//! ([`Claim::Suppressed`]) — charter's own would be skipped there without a word, so arming it
//! would be charter claiming a gauge it cannot deliver.
//!
//! # What charter can see, and what it cannot
//!
//! Every source below is a FILE, which is what makes any of this answerable. Claude Code's
//! documented precedence has sources that are not files at all, and [`UNSEEN`] is the sentence
//! that says so wherever this answer is reported — because a green row that did not say it
//! would be claiming more than it looked at (ADR 0013).
//!
//! Read, highest precedence first:
//!
//! 1. the managed policy file and its `managed-settings.d/*.json` drop-ins, per operating
//!    system — `/Library/Application Support/ClaudeCode` on macOS, `/etc/claude-code` on Linux,
//!    `C:\Program Files\ClaudeCode` on Windows (**not** the legacy `ProgramData` path, which
//!    Claude Code's docs say it does not read);
//! 2. `remote-settings.json` in the config folder — the local cache of what an organisation
//!    serves. A cache is not the authority, so what it holds is evidence and its absence is
//!    not;
//! 3. `.claude/settings.local.json`, which since 2.1.211 lives at the **git root**, and in a
//!    worktree at the **main checkout's** root — all three candidates are read, because reading
//!    one too many can only make charter more careful;
//! 4. `.claude/settings.json` in the session's own directory (the host does not walk up:
//!    measured for `doctor`'s `session layer` row on 2.1.259, re-measured on 2.1.267);
//! 5. `settings.json` in the config folder (`$CLAUDE_CONFIG_DIR`, else `~/.claude`).
//!
//! Plugins are not on that list and do not need to be: a plugin's own `settings.json` may
//! carry `agent` and `subagentStatusLine` and nothing else, so no plugin can introduce the key
//! this module is about. `subagentStatusLine` is a different key, a different row, and not
//! charter's to fill.

use std::path::{Path, PathBuf};

/// The key this module is about.
const KEY: &str = "statusLine";

/// The keys that turn a non-managed status line off, so charter's would be skipped too.
///
/// `disableAllHooks` outside managed settings, and `allowManagedHooksOnly` anywhere, each
/// narrow the status line to a managed one — and Claude Code "skips your value without
/// warning". `--safe-mode` does the same and is a flag on the operator's own launch, which is
/// not something charter can see ([`UNSEEN`]).
const GATES: [&str; 2] = ["disableAllHooks", "allowManagedHooksOnly"];

/// How long git may take to say where this chat's repository is. Short, because a chat is
/// waiting on it and a git that does not answer only costs this check two candidate files.
const ASKING_GIT: std::time::Duration = std::time::Duration::from_secs(5);

/// How big a settings file may be before charter stops reading it. Claude Code's own are a few
/// kilobytes; a hostile or corrupt one is not read into memory whole.
const MOST: u64 = 1_048_576;

/// What this answer did NOT look at, to be said wherever it is reported.
///
/// Claude Code takes managed settings from an MDM profile and from the Windows registry, from
/// a `policyHelper` executable whose OUTPUT is the policy, and from an organisation's server at
/// sign-in; an embedding host can inject them; `/statusline` writes one mid-session; and the
/// operator's own `claude` can carry `--settings` of its own. None of those is a file charter
/// can read here.
pub const UNSEEN: &str = "charter reads the settings files. It cannot see an MDM or registry \
                          policy, a policyHelper's output, settings an organisation serves at \
                          sign-in, a `/statusline` written after this, or a `--settings` on \
                          somebody else's launch.";

/// What charter found out about the status line in force where a chat will run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// Nothing charter can see fills it. charter may arm its own for this chat.
    Free,
    /// Something fills it already, from `file`.
    Held {
        file: PathBuf,
        /// Whether that something IS `charter statusline` — the operator wired charter's own
        /// footer themselves, which is what charter's documentation has told them to do since
        /// 0.57.0. Then nothing is missing: their command records the turns, not charter's.
        charters_own: bool,
    },
    /// `file` turns off every status line but a managed one, so charter's would be skipped
    /// without a word. Nothing is armed.
    Suppressed { file: PathBuf, key: &'static str },
    /// A settings file is there and charter could not read it. **Arms nothing**: see the
    /// module note on failing toward leaving the operator's configuration alone.
    Unknown { file: PathBuf, why: String },
}

impl Claim {
    /// Whether charter may put its own `statusLine` on this chat's command line.
    pub fn free(&self) -> bool {
        matches!(self, Self::Free)
    }
}

/// What fills Claude Code's status line for a session run in `cwd`.
pub fn status_line(cwd: Option<&Path>) -> Claim {
    status_line_in(cwd, &crate::doctor::session::claude_config_home())
}

/// The same, against a named Claude Code config folder — which is what a test can hold still.
pub fn status_line_in(cwd: Option<&Path>, config_home: &Path) -> Claim {
    for file in sources(cwd, config_home) {
        match read(&file) {
            Ok(Found::Nothing) => {}
            Ok(Found::StatusLine { charters_own }) => return Claim::Held { file, charters_own },
            Ok(Found::Gate(key)) => return Claim::Suppressed { file, key },
            Err(why) => return Claim::Unknown { file, why },
        }
    }
    Claim::Free
}

/// What one settings file says about the status line.
enum Found {
    Nothing,
    StatusLine { charters_own: bool },
    Gate(&'static str),
}

/// Every file that can carry a `statusLine`, highest precedence first.
///
/// A file that is not there is simply not in force, so absence is not a source of doubt — only
/// a file charter cannot READ is ([`Claim::Unknown`]).
fn sources(cwd: Option<&Path>, config_home: &Path) -> Vec<PathBuf> {
    let mut files = managed();
    files.push(config_home.join("remote-settings.json"));
    if let Some(cwd) = cwd {
        // The local file's home moved to the git root in 2.1.211, and a worktree reads the
        // main checkout's. Every candidate is read: one extra file can only find a claim
        // charter would otherwise have armed over.
        for root in local_roots(cwd) {
            files.push(root.join(crate::guest::LOCAL_SETTINGS));
        }
        files.push(cwd.join(crate::layer::SETTINGS));
    }
    files.push(config_home.join("settings.json"));
    files
}

/// The managed policy file and its drop-ins, per operating system, in the order Claude Code
/// combines them (the base file, then `managed-settings.d/*.json` by name).
fn managed() -> Vec<PathBuf> {
    let dir = PathBuf::from(if cfg!(target_os = "macos") {
        "/Library/Application Support/ClaudeCode"
    } else if cfg!(target_os = "windows") {
        r"C:\Program Files\ClaudeCode"
    } else {
        "/etc/claude-code"
    });
    let mut files = vec![dir.join("managed-settings.json")];
    let mut drops: Vec<PathBuf> = std::fs::read_dir(dir.join("managed-settings.d"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    drops.sort();
    files.extend(drops);
    files
}

/// The directories whose `.claude/settings.local.json` could be this session's: the directory
/// itself, its git root, and — in a worktree — the main checkout's root.
///
/// Through [`crate::worktree::git`], like every other git this binary runs, and with a short
/// deadline: this is on the path that starts a chat. A git that does not answer costs this
/// list its git entries and nothing else — the candidates charter did read still decide, and
/// what charter did not look at is what [`UNSEEN`] is for.
fn local_roots(cwd: &Path) -> Vec<PathBuf> {
    let mut roots = vec![cwd.to_path_buf()];
    let asked = crate::worktree::git::run(
        cwd,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
        ],
        ASKING_GIT,
    );
    if let Ok(run) = asked
        && run.ok()
    {
        {
            let mut lines = run.out.lines().map(str::trim).filter(|l| !l.is_empty());
            if let Some(top) = lines.next() {
                roots.push(PathBuf::from(top));
            }
            // `<main checkout>/.git` for a linked worktree, and this repository's own `.git`
            // otherwise — either way its parent is the checkout that holds the file.
            if let Some(main) = lines.next().and_then(|common| {
                PathBuf::from(common)
                    .parent()
                    .map(std::path::Path::to_path_buf)
            }) {
                roots.push(main);
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

/// What `file` says, or why charter could not tell.
///
/// `Ok(Found::Nothing)` includes the file not being there at all.
fn read(file: &Path) -> Result<Found, String> {
    let found = match std::fs::metadata(file) {
        Ok(found) => found,
        // Not there is not doubt: nothing in it can be in force.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Found::Nothing),
        Err(e) => return Err(e.to_string()),
    };
    if !found.is_file() {
        return Err("it is not a file".to_owned());
    }
    if found.len() > MOST {
        return Err(format!(
            "it is {} bytes, which charter will not read",
            found.len()
        ));
    }
    let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let doc: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("it is not JSON charter can read: {e}"))?;
    if let Some(key) = GATES
        .iter()
        .find(|key| doc.get(**key) == Some(&serde_json::Value::Bool(true)))
    {
        return Ok(Found::Gate(key));
    }
    match doc.get(KEY) {
        // `null` is what a key explicitly turned off looks like, and Claude Code reads it as
        // nothing: it does not fill the line, so it does not hold it against charter either.
        None | Some(serde_json::Value::Null) => Ok(Found::Nothing),
        Some(claim) => Ok(Found::StatusLine {
            charters_own: is_charters_own(claim),
        }),
    }
}

/// Whether a `statusLine` value is `charter statusline`.
///
/// **This decides a sentence, never the arming.** charter does not arm where anything at all is
/// in force; this only lets `doctor` say "yours already runs charter's" rather than warning an
/// operator who did exactly what charter's own documentation told them to do (`frame.md`:
/// *"Wire a `statusLine` to `charter statusline` yourself"*).
///
/// Deliberately narrow: a command whose first word names `charter` and whose second word is
/// `statusline`. Anything cleverer would be charter guessing at a shell line.
fn is_charters_own(claim: &serde_json::Value) -> bool {
    let Some(command) = claim.get("command").and_then(serde_json::Value::as_str) else {
        return false;
    };
    let mut words = command.split_whitespace();
    let program = words.next().unwrap_or_default().trim_matches(['\'', '"']);
    let verb = words.next().unwrap_or_default();
    let named = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    named == "charter" && verb == "statusline"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory with `body` at `name` in it.
    fn project(body: &str, name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let cwd = std::fs::canonicalize(dir.path()).expect("it resolves");
        std::fs::create_dir_all(cwd.join(".claude")).expect(".claude");
        std::fs::write(cwd.join(name), body).expect("settings");
        (dir, cwd)
    }

    /// One `statusLine`, as a settings file holds it.
    fn fills(command: &str) -> String {
        format!(r#"{{"statusLine": {{"type": "command", "command": "{command}"}}}}"#)
    }

    /// The claim for `cwd` alone: the config folder is an empty directory, so the machine
    /// running the test cannot decide the answer.
    fn claim_in(cwd: &Path) -> Claim {
        let empty = tempfile::tempdir().expect("a directory");
        status_line_in(Some(cwd), empty.path())
    }

    #[test]
    fn a_directory_with_no_settings_at_all_leaves_the_line_free() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(claim_in(dir.path()), Claim::Free);
    }

    #[test]
    fn a_project_settings_file_that_fills_the_line_holds_it() {
        let (_d, cwd) = project(&fills("~/bin/my-line"), ".claude/settings.json");

        let claim = claim_in(&cwd);

        assert_eq!(
            claim,
            Claim::Held {
                file: cwd.join(".claude/settings.json"),
                charters_own: false
            }
        );
        assert!(
            !claim.free(),
            "charter would have armed over the operator's"
        );
    }

    #[test]
    fn the_local_settings_file_is_read_before_the_shared_one() {
        // Claude Code's precedence: `.claude/settings.local.json` outranks
        // `.claude/settings.json`, so the file a row names has to be the one that would run.
        let (_d, cwd) = project(&fills("shared"), ".claude/settings.json");
        std::fs::write(cwd.join(".claude/settings.local.json"), fills("local")).unwrap();

        assert_eq!(
            claim_in(&cwd),
            Claim::Held {
                file: cwd.join(".claude/settings.local.json"),
                charters_own: false
            }
        );
    }

    #[test]
    fn a_settings_file_with_no_status_line_in_it_holds_nothing() {
        let (_d, cwd) = project(r#"{"env": {"A": "b"}}"#, ".claude/settings.json");

        assert_eq!(claim_in(&cwd), Claim::Free);
    }

    #[test]
    fn a_status_line_explicitly_set_to_null_fills_nothing_and_holds_nothing() {
        let (_d, cwd) = project(r#"{"statusLine": null}"#, ".claude/settings.json");

        assert_eq!(claim_in(&cwd), Claim::Free);
    }

    #[test]
    fn a_settings_file_charter_cannot_read_arms_nothing() {
        // The rule the operator gave: fail toward leaving their configuration alone. An
        // unreadable file is not evidence that the key is free.
        let (_d, cwd) = project("{ this is not json", ".claude/settings.json");

        let claim = claim_in(&cwd);

        assert!(!claim.free(), "charter armed over a file it could not read");
        let Claim::Unknown { file, why } = claim else {
            panic!("an unreadable file is not an answer: {claim:?}");
        };
        assert_eq!(file, cwd.join(".claude/settings.json"));
        assert!(why.contains("not JSON"), "{why}");
    }

    #[test]
    fn a_file_that_turns_every_unmanaged_status_line_off_arms_nothing() {
        // Claude Code "skips your value without warning" under either key, so charter's would
        // be skipped too — and a gauge charter cannot deliver is not one it may claim.
        for key in GATES {
            let (_d, cwd) = project(&format!(r#"{{"{key}": true}}"#), ".claude/settings.json");

            let claim = claim_in(&cwd);

            assert_eq!(
                claim,
                Claim::Suppressed {
                    file: cwd.join(".claude/settings.json"),
                    key
                }
            );
            assert!(!claim.free());
        }
        // And `false` is not a gate: it is the ordinary state written out.
        let (_d, cwd) = project(r#"{"disableAllHooks": false}"#, ".claude/settings.json");
        assert_eq!(claim_in(&cwd), Claim::Free);
    }

    #[test]
    fn the_operators_own_charter_statusline_is_recognised_rather_than_warned_about() {
        // charter's own documentation tells an operator to wire this themselves (#895). An
        // operator who did is not missing a gauge, and `doctor` says so instead of warning.
        for command in [
            "charter statusline",
            "'/opt/charter.app/Contents/MacOS/charter' statusline",
            "/usr/local/bin/charter statusline",
        ] {
            let (_d, cwd) = project(&fills(command), ".claude/settings.json");

            assert_eq!(
                claim_in(&cwd),
                Claim::Held {
                    file: cwd.join(".claude/settings.json"),
                    charters_own: true
                },
                "{command}"
            );
        }
    }

    #[test]
    fn something_that_merely_mentions_charter_is_not_charters_statusline() {
        for command in [
            "my-charter-wrapper statusline",
            "charter status",
            "charter",
            "sh -c 'charter statusline'",
        ] {
            let (_d, cwd) = project(&fills(command), ".claude/settings.json");

            assert_eq!(
                claim_in(&cwd),
                Claim::Held {
                    file: cwd.join(".claude/settings.json"),
                    charters_own: false
                },
                "{command}"
            );
        }
    }

    #[test]
    fn the_config_folders_own_settings_hold_the_line_for_a_chat_with_no_directory() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("settings.json"), fills("my-line")).unwrap();

        let claim = status_line_in(None, home.path());

        assert_eq!(
            claim,
            Claim::Held {
                file: home.path().join("settings.json"),
                charters_own: false
            }
        );
    }

    #[test]
    fn what_an_organisation_serves_is_read_from_the_cache_it_is_kept_in() {
        // `remote-settings.json` is a cache and not the authority, so what it holds is
        // evidence and its absence is not — which is why `UNSEEN` says so out loud.
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("remote-settings.json"),
            fills("policy-line"),
        )
        .unwrap();
        std::fs::write(home.path().join("settings.json"), fills("mine")).unwrap();

        let claim = status_line_in(None, home.path());

        assert_eq!(
            claim,
            Claim::Held {
                file: home.path().join("remote-settings.json"),
                charters_own: false
            },
            "the organisation's cached policy outranks the operator's own file"
        );
    }

    /// git for a fixture's own setup, never the code under test: pinned identity, no global
    /// config — the same shape `doctor`'s own tests use.
    fn fixture_git(dir: &Path, args: &[&str]) {
        // `forklock::output`, never `Command::output`, which this workspace disallows.
        let mut cmd = std::process::Command::new("git");
        cmd.arg("-C")
            .arg(dir)
            .args(crate::testgit::unsigned(args))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid");
        let out = crate::forklock::output(&mut cmd).expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn a_worktrees_chat_reads_the_local_settings_at_the_main_checkouts_root() {
        // Claude Code's documented rule since 2.1.211: in a worktree the local settings file
        // in force is the MAIN checkout's. charter starts chats in worktrees, so a check that
        // read only the worktree's own directory would arm over a status line that runs.
        let held = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(held.path()).unwrap();
        let main = here.join("main");
        std::fs::create_dir_all(&main).unwrap();
        fixture_git(&main, &["init", "-q"]);
        std::fs::write(main.join("a"), "a").unwrap();
        fixture_git(&main, &["add", "a"]);
        fixture_git(&main, &["commit", "-qm", "one"]);
        std::fs::create_dir_all(main.join(".claude")).unwrap();
        std::fs::write(main.join(".claude/settings.local.json"), fills("theirs")).unwrap();
        let cut = here.join("cut");
        fixture_git(
            &main,
            &["worktree", "add", "-q", cut.to_str().unwrap(), "-b", "side"],
        );

        assert_eq!(
            claim_in(&cut),
            Claim::Held {
                file: main.join(".claude/settings.local.json"),
                charters_own: false
            },
            "the main checkout's local settings were not read for a worktree's chat"
        );
    }

    #[test]
    fn a_chat_deep_in_a_worktree_reads_that_worktrees_own_root_as_well() {
        // The second git answer, and why both are asked for. A file left at the worktree's own
        // root — where Claude Code put it before 2.1.211, and where its documented fallbacks
        // still put it — is above this chat's directory and below the main checkout, so
        // neither the directory itself nor the main checkout's root would find it.
        let held = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(held.path()).unwrap();
        let main = here.join("main");
        std::fs::create_dir_all(&main).unwrap();
        fixture_git(&main, &["init", "-q"]);
        std::fs::write(main.join("a"), "a").unwrap();
        fixture_git(&main, &["add", "a"]);
        fixture_git(&main, &["commit", "-qm", "one"]);
        let cut = here.join("cut");
        fixture_git(
            &main,
            &["worktree", "add", "-q", cut.to_str().unwrap(), "-b", "side"],
        );
        std::fs::create_dir_all(cut.join(".claude")).unwrap();
        std::fs::write(cut.join(".claude/settings.local.json"), fills("theirs")).unwrap();
        let deeper = cut.join("packages/thing");
        std::fs::create_dir_all(&deeper).unwrap();

        assert_eq!(
            claim_in(&deeper),
            Claim::Held {
                file: cut.join(".claude/settings.local.json"),
                charters_own: false
            },
            "the worktree's own root was not read for a chat inside it"
        );
    }

    #[test]
    fn the_local_settings_file_at_the_repositorys_root_is_read_as_well_as_the_directorys() {
        // Since 2.1.211 the local file lives at the git root rather than where the session
        // was started, so a chat in a subdirectory reads a file charter would otherwise miss.
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        fixture_git(&root, &["init", "-q"]);
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(root.join(".claude/settings.local.json"), fills("theirs")).unwrap();
        let deeper = root.join("packages/thing");
        std::fs::create_dir_all(&deeper).unwrap();

        assert_eq!(
            claim_in(&deeper),
            Claim::Held {
                file: root.join(".claude/settings.local.json"),
                charters_own: false
            }
        );
    }
}
