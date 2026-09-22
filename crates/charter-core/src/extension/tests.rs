//! The registry's guards, each with a test that has been seen to go red without it.
//!
//! charter ADR 0041's gate item 6 is *"every unreadable state asks, and each of those states
//! has a test that has been seen to go red with the guard removed — this repository's own
//! standard, and the one it keeps missing."* The mutation run that measured these is in the
//! PR that landed them.

use super::*;

/// A directory holding a manifest and whatever files it declares.
struct Made {
    dir: tempfile::TempDir,
}

impl Made {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("a directory"),
        }
    }

    fn at(&self) -> PathBuf {
        self.dir.path().join("ext")
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join("config")
    }

    /// An extension contributing one theme, which is the shape everything else varies from.
    fn ordinary(&self) -> PathBuf {
        self.manifest(
            r#"{"version":1,"id":"solarized","name":"Solarized",
                "contributes":{"themes":[{"name":"Solarized Dark","file":"dark.json"}]}}"#,
        );
        self.file(
            "dark.json",
            r#"{"name":"Solarized Dark","appearance":"dark","tokens":{}}"#,
        );
        self.at()
    }

    fn manifest(&self, text: &str) {
        self.file(MANIFEST, text);
    }

    fn file(&self, name: &str, text: &str) {
        let at = self.at().join(name);
        std::fs::create_dir_all(at.parent().expect("a parent")).expect("the directory");
        std::fs::write(at, text).expect("the file");
    }
}

// -------------------------------------------------------------------------------------
// Reading an extension: every unreadable state, and each one is a refusal with a reason
// -------------------------------------------------------------------------------------

#[test]
fn an_extension_is_its_manifest_and_what_it_declares() {
    let made = Made::new();
    let found = read_at(&made.ordinary()).expect("an extension");

    assert_eq!(found.id(), "solarized");
    assert_eq!(found.manifest.name, "Solarized");
    assert_eq!(found.manifest.themes.len(), 1);
    assert_eq!(found.manifest.themes[0].name, "Solarized Dark");
    assert_eq!(found.manifest.program, None);
    assert_eq!(found.fingerprint.len(), 64, "{}", found.fingerprint);
}

#[test]
fn a_directory_that_is_not_there_is_refused_by_name() {
    let made = Made::new();
    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("is not there"), "{why}");
}

#[test]
fn a_manifest_that_is_not_there_is_refused_rather_than_read_as_nothing_declared() {
    let made = Made::new();
    std::fs::create_dir_all(made.at()).expect("the directory");
    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains(MANIFEST), "{why}");
    assert!(why.contains("is not there"), "{why}");
}

#[cfg(unix)]
#[test]
fn an_extension_directory_that_is_a_symlink_is_refused() {
    // The approval is recorded against a path. A link at the extension's own directory can be
    // re-pointed at another tree afterwards and every declared file's contents would change
    // without one of them having been written — so the fingerprint would not notice.
    let made = Made::new();
    made.ordinary();
    let link = made.dir.path().join("link");
    std::os::unix::fs::symlink(made.at(), &link).expect("a link");

    let why = read_at(&link).expect_err("no extension through a link");
    assert!(why.contains("is a symlink"), "{why}");
}

#[cfg(unix)]
#[test]
fn a_manifest_reached_through_a_symlink_is_refused() {
    // `contain::open_no_link` is what refuses this, and it refuses it at the instant of the
    // open rather than at a `stat` a moment earlier.
    let made = Made::new();
    made.ordinary();
    let elsewhere = made.dir.path().join("elsewhere.json");
    std::fs::write(
        &elsewhere,
        r#"{"version":1,"id":"x","contributes":{"runs":"p"}}"#,
    )
    .expect("a file outside");
    let at = made.at().join(MANIFEST);
    std::fs::remove_file(&at).expect("the real manifest");
    std::os::unix::fs::symlink(&elsewhere, &at).expect("a link");

    let why = read_at(&made.at()).expect_err("no extension through a link");
    assert!(why.contains("could not be opened"), "{why}");
}

