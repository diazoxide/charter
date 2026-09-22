//! Each alert on a plane built for it, and the rule the port adds: a reading that stopped says
//! so. Byte-for-byte agreement with charter is the `statusline-alerts-*` differential scenarios'
//! job; the rows asserted here are copied from what the pinned oracle printed for those same
//! planes, so a regression shows up in `cargo test` before it reaches the differential.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

const W: &str = "\x1b[33m";
const B: &str = "\x1b[31m";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = crate::forklock::output(
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid"),
    )
    .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A healthy plane: a front door that is there, two workspaces at the current layout.
fn plane(toml: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap().join("plane");
    std::fs::create_dir_all(root.join("personas/steward")).unwrap();
    std::fs::write(root.join("personas/steward/persona.md"), "# steward\n").unwrap();
    std::fs::write(root.join("charter.toml"), toml).unwrap();
    for ws in ["alpha", "beta"] {
        let wd = root.join("workspaces").join(ws);
        std::fs::create_dir_all(wd.join("memory")).unwrap();
        std::fs::create_dir_all(wd.join("refs")).unwrap();
        std::fs::write(wd.join("workspace.md"), "# ws\n").unwrap();
        std::fs::write(wd.join("workspace.json"), "{}\n").unwrap();
        std::fs::write(wd.join("memory/MEMORY.md"), "# memory\n").unwrap();
        std::fs::write(wd.join("refs/README.md"), "# refs\n").unwrap();
        std::fs::write(wd.join(".charter-structure"), "5\n").unwrap();
    }
    (dir, root)
}

const HEALTHY: &str = "schema = 1\n\n[persona]\ndefault = \"steward\"\n";

fn stale(root: &Path, ws: &str) {
    std::fs::write(
        root.join("workspaces").join(ws).join(".charter-structure"),
        "4\n",
    )
    .unwrap();
}

fn reading(root: &Path) -> Reading {
    read(&Asking {
        root,
        active: None,
        standing: root,
    })
}

fn lines(root: &Path) -> Vec<String> {
    reading(root)
        .alerts
        .iter()
        .map(|a| a.line(&Look::default()))
        .collect()
}

/// The plane root as a repository on `main`, one commit, clean, `.charter/` ignored.
fn repo(root: &Path) {
    std::fs::write(root.join(".gitignore"), ".charter/\n").unwrap();
    std::fs::write(root.join("notes.md"), "# notes\n").unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "the plane"]);
}

fn push_record(root: &Path, record: serde_json::Value) {
    let head = git(root, &["rev-parse", "HEAD"]);
    let mut record = record;
    record["head"] = serde_json::Value::String(head);
    std::fs::create_dir_all(root.join(".charter")).unwrap();
    std::fs::write(root.join(".charter/plane-push.json"), record.to_string()).unwrap();
}

const ROOT_REMEDY: &str = "\x1b[2m · work belongs in a workspace clone\x1b[0m";

#[test]
fn a_healthy_plane_has_no_alerts_and_can_say_so_with_a_number() {
    let (_held, root) = plane(HEALTHY);
    let got = reading(&root);
    assert_eq!(got.alerts, vec![]);
    assert_eq!(got.stopped, None);
    // Zero IS a number here — the reading finished, so "nothing" is a claim it can make.
    assert_eq!(got.count(), Some(0));
}

#[test]
fn a_pin_this_charter_does_not_meet_names_both_numbers_and_the_command() {
    let (_held, root) = plane(&format!("{HEALTHY}\n[charter]\nversion = \"0.44.0\"\n"));
    let running = crate::news::shipped_version();
    assert_ne!(
        running, "0.44.0",
        "this test is vacuous if the corpus reaches the pin"
    );
    assert_eq!(
        lines(&root),
        vec![format!(
            "{W}⚠\x1b[0m \x1b[2mcharter\x1b[0m {running} \x1b[2m→ pinned\x1b[0m 0.44.0\x1b[2m · \
             charter version\x1b[0m"
        )]
    );
}

