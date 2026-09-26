//! `charter browser install` — generate Playwright's own page-driving skill into the plane,
//! from the tool that owns it. A port of `charter/browser.py` and `commands.cmd_browser_install`.
//!
//! A plane that drives a browser needs two things, and only one of them is charter's. **How to
//! drive a page** is Playwright's: long, Apache-2.0, and published far more often than charter
//! is, so charter vendors none of it and runs `npx @playwright/cli@<version> install --skills`
//! instead, which writes `.claude/skills/playwright-cli/`. **Where credentials come from** is
//! charter's, and that is the `browser` skill charter's plugin ships.
//!
//! # The version is a version and nothing else (charter#332)
//!
//! The right-hand side of `@playwright/cli@<spec>` is not a version slot to npm: it takes a
//! range, a dist-tag, an alias or a git URL, and `--yes` runs whatever that fetches. So the
//! version is checked as an exact semver string before it reaches the spec, and asking npm
//! would make a hostile spec legal exactly when the attacker's package exists.
//!
//! # What it leaves behind
//!
//! `.playwright-cli/` is where the CLI writes traces and snapshots, and a trace records the
//! network — a traced login holds the credential the vault kept out of the transcript — so it
//! is gitignored. The generated pages and `.playwright/cli.config.json` carry no credential,
//! so whether they are committed is the plane's choice, and the command says so rather than
//! deciding.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::repocmd::{Say, Sink};

/// The `@playwright/cli` version installed when none is asked for — known to work with the
/// `browser` skill, not a claim about what is current.
pub const PINNED: &str = "0.1.18";

/// Where the generator writes its pages, which Claude Code reads as project skills.
pub const SKILL_DIR: &str = ".claude/skills/playwright-cli";

/// The CLI's output directory: traces, snapshots, screenshots.
pub const OUTPUT_DIR: &str = ".playwright-cli";

/// The directory `install` creates for `.playwright/cli.config.json`.
pub const CONFIG_DIR: &str = ".playwright";

/// How long the generator may run: a cold npm cache fetches a package, and a registry auth
/// prompt must not hang the command for ever.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);

/// Whether `version` is an exact semver version — `browser.version_ok`, semver.org's grammar.
pub fn version_ok(version: &str) -> bool {
    static EXACT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+(?:[0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$",
        )
        .expect("the semver pattern compiles")
    });
    EXACT.is_match(version)
}

/// What charter says to a `--version` that is not one — `browser.NOT_A_VERSION`.
pub fn not_a_version(version: &str) -> String {
    format!(
        "'{}' is not a version. It is interpolated into the npm package spec \
         `@playwright/cli@<version>`, where npm would also accept a dist-tag, an alias or a git \
         URL — so this slot takes an exact version (1.2.3, or 1.2.3-rc.1) and nothing else",
        crate::shown::short(version)
    )
}

/// The generator's command line — `browser.install_argv`. `--yes`, because npx asks a
/// question on a terminal with a cold cache and charter has captured the pipe it asks on.
/// `None` for a version [`version_ok`] refuses: this is the last gate before the spec.
pub fn install_argv(version: &str) -> Option<Vec<String>> {
    version_ok(version).then(|| {
        vec![
            "npx".into(),
            "--yes".into(),
            format!("@playwright/cli@{version}"),
            "install".into(),
            "--skills".into(),
        ]
    })
}

/// How the generator is run, so a test can stand one in without a network.
pub trait Generator {
    /// Run `argv` in `cwd`: `(exit status, combined output)`, or why it could not run.
    fn run(&mut self, argv: &[String], cwd: &Path) -> Result<(i32, String), String>;
}

/// The real one: `npx` from `$PATH`, bounded by [`INSTALL_TIMEOUT`].
pub struct Npx;