#[cfg(unix)]
#[test]
fn a_declared_file_reached_through_a_symlink_is_refused() {
    // The file whose bytes go into the fingerprint. A link here is a fingerprint taken over
    // somebody else's file, which then reads as unchanged when the link is re-pointed.
    let made = Made::new();
    made.ordinary();
    let elsewhere = made.dir.path().join("elsewhere.json");
    std::fs::write(&elsewhere, "{}").expect("a file outside");
    let at = made.at().join("dark.json");
    std::fs::remove_file(&at).expect("the real theme");
    std::os::unix::fs::symlink(&elsewhere, &at).expect("a link");

    let why = read_at(&made.at()).expect_err("no extension through a link");
    assert!(why.contains("dark.json"), "{why}");
}

#[cfg(unix)]
#[test]
fn a_fifo_where_a_manifest_should_be_is_refused_rather_than_read() {
    // Not a slow read, a permanent one — and this runs on the path that draws the window, so
    // the app would hang before there was anything to close it from.
    let made = Made::new();
    std::fs::create_dir_all(made.at()).expect("the directory");
    let at = made.at().join(MANIFEST);
    let made_it = crate::forklock::status(std::process::Command::new("mkfifo").arg(&at))
        .expect("mkfifo runs");
    assert!(made_it.success(), "the test needs a fifo to plant");

    let why = read_at(&made.at()).expect_err("no extension from a pipe");
    assert!(why.contains("is not a plain file"), "{why}");
}

#[test]
fn a_manifest_bigger_than_charter_reads_is_refused_rather_than_read() {
    let made = Made::new();
    made.ordinary();
    made.manifest(&"x".repeat(MOST_MANIFEST_BYTES as usize + 1));

    let why = read_at(&made.at()).expect_err("no extension from a giant");
    assert!(why.contains("charter reads no more than"), "{why}");
}

#[test]
fn a_manifest_that_is_not_json_is_refused() {
    let made = Made::new();
    made.ordinary();
    made.manifest("not json at all");

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("is not JSON"), "{why}");
}

#[test]
fn a_manifest_of_another_version_is_refused_rather_than_guessed_at() {
    let made = Made::new();
    made.ordinary();
    made.manifest(r#"{"version":2,"id":"x","contributes":{"runs":"p"}}"#);

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("is version 2"), "{why}");
}

#[test]
fn a_manifest_with_no_version_is_refused() {
    let made = Made::new();
    made.ordinary();
    made.manifest(r#"{"id":"x","contributes":{"runs":"p"}}"#);

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("says no version"), "{why}");
}

#[test]
fn a_manifest_declaring_nothing_is_refused_because_there_is_nothing_to_consent_to() {
    let made = Made::new();
    made.ordinary();
    made.manifest(r#"{"version":1,"id":"x","contributes":{}}"#);

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("nothing to consent to"), "{why}");
}

#[test]
fn an_id_that_is_a_path_is_refused_because_the_record_is_keyed_by_it() {
    // The record is one object keyed by id. An id of `../../machine.json` would be a key that
    // names a file, and every later reader of the record would have to remember that.
    for bad in ["../x", "a/b", "..", "", ".hidden", "a b", "a\0b", "/abs"] {
        let made = Made::new();
        made.ordinary();
        made.manifest(&format!(
            r#"{{"version":1,"id":{},"contributes":{{"runs":"p"}}}}"#,
            serde_json::Value::String(bad.to_owned())
        ));

        let why = read_at(&made.at()).expect_err("no extension for the id {bad:?}");
        assert!(why.contains("id"), "{bad:?}: {why}");
    }
}

#[test]
fn a_declared_file_that_walks_up_out_of_the_extension_is_refused() {
    // Checked here as well as at the open, because this string goes into the fingerprint and
    // onto the screen: the operator must not be shown a path charter has not already said is
    // one.
    let made = Made::new();
    made.ordinary();
    made.manifest(
        r#"{"version":1,"id":"x","contributes":{"themes":[{"file":"../../etc/passwd"}]}}"#,
    );

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("walks up out of"), "{why}");
}

