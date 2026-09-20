//! What ConPTY does that a unix pty does not, measured on a real Windows runner.
//!
//! `crates/charter-core/src/session.rs` is written against three properties of a unix pty,
//! and none of them is stated anywhere as a property of `portable-pty`'s *interface* — they
//! are properties of its unix *implementation*. A read of `portable-pty` 0.9's `src/win`
//! says all three are different on Windows. A read is not a measurement, so this asks:
//!
//! 1. **Does the reader see EOF when the program exits?** `Session::start` does
//!    `drop(pair.slave)` with the comment *"Only the program may hold the terminal's other
//!    end, or the reader never sees it close"*, and `Session::when_it_ends` waits for the
//!    output to end before it asks for a status. On Windows the slave is an `Arc` clone of
//!    the same `Inner` the master holds, and the pipe's write end belongs to the pseudo
//!    console, which lives until `ClosePseudoConsole` — i.e. until the MASTER drops.
//!
//! 2. **What exit code comes back?** `WinChild::is_complete` reads `STILL_ACTIVE` (259) as
//!    "not finished". A program that exits 259 is then never finished.
//!
//! 3. **Does killing the program kill what it started?** `session::end` kills the process
//!    GROUP on unix. `portable-pty` spawns with `CreateProcessW` and no job object, and
//!    `WinChild::kill` is `TerminateProcess` on the one process.
//!
//! Every answer is printed and the probe always exits 0: this is evidence for the M4
//! inventory, not a gate.
//!
//! Every program it runs is a `.bat` file written beside the probe rather than a command
//! line. `cmd.exe /c "a & b"` has to survive both `CommandBuilder`'s quoting and `cmd`'s own,
//! and a probe that measured THAT by accident would be worth nothing.

use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

/// How long a question is given before "it did not happen" is the answer.
const PATIENCE: Duration = Duration::from_secs(10);

fn main() {
    println!("# windows-probe: what ConPTY does that a unix pty does not");
    println!("# portable-pty 0.9, the version crates/charter-core pins");

    let dir = std::env::temp_dir().join(format!("windows-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a directory to work in");

    the_same_program_without_a_pty(&dir);
    eof_when_the_program_exits(&dir);
    an_exit_code_of_259(&dir);
    what_the_program_started(&dir);

    let _ = std::fs::remove_dir_all(&dir);
    println!("# done");
    // `ClosePseudoConsole` — which is what dropping a master runs — blocks while a client is
    // still attached, and question 3 deliberately leaves one running. Nothing below this line
    // is a measurement, so the process leaves rather than waiting on a destructor whose
    // blocking is the very thing being reported.
    std::io::Write::flush(&mut std::io::stdout()).ok();
    std::process::exit(0);
}

/// Drops a master on a thread, and says whether the drop itself returned.
///
/// The drop is `ClosePseudoConsole`, and on Windows it waits for the last attached client.
/// A probe that blocks there reports nothing at all, so the block is measured instead of
/// suffered.
fn close(master: Box<dyn portable_pty::MasterPty + Send>, what: &str) {
    let (say, heard) = mpsc::channel();
    std::thread::spawn(move || {
        drop(master);
        let _ = say.send(());
    });
    match heard.recv_timeout(PATIENCE) {
        Ok(()) => println!("close-{what}: the master dropped"),
        Err(_) => println!(
            "close-{what}: BLOCKED — {PATIENCE:?} inside the drop. ClosePseudoConsole waits \
             for the last attached client, and charter drops a master on the UI's thread"
        ),
    }
}

fn size() -> PtySize {
    PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Writes `name` in `dir` with `body` and hands back the name to run it by.
fn script(dir: &Path, name: &str, body: &str) -> String {
    std::fs::write(dir.join(name), body.replace('\n', "\r\n")).expect("a script");
    name.to_owned()
}

/// Drains a reader into nothing, so no question is answered by a full pipe.
fn drain(mut reader: Box<dyn Read + Send>) {
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let _ = reader.read_to_end(&mut sink);
    });
}

