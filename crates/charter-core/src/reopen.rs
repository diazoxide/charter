//! What was open when the app last quit, so the next launch can put it back.
//!
//! This is the app's own file, at `.charter/app/reopen.json` in the plane. It is not the
//! tmux frame's `.charter/frame/reopen.json` — that one belongs to the frame this app
//! replaces, and the app never touches it (`docs/plane-format.md`).
//!
//! Nothing here trusts the file. It is written by a process that may have been killed
//! halfway, edited by hand, or left behind by an older version, and every value in it
//! becomes part of a command line — so reading is a conversion into types that hold their
//! own shape, and anything that does not convert is dropped rather than repaired.

use std::path::{Path, PathBuf};

use crate::harness::{Harness, SessionId};

/// The one version of this file this app writes and reads. A record of any other version is
/// ignored whole, the way the frame's own manifest is: a format that changed means the
/// chats in it cannot be trusted to mean what they say.
pub const VERSION: u32 = 1;

/// Where the record lives, relative to a plane root.
pub const IN_PLANE: &str = ".charter/app/reopen.json";

/// One chat as it was: what it was running, where, and the conversation to bring back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    /// The program, as it was launched. A path or a bare name.
    pub program: String,
    /// Its arguments, without any charter added — those are decided again at the reopen,
    /// because a resume spells them differently from a start.
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// What the operator calls this chat, and what a harness that takes a name is given.
    pub name: String,
    /// The conversation to bring back, where the app knows one.
    pub resume: Option<SessionId>,
    /// Whether this was the chat in front.
    pub active: bool,
}

/// Every chat that was open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    pub chats: Vec<Chat>,
}

/// How a chat came back, which is what the pane showing it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reopened {
    /// The harness was given the conversation to bring back.
    Resumed(SessionId),
    /// It started as a new chat, for this reason.
    Fresh(Fresh),
}

/// Why a chat came back as a new one rather than the conversation it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fresh {
    /// Nothing recorded a conversation for it — the app never learnt this harness's session
    /// id. Codex reports its id only through a hook, inside its first turn.
    NoConversationRecorded,
    /// Its program is not a harness charter has measured a resume for — a shell, say.
    NoResumeForThisProgram,
}

/// What starts a chat, and what the app has to remember about having started it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    /// The conversation this chat is now under — the one resumed, or the one the app just
    /// chose for it. This is what the next quit records, so a chat started fresh today can
    /// be resumed tomorrow. None where the harness chooses its own id, or is not one.
    pub session: Option<SessionId>,
    pub how: Reopened,
}

impl Chat {
    /// The harness this chat runs, or none for a program that is not one.
    pub fn harness(&self) -> Option<Harness> {
        Harness::of_command(&self.program)
    }

    /// The program and arguments that bring this chat back, and which of the two happened.
    ///
    /// The recorded arguments come first and charter's own go after them, which is the order
    /// the Python charter uses: the operator's own words are never rewritten, only added to.
    pub fn launch(&self) -> Launch {
        let mut args = self.args.clone();
        let (added, session, how) = match (self.harness(), self.resume.as_ref()) {
            // Not a harness charter has measured — a shell. It comes back as itself, and
            // there is nothing to resume it by.
            (None, _) => (
                Vec::new(),
                None,
                Reopened::Fresh(Fresh::NoResumeForThisProgram),
            ),
            (Some(harness), Some(id)) => match harness.resume_argv(id, &self.name) {
                Some(argv) => (argv, Some(id.clone()), Reopened::Resumed(id.clone())),
                None => (
                    Vec::new(),
                    None,
                    Reopened::Fresh(Fresh::NoResumeForThisProgram),
                ),
            },
            // No conversation to bring back, so this is a new chat. Where the harness takes
            // an id charter chose, it is given one and that id is kept — otherwise the next
            // quit would have nothing to record and the chat could never be resumed at all.
            (Some(harness), None) => {
                let chosen = harness.chooses_session_id().then(SessionId::fresh);
                let argv = chosen
                    .as_ref()
                    .map(|id| harness.new_session_argv(id, &self.name))
                    .unwrap_or_default();
                (argv, chosen, Reopened::Fresh(Fresh::NoConversationRecorded))
            }
        };
        args.extend(added);
        Launch {
            program: self.program.clone(),
            args,
            session,
            how,
        }
    }
}