#[test]
fn a_declared_file_that_is_absolute_is_refused() {
    let made = Made::new();
    made.ordinary();
    made.manifest(r#"{"version":1,"id":"x","contributes":{"runs":"/bin/sh"}}"#);

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("is absolute"), "{why}");
}

#[test]
fn a_declared_file_that_is_not_there_is_refused_rather_than_hashed_as_absent() {
    // An extension whose theme file is missing must not read as an extension with a stable
    // fingerprint: putting the file there afterwards would then contribute a theme nobody was
    // asked about.
    let made = Made::new();
    made.ordinary();
    std::fs::remove_file(made.at().join("dark.json")).expect("the theme");

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("dark.json"), "{why}");
    assert!(why.contains("is not there"), "{why}");
}

#[test]
fn a_manifest_declaring_more_files_than_charter_hashes_is_refused() {
    let made = Made::new();
    made.ordinary();
    let themes: Vec<String> = (0..=MOST_DECLARED_FILES)
        .map(|n| format!(r#"{{"file":"t{n}.json"}}"#))
        .collect();
    made.manifest(&format!(
        r#"{{"version":1,"id":"x","contributes":{{"themes":[{}]}}}}"#,
        themes.join(",")
    ));

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("declares more than"), "{why}");
}

// -------------------------------------------------------------------------------------
// The fingerprint, which is over CODE and is what makes a change re-ask
// -------------------------------------------------------------------------------------

#[test]
fn a_changed_declared_file_changes_the_fingerprint() {
    // ADR 0041's whole difference from ADR 0035. An extension that rewrites itself after
    // approval is the attack the record is about, and a fingerprint over the manifest alone
    // would not see it: the manifest here is untouched.
    let made = Made::new();
    let before = read_at(&made.ordinary()).expect("an extension");
    made.file(
        "dark.json",
        r#"{"name":"Solarized Dark","appearance":"dark","tokens":{"x":1}}"#,
    );
    let after = read_at(&made.at()).expect("an extension");

    assert_eq!(
        before.manifest, after.manifest,
        "the manifest did not change"
    );
    assert_ne!(
        before.fingerprint, after.fingerprint,
        "a declared file changed and the fingerprint did not"
    );
}

#[test]
fn a_changed_manifest_changes_the_fingerprint() {
    let made = Made::new();
    let before = read_at(&made.ordinary()).expect("an extension");
    made.manifest(
        r#"{"version":1,"id":"solarized","name":"Solarized II",
            "contributes":{"themes":[{"name":"Solarized Dark","file":"dark.json"}]}}"#,
    );
    let after = read_at(&made.at()).expect("an extension");

    assert_ne!(before.fingerprint, after.fingerprint);
}

