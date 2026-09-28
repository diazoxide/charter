//! Editing a memory in place, and archiving and unarchiving one (SI-9a, ADR 0065): the core
//! operations the window's memory tab and `charter … edit|archive|unarchive` both call, in all
//! three stores — a workspace's journal, a persona's memory and `_shared`.

use charter_core::memstore::{Base, EditRefused, TITLE_MAX};
use charter_core::personas::Persona;
use charter_core::workspaces::{Plane, Workspace};

fn plane(tmp: &tempfile::TempDir) -> Plane {
    std::fs::write(tmp.path().join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("workspaces/alpha")).unwrap();
    std::fs::create_dir_all(tmp.path().join("personas/devops/memory")).unwrap();
    std::fs::create_dir_all(tmp.path().join("personas/_shared/memory")).unwrap();
    Plane::open(tmp.path())
}

fn alpha(tmp: &tempfile::TempDir) -> Workspace {
    plane(tmp).workspace("alpha").unwrap()
}

fn at() -> chrono::NaiveDateTime {
    "2026-03-02T09:14:00".parse().unwrap()
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// The index's link lines, without the header.
fn lines(index: &std::path::Path) -> Vec<String> {
    read(index)
        .lines()
        .filter(|l| l.starts_with("- ["))
        .map(str::to_string)
        .collect()
}

fn slug_of(path: &std::path::Path) -> String {
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

// --- edit -----------------------------------------------------------------------------------

#[test]
fn an_edit_that_retitles_a_memory_keeps_its_filename() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("The API returns 418 on Mondays", at()).unwrap();
    let opened = ws.open_memory(&slug_of(&path)).unwrap();

    let edited = ws
        .edit_memory(
            &slug_of(&path),
            "The API returns 418 on Tuesdays",
            "It moved a day.",
            Base::Read(&opened.text),
        )
        .unwrap();

    assert_eq!(edited, path, "the slug is how every command names it");
    let names: Vec<String> = std::fs::read_dir(ws.dir().join("memory"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != "MEMORY.md")
        .collect();
    assert_eq!(names, ["20260302-091400-the-api-returns-418-on-mondays.md"]);
}

#[test]
fn an_edit_rewrites_the_heading_and_body_and_keeps_the_stamp_line_verbatim() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("Old title\nold body", at()).unwrap();
    let opened = ws.open_memory(&slug_of(&path)).unwrap();

    ws.edit_memory(
        &slug_of(&path),
        "  New title  ",
        "\nnew body\n\nsecond paragraph\n",
        Base::Read(&opened.text),
    )
    .unwrap();

    assert_eq!(
        read(&path),
        "# New title\n\n_2026-03-02 09:14 · persistent_\n\nnew body\n\nsecond paragraph\n"
    );
}

#[test]
fn an_edit_retitles_the_memorys_index_line_where_it_stands() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let first = ws.remember("First fact", at()).unwrap();
    let second = ws.remember("Second fact", at()).unwrap();
    let third = ws.remember("Third fact", at()).unwrap();
    let opened = ws.open_memory(&slug_of(&second)).unwrap();

    ws.edit_memory(
        &slug_of(&second),
        "Second fact, corrected",
        "Second fact, corrected",
        Base::Read(&opened.text),
    )
    .unwrap();

    let name = |p: &std::path::Path| p.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        lines(&ws.dir().join("memory/MEMORY.md")),
        [
            format!("- [First fact]({})", name(&first)),
            format!("- [Second fact, corrected]({})", name(&second)),
            format!("- [Third fact]({})", name(&third)),
        ]
    );
}

#[test]
fn a_retitle_keeps_what_a_hand_wrote_after_the_link() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("A fact", at()).unwrap();
    let index = ws.dir().join("memory/MEMORY.md");
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let text = read(&index).replace(
        &format!("- [A fact]({name})"),
        &format!("- [A fact]({name}) — see also the runbook"),
    );
    std::fs::write(&index, text).unwrap();

    ws.edit_memory(&slug_of(&path), "A better fact", "body", Base::Overwrite)
        .unwrap();

    assert_eq!(
        lines(&index),
        [format!("- [A better fact]({name}) — see also the runbook")]
    );
}

