//! The executor: how charter starts an approved extension's program, what it hands it, what it
//! will take back, and how it stops it. **charter ADR 0041, stage 2.**
//!
//! This is the first place charter runs code a stranger wrote, with the operator's authority.
//! Everything below is built around one sentence the consent dialog already says —
//! [`crate::extension::FINGERPRINTED`]: *"charter has read every file in this extension's
//! directory … and will ask again if any of them changes."* **This module is what makes that
//! sentence enforceable**, and the order of what it does is the argument:
//!
//! 1. **The gate, re-taken at the press** ([`cleared`]). The record is read, the entry must be
//!    approved, and the extension's whole directory is fingerprinted again *now* —
//!    [`crate::extension::read_at`], the same walk that produced the fingerprint the operator
//!    said yes to. Anything but [`Standing::Approved`] starts nothing and says why. A survey
//!    taken when the window opened is never the evidence: a button drawn from it is a question
//!    the operator may ask, and this is where the question is checked.
//! 2. **One question, one answer, one process.** The program is started for the question and
//!    stopped after it. No daemon, no standing subscription, no second request on the same
//!    process — ADR 0041's minimum capability is *"one round trip per deliberate human
//!    action"*, and a process that lives only as long as the round trip has no state for a
//!    later question to find and no time in which to be doing anything nobody asked for.
//! 3. **Bounded in every direction charter controls** — time ([`DEADLINE`]), the answer's size
//!    ([`MOST_ANSWER_BYTES`]), what is kept of its stderr ([`MOST_STDERR_BYTES`]), and one
//!    question in flight per extension. A stalled, looping, flooding or crashing program costs
//!    the operator one refusal in the surface he opened, and nothing else: this runs on a
//!    blocking thread the window never waits on, and every wait in it has a deadline.
//! 4. **Answered in the panel vocabulary** ([`crate::panel::answered`]) and nothing else. A
//!    program cannot put markup, a verb, a colour or a file in the window; it can put a note, a
//!    list or a chart there, which charter draws.
//!
//! # A subprocess over a unix socket, and which one
//!
//! ADR 0041 decision 1 and its amendment: *"a subprocess over a unix socket, no OS sandbox."*
//! The socket is a **`socketpair`**, not a path: charter makes both ends, keeps one, and hands
//! the program the other as its standard input and standard output. Three things follow, and
//! they are why it is this rather than a bound socket file:
//!
//! - **Nothing else can connect.** A socketpair has no name on the filesystem, so there is no
//!   `0600` to get right, no directory to create at the right mode, and no race between
//!   binding and chmod — `hookwire`'s review-found defect cannot recur here because the thing
//!   it was about does not exist. And one extension's program cannot reach another's channel,
//!   because there is no address to reach it at.
//! - **The program is an ordinary program.** It reads a line on stdin and prints a line on
//!   stdout, so an author runs it in a terminal, pastes a request in, and reads the answer —
//!   ADR 0041's first argument for a subprocess, *"its whole conversation with charter is a log
//!   a human can read."* That is LSP's and MCP's shape, which is the standard-practice half.
//! - **Windows refuses rather than degrades** (ADR 0031, gate item 5). Rust's standard library
//!   has no `AF_UNIX` there, exactly as `hookwire`'s Windows arm already found; [`ask`] answers
//!   [`REFUSED_HERE`] before it reads anything.
//!
//! # What it is NOT, said beside the code rather than after it
//!
//! **Not a sandbox.** The operator ruled on 2026-09-22 to ship without one, having been shown
//! what that costs. The program runs as him, with his files, his network and his ability to
//! start programs. It can write anywhere he can — outside its state directory, into the machine
//! store, into `charter.local.toml` — and nothing here stops it; the consent dialog says so in
//! [`crate::extension::RUNS_AS_YOU`], from the core, and [`HOW_IT_RUNS`] says what charter does
//! bound. Everything this module does is **charter's conduct**: what charter starts, when, with
//! what, and for how long.
//!
//! **Not proof against a program set on outliving its question.** It is started in its own
//! process group and the whole group is killed when the question is over, so a helper it left
//! running goes with it. A program that calls `setsid` or double-forks out of the group is a
//! program that decided to escape, and — running as the operator — it can. What closes the
//! ordinary case is the group; what would close the deliberate one is the sandbox that was
//! declined.
//!
//! **Not atomic with the fingerprint.** The tree is hashed, and then the program is started by
//! path. A write between the two is run without having been hashed. That is ADR 0028's race —
//! *containment checks a path and does not hold it* — and it is accepted here on 0028's own
//! ground, which the 2026-09-22 amendment re-examined for exactly this runtime: with no
//! sandbox, a writer who can win that race already runs as the operator and needs no race.
//! Closing it would mean executing from a descriptor (`fexecve`, which needs `unsafe`) or from
//! a private copy of the binary (a new, unseen executable per press, which macOS assesses before
//! it runs — measured on this machine as the thing that hangs). Named, not closed.

