//! The registry's guards, each with a test that has been seen to go red without it.
//!
//! ADR 0041's gate item 6 is *"every unreadable state asks, and each of those states
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

    /// An extension contributing one theme and declaring `state` as the one directory charter
    /// does not read.
    fn with_state(&self, state: &str) -> PathBuf {
        self.manifest(&format!(
            r#"{{"version":1,"id":"solarized","name":"Solarized","state":"{state}",
                "contributes":{{"themes":[{{"name":"Solarized Dark","file":"dark.json"}}]}}}}"#
        ));
        self.file(
            "dark.json",
            r#"{"name":"Solarized Dark","appearance":"dark","tokens":{}}"#,
        );
        self.at()
    }

    /// The fingerprint of what is on disk right now, for a test that only cares whether it
    /// moved.
    fn now(&self) -> String {
        read_at(&self.at()).expect("an extension").fingerprint
    }

    #[cfg(unix)]
    fn chmod(&self, name: &str, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(self.at().join(name), std::fs::Permissions::from_mode(mode))
            .expect("the mode");
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
fn a_declared_file_that_walks_up_out_of_the_extension_is_refused_by_the_manifest_check() {
    // Checked here as well as at the open, because this string goes into the fingerprint and
    // onto the screen: the operator must not be shown a path charter has not already said is
    // one.
    //
    // **Asserted on `declarable`'s OWN words, not on the shared ones.** The first version
    // looked for "walks up out of", which is also what `contain::no_link_on_the_way` says a
    // moment later — so it passed with `declarable`'s arm deleted, crediting this check with
    // a refusal the walk had made. Measured: one of four mutations nothing held.
    let made = Made::new();
    made.ordinary();
    made.manifest(
        r#"{"version":1,"id":"x","contributes":{"themes":[{"file":"../../etc/passwd"}]}}"#,
    );

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("names a parent directory"), "{why}");
}

#[test]
fn a_manifest_that_declares_itself_is_refused() {
    // The manifest is already a part of the digest, over the bytes that were parsed. Declaring
    // it as a theme asks for those bytes back as theme text, which the walk does not keep — so
    // the theme would be dropped in silence, which reads as a theme that did nothing wrong.
    let made = Made::new();
    made.ordinary();
    made.manifest(&format!(
        r#"{{"version":1,"id":"x","contributes":{{"themes":[{{"file":"{MANIFEST}"}}]}}}}"#
    ));

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("is the manifest itself"), "{why}");
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

/// An approval is recorded against this digest, so its spelling is pinned to one computed
/// outside the crate: Python's `hashlib.sha256` over each part as a big-endian `u64` name
/// length, the name, a `u64` byte length and the bytes. A bump of `sha2` that respelled it
/// would ask the operator to approve every extension again.
#[test]
fn the_framed_digest_is_the_known_sha256_of_the_framing() {
    let parts: [(&str, &[u8]); 2] = [
        ("manifest.toml", b"name = \"x\"\n"),
        ("bin/run", b"\x00\xff"),
    ];
    assert_eq!(
        digest(&parts),
        "f9a8b19e89b98cb531dc03dc6d52802449a9d6fd6720cb02c754c58b41a6496a"
    );
}

#[test]
fn a_name_and_its_contents_cannot_run_together() {
    // The length framing, driven through `digest` rather than through an extension.
    //
    // **Through `read_at` this cannot be tested, and the first version of this test did not
    // know that.** The manifest is itself a part and it names every file the later parts
    // carry, so two extensions that would collide already differ in the manifest — the test
    // passed with the framing removed, and the sweep caught it as one of four guards nothing
    // held. The framing is defence for the first part the manifest does not name, and this
    // drives the function that does the framing so that dropping it reddens something today.
    let split_one: [(&str, &[u8]); 1] = [("ab", b"c")];
    let split_two: [(&str, &[u8]); 1] = [("a", b"bc")];

    assert_eq!(
        digest(&split_one),
        digest(&split_one),
        "the same parts must hash the same, or nothing below means anything"
    );
    assert_ne!(
        digest(&split_one),
        digest(&split_two),
        "a part's name ran into its contents, so two different part lists hash the same"
    );
}

