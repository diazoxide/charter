//! Brokered writes (ADR 0067 §2, spec #1330, #1333): the project's own files a chat asks the
//! app to write for it, because its sandbox does not let it write them.
//!
//! A `purlis` command run inside a chat the app started hands one of these to the app over the
//! chat's hook socket ([`crate::hookwire::Ask::Write`]), and the app performs it here, with the
//! same core functions the terminal's command uses. The chat's sandbox does not widen.
//!
//! **A bounded set of typed writes, never "run this command".** Each [`Write`] is one thing a
//! command does, with only what the model writes in it: the text, a title, whether it is
//! shared. **No write names a place.** The workspace and the persona are the app's record of
//! the chat whose token the line carries ([`Asker`]), so a chat writes its own workspace, its
//! own persona's memory and shared memory, and nothing it can name.
//!
//! **Nothing brokered changes what a chat runs under.** A persona's store is held by descriptor,
//! as a workspace's is (V74), and every file a write touches is asked of [`guard`]: never under
//! a name the sandbox denies as later code, never an ignore file, and never a manifest change
//! to `[sandbox]`, `[chat_env]` or `[dispatch]` (the #1341 review; #1439).

use std::path::{Path, PathBuf};

use crate::active::Place;

/// One write a chat asks the app to make. Serialized on the hook socket.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Write {
    /// `purlis persona remember [--shared]` and the `persona_remember` tool: one persistent
    /// memory of the chat's persona, or of shared memory.
    PersonaRemember {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        shared: bool,
    },
    /// `purlis workspace remember` (and `note`): one memory of the chat's workspace.
    WorkspaceRemember {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// `purlis workspace todo "<text>"`: one todo in the chat's workspace.
    Todo { text: String },
}

/// The longest text a brokered write carries, in bytes. Well below what one line of the hook
/// socket holds even when every byte is a control character JSON spells in six, so a write is
/// always read whole and answered, never cut and dropped (#1333). Longer is refused, saying so.
pub const MOST_TEXT_BYTES: usize = 12 * 1024;

impl Write {
    /// Why this write is refused before anything is looked at: a text too long to carry, or a
    /// title that is not one line.
    pub fn check(&self) -> Result<(), String> {
        let (text, title) = match self {
            Self::PersonaRemember { text, title, .. } | Self::WorkspaceRemember { text, title } => {
                (text, title.as_deref())
            }
            Self::Todo { text } => (text, None),
        };
        let bytes = text.len() + title.map_or(0, str::len);
        if bytes > MOST_TEXT_BYTES {
            return Err(format!(
                "this is {bytes} bytes, and what purlis writes for a chat holds at most \
                 {MOST_TEXT_BYTES}: make it shorter, or split it in two"
            ));
        }
        if title.is_some_and(|title| title.chars().any(char::is_control)) {
            return Err(
                "a title is one line, with no newline or other control character in it".to_owned(),
            );
        }
        Ok(())
    }

    /// The write's word in the trace.
    fn word(&self) -> &'static str {
        match self {
            Self::PersonaRemember { .. } => "persona_remember",
            Self::WorkspaceRemember { .. } => "workspace_remember",
            Self::Todo { .. } => "todo",
        }
    }
}

/// The chat a write is made for, as the app records it — never as the request says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asker {
    /// The app's number for the chat.
    pub chat: u32,
    /// Where it works.
    pub place: Place,
    /// Its persona, if it runs as one.
    pub persona: Option<String>,
}

/// What a write wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The workspace or persona it went to (`_shared` for shared memory).
    pub to: String,
    /// The file, project-relative, with `/` between its parts.
    pub path: String,
}

/// Performs `write` for `asker` in the project at `root`, or says why not. Nothing is written
/// when it refuses.
pub fn perform(
    root: &Path,
    asker: &Asker,
    write: &Write,
    now: chrono::NaiveDateTime,
) -> Result<Written, String> {
    if let crate::compat::Compat::ReadOnly(why) = crate::compat::read(root) {
        return Err(format!("nothing was written: {why}"));
    }
    write.check()?;
    let written = match write {
        Write::PersonaRemember {
            text,
            title,
            shared,
        } => {
            let owner = if *shared {
                crate::contain::SHARED_PERSONA
            } else {
                asker.persona.as_deref().ok_or_else(|| {
                    format!(
                        "chat {} runs as no persona, so it has no persona memory of its own: \
                         remember it --shared, or in its workspace",
                        asker.chat
                    )
                })?
            };
            let path = remember_persona(root, owner, text, title.as_deref(), now)?;
            Written {
                to: owner.to_owned(),
                path: shown(root, &path),
            }
        }
        Write::WorkspaceRemember { text, title } => {
            let ws = own_workspace(root, asker)?;
            guard(root, &ws.dir().join("memory"), None)?;
            let path = ws
                .remember_titled(text, title.as_deref(), now)
                .map_err(|e| e.to_string())?;
            Written {
                to: ws.name().to_owned(),
                path: shown(root, &path),
            }
        }
        Write::Todo { text } => {
            let ws = own_workspace(root, asker)?;
            guard(root, &ws.dir().join("todos"), None)?;
            let path = ws.record_todo(text, now).map_err(|e| e.to_string())?;
            Written {
                to: ws.name().to_owned(),
                path: shown(root, &path),
            }
        }
    };
    let chat = asker.chat.to_string();
    crate::trace::record(
        root,
        &chat,
        "brokered",
        &[
            ("chat", chat.as_str()),
            ("write", write.word()),
            ("to", written.to.as_str()),
            ("path", written.path.as_str()),
        ],
        now,
    );
    Ok(written)
}

