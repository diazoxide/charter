//! The executor: how charter starts an approved extension's program, what it hands it, what it
//! will take back, and how it stops it. **ADR 0041, stage 2.**
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
//!    Since charter-app#343 an event an extension hears ([`Executor::tell`]) and a chat's
//!    start ([`Executor::brief`]) are questions too — asked without a press, which the approval
//!    prompt says ([`how_it_runs`]) — and each is still one process, gated and bounded alike.
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
//! it runs — measured on this machine as the thing that hangs). Named, not closed — **but kept
//! as narrow as the order allows**: the question is built (the plane read, the slow part)
//! *before* the tree is hashed, so what lies between the hash and the start is the slot and the
//! fork and nothing else. What makes a refused extension cost no plane read is the record, read
//! first: an extension nobody approved is refused before `hand` runs.
//!
//! **Nothing of charter's but its two sockets.** A program is handed its stdin, stdout and
//! stderr and no other descriptor: every one above 2 is closed as it starts
//! ([`inherit_nothing_else`], this workspace's one audited `unsafe`), and the executor's pairs
//! are made under [`crate::forklock::while_descriptors_are_made`], so no *other* program can
//! inherit them half-made either.
//!
//! # A program ends when its stdin does
//!
//! **Part of the protocol, and the one thing a program owes charter.** charter kills a program's
//! group when the question is over and every program's group when the window closes. It cannot
//! when charter itself dies — a crash, a `kill -9`, a power cut to the process — and on macOS
//! there is nothing that makes the kernel do it instead. Linux's `PR_SET_PDEATHSIG` would, for
//! the program though not for what it starts, and it is not set: it has to be set in the child,
//! and the one `unsafe` block allowed there closes descriptors and does nothing else. What does
//! happen
//! is that charter's end of the socket closes with it, so the program's stdin reaches
//! end-of-file. **A program that reads end-of-file where its question should be exits**, rather
//! than waiting for a question nobody is left to ask; one that works for longer than a moment
//! checks its stdin as it goes. `persona-statistics` does the first, and needs nothing more.
//!
use std::path::Path;
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::extension::{self, Extension, Standing};
use crate::panel::{self, Subject};

/// The newest protocol a request and an answer are written in. charter answers every protocol
/// from 1 to this one, and **asks each extension in the protocol its manifest declares**
/// (`version`, ADR 0053) — so a program written for protocol 1 is asked exactly the question it
/// was written for, whatever this number is.
///
/// - **1**: a view's question, answered with blocks.
/// - **2** (charter-app#343): an event's question ([`Executor::tell`]) and the briefing's
///   ([`Executor::brief`]), each a request kind of its own. A view is asked as in 1.
///
/// **The declared protocol is inside the fingerprint of every extension that declares a
/// program** ([`crate::extension`]'s tree hash), so an extension that changes the protocol it
/// speaks is asked about again — and raising this number re-asks nobody, because no extension's
/// declared protocol moved. Bump it when a request or an answer gains a fact.
pub const PROTOCOL: u32 = 2;

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

/// Whether charter starts an extension's program on this platform at all — `false` on
/// Windows, where [`ask`](Executor::ask) answers [`REFUSED_HERE`] to every question.
///
/// **For the window, so that it does not draw a button that can only ever refuse.** A view is
/// still read, fingerprinted and shown in the consent dialog everywhere; what this says is
/// whether opening one could ever do anything.
pub const RUNS_PROGRAMS: bool = cfg!(unix);

/// How much of a question is written in one `write(2)` ([`ask_within`]).
#[cfg_attr(not(unix), allow(dead_code))]
const WRITTEN_AT_ONCE: usize = 64 << 10;

/// What charter says on a platform where it will not start an extension's program.
pub const REFUSED_HERE: &str = "charter does not start an extension's program on this platform: \
     it talks to one over a unix socket, and this platform has none charter can use without \
     writing unsafe code. A guard that cannot be expressed refuses rather than degrades \
     (ADR 0031).";

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

/// [`HOW_IT_RUNS`] for a program that is also started without the operator opening anything —
/// because it hears events or adds a briefing section (charter-app#343). **"Never on its own"
/// would be false of it**, so it says when charter does start it instead, and keeps every bound.
pub fn how_it_runs(manifest: &extension::Manifest) -> String {
    let mut when: Vec<&str> = Vec::new();
    if !manifest.views.is_empty() {
        when.push("when you open one of this extension's views");
    }
    if manifest.events.is_some() {
        when.push("after each thing it hears about has happened");
    }
    if manifest.briefing.is_some() {
        when.push("when a chat starts, for its briefing section");
    }
    let when = match when.as_slice() {
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
        [] => String::new(),
    };
    format!(
        "charter starts it {when} — never at launch and never on a timer. Each time it is \
         asked one question, given at most {} seconds to answer, and then stopped along with \
         anything it started. A program set on outliving that can, because it runs as you do.",
        DEADLINE.as_secs()
    )
}

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
#[derive(Debug)]
pub struct Executor {
    table: Mutex<Table>,
    /// Signalled whenever a slot is given back, for an event waiting its turn
    /// ([`Self::hold_within`]).
    freed: Condvar,
    /// How long each program has. [`DEADLINE`] for every executor charter makes; only a test
    /// gives an executor another (charter-app#303), so that a test whose subject is not the
    /// deadline is not failed by a busy machine taking five seconds to start its program.
    deadline: Duration,
}

impl Default for Executor {
    fn default() -> Self {
        Self {
            table: Mutex::default(),
            freed: Condvar::new(),
            deadline: DEADLINE,
        }
    }
}

