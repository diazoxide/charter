use super::*;

/// A token of the shape `token_kind` knows, built at run time so no credential-shaped literal
/// sits in the source.
fn a_token() -> String {
    format!("ghp_{}", "A1b2C3d4".repeat(5))
}

fn known() -> Known {
    Known {
        plane: Some("/srv/planes/acme-plane".to_string()),
        home: Some("/var/root".to_string()),
        env: vec![
            (
                "DB_PASSWORD_FOR_CI".to_string(),
                "correct-horse-battery".to_string(),
            ),
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("SHORTY".to_string(), "abc".to_string()),
        ],
        names: vec![
            ("acme".to_string(), "[workspace]"),
            ("acme-migration".to_string(), "[workspace]"),
            ("billing-api".to_string(), "[repo]"),
            ("clientvault".to_string(), "[vault]"),
        ],
    }
}

#[test]
fn a_value_in_the_environment_never_reaches_the_report_and_its_name_says_what_went() {
    let (out, used) = known().scrub("it printed correct-horse-battery and stopped");
    assert_eq!(out, "it printed [env $DB_PASSWORD_FOR_CI] and stopped");
    assert!(used.contains(&"environment values".to_string()), "{used:?}");
}

#[test]
fn a_terminal_name_and_a_short_value_are_left_alone() {
    let (out, _) = known().scrub("in xterm-256color, the flag abc did nothing");
    assert_eq!(out, "in xterm-256color, the flag abc did nothing");
}

#[test]
fn a_line_secretshape_reads_as_a_credential_goes_whole() {
    let text = format!("first line\nexport X={}\nlast line", a_token());
    let (out, used) = known().scrub(&text);
    assert!(!out.contains(&a_token()), "{out}");
    assert_eq!(
        out,
        "first line\n[redacted: a line that looks like a token by its forge's prefix]\nlast line"
    );
    assert!(used.contains(&"lines that look like a credential".to_string()));

    let (out, _) = known().scrub("password = hunter2hunter2");
    assert!(!out.contains("hunter2"), "{out}");
}

#[test]
fn the_planes_path_and_home_paths_are_removed_and_the_markdown_around_them_survives() {
    let (out, used) = known().scrub(
        "`/srv/planes/acme-plane/charter.toml` and `/Users/someone/work/x.rs` and \
         /home/who/y and /var/root/z",
    );
    assert_eq!(
        out,
        "`[plane]/charter.toml` and `[home path]` and [home path] and [home]/z"
    );
    assert!(used.contains(&"the plane's path".to_string()), "{used:?}");
    assert!(used.contains(&"home paths".to_string()), "{used:?}");
}

#[test]
fn a_known_name_goes_whole_and_only_as_a_whole_word() {
    let (out, used) = known().scrub(
        "acme-migration broke, acme too, billing-api cloned, clientvault read; acmeish stays",
    );
    assert_eq!(
        out,
        "[workspace] broke, [workspace] too, [repo] cloned, [vault] read; acmeish stays"
    );
    assert!(used.contains(&"workspace names".to_string()), "{used:?}");
    assert!(used.contains(&"repo names".to_string()), "{used:?}");
    assert!(used.contains(&"vault names".to_string()), "{used:?}");
}

#[test]
fn a_planes_names_are_read_from_its_directories_and_default_is_not_one() {
    let dir = tempfile::tempdir().expect("a directory");
    let plane = dir.path();
    for d in [
        "workspaces/acme/billing-api/.git",
        "workspaces/acme/notes",
        "workspaces/default",
        "personas/steward",
        "personas/_shared",
    ] {
        std::fs::create_dir_all(plane.join(d)).expect("mkdir");
    }
    let mut names = names_in(plane);
    names.sort();
    assert_eq!(
        names,
        vec![
            ("acme".to_string(), "[workspace]"),
            ("billing-api".to_string(), "[repo]"),
            ("steward".to_string(), "[persona]"),
        ]
    );
}

