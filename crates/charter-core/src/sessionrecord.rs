//! Session records: what a chat leaves behind when it closes through Smart close (SI-8,
//! ADR 0064).
//!
//! A **session record** is a summary a chat writes of its own session — its goal, what it did,
//! what it decided, what is still open and how to pick it up — and never the transcript. One
//! file per record, in the directory of the place the chat worked:
//!
//! - `workspaces/<ws>/sessions/<YYYYMMDD-HHMMSS>-<slug>.md` for a chat in a workspace;
//! - `sessions/<YYYYMMDD-HHMMSS>-<slug>.md` at the plane's own root for a chat at the plane
//!   root, which is in no workspace (SI-1).
//!
//! **Split between who knows what.** The model writes the title and the five sections, and
//! nothing else: [`check`] holds them to a shape. Everything in the frontmatter is charter's —
//! the time, the chat's number and name, its persona, harness and conversation, the place, and
//! the pieces it touched as git reports them — so no record can claim a chat, a branch or a
//! workspace it did not have. `sessions/index.md` and the `## Sessions` line in `workspace.md`
//! are charter's too, rebuilt from the records themselves every time one is written, so a hand
//! edit to either lasts until the next record and the records stay the one source of truth.
//!
//! `charter session record` is the one writer. The app never writes a record: it waits for the
//! [`crate::hookwire::SessionSaved`] line that command sends once all of this is on disk.

use std::io;
use std::path::{Path, PathBuf};

use crate::active::Place;

pub mod relay;

/// The directory a place's records live in, under the workspace or the plane root.
pub const DIR: &str = "sessions";

/// The index charter keeps beside them.
pub const INDEX: &str = "index.md";

/// A record's body sections: exactly these, each once, in this order.
pub const SECTIONS: [&str; 5] = ["Goal", "Done", "Decisions", "Open", "How to resume"];

/// The longest title, in characters: a title is one line of an index and of a briefing.
pub const MOST_TITLE_CHARS: usize = 120;

/// The largest body, in bytes. A summary of a session, however long the session, is a page or
/// two; anything near this is a transcript, which a record is not.
pub const MOST_BODY_BYTES: usize = 32 * 1024;

/// What `workspace.md`'s `## Sessions` says before the first record.
pub const NONE_YET: &str = "_No session records yet — a chat's Smart close writes one, and charter keeps this line pointing at them._";

/// The frontmatter value of a fact charter does not have.
const UNKNOWN: &str = "unknown";

/// The frontmatter value of a chat that has no persona.
const NO_PERSONA: &str = "none";

/// What charter knows about the chat whose record this is, from the app's own record of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFacts {
    /// The app's number for the chat (`$CHARTER_CHAT`).
    pub number: u32,
    /// The name its tab shows it under.
    pub name: Option<String>,
    /// The harness it runs, by the word the plane calls it.
    pub harness: Option<String>,
    /// The harness profile it started on, by name (the app's record of it).
    pub profile: Option<String>,
    /// The harness's id for the conversation now ([`crate::reopen::conversation_of`]).
    pub conversation: Option<String>,
    /// The directory it runs in, plane-relative — `.` for the plane root — from the app's
    /// record of it; `None` for a chat outside the plane.
    pub cwd: Option<String>,
}

/// One piece a chat worked in, as git reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Touched {
    pub repo: String,
    pub piece: String,
    /// The branch git has the piece on; `None` for a detached one.
    pub branch: Option<String>,
}

/// Everything in a record that is charter's to say, never the model's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// Where the chat worked: a workspace, or the plane root.
    pub place: Place,
    /// When the record is made, on this machine's clock.
    pub at: chrono::NaiveDateTime,
    /// The chat, where the command runs inside one the app started.
    pub chat: Option<ChatFacts>,
    /// The persona the chat runs as, or `None` for none.
    pub persona: Option<String>,
    /// The pieces the chat worked in.
    pub pieces: Vec<Touched>,
}

/// A record to write: the model's title and body, and charter's facts.
pub struct New<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub facts: &'a Facts,
}

/// A record written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    /// The file, absolute.
    pub path: PathBuf,
    /// The file, plane-relative, as it is shown.
    pub shown: String,
    /// What did not follow it: the index or the pointer that could not be rewritten. The
    /// record is on disk either way, and the next record rebuilds both.
    pub warnings: Vec<String>,
}