#[test]
fn an_edit_of_a_file_that_changed_since_it_was_read_is_refused_as_stale() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("A fact", at()).unwrap();
    let opened = ws.open_memory(&slug_of(&path)).unwrap();
    let theirs = "# A fact\n\n_2026-03-02 09:14 · persistent_\n\nSomebody else's words\n";
    std::fs::write(&path, theirs).unwrap();
    let index_before = read(&ws.dir().join("memory/MEMORY.md"));

    let refused = ws
        .edit_memory(
            &slug_of(&path),
            "Mine",
            "My words",
            Base::Read(&opened.text),
        )
        .unwrap_err();

    assert!(matches!(refused, EditRefused::Stale), "{refused}");
    assert_eq!(read(&path), theirs, "nothing was written over their change");
    assert_eq!(read(&ws.dir().join("memory/MEMORY.md")), index_before);
}

#[test]
fn an_overwrite_skips_the_staleness_check() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("A fact", at()).unwrap();
    std::fs::write(
        &path,
        "# A fact\n\n_2026-03-02 09:14 · persistent_\n\nSomebody else's words\n",
    )
    .unwrap();

    ws.edit_memory(&slug_of(&path), "Mine", "My words", Base::Overwrite)
        .unwrap();

    assert_eq!(
        read(&path),
        "# Mine\n\n_2026-03-02 09:14 · persistent_\n\nMy words\n"
    );
}

#[test]
fn an_edited_title_is_capped_at_title_max() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("A fact", at()).unwrap();
    let long = "x".repeat(TITLE_MAX + 20);

    ws.edit_memory(&slug_of(&path), &long, "body", Base::Overwrite)
        .unwrap();

    let capped = "x".repeat(TITLE_MAX);
    assert!(read(&path).starts_with(&format!("# {capped}\n\n")));
    assert_eq!(
        ws.memories().unwrap()[0].title,
        capped,
        "the heading and the index agree"
    );
    assert!(lines(&ws.dir().join("memory/MEMORY.md"))[0].starts_with(&format!("- [{capped}](")));
}

#[test]
fn an_empty_title_is_the_texts_first_line_as_a_new_memorys_is() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("A fact", at()).unwrap();

    ws.edit_memory(
        &slug_of(&path),
        "   ",
        "Derived here\nand more",
        Base::Overwrite,
    )
    .unwrap();

    assert!(read(&path).starts_with("# Derived here\n\n"));
}

#[test]
fn a_title_with_a_line_break_is_refused_and_nothing_changes() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("A fact", at()).unwrap();
    let before = read(&path);

    let refused = ws
        .edit_memory(&slug_of(&path), "two\nlines", "body", Base::Overwrite)
        .unwrap_err();

    match refused {
        EditRefused::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput),
        other => panic!("{other}"),
    }
    assert_eq!(read(&path), before);
}

#[test]
fn an_edit_with_an_empty_body_is_refused_and_nothing_changes() {
    charter_core::unsteered!();
    // `write` refuses an empty body so a failed substitution cannot land a secret in a memory;
    // an edit is the same write.
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("A fact", at()).unwrap();
    let before = read(&path);

    let refused = ws
        .edit_memory(&slug_of(&path), "A fact", " \n\n ", Base::Overwrite)
        .unwrap_err();

    match refused {
        EditRefused::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput),
        other => panic!("{other}"),
    }
    assert_eq!(read(&path), before);
}

#[test]
fn an_edit_of_a_slug_that_is_not_there_is_not_found() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.remember("A fact", at()).unwrap();

    let refused = ws
        .edit_memory("no-such-memory", "T", "body", Base::Overwrite)
        .unwrap_err();

    match refused {
        EditRefused::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
        other => panic!("{other}"),
    }
}

