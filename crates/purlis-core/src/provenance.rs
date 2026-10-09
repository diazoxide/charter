//! Provenance trailers on the commits an agent run makes (GL-8, ruling V67).
//!
//! **`git log` is the audit trail.** A commit an agent made carries, as git trailers that
//! `git interpret-trailers` parses:
//!
//! ```text
//! Assisted-by: <harness>:<model>
//! Purlis-Chat: <chat ULID>
//! Purlis-Persona: <persona>
//! Purlis-Change: <change slug>
//! ```
//!
//! Commits made before the rename carry `Charter-Chat`, `Charter-Persona` and `Charter-Change`.
//! History is never rewritten, so those keys are recognised forever ([`crate::names`], V93j):
//! a message that already carries one with the same value is not given its purlis twin.
//!
//! `Assisted-by` is the Linux kernel's original form (`Documentation/process/coding-assistants.rst`,
//! 78d979db6cef). The kernel later cut it to `Assisted-by: LLM` (816d9992d9ed), and a project
//! that follows that policy says so per repo ([`Form::Llm`]). The chat is named by its ULID, never
//! its number (ADR 0066: the number never leaves the clone).
//!
//! **They are a claim, not proof.** The agent writes its own message and can type, change or
//! skip any of these lines, so nothing may treat them as a security signal.
//!
//! **What is not known is left out**, trailer by trailer, and a model charter has not been told
//! leaves `Assisted-by: <harness>` with no colon. The model is the one the chat's harness last
//! reported in its `SessionStart`, as the app records it ([`crate::reopen::Chat::model`]). A
//! value that could not stand on one trailer line is not known either. **No harness, no
//! trailers**: a commit no agent run made is a human's, and a human's commit carries nothing of
//! charter's. There is no on-behalf-of trailer: the human is the commit's author.
//!
//! Two places add them: the project save `charter save` makes from inside a chat
//! ([`crate::planegit`]), and every commit an agent makes itself in a chat whose git runs
//! charter's hooks, through `commit-msg` ([`crate::githooks`]).

use std::path::{Path, PathBuf};

use crate::worktree::git;

/// Where a chat's environment names its project.
const ROOT_ENV: &str = "PURLIS_ROOT";

/// The trailer naming who assisted: the harness and its model, or the kernel's bare `LLM`.
pub const ASSISTED_BY: &str = "Assisted-by";
/// The chat, by its ULID.
pub const CHAT: &str = crate::names::TRAILER_CHAT.write;
/// The persona the chat adopted.
pub const PERSONA: &str = crate::names::TRAILER_PERSONA.write;
/// The cross-repo change the commit's branch is a member of (ADR 0060).
pub const CHANGE: &str = crate::names::TRAILER_CHANGE.write;

/// The trailer keys charter has written, each under every spelling it has had.
const RENAMED: [crate::names::Name; 3] = [
    crate::names::TRAILER_CHAT,
    crate::names::TRAILER_PERSONA,
    crate::names::TRAILER_CHANGE,
];

/// Every spelling of the trailer line `line`: itself, and for a renamed key the same value under
/// each of the key's other names.
fn spellings_of(line: &str) -> Vec<String> {
    let Some((key, value)) = line.split_once(": ") else {
        return vec![line.to_owned()];
    };
    match RENAMED.iter().find(|name| name.recognises(key)) {
        Some(name) => name.spellings().map(|k| format!("{k}: {value}")).collect(),
        None => vec![line.to_owned()],
    }
}

/// The value [`Form::Llm`] writes.
pub const BARE: &str = "LLM";

/// The longest value a trailer carries. A ULID is 26, a model id rarely 60.
const MOST: usize = 100;

/// git's scissors line after the comment string: everything below it is dropped from the
/// message (`git commit --verbose`, `--cleanup=scissors`).
const SCISSORS: &str = " ------------------------ >8 ------------------------";

/// Who made a commit, as far as charter knows it. Every field is `None` when it does not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provenance {
    /// The harness, by V67's word for it ([`harness_word`]).
    pub harness: Option<String>,
    /// The model, as its provider names it.
    pub model: Option<String>,
    /// The chat's ULID.
    pub chat: Option<String>,
    pub persona: Option<String>,
    /// The change slug.
    pub change: Option<String>,
}

/// How `Assisted-by` is spelled: `[plane] assisted_by` and `[repos.<name>] assisted_by`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Form {
    /// `Assisted-by: <harness>:<model>`, the default.
    #[default]
    Full,
    /// `Assisted-by: LLM`, the kernel's policy since 816d9992d9ed.
    Llm,
}