#[test]
fn the_same_extension_read_twice_has_the_same_fingerprint() {
    let made = Made::new();
    let once = read_at(&made.ordinary()).expect("an extension");
    let twice = read_at(&made.at()).expect("an extension");

    assert_eq!(
        once.fingerprint, twice.fingerprint,
        "the fingerprint is not stable, so nothing else about it means anything"
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
    // Nothing consumes them — the executor starts a program by path, and never from bytes held
    // here — and holding a program in memory at every launch is a cost with no reader.
    let made = Made::new();
    made.manifest(r#"{"version":1,"id":"x","contributes":{"runs":"bin/x"}}"#);
    made.file("bin/x", "#!/bin/sh\n");
    let found = read_at(&made.at()).expect("an extension");

    assert!(found.theme_text.is_empty(), "{:?}", found.theme_text);
}

// -------------------------------------------------------------------------------------
// The fingerprint covers the DIRECTORY (charter-app#152), and the carve-out is one
// directory that cannot hold code
// -------------------------------------------------------------------------------------

#[test]
fn an_undeclared_sibling_changing_changes_the_fingerprint() {
    // **charter-app#152 itself, and the test to read first.** Before the tree hash this file
    // was invisible: the manifest did not name it, so nothing hashed it, and the dialog's
    // "charter has read this extension's files" was false about exactly the kind of file a
    // program loads — a `.dylib` beside it, a script it sources, a config it reads.
    let made = Made::new();
    made.ordinary();
    made.file("helper.dylib", "the bytes the operator approved");
    let before = made.now();

    made.file("helper.dylib", "the bytes nobody was asked about");

    assert_ne!(
        before,
        made.now(),
        "an undeclared sibling changed and the fingerprint did not"
    );
}

#[test]
fn a_file_added_changes_the_fingerprint() {
    // The set of PATHS is hashed, not only the contents of the paths that were there. An
    // extension that gains a file after approval has gained something nobody was asked about,
    // and a hash over contents alone cannot see it arrive.
    let made = Made::new();
    made.ordinary();
    let before = made.now();

    made.file("added.js", "");

    assert_ne!(before, made.now(), "a file appeared and nothing noticed");
}

#[test]
fn a_file_taken_away_changes_the_fingerprint() {
    // The other half of the same property, and the one a content hash gets wrong quietly: a
    // file removed is a change to what the operator approved, not an absence of change.
    let made = Made::new();
    made.ordinary();
    made.file("extra.json", "{}");
    let before = made.now();

    std::fs::remove_file(made.at().join("extra.json")).expect("the file");

    assert_ne!(before, made.now(), "a file went away and nothing noticed");
}

#[test]
fn renaming_a_file_changes_the_fingerprint() {
    // **The sharp version of "the set of paths is hashed", and the one a sweep needed.** A test
    // that only ADDS a file passes against a hash that frames every part under the same name,
    // because the extra part is still an extra part — measured, it survived. A rename changes
    // nothing but the name: same count, same kinds, same bytes. Only a digest that has the path
    // in it can tell the two apart, and `config.json` renamed to `preload.js` is exactly the
    // kind of change that means something.
    let made = Made::new();
    made.ordinary();
    made.file("one.json", "the same bytes either way");
    let before = made.now();

    std::fs::rename(made.at().join("one.json"), made.at().join("two.json")).expect("renamed");

    assert_ne!(
        before,
        made.now(),
        "a file was renamed and the fingerprint did not move"
    );
}

#[test]
fn an_empty_directory_appearing_changes_the_fingerprint() {
    // A directory is a part with no bytes, so it is in the hash by name. Without that, an
    // extension could be reshaped — a tree of empty directories is a shape — with nothing to
    // see.
    let made = Made::new();
    made.ordinary();
    let before = made.now();

    std::fs::create_dir(made.at().join("plugins")).expect("the directory");

    assert_ne!(
        before,
        made.now(),
        "a directory appeared and nothing noticed"
    );
}

#[cfg(unix)]
#[test]
fn making_a_file_runnable_changes_the_fingerprint() {
    // The smallest edit that turns data into something a shell will run, and it does not touch
    // a byte of the file. The executable bit is in the part's NAME for exactly this.
    let made = Made::new();
    made.ordinary();
    made.file("run.sh", "#!/bin/sh\necho one\n");
    made.chmod("run.sh", 0o644);
    let before = made.now();

    made.chmod("run.sh", 0o755);

    assert_ne!(
        before,
        made.now(),
        "a file became runnable and the fingerprint did not move"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_tree_is_hashed_as_a_link_and_never_followed() {
    // **What a symlink IS to the hash**, decided rather than left to whatever `read_dir`
    // happens to do: it is its own kind, its TARGET STRING is what is hashed, and nothing is
    // read through it. So a link that leaves the extension reads nothing out there — the file
    // it points at can change without the fingerprint moving, because that file was never part
    // of the extension — and re-pointing the link is a change to the extension, which it is.
    //
    // Refusing links outright was the alternative and is the wrong one: `node_modules/.bin/`
    // is a tree of them, and "charter refuses my extension over a file I didn't write" is the
    // reaction the operator ruled against.
    let made = Made::new();
    made.ordinary();
    let outside = made.dir.path().join("outside.txt");
    std::fs::write(&outside, "one").expect("a file outside");
    let elsewhere = made.dir.path().join("elsewhere.txt");
    std::fs::write(&elsewhere, "other").expect("another file outside");
    std::os::unix::fs::symlink(&outside, made.at().join("points-out")).expect("a link");
    let before = made.now();

    std::fs::write(&outside, "two").expect("the file outside changed");
    assert_eq!(
        before,
        made.now(),
        "charter followed a link out of the extension and hashed what it found"
    );

    std::fs::remove_file(made.at().join("points-out")).expect("the link");
    std::os::unix::fs::symlink(&elsewhere, made.at().join("points-out")).expect("a link");
    assert_ne!(
        before,
        made.now(),
        "the link was re-pointed and the fingerprint did not move"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_loop_does_not_hang_the_walk() {
    // A loop is only a hang for a walk that goes THROUGH a link. This one records links and
    // descends into nothing but directories it saw itself, so the loop is two ordinary parts.
    // This test hangs, rather than fails, if that stops being true — which is worth more than
    // a passing assertion, because a launch that never finishes is the failure it stands for.
    let made = Made::new();
    made.ordinary();
    std::os::unix::fs::symlink("round", made.at().join("about")).expect("a link");
    std::os::unix::fs::symlink("about", made.at().join("round")).expect("the other link");

    let found = read_at(&made.at()).expect("an extension with a loop in it");

    assert_eq!(found.fingerprint.len(), 64, "{}", found.fingerprint);
}

#[cfg(unix)]
#[test]
fn a_directory_symlinked_into_itself_does_not_walk_for_ever() {
    // The shape that actually recurses: a link at a directory pointing back at its own parent.
    // A walk that followed it would descend `deep/up/deep/up/…` until the budget or the stack
    // ran out.
    let made = Made::new();
    made.ordinary();
    std::fs::create_dir(made.at().join("deep")).expect("the directory");
    std::os::unix::fs::symlink(made.at(), made.at().join("deep/up")).expect("a link back up");

    let found = read_at(&made.at()).expect("an extension");

    assert_eq!(found.fingerprint.len(), 64, "{}", found.fingerprint);
}

#[test]
fn more_files_than_charter_reads_is_refused_with_what_to_do() {
    // A directory walk is attacker-influenced input and a bound is not optional: the read
    // happens on the path that draws the window. The refusal has to say what to do about it,
    // because "too big" with no yardstick is a launch telling the operator nothing.
    let made = Made::new();
    made.ordinary();
    for n in 0..=MOST_TREE_ENTRIES {
        made.file(&format!("f{n}"), "");
    }

    let why = read_at(&made.at()).expect_err("no extension past the bound");
    assert!(why.contains("files and directories"), "{why}");
    assert!(
        why.contains("a directory holding the extension and nothing else"),
        "the refusal must say what to do: {why}"
    );
}

#[test]
fn more_bytes_than_charter_hashes_is_refused_with_what_to_do() {
    // The other bound: one file can be as large as the disk, and the entry count would not
    // see it. Refused on the length the DESCRIPTOR reports, before a byte is read, so a
    // planted giant costs a `stat` rather than a launch.
    let made = Made::new();
    made.ordinary();
    std::fs::write(
        made.at().join("giant"),
        vec![0u8; MOST_TREE_BYTES as usize + 1],
    )
    .expect("a giant");

    let why = read_at(&made.at()).expect_err("no extension past the bound");
    assert!(why.contains("bytes charter would hash"), "{why}");
    assert!(
        why.contains("a directory holding the extension and nothing else"),
        "the refusal must say what to do: {why}"
    );
}

#[cfg(unix)]
#[test]
fn something_that_is_not_a_file_a_directory_or_a_link_is_refused_rather_than_skipped() {
    // Skipping it would be a hole in the tree hash with no name on it: anything charter cannot
    // hash is something that can change without the fingerprint moving.
    let made = Made::new();
    made.ordinary();
    let at = made.at().join("pipe");
    let made_it = crate::forklock::status(std::process::Command::new("mkfifo").arg(&at))
        .expect("mkfifo runs");
    assert!(made_it.success(), "the test needs a fifo to plant");

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(
        why.contains("cannot fingerprint what it cannot read"),
        "{why}"
    );
}

#[test]
fn the_state_directory_is_the_one_place_that_does_not_ask_again() {
    // **The reason the carve-out exists.** An extension that keeps a cache or a log beside
    // itself would otherwise re-prompt at every launch, and a consent dialog people click
    // through is worse than no dialog at all.
    let made = Made::new();
    let at = made.with_state("state");
    let found = install(&made.config(), &BuiltIn::none(), &at).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");

    std::fs::create_dir_all(made.at().join("state/deep")).expect("the state directory");
    std::fs::write(made.at().join("state/log"), "a line\n").expect("a log");
    std::fs::write(made.at().join("state/deep/cache"), "bytes").expect("a cache");

    let now = read_at(&made.at()).expect("an extension");
    assert_eq!(
        read(&made.config(), &BuiltIn::none()).standing(&now),
        Standing::Approved,
        "an extension that wrote its own state re-prompted the operator"
    );
}

#[test]
fn a_state_directory_that_is_not_there_yet_is_not_itself_a_change() {
    // First launch has written nothing. If the exclusion depended on the directory existing,
    // the extension's first write would be the change that asks — which is the same prompt
    // fatigue with one more step in front of it.
    let made = Made::new();
    let at = made.with_state("state");
    let found = install(&made.config(), &BuiltIn::none(), &at).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");

    std::fs::create_dir(made.at().join("state")).expect("the state directory");
    std::fs::write(made.at().join("state/first"), "").expect("its first file");

    let now = read_at(&made.at()).expect("an extension");
    assert_eq!(
        read(&made.config(), &BuiltIn::none()).standing(&now),
        Standing::Approved
    );
}

#[cfg(unix)]
#[test]
fn a_program_in_the_state_directory_is_refused_rather_than_excluded() {
    // **The enforcement the carve-out has to carry.** The state directory is the one place
    // charter does not look, so a `.dylib` or a script there would be code the operator is
    // never asked about — which hands back the whole property the tree hash exists for.
    let made = Made::new();
    made.with_state("state");
    std::fs::create_dir(made.at().join("state")).expect("the state directory");
    made.file("state/helper", "#!/bin/sh\n");
    made.chmod("state/helper", 0o755);

    let why = read_at(&made.at()).expect_err("no extension with a program in its state");
    assert!(why.contains("can be run"), "{why}");
    assert!(
        why.contains("Move it out of 'state'"),
        "the refusal must say what to do: {why}"
    );
}

#[cfg(unix)]
#[test]
fn a_link_in_the_state_directory_is_refused() {
    // A link there names code from anywhere without naming it here, which is the same hole
    // through a different door.
    let made = Made::new();
    made.with_state("state");
    std::fs::create_dir(made.at().join("state")).expect("the state directory");
    std::os::unix::fs::symlink("/bin/sh", made.at().join("state/sh")).expect("a link");

    let why = read_at(&made.at()).expect_err("no extension with a link in its state");
    assert!(
        why.contains("is a symlink inside the state directory"),
        "{why}"
    );
}

#[cfg(unix)]
#[test]
fn a_state_directory_that_is_itself_a_symlink_is_refused() {
    // charter does not read what is in there, so a link at the state directory itself puts
    // "the one place this extension may write" wherever it points — and the dialog names a
    // directory the operator can look in.
    let made = Made::new();
    made.with_state("state");
    let outside = made.dir.path().join("outside");
    std::fs::create_dir(&outside).expect("a directory outside");
    std::os::unix::fs::symlink(&outside, made.at().join("state")).expect("a link");

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("state directory and is a symlink"), "{why}");
}

#[test]
fn a_state_directory_that_is_a_file_is_refused() {
    let made = Made::new();
    made.with_state("state");
    made.file("state", "not a directory");

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("is not a directory"), "{why}");
}

#[test]
fn a_state_directory_that_is_not_one_plain_name_is_refused() {
    // One segment, so the carve-out is visible in a directory listing rather than buried at
    // `build/tmp/state`. A hole the operator cannot see is a hole charter is keeping from them.
    for bad in ["../elsewhere", "build/tmp", "/abs", ".", "..", ""] {
        let made = Made::new();
        made.ordinary();
        made.manifest(&format!(
            r#"{{"version":1,"id":"x","state":{},
                "contributes":{{"themes":[{{"file":"dark.json"}}]}}}}"#,
            serde_json::Value::String(bad.to_owned())
        ));

        let why = read_at(&made.at()).expect_err("no extension for the state {bad:?}");
        assert!(why.contains("state directory"), "{bad:?}: {why}");
    }
}

#[test]
fn a_declared_file_inside_the_state_directory_is_refused() {
    // Otherwise a theme or a program is exempt from the fingerprint by declaration, which is
    // charter-app#152 with the hole moved rather than closed.
    let made = Made::new();
    made.ordinary();
    made.manifest(
        r#"{"version":1,"id":"x","state":"state",
            "contributes":{"themes":[{"file":"state/dark.json"}]}}"#,
    );
    made.file("state/dark.json", "{}");

    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("inside its state directory"), "{why}");
}

