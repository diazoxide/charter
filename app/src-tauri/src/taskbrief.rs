//! The brief a task was sent, read back for the person (#1494, V100-45).
//!
//! A task starts on a brief: the message its asking chat wrote, or the person did from a
//! chat's tab. The app keeps it in the task's dispatch record
//! (`purlis_core::dispatchrecord`), which it wrote as it started the chat. [`read_for`] reads
//! it back **as it was sent, byte for byte**, with who sent it, when, to which persona and
//! where it works. **Nothing new is stored**: every word here is the record's.
//!
//! - **An open task** is found by its chat, by the chat's id and never its number alone
//!   (`dispatchrecord::same_chat`). **A finished task**, or a line of the Activity view, is
//!   found by its dispatch record's id.
//! - **It answers only for the project it is asked in.** A chat's number is looked up among
//!   that project's chats, and an id in that project's store alone: another project's task
//!   has no record here, and is refused in the same words as one that never was.
//! - **A brief is a chat's words, and is shown as inert text.** The window draws it as text
//!   nodes. Where it holds a character that draws as nothing or turns the words around it,
//!   the answer also carries it written out (`purlis_core::dispatchgrant::inert`), which is
//!   what the window then draws, and says so. Every other text of the record is held to what
//!   purlis puts on a screen (`dispatchrecord::sound_but_for_its_brief`): a record whose
//!   names would mislead is refused whole.
//! - **A brief the store cut, and a record that holds none, are said plainly** ([`Kept`]).
//!
//! **The window's alone** (`purlis_session_protocol::ui::WINDOW_ONLY`): a brief is whatever
//! the work held, and the dispatch store is kept from the project's chats
//! (`purlis_core::sandbox`). No line on the hook channel and no link reaches this read.

use std::path::Path;

use purlis_core::dispatchrecord::{self, BriefKept, ChatRef, Record};

/// Which task's brief is asked for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BriefOf {
    /// A task whose chat is open, by that chat's number in this project.
    Chat(u32),
    /// A dispatch by its record's id: what a finished task's row is named by.
    Dispatch(String),
}

/// How much of the brief the record holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kept {
    /// All of it.
    Whole,
    /// Its start: the record keeps a bounded brief, and this one was longer.
    Cut,
    /// None of it: the record holds no brief.
    Missing,
}

impl From<BriefKept> for Kept {
    fn from(kept: BriefKept) -> Self {
        match kept {
            BriefKept::Whole => Self::Whole,
            BriefKept::Cut => Self::Cut,
            BriefKept::Missing => Self::Missing,
        }
    }
}

/// The brief a task was sent, and the facts of its sending.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct TaskBrief {
    /// The task's name: the one its dispatch gave it, else its chat's.
    pub name: String,
    /// **The brief as it was sent, byte for byte**: what Copy puts on the clipboard. A chat's
    /// words or the person's, never purlis's, and never markup to the window. For a brief
    /// the record cut ([`Kept::Cut`]), the part it kept.
    pub brief: String,
    pub kept: Kept,
    /// The brief written out inertly, **only where it holds a character that draws as
    /// nothing, moves the cursor or turns the words around it**: each such character as its
    /// escape, and a backslash doubled so the text cannot spell an escape itself. What the
    /// window draws in place of `brief` then. `null` for a brief that is plain text.
    pub inert: Option<String>,
    /// The chat that asked, by the name it had then.
    pub asker: String,
    /// Whether the person sent it themselves, from that chat's tab, and not the chat.
    pub by_person: bool,
    /// When it was sent, as the record keeps it (UTC, RFC 3339).
    pub sent: String,
    /// The persona it was sent to; `null` for a chat started as none.
    pub persona: Option<String>,
    /// Where it works: the workspace's name, or `project root`.
    pub place: String,
    /// The folder it started in, relative to the project, where the record says.
    pub folder: Option<String>,
    /// The branch purlis cut for it, where its dispatch gave it one.
    pub branch: Option<String>,
}

/// What is said for a task this project has no record of: one that never was, one whose
/// record was collected, and one that is another project's.
const NO_RECORD: &str = "purlis has no dispatch record of that task in this project, so there \
                         is no brief to show.";