impl Form {
    /// The setting's word for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Llm => "llm",
        }
    }

    /// The form a setting's word names, in any case (`llm`, `LLM`), or `None` for a word
    /// that names none.
    pub fn parse(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "full" => Some(Self::Full),
            "llm" => Some(Self::Llm),
            _ => None,
        }
    }
}

impl Provenance {
    /// [`Self::of`] chat `number` of the project at `plane`, for the tests only: it knows
    /// nothing of which process is asking, so nothing but [`Self::in_chat`], the one gate a
    /// stamp passes, may read a chat's provenance (V82, #1018).
    #[cfg(test)]
    fn of_chat(plane: &Path, number: u32) -> Option<Self> {
        Self::of(plane, &chat_numbered(plane, number)?)
    }

    /// `chat` of the project at `plane`, as the app's record holds it: its ULID, its harness,
    /// the model its harness reported, and its persona. `None` — no agent run — where the chat
    /// was started on no harness profile this project has (a shell tab is the operator's). The
    /// harness is the profile's declared kind, never guessed from the program the chat runs.
    /// Read the way `charter session record` reads it (ADR 0066: the id is asked for by number,
    /// never carried in the environment).
    fn of(plane: &Path, chat: &crate::reopen::Chat) -> Option<Self> {
        let profiles = crate::profiles::derive(plane);
        let harness = harness_word(&profiles.get(chat.profile.as_deref()?)?.kind)?;
        Some(Self {
            harness: Some(harness.to_owned()),
            // As the chat's harness last reported it (ADR 0087 §6, #1021).
            model: chat.model.clone(),
            chat: chat.identity.id.clone(),
            persona: chat.persona.clone(),
            change: None,
        })
    }

    /// The chat this process runs inside, by the number the app sets in every chat's
    /// environment (`$CHARTER_SESSION_ID`), asked of the record. `None` outside a chat (a
    /// terminal of the operator's is not an agent run), and where the record does not hold the
    /// chat or cannot be read.
    ///
    /// **And `None` outside the chat's harness** (V82, #1018). The environment is inherited by
    /// whatever the chat starts — an editor opened from it keeps `$CHARTER_SESSION_ID` for the
    /// rest of its life, and the operator's commits there are not the agent's (V67). So this
    /// process must run below the program the record says the app started for that chat
    /// ([`crate::reopen::Chat::pid`], [`crate::process::descends_from`]); a chat recorded with
    /// no running program has no agent run to claim.
    pub fn in_chat(plane: &Path, env: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let number = env(crate::active::SESSION_ID_ENV)?.trim().parse().ok()?;
        let chat = chat_numbered(plane, number)?;
        if !chat.pid.is_some_and(crate::process::descends_from) {
            return None;
        }
        Self::of(plane, &chat)
    }

    /// Chat `number` of the project at `plane`, for a commit **the app makes for it** (#1055):
    /// the app took the ask from a process inside that chat, on the chat's own token
    /// (`hookwire::ChatTokens::admission`), which is the gate [`Self::in_chat`]'s walk is for a
    /// commit the chat makes itself. Nothing else may read a chat's provenance by number.
    pub fn of_the_chat_the_app_commits_for(plane: &Path, number: u32) -> Option<Self> {
        Self::of(plane, &chat_numbered(plane, number)?)
    }

    /// The trailer lines, `Key: value`, in V67's order, each once.
    pub fn trailers(&self, form: Form) -> Vec<String> {
        let Some(harness) = known(&self.harness) else {
            return Vec::new();
        };
        let assisted = match (form, known(&self.model)) {
            (Form::Llm, _) => BARE.to_owned(),
            // A model too long to stand beside its harness on one value is not known
            // either: the bare harness is written, never a value its reader would drop.
            (Form::Full, Some(model)) if harness.len() + 1 + model.len() <= MOST => {
                format!("{harness}:{model}")
            }
            (Form::Full, _) => harness.to_owned(),
        };
        let mut out = vec![format!("{ASSISTED_BY}: {assisted}")];
        for (key, value) in [
            (CHAT, &self.chat),
            (PERSONA, &self.persona),
            (CHANGE, &self.change),
        ] {
            if let Some(value) = known(value) {
                out.push(format!("{key}: {value}"));
            }
        }
        out
    }
}

/// Chat `number` as the project's record holds it, or `None` where it does not or cannot be
/// read.
fn chat_numbered(plane: &Path, number: u32) -> Option<crate::reopen::Chat> {
    crate::reopen::read_or_refusal(plane)
        .ok()?
        .chats
        .into_iter()
        .find(|chat| chat.number == Some(number))
}

