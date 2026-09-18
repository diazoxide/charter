//! A profile whose wiring charter cannot find does not start a chat.
//!
//! Measured on claude 2.1.268/2.1.276, codex-cli 0.147.0 and opencode 1.18.23, in throwaway
//! folders: **a harness pointed at another config folder loads none of charter's wiring.**
//! Claude Code lists no charter plugin — not even "enabled but not installed" — because the
//! `charter` marketplace is known only to the default config folder. A chat in that state
//! looks guarded and is not, which is the same failure whichever profile started it.
//!
//! The stand-in `claude` here answers `plugin list --json` with a fixture and writes down
//! what it was asked, which is how these tests drive an answer a real binary would take a
//! network clone to produce. What it CANNOT prove is what the real binary says; that is
//! measured by hand and recorded in `wiring.rs`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use charter_core::profiles::{self, Profile};
use charter_core::wiring::{self, State};

/// A plane declaring one profile whose command is a stand-in `claude`.
///
/// Built plane-first, so a fixture can name the plane's own directories — which is the
/// whole question `covers` asks. What the stand-in was given is written to a file whose
/// path is baked into the script: an environment variable would need `set_var`, and
/// `unsafe` is forbidden here.
struct Stand {
    dir: tempfile::TempDir,
}

impl Stand {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("charter.toml"), "").unwrap();
        let stand = Self { dir };
        stand.answers("[]");
        stand.declares("/tmp/does-not-matter");
        stand
    }

    /// What the stand-in answers `plugin list --json` with.
    fn answers(&self, json: &str) -> &Self {
        self.script(&format!(
            "#!/bin/sh\n\
             printf '%s\\n' \"$PWD\" \"$CLAUDE_CONFIG_DIR\" \"$*\" >> {:?}\n\
             cat <<'JSON'\n{json}\nJSON\n",
            self.asked().display().to_string()
        ))
    }

    fn script(&self, body: &str) -> &Self {
        let bin = self.bin();
        fs::write(&bin, body).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        self
    }

    /// The profile, pointing at the stand-in, with `config_dir` as its config folder.
    fn declares(&self, config_dir: &str) -> &Self {
        fs::write(
            self.root().join(profiles::LOCAL_FILE),
            format!(
                "[harness.work]\nkind = \"claude\"\ncommand = [{:?}]\n\
                 env = {{ CLAUDE_CONFIG_DIR = {:?} }}\n",
                self.bin().display().to_string(),
                config_dir
            ),
        )
        .unwrap();
        self
    }

    fn bin(&self) -> PathBuf {
        self.root().join("claude-stand-in")
    }

    fn asked(&self) -> PathBuf {
        self.root().join("asked.txt")
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// The profile, already approved — the trust gate is not what most of these are about.
    fn profile(&self) -> Profile {
        let p = profiles::current(self.root()).get("work").unwrap().clone();
        charter_core::profiletrust::record_launched(
            self.root(),
            &p.name,
            &charter_core::profiletrust::fingerprint(&p),
        )
        .unwrap();
        p
    }

    /// The workspace directory a chat in this plane would start in.
    fn cwd(&self) -> PathBuf {
        let cwd = self.root().join("workspaces").join("ide");
        fs::create_dir_all(&cwd).unwrap();
        cwd
    }

    /// One entry of `claude plugin list --json`, naming a directory of this plane.
    fn entry(&self, scope: &str, enabled: bool, project: Option<&Path>) -> String {
        let project = match project {
            Some(path) => format!(",\"projectPath\":{:?}", path.display().to_string()),
            None => String::new(),
        };
        format!(r#"[{{"id":"charter@charter","scope":"{scope}","enabled":{enabled}{project}}}]"#)
    }
}

#[test]
fn a_config_folder_with_no_charter_plugin_is_not_wired_and_the_fix_is_the_install() {
    let stand = Stand::new();
    let p = stand.profile();

    let w = wiring::detect(&p, &stand.cwd(), stand.root());

    assert_eq!(w.state, State::Unwired);
    assert!(
        w.detail.starts_with("charter@charter is not installed in"),
        "{w:?}"
    );
    assert_eq!(w.fix, "charter harness install work");
    assert!(
        wiring::would_install(&p, &w),
        "a definite unwired whose fix IS the install is what a launch installs over"
    );
}

#[test]
fn an_enabled_install_bound_to_another_checkout_does_not_answer_for_this_plane() {
    // `project` and `local` scope are bound to the directory they were installed from, so
    // somebody else's checkout is not an answer here — reading it as one is how "already
    // installed" gets printed over a plane with no plugin at all.
    let stand = Stand::new();
    let elsewhere = tempfile::tempdir().unwrap();
    stand.answers(&stand.entry("project", true, Some(elsewhere.path())));
    let p = stand.profile();

    let w = wiring::detect(&p, &stand.cwd(), stand.root());

    assert_eq!(w.state, State::Unwired, "{w:?}");
}

#[test]
fn an_enabled_install_bound_to_the_chats_own_directory_is_wired() {
    let stand = Stand::new();
    let cwd = stand.cwd();
    stand.answers(&stand.entry("project", true, Some(&cwd)));
    let p = stand.profile();

    assert_eq!(wiring::detect(&p, &cwd, stand.root()).state, State::Wired);
}

#[test]
fn a_user_scope_install_is_machine_wide_and_covers_every_directory() {
    let stand = Stand::new();
    stand.answers(&stand.entry("user", true, None));
    let p = stand.profile();

    let w = wiring::detect(&p, &stand.cwd(), stand.root());

    assert_eq!(w.state, State::Wired, "{w:?}");
    assert_eq!(w.fix, "", "a wired profile has nothing to fix");
}

#[test]
fn an_install_that_covers_the_plane_covers_a_chat_in_that_planes_workspace() {
    // `charter init` installs at project scope for the PLANE root, and a chat stands in
    // `workspaces/<ws>/` — a different projectPath, which coverage alone reads as somebody
    // else's checkout. It is safe to count only because `enabled` on that record is what
    // the binary resolved at the CHAT's directory, so a plane install the chat's own
    // directory disables still reads as disabled.
    let stand = Stand::new();
    let root = stand.root().to_path_buf();
    stand.answers(&stand.entry("project", true, Some(&root)));
    let p = stand.profile();

    assert_eq!(
        wiring::detect(&p, &stand.cwd(), stand.root()).state,
        State::Wired
    );
}

#[test]
fn a_plane_install_the_chats_directory_disables_is_not_wired() {
    let stand = Stand::new();
    let root = stand.root().to_path_buf();
    stand.answers(&stand.entry("project", false, Some(&root)));
    let p = stand.profile();

    assert_eq!(
        wiring::detect(&p, &stand.cwd(), stand.root()).state,
        State::Unwired
    );
}

#[test]
fn a_plugin_that_is_installed_and_disabled_is_refused_with_the_fix_that_changes_something() {
    // NOT `charter harness install`: the install answers `present` for an install that
    // exists, so pointing back at it would print a fix that changes nothing and loops.
    let stand = Stand::new();
    stand.answers(&stand.entry("user", false, None));
    let p = stand.profile();

    let w = wiring::detect(&p, &stand.cwd(), stand.root());

    assert_eq!(w.state, State::Unwired);
    assert!(
        w.detail
            .contains("is installed at user scope and reads as disabled in"),
        "{w:?}"
    );
    assert!(
        w.fix
            .contains("plugin enable charter@charter --scope local"),
        "{w:?}"
    );
    assert!(
        !wiring::would_install(&p, &w),
        "a launch would have installed over a plugin that is already there"
    );
}

#[test]
fn a_probe_that_will_not_answer_is_unknown_and_never_installed_over() {
    // ADR 0009, ruling 12: "charter could not look" is not "charter looked and the guard is
    // absent", and installing over an unknown state is how a second copy appears.
    let stand = Stand::new();
    stand.script("#!/bin/sh\nexit 3\n");
    let p = stand.profile();

    let w = wiring::detect(&p, &stand.cwd(), stand.root());

    assert_eq!(w.state, State::Unknown, "{w:?}");
    assert!(!wiring::would_install(&p, &w));
}

#[test]
fn an_answer_that_is_not_a_list_of_rows_is_unknown_rather_than_nothing_installed() {
    // "could not read" and "`--json` answered something that is not a list of rows" are the
    // same answer — nothing is known — and neither is "there is nothing installed".
    let stand = Stand::new();
    stand.answers("{\"plugins\": []}");
    let p = stand.profile();

    assert_eq!(
        wiring::detect(&p, &stand.cwd(), stand.root()).state,
        State::Unknown
    );
}

#[test]
fn a_probe_runs_the_profiles_own_command_under_the_profiles_own_environment() {
    // A probe that asked under a different environment than the launch would use is asking
    // about a session nobody is about to start.
    let stand = Stand::new();
    stand.declares("~/.claude-work");
    let p = stand.profile();
    let cwd = stand.cwd();

    wiring::detect(&p, &cwd, stand.root());

    let said = fs::read_to_string(stand.asked()).unwrap();
    let mut lines = said.lines();
    assert_eq!(
        fs::canonicalize(lines.next().unwrap()).unwrap(),
        fs::canonicalize(&cwd).unwrap(),
        "the probe did not ask from the directory the chat would start in"
    );
    let home = std::env::var("HOME").unwrap();
    assert_eq!(
        lines.next().unwrap(),
        format!("{home}/.claude-work"),
        "the probe did not carry the profile's own environment, with `~` expanded"
    );
    assert_eq!(lines.next().unwrap(), "plugin list --json");
}

#[test]
fn a_profile_charter_may_not_run_a_command_for_is_never_probed_at_all() {
    // The gate a launch makes before its own, in the same order: a probe IS a run of the
    // profile's command. An unapproved profile is told why, and nothing is asked or written.
    let stand = Stand::new();
    let p = profiles::current(stand.root()).get("work").unwrap().clone();

    let answer = wiring::wired_or_refusal(&p, &stand.cwd(), stand.root());

    assert!(
        answer.refusal.contains("profile 'work' is new"),
        "an unapproved profile was probed: {answer:?}"
    );
    assert_eq!(answer.wired, "");
}

/// Against the REAL `claude`, which the stand-ins above cannot speak for.
///
/// Ignored by default: it needs `claude` on the PATH and, for the wired half, a config
/// folder somebody has already wired — the first wiring of one is a git clone over the
/// network, measured at 7–20 s, which is not a thing a unit test should do on every run.
/// Run it with `cargo test -p charter-core -- --ignored` and the two folders named below.
///
/// `CHARTER_REAL_CLAUDE_WIRED` is a `CLAUDE_CONFIG_DIR` holding charter's plugin installed
/// at project scope for `CHARTER_REAL_CLAUDE_PROJECT`; `CHARTER_REAL_CLAUDE_EMPTY` is a
/// folder nobody has wired.
#[test]
#[ignore = "needs a real `claude` and two prepared config folders"]
fn the_real_claude_answers_what_this_port_expects() {
    let Ok(empty) = std::env::var("CHARTER_REAL_CLAUDE_EMPTY") else {
        panic!(
            "set CHARTER_REAL_CLAUDE_EMPTY, CHARTER_REAL_CLAUDE_WIRED and CHARTER_REAL_CLAUDE_PROJECT"
        );
    };
    let wired = std::env::var("CHARTER_REAL_CLAUDE_WIRED").unwrap();
    let project = PathBuf::from(std::env::var("CHARTER_REAL_CLAUDE_PROJECT").unwrap());

    let stand = Stand::new();
    stand.script("#!/bin/sh\nexec claude \"$@\"\n");

    stand.declares(&empty);
    let w = wiring::detect(&stand.profile(), &project, stand.root());
    assert_eq!(w.state, State::Unwired, "an unwired folder: {w:?}");

    stand.declares(&wired);
    let w = wiring::detect(&stand.profile(), &project, stand.root());
    assert_eq!(
        w.state,
        State::Wired,
        "the wired folder, asked from its own project: {w:?}"
    );

    // The measurement this port turns on: `enabled` is resolved at the probe's own working
    // directory, so the same install reads as disabled from a directory beside it.
    let beside = project.parent().expect("the project has a parent");
    let w = wiring::detect(&stand.profile(), beside, stand.root());
    assert_eq!(
        w.state,
        State::Unwired,
        "the same install, asked from a directory it does not cover: {w:?}"
    );
}

#[test]
fn a_kind_this_app_does_not_start_is_refused_by_that_name_and_not_by_a_missing_probe() {
    // Parsing is not launching. `profiles` knows all three kinds, including opencode,
    // because the Python charter accepts one and this plane is read by both until M4 — an
    // operator must not get two answers about their own file. What this app will not do is
    // START one, and that decision is the spec's (decision 6), so it is said here in the
    // launch path rather than left to fall out of a wiring check that happens not to exist.
    //
    // Said HERE and not inferred: a declared `kind` is a known value. `Harness::of_command`
    // cannot tell a shell from a harness charter has not measured, and no refusal is built
    // on that.
    let stand = Stand::new();
    fs::write(
        stand.root().join(profiles::LOCAL_FILE),
        "[harness.work]\nkind = \"opencode\"\ncommand = [\"opencode\"]\n",
    )
    .unwrap();
    let p = stand.profile();

    let answer = wiring::wired_or_refusal(&p, &stand.cwd(), stand.root());

    assert!(!answer.may_start(), "an opencode profile started a chat");
    assert_eq!(
        answer.refusal,
        "profile 'work' runs opencode, which this app does not start — charter-app v1 \
         starts Claude Code and Codex, and opencode follows. The Python charter on this \
         same plane still starts it: charter work."
    );
    assert_eq!(
        answer.wired, "",
        "nothing was installed for a chat that cannot start"
    );
}

#[test]
fn the_kind_refusal_comes_before_every_gate_that_would_run_or_ask_anything() {
    // A profile that can never start is not worth approving, probing or installing for, and
    // the sentence an operator wants is the one about v1 rather than one about consent.
    let stand = Stand::new();
    fs::write(
        stand.root().join(profiles::LOCAL_FILE),
        "[harness.work]\nkind = \"opencode\"\ncommand = [\"opencode\"]\n",
    )
    .unwrap();
    // Deliberately NOT approved, which is the gate that would otherwise answer first.
    let p = profiles::current(stand.root()).get("work").unwrap().clone();

    let answer = wiring::wired_or_refusal(&p, &stand.cwd(), stand.root());

    assert!(
        answer.refusal.contains("which this app does not start"),
        "the approval gate answered for a profile that can never start: {answer:?}"
    );
}

#[test]
fn both_kinds_this_app_does_start_are_taken_by_their_declared_word() {
    for kind in ["claude", "codex"] {
        assert!(
            charter_core::harness::Harness::of_kind(kind).is_some(),
            "{kind} is a v1 harness and was not recognised by its declared word"
        );
    }
    assert_eq!(charter_core::harness::Harness::of_kind("opencode"), None);
}

#[test]
fn detect_says_a_kind_is_not_startable_rather_than_reporting_a_probe_it_never_ran() {
    // The other half of the same rule. `wired_or_refusal` refuses before it asks anything,
    // but `detect` is public and `doctor`-shaped callers reach it directly — and its answer
    // for opencode used to be "charter has no wiring check for opencode", which reads as
    // *charter could not look* when the truth is *this app does not do that yet*.
    //
    // Without this test a mutation INSIDE `not_startable` leaves the detect path unproven:
    // the refusal is one function with two callers, and only one of them was covered.
    let stand = Stand::new();
    fs::write(
        stand.root().join(profiles::LOCAL_FILE),
        "[harness.work]\nkind = \"opencode\"\ncommand = [\"opencode\"]\n",
    )
    .unwrap();

    let w = wiring::detect(&stand.profile(), &stand.cwd(), stand.root());

    assert_eq!(
        w.state,
        State::Unknown,
        "a kind this app cannot start is not a pass"
    );
    assert!(
        w.detail.contains(
            "which this app does not start — charter-app v1 starts Claude Code \
                           and Codex"
        ),
        "detect reported a missing probe instead of the v1 decision: {w:?}"
    );
    assert!(
        !w.detail.contains("charter has no wiring check"),
        "the missing-probe sentence came back: {w:?}"
    );
}
