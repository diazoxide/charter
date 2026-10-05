//! A `CHARTER_*` steering variable that IS set reaches the code that reads it.
//!
//! `steer` hides every steering variable from this crate's unit tests (`cfg(test)`), and the
//! `unsteered!` guard sheds them from these integration tests, so between the two nothing in
//! charter-core's own suite ever ran with one set. What a set one does was proven only by
//! charter-cli's tests, which the nightly mutation run does not build, and `steer::var_os`,
//! `steer::var` and `wscmd::select::warn_env_override` could each be replaced by "unset" with
//! nothing in charter-core noticing (#464).
//!
//! So this test re-runs itself with the variables set ON PURPOSE
//! ([`charter_core::testrun::rerun_steered`]), and the child asks the library what it sees.

use std::path::Path;

use charter_core::active::Ids;
use charter_core::repocmd::Say;

/// Set on the child, and only there: it is the half of the test that runs steered.
const CHILD: &str = "STEERING_REACHES_THE_LIBRARY_CHILD";

const ME: &str = "a_steering_variable_set_on_the_process_is_what_the_library_reads";

#[test]
fn a_steering_variable_set_on_the_process_is_what_the_library_reads() {
    charter_core::unsteered!();
    let Some(home) = std::env::var_os(CHILD) else {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("state-the-operator-chose");
        charter_core::testrun::rerun_steered(
            &[ME],
            &[
                (CHILD, home.as_os_str()),
                ("CHARTER_HOME", home.as_os_str()),
                ("CHARTER_WORKSPACE", "elsewhere".as_ref()),
            ],
        );
        return;
    };

    // `CHARTER_HOME`, read through `steer::var_os`: the state directory is the one it names,
    // not the plane's own `.charter`.
    let plane = tempfile::tempdir().unwrap();
    std::fs::write(plane.path().join("charter.toml"), "").unwrap();
    std::fs::create_dir_all(plane.path().join("workspaces")).unwrap();
    assert_eq!(
        charter_core::plane::state_dir(plane.path()),
        Path::new(&home),
        "a CHARTER_HOME that is set was not where the state went"
    );

    // `CHARTER_WORKSPACE`, read through `steer::var`: choosing another workspace while it
    // names one is said out loud, naming the one that will really be acted on.
    assert_eq!(
        warnings(plane.path(), "default"),
        [
            "$CHARTER_WORKSPACE='elsewhere' is set and takes precedence — commands in this \
             session will still act on 'elsewhere', not 'default'."
        ]
    );
    // Choosing the very workspace it names is no override, and nothing is said.
    std::fs::create_dir_all(plane.path().join("workspaces/elsewhere")).unwrap();
    assert!(warnings(plane.path(), "elsewhere").is_empty());
}

