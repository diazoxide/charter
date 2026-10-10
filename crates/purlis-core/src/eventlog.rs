//! The host's event log (FD-9, #649): one structured event per hook call, under the chat and
//! the run it happened in.
//!
//! **One log per device, one writer** (ADR 0066, ADR 0068): the app today, `charterd` once
//! FD-5 moves the board there. A `charter` command never writes it. It reaches the writer over
//! the hook channel, which carries a chat's number and token and nothing more, and the host
//! maps the number to the chat's id and current run itself ([`Recorder`]).
//!
//! **Where:** `<data>/events/<device>/`, as `events.jsonl` and the sealed segments before it
//! ([`Retention`]), with the writer's lock in `events.lock` ([`crate::datahome`]). Machine,
//! device-bound, backed up by FR-10 (ADR 0069 row 63), never committed and never sent.
//! `docs/plane-format.md` records it.
//!
//! **What a line is:** ADR 0066's envelope ([`Event`]). The `ulid`'s time part is the event's
//! time, so there is no timestamp field. `seq` counts from 1 per device and is never reused,
//! including across launches.
//!
//! **What a tool event holds:** the tool, the harness's id for the call, a digest of its
//! arguments keyed by this device ([`ArgsKey`], O3: "an args digest, never raw args"), what
//! charter answered, the rule that refused, and how long the hook and the tool took. The
//! arguments never reach this process: a hook sends only their SHA-256.
//!
//! **What it feeds:** the audit is written from it, one entry per event (ADR 0075 §1), and
//! telemetry derives its OTel logs from it (ADR 0083). It is neither of them (O1).
//!
//! **How it is read** (FD-24): a client [`subscribe`]s from its cursor, the last `seq` it
//! holds, and gets every later event in order, each once, across sealed segments and across a
//! writer that was killed and started again. A cursor older than what [`Retention`] keeps is
//! told so ([`Delivery::Missed`]), never given a silent gap; and so is a cursor ahead of the
//! log ([`Delivery::Ahead`]), after a power loss took lines the client had read.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::state::run::{Cause, RunState};

/// One event, in ADR 0066's envelope.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub v: u32,
    pub device_id: String,
    pub seq: u64,
    pub ulid: String,
    pub chat: Option<String>,
    pub run: Option<String>,
    pub parent_run: Option<String>,
    pub kind: String,
    pub body: serde_json::Value,
}

/// The version of the envelope, and of every kind's body this module writes.
pub const VERSION: u32 = 1;

/// The directory under `<data>` that holds each device's log.
pub const EVENTS: &str = "events";

/// The file in a device's directory the events are appended to.
pub const FILE: &str = "events.jsonl";

/// How far into a segment, from its end or its start, charter reads to find its last or its
/// first event: far more than one line.
const TAIL: u64 = 64 * 1024;

/// The file in a device's directory whose lock is the device's one writer. A file of its own,
/// so the lock outlives each segment the writer seals.
pub const LOCK: &str = "events.lock";

/// How long the log is kept, and how big a segment grows before it is sealed.
///
/// **What is kept:** every event of the last [`Retention::keep`], and more. The log is
/// `events.jsonl`, the segment being written, and the sealed segments before it,
/// `events.<first seq>.jsonl`, each sealed once it reaches [`Retention::segment_bytes`]. A
/// sealed segment is deleted, when the log is opened or a segment sealed, once its newest
/// event is older than `keep`; the segment being written and the newest sealed one never are.
/// So a light user keeps more than `keep`, and nobody keeps less.
///
/// **Why 30 days:** the audit (ADR 0075) and telemetry (ADR 0083) are written from this log,
/// and a writer of either that was down must find what it has not taken still here: "days,
/// not minutes" (ADR 0075). Telemetry's own default is 30 days, so the log reaches as far back
/// as the longest store derived from it by default. About 30 MB a busy day (ADR 0075 §7's
/// estimate) is about 1 GB at the most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    pub segment_bytes: u64,
    pub keep: std::time::Duration,
}

impl Retention {
    pub const DEFAULT: Retention = Retention {
        segment_bytes: 16 * 1024 * 1024,
        keep: std::time::Duration::from_secs(30 * 24 * 60 * 60),
    };
}

/// What [`Log::on_seal`] is given.
type SealHook = Box<dyn FnMut(&File) + Send>;

/// What makes a log's events durable: one `fsync` for every event appended before it, so
/// hooks recorded at once share one (group commit).
///
/// The operating system's ordinary `fsync`. On macOS that hands the data to the drive, not
/// through the drive's own cache (`F_FULLFSYNC`, measured at about 10 ms against 0.1 ms on
/// the operator's machine), which is what SQLite does by default there.
///
/// **It follows the segment being written** (FD-24): each seal swaps in a handle on the new
/// `events.jsonl` before any event is written to it. Every event of the sealed segment was
/// `fsync`ed by the seal itself, so a number at or below the last one appended before the swap
/// is durable whichever handle is synced. A seal that could not open the next segment leaves
/// no handle, and `through` answers an error until one is open, so nothing is told durable that
/// is not.
pub struct Durable {
    appended: std::sync::atomic::AtomicU64,
    /// The last number known durable, and the segment being written.
    state: std::sync::Mutex<(u64, Option<File>)>,
}

impl Durable {
    /// Returns once every event up to `seq` is durable.
    pub fn through(&self, seq: u64) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.0 >= seq {
            return Ok(());
        }
        let appended = self.appended.load(std::sync::atomic::Ordering::SeqCst);
        let Some(file) = state.1.as_ref() else {
            return Err(io::Error::other(
                "the event log has no segment open to make durable",
            ));
        };
        #[cfg(unix)]
        rustix::fs::fsync(file)?;
        #[cfg(not(unix))]
        file.sync_data()?;
        state.0 = appended.max(seq);
        Ok(())
    }

    /// The last number known to be durable.
    pub fn synced(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0
    }

    /// The inode of the segment it syncs, for the test that it follows a seal.
    #[cfg(all(test, unix))]
    fn inode(&self) -> Option<u64> {
        use std::os::unix::fs::MetadataExt;
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .map(|meta| meta.ino())
    }

    /// Notes that every event up to `seq` is durable: a seal `fsync`ed them.
    fn durable_through(&self, seq: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.0 = state.0.max(seq);
    }

    /// Moves onto `file`, the segment a seal just opened.
    fn follow(&self, file: Option<File>) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1 = file;
    }
}

/// The log of one device, open for appending. Holding it is being the device's one writer.
pub struct Log {
    dir: PathBuf,
    /// The segment being written, `events.jsonl`. `None` once a seal renamed it and the next
    /// one could not be opened yet: nothing is written until it is, so no line ever goes into
    /// a segment already sealed.
    file: Option<File>,
    /// What makes its events durable, which follows each seal.
    durable: std::sync::Arc<Durable>,
    device: String,
    retention: Retention,
    next: u64,
    /// How long the file is up to the end of its last whole line.
    good: u64,
    /// Whether a write failed partway, so the file may end in a torn line.
    torn: bool,
    /// How long the segment grows before a seal is tried: [`Retention::segment_bytes`], and
    /// further after a seal whose rename failed, so a failing rename is not retried at every
    /// append.
    seal_at: u64,
    /// Told the segment being written after each seal ([`Log::on_seal`]).
    on_seal: Option<SealHook>,
    /// A test's way to make the next write fail after this many bytes.
    #[cfg(test)]
    fail_after: Option<usize>,
    /// A test's way to make the next opens of a new segment fail.
    #[cfg(test)]
    fail_opens: u32,
    /// Held locked for as long as the `Log` lives, and unlocked when it is dropped, so a
    /// program forked meanwhile does not keep the next writer out (#1316). Declared last, so
    /// it is dropped last: the log's own files close before another writer can start.
    _lock: crate::filelock::Held,
}

impl Log {
    /// [`Log::open_with`] the [`Retention::DEFAULT`].
    pub fn open(dir: &Path, device: &str) -> io::Result<Log> {
        Log::open_with(dir, device, Retention::DEFAULT)
    }

