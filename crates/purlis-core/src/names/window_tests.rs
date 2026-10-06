use super::*;
use std::fs;

/// A plane holding the names `old`, `new`, both or neither, as files and a state folder.
struct Plane {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

#[derive(Clone, Copy)]
enum Has {
    Old,
    New,
    Both,
    Neither,
}

fn plane(has: Has) -> Plane {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let (old, new) = match has {
        Has::Old => (true, false),
        Has::New => (false, true),
        Has::Both => (true, true),
        Has::Neither => (false, false),
    };
    for (on, file, dir) in [
        (old, "charter.toml", ".charter"),
        (new, "purlis.toml", ".purlis"),
    ] {
        if on {
            fs::write(root.join(file), "[plane]\n").unwrap();
            fs::create_dir(root.join(dir)).unwrap();
        }
    }
    for (on, local, allow) in [
        (old, "charter.local.toml", ".charter-scan-allow.toml"),
        (new, "purlis.local.toml", ".purlis-scan-allow.toml"),
    ] {
        if on {
            fs::write(root.join(local), "").unwrap();
            fs::write(root.join(allow), "").unwrap();
        }
    }
    Plane { _tmp: tmp, root }
}

#[test]
fn a_plane_with_only_old_names_reads_and_writes_the_old_names() {
    let p = plane(Has::Old);
    assert_eq!(manifest(&p.root), p.root.join("charter.toml"));
    assert_eq!(local_settings(&p.root), p.root.join("charter.local.toml"));
    assert_eq!(scan_allow(&p.root), p.root.join(".charter-scan-allow.toml"));
    assert_eq!(state(&p.root), p.root.join(".charter"));
    assert!(has_manifest(&p.root));
}

#[test]
fn a_plane_with_only_purlis_names_reads_and_writes_the_purlis_names() {
    let p = plane(Has::New);
    assert_eq!(manifest(&p.root), p.root.join("purlis.toml"));
    assert_eq!(local_settings(&p.root), p.root.join("purlis.local.toml"));
    assert_eq!(scan_allow(&p.root), p.root.join(".purlis-scan-allow.toml"));
    assert_eq!(state(&p.root), p.root.join(".purlis"));
    assert!(has_manifest(&p.root));
}

#[test]
fn a_plane_with_both_uses_the_purlis_files_and_the_old_ones_are_leftovers() {
    let p = plane(Has::Both);
    assert_eq!(manifest(&p.root), p.root.join("purlis.toml"));
    assert_eq!(local_settings(&p.root), p.root.join("purlis.local.toml"));
    assert_eq!(scan_allow(&p.root), p.root.join(".purlis-scan-allow.toml"));
    for (name, old) in [
        (PLANE_MANIFEST, "charter.toml"),
        (LOCAL_SETTINGS, "charter.local.toml"),
        (SCAN_ALLOW, ".charter-scan-allow.toml"),
    ] {
        assert_eq!(name.file_in(&p.root).leftovers, vec![p.root.join(old)]);
    }
    assert_eq!(
        STATE_DIR.dir_in(&p.root).leftovers,
        vec![p.root.join(".charter")]
    );
}

#[test]
fn a_plane_with_neither_keeps_the_old_names_until_it_is_migrated() {
    // Nothing is there yet: the first write starts the file an unmigrated plane has always had,
    // so it never sits beside one an older build still reads.
    let p = plane(Has::Neither);
    assert_eq!(manifest(&p.root), p.root.join("charter.toml"));
    assert_eq!(local_settings(&p.root), p.root.join("charter.local.toml"));
    assert_eq!(scan_allow(&p.root), p.root.join(".charter-scan-allow.toml"));
    assert_eq!(state(&p.root), p.root.join(".charter"));
    assert!(!has_manifest(&p.root));
}

#[test]
fn a_write_lands_in_the_file_that_is_there_and_never_starts_a_second() {
    for (has, wrote, untouched) in [
        (Has::Old, "charter.local.toml", "purlis.local.toml"),
        (Has::New, "purlis.local.toml", "charter.local.toml"),
    ] {
        let p = plane(has);
        fs::write(local_settings(&p.root), "[harness]\n").unwrap();
        fs::create_dir_all(state(&p.root).join("app")).unwrap();
        fs::write(state(&p.root).join("app/reopen.json"), "{}").unwrap();

        assert_eq!(
            fs::read_to_string(p.root.join(wrote)).unwrap(),
            "[harness]\n"
        );
        assert!(!p.root.join(untouched).exists(), "{untouched} was created");
        let other = if wrote.starts_with("purlis") {
            ".charter"
        } else {
            ".purlis"
        };
        assert!(!p.root.join(other).exists(), "{other}/ was created");
    }
}

#[test]
fn a_folder_of_a_file_name_and_a_file_of_a_folder_name_count_as_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir(root.join("purlis.toml")).unwrap();
    fs::write(root.join("charter.toml"), "").unwrap();
    fs::write(root.join(".purlis"), "").unwrap();
    assert_eq!(manifest(root), root.join("charter.toml"));
    assert_eq!(state(root), root.join(".charter"));
}

#[test]
fn a_guard_recognises_every_spelling_and_nothing_else() {
    assert!(STATE_DIR.is(".charter"));
    assert!(STATE_DIR.is(".purlis"));
    assert!(!STATE_DIR.is(".git"));
    assert!(SCAN_ALLOW.is(".purlis-scan-allow.toml"));
    assert!(SCAN_ALLOW.is(".charter-scan-allow.toml"));
    assert!(!SCAN_ALLOW.is("sub/.charter-scan-allow.toml"));
}

