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

/// The log of one device, open for appending. Holding it is being the device's one writer.
pub struct Log {
    file: File,
    device: String,
    next: u64,
    /// How long the file is up to the end of its last whole line.
    good: u64,
    /// Whether a write failed partway, so the file may end in a torn line.
    torn: bool,
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
        Ok(Log {
            file,
            device: device.to_owned(),
            next,
            good,
            torn: false,
        })
    }

    /// Appends one event and answers it as written. When this returns `Ok` the whole line has
    /// been handed to the operating system, which a reader sees at once; it is not `fsync`ed,
    /// so a machine that loses power can lose the last events. ADR 0075 §7's "answered once the
    /// event is in the log" is not met yet: the hook has already answered when its line
    /// reaches the host, until FD-30's spool.
    pub fn append(
        &mut self,
        chat: Option<&str>,
        run: Option<&str>,
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
            ulid: ulid::Ulid::new().to_string(),
            chat: chat.map(str::to_owned),
            run: run.map(str::to_owned),
            parent_run: None,
            kind: kind.to_owned(),
            body,
        };
        let mut line = serde_json::to_vec(&event).map_err(io::Error::other)?;
        line.push(b'\n');
        // One `write_all` of one whole line on an append-mode file.
        if let Err(why) = self.file.write_all(&line) {
            self.torn = true;
            return Err(why);
        }
        self.good += line.len() as u64;
        self.next = after(seq)?;
        Ok(event)
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
/// arguments. The key is made once and kept, as `fingerprint.key` is for a plane's `fp:`
/// values, so a digest compares with yesterday's; AU-19 may move it into the keyring.
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
pub enum Followed<'a> {
    /// The chat is in the conversation it was in, or the board refused the report.
    No,
    /// The harness named its conversation for the first time (Codex and opencode do, in the
    /// first turn's hook). The run gains a conversation; it does not start again.
    FirstNamed,
    /// The board followed the chat off this conversation onto another: `/clear`, a new run.
    From(&'a str),
}

/// Who a chat is in the log: its id and its current run's.
#[derive(Debug, Clone)]
struct Identity {
    chat: String,
    run: String,
}

/// The host's side of the log: it turns what the hook channel hears into events, one per hook
/// call, each under the chat and the run it happened in.
///
/// **The host decides runs, never a hook** (ADR 0066, "What changes where"): a hook carries
/// the chat's number and token and nothing more, and the host maps the number to the chat's
/// id and current run in its own memory. A chat it has not seen yet is given an id and its
/// first run, `cause: start`; a chat the board follows onto another conversation begins a run
/// with `cause: clear`.
pub struct Recorder {
    log: Log,
    key: ArgsKey,
    /// By plane and number: a number means nothing outside the plane that dealt it.
    chats: HashMap<(PathBuf, u32), Identity>,
    /// When each tool call's pre hook was heard, by run and the harness's id for the call.
    calls: HashMap<(String, String), Instant>,
}

/// How many tool calls awaiting their post hook are remembered.
const CALLS_HELD: usize = 4096;

impl Recorder {
    /// The recorder for this device, its log under `<data>` and its device id from the
    /// machine store under `config`.
    ///
    /// `<data>/events/<device>/`: by device, as the audit is (ADR 0075 §6), so a backup
    /// restored onto another machine is kept as that device's records and never appended to
    /// (ADR 0069 §5). A `<data>` inside a plane or a git work tree is refused before anything
    /// is made there.
    pub fn open_in(config: &Path, data: &Path) -> io::Result<Recorder> {
        if let Some(why) = crate::datahome::refusal(data) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, why));
        }
        let device = crate::machine::device_id(config)?;
        let dir = data.join(EVENTS).join(&device);
        Ok(Recorder::new(
            Log::open(&dir, &device)?,
            ArgsKey::open(&dir)?,
        ))
    }

    /// [`Recorder::open_in`] for this process: the machine store's and `<data>`'s own homes.
    pub fn open() -> io::Result<Recorder> {
        let config = crate::machine::config_root()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config home"))?;
        let data = crate::datahome::root()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no data home"))?;
        // A fenced build never writes a test run's events into the operator's own data home,
        // for the reason `machine::config_root` gives (charter-app#129).
        crate::fence::hold(crate::fence::Act::Store, &data);
        Recorder::open_in(&config, &data)
    }

    pub fn new(log: Log, key: ArgsKey) -> Recorder {
        Recorder {
            log,
            key,
            chats: HashMap::new(),
            calls: HashMap::new(),
        }
    }

    /// The event for one state hook: `hook.<word>`, after a `run.started` when the report
    /// begins a run.
    pub fn report(
        &mut self,
        plane: &Path,
        report: &crate::hookwire::Report,
        followed: Followed<'_>,
    ) -> io::Result<Event> {
        let who = match followed {
            Followed::From(_) => self.new_run(plane, report.chat, "clear")?,
            Followed::No | Followed::FirstNamed => self.identity(plane, report.chat)?,
        };
        let mut body = serde_json::json!({});
        if report.event == crate::state::Event::SessionStart {
            body["started"] = started(report.detail.started).into();
        }
        self.log.append(
            Some(&who.chat),
            Some(&who.run),
            &format!("hook.{}", report.event.word()),
            body,
        )
    }

    /// The event for one tool hook: `hook.<word>`, with the tool, the keyed digest of its
    /// arguments, what charter answered and how long the hook took. A post hook whose call's
    /// pre hook was heard also says how long the tool ran, from one to the other, as the host
    /// heard them (`now`).
    pub fn tool(
        &mut self,
        plane: &Path,
        call: &crate::hookwire::ToolCall,
        now: Instant,
    ) -> io::Result<Event> {
        let who = self.identity(plane, call.chat)?;
        let mut body = serde_json::json!({
            "decision": call.decision.word(),
            "hook_ms": call.hook_ms,
        });
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
            let key = (who.run.clone(), id.clone());
            if call.tool_hook.starts_with("pretooluse") {
                if self.calls.len() >= CALLS_HELD {
                    // A call whose post hook never came (the harness died, or the tool was
                    // refused) is only a duration lost, so the oldest are simply let go.
                    self.calls.clear();
                }
                self.calls.entry(key).or_insert(now);
            } else if call.tool_hook.starts_with("posttooluse")
                && let Some(began) = self.calls.get(&key)
            {
                let ran = now.saturating_duration_since(*began).as_millis();
                body["tool_ms"] = u64::try_from(ran).unwrap_or(u64::MAX).into();
            }
        }
        self.log.append(
            Some(&who.chat),
            Some(&who.run),
            &format!("hook.{}", call.tool_hook),
            body,
        )
    }

    /// The chat's identity, giving it one and its first run when it has none.
    fn identity(&mut self, plane: &Path, number: u32) -> io::Result<Identity> {
        match self.chats.get(&(plane.to_path_buf(), number)) {
            Some(who) => Ok(who.clone()),
            None => self.new_run(plane, number, "start"),
        }
    }

    /// Begins a run of the chat for `cause`, and says so in the log.
    fn new_run(&mut self, plane: &Path, number: u32, cause: &str) -> io::Result<Identity> {
        let key = (plane.to_path_buf(), number);
        let chat = self
            .chats
            .get(&key)
            .map_or_else(|| ulid::Ulid::new().to_string(), |who| who.chat.clone());
        let who = Identity {
            chat,
            run: ulid::Ulid::new().to_string(),
        };
        self.log.append(
            Some(&who.chat),
            Some(&who.run),
            "run.started",
            serde_json::json!({ "cause": cause }),
        )?;
        self.chats.insert(key, who.clone());
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