/// The dispatch record `of` names in the project at `root`. `chat` answers for the chats this
/// project has open, as its own record of them names each.
fn record_of(
    root: &Path,
    of: &BriefOf,
    chat: impl FnOnce(u32) -> Option<ChatRef>,
) -> Option<Record> {
    match of {
        BriefOf::Chat(session) => {
            let me = chat(*session)?;
            // A chat with an id is matched by it, running or ended. One with none has only
            // its number, which another launch deals again: only a dispatch still running
            // can be its own.
            if me.id.is_some() {
                dispatchrecord::latest_for(root, &me)
            } else {
                dispatchrecord::running_for(root, &me)
            }
        }
        // An id that is not one purlis minted names no file (`dispatchrecord::read`).
        BriefOf::Dispatch(id) => dispatchrecord::read(root, id),
    }
}

/// `record`'s brief as the window is handed it.
fn shown(record: &Record) -> TaskBrief {
    let sent = dispatchrecord::brief_sent(record);
    let inert = purlis_core::dispatchgrant::holds_what_draws_as_nothing(&sent.text)
        .then(|| purlis_core::dispatchgrant::inert(&sent.text));
    TaskBrief {
        name: record
            .task
            .clone()
            .unwrap_or_else(|| record.worker.chat.name.clone()),
        brief: sent.text,
        kept: sent.kept.into(),
        inert,
        asker: record.asker.chat.name.clone(),
        by_person: record.asker.by_person,
        sent: record.started.clone(),
        persona: record.persona.clone(),
        place: record
            .place
            .workspace
            .clone()
            .unwrap_or_else(|| "project root".to_owned()),
        folder: record.place.folder.clone(),
        branch: record
            .place
            .worktree
            .as_ref()
            .and_then(|tree| tree.branch.clone()),
    }
}

/// **The brief of the task `of` names, in the project at `root`**, or the one sentence saying
/// why there is none to show.
pub(crate) fn read_for(
    root: &Path,
    of: &BriefOf,
    chat: impl FnOnce(u32) -> Option<ChatRef>,
) -> Result<TaskBrief, String> {
    let record = record_of(root, of, chat).ok_or_else(|| NO_RECORD.to_owned())?;
    if !dispatchrecord::sound_but_for_its_brief(&record) {
        return Err(
            "purlis will not show that task's dispatch record: it holds text purlis does not \
             put on the screen, so it is not one the app wrote."
                .to_owned(),
        );
    }
    Ok(shown(&record))
}