/// What [`Executor`] guards with its one lock.
#[derive(Debug, Default)]
struct Table {
    /// Each extension with a question in flight, and its program's process group — `0` from
    /// the moment the slot is taken until the program exists, and again once it has been taken
    /// out to be reaped.
    running: std::collections::BTreeMap<String, i32>,
    /// Set by [`Executor::stop_all`], and never cleared: charter is closing. A slot is not given
    /// out after it, and a program that was started before it and recorded after it is killed
    /// the moment it is recorded.
    closing: bool,
}

impl Executor {
    /// An executor whose programs have `deadline` instead of [`DEADLINE`]: for the session-start
    /// briefing, which holds a chat's start and so gives each program less
    /// ([`crate::extension::briefing::Bounds`]), and for a test whose subject is not the deadline.
    pub fn with_deadline(deadline: Duration) -> Self {
        Self {
            deadline,
            ..Self::default()
        }
    }

    /// Ask `extension`'s program about `view`, or say why charter will not.
    ///
    /// `hand` builds what the program is given, from the view's subject. It is called once the
    /// **record** says the operator approved this extension — so an extension nobody installed
    /// or approved costs no plane read at all — and *before* the extension's directory is
    /// fingerprinted, so that the fingerprint is the last thing taken before the program starts
    /// (see the module's note on ADR 0028). An extension whose directory changed since its yes
    /// therefore costs one plane read and is then refused.
    /// `focus` is the one thing on screen the operator opened it from, when there is one (a
    /// persona's card), and is handed as a name.
    ///
    /// `project` is what the plane the view was opened in says about extensions
    /// ([`extension::project::Choices::read`], ADR 0048). It is asked **after** this machine's
    /// record, so no project file can stand in for the operator's yes: a project that turned the
    /// extension off is refused, and an extension that declares settings is handed the project's
    /// resolved values as `settings` in the question. One that declares none is asked exactly
    /// the question it was approved under.
    ///
    /// **Every `Err` is a sentence the operator can act on**, naming the extension and saying
    /// what to do: open Extensions and approve it again, make the file runnable, look at what
    /// the program printed. None of them is a stack trace and none is silence.
    pub fn ask(
        &self,
        config_root: &Path,
        project: &extension::project::Choices,
        extension: &str,
        view: &str,
        focus: Option<&str>,
        hand: impl FnOnce(Subject) -> serde_json::Value,
    ) -> Result<Answer, String> {
        supported()?;
        let began = Instant::now();
        // The record alone first: whether this is an extension the operator said yes to at all.
        // **Before the project is asked anything**, so that no project file can stand in for this
        // machine's yes (ADR 0048).
        let loaded = extension::read(config_root);
        let entry = approved(&loaded, extension)?;
        // Then only its manifest, for which view was asked and what that view is about.
        let declared =
            extension::manifest_at(&entry.path).map_err(|why| could_not_reread(extension, &why))?;
        // Then the project the view was opened in: whether it has this extension on, and what it
        // set for it — `project::resolve`, the one answer every consumer takes.
        let settings = in_this_project(project, extension, &declared)?;
        let asked = declared_view(extension, &declared, view)?;
        let before_hand = began.elapsed();

        // The question, built before the fingerprint rather than after it: `hand` reads the
        // plane, which is the slow part, and every moment between the fingerprint and the start
        // is a moment in which a write is run without having been hashed.
        let request = request_line(
            extension,
            declared.protocol,
            &asked,
            focus,
            hand(asked.about),
            settings,
        )?;

        // **The gate, re-taken now over the whole tree**, and held to the view the question was
        // built for: a manifest that changed between the two reads is refused, never run on the
        // question it was not asked.
        let heard = self.gated_and_asked(
            &loaded,
            &entry,
            extension,
            &request,
            |found| {
                found.protocol == declared.protocol
                    && declared_view(extension, found, view).ok().as_ref() == Some(&asked)
            },
            Waits::No,
        )?;

        let blocks = read_answer(extension, &heard.line, declared.protocol)?;
        Ok(Answer {
            blocks,
            gate: before_hand + heard.gate,
            round_trip: heard.round_trip,
        })
    }

    /// Tell `extension`'s program that `event` happened, or say why charter did not
    /// (charter-app#343).
    ///
    /// **The same gate a view's question takes, in the same order** — the record, the manifest,
    /// the project, the fingerprint re-taken last — and the same bounds. Two differences, both
    /// because nobody pressed anything: an extension that does not hear `event` is refused
    /// rather than asked, and a program still answering its last question is **waited for**,
    /// up to this executor's deadline, rather than refused — two events a moment apart are two
    /// things it was promised, not a double click.
    ///
    /// The caller has already finished the action the event reports; nothing answered here can
    /// change it ([`crate::extension::events::deliver`]).
    pub fn tell(
        &self,
        config_root: &Path,
        project: &extension::project::Choices,
        extension: &str,
        event: &extension::events::Event,
    ) -> Result<(), String> {
        supported()?;
        let loaded = extension::read(config_root);
        let entry = approved(&loaded, extension)?;
        let declared =
            extension::manifest_at(&entry.path).map_err(|why| could_not_reread(extension, &why))?;
        in_this_project(project, extension, &declared)?;
        let kind = event.kind();
        if !declared.hears(kind) {
            return Err(format!(
                "'{extension}' does not hear {}, so charter does not tell it",
                kind.said()
            ));
        }
        let request = event_line(extension, declared.protocol, event)?;
        let heard = self.gated_and_asked(
            &loaded,
            &entry,
            extension,
            &request,
            |found| found.protocol == declared.protocol && found.hears(kind),
            Waits::Yes,
        )?;
        reply_of(extension, &heard.line, declared.protocol, &[]).map(|_| ())
    }

