use std::path::{Path, PathBuf};

use super::*;

/// A machine under a directory of its own, with a local project in the old place holding a
/// clone, one worktree of it, a file, and a record of an open chat in the worktree.
struct Old {
    _dir: tempfile::TempDir,
    places: Places,
    /// The worktree, in the old place.
    worktree: PathBuf,
}

fn an_old_local_project() -> Old {
    let dir = tempfile::tempdir().expect("a directory");
    let base = dir.path().canonicalize().expect("the directory");
    let places = Places {
        config_root: base.join("config"),
        data_home: base.join("data/purlis"),
    };
    let old = places.old();
    let clone = old.join("workspaces/shop/shop");
    let worktree = old.join("workspaces/shop/.worktrees/shop/first-task-1");
    // A clone and one linked worktree, linked both ways by absolute paths, as git links them.
    let record = clone.join(".git/worktrees/first-task-1");
    std::fs::create_dir_all(&record).expect("the worktree's record");
    std::fs::create_dir_all(&worktree).expect("the worktree");
    std::fs::write(
        record.join("gitdir"),
        format!("{}\n", worktree.join(".git").display()),
    )
    .expect("gitdir");
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", record.display()),
    )
    .expect(".git");
    std::fs::write(clone.join("README.md"), "# shop\n").expect("a file");
    crate::reopen::write(
        &old,
        &crate::reopen::Record {
            chats: vec![crate::reopen::Chat {
                program: "claude".to_owned(),
                cwd: Some(worktree.clone()),
                name: "first task 1".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        },
    )
    .expect("the record of open chats");
    crate::machine::update(&places.config_root, |store| {
        store.recents.push(crate::machine::Recent {
            plane: old.clone(),
            opened: 1,
            trust: None,
            pinned: true,
            pinned_workspaces: vec!["shop".to_owned()],
            most_active_pinned: false,
        });
        store.windows.push(crate::machine::Window {
            planes: vec![old.clone()],
            active: 0,
        });
    })
    .expect("remembered");
    Old {
        _dir: dir,
        places,
        worktree,
    }
}

fn nothing_in_use(_: &Path) -> Option<String> {
    None
}

fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

const REAL: Seams<'static> = Seams {
    in_use: &nothing_in_use,
    rename: &rename,
};

/// The path `old` names, moved into the new place.
fn moved(machine: &Old, old: &Path) -> PathBuf {
    machine.places.current().join(
        old.strip_prefix(machine.places.old())
            .expect("in the old place"),
    )
}

#[test]
fn a_local_project_in_the_config_home_moves_to_the_data_home_whole() {
    let machine = an_old_local_project();
    let (old, new) = (machine.places.old(), machine.places.current());

    let answer = run(&machine.places, &REAL);

    assert_eq!(
        answer,
        Moved::Moved {
            from: old.clone(),
            to: new.clone()
        }
    );
    assert!(
        std::fs::symlink_metadata(&old).is_err(),
        "the old place is still there"
    );
    assert_eq!(
        std::fs::read_to_string(new.join("workspaces/shop/shop/README.md")).expect("moved"),
        "# shop\n"
    );
}

#[test]
fn what_named_the_old_place_names_the_new_one_after_a_move() {
    let machine = an_old_local_project();
    let new = machine.places.current();
    let worktree = moved(&machine, &machine.worktree);

    run(&machine.places, &REAL);

    let record = new.join("workspaces/shop/shop/.git/worktrees/first-task-1");
    assert_eq!(
        std::fs::read_to_string(record.join("gitdir")).expect("gitdir"),
        format!("{}\n", worktree.join(".git").display())
    );
    assert_eq!(
        std::fs::read_to_string(worktree.join(".git")).expect(".git"),
        format!("gitdir: {}\n", record.display())
    );
    let chats = crate::reopen::read_strictly(&new)
        .expect("read")
        .expect("a record")
        .chats;
    assert_eq!(chats[0].cwd.as_deref(), Some(worktree.as_path()));
    let store = crate::machine::read(&machine.places.config_root).store;
    assert_eq!(store.recents[0].plane, new);
    assert!(store.recents[0].pinned, "the pin did not go with it");
    assert_eq!(store.windows[0].planes, vec![new.clone()]);
}