/// Why no record was written.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    /// The title or the body is not a record's ([`check`]'s sentence).
    #[error("{0}")]
    Shape(String),
    /// The place is a workspace this plane does not have.
    #[error("no workspace '{0}'")]
    NoWorkspace(String),
    /// The disk refused.
    #[error("could not write the session record: {0}")]
    Io(#[from] io::Error),
}

/// One record, as a listing shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The file name.
    pub file: String,
    /// Its title, or the file name for a record whose frontmatter has none.
    pub title: String,
    /// When it was made, `YYYY-MM-DD HH:MM`, from its file name.
    pub when: String,
    /// The file, plane-relative.
    pub shown: String,
    /// The persona the chat ran as, where the record names one this plane could have: charter's
    /// `none` is `None`.
    pub persona: Option<String>,
    /// The harness it ran on, by the word the plane calls it (`claude`, `codex`, `opencode`), or
    /// `None` where the record says `unknown` or something that is not a word.
    pub harness: Option<String>,
    /// The harness's id for its conversation, where the record holds one that is a session
    /// id's shape ([`crate::harness::SessionId::new`]) — so a value a hand edit left is never
    /// handed to a harness as a flag.
    pub conversation: Option<String>,
    /// The harness profile it ran on, where the record names one in a profile name's shape.
    pub profile: Option<String>,
    /// The directory it ran in, plane-relative, where the record names one in that shape
    /// ([`relative_dir_ok`]). Whether it is still a directory inside the record's place is
    /// the reader's to ask ([`crate::sessionresume`]).
    pub cwd: Option<String>,
}

/// A record read back by its plane-relative path ([`open`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The place it is a record of.
    pub place: Place,
    /// What a listing says of it.
    pub listed: Listed,
    /// Its whole text, frontmatter and all.
    pub text: String,
}

impl Opened {
    /// The record after charter's frontmatter: its `# title` and its five sections.
    pub fn body(&self) -> &str {
        after_frontmatter(&self.text)
    }
}

/// The variable a chat started by the Sessions panel's **Resume** carries: the plane-relative
/// path of the record it resumes from, which its session-start briefing quotes (SI-8d). Set by
/// charter alone — a profile may not set a `CHARTER_` name — and read through [`open`], so a
/// value that is not a record's path reads nothing.
pub const RESUMING_ENV: &str = "CHARTER_RESUMING_RECORD";

// ---- the shape ------------------------------------------------------------------------------

/// Whether `title` and `body` make a record, or the sentence saying what is wrong.
///
/// The one judgment every writer asks: the title is one drawable line of at most
/// [`MOST_TITLE_CHARS`]; the body is at most [`MOST_BODY_BYTES`], holds no control or invisible
/// formatting character but a line feed or a tab, and is exactly the five [`SECTIONS`] as
/// `## ` headings, in order, each with something under it and nothing before the first. A
/// heading inside a fenced code block is text. And neither may look like it holds a
/// credential, by the rule `charter save` refuses a memory with ([`crate::secretshape`]) —
/// named by its kind and never by its value.
pub fn check(title: &str, body: &str) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("a session record needs a title (--title)".to_owned());
    }
    if title.chars().any(crate::panel::undrawable) {
        return Err(
            "the title is one line of plain text: it holds a line break or a control character"
                .to_owned(),
        );
    }
    let length = title.chars().count();
    if length > MOST_TITLE_CHARS {
        return Err(format!(
            "the title is {length} characters; a session record's is at most {MOST_TITLE_CHARS}"
        ));
    }
    let body = body.trim();
    if body.len() > MOST_BODY_BYTES {
        return Err(format!(
            "the body is {} bytes; a session record is a summary of at most {MOST_BODY_BYTES} \
             bytes, never the transcript",
            body.len()
        ));
    }
    if body
        .chars()
        .any(|c| c != '\n' && c != '\t' && crate::panel::undrawable(c))
    {
        return Err(
            "the body holds a control or invisible formatting character; a session record is \
             plain Markdown"
                .to_owned(),
        );
    }
    sections(body)?;
    for (what, text) in [("title", title), ("body", body)] {
        if let Some(kind) = crate::secretshape::secret_kind(text) {
            return Err(format!(
                "the {what} looks like it holds a credential ({kind}); a session record is \
                 shared with the workspace, so take the value out — name the vault it lives in \
                 instead"
            ));
        }
    }
    Ok(())
}