#[test]
fn a_pin_this_charter_meets_is_not_drift() {
    let running = crate::news::shipped_version();
    let (_held, root) = plane(&format!(
        "{HEALTHY}\n[charter]\nversion = \"  {running} \"\n"
    ));
    // Stripped as charter strips it, so a pin padded by hand still meets.
    assert_eq!(lines(&root), Vec::<String>::new());
}

#[test]
fn a_pin_beside_the_dev_channel_is_its_own_row_and_not_drift() {
    let (_held, root) = plane(&format!(
        "{HEALTHY}\n[charter]\nversion = \"0.44.0\"\n\n[update]\nchannel = \"dev\"\n"
    ));
    assert_eq!(
        lines(&root),
        vec![format!(
            "{W}⚠\x1b[0m \x1b[2mcharter\x1b[0m 0.44.0 \x1b[2mpin + dev channel: two different \
             charters · charter version\x1b[0m"
        )]
    );
}

#[test]
fn a_channel_charter_does_not_know_is_stable_and_the_pin_is_ordinary_drift() {
    let (_held, root) = plane(&format!(
        "{HEALTHY}\n[charter]\nversion = \"0.44.0\"\n\n[update]\nchannel = \"DEV\"\n"
    ));
    assert!(matches!(
        reading(&root).alerts.as_slice(),
        [Alert::PinDrift { .. }]
    ));
}

#[test]
fn a_pin_that_is_not_a_string_is_no_pin() {
    let (_held, root) = plane(&format!("{HEALTHY}\n[charter]\nversion = 44\n"));
    assert_eq!(reading(&root), Reading::default());
}

#[test]
fn a_front_door_naming_no_persona_is_said_with_its_remedy() {
    let (_held, root) = plane("schema = 1\n\n[persona]\ndefault = \"ghost\"\n");
    assert_eq!(
        lines(&root),
        vec![format!(
            "{W}⚠\x1b[0m \x1b[2mfront door\x1b[0m ghost \x1b[2m— no such persona · charter \
             persona default <name>\x1b[0m"
        )]
    );
}

#[test]
fn a_front_door_that_is_not_a_string_is_quoted_back_as_charter_prints_it() {
    let (_held, root) = plane("schema = 1\n\n[persona]\ndefault = 7\n");
    assert_eq!(
        reading(&root).alerts,
        vec![Alert::FrontDoor {
            declared: "7".into()
        }]
    );
    let (_held, root) = plane("schema = 1\n\n[persona]\ndefault = false\n");
    assert_eq!(
        reading(&root).alerts,
        vec![Alert::FrontDoor {
            declared: "False".into()
        }]
    );
}

#[test]
fn a_blank_front_door_is_no_front_door() {
    let (_held, root) = plane("schema = 1\n\n[persona]\ndefault = \"   \"\n");
    assert_eq!(reading(&root), Reading::default());
}

#[test]
fn a_front_door_in_the_legacy_flat_layout_is_there() {
    let (_held, root) = plane("schema = 1\n\n[persona]\ndefault = \"flat\"\n");
    std::fs::write(root.join("personas/flat.md"), "# flat\n").unwrap();
    assert_eq!(reading(&root), Reading::default());
}