#[test]
fn a_local_project_something_works_in_is_left_where_it_is_and_says_why() {
    let machine = an_old_local_project();
    let old = machine.places.old();
    let busy = |_: &Path| Some("process 42 works in it".to_owned());

    let answer = run(
        &machine.places,
        &Seams {
            in_use: &busy,
            rename: &rename,
        },
    );

    assert_eq!(
        answer,
        Moved::Left {
            at: old.clone(),
            why: "process 42 works in it".to_owned()
        }
    );
    assert!(old.join("workspaces/shop/shop/README.md").is_file());
    assert!(!machine.places.current().exists(), "something was made");
    assert!(
        answer
            .to_string()
            .contains("purlis moves it at a launch when nothing works in it"),
        "{answer}"
    );
}

#[test]
fn a_new_place_that_is_taken_is_never_merged_into() {
    let machine = an_old_local_project();
    let new = machine.places.current();
    std::fs::create_dir_all(&new).expect("taken");

    let answer = run(&machine.places, &REAL);

    assert!(
        matches!(&answer, Moved::Left { why, .. } if why.contains("never merges")),
        "{answer:?}"
    );
    assert!(machine.places.old().join("workspaces").is_dir());
    assert_eq!(std::fs::read_dir(&new).expect("read").count(), 0);
}

#[test]
fn a_rename_the_system_refuses_leaves_the_project_where_it_was() {
    let machine = an_old_local_project();
    let refuse = |_: &Path, _: &Path| -> std::io::Result<()> {
        Err(std::io::Error::other("across devices"))
    };

    let answer = run(
        &machine.places,
        &Seams {
            in_use: &nothing_in_use,
            rename: &refuse,
        },
    );

    assert!(
        matches!(&answer, Moved::Left { why, .. } if why.contains("across devices")),
        "{answer:?}"
    );
    assert!(machine.places.old().join("workspaces").is_dir());
}

#[test]
fn the_pointer_stays_until_every_record_is_pointed_and_a_later_launch_finishes() {
    let machine = an_old_local_project();
    let (old, new) = (machine.places.old(), machine.places.current());
    // A launch that died after the rename and its pointer, before anything was pointed.
    std::fs::create_dir_all(new.parent().expect("a parent")).expect("the data home");
    std::fs::rename(&old, &new).expect("renamed");
    std::os::unix::fs::symlink(&new, &old).expect("the pointer");
    // And a store that cannot be written yet: a directory where the file goes.
    let store = crate::machine::file(&machine.places.config_root);
    let aside = store.with_extension("aside");
    std::fs::rename(&store, &aside).expect("aside");
    std::fs::create_dir_all(&store).expect("in the way");

    let first = run(&machine.places, &REAL);

    assert!(
        matches!(&first, Moved::PointerKept { why, .. } if why.contains("remembered projects")),
        "{first:?}"
    );
    assert_eq!(std::fs::read_link(&old).expect("the pointer"), new);
    assert!(
        old.join("workspaces/shop/shop/README.md").is_file(),
        "the old path no longer finds the project"
    );

    std::fs::remove_dir(&store).expect("out of the way");
    std::fs::rename(&aside, &store).expect("back");
    let second = run(&machine.places, &REAL);

    assert_eq!(
        second,
        Moved::Moved {
            from: old.clone(),
            to: new.clone()
        }
    );
    assert!(
        std::fs::symlink_metadata(&old).is_err(),
        "the pointer stayed"
    );
    assert_eq!(
        crate::machine::read(&machine.places.config_root)
            .store
            .recents[0]
            .plane,
        new
    );
}