#[test]
fn a_hand_written_memory_with_no_stamp_is_edited_without_inventing_one() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    std::fs::create_dir_all(ws.dir().join("memory")).unwrap();
    let path = ws.dir().join("memory/hand.md");
    std::fs::write(&path, "# Hand\n\nwritten by a person\n").unwrap();
    let opened = ws.open_memory("hand").unwrap();
    assert_eq!(
        opened.entry.body, "written by a person",
        "the heading is not body"
    );

    ws.edit_memory("hand", "Hand", &opened.entry.body, Base::Read(&opened.text))
        .unwrap();

    assert_eq!(read(&path), "# Hand\n\nwritten by a person\n");
}

#[test]
fn opening_a_memory_gives_its_text_title_stamp_and_body_without_the_header_lines() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("A fact\nwith a body", at()).unwrap();

    let opened = ws.open_memory(&slug_of(&path)).unwrap();

    assert_eq!(opened.path, path);
    assert_eq!(opened.text, read(&path));
    assert_eq!(opened.entry.title, "A fact");
    assert_eq!(opened.entry.stamp, "2026-03-02 09:14");
    assert_eq!(opened.entry.body, "A fact\nwith a body");
}

#[test]
fn a_slug_that_is_a_path_is_refused_by_every_operation() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.remember("A fact", at()).unwrap();
    let outside = tmp.path().join("workspaces/alpha/workspace.md");
    std::fs::write(&outside, "# mine\n").unwrap();

    for slug in ["../workspace", "../workspace.md", "a/b"] {
        match ws.edit_memory(slug, "T", "body", Base::Overwrite) {
            Err(EditRefused::Io(e)) => {
                assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{slug}")
            }
            other => panic!("{slug}: {other:?}"),
        }
        assert_eq!(
            ws.archive_memory(slug).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput,
            "{slug}"
        );
        assert_eq!(
            ws.unarchive_memory(slug, None).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput,
            "{slug}"
        );
    }
    assert_eq!(read(&outside), "# mine\n");
}

// --- archive and unarchive --------------------------------------------------------------------

#[test]
fn an_archived_memory_is_gone_from_the_listing_and_the_index() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let keep = ws.remember("Keep this", at()).unwrap();
    let gone = ws.remember("Archive this", at()).unwrap();

    let moved = ws.archive_memory(&slug_of(&gone)).unwrap();

    assert_eq!(
        moved,
        ws.dir()
            .join("memory/archive")
            .join(gone.file_name().unwrap())
    );
    assert!(moved.is_file() && !gone.exists());
    let titles: Vec<String> = ws
        .memories()
        .unwrap()
        .into_iter()
        .map(|m| m.title)
        .collect();
    assert_eq!(titles, ["Keep this"]);
    assert_eq!(
        lines(&ws.dir().join("memory/MEMORY.md")),
        [format!(
            "- [Keep this]({})",
            keep.file_name().unwrap().to_string_lossy()
        )]
    );
}

#[test]
fn unarchiving_puts_the_file_and_its_index_line_back() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("Bring me back", at()).unwrap();
    ws.archive_memory(&slug_of(&path)).unwrap();

    let restored = ws.unarchive_memory(&slug_of(&path), None).unwrap();

    assert_eq!(restored, path);
    assert!(
        !ws.dir()
            .join("memory/archive")
            .join(path.file_name().unwrap())
            .exists()
    );
    let titles: Vec<String> = ws
        .memories()
        .unwrap()
        .into_iter()
        .map(|m| m.title)
        .collect();
    assert_eq!(titles, ["Bring me back"]);
    assert_eq!(
        lines(&ws.dir().join("memory/MEMORY.md")),
        [format!(
            "- [Bring me back]({})",
            path.file_name().unwrap().to_string_lossy()
        )]
    );
}

#[test]
fn archive_then_unarchive_leaves_the_memory_byte_for_byte_as_it_was() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("Round trip\nwith a body", at()).unwrap();
    let file_before = read(&path);
    let index_before = read(&ws.dir().join("memory/MEMORY.md"));

    ws.archive_memory(&slug_of(&path)).unwrap();
    ws.unarchive_memory(&slug_of(&path), None).unwrap();

    assert_eq!(read(&path), file_before);
    assert_eq!(read(&ws.dir().join("memory/MEMORY.md")), index_before);
}

