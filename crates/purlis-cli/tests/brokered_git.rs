//! `purlis clone` and `purlis worktree add` in a chat the app started: handed to the app over
//! the chat's hook socket, run there by the same core functions, and printed here as the
//! terminal's own run prints them (#1335, ADR 0067 §2).
//!
//! **The app here is the core's broker on a real listener** (`purlis_core::gitbroker`, which
//! the app's answerer calls with its record of the chat), and the forge is a directory of bare
//! repositories that `$HOME/.gitconfig` rewrites GitHub's `acme` to, as `repo_commands.rs`
//! does. The broker's git reads THIS process's `HOME`, so the test that runs it re-runs itself
//! with `HOME` set ([`purlis_core::testrun::rerun`]).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use purlis_core::gitbroker::{self, Asker};
use purlis_core::hookwire::{
    self, Answer, Ask, CHAT_ENV, ChatToken, GitWork, Listener, Reading, SOCKET_ENV, TOKEN_ENV,
};
use purlis_core::worktree::git;
use serde_json::json;

fn purlis() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_purlis"))
}

const IDENTITY: [(&str, &str); 6] = [
    ("GIT_AUTHOR_NAME", "Tester"),
    ("GIT_AUTHOR_EMAIL", "t@e.invalid"),
    ("GIT_COMMITTER_NAME", "Tester"),
    ("GIT_COMMITTER_EMAIL", "t@e.invalid"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_TERMINAL_PROMPT", "0"),
];

/// A home whose git config points GitHub's `acme` at a directory of bare repos, and planes
/// beside it.
struct World {
    base: PathBuf,
    home: PathBuf,
    forge: PathBuf,
}

impl World {
    fn at(base: &Path) -> World {
        let base = std::fs::canonicalize(base).unwrap();
        let home = base.join("home");
        let forge = base.join("forge");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(forge.join("acme")).unwrap();
        std::fs::write(
            home.join(".gitconfig"),
            // And a content filter of the operator's, which `widget`'s `.gitattributes` asks
            // for: it runs on a terminal's clone, and never on one the app makes for a chat.
            format!(
                "[url \"file://{}/acme/\"]\n\tinsteadOf = https://github.com/acme/\n\
                 [filter \"mark\"]\n\tsmudge = sh -c 'touch smudged; cat'\n\tclean = cat\n",
                forge.display()
            ),
        )
        .unwrap();
        World { base, home, forge }
    }

    fn git(&self, dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("HOME", &self.home)
            .envs(IDENTITY)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// `acme/widget` on the stand-in forge, carrying the editor settings a sandboxed chat may
    /// not write: `.vscode/settings.json` and `.claude/settings.json`.
    fn widget(&self) {
        let src = self.forge.join("widget-src");
        std::fs::create_dir_all(src.join(".vscode")).unwrap();
        std::fs::create_dir_all(src.join(".claude")).unwrap();
        self.git(&src, &["init", "-q", "-b", "main", "."]);
        std::fs::write(src.join("README.md"), "hello\n").unwrap();
        std::fs::write(src.join(".vscode/settings.json"), "{}\n").unwrap();
        std::fs::write(src.join(".claude/settings.json"), "{}\n").unwrap();
        std::fs::write(src.join(".gitattributes"), "README.md filter=mark\n").unwrap();
        self.git(&src, &["add", "-A"]);
        self.git(&src, &["commit", "-q", "-m", "one"]);
        let bare = self.forge.join("acme/widget.git");
        self.git(
            &self.forge,
            &[
                "clone",
                "-q",
                "--bare",
                &src.display().to_string(),
                &bare.display().to_string(),
            ],
        );
    }

    /// A plane with workspace `alpha`, GitHub's `acme` as its forge, the sandbox on with
    /// `egress`, and `widget` in its inventory.
    fn plane(&self, name: &str, egress: &str) -> PathBuf {
        let root = self.base.join(name);
        std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
        std::fs::create_dir_all(root.join("inventory")).unwrap();
        std::fs::write(
            root.join("charter.toml"),
            format!(
                "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n\n\
                 [sandbox]\nmode = \"on\"\negress = {egress}\n"
            ),
        )
        .unwrap();
        std::fs::write(
            root.join("inventory/repos.json"),
            serde_json::to_string_pretty(&json!({"group": "acme", "count": 1, "repos": [{
                "name": "widget",
                "path_with_namespace": "acme/widget",
                "ssh_url": "git@github.com:acme/widget.git",
                "default_branch": "main",
                "kind": "app",
                "stack": "unknown",
                "description": "",
                "topics": [],
                "web_url": "https://github.com/acme/widget",
                "forge": "github"
            }]}))
            .unwrap(),
        )
        .unwrap();
        root
    }

    /// `purlis <args>` in `root`, with `chat` the env of a chat the app started, or none.
    fn purlis(&self, root: &Path, args: &[&str], chat: &[(&str, &str)]) -> Output {
        Command::new(purlis())
            .args(args)
            .current_dir(root)
            .env_clear()
            .env("CHARTER_ROOT", root)
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .env("USER", "tester")
            .envs(chat.iter().copied())
            .output()
            .expect("the binary runs")
    }
}

/// An app listening at `<base>/app/hooks.sock` whose answer to every git ask is `answer`'s,
/// with chat 3's token, and every ask it was sent.
fn an_app(
    base: &Path,
    answer: impl Fn(Ask) -> Answer + Send + Sync + 'static,
) -> (Reading, ChatToken, PathBuf, mpsc::Receiver<Ask>) {
    let socket = base.join("app").join("hooks.sock");
    let listener = Listener::bind(base, &socket).expect("a socket");
    let token = listener.tokens().issue_to_this_process(3).expect("a token");
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let reading = listener.each_answering(
        Box::new(|_| {}),
        Box::new(move |_, ask| {
            tx.lock().unwrap().send(ask.clone()).unwrap();
            answer(ask)
        }),
    );
    (reading, token, socket, rx)
}

/// The environment of chat 3, as the app starts it.
fn chat_3<'a>(socket: &'a Path, token: &'a ChatToken) -> [(&'a str, &'a str); 3] {
    [
        (SOCKET_ENV, socket.to_str().unwrap()),
        (CHAT_ENV, "3"),
        (TOKEN_ENV, token.expose()),
    ]
}

/// The core's broker, for chat 3 standing at `root`'s top, sandboxed, as persona `steward`.
///
/// Its git reads no global config, so the stand-in forge's rewrite is handed to it on top of
/// the identity, which is all the app hands it.
fn the_broker(root: PathBuf, forge: &Path) -> impl Fn(Ask) -> Answer + Send + Sync + 'static {
    let isolation = git::Isolated::identity(Some("Tester"), Some("t@e.invalid")).also(
        &format!("url.file://{}/acme/.insteadOf", forge.display()),
        "https://github.com/acme/",
    );
    move |ask| match ask {
        Ask::Git(asked) => gitbroker::answer(
            &root,
            &Asker {
                chat: 3,
                cwd: Some(root.clone()),
                persona: Some("steward".to_owned()),
                harnessed: true,
                unsandboxed: false,
            },
            &asked,
            &isolation,
            chrono::Utc::now(),
        ),
        _ => Answer::No {
            why: hookwire::NOTHING_ANSWERS.to_owned(),
        },
    }
}