#[test]
fn a_machine_with_nothing_in_the_old_place_has_nothing_moved() {
    let dir = tempfile::tempdir().expect("a directory");
    let places = Places {
        config_root: dir.path().join("config"),
        data_home: dir.path().join("data"),
    };

    assert_eq!(run(&places, &REAL), Moved::Nothing);
    assert!(!places.current().exists());
}

#[test]
fn a_link_in_the_old_place_that_purlis_did_not_leave_is_left_alone() {
    let dir = tempfile::tempdir().expect("a directory");
    let places = Places {
        config_root: dir.path().join("config"),
        data_home: dir.path().join("data"),
    };
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("elsewhere");
    std::fs::create_dir_all(crate::machine::dir(&places.config_root)).expect("config");
    std::os::unix::fs::symlink(&elsewhere, places.old()).expect("a link");

    assert_eq!(run(&places, &REAL), Moved::Nothing);
    assert_eq!(std::fs::read_link(places.old()).expect("kept"), elsewhere);
}

#[test]
fn a_folder_another_process_works_in_is_found_by_lsofs_answer() {
    let listing =
        "p100\nfcwd\nn/Users/op/.config/purlis/local-plane/workspaces\np200\nfcwd\nn/tmp\n";

    assert_eq!(
        lsof_cwds(listing, "200"),
        Ok(vec![(
            "process 100".to_owned(),
            PathBuf::from("/Users/op/.config/purlis/local-plane/workspaces")
        )])
    );
}

#[test]
fn an_lsof_answer_that_does_not_show_this_process_is_refused() {
    let listing = "p100\nfcwd\nn/tmp\n";

    assert!(lsof_cwds(listing, "200").is_err());
}

#[test]
fn this_process_working_in_a_folder_does_not_make_it_in_use() {
    let dir = tempfile::tempdir().expect("a directory");

    assert_eq!(in_use_with(dir.path(), &[]), None);
}

#[test]
fn a_folder_another_process_works_in_is_in_use() {
    let dir = tempfile::tempdir().expect("a directory");
    let at = dir.path().canonicalize().expect("the directory");
    std::fs::create_dir_all(at.join("workspaces")).expect("a folder");
    let mut sleep = std::process::Command::new("sleep");
    sleep.arg("30").current_dir(at.join("workspaces"));
    let mut other = crate::forklock::spawn(&mut sleep).expect("another process");

    let said = in_use_with(&at, &[]);

    let _ = other.kill();
    let _ = other.wait();
    let said = said.expect("in use");
    assert!(said.contains(&format!("process {}", other.id())), "{said}");
}

#[test]
fn the_doctor_names_only_the_local_project_still_in_the_config_home() {
    let dir = tempfile::tempdir().expect("a directory");
    let base = dir.path().canonicalize().expect("the directory");
    let places = Places {
        config_root: base.join("config"),
        data_home: base.join("data"),
    };
    let elsewhere = base.join("elsewhere");
    std::fs::create_dir_all(places.old()).expect("the old place");
    std::fs::create_dir_all(&elsewhere).expect("another project");

    let said = still_in_the_config_home(&places.old(), &places.config_root).expect("named");

    assert!(said.contains("no sandboxed chat starts here"), "{said}");
    assert_eq!(
        still_in_the_config_home(&elsewhere, &places.config_root),
        None
    );
    // Moved, with the pointer still there: the project opens from the new place.
    std::fs::create_dir_all(&places.data_home).expect("the data home");
    std::fs::rename(places.old(), places.current()).expect("moved");
    std::os::unix::fs::symlink(places.current(), places.old()).expect("the pointer");
    assert_eq!(
        still_in_the_config_home(&places.current(), &places.config_root),
        None
    );
}

/// A launch that died after the rename and before the pointer: the old place is empty, and
/// the journal names the folder now in the new place.
fn died_before_the_pointer(machine: &Old) {
    let (old, new) = (machine.places.old(), machine.places.current());
    let id = folder_id(&old).expect("an identity");
    write_journal(&machine.places, id).expect("the journal");
    std::fs::create_dir_all(new.parent().expect("a parent")).expect("the data home");
    std::fs::rename(&old, &new).expect("renamed");
}