impl Generator for Npx {
    fn run(&mut self, argv: &[String], cwd: &Path) -> Result<(i32, String), String> {
        use std::io::Read;
        // Windows spells the program `npx.cmd`, and `Command` does not add the extension.
        let program = if cfg!(windows) {
            "npx.cmd"
        } else {
            argv[0].as_str()
        };
        let mut command = std::process::Command::new(program);
        command
            .args(&argv[1..])
            .current_dir(cwd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        // Its own process group, so a timeout stops npx AND the node it started, rather than
        // leaving that writing into the plane after charter said it was stopped.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = crate::forklock::spawn(&mut command)
            .map_err(|e| format!("npx could not be started ({e})"))?;
        // Each stream is read on its own thread (no pipe deadlock) and handed back over a
        // channel, so a grandchild that keeps a pipe open cannot hold this command past the
        // deadline: what arrived by then is what is said.
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let streams: [Option<Box<dyn Read + Send>>; 2] = [
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ];
        for stream in streams.into_iter().flatten() {
            let tx = tx.clone();
            let mut stream = stream;
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = stream.read_to_string(&mut s);
                let _ = tx.send(s);
            });
        }
        drop(tx);
        let started = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if started.elapsed() > INSTALL_TIMEOUT {
                #[cfg(unix)]
                if let Some(group) = rustix::process::Pid::from_raw(child.id() as i32) {
                    let _ =
                        rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Ok((
                    1,
                    format!(
                        "the generator did not finish within {}s and was stopped. npm may be \
                         unreachable, or a registry auth prompt may be waiting; try the command \
                         by hand:\n  {}",
                        INSTALL_TIMEOUT.as_secs(),
                        argv.join(" ")
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let mut text = String::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(part) => text.push_str(&part),
                Err(_) => break,
            }
        }
        Ok((status.code().unwrap_or(1), text))
    }
}

/// Whether an `npx` is on `path` (a `$PATH` value).
pub fn npx_on(path: Option<&std::ffi::OsStr>) -> bool {
    // Windows spells it `npx.cmd`, and resolves the extension itself.
    let names: &[&str] = if cfg!(windows) {
        &["npx.cmd", "npx.exe", "npx"]
    } else {
        &["npx"]
    };
    path.is_some_and(|p| {
        std::env::split_paths(p).any(|dir| names.iter().any(|n| dir.join(n).is_file()))
    })
}

/// Every `.md` page under `dir`, links not followed.
fn pages(dir: &Path) -> usize {
    let Ok(reader) = std::fs::read_dir(dir) else {
        return 0;
    };
    reader
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => pages(&e.path()),
            Ok(t) if t.is_file() && e.path().extension().is_some_and(|x| x == "md") => 1,
            _ => 0,
        })
        .sum()
}

