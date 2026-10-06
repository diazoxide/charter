//! Resuming a session from its record: what the Sessions panel's **Resume** starts (SI-8d,
//! ADR 0064).
//!
//! The operator's ruling: *the user can always get old sessions back.* Resume starts a NEW chat
//! in the record's place — its workspace, or the plane root — on the record's harness, given
//! the record's conversation, as the record's persona where the plane still has it, and with
//! the record in its session-start briefing, quoted as data
//! ([`crate::sessionrecord::RESUMING_ENV`]).
//!
//! **One argument builder.** The conversation is handed over as a relaunch hands one over:
//! [`crate::start::Start::resume`], through [`crate::start::ready`], which builds
//! `claude --resume <id>`, `codex resume <id>` and `opencode -s <id>` from
//! [`crate::harness::Harness::resume_argv`]. Nothing here spells a harness's flag.
//!
//! **When the conversation cannot be given, the chat still comes, fresh, with the record.** The
//! record holds no id; it names no harness charter starts; or no profile on this machine runs
//! it. Each is a [`NotResumed`], which the window says on the new chat. The one case that
//! cannot be known in advance is a harness that no longer has the conversation: it says so by
//! ending its program, and the window then asks again with `after_failure`, which starts the
//! same record fresh and says that is why.

use std::path::{Path, PathBuf};

use crate::active::Place;
use crate::harness::{Harness, SessionId};
use crate::sessionrecord::{self, Listed};
use crate::start::{self, Ready, Start};

/// Why a Resume started a fresh chat rather than the record's conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotResumed {
    /// The record holds no conversation id: the chat that wrote it never had one the app knew,
    /// or it was written outside the app.
    NoConversation,
    /// The record does not say which harness it ran on.
    NoHarness,
    /// No harness profile on this machine runs the record's harness, or none of them would
    /// start.
    NoProfileFor(String),
    /// The harness was given the conversation and could not bring it back: it ended before it
    /// reported a session (the window's `after_failure`).
    HarnessLostIt {
        harness: String,
        conversation: String,
    },
}

impl NotResumed {
    /// The words the new chat's note says after *came back as a new chat:*.
    pub fn said(&self) -> String {
        match self {
            Self::NoConversation => {
                "its session record holds no conversation id, so it starts with the record in \
                 its briefing"
                    .to_owned()
            }
            Self::NoHarness => "its session record does not say which harness it ran on, so it \
                                starts on this project's default with the record in its briefing"
                .to_owned(),
            Self::NoProfileFor(harness) => format!(
                "no harness profile on this machine runs {harness}, so it starts on this \
                 project's default with the record in its briefing"
            ),
            Self::HarnessLostIt {
                harness,
                conversation,
            } => format!(
                "{harness} could not bring back conversation {conversation}, so it starts with the \
                 record in its briefing"
            ),
        }
    }
}

/// A Resume that may start, with everything the app needs to start it.
#[derive(Debug, Clone)]
pub struct Resumed {
    /// The record, as a listing says it.
    pub record: Listed,
    /// Its place, which is where the chat starts.
    pub place: Place,
    /// What was asked of [`start::ready`] — what the app's record of the chat keeps.
    pub start: Start,
    /// What it answered.
    pub ready: Ready,
    /// `None` when the chat is given the record's conversation; why not, otherwise.
    pub fresh: Option<NotResumed>,
    /// What was guessed because the record could not say it — its profile, its directory —
    /// each one sentence the window says on the new chat.
    pub notes: Vec<String>,
}

/// What a chat resuming a record as `persona` holds (#1362, D-1362-6): the default persona's
/// grants where `persona`'s hosts reach past them, else its own.
pub fn resumed_holds(root: &Path, persona: Option<&str>) -> Option<crate::reopen::HeldGrants> {
    let default = start::persona_for_a_new_chat(root);
    crate::sandbox::persona::held_unless_within(
        crate::sandbox::Plane::read(root).said().policy.as_ref(),
        default.as_deref(),
        persona.or(default.as_deref()),
    )
}

