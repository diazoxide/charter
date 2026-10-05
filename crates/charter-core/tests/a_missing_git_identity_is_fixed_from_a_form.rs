//! The git identity fix (FX-3): the doctor's `git identity` row carries the fix id
//! `git-identity`, and the fix takes a name and an email — from the window's form or from
//! `charter doctor --fix git-identity --name … --email …` — and writes them where the row's
//! hint names, git's global config.
//!
//! **Never the real user's global config.** The product's git keeps only `HOME` from the
//! environment, and a test cannot set `HOME` in its own process, so every test here re-runs
//! itself as a child with a temporary `HOME` ([`charter_core::testrun`]).

use std::path::{Path, PathBuf};

use charter_core::doctor::fix::{self, FixId, Fixed, identity};
use charter_core::doctor::{Doctor, Row, Status};

/// Set on the child: the temporary home it was started with.
const CHILD: &str = "CHARTER_TEST_FX3_HOME";

/// Run `test` again in a child whose `HOME` is a new empty directory. Answers the home when
/// this is the child, and `None` in the parent once the child has passed.
fn in_a_temporary_home(test: &str) -> Option<(PathBuf, tempfile::TempDir)> {
    if let Some(home) = std::env::var_os(CHILD) {
        // Never the real home: the child runs only with the one it was handed.
        assert_eq!(
            std::env::var_os("HOME").as_deref(),
            Some(home.as_os_str()),
            "the child's HOME is not the temporary one"
        );
        let project = tempfile::tempdir().unwrap();
        return Some((PathBuf::from(home), project));
    }
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    charter_core::testrun::rerun(
        &[test],
        &[(CHILD, home.as_os_str()), ("HOME", home.as_os_str())],
    );
    None
}