#[cfg(unix)]
#[test]
fn a_front_door_the_filesystem_will_not_answer_about_is_not_called_missing() {
    use std::os::unix::fs::PermissionsExt;
    let (_held, root) = plane("schema = 1\n\n[persona]\ndefault = \"ghost\"\n");
    let personas = root.join("personas");
    std::fs::set_permissions(&personas, std::fs::Permissions::from_mode(0o000)).unwrap();
    let got = reading(&root);
    std::fs::set_permissions(&personas, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Unknown is not missing: an alert made up from a look that failed is the false alarm.
    assert_eq!(got, Reading::default());
}

#[test]
fn another_workspace_behind_the_layout_is_counted_and_the_active_one_is_not() {
    let (_held, root) = plane(HEALTHY);
    stale(&root, "beta");
    assert_eq!(
        lines(&root),
        vec![format!(
            "{W}⚠\x1b[0m \x1b[2mreinit\x1b[0m 1 \x1b[2mws · charter ws reinit --all\x1b[0m"
        )]
    );
    // The active workspace's stale layout is the identity row's to flag.
    stale(&root, "alpha");
    let asked = |active| {
        read(&Asking {
            root: &root,
            active,
            standing: &root,
        })
        .alerts
    };
    assert_eq!(
        asked(Some("alpha")),
        vec![Alert::Reinit {
            stale: vec!["beta".into()]
        }]
    );
    // …and a surface that flags none counts them all.
    assert_eq!(
        asked(None),
        vec![Alert::Reinit {
            stale: vec!["alpha".into(), "beta".into()]
        }]
    );
}

#[test]
fn a_manifest_charter_cannot_read_stops_the_reading_before_the_first_row() {
    let (_held, root) = plane("schema = 1\n[[[ not toml\n");
    stale(&root, "beta");
    let got = reading(&root);
    assert_eq!(got.alerts, vec![]);
    assert!(
        got.stopped
            .as_deref()
            .is_some_and(|w| w.contains("not valid TOML")),
        "{got:?}"
    );
    // The number that must never be drawn: this plane has a stale workspace.
    assert_eq!(got.count(), None);
}

#[test]
fn a_raise_keeps_the_rows_before_it_and_drops_the_rows_after_it() {
    // `persona = "steward"` is truthy and not a table: charter's `.get` raises there, after the
    // pin row and before the reinit row.
    let (_held, root) =
        plane("schema = 1\npersona = \"steward\"\n\n[charter]\nversion = \"0.44.0\"\n");
    stale(&root, "beta");
    let got = reading(&root);
    assert!(
        matches!(got.alerts.as_slice(), [Alert::PinDrift { .. }]),
        "{got:?}"
    );
    assert!(got.stopped.is_some());
    assert_eq!(got.count(), None);
}

#[test]
fn a_falsy_section_is_an_empty_one_and_the_reading_goes_on() {
    let (_held, root) = plane("schema = 1\ncharter = false\npersona = \"\"\n");
    stale(&root, "beta");
    let got = reading(&root);
    assert_eq!(
        got.alerts,
        vec![Alert::Reinit {
            stale: vec!["beta".into()]
        }]
    );
    assert_eq!(got.stopped, None);
}

#[test]
fn a_plane_pinned_inside_another_planes_workspaces_says_where_memory_goes() {
    let dir = tempfile::tempdir().unwrap();
    let outer = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(outer.join("charter.toml"), "").unwrap();
    let inner = outer.join("workspaces/ide/charter");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("charter.toml"), "").unwrap();

    let standing_in = |standing: &Path| {
        read(&Asking {
            root: &inner,
            active: None,
            standing,
        })
        .alerts
    };
    let got = standing_in(&inner);
    assert_eq!(
        got,
        vec![Alert::NestedPlane {
            inner: inner.clone(),
            outer: outer.clone()
        }]
    );
    assert!(
        got[0]
            .line(&Look::default())
            .starts_with(&format!("{B}⚠\x1b[0m \x1b[2mnested plane"))
    );
    assert_eq!(got[0].severity(), Severity::Bad);
    // Standing anywhere but the inner plane is not the overridden hop.
    assert_eq!(standing_in(&outer), vec![]);
    // …and a plane nobody nested says nothing.
    assert_eq!(
        read(&Asking {
            root: &outer,
            active: None,
            standing: &outer,
        })
        .alerts,
        vec![]
    );
}

#[test]
fn a_clean_plane_root_on_its_default_branch_is_not_an_alert() {
    let (_held, root) = plane(HEALTHY);
    repo(&root);
    assert_eq!(reading(&root), Reading::default());
    // Untracked files are not the root being worked in: memory is local by default.
    std::fs::write(root.join("personas/steward/a-memory.md"), "# m\n").unwrap();
    assert_eq!(reading(&root), Reading::default());
}