/// The body's `## ` sections, held to [`SECTIONS`].
fn sections(body: &str) -> Result<(), String> {
    let expected = || {
        SECTIONS
            .iter()
            .map(|s| format!("## {s}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut found: Vec<(&str, bool)> = Vec::new();
    let mut fenced = false;
    for line in body.split('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
        }
        if !fenced && let Some(name) = line.strip_prefix("## ") {
            found.push((name.trim(), false));
            continue;
        }
        match found.last_mut() {
            Some((_, has_text)) => *has_text |= !line.trim().is_empty(),
            None if !line.trim().is_empty() => {
                return Err(format!(
                    "the body starts with text before its first section; it is exactly the \
                     sections {} (charter writes the title)",
                    expected()
                ));
            }
            None => {}
        }
    }
    for (at, want) in SECTIONS.iter().enumerate() {
        match found.get(at) {
            Some((name, _)) if name == want => {}
            Some((name, _)) if !SECTIONS.contains(name) => {
                return Err(format!(
                    "'## {name}' is not a section of a session record; it has exactly {}",
                    expected()
                ));
            }
            _ if !found.iter().any(|(name, _)| name == want) => {
                return Err(format!(
                    "the body has no '## {want}' section; it needs exactly {}, in that order",
                    expected()
                ));
            }
            _ => {
                return Err(format!(
                    "the sections are out of order; a session record has exactly {}, in that \
                     order",
                    expected()
                ));
            }
        }
    }
    if let Some((name, _)) = found.get(SECTIONS.len()) {
        return Err(format!(
            "'## {name}' is one section too many; a session record has exactly {}",
            expected()
        ));
    }
    if let Some((name, _)) = found.iter().find(|(_, has_text)| !has_text) {
        return Err(format!(
            "the '## {name}' section is empty; say so in words (\"Nothing.\") if there is \
             nothing to say"
        ));
    }
    Ok(())
}

// ---- where ------------------------------------------------------------------------------------

/// The directory `place`'s records live in, or `None` for a workspace name that cannot be one.
pub fn dir(root: &Path, place: &Place) -> Option<PathBuf> {
    match place {
        Place::PlaneRoot => Some(root.join(DIR)),
        Place::Workspace(ws) => crate::contain::workspace_name_ok(ws)
            .then(|| root.join("workspaces").join(ws).join(DIR)),
    }
}

/// The plane-relative spelling of a file in `place`'s directory.
fn shown(place: &Place, file: &str) -> String {
    match place {
        Place::PlaneRoot => format!("{DIR}/{file}"),
        Place::Workspace(ws) => format!("workspaces/{ws}/{DIR}/{file}"),
    }
}

/// Which way a path is about to be used.
#[derive(Clone, Copy)]
enum Verb {
    Read,
    Write,
}

/// Whether charter may use `path`, in `place`'s directory, that way.
///
/// A workspace's records are inside the plane's data directories, and are gated as its memory
/// is ([`crate::contain::writable`]): a link that lands inside them is followed, one that
/// leaves is refused. The plane root's `sessions/` is not one of those directories, so it is
/// gated as charter's own paths are ([`crate::contain::no_link_on_the_way`]): no link anywhere
/// below the plane root, which is stricter and changes no other writer's answer.
fn gated(root: &Path, place: &Place, path: &Path, verb: Verb) -> io::Result<()> {
    let refused = |e: crate::contain::Refused| io::Error::other(e.to_string());
    match (place, verb) {
        (Place::Workspace(_), Verb::Write) => crate::contain::writable(root, path).map_err(refused),
        (Place::Workspace(_), Verb::Read) => crate::contain::readable(root, path).map_err(refused),
        (Place::PlaneRoot, _) => crate::contain::no_link_on_the_way(root, path),
    }
}

/// Whether `path` is a record charter will read: gated as [`gated`] says, a regular file, and
/// within the plane's bound on one file ([`crate::memstore::MAX_BYTES`]).
fn readable_record(root: &Path, place: &Place, path: &Path) -> bool {
    gated(root, place, path, Verb::Read).is_ok()
        && std::fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.len() <= crate::memstore::MAX_BYTES)
}

/// Whether `name` is a record's file name: `YYYYMMDD-HHMMSS-<slug>.md`.
pub(crate) fn is_record_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    let bytes = stem.as_bytes();
    bytes.len() > 16
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'-'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'-'
        && stem[16..]
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `YYYY-MM-DD HH:MM` from a record's file name.
fn when_of(name: &str) -> String {
    format!(
        "{}-{}-{} {}:{}",
        &name[..4],
        &name[4..6],
        &name[6..8],
        &name[9..11],
        &name[11..13]
    )
}

// ---- writing ----------------------------------------------------------------------------------

/// Writes `new` as a record of its place, then rebuilds that place's index and, for a
/// workspace, the `## Sessions` line in its `workspace.md`.
///
/// Nothing is written unless [`check`] passes and the place is one this plane has. The record
/// is replaced into place whole ([`crate::rewrite::replace`]), and its name is chosen and taken
/// under a lock on the directory, so two records made in the same second are two files. The
/// index and the pointer follow it; either failing is a warning on the [`Recorded`], not a lost
/// record, because both are rebuilt from the records by the next one.
pub fn record(root: &Path, new: &New) -> Result<Recorded, Refused> {
    check(new.title, new.body).map_err(Refused::Shape)?;
    let place = &new.facts.place;
    let Some(dir) = dir(root, place) else {
        return Err(Refused::NoWorkspace(place.word().to_owned()));
    };
    if let Place::Workspace(ws) = place
        && !crate::wscmd::workspace_dir_exists(root, ws)
    {
        return Err(Refused::NoWorkspace(ws.clone()));
    }
    gated(root, place, &dir, Verb::Write)?;
    std::fs::create_dir_all(&dir)?;
    let text = render(new);
    let stamp = new.facts.at.format("%Y%m%d-%H%M%S").to_string();
    let slug = crate::memstore::slug(new.title.trim());
    let path = {
        let _held = crate::rewrite::Lock::on(&dir);
        let mut n = 1;
        let path = loop {
            let name = if n == 1 {
                format!("{stamp}-{slug}.md")
            } else {
                format!("{stamp}-{slug}-{n}.md")
            };
            let candidate = dir.join(name);
            if candidate.symlink_metadata().is_err() {
                break candidate;
            }
            n += 1;
        };
        crate::rewrite::replace(&dir, &path, text.as_bytes(), crate::rewrite::Mode::Kept)?;
        path
    };
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut warnings = Vec::new();
    if let Err(e) = write_index(root, place) {
        warnings.push(format!(
            "the record is written, and {} could not be rebuilt: {e}",
            shown(place, INDEX)
        ));
    }
    if let Place::Workspace(ws) = place
        && let Err(e) = point(root, ws)
    {
        warnings.push(format!(
            "the record is written, and workspaces/{ws}/workspace.md's ## Sessions could not \
             be updated: {e}"
        ));
    }
    Ok(Recorded {
        path,
        shown: shown(place, &file),
        warnings,
    })
}

/// The whole file: charter's frontmatter, the title as a heading, then the body.
fn render(new: &New) -> String {
    let facts = new.facts;
    let value = |v: Option<&str>, absent: &str| -> String {
        v.map(|v| crate::briefing::one_line(v, 200))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| absent.to_owned())
    };
    let chat = facts.chat.as_ref();
    let mut lines = vec![
        "---".to_owned(),
        format!("title: {}", new.title.trim()),
        format!("date: {}", facts.at.format("%Y-%m-%d %H:%M:%S")),
        format!(
            "chat: {}",
            chat.map_or_else(|| UNKNOWN.to_owned(), |c| c.number.to_string())
        ),
        format!(
            "chat-name: {}",
            value(chat.and_then(|c| c.name.as_deref()), UNKNOWN)
        ),
        format!("persona: {}", value(facts.persona.as_deref(), NO_PERSONA)),
        format!(
            "harness: {}",
            value(chat.and_then(|c| c.harness.as_deref()), UNKNOWN)
        ),
        format!(
            "profile: {}",
            value(chat.and_then(|c| c.profile.as_deref()), UNKNOWN)
        ),
        format!(
            "conversation: {}",
            value(chat.and_then(|c| c.conversation.as_deref()), UNKNOWN)
        ),
        format!("workspace: {}", facts.place.word()),
        format!(
            "cwd: {}",
            value(chat.and_then(|c| c.cwd.as_deref()), UNKNOWN)
        ),
    ];
    for piece in &facts.pieces {
        lines.push(format!(
            "piece: {}/{} @ {}",
            piece.repo,
            piece.piece,
            piece.branch.as_deref().unwrap_or("(detached)")
        ));
    }
    lines.push("---".to_owned());
    format!(
        "{}\n\n# {}\n\n{}\n",
        lines.join("\n"),
        new.title.trim(),
        new.body.trim()
    )
}