fn err(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// What `out` said, with `root` written as `<root>` so two planes' runs can be compared.
fn said(out: &Output, root: &Path) -> String {
    err(out).replace(&root.display().to_string(), "<root>")
}

const IN_THE_CHILD: &str = "BROKERED_GIT_CHILD";

#[test]
fn a_sandboxed_chats_clone_and_worktree_are_made_by_the_app_and_say_what_the_terminal_says() {
    purlis_core::unsteered!();
    let Some(base) = std::env::var_os(IN_THE_CHILD) else {
        let tmp = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(tmp.path()).unwrap().join("home");
        purlis_core::testrun::rerun(
            &[
                "a_sandboxed_chats_clone_and_worktree_are_made_by_the_app_and_say_what_the_terminal_says",
            ],
            &[
                (IN_THE_CHILD, tmp.path().as_os_str()),
                ("HOME", home.as_os_str()),
            ],
        );
        return;
    };
    let world = World::at(Path::new(&base));
    world.widget();
    let terminal = world.plane("terminal", "[\"forge\"]");
    let brokered = world.plane("brokered", "[\"forge\"]");
    let (_reading, token, socket, asked) =
        an_app(&world.base, the_broker(brokered.clone(), &world.forge));
    let chat = chat_3(&socket, &token);

    // The clone: in a chat, the app makes it.
    let here = world.purlis(&terminal, &["clone", "widget", "-w", "alpha"], &[]);
    let there = world.purlis(&brokered, &["clone", "widget", "-w", "alpha"], &chat);
    assert!(there.status.success(), "{}", err(&there));
    assert_eq!(there.status.code(), here.status.code());
    assert_eq!(said(&there, &brokered), said(&here, &terminal));
    let Ok(Ask::Git(git)) = asked.recv_timeout(Duration::from_secs(5)) else {
        panic!("the app was not asked to clone");
    };
    assert_eq!(git.chat, 3);
    assert_eq!(git.workspace, "alpha");
    assert_eq!(
        git.work,
        GitWork::Clone {
            repos: vec!["widget".to_owned()]
        }
    );
    let clone = brokered.join("workspaces/alpha/widget");
    for written in [
        ".git/config",
        ".vscode/settings.json",
        ".claude/settings.json",
    ] {
        assert!(
            clone.join(written).is_file(),
            "{written} is not in the clone"
        );
    }
    // The operator's global filter ran on the terminal's clone, and not on the app's.
    assert!(terminal.join("workspaces/alpha/widget/smudged").exists());
    assert!(
        !clone.join("smudged").exists(),
        "a global-config filter ran on a clone made for a chat"
    );

    // The worktree: the same.
    let args = ["worktree", "add", "widget", "p1", "-w", "alpha"];
    let here = world.purlis(&terminal, &args, &[]);
    let there = world.purlis(&brokered, &args, &chat);
    assert!(there.status.success(), "{}", err(&there));
    assert_eq!(said(&there, &brokered), said(&here, &terminal));
    let Ok(Ask::Git(git)) = asked.recv_timeout(Duration::from_secs(5)) else {
        panic!("the app was not asked to cut a worktree");
    };
    assert_eq!(
        git.work,
        GitWork::WorktreeAdd {
            repo: "widget".to_owned(),
            piece: "p1".to_owned(),
            branch: None,
        }
    );
    let piece = brokered.join("workspaces/alpha/.worktrees/widget/p1");
    assert!(piece.join(".vscode/settings.json").is_file(), "no piece");
    assert!(piece.join(".claude/settings.json").is_file(), "no piece");

    // Credited to the chat and its persona, by the app's record.
    let log = std::fs::read_dir(brokered.join("workspaces/alpha/pieces"))
        .expect("a piece log")
        .filter_map(Result::ok)
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .collect::<String>();
    let claimed: serde_json::Value =
        serde_json::from_str(log.lines().last().expect("a line")).expect("json");
    assert_eq!(claimed["event"], "claimed");
    assert_eq!(claimed["session"], "3");
    assert_eq!(claimed["persona"], "steward");
}

#[test]
fn a_host_outside_the_chats_internet_access_is_refused_and_never_cloned() {
    purlis_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let world = World::at(tmp.path());
    // No preset: the chat reaches no host, so the forge's is refused before git runs. The
    // broker's git would read the suite's HOME, which is why nothing here may reach it.
    let root = world.plane("closed", "[]");
    let (_reading, token, socket, _asked) =
        an_app(&world.base, the_broker(root.clone(), &world.forge));

    let ran = world.purlis(
        &root,
        &["clone", "widget", "-w", "alpha"],
        &chat_3(&socket, &token),
    );

    assert_eq!(ran.status.code(), Some(1), "{}", err(&ran));
    assert!(err(&ran).contains("'github.com'"), "{}", err(&ran));
    assert!(err(&ran).contains("sandbox"), "{}", err(&ran));
    assert!(!root.join("workspaces/alpha/widget").exists());
}

#[test]
fn a_git_action_the_app_refuses_is_refused_here_and_never_run_here() {
    purlis_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let world = World::at(tmp.path());
    world.widget();
    let root = world.plane("refused", "[\"forge\"]");
    let (_reading, token, socket, _asked) = an_app(&world.base, |_| Answer::No {
        why: "this chat works in workspace 'beta'".to_owned(),
    });

    for args in [
        &["clone", "widget", "-w", "alpha"][..],
        &["worktree", "add", "widget", "p1", "-w", "alpha"][..],
    ] {
        let ran = world.purlis(&root, args, &chat_3(&socket, &token));
        assert_eq!(ran.status.code(), Some(1), "{args:?}: {}", err(&ran));
        assert!(
            err(&ran).contains("the app did not run this for the chat: this chat works in"),
            "{}",
            err(&ran)
        );
    }
    assert!(!root.join("workspaces/alpha/widget").exists());
}

#[test]
fn where_no_app_takes_the_ask_the_command_runs_here() {
    // An app with nothing that answers asks, and no app at all: the clone is this process's,
    // as it always was. Its git reads the HOME this test hands the binary.
    purlis_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let world = World::at(tmp.path());
    world.widget();
    let root = world.plane("unanswered", "[\"forge\"]");
    let taken = Arc::new(Mutex::new(0));
    let count = Arc::clone(&taken);
    let (_reading, token, socket, _asked) = an_app(&world.base, move |_| {
        *count.lock().unwrap() += 1;
        Answer::No {
            why: hookwire::NOTHING_ANSWERS.to_owned(),
        }
    });

    let ran = world.purlis(
        &root,
        &["clone", "widget", "-w", "alpha"],
        &chat_3(&socket, &token),
    );

    assert!(ran.status.success(), "{}", err(&ran));
    assert_eq!(*taken.lock().unwrap(), 1, "the app was not asked first");
    assert!(root.join("workspaces/alpha/widget/.git").exists());

    let alone = world.plane("alone", "[\"forge\"]");
    let ran = world.purlis(&alone, &["clone", "widget", "-w", "alpha"], &[]);
    assert!(ran.status.success(), "{}", err(&ran));
    assert!(alone.join("workspaces/alpha/widget/.git").exists());
}
