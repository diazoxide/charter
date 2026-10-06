//! The host that runs chats has no controlling terminal (V77, ADR 0080 §1 as amended).
//!
//! A level-3 agent runs in a process group of its own ([`crate::acp`]), not a session of its
//! own: `setsid` in the child would need a `pre_exec`, which is `unsafe`, and the one audited
//! block is the executor's. So the agent stays in the host's session, and whatever terminal
//! controls that session the agent could open as `/dev/tty`. The fix is on the host's side:
//! it leaves its terminal once, as it starts, before any chat exists, with [`leave`].
//!
//! - **No terminal** (launched from the Finder, the Dock or a desktop file): nothing to do.
//! - **Not a group leader** (started by `cargo`, `npm` or another program): `setsid` makes the
//!   host the leader of a new session, which has no terminal. It no longer hears the terminal's
//!   Ctrl-C, so it ends when the program that started it ends.
//! - **A group leader** (started straight from a shell, which gives each job its own group):
//!   `setsid` is refused, so the host starts itself again as its own child, which is no group's
//!   leader and so can leave. The first process only waits for it, passes on the signals a
//!   terminal sends, and ends as it ends, by the same signal where a signal ended it
//!   ([`Left::Relaunched`]). The child watches a pipe from it and ends when it does, so a
//!   first process killed outright leaves no orphaned window.
//!
//! A session leader with no terminal could take one by opening a terminal device without
//! `O_NOCTTY`. The host opens terminals only as `portable-pty` pairs, and its test opens one
//! after leaving and checks that `/dev/tty` still does not open.

/// What [`leave`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// This process had no controlling terminal.
    NoneToLeave,
    /// This process now leads a session of its own, which has no terminal.
    NewSession,
    /// This process could not leave, so it ran itself again as a child that did, and the child
    /// has ended. The caller exits with this code and does nothing else.
    Relaunched(i32),
}

/// Set on the child [`leave`] starts, to the pid of the process that started it, so that it
/// never starts another and knows its standard input is that process's pipe. Never passed on:
/// chats, shells and ACP agents are started without it, so an app started from one of them
/// leaves its own terminal.
pub const RELAUNCHED_ENV: &str = "PURLIS_HOST_LEFT_ITS_TERMINAL";

/// Whether this process is the child [`relaunch`] started: the marker names its parent.
#[cfg(unix)]
fn relaunched_by_parent() -> bool {
    let parent = rustix::process::getppid().map(|pid| pid.as_raw_nonzero().get());
    crate::envvar::var(RELAUNCHED_ENV)
        .and_then(|value| value.parse::<i32>().ok())
        .is_some_and(|marked| Some(marked) == parent)
}

/// The relaunched child's standard input is a pipe whose other end only its parent holds. It is
/// moved off standard input, which becomes `/dev/null`, and watched: at its end the parent is
/// gone, SIGKILLed even, and this process ends as a terminal's hangup would end it, rather than
/// live on as a window nothing started.
#[cfg(unix)]
fn end_with_the_parent() -> std::io::Result<()> {
    use std::io::Read;
    use std::os::fd::AsFd;
    let watched = std::io::stdin().as_fd().try_clone_to_owned()?;
    rustix::stdio::dup2_stdin(std::fs::File::open("/dev/null")?)?;
    std::thread::Builder::new()
        .name("parent watch".to_owned())
        .spawn(move || {
            let mut pipe = std::fs::File::from(watched);
            let mut byte = [0u8; 1];
            while matches!(pipe.read(&mut byte), Ok(1..)) {}
            hang_up();
        })?;
    Ok(())
}

/// How often a host in a session of its own looks for its parent.
#[cfg(unix)]
const PARENT_LOOKED_FOR: std::time::Duration = std::time::Duration::from_millis(200);

/// A host that made a session of its own no longer hears its terminal's Ctrl-C or hangup, which
/// reached it through the session it left. The program that started it does still hear them,
/// and some (tauri-cli's `dev`) end on Ctrl-C without ending it. So the host ends when its
/// parent is gone, as the hangup would have ended it: it looks for its parent every
/// [`PARENT_LOOKED_FOR`], and a new parent id means the old one is gone.
#[cfg(unix)]
fn end_when_the_parent_goes(parent: rustix::process::Pid) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("parent watch".to_owned())
        .spawn(move || {
            while rustix::process::getppid() == Some(parent) {
                std::thread::sleep(PARENT_LOOKED_FOR);
            }
            hang_up();
        })?;
    Ok(())
}

/// Ends this process as a terminal's hangup would.
#[cfg(unix)]
fn hang_up() -> ! {
    let _ = signal_hook::low_level::emulate_default_handler(signal_hook::consts::SIGHUP);
    std::process::exit(128 + signal_hook::consts::SIGHUP);
}

/// Leaves this process's controlling terminal, if it has one. Called first thing by the host.
pub fn leave() -> std::io::Result<Left> {
    #[cfg(unix)]
    {
        let relaunched = relaunched_by_parent();
        if relaunched {
            end_with_the_parent()?;
        }
        if !has_one() {
            return Ok(Left::NoneToLeave);
        }
        let parent = rustix::process::getppid();
        match rustix::process::setsid() {
            Ok(_) => {
                if let Some(parent) = parent {
                    end_when_the_parent_goes(parent)?;
                }
                Ok(Left::NewSession)
            }
            Err(rustix::io::Errno::PERM) if !relaunched => relaunch().map(Left::Relaunched),
            Err(err) => Err(err.into()),
        }
    }
    #[cfg(not(unix))]
    Ok(Left::NoneToLeave)
}

/// Whether this process has a controlling terminal: whether `/dev/tty` opens.
pub fn has_one() -> bool {
    #[cfg(unix)]
    {
        use rustix::fs::{Mode, OFlags};
        rustix::fs::open(
            "/dev/tty",
            OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .is_ok()
    }
    #[cfg(not(unix))]
    false
}

/// Runs this program again, with the same arguments, as a child that is no group's leader,
/// passes it the signals a terminal sends, and returns the code it ended with.
#[cfg(unix)]
fn relaunch() -> std::io::Result<i32> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    use std::os::unix::process::ExitStatusExt;

    // The child's standard input: a pipe only this process writes to, so the child sees it end
    // when this process does, however it ends.
    let (watched, held) = std::io::pipe()?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .args(std::env::args_os().skip(1))
        .env(RELAUNCHED_ENV, std::process::id().to_string())
        .stdin(watched);
    // Taken before the child exists, so a signal that comes while it starts is not lost.
    let mut signals = signal_hook::iterator::Signals::new([SIGHUP, SIGINT, SIGQUIT, SIGTERM])?;
    let handle = signals.handle();
    let mut child = crate::forklock::spawn(&mut command)?;
    let pid = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw);
    let forwarding = std::thread::spawn(move || {
        for signal in signals.forever() {
            if let (Some(pid), Some(signal)) =
                (pid, rustix::process::Signal::from_named_raw(signal))
            {
                let _ = rustix::process::kill_process(pid, signal);
            }
        }
    });
    let status = child.wait()?;
    drop(held);
    handle.close();
    let _ = forwarding.join();
    if let Some(signal) = status.signal() {
        // Ended by a signal: end by the same one, so a shell reads what happened as it would
        // have without the relaunch. Only if that fails is it a code.
        let _ = signal_hook::low_level::emulate_default_handler(signal);
        return Ok(128 + signal);
    }
    Ok(status.code().unwrap_or(1))
}
