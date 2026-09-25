//! Persona statistics as the app ships it: a built-in extension, inside the app's bundle, trusted
//! through the app rather than through an approval prompt (charter-app#339, ADR 0041's
//! amendment of 2026-09-25 on built-in trust).
//!
//! The bundle here is a directory laid out as the app's resources are — `extensions/<id>/` —
//! with the program `assemble`d into it exactly as the release build does. Nothing is installed
//! and nothing is approved: the registry learns of it from [`extension::BuiltIn`], which the app
//! builds from its own resource path and from nothing a file says.

#![cfg(unix)]

use std::path::{Path, PathBuf};

use charter_core::executor::Executor;
use charter_core::extension::{self, BuiltIn, Source, Standing};
use charter_core::handed;
use charter_core::panel::Block;

const ID: &str = "persona-statistics";

/// A plane, an app bundle holding the built-in extension, and an empty config root.
struct Shipped {
    dir: tempfile::TempDir,
}

impl Shipped {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a directory");
        let plane = dir.path().join("plane");
        std::fs::create_dir_all(&plane).expect("a plane");
        std::fs::write(
            plane.join("charter.toml"),
            "[persona]\ndefault = \"steward\"\n",
        )
        .expect("a manifest");
        for (name, days) in [
            ("steward", &["2026-09-22", "2026-09-10", "2026-06-01"][..]),
            ("release", &["2026-09-20"][..]),
            ("forge", &[][..]),
        ] {
            let at = plane.join("personas").join(name);
            std::fs::create_dir_all(at.join("memory")).expect("a persona");
            std::fs::write(at.join("persona.md"), "---\nrole: x\n---\n").expect("a definition");
            for (n, day) in days.iter().enumerate() {
                std::fs::write(
                    at.join("memory").join(format!("fact-{n}.md")),
                    format!("# fact {n}\n\n_{day} 10:00 · persistent_\n\nbody {n}\n"),
                )
                .expect("a memory");
            }
        }
        // A memory written by hand with no stamp line, dated by its file name — the case where a
        // reader of its own would date it differently from `charter persona stats`.
        std::fs::write(
            plane.join("personas/release/memory/20260921-note.md"),
            "# a note written by hand\n\nno stamp line here\n",
        )
        .expect("a hand-written memory");

        let status = std::process::Command::new(env!("CARGO_BIN_EXE_persona-statistics"))
            .arg("assemble")
            .arg(dir.path().join("charter.app/Contents/Resources/extensions").join(ID))
            .status()
            .expect("assemble runs");
        assert!(status.success(), "assemble failed: {status}");
        Self { dir }
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join("config")
    }

    fn plane(&self) -> PathBuf {
        self.dir.path().join("plane")
    }

    /// Where the running app's built-in extensions are — what the app hands the core.
    fn built_in(&self) -> BuiltIn {
        BuiltIn::at(
            self.dir
                .path()
                .join("charter.app/Contents/Resources/extensions"),
        )
    }

    fn inside(&self) -> PathBuf {
        self.built_in().root().expect("a root").join(ID)
    }

    fn ask(&self, focus: Option<&str>) -> Result<charter_core::executor::Answer, String> {
        let plane = self.plane();
        Executor::with_built_in(self.built_in()).ask(
            &self.config(),
            &extension::project::Choices::read(&plane),
            ID,
            "statistics",
            focus,
            |_| handed::personas(&plane, noon()),
        )
    }
}

fn noon() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 9, 23)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .expect("a time")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("a listing") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("a copy");
        }
    }
}

#[test]
fn a_built_in_extension_is_listed_as_the_app_s_and_answers_with_no_approval() {
    let shipped = Shipped::new();

    let survey = extension::survey(&shipped.config(), &shipped.built_in());
    let row = survey
        .installed
        .iter()
        .find(|row| row.id == ID)
        .expect("the built-in is listed");
    assert_eq!(row.source, Source::App);
    assert_eq!(row.standing, Standing::Approved);
    assert!(row.on);
    assert_eq!(row.views_in_force().len(), 1);

    let answer = shipped.ask(None).expect("an answer, with nothing approved");
    assert!(!answer.blocks.is_empty());
}