/// The record's path inside `plane_root`.
pub fn path(plane_root: &Path) -> PathBuf {
    plane_root.join(IN_PLANE)
}

/// Writes the record, creating `.charter/app/` if it is not there.
///
/// The file is written beside itself and renamed over, so a launch that reads it never sees
/// half of one — the app can be killed at any moment, and quitting is exactly when it is.
pub fn write(plane_root: &Path, record: &Record) -> std::io::Result<()> {
    let file = path(plane_root);
    let dir = file.parent().expect("the record's path has a directory");
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(&OnDisk::from(record))
        .expect("the record is plain data serde can always write");

    // Beside itself, then renamed over: a rename is atomic on every platform charter runs
    // on, so a launch reading this file sees the whole of one record or the whole of the
    // one before it. Quitting is when the app is most likely to be killed halfway.
    let beside = file.with_extension("json.writing");
    std::fs::write(&beside, text + "\n")?;
    std::fs::rename(&beside, &file)
}

/// What was open, or nothing.
///
/// This never fails. No file is a first launch; a file of another version, or one that is
/// not the record at all, is nothing open — there is no repair that would be honest, and
/// refusing to start would be worse than starting empty.
pub fn read(plane_root: &Path) -> Record {
    let Ok(text) = std::fs::read_to_string(path(plane_root)) else {
        return Record::default();
    };
    let Ok(on_disk) = serde_json::from_str::<OnDisk>(&text) else {
        return Record::default();
    };
    if on_disk.version != VERSION {
        return Record::default();
    }
    Record {
        chats: on_disk.chats.into_iter().map(Chat::from).collect(),
    }
}

/// The record as JSON, and the only place this file's field names are written down.
///
/// It is deliberately a shape apart from `Record`: every field is the plainest type JSON
/// has, so reading cannot fail on a value, and the conversion into `Chat` is the one place
/// a value is held to what it has to be. A `SessionId` that derived `Deserialize` would put
/// that guard where a later edit could quietly drop it.
#[derive(serde::Serialize, serde::Deserialize)]
struct OnDisk {
    version: u32,
    /// When it was recorded, in seconds since the epoch. Nothing reads it; it is here
    /// because a record nobody can date is one nobody can debug.
    at: u64,
    chats: Vec<ChatOnDisk>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ChatOnDisk {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    name: String,
    /// The conversation's id, or empty. A value that is not an id reads as empty: the chat
    /// comes back as a new one rather than putting an unknown word on a command line.
    #[serde(default)]
    resume: String,
    #[serde(default)]
    active: bool,
}

impl From<&Record> for OnDisk {
    fn from(record: &Record) -> Self {
        Self {
            version: VERSION,
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_secs())
                .unwrap_or_default(),
            chats: record
                .chats
                .iter()
                .map(|chat| ChatOnDisk {
                    program: chat.program.clone(),
                    args: chat.args.clone(),
                    cwd: chat
                        .cwd
                        .as_ref()
                        .map(|cwd| cwd.display().to_string())
                        .unwrap_or_default(),
                    name: chat.name.clone(),
                    resume: chat
                        .resume
                        .as_ref()
                        .map(SessionId::to_string)
                        .unwrap_or_default(),
                    active: chat.active,
                })
                .collect(),
        }
    }
}

