//! Where a provider's program is looked for (#1516): which directories, in which order, which
//! file is never run, and what a refusal says.
//!
//! A test that runs the search makes its stand-ins in temp folders and so turns the temp
//! directories off as a place a chat may write ([`stand_ins_live_in_temp_folders`]). The two
//! tests of that rule leave it on.

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
    let places = places_from(Some(OsStr::new(DOCK)), Some(Path::new("/Users/op")), true);
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
fn a_relative_directory_on_path_is_never_searched() {
    let places = places_from(Some(OsStr::new(".:bin::/usr/bin")), None, false);
    assert_eq!(dirs(&places), ["/usr/bin"]);
}

#[test]
fn a_fenced_build_searches_no_machine_wide_directory_its_path_does_not_name() {
    let places = places_from(
        Some(OsStr::new("/usr/bin:/bin")),
        Some(Path::new("/Users/op")),
        false,
    );
    let dirs = dirs(&places);
    assert_eq!(dirs[..3], ["/usr/bin", "/bin", "/Users/op/.local/bin"]);
    assert!(!dirs.contains(&"/opt/homebrew/bin"), "{dirs:?}");
    assert!(!dirs.contains(&"/usr/local/bin"), "{dirs:?}");
    // Every test build is fenced, so no test runs the `op` this machine has installed.
    const _: () = assert!(crate::fence::FENCED);
}