    /// Opens the log in `dir`, the device's own directory under `<data>/events/`, making it if
    /// it is not there, and deletes the sealed segments `retention` no longer keeps.
    ///
    /// **One writer per device** (ADR 0066, ADR 0068): [`LOCK`] is locked for as long as the
    /// `Log` lives, and a second `open` while it is held is refused rather than interleaved.
    /// A lock held for only a moment is waited for, up to [`LOCK_WAIT`] (#972).
    /// The numbering carries on from the last event already in the log, so a `seq` is never
    /// used twice.
    pub fn open_with(dir: &Path, device: &str, retention: Retention) -> io::Result<Log> {
        crate::secrets::make_private_dir(dir)?;
        let lock = private_file(&dir.join(LOCK))?;
        lock_as_the_writer(&lock)?;
        let mut file = private_file(&dir.join(FILE))?;
        let good = cut_a_torn_line(&mut file)?;
        let last = match last_seq(&mut file)? {
            Some(last) => Some(last),
            None if good == 0 => match segments(dir)?.last() {
                Some((_, sealed)) => {
                    Some(last_seq(&mut File::open(sealed)?)?.ok_or_else(|| {
                        unreadable("its newest sealed segment holds no event purlis can read")
                    })?)
                }
                None => None,
            },
            // Lines, and not one of them an event charter wrote: counting from 1 again would
            // reuse every number already handed out, which is the one thing a seq may not do.
            None => return Err(unreadable("the event log holds no event purlis can read")),
        };
        let next = match last {
            Some(last) => after(last)?,
            None => 1,
        };
        let durable = std::sync::Arc::new(Durable {
            appended: std::sync::atomic::AtomicU64::new(next - 1),
            state: std::sync::Mutex::new((next - 1, Some(file.try_clone()?))),
        });
        let mut log = Log {
            dir: dir.to_owned(),
            file: Some(file),
            durable,
            device: device.to_owned(),
            retention,
            next,
            good,
            torn: false,
            seal_at: retention.segment_bytes,
            on_seal: None,
            #[cfg(test)]
            fail_after: None,
            #[cfg(test)]
            fail_opens: 0,
            _lock: crate::filelock::Held::locked(lock),
        };
        if log.good >= log.seal_at {
            log.seal_or_warn();
        }
        // A segment that could not be deleted is kept a little longer, never a refusal to log.
        if let Err(why) = prune(dir, retention.keep, std::time::SystemTime::now()) {
            tracing::warn!(error = %why, "the event log could not delete an old segment");
        }
        Ok(log)
    }

    /// Appends one event and answers it as written. When this returns `Ok` the whole line has
    /// been handed to the operating system, which a reader sees at once; it is durable once
    /// [`Durable::through`] its number has returned, which the host waits for before it tells
    /// a hook its line is taken (ADR 0075 §7, FD-30).
    pub fn append(
        &mut self,
        chat: Option<&str>,
        run: Option<&str>,
        parent_run: Option<&str>,
        kind: &str,
        body: serde_json::Value,
    ) -> io::Result<Event> {
        // Before anything else, so an append with no segment open uses no seq.
        self.live()?;
        if self.torn {
            // A write that failed partway may have left half a line, and the next line would
            // be glued to it: cut back to the last whole line first.
            let good = self.good;
            self.live()?.set_len(good)?;
            self.torn = false;
        }
        let seq = self.next;
        let event = Event {
            v: VERSION,
            device_id: self.device.clone(),
            seq,
            ulid: ulid::Ulid::generate().to_string(),
            chat: chat.map(str::to_owned),
            run: run.map(str::to_owned),
            parent_run: parent_run.map(str::to_owned),
            kind: kind.to_owned(),
            body,
        };
        let mut line = serde_json::to_vec(&event).map_err(io::Error::other)?;
        line.push(b'\n');
        // One `write_all` of one whole line on an append-mode file.
        #[cfg(test)]
        if let Some(after) = self.fail_after.take() {
            let _ = self.live()?.write_all(&line[..after.min(line.len())]);
            self.torn = true;
            return Err(io::Error::other("a write the test made fail"));
        }
        if let Err(why) = self.live()?.write_all(&line) {
            self.torn = true;
            return Err(why);
        }
        self.good += line.len() as u64;
        self.next = after(seq)?;
        self.durable
            .appended
            .store(seq, std::sync::atomic::Ordering::SeqCst);
        if self.good >= self.seal_at {
            // The event is written whatever sealing makes of it.
            self.seal_or_warn();
        }
        Ok(event)
    }

    /// Tells `hook` the segment being written each time a seal starts a new one, so whatever
    /// holds its own handle on the log's file (an `fsync`er, FD-30) moves to the new segment.
    /// Every event of the sealed segment is `fsync`ed before the seal, so nothing written
    /// before the hook runs is left behind on the old handle.
    pub fn on_seal(&mut self, hook: impl FnMut(&File) + Send + 'static) {
        self.on_seal = Some(Box::new(hook));
    }

    /// The segment being written, opened again if a seal left none open.
    fn live(&mut self) -> io::Result<&mut File> {
        if self.file.is_none() {
            let file = self.open_next()?;
            self.file = Some(file);
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("no segment of the event log is open"))
    }

    /// Opens a new, empty `events.jsonl`, `fsync`s the directory so its name is durable, and
    /// tells [`Log::on_seal`]'s hook.
    fn open_next(&mut self) -> io::Result<File> {
        #[cfg(test)]
        if self.fail_opens > 0 {
            self.fail_opens -= 1;
            return Err(io::Error::other("an open the test made fail"));
        }
        let file = private_file(&self.dir.join(FILE))?;
        sync_dir(&self.dir)?;
        self.good = 0;
        self.torn = false;
        // Before any event is written to it, so `Durable::through` never syncs a sealed one.
        self.durable.follow(file.try_clone().ok());
        if let Some(hook) = self.on_seal.as_mut() {
            hook(&file);
        }
        Ok(file)
    }

    /// [`Log::seal`], with a failure logged: the event before it is written either way. A
    /// rename that failed is tried again only after another quarter of a segment.
    fn seal_or_warn(&mut self) {
        match self.seal() {
            Ok(()) => self.seal_at = self.retention.segment_bytes,
            Err(why) => {
                if self.file.is_some() {
                    self.seal_at = self
                        .good
                        .saturating_add((self.retention.segment_bytes / 4).max(1));
                }
                tracing::warn!(error = %why, "the event log could not seal its segment");
            }
        }
    }

    /// Seals the segment being written as `events.<its first seq>.jsonl`, starts the next one,
    /// and deletes the sealed segments the retention no longer keeps.
    ///
    /// **The order is a subscriber's guarantee:** the sealed name appears only after its last
    /// line is written, so a reader that sees it can read the file to its end and be sure it
    /// has every line ([`Subscription::poll`]).
    ///
    /// **And a seal is durable:** the segment is `fsync`ed before its rename, and the
    /// directory after the rename and after the next segment is made. Once the rename is done
    /// the old file is never written again: if the next segment cannot be opened, appends
    /// fail until it can ([`Log::live`]).
    fn seal(&mut self) -> io::Result<()> {
        let Some(file) = self.file.as_mut() else {
            return Ok(());
        };
        let Some(first) = first_seq(file)? else {
            return Ok(());
        };
        file.sync_data()?;
        // Every event so far is in this segment and durable now.
        self.durable.durable_through(self.next.saturating_sub(1));
        let sealed = self.dir.join(sealed_name(first));
        std::fs::rename(self.dir.join(FILE), &sealed)
            .map_err(crate::rewrite::refused_at(&sealed))?;
        self.file = None;
        sync_dir(&self.dir)?;
        let next = self.open_next()?;
        self.file = Some(next);
        if let Err(why) = prune(&self.dir, self.retention.keep, std::time::SystemTime::now()) {
            tracing::warn!(error = %why, "the event log could not delete an old segment");
        }
        Ok(())
    }
}

impl Log {
    /// What makes this log's events durable, shared so the `fsync` can happen outside whatever
    /// lock holds the log.
    pub fn durable(&self) -> std::sync::Arc<Durable> {
        std::sync::Arc::clone(&self.durable)
    }
}