#[test]
fn archiving_or_unarchiving_twice_changes_nothing_the_second_time() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("Twice", at()).unwrap();
    let slug = slug_of(&path);

    let first = ws.archive_memory(&slug).unwrap();
    let second = ws.archive_memory(&slug).unwrap();
    assert_eq!(first, second, "already archived is where it is");
    assert_eq!(
        std::fs::read_dir(ws.dir().join("memory/archive"))
            .unwrap()
            .count(),
        1
    );

    ws.unarchive_memory(&slug, None).unwrap();
    let again = ws.unarchive_memory(&slug, None).unwrap();
    assert_eq!(again, path, "already back is where it is");
    assert_eq!(
        lines(&ws.dir().join("memory/MEMORY.md")).len(),
        1,
        "the index line is not appended twice"
    );
}

#[test]
fn archiving_or_unarchiving_a_slug_in_neither_place_is_not_found() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    ws.remember("A fact", at()).unwrap();

    assert_eq!(
        ws.archive_memory("nothing-here").unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(
        ws.unarchive_memory("nothing-here", None)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn a_memory_archiving_had_to_number_is_restored_under_the_name_it_is_given() {
    charter_core::unsteered!();
    // A memory of this name was archived before, so this one lands as `-2`: an undo has to put
    // it back under its own slug, or the tab that holds it points at nothing.
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("Same name", at()).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    std::fs::create_dir_all(ws.dir().join("memory/archive")).unwrap();
    std::fs::write(ws.dir().join("memory/archive").join(&name), "# older\n").unwrap();

    let moved = ws.archive_memory(&slug_of(&path)).unwrap();
    assert_eq!(
        moved.file_name().unwrap().to_string_lossy(),
        name.replace(".md", "-2.md")
    );

    let restored = ws
        .unarchive_memory(
            &moved.file_name().unwrap().to_string_lossy(),
            Some(&slug_of(&path)),
        )
        .unwrap();

    assert_eq!(restored, path);
    assert_eq!(
        read(&ws.dir().join("memory/archive").join(&name)),
        "# older\n"
    );
    assert_eq!(
        lines(&ws.dir().join("memory/MEMORY.md")),
        [format!("- [Same name]({name})")]
    );
}

#[test]
fn unarchiving_onto_a_name_the_store_already_holds_is_refused_and_nothing_moves() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("Taken", at()).unwrap();
    ws.archive_memory(&slug_of(&path)).unwrap();
    std::fs::write(&path, "# a new one\n").unwrap();

    let refused = ws.unarchive_memory(&slug_of(&path), None).unwrap_err();

    assert_eq!(refused.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(read(&path), "# a new one\n");
    assert!(
        ws.dir()
            .join("memory/archive")
            .join(path.file_name().unwrap())
            .is_file()
    );
}

// --- persona and _shared ----------------------------------------------------------------------

fn devops(tmp: &tempfile::TempDir) -> Persona {
    plane(tmp).persona("devops").unwrap()
}

fn shared(tmp: &tempfile::TempDir) -> Persona {
    Plane::open(tmp.path()).persona("_shared").unwrap()
}

#[test]
fn a_persona_memory_is_edited_in_place_and_keeps_its_slug() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let persona = devops(&tmp);
    let path = persona
        .remember("Cluster prod-1 lives in eu-west-1", at())
        .unwrap();
    let opened = persona
        .open_memory("cluster-prod-1-lives-in-eu-west-1")
        .unwrap();

    let edited = persona
        .edit_memory(
            "cluster-prod-1-lives-in-eu-west-1",
            "Cluster prod-1 lives in eu-central-1",
            "It moved in September.",
            Base::Read(&opened.text),
        )
        .unwrap();

    assert_eq!(edited, path);
    assert_eq!(
        read(&path),
        "# Cluster prod-1 lives in eu-central-1\n\n_2026-03-02 09:14 · persistent_\n\nIt moved in September.\n"
    );
    assert_eq!(
        lines(&persona.dir().join("memory/MEMORY.md")),
        ["- [Cluster prod-1 lives in eu-central-1](cluster-prod-1-lives-in-eu-west-1.md)"]
    );
}

#[test]
fn a_shared_memory_is_edited_archived_and_unarchived_in_the_shared_store() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let _ = plane(&tmp);
    let shared = shared(&tmp);
    let path = shared
        .remember("The plane is the unit of work", at())
        .unwrap();
    assert!(path.starts_with(tmp.path().join("personas/_shared/memory")));

    shared
        .edit_memory(
            "the-plane-is-the-unit-of-work",
            "The plane is the unit of work, always",
            "Always.",
            Base::Overwrite,
        )
        .unwrap();
    let archived = shared
        .archive_memory("the-plane-is-the-unit-of-work")
        .unwrap();
    assert!(archived.starts_with(tmp.path().join("personas/_shared/memory/archive")));
    assert!(shared.memories().unwrap().is_empty());

    shared
        .unarchive_memory("the-plane-is-the-unit-of-work", None)
        .unwrap();

    assert_eq!(
        lines(&shared.dir().join("memory/MEMORY.md")),
        ["- [The plane is the unit of work, always](the-plane-is-the-unit-of-work.md)"]
    );
    assert_eq!(shared.memories().unwrap()[0].body, "Always.");
}