#[test]
fn the_state_directory_is_excluded_only_at_the_top() {
    // `state` is one segment and it names the directory beside the manifest. A directory
    // called `state` further down is an ordinary directory and is hashed — otherwise the
    // carve-out would be a name an extension could sprinkle anywhere.
    let made = Made::new();
    made.with_state("state");
    made.file("deep/state/kept", "one");
    let before = made.now();

    made.file("deep/state/kept", "two");

    assert_ne!(
        before,
        made.now(),
        "a directory named 'state' below the top was treated as the carve-out"
    );
}

#[test]
fn the_prompt_says_which_directory_charter_does_not_read() {
    // The exception goes where the operator consents. `FINGERPRINTED` says charter read every
    // file in the directory; if that is true of everything but one directory, the one
    // directory is the operator's to weigh and not a doc comment's to keep.
    let made = Made::new();
    let found = read_at(&made.with_state("cache")).expect("an extension");

    let asked = prompt(&found, Standing::New);
    let said = asked.state_note.expect("a state note");
    assert_eq!(said, state_note("cache"));
    assert!(said.contains("'cache/'"), "{said}");
    assert!(
        said.contains(
            "cannot stop a program it has already read from treating its own data as \
                       code"
        ),
        "the note must not claim more than the check does: {said}"
    );
}

#[test]
fn an_extension_with_no_state_directory_has_no_exception_to_state() {
    let made = Made::new();
    let found = read_at(&made.ordinary()).expect("an extension");

    assert_eq!(prompt(&found, Standing::New).state_note, None);
}

#[test]
fn the_fingerprint_note_says_the_whole_directory_and_not_only_what_is_declared() {
    // The sentence charter-app#152 was opened over. It said "this extension's files", which
    // the operator reads as all of them and which the code meant as the declared ones.
    let made = Made::new();
    let found = read_at(&made.ordinary()).expect("an extension");

    let said = prompt(&found, Standing::New).fingerprint_note;
    assert!(
        said.contains("every file in this extension's directory"),
        "{said}"
    );
    assert!(said.contains("not only the ones it declares"), "{said}");
    assert!(said.contains("added or taken away"), "{said}");
}