// ---- the index and the pointer ------------------------------------------------------------------

/// Every record of `place`, newest first. A file that is not named as a record, is a link, or
/// is past the plane's bound on one file is not one, and is left out.
pub fn list(root: &Path, place: &Place) -> Vec<Listed> {
    let Some(dir) = dir(root, place) else {
        return Vec::new();
    };
    if gated(root, place, &dir, Verb::Read).is_err() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_record_name(name))
        .collect();
    // The name starts with the time, so its order is the records' order; newest first.
    names.sort_unstable_by(|a, b| b.cmp(a));
    names
        .into_iter()
        .filter_map(|file| {
            let path = dir.join(&file);
            if !readable_record(root, place, &path) {
                return None;
            }
            let text = crate::memstore::read_text(&path)?;
            Some(listed(place, file, &text))
        })
        .collect()
}

/// What a listing says of the record `file` of `place`, whose text is `text`.
///
/// **Every value is read back as untrusted**: the file is on disk, a hand edit or a pull of a
/// LIVE workspace can change it, and only the title is ever drawn as it is. A persona is kept
/// only as a name charter would read, a harness only as a short word, and a conversation only
/// in a session id's shape.
pub(crate) fn listed(place: &Place, file: String, text: &str) -> Listed {
    let pairs = crate::personas::frontmatter(text);
    let value = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty() && v != UNKNOWN)
    };
    let title = value("title").unwrap_or_else(|| file.clone());
    let persona = value("persona").filter(|v| v != NO_PERSONA && crate::personas::valid_name(v));
    let harness = value("harness").filter(|v| {
        v.len() <= 40
            && v.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    });
    let conversation =
        value("conversation").filter(|v| crate::harness::SessionId::new(v.as_str()).is_ok());
    let profile = value("profile").filter(|v| crate::profiles::name_ok(v));
    let cwd = value("cwd").filter(|v| relative_dir_ok(v));
    Listed {
        when: when_of(&file),
        shown: shown(place, &file),
        title,
        file,
        persona,
        harness,
        conversation,
        profile,
        cwd,
    }
}