#[cfg(test)]
impl Log {
    /// Makes the next append write `bytes` of its line and fail, as a full disk would.
    pub fn fail_the_next_write_after(&mut self, bytes: usize) {
        self.fail_after = Some(bytes);
    }

    /// Makes the next `times` opens of a new segment fail, as a full directory would.
    pub fn fail_the_next_opens(&mut self, times: u32) {
        self.fail_opens = times;
    }
}

/// `fsync`s the directory `dir`, so a name made or renamed in it survives a power loss. A
/// directory cannot be opened as a file on Windows, where a rename is durable on its own.
fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// How long [`Log::open_with`] waits for a lock someone else holds before refusing (#972).
///
/// A lock is held by an open file, and every process forked while the log is open shares the
/// log's open files until it execs a program. A `Log` that is dropped unlocks first, so that
/// copy does not keep its lock (#1316); a host that just exited never unlocked, and its lock
/// outlives it while one of its children has not exec'd yet. That is not a second writer, and
/// it is over in far less than this.
pub const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// Locks `lock`, the device's [`LOCK`], waiting up to [`LOCK_WAIT`] for whoever holds it.
fn lock_as_the_writer(lock: &File) -> io::Result<()> {
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "another host is already writing this device's event log",
                ));
            }
            Err(std::fs::TryLockError::Error(why)) => return Err(why),
        }
    }
}

fn unreadable(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{what}, so it is not written to (starting again from 1 would reuse its numbers)"),
    )
}

/// `path`, opened to append and read, made private to this user if it is new.
fn private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true).read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(crate::rewrite::refused_at(path))
}

/// The name of the sealed segment whose first event is `first`: zero-padded, so the names
/// sort as the numbers do.
fn sealed_name(first: u64) -> String {
    format!("events.{first:020}.jsonl")
}

/// The sealed segments in `dir`, by their first seq, oldest first.
fn segments(dir: &Path) -> io::Result<Vec<(u64, PathBuf)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(why) => return Err(why),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(first) = name
            .to_str()
            .and_then(|name| name.strip_prefix("events."))
            .and_then(|rest| rest.strip_suffix(".jsonl"))
            .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|digits| digits.parse::<u64>().ok())
        else {
            continue;
        };
        found.push((first, entry.path()));
    }
    found.sort();
    Ok(found)
}

/// Deletes each sealed segment in `dir` whose newest event is older than `keep` at `now`, but
/// never the newest sealed segment: while the segment being written is empty, it alone holds
/// the last `seq`, and without it the next writer would count from 1 again. One whose newest
/// event cannot be read or dated is kept: nothing is deleted on a guess.
fn prune(dir: &Path, keep: std::time::Duration, now: std::time::SystemTime) -> io::Result<()> {
    let Some(oldest) = now.checked_sub(keep) else {
        return Ok(());
    };
    let oldest = oldest
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        });
    let mut sealed = segments(dir)?;
    sealed.pop();
    for (_, path) in sealed {
        let newest = last_event(&mut File::open(&path)?)?
            .and_then(|event| ulid::Ulid::from_string(&event.ulid).ok())
            .map(|id| id.timestamp_ms());
        if newest.is_some_and(|newest| newest < oldest) {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// The number after `seq`, or an error at the end of the range rather than a wrap to 0.
fn after(seq: u64) -> io::Result<u64> {
    seq.checked_add(1)
        .ok_or_else(|| io::Error::other("the event log has used every seq there is"))
}

/// Cuts `file` back to the end of its last whole line, where a crash left a part of one, and
/// answers how long it is then.
fn cut_a_torn_line(file: &mut File) -> io::Result<u64> {
    let len = file.metadata()?.len();
    let mut end = len;
    while end > 0 {
        let from = end.saturating_sub(TAIL);
        file.seek(SeekFrom::Start(from))?;
        let mut chunk = vec![0; usize::try_from(end - from).map_err(io::Error::other)?];
        file.read_exact(&mut chunk)?;
        if let Some(at) = chunk.iter().rposition(|byte| *byte == b'\n') {
            let whole = from + at as u64 + 1;
            if whole < len {
                file.set_len(whole)?;
            }
            return Ok(whole);
        }
        end = from;
    }
    if len > 0 {
        file.set_len(0)?;
    }
    Ok(0)
}

/// The last whole event in `file`, read from its end.
fn last_event(file: &mut File) -> io::Result<Option<Event>> {
    let len = file.metadata()?.len();
    let from = len.saturating_sub(TAIL);
    file.seek(SeekFrom::Start(from))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    Ok(tail
        .split(|byte| *byte == b'\n')
        .rev()
        .find_map(|line| serde_json::from_slice::<Event>(line).ok()))
}

/// The `seq` of the last whole event in `file`, read from its end.
fn last_seq(file: &mut File) -> io::Result<Option<u64>> {
    Ok(last_event(file)?.map(|event| event.seq))
}

/// The `seq` of the first event in `file`: its first line's, read alone, or the first event
/// in its first [`TAIL`] bytes where that line is not one.
fn first_seq(file: &mut File) -> io::Result<Option<u64>> {
    use std::io::BufRead;
    file.seek(SeekFrom::Start(0))?;
    let mut line = Vec::new();
    io::BufReader::new((&*file).take(TAIL)).read_until(b'\n', &mut line)?;
    if let Ok(event) = serde_json::from_slice::<Event>(&line) {
        return Ok(Some(event.seq));
    }
    file.seek(SeekFrom::Start(0))?;
    let mut head = Vec::new();
    file.take(TAIL).read_to_end(&mut head)?;
    Ok(head
        .split(|byte| *byte == b'\n')
        .find_map(|line| serde_json::from_slice::<Event>(line).ok())
        .map(|event| event.seq))
}

/// Every event in the log in `dir`, oldest first, across its sealed segments and the one being
/// written. A line that is not an event (a write the machine died in the middle of) is
/// skipped.
pub fn read(dir: &Path) -> io::Result<Vec<Event>> {
    let mut paths: Vec<PathBuf> = segments(dir)?.into_iter().map(|(_, path)| path).collect();
    paths.push(dir.join(FILE));
    let mut events = Vec::new();
    for path in paths {
        let text = match std::fs::read(&path) {
            Ok(text) => text,
            Err(why) if why.kind() == io::ErrorKind::NotFound => continue,
            Err(why) => return Err(why),
        };
        events.extend(
            text.split(|byte| *byte == b'\n')
                .filter_map(|line| serde_json::from_slice::<Event>(line).ok()),
        );
    }
    Ok(events)
}

/// What a subscription hands its client: an event, or word that some were missed.
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "nearly every delivery is an event: boxing each would allocate once per event \
              for the sake of a marker that comes rarely"
)]
pub enum Delivery {
    Event(Event),
    /// The events after `after` up to `resumes_at` are no longer kept ([`Retention`]): the
    /// stream carries on from the oldest event the log still holds, which is `resumes_at`. A
    /// client that held state built from the events it missed rebuilds it from what follows.
    /// Never a silent gap (ADR 0066).
    Missed {
        after: u64,
        resumes_at: u64,
    },
    /// The client's `cursor` is past the log's last event, `log_ends_at` (0 for none): the
    /// log lost lines the client had already read (a power loss, since a line is `fsync`ed
    /// only as the host answers it), and the next host numbers its events from after
    /// `log_ends_at` again. The stream carries on from there, so each of those events reaches
    /// the client; what it held past `log_ends_at` is no longer in the log, and it rebuilds
    /// from what follows. Never a silent skip of an event that reuses a number (#941).
    Ahead {
        cursor: u64,
        log_ends_at: u64,
    },
}

