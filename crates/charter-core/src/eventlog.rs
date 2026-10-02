//! The host's event log (FD-9, #649): one structured event per hook call, under the chat and
//! the run it happened in.
//!
//! **One log per device, one writer** (ADR 0066, ADR 0068): the app today, `charterd` once
//! FD-5 moves the board there. A `charter` command never writes it. It reaches the writer over
//! the hook channel, which carries a chat's number and token and nothing more, and the host
//! maps the number to the chat's id and current run itself ([`Recorder`]).
//!
//! **Where:** `<data>/events/<device>/events.jsonl` ([`crate::datahome`]). Machine,
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
//! telemetry derives its OTel logs from it (ADR 0083). It is neither of them (O1). FD-24 adds
//! the cursor subscription and the retention; nothing is pruned yet.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

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

/// How far from the end [`Log::open`] reads to find the last event: far more than one line.
const TAIL: u64 = 64 * 1024;

/// What makes a log's events durable: one `fsync` for every event appended before it, so
/// hooks recorded at once share one (group commit).
///
/// The operating system's ordinary `fsync`. On macOS that hands the data to the drive, not
/// through the drive's own cache (`F_FULLFSYNC`, measured at about 10 ms against 0.1 ms on
/// the operator's machine), which is what SQLite does by default there.
pub struct Durable {
    file: File,
    appended: std::sync::atomic::AtomicU64,
    synced: std::sync::Mutex<u64>,
}

impl Durable {
    /// Returns once every event up to `seq` is durable.
    pub fn through(&self, seq: u64) -> io::Result<()> {
        let mut synced = self
            .synced
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *synced >= seq {
            return Ok(());
        }
        let appended = self.appended.load(std::sync::atomic::Ordering::SeqCst);
        #[cfg(unix)]
        rustix::fs::fsync(&self.file)?;
        #[cfg(not(unix))]
        self.file.sync_data()?;
        *synced = appended.max(seq);
        Ok(())
    }

    /// The last number known to be durable.
    pub fn synced(&self) -> u64 {
        *self
            .synced
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The log of one device, open for appending. Holding it is being the device's one writer.
pub struct Log {
    file: File,
    durable: std::sync::Arc<Durable>,
    device: String,
    next: u64,
    /// How long the file is up to the end of its last whole line.
    good: u64,
    /// Whether a write failed partway, so the file may end in a torn line.
    torn: bool,
    /// A test's way to make the next write fail after this many bytes.
    #[cfg(test)]
    fail_after: Option<usize>,
}

impl Log {
    /// Opens the log in `dir`, the device's own directory under `<data>/events/`, making it if
    /// it is not there.
    ///
    /// **One writer per device** (ADR 0066, ADR 0068): the file is locked for as long as the
    /// `Log` lives, and a second `open` while it is held is refused rather than interleaved.
    /// The numbering carries on from the last event already in the file, so a `seq` is never
    /// used twice.
    pub fn open(dir: &Path, device: &str) -> io::Result<Log> {
        crate::secrets::make_private_dir(dir)?;
        let path = dir.join(FILE);
        let mut options = OpenOptions::new();
        options.create(true).append(true).read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.try_lock().map_err(|why| match why {
            std::fs::TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::WouldBlock,
                "another host is already writing this device's event log",
            ),
            std::fs::TryLockError::Error(why) => why,
        })?;
        let good = cut_a_torn_line(&mut file)?;
        let next = match last_seq(&mut file)? {
            Some(last) => after(last)?,
            None if good == 0 => 1,
            // Lines, and not one of them an event charter wrote: counting from 1 again would
            // reuse every number already handed out, which is the one thing a seq may not do.
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the event log holds no event charter can read, so it is not written to \
                     (starting again from 1 would reuse its numbers)",
                ));
            }
        };
        let durable = std::sync::Arc::new(Durable {
            file: file.try_clone()?,
            appended: std::sync::atomic::AtomicU64::new(next - 1),
            synced: std::sync::Mutex::new(next - 1),
        });
        Ok(Log {
            file,
            durable,
            device: device.to_owned(),
            next,
            good,
            torn: false,
            #[cfg(test)]
            fail_after: None,
        })
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
        if self.torn {
            // A write that failed partway may have left half a line, and the next line would
            // be glued to it: cut back to the last whole line first.
            self.file.set_len(self.good)?;
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
            let _ = self.file.write_all(&line[..after.min(line.len())]);
            self.torn = true;
            return Err(io::Error::other("a write the test made fail"));
        }
        if let Err(why) = self.file.write_all(&line) {
            self.torn = true;
            return Err(why);
        }
        self.good += line.len() as u64;
        self.next = after(seq)?;
        self.durable
            .appended
            .store(seq, std::sync::atomic::Ordering::SeqCst);
        Ok(event)
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

