//! The identity row, the frame, and the three things that decide whether an item is on the
//! row at all.

use std::path::{Path, PathBuf};

use super::*;

/// A plane with one workspace whose structure is current — the shape every other case starts
/// from, so a test that asserts "nothing extra on the row" is asserting it against a healthy
/// plane rather than against a broken one that happens to render the same.
fn a_plane(ws: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("charter.toml"), "[plane]\nname = \"fixture\"\n").unwrap();
    let wd = root.join("workspaces").join(ws);
    std::fs::create_dir_all(wd.join("memory")).unwrap();
    std::fs::create_dir_all(wd.join("refs")).unwrap();
    std::fs::create_dir_all(wd.join("todos")).unwrap();
    std::fs::write(wd.join("workspace.md"), "# ws\n").unwrap();
    std::fs::write(wd.join("workspace.json"), "{}\n").unwrap();
    std::fs::write(wd.join("memory").join("MEMORY.md"), "# memory\n").unwrap();
    std::fs::write(wd.join("refs").join("README.md"), "# refs\n").unwrap();
    std::fs::write(wd.join(STRUCTURE_MARKER), "5\n").unwrap();
    (dir, root)
}

fn columns(n: usize) -> impl Fn(&str) -> Option<String> {
    move |name: &str| (name == "COLUMNS").then(|| n.to_string())
}

fn ambient<'a>(env: &'a dyn Fn(&str) -> Option<String>, cwd: &'a Path) -> Ambient<'a> {
    Ambient {
        env,
        cwd,
        now: chrono::DateTime::from_timestamp(1_772_000_000, 0).unwrap(),
        config: None,
    }
}

/// The row, without the frame around it, for a plane and an environment.
fn row(root: &Path, payload: &serde_json::Value, env: &dyn Fn(&str) -> Option<String>) -> String {
    let look = Look::default();
    let active = active_workspace(root, payload, &ambient(env, root));
    identity_row(root, &active, &look, &ambient(env, root))
}

#[test]
fn the_row_names_the_workspace_and_how_many_others_there_are() {
    let (_held, root) = a_plane("alpha");
    std::fs::create_dir_all(root.join("workspaces").join("beta")).unwrap();
    let env = |name: &str| (name == "CHARTER_WORKSPACE").then(|| "alpha".to_string());
    assert_eq!(
        row(&root, &serde_json::Value::Null, &env),
        // The pin is there because `$CHARTER_WORKSPACE` decided, which is the only rung that
        // earns one: that session cannot be moved with `charter ws use`.
        "\x1b[36m⬢\x1b[0m \x1b[1malpha\x1b[0m\x1b[33m*\x1b[0m\x1b[2m · \x1b[0m\x1b[2mws\x1b[0m 2"
    );
}

#[test]
fn a_workspace_chosen_by_anything_but_the_environment_carries_no_pin() {
    let (_held, root) = a_plane("alpha");
    std::fs::write(
        root.join("charter.toml"),
        "[workspace]\ndefault = \"alpha\"\n",
    )
    .unwrap();
    let env = |_: &str| None;
    assert_eq!(
        row(&root, &serde_json::Value::Null, &env),
        "\x1b[36m⬢\x1b[0m \x1b[1malpha\x1b[0m\x1b[2m · \x1b[0m\x1b[2mws\x1b[0m 1"
    );
}

#[test]
fn zero_renders_nothing_and_a_count_renders_beside_what_it_counts() {
    let (_held, root) = a_plane("alpha");
    let env = |name: &str| (name == "CHARTER_WORKSPACE").then(|| "alpha".to_string());
    // Nothing open: no `todo` at all, not `todo 0`.
    assert!(!row(&root, &serde_json::Value::Null, &env).contains("todo"));

    let todos = root.join("workspaces").join("alpha").join("todos");
    std::fs::write(todos.join("MEMORY.md"), "# todos\n").unwrap();
    std::fs::write(todos.join("20260302-090000-one.md"), "# one\n").unwrap();
    std::fs::write(todos.join("20260302-090100-two.md"), "# two\n").unwrap();
    // The index is not a todo, and the count is the store's own gate — two files, not three.
    assert!(row(&root, &serde_json::Value::Null, &env).contains("\x1b[2mtodo\x1b[0m 2"));
}

#[test]
fn a_stale_structure_is_named_before_anything_informational() {
    let (_held, root) = a_plane("alpha");
    let wd = root.join("workspaces").join("alpha");
    std::fs::write(wd.join("todos").join("20260302-090000-one.md"), "# one\n").unwrap();
    std::fs::write(wd.join(STRUCTURE_MARKER), "4\n").unwrap();
    let env = |name: &str| (name == "CHARTER_WORKSPACE").then(|| "alpha".to_string());
    let line = row(&root, &serde_json::Value::Null, &env);
    let tip = line.find("reinit").expect("the tip is on the row");
    let todo = line.find("todo").expect("the count is on the row");
    // The row's order IS its truncation order: on a pane with room for one of the two, the
    // item naming something BROKEN is the one that survives.
    assert!(tip < todo, "{line:?}");
}