/// A client's place in one device's log ([`subscribe`]): it reads every event after its
/// cursor, in order, and then each one written after that, for as long as it is polled.
///
/// **It holds nothing but its cursor and the file it is reading**, so it outlives the writer:
/// a host that is killed and started again carries on in the same files, and a subscription
/// made again from [`Subscription::cursor`] reads on from the same place. A line the writer
/// died in the middle of is never delivered, because only whole lines are read, and the next
/// writer cuts it off and writes that `seq` whole.
pub struct Subscription {
    dir: PathBuf,
    /// The last `seq` delivered, or the cursor it was opened at.
    last: u64,
    reading: Option<Reading>,
    /// The first seq of the last segment read to its end, so it is not read again.
    past: u64,
    /// Whether the log has been seen to reach the cursor, so it is not ahead of it. Asked
    /// until it has: a log that has reached the cursor can only lose lines past it by losing
    /// the client with them, as one machine.
    reached: bool,
}

/// The segment a subscription is reading.
struct Reading {
    file: File,
    /// How far into it the whole lines read reach.
    offset: u64,
    /// The seq of its first event, once one is read: the name it is sealed under.
    first: Option<u64>,
    /// Whether it was a sealed segment when opened, so its end is the end.
    sealed: bool,
}

/// Subscribes to the log in `dir` from `since`, the last `seq` the client holds (0 for none):
/// ADR 0066's `subscribe(since)`.
pub fn subscribe(dir: &Path, since: u64) -> Subscription {
    Subscription {
        dir: dir.to_owned(),
        last: since,
        reading: None,
        past: 0,
        reached: since == 0,
    }
}

impl Subscription {
    /// The last `seq` this subscription delivered, which is what a client resubscribes from.
    pub fn cursor(&self) -> u64 {
        self.last
    }

    /// Every event written since the last poll, in order, each once. Never blocks: an empty
    /// answer means nothing new yet.
    pub fn poll(&mut self) -> io::Result<Vec<Delivery>> {
        let mut got = self.read_on()?;
        if !got.is_empty() {
            self.reached = true;
        } else if !self.reached {
            let end = log_end(&self.dir)?;
            self.reached = true;
            if end < self.last {
                got.push(Delivery::Ahead {
                    cursor: self.last,
                    log_ends_at: end,
                });
                self.last = end;
                // From the start again, with the cursor where the log is.
                self.reading = None;
                self.past = 0;
                got.extend(self.read_on()?);
            }
        }
        Ok(got)
    }

    /// What [`Subscription::poll`] reads: every event after the cursor that is there now.
    fn read_on(&mut self) -> io::Result<Vec<Delivery>> {
        let mut got = Vec::new();
        loop {
            let mut reading = match self.reading.take() {
                Some(reading) => reading,
                None => match self.find()? {
                    Some(reading) => reading,
                    None => return Ok(got),
                },
            };
            // Whether the segment is finished is asked BEFORE it is read: it is renamed only
            // after its last line was written, so what is read next is all of it. A segment
            // still being written is the live end, asked again at the next poll (by then with
            // its first seq, read now, where it had none).
            let sealed = reading.sealed || self.is_finished(reading.first)?;
            read_on(&mut reading, &mut self.last, &mut got)?;
            if !sealed {
                self.reading = Some(reading);
                return Ok(got);
            }
            self.past = reading.first.unwrap_or(self.past);
        }
    }

    /// Whether the live segment being read, whose first event is `first`, is no longer the
    /// one being written: `events.jsonl` is now another file, or none. That holds whether the
    /// segment is still there under its sealed name or was sealed and pruned between two
    /// polls. Told by the first seq, not an inode, so it means the same on every platform:
    /// two segments never begin with the same `seq`, and a new one begins empty.
    fn is_finished(&self, first: Option<u64>) -> io::Result<bool> {
        let Some(first) = first else {
            // Nothing read yet, so nothing to tell it by; an empty segment is never sealed.
            return Ok(false);
        };
        match File::open(self.dir.join(FILE)) {
            Ok(mut now) => Ok(first_seq(&mut now)? != Some(first)),
            Err(why) if why.kind() == io::ErrorKind::NotFound => Ok(true),
            Err(why) => Err(why),
        }
    }

    /// Opens the segment that holds the event after the cursor: the newest sealed segment not
    /// yet read that begins at or before it; else, when the cursor falls in a hole before a
    /// sealed segment (one pruned, or older than every one kept), the oldest sealed segment
    /// not yet read; else the one being written. The gap itself is told by [`read_on`].
    fn find(&mut self) -> io::Result<Option<Reading>> {
        loop {
            // The segment being written is opened BEFORE the sealed ones are listed. Listed
            // first, a seal between the two would go unseen: the list would lack the segment
            // just sealed and the open would find the new, later one, and every event in
            // between would be skipped. Opened first, the file held is either listed as sealed
            // by then, or it is still the newest segment there is.
            let live = match File::open(self.dir.join(FILE)) {
                Ok(file) => Some(file),
                Err(why) if why.kind() == io::ErrorKind::NotFound => None,
                Err(why) => return Err(why),
            };
            let sealed = segments(&self.dir)?;
            let wanted = self.last.saturating_add(1);
            let unread = |first: &u64| *first > self.past;
            let next = sealed
                .iter()
                .rev()
                .find(|(first, _)| unread(first) && *first <= wanted)
                .or_else(|| sealed.iter().find(|(first, _)| unread(first)));
            let Some((first, path)) = next else {
                return Ok(live.map(|file| Reading {
                    file,
                    offset: 0,
                    first: None,
                    sealed: false,
                }));
            };
            match File::open(path) {
                Ok(file) => {
                    return Ok(Some(Reading {
                        file,
                        offset: 0,
                        first: Some(*first),
                        sealed: true,
                    }));
                }
                // Pruned since it was listed: list again.
                Err(why) if why.kind() == io::ErrorKind::NotFound => continue,
                Err(why) => return Err(why),
            }
        }
    }
}

/// The `seq` of the last whole event in the log in `dir`, 0 for none: the segment being
/// written's, or where that has none, the newest sealed segment's, which is never pruned.
/// Read in that order, so a seal between the two reads is seen in one or the other.
fn log_end(dir: &Path) -> io::Result<u64> {
    let live = match File::open(dir.join(FILE)) {
        Ok(mut file) => last_seq(&mut file)?,
        Err(why) if why.kind() == io::ErrorKind::NotFound => None,
        Err(why) => return Err(why),
    };
    if let Some(seq) = live {
        return Ok(seq);
    }
    match segments(dir)?.last() {
        Some((_, path)) => match File::open(path) {
            Ok(mut file) => Ok(last_seq(&mut file)?.unwrap_or(0)),
            Err(why) if why.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(why) => Err(why),
        },
        None => Ok(0),
    }
}

/// Reads the whole lines of `reading` after its offset, delivering each event after `last`.
fn read_on(reading: &mut Reading, last: &mut u64, got: &mut Vec<Delivery>) -> io::Result<()> {
    reading.file.seek(SeekFrom::Start(reading.offset))?;
    let mut bytes = Vec::new();
    (&reading.file).read_to_end(&mut bytes)?;
    // Only whole lines: one the writer is still writing is read on a later poll.
    let whole = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);
    reading.offset += whole as u64;
    for line in bytes[..whole].split(|byte| *byte == b'\n') {
        let Ok(event) = serde_json::from_slice::<Event>(line) else {
            continue;
        };
        reading.first.get_or_insert(event.seq);
        if event.seq <= *last {
            continue;
        }
        if event.seq > last.saturating_add(1) {
            // Events between the cursor and this one are no longer kept: say so, never a
            // silent gap (ADR 0066).
            got.push(Delivery::Missed {
                after: *last,
                resumes_at: event.seq,
            });
        }
        *last = event.seq;
        got.push(Delivery::Event(event));
    }
    Ok(())
}

/// The file in a device's directory that holds the key its args digests are made with.
pub const ARGS_KEY: &str = "args.key";

/// How many random bytes the args key is.
const KEY_BYTES: usize = 32;

/// The device's key for args digests (O3: "with an args digest, never raw args").
///
/// **Keyed, because a plain hash of a short command can be guessed.** `ls -la` has one SHA-256
/// and anybody can compute it; an HMAC under a key only this device holds cannot be matched
/// from outside, and still lets two calls on this device be seen to have had the same
/// arguments. The key is made once and kept, as `fingerprint.key` is for a project's `fp:`
/// values, so a digest compares with yesterday's; AU-19 may move it into the keyring.
///
/// **Who can read it:** anything running as the same OS user, and whoever holds a backup that
/// carries it (FR-10 backs it up with the log). The key keeps the digests from everyone else.
/// The hook sends the host the arguments' *unkeyed* SHA-256, on the chat's own hook channel,
/// which only the same user can reach.
pub struct ArgsKey(Vec<u8>);