/// The warnings `charter workspace use <name>` says, for a session of its own.
fn warnings(plane: &Path, name: &str) -> Vec<String> {
    let ids = Ids {
        session: Some(format!("session-for-{name}")),
        terminal: None,
    };
    let mut said = Vec::new();
    let code = charter_core::wscmd::select::use_workspace(
        plane,
        name,
        &ids,
        false,
        false,
        "2026-05-04T11:32:17Z".parse().unwrap(),
        &mut |line: Say| said.push(line),
    );
    assert_eq!(code, 0, "{said:?}");
    said.into_iter()
        .filter_map(|line| match line {
            Say::Warn(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// Set on the child of the test below: the plane `CHARTER_ROOT` pins.
const PINNED: &str = "STEERING_PINS_THE_PLANE_CHILD";

const PINS: &str = "a_pinned_plane_is_the_one_acted_on_and_an_empty_home_is_no_home";

/// `CHARTER_ROOT` pins the plane wherever the command runs, and an empty `CHARTER_HOME` is
/// unset rather than a state directory with no name (#480).
#[test]
fn a_pinned_plane_is_the_one_acted_on_and_an_empty_home_is_no_home() {
    charter_core::unsteered!();
    let Some(pinned) = std::env::var_os(PINNED) else {
        let dir = tempfile::tempdir().unwrap();
        let pinned = dir.path().join("pinned");
        std::fs::create_dir_all(&pinned).unwrap();
        std::fs::write(pinned.join("charter.toml"), "").unwrap();
        charter_core::testrun::rerun_steered(
            &[PINS],
            &[
                (PINNED, pinned.as_os_str()),
                ("CHARTER_ROOT", pinned.as_os_str()),
                ("CHARTER_HOME", "".as_ref()),
            ],
        );
        return;
    };

    // Standing in another plane altogether, the pinned one is still the one resolved.
    let here = tempfile::tempdir().unwrap();
    std::fs::write(here.path().join("charter.toml"), "").unwrap();
    assert_eq!(
        charter_core::plane::resolve(here.path()).unwrap(),
        Path::new(&pinned)
    );
    // And `place`, which `init` and `doctor` stand on, takes it the same way (#464).
    let place = charter_core::plane::place(here.path());
    assert_eq!(
        place.root,
        Path::new(&pinned).canonicalize().unwrap(),
        "{place:?}"
    );
    assert!(place.is_plane, "{place:?}");
    assert_eq!(
        charter_core::plane::state_dir(here.path()),
        here.path().join(".charter"),
        "an empty CHARTER_HOME names no state directory"
    );
}

/// Set on the child of the doctor test below; its value is the case the child runs.
const DOCTOR_CHILD: &str = "STEERING_REACHES_THE_DOCTOR_CHILD";

const DOCTOR: &str = "the_harness_the_environment_names_is_the_one_doctor_answers_for";

/// `doctor`'s `session layer` row, asked from a workspace of a fresh plane.
fn session_layer() -> String {
    let plane = tempfile::tempdir().unwrap();
    std::fs::write(plane.path().join("charter.toml"), "schema = 1\n").unwrap();
    let here = plane.path().join("workspaces/alpha");
    std::fs::create_dir_all(&here).unwrap();
    let rows = charter_core::doctor::Doctor::at(plane.path(), &here, true, true).run();
    rows.into_iter()
        .find(|row| row.name == "session layer")
        .expect("a session layer row")
        .detail
}

#[test]
fn the_harness_the_environment_names_is_the_one_doctor_answers_for() {
    charter_core::unsteered!();
    let Some(case) = std::env::var_os(DOCTOR_CHILD) else {
        // `CHARTER_HARNESS` names the harness, whatever it is; with none named, Claude Code's
        // own `CLAUDE_PLUGIN_ROOT` is its evidence; an empty value of either names nothing.
        for (case, env) in [
            (
                "named",
                &[("CHARTER_HARNESS", "a-harness-charter-never-met")][..],
            ),
            ("plugin", &[("CLAUDE_PLUGIN_ROOT", "/plugins/charter")][..]),
            (
                "empty",
                &[("CHARTER_HARNESS", ""), ("CLAUDE_PLUGIN_ROOT", "")][..],
            ),
        ] {
            let mut vars: Vec<(&str, &std::ffi::OsStr)> =
                env.iter().map(|(k, v)| (*k, v.as_ref())).collect();
            vars.push((DOCTOR_CHILD, case.as_ref()));
            charter_core::testrun::rerun_steered(&[DOCTOR], &vars);
        }
        return;
    };

    let layer = session_layer();
    match case.to_str() {
        Some("named") => assert!(
            layer.contains(
                "charter has no record of how a-harness-charter-never-met finds an in-repo layer"
            ),
            "{layer}"
        ),
        Some("plugin") => {
            assert!(layer.contains("claude-code: "), "{layer}");
            assert!(
                !layer.contains("codex: "),
                "only the running harness: {layer}"
            );
        }
        Some("empty") => {
            assert!(layer.contains("claude-code: "), "{layer}");
            assert!(
                layer.contains("codex: "),
                "every harness, none named: {layer}"
            );
        }
        other => panic!("no case {other:?}"),
    }
}

/// Set on the child of the test below; its value is the case the child runs.
const RENAMED_CHILD: &str = "STEERING_UNDER_EITHER_NAME_CHILD";

const RENAMED: &str = "a_purlis_variable_wins_and_a_charter_one_is_read_when_it_is_absent";

/// The rename's window (V93k): `PURLIS_ROOT` pins the plane, and wins over a `CHARTER_ROOT`
/// set beside it; a `CHARTER_ROOT` alone still pins it, as it did before the rename.
#[test]
fn a_purlis_variable_wins_and_a_charter_one_is_read_when_it_is_absent() {
    charter_core::unsteered!();
    let Some(case) = std::env::var_os(RENAMED_CHILD) else {
        let dir = tempfile::tempdir().unwrap();
        for name in ["purlis", "charter"] {
            let plane = dir.path().join(name);
            std::fs::create_dir_all(&plane).unwrap();
            std::fs::write(plane.join("charter.toml"), "").unwrap();
        }
        let purlis = dir.path().join("purlis");
        let charter = dir.path().join("charter");
        for (case, env) in [
            (
                "both",
                vec![
                    ("PURLIS_ROOT", purlis.as_os_str()),
                    ("CHARTER_ROOT", charter.as_os_str()),
                ],
            ),
            ("old only", vec![("CHARTER_ROOT", charter.as_os_str())]),
            ("new only", vec![("PURLIS_ROOT", purlis.as_os_str())]),
        ] {
            let mut vars = env;
            vars.push((RENAMED_CHILD, case.as_ref()));
            charter_core::testrun::rerun_steered(&[RENAMED], &vars);
        }
        return;
    };

    let here = tempfile::tempdir().unwrap();
    std::fs::write(here.path().join("charter.toml"), "").unwrap();
    let resolved = charter_core::plane::resolve(here.path()).unwrap();
    let want = match case.to_str() {
        Some("both" | "new only") => "purlis",
        Some("old only") => "charter",
        other => panic!("no case {other:?}"),
    };
    assert_eq!(
        resolved.file_name().and_then(|n| n.to_str()),
        Some(want),
        "{case:?}: {resolved:?}"
    );
}