#[test]
fn stale_is_a_missing_baseline_file_or_an_old_marker_and_nothing_else() {
    let (_held, root) = a_plane("alpha");
    let wd = root.join("workspaces").join("alpha");
    assert!(!needs_reinit(&root, "alpha"));

    // An older layout.
    std::fs::write(wd.join(STRUCTURE_MARKER), "4\n").unwrap();
    assert!(needs_reinit(&root, "alpha"));
    std::fs::write(wd.join(STRUCTURE_MARKER), "5\n").unwrap();
    assert!(!needs_reinit(&root, "alpha"));

    // No marker at all is version 0.
    std::fs::remove_file(wd.join(STRUCTURE_MARKER)).unwrap();
    assert!(needs_reinit(&root, "alpha"));
    // The pre-rename name still answers, without being renamed on a render path.
    std::fs::write(wd.join(LEGACY_STRUCTURE_MARKER), "5\n").unwrap();
    assert!(!needs_reinit(&root, "alpha"));
    assert!(!wd.join(STRUCTURE_MARKER).exists());

    // A marker that is not a regular file is not a version charter wrote.
    std::fs::remove_file(wd.join(LEGACY_STRUCTURE_MARKER)).unwrap();
    std::fs::create_dir(wd.join(STRUCTURE_MARKER)).unwrap();
    assert!(needs_reinit(&root, "alpha"));
    std::fs::remove_dir(wd.join(STRUCTURE_MARKER)).unwrap();
    std::fs::write(wd.join(STRUCTURE_MARKER), "5\n").unwrap();

    // A missing baseline file.
    std::fs::remove_file(wd.join("refs").join("README.md")).unwrap();
    assert!(needs_reinit(&root, "alpha"));

    // …unless something stands in its way, which `reinit` could not create through anyway.
    std::fs::remove_dir(wd.join("refs")).unwrap();
    std::fs::write(wd.join("refs"), "not a directory\n").unwrap();
    assert!(!needs_reinit(&root, "alpha"));

    // A workspace that is not there is not a stale one.
    assert!(!needs_reinit(&root, "nowhere"));
}

#[test]
fn an_accent_is_the_planes_word_and_falls_back_to_the_one_charter_ships() {
    let (_held, root) = a_plane("alpha");
    // Nothing said: the shipped three.
    let look = Look::of(&root);
    assert_eq!(look.accent(Role::Ok), "\x1b[32m");
    assert_eq!(look.accent(Role::Warn), "\x1b[33m");
    assert_eq!(look.accent(Role::Bad), "\x1b[31m");

    std::fs::write(
        root.join("charter.toml"),
        "[frame]\nok = \"blue\"\nwarn = \"brightmagenta\"\nbad = \"default\"\n",
    )
    .unwrap();
    let look = Look::of(&root);
    assert_eq!(look.accent(Role::Ok), "\x1b[34m");
    assert_eq!(look.accent(Role::Warn), "\x1b[95m");
    // `default` is SGR 39, the pane's own foreground: a plane that says so has asked for its
    // warnings uncoloured and gets the colour the rest of its frame is in.
    assert_eq!(look.accent(Role::Bad), "\x1b[39m");

    // A word charter does not know, and a value that is not a word at all.
    std::fs::write(
        root.join("charter.toml"),
        "[frame]\nok = \"chartreuse\"\nwarn = 7\nbad = [\"red\"]\n",
    )
    .unwrap();
    let look = Look::of(&root);
    assert_eq!(look.accent(Role::Ok), "\x1b[32m");
    assert_eq!(look.accent(Role::Warn), "\x1b[33m");
    assert_eq!(look.accent(Role::Bad), "\x1b[31m");
}

#[test]
fn the_frame_is_a_ruler_and_every_row_carries_one_border_each_side() {
    let body = vec![
        format!("{DIM}a{R}"),
        RULE_LINE.to_string(),
        "tail".to_string(),
    ];
    assert_eq!(
        boxed(&body, 30),
        "\x1b[2m┌────────────────────────────┐\x1b[0m\n\
         \x1b[2m│\x1b[0m \x1b[2ma\x1b[0m                          \x1b[2m│\x1b[0m\n\
         \x1b[2m├────────────────────────────┤\x1b[0m\n\
         \x1b[2m│\x1b[0m tail                       \x1b[2m│\x1b[0m\n\
         \x1b[2m└────────────────────────────┘\x1b[0m"
    );
    // Too narrow to frame: the box is decoration and must never cost content — and the rule
    // SENTINEL is dropped rather than printed, because a rule with no borders to join is not
    // a rule and the sentinel must never reach a terminal.
    assert_eq!(boxed(&body, 23), "\x1b[2ma\x1b[0m\ntail");
}