#[test]
#[ignore = "a measurement, not a guard: cargo test -p charter-core -- --ignored --nocapture"]
fn what_the_re_hash_costs_at_launch() {
    // charter-app#150's point 6 left this unmeasured and the operator asked for it. The
    // numbers this prints are the ones in #152's PR body; it is here rather than in a
    // benchmark harness so that the next person to widen the hash can re-run it in one
    // command against the code as it actually is.
    for (files, bytes) in [
        // A theme extension as one actually ships: a manifest and a few hundred hex strings.
        (4usize, 16usize << 10),
        (100, 1 << 20),
        (1_000, 50 << 20),
        // At the bound, with room for the sixteen directories and the two declared files.
        (MOST_TREE_ENTRIES - 32, MOST_TREE_BYTES as usize - (1 << 20)),
    ] {
        let made = Made::new();
        made.ordinary();
        let each = bytes / files;
        for n in 0..files {
            let at = made.at().join(format!("d{}/f{n}", n % 16));
            std::fs::create_dir_all(at.parent().expect("a parent")).expect("the directory");
            std::fs::write(&at, vec![b'x'; each]).expect("the file");
        }
        // Once to warm the page cache, which is the state a second launch is in.
        read_at(&made.at()).expect("an extension");
        let began = std::time::Instant::now();
        let rounds = 5;
        for _ in 0..rounds {
            read_at(&made.at()).expect("an extension");
        }
        let each_took = began.elapsed() / rounds;
        println!(
            "{files} files / {} MiB: {each_took:?} per re-hash",
            bytes >> 20
        );
    }
}

// -------------------------------------------------------------------------------------
// The record: every unreadable state means ask, and none of them means clobber
// -------------------------------------------------------------------------------------

#[test]
fn no_record_is_a_machine_with_no_extensions_rather_than_an_error() {
    let made = Made::new();
    let loaded = read(&made.config(), &BuiltIn::none());

    assert!(loaded.unreadable.is_none(), "{:?}", loaded.unreadable);
    assert!(loaded.registry.entries.is_empty());
}

#[test]
fn installing_writes_a_row_with_no_approval_on_it() {
    // Installing is not consenting. ADR 0041 rejects `charter plugin install <name>` as a
    // consent step for ADR 0022's reason, and the same sentence makes `install` here a thing
    // that records a path and asks nothing.
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");

    let loaded = read(&made.config(), &BuiltIn::none());
    let entry = loaded.entry("solarized").expect("a row");
    assert_eq!(entry.path, made.at());
    assert_eq!(entry.approved, None, "installing must not approve");
    assert_eq!(loaded.standing(&found), Standing::New);
}

#[test]
fn an_approval_is_of_the_bytes_that_were_shown() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    let asked = prompt(&found, Standing::New);
    approve(&made.config(), &asked.id, &found.path, &asked.fingerprint).expect("approved");

    let loaded = read(&made.config(), &BuiltIn::none());
    assert_eq!(loaded.standing(&found), Standing::Approved);
    assert!(loaded.standing(&found).may_contribute());
}

#[test]
fn an_extension_that_changed_after_approval_asks_again() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    made.file(
        "dark.json",
        r#"{"name":"Other","appearance":"light","tokens":{}}"#,
    );
    let now = read_at(&made.at()).expect("an extension");

    assert_eq!(
        read(&made.config(), &BuiltIn::none()).standing(&now),
        Standing::Changed
    );
}

#[test]
fn an_extension_approved_at_one_path_is_not_approved_at_another() {
    // The operator approved a thing at a place. The same bytes in another directory are
    // another extension that kept a name — `machine::still_a_plane`'s reasoning, one level in.
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");

    let moved = Extension {
        path: made.dir.path().join("somewhere-else"),
        ..found
    };
    assert_eq!(
        read(&made.config(), &BuiltIn::none()).standing(&moved),
        Standing::Changed
    );
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
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    for forged in ["true", "\"yes\"", "1", "{}", "\"abc\""] {
        std::fs::write(
            file(&made.config()),
            format!(
                r#"{{"version":1,"extensions":{{"solarized":{{"path":{},"approved":{forged}}}}}}}"#,
                serde_json::Value::String(made.at().display().to_string())
            ),
        )
        .expect("the record");

        let loaded = read(&made.config(), &BuiltIn::none());
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
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    std::fs::write(file(&made.config()), "}{ not json").expect("the record");

    let loaded = read(&made.config(), &BuiltIn::none());
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
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    std::fs::write(file(&made.config()), r#"{"version":99,"extensions":{}}"#).expect("the record");

    let loaded = read(&made.config(), &BuiltIn::none());
    assert!(
        loaded.unreadable.is_some(),
        "another version is not this one"
    );
    assert_eq!(loaded.standing(&found), Standing::New);
}

#[test]
fn a_record_bigger_than_charter_reads_approves_nothing() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    std::fs::write(
        file(&made.config()),
        "x".repeat(MOST_RECORD_BYTES as usize + 1),
    )
    .expect("a giant");

    let loaded = read(&made.config(), &BuiltIn::none());
    assert!(loaded.unreadable.is_some());
    assert_eq!(loaded.standing(&found), Standing::New);
}

#[cfg(unix)]
#[test]
fn a_record_reached_through_a_symlink_approves_nothing() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
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

    let loaded = read(&made.config(), &BuiltIn::none());
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
    install(&made.config(), &BuiltIn::none(), &at).expect("installed");
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

    let loaded = read(&made.config(), &BuiltIn::none());
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

    let loaded = read(&made.config(), &BuiltIn::none());
    assert!(loaded.entry("good").is_some(), "{:?}", loaded.registry);
    assert!(loaded.entry("bad").is_none());
}

#[test]
fn forgetting_takes_the_path_and_the_approval_and_leaves_the_rest() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    forget(&made.config(), "solarized").expect("forgotten");

    assert!(
        read(&made.config(), &BuiltIn::none())
            .entry("solarized")
            .is_none()
    );
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
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    install(&made.config(), &BuiltIn::none(), &made.at()).expect("installed again");

    assert_eq!(
        read(&made.config(), &BuiltIn::none()).standing(&found),
        Standing::Approved
    );
}

#[test]
fn re_installing_something_that_changed_drops_its_approval() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    made.file(
        "dark.json",
        r#"{"name":"Other","appearance":"light","tokens":{}}"#,
    );
    let now = install(&made.config(), &BuiltIn::none(), &made.at()).expect("installed again");

    assert_eq!(
        read(&made.config(), &BuiltIn::none()).standing(&now),
        Standing::New
    );
}

// -------------------------------------------------------------------------------------
// Where it lives, and what it is not allowed to be
// -------------------------------------------------------------------------------------

#[test]
fn the_record_is_a_file_beside_the_machine_store_and_not_a_key_inside_it() {
    // ADR 0034 is "four things, and nothing else", argued to five by ADR 0040. A sixth
    // key appended without an argument would spend the limit; a separate file in the same
    // directory leaves `machine.json` holding exactly what those records say it holds.
    let made = Made::new();
    install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");

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
    install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");

    let mode = |at: &Path| std::fs::metadata(at).expect("there").permissions().mode() & 0o777;
    assert_eq!(mode(&file(&made.config())), 0o600);
    assert_eq!(mode(&crate::machine::dir(&made.config())), 0o700);
}