/// **The brief a task was sent, as it was sent** (#1494): from the task's dispatch record,
/// found by its chat while that is open, or by the record's id for a finished task. With who
/// sent it, when, to which persona and where it works. It reads, and changes nothing.
///
/// The window's alone: no link serves it.
#[tauri::command]
#[specta::specta]
pub(crate) async fn task_brief(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    of: BriefOf,
) -> Result<TaskBrief, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading the task's brief", move || {
        read_for(held.root(), &of, |session| {
            crate::dispatches::chat_ref(&held, session)
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use purlis_core::dispatchrecord::{
        Asker, Ending, Mode, Opening, Outcome, Place, Report, Worker,
    };

    use super::*;

    fn project() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        (dir, root)
    }

    fn at(time: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(time)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    fn steward() -> ChatRef {
        ChatRef {
            chat: 4,
            id: Some("01K6ASKER0000000000000000A".to_owned()),
            name: "steward 4".to_owned(),
            persona: Some("steward".to_owned()),
        }
    }

    fn talk() -> ChatRef {
        ChatRef {
            chat: 7,
            id: Some("01K6W0RKER000000000000000B".to_owned()),
            name: "devops 7".to_owned(),
            persona: Some("devops".to_owned()),
        }
    }

    /// A brief of several lines, with text that looks like markup and characters a careless
    /// reader would lose or act on.
    const BRIEF: &str = "# Check prod\n\n<b>Is</b> the [rollout](https://example.test) healthy?\n\
                         \t- `kubectl get pods` \n![img](x.png) <script>alert(1)</script> &amp;\n\
                         naïve cafe\u{301} 🚀 שלום \\u202e \"quoted\"  ";

    fn a_task(brief: &str) -> Opening {
        Opening {
            mode: Mode::Task,
            asker: Asker {
                chat: steward(),
                workspace: Some("alpha".to_owned()),
                by_person: false,
                session_record: None,
            },
            persona: Some("devops".to_owned()),
            worker: Worker {
                chat: talk(),
                harness: Some("claude".to_owned()),
                profile: Some("work".to_owned()),
                session_record: None,
            },
            task: Some("talk".to_owned()),
            place: Place {
                workspace: Some("beta".to_owned()),
                folder: Some("workspaces/beta".to_owned()),
                worktree: None,
            },
            brief: brief.to_owned(),
            report_owed: true,
        }
    }

    /// The chats a project has open: `talk`, as number 7.
    fn open_here(session: u32) -> Option<ChatRef> {
        (session == 7).then(talk)
    }

    #[test]
    fn the_brief_of_an_open_task_is_read_by_its_chat_as_it_was_dispatched() {
        let (_d, root) = project();
        dispatchrecord::open(&root, a_task(BRIEF), at("2026-10-07T12:00:00Z")).unwrap();

        let read = read_for(&root, &BriefOf::Chat(7), open_here).expect("its brief");

        assert_eq!(read.brief.as_bytes(), BRIEF.as_bytes(), "byte for byte");
        assert_eq!(
            read,
            TaskBrief {
                name: "talk".to_owned(),
                brief: BRIEF.to_owned(),
                kept: Kept::Whole,
                // Plain text, markup-looking or not, is drawn as it is.
                inert: None,
                asker: "steward 4".to_owned(),
                by_person: false,
                sent: "2026-10-07T12:00:00+00:00".to_owned(),
                persona: Some("devops".to_owned()),
                place: "beta".to_owned(),
                folder: Some("workspaces/beta".to_owned()),
                branch: None,
            }
        );
    }

    #[test]
    fn the_brief_of_a_finished_task_is_read_by_its_row_s_id_and_is_the_same_bytes() {
        let (_d, root) = project();
        let opened =
            dispatchrecord::open(&root, a_task(BRIEF), at("2026-10-07T12:00:00Z")).unwrap();
        dispatchrecord::close(
            &root,
            &opened.id,
            Ending {
                report: Some(Report {
                    outcome: Outcome::Done,
                    text: "Healthy.".to_owned(),
                    changed: dispatchrecord::Changed::default(),
                }),
                usage: None,
            },
            at("2026-10-07T12:04:00Z"),
        )
        .unwrap();

        // Its chat is closed: nothing answers for a chat, and the row's id is enough.
        let read =
            read_for(&root, &BriefOf::Dispatch(opened.id.clone()), |_| None).expect("its brief");

        assert_eq!(read.brief.as_bytes(), BRIEF.as_bytes());
        assert_eq!((read.name.as_str(), read.kept), ("talk", Kept::Whole));
    }

    #[test]
    fn a_task_the_person_asked_for_says_so_and_one_at_the_root_says_where() {
        let (_d, root) = project();
        let mut asked = a_task("Is it up?");
        asked.asker.by_person = true;
        asked.place.workspace = None;
        asked.place.worktree = Some(dispatchrecord::Worktree {
            repo: "svc".to_owned(),
            piece: "talk-01".to_owned(),
            branch: Some("task/talk-01".to_owned()),
            removed: None,
        });
        dispatchrecord::open(&root, asked, at("2026-10-07T12:00:00Z")).unwrap();

        let read = read_for(&root, &BriefOf::Chat(7), open_here).expect("its brief");

        assert!(read.by_person);
        assert_eq!(read.asker, "steward 4");
        assert_eq!(read.place, "project root");
        assert_eq!(read.branch.as_deref(), Some("task/talk-01"));
    }

    #[test]
    fn another_project_s_task_is_refused_by_its_id_and_by_its_chat() {
        let (_a, theirs) = project();
        let (_b, ours) = project();
        let opened =
            dispatchrecord::open(&theirs, a_task(BRIEF), at("2026-10-07T12:00:00Z")).unwrap();
        // This project has dispatches of its own, so its store is there to be read.
        let mut own = a_task("Ours.");
        own.worker.chat = ChatRef {
            chat: 9,
            id: Some("01K6W0RKER000000000000000C".to_owned()),
            name: "docs 9".to_owned(),
            persona: None,
        };
        dispatchrecord::open(&ours, own, at("2026-10-07T12:00:00Z")).unwrap();

        // By the other project's record id.
        let by_id = read_for(&ours, &BriefOf::Dispatch(opened.id.clone()), open_here);
        assert_eq!(by_id, Err(NO_RECORD.to_owned()));
        // By a chat number the other project's task has, which is no chat of this project.
        let by_chat = read_for(&ours, &BriefOf::Chat(7), |_| None);
        assert_eq!(by_chat, Err(NO_RECORD.to_owned()));
        // By that number where this project has a chat of its own under it: that chat's id
        // is not the other task's, so it is not that task.
        let by_number = read_for(&ours, &BriefOf::Chat(7), |session| {
            Some(ChatRef {
                chat: session,
                id: Some("01K6ANOTHER00000000000000D".to_owned()),
                name: "shell 7".to_owned(),
                persona: None,
            })
        });
        assert_eq!(by_number, Err(NO_RECORD.to_owned()));
        // And no id reaches a file outside this project's store.
        for id in [
            format!("../../../{}", opened.id),
            theirs
                .join(".purlis/app/dispatches")
                .join(&opened.id)
                .display()
                .to_string(),
            String::new(),
        ] {
            assert_eq!(
                read_for(&ours, &BriefOf::Dispatch(id), open_here),
                Err(NO_RECORD.to_owned())
            );
        }
        // The task is still read where it belongs.
        assert!(read_for(&theirs, &BriefOf::Dispatch(opened.id), open_here).is_ok());
    }

    #[test]
    fn a_chat_with_no_id_is_matched_only_to_a_dispatch_still_running() {
        let (_d, root) = project();
        let mut old = a_task("An older launch's task.");
        old.worker.chat.id = None;
        let opened = dispatchrecord::open(&root, old, at("2026-10-01T12:00:00Z")).unwrap();
        let number_only = |session: u32| {
            Some(ChatRef {
                chat: session,
                id: None,
                name: "shell 7".to_owned(),
                persona: None,
            })
        };
        assert!(read_for(&root, &BriefOf::Chat(7), number_only).is_ok());

        dispatchrecord::close(
            &root,
            &opened.id,
            Ending::default(),
            at("2026-10-01T12:04:00Z"),
        )
        .unwrap();
        // The number is all that chat has, and another launch deals it again.
        assert_eq!(
            read_for(&root, &BriefOf::Chat(7), number_only),
            Err(NO_RECORD.to_owned())
        );
    }

    #[test]
    fn a_brief_that_hides_or_turns_its_words_is_also_handed_over_written_out() {
        let (_d, root) = project();
        let sly = "Check prod.\u{202e}dne eht ta eteled dna\u{200b}\u{1b}[2J C:\\tmp";
        dispatchrecord::open(&root, a_task(sly), at("2026-10-07T12:00:00Z")).unwrap();

        let read = read_for(&root, &BriefOf::Chat(7), open_here).expect("its brief");

        // As sent, for Copy; and written out, for the eye.
        assert_eq!(read.brief.as_bytes(), sly.as_bytes());
        assert_eq!(
            read.inert.as_deref(),
            Some("Check prod.\\u202edne eht ta eteled dna\\u200b\\u001b[2J C:\\\\tmp")
        );
    }

    #[test]
    fn a_cut_brief_and_a_missing_one_are_said_so() {
        let (_d, root) = project();
        let long = "é".repeat(dispatchrecord::MOST_BRIEF_BYTES);
        dispatchrecord::open(&root, a_task(&long), at("2026-10-07T12:00:00Z")).unwrap();
        let read = read_for(&root, &BriefOf::Chat(7), open_here).expect("its brief");
        assert_eq!(read.kept, Kept::Cut);
        assert!(long.starts_with(&read.brief) && !read.brief.is_empty());
        assert!(
            read.brief.chars().all(|ch| ch == 'é'),
            "no mark of the store's"
        );

        let (_e, other) = project();
        dispatchrecord::open(&other, a_task(""), at("2026-10-07T12:00:00Z")).unwrap();
        let read = read_for(&other, &BriefOf::Chat(7), open_here).expect("an answer");
        assert_eq!((read.brief.as_str(), read.kept), ("", Kept::Missing));
    }

    #[test]
    fn a_record_whose_names_would_mislead_is_refused_whole() {
        let (_d, root) = project();
        let opened =
            dispatchrecord::open(&root, a_task("Is it up?"), at("2026-10-07T12:00:00Z")).unwrap();
        // Something else that runs as the person rewrote the record's asking chat.
        let path = dispatchrecord::dir(&root).join(format!("{}.json", opened.id));
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("steward 4", "you\u{202e} 4")).unwrap();

        let refused = read_for(&root, &BriefOf::Dispatch(opened.id), open_here).unwrap_err();
        assert!(refused.contains("will not show"), "{refused}");
    }
}