#[test]
fn a_program_is_hashed_like_any_other_declared_file() {
    let made = Made::new();
    made.manifest(r#"{"version":1,"id":"x","contributes":{"runs":"bin/run"}}"#);
    made.file("bin/run", "#!/bin/sh\necho one\n");
    let before = read_at(&made.at()).expect("an extension");
    made.file("bin/run", "#!/bin/sh\necho two\n");
    let after = read_at(&made.at()).expect("an extension");

    assert_ne!(
        before.fingerprint, after.fingerprint,
        "the declared program changed and the fingerprint did not"
    );
}

#[test]
fn moving_a_files_bytes_into_its_name_does_not_hash_the_same() {
    // The length framing. Without it, a part named `a` holding `bc` and a part named `ab`
    // holding `c` are the same byte stream — and a fingerprint with a collision in it can be
    // changed under the operator without asking again.
    let one = Made::new();
    one.manifest(r#"{"version":1,"id":"x","contributes":{"themes":[{"file":"ab"}]}}"#);
    one.file("ab", "c");
    let two = Made::new();
    two.manifest(r#"{"version":1,"id":"x","contributes":{"themes":[{"file":"ab"}]}}"#);
    two.file("ab", "c");

    assert_eq!(
        read_at(&one.at()).expect("one").fingerprint,
        read_at(&two.at()).expect("two").fingerprint,
        "the same bytes must hash the same, or nothing below means anything"
    );

    let three = Made::new();
    three.manifest(r#"{"version":1,"id":"x","contributes":{"themes":[{"file":"a"}]}}"#);
    three.file("a", "bc");

    assert_ne!(
        read_at(&one.at()).expect("one").fingerprint,
        read_at(&three.at()).expect("three").fingerprint,
        "a name and its contents ran together"
    );
}

#[test]
fn the_theme_text_handed_on_is_the_text_that_was_hashed() {
    // charter-app#123 one level down. If the window fetched the theme again instead of being
    // handed what the fingerprint was taken over, a write between the two would be drawn
    // without having been consented to — and the fingerprint would go on matching, because it
    // was taken before the write.
    let made = Made::new();
    let found = read_at(&made.ordinary()).expect("an extension");
    let theme = &found.manifest.themes[0];
    let was = found
        .theme_text(theme)
        .expect("the theme's text")
        .to_owned();

    made.file(
        "dark.json",
        r#"{"name":"Swapped","appearance":"light","tokens":{}}"#,
    );

    assert_eq!(
        found.theme_text(theme).expect("still the text it hashed"),
        was,
        "the text moved after the fingerprint was taken"
    );
    assert_ne!(
        read_at(&made.at()).expect("read again").fingerprint,
        found.fingerprint,
        "and the next read must see the change"
    );
}

#[test]
fn a_declared_program_is_hashed_but_its_bytes_are_not_kept() {
    // Nothing consumes them — there is no executor — and holding a program in memory at every
    // launch is a cost with no reader.
    let made = Made::new();
    made.manifest(r#"{"version":1,"id":"x","contributes":{"runs":"bin/x"}}"#);
    made.file("bin/x", "#!/bin/sh\n");
    let found = read_at(&made.at()).expect("an extension");

    assert!(found.theme_text.is_empty(), "{:?}", found.theme_text);
}

// -------------------------------------------------------------------------------------
// The record: every unreadable state means ask, and none of them means clobber
// -------------------------------------------------------------------------------------

#[test]
fn no_record_is_a_machine_with_no_extensions_rather_than_an_error() {
    let made = Made::new();
    let loaded = read(&made.config());

    assert!(loaded.unreadable.is_none(), "{:?}", loaded.unreadable);
    assert!(loaded.registry.entries.is_empty());
}

#[test]
fn installing_writes_a_row_with_no_approval_on_it() {
    // Installing is not consenting. ADR 0041 rejects `charter plugin install <name>` as a
    // consent step for ADR 0022's reason, and the same sentence makes `install` here a thing
    // that records a path and asks nothing.
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");

    let loaded = read(&made.config());
    let entry = loaded.entry("solarized").expect("a row");
    assert_eq!(entry.path, made.at());
    assert_eq!(entry.approved, None, "installing must not approve");
    assert_eq!(loaded.standing(&found), Standing::New);
}

#[test]
fn an_approval_is_of_the_bytes_that_were_shown() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    let asked = prompt(&found, Standing::New);
    approve(&made.config(), &asked.id, &found.path, &asked.fingerprint).expect("approved");

    let loaded = read(&made.config());
    assert_eq!(loaded.standing(&found), Standing::Approved);
    assert!(loaded.standing(&found).may_contribute());
}

#[test]
fn an_extension_that_changed_after_approval_asks_again() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    made.file(
        "dark.json",
        r#"{"name":"Other","appearance":"light","tokens":{}}"#,
    );
    let now = read_at(&made.at()).expect("an extension");

    assert_eq!(read(&made.config()).standing(&now), Standing::Changed);
}

#[test]
fn an_extension_approved_at_one_path_is_not_approved_at_another() {
    // The operator approved a thing at a place. The same bytes in another directory are
    // another extension that kept a name — `machine::still_a_plane`'s reasoning, one level in.
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");

    let moved = Extension {
        path: made.dir.path().join("somewhere-else"),
        ..found
    };
    assert_eq!(read(&made.config()).standing(&moved), Standing::Changed);
}

#[test]
fn an_approval_that_is_not_a_fingerprint_is_refused_rather_than_stored() {
    let made = Made::new();
    let at = made.ordinary();
    for bad in ["", "true", "yes", &"a".repeat(63), &"z".repeat(64)] {
        approve(&made.config(), "solarized", &at, bad).expect_err("not a fingerprint: {bad:?}");
    }
}

#[test]
fn an_approval_that_is_not_a_fingerprint_in_the_file_reads_as_no_approval() {
    // `profiletrust`'s rule: an entry that is not a fingerprint reads as "no record", never as
    // approval. The state this keeps out is a record whose `"approved": true` grants
    // everything.
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    for forged in ["true", "\"yes\"", "1", "{}", "\"abc\""] {
        std::fs::write(
            file(&made.config()),
            format!(
                r#"{{"version":1,"extensions":{{"solarized":{{"path":{},"approved":{forged}}}}}}}"#,
                serde_json::Value::String(made.at().display().to_string())
            ),
        )
        .expect("the record");

        let loaded = read(&made.config());
        assert!(
            loaded.unreadable.is_none(),
            "{forged}: {:?}",
            loaded.unreadable
        );
        assert_eq!(
            loaded.standing(&found),
            Standing::New,
            "{forged} was read as an approval"
        );
    }
}

#[test]
fn a_record_that_is_not_json_approves_nothing_and_says_why() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    std::fs::write(file(&made.config()), "}{ not json").expect("the record");

    let loaded = read(&made.config());
    assert!(loaded.unreadable.is_some());
    assert_eq!(
        loaded.standing(&found),
        Standing::New,
        "silence read as a yes"
    );
}

#[test]
fn a_record_of_another_version_approves_nothing() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    std::fs::write(file(&made.config()), r#"{"version":99,"extensions":{}}"#).expect("the record");

    let loaded = read(&made.config());
    assert!(
        loaded.unreadable.is_some(),
        "another version is not this one"
    );
    assert_eq!(loaded.standing(&found), Standing::New);
}

#[test]
fn a_record_bigger_than_charter_reads_approves_nothing() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    std::fs::write(
        file(&made.config()),
        "x".repeat(MOST_RECORD_BYTES as usize + 1),
    )
    .expect("a giant");

    let loaded = read(&made.config());
    assert!(loaded.unreadable.is_some());
    assert_eq!(loaded.standing(&found), Standing::New);
}

#[cfg(unix)]
#[test]
fn a_record_reached_through_a_symlink_approves_nothing() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    let elsewhere = made.dir.path().join("elsewhere.json");
    std::fs::write(
        &elsewhere,
        format!(
            r#"{{"version":1,"extensions":{{"solarized":{{"path":{},"approved":{}}}}}}}"#,
            serde_json::Value::String(made.at().display().to_string()),
            serde_json::Value::String(found.fingerprint.clone())
        ),
    )
    .expect("a record outside");
    let at = file(&made.config());
    std::fs::remove_file(&at).expect("the real record");
    std::os::unix::fs::symlink(&elsewhere, &at).expect("a link");

    let loaded = read(&made.config());
    assert!(loaded.unreadable.is_some(), "a link was followed");
    assert_eq!(
        loaded.standing(&found),
        Standing::New,
        "an approval was taken from a file charter did not write"
    );
}