/// The `seq` of the last whole event in `file`, read from its end.
fn last_seq(file: &mut File) -> io::Result<Option<u64>> {
    let len = file.metadata()?.len();
    let from = len.saturating_sub(TAIL);
    file.seek(SeekFrom::Start(from))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    Ok(tail
        .split(|byte| *byte == b'\n')
        .rev()
        .filter_map(|line| serde_json::from_slice::<Event>(line).ok())
        .map(|event| event.seq)
        .next())
}

/// Every event in the log in `dir`, oldest first. A line that is not an event (a write the
/// machine died in the middle of) is skipped.
pub fn read(dir: &Path) -> io::Result<Vec<Event>> {
    let text = match std::fs::read(dir.join(FILE)) {
        Ok(text) => text,
        Err(why) if why.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(why) => return Err(why),
    };
    Ok(text
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect())
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
    /// After it ran: `PostToolUse`.
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
        "PostToolUse" => Some(Phase::Post),
        _ => None,
    }
}

/// A spool drain's kinds (ADR 0075 §4's registry): a gap in a chat's sequence, a line that did
/// not check, a sequence drained, and a refused commit's line drained from a spool.
pub const SPOOL_GAP: &str = "hook.spool.gap";
pub const SPOOL_REJECTED: &str = "hook.spool.rejected";
pub const SPOOL_DRAINED: &str = "hook.spool.drained";
pub const COMMIT_REFUSED: &str = "hook.commit_refused";

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
        if let Some(seq) = spooled {
            body["spooled"] = seq.into();
        }
        let event = self.append(&under, &format!("hook.{}", report.event.word()), body)?;
        match (report.event, &report.agent) {
            (crate::state::Event::SubagentStop, Some(agent)) => {
                let key = (under.chat.clone(), agent.clone());
                self.children.remove(&key);
                if self.stopped.len() >= AGENTS_HELD {
                    self.stopped.clear();
                }
                self.stopped.insert(key);
            }
            // The chat's session is over, and every sub-agent of its run with it.
            (crate::state::Event::SessionEnd, None) => self.end_children(&under.chat),
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
        let mut body = self.tool_body(call);
        if let Some(seq) = spooled {
            body["spooled"] = seq.into();
        }
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
        let number = match &item {
            Drained::Line { chat, .. }
            | Drained::Gap { chat, .. }
            | Drained::Rejected { chat, .. }
            | Drained::Spool { chat, .. } => *chat,
        };
        let known = self.chats.get(&(plane.to_path_buf(), number)).cloned();
        let (kind, mut body) = match item {
            Drained::Line { seq, line, .. } => match (line, &known) {
                (Spooled::Report(report), Some(_)) => {
                    return self.report_spooled(plane, &report, Followed::No, None, Some(seq));
                }
                (Spooled::Tool(call), Some(_)) => {
                    return self.tool_spooled(plane, &call, Instant::now(), Some(seq));
                }
                (Spooled::Report(report), None) => (
                    format!("hook.{}", report.event.word()),
                    serde_json::json!({ "spooled": seq }),
                ),
                (Spooled::Tool(call), None) => {
                    let mut body = self.tool_body(&call);
                    body["spooled"] = seq.into();
                    (tool_kind(&call), body)
                }
                (Spooled::Refused(_), _) => (
                    COMMIT_REFUSED.to_owned(),
                    serde_json::json!({ "spooled": seq }),
                ),
            },
            Drained::Gap { from, to, .. } => (
                SPOOL_GAP.to_owned(),
                serde_json::json!({ "from": from, "to": to }),
            ),
            Drained::Rejected { seq, why, .. } => (
                SPOOL_REJECTED.to_owned(),
                serde_json::json!({ "seq": seq, "why": why }),
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
        // Held before the line is written: a log that refuses the line must not leave the
        // chat's next hook line to mint it a second id.
        self.end_children(chat);
        self.chats.insert(
            (plane.to_path_buf(), number),
            Identity {
                chat: chat.to_owned(),
                run: run.to_owned(),
            },
        );
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

    /// Forgets every sub-agent of `chat`: its run ended, and theirs with it. This is how a
    /// Codex chat's child runs end at all, since charter does not arm Codex's `SubagentStop`.
    fn end_children(&mut self, chat: &str) {
        self.children.retain(|(of, _), _| of != chat);
        self.stopped.retain(|(of, _)| of != chat);
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
        self.log.append(
            Some(&who.chat),
            Some(&who.run),
            None,
            "run.started",
            serde_json::json!({ "cause": cause.word() }),
        )?;
        self.chats.insert(key, who.clone());
        if cause == Began::Clear {
            self.end_children(&who.chat);
        }
        Ok(who)
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