#[test]
fn a_dirty_plane_root_is_one_row_naming_the_root() {
    let (_held, root) = plane(HEALTHY);
    repo(&root);
    std::fs::write(root.join("notes.md"), "# notes\n\nedited\n").unwrap();
    assert_eq!(
        lines(&root),
        vec![format!(
            "{W}⚠\x1b[0m \x1b[2mplane root\x1b[0m plane\x1b[2m · \x1b[0m{W}dirty\x1b[0m{ROOT_REMEDY}"
        )]
    );
}

#[test]
fn a_plane_root_off_its_default_branch_names_both() {
    let (_held, root) = plane(HEALTHY);
    repo(&root);
    git(&root, &["checkout", "-q", "-b", "feature/x"]);
    assert_eq!(
        lines(&root),
        vec![format!(
            "{W}⚠\x1b[0m \x1b[2mplane root\x1b[0m plane\x1b[2m · \x1b[0m\x1b[2mon\x1b[0m \
             feature/x\x1b[2m, not\x1b[0m main{ROOT_REMEDY}"
        )]
    );
}

#[test]
fn a_detached_plane_root_says_detached_and_not_a_short_sha() {
    let (_held, root) = plane(HEALTHY);
    repo(&root);
    git(&root, &["checkout", "-q", "--detach"]);
    assert_eq!(
        lines(&root),
        vec![format!(
            "{W}⚠\x1b[0m \x1b[2mplane root\x1b[0m plane\x1b[2m · \x1b[0m{W}detached \
             HEAD\x1b[0m{ROOT_REMEDY}"
        )]
    );
}

#[test]
fn origin_head_decides_the_default_and_a_packed_main_is_still_main() {
    let (_held, root) = plane(HEALTHY);
    repo(&root);
    git(&root, &["checkout", "-q", "-b", "trunk"]);
    // The repository's own answer wins outright: a plane living on `trunk` is not off `main`.
    std::fs::create_dir_all(root.join(".git/refs/remotes/origin")).unwrap();
    std::fs::write(
        root.join(".git/refs/remotes/origin/HEAD"),
        "ref: refs/remotes/origin/trunk\n",
    )
    .unwrap();
    assert_eq!(reading(&root), Reading::default());
    // With no remote answer, a `main` that exists only in `packed-refs` is the default.
    std::fs::remove_file(root.join(".git/refs/remotes/origin/HEAD")).unwrap();
    git(&root, &["pack-refs", "--all"]);
    assert!(!root.join(".git/refs/heads/main").exists());
    assert!(matches!(
        reading(&root).alerts.as_slice(),
        [Alert::PlaneRoot { off: Some(_), .. }]
    ));
}

#[test]
fn a_plane_root_with_no_default_to_be_off_says_nothing_about_its_branch() {
    let (_held, root) = plane(HEALTHY);
    repo(&root);
    git(&root, &["branch", "-m", "main", "work"]);
    assert_eq!(reading(&root), Reading::default());
}

#[test]
fn a_memory_commit_never_pushed_is_red_and_one_awaiting_a_pull_request_is_not() {
    let (_held, root) = plane(HEALTHY);
    repo(&root);
    push_record(
        &root,
        serde_json::json!({"outcome": "rejected", "branch": "main"}),
    );
    let got = reading(&root);
    assert_eq!(
        got.alerts[0].line(&Look::default()),
        format!(
            "{W}⚠\x1b[0m \x1b[2mplane root\x1b[0m plane\x1b[2m · \x1b[0m{B}memory commit not \
             pushed\x1b[0m{ROOT_REMEDY}"
        )
    );
    assert_eq!(got.alerts[0].severity(), Severity::Bad);

    push_record(
        &root,
        serde_json::json!({"outcome": "branched", "landed": "charter/1a2b3c4d", "branch": "main"}),
    );
    let got = reading(&root);
    assert_eq!(
        got.alerts[0].line(&Look::default()),
        format!(
            "{W}⚠\x1b[0m \x1b[2mplane root\x1b[0m plane\x1b[2m · \x1b[0m{W}memory awaiting a pull \
             request\x1b[0m{ROOT_REMEDY}"
        )
    );
    assert_eq!(got.alerts[0].severity(), Severity::Warn);
}