    /// Ask `extension`'s program for its section of a chat's session-start briefing, or say why
    /// charter did not (charter-app#343). `asked` is what the chat is — its workspace and
    /// persona — and is handed as `briefing`.
    ///
    /// The section comes back as the program wrote it. **Whether charter will quote it is
    /// [`crate::extension::briefing`]'s to decide**, not this: this answers for the protocol
    /// and the gate, which are the same as a view's.
    pub fn brief(
        &self,
        config_root: &Path,
        project: &extension::project::Choices,
        extension: &str,
        asked: serde_json::Value,
    ) -> Result<String, String> {
        supported()?;
        let loaded = extension::read(config_root);
        let entry = approved(&loaded, extension)?;
        let declared =
            extension::manifest_at(&entry.path).map_err(|why| could_not_reread(extension, &why))?;
        in_this_project(project, extension, &declared)?;
        if declared.briefing.is_none() {
            return Err(format!(
                "'{extension}' adds no section to the briefing, so charter does not ask it for one"
            ));
        }
        let request = briefing_line(extension, declared.protocol, asked)?;
        let heard = self.gated_and_asked(
            &loaded,
            &entry,
            extension,
            &request,
            |found| found.protocol == declared.protocol && found.briefing == declared.briefing,
            Waits::No,
        )?;
        let doc = reply_of(extension, &heard.line, declared.protocol, &["section"])?;
        match doc.get("section") {
            Some(serde_json::Value::String(text)) => Ok(text.clone()),
            Some(serde_json::Value::Null) | None => Ok(String::new()),
            Some(_) => Err(format!(
                "'{extension}' answered a 'section' that is not text"
            )),
        }
    }

    /// **The gate, re-taken now over the whole tree**, then one question asked and its one line
    /// read. `same` says whether the manifest the fingerprint was taken over still declares what
    /// the question was built for: one that changed between the two reads is refused, never run
    /// on a question it was not asked.
    fn gated_and_asked(
        &self,
        loaded: &extension::Loaded,
        entry: &extension::Entry,
        extension: &str,
        request: &[u8],
        same: impl FnOnce(&extension::Manifest) -> bool,
        waits: Waits,
    ) -> Result<Replied, String> {
        let began = Instant::now();
        let found = fingerprinted(loaded, extension, entry)?;
        if !same(&found.manifest) {
            return Err(changed(extension));
        }
        let program = found.manifest.program.as_deref().ok_or_else(|| {
            // The parse refuses a view, an event or a briefing with no program, so this is a
            // manifest that changed shape between two reads of the same bytes — which cannot
            // happen, and is answered rather than unwrapped because a window waits on this.
            format!("'{extension}' declares no program to ask")
        })?;
        let at = found.path.join(program);
        runnable(&at, extension)?;
        let gate = began.elapsed();

        let _held = match waits {
            Waits::No => self.hold(extension)?,
            Waits::Yes => self.hold_within(extension, Instant::now() + self.deadline)?,
        };
        let started = Instant::now();
        let line = self.converse(extension, &found, &at, request)?;
        Ok(Replied {
            line,
            gate,
            round_trip: started.elapsed(),
        })
    }

    /// Kill every program an extension was asked to run and has not finished, and start no
    /// more. For the app's exit, so that nothing charter started outlives the window that
    /// started it.
    ///
    /// **Only what is still in the table**, and an entry leaves the table *before* its program
    /// is reaped ([`Running::stop`]), so a process group named here is always one whose leader
    /// has not been reaped — its id cannot have been given to anything else yet.
    ///
    /// **And what is not in it yet is caught on its way in.** A program is started and then
    /// recorded, and a `stop_all` between the two would find a slot with no group in it. So
    /// this marks the executor closing, under the same lock [`Self::started`] takes: whichever
    /// of the two runs second sees the other, and a program recorded after this is killed as it
    /// is recorded.
    pub fn stop_all(&self) {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        table.closing = true;
        for group in table.running.values() {
            kill_group(*group);
        }
        // An event waiting its turn ([`Self::hold_within`]) hears it now, and starts nothing.
        self.freed.notify_all();
    }

    /// Which extensions have a program running right now. For a test, and for anything that
    /// wants to say so.
    pub fn running(&self) -> Vec<String> {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .running
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
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        if table.closing {
            return Err(format!(
                "charter is closing, so it starts nothing more — '{extension}' was not asked."
            ));
        }
        if table.running.contains_key(extension) {
            return Err(format!(
                "'{extension}' is still answering the last thing it was asked. charter asks an \
                 extension one thing at a time; this one has at most {} left.",
                self.deadline.as_secs()
            ));
        }
        // Zero until the program exists: `stop_all` never signals a group of 0, which would be
        // charter's own.
        table.running.insert(extension.to_owned(), 0);
        Ok(Held {
            by: self,
            extension: extension.to_owned(),
        })
    }