#[cfg(unix)]
#[test]
fn a_link_at_the_temp_file_the_write_lands_on_is_refused_by_the_walk() {
    // The gate is on the path the write ACTUALLY lands on, which is the temp file and not the
    // record. Guarding the destination of a rename while the bytes go somewhere unguarded is
    // the mistake this repo has had six review rounds on, and a test that plants its link at
    // the record's own path would pass against exactly that defect.
    //
    // **`PermissionDenied` exactly, not "either of two kinds".** Two guards refuse this — the
    // containment walk, and `O_CREAT|O_EXCL` on a symlink — and accepting either kind made
    // this test pass with the walk deleted, crediting it with a refusal `O_EXCL` had made.
    // Measured: it was one of four mutations nothing held. The walk answers first and answers
    // `PermissionDenied`; `create_new` answers `AlreadyExists` and has its own test below.
    let made = Made::new();
    let dir = crate::machine::dir(&made.config());
    std::fs::create_dir_all(&dir).expect("the directory");
    let outside = made.dir.path().join("outside.json");
    let temp = dir.join("extensions.json.planted.writing");
    std::os::unix::fs::symlink(&outside, &temp).expect("a link at the temp path");

    let why = write_through(&made.config(), &dir.join(RECORD), &temp, b"{}")
        .expect_err("a link at the temp path is refused");
    assert_eq!(why.kind(), io::ErrorKind::PermissionDenied, "{why}");
    assert!(
        !outside.exists(),
        "charter wrote through a link out of its own directory"
    );
}

#[cfg(unix)]
#[test]
fn a_link_on_the_way_to_the_temp_file_is_refused_where_only_the_walk_can_see_it() {
    // `O_NOFOLLOW` answers for the LAST component and for nothing above it, so a link at an
    // intermediate directory is the walk's alone. Without the walk this write lands in a tree
    // outside the config home and reports success.
    let made = Made::new();
    let dir = crate::machine::dir(&made.config());
    std::fs::create_dir_all(&dir).expect("the directory");
    let outside = made.dir.path().join("outside");
    std::fs::create_dir_all(&outside).expect("a directory outside");
    std::os::unix::fs::symlink(&outside, dir.join("through")).expect("a link on the way");
    let temp = dir.join("through").join("extensions.json.writing");

    let why = write_through(&made.config(), &dir.join(RECORD), &temp, b"{}")
        .expect_err("a link on the way is refused");
    assert_eq!(why.kind(), io::ErrorKind::PermissionDenied, "{why}");
    assert!(
        !outside.join("extensions.json.writing").exists(),
        "charter wrote through a link at a directory the walk was the only guard on"
    );
}

#[test]
fn a_file_already_at_the_temp_path_is_refused_rather_than_written_through() {
    // `create_new` and not `create`+`truncate`, which is what tells this guard apart from the
    // walk: a plain file is not a link, so the walk passes it and only `O_EXCL` refuses. It is
    // the right refusal because a `create_new` that fails failed because something was ALREADY
    // there, and unlinking that something is charter deleting a file it did not make
    // (`machine::write_through`'s rule, and this is the same directory).
    let made = Made::new();
    let dir = crate::machine::dir(&made.config());
    std::fs::create_dir_all(&dir).expect("the directory");
    let temp = dir.join("extensions.json.someone-elses.writing");
    std::fs::write(&temp, "somebody else was here").expect("a file at the temp path");

    let why = write_through(&made.config(), &dir.join(RECORD), &temp, b"{}")
        .expect_err("an existing file at the temp path is refused");
    assert_eq!(why.kind(), io::ErrorKind::AlreadyExists, "{why}");
    assert_eq!(
        std::fs::read_to_string(&temp).expect("still there"),
        "somebody else was here",
        "charter wrote through a file it did not make"
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
    install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    let seen = survey(&made.config(), &BuiltIn::none());

    assert_eq!(seen.built_in_themes, BUILT_IN_THEMES.to_vec());
    assert_eq!(seen.installed.len(), 1);
    assert_eq!(seen.installed[0].id, "solarized");
}

#[test]
fn nothing_is_in_force_until_it_is_approved() {
    let made = Made::new();
    install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");

    let seen = survey(&made.config(), &BuiltIn::none());
    assert!(
        seen.installed[0].themes_in_force().is_empty(),
        "an unapproved extension contributed a theme"
    );

    let found = read_at(&made.at()).expect("an extension");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    let seen = survey(&made.config(), &BuiltIn::none());
    assert_eq!(seen.installed[0].themes_in_force().len(), 1);
}

#[test]
fn a_panel_is_the_second_word_in_the_vocabulary_and_travels_on_the_theme_s_terms() {
    // **The seam, end to end.** A panel is declarative data in the manifest — so it is inside
    // the bytes the fingerprint is taken over, there is no second file for a later read to
    // disagree with, and it is in force on exactly the terms a theme is: nothing until the
    // operator has been shown it and said yes.
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"acme","name":"Acme",
            "contributes":{"panels":[{"id":"reviews","title":"Reviews","order":30,
              "rows":[{"key":"a","text":"Land the contract"}]}]}}"#,
    );
    install(&made.config(), &BuiltIn::none(), &made.at()).expect("installed");

    let seen = survey(&made.config(), &BuiltIn::none());
    assert!(
        seen.installed[0].panels_in_force().is_empty(),
        "an unapproved extension contributed a panel"
    );

    let found = read_at(&made.at()).expect("an extension");
    assert_eq!(
        prompt(&found, Standing::New).declares,
        ["a panel, “Reviews” — 1 row charter draws in the window's side region"],
        "the operator was not shown the panel he is being asked about"
    );

    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    let seen = survey(&made.config(), &BuiltIn::none());
    let [panel] = seen.installed[0].panels_in_force() else {
        panic!("one panel in force")
    };
    assert_eq!(panel.key(), "ext/acme/reviews");
    assert_eq!(panel.order, 30);
}

#[test]
fn an_extension_that_declares_only_a_panel_is_still_a_contribution() {
    // A theme was the whole vocabulary until now, and `declares no contributions` was the
    // refusal for a manifest with none. A panel-only extension must not fall into it — this is
    // the check that the new word was added to that condition and not only to the parser.
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"acme","name":"Acme",
            "contributes":{"panels":[{"id":"p","title":"P"}]}}"#,
    );

    read_at(&made.at()).expect("a panel is a contribution charter can consent to");
}

#[test]
fn a_panel_charter_will_not_read_refuses_the_whole_extension_rather_than_going_quiet() {
    // `read_at`'s rule: every refusal is a state that would otherwise read as "nothing
    // declared", which reads as "safe". An extension whose panel charter dropped would be
    // approved for a contribution the operator saw and would then not have.
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"acme","name":"Acme",
            "contributes":{"themes":[{"name":"T","file":"dark.json"}],
              "panels":[{"id":"p","title":"P",
                "rows":[{"key":"a","text":"t","runs":"workspace.delete:alpha"}]}]}}"#,
    );
    made.file(
        "dark.json",
        r#"{"name":"T","appearance":"dark","tokens":{}}"#,
    );

    let why = read_at(&made.at()).expect_err("a row that runs a charter verb is refused");

    assert!(why.contains(crate::panel::NO_VERB), "{why}");
}