#[test]
fn a_record_charter_could_not_read_is_never_overwritten() {
    // `machine::update`'s rule, not `profiletrust`'s. "Could not read" must not become
    // "approved", and it must not become "your list is gone" either.
    let made = Made::new();
    let at = made.ordinary();
    install(&made.config(), &at).expect("installed");
    std::fs::write(file(&made.config()), "}{ not json").expect("the record");

    let why = approve(&made.config(), "solarized", &at, &"a".repeat(64)).expect_err("refused");
    assert_eq!(why.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        std::fs::read_to_string(file(&made.config())).expect("still there"),
        "}{ not json",
        "the operator's record was clobbered"
    );
}

#[test]
fn a_row_with_a_relative_path_is_dropped_with_a_reason_rather_than_read() {
    let made = Made::new();
    std::fs::create_dir_all(crate::machine::dir(&made.config())).expect("the directory");
    std::fs::write(
        file(&made.config()),
        r#"{"version":1,"extensions":{"solarized":{"path":"ext","approved":null}}}"#,
    )
    .expect("the record");

    let loaded = read(&made.config());
    assert!(loaded.registry.entries.is_empty());
    assert_eq!(loaded.dropped.len(), 1, "{:?}", loaded.dropped);
    assert!(
        loaded.dropped[0].contains("absolute"),
        "{:?}",
        loaded.dropped
    );
}