impl From<ChatOnDisk> for Chat {
    fn from(chat: ChatOnDisk) -> Self {
        Self {
            program: chat.program,
            args: chat.args,
            cwd: (!chat.cwd.is_empty()).then(|| PathBuf::from(chat.cwd)),
            name: chat.name,
            resume: SessionId::new(chat.resume).ok(),
            active: chat.active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn claude(name: &str, resume: Option<&str>) -> Chat {
        Chat {
            program: "claude".to_owned(),
            args: vec!["--model".to_owned(), "opus".to_owned()],
            cwd: Some(PathBuf::from("/Users/aharon/IdeaProjects/charter")),
            name: name.to_owned(),
            resume: resume.map(|id| SessionId::new(id).expect("a valid id in a test")),
            active: false,
        }
    }

    const ID: &str = "11111111-2222-4333-8444-555555555555";

    #[test]
    fn what_was_written_is_what_the_next_launch_reads() {
        let plane = tempfile::tempdir().unwrap();
        let record = Record {
            chats: vec![claude("ide.7", Some(ID)), claude("ide.8", None)],
        };

        write(plane.path(), &record).expect("the record is written");

        assert_eq!(read(plane.path()), record);
    }

    #[test]
    fn the_record_is_written_where_the_plane_format_says() {
        let plane = tempfile::tempdir().unwrap();

        write(plane.path(), &Record::default()).expect("the record is written");

        assert!(plane.path().join(".charter/app/reopen.json").is_file());
    }

    #[test]
    fn a_plane_with_no_record_has_nothing_to_reopen() {
        let plane = tempfile::tempdir().unwrap();

        assert_eq!(read(plane.path()), Record::default());
    }

    #[test]
    fn a_record_of_another_version_is_ignored_whole() {
        let plane = tempfile::tempdir().unwrap();
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", Some(ID))],
            },
        )
        .unwrap();
        let file = path(plane.path());
        let text = fs::read_to_string(&file).unwrap();
        fs::write(&file, text.replace("\"version\": 1", "\"version\": 2")).unwrap();

        assert_eq!(read(plane.path()), Record::default());
    }

    #[test]
    fn a_file_that_is_not_the_record_at_all_is_nothing_to_reopen() {
        let plane = tempfile::tempdir().unwrap();
        fs::create_dir_all(path(plane.path()).parent().unwrap()).unwrap();
        fs::write(path(plane.path()), "{ this is not json").unwrap();

        assert_eq!(read(plane.path()), Record::default());
    }