#[test]
fn an_extension_that_changed_contributes_nothing_until_it_is_asked_about_again() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    made.file(
        "dark.json",
        r#"{"name":"Other","appearance":"light","tokens":{}}"#,
    );

    let seen = survey(&made.config(), &BuiltIn::none());
    assert_eq!(seen.installed[0].standing, Standing::Changed);
    assert!(seen.installed[0].themes_in_force().is_empty());
}

#[test]
fn an_extension_charter_cannot_read_is_a_row_that_says_why_rather_than_a_silence() {
    let made = Made::new();
    install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    std::fs::remove_file(made.at().join(MANIFEST)).expect("the manifest");

    let seen = survey(&made.config(), &BuiltIn::none());
    assert_eq!(seen.installed.len(), 1);
    assert!(seen.installed[0].refused.is_some(), "it went quiet");
    assert!(seen.installed[0].themes_in_force().is_empty());
}

#[test]
fn a_directory_whose_manifest_now_declares_another_id_reads_as_changed() {
    // Otherwise a row's approval would carry to a directory holding a different extension.
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    made.manifest(
        r#"{"version":1,"id":"somebody-else","name":"Other",
            "contributes":{"themes":[{"name":"Solarized Dark","file":"dark.json"}]}}"#,
    );

    let seen = survey(&made.config(), &BuiltIn::none());
    assert_eq!(seen.installed[0].standing, Standing::Changed);
    assert!(seen.installed[0].themes_in_force().is_empty());
}

#[test]
fn an_unreadable_record_puts_nothing_in_force() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    std::fs::write(file(&made.config()), "}{").expect("the record");

    let seen = survey(&made.config(), &BuiltIn::none());
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
fn a_declared_program_with_no_view_is_named_as_one_nothing_starts() {
    // A program declared with no view has nothing that would ever ask charter to start it,
    // and the operator is owed that sentence rather than the executor's.
    let made = Made::new();
    made.manifest(r#"{"version":1,"id":"x","contributes":{"runs":"bin/x"}}"#);
    made.file("bin/x", "#!/bin/sh\n");
    let found = read_at(&made.at()).expect("an extension");

    let said = prompt(&found, Standing::New).declares.join("\n");
    assert!(
        said.contains("nothing ever asks charter to start it"),
        "{said}"
    );
}

/// An extension with a program and one view about personas.
fn with_a_view(made: &Made) -> Extension {
    made.manifest(
        r#"{"version":1,"id":"x","contributes":{"runs":"bin/x",
            "views":[{"id":"stats","title":"Statistics","about":"personas"}]}}"#,
    );
    made.file("bin/x", "#!/bin/sh\n");
    read_at(&made.at()).expect("an extension")
}

#[test]
fn a_program_that_answers_a_view_is_named_with_when_charter_starts_it() {
    // ADR 0041's amendment: the prompt says what charter will do with the program, from the
    // core, and that what it does is conduct and not a cage.
    let made = Made::new();
    let asked = prompt(&with_a_view(&made), Standing::New);
    let said = asked.declares.join("\n");

    assert!(said.contains(crate::executor::HOW_IT_RUNS), "{said}");
    assert!(
        said.contains(crate::handed::what(crate::panel::Subject::Personas)),
        "the view does not say what it is handed: {said}"
    );
    assert_eq!(asked.runs_as_you, RUNS_AS_YOU);
}

#[test]
fn a_view_is_read_with_its_title_and_what_it_is_about() {
    let made = Made::new();
    let found = with_a_view(&made);
    assert_eq!(
        found.manifest.views,
        vec![View {
            id: "stats".into(),
            title: "Statistics".into(),
            about: crate::panel::Subject::Personas
        }]
    );
}

#[test]
fn a_view_with_no_program_to_answer_it_is_refused() {
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"x","contributes":{
            "views":[{"id":"stats","title":"Statistics","about":"personas"}]}}"#,
    );
    let refused = read_at(&made.at()).expect_err("a button that can never do anything");
    assert!(refused.contains("no program"), "{refused}");
}

#[test]
fn a_view_about_something_charter_hands_nothing_for_is_refused() {
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"x","contributes":{"runs":"bin/x",
            "views":[{"id":"stats","title":"Statistics","about":"vaults"}]}}"#,
    );
    made.file("bin/x", "#!/bin/sh\n");
    let refused = read_at(&made.at()).expect_err("a view about vaults");
    assert!(refused.contains("\"vaults\""), "{refused}");
}

#[test]
fn a_view_that_says_where_it_goes_is_refused() {
    // ADR 0043 property 3, for the one contribution that runs: charter decides where a view is
    // offered, and a key that tried to decide it is refused by name rather than ignored.
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"x","contributes":{"runs":"bin/x",
            "views":[{"id":"stats","title":"Statistics","about":"personas","region":"left"}]}}"#,
    );
    made.file("bin/x", "#!/bin/sh\n");
    let refused = read_at(&made.at()).expect_err("a view that chose its place");
    assert!(refused.contains("\"region\""), "{refused}");
}

#[test]
fn a_view_title_that_would_turn_the_extension_s_name_around_is_refused() {
    // A view's button and surface draw `<title> · <extension id>`. A right-to-left override at
    // the end of the title would draw that id backwards, and the surface would no longer say
    // honestly which extension it came from. `is_control` does not see it: it is `Cf`.
    for invisible in ["\\u202E", "\\u2067", "\\u200B", "\\uFEFF"] {
        let made = Made::new();
        made.manifest(&format!(
            r#"{{"version":1,"id":"x","contributes":{{"runs":"bin/x",
                "views":[{{"id":"stats","title":"Statistics{invisible}","about":"personas"}}]}}}}"#
        ));
        made.file("bin/x", "#!/bin/sh\n");
        let refused = read_at(&made.at()).expect_err("an invisible character in a title");
        assert!(refused.contains("title"), "{refused}");
    }
}

#[test]
fn an_extension_name_holding_an_invisible_character_is_refused() {
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"x","name":"Stats\u2066x","contributes":{"runs":"bin/x",
        "views":[{"id":"stats","title":"Statistics","about":"personas"}]}}"#,
    );
    made.file("bin/x", "#!/bin/sh\n");
    let refused = read_at(&made.at()).expect_err("an isolate in the consent dialog's name");
    assert!(refused.contains("name"), "{refused}");
}

#[test]
fn two_views_with_one_id_are_refused() {
    let made = Made::new();
    made.manifest(
        r#"{"version":1,"id":"x","contributes":{"runs":"bin/x","views":[
            {"id":"stats","title":"A","about":"personas"},
            {"id":"stats","title":"B","about":"personas"}]}}"#,
    );
    made.file("bin/x", "#!/bin/sh\n");
    let refused = read_at(&made.at()).expect_err("two views, one name");
    assert!(refused.contains("two views"), "{refused}");
}