impl ArgsKey {
    /// The key in `dir`, made on first use: [`crate::secrets::key_file`], as `fingerprint.key`
    /// is.
    pub fn open(dir: &Path) -> io::Result<ArgsKey> {
        crate::secrets::key_file(dir, &dir.join(ARGS_KEY), KEY_BYTES).map(ArgsKey)
    }

    /// The digest the log holds for arguments whose [`args_hash`] is `hash`: HMAC-SHA256 under
    /// this device's key, in hex.
    pub fn digest(&self, hash: &str) -> String {
        use hmac::{KeyInit, Mac};
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(&self.0)
            .expect("HMAC takes a key of any length");
        mac.update(hash.as_bytes());
        crate::extension::hex(&mac.finalize().into_bytes())
    }
}

/// SHA-256 of a tool call's arguments as JSON, in hex: what a hook sends the host in place of
/// the arguments, which never leave the hook's process.
pub fn args_hash(args: &serde_json::Value) -> String {
    use sha2::Digest;
    let text = serde_json::to_vec(args).unwrap_or_default();
    crate::extension::hex(&sha2::Sha256::digest(&text))
}

/// What the board did with a report's conversation, which is how the host tells a new run
/// (ADR 0066, "A run is a stretch…").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Followed {
    /// The chat is in the conversation it was in, or the board refused the report.
    No,
    /// The harness named its conversation for the first time (Codex and opencode do, in the
    /// first turn's hook). The run gains a conversation; it does not start again.
    FirstNamed,
    /// The board followed the chat off one conversation onto another: `/clear`, a new run.
    Moved,
}

/// Why a run began: ADR 0066's `cause`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Began {
    /// The chat was created: its first run. Also a chat's first line when the host was never
    /// told who it is, which no chat charter starts produces.
    Start,
    /// The board followed the chat onto another conversation.
    Clear,
    /// A sub-agent's first call: a child run of the run that was current.
    Child,
    /// The app relaunched the chat and resumed its conversation (`reopen.json`'s `resume`).
    Reopen,
    /// A hibernated chat was resumed (SC-4).
    Wake,
    /// The chat was started again without its conversation: its harness could not find it
    /// (`lostOnResume`), or a workspace rename dropped it.
    Fresh,
    /// An attribute of the run changed while the chat carried on: its harness, profile, model,
    /// persona or sandbox.
    Switch,
}

impl Began {
    /// The cause's word, as `run.started`'s body says it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Clear => "clear",
            Self::Child => "child",
            Self::Reopen => "reopen",
            Self::Wake => "wake",
            Self::Fresh => "fresh",
            Self::Switch => "switch",
        }
    }

    /// Why a chat a relaunch put back begins a run, from how it came back: `reopen` when it
    /// is in the conversation it had, or had none to lose (a program that is no harness), and
    /// `fresh` when a harness chat came back without it.
    ///
    /// A chat whose own arguments name a session is `reopen`: the operator's words resume it,
    /// and charter has no other answer to hold them to.
    pub fn at_relaunch(how: &crate::reopen::Reopened, harness: bool) -> Began {
        use crate::reopen::{Fresh, Reopened};
        match how {
            Reopened::Resumed(_) | Reopened::Fresh(Fresh::SessionNamedByTheOperator) => {
                Self::Reopen
            }
            Reopened::Fresh(_) if !harness => Self::Reopen,
            Reopened::Fresh(_) => Self::Fresh,
        }
    }
}

/// A run by its two ids, the chat's and its own: named, so a call cannot swap the two ULIDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOf<'a> {
    pub chat: &'a str,
    pub run: &'a str,
}

/// Which end of a tool call a tool hook is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Before the tool runs: `PreToolUse`.
    Pre,
    /// After it ran: `PostToolUse`, or `PostToolUseFailure` for a call that failed.
    Post,
}

/// The phase of a tool hook word charter answers, from `hookreg` and nothing else: a word it
/// does not answer is no phase, and no event kind.
pub fn phase(word: &str) -> Option<Phase> {
    let event = crate::hookreg::find(word)
        .filter(|handler| handler.matcher.is_some())
        .map(|handler| handler.event)
        .or_else(|| {
            crate::hookreg::NO_OPS
                .contains(&word)
                .then_some("PostToolUse")
        })?;
    match event {
        "PreToolUse" => Some(Phase::Pre),
        // A failed call's post hook is still the end of the call.
        "PostToolUse" | "PostToolUseFailure" => Some(Phase::Post),
        _ => None,
    }
}

/// A spool drain's kinds (ADR 0075 §4's registry): a gap in a chat's sequence, a line that did
/// not check, a sequence drained, and a refused commit's line drained from a spool.
pub const SPOOL_GAP: &str = "hook.spool.gap";
pub const SPOOL_REJECTED: &str = "hook.spool.rejected";
pub const SPOOL_DRAINED: &str = "hook.spool.drained";
pub const COMMIT_REFUSED: &str = "hook.commit_refused";

/// `body` with `spooled`, the number a drained line had in its chat's spool, when it had one
/// (FD-30): the one place an event is marked as having come through the spool. Added after
/// what the body already says, so every event keeps its keys in the order it always had.
fn spooled_as(mut body: serde_json::Value, seq: Option<u64>) -> serde_json::Value {
    if let Some(seq) = seq {
        body["spooled"] = seq.into();
    }
    body
}

/// The kind a tool call's event is recorded under: `hook.<word>` for a word charter answers.
fn tool_kind(call: &crate::hookwire::ToolCall) -> String {
    match phase(&call.tool_hook) {
        Some(_) => format!("hook.{}", call.tool_hook),
        None => UNKNOWN_HOOK.to_owned(),
    }
}

/// The kind a tool hook word charter does not answer is recorded under, so a line cannot mint
/// a kind of its own: the word goes in the body, shortened.
pub const UNKNOWN_HOOK: &str = "hook.unknown";

/// Who a chat is in the log: its id and its current run's.
#[derive(Debug, Clone)]
struct Identity {
    chat: String,
    run: String,
}

/// The run an event is recorded under, and the run that one came from.
#[derive(Debug, Clone)]
struct Under {
    chat: String,
    run: String,
    parent: Option<String>,
}

/// The host's side of the log: it turns what the hook channel hears into events, one per hook
/// call, each under the chat and the run it happened in.
///
/// **The host decides runs, never a hook** (ADR 0066, "What changes where"): a hook carries
/// the chat's number and token and nothing more, and the host maps the number to the chat's
/// id and current run in its own memory.
///
/// - The host tells it each chat it starts ([`Recorder::begin`]): the chat's id, which the app
///   keeps across a relaunch, and the run it begins, with why (`start`, `reopen`, `fresh`,
///   `switch`, `wake`).
/// - A chat it was never told of is given an id and its first run, `cause: start`, at its
///   first line, whatever the board made of that line. Only a chat charter started holds a
///   token the channel admits, so such a line is from a chat this host started.
/// - A chat the board follows onto another conversation begins a run with `cause: clear`. A
///   line the board refused (a harness nested in the chat's shell, ADR 0024 C5) is `No` and
///   never does.
/// - A sub-agent's calls are a child run (`cause: child`) of the run that was current when it
///   first appeared, until its `SubagentStop` (ADR 0066, "A child agent…").
pub struct Recorder {
    /// The device's directory under `<data>/events/`, where it is known.
    dir: PathBuf,
    log: Log,
    key: ArgsKey,
    /// By project and number: a number means nothing outside the project that dealt it.
    chats: HashMap<(PathBuf, u32), Identity>,
    /// Each live sub-agent's child run, by chat id and the harness's agent id.
    children: HashMap<(String, String), (String, String)>,
    /// The sub-agents whose `SubagentStop` was heard, by chat id and agent id: a later line
    /// from one is its parent run's, never a second child run.
    stopped: std::collections::HashSet<(String, String)>,
    /// The first of each tool call's pre and post hooks heard, by run and the harness's id for
    /// the call: which it was, when the host heard it, and when the hook said it began.
    calls: HashMap<(String, String), Heard>,
}