/// V67's word for a profile's harness kind (ruling V67(b)). A small table of its own, so the
/// words written into people's history never move with charter's internal names.
pub fn harness_word(kind: &str) -> Option<&'static str> {
    match kind {
        "claude" => Some("claude-code"),
        "codex" => Some("codex"),
        "opencode" => Some("opencode"),
        _ => None,
    }
}

/// `message` with each of `trailers` that is not already one of its lines appended, and
/// nothing else changed: every byte the message had stays where it was.
///
/// **No git runs** (the PR #1012 review): `git interpret-trailers` reformats an agent's own
/// trailer lines, takes a `---` line for a patch divider, and reads trailer configuration that
/// can run a program. This is the one rule both places use:
///
/// - the lines go after the message's last line of text; when that line's paragraph is
///   already a trailer block (every line `Token: value`, or a continuation, and not the
///   subject's paragraph) they join it, and otherwise a blank line comes first;
/// - with `comment` (an edited message, `commit-msg`'s file), lines starting with it are not
///   text, so ours go before a closing comment block, and nothing below git's scissors line is
///   looked at or changed;
/// - a message with no text at all is left as it is, for git to refuse as empty.
pub fn append(message: &str, trailers: &[String], comment: Option<&str>) -> String {
    let scissors = comment.map(|c| format!("{c}{SCISSORS}"));
    let cut = scissors
        .as_deref()
        .and_then(|line| {
            let mut at = 0;
            for piece in message.split_inclusive('\n') {
                if piece.trim_end_matches(['\n', '\r']) == line {
                    return Some(at);
                }
                at += piece.len();
            }
            None
        })
        .unwrap_or(message.len());
    let (body, tail) = message.split_at(cut);
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let text = |line: &str| {
        let line = line.trim_end_matches(['\n', '\r']);
        !line.trim().is_empty() && !comment.is_some_and(|c| line.starts_with(c))
    };
    let Some(last) = lines.iter().rposition(|l| text(l)) else {
        return message.to_owned();
    };
    let had: std::collections::HashSet<&str> = lines
        .iter()
        .filter(|l| text(l))
        .map(|l| l.trim_end_matches(['\n', '\r']).trim_end())
        .collect();
    // A trailer the message has under an older key — `Charter-Chat` for `Purlis-Chat`, on an
    // amend of a commit made before the rename — is had: the same claim is never made twice.
    let added: Vec<&str> = trailers
        .iter()
        .map(String::as_str)
        .filter(|t| !spellings_of(t).iter().any(|s| had.contains(s.as_str())))
        .collect();
    if added.is_empty() {
        return message.to_owned();
    }
    // The last paragraph's lines of text, and whether one before it exists (the subject's).
    let mut paragraph: Vec<&str> = Vec::new();
    let mut earlier = false;
    for (i, line) in lines[..=last].iter().enumerate().rev() {
        if text(line) {
            paragraph.push(line.trim_end_matches(['\n', '\r']));
        } else if line.trim().is_empty() {
            earlier = lines[..i].iter().any(|l| text(l));
            break;
        }
    }
    paragraph.reverse();
    let block = earlier
        && paragraph.first().is_some_and(|l| trailer_line(l))
        && paragraph
            .iter()
            .all(|l| trailer_line(l) || l.starts_with([' ', '\t']));
    // The message's own line ending (#1021): a CRLF message keeps CRLF on the lines added to
    // it, which shows under `--cleanup=verbatim`, where git strips no stray `\r`.
    let eol = lines[..=last]
        .iter()
        .rev()
        .find(|l| l.ends_with('\n'))
        .map_or("\n", |l| if l.ends_with("\r\n") { "\r\n" } else { "\n" });
    let mut out = String::with_capacity(message.len() + 128);
    for line in &lines[..=last] {
        out.push_str(line);
    }
    if !out.ends_with('\n') {
        out.push_str(eol);
    }
    if !block {
        out.push_str(eol);
    }
    for line in added {
        out.push_str(line);
        out.push_str(eol);
    }
    for line in &lines[last + 1..] {
        out.push_str(line);
    }
    out.push_str(tail);
    out
}