use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::extension::{self, Extension, Standing};
use crate::panel::{self, Subject};

/// The protocol a request and an answer are written in.
///
/// **It is inside the fingerprint of every extension that declares a program**
/// ([`crate::extension`]'s tree hash), so changing what charter hands a program re-asks every
/// operator who approved one under the old terms. Bump it when a request gains a fact.
pub const PROTOCOL: u32 = 1;

/// How long a program has, from being started to its answer's last byte.
///
/// **Five seconds, and it is a bound on the operator's patience rather than on the work.** The
/// persona statistics producer answers in milliseconds (measured in this change's PR); a view
/// is something the operator opened and is looking at, and a surface that spins for longer than
/// this is one he has already given up on. A program that needs longer is doing work that
/// belongs in its state directory ahead of time, not in the round trip.
pub const DEADLINE: Duration = Duration::from_secs(5);

/// How long a program that has answered is given to finish on its own before its group is
/// killed. Long enough to flush a file it was writing into its state directory; short enough
/// that nothing it started is still running when the operator looks at the answer.
pub const GRACE: Duration = Duration::from_millis(250);

/// The most an answer may be. A view is one surface, and half a megabyte of JSON is a
/// thousand-row list with a card on every row — more than anybody reads in one sitting, and far
/// below anything that would hold the window's memory. The vocabulary's own bounds
/// ([`panel::MOST_BLOCKS`], [`panel::MOST_POINTS`], `MOST_ROWS`) still apply to what gets
/// through.
pub const MOST_ANSWER_BYTES: usize = 512 << 10;

/// The most of a program's stderr charter keeps — the tail, which is where a panic or an
/// error message is. It is quoted in the refusal when the program did not answer, because
/// "it crashed" is not a sentence anybody can act on and the program's own last words are.
pub const MOST_STDERR_BYTES: usize = 2 << 10;

/// How long charter goes on reading a program's stderr once the program has been stopped —
/// long enough for the last words of one that crashed to be read out of the socket, bounded
/// because a process that escaped the group can keep writing to it for ever
/// ([`Executor::converse`]).
#[cfg_attr(not(unix), allow(dead_code))]
const STDERR_AFTER_STOP: Duration = Duration::from_millis(100);

/// What charter says on a platform where it will not start an extension's program.
pub const REFUSED_HERE: &str = "charter does not start an extension's program on this platform: \
     it talks to one over a unix socket, and this platform has none charter can use without \
     writing unsafe code. A guard that cannot be expressed refuses rather than degrades \
     (charter ADR 0031).";

/// What charter tells the operator it does with a program, in the consent dialog.
///
/// **The core's words, for [`crate::extension::RUNS_AS_YOU`]'s reason**: a dialog that
/// composed its own sentence about when a program runs could drift kinder than the truth one
/// edit at a time. It says what charter bounds, and it ends by pointing at what charter does
/// not — which RUNS_AS_YOU then says in full.
pub const HOW_IT_RUNS: &str = "charter starts it only when you open one of this extension's \
     views — never on its own, never at launch and never in the background. It is asked one \
     question, given 5 seconds to answer, and then stopped along with anything it started. \
     A program set on outliving that can, because it runs as you do.";

/// What a program answered, drawn by charter, and what it cost.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The blocks, parsed against the panel vocabulary.
    pub blocks: Vec<panel::Block>,
    /// How long the gate took — re-reading the record and re-hashing the whole tree.
    pub gate: Duration,
    /// How long the program took, from being started to its answer being read.
    pub round_trip: Duration,
}

