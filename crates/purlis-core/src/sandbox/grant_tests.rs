//! What a grant may name (#1342): a host by the project's own hosts' rules, a folder outside
//! every denial class, and never the classes themselves, which get the way that works.

use std::path::PathBuf;

use super::*;
use crate::sandbox::{Access, Class, Denial};

/// A machine on disk: a project, a home, a folder you listed in Settings, the chat's own temp
/// folder, and what `PATH` and the harnesses hold — every path as the kernel names it.
struct Disk {
    _dir: tempfile::TempDir,
    at: PathBuf,
    root: PathBuf,
    chat: PathBuf,
    home: PathBuf,
    denied: Vec<Denial>,
    allowed: Vec<PathBuf>,
    refused: Vec<PathBuf>,
}

impl Disk {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a folder");
        let at = dir.path().canonicalize().expect("real");
        for folder in [
            "plane/workspaces/alpha/repo",
            "plane/workspaces/beta",
            "plane/.purlis/app",
            "home/Library/LaunchAgents",
            "home/.config/git",
            "home/.config/purlis/vaults",
            "home/.cache/pip",
            "home/.local/bin",
            "opt/tools/homebrew/bin",
            "tmp/chat",
        ] {
            std::fs::create_dir_all(at.join(folder)).expect("made");
        }
        let denial = |class, path: &str| Denial {
            class,
            path: at.join(path),
            access: Access::Write,
            named: None,
        };
        Self {
            denied: vec![
                denial(Class::Vaults, "home/.config/purlis/vaults"),
                denial(Class::HumanPowers, "home/.config/purlis/purlisd"),
                denial(Class::Integrity, "plane/.purlis/app"),
            ],
            allowed: vec![at.join("opt/tools")],
            refused: vec![
                at.join("home/.local/bin"),
                at.join("opt/tools/homebrew/bin"),
                at.join("home/.claude"),
            ],
            root: at.join("plane"),
            chat: at.join("plane/workspaces/alpha/repo"),
            home: at.join("home"),
            _dir: dir,
            at,
        }
    }

    fn path(&self, below: &str) -> String {
        self.at.join(below).display().to_string()
    }

    fn place(&self) -> Place<'_> {
        Place {
            root: &self.root,
            chat: &self.chat,
            home: Some(&self.home),
            denied: &self.denied,
            allowed: &self.allowed,
            // No harness temp root: the fixture itself lives in one on some machines, and the
            // rule has its own test.
            harness_temp: &[],
            refused: &self.refused,
            folded: cfg!(target_os = "macos"),
        }
    }

    fn link(&self, at: &str, to: &str) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(self.at.join(to), self.at.join(at)).expect("linked");
    }
}

#[test]
fn a_folder_on_the_allowlist_is_granted_whole_clean_and_resolved() {
    let m = Disk::new();
    let here = m.place();
    for (typed, granted) in [
        // Another workspace of the same project.
        (
            "plane/workspaces/beta/sessions",
            "plane/workspaces/beta/sessions",
        ),
        // A folder you listed in Settings, and one inside it.
        ("opt/tools/cache/../x", "opt/tools/x"),
    ] {
        assert_eq!(
            write(&m.path(typed), &here),
            Ok(m.at.join(granted)),
            "{typed}"
        );
    }
}