/// `charter browser install [--version V]`, and its exit code. `npx_found` is whether an
/// `npx` is on the caller's `$PATH`.
pub fn install(
    root: &Path,
    version: Option<&str>,
    npx_found: bool,
    generator: &mut dyn Generator,
    say: Sink,
) -> u8 {
    let version = version.filter(|v| !v.is_empty()).unwrap_or(PINNED);
    if !version_ok(version) {
        say(Say::Fail(not_a_version(version)));
        return 1;
    }
    if !npx_found {
        say(Say::Fail(
            "npx not found — install Node.js to generate the Playwright skill.".into(),
        ));
        say(Say::Info(
            "The `browser` skill's credential bridge works regardless; only the page-driving \
             reference needs this."
                .into(),
        ));
        return 1;
    }
    let skill_dir: PathBuf = root.join(SKILL_DIR);
    // The generator writes where it runs — its pages and its config directory — and a
    // committed link at either that leaves the plane is not somewhere charter points one.
    for (shown, path) in [
        (SKILL_DIR, &skill_dir),
        (CONFIG_DIR, &root.join(CONFIG_DIR)),
    ] {
        if !crate::contain::within_plane(root, path) {
            say(Say::Fail(format!(
                "{shown} resolves out of the plane — charter will not run a generator that \
                 writes through it."
            )));
            return 1;
        }
    }
    let argv = install_argv(version).expect("the version was checked above");
    say(Say::Info(format!(
        "Generating the Playwright driving surface (@playwright/cli@{version})…"
    )));
    let (code, output) = match generator.run(&argv, root) {
        Ok(ran) => ran,
        Err(why) => {
            say(Say::Fail(why));
            return 1;
        }
    };
    if code != 0 {
        say(Say::Fail(format!("the generator failed (exit {code}).")));
        // npm's own diagnosis, handed back as it said it.
        say(Say::Out(output.trim_end().to_string()));
        return 1;
    }
    if !skill_dir.is_dir() {
        say(Say::Warn(format!(
            "The generator reported success but {SKILL_DIR} is not there."
        )));
        return 1;
    }
    say(Say::Done(format!(
        "Wrote {SKILL_DIR} ({} page(s))",
        pages(&skill_dir)
    )));
    say(Say::Info(
        "It is Playwright's, not charter's — regenerate it with this command rather than \
         editing it."
            .into(),
    ));
    let gitignore = root.join(".gitignore");
    let ignored = if crate::contain::within_plane(root, &gitignore) {
        crate::scaffold::append_gitignore(
            &gitignore,
            &[&format!("{OUTPUT_DIR}/")],
            "added by `charter browser install`",
        )
    } else {
        Err(".gitignore resolves out of the plane".to_string())
    };
    match ignored {
        Ok(lines) => {
            for line in lines {
                say(Say::Done(format!(
                    "Ignored {line} — traces and snapshots of authenticated runs, not source."
                )));
            }
        }
        Err(why) => say(Say::Warn(format!(
            "{OUTPUT_DIR}/ is NOT ignored ({why}) — a trace there records authenticated \
             traffic; add it to .gitignore yourself."
        ))),
    }
    say(Say::Info(format!(
        "{SKILL_DIR} and {CONFIG_DIR}/ are left tracked-or-not as you choose: commit the pages \
         and a fresh clone needs no npx round trip, at the cost of a tree a later `install` \
         rewrites under you. Both answers are fine; charter does not pick for you."
    )));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personaverbs::tests_plane::Heard;

    #[test]
    fn a_version_is_an_exact_semver_and_nothing_npm_would_also_take() {
        for ok in ["0.1.18", "1.2.3", "1.2.3-rc.1", "1.0.0+build.5", "10.20.30"] {
            assert!(version_ok(ok), "{ok}");
        }
        for bad in [
            "latest",
            "^1.2.3",
            "1.2",
            "01.2.3",
            "github:attacker/x",
            "npm:other@1",
            "1.2.3 || 2",
            "1.2.3\n",
            "",
        ] {
            assert!(!version_ok(bad), "{bad:?}");
            assert_eq!(install_argv(bad), None);
        }
        assert_eq!(
            install_argv("0.1.18").unwrap(),
            [
                "npx",
                "--yes",
                "@playwright/cli@0.1.18",
                "install",
                "--skills"
            ]
        );
    }

    /// Stands in for npx: records what it was asked, and writes `pages` pages where a real
    /// generator would.
    struct Fake {
        asked: Vec<Vec<String>>,
        code: i32,
        pages: usize,
    }

    impl Generator for Fake {
        fn run(&mut self, argv: &[String], cwd: &Path) -> Result<(i32, String), String> {
            self.asked.push(argv.to_vec());
            if self.pages > 0 {
                let dir = cwd.join(SKILL_DIR);
                std::fs::create_dir_all(&dir).unwrap();
                for n in 0..self.pages {
                    std::fs::write(dir.join(format!("p{n}.md")), "x").unwrap();
                }
            }
            Ok((self.code, "npm said so\n".into()))
        }
    }

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
        dir
    }

    fn run(root: &Path, version: Option<&str>, fake: &mut Fake) -> (u8, Heard) {
        let mut heard = Heard::default();
        let rc = install(root, version, true, fake, &mut heard.sink());
        (rc, heard)
    }

    #[test]
    fn install_runs_the_pinned_generator_and_ignores_the_trace_directory_once() {
        let dir = plane();
        std::fs::write(dir.path().join(".gitignore"), "/.charter/\n").unwrap();
        let mut fake = Fake {
            asked: Vec::new(),
            code: 0,
            pages: 2,
        };
        let (rc, heard) = run(dir.path(), None, &mut fake);
        assert_eq!(rc, 0, "{}", heard.err);
        assert_eq!(fake.asked, [install_argv(PINNED).unwrap()]);
        assert!(
            heard
                .err
                .contains("✓ Wrote .claude/skills/playwright-cli (2 page(s))\n")
        );
        assert!(heard.err.contains("✓ Ignored .playwright-cli/ — traces"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "/.charter/\n\n# added by `charter browser install`\n.playwright-cli/\n"
        );
        let (rc, heard) = run(dir.path(), Some("0.2.0"), &mut fake);
        assert_eq!(rc, 0);
        assert!(!heard.err.contains("Ignored"), "{}", heard.err);
        assert_eq!(fake.asked[1][2], "@playwright/cli@0.2.0");
    }

    #[test]
    fn a_version_that_is_not_one_runs_nothing() {
        let dir = plane();
        let mut fake = Fake {
            asked: Vec::new(),
            code: 0,
            pages: 1,
        };
        let (rc, heard) = run(dir.path(), Some("github:attacker/x"), &mut fake);
        assert_eq!(rc, 1);
        assert!(
            heard
                .err
                .starts_with("✗ 'github:attacker/x' is not a version.")
        );
        assert!(fake.asked.is_empty());
        assert!(!dir.path().join(".gitignore").exists());
    }

    #[test]
    fn a_failed_generator_hands_back_npms_words_and_writes_nothing() {
        let dir = plane();
        let mut fake = Fake {
            asked: Vec::new(),
            code: 7,
            pages: 0,
        };
        let (rc, heard) = run(dir.path(), None, &mut fake);
        assert_eq!(rc, 1);
        assert!(heard.err.contains("✗ the generator failed (exit 7).\n"));
        assert_eq!(heard.out, "npm said so\n");
        assert!(!dir.path().join(".gitignore").exists());
        // Success with nothing written is not success.
        fake.code = 0;
        let (rc, heard) = run(dir.path(), None, &mut fake);
        assert_eq!(rc, 1);
        assert!(heard.err.contains("is not there"), "{}", heard.err);
    }

    #[test]
    fn without_npx_it_says_so_and_runs_nothing() {
        let dir = plane();
        let mut fake = Fake {
            asked: Vec::new(),
            code: 0,
            pages: 1,
        };
        let mut heard = Heard::default();
        let rc = install(dir.path(), None, false, &mut fake, &mut heard.sink());
        assert_eq!(rc, 1);
        assert!(heard.err.starts_with("✗ npx not found"));
        assert!(fake.asked.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_the_generator_writes_linked_out_of_the_plane_is_never_generated_into() {
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".playwright")).unwrap();
        let mut fake = Fake {
            asked: Vec::new(),
            code: 0,
            pages: 1,
        };
        let (rc, _) = run(dir.path(), None, &mut fake);
        assert_eq!(rc, 1);
        assert!(fake.asked.is_empty());
        std::fs::remove_file(dir.path().join(".playwright")).unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".claude/skills")).unwrap();
        let mut fake = Fake {
            asked: Vec::new(),
            code: 0,
            pages: 1,
        };
        let (rc, heard) = run(dir.path(), None, &mut fake);
        assert_eq!(rc, 1, "{}", heard.err);
        assert!(fake.asked.is_empty());
        assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
    }
}