/// The executor, as a value the app holds for as long as it runs.
///
/// It holds one thing: **which extensions have a program running right now, and the process
/// group each one is in.** That is what lets it refuse a second question to a program still
/// answering the first, and what lets [`Self::stop_all`] kill every one of them when the window
/// goes, so that no program an extension was asked to run outlives the app that asked.
#[derive(Debug, Default)]
pub struct Executor {
    running: Mutex<std::collections::BTreeMap<String, i32>>,
}

impl Executor {
    /// Ask `extension`'s program about `view`, or say why charter will not.
    ///
    /// `hand` builds what the program is given, from the view's subject, and is called only
    /// once the gate has passed — so a refused extension costs no plane read at all.
    /// `focus` is the one thing on screen the operator opened it from, when there is one (a
    /// persona's card), and is handed as a name.
    ///
    /// **Every `Err` is a sentence the operator can act on**, naming the extension and saying
    /// what to do: open Extensions and approve it again, make the file runnable, look at what
    /// the program printed. None of them is a stack trace and none is silence.
    pub fn ask(
        &self,
        config_root: &Path,
        extension: &str,
        view: &str,
        focus: Option<&str>,
        hand: impl FnOnce(Subject) -> serde_json::Value,
    ) -> Result<Answer, String> {
        supported()?;
        let began = Instant::now();
        let found = cleared(config_root, extension)?;
        let Some(asked) = found.manifest.views.iter().find(|it| it.id == view) else {
            return Err(format!(
                "'{extension}' has no view called '{view}'. The window's list of what is in \
                 force is taken when it opens; reopen the window to see what '{extension}' \
                 offers now."
            ));
        };
        let program = found.manifest.program.as_deref().ok_or_else(|| {
            // `views_of` refuses a view with no program at parse, so this is a manifest that
            // changed shape between two reads of the same bytes — which cannot happen, and is
            // answered rather than unwrapped because this is the path the window waits on.
            format!("'{extension}' declares no program to answer its view '{view}'")
        })?;
        let at = found.path.join(program);
        runnable(&at, extension)?;
        let gate = began.elapsed();

        let request = request_line(extension, asked, focus, hand(asked.about))?;
        let _held = self.hold(extension)?;
        let started = Instant::now();
        let line = self.converse(extension, &found, &at, &request)?;
        let round_trip = started.elapsed();

        let blocks = read_answer(extension, &line)?;
        Ok(Answer {
            blocks,
            gate,
            round_trip,
        })
    }

    /// Kill every program an extension was asked to run and has not finished. For the app's
    /// exit, so that nothing charter started outlives the window that started it.
    ///
    /// **Only what is still in the table**, and an entry leaves the table *before* its program
    /// is reaped ([`Self::converse`]), so a process group named here is always one whose leader
    /// has not been reaped — its id cannot have been given to anything else yet.
    pub fn stop_all(&self) {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        for group in running.values() {
            kill_group(*group);
        }
    }