#[test]
fn a_described_report_is_titled_by_its_first_line_without_a_heading_marker() {
    let d = Draft::described(
        Kind::Feature,
        "## `charter report` should exist\n\nbody",
        None,
        &Known::default(),
    )
    .expect("a draft");
    assert_eq!(d.title, "`charter report` should exist");
    assert!(
        d.body
            .starts_with("## `charter report` should exist\n\nbody\n\n---\ncharter ")
    );
    assert!(
        d.body.ends_with("_Filed with `charter report feature`._"),
        "{}",
        d.body
    );

    let long = "word ".repeat(40);
    let d = Draft::described(Kind::Bug, &long, None, &Known::default()).expect("a draft");
    assert!(
        d.title.chars().count() <= TITLE_MAX && d.title.ends_with("word…"),
        "{}",
        d.title
    );
}

#[test]
fn an_empty_description_is_refused() {
    let err = Draft::described(Kind::Bug, "  \n ", None, &Known::default()).unwrap_err();
    assert!(err.0.contains("needs a description"), "{err:?}");
}

#[test]
fn the_title_is_scrubbed_too() {
    let d = Draft::described(Kind::Bug, "x", Some("acme broke"), &known()).expect("a draft");
    assert_eq!(d.title, "[workspace] broke");
}

#[test]
fn the_digest_names_exactly_one_draft() {
    let a = Draft::described(Kind::Bug, "one", None, &Known::default()).unwrap();
    let b = Draft::described(Kind::Bug, "one", None, &Known::default()).unwrap();
    let c = Draft::described(Kind::Bug, "one.", None, &Known::default()).unwrap();
    assert_eq!(a.digest(), b.digest());
    assert_ne!(a.digest(), c.digest());
    assert_eq!(a.digest().len(), 12);
}

const LOG: &str = "charter-panic pid 1 at 5 (seconds since 1970)\n\
thread: main\n\
place: app/src-tauri/src/old.rs:1:1\n\
message: the old one\n\
backtrace:\n  0: frame\n\
charter-panic end\n\
charter-panic pid 42 at 9 (seconds since 1970)\n\
version: 9.8.7\n\
thread: charter-view\n\
place: /Users/someone/.cargo/registry/src/index/tokio-1.0/src/rt.rs:12:5\n\
message: workspace acme is gone\nsecond line\n\
backtrace:\n  0: /Users/someone/secret/frame.rs\n\
charter-panic end\n";

#[test]
fn the_newest_panic_in_the_log_is_the_one_read() {
    let p = latest_panic(LOG).expect("a panic");
    assert_eq!(
        p,
        Panic {
            place: "/Users/someone/.cargo/registry/src/index/tokio-1.0/src/rt.rs:12:5".into(),
            message: "workspace acme is gone\nsecond line".into(),
            version: Some("9.8.7".into()),
        }
    );
    let old = latest_panic(&LOG[..LOG.find("charter-panic pid 42").unwrap()]).unwrap();
    assert_eq!(old.version, None);
    assert_eq!(old.place, "app/src-tauri/src/old.rs:1:1");
    assert_eq!(latest_panic("nothing here"), None);
}

#[test]
fn a_panic_becomes_a_bug_with_its_place_and_version_and_nothing_else_of_the_record() {
    let p = latest_panic(LOG).unwrap();
    let d = Draft::of_panic(&p, Some("I clicked Save"), &known()).expect("a draft");
    assert_eq!(d.kind, Kind::Bug);
    assert_eq!(d.title, "charter panicked at …/tokio-1.0/src/rt.rs:12:5");
    assert!(
        d.body.starts_with("I clicked Save\n\ncharter panicked."),
        "{}",
        d.body
    );
    assert!(
        d.body.contains("`…/tokio-1.0/src/rt.rs:12:5`"),
        "{}",
        d.body
    );
    assert!(d.body.contains("**charter version:** 9.8.7"), "{}", d.body);
    assert!(
        d.body
            .contains("  > workspace [workspace] is gone\n  > second line"),
        "{}",
        d.body
    );
    for gone in ["someone", "frame.rs", "charter-view", "acme"] {
        assert!(
            !d.body.contains(gone),
            "{gone} reached the body: {}",
            d.body
        );
    }
}