    /// Claim `extension`'s one slot, waiting for it until `until` if its program is answering
    /// something else — for an event, which nobody pressed twice ([`Self::tell`]). Still one
    /// question in flight per extension; this only queues behind it rather than refusing.
    fn hold_within(&self, extension: &str, until: Instant) -> Result<Held<'_>, String> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if table.closing {
                return Err(format!(
                    "charter is closing, so it starts nothing more — '{extension}' was not asked."
                ));
            }
            if !table.running.contains_key(extension) {
                table.running.insert(extension.to_owned(), 0);
                return Ok(Held {
                    by: self,
                    extension: extension.to_owned(),
                });
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!(
                    "'{extension}' was still answering the last thing it was asked after {} \
                     seconds, so charter did not ask it this.",
                    self.deadline.as_secs()
                ));
            }
            table = self
                .freed
                .wait_timeout(table, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Record the process group a running program is in, so [`Self::stop_all`] can reach it —
    /// or, if `stop_all` has already run, kill it now: it was started before charter began
    /// closing and would otherwise be the one program nobody stopped.
    #[cfg_attr(not(unix), allow(dead_code))]
    fn started(&self, extension: &str, group: i32) {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = table.running.get_mut(extension) {
            *slot = group;
        }
        if table.closing {
            kill_group(group);
        }
    }

    /// Take the process group out of the table, before it is killed and reaped. See
    /// [`Self::stop_all`] for why the order is load-bearing.
    #[cfg_attr(not(unix), allow(dead_code))]
    fn finished(&self, extension: &str) {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = table.running.get_mut(extension) {
            *slot = 0;
        }
    }

    /// Start the program, ask it `request`, and read its one line — on unix. Off unix
    /// [`supported`] refuses before this is reached, and this says the same.
    ///
    /// One function with a platform arm rather than a twin per platform, so that a mutant of
    /// `converse` is a mutant of the code this build runs.
    fn converse(
        &self,
        extension: &str,
        found: &Extension,
        program: &Path,
        request: &[u8],
    ) -> Result<Vec<u8>, String> {
        #[cfg(unix)]
        {
            self.converse_on_unix(extension, found, program, request)
        }
        #[cfg(not(unix))]
        {
            let _ = (extension, found, program, request);
            Err(REFUSED_HERE.to_owned())
        }
    }

    #[cfg(unix)]
    fn converse_on_unix(
        &self,
        extension: &str,
        found: &Extension,
        program: &Path,
        request: &[u8],
    ) -> Result<Vec<u8>, String> {
        use std::os::fd::OwnedFd;
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let could_not =
            |why: std::io::Error| format!("charter could not start '{extension}''s program: {why}");
        let Channel {
            ours,
            theirs,
            err_ours,
            err_theirs,
        } = channel().map_err(could_not)?;
        // Every clone is made before the program exists, so nothing between its start and its
        // stop can fail on the way and leave it running with nobody holding it.
        let theirs_in = theirs.try_clone().map_err(could_not)?;
        let ours_to_write = ours.try_clone().map_err(could_not)?;

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
            .stdin(Stdio::from(OwnedFd::from(theirs_in)))
            .stdout(Stdio::from(OwnedFd::from(theirs)))
            .stderr(Stdio::from(OwnedFd::from(err_theirs)));
        // And nothing else of charter's: every descriptor above 2 is closed as the program
        // starts, whoever opened it and however.
        inherit_nothing_else(&mut command);
        // `forklock`, never `Command::spawn` (charter-app#53): this process opens terminals, and
        // a program forked while one is half-open inherits a chat's pty and holds it for as long
        // as it lives. An extension's program is the last thing that should be holding a chat.
        let spawned = crate::forklock::spawn(&mut command);
        // **The program's ends are dropped here, with the Command that owns them**, so the only
        // holders of the socket's far side are the program and whatever it starts. Without
        // this, charter's own copy would keep the socket open and "the program closed its
        // output" could never be seen.
        drop(command);
        let child = spawned.map_err(|why| match why.kind() {
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
        // **From here the program is owned by `running`**, and every way out of this function —
        // an answer, a refusal, an error on the way, a panic — stops it: its group killed and it
        // reaped, in the order `stop_all` relies on.
        let mut running = Running::new(self, extension, child);
        let until = Instant::now() + self.deadline;

        // Its stderr is drained while it runs — a program that logs more than a socket buffer
        // would otherwise stall on its own diagnostics and read as hung — and only the tail is
        // kept.
        let stop = Arc::new(AtomicBool::new(false));
        let tail = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name(format!("extension {extension} stderr"))
                .spawn(move || drain(&err_ours, &stop))
                .map_err(could_not)?
        };

        // The question is written on a thread of its own, so a program that answers before it
        // has read the whole of it cannot deadlock against charter reading the answer.
        let writer = match std::thread::Builder::new()
            .name(format!("extension {extension} request"))
            .spawn({
                let request = request.to_vec();
                move || ask_within(&ours_to_write, &request, until)
            }) {
            Ok(writer) => writer,
            Err(why) => {
                stop.store(true, Ordering::Relaxed);
                return Err(could_not(why));
            }
        };

        let heard = listen(&ours, until);
        // **The conversation is over, whatever the program or anything it started still
        // holds.** Shutting charter's end down wakes the writer if it is still in a `write` —
        // a helper that left the group and holds the program's stdin, reading a byte a second,
        // otherwise held this call for as long as the request took to trickle through (15.6 s
        // measured, three times the deadline) — and it is what every other holder of the
        // socket's far side reads as the end.
        let _ = ours.shutdown(std::net::Shutdown::Both);

        let status = running.stop();
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
                self.deadline.as_secs(),
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
}

/// An extension's one slot, released however the question ends.
struct Held<'a> {
    by: &'a Executor,
    extension: String,
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.by
            .table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .running
            .remove(&self.extension);
        self.by.freed.notify_all();
    }
}

/// Whether a question waits for a slot its extension is using, or is refused.
#[derive(Debug, Clone, Copy)]
enum Waits {
    /// A view: the operator pressed a button twice, and the second press is refused.
    No,
    /// An event: waits its turn, up to the deadline ([`Executor::hold_within`]).
    Yes,
}

/// One answer line, and what it cost.
struct Replied {
    line: Vec<u8>,
    /// Re-taking the fingerprint, and checking the program is runnable.
    gate: Duration,
    /// From the program being started to its answer being read.
    round_trip: Duration,
}