    /// Which extensions have a program running right now. For a test, and for anything that
    /// wants to say so.
    pub fn running(&self) -> Vec<String> {
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// Claim `extension`'s one slot, or say it is busy.
    ///
    /// **One question in flight per extension**, so a double click, an impatient operator or a
    /// window that asks twice does not start a second copy of a program that is already
    /// running — and so one extension that stalls holds one slot of its own and nobody else's.
    fn hold(&self, extension: &str) -> Result<Held<'_>, String> {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if running.contains_key(extension) {
            return Err(format!(
                "'{extension}' is still answering the last thing it was asked. charter asks an \
                 extension one thing at a time; this one has at most {} left.",
                DEADLINE.as_secs()
            ));
        }
        // Zero until the program exists: `stop_all` never signals a group of 0, which would be
        // charter's own.
        running.insert(extension.to_owned(), 0);
        Ok(Held {
            by: self,
            extension: extension.to_owned(),
        })
    }

    /// Record the process group a running program is in, so [`Self::stop_all`] can reach it.
    #[cfg_attr(not(unix), allow(dead_code))]
    fn started(&self, extension: &str, group: i32) {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = running.get_mut(extension) {
            *slot = group;
        }
    }

    /// Take the process group out of the table, before it is killed and reaped. See
    /// [`Self::stop_all`] for why the order is load-bearing.
    #[cfg_attr(not(unix), allow(dead_code))]
    fn finished(&self, extension: &str) {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = running.get_mut(extension) {
            *slot = 0;
        }
    }

    #[cfg(unix)]
    fn converse(
        &self,
        extension: &str,
        found: &Extension,
        program: &Path,
        request: &[u8],
    ) -> Result<Vec<u8>, String> {
        use std::io::{Read, Write};
        use std::os::fd::OwnedFd;
        use std::os::unix::net::UnixStream;
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let could_not =
            |why: std::io::Error| format!("charter could not start '{extension}''s program: {why}");
        let (ours, theirs) = UnixStream::pair().map_err(could_not)?;
        let (err_ours, err_theirs) = UnixStream::pair().map_err(could_not)?;

        let mut command = Command::new(program);
        command
            // **Nothing of charter's environment but what a program needs to be a program.**
            // charter's own process carries its plane fence, its launch log, whatever the
            // operator's shell exported into the app — none of which is the extension's
            // business. ADR 0041's table: the harness environment is not grantable, and a
            // variable handed to a program is a variable granted.
            .env_clear()
            .envs(environment(found))
            .current_dir(&found.path)
            // Its own process group, so the whole of what it starts is one thing to kill.
            .process_group(0)
            .stdin(Stdio::from(OwnedFd::from(
                theirs.try_clone().map_err(could_not)?,
            )))
            .stdout(Stdio::from(OwnedFd::from(theirs)))
            .stderr(Stdio::from(OwnedFd::from(err_theirs)));
        // `forklock`, never `Command::spawn` (charter-app#53): this process opens terminals, and
        // a program forked while one is half-open inherits a chat's pty and holds it for as long
        // as it lives. An extension's program is the last thing that should be holding a chat.
        let spawned = crate::forklock::spawn(&mut command);
        // **The program's ends are dropped here, with the Command that owns them**, so the only
        // holders of the socket's far side are the program and whatever it starts. Without
        // this, charter's own copy would keep the socket open and "the program closed its
        // output" could never be seen.
        drop(command);
        let mut child = spawned.map_err(|why| match why.kind() {
            std::io::ErrorKind::PermissionDenied => format!(
                "'{}' could not be started: {why}. charter runs an extension's program \
                 directly, never through a shell, so it has to be executable by you.",
                program.display()
            ),
            _ => format!(
                "'{}' could not be started: {why}. If it is a script, its first line has to \
                 name an interpreter that exists on this machine.",
                program.display()
            ),
        })?;
        let group = i32::try_from(child.id()).unwrap_or(0);
        self.started(extension, group);

        // Its stderr is drained while it runs — a program that logs more than a socket buffer
        // would otherwise stall on its own diagnostics and read as hung — and only the tail is
        // kept.
        let stop = Arc::new(AtomicBool::new(false));
        let tail = {
            let stop = Arc::clone(&stop);
            let mut err_ours = err_ours;
            std::thread::spawn(move || {
                let _ = err_ours.set_read_timeout(Some(Duration::from_millis(50)));
                let mut kept: Vec<u8> = Vec::new();
                let mut chunk = [0u8; 1024];
                // When charter said stop. What a program wrote just before it died is still in
                // the socket, and it is the part worth quoting, so the drain goes on after stop
                // — but for a bounded TIME, not a bounded number of bytes: a process that
                // escaped the group (`setsid`) can hold stderr open and trickle into it for
                // ever, and a byte cap is then a wait of however long it takes to trickle that
                // many (measured: a byte a millisecond held a 64 KiB cap for 86 s). Either way
                // the cost was this thread, and the extension's one slot, held with it.
                let mut stopped_at: Option<Instant> = None;
                loop {
                    if stopped_at.is_none() && stop.load(Ordering::Relaxed) {
                        stopped_at = Some(Instant::now());
                    }
                    if stopped_at.is_some_and(|at| at.elapsed() > STDERR_AFTER_STOP) {
                        break;
                    }
                    match err_ours.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(got) => {
                            kept.extend_from_slice(&chunk[..got]);
                            if kept.len() > MOST_STDERR_BYTES {
                                let cut = kept.len() - MOST_STDERR_BYTES;
                                kept.drain(..cut);
                            }
                        }
                        Err(_) if stopped_at.is_none() => {}
                        Err(_) => break,
                    }
                }
                kept
            })
        };

        // The question is written on a thread of its own, so a program that answers before it
        // has read the whole of it cannot deadlock against charter reading the answer.
        let writer = {
            let mut ours = ours.try_clone().map_err(could_not)?;
            let request = request.to_vec();
            std::thread::spawn(move || {
                let _ = ours.set_write_timeout(Some(DEADLINE));
                let wrote = ours.write_all(&request).is_ok();
                // EOF after the question: a program may read "all of stdin" rather than a line.
                let _ = ours.shutdown(std::net::Shutdown::Write);
                wrote
            })
        };

        let heard = listen(&ours);

        // **The order below is load-bearing** (see `stop_all`): out of the table, then the
        // whole group killed, then reaped — so a group id charter signals is never one whose
        // leader has already been reaped and whose number the system could have reused.
        self.finished(extension);
        kill_group(group);
        // And the program itself, by the pid charter holds and has not reaped. A program that
        // moved its own process out of the group it was started in (`setpgid`) is not reached
        // by the group kill, and `wait` below would then wait for it for as long as it liked —
        // a blocking thread, and this extension's slot, held for ever.
        let _ = child.kill();
        let status = child.wait().ok();
        stop.store(true, Ordering::Relaxed);
        let said = tail.join().unwrap_or_default();
        let wrote = writer.join().unwrap_or(false);

        let last_words = || {
            let text = String::from_utf8_lossy(&said);
            let text = text.trim();
            if text.is_empty() {
                " It printed nothing to stderr.".to_owned()
            } else {
                format!(
                    " The last of what it printed: {}",
                    crate::shown::readable(text, MOST_STDERR_BYTES)
                )
            }
        };
        match heard {
            Heard::Line(line) => Ok(line),
            Heard::TooLate => Err(format!(
                "'{extension}' did not answer within {} seconds, so charter stopped it. It was \
                 asked one question{}.{}",
                DEADLINE.as_secs(),
                if wrote {
                    ""
                } else {
                    ", and it never finished reading it"
                },
                last_words()
            )),
            Heard::TooMuch => Err(format!(
                "'{extension}' answered more than {} KiB, so charter stopped it without drawing \
                 any of it. A view is one surface; a program that needs more is answering a \
                 different question.",
                MOST_ANSWER_BYTES >> 10
            )),
            Heard::Nothing(partial) => {
                let ended = status.map_or_else(
                    || "ended".to_owned(),
                    |status| match status.code() {
                        Some(code) => format!("exited with status {code}"),
                        None => "was killed by a signal".to_owned(),
                    },
                );
                Err(format!(
                    "'{extension}' {ended} without answering{}.{}",
                    if partial {
                        " — it wrote part of a line and never finished it"
                    } else {
                        ""
                    },
                    last_words()
                ))
            }
            Heard::Broken(why) => Err(format!(
                "charter lost its connection to '{extension}''s program: {why}.{}",
                last_words()
            )),
        }
    }

    #[cfg(not(unix))]
    fn converse(
        &self,
        _extension: &str,
        _found: &Extension,
        _program: &Path,
        _request: &[u8],
    ) -> Result<Vec<u8>, String> {
        Err(REFUSED_HERE.to_owned())
    }
}