/// The chat that resumes the record at the plane-relative `path` ([`sessionrecord::locate`]),
/// named `name`, or the one sentence saying why nothing may start.
///
/// `after_failure` is the window saying the harness was given this record's conversation and
/// ended without bringing it back: the same record then starts fresh.
///
/// **Which profile.** The one the record names (`profile:`, SI-8e) where this machine still has
/// it as a profile of the record's harness; after it — and in its place for a record written
/// before the key, or naming one that is gone — the project's default, then the ones this
/// machine declares, then the built-in, because a declared profile is the likelier to be the
/// account the conversation is under. The first of those [`start::ready`] lets start is used,
/// and where it is not the record's own, [`Resumed::notes`] says so. With no such profile, or
/// none that starts, the chat starts fresh on the project's default.
///
/// **Where.** The record's `cwd:` where it is still a directory inside the record's place,
/// else the place's own directory, said in a note ([`where_it_starts`]).
pub fn ready(root: &Path, path: &str, name: &str, after_failure: bool) -> Result<Resumed, String> {
    let opened = sessionrecord::open(root, path)?;
    let place = opened.place.clone();
    let record = opened.listed;
    let (cwd, moved) = where_it_starts(root, &place, record.cwd.as_deref())?;
    let mut notes: Vec<String> = moved.into_iter().collect();
    let persona = record
        .persona
        .clone()
        .filter(|who| start::persona_for(root, who));
    let conversation = record
        .conversation
        .as_deref()
        .and_then(|id| SessionId::new(id).ok());
    let base = Start {
        profile: None,
        persona,
        name: name.to_owned(),
        cwd: Some(cwd),
        resume: None,
        show_footer: false,
        resuming: Some(record.shown.clone()),
        // A resumed chat starts sandboxed, or is refused, whatever the chat it resumes ran under
        // (ADR 0067 §7): an opt-out is never inherited.
        without_sandbox: None,
        // A record's `persona:` is written from what the chat said it was, so a Resume never
        // takes a persona's hosts past the default persona's on the record's word: it holds the
        // default's until the person allows its own (#1362, D-1362-6).
        held: None,
    };
    let base = Start {
        held: resumed_holds(root, base.persona.as_deref()),
        ..base
    };

    let (set, _) = crate::profiles::for_launch(root);
    let harness = record.harness.as_deref();
    let kind = harness.and_then(Harness::of_kind);
    let mut fresh = match (harness, kind, &conversation) {
        (None, _, _) => Some(NotResumed::NoHarness),
        (Some(word), None, _) => Some(NotResumed::NoProfileFor(word.to_owned())),
        (Some(_), Some(_), None) => Some(NotResumed::NoConversation),
        (Some(word), Some(_), Some(id)) if after_failure => Some(NotResumed::HarnessLostIt {
            harness: word.to_owned(),
            conversation: id.as_str().to_owned(),
        }),
        _ => None,
    };

    // The record's own harness, on the first of its profiles that starts.
    let mut refused = None;
    if let Some(kind) = kind {
        let named = record.profile.as_deref();
        for profile in candidates(&set, kind, named) {
            let start = Start {
                profile: Some(profile),
                resume: if fresh.is_none() {
                    conversation.clone()
                } else {
                    None
                },
                ..base.clone()
            };
            match start::ready(&start, root) {
                Ok(ready) => {
                    let used = start.profile.as_deref().unwrap_or_default();
                    if named != Some(used) {
                        notes.push(profile_guessed(named, used, &set, kind, refused.is_some()));
                    }
                    return Ok(Resumed {
                        record,
                        place,
                        start,
                        ready,
                        fresh,
                        notes,
                    });
                }
                Err(why) => refused = Some(why),
            }
        }
        if let Some(word) = harness {
            fresh = Some(NotResumed::NoProfileFor(word.to_owned()));
        }
    }

    // Fresh, on the project's default: the record still reaches the chat's briefing.
    let Some(default) = default_profile(&set) else {
        return Err(refused.unwrap_or_else(|| {
            "this machine has no harness profile to start a chat on, so nothing was started."
                .to_owned()
        }));
    };
    let start = Start {
        profile: Some(default),
        ..base
    };
    let ready = start::ready(&start, root)?;
    Ok(Resumed {
        record,
        place,
        start,
        ready,
        fresh,
        notes,
    })
}