#[test]
fn a_launch_that_died_before_the_pointer_is_finished_by_the_next() {
    let machine = an_old_local_project();
    let (old, new) = (machine.places.old(), machine.places.current());
    died_before_the_pointer(&machine);

    let answer = run(&machine.places, &REAL);

    assert_eq!(
        answer,
        Moved::Moved {
            from: old.clone(),
            to: new.clone()
        }
    );
    let store = crate::machine::read(&machine.places.config_root).store;
    assert_eq!(store.recents[0].plane, new);
    assert!(store.recents[0].pinned, "the pin did not go with it");
    let worktree = moved(&machine, &machine.worktree);
    assert!(
        std::fs::read_to_string(worktree.join(".git"))
            .expect(".git")
            .contains(new.to_str().expect("UTF-8")),
        "the worktree still names the old place"
    );
    assert!(
        std::fs::symlink_metadata(machine.places.journal()).is_err(),
        "the journal stayed"
    );
    assert_eq!(
        run(&machine.places, &REAL),
        Moved::Nothing,
        "a second launch"
    );
}

#[test]
fn a_journal_of_another_folder_moves_no_approval_to_the_new_place() {
    let machine = an_old_local_project();
    let (old, new) = (machine.places.old(), machine.places.current());
    died_before_the_pointer(&machine);
    // The moved folder went away and another was made in its place.
    std::fs::rename(&new, old.with_file_name("gone")).expect("away");
    std::fs::create_dir_all(&new).expect("another folder");

    assert_eq!(run(&machine.places, &REAL), Moved::Nothing);

    let store = crate::machine::read(&machine.places.config_root).store;
    assert_eq!(
        store.recents[0].plane, old,
        "the old place's approval moved"
    );
    assert!(
        std::fs::symlink_metadata(&old).is_err(),
        "a pointer was made"
    );
    assert!(std::fs::symlink_metadata(machine.places.journal()).is_err());
}

#[test]
fn a_worktree_link_half_rewritten_by_a_launch_that_died_is_finished() {
    let machine = an_old_local_project();
    let (old, new) = (machine.places.old(), machine.places.current());
    let worktree = moved(&machine, &machine.worktree);
    let record = new.join("workspaces/shop/shop/.git/worktrees/first-task-1");
    std::fs::create_dir_all(new.parent().expect("a parent")).expect("the data home");
    std::fs::rename(&old, &new).expect("renamed");
    std::os::unix::fs::symlink(&new, &old).expect("the pointer");
    // The record names the new place; the worktree's own file was never reached.
    std::fs::write(
        record.join("gitdir"),
        format!("{}\n", worktree.join(".git").display()),
    )
    .expect("gitdir");

    let answer = run(&machine.places, &REAL);

    assert!(matches!(answer, Moved::Moved { .. }), "{answer:?}");
    assert_eq!(
        std::fs::read_to_string(worktree.join(".git")).expect(".git"),
        format!("gitdir: {}\n", record.display())
    );
}

#[test]
fn a_link_a_chat_planted_in_the_project_is_never_written_through() {
    let machine = an_old_local_project();
    let old = machine.places.old();
    let base = machine
        .places
        .config_root
        .parent()
        .expect("the base")
        .to_path_buf();
    let outside = base.join("outside.txt");
    std::fs::write(&outside, format!("gitdir: {}/x\n", old.display())).expect("outside");
    // A worktree record that is a link, and one whose path walks up out of the project.
    let records = old.join("workspaces/shop/shop/.git/worktrees");
    std::fs::create_dir_all(records.join("linked")).expect("a record");
    std::os::unix::fs::symlink(&outside, records.join("linked/gitdir")).expect("a link");
    std::fs::create_dir_all(records.join("up")).expect("a record");
    std::fs::write(
        records.join("up/gitdir"),
        format!("{}/../../../outside.txt\n", old.display()),
    )
    .expect("gitdir");

    run(&machine.places, &REAL);

    assert_eq!(
        std::fs::read_to_string(&outside).expect("outside"),
        format!("gitdir: {}/x\n", old.display()),
        "a file outside the project was written"
    );
}