/// An extension's one slot, released however the question ends.
struct Held<'a> {
    by: &'a Executor,
    extension: String,
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.by
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.extension);
    }
}

/// Whether charter starts extension programs on this platform at all.
#[cfg(unix)]
fn supported() -> Result<(), String> {
    Ok(())
}

#[cfg(not(unix))]
fn supported() -> Result<(), String> {
    Err(REFUSED_HERE.to_owned())
}

/// **The gate**: `extension` as it is on disk right now, if and only if the operator approved
/// exactly these bytes at exactly this path.
///
/// It is public so that it can be tested on its own, and because it is the one question every
/// later caller that starts anything must ask. Each refusal is a state that ADR 0041 decision 3
/// says *means ask* — *"a missing file, a malformed one, an entry that is not a fingerprint, a
/// link, a FIFO, a planted giant — each reads as no record, never as approval"* — and each says
/// so in words the operator can act on.
pub fn cleared(config_root: &Path, extension: &str) -> Result<Extension, String> {
    let loaded = extension::read(config_root);
    if let Some(why) = &loaded.unreadable {
        return Err(format!(
            "charter could not read its record of which extensions you approved, so it starts \
             none of them: {why}"
        ));
    }
    let Some(entry) = loaded.entry(extension) else {
        return Err(format!(
            "no extension called '{extension}' is installed on this machine, so there is \
             nothing to start. Install it from Extensions."
        ));
    };
    if entry.approved.is_none() {
        return Err(format!(
            "you have not approved '{extension}', so charter will not start its program. Open \
             Extensions to see what it declares and decide."
        ));
    }
    // **Re-read now, the whole tree**, and never a fingerprint remembered from a survey. This
    // is the line that makes "will ask again if any of them changes" true at the moment it
    // matters, which is the moment something is about to run.
    let found = extension::read_at(&entry.path).map_err(|why| {
        format!(
            "charter could not re-read '{extension}' just now, so it will not start anything \
             from it: {why}"
        )
    })?;
    let standing = if found.id() == extension {
        loaded.standing(&found)
    } else {
        Standing::Changed
    };
    if standing != Standing::Approved {
        return Err(format!(
            "'{extension}' has changed since you approved it — charter re-read its directory \
             just now and it is not what you said yes to, so charter will not run it. Open \
             Extensions: charter will show you what it declares now and ask again."
        ));
    }
    Ok(found)
}