#[test]
fn a_guard_matching_text_knows_every_spelling_of_the_state_folder() {
    assert_eq!(state_alternation(), "purlis|charter");
}

#[test]
fn every_state_file_follows_the_state_folder_the_plane_has() {
    for (has, folder) in [
        (Has::Old, ".charter"),
        (Has::New, ".purlis"),
        (Has::Neither, ".charter"),
    ] {
        let p = plane(has);
        let state = p.root.join(folder);
        for path in [
            crate::reopen::path(&p.root),
            crate::usage::sessions_dir(&p.root),
            crate::cistate::cache(&p.root),
            crate::glrefresh::lock(&p.root),
            crate::sandbox::local::path(&p.root),
            crate::wscmd::rename::journal_path(&p.root),
        ] {
            assert!(path.starts_with(&state), "{}", path.display());
        }
    }
}

#[test]
fn a_plane_known_only_by_purlis_toml_is_read_by_every_reader_of_the_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    fs::write(
        root.join("purlis.toml"),
        "schema = 1\n[charter]\nversion = \"9.9.9\"\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n",
    )
    .unwrap();
    // An old file beside it is a leftover, and nothing reads it.
    fs::write(root.join("charter.toml"), "not toml [").unwrap();

    assert_eq!(
        crate::adopt::locked_version(&root).as_deref(),
        Some("9.9.9")
    );
    let cfg = crate::forge::load_config(&root).expect("the purlis file is read");
    assert!(cfg.contains_key("forge"), "{cfg:?}");
    // The leftover does not parse; the format gate reads the purlis file, so it is writable.
    assert!(matches!(
        crate::compat::read(&root),
        crate::compat::Compat::Writable
    ));
}

// ---- the state folder is not moved by a folder appearing (D-RN2a-7) ------------------ //

#[test]
fn a_planted_purlis_folder_beside_charter_does_not_move_the_state_without_a_record() {
    let p = plane(Has::Old);
    // What anything able to make a folder in the project root could do.
    fs::create_dir(p.root.join(".purlis")).unwrap();

    assert_eq!(state_name_with(&p.root, |_| false), ".charter");
    // The doctor's `renamed leftovers` row still names both.
    let flagged = crate::names::leftovers_in_plane(&p.root);
    assert!(
        flagged
            .iter()
            .any(|at| at.leftovers.contains(&p.root.join(".charter"))
                && at.path() == p.root.join(".purlis")),
        "{flagged:?}"
    );
}

#[test]
fn with_both_folders_the_rename_local_record_says_which_is_the_state() {
    let p = plane(Has::Both);
    let mut asked = None;
    assert_eq!(
        state_name_with(&p.root, |dir| {
            asked = Some(dir.to_path_buf());
            true
        }),
        ".purlis"
    );
    assert_eq!(asked.as_deref(), Some(p.root.as_path()));
    assert_eq!(state_name_with(&p.root, |_| false), ".charter");
}

#[test]
fn the_record_is_asked_only_when_both_folders_are_there() {
    for has in [Has::Old, Has::New, Has::Neither] {
        let p = plane(has);
        state_name_with(&p.root, |_| {
            panic!("asked the record with one folder or none")
        });
    }
    // Only `.purlis/`: rename-local moved `.charter/` away, so it is the state.
    let p = plane(Has::New);
    assert_eq!(state(&p.root), p.root.join(".purlis"));
}

#[test]
fn the_record_names_a_project_by_its_root_one_line_each() {
    let tmp = tempfile::tempdir().unwrap();
    let record = tmp.path().join("state-moved");
    let p = plane(Has::Both);
    assert!(!listed_in(&record, &p.root), "no record");
    fs::write(&record, "/somewhere/else\n").unwrap();
    assert!(!listed_in(&record, &p.root));
    fs::write(&record, format!("/somewhere/else\n{}\n", p.root.display())).unwrap();
    assert!(listed_in(&record, &p.root));
}

#[test]
fn a_recorded_path_under_either_state_spelling_is_read_under_the_state_folder_there_now() {
    for (has, folder) in [(Has::Old, ".charter"), (Has::New, ".purlis")] {
        let p = plane(has);
        for recorded in [
            ".charter/vaults/app.json",
            ".purlis/vaults/app.json",
            "./.charter/vaults/app.json",
        ] {
            assert_eq!(
                under_state(&p.root, Path::new(recorded)),
                p.root.join(folder).join("vaults/app.json"),
                "{recorded}"
            );
        }
        // Anything else is joined as recorded: another folder, a name that only starts like
        // the state folder, and an absolute path.
        for recorded in ["secrets/app.json", ".charterx/app.json", "app.json"] {
            assert_eq!(
                under_state(&p.root, Path::new(recorded)),
                p.root.join(recorded)
            );
        }
        assert_eq!(
            under_state(&p.root, Path::new("/abs/.charter/app.json")),
            PathBuf::from("/abs/.charter/app.json")
        );
    }
}

#[test]
fn a_path_under_the_state_folder_is_recorded_in_the_old_spelling_during_the_window() {
    for path in [
        ".purlis/vaults/app.json",
        ".charter/vaults/app.json",
        "./.purlis/vaults/app.json",
    ] {
        assert_eq!(
            recorded_under_state(Path::new(path)),
            PathBuf::from(".charter/vaults/app.json"),
            "{path}"
        );
    }
    for path in [
        "secrets/app.json",
        ".purlisx/app.json",
        "/abs/.purlis/app.json",
    ] {
        assert_eq!(recorded_under_state(Path::new(path)), PathBuf::from(path));
    }
}