/// The directory a resume of a record of `place` starts in, and the sentence saying it is not
/// the record's own where it is not.
///
/// **The record's `cwd:` where it still can be**: a directory that, with every link followed,
/// is inside the record's place — the workspace's directory for a workspace's record, the
/// plane for the plane root's. Claude Code keeps a conversation per directory, so a record
/// written from a piece resumes only from that piece. Anything else — no `cwd:`, one gone, one
/// that leads out of the place — starts in the place's own directory, and says so. The value
/// was already held to a plane-relative spelling on the way in
/// ([`sessionrecord::relative_dir_ok`]); this is where the file system is asked.
fn where_it_starts(
    root: &Path,
    place: &Place,
    recorded: Option<&str>,
) -> Result<(PathBuf, Option<String>), String> {
    let home = place_dir(root, place)?;
    let said_home = match place {
        Place::PlaneRoot => "the plane root".to_owned(),
        Place::Workspace(ws) => format!("workspace {ws}'s own directory"),
    };
    let Some(rel) = recorded else {
        return Ok((
            home,
            Some(format!(
                "its session record does not say which directory it ran in, so it starts in \
                 {said_home}"
            )),
        ));
    };
    let candidate = if rel == "." {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let inside = sessionrecord::relative_dir_ok(rel)
        && candidate.is_dir()
        && match (
            crate::contain::resolved(&candidate),
            crate::contain::resolved(&home),
        ) {
            (Some(lands), Some(home)) => lands.starts_with(home),
            _ => false,
        };
    if inside {
        return Ok((candidate, None));
    }
    Ok((
        home,
        Some(format!(
            "the directory its session record ran in, {}, is not a directory in {} now, so it \
             starts in {said_home}",
            crate::shown::short(rel),
            match place {
                Place::PlaneRoot => "this plane".to_owned(),
                Place::Workspace(ws) => format!("workspace {ws}"),
            }
        )),
    ))
}

/// The sentence saying the chat runs on `used` rather than the profile the record names.
fn profile_guessed(
    named: Option<&str>,
    used: &str,
    set: &crate::profiles::ProfileSet,
    kind: Harness,
    named_refused: bool,
) -> String {
    match named {
        None => format!(
            "its session record does not name the harness profile it ran on, so it runs on \
             {used}, this machine's first {} profile",
            kind.name()
        ),
        Some(named)
            if named_refused
                && set
                    .get(named)
                    .is_some_and(|p| Harness::of_kind(&p.kind) == Some(kind)) =>
        {
            format!("its session record's profile {named} would not start, so it runs on {used}")
        }
        Some(named) => format!(
            "its session record's profile {named} is not a {} profile on this machine now, so \
             it runs on {used}",
            kind.name()
        ),
    }
}

/// The directory a chat in `place` starts in when the record names none: the workspace's, or
/// the plane root.
fn place_dir(root: &Path, place: &Place) -> Result<PathBuf, String> {
    match place {
        Place::PlaneRoot => Ok(root.to_path_buf()),
        Place::Workspace(ws) => {
            if !crate::wscmd::workspace_dir_exists(root, ws) {
                return Err(format!(
                    "this plane has no workspace '{ws}' any more, so its session record has \
                     nowhere to resume"
                ));
            }
            crate::workspaces::Plane::open(root)
                .workspace(ws)
                .map(|found| found.dir().to_path_buf())
                .map_err(|why| why.to_string())
        }
    }
}

/// The profiles of `kind`, in the order a Resume tries them: the one the record names, where
/// this machine has it; then the default, then the declared, then the built-in.
fn candidates(
    set: &crate::profiles::ProfileSet,
    kind: Harness,
    named: Option<&str>,
) -> Vec<String> {
    let mut of_kind: Vec<&crate::profiles::Profile> = set
        .profiles()
        .iter()
        .filter(|p| Harness::of_kind(&p.kind) == Some(kind))
        .collect();
    of_kind.sort_by_key(|p| {
        (
            named != Some(p.name.as_str()),
            set.default.as_deref() != Some(p.name.as_str()),
            p.source == crate::profiles::Source::BuiltIn,
        )
    });
    of_kind.into_iter().map(|p| p.name.clone()).collect()
}

/// The profile a fresh chat starts on: the project's default, else the first there is.
fn default_profile(set: &crate::profiles::ProfileSet) -> Option<String> {
    set.default
        .clone()
        .or_else(|| set.profiles().first().map(|p| p.name.clone()))
}