#[test]
fn a_folder_off_the_allowlist_gets_no_allow_and_says_why() {
    // D-1342-10 and D-1342-11: the home folder, the caches every project shares, the system
    // temp folders and the machine are where later code is loaded from, outside any sandbox.
    let m = Disk::new();
    let here = m.place();
    for typed in [
        // `~/.config` holds git's own config (core.hooksPath, core.fsmonitor): never grantable.
        "home/.config",
        "home/.config/git",
        "home/Library/LaunchAgents",
        "home/.ssh",
        // A cache shared by every project is no longer offered (D-1342-11).
        "home/.cache/pip",
        "home/Library/Caches/go-build",
        // No temp folder is on the allowlist (D-1342-14).
        "tmp/chat",
        "tmp/chat/build",
        "tmp/other",
        // On PATH, inside one, or holding one; and a harness's own home.
        "opt/tools/homebrew/bin",
        "home/.local/bin/x",
        "opt/tools/homebrew",
        "home/.claude/hooks",
    ] {
        assert!(
            matches!(write(&m.path(typed), &here), Err(Refused::Outside(_))),
            "{typed}: {:?}",
            write(&m.path(typed), &here)
        );
    }
    // The system temp folders, and a harness's temp root in one: never granted (a test's own
    // project may sit inside one, which then refuses them as holding the project instead).
    for typed in [
        "/tmp",
        "/private/tmp",
        "/private/tmp/claude-1/x",
        "/private/var/folders",
    ] {
        assert!(write(typed, &here).is_err(), "{typed}");
    }
    // The sentence names no folder: the window shows the chat's path apart from purlis's words.
    let why = write(&m.path("home/.config"), &here)
        .unwrap_err()
        .to_string();
    assert!(
        why.starts_with("purlis will not let a chat write that folder"),
        "{why}"
    );
    assert!(!why.contains(&m.at.display().to_string()), "{why}");
    // A folder you list in Settings is judged by every refusal but the allowlist.
    assert_eq!(
        grantable(&m.path("home/.cache/pip"), &here),
        Ok(m.at.join("home/.cache/pip"))
    );
    assert!(matches!(
        grantable(&m.path("home/.local/bin"), &here),
        Err(Refused::Outside(_))
    ));
    assert!(matches!(
        grantable(&m.path("home"), &here),
        Err(Refused::Not(_))
    ));
}

#[test]
fn a_link_is_judged_as_where_it_points_and_only_that_is_granted() {
    // D-1342-12 (review R1): a chat makes a link inside what it may be granted, pointing out.
    let m = Disk::new();
    m.link(
        "plane/workspaces/alpha/repo/cache",
        "home/Library/LaunchAgents",
    );
    m.link("opt/tools/agents", "home/Library/LaunchAgents");
    let here = m.place();
    for typed in [
        "plane/workspaces/alpha/repo/cache",
        "plane/workspaces/alpha/repo/cache/sub",
        "opt/tools/agents",
    ] {
        assert!(
            matches!(write(&m.path(typed), &here), Err(Refused::Outside(_))),
            "{typed}: {:?}",
            write(&m.path(typed), &here)
        );
    }
    // A link to a folder that IS on the allowlist is granted as where it points, and only that.
    m.link("opt/tools/to-beta", "plane/workspaces/beta");
    assert_eq!(
        write(&m.path("opt/tools/to-beta"), &here),
        Ok(m.at.join("plane/workspaces/beta"))
    );
    // A folder granted, then swapped for a link, is dropped at the next start.
    let granted = write(&m.path("plane/workspaces/beta/out"), &here).expect("granted");
    assert_eq!(
        still_grantable(std::slice::from_ref(&granted), &here),
        std::slice::from_ref(&granted)
    );
    m.link("plane/workspaces/beta/out", "home/Library/LaunchAgents");
    assert!(still_grantable(std::slice::from_ref(&granted), &here).is_empty());
}

#[test]
fn a_folder_you_listed_is_compared_as_listed_and_a_link_in_its_place_widens_nothing() {
    // Review R8: a folder listed inside what a chat may write is swapped for a link out. The
    // allowlist keeps the spelling it was listed under, and the machine's ground drops it.
    let m = Disk::new();
    let listed = m.at.join("plane/workspaces/beta/tools");
    std::fs::create_dir_all(&listed).expect("made");
    let allowed = vec![listed.clone()];
    let here = Place {
        allowed: &allowed,
        ..m.place()
    };
    assert!(still_itself(&listed));
    std::fs::remove_dir(&listed).expect("removed");
    m.link("plane/workspaces/beta/tools", "home/Library/LaunchAgents");
    assert!(!still_itself(&listed), "it resolves elsewhere now");
    // What a block names through it is judged where it really is: LaunchAgents, off the list.
    assert!(matches!(
        write(&m.path("plane/workspaces/beta/tools/x.plist"), &here),
        Err(Refused::Outside(_))
    ));
    assert!(matches!(
        write(&m.path("home/Library/LaunchAgents"), &here),
        Err(Refused::Outside(_))
    ));
}