/// Refuse a program that is not a file charter can start directly.
fn runnable(at: &Path, extension: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let found = std::fs::symlink_metadata(at)
            .map_err(|why| format!("'{}' could not be read: {why}", at.display()))?;
        if found.permissions().mode() & 0o111 == 0 {
            return Err(format!(
                "'{}' is '{extension}''s program and is not executable. charter starts it \
                 directly, never through a shell. Making it executable changes the \
                 extension's fingerprint, so charter will ask you about it again first.",
                at.display()
            ));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (at, extension);
    }
    Ok(())
}

/// The environment a program is started with: the few variables that make a program work,
/// taken from charter's own when charter has them, and the three that say what it is.
fn environment(found: &Extension) -> Vec<(String, String)> {
    const PASSED: [&str; 8] = [
        "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR",
    ];
    let mut env: Vec<(String, String)> = PASSED
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).to_owned(), value))
        })
        .collect();
    env.push(("CHARTER_EXTENSION".into(), found.id().to_owned()));
    env.push(("CHARTER_PROTOCOL".into(), PROTOCOL.to_string()));
    if let Some(state) = &found.manifest.state {
        env.push((
            "CHARTER_EXTENSION_STATE".into(),
            found.path.join(state).display().to_string(),
        ));
    }
    env
}

/// The one line a program is asked.
fn request_line(
    extension: &str,
    view: &extension::View,
    focus: Option<&str>,
    given: serde_json::Value,
) -> Result<Vec<u8>, String> {
    let mut doc = serde_json::Map::new();
    doc.insert("charter".into(), PROTOCOL.into());
    doc.insert("extension".into(), extension.into());
    doc.insert("view".into(), view.id.clone().into());
    doc.insert("about".into(), view.about.as_str().into());
    doc.insert(
        "focus".into(),
        focus.map_or(serde_json::Value::Null, Into::into),
    );
    doc.insert("given".into(), given);
    let mut line = serde_json::to_vec(&serde_json::Value::Object(doc))
        .map_err(|why| format!("charter could not write the question for '{extension}': {why}"))?;
    line.push(b'\n');
    Ok(line)
}

/// What charter heard on the socket.
#[cfg(unix)]
enum Heard {
    /// One whole line, without its newline.
    Line(Vec<u8>),
    /// The deadline passed first.
    TooLate,
    /// More than [`MOST_ANSWER_BYTES`] arrived without a newline.
    TooMuch,
    /// The program closed its output first; `true` when it had written part of a line.
    Nothing(bool),
    /// The socket failed.
    Broken(String),
}