#[test]
fn every_finding_shares_one_row_in_charters_order() {
    let (_held, root) = plane(HEALTHY);
    repo(&root);
    git(&root, &["checkout", "-q", "-b", "side"]);
    std::fs::write(root.join("notes.md"), "# notes\n\nedited\n").unwrap();
    push_record(
        &root,
        serde_json::json!({"outcome": "rejected", "branch": "main"}),
    );
    assert_eq!(
        lines(&root),
        vec![format!(
            "{W}⚠\x1b[0m \x1b[2mplane root\x1b[0m plane\x1b[2m · \x1b[0m{W}dirty\x1b[0m\x1b[2m · \
             \x1b[0m\x1b[2mon\x1b[0m side\x1b[2m, not\x1b[0m main\x1b[2m · \x1b[0m{B}memory \
             commit not pushed\x1b[0m{ROOT_REMEDY}"
        )]
    );
}

#[test]
fn every_row_comes_in_charters_order() {
    let (_held, root) =
        plane("schema = 1\n\n[persona]\ndefault = \"ghost\"\n\n[charter]\nversion = \"0.44.0\"\n");
    stale(&root, "beta");
    repo(&root);
    std::fs::write(root.join("notes.md"), "# notes\n\nedited\n").unwrap();
    let got = reading(&root).alerts;
    assert!(
        matches!(
            got.as_slice(),
            [
                Alert::PinDrift { .. },
                Alert::FrontDoor { .. },
                Alert::Reinit { .. },
                Alert::PlaneRoot { .. }
            ]
        ),
        "{got:?}"
    );
}

#[test]
fn the_rows_follow_the_accents_the_plane_chose() {
    let (_held, root) = plane(&format!(
        "{HEALTHY}\n[frame]\nwarn = \"magenta\"\nbad = \"brightcyan\"\n"
    ));
    stale(&root, "beta");
    let look = Look::of(&root);
    let got = reading(&root).alerts;
    assert!(
        got[0].line(&look).starts_with("\x1b[35m⚠\x1b[0m"),
        "{:?}",
        got[0].line(&look)
    );
}

#[test]
fn the_drawer_gets_the_same_facts_as_words_with_no_escapes() {
    let alerts = [
        Alert::PinDrift {
            running: "0.62.1".into(),
            pinned: "0.44.0".into(),
        },
        Alert::FrontDoor {
            declared: "ghost".into(),
        },
        Alert::Reinit {
            stale: vec!["alpha".into(), "beta".into()],
        },
        Alert::PlaneRoot {
            name: "plane".into(),
            dirty: true,
            detached: false,
            off: Some(("side".into(), "main".into())),
            memory: Some(Memory::NotPushed),
        },
    ];
    let shown: Vec<Shown> = alerts.iter().map(Alert::shown).collect();
    for s in &shown {
        assert!(
            !s.detail.contains('\x1b') && !s.remedy.contains('\x1b'),
            "{s:?}"
        );
    }
    assert_eq!(shown[0].remedy, "charter version");
    assert_eq!(shown[1].detail, "ghost — no such persona");
    assert_eq!(
        shown[2].detail,
        "2 workspaces are behind the current layout: alpha, beta"
    );
    assert_eq!(
        shown[3].detail,
        "plane · dirty · on side, not main · memory commit not pushed"
    );
    assert_eq!(shown[3].severity, Severity::Bad);
    assert_eq!(shown[3].subject, "plane root");
}