#[test]
fn a_stale_persona_edit_is_refused_like_a_workspace_one() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let persona = devops(&tmp);
    let path = persona.remember("A fact", at()).unwrap();
    let opened = persona.open_memory("a-fact").unwrap();
    std::fs::write(&path, "# A fact\n\nchanged\n").unwrap();

    let refused = persona
        .edit_memory("a-fact", "Mine", "mine", Base::Read(&opened.text))
        .unwrap_err();

    assert!(matches!(refused, EditRefused::Stale), "{refused}");
    assert_eq!(read(&path), "# A fact\n\nchanged\n");
}

#[test]
fn unarchiving_a_memory_the_index_still_lists_does_not_list_it_twice() {
    charter_core::unsteered!();
    // `MEMORY.md` is `merge=union`: a pull can bring an archived memory's line back while its
    // file stays in `archive/`.
    let tmp = tempfile::tempdir().unwrap();
    let ws = alpha(&tmp);
    let path = ws.remember("Listed still", at()).unwrap();
    let index = ws.dir().join("memory/MEMORY.md");
    let before = read(&index);
    ws.archive_memory(&slug_of(&path)).unwrap();
    std::fs::write(&index, &before).unwrap();

    ws.unarchive_memory(&slug_of(&path), None).unwrap();

    assert_eq!(read(&index), before);
}

// --- create, under a title of the caller's ----------------------------------------------------

#[test]
fn a_persona_memory_made_under_a_title_is_named_and_indexed_for_that_title() {
    charter_core::unsteered!();
    // The window's create (SI-9b): the title is the operator's field, and the body its own
    // text, so the file is the one `charter persona remember --title` would have written.
    let tmp = tempfile::tempdir().unwrap();
    let devops: Persona = plane(&tmp).persona("devops").unwrap();

    let path = devops
        .remember_titled("It lives in eu-west-1.", Some("Where prod-1 is"), at())
        .unwrap();

    assert_eq!(path.file_name().unwrap(), "where-prod-1-is.md");
    assert_eq!(
        read(&path),
        "# Where prod-1 is\n\n_2026-03-02 09:14 · persistent_\n\nIt lives in eu-west-1.\n"
    );
    assert_eq!(
        lines(&devops.dir().join("memory/MEMORY.md")),
        ["- [Where prod-1 is](where-prod-1-is.md)"]
    );
}

#[test]
fn a_persona_memory_made_with_no_title_is_titled_by_its_first_line() {
    charter_core::unsteered!();
    let tmp = tempfile::tempdir().unwrap();
    let shared = plane(&tmp).persona("_shared").unwrap();

    let path = shared
        .remember_titled("Deploys freeze on Fridays\nAsk first.", None, at())
        .unwrap();

    assert_eq!(path.file_name().unwrap(), "deploys-freeze-on-fridays.md");
}