#[test]
fn one_bad_row_does_not_take_the_others_with_it() {
    let made = Made::new();
    std::fs::create_dir_all(crate::machine::dir(&made.config())).expect("the directory");
    std::fs::write(
        file(&made.config()),
        format!(
            r#"{{"version":1,"extensions":{{
                 "bad":{{"path":"relative"}},
                 "good":{{"path":{},"approved":null}}}}}}"#,
            serde_json::Value::String(made.at().display().to_string())
        ),
    )
    .expect("the record");

    let loaded = read(&made.config());
    assert!(loaded.entry("good").is_some(), "{:?}", loaded.registry);
    assert!(loaded.entry("bad").is_none());
}

#[test]
fn forgetting_takes_the_path_and_the_approval_and_leaves_the_rest() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    forget(&made.config(), "solarized").expect("forgotten");

    assert!(read(&made.config()).entry("solarized").is_none());
    assert!(
        made.at().join(MANIFEST).exists(),
        "charter deleted an extension's own files, which are not charter's"
    );
}

#[test]
fn re_installing_something_that_did_not_change_keeps_its_approval() {
    // Otherwise the operator is asked again for pressing the button that was already answered,
    // which is how a prompt becomes a reflex.
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    install(&made.config(), &made.at()).expect("installed again");

    assert_eq!(read(&made.config()).standing(&found), Standing::Approved);
}

#[test]
fn re_installing_something_that_changed_drops_its_approval() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    made.file(
        "dark.json",
        r#"{"name":"Other","appearance":"light","tokens":{}}"#,
    );
    let now = install(&made.config(), &made.at()).expect("installed again");

    assert_eq!(read(&made.config()).standing(&now), Standing::New);
}

// -------------------------------------------------------------------------------------
// Where it lives, and what it is not allowed to be
// -------------------------------------------------------------------------------------

#[test]
fn the_record_is_a_file_beside_the_machine_store_and_not_a_key_inside_it() {
    // charter ADR 0034 is "four things, and nothing else", argued to five by ADR 0040. A sixth
    // key appended without an argument would spend the limit; a separate file in the same
    // directory leaves `machine.json` holding exactly what those records say it holds.
    let made = Made::new();
    install(&made.config(), &made.ordinary()).expect("installed");

    let store = crate::machine::file(&made.config());
    assert_eq!(
        file(&made.config()).parent(),
        store.parent(),
        "the record left charter's own directory"
    );
    assert_ne!(file(&made.config()), store);
    assert!(
        !store.exists(),
        "installing an extension wrote the machine store, which ADR 0034 fences"
    );
}

#[cfg(unix)]
#[test]
fn the_record_is_0600_in_a_0700_directory() {
    use std::os::unix::fs::PermissionsExt;
    let made = Made::new();
    install(&made.config(), &made.ordinary()).expect("installed");

    let mode = |at: &Path| std::fs::metadata(at).expect("there").permissions().mode() & 0o777;
    assert_eq!(mode(&file(&made.config())), 0o600);
    assert_eq!(mode(&crate::machine::dir(&made.config())), 0o700);
}