fn project(dir: &tempfile::TempDir) -> PathBuf {
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
    for d in ["personas", "inventory", "workspaces"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    root
}

fn row(root: &Path) -> Row {
    Doctor::at(root, root, false, true)
        .run()
        .into_iter()
        .find(|r| r.name == "git identity")
        .expect("a git identity row")
}

#[test]
fn a_missing_identity_is_offered_as_a_fix_set_from_a_name_and_an_email_and_then_checks_clean() {
    charter_core::unsteered!();
    let Some((home, dir)) = in_a_temporary_home(
        "a_missing_identity_is_offered_as_a_fix_set_from_a_name_and_an_email_and_then_checks_clean",
    ) else {
        return;
    };
    let root = project(&dir);

    let found = row(&root);
    assert_eq!(found.status, Status::Fail, "{found:?}");
    assert_eq!(found.fix, Some(FixId::GitIdentity), "{found:?}");
    assert!(
        Doctor::at(&root, &root, false, true)
            .fixes()
            .contains(&FixId::GitIdentity)
    );
    assert!(FixId::GitIdentity.takes_input());
    assert!(!FixId::Reinit.takes_input());

    let applied = identity::apply("  Ann Example ", " ann@example.invalid ")
        .expect("a name and an email that are fine");
    let Fixed::Ran { said, complete } = &applied else {
        panic!("refused: {applied:?}");
    };
    assert!(complete, "{said:?}");
    assert!(
        said.iter()
            .any(|l| l.contains("user.name") && l.contains("Ann Example")),
        "it says what it set: {said:?}"
    );
    let written = std::fs::read_to_string(home.join(".gitconfig")).expect("the global config");
    assert!(written.contains("Ann Example"), "{written}");
    assert!(written.contains("ann@example.invalid"), "{written}");

    let after = row(&root);
    assert_eq!(after.status, Status::Ok, "{after:?}");
    assert_eq!(after.detail, "Ann Example <ann@example.invalid>");
    assert_eq!(after.fix, None, "a clean row offers no fix: {after:?}");
}

#[test]
fn the_fix_by_its_id_alone_is_refused_because_it_needs_a_name_and_an_email() {
    charter_core::unsteered!();
    let Some((home, dir)) = in_a_temporary_home(
        "the_fix_by_its_id_alone_is_refused_because_it_needs_a_name_and_an_email",
    ) else {
        return;
    };
    let root = project(&dir);

    let applied = fix::apply(&root, FixId::GitIdentity);

    let Fixed::Refused(why) = &applied else {
        panic!("applied with nothing to write: {applied:?}");
    };
    assert!(
        why.contains("--name") && why.contains("--email"),
        "it says what to give: {why}"
    );
    assert!(!home.join(".gitconfig").exists());
}

#[test]
fn a_bad_name_or_email_is_refused_field_by_field_and_nothing_is_written() {
    charter_core::unsteered!();
    let Some((home, _dir)) =
        in_a_temporary_home("a_bad_name_or_email_is_refused_field_by_field_and_nothing_is_written")
    else {
        return;
    };

    let both = identity::apply("   ", "not an email").expect_err("both are wrong");
    assert_eq!(both.name.len(), 1, "{both:?}");
    assert_eq!(both.email.len(), 1, "{both:?}");

    let only_email = identity::apply("Ann", "").expect_err("an empty email");
    assert!(only_email.name.is_empty(), "{only_email:?}");
    assert!(only_email.email[0].contains("email"), "{only_email:?}");

    for email in [
        "ann@",
        "@example.invalid",
        "ann@@example.invalid",
        "a nn@x.invalid",
        "<a@x.y>",
    ] {
        let refused = identity::apply("Ann", email).expect_err(email);
        assert_eq!(refused.email.len(), 1, "{email}: {refused:?}");
    }
    for name in [
        "Ann <ann@x.y>",
        "Ann\nExample",
        "Ann\u{202e}elpmaxE",
        "Ann\u{200b}Example",
        "Ann\u{2028}Example",
        "Ann\tExample",
    ] {
        let refused = identity::apply(name, "ann@example.invalid").expect_err(name);
        assert_eq!(refused.name.len(), 1, "{name:?}: {refused:?}");
        assert!(refused.email.is_empty(), "{refused:?}");
    }

    for email in ["ann\u{200d}@example.invalid", "ann@exa\u{2029}mple.invalid"] {
        let refused = identity::apply("Ann", email).expect_err(email);
        assert_eq!(refused.email.len(), 1, "{email:?}: {refused:?}");
    }
    // 256 characters is the cap: 257 is refused, 256 is not (checked beside a bad email, so
    // nothing is written).
    let at_cap = identity::apply(&"a".repeat(256), "nope").expect_err("the email");
    assert!(at_cap.name.is_empty(), "{at_cap:?}");
    let over = identity::apply(&"a".repeat(257), "nope").expect_err("both");
    assert_eq!(over.name.len(), 1, "{over:?}");
    let long_email = format!("{}@example.invalid", "a".repeat(250));
    let over = identity::apply("Ann", &long_email).expect_err("a long email");
    assert_eq!(over.email.len(), 1, "{over:?}");

    assert!(
        !home.join(".gitconfig").exists(),
        "a refusal wrote the config"
    );
}

fn write_global(home: &Path, body: &str) {
    std::fs::write(home.join(".gitconfig"), body).unwrap();
}

#[test]
fn a_complete_identity_is_never_overwritten_and_the_fix_says_it_is_already_set() {
    charter_core::unsteered!();
    let Some((home, _dir)) = in_a_temporary_home(
        "a_complete_identity_is_never_overwritten_and_the_fix_says_it_is_already_set",
    ) else {
        return;
    };
    // Set from a terminal after the dialog was opened: the form's values must not win.
    let before = "[user]\n\tname = Bea Terminal\n\temail = bea@example.invalid\n";
    write_global(&home, before);

    let applied = identity::apply("Ann Example", "ann@example.invalid").expect("valid input");

    let Fixed::Refused(why) = &applied else {
        panic!("a complete identity was written over: {applied:?}");
    };
    assert!(why.contains("already set"), "{why}");
    assert!(why.contains("Bea Terminal <bea@example.invalid>"), "{why}");
    assert_eq!(
        std::fs::read_to_string(home.join(".gitconfig")).unwrap(),
        before
    );
    assert_eq!(
        identity::current().expect("read"),
        identity::Current {
            name: "Bea Terminal".into(),
            email: "bea@example.invalid".into()
        }
    );
}

#[test]
fn with_only_the_name_set_only_the_email_is_written_and_an_empty_name_is_not_refused() {
    charter_core::unsteered!();
    let Some((home, dir)) = in_a_temporary_home(
        "with_only_the_name_set_only_the_email_is_written_and_an_empty_name_is_not_refused",
    ) else {
        return;
    };
    let root = project(&dir);
    write_global(&home, "[user]\n\tname = Bea Terminal\n");
    assert_eq!(
        identity::current().expect("read"),
        identity::Current {
            name: "Bea Terminal".into(),
            email: String::new()
        }
    );

    let applied = identity::apply("", "ann@example.invalid").expect("the name is not asked for");

    let Fixed::Ran { said, complete } = &applied else {
        panic!("refused: {applied:?}");
    };
    assert!(complete, "{said:?}");
    assert!(
        !said.iter().any(|l| l.starts_with("✓ set user.name")),
        "{said:?}"
    );
    let after = row(&root);
    assert_eq!(after.status, Status::Ok, "{after:?}");
    assert_eq!(after.detail, "Bea Terminal <ann@example.invalid>");
}

#[test]
fn with_only_the_name_set_a_different_name_given_is_left_out_and_said() {
    charter_core::unsteered!();
    let Some((home, _dir)) =
        in_a_temporary_home("with_only_the_name_set_a_different_name_given_is_left_out_and_said")
    else {
        return;
    };
    write_global(&home, "[user]\n\tname = Bea Terminal\n");

    let applied = identity::apply("Ann Example", "ann@example.invalid").expect("valid input");

    let Fixed::Ran { said, complete } = &applied else {
        panic!("refused: {applied:?}");
    };
    assert!(complete, "{said:?}");
    assert!(
        said.iter()
            .any(|l| l.contains("user.name is already set") && l.contains("Bea Terminal")),
        "it says the name it kept: {said:?}"
    );
    let written = std::fs::read_to_string(home.join(".gitconfig")).unwrap();
    assert!(written.contains("Bea Terminal"), "{written}");
    assert!(!written.contains("Ann Example"), "{written}");
    assert!(written.contains("ann@example.invalid"), "{written}");
}

#[test]
fn with_only_the_email_set_what_is_sent_for_it_is_not_checked_and_only_the_name_is_written() {
    charter_core::unsteered!();
    let Some((home, _dir)) = in_a_temporary_home(
        "with_only_the_email_set_what_is_sent_for_it_is_not_checked_and_only_the_name_is_written",
    ) else {
        return;
    };
    write_global(&home, "[user]\n\temail = bea@example.invalid\n");

    // The form locks a field that is set, so what it sends for it is ignored, not checked.
    let still = identity::apply("", "nope").expect_err("the name is still missing");
    assert!(still.email.is_empty(), "{still:?}");
    let applied = identity::apply("Ann Example", "nope").expect("only the name is asked for");

    assert!(applied.complete(), "{applied:?}");
    let written = std::fs::read_to_string(home.join(".gitconfig")).unwrap();
    assert!(written.contains("Ann Example"), "{written}");
    assert!(written.contains("bea@example.invalid"), "{written}");
}
