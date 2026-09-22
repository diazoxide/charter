//! Is the process that wrote this marker still there?
//!
//! **One answer, in one place** (charter-app#102). Two modules asked exactly this question
//! and their `cfg(not(unix))` arms answered it in opposite directions.
//! [`crate::glstate`]'s refresh lock read every pid as GONE, so nothing would ever have
//! stopped a second refresher starting beside the first; [`crate::news`]'s probe marker read
//! every pid as THERE, so a stale marker would have suppressed the check for ever. Each had a
//! defensible reason of its own — fail-open for a lock, fail-closed for a mutation — and
//! `news`'s cited `charter/news.py` for it, which is the right instinct. But two answers to
//! one question on one platform, written on different days, is not a platform decision; it is
//! two guesses.
//!
//! They also disagreed on POSIX, which the issue did not say and nothing had noticed: `news`
//! read **every errno but `ESRCH`** as alive, and `glstate` read everything but `ESRCH` and
//! `EPERM` as gone. No errno charter can provoke falls in that gap — the null signal is
//! always valid and the pid is always one `Pid::from_raw` accepted — so the two never
//! differed in a run. They differed in what they would do with an answer nobody predicted,
//! which is the only kind of answer this matters for.
//!
//! # The direction, and why it is this one
//!
//! **Not alive, unless charter has positive evidence that it is.** `Ok` is the kernel saying
//! the process exists; `EPERM` is the kernel saying it exists and is somebody else's to
//! signal, which is still a process. Everything else — an errno charter cannot place, and
//! every platform where charter cannot ask at all — is *gone*.
//!
//! The two mistakes are not the same size, and that is the whole argument:
//!
//! - A wrong **"alive"** is **permanent**. Nothing clears it. `glstate`'s lock would name a
//!   pid that reads as in flight for ever and no refresh would ever run again; `news`'s
//!   marker would turn every probe on that machine into `unknown` for good. `news`'s own
//!   docstring says this out loud about a stale marker — *"a scrap of stale environment would
//!   turn every probe on that machine into `unknown` for good and say nothing about why"* —
//!   and then its `cfg(not(unix))` arm did exactly the believing it warns about.
//! - A wrong **"gone"** is **transient**, and both callers already carry the cost. `glstate`
//!   has [`crate::glstate::SPAWN_COOLDOWN`] and [`crate::glstate::STUCK_AFTER`] standing
//!   behind the pid for precisely this — its own docstring calls the pid check the thing that
//!   stops a RECYCLED pid suppressing refreshes, so a duplicate refresh is a cost it is built
//!   to absorb. `news` re-runs a `check:` that was already running once, and the round after
//!   that asks again.
//!
//! So the failure this refuses is the unrecoverable one, and the failure it accepts is the
//! one the next round undoes. It costs `news` something real off POSIX — a descendant would
//! not decline a mutation inside a probe it could not see — and that is named here rather
//! than hidden, because it is the price of not wedging a machine no operator could debug.
//!
//! # This is a placeholder for a real answer, not a platform policy
//!
//! **It compiles on Windows; it has never run there** — a narrower claim than charter-app#102's
//! *"neither arm has ever been compiled"*, and worth measuring rather than repeating. The
//! `windows (build, test, evidence only)` job builds `charter-cli` on `windows-latest` on every
//! PR, and `charter-core`'s library compiles there with one warning, so this arm has been
//! compiled on every run since that job landed. What does not compile there is part of the
//! TEST tree, which is why nothing below has ever been executed on the platform it is for.
//!
//! Windows has a real answer:
//! `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, …)` then `GetExitCodeProcess`
//! distinguishes "gone" from "there and not ours" the same way `kill(pid, 0)` does with
//! `ESRCH` and `EPERM`. It is Win32, so under `unsafe_code = "forbid"` it means a crate —
//! `sysinfo` would do it with no `unsafe` in charter. That belongs with the rest of the
//! Windows question (charter-app#93, M4), where the dependency can be weighed once for the
//! whole core instead of guessed at twice here. When it lands, it lands in this function and
//! both callers get it.

/// Whether `pid` is a live process — `charter/glstate.py:_alive`, and the same question
/// `charter/news.py` asks of its probe marker.
///
/// The null signal checks for existence and delivers nothing. See the module docstring for
/// why `EPERM` is alive, why every other errno is not, and why a platform charter cannot ask
/// answers *gone*.
#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    // `Pid::from_raw` refuses 0, which is the whole process group rather than a process, and
    // any negative — so a marker holding either is not a process charter may ask about.
    let Some(pid) = rustix::process::Pid::from_raw(raw) else {
        return false;
    };
    match rustix::process::test_kill_process(pid) {
        Ok(()) => true,
        Err(rustix::io::Errno::PERM) => true,
        Err(_) => false,
    }
}

/// Off POSIX charter cannot ask, so it does not claim — see the module docstring.
///
/// **Compiled on Windows and exercised by nothing**: the evidence-only job builds this and
/// cannot run the tests. So it is written to be the same decision the unix arm makes when it
/// has no evidence — one direction rather than two — and the day charter-app#93 gives Windows
/// a real answer there is one place to put it.
#[cfg(not(unix))]
pub fn alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one question this module answers, against the two pids a test can be sure of.
    #[test]
    fn this_process_is_alive_and_one_that_has_been_reaped_is_not() {
        assert!(alive(std::process::id()));

        let mut gone =
            crate::forklock::spawn(std::process::Command::new("/bin/sh").args(["-c", "exit 0"]))
                .expect("a program starts");
        let pid = gone.id();
        gone.wait().expect("it ends");

        assert!(!alive(pid), "a reaped pid is not a process still running");
    }

    /// charter-app#102. A pid that is not a pid is not a process, in every caller at once.
    ///
    /// `0` is the whole process group to `kill(2)` and debris to everything that reads a
    /// marker, and a `u32` that does not fit an `i32` is debris too. Both callers used to
    /// answer this themselves; both now answer it here.
    #[test]
    fn nothing_that_is_not_a_pid_is_a_live_process() {
        assert!(!alive(0), "pid 0 is not a process charter may ask about");
        assert!(
            !alive(u32::MAX),
            "a pid that does not fit an i32 is not one the kernel could have given out"
        );
    }
}