/// `Token: value` the way git reads a trailer: a token of letters, digits and `-`, then `:`.
fn trailer_line(line: &str) -> bool {
    let Some((token, _)) = line.split_once(':') else {
        return false;
    };
    let token = token.trim_end();
    token.starts_with(|c: char| c.is_ascii_alphanumeric())
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The work tree at `top`'s workspace and repo: a clone `workspaces/<ws>/<repo>` or a piece of
/// one.
fn repo_at(plane: &Path, top: &Path) -> Option<(String, String)> {
    crate::chatpiece::clone_at(plane, top)
        .or_else(|| crate::worktree::locate(plane, top).map(|piece| (piece.workspace, piece.repo)))
}

/// How `Assisted-by` is spelled for a commit in the work tree at `top`: its repo's
/// `[repos.<name>] assisted_by`, the project's `[plane] assisted_by` for the project itself, and
/// the full form for a repo the project does not hold.
fn form_at(plane: &Path, top: &Path) -> Form {
    let settings = crate::planesave::Settings::read(plane);
    if let Some((_, repo)) = repo_at(plane, top) {
        return settings.repo(&repo).assisted_by.value;
    }
    let same = |a: &Path, b: &Path| a.canonicalize().ok() == b.canonicalize().ok();
    if same(plane, top) {
        settings.plane.assisted_by.value
    } else {
        Form::Full
    }
}

/// The trailers of a commit the app makes for chat `number` in the work tree at `top`
/// (#1055, ADR 0074): what [`stamp`] would have added had the chat's own git made it. Empty
/// where the record holds no such chat on a harness.
pub fn trailers_for(plane: &Path, top: &Path, number: u32) -> Vec<String> {
    let Some(mut provenance) = Provenance::of_the_chat_the_app_commits_for(plane, number) else {
        return Vec::new();
    };
    provenance.change = change_of(plane, top);
    provenance.trailers(form_at(plane, top))
}

/// `commit-msg`'s work in a chat: stamp the message file git hands the hook with the
/// provenance of the chat this process runs inside, for a commit in the work tree at `top`.
///
/// Best effort, and silent: outside a chat (`env` names no chat on a harness, or no project),
/// or where the file cannot be read or written, the message is left exactly as it was written.
/// A commit is never refused for its trailers. The lines are added by [`append`], with the
/// repository's comment string where an editor ran ([`comment_for`]), so a `--verbose` diff
/// below the scissors stays below it.
pub fn stamp(message: &Path, top: &Path, env: &dyn Fn(&str) -> Option<String>) {
    let Some(plane) = env(ROOT_ENV).filter(|p| !p.is_empty()).map(PathBuf::from) else {
        return;
    };
    let Some(mut provenance) = Provenance::in_chat(&plane, env) else {
        return;
    };
    provenance.change = change_of(&plane, top);
    let trailers = provenance.trailers(form_at(&plane, top));
    if trailers.is_empty() {
        return;
    }
    let Ok(text) = std::fs::read_to_string(message) else {
        return;
    };
    let stamped = append(&text, &trailers, comment_for(top, env).as_deref());
    if stamped != text {
        let _ = std::fs::write(message, stamped);
    }
}

/// What starts a comment line in the message `commit-msg` is handed, or `None` where git keeps
/// such lines (#1021). git tells its hook that no editor ran by setting `GIT_EDITOR` to `:` in
/// the hook's environment: a `-m` or `-F` message, whose default cleanup (`whitespace`) keeps a
/// line starting with `#`. Read as text there, a last line starting with `#` gets the trailers
/// after it, not before it. A `--cleanup=strip` beside `-m` strips that line afterwards, which
/// leaves the trailers last all the same.
fn comment_for(top: &Path, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    if env("GIT_EDITOR").as_deref() == Some(":") {
        return None;
    }
    Some(comment_of(top))
}

/// The string an edited message's comment lines start with in the repository at `top`:
/// `core.commentString`, else `core.commentChar`, else `#`. `auto` picks a character only git
/// knows once it has the message, so it reads as `#` too.
fn comment_of(top: &Path) -> String {
    ["core.commentString", "core.commentChar"]
        .into_iter()
        .find_map(|key| {
            git::run(top, &["config", "--get", key], git::READ)
                .ok()
                .filter(git::Run::ok)
                .map(|r| r.line().trim_end_matches(['\n', '\r']).to_owned())
                .filter(|v| !v.is_empty() && v != "auto")
        })
        .unwrap_or_else(|| "#".to_owned())
}

/// The change a commit in the work tree at `top` belongs to: the one whose record, in the
/// workspace `top` sits in, has a member for this repo on the branch `top` is on. Read from the
/// records and git, never stored (ADR 0060). `None` for a tree that is not a workspace's clone
/// or piece, a detached HEAD, or a branch no change names.
pub fn change_of(plane: &Path, top: &Path) -> Option<String> {
    let (workspace, repo) = repo_at(plane, top)?;
    let branch = git::run(
        top,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        git::READ,
    )
    .ok()
    .filter(git::Run::ok)?
    .line()
    .trim()
    .to_owned();
    crate::change::store::read_all(plane, &workspace)
        .records
        .into_iter()
        .find(|record| record.member(&repo).is_some_and(|m| m.branch == branch))
        .map(|record| record.change)
}

/// What a commit's trailers claim for one of the provenance keys, read by the one rule every
/// reader follows (#1019, `docs/plane-format.md`, *Provenance trailers*).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Claim {
    /// No line names the key, or one value that could not stand on a trailer line does.
    #[default]
    Unsaid,
    /// Every line that names the key names this value, under any spelling it has had.
    One(String),
    /// The lines name more than one value, in the order they stand. **The commit's provenance
    /// for this key is not known**: a reader shows that several are claimed and attributes the
    /// commit to none of them. A value that could not stand on a trailer line is one of them,
    /// shown as [`crate::shown::readable`] makes it. The last is not purlis's: purlis appends its own only where the
    /// same line is not already there, and an agent can type any line after it as well as
    /// before it, so no position says who wrote a line.
    Several(Vec<String>),
}