#[test]
fn a_worktree_outside_the_project_keeps_the_pointer_and_is_never_written() {
    let machine = an_old_local_project();
    let (old, new) = (machine.places.old(), machine.places.current());
    let base = machine
        .places
        .config_root
        .parent()
        .expect("the base")
        .to_path_buf();
    let outside = base.join("elsewhere/wt");
    std::fs::create_dir_all(&outside).expect("an outside worktree");
    let record = old.join("workspaces/shop/shop/.git/worktrees/wt");
    std::fs::create_dir_all(&record).expect("its record");
    std::fs::write(
        record.join("gitdir"),
        format!("{}\n", outside.join(".git").display()),
    )
    .expect("gitdir");
    let back = format!("gitdir: {}\n", record.display());
    std::fs::write(outside.join(".git"), &back).expect(".git");

    let answer = run(&machine.places, &REAL);

    assert!(
        matches!(&answer, Moved::PointerKept { why, .. } if why.contains("git worktree repair")),
        "{answer:?}"
    );
    assert_eq!(std::fs::read_link(&old).expect("the pointer"), new);
    assert_eq!(
        std::fs::read_to_string(outside.join(".git")).expect(".git"),
        back
    );
}

#[test]
fn a_data_home_in_the_config_home_is_no_new_place() {
    let machine = an_old_local_project();
    let places = Places {
        config_root: machine.places.config_root.clone(),
        data_home: crate::machine::dir(&machine.places.config_root).join("data"),
    };

    let answer = run(&places, &REAL);

    assert!(
        matches!(&answer, Moved::Left { why, .. } if why.contains("config home")),
        "{answer:?}"
    );
    assert!(places.old().join("workspaces").is_dir());
}

/// A task's record in the project, naming the folder it worked in as `folder`.
fn a_task_record(project: &Path, folder: &str) -> String {
    use crate::dispatchrecord::{Asker, ChatRef, Mode, Opening, Place, Worker};
    let chat = |n: u32, name: &str| ChatRef {
        chat: n,
        id: None,
        name: name.to_owned(),
        persona: None,
    };
    crate::dispatchrecord::open(
        project,
        Opening {
            mode: Mode::Task,
            asker: Asker {
                chat: chat(1, "steward 1"),
                ..Default::default()
            },
            persona: Some("devops".to_owned()),
            worker: Worker {
                chat: chat(2, "first task"),
                ..Default::default()
            },
            task: Some("first task".to_owned()),
            place: Place {
                workspace: Some("shop".to_owned()),
                folder: Some(folder.to_owned()),
                worktree: None,
            },
            brief: "Look.".to_owned(),
            report_owed: true,
        },
        chrono::Utc::now(),
    )
    .expect("a task's record")
    .id
}

/// #1698: a task record that names a folder in the old place by its whole path names it in the
/// new one, as a path in the project; one that names a folder outside the project is left as
/// it was.
#[test]
fn a_task_record_naming_a_folder_in_the_old_place_names_it_in_the_new_one() {
    let machine = an_old_local_project();
    let old = machine.places.old();
    let inside = a_task_record(&old, &machine.worktree.display().to_string());
    let outside = a_task_record(&old, "/somewhere/else");
    let relative = a_task_record(&old, "workspaces/shop");

    let answer = run(&machine.places, &REAL);

    assert!(matches!(answer, Moved::Moved { .. }), "{answer:?}");
    let new = machine.places.current();
    let folder = |id: &str| {
        crate::dispatchrecord::read(&new, id)
            .expect("the record")
            .place
            .folder
    };
    assert_eq!(
        folder(&inside).as_deref(),
        Some("workspaces/shop/.worktrees/shop/first-task-1")
    );
    assert_eq!(folder(&outside).as_deref(), Some("/somewhere/else"));
    assert_eq!(folder(&relative).as_deref(), Some("workspaces/shop"));
}
