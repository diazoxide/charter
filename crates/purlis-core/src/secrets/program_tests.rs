//! Where a provider's program is looked for (#1516): which directories, in which order, and
//! what a refusal says of them.

use super::*;
use crate::secrets::Env;

/// The `PATH` macOS gives an app started from the Dock.
const DOCK: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

fn dirs(places: &[Place]) -> Vec<&str> {
    places
        .iter()
        .map(|place| place.dir.to_str().unwrap())
        .collect()
}

#[test]
fn an_app_started_from_the_dock_searches_where_a_persons_installers_put_programs() {
    let places = places_from(
        Some(OsStr::new(DOCK)),
        Some(Path::new("/Users/op")),
        true,
        &|_| false,
    );
    assert_eq!(
        dirs(&places),
        [
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
            "/Users/op/.local/bin",
            "/Users/op/bin",
            "/Users/op/.opencode/bin",
            "/Users/op/.bun/bin",
            "/Users/op/.volta/bin",
            "/Users/op/.npm-global/bin",
            "/Users/op/.cargo/bin",
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/opt/homebrew/opt/rustup/bin",
            "/usr/local/opt/rustup/bin",
        ]
    );
    let route = |dir: &str| {
        places
            .iter()
            .find(|p| p.dir == Path::new(dir))
            .unwrap()
            .route
    };
    // `/usr/bin` is on `PATH` and is also searched whatever `PATH` says; `/usr/sbin` is not.
    assert_eq!(route("/usr/bin"), Route::Always);
    assert_eq!(route("/usr/sbin"), Route::Path);
    assert_eq!(route("/opt/homebrew/bin"), Route::Always);
    assert_eq!(route("/Users/op/.local/bin"), Route::Always);
}

#[test]
fn a_directory_a_chat_may_write_is_searched_after_every_other_wherever_path_names_it() {
    let places = places_from(
        Some(OsStr::new(
            "/Users/op/project/work/bin:/tmp/planted:/nix/me/bin:/usr/bin",
        )),
        Some(Path::new("/Users/op")),
        true,
        &|dir| dir.starts_with("/Users/op/project") || dir.starts_with("/tmp"),
    );
    let dirs = dirs(&places);
    assert_eq!(dirs[..2], ["/nix/me/bin", "/usr/bin"]);
    assert_eq!(
        dirs[dirs.len() - 2..],
        ["/Users/op/project/work/bin", "/tmp/planted"]
    );
    assert!(
        dirs.iter().position(|d| *d == "/opt/homebrew/bin")
            < dirs.iter().position(|d| *d == "/tmp/planted")
    );
}

#[test]
fn a_relative_directory_on_path_is_never_searched() {
    let places = places_from(Some(OsStr::new(".:bin::/usr/bin")), None, false, &|_| false);
    assert_eq!(dirs(&places), ["/usr/bin"]);
}

#[test]
fn a_fenced_build_searches_no_machine_wide_directory_its_path_does_not_name() {
    let places = places_from(
        Some(OsStr::new("/usr/bin:/bin")),
        Some(Path::new("/Users/op")),
        false,
        &|_| false,
    );
    let dirs = dirs(&places);
    assert_eq!(dirs[..3], ["/usr/bin", "/bin", "/Users/op/.local/bin"]);
    assert!(!dirs.contains(&"/opt/homebrew/bin"), "{dirs:?}");
    assert!(!dirs.contains(&"/usr/local/bin"), "{dirs:?}");
    // Every test build is fenced, so no test runs the `op` this machine has installed.
    const _: () = assert!(crate::fence::FENCED);
}

/// A context for a project and a home that are nowhere on this disk.
fn ctx(path: &str) -> Ctx {
    Ctx::new(
        Path::new("/Users/op/project"),
        Env::of(&[("PATH", path), ("HOME", "/Users/op")]),
    )
}

#[test]
fn a_chat_may_write_its_project_the_temp_directories_and_a_harnesss_homes() {
    let writable = chat_may_write(&ctx(DOCK));
    for dir in [
        "/Users/op/project",
        "/tmp",
        "/private/tmp",
        "/Users/op/.config",
        "/Users/op/.cache",
        "/Users/op/.codex",
    ] {
        assert!(
            writable.contains(&PathBuf::from(dir)),
            "{dir}: {writable:?}"
        );
    }
    assert!(writable.contains(&std::env::temp_dir()), "{writable:?}");
    // None of them holds a directory purlis searches for the person's own programs.
    for rel in crate::programs::USER_BIN {
        let dir = Path::new("/Users/op").join(rel);
        assert!(
            !writable.iter().any(|grant| dir.starts_with(grant)),
            "{} is searched, and a chat may write it",
            dir.display()
        );
    }
}

#[test]
fn a_program_planted_in_the_project_or_a_temp_directory_is_searched_for_last() {
    let places = ctx("/Users/op/project/workspaces/x/bin:/tmp/planted:/usr/bin").program_places();
    let dirs = dirs(&places);
    assert_eq!(dirs[0], "/usr/bin");
    assert_eq!(
        dirs[dirs.len() - 2..],
        ["/Users/op/project/workspaces/x/bin", "/tmp/planted"]
    );
    assert!(dirs.contains(&"/Users/op/.local/bin"));
}

#[cfg(unix)]
#[test]
fn a_program_is_found_with_the_route_it_was_found_by_or_every_directory_searched() {
    let on_path = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    stand_in::program(on_path.path(), "op", "#!/bin/sh\n");
    let local = home.path().join(".local/bin");
    std::fs::create_dir_all(&local).unwrap();
    stand_in::program(&local, "vault", "#!/bin/sh\n");
    let ctx = Ctx::new(
        Path::new("/Users/op/project"),
        Env::of(&[
            ("PATH", &on_path.path().to_string_lossy()),
            ("HOME", &home.path().to_string_lossy()),
        ]),
    );
    assert_eq!(
        ctx.program("op"),
        Ok(Found {
            path: on_path.path().join("op"),
            route: Route::Path
        })
    );
    assert_eq!(
        ctx.program("vault"),
        Ok(Found {
            path: local.join("vault"),
            route: Route::Always
        })
    );
    let not = ctx.program("no-such-program-1516").unwrap_err();
    assert_eq!(not.looked[0], on_path.path());
    assert!(not.looked.contains(&local), "{:?}", not.looked);
    assert_eq!(not.looked.len(), 1 + crate::programs::USER_BIN.len());
}

#[test]
fn a_refusal_names_every_directory_whole_and_where_to_link_a_program_kept_elsewhere() {
    let not = NotFound {
        program: "op".to_owned(),
        looked: vec![
            PathBuf::from("/usr/bin"),
            PathBuf::from("/Users/a-person-with-a-long-name/.local/bin"),
        ],
    };
    assert_eq!(
        ctx(DOCK).looked_in(&not),
        "It looked in: /usr/bin, /Users/a-person-with-a-long-name/.local/bin. If it is \
         installed somewhere else, put a link to it in /Users/op/.local/bin."
    );
    let nowhere = NotFound {
        program: "op".to_owned(),
        looked: Vec::new(),
    };
    let homeless = Ctx::new(Path::new("/Users/op/project"), Env::of(&[]));
    assert_eq!(
        homeless.looked_in(&nowhere),
        "It had no directory to look in: this process has no PATH and no home directory."
    );
    assert_eq!(
        homeless.looked_in(&not),
        "It looked in: /usr/bin, /Users/a-person-with-a-long-name/.local/bin."
    );
}