/// A started program, stopped when this goes — however it goes.
///
/// **The one owner of the child from the moment it exists.** Between starting a program and
/// reading its answer are two thread starts, and either can fail; a `?` or a panic there used to
/// return with the program still running, its group never killed and its process never reaped.
/// Now the only way out of [`Executor::converse`] runs [`Self::stop`], explicitly on the ordinary
/// path and from `Drop` on every other.
#[cfg(unix)]
struct Running<'a> {
    by: &'a Executor,
    extension: &'a str,
    group: i32,
    /// `None` once reaped, so that nothing is ever signalled by a number that may have been
    /// given to another process since.
    child: Option<std::process::Child>,
}

#[cfg(unix)]
impl<'a> Running<'a> {
    /// Own `child` and record its group where [`Executor::stop_all`] can reach it.
    fn new(by: &'a Executor, extension: &'a str, child: std::process::Child) -> Self {
        let group = i32::try_from(child.id()).unwrap_or(0);
        by.started(extension, group);
        Self {
            by,
            extension,
            group,
            child: Some(child),
        }
    }

    /// Stop it and reap it, once. **The order is load-bearing** (see
    /// [`Executor::stop_all`]): out of the table, then the whole group killed, then reaped — so
    /// a group id charter signals is never one whose leader has already been reaped and whose
    /// number the system could have reused.
    fn stop(&mut self) -> Option<std::process::ExitStatus> {
        let mut child = self.child.take()?;
        self.by.finished(self.extension);
        kill_group(self.group);
        // And the program itself, by the pid charter holds and has not reaped. A program that
        // moved its own process out of the group it was started in (`setpgid`) is not reached
        // by the group kill, and `wait` below would then wait for it for as long as it liked —
        // a blocking thread, and this extension's slot, held for ever.
        let _ = child.kill();
        child.wait().ok()
    }
}

#[cfg(unix)]
impl Drop for Running<'_> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// The highest descriptor number the program could be handed, read in charter before the fork
/// so that the child does nothing but system calls: the soft `RLIMIT_NOFILE`, or the hard one
/// when the soft one is unlimited, or [`MOST_DESCRIPTORS`] when both are.
///
/// A descriptor above the soft limit can exist only if the limit was lowered after it was
/// opened; charter lowers no limit.
#[cfg(unix)]
fn highest_descriptor() -> i32 {
    let limit = rustix::process::getrlimit(rustix::process::Resource::Nofile);
    let most = limit.current.or(limit.maximum).unwrap_or(MOST_DESCRIPTORS);
    i32::try_from(most.min(MOST_DESCRIPTORS)).unwrap_or(i32::MAX)
}

/// The bound on [`highest_descriptor`]: `kern.maxfilesperproc` on a stock macOS is 184 320, and
/// no process on Linux is given more than `nr_open` (1 048 576 by default).
#[cfg(unix)]
const MOST_DESCRIPTORS: u64 = 1 << 20;

/// Every descriptor above 2 is closed in the program as it starts: **the one `unsafe` in this
/// workspace**, allowed by the operator on 2026-09-23 ("Allow one audited block").
///
/// charter's own descriptors are close-on-exec, but not everything in the process is charter's:
/// a C library can hold a descriptor without the flag, and on macOS every socket the standard
/// library makes is one for the moment between `socket(2)` and `FIOCLEX`. Measured before this:
/// a descriptor `dup`ed in charter reached the program, and programs started beside a thread
/// making socket pairs inherited one in 112 and 141 of 300. The standard remedy is the one
/// `portable-pty` applies to every terminal charter opens: close what is above 2 in the child,
/// between the fork and the exec. The only way to run code there is `pre_exec`, which is
/// `unsafe` because of where it runs.
///
/// **Marked close-on-exec rather than closed**, so that the exec closes them: the standard
/// library reports a failed exec back to charter through a pipe of its own, already
/// close-on-exec, and closing that one would turn "the interpreter does not exist" into a
/// program that silently exited.
///
/// **Which numbers**, per platform, cheapest first:
/// - Linux: `close_range(3, ~0, CLOSE_RANGE_CLOEXEC)`, the whole range in one call (5.11+).
/// - macOS: the child's own descriptor table, read with `proc_pidinfo(PROC_PIDLISTFDS)` into a
///   buffer allocated before the fork, and one `fcntl` per descriptor it lists. A bounded loop
///   is not good enough there: this machine's soft limit is 1 048 576, and a `fcntl` per number
///   cost about 0.8 s per question.
/// - Otherwise, or when either of those fails or the table did not fit the buffer: one `fcntl`
///   per number up to [`highest_descriptor`], which answers `EBADF` at once for a number that
///   is not open.
///
/// **What mutation testing cannot see here** (`.cargo/mutants.toml`). The nightly builds Linux,
/// where the macOS arm is not compiled at all, and on Linux a failed `close_range` check only
/// sends the child to the per-number loop, which marks the same descriptors. On macOS the fast
/// path's bounds (`got > 0 && got < size`) likewise only choose between the listed table and
/// that same loop, except when `proc_pidinfo` fails on the child's own pid, which no test can
/// arrange, and `got / each` as `got * each` reads the zeroed entries past the table, which
/// `proc_fd > 2` skips. What does change the answer there — the count taken, which entries are
/// marked — is pinned on macOS by
/// `a_descriptor_charter_holds_without_close_on_exec_does_not_reach_the_program`.
///
/// **Not `portable_pty::unix::close_random_fds`**, which is what portable-pty itself calls
/// there: it lists `/dev/fd` with `read_dir`, which allocates, and after a fork in a
/// multi-threaded process the allocator's lock may be held by a thread that no longer exists.
#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "the one audited block the operator allowed (2026-09-23); see the SAFETY comment"
)]
fn inherit_nothing_else(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    let highest = highest_descriptor();
    #[cfg(target_os = "macos")]
    let mut listed = vec![
        libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0,
        };
        LISTED_AT_ONCE
    ];
    // SAFETY: `pre_exec` runs the closure in the child between `fork` and `exec`, where only
    // async-signal-safe functions may be called — another thread may have held a lock (the
    // allocator's among them) at the fork, and nothing in the child will ever release it.
    // The closure allocates nothing, takes no lock and panics nowhere (no indexing, no
    // unwrap). Everything it uses was made before the fork and moved in: `highest`, an `i32`,
    // and on macOS `listed`, a buffer the child owns its own copy of. It calls only:
    // - `fcntl(fd, F_SETFD, FD_CLOEXEC)`, which POSIX lists as async-signal-safe. On a number
    //   that is not open it fails with `EBADF`, ignored; on one that is, it sets the one flag
    //   and reads or writes no memory. Descriptors 0-2, the program's sockets, are never
    //   passed to it.
    // - Linux: the `close_range` system call through `syscall(2)`, a trap with no lock.
    // - macOS: `getpid`, async-signal-safe, and `proc_pidinfo`, not on the POSIX list but a
    //   single `proc_info` system call in libsystem with no lock and no allocation. It writes at
    //   most `size` bytes into `listed`, which is exactly the buffer's length in bytes, and
    //   what is read back is bounded by both what it returned and the buffer's length.
    unsafe {
        command.pre_exec(move || {
            #[cfg(target_os = "linux")]
            if libc::syscall(
                libc::SYS_close_range,
                3u32,
                u32::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            ) == 0
            {
                return Ok(());
            }
            #[cfg(target_os = "macos")]
            {
                let each = libc::PROC_PIDLISTFD_SIZE;
                let size = i32::try_from(listed.len())
                    .unwrap_or(0)
                    .saturating_mul(each);
                let got = libc::proc_pidinfo(
                    libc::getpid(),
                    libc::PROC_PIDLISTFDS,
                    0,
                    listed.as_mut_ptr().cast(),
                    size,
                );
                // Equal to the buffer means it may not have been the whole table.
                if got > 0 && got < size {
                    let count = usize::try_from(got / each).unwrap_or(0);
                    for one in listed.iter().take(count) {
                        if one.proc_fd > 2 {
                            libc::fcntl(one.proc_fd, libc::F_SETFD, libc::FD_CLOEXEC);
                        }
                    }
                    return Ok(());
                }
            }
            for fd in 3..=highest {
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
            Ok(())
        });
    }
}