/// Waits for the program to end, but never longer than [`PATIENCE`].
///
/// `Child::wait` has no deadline of its own, and a probe that hangs teaches nothing while
/// holding a CI run open. Polling `try_wait` is the same question with an end to it.
fn ended(child: &mut Box<dyn portable_pty::Child + Send + Sync>) -> Option<u32> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.exit_code()),
            Ok(None) | Err(_) if Instant::now() >= deadline => return None,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// Question 0, and it is the CONTROL the first run did not have.
///
/// The same `.bat`, the same working directory, through `std::process::Command` and no pty at
/// all. If this ends with `7` and the pty run does not, the difference is ConPTY. If neither
/// does, the probe is measuring its own `.bat` and nothing else — which the first run could
/// not tell apart, and reported as "unanswered" for that reason.
fn the_same_program_without_a_pty(dir: &Path) {
    let bat = script(dir, "exit7.bat", "@echo off\necho hello\nexit 7\n");
    let run = std::process::Command::new("cmd.exe")
        .args(["/c", bat.as_str()])
        .current_dir(dir)
        .output();
    match run {
        Ok(out) => println!(
            "no-pty-baseline: {:?}, stdout {:?}, stderr {:?}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(err) => println!("no-pty-baseline: the program would not even start: {err}"),
    }
}

/// Question 1. Spawns something that prints and exits, drops the slave exactly as
/// `Session::start` does, and reports whether the reader ever reaches EOF.
fn eof_when_the_program_exits(dir: &Path) {
    let bat = script(dir, "exit7.bat", "@echo off\necho hello\nexit 7\n");

    let pair = native_pty_system().openpty(size()).expect("a pty");
    let mut reader = pair.master.try_clone_reader().expect("a reader");

    let mut command = CommandBuilder::new("cmd.exe");
    command.args(["/c", bat.as_str()]);
    command.cwd(dir);
    let mut child = pair.slave.spawn_command(command).expect("the program runs");
    // Exactly what Session::start does, and the line whose comment this probe is checking.
    drop(pair.slave);

    println!("pty-child-pid: {:?}", child.process_id());

    // Shared, not only sent at EOF: the first run could not tell "the program never started"
    // from "it started and never ended", because the only thing it reported about the output
    // was a count it never got to print.
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let (say, heard) = mpsc::channel();
    let writing = std::sync::Arc::clone(&seen);
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut chunk = [0u8; 4096];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => writing.lock().unwrap().extend_from_slice(&chunk[..n]),
                Err(_) => break,
            }
        }
        let _ = say.send(started.elapsed());
    });

    match ended(&mut child) {
        Some(code) => println!("exit-code: the program exited 7, and portable-pty reports {code}"),
        None => println!("exit-code: UNANSWERED — {PATIENCE:?} and the program had not ended"),
    }
    println!("pty-said-by-then: {}", so_far(&seen));

    match heard.recv_timeout(PATIENCE) {
        Ok(took) => println!("eof-after-exit: YES, the reader reached EOF after {took:?}"),
        Err(_) => println!(
            "eof-after-exit: NO, the reader was still open {PATIENCE:?} after the program \
             exited — `drop(pair.slave)` does not end the output on this platform"
        ),
    }
    println!("pty-said-in-all: {}", so_far(&seen));
    // Dropping the master closes the pseudo console, which is what DOES end the reader here.
    close(pair.master, "q1");
    match heard.recv_timeout(PATIENCE) {
        Ok((took, _)) => println!("eof-after-master-drop: YES, after {took:?}"),
        Err(_) => println!("eof-after-master-drop: NO, not even then"),
    }
}