#[test]
fn a_harness_temp_root_is_refused_wherever_the_project_is() {
    // Never weakened (round 4): a listed folder in a harness's temp root grants nothing in it.
    let m = Disk::new();
    let allowed = vec![std::path::PathBuf::from("/private/tmp/claude-9/x")];
    let with = Place {
        allowed: &allowed,
        harness_temp: &HARNESS_TEMP,
        ..m.place()
    };
    assert!(matches!(
        write("/private/tmp/claude-9/x/y", &with),
        Err(Refused::Outside(_))
    ));
    let without = Place {
        allowed: &allowed,
        ..m.place()
    };
    assert_eq!(
        write("/private/tmp/claude-9/x/y", &without),
        Ok(std::path::PathBuf::from("/private/tmp/claude-9/x/y"))
    );
}

#[cfg(target_os = "macos")]
#[test]
fn on_macos_a_case_variant_of_a_refused_folder_is_refused_before_it_exists() {
    // Review R4: on a case-insensitive volume `HOMEBREW/BIN` is the folder on `PATH`.
    let m = Disk::new();
    let here = m.place();
    assert!(matches!(
        write(&m.path("opt/tools/HOMEBREW/BIN/new"), &here),
        Err(Refused::Outside(_))
    ));
    assert!(matches!(
        write(&m.path("plane/workspaces/beta/repo/.GIT/hooks"), &here),
        Err(Refused::Never(_))
    ));
}

#[test]
fn the_machine_the_home_the_project_and_what_holds_them_are_never_a_grant() {
    let m = Disk::new();
    let here = m.place();
    for typed in [
        "/".to_owned(),
        m.path("home"),
        m.path("home/"),
        m.path(""),
        m.path("plane"),
        m.path("plane/workspaces"),
        m.path("plane/workspaces/alpha/repo"),
        m.path("plane/workspaces/alpha/repo/.."),
        "relative/path".to_owned(),
        "~/cache".to_owned(),
        m.path("opt/tools/a\nb"),
        m.path("opt/tools/a`b"),
    ] {
        assert!(
            matches!(write(&typed, &here), Err(Refused::Not(_))),
            "{typed:?} was granted: {:?}",
            write(&typed, &here)
        );
    }
}

#[test]
fn every_denial_class_is_refused_with_the_way_that_works() {
    let m = Disk::new();
    let here = m.place();
    let never = |typed: &str| match write(&m.path(typed), &here) {
        Err(Refused::Never(why)) => why,
        other => panic!("{typed}: {other:?}"),
    };
    assert!(never("home/.config/purlis/vaults/forge").contains("purlis secret exec"));
    assert!(never("plane/.purlis/app").contains("purlis session record"));
    // purlis's state folder under the other spelling, which this project does not have.
    assert!(never("plane/.charter/handbacks").contains("purlis session record"));
    assert!(never("home/.config/purlis/purlisd").contains("only a person approves"));
    // A name a later program loads code from, wherever it is.
    for planted in [
        "plane/workspaces/beta/repo/.git/hooks",
        "tmp/chat/repo/.vscode",
        "opt/tools/.zshrc",
        "opt/tools/other/charter.toml",
    ] {
        assert!(never(planted).contains("later program"), "{planted}");
    }
}

#[test]
fn a_grant_judged_again_at_the_start_drops_what_a_class_now_covers() {
    let m = Disk::new();
    let here = m.place();
    let kept = still_grantable(
        &[
            m.at.join("opt/tools/x"),
            m.at.join("opt/tools/x"),
            m.at.join("plane/.purlis/app/x"),
            m.at.join("home"),
            m.at.join("home/.config"),
        ],
        &here,
    );
    assert_eq!(kept, [m.at.join("opt/tools/x")]);
}

#[test]
fn settings_names_what_a_folder_you_list_holds_that_later_code_loads() {
    let m = Disk::new();
    let here = m.place();
    let held = holds_later_code(&m.at.join("home/Library"), &here);
    assert!(
        held.iter().any(|one| one.ends_with("Library/LaunchAgents")),
        "{held:?}"
    );
    assert!(holds_later_code(&m.at.join("home/.cache/pip"), &here).is_empty());
}

#[test]
fn a_host_grant_takes_the_project_hosts_rules() {
    assert_eq!(
        host(" API.example.com ").map(|h| h.to_string()),
        Ok("api.example.com".to_owned())
    );
    assert_eq!(
        host("10.100.39.145:6443").map(|h| h.to_string()),
        Ok("10.100.39.145:6443".to_owned())
    );
    for typed in [
        "localhost",
        "169.254.169.254",
        "https://x.example/y",
        "x",
        "",
    ] {
        assert!(matches!(host(typed), Err(Refused::Not(_))), "{typed}");
    }
}