/// How many descriptors `proc_pidinfo` is given room to list: far more than charter holds (a few
/// dozen, and a few hundred with many chats open), for 64 KiB allocated per question.
#[cfg(target_os = "macos")]
const LISTED_AT_ONCE: usize = 8192;

/// Both sockets a program is started with: its stdin and stdout, and its stderr. Charter keeps
/// the `_ours` ends.
#[cfg(unix)]
pub(crate) struct Channel {
    ours: std::os::unix::net::UnixStream,
    theirs: std::os::unix::net::UnixStream,
    err_ours: std::os::unix::net::UnixStream,
    err_theirs: std::os::unix::net::UnixStream,
}

/// Make a program's two socket pairs, with no program anywhere started while they are.
///
/// **One extension's channel is that extension's alone** — this module's header says so — and on
/// macOS that is only true under [`crate::forklock::while_descriptors_are_made`]: a pair is made
/// there in two system calls, and a program started on another thread between them inherits
/// both ends, whoever's program it is.
#[cfg(unix)]
pub(crate) fn channel() -> std::io::Result<Channel> {
    use std::os::unix::net::UnixStream;
    crate::forklock::while_descriptors_are_made(|| {
        let (ours, theirs) = UnixStream::pair()?;
        let (err_ours, err_theirs) = UnixStream::pair()?;
        Ok(Channel {
            ours,
            theirs,
            err_ours,
            err_theirs,
        })
    })
}