    #[test]
    fn a_chat_whose_recorded_conversation_could_be_read_as_a_flag_comes_back_without_one() {
        // The file is on disk and every value in it reaches a command line. A chat with a
        // hostile id is still reopened — it is a chat the operator had — but as a new one.
        let plane = tempfile::tempdir().unwrap();
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", Some(ID))],
            },
        )
        .unwrap();
        let file = path(plane.path());
        let text = fs::read_to_string(&file).unwrap();
        fs::write(&file, text.replace(ID, "--dangerously-skip-permissions")).unwrap();

        let back = read(plane.path());

        assert_eq!(back.chats.len(), 1, "the chat itself was dropped: {back:?}");
        assert_eq!(back.chats[0].resume, None);
    }

    #[test]
    fn a_chat_with_a_recorded_conversation_is_resumed() {
        let chat = claude("ide.7", Some(ID));

        let launch = chat.launch();

        assert_eq!(launch.program, "claude");
        assert_eq!(
            launch.args,
            vec!["--model", "opus", "--resume", ID, "--name", "ide.7"]
        );
        assert_eq!(launch.how, Reopened::Resumed(SessionId::new(ID).unwrap()));
    }

    #[test]
    fn a_chat_with_no_recorded_conversation_comes_back_as_a_new_one() {
        // A Codex chat is always this: Codex reports its id through a hook inside its first
        // turn, so until hooks land the app has none to record.
        let chat = Chat {
            program: "codex".to_owned(),
            ..claude("ide.7", None)
        };

        let launch = chat.launch();

        assert_eq!(launch.program, "codex");
        assert_eq!(launch.args, vec!["--model", "opus"]);
        assert_eq!(launch.how, Reopened::Fresh(Fresh::NoConversationRecorded));
    }

    #[test]
    fn a_new_claude_chat_is_started_under_an_id_the_app_chose_so_it_can_be_resumed_next_time() {
        let chat = claude("ide.7", None);

        let launch = chat.launch();

        assert_eq!(launch.how, Reopened::Fresh(Fresh::NoConversationRecorded));
        let chosen = launch
            .args
            .iter()
            .position(|word| word == "--session-id")
            .map(|at| launch.args[at + 1].clone())
            .expect("a new Claude Code chat is given an id");
        assert!(SessionId::new(&chosen).is_ok(), "{chosen:?} is not an id");
        assert_eq!(launch.args[..2], ["--model", "opus"]);
    }

    #[test]
    fn the_id_a_new_chat_was_given_is_the_one_the_next_quit_records() {
        // Without this a chat could never be resumed a second time: the app would choose an
        // id, hand it to the harness, and forget it before the quit that has to write it down.
        let chat = claude("ide.7", None);

        let launch = chat.launch();

        let chosen = launch.session.as_ref().expect("the chat is under an id");
        assert!(
            launch.args.contains(&chosen.to_string()),
            "the recorded id {chosen} is not the one the harness was given: {:?}",
            launch.args
        );
    }

    #[test]
    fn a_resumed_chat_stays_under_the_id_it_was_resumed_by() {
        let launch = claude("ide.7", Some(ID)).launch();

        assert_eq!(launch.session, Some(SessionId::new(ID).unwrap()));
    }

    #[test]
    fn a_codex_chat_is_under_no_id_the_app_knows_because_codex_chose_it() {
        let launch = Chat {
            program: "codex".to_owned(),
            ..claude("ide.7", None)
        }
        .launch();

        assert_eq!(launch.session, None);
    }

    #[test]
    fn a_pane_running_a_shell_comes_back_as_a_shell_and_says_why_it_is_not_resumed() {
        let chat = Chat {
            program: "/bin/zsh".to_owned(),
            args: vec![],
            resume: None,
            ..claude("ide.9", None)
        };

        let launch = chat.launch();

        assert_eq!(launch.program, "/bin/zsh");
        assert_eq!(launch.args, Vec::<String>::new());
        assert_eq!(launch.how, Reopened::Fresh(Fresh::NoResumeForThisProgram));
        assert_eq!(launch.session, None);
    }

    #[test]
    fn writing_the_record_leaves_nothing_beside_it() {
        // The record is written beside itself and renamed over, so a launch never reads
        // half of one. This is the guard on that: a write that stopped at the file beside
        // it would leave two here, and the record itself would never appear.
        let plane = tempfile::tempdir().unwrap();

        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", Some(ID))],
            },
        )
        .unwrap();

        let left: Vec<String> = fs::read_dir(plane.path().join(".charter/app"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["reopen.json"]);
    }

    #[test]
    fn a_record_written_over_an_older_one_replaces_it() {
        let plane = tempfile::tempdir().unwrap();
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", Some(ID))],
            },
        )
        .unwrap();

        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.8", None)],
            },
        )
        .unwrap();

        assert_eq!(read(plane.path()).chats.len(), 1);
        assert_eq!(read(plane.path()).chats[0].name, "ide.8");
    }

    #[test]
    fn the_chat_that_was_in_front_is_the_one_marked_active() {
        let plane = tempfile::tempdir().unwrap();
        let front = Chat {
            active: true,
            ..claude("ide.8", None)
        };
        write(
            plane.path(),
            &Record {
                chats: vec![claude("ide.7", None), front],
            },
        )
        .unwrap();

        let back = read(plane.path());

        assert_eq!(
            back.chats
                .iter()
                .filter(|chat| chat.active)
                .map(|chat| chat.name.as_str())
                .collect::<Vec<_>>(),
            vec!["ide.8"]
        );
    }
}