#[test]
fn one_chats_grants_hold_each_thing_once() {
    let mut grants = Grants::default();
    assert!(grants.is_empty());
    let api = What::Host(host("api.example.com").unwrap());
    let cache = What::Write(PathBuf::from("/opt/cache"));
    for what in [&api, &cache, &api, &cache] {
        grants.add(what);
    }
    assert_eq!(grants.hosts.len(), 1);
    assert_eq!(grants.writes, [PathBuf::from("/opt/cache")]);
}

#[test]
fn a_write_block_proposes_the_folder_it_was_refused_in() {
    let dir = tempfile::tempdir().expect("a folder");
    let real = dir.path().canonicalize().unwrap();
    assert_eq!(proposed_folder(&real), real, "a folder is itself");
    assert_eq!(
        proposed_folder(&real.join("not-there/file.lock")),
        real.join("not-there")
    );
}

#[test]
fn the_chat_is_told_what_was_allowed_and_to_retry() {
    let told = told(&What::Host(host("api.example.com").unwrap()), Level::Chat);
    // The target in backticks: it is what the block named, inside purlis's own sentence.
    assert!(
        told.contains("reaching `api.example.com` for this chat"),
        "{told}"
    );
    let folder = super::told(&What::Write(PathBuf::from("/tmp/a b`c")), Level::You);
    assert!(
        folder.contains(
            "writing `/tmp/a bc` and everything in it for every chat of this project on their \
             machine"
        ),
        "{folder}"
    );
    assert!(told.contains("same conversation"), "{told}");
    assert!(told.contains("Retry"), "{told}");
    assert_eq!(Level::of_word("you"), Some(Level::You));
    assert_eq!(Level::of_word("everyone"), None);
}

// ---- what a start compiles in --------------------------------------------------------------

use crate::sandbox::{Compiled, Machine, Os, Plane, claude, codex, local, opencode};

const ON: &str = "[sandbox]\nmode = \"on\"\negress = [\"model-providers\"]\n";

/// A project folder with no manifest on disk (the policy is handed over as text), and a home
/// beside it.
struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
}

fn project() -> Project {
    let dir = tempfile::tempdir().expect("a folder");
    let real = dir.path().canonicalize().expect("real");
    let root = real.join("plane");
    let home = real.join("home");
    std::fs::create_dir_all(root.join("workspaces/alpha")).expect("project");
    std::fs::create_dir_all(&home).expect("home");
    Project {
        _dir: dir,
        root,
        home,
    }
}

fn compiled_with(project: &Project, os: Os, chat: &Grants) -> Compiled {
    // The fixture may sit in a harness's temp root on this machine: name none here.
    super::TEST_HARNESS_TEMP.with(|roots| roots.set(&[]));
    let plane = Plane::of(Some(ON));
    let policy = plane.said().policy.expect("on");
    Compiled::granted(
        &policy,
        &plane,
        &project.root,
        &Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(project.home.clone()),
            os,
        },
        chat,
    )
}

#[test]
fn a_chats_own_grants_reach_its_compiled_sandbox_and_no_other_start() {
    let project = project();
    let cache = project.root.join("workspaces/beta/out");
    let mut chat = Grants::default();
    chat.add(&What::Host(host("api.example.com").unwrap()));
    chat.add(&What::Write(cache.clone()));
    // One that a class covers is dropped as it is compiled, whoever granted it.
    chat.add(&What::Write(project.root.join(".purlis/app")));

    let granted = compiled_with(&project, Os::MacOs, &chat);
    assert!(granted.hosts.contains(&"api.example.com".to_owned()));
    assert_eq!(granted.writable, std::slice::from_ref(&cache));

    // Another chat of the same project, started with no grant of its own: none of it.
    let other = compiled_with(&project, Os::MacOs, &Grants::default());
    assert!(!other.hosts.contains(&"api.example.com".to_owned()));
    assert!(other.writable.is_empty());
}