impl Claim {
    /// The value, where exactly one is claimed.
    pub fn one(&self) -> Option<&str> {
        match self {
            Self::One(value) => Some(value),
            Self::Unsaid | Self::Several(_) => None,
        }
    }

    /// The claim the distinct values `heard` for one key make, in the order they stood: each
    /// with whether it could stand on a trailer line ([`known`]).
    fn of(heard: Vec<(String, bool)>) -> Self {
        match heard.as_slice() {
            [] => Self::Unsaid,
            [(value, true)] => Self::One(value.clone()),
            [(_, false)] => Self::Unsaid,
            _ => Self::Several(
                heard
                    .into_iter()
                    .map(|(value, known)| {
                        if known {
                            value
                        } else {
                            crate::shown::readable(&value, MOST)
                        }
                    })
                    .collect(),
            ),
        }
    }
}

/// What a commit's provenance trailers claim, key by key ([`Claim`]): the reader's side of the
/// lines [`append`] writes (#1019).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Claims {
    pub assisted_by: Claim,
    pub chat: Claim,
    pub persona: Claim,
    pub change: Claim,
}

impl Claims {
    /// The claims in `trailers`, the commit's trailer block as git parses it, one `Key: value`
    /// a line: what `git log --format='%(trailers:only,unfold)'` prints. A reader asks git for
    /// the block and never greps the whole message, so a body line that only looks like a
    /// trailer claims nothing.
    ///
    /// **The rule** (#1019): a key is read under every spelling it has had (`Purlis-Chat` and
    /// its older name are one key) and in any case, as git compares trailer keys. The same
    /// value said twice is one claim. Two different values are [`Claim::Several`], and the
    /// commit's provenance for that key is unknown. They are a claim, not proof, whatever they
    /// say.
    ///
    /// **A value that could not stand on one trailer line** (one word of printable ASCII, at
    /// most 100 characters) is not a value purlis would write, and alone it claims nothing. It
    /// is still a line naming the key, so beside any other value it makes the key
    /// [`Claim::Several`] (#1021): a malformed second `Purlis-Chat:` line leaves the chat
    /// unknown rather than letting the well-formed one stand alone. Reading fails closed,
    /// because nothing tells which of the two lines was the commit's.
    pub fn read(trailers: &str) -> Self {
        let (mut assisted_by, mut chat, mut persona, mut change) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for line in trailers.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let same = |name: &str| name.eq_ignore_ascii_case(key);
            let heard: &mut Vec<(String, bool)> = if same(ASSISTED_BY) {
                &mut assisted_by
            } else if crate::names::TRAILER_CHAT.spellings().any(same) {
                &mut chat
            } else if crate::names::TRAILER_PERSONA.spellings().any(same) {
                &mut persona
            } else if crate::names::TRAILER_CHANGE.spellings().any(same) {
                &mut change
            } else {
                continue;
            };
            if !heard.iter().any(|(had, _)| had == value) {
                heard.push((value.to_owned(), one_word(value)));
            }
        }
        Self {
            assisted_by: Claim::of(assisted_by),
            chat: Claim::of(chat),
            persona: Claim::of(persona),
            change: Claim::of(change),
        }
    }
}

/// `value`, where it can stand on one trailer line as one word: printable ASCII, no space.
fn known(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|v| one_word(v))
}

/// Whether `value` can stand on one trailer line as one word: printable ASCII, no space, at
/// most [`MOST`] bytes.
fn one_word(value: &str) -> bool {
    !value.is_empty() && value.len() <= MOST && value.bytes().all(|b| b.is_ascii_graphic())
}

#[cfg(test)]
mod tests;
