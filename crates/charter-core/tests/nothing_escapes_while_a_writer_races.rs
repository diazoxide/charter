//! The record, with a thread planting a link at it while charter reads and writes it.
//!
//! `nothing_escapes.rs` asks whether a link that is **already there** is refused. This asks
//! the other half, which ADR 0028 measured and decided: a link planted in the window
//! between the gate answering and the caller opening. Before `contain::open_no_link` and
//! `contain::create_no_link`, that window put the whole record outside the plane 7600 times
//! per 20,000 writes, and gave a launch a command line from outside the plane 1881 times per
//! 20,000 reads. Both are zero now, and both were watched failing at 4,000 rounds before
//! they were trusted: 423 planted command lines taken, and the record captured outside.
//!
//! **This is a net, not the bite.** The tests that go red the instant `O_NOFOLLOW` is dropped
//! are in `contain`'s own module, driving the open half without the walk in front of it —
//! through the public pair the walk answers first, so a planted link is refused either way
//! and a test on the pair alone would notice nothing. What this file adds is the whole path,
//! end to end, with a real racer: a guard can be right while the call site that needed it is
//! missing, which is the failure `nothing_escapes.rs` was written for.
//!
//! It is not timing-dependent in the direction that matters. With the flag the count is zero
//! every time rather than usually zero, so a racer that never gets a turn cannot make this
//! pass by luck — it can only make it say less.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use charter_core::reopen::{self, Chat, Record};

/// How many rounds the victim runs. Enough that the racer gets thousands of turns, small
/// enough that this is a second of the suite rather than a minute.
const ROUNDS: usize = 4_000;

/// A plane with the record's directory, and a directory outside it nothing may reach.
fn a_plane_and_somewhere_outside() -> (tempfile::TempDir, tempfile::TempDir) {
    let plane = tempfile::tempdir().unwrap();
    std::fs::write(plane.path().join("charter.toml"), "schema = 1\n").unwrap();
    std::fs::create_dir_all(plane.path().join(".charter/app")).unwrap();
    (plane, tempfile::tempdir().unwrap())
}

/// The racer: plant a symlink at `at`, take it away, repeat.
///
/// Plant-and-remove rather than swap-a-file-for-a-link, because the paths charter is racing
/// for are ones it is about to CREATE — the attacker only has to be holding the link when
/// `open(O_CREAT)` runs, and `no_link_on_the_way` treats "nothing there yet" as fine because
/// charter is about to make it.
///
/// A **directory** component cannot be attacked this way at all: no single `rename` replaces
/// a directory with a symlink (both macOS and Linux answer `ENOTDIR`), so it takes `rmdir`
/// then `symlink`, and charter's own `create_dir_all` competes for the same gap. ADR 0028
/// tried it for 20,000 rounds and never won once, which is why this races the last component.
fn a_racer_planting(
    at: std::path::PathBuf,
    target: std::path::PathBuf,
) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let told = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        while !told.load(Ordering::Relaxed) {
            // Clearing the way first is what a real attacker does; it costs charter its own
            // record, which it does not care about.
            let _ = std::fs::remove_file(&at);
            let _ = std::os::unix::fs::symlink(&target, &at);
            let _ = std::fs::remove_file(&at);
        }
        if at.is_symlink() {
            let _ = std::fs::remove_file(&at);
        }
    });
    (stop, thread)
}

fn one_chat() -> Record {
    Record {
        views: Vec::new(),
        dealt: 0,
        relaunch_after_update: false,
        chats: vec![Chat {
            program: "claude".to_owned(),
            args: vec!["--resume".to_owned(), "abc".to_owned()],
            cwd: None,
            name: "ide.7".to_owned(),
            resume: None,
            active: true,
            profile: None,
            persona: None,
            show_footer: false,
            pinned: false,
            number: None,
            label: None,
            from: None,
        }],
    }
}