/// One end of a tool call, heard and waiting for the other.
#[derive(Debug, Clone, Copy)]
struct Heard {
    phase: Phase,
    at: Instant,
    at_ms: u64,
}

/// How long one end of a tool call is waited on for the other. One that never came (the
/// harness died, or a guard refused the call) is a duration lost, not a leak: it is let go
/// when anything is next recorded after this long, and an end heard later gives no duration.
pub const CALL_IS_OPEN_AT_MOST: std::time::Duration = std::time::Duration::from_secs(3600);

/// How many stopped sub-agents are remembered.
const AGENTS_HELD: usize = 4096;

impl Recorder {
    /// The recorder for this device, its log under `<data>` and its device id from the
    /// machine store under `config`.
    ///
    /// `<data>/events/<device>/`: by device, as the audit is (ADR 0075 §6), so a backup
    /// restored onto another machine is kept as that device's records and never appended to
    /// (ADR 0069 §5). A `<data>` inside a project or a git work tree is refused before
    /// anything is made there.
    pub fn open_in(config: &Path, data: &Path) -> io::Result<Recorder> {
        if let Some(why) = crate::datahome::refusal(data) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, why));
        }
        let device = crate::machine::device_id(config)?;
        let dir = data.join(EVENTS).join(&device);
        let mut recorder = Recorder::new(Log::open(&dir, &device)?, ArgsKey::open(&dir)?);
        recorder.dir = dir;
        Ok(recorder)
    }

    /// [`Recorder::open_in`] for this process: the machine store's and `<data>`'s own homes.
    pub fn open() -> io::Result<Recorder> {
        let config = crate::machine::config_root()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config home"))?;
        let data = crate::datahome::root()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no data home"))?;
        Recorder::open_in(&config, &data)
    }

    pub fn new(log: Log, key: ArgsKey) -> Recorder {
        Recorder {
            dir: PathBuf::new(),
            log,
            key,
            chats: HashMap::new(),
            children: HashMap::new(),
            stopped: std::collections::HashSet::new(),
            calls: HashMap::new(),
        }
    }

    /// The device's directory the log is in, as [`Recorder::open_in`] found it.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The event for one state hook: `hook.<word>`, after a `run.started` when the report
    /// begins a run.
    pub fn report(
        &mut self,
        plane: &Path,
        report: &crate::hookwire::Report,
        followed: Followed,
    ) -> io::Result<Event> {
        self.report_with(plane, report, followed, None)
    }

    /// [`Recorder::report`], with the run the host minted for a move (`cleared`): the run a
    /// `/clear` begins is the host's, so the record can name it whether or not there is a log.
    /// `None` mints one here.
    pub fn report_with(
        &mut self,
        plane: &Path,
        report: &crate::hookwire::Report,
        followed: Followed,
        cleared: Option<&str>,
    ) -> io::Result<Event> {
        self.report_spooled(plane, report, followed, cleared, None)
    }

    /// [`Recorder::report_with`], saying the spool number a drained report had.
    fn report_spooled(
        &mut self,
        plane: &Path,
        report: &crate::hookwire::Report,
        followed: Followed,
        cleared: Option<&str>,
        spooled: Option<u64>,
    ) -> io::Result<Event> {
        let known = self.chats.contains_key(&(plane.to_path_buf(), report.chat));
        let top = match followed {
            Followed::Moved if known => self.new_run(plane, report.chat, Began::Clear, cleared)?,
            Followed::Moved | Followed::No | Followed::FirstNamed => {
                self.identity(plane, report.chat)?
            }
        };
        let under = self.under(top, report.agent.as_deref())?;
        let mut body = serde_json::json!({});
        if report.event == crate::state::Event::SessionStart {
            body["started"] = started(report.detail.started).into();
        }
        let body = spooled_as(body, spooled);
        let event = self.append(&under, &format!("hook.{}", report.event.word()), body)?;
        match (report.event, &report.agent) {
            (crate::state::Event::SubagentStop, Some(agent)) => {
                let key = (under.chat.clone(), agent.clone());
                let live = self.children.remove(&key);
                if self.stopped.len() >= AGENTS_HELD {
                    self.stopped.clear();
                }
                self.stopped.insert(key);
                // Its own stop ends its run (ADR 0076 §6): once, for the child that was live.
                if let Some((run, parent)) = live {
                    self.end_child(
                        &under.chat,
                        run,
                        parent,
                        RunState::Completed,
                        Cause::ChildEnded,
                    )?;
                }
            }
            // The chat's session is over, and every sub-agent of its run with it: in the end
            // its parent's run comes to. A clear supersedes the run; a session over for good
            // is the run that ends `completed | exited` at the exit that follows (ADR 0076,
            // ADR 0068 amended). A child has no exit code of its own to say otherwise.
            (crate::state::Event::SessionEnd, None) => {
                let cause = match report.detail.ending {
                    crate::state::Ending::Cleared => Cause::Superseded,
                    crate::state::Ending::ForGood => Cause::Exited(crate::state::run::Exit::Lost {
                        session_ended: true,
                    }),
                };
                self.end_children(&under.chat, RunState::Completed, cause)?;
            }
            _ => {}
        }
        Ok(event)
    }

    /// The event for one tool hook: `hook.<word>` for a word charter answers, with the tool,
    /// the keyed digest of its arguments, what charter answered and how long the hook took. A
    /// post hook whose call's pre hook was heard also says how long the tool ran, from one to
    /// the other, as the host heard them (`now`). A word charter does not answer is recorded
    /// as [`UNKNOWN_HOOK`].
    pub fn tool(
        &mut self,
        plane: &Path,
        call: &crate::hookwire::ToolCall,
        now: Instant,
    ) -> io::Result<Event> {
        self.tool_spooled(plane, call, now, None)
    }

    /// [`Recorder::tool`], saying the spool number a drained call had.
    fn tool_spooled(
        &mut self,
        plane: &Path,
        call: &crate::hookwire::ToolCall,
        now: Instant,
        spooled: Option<u64>,
    ) -> io::Result<Event> {
        let top = self.identity(plane, call.chat)?;
        let under = self.under(top, call.agent.as_deref())?;
        let phase = phase(&call.tool_hook);
        let mut body = spooled_as(self.tool_body(call), spooled);
        if let Some(id) = &call.call {
            let key = (under.run.clone(), id.clone());
            if let Some(phase) = phase
                && let Some(ms) = self.pair(key, phase, now, call.at_ms)
            {
                body["tool_ms"] = ms.into();
            }
        }
        self.append(&under, &tool_kind(call), body)
    }

    /// What a tool event's body says of its call, before any duration.
    fn tool_body(&self, call: &crate::hookwire::ToolCall) -> serde_json::Value {
        let mut body = serde_json::json!({
            "decision": call.decision.word(),
            "hook_ms": call.hook_ms,
        });
        if phase(&call.tool_hook).is_none() {
            body["word"] =
                crate::shown::readable(&call.tool_hook, crate::shown::DISPLAY_LIMIT).into();
        }
        if let Some(tool) = &call.tool {
            body["tool"] = tool.as_str().into();
        }
        if let Some(args) = &call.args {
            body["args"] = self.key.digest(args).into();
        }
        if let Some(rule) = &call.rule {
            body["rule"] = rule.as_str().into();
        }
        if let Some(id) = &call.call {
            body["call"] = id.as_str().into();
        }
        body
    }

    /// Tells the recorder who chat `number` in `plane` is and which run it is in, without an
    /// event: a chat the reopen record names, before the host starts anything, so a line it
    /// spooled before the host went away is recorded under the run it ran in (FD-30).
    pub fn knows(&mut self, plane: &Path, number: u32, RunOf { chat, run }: RunOf<'_>) {
        self.chats.insert(
            (plane.to_path_buf(), number),
            Identity {
                chat: chat.to_owned(),
                run: run.to_owned(),
            },
        );
    }

    /// The event for one thing a drain of the hook spool found (FD-30, ADR 0075 §4): a line
    /// recorded as it would have been live, with `spooled` its number, or `hook.spool.gap`,
    /// `hook.spool.rejected` or `hook.spool.drained`.
    ///
    /// **A chat the recorder does not know is not given a run.** A drain runs before the host
    /// starts a chat, so a chat it cannot name ([`Recorder::knows`]) is one that had closed; its
    /// events are recorded with no chat or run, under its number (`chat_number`).
    #[cfg(unix)]
    pub fn spooled(
        &mut self,
        plane: &Path,
        item: crate::hookwire::spool::Drained,
    ) -> io::Result<Event> {
        use crate::hookwire::spool::{Drained, Spooled};
        let number = item.chat();
        let known = self.chats.get(&(plane.to_path_buf(), number)).cloned();
        let (kind, mut body) = match item {
            Drained::Line { seq, line, .. } => match (line, &known) {
                (Spooled::Report(report), Some(_)) => {
                    return self.report_spooled(plane, &report, Followed::No, None, Some(seq));
                }
                (Spooled::Tool(call), Some(_)) => {
                    return self.tool_spooled(plane, &call, Instant::now(), Some(seq));
                }
                (Spooled::Refused(refused), Some(_)) => {
                    return self.refused_spooled(plane, refused.chat, Some(seq));
                }
                (Spooled::Report(report), None) => (
                    format!("hook.{}", report.event.word()),
                    spooled_as(serde_json::json!({}), Some(seq)),
                ),
                (Spooled::Tool(call), None) => (
                    tool_kind(&call),
                    spooled_as(self.tool_body(&call), Some(seq)),
                ),
                (Spooled::Refused(_), None) => (
                    COMMIT_REFUSED.to_owned(),
                    spooled_as(serde_json::json!({}), Some(seq)),
                ),
            },
            Drained::Gap { from, to, .. } => (
                SPOOL_GAP.to_owned(),
                serde_json::json!({ "from": from, "to": to }),
            ),
            Drained::Rejected { seq, why, .. } => (
                SPOOL_REJECTED.to_owned(),
                serde_json::json!({ "seq": seq, "why": why.word() }),
            ),
            Drained::Spool { from, to, .. } => (
                SPOOL_DRAINED.to_owned(),
                serde_json::json!({ "from": from, "to": to }),
            ),
        };
        body["chat_number"] = number.into();
        match known {
            Some(who) => self
                .log
                .append(Some(&who.chat), Some(&who.run), None, &kind, body),
            None => self.log.append(None, None, None, &kind, body),
        }
    }

    /// The event for a commit charter's git hook refused in chat `number` (SQ-16):
    /// `hook.commit_refused`, under the chat's run. Metadata only: what was found is the
    /// needs-you item's, never the log's.
    pub fn refused(&mut self, plane: &Path, number: u32) -> io::Result<Event> {
        self.refused_spooled(plane, number, None)
    }

    fn refused_spooled(
        &mut self,
        plane: &Path,
        number: u32,
        spooled: Option<u64>,
    ) -> io::Result<Event> {
        let who = self.identity(plane, number)?;
        let body = spooled_as(serde_json::json!({}), spooled);
        self.log
            .append(Some(&who.chat), Some(&who.run), None, COMMIT_REFUSED, body)
    }

    /// The trust event for a change to a chat's sandbox: `trust.sandbox.off` or
    /// `trust.sandbox.on` (ADR 0067 §7), under the chat and the run the host is about to begin
    /// for it ([`RunOf`]), with the chat's `harness` and `persona`. Written **before** the
    /// chat's program runs, so an unsandboxed chat is never running unrecorded; an error is
    /// the host's to act on (a person's opt-out is then not started). Written here until the
    /// audit exists, which is written from this log (ADR 0075, amending ADR 0067 §7).
    pub fn trust(
        &mut self,
        RunOf { chat, run }: RunOf<'_>,
        change: &crate::sandbox::Change,
        harness: Option<crate::harness::Harness>,
        persona: Option<&str>,
    ) -> io::Result<Event> {
        self.log.append(
            Some(chat),
            Some(run),
            None,
            change.kind(),
            change.body(harness, persona),
        )
    }

    /// The audit of a sandbox grant or revoke (#1342, [`crate::sandbox::grant::Audited`]):
    /// under chat `number`'s current run where it came from one, with no chat for one taken
    /// back in Settings. Made durable before it answers, so no grant reaches a chat unrecorded.
    pub fn sandbox_grant(
        &mut self,
        plane: &Path,
        number: Option<u32>,
        audited: &crate::sandbox::grant::Audited<'_>,
    ) -> io::Result<Event> {
        let who = number
            .map(|number| self.identity(plane, number))
            .transpose()?;
        let event = self.log.append(
            who.as_ref().map(|who| who.chat.as_str()),
            who.as_ref().map(|who| who.run.as_str()),
            None,
            audited.kind(),
            audited.body(),
        )?;
        self.log.durable().through(event.seq)?;
        Ok(event)
    }

    /// The audit of a dispatch grant or revoke (#1437, [`crate::dispatchgrant::Audited`]):
    /// under chat `number`'s current run where it was allowed from one, with no chat for one
    /// taken back in Settings. Made durable before it answers, so no grant is in force
    /// unrecorded.
    pub fn dispatch_grant(
        &mut self,
        plane: &Path,
        number: Option<u32>,
        audited: &crate::dispatchgrant::Audited<'_>,
    ) -> io::Result<Event> {
        self.dispatch_grant_said(plane, number, audited, None)
    }

    /// [`Recorder::dispatch_grant`] for a grant or a never the person made somewhere other
    /// than a chat's Notice or Settings: the same event, whose body also says where, as
    /// `from` (#1507: `"away"` for one made on a refusal kept while nobody was there,
    /// [`crate::dispatchaway::AUDITED_FROM`]). So a later reader can tell such a grant from
    /// one made in Settings.
    pub fn dispatch_grant_from(
        &mut self,
        plane: &Path,
        number: Option<u32>,
        audited: &crate::dispatchgrant::Audited<'_>,
        from: &str,
    ) -> io::Result<Event> {
        self.dispatch_grant_said(plane, number, audited, Some(from))
    }

    fn dispatch_grant_said(
        &mut self,
        plane: &Path,
        number: Option<u32>,
        audited: &crate::dispatchgrant::Audited<'_>,
        from: Option<&str>,
    ) -> io::Result<Event> {
        let who = number
            .map(|number| self.identity(plane, number))
            .transpose()?;
        let mut body = audited.body();
        if let (Some(from), Some(fields)) = (from, body.as_object_mut()) {
            fields.insert("from".to_owned(), from.into());
        }
        let event = self.log.append(
            who.as_ref().map(|who| who.chat.as_str()),
            who.as_ref().map(|who| who.run.as_str()),
            None,
            audited.kind(),
            body,
        )?;
        self.log.durable().through(event.seq)?;
        Ok(event)
    }

    /// Chat `number`'s current run has ended, in `state` for `cause`: each of its sub-agents
    /// still live ends with it, in the same state and for the same cause (ADR 0076 §6), with a
    /// `run.ended` under its child run. What the host calls at the run's exit or its own stop;
    /// a `/clear`, a `SessionEnd` and a new run begun for the chat end them here already. The
    /// events written, none when no child was live.
    ///
    /// Refused, with nothing ended, for a `state` that is not an end state.
    pub fn end_children_with(
        &mut self,
        plane: &Path,
        number: u32,
        state: RunState,
        cause: Cause,
    ) -> io::Result<Vec<Event>> {
        if !state.is_end() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("a run does not end {state:?}"),
            ));
        }
        let Some(chat) = self
            .chats
            .get(&(plane.to_path_buf(), number))
            .map(|who| who.chat.clone())
        else {
            return Ok(Vec::new());
        };
        self.end_children(&chat, state, cause)
    }

    /// What makes this recorder's events durable: [`Durable::through`] after each write.
    pub fn durable(&self) -> std::sync::Arc<Durable> {
        self.log.durable()
    }

    /// How long a tool call ran, once both of its ends have been heard, in either order.
    ///
    /// The first end heard is kept. The second gives the duration: by the hooks' own clocks
    /// when both said when they began (`at_ms`), else by when the host heard each. A second
    /// pre hook of one call (`pretooluse` and `pretooluse-read`) is not a later start, and an
    /// end left waiting longer than [`CALL_IS_OPEN_AT_MOST`] is let go: the other end, heard
    /// later than that, gives no duration.
    fn pair(
        &mut self,
        key: (String, String),
        phase: Phase,
        now: Instant,
        at_ms: u64,
    ) -> Option<u64> {
        self.calls
            .retain(|_, heard| now.saturating_duration_since(heard.at) <= CALL_IS_OPEN_AT_MOST);
        match self.calls.get(&key) {
            Some(first) if first.phase != phase => {
                let first = self.calls.remove(&key)?;
                let (pre, post) = match phase {
                    Phase::Post => (first.at_ms, at_ms),
                    Phase::Pre => (at_ms, first.at_ms),
                };
                if pre > 0 && post > 0 {
                    return Some(post.saturating_sub(pre));
                }
                let ran = now.saturating_duration_since(first.at).as_millis();
                Some(u64::try_from(ran).unwrap_or(u64::MAX))
            }
            Some(_) => None,
            None => {
                self.calls.insert(
                    key,
                    Heard {
                        phase,
                        at: now,
                        at_ms,
                    },
                );
                None
            }
        }
    }

    fn append(&mut self, under: &Under, kind: &str, body: serde_json::Value) -> io::Result<Event> {
        self.log.append(
            Some(&under.chat),
            Some(&under.run),
            under.parent.as_deref(),
            kind,
            body,
        )
    }

    /// The run an event of `top`'s chat is under: `top`'s, or the child run of `agent`, begun
    /// the first time that agent is seen.
    fn under(&mut self, top: Identity, agent: Option<&str>) -> io::Result<Under> {
        let Some(agent) = agent else {
            return Ok(Under {
                chat: top.chat,
                run: top.run,
                parent: None,
            });
        };
        let key = (top.chat.clone(), agent.to_owned());
        if self.stopped.contains(&key) {
            // A line from an agent whose stop was heard: its run is over, so the line is the
            // parent run's, as an unmeasured harness's sub-agent's would be.
            return Ok(Under {
                chat: top.chat,
                run: top.run,
                parent: None,
            });
        }
        if let Some((run, parent)) = self.children.get(&key) {
            return Ok(Under {
                chat: top.chat,
                run: run.clone(),
                parent: Some(parent.clone()),
            });
        }
        let under = Under {
            chat: top.chat,
            run: crate::reopen::mint(),
            parent: Some(top.run),
        };
        self.append(
            &under,
            "run.started",
            serde_json::json!({ "cause": Began::Child.word() }),
        )?;
        self.children.insert(
            key,
            (under.run.clone(), under.parent.clone().unwrap_or_default()),
        );
        Ok(under)
    }

    /// Begins run `run` of chat `number`, under the chat id `chat` ([`RunOf`]), for `cause`, and says so
    /// in the log: what the host does when it starts a chat (ADR 0066, "A new run begins
    /// exactly when…").
    ///
    /// **The id is the host's, never minted here**: the app keeps it in `reopen.json`, so a
    /// chat put back after a relaunch is the chat it was, and its first event after the
    /// relaunch is this run's `run.started`. Every sub-agent of the run before ends with it.
    pub fn begin(
        &mut self,
        plane: &Path,
        number: u32,
        RunOf { chat, run }: RunOf<'_>,
        cause: Began,
    ) -> io::Result<Event> {
        // A child still live is of the run this one takes the place of, which has ended
        // superseded (ADR 0076 §2): a run whose own end was told has none left.
        let ended = self.end_children(chat, RunState::Completed, Cause::Superseded);
        // Held before the line is written: a log that refuses the line must not leave the
        // chat's next hook line to mint it a second id.
        self.chats.insert(
            (plane.to_path_buf(), number),
            Identity {
                chat: chat.to_owned(),
                run: run.to_owned(),
            },
        );
        ended?;
        self.log.append(
            Some(chat),
            Some(run),
            None,
            "run.started",
            serde_json::json!({ "cause": cause.word() }),
        )
    }

    /// The chat's identity, giving it one and its first run when it has none.
    fn identity(&mut self, plane: &Path, number: u32) -> io::Result<Identity> {
        match self.chats.get(&(plane.to_path_buf(), number)) {
            Some(who) => Ok(who.clone()),
            None => self.new_run(plane, number, Began::Start, None),
        }
    }

    /// Ends every sub-agent of `chat` still live: its run ended in `state` for `cause`, and
    /// theirs with it, each with a `run.ended` (ADR 0076 §6). This is how a Codex chat's child
    /// runs end at all, since charter does not arm Codex's `SubagentStop`.
    ///
    /// Every one is forgotten whether or not its line is written, and every line is tried:
    /// the first refusal is the answer.
    fn end_children(
        &mut self,
        chat: &str,
        state: RunState,
        cause: Cause,
    ) -> io::Result<Vec<Event>> {
        let mut live = Vec::new();
        self.children.retain(|(of, _), (run, parent)| {
            let keep = of != chat;
            if !keep {
                live.push((run.clone(), parent.clone()));
            }
            keep
        });
        self.stopped.retain(|(of, _)| of != chat);
        // In the order they began: a ULID's order is its time's.
        live.sort();
        let mut ended = Vec::new();
        let mut refused = None;
        for (run, parent) in live {
            match self.end_child(chat, run, parent, state, cause) {
                Ok(event) => ended.push(event),
                Err(why) => {
                    refused.get_or_insert(why);
                }
            }
        }
        refused.map_or(Ok(ended), Err)
    }

    /// The `run.ended` of child run `run` of `parent`: its end state and cause.
    fn end_child(
        &mut self,
        chat: &str,
        run: String,
        parent: String,
        state: RunState,
        cause: Cause,
    ) -> io::Result<Event> {
        let under = Under {
            chat: chat.to_owned(),
            run,
            parent: Some(parent),
        };
        self.append(
            &under,
            "run.ended",
            serde_json::json!({ "state": end_word(state), "cause": cause.word() }),
        )
    }

    /// Begins a run of the chat for `cause`, as `run` where the host minted it, and says so in
    /// the log.
    fn new_run(
        &mut self,
        plane: &Path,
        number: u32,
        cause: Began,
        run: Option<&str>,
    ) -> io::Result<Identity> {
        let key = (plane.to_path_buf(), number);
        let chat = self
            .chats
            .get(&key)
            .map_or_else(crate::reopen::mint, |who| who.chat.clone());
        let who = Identity {
            chat,
            run: run.map_or_else(crate::reopen::mint, str::to_owned),
        };
        if cause == Began::Clear {
            // The cleared run is superseded, and its children end with it, before the run
            // that takes its place begins.
            self.end_children(&who.chat, RunState::Completed, Cause::Superseded)?;
        }
        self.log.append(
            Some(&who.chat),
            Some(&who.run),
            None,
            "run.started",
            serde_json::json!({ "cause": cause.word() }),
        )?;
        self.chats.insert(key, who.clone());
        Ok(who)
    }
}

/// An end state's word, as `run.ended`'s body says it (ADR 0076 §1). Every caller passes an
/// end state; any other would read as no end at all.
fn end_word(state: RunState) -> &'static str {
    match state {
        RunState::Completed => "completed",
        RunState::Failed => "failed",
        RunState::Stopped => "stopped",
        _ => "not-ended",
    }
}

/// What a `SessionStart` said it was for, as its body says it.
fn started(how: crate::state::Started) -> &'static str {
    use crate::state::Started;
    match how {
        Started::Freshly => "fresh",
        Started::Cleared => "cleared",
        Started::Compacted => "compacted",
        Started::Unsaid => "unsaid",
    }
}

#[cfg(test)]
#[path = "eventlog/tests.rs"]
mod tests;