#[test]
fn a_folder_you_granted_reaches_every_chat_here_until_it_is_revoked() {
    let project = project();
    let folder = project.home.join("tools/cache");
    // Off the allowlist until you list a folder that holds it (D-1342-10).
    local::grant_write(&project.root, &folder).expect("granted");
    let unlisted = compiled_with(&project, Os::MacOs, &Grants::default());
    assert!(unlisted.writable.is_empty(), "{:?}", unlisted.writable);
    std::fs::create_dir_all(project.home.join("tools")).expect("made");
    local::list_grantable(&project.root, &project.home.join("tools")).expect("listed");
    assert_eq!(
        local::granted_writes(&project.root),
        std::slice::from_ref(&folder)
    );
    let started = compiled_with(&project, Os::MacOs, &Grants::default());
    assert_eq!(started.writable, std::slice::from_ref(&folder));

    local::revoke_write(&project.root, &folder).expect("revoked");
    let after = compiled_with(&project, Os::MacOs, &Grants::default());
    assert!(after.writable.is_empty(), "{:?}", after.writable);
}

#[test]
fn claude_code_is_handed_a_granted_folder_under_its_denials_and_nothing_without_one() {
    let project = project();
    let none = claude::settings(&compiled_with(&project, Os::MacOs, &Grants::default()))
        .expect("compiles");
    assert!(
        none.sandbox["filesystem"].get("allowWrite").is_none(),
        "a chat with no grant is handed what it always was: {}",
        none.sandbox
    );

    let cache = project.root.join("workspaces/beta/out");
    let mut chat = Grants::default();
    chat.add(&What::Write(cache.clone()));
    let settings = claude::settings(&compiled_with(&project, Os::MacOs, &chat)).expect("compiles");
    assert_eq!(
        settings.sandbox["filesystem"]["allowWrite"],
        serde_json::json!([cache.display().to_string()])
    );
    // Its denials are all still there, and Claude Code holds a denial over an allow.
    assert!(
        settings.sandbox["filesystem"]["denyWrite"]
            .as_array()
            .is_some_and(|denied| !denied.is_empty())
    );
    assert_eq!(settings.sandbox["allowUnsandboxedCommands"], false);
    // Every later-code name is denied again under the granted folder, by absolute path: a
    // relative `**/<name>` is read against the chat's folder, not this one (review S2).
    let denied: Vec<&str> = settings.sandbox["filesystem"]["denyWrite"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|one| one.as_str())
        .collect();
    for name in [
        ".git/hooks",
        ".git/config",
        ".envrc",
        ".vscode",
        "charter.toml",
    ] {
        let glob = format!("{}/**/{name}", cache.display());
        assert!(
            denied.contains(&glob.as_str()),
            "{glob} is not denied: {denied:?}"
        );
        assert!(
            settings.deny.contains(&format!("Edit(/{glob})")),
            "the Edit tool is not held off {glob}"
        );
    }
}

#[test]
fn a_wrapped_chat_writes_a_granted_folder_with_every_denial_after_it() {
    let project = project();
    let cache = project.root.join("workspaces/beta/out");
    let mut chat = Grants::default();
    chat.add(&What::Write(cache.clone()));
    let compiled = compiled_with(&project, Os::MacOs, &chat);
    let cwd = project.root.join("workspaces/alpha");
    let tmp = project.home.join("tmp");
    let quoted = format!("(subpath \"{}\")", cache.display());

    let opencode = opencode::profile(
        &opencode::wrap(&compiled).expect("wraps"),
        &cwd,
        &tmp,
        4000,
        None,
    )
    .expect("a profile");
    let codex = codex::profile(
        &codex::wrap(&compiled).expect("wraps"),
        &cwd,
        &tmp,
        4000,
        None,
    )
    .expect("a profile");
    for profile in [opencode, codex] {
        let allowed = profile.find(&quoted).expect("the grant is written");
        let denial = profile
            .find("(deny file-write* (subpath")
            .expect("the classes are denied");
        assert!(allowed < denial, "a denial comes after the grant");
        // The later-code names are denied at any depth of the granted folder too.
        assert!(
            profile.contains(&format!(
                "(deny file-write* (regex \"^{}/(.*/)?\\\\.git/config(/.*)?$\"))",
                crate::sandbox::seatbelt::escaped(&cache.display().to_string())
                    .replace('\\', "\\\\")
            )),
            "{profile}"
        );
    }
}