#[cfg(unix)]
#[test]
fn a_link_at_the_temp_file_the_write_lands_on_is_refused() {
    // The gate is on the path the write ACTUALLY lands on, which is the temp file and not the
    // record. Guarding the destination of a rename while the bytes go somewhere unguarded is
    // the mistake this repo has had six review rounds on, and a test that plants its link at
    // the record's own path would pass against exactly that defect.
    let made = Made::new();
    let dir = crate::machine::dir(&made.config());
    std::fs::create_dir_all(&dir).expect("the directory");
    let outside = made.dir.path().join("outside.json");
    let temp = dir.join("extensions.json.planted.writing");
    std::os::unix::fs::symlink(&outside, &temp).expect("a link at the temp path");

    let why = write_through(&made.config(), &dir.join(RECORD), &temp, b"{}")
        .expect_err("a link at the temp path is refused");
    assert!(
        matches!(
            why.kind(),
            io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists
        ),
        "{why}"
    );
    assert!(
        !outside.exists(),
        "charter wrote through a link out of its own directory"
    );
}

#[cfg(unix)]
#[test]
fn a_link_at_charters_own_directory_is_refused_rather_than_written_through() {
    let made = Made::new();
    let outside = made.dir.path().join("outside");
    std::fs::create_dir_all(&outside).expect("a directory outside");
    std::fs::create_dir_all(made.config()).expect("the config home");
    std::os::unix::fs::symlink(&outside, crate::machine::dir(&made.config()))
        .expect("a link at charter's directory");

    write(&made.config(), &Registry::default()).expect_err("a link is refused");
    assert!(
        !outside.join(RECORD).exists(),
        "charter wrote its record through a link out of the config home"
    );
}

// -------------------------------------------------------------------------------------
// The survey — ADR 0041 item 2's actual question
// -------------------------------------------------------------------------------------

#[test]
fn the_survey_names_charters_own_themes_as_well_as_the_installed_ones() {
    let made = Made::new();
    install(&made.config(), &made.ordinary()).expect("installed");
    let seen = survey(&made.config());

    assert_eq!(seen.built_in_themes, BUILT_IN_THEMES.to_vec());
    assert_eq!(seen.installed.len(), 1);
    assert_eq!(seen.installed[0].id, "solarized");
}

#[test]
fn nothing_is_in_force_until_it_is_approved() {
    let made = Made::new();
    install(&made.config(), &made.ordinary()).expect("installed");

    let seen = survey(&made.config());
    assert!(
        seen.installed[0].themes_in_force().is_empty(),
        "an unapproved extension contributed a theme"
    );

    let found = read_at(&made.at()).expect("an extension");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    let seen = survey(&made.config());
    assert_eq!(seen.installed[0].themes_in_force().len(), 1);
}

#[test]
fn an_extension_that_changed_contributes_nothing_until_it_is_asked_about_again() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    made.file(
        "dark.json",
        r#"{"name":"Other","appearance":"light","tokens":{}}"#,
    );

    let seen = survey(&made.config());
    assert_eq!(seen.installed[0].standing, Standing::Changed);
    assert!(seen.installed[0].themes_in_force().is_empty());
}

#[test]
fn an_extension_charter_cannot_read_is_a_row_that_says_why_rather_than_a_silence() {
    let made = Made::new();
    install(&made.config(), &made.ordinary()).expect("installed");
    std::fs::remove_file(made.at().join(MANIFEST)).expect("the manifest");

    let seen = survey(&made.config());
    assert_eq!(seen.installed.len(), 1);
    assert!(seen.installed[0].refused.is_some(), "it went quiet");
    assert!(seen.installed[0].themes_in_force().is_empty());
}

#[test]
fn a_directory_whose_manifest_now_declares_another_id_reads_as_changed() {
    // Otherwise a row's approval would carry to a directory holding a different extension.
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    made.manifest(
        r#"{"version":1,"id":"somebody-else","name":"Other",
            "contributes":{"themes":[{"name":"Solarized Dark","file":"dark.json"}]}}"#,
    );

    let seen = survey(&made.config());
    assert_eq!(seen.installed[0].standing, Standing::Changed);
    assert!(seen.installed[0].themes_in_force().is_empty());
}