/// Read one line off `ours`, within [`DEADLINE`] of now, and then give the program [`GRACE`]
/// to close its end on its own.
#[cfg(unix)]
fn listen(ours: &std::os::unix::net::UnixStream) -> Heard {
    use std::io::Read;
    let mut ours = ours;
    let until = Instant::now() + DEADLINE;
    let mut heard: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 16 << 10];
    let line = loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Heard::TooLate;
        }
        if ours.set_read_timeout(Some(left)).is_err() {
            return Heard::Broken("its read deadline could not be set".into());
        }
        match ours.read(&mut chunk) {
            Ok(0) => return Heard::Nothing(!heard.is_empty()),
            Ok(got) => {
                let before = heard.len();
                heard.extend_from_slice(&chunk[..got]);
                if let Some(at) = memchr::memchr(b'\n', &heard[before..]) {
                    heard.truncate(before + at);
                    break heard;
                }
                if heard.len() > MOST_ANSWER_BYTES {
                    return Heard::TooMuch;
                }
            }
            Err(why)
                if matches!(
                    why.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Heard::TooLate;
            }
            Err(why) if why.kind() == std::io::ErrorKind::Interrupted => {}
            Err(why) => return Heard::Broken(why.to_string()),
        }
    };
    if line.len() > MOST_ANSWER_BYTES {
        return Heard::TooMuch;
    }
    // The answer is in. What follows is courtesy: a program that is flushing a cache file into
    // its state directory gets a moment to finish before its group is killed. Whatever it
    // writes in that moment is not read as anything.
    let grace = Instant::now() + GRACE;
    loop {
        let left = grace.saturating_duration_since(Instant::now());
        if left.is_zero() || ours.set_read_timeout(Some(left)).is_err() {
            break;
        }
        match ours.read(&mut chunk) {
            Ok(0) => break,
            Ok(_) => {}
            Err(why) if why.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    Heard::Line(line)
}

/// Kill a program's whole process group.
///
/// **Never a group of 0 or 1.** `kill(0, …)` is charter's own group and `kill(-1, …)` is every
/// process the operator owns; a group id that somehow arrived as either is refused here rather
/// than trusted to be impossible.
#[cfg(unix)]
fn kill_group(group: i32) {
    if group <= 1 {
        return;
    }
    if let Some(pid) = rustix::process::Pid::from_raw(group) {
        // ESRCH is the ordinary answer for a program that exited and started nothing.
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
}

#[cfg(not(unix))]
fn kill_group(_group: i32) {}

/// The blocks in an answer line, or why charter will not draw them.
///
/// **An answer says which protocol it is in.** A program written against a different one is
/// refused with the number named, rather than half-read under rules it was not written to.
fn read_answer(extension: &str, line: &[u8]) -> Result<Vec<panel::Block>, String> {
    let doc: serde_json::Value = serde_json::from_slice(line).map_err(|why| {
        format!("'{extension}' answered something that is not one line of JSON: {why}")
    })?;
    let doc = doc
        .as_object()
        .ok_or_else(|| format!("'{extension}' answered JSON that is not an object"))?;
    for key in doc.keys() {
        if !["charter", "blocks", "error"].contains(&key.as_str()) {
            return Err(format!(
                "'{extension}' answered {key:?}, which is not part of charter's protocol {PROTOCOL} \
                 — an answer is 'charter', and either 'blocks' or 'error'"
            ));
        }
    }
    match doc.get("charter").and_then(serde_json::Value::as_u64) {
        Some(found) if found == u64::from(PROTOCOL) => {}
        Some(found) => {
            return Err(format!(
                "'{extension}' answered in protocol {found}, and this charter speaks protocol \
                 {PROTOCOL}"
            ));
        }
        None => {
            return Err(format!(
                "'{extension}' answered without saying which protocol it speaks ('charter': \
                 {PROTOCOL}), so charter cannot say what its answer means"
            ));
        }
    }
    if let Some(said) = doc.get("error") {
        let said = said.as_str().unwrap_or("(an error that was not text)");
        return Err(format!(
            "'{extension}' answered that it could not: {}",
            crate::shown::readable(said, 1 << 10)
        ));
    }
    let blocks = doc.get("blocks").ok_or_else(|| {
        format!("'{extension}' answered neither 'blocks' nor 'error', so there is nothing to draw")
    })?;
    panel::answered(blocks).map_err(|why| format!("'{extension}' {why}"))
}

#[cfg(test)]
mod tests;