#[test]
fn a_yes_to_a_program_given_before_charter_started_programs_does_not_cover_running_it() {
    // Until the executor, the prompt said of a program "this charter has no extension runtime
    // and does not start it". A yes to THAT is not a yes to running it, so a program-declaring
    // extension carries the executor's protocol in its fingerprint: the digest over the same
    // bytes with no program declared is a different one, which is what makes every such yes
    // given before this read as changed.
    let made = Made::new();
    let found = with_a_view(&made);
    let bytes = std::fs::read(made.at().join(MANIFEST)).expect("the manifest");
    let mut without = found.manifest.clone();
    without.program = None;
    without.views = Vec::new();

    let (with_it, _) =
        tree(&made.at(), &bytes, &found.manifest, MOST_TREE_ENTRIES).expect("hashed");
    let (without_it, _) = tree(&made.at(), &bytes, &without, MOST_TREE_ENTRIES).expect("hashed");
    assert_ne!(
        with_it, without_it,
        "declaring a program that charter will start did not change what the yes covers"
    );
}

#[test]
fn only_an_approved_extension_offers_its_views() {
    let made = Made::new();
    with_a_view(&made);
    let found = install(&made.config(), &BuiltIn::none(), &made.at()).expect("installed");
    assert!(
        survey(&made.config(), &BuiltIn::none()).installed[0]
            .views_in_force()
            .is_empty(),
        "an unapproved extension offered a view"
    );

    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    assert_eq!(
        survey(&made.config(), &BuiltIn::none()).installed[0]
            .views_in_force()
            .len(),
        1
    );

    made.file("bin/x", "#!/bin/sh\necho changed\n");
    assert!(
        survey(&made.config(), &BuiltIn::none()).installed[0]
            .views_in_force()
            .is_empty(),
        "a changed extension still offered its view"
    );
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
fn the_registry_runs_nothing() {
    // ADR 0041's stage 1 was "the registry with NO executor", and stage 2 kept it that way on
    // purpose: the executor is `crate::executor`, which ASKS this module, and the module that
    // decides what a yes covers has no way to run anything — so a defect here can only ever
    // be a refusal, never an execution. Pinned as source rather than as behaviour because
    // behaviour cannot prove an absence.
    let source = include_str!("../extension.rs");
    for spawning in ["Command::new", "std::process::Command", "spawn(", "exec("] {
        assert!(
            !source.contains(spawning),
            "{spawning} is in the registry; the executor is crate::executor, and only it starts \
             anything"
        );
    }
}

#[test]
fn the_executor_is_the_only_thing_in_the_core_that_starts_an_extension_s_program() {
    // The other half of the claim above: an extension's program has one way to start, and it
    // is the one with the gate in it. Any other module that learned where an extension's
    // program is would be a second door.
    for (name, source) in [
        ("panel.rs", include_str!("../panel.rs")),
        ("handed.rs", include_str!("../handed.rs")),
        ("machine.rs", include_str!("../machine.rs")),
    ] {
        assert!(
            !source.contains("manifest.program"),
            "{name} reads an extension's program; only executor.rs may"
        );
    }
    assert!(include_str!("../executor.rs").contains("fn cleared("));
}

// -------------------------------------------------------------------------------------
// Settings a project may choose (charter-app#253)
// -------------------------------------------------------------------------------------

/// A view extension declaring `settings`, with its program on disk.
fn with_settings(made: &Made, settings: &str) -> Result<Extension, String> {
    made.manifest(&format!(
        r#"{{"version":1,"id":"stats","name":"Stats","settings":{settings},
            "contributes":{{"runs":"run","views":[{{"id":"s","title":"S","about":"personas"}}]}}}}"#
    ));
    made.file("run", "#!/bin/sh\n");
    read_at(&made.at())
}

#[test]
fn an_extension_declares_the_settings_a_project_may_choose() {
    let made = Made::new();
    let found = with_settings(
        &made,
        r#"[{"key":"window","title":"Window","type":"choice","choices":["7d","30d"],"default":"30d"},
            {"key":"compact","type":"bool"}]"#,
    )
    .expect("read");
    assert_eq!(
        found.manifest.settings,
        vec![
            Setting {
                key: "window".into(),
                title: "Window".into(),
                kind: SettingKind::Choice(vec!["7d".into(), "30d".into()]),
                default: SettingValue::Text("30d".into()),
            },
            Setting {
                key: "compact".into(),
                title: "compact".into(),
                kind: SettingKind::Bool,
                default: SettingValue::Bool(false),
            },
        ]
    );
    assert!(
        prompt(&found, Standing::New)
            .declares
            .contains(&"settings a project may choose, handed to its program with each question: Window, compact".to_owned()),
        "the operator was not shown what a project can hand the program"
    );
}