#[test]
fn a_copy_outside_the_bundle_is_not_built_in() {
    let shipped = Shipped::new();
    let copy = shipped.dir.path().join("elsewhere");
    copy_tree(&shipped.inside(), &copy);
    let found = extension::read_at(&copy).expect("the copy reads");

    // The same bytes and the same id as the built-in, one directory over: new.
    let loaded = extension::read(&shipped.config(), &shipped.built_in());
    assert_eq!(loaded.standing(&found), Standing::New);

    // A record that claims the copy is the app's changes nothing: where a built-in is comes
    // from the app, never from a file.
    std::fs::create_dir_all(shipped.config().join("charter")).expect("a config dir");
    std::fs::write(
        extension::file(&shipped.config()),
        format!(
            r#"{{"version":1,"extensions":{{"{ID}":{{"source":"app","path":"{}","approved":null}}}}}}"#,
            copy.display()
        ),
    )
    .expect("a forged record");
    let loaded = extension::read(&shipped.config(), &shipped.built_in());
    assert_eq!(loaded.standing(&found), Standing::New);
    assert_eq!(
        loaded.entry(ID).map(|entry| entry.path.clone()),
        Some(shipped.inside())
    );

    // And with no app shipping it, the forged row is no approval of anything either.
    let loaded = extension::read(&shipped.config(), &BuiltIn::none());
    assert_eq!(loaded.standing(&found), Standing::New);

    // Installing the copy while the app ships one of that id is refused, by name.
    let refused = extension::install(&shipped.config(), &shipped.built_in(), &copy)
        .expect_err("a copy took the built-in's id");
    assert!(refused.to_string().contains("ships"), "{refused}");
}

#[test]
fn new_bytes_inside_the_bundle_are_trusted_as_an_update_brings_them() {
    let shipped = Shipped::new();
    shipped.ask(None).expect("the first version answers");

    // An update replaces the bundle, so the program's bytes change under the same path.
    let program = shipped.inside().join("bin").join(ID);
    let mut bytes = std::fs::read(&program).expect("the program");
    bytes.extend_from_slice(b"\0the next version");
    let beside = shipped.inside().join("bin/.next");
    std::fs::write(&beside, &bytes).expect("the next version");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o755))
            .expect("the mode");
    }
    std::fs::rename(&beside, &program).expect("in place");

    shipped
        .ask(None)
        .expect("the updated built-in answers without asking");
}

#[test]
fn turned_off_on_this_machine_it_offers_nothing_and_is_not_started() {
    let shipped = Shipped::new();
    extension::turn_on(&shipped.config(), ID, false).expect("turned off");

    let survey = extension::survey(&shipped.config(), &shipped.built_in());
    let row = survey
        .installed
        .iter()
        .find(|row| row.id == ID)
        .expect("still listed, so it can be turned back on");
    assert!(!row.on);
    assert!(row.views_in_force().is_empty());
    assert!(row.panels_in_force().is_empty());
    let refused = shipped.ask(None).expect_err("it ran while off");
    assert!(refused.contains("turned off on this machine"), "{refused}");
    let loaded = extension::read(&shipped.config(), &shipped.built_in());
    assert!(
        extension::project::resolve(
            &extension::project::Installed::from_record(&loaded),
            &extension::project::Choices::default()
        )
        .iter()
        .all(|it| it.id != ID || !it.is_on()),
        "a project still has it on"
    );

    extension::turn_on(&shipped.config(), ID, true).expect("turned back on");
    shipped.ask(None).expect("it answers again");
}

#[test]
fn a_project_or_a_workspace_can_turn_it_off_as_any_extension() {
    let shipped = Shipped::new();
    std::fs::write(
        shipped.plane().join("charter.toml"),
        format!("[persona]\ndefault = \"steward\"\n\n[extensions.{ID}]\nenabled = false\n"),
    )
    .expect("a project turning it off");
    let refused = shipped.ask(None).expect_err("it ran in a project that turned it off");
    assert!(refused.contains("turned off in charter.toml"), "{refused}");
}

#[test]
fn its_numbers_are_the_numbers_charter_persona_stats_gives() {
    use charter_core::personaverbs::stats;
    let shipped = Shipped::new();
    let today = noon().date();

    let answer = shipped.ask(None).expect("an answer");
    let per = answer
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Chart(chart) if chart.title == "Memories per persona" => Some(chart),
            _ => None,
        })
        .expect("the per-persona chart");
    let mut recent_total = 0;
    for name in ["steward", "release", "forge"] {
        let row = stats::row(&shipped.plane(), name, stats::RECENT_DAYS, today);
        let drawn = per
            .points
            .iter()
            .find(|point| point.label == name)
            .unwrap_or_else(|| panic!("no bar for {name}"));
        assert_eq!(u64::from(drawn.value), row.count as u64, "{name}'s MEM");
        recent_total += row.recent;

        let focused = shipped.ask(Some(name)).expect("an answer about one");
        let Some(Block::Note { text, .. }) = focused.blocks.first() else {
            panic!("no sentence about {name}");
        };
        assert!(
            text.contains(&format!(
                "{} in the last {} days",
                row.recent,
                stats::RECENT_DAYS
            )),
            "{name}'s RECENT is {}: {text}",
            row.recent
        );
    }
    let Some(Block::Note { text, .. }) = answer.blocks.first() else {
        panic!("no headline");
    };
    assert!(
        text.contains(&format!(
            "{recent_total} in the last {} days",
            stats::RECENT_DAYS
        )),
        "{text}"
    );
}