/// Question 2. 259 is `STILL_ACTIVE`, so a program that exits with it may never be seen to
/// have exited at all.
fn an_exit_code_of_259(dir: &Path) {
    let bat = script(dir, "exit259.bat", "@echo off\nexit 259\n");

    let pair = native_pty_system().openpty(size()).expect("a pty");
    drain(pair.master.try_clone_reader().expect("a reader"));

    let mut command = CommandBuilder::new("cmd.exe");
    command.args(["/c", bat.as_str()]);
    command.cwd(dir);
    let mut child = pair.slave.spawn_command(command).expect("the program runs");
    drop(pair.slave);

    let deadline = Instant::now() + PATIENCE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                println!(
                    "exit-259: try_wait saw it end, reporting {}",
                    status.exit_code()
                );
                break;
            }
            Ok(None) if Instant::now() >= deadline => {
                println!(
                    "exit-259: NO — {PATIENCE:?} of try_wait and the program still reads as \
                     running. 259 is STILL_ACTIVE, and a chat that ends with it never ends"
                );
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(err) => {
                println!("exit-259: try_wait failed: {err}");
                break;
            }
        }
    }
    close(pair.master, "q2");
}

/// Question 3. Kills the program the way `session::end`'s `cfg(not(unix))` arm does, and
/// asks whether something it started is still running afterwards.
///
/// No pid is chased: the grandchild appends to a file every two seconds, and a file that is
/// still growing after the kill is a grandchild that is still there.
fn what_the_program_started(dir: &Path) {
    let ticks = dir.join("ticks.txt");
    let _ = std::fs::remove_file(&ticks);
    script(
        dir,
        "ticker.bat",
        "@echo off\nfor /l %%i in (1,1,60) do (echo tick>>ticks.txt & ping -n 3 127.0.0.1 >NUL)\n",
    );
    // `start /b` detaches the ticker, so killing this cmd is the exact question: does the
    // kill reach what the program started, or only the program?
    let bat = script(
        dir,
        "parent.bat",
        "@echo off\nstart \"\" /b cmd.exe /c ticker.bat\nping -n 60 127.0.0.1 >NUL\n",
    );

    let pair = native_pty_system().openpty(size()).expect("a pty");
    drain(pair.master.try_clone_reader().expect("a reader"));

    let mut command = CommandBuilder::new("cmd.exe");
    command.args(["/c", bat.as_str()]);
    command.cwd(dir);
    let mut child = pair.slave.spawn_command(command).expect("the program runs");
    drop(pair.slave);

    std::thread::sleep(Duration::from_secs(6));
    let before_kill = size_of(&ticks);
    // The whole of what charter does on this platform today.
    let _ = child.kill();
    let _ = ended(&mut child);
    std::thread::sleep(Duration::from_secs(1));
    let just_after = size_of(&ticks);
    std::thread::sleep(Duration::from_secs(8));
    let later = size_of(&ticks);

    println!(
        "grandchild-ticks: {before_kill} bytes before the kill, {just_after} just after, \
         {later} eight seconds later"
    );
    if before_kill == 0 {
        println!(
            "kill-reaches-descendants: UNANSWERED — the ticker never wrote anything, so the \
             probe measured nothing. Fix the probe before believing either answer"
        );
    } else if later > just_after {
        println!(
            "kill-reaches-descendants: NO — what the program started outlived the kill. \
             TerminateProcess ends one process; there is no process group here and \
             portable-pty puts the child in no job object"
        );
    } else {
        println!("kill-reaches-descendants: YES — nothing kept ticking after the kill");
    }
    close(pair.master, "q3");
}

/// How many bytes are at `path`, or 0 if it is not there.
fn size_of(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// What the terminal has said so far, short enough to read and escaped enough to trust.
fn so_far(seen: &std::sync::Mutex<Vec<u8>>) -> String {
    let bytes = seen.lock().unwrap();
    format!(
        "{} bytes, {:?}",
        bytes.len(),
        String::from_utf8_lossy(&bytes[..bytes.len().min(160)])
    )
}