#[test]
fn the_divider_goes_under_the_workspace_line_and_nowhere_else() {
    let body = vec!["one".to_string(), "two".to_string(), "three".to_string()];
    assert_eq!(zone_rules(&body), vec!["one", RULE_LINE, "two", "three"]);
    // Nothing to divide.
    assert_eq!(zone_rules(&["one".to_string()]), vec!["one"]);
}

#[test]
fn the_body_says_what_it_does_not_draw_rather_than_leaving_it_out() {
    let (_held, root) = a_plane("alpha");
    // Pinned, because standing in the plane ROOT is not standing in a workspace: with nothing
    // else to go on the ladder ends on the built-in `default`, and a row naming a workspace
    // that exists would be this test asserting something it had not arranged.
    let env = |name: &str| match name {
        "COLUMNS" => Some("80".to_string()),
        "CHARTER_WORKSPACE" => Some("alpha".to_string()),
        _ => None,
    };
    let out = render(&root, &serde_json::Value::Null, &ambient(&env, &root));
    let lines: Vec<&str> = out.lines().collect();
    // Frame, identity row, rule, the declaration, frame.
    assert_eq!(lines.len(), 5, "{out:?}");
    assert!(lines[1].contains("alpha"));
    assert!(lines[2].contains('├'));
    assert!(
        lines[3].contains(NOT_DRAWN_YET),
        "an omitted section is named, never merely absent: {:?}",
        lines[3]
    );
    // charter's `render` ends on a newline and `print` adds the second one.
    assert!(out.ends_with('\n'));
}

#[test]
fn the_frame_is_the_pane_less_the_safety_margin() {
    let (_held, root) = a_plane("alpha");
    // A pane narrower than the floor still frames at the floor — the box is never allowed to
    // collapse to something a row cannot sit in. `$COLUMNS` of zero is `judged`'s to answer,
    // and it is asserted there rather than here, where the tty behind it is whatever terminal
    // the suite happened to run under.
    for (cols, frame) in [(80usize, 76usize), (40, 36), (20, 24)] {
        let env = columns(cols);
        let out = render(&root, &serde_json::Value::Null, &ambient(&env, &root));
        let top = out.lines().next().unwrap();
        assert_eq!(
            crate::tui::width(top),
            frame,
            "COLUMNS={cols} should frame at {frame}: {top:?}"
        );
    }
}

#[test]
fn a_name_wider_than_the_pane_is_cut_inside_the_frame_and_the_border_still_lines_up() {
    let (_held, root) = a_plane("日本語の作業スペース");
    let env = |name: &str| match name {
        "COLUMNS" => Some("40".to_string()),
        "CHARTER_WORKSPACE" => Some("日本語の作業スペース".to_string()),
        _ => None,
    };
    let out = render(&root, &serde_json::Value::Null, &ambient(&env, &root));
    let widths: Vec<usize> = out.lines().map(crate::tui::width).collect();
    // Every line is the same number of COLUMNS — which is the whole claim the east-asian
    // table buys, and which a character count gets wrong by one per glyph.
    assert!(
        widths.iter().all(|w| *w == widths[0]),
        "{widths:?} for {out:?}"
    );
}

#[test]
fn an_alert_row_follows_the_declaration_inside_the_frame_and_the_active_workspace_is_not_one() {
    let (_held, root) = a_plane("alpha");
    let (_other, other) = a_plane("beta");
    std::fs::rename(other.join("workspaces/beta"), root.join("workspaces/beta")).unwrap();
    std::fs::write(root.join("workspaces/alpha").join(STRUCTURE_MARKER), "4\n").unwrap();
    std::fs::write(root.join("workspaces/beta").join(STRUCTURE_MARKER), "4\n").unwrap();
    let env = |name: &str| match name {
        "COLUMNS" => Some("80".to_string()),
        "CHARTER_WORKSPACE" => Some("alpha".to_string()),
        _ => None,
    };
    let out = render(&root, &serde_json::Value::Null, &ambient(&env, &root));
    let lines: Vec<&str> = out.lines().collect();
    // Frame, identity row, rule, the declaration, ONE alert, frame: alpha's own stale layout
    // is the identity row's tip, so the alert counts beta alone.
    assert_eq!(lines.len(), 6, "{out:?}");
    assert!(lines[1].contains("reinit"), "{:?}", lines[1]);
    assert!(
        lines[4].contains("⚠\x1b[0m \x1b[2mreinit\x1b[0m 1 \x1b[2mws · charter ws reinit --all"),
        "{:?}",
        lines[4]
    );
    assert!(!NOT_DRAWN_YET.contains("alerts"), "alerts are drawn now");
    let widths: Vec<usize> = lines.iter().map(|l| crate::tui::width(l)).collect();
    assert!(widths.iter().all(|w| *w == widths[0]), "{widths:?}");
}