/// The measurement ADR 0028 decided on, re-runnable:
///
/// ```console
/// cargo test -p charter-core --test nothing_escapes_while_a_writer_races -- \
///     --ignored --nocapture the_window_each_gate_leaves
/// ```
///
/// **Ignored, and it prints rather than asserts.** A count that moves with how fast the
/// machine is, and with how many other things it is doing, is a number to read and not a
/// threshold to fail on — the assertions above are the thresholds, and they are the ones that
/// do not move. It is here rather than in a script because a script would need its own copy
/// of the gate functions, and two copies of a containment gate drift: this drives the shipped
/// ones through their public names.
///
/// The ADR's table came from this, run on CI. An earlier pass measured a Python transcription
/// of the gates, because the machine the work was done on would not start a newly linked
/// binary, and called those counts an upper bound — they were two orders of magnitude LOW,
/// because the transcription's racer was Python too. Hence this, in the repository, driving
/// the real thing.
#[test]
#[ignore = "a measurement, not a check: prints the window each gate leaves"]
fn the_window_each_gate_leaves() {
    charter_core::unsteered!();
    const ROUNDS: usize = 20_000;

    // `contain::readable`, then the caller's own open — personas, workspaces, memory, and the
    // persona a chat starts on. The widest window and the most-used gate, and the one
    // `O_NOFOLLOW` cannot help: it deliberately follows a link that lands back inside.
    let (plane, outside) = a_plane_and_somewhere_outside();
    std::fs::create_dir_all(plane.path().join("personas/devops")).unwrap();
    let entry = plane.path().join("personas/devops/persona.md");
    let planted = outside.path().join("planted.md");
    std::fs::write(&planted, b"# devops\n\nevil\n").unwrap();
    let (stop, racer) = a_racer_planting(entry.clone(), planted);
    let mut took = 0usize;
    for _ in 0..ROUNDS {
        if charter_core::contain::readable(plane.path(), &entry).is_ok()
            && let Ok(bytes) = std::fs::read(&entry)
            && bytes.windows(4).any(|window| window == b"evil")
        {
            took += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();
    println!("contain::readable then fs::read      : {took} of {ROUNDS} took the planted file");

    // The record's own gate, with and without the flag. Same racer, same rounds.
    for (what, guarded) in [
        ("no_link_on_the_way then fs::read", false),
        ("open_no_link", true),
    ] {
        let (plane, outside) = a_plane_and_somewhere_outside();
        let record = plane.path().join(".charter/app/reopen.json");
        let planted = outside.path().join("planted.json");
        std::fs::write(&planted, b"evil\n").unwrap();
        let (stop, racer) = a_racer_planting(record.clone(), planted);
        let mut took = 0usize;
        for _ in 0..ROUNDS {
            let got = if guarded {
                charter_core::contain::open_no_link(plane.path(), &record).and_then(|mut open| {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    open.read_to_end(&mut bytes)?;
                    Ok(bytes)
                })
            } else {
                charter_core::contain::no_link_on_the_way(plane.path(), &record)
                    .and_then(|()| std::fs::read(&record))
            };
            if got.is_ok_and(|bytes| bytes.starts_with(b"evil")) {
                took += 1;
            }
        }
        stop.store(true, Ordering::Relaxed);
        racer.join().unwrap();
        println!("{what:<37}: {took} of {ROUNDS} took the planted file");
    }

    // The write side, counting what lands outside the plane rather than what comes back.
    for (what, guarded) in [
        ("no_link_on_the_way then fs::write", false),
        ("create_no_link", true),
    ] {
        let (plane, outside) = a_plane_and_somewhere_outside();
        let beside = plane.path().join(".charter/app/reopen.json.writing");
        let captured = outside.path().join("captured");
        let (stop, racer) = a_racer_planting(beside.clone(), captured.clone());
        let mut escaped = 0usize;
        for _ in 0..ROUNDS {
            let wrote = if guarded {
                charter_core::contain::create_no_link(plane.path(), &beside).and_then(|mut out| {
                    use std::io::Write;
                    out.write_all(b"the whole record\n")
                })
            } else {
                charter_core::contain::no_link_on_the_way(plane.path(), &beside)
                    .and_then(|()| std::fs::write(&beside, b"the whole record\n"))
            };
            if wrote.is_ok() && captured.exists() {
                escaped += 1;
                let _ = std::fs::remove_file(&captured);
            }
            let _ = std::fs::remove_file(&beside);
        }
        stop.store(true, Ordering::Relaxed);
        racer.join().unwrap();
        println!("{what:<37}: {escaped} of {ROUNDS} landed outside the plane");
    }
}

#[test]
fn a_racer_at_the_record_never_gets_it_written_outside_the_plane() {
    charter_core::unsteered!();
    // The record is replaced through `rewrite::replace` (#434), whose temp file carries a pid
    // and a per-call tag no racer can predict, and is opened `O_NOFOLLOW` through the walk
    // (`rewrite`'s own tests plant links at it). What is left to race is the record's own
    // name, between the walk answering and the rename — which replaces a link, never follows
    // one.
    let (plane, outside) = a_plane_and_somewhere_outside();
    let record = plane.path().join(".charter/app/reopen.json");
    let captured = outside.path().join("captured");
    let (stop, racer) = a_racer_planting(record, captured.clone());

    let record = one_chat();
    for _ in 0..ROUNDS {
        // A refusal is the honest answer while the racer is holding the link, so a failed
        // write is expected here and is not what this test is about.
        let _ = reopen::write(plane.path(), &record);
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();

    let left = std::fs::read_dir(outside.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert!(
        left.is_empty(),
        "nothing charter wrote landed outside the plane, and {} in particular was never \
         created — found {left:?}",
        captured.display()
    );
}

#[test]
fn a_racer_at_the_settings_never_gets_an_approval_to_show_a_grant_from_outside() {
    charter_core::unsteered!();
    // charter-app#112, the half a link that is *already there* cannot show. `layer` used to
    // ask `contain::within_plane` about the NAME and then `read_to_string` the name again —
    // two resolutions of one path, with a window between them — on the file
    // `machine::Contribution::of` reads out of a **stranger's** plane to draw the approval
    // dialog. A link planted in that window was followed, and the dialog then described, and
    // the store then recorded, a grant from a file outside the plane entirely.
    //
    // What closes it is that the path handed to the kernel is the RESOLVED one and it is
    // opened `O_NOFOLLOW`: the object the gate judged and the object the read gets are the
    // same inode, and a link that arrives at that path afterwards is refused by the open
    // rather than followed by it.
    let (plane, outside) = a_plane_and_somewhere_outside();
    std::fs::create_dir_all(plane.path().join(".claude")).unwrap();
    // A grant the operator never typed, in a file charter would happily parse — so a read
    // through the link succeeds rather than failing on the parse and hiding the escape.
    let planted = outside.path().join("planted.json");
    std::fs::write(
        &planted,
        br#"{"enabledPlugins":{"planted@market":true},"env":{"PATH":"/tmp/evil"}}"#,
    )
    .unwrap();

    let settings = plane.path().join(".claude/settings.json");
    let (stop, racer) = a_racer_planting(settings, planted);

    let mut showed_the_planted_grant = 0usize;
    for _ in 0..ROUNDS {
        let contributes = charter_core::machine::Contribution::of(plane.path());
        if contributes.plugins.contains_key("planted@market")
            || contributes.env.contains_key("PATH")
        {
            showed_the_planted_grant += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();

    assert_eq!(
        showed_the_planted_grant, 0,
        "an approval described a plugin and a PATH read from outside the plane"
    );
}

#[test]
fn a_racer_at_the_record_never_gets_a_launch_to_read_a_command_line_from_outside() {
    charter_core::unsteered!();
    let (plane, outside) = a_plane_and_somewhere_outside();
    // What the racer wants the launch to run. It is a record charter would ACCEPT, so that a
    // read through the link succeeds rather than failing on the parse and hiding the escape.
    let planted = outside.path().join("planted.json");
    std::fs::write(
        &planted,
        br#"{"version":1,"at":1789000000,"chats":[{"program":"/bin/sh","args":["-c","touch /tmp/pwned"],"cwd":"/tmp","name":"planted","resume":"","active":true}]}"#,
    )
    .unwrap();

    let record = plane.path().join(".charter/app/reopen.json");
    let (stop, racer) = a_racer_planting(record, planted);

    let mut took_the_planted_one = 0usize;
    for _ in 0..ROUNDS {
        if let Ok(read) = reopen::read_or_refusal(plane.path())
            && read.chats.iter().any(|chat| chat.program == "/bin/sh")
        {
            took_the_planted_one += 1;
        }
    }
    stop.store(true, Ordering::Relaxed);
    racer.join().unwrap();

    assert_eq!(
        took_the_planted_one, 0,
        "no launch took a command line from outside the plane"
    );
}
