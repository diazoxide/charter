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
    /// The harness's id for the conversation now ([`crate::reopen::conversation_of`]).
    pub conversation: Option<String>,
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
}

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
fn is_record_name(name: &str) -> bool {
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
            "conversation: {}",
            value(chat.and_then(|c| c.conversation.as_deref()), UNKNOWN)
        ),
        format!("workspace: {}", facts.place.word()),
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
            let title = crate::personas::frontmatter(&text)
                .into_iter()
                .find(|(key, _)| key == "title")
                .map(|(_, value)| value)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| file.clone());
            Some(Listed {
                when: when_of(&file),
                shown: shown(place, &file),
                title,
                file,
            })
        })
        .collect()
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
            conversation: None,
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
    }
}

#[cfg(test)]
#[path = "sessionrecord_tests.rs"]
mod tests;