/// Read a program's stderr until it closes or charter says stop, keeping the tail.
#[cfg(unix)]
fn drain(
    err_ours: &std::os::unix::net::UnixStream,
    stop: &std::sync::atomic::AtomicBool,
) -> Vec<u8> {
    use std::io::Read;
    use std::sync::atomic::Ordering;
    let mut err_ours = err_ours;
    let _ = err_ours.set_read_timeout(Some(Duration::from_millis(50)));
    let mut kept: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    // When charter said stop. What a program wrote just before it died is still in the socket,
    // and it is the part worth quoting, so the drain goes on after stop — but for a bounded
    // TIME, not a bounded number of bytes: a process that escaped the group (`setsid`) can hold
    // stderr open and trickle into it for ever, and a byte cap is then a wait of however long it
    // takes to trickle that many (measured: a byte a millisecond held a 64 KiB cap for 86 s).
    // Either way the cost was this thread, and the extension's one slot, held with it.
    let mut stopped_at: Option<Instant> = None;
    loop {
        if stopped_at.is_none() && stop.load(Ordering::Relaxed) {
            stopped_at = Some(Instant::now());
        }
        // `>` against `>=` differs at one instant of a clock no test can land on.
        if stopped_at.is_some_and(|at| at.elapsed() > STDERR_AFTER_STOP) {
            break;
        }
        match err_ours.read(&mut chunk) {
            Ok(0) => break,
            Ok(got) => {
                kept.extend_from_slice(&chunk[..got]);
                // At exactly the bound the cut is 0, so `>=` here changes nothing.
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
}

/// Write the whole question to the program, by `until` or not at all, then close charter's
/// writing half so a program that reads "all of stdin" sees the end of it. `true` when all of
/// it was written.
///
/// **What bounds this is not in here.** A socket's write timeout bounds one wait for buffer
/// space, and it starts again each time the reader takes some — measured: one `write(2)` of 8
/// MiB, read 512 bytes every 20 ms, ran 393 s under a 300 ms timeout. So a reader that trickles
/// holds any write for as long as it likes, whatever the timeout says, and `write_all` held the
/// question for as long as the request took to go through. The bound is [`Executor::converse`]
/// shutting charter's end down once [`listen`] returns — at the deadline at the latest — which
/// ends a write in progress on every platform. The timeout here, set to what is left of the
/// deadline on each call and over a small piece at a time, only stops a reader that takes
/// nothing at all from being waited on past the deadline by this thread alone.
#[cfg(unix)]
fn ask_within(ours: &std::os::unix::net::UnixStream, request: &[u8], until: Instant) -> bool {
    use std::io::Write;
    let mut ours = ours;
    let mut at = 0;
    let wrote = loop {
        if at == request.len() {
            break true;
        }
        let left = until.saturating_duration_since(Instant::now());
        // A zero timeout is itself an error to `set_write_timeout`, so the second half is
        // true whenever the first is and `&&` would decide the same.
        if left.is_zero() || ours.set_write_timeout(Some(left)).is_err() {
            break false;
        }
        let piece = &request[at..request.len().min(at + WRITTEN_AT_ONCE)];
        match ours.write(piece) {
            Ok(0) => break false,
            Ok(put) => at += put,
            Err(why) if why.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break false,
        }
    };
    // EOF after the question: a program may read "all of stdin" rather than a line.
    let _ = ours.shutdown(std::net::Shutdown::Write);
    wrote
}

/// Whether charter starts extension programs on this platform at all ([`RUNS_PROGRAMS`]).
///
/// On unix this IS `Ok(())`, so its one mutant is the function a unix build runs;
/// `.cargo/mutants.toml` records that.
fn supported() -> Result<(), String> {
    if RUNS_PROGRAMS {
        Ok(())
    } else {
        Err(REFUSED_HERE.to_owned())
    }
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
    let entry = approved(&loaded, extension)?;
    fingerprinted(&loaded, extension, &entry)
}

/// The gate's first half: what the record says, and nothing read from the extension. Cheap —
/// one small file — so that an extension nobody approved costs nothing more than this.
fn approved(loaded: &extension::Loaded, extension: &str) -> Result<extension::Entry, String> {
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
    Ok(entry.clone())
}

/// The gate's second half: the extension's whole directory, fingerprinted now and held to the
/// fingerprint the operator approved at the path he approved it at.
fn fingerprinted(
    loaded: &extension::Loaded,
    extension: &str,
    entry: &extension::Entry,
) -> Result<Extension, String> {
    // **Re-read now, the whole tree**, and never a fingerprint remembered from a survey. This
    // is the line that makes "will ask again if any of them changes" true at the moment it
    // matters, which is the moment something is about to run.
    let found = extension::read_at(&entry.path).map_err(|why| could_not_reread(extension, &why))?;
    let standing = if found.id() == extension {
        loaded.standing(&found)
    } else {
        Standing::Changed
    };
    if standing != Standing::Approved {
        return Err(changed(extension));
    }
    Ok(found)
}

/// Whether the project this view was opened in has `extension` on, and the settings it chose
/// for it when it declares any — or the sentence for a project that turned it off.
///
/// The machine's yes was already checked by [`approved`], so it is `approved: true` here; the
/// fingerprint is re-taken after this, by [`fingerprinted`].
fn in_this_project(
    project: &extension::project::Choices,
    extension: &str,
    declared: &extension::Manifest,
) -> Result<Option<serde_json::Value>, String> {
    use extension::project::{self, State};
    let here = project::Installed {
        id: extension.to_owned(),
        name: declared.name.clone(),
        approved: true,
        settings: declared.settings.clone(),
    };
    // `resolve` answers for every extension it is given, so the one asked about is always there;
    // an empty answer would be a defect in it, and is refused rather than read as "on".
    let Some(effective) = project::resolve(&[here], project)
        .into_iter()
        .find(|it| it.id == extension)
    else {
        return Err(format!(
            "charter could not say whether this project has '{extension}' on"
        ));
    };
    if effective.state == State::Off {
        // Off is only ever decided by a file: with none saying, an approved extension is on.
        let (file, whose, tab) = match (effective.source, project.workspace_file()) {
            (project::Source::Workspace, Some(file)) => {
                (file, "this workspace", "Workspace settings")
            }
            (source, _) => (
                source
                    .file()
                    .unwrap_or(crate::profiles::COMMITTED_FILE)
                    .to_owned(),
                "this project",
                "Project settings",
            ),
        };
        return Err(format!(
            "'{extension}' is turned off in {file} for {whose}, so charter will not start its \
             program here. Turn it on in {tab} to use this view."
        ));
    }
    Ok((!declared.settings.is_empty()).then(|| effective.settings_json()))
}

fn could_not_reread(extension: &str, why: &str) -> String {
    format!(
        "charter could not re-read '{extension}' just now, so it will not start anything from \
         it: {why}"
    )
}

fn changed(extension: &str) -> String {
    format!(
        "'{extension}' has changed since you approved it — charter re-read its directory just \
         now and it is not what you said yes to, so charter will not run it. Open Extensions: \
         charter will show you what it declares now and ask again."
    )
}

/// The view called `view` in `manifest`, or the sentence for its absence.
fn declared_view(
    extension: &str,
    manifest: &extension::Manifest,
    view: &str,
) -> Result<extension::View, String> {
    manifest
        .views
        .iter()
        .find(|it| it.id == view)
        .cloned()
        .ok_or_else(|| {
            format!(
                "'{extension}' has no view called '{view}'. The window's list of what is in \
                 force is taken when it opens; reopen the window to see what '{extension}' \
                 offers now."
            )
        })
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
    env.push((
        "CHARTER_PROTOCOL".into(),
        found.manifest.protocol.to_string(),
    ));
    if let Some(state) = &found.manifest.state {
        env.push((
            "CHARTER_EXTENSION_STATE".into(),
            found.path.join(state).display().to_string(),
        ));
    }
    env
}

/// The one line a program is asked about a view, in `protocol` — the one its manifest declares.
fn request_line(
    extension: &str,
    protocol: u32,
    view: &extension::View,
    focus: Option<&str>,
    given: serde_json::Value,
    settings: Option<serde_json::Value>,
) -> Result<Vec<u8>, String> {
    let mut doc = serde_json::Map::new();
    doc.insert("charter".into(), protocol.into());
    doc.insert("extension".into(), extension.into());
    doc.insert("view".into(), view.id.clone().into());
    doc.insert("about".into(), view.about.as_str().into());
    doc.insert(
        "focus".into(),
        focus.map_or(serde_json::Value::Null, Into::into),
    );
    doc.insert("given".into(), given);
    // Only for an extension that declares settings, so the question every other program is
    // asked is the one it was approved under, byte for byte.
    if let Some(settings) = settings {
        doc.insert("settings".into(), settings);
    }
    one_line(extension, doc)
}

/// The one line a program is told an event in (protocol 2): `event` is the word its manifest
/// hears it by, and `workspace` and `from` are there when the event has them. Nothing of the
/// plane is handed with it: an extension that wants more reads it, as it runs as the operator.
fn event_line(
    extension: &str,
    protocol: u32,
    event: &extension::events::Event,
) -> Result<Vec<u8>, String> {
    let mut doc = serde_json::Map::new();
    doc.insert("charter".into(), protocol.into());
    doc.insert("extension".into(), extension.into());
    doc.insert("event".into(), event.kind().as_str().into());
    if let Some(workspace) = event.workspace() {
        doc.insert("workspace".into(), workspace.into());
    }
    if let Some(from) = event.from() {
        doc.insert("from".into(), from.into());
    }
    one_line(extension, doc)
}

/// The one line a program is asked for its briefing section in (protocol 2).
fn briefing_line(
    extension: &str,
    protocol: u32,
    asked: serde_json::Value,
) -> Result<Vec<u8>, String> {
    let mut doc = serde_json::Map::new();
    doc.insert("charter".into(), protocol.into());
    doc.insert("extension".into(), extension.into());
    doc.insert("briefing".into(), asked);
    one_line(extension, doc)
}

fn one_line(
    extension: &str,
    doc: serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<u8>, String> {
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

/// Read one line off `ours` by `until`, and then give the program [`GRACE`] to close its end
/// on its own.
#[cfg(unix)]
fn listen(ours: &std::os::unix::net::UnixStream, until: Instant) -> Heard {
    use std::io::Read;
    let mut ours = ours;
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
            // **A reset is the program ending, not the connection breaking.** On Linux a stream
            // socket whose peer exits with bytes still unread in its receive queue — a program
            // that crashed before reading the whole question — reads as `ECONNRESET` here
            // rather than as end-of-file. To charter that is the same event as EOF: the
            // program is gone, and its exit status and last words are what to report. Read
            // as `Broken`, a crash was drawn as "charter lost its connection", and whether a
            // run saw EOF or a reset depended on how much of the question the program had
            // read before it died — main went red on exactly that.
            Err(why) if why.kind() == std::io::ErrorKind::ConnectionReset => {
                return Heard::Nothing(!heard.is_empty());
            }
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
fn read_answer(extension: &str, line: &[u8], protocol: u32) -> Result<Vec<panel::Block>, String> {
    let doc = reply_of(extension, line, protocol, &["blocks"])?;
    let blocks = doc.get("blocks").ok_or_else(|| {
        format!("'{extension}' answered neither 'blocks' nor 'error', so there is nothing to draw")
    })?;
    panel::answered(blocks).map_err(|why| format!("'{extension}' {why}"))
}

/// An answer line read as far as every kind of question shares: one JSON object, in the
/// `protocol` it was asked in, holding `charter`, `error` and only the `keys` this kind of
/// answer has. An `error` is the refusal, quoted.
fn reply_of(
    extension: &str,
    line: &[u8],
    protocol: u32,
    keys: &[&str],
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let doc: serde_json::Value = serde_json::from_slice(line).map_err(|why| {
        format!("'{extension}' answered something that is not one line of JSON: {why}")
    })?;
    let serde_json::Value::Object(doc) = doc else {
        return Err(format!("'{extension}' answered JSON that is not an object"));
    };
    for key in doc.keys() {
        if !["charter", "error"].contains(&key.as_str()) && !keys.contains(&key.as_str()) {
            let allowed: Vec<String> = keys.iter().map(|it| format!("'{it}'")).collect();
            return Err(format!(
                "'{extension}' answered {key:?}, which is not part of charter's protocol \
                 {protocol} — this answer is 'charter', and {}'error'",
                if allowed.is_empty() {
                    String::new()
                } else {
                    format!("either {} or ", allowed.join(" or "))
                }
            ));
        }
    }
    match doc.get("charter").and_then(serde_json::Value::as_u64) {
        Some(found) if found == u64::from(protocol) => {}
        Some(found) => {
            return Err(format!(
                "'{extension}' answered in protocol {found}, and charter asked it in protocol \
                 {protocol}, the one its manifest declares"
            ));
        }
        None => {
            return Err(format!(
                "'{extension}' answered without saying which protocol it speaks ('charter': \
                 {protocol}), so charter cannot say what its answer means"
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
    Ok(doc)
}

#[cfg(test)]
mod tests;