/// One persistent memory of persona `owner` (or of [`crate::contain::SHARED_PERSONA`]), indexed:
/// the write `purlis persona remember` makes, here or brokered.
pub fn remember_persona(
    root: &Path,
    owner: &str,
    text: &str,
    title: Option<&str>,
    now: chrono::NaiveDateTime,
) -> Result<PathBuf, String> {
    if owner != crate::contain::SHARED_PERSONA
        && let Some(refused) = crate::personas::name_refusal(root, owner)
    {
        return Err(refused);
    }
    let text = crate::memstore::py_strip(text);
    if text.is_empty() {
        return Err("empty memory".to_owned());
    }
    let title = crate::personas::memory_title(text, title);
    let title = if title.is_empty() {
        crate::memstore::title_of(text)
    } else {
        title
    };
    guard(
        root,
        &root.join("personas").join(owner).join("memory"),
        None,
    )?;
    held_persona_write(root, owner, text, &title, now)
}

/// Writes one memory into persona `owner`'s store held by descriptor (V74): `personas/`, the
/// persona and `memory/` opened one at a time without following a link, the file and its index
/// line written relative to that, so a link planted on the way, or swapped in while it is
/// written, carries nothing anywhere else.
#[cfg(unix)]
fn held_persona_write(
    root: &Path,
    owner: &str,
    text: &str,
    title: &str,
    now: chrono::NaiveDateTime,
) -> Result<PathBuf, String> {
    use crate::held::{Make, Store, Who};
    let link = |below: &str| {
        format!(
            "personas/{below} could not be opened without following a link, so nothing was written"
        )
    };
    let personas = Store::hold(
        root,
        &Place::PlaneRoot,
        "personas",
        Make::Store,
        Who::Operator,
    )
    .map_err(String::from)?
    .ok_or_else(|| link(""))?;
    let shared = owner == crate::contain::SHARED_PERSONA;
    let persona = personas
        .sub(owner, shared)
        .map_err(String::from)?
        .ok_or_else(|| format!("no persona '{owner}'"))?;
    let memory = persona
        .sub("memory", true)
        .map_err(String::from)?
        .ok_or_else(|| link(&format!("{owner}/memory")))?;
    let _locked = memory.lock()?;
    let file = crate::memstore::write_in(
        &memory,
        &crate::memstore::NoGates,
        text,
        title,
        false,
        "persistent",
        true,
        now,
    )
    .map_err(|e| e.to_string())?;
    Ok(root.join("personas").join(owner).join("memory").join(file))
}

/// Fail closed: the stores are held through descriptors that refuse links, which only the
/// unix build does yet.
#[cfg(not(unix))]
fn held_persona_write(
    _root: &Path,
    _owner: &str,
    _text: &str,
    _title: &str,
    _now: chrono::NaiveDateTime,
) -> Result<PathBuf, String> {
    Err("persona memory is not written on this platform yet".to_owned())
}

/// The chat's own workspace, which must be there: a brokered write never makes one.
fn own_workspace(root: &Path, asker: &Asker) -> Result<crate::workspaces::Workspace, String> {
    let Place::Workspace(name) = &asker.place else {
        return Err(format!(
            "chat {} works at the project root, not in a workspace, so it has no workspace \
             memory or todos to write",
            asker.chat
        ));
    };
    let ws = crate::workspaces::Plane::open(root)
        .workspace(name)
        .map_err(|e| e.to_string())?;
    if !ws.dir().is_dir() {
        return Err(format!("no workspace '{name}' in this project"));
    }
    Ok(ws)
}