/// A project, a home and a directory on `PATH`, each a folder of its own on this disk, and a
/// context that reads the project's vaults with that home and that `PATH`.
struct Machine {
    project: tempfile::TempDir,
    home: tempfile::TempDir,
    on_path: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        Self {
            project: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
            on_path: tempfile::tempdir().unwrap(),
        }
    }

    /// The context of a process whose `PATH` is `first`, then this machine's own directory.
    fn ctx_with(&self, first: &[&Path]) -> Ctx {
        let mut path: Vec<String> = first.iter().map(|d| d.display().to_string()).collect();
        path.push(self.on_path.path().display().to_string());
        Ctx::new(
            self.project.path(),
            Env::of(&[
                ("PATH", &path.join(":")),
                ("HOME", &self.home.path().to_string_lossy()),
            ]),
        )
    }

    fn ctx(&self) -> Ctx {
        self.ctx_with(&[])
    }

    /// `<home>/<rel>`, made.
    fn in_home(&self, rel: &str) -> PathBuf {
        let dir = self.home.path().join(rel);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

/// A stand-in program called `name` in `dir`.
fn a_program(dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    stand_in::program(dir, name, "#!/bin/sh\n")
}

#[cfg(unix)]
#[test]
fn a_program_is_found_with_the_route_it_was_found_by_or_every_directory_searched() {
    stand_ins_live_in_temp_folders();
    let machine = Machine::new();
    let op = a_program(machine.on_path.path(), "op");
    let local = machine.in_home(".local/bin");
    let vault = a_program(&local, "vault");
    let ctx = machine.ctx();
    assert_eq!(
        ctx.program("op"),
        Ok(Found {
            path: real(&op),
            found: op,
            route: Route::Path
        })
    );
    assert_eq!(
        ctx.program("vault"),
        Ok(Found {
            path: real(&vault),
            found: vault,
            route: Route::Always
        })
    );
    let NotRun::NotFound(not) = ctx.program("no-such-program-1516").unwrap_err() else {
        panic!("it is nowhere");
    };
    assert_eq!(not.looked[0], machine.on_path.path());
    assert!(not.looked.contains(&local), "{:?}", not.looked);
    assert_eq!(not.looked.len(), 1 + crate::programs::USER_BIN.len());
}

#[cfg(unix)]
#[test]
fn what_runs_is_the_file_the_disk_names_and_what_is_recorded_is_where_it_was_found() {
    stand_ins_live_in_temp_folders();
    let machine = Machine::new();
    // As an installer leaves it: the program in a folder of its version, a link on `PATH`.
    let installed = a_program(&machine.in_home("Caskroom/op/2.30.0"), "op");
    let link = machine.on_path.path().join("op");
    std::os::unix::fs::symlink(&installed, &link).unwrap();
    let found = machine.ctx().program("op").unwrap();
    assert_eq!(found.path, real(&installed));
    assert_eq!(found.found, link);
}

// ---- no program is run from where a chat may write (D-1516-9) ------------------------------

/// The one file passed over, for a search that ran none.
fn passed_over(ctx: &Ctx, name: &str) -> PathBuf {
    match ctx.program(name) {
        Err(NotRun::Writable { path, .. }) => path,
        other => panic!("refused as writable: {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn a_program_found_only_inside_the_project_is_refused_and_named() {
    stand_ins_live_in_temp_folders();
    let machine = Machine::new();
    let planted = a_program(&machine.project.path().join("workspaces/x/bin"), "op");
    let ctx = machine.ctx_with(&[planted.parent().unwrap()]);
    assert_eq!(passed_over(&ctx, "op"), planted);
    assert_eq!(
        ctx.not_run(
            "the 1Password CLI ('op')",
            "Install it.",
            &ctx.program("op").unwrap_err()
        ),
        format!(
            "purlis found the 1Password CLI ('op') only where a chat can write: {}, so it was \
             not run. Keep the program outside the project and outside what a chat may write. \
             {}",
            planted.display(),
            ctx.looked_in(
                &ctx.program_places()
                    .into_iter()
                    .map(|place| place.dir)
                    .collect::<Vec<_>>()
            )
        )
    );
}

#[cfg(unix)]
#[test]
fn a_program_a_chat_may_write_is_passed_over_for_one_further_along_that_it_may_not() {
    stand_ins_live_in_temp_folders();
    let machine = Machine::new();
    let planted = a_program(&machine.project.path().join("bin"), "op");
    let installed = a_program(machine.on_path.path(), "op");
    // The project's folder is first on `PATH`, and still its `op` is not the one.
    let ctx = machine.ctx_with(&[planted.parent().unwrap()]);
    assert_eq!(ctx.program("op").unwrap().found, installed);
}

/// F3: the half of the test that asks the disk. Neither spelling below is under the project
/// as written; only the real path is.
#[cfg(unix)]
#[test]
fn a_directory_on_path_that_is_a_link_into_the_project_is_judged_by_where_it_leads() {
    stand_ins_live_in_temp_folders();
    let machine = Machine::new();
    let planted = a_program(&machine.project.path().join("tools"), "op");
    let link = machine.in_home("links").join("tools");
    std::os::unix::fs::symlink(planted.parent().unwrap(), &link).unwrap();
    let ctx = machine.ctx_with(&[&link]);
    assert_eq!(passed_over(&ctx, "op"), real(&planted));
}

#[cfg(unix)]
#[test]
fn a_link_in_a_directory_no_chat_writes_to_a_file_a_chat_does_write_is_not_run() {
    stand_ins_live_in_temp_folders();
    let machine = Machine::new();
    let planted = a_program(&machine.project.path().join("tools"), "op");
    // `~/.local/bin/op`, where the refusal tells a person to put a link, pointing at it.
    let local = machine.in_home(".local/bin");
    std::os::unix::fs::symlink(&planted, local.join("op")).unwrap();
    assert_eq!(passed_over(&machine.ctx(), "op"), real(&planted));
}

/// M1: a folder the person let every chat write can be one of the fixed directories, which
/// are searched before the machine-wide ones.
#[cfg(unix)]
#[test]
fn a_fixed_directory_the_person_let_every_chat_write_is_not_where_the_program_comes_from() {
    stand_ins_live_in_temp_folders();
    let machine = Machine::new();
    let local = machine.in_home(".local/bin");
    let planted = a_program(&local, "op");
    let ctx = machine.ctx();
    assert_eq!(
        ctx.program("op").unwrap().found,
        planted,
        "before the grant"
    );

    crate::sandbox::local::grant_write(machine.project.path(), &local).unwrap();
    assert_eq!(passed_over(&ctx, "op"), planted);

    // One further along, in a directory nobody granted, is the one that runs.
    let installed = a_program(&machine.in_home("bin"), "op");
    assert_eq!(ctx.program("op").unwrap().found, installed);
}

/// M1: what the app recorded of the chat it reads for, beyond what every chat may write.
#[cfg(unix)]
#[test]
fn a_chats_own_folder_and_what_its_sandbox_lets_it_write_are_not_where_the_program_comes_from() {
    stand_ins_live_in_temp_folders();
    let machine = Machine::new();
    // A chat working outside the project, and a folder the person let that one chat write.
    let outside = tempfile::tempdir().unwrap();
    let granted = tempfile::tempdir().unwrap();
    let in_folder = a_program(&outside.path().join("bin"), "op");
    let in_grant = a_program(granted.path(), "vault");
    let ctx = machine.ctx_with(&[in_folder.parent().unwrap(), granted.path()]);
    assert_eq!(ctx.program("op").unwrap().found, in_folder, "no chat asked");
    assert_eq!(
        ctx.program("vault").unwrap().found,
        in_grant,
        "no chat asked"
    );

    let confines = crate::sandbox::Confines {
        writable: vec![granted.path().to_path_buf()],
        ..Default::default()
    };
    let ctx = ctx.chat(&confines, Some(outside.path()));
    assert_eq!(passed_over(&ctx, "op"), in_folder);
    assert_eq!(passed_over(&ctx, "vault"), in_grant);
}

#[cfg(unix)]
#[test]
fn a_program_in_a_temp_directory_is_refused() {
    // The rule as it ships: this test leaves the temp directories on.
    let machine = Machine::new();
    let planted = a_program(machine.on_path.path(), "op");
    assert_eq!(passed_over(&machine.ctx(), "op"), planted);
}

#[test]
fn a_chat_may_write_the_project_its_cache_home_every_harnesss_homes_and_the_temp_directories() {
    let ctx = Ctx::new(
        Path::new("/Users/op/project"),
        Env::of(&[
            ("PATH", DOCK),
            ("HOME", "/Users/op"),
            ("TMPDIR", "/Users/op/scratch"),
        ]),
    );
    let writes: Vec<PathBuf> = ctx
        .chat_writes()
        .into_iter()
        .map(|(named, _)| named)
        .collect();
    for dir in [
        "/Users/op/project",
        "/tmp",
        "/private/tmp",
        "/Users/op/scratch",
        "/Users/op/.codex",
        "/Users/op/.claude",
        "/Users/op/.config/opencode",
        "/Users/op/.local/share/opencode",
    ] {
        assert!(writes.contains(&PathBuf::from(dir)), "{dir}: {writes:?}");
    }
    assert!(
        writes
            .iter()
            .any(|dir| dir.to_string_lossy().contains("cache-homes")),
        "the project's cache home: {writes:?}"
    );
    // Not a whole base directory: a program a version manager keeps under one is a real
    // install. And none of them holds a directory purlis searches.
    for kept in ["/Users/op/.local/share/mise/shims", "/Users/op/.config/op"] {
        assert!(
            !crate::sandbox::program::writable(Path::new(kept), &ctx.chat_writes()),
            "{kept}"
        );
    }
    for place in ctx.program_places() {
        assert!(
            !crate::sandbox::program::writable(&place.dir, &ctx.chat_writes()),
            "{} is searched, and a chat may write it",
            place.dir.display()
        );
    }
}

#[test]
fn a_refusal_names_every_directory_whole_and_where_to_link_a_program_kept_elsewhere() {
    let ctx = Ctx::new(
        Path::new("/Users/op/project"),
        Env::of(&[("PATH", DOCK), ("HOME", "/Users/op")]),
    );
    let looked = vec![
        PathBuf::from("/usr/bin"),
        PathBuf::from("/Users/a-person-with-a-long-name/.local/bin"),
    ];
    assert_eq!(
        ctx.looked_in(&looked),
        "It looked in: /usr/bin, /Users/a-person-with-a-long-name/.local/bin. If it is \
         installed somewhere else, put a link to it in /Users/op/.local/bin."
    );
    let homeless = Ctx::new(Path::new("/Users/op/project"), Env::of(&[]));
    assert_eq!(
        homeless.looked_in(&[]),
        "It had no directory to look in: this process has no PATH and no home directory."
    );
    assert_eq!(
        homeless.looked_in(&looked),
        "It looked in: /usr/bin, /Users/a-person-with-a-long-name/.local/bin."
    );
    assert_eq!(
        ctx.not_run(
            "the 1Password CLI ('op')",
            "Install it and sign in, then retry.",
            &NotRun::NotFound(NotFound {
                program: "op".to_owned(),
                looked: looked.clone(),
            })
        ),
        format!(
            "purlis could not find the 1Password CLI ('op'). Install it and sign in, then \
             retry. {}",
            ctx.looked_in(&looked)
        )
    );
}