#[test]
fn an_unreadable_record_puts_nothing_in_force() {
    let made = Made::new();
    let found = install(&made.config(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    std::fs::write(file(&made.config()), "}{").expect("the record");

    let seen = survey(&made.config());
    assert!(seen.unreadable.is_some());
    assert!(
        seen.installed.is_empty(),
        "an unreadable record listed extensions"
    );
}

// -------------------------------------------------------------------------------------
// The consent surface, which is the part that has to be honest
// -------------------------------------------------------------------------------------

#[test]
fn the_prompt_says_that_an_extension_runs_as_the_operator_does() {
    // The sentence the operator was shown before he ruled on 2026-09-22 to ship without a
    // sandbox. It is in the core so the window cannot say something kinder, and it is pinned
    // here so that softening it is a deliberate edit to a test rather than a quiet one.
    let made = Made::new();
    let found = read_at(&made.ordinary()).expect("an extension");
    let asked = prompt(&found, Standing::New);

    assert_eq!(asked.runs_as_you, RUNS_AS_YOU);
    assert!(
        asked.runs_as_you.contains("does not confine"),
        "{}",
        asked.runs_as_you
    );
    assert!(
        asked.runs_as_you.contains("runs as you do"),
        "{}",
        asked.runs_as_you
    );
    assert!(
        asked.runs_as_you.contains("vaults"),
        "{}",
        asked.runs_as_you
    );
    assert!(
        asked.runs_as_you.contains("DECLARES") && asked.runs_as_you.contains("LIMITED"),
        "the prompt must say the list is a declaration and not a limit: {}",
        asked.runs_as_you
    );
}

#[test]
fn the_prompt_says_the_fingerprint_is_not_a_boundary() {
    let made = Made::new();
    let found = read_at(&made.ordinary()).expect("an extension");
    let asked = prompt(&found, Standing::New);

    assert_eq!(asked.fingerprint_note, FINGERPRINTED);
    assert!(
        asked.fingerprint_note.contains("not a boundary"),
        "{}",
        asked.fingerprint_note
    );
}

#[test]
fn the_prompt_names_every_contribution_and_carries_back_what_was_shown() {
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"x","name":"Both",
            "contributes":{"themes":[{"name":"Midnight","file":"m.json"}],"runs":"bin/x"}}"#,
    );
    made.file("m.json", "{}");
    made.file("bin/x", "#!/bin/sh\n");
    let found = read_at(&made.at()).expect("an extension");
    let asked = prompt(&found, Standing::New);

    assert_eq!(asked.declares.len(), 2, "{:?}", asked.declares);
    assert!(
        asked.declares[0].contains("Midnight"),
        "{:?}",
        asked.declares
    );
    assert!(asked.declares[1].contains("bin/x"), "{:?}", asked.declares);
    assert_eq!(
        asked.fingerprint, found.fingerprint,
        "the yes must carry back the fingerprint that was shown (charter-app#123)"
    );
}

#[test]
fn a_declared_program_is_named_as_one_this_charter_does_not_start() {
    // An operator who approved an extension under a charter with no executor must not be left
    // believing they approved it running — nor believing they refused it, when the next
    // charter will run it on the same approval.
    let made = Made::new();
    made.manifest(r#"{"version":1,"id":"x","contributes":{"runs":"bin/x"}}"#);
    made.file("bin/x", "#!/bin/sh\n");
    let found = read_at(&made.at()).expect("an extension");

    let said = prompt(&found, Standing::New).declares.join("\n");
    assert!(said.contains("does not start it"), "{said}");
}

#[test]
fn a_re_ask_says_it_is_one() {
    let made = Made::new();
    let found = read_at(&made.ordinary()).expect("an extension");

    assert!(prompt(&found, Standing::New).first);
    assert!(
        !prompt(&found, Standing::Changed).first,
        "a re-ask read as a first ask"
    );
}

#[test]
fn charter_runs_nothing() {
    // ADR 0041's stage 1 is "the registry with NO executor". This is the whole of the claim,
    // and it is pinned as source rather than as behaviour because behaviour cannot prove an
    // absence: a test that watched for a spawned process would pass against a runtime that
    // simply had not been reached.
    let source = include_str!("../extension.rs");
    for spawning in ["Command::new", "std::process::Command", "spawn(", "exec("] {
        assert!(
            !source.contains(spawning),
            "{spawning} is in the registry, which is ADR 0041 stage 1 and has no executor"
        );
    }
}