fn shown(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The tables of a manifest a chat runs under, which no brokered write may change: its
/// sandbox, its environment, and the limits on what it may dispatch (#1439).
const RUNS_UNDER: [&str; 3] = ["sandbox", "chat_env", crate::dispatchlimits::TABLE];

/// Whether a brokered write may write `target` in the project at `root`, with `after` the whole
/// text it would leave there when that is known; or the sentence saying why not.
///
/// Refused, with every name compared case-folded (a case-insensitive disk opens `.CLAUDE` as
/// `.claude`), and read through links:
///
/// - anything at or below a name the sandbox denies a chat as later code
///   ([`crate::sandbox::PLANTED`]: git's folder, hook managers', every harness's and editor's
///   project config, shell startup files), at any depth;
/// - any `.gitignore`;
/// - a manifest or local settings file whose `[sandbox]`, `[chat_env]` or `[dispatch]` would
///   change, or whose new text is not known.
///
/// What decides whether the local settings file is this machine's own (the index and the
/// ignore files) is what decides which sandbox a chat runs under, so it is refused with the
/// tables themselves (the #1341 review).
pub fn guard(root: &Path, target: &Path, after: Option<&str>) -> Result<(), String> {
    let lands = crate::contain::resolved(target).unwrap_or_else(|| target.to_path_buf());
    let base = crate::contain::resolved(root).unwrap_or_else(|| root.to_path_buf());
    let inside = lands.strip_prefix(&base).unwrap_or(&lands);
    let said = || shown(&base, &lands);
    let parts: Vec<String> = inside
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let manifests: Vec<String> = [crate::names::PLANE_MANIFEST, crate::names::LOCAL_SETTINGS]
        .iter()
        .flat_map(|n| std::iter::once(n.write).chain(n.reads.iter().copied()))
        .map(str::to_lowercase)
        .collect();
    for (at, part) in parts.iter().enumerate() {
        if part == ".gitignore" {
            return Err(format!(
                "{} is not purlis's to write for a chat: an ignore file decides whether this \
                 machine's settings, and its sandbox, are its own",
                said()
            ));
        }
        if manifests.contains(part) {
            let last = at + 1 == parts.len();
            let before = std::fs::read_to_string(&lands).ok();
            let changes = !last
                || after
                    .is_none_or(|after| changes_what_a_chat_runs_under(before.as_deref(), after));
            if changes {
                return Err(format!(
                    "{} is not purlis's to change for a chat this way: a chat never changes its \
                     sandbox settings ([sandbox]), its environment ([chat_env]) or its dispatch \
                     limits ([dispatch])",
                    said()
                ));
            }
            continue;
        }
        if planted_name(part) {
            return Err(format!(
                "{} is not purlis's to write for a chat: what is under {part} is loaded by a \
                 program run later, outside the sandbox",
                said()
            ));
        }
    }
    Ok(())
}

/// Whether `name`, already lower-cased, is the first name of one the sandbox denies a chat as
/// later code ([`crate::sandbox::PLANTED`]).
fn planted_name(name: &str) -> bool {
    crate::sandbox::PLANTED.iter().any(|planted| {
        planted
            .path
            .split('/')
            .next()
            .is_some_and(|first| first.to_lowercase() == name)
    })
}

/// Whether `after` holds different `[sandbox]`, `[chat_env]` or `[dispatch]` tables from
/// `before` (none: no file). A text that is not TOML changes them: it cannot be read to say it does not.
fn changes_what_a_chat_runs_under(before: Option<&str>, after: &str) -> bool {
    let tables = |text: &str| -> Option<Vec<Option<toml::Value>>> {
        let top = text.parse::<toml::Table>().ok()?;
        Some(RUNS_UNDER.iter().map(|t| top.get(*t).cloned()).collect())
    };
    let was = match before {
        None => Some(vec![None; RUNS_UNDER.len()]),
        Some(text) => tables(text),
    };
    match (was, tables(after)) {
        (Some(was), Some(now)) => was != now,
        _ => true,
    }
}

/// How many brokered writes one chat may make in a window: a chat that writes memory in a loop
/// is stopped, and told so, long before it fills the project.
#[derive(Debug)]
pub struct Rate {
    most: usize,
    per: std::time::Duration,
    seen: std::sync::Mutex<
        std::collections::HashMap<u32, std::collections::VecDeque<std::time::Instant>>,
    >,
}

/// The cap the app holds every chat to: 60 brokered writes a minute. A session record, a few
/// memories and todos a turn are far under it.
pub const WRITES_A_MINUTE: usize = 60;

impl Default for Rate {
    fn default() -> Self {
        Self::new(WRITES_A_MINUTE, std::time::Duration::from_secs(60))
    }
}

impl Rate {
    pub fn new(most: usize, per: std::time::Duration) -> Self {
        Self {
            most,
            per,
            seen: std::sync::Mutex::default(),
        }
    }

    /// Counts one write by `chat` at `now`, or says why it is refused.
    pub fn allow(&self, chat: u32, now: std::time::Instant) -> Result<(), String> {
        let mut seen = self
            .seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Every chat's old writes go, and a chat with none left goes with them, so the map
        // holds only chats that wrote within the window.
        seen.retain(|_, times| {
            while times
                .front()
                .is_some_and(|at| now.saturating_duration_since(*at) >= self.per)
            {
                times.pop_front();
            }
            !times.is_empty()
        });
        let times = seen.entry(chat).or_default();
        if times.len() >= self.most {
            return Err(format!(
                "chat {chat} has asked purlis for {} writes in the last {} seconds, which is its \
                 cap; nothing was written. Wait a little, and write fewer, larger memories",
                self.most,
                self.per.as_secs()
            ));
        }
        times.push_back(now);
        Ok(())
    }

    /// Lets go of what is counted for `chat`, which has closed.
    pub fn forget(&self, chat: u32) {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&chat);
    }

    /// How many chats are counted now.
    pub fn counted(&self) -> usize {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[cfg(test)]
#[path = "brokered_tests.rs"]
mod tests;