#[test]
fn a_setting_charter_could_not_honour_refuses_the_extension() {
    for (settings, why) in [
        (r#"[{"key":"../x","type":"bool"}]"#, "a setting's key is"),
        (r#"[{"key":"a","type":"number"}]"#, "no type charter knows"),
        (
            r#"[{"key":"a","type":"choice"}]"#,
            "without a list of words",
        ),
        (
            r#"[{"key":"a","type":"choice","choices":["x"],"default":"y"}]"#,
            "a default it would not accept",
        ),
        (
            r#"[{"key":"a","type":"bool"},{"key":"a","type":"text"}]"#,
            "two settings called",
        ),
        (
            r#"[{"key":"a","type":"bool","colour":"red"}]"#,
            "not part of what a setting may say",
        ),
    ] {
        let made = Made::new();
        let refused = with_settings(&made, settings).expect_err(settings);
        assert!(refused.contains(why), "{settings}: {refused}");
    }
}

#[test]
fn settings_with_no_program_to_hand_them_to_are_refused() {
    let made = Made::new();
    made.ordinary();
    made.manifest(
        r#"{"version":1,"id":"solarized","settings":[{"key":"a","type":"bool"}],
            "contributes":{"themes":[{"name":"Solarized Dark","file":"dark.json"}]}}"#,
    );
    let refused = read_at(&made.at()).expect_err("it read");
    assert!(
        refused.contains("no program ('runs') to hand them to"),
        "{refused}"
    );
}

// ---- a view's edges: exactly at a bound is offered, one past it is not (#311) ----

fn views(json: serde_json::Value) -> Result<Vec<View>, String> {
    views_of(&json, Some("bin/x"))
}

fn a_view(id: &str, title: &str) -> serde_json::Value {
    serde_json::json!({ "id": id, "title": title, "about": "personas" })
}

#[test]
fn exactly_the_most_views_are_offered_and_one_more_is_refused() {
    let listed: Vec<_> = (0..MOST_VIEWS)
        .map(|n| a_view(&format!("v{n}"), "V"))
        .collect();
    assert_eq!(
        views(serde_json::Value::Array(listed.clone()))
            .expect("exactly the most views")
            .len(),
        MOST_VIEWS
    );

    let mut more = listed;
    more.push(a_view("one-more", "V"));
    let refused = views(serde_json::Value::Array(more)).expect_err("one view too many");
    assert!(
        refused.contains(&format!("at most {MOST_VIEWS}")),
        "{refused}"
    );

    assert!(views(serde_json::json!([])).unwrap().is_empty());
}

#[test]
fn a_view_id_may_hold_dashes_and_underscores_after_its_first_letter() {
    for id in ["a-b_c", "9lives", "x_", "y-"] {
        let got = views(serde_json::json!([a_view(id, "V")]))
            .unwrap_or_else(|why| panic!("{id:?} is a view id: {why}"));
        assert_eq!(got[0].id, id);
    }
    // One segment each, so only the charset or the first character refuses it.
    for id in ["a.b", "a b", "_under", "-lead", "é", "", ".."] {
        let refused = views(serde_json::json!([a_view(id, "V")])).expect_err(id);
        assert!(refused.contains("view id"), "{id:?}: {refused}");
    }
}

#[test]
fn a_view_title_of_exactly_two_hundred_bytes_is_drawn_and_one_more_is_not() {
    let longest = "t".repeat(200);
    let got = views(serde_json::json!([a_view("v", &longest)])).expect("200 bytes");
    assert_eq!(got[0].title, longest);

    for title in ["t".repeat(201), "t".repeat(300), "a\u{7}b".to_owned()] {
        let refused = views(serde_json::json!([a_view("v", &title)])).expect_err("refused");
        assert!(refused.contains("will not draw on a button"), "{refused}");
    }
}

// ---------------------------------------------------------------------------------------
// Capabilities (ADR 0053)
// ---------------------------------------------------------------------------------------

#[test]
fn a_manifest_in_the_format_before_capabilities_loads_as_it_did_and_keeps_its_approval() {
    // No `capabilities`, `version` 1: every manifest written before the list existed. It asks
    // for nothing, and its fingerprint is the one it had, so no operator is asked again.
    let made = Made::new();
    made.ordinary();
    let found = read_at(&made.at()).expect("an extension");

    assert!(found.manifest.capabilities.is_empty());
    assert_eq!(found.manifest.protocol, 1);
    assert_eq!(
        found.fingerprint, "856644aa9b9df7019cc8a4f85ebd2111df9f0b8d76c2fe979522e839d74cc1bf",
        "the fingerprint of a manifest in today's format moved, which re-asks every operator"
    );
}

#[test]
fn a_capabilities_that_is_not_a_list_of_distinct_words_is_refused() {
    let made = Made::new();
    made.ordinary();
    for (capabilities, said) in [
        (r#""probe""#, "is not a list of words"),
        ("[1]", "is not a word"),
        (r#"["probe","probe"]"#, "\"probe\" twice"),
    ] {
        made.manifest(&format!(
            r#"{{"version":1,"id":"solarized","capabilities":{capabilities},
                "contributes":{{"themes":[{{"name":"Solarized Dark","file":"dark.json"}}]}}}}"#
        ));
        let why = read_at(&made.at()).expect_err("no extension");
        assert!(why.contains(said), "{capabilities}: {why}");
    }
}

#[test]
fn an_unknown_capability_is_named_in_the_refusal_even_beside_a_known_one() {
    let made = Made::new();
    made.ordinary();
    made.manifest(
        r#"{"version":1,"id":"solarized","capabilities":["probe","badges"],
            "contributes":{"themes":[{"name":"Solarized Dark","file":"dark.json"}]}}"#,
    );
    let why = read_at(&made.at()).expect_err("no extension");
    assert!(
        why.contains("asks for the capability \"badges\", which this charter does not know"),
        "{why}"
    );
    assert!(why.contains("This charter knows probe."), "{why}");
}

#[test]
fn a_version_below_the_first_protocol_is_refused() {
    let made = Made::new();
    made.ordinary();
    made.manifest(r#"{"version":0,"id":"x","contributes":{"runs":"p"}}"#);
    let why = read_at(&made.at()).expect_err("no extension");
    assert!(why.contains("is version 0"), "{why}");
}

// ---------------------------------------------------------------------------------------
// Built-in extensions (charter-app#339)
// ---------------------------------------------------------------------------------------

#[test]
fn turning_a_built_in_off_keeps_every_installed_extension_and_its_yes() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");

    turn_on(&made.config(), "persona-statistics", false).expect("turned off");

    let loaded = read(&made.config(), &BuiltIn::none());
    assert_eq!(loaded.standing(&found), Standing::Approved);
    assert!(loaded.registry.off.contains("persona-statistics"));
    // A built-in this charter does not ship is not an entry: the choice is kept, and nothing
    // is listed for it.
    assert!(loaded.entry("persona-statistics").is_none());

    turn_on(&made.config(), "persona-statistics", true).expect("turned on");
    let loaded = read(&made.config(), &BuiltIn::none());
    assert!(loaded.registry.off.is_empty());
    assert_eq!(loaded.standing(&found), Standing::Approved);
}

#[test]
fn a_record_row_that_names_the_app_as_its_source_grants_nothing() {
    let made = Made::new();
    let at = made.ordinary();
    let found = read_at(&at).expect("reads");
    std::fs::create_dir_all(made.config().join("charter")).expect("a config dir");
    std::fs::write(
        file(&made.config()),
        format!(
            r#"{{"version":1,"extensions":{{"solarized":{{"source":"app","path":"{}","approved":"{}"}}}}}}"#,
            at.display(),
            found.fingerprint
        ),
    )
    .expect("a forged record");

    let loaded = read(&made.config(), &BuiltIn::none());
    assert_eq!(loaded.standing(&found), Standing::New);
    assert!(
        survey(&made.config(), &BuiltIn::none())
            .installed
            .is_empty()
    );
}

#[test]
fn a_built_in_s_installed_namesake_is_set_aside_and_said() {
    let made = Made::new();
    let found = install(&made.config(), &BuiltIn::none(), &made.ordinary()).expect("installed");
    approve(&made.config(), found.id(), &found.path, &found.fingerprint).expect("approved");
    // The app now ships an extension with the same id.
    let bundle = made.dir.path().join("bundle");
    let shipped = bundle.join("solarized");
    std::fs::create_dir_all(&shipped).expect("a bundle");
    for name in [MANIFEST, "dark.json"] {
        std::fs::copy(made.at().join(name), shipped.join(name)).expect("a copy");
    }

    let loaded = read(&made.config(), &BuiltIn::at(bundle));
    let entry = loaded.entry("solarized").expect("the built-in");
    assert_eq!(entry.source, Source::App);
    assert_eq!(entry.path, shipped);
    assert_eq!(loaded.standing(&found), Standing::New);
    assert!(
        loaded
            .dropped
            .iter()
            .any(|why| why.contains("ships this extension itself")),
        "{:?}",
        loaded.dropped
    );
}