/// The longest `cwd:` charter reads back.
const MOST_CWD_BYTES: usize = 1024;

/// Whether `dir` is a directory below the plane root spelled as charter writes one: `.` for
/// the plane root itself, else plain `/`-separated components — never absolute, no `..`, no
/// `.` inside it, no empty component, no backslash and nothing a line cannot draw.
///
/// Only the spelling. Whether it is still a directory, and where it lands once links are
/// followed, is asked where it is used ([`crate::sessionresume`]).
pub fn relative_dir_ok(dir: &str) -> bool {
    if dir == "." {
        return true;
    }
    !dir.is_empty()
        && dir.len() <= MOST_CWD_BYTES
        && !dir.contains('\\')
        && !dir.chars().any(crate::panel::undrawable)
        && dir
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// `dir`, absolute, as a directory below `root` ([`relative_dir_ok`]'s spelling), or `None`
/// where it is not below it. Lexically first, then with both ends resolved, because the app
/// may hold the plane by a path a link leads to (`/tmp` and `/private/tmp` on macOS).
fn plane_relative(root: &Path, dir: &Path) -> Option<String> {
    let below = |root: &Path, dir: &Path| -> Option<String> {
        let rest = dir.strip_prefix(root).ok()?;
        let parts: Option<Vec<&str>> = rest
            .components()
            .map(|c| match c {
                std::path::Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect();
        let parts = parts?;
        let shown = if parts.is_empty() {
            ".".to_owned()
        } else {
            parts.join("/")
        };
        relative_dir_ok(&shown).then_some(shown)
    };
    if !dir.is_absolute() {
        return None;
    }
    below(root, dir).or_else(|| {
        below(
            &crate::contain::resolved(root)?,
            &crate::contain::resolved(dir)?,
        )
    })
}

/// The text after a record's frontmatter, or the whole text where it has none.
fn after_frontmatter(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("---\n") else {
        return text;
    };
    match rest.find("\n---\n") {
        Some(end) => rest[end + 5..].trim_start_matches('\n'),
        None => text,
    }
}

/// The newest record of `place`, or `None` where it has none.
pub fn latest(root: &Path, place: &Place) -> Option<Listed> {
    list(root, place).into_iter().next()
}

/// A title as the text of a Markdown link: its brackets escaped.
fn link_text(title: &str) -> String {
    title.replace('[', "\\[").replace(']', "\\]")
}

/// The index's whole text for `records` of `place`.
pub fn index_text(place: &Place, records: &[Listed]) -> String {
    let heading = match place {
        Place::PlaneRoot => "# Sessions — plane root".to_owned(),
        Place::Workspace(ws) => format!("# Sessions — workspace `{ws}`"),
    };
    let mut text = format!(
        "{heading}\n\nOne file per session record, newest first. charter rebuilds this index from \
         the records each time one is written (`charter session record`), so an edit here does \
         not last.\n\n"
    );
    for record in records {
        text.push_str(&format!(
            "- {} · [{}]({})\n",
            record.when,
            link_text(&record.title),
            record.file
        ));
    }
    text
}

/// Rebuilds `place`'s `sessions/index.md` from its records.
fn write_index(root: &Path, place: &Place) -> io::Result<()> {
    let Some(dir) = dir(root, place) else {
        return Ok(());
    };
    let text = index_text(place, &list(root, place));
    let path = dir.join(INDEX);
    gated(root, place, &path, Verb::Write)?;
    crate::rewrite::update(root, &path, |now| {
        Ok((now != Some(text.as_str())).then(|| text.clone()))
    })
    .map(|_| ())
}

/// What `workspace.md`'s `## Sessions` says for `records`.
pub fn pointer(records: &[Listed]) -> String {
    let Some(newest) = records.first() else {
        return NONE_YET.to_owned();
    };
    let count = records.len();
    format!(
        "{count} session record{} — the latest is [{}]({DIR}/{}) ({}); all of them, newest \
         first, in [{DIR}/{INDEX}]({DIR}/{INDEX}).",
        if count == 1 { "" } else { "s" },
        link_text(&newest.title),
        newest.file,
        newest.when
    )
}

/// Sets `ws`'s `workspace.md` `## Sessions` line to what its records say, creating the charter
/// from its template first when it is missing. Writes nothing when the line already says it.
///
/// charter's line and nothing else: the other sections are the operator's and the skill's.
pub fn point(root: &Path, ws: &str) -> io::Result<()> {
    let workspace = crate::workspaces::Plane::open(root)
        .workspace(ws)
        .map_err(|e| io::Error::other(e.to_string()))?;
    workspace.scaffold_charter()?;
    let path = workspace.dir().join("workspace.md");
    crate::contain::writable(root, &path).map_err(|e| io::Error::other(e.to_string()))?;
    let body = pointer(&list(root, &Place::Workspace(ws.to_owned())));
    crate::rewrite::update(root, &path, |now| {
        let now = now.unwrap_or_default();
        let next = crate::mdsection::replace(now, "Sessions", &body);
        Ok((next != now).then_some(next))
    })
    .map(|_| ())
}

// ---- reading one back ---------------------------------------------------------------------------

/// The text of the record `file` of `place`. `file` is a record's file name and nothing else:
/// no directory, and not the index.
pub fn show(root: &Path, place: &Place, file: &str) -> Result<String, String> {
    if !is_record_name(file) {
        return Err(format!(
            "'{file}' is not a session record's file name (YYYYMMDD-HHMMSS-<title>.md)"
        ));
    }
    let Some(dir) = dir(root, place) else {
        return Err(format!("no workspace '{}'", place.word()));
    };
    let path = dir.join(file);
    if !readable_record(root, place, &path) {
        return Err(format!("no session record {}", shown(place, file)));
    }
    crate::memstore::read_text(&path).ok_or_else(|| format!("{} is not text", shown(place, file)))
}

/// The place and file name a plane-relative record path names: `sessions/<file>` for the plane
/// root's, `workspaces/<ws>/sessions/<file>` for a workspace's — and nothing else.
///
/// **The one reading of a record's path**, for the command line, the window and the briefing
/// alike. Every other spelling is refused, never normalised: no `..`, no `.`, no leading or
/// trailing `/`, no backslash, no directory below `sessions/`, the index, and a workspace name
/// charter would not read ([`crate::contain::workspace_name_ok`]). A file name has to be a
/// record's ([`is_record_name`]), which holds no `/` and no `..`.
pub fn locate(path: &str) -> Result<(Place, String), String> {
    let refused = || {
        format!(
            "'{}' is not a session record's path (sessions/<file> or \
             workspaces/<ws>/sessions/<file>)",
            crate::shown::short(path)
        )
    };
    if path.contains('\\') {
        return Err(refused());
    }
    let parts: Vec<&str> = path.split('/').collect();
    let (place, file) = match parts.as_slice() {
        [DIR, file] => (Place::PlaneRoot, *file),
        ["workspaces", ws, DIR, file] if crate::contain::workspace_name_ok(ws) => {
            (Place::Workspace((*ws).to_owned()), *file)
        }
        _ => return Err(refused()),
    };
    if !is_record_name(file) {
        return Err(refused());
    }
    Ok((place, file.to_owned()))
}

/// The record at the plane-relative `path` ([`locate`]), read as [`show`] reads one: gated, a
/// regular file within the plane's bound, and text.
pub fn open(root: &Path, path: &str) -> Result<Opened, String> {
    let (place, file) = locate(path)?;
    let text = show(root, &place, &file)?;
    Ok(Opened {
        listed: listed(&place, file, &text),
        place,
        text,
    })
}

/// The record a saved-record line names (`SessionSaved::session_saved`), as a listing says it —
/// or `None` for a path that is not one of this plane's records.
///
/// The line carries the path the command wrote, and the app may know the plane by another
/// spelling of the same directory (a `/tmp` that is `/private/tmp`), so a path that is not below
/// `root` as written is asked again with both resolved. Below the root it is read only as
/// [`open`] reads a record's plane-relative path, which refuses every other spelling.
pub fn saved(root: &Path, path: &Path) -> Option<Listed> {
    let below = match path.strip_prefix(root) {
        Ok(below) => below.to_path_buf(),
        Err(_) => {
            let (root, path) = (root.canonicalize().ok()?, path.canonicalize().ok()?);
            path.strip_prefix(&root).ok()?.to_path_buf()
        }
    };
    let parts = below
        .components()
        .map(|part| match part {
            std::path::Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<&str>>>()?;
    open(root, &parts.join("/"))
        .ok()
        .map(|opened| opened.listed)
}

// ---- the chat, from the app's record of it -------------------------------------------------------

/// What the app's record of the open chats says about chat `number`: the name its tab shows, the
/// harness it runs and its conversation. A chat the record does not name is its number alone.
pub fn chat_facts(root: &Path, number: u32) -> ChatFacts {
    let found = crate::reopen::read_or_refusal(root)
        .ok()
        .and_then(|record| record.chats.into_iter().find(|c| c.number == Some(number)));
    let Some(chat) = found else {
        return ChatFacts {
            number,
            name: None,
            harness: None,
            profile: None,
            conversation: None,
            cwd: None,
        };
    };
    let harness = chat.harness().map(|h| h.name().to_owned());
    ChatFacts {
        number,
        name: Some(crate::reopen::shown_name(&chat, harness.as_deref())),
        harness,
        // The one answer to "which conversation is this chat in now" (SI-8a): the app keeps
        // `resume` current from the chat's own hook reports.
        conversation: crate::reopen::conversation_of(root, number)
            .ok()
            .flatten()
            .map(|id| id.as_str().to_owned()),
        profile: chat.profile.clone().filter(|p| crate::profiles::name_ok(p)),
        cwd: chat
            .cwd
            .as_deref()
            .and_then(|dir| plane_relative(root, dir)),
    }
}

#[cfg(test)]
#[path = "sessionrecord_tests.rs"]
mod tests;
