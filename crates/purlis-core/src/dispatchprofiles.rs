//! **The profiles the project lists for a persona's dispatched chats** (#1509, spec #1483;
//! ruling V100-60): `[dispatch.profiles]` of the project's committed file, each persona with
//! the harness profiles a chat dispatched to it may start on (`devops = ["work", "codex"]`).
//!
//! A dispatch chooses a profile three ways: the asking chat names one (`--profile`), else the
//! persona's own definition does, else it is the asking chat's own
//! ([`crate::personaprofile::for_dispatch`]). Where the project lists profiles for the target
//! persona, **the one chosen is one of them, whoever chose it**, or nothing starts.
//!
//! # A list only ever narrows
//!
//! With no list for a persona, every profile the project offers on this machine may be
//! chosen, as before this key existed. A list takes profiles away from that and adds none: a
//! name in it is still only looked up among the profiles the project offers, still needs its
//! approval on this machine, and is still refused where its command switches the harness's
//! permission prompts off ([`crate::dispatchunattended::bypass_refusal`]). So a list a pull
//! brought in can never let a chat start on something it could not start on without it, and
//! needs nobody's acknowledgement.
//!
//! # Why it is in the project's file
//!
//! A persona's own `persona.md` can be edited by a chat running as that persona, and a list
//! kept there could be lifted by the chat it holds. The committed file is a name a sandboxed
//! chat is denied writing, and a brokered write refuses any change under `[dispatch]`
//! (`brokered::guard`), as for the limits and the grants kept beside it. **That denial is the
//! sandbox's**: in a project with the sandbox off, a chat runs as the person and can write the
//! file.
//!
//! # What cannot be read narrows
//!
//! A persona's entry that is not a list lists nothing for it, and a `profiles` that is not a
//! table lists nothing for every persona: no dispatched chat starts until a person fixes the
//! file. So does a project file that is there and cannot be read at all: one that is not
//! TOML, is reached through a link, or is too large. An entry of a list that is not a
//! profile's name is dropped, which narrows too. Each is said in a sentence
//! ([`Listed::refused`]), with the rest of what `[dispatch]` holds that is not read.
//!
//! # A persona that extends another
//!
//! A persona with no list of its own is held to the list of the persona it `extends:`, the
//! nearest one up its chain that has a list ([`Listed::for_chain`]), as it inherits that
//! persona's `profile:`. There is no list for every persona at once: a persona nobody listed,
//! whose chain lists none, has none.
//!
//! # A chat started again
//!
//! A dispatch is held to the list when it starts its chat. That chat may be started again
//! later: when purlis is opened again, by Restart chat, by the restart a grant owes, by Start
//! fresh. The list and the profile's own command may have changed since, so the same two
//! questions are asked again there ([`may_start_again`], [`task_profile_refusal`]).

use std::collections::BTreeMap;
use std::path::Path;

/// The table the lists are kept in: `[dispatch]`.
pub const TABLE: &str = crate::dispatchlimits::TABLE;

/// The key under [`TABLE`] the lists are kept under.
pub const KEY: &str = "profiles";

/// What `[dispatch.profiles]` of a project file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listed {
    /// Each persona that has a list, with the profiles in it, in file order, once each.
    personas: BTreeMap<String, Vec<String>>,
    /// Whether the key is there and is not a table: nothing is listed for any persona.
    unreadable: bool,
    /// Each thing in it that is not read as written, as one sentence.
    pub refused: Vec<String>,
}

impl Listed {
    /// The profiles listed for `persona` by its own name: `None` where the project lists none
    /// for it. `Some` of an empty list is a list that could not be read, or one that holds
    /// nothing: no profile may be chosen.
    pub fn for_persona(&self, persona: &str) -> Option<Vec<String>> {
        if self.unreadable {
            return Some(Vec::new());
        }
        self.personas.get(persona).cloned()
    }

    /// The profiles a persona is held to, by `chain`, its `extends:` chain with itself first
    /// ([`crate::personas::lineage`]): its own list, else the nearest one above it. `None`
    /// where no persona of the chain has one, so every profile the project offers may be
    /// chosen.
    pub fn for_chain(&self, chain: &[String]) -> Option<Vec<String>> {
        if self.unreadable {
            return Some(Vec::new());
        }
        chain.iter().find_map(|who| self.personas.get(who).cloned())
    }

    /// What a project file that is there and could not be read at all lists: nothing, for
    /// every persona.
    fn of_a_file_that_cannot_be_read() -> Self {
        Self {
            unreadable: true,
            refused: vec![
                "the project's file cannot be read, so no profile is listed for any persona \
                 and no chat is dispatched until it is fixed"
                    .to_owned(),
            ],
            ..Self::default()
        }
    }
}

/// **The lists as `text`, the whole project file, writes them** (`None`: no file, which lists
/// none). A file that is not TOML lists nothing for every persona: what cannot be read
/// narrows, whatever another reader of the file does about it.
pub fn listed(text: Option<&str>) -> Listed {
    let mut out = Listed::default();
    let Some(text) = text else {
        return out;
    };
    let Ok(top) = text.parse::<toml::Table>() else {
        return Listed::of_a_file_that_cannot_be_read();
    };
    let Some(value) = top.get(TABLE).and_then(|table| table.get(KEY)) else {
        return out;
    };
    let at = format!("{TABLE}.{KEY}");
    let Some(table) = value.as_table() else {
        out.unreadable = true;
        out.refused.push(format!(
            "{at} is not a table, so no profile is listed for any persona and no chat is \
             dispatched until it is fixed: write the persona, then the profiles its dispatched \
             chats may start on, as devops = [\"work\"]"
        ));
        return out;
    };
    for (persona, profiles) in table {
        let here = format!("{at}.{}", crate::shown::short(persona));
        if !crate::personas::valid_name(persona) {
            out.refused.push(format!(
                "{here} is not a persona's name, so it lists nothing"
            ));
            continue;
        }
        let list = out.personas.entry(persona.clone()).or_default();
        let Some(profiles) = profiles.as_array() else {
            out.refused.push(format!(
                "{here} is not a list of profiles, so no profile is listed for that persona \
                 and no chat is dispatched to it until it is fixed"
            ));
            continue;
        };
        for profile in profiles {
            match profile.as_str().map(str::trim) {
                Some(name) if !name.is_empty() => {
                    if !list.iter().any(|one| one == name) {
                        list.push(name.to_owned());
                    }
                }
                _ => out.refused.push(format!(
                    "{here} holds something that is not a profile's name, which lists nothing"
                )),
            }
        }
    }
    out
}

/// The lists of the project at `root`, read as the sandbox reads the project file: never
/// through a link, and never one too large. A file that is there and is not read that way
/// lists nothing for every persona.
pub fn listed_in(root: &Path) -> Listed {
    match crate::sandbox::read_plane_file(&crate::names::manifest(root)) {
        Ok(text) => listed(text.as_deref()),
        Err(()) => Listed::of_a_file_that_cannot_be_read(),
    }
}

/// The profiles the project at `root` holds `persona` to: its own list, else the nearest one
/// up its `extends:` chain ([`Listed::for_chain`]).
pub fn listed_at(root: &Path, persona: &str) -> Option<Vec<String>> {
    let mut chain = crate::personas::lineage(root, persona);
    // A persona that does not load has no chain; its own name is still asked.
    if chain.is_empty() {
        chain.push(persona.to_owned());
    }
    listed_in(root).for_chain(&chain)
}

/// **Why a chat a dispatch started may not start on `profile` now**, or `None` where it may:
/// the two rules a dispatch is held to when it chooses a profile, asked again of a profile
/// already chosen. `listed` is what the project holds the chat's persona to ([`listed_at`]),
/// and `command` the profile's own, as this machine declares it now.
///
/// - Where the project lists profiles for the persona, `profile` is one of them.
/// - `command` does not switch the harness's permission prompts off
///   ([`crate::dispatchunattended::bypass_in`]).
///
/// The sentence is for the person, who is the one starting it again: what stands in the way,
/// and what they can do.
pub fn started_again_refusal(
    persona: Option<&str>,
    profile: &str,
    command: Option<&[String]>,
    listed: Option<&[String]>,
) -> Option<String> {
    refusal_on(Again::Started, persona, profile, command, listed)
}

/// [`started_again_refusal`], for a persona chat the person started with Ask from a tab: the
/// same two rules, and a sentence that does not say another chat dispatched it.
pub fn asked_again_refusal(
    persona: Option<&str>,
    profile: &str,
    command: Option<&[String]>,
    listed: Option<&[String]>,
) -> Option<String> {
    refusal_on(Again::Asked, persona, profile, command, listed)
}

/// Which way a chat a dispatch started is being started once more: what its refusal calls it
/// and tells its reader to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Again {
    /// The chat is open or recorded, and is started again: a relaunch, a restart, a retry.
    Started,
    /// Its task finished and its row is being reopened: there is no chat to close, and its
    /// report is still there.
    Reopened,
    /// A persona chat the person started with Ask from a tab, started again.
    Asked,
    /// Its task, which the person asked for with Ask from a tab, finished and is reopened.
    ReopenedAsked,
}

/// [`started_again_refusal`], said for `how` it is being started.
fn refusal_on(
    how: Again,
    persona: Option<&str>,
    profile: &str,
    command: Option<&[String]>,
    listed: Option<&[String]>,
) -> Option<String> {
    let shown = crate::shown::short(profile);
    if let (Some(listed), Some(persona)) = (listed, persona)
        && !listed.iter().any(|one| one == profile)
    {
        let persona = crate::shown::short(persona);
        let names: Vec<String> = listed
            .iter()
            .map(|name| format!("'{}'", crate::shown::short(name)))
            .collect();
        let lists = match names.split_last() {
            None => "no profile".to_owned(),
            Some((only, [])) => format!("only {only}"),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        };
        return Some(match how {
            Again::Started => format!(
                "This chat was dispatched as persona '{persona}' on profile '{shown}', and the \
                 project now lists {lists} for that persona, so it was not started again. List \
                 '{shown}' for {persona} under [dispatch.profiles] in the project's file and \
                 start it again, or close it and dispatch the work again."
            ),
            Again::Reopened => format!(
                "This task ran as persona '{persona}' on profile '{shown}', and the project \
                 now lists {lists} for that persona, so it was not reopened. List '{shown}' \
                 for {persona} under [dispatch.profiles] in the project's file and reopen it, \
                 or dispatch the work again. Its report is still here to read."
            ),
            Again::Asked => format!(
                "You started this chat with Ask as persona '{persona}' on profile '{shown}', \
                 and the project now lists {lists} for that persona, so it was not started \
                 again. List '{shown}' for {persona} under [dispatch.profiles] in the \
                 project's file and start it again, or close it and ask again."
            ),
            Again::ReopenedAsked => format!(
                "You started this task with Ask as persona '{persona}' on profile '{shown}', \
                 and the project now lists {lists} for that persona, so it was not reopened. \
                 List '{shown}' for {persona} under [dispatch.profiles] in the project's file \
                 and reopen it, or ask again. Its report is still here to read."
            ),
        });
    }
    let flag = crate::dispatchunattended::bypass_in(command?)?;
    let flag = crate::shown::short(&flag);
    Some(match how {
        Again::Started => format!(
            "This chat was dispatched by another chat, and its profile '{shown}' now starts \
             its harness with the permission prompts off ({flag}), which a dispatched chat \
             never runs with, so it was not started again. Take that out of the profile's \
             command in Settings › Harness and start it again, or close it and dispatch the \
             work again on a profile that asks."
        ),
        Again::Reopened => format!(
            "This task was dispatched by another chat, and its profile '{shown}' now starts \
             its harness with the permission prompts off ({flag}), which a dispatched chat \
             never runs with, so it was not reopened. Take that out of the profile's command \
             in Settings › Harness and reopen it, or dispatch the work again on a profile \
             that asks. Its report is still here to read."
        ),
        Again::Asked => format!(
            "You started this chat with Ask, and its profile '{shown}' now starts its harness \
             with the permission prompts off ({flag}), which a persona chat started that way \
             never runs with, so it was not started again. Take that out of the profile's \
             command in Settings › Harness and start it again, or close it and ask again on a \
             profile that asks."
        ),
        Again::ReopenedAsked => format!(
            "You started this task with Ask, and its profile '{shown}' now starts its harness \
             with the permission prompts off ({flag}), which a persona chat started that way \
             never runs with, so it was not reopened. Take that out of the profile's command \
             in Settings › Harness and reopen it, or ask again on a profile that asks. Its \
             report is still here to read."
        ),
    })
}

/// **Why the task recorded as running as `persona` on `profile` may not start on that profile
/// now**, in the project at `root`, or `None` where it may: [`started_again_refusal`], fed
/// the project's list and the profile's command as they stand. For a caller that starts a
/// dispatched chat again by a road of its own. Reopening a finished task has its own words
/// for the same answer ([`task_reopen_refusal`]).
///
/// A profile this machine no longer declares is not refused here: the start refuses it by
/// name ([`crate::start::ready`]).
pub fn task_profile_refusal(root: &Path, persona: Option<&str>, profile: &str) -> Option<String> {
    task_refusal_on(Again::Started, root, persona, profile)
}

/// **Why the finished task recorded as having run as `persona` on `profile` may not be
/// reopened on that profile now**, or `None` where it may: [`task_profile_refusal`]'s two
/// rules, said for a Reopen. There is no chat to start again or to close: the sentence says
/// the task was not reopened, what to mend, and that its report is still there.
///
/// `by_person` is the record's: a task the person asked for with Ask from a tab is told so,
/// never that another chat dispatched it.
pub fn task_reopen_refusal(
    root: &Path,
    persona: Option<&str>,
    profile: &str,
    by_person: bool,
) -> Option<String> {
    let how = if by_person {
        Again::ReopenedAsked
    } else {
        Again::Reopened
    };
    task_refusal_on(how, root, persona, profile)
}

fn task_refusal_on(
    how: Again,
    root: &Path,
    persona: Option<&str>,
    profile: &str,
) -> Option<String> {
    let listed = persona.and_then(|persona| listed_at(root, persona));
    let set = crate::profiles::current(root);
    refusal_on(
        how,
        persona,
        profile,
        set.get(profile).map(|profile| profile.command.as_slice()),
        listed.as_deref(),
    )
}

/// **Whether `chat`, as the app recorded it, may be started again as it stands**: any chat a
/// dispatch started (its record has `from`), a task or a handoff, on a profile, is asked
/// [`task_profile_refusal`]. A chat the person started is theirs to run as they declared it.
pub fn may_start_again(root: &Path, chat: &crate::reopen::Chat) -> Result<(), String> {
    let (Some(from), Some(profile)) = (&chat.from, chat.profile.as_deref()) else {
        return Ok(());
    };
    // A chat the person asked for from a tab is told so, never that another chat sent it.
    let how = if from.by_person {
        Again::Asked
    } else {
        Again::Started
    };
    match task_refusal_on(how, root, chat.persona.as_deref(), profile) {
        Some(refused) => Err(refused),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Option<Vec<String>> {
        Some(list.iter().map(|name| (*name).to_owned()).collect())
    }

    #[test]
    fn a_persona_the_project_lists_profiles_for_has_exactly_those() {
        let read = listed(Some(
            "[dispatch.profiles]\ndevops = [\"work\", \"codex\", \"work\"]\nqa = [\"work\"]\n",
        ));
        assert_eq!(read.for_persona("devops"), names(&["work", "codex"]));
        assert_eq!(read.for_persona("qa"), names(&["work"]));
        assert_eq!(read.refused, Vec::<String>::new());
    }

    #[test]
    fn a_persona_with_no_list_is_held_to_none() {
        // No file, no table, no key, and a persona the table does not name.
        for text in [
            None,
            Some(""),
            Some("[dispatch]\ndepth = 2\n"),
            Some("[dispatch.profiles]\ndevops = [\"work\"]\n"),
        ] {
            assert_eq!(listed(text).for_persona("steward"), None, "{text:?}");
        }
    }

    #[test]
    fn a_list_that_is_empty_or_cannot_be_read_lists_no_profile_and_says_so() {
        let empty = listed(Some("[dispatch.profiles]\ndevops = []\n"));
        assert_eq!(empty.for_persona("devops"), names(&[]));
        assert_eq!(empty.refused, Vec::<String>::new());

        let word = listed(Some("[dispatch.profiles]\ndevops = \"work\"\n"));
        assert_eq!(word.for_persona("devops"), names(&[]));
        assert_eq!(
            word.refused,
            vec![
                "dispatch.profiles.devops is not a list of profiles, so no profile is listed \
                 for that persona and no chat is dispatched to it until it is fixed"
                    .to_owned()
            ]
        );
        // Another persona's list is read as written.
        assert_eq!(word.for_persona("steward"), None);
    }

    #[test]
    fn an_entry_that_is_not_a_name_is_dropped_which_narrows() {
        let read = listed(Some("[dispatch.profiles]\ndevops = [\"work\", 7, \" \"]\n"));
        assert_eq!(read.for_persona("devops"), names(&["work"]));
        assert_eq!(
            read.refused,
            vec![
                "dispatch.profiles.devops holds something that is not a profile's name, which \
                 lists nothing"
                    .to_owned();
                2
            ]
        );
    }

    #[test]
    fn a_key_that_is_not_a_table_lists_nothing_for_every_persona() {
        let read = listed(Some("[dispatch]\nprofiles = [\"work\"]\n"));
        for persona in ["devops", "steward"] {
            assert_eq!(read.for_persona(persona), names(&[]), "{persona}");
        }
        assert_eq!(read.refused.len(), 1);
        assert!(
            read.refused[0].starts_with("dispatch.profiles is not a table, so no profile is"),
            "{}",
            read.refused[0]
        );
    }

    #[test]
    fn a_project_file_that_cannot_be_read_lists_nothing_for_every_persona() {
        // Not TOML: a merge conflict's markers, half a write.
        let read = listed(Some(
            "<<<<<<< ours\n[dispatch.profiles]\ndevops = [\"work\"]\n",
        ));
        for persona in ["devops", "steward"] {
            assert_eq!(read.for_persona(persona), names(&[]), "{persona}");
            assert_eq!(
                read.for_chain(&[persona.to_owned()]),
                names(&[]),
                "{persona}"
            );
        }
        assert_eq!(
            read.refused,
            vec![
                "the project's file cannot be read, so no profile is listed for any persona \
                 and no chat is dispatched until it is fixed"
                    .to_owned()
            ]
        );
        // No file at all is a project with no lists, as before.
        assert_eq!(listed(None), Listed::default());
    }

    #[cfg(unix)]
    #[test]
    fn a_project_file_reached_through_a_link_lists_nothing_for_every_persona() {
        let root = tempfile::tempdir().expect("a project");
        let elsewhere = root.path().join("elsewhere.toml");
        std::fs::write(&elsewhere, "[dispatch.profiles]\ndevops = [\"work\"]\n").unwrap();
        std::os::unix::fs::symlink(&elsewhere, crate::names::manifest(root.path())).unwrap();
        assert_eq!(listed_at(root.path(), "devops"), names(&[]));
        assert_eq!(listed_at(root.path(), "steward"), names(&[]));
    }

    #[test]
    fn a_persona_with_no_list_of_its_own_is_held_to_the_nearest_one_it_extends() {
        let read = listed(Some(
            "[dispatch.profiles]\nbase = [\"work\"]\nmid = [\"codex\"]\nown = []\n",
        ));
        let chain = |names: &[&str]| -> Vec<String> {
            names.iter().map(|name| (*name).to_owned()).collect()
        };
        // Its parent's, and the nearest where two above it have one.
        assert_eq!(read.for_chain(&chain(&["child", "base"])), names(&["work"]));
        assert_eq!(
            read.for_chain(&chain(&["child", "mid", "base"])),
            names(&["codex"])
        );
        // Its own, an empty one included, answers before any above it.
        assert_eq!(read.for_chain(&chain(&["own", "base"])), names(&[]));
        assert_eq!(read.for_chain(&chain(&["mid", "base"])), names(&["codex"]));
        // A chain nobody listed has none.
        assert_eq!(read.for_chain(&chain(&["child", "other"])), None);
    }

    /// The same through the project's files: the chain is the one the definitions give.
    #[test]
    fn the_chain_is_read_from_the_personas_definitions() {
        let root = tempfile::tempdir().expect("a project");
        for (name, more) in [("base", ""), ("child", "extends: base\n"), ("lone", "")] {
            let dir = root.path().join("personas").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("persona.md"),
                format!("---\nname: {name}\ndescription: d\n{more}---\n# {name}\n"),
            )
            .unwrap();
        }
        std::fs::write(
            crate::names::manifest(root.path()),
            "[dispatch.profiles]\nbase = [\"work\"]\n",
        )
        .unwrap();
        assert_eq!(listed_at(root.path(), "base"), names(&["work"]));
        assert_eq!(listed_at(root.path(), "child"), names(&["work"]));
        assert_eq!(listed_at(root.path(), "lone"), None);
        // A persona that does not load is asked by its own name.
        assert_eq!(listed_at(root.path(), "ghost"), None);
    }

    // ----- a chat a dispatch started, started again -----------------------------------------

    fn words(command: &[&str]) -> Vec<String> {
        command.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn a_dispatched_chat_is_not_started_again_on_a_profile_the_project_stopped_listing() {
        let asks = words(&["claude"]);
        let listed = names(&["work"]).unwrap();
        assert_eq!(
            started_again_refusal(Some("devops"), "codex", Some(&asks), Some(&listed)).as_deref(),
            Some(
                "This chat was dispatched as persona 'devops' on profile 'codex', and the \
                 project now lists only 'work' for that persona, so it was not started again. \
                 List 'codex' for devops under [dispatch.profiles] in the project's file and \
                 start it again, or close it and dispatch the work again."
            )
        );
        // Still listed, or no list at all: it starts.
        assert_eq!(
            started_again_refusal(Some("devops"), "work", Some(&asks), Some(&listed)),
            None
        );
        assert_eq!(
            started_again_refusal(Some("devops"), "codex", Some(&asks), None),
            None
        );
        // A list that holds nothing, or could not be read, starts none.
        let said =
            started_again_refusal(Some("devops"), "work", Some(&asks), Some(&[])).expect("refused");
        assert!(
            said.contains("the project now lists no profile for that persona"),
            "{said}"
        );
    }

    #[test]
    fn a_dispatched_chat_is_not_started_again_on_a_profile_that_now_asks_nobody() {
        let yolo = words(&["claude", "--dangerously-skip-permissions"]);
        assert_eq!(
            started_again_refusal(Some("devops"), "work", Some(&yolo), None).as_deref(),
            Some(
                "This chat was dispatched by another chat, and its profile 'work' now starts \
                 its harness with the permission prompts off \
                 (--dangerously-skip-permissions), which a dispatched chat never runs with, so \
                 it was not started again. Take that out of the profile's command in Settings \
                 › Harness and start it again, or close it and dispatch the work again on a \
                 profile that asks."
            )
        );
        // Listed and asking nobody is refused too: a list adds nothing.
        let listed = names(&["work"]).unwrap();
        assert!(
            started_again_refusal(Some("devops"), "work", Some(&yolo), Some(&listed)).is_some()
        );
        // A chat on no persona has no list, and is still held to the second rule.
        assert!(started_again_refusal(None, "work", Some(&yolo), None).is_some());
        // A profile this machine no longer declares is the start's to refuse by name.
        assert_eq!(
            started_again_refusal(Some("devops"), "work", None, None),
            None
        );
    }

    #[test]
    fn a_reopen_is_refused_by_the_same_two_rules_in_words_that_fit_a_finished_task() {
        // The rules are one function's; only what the reader is told differs. A finished
        // task has no chat to start again or to close, and its report is still there.
        let other = names(&["other"]).unwrap();
        assert_eq!(
            refusal_on(Again::Reopened, Some("devops"), "work", None, Some(&other)).as_deref(),
            Some(
                "This task ran as persona 'devops' on profile 'work', and the project now \
                 lists only 'other' for that persona, so it was not reopened. List 'work' for \
                 devops under [dispatch.profiles] in the project's file and reopen it, or \
                 dispatch the work again. Its report is still here to read."
            )
        );
        let yolo = words(&["claude", "--dangerously-skip-permissions"]);
        assert_eq!(
            refusal_on(Again::Reopened, Some("devops"), "work", Some(&yolo), None).as_deref(),
            Some(
                "This task was dispatched by another chat, and its profile 'work' now starts \
                 its harness with the permission prompts off \
                 (--dangerously-skip-permissions), which a dispatched chat never runs with, so \
                 it was not reopened. Take that out of the profile's command in Settings › \
                 Harness and reopen it, or dispatch the work again on a profile that asks. Its \
                 report is still here to read."
            )
        );
        // Refused exactly where a start again is, and nowhere else.
        for (command, listed) in [
            (None, Some(&other)),
            (Some(&yolo), None),
            (None, None),
            (Some(&yolo), Some(&other)),
        ] {
            let (command, listed) = (command.map(Vec::as_slice), listed.map(Vec::as_slice));
            assert_eq!(
                refusal_on(Again::Reopened, Some("devops"), "work", command, listed).is_some(),
                started_again_refusal(Some("devops"), "work", command, listed).is_some()
            );
            if let Some(said) = refusal_on(Again::Reopened, Some("devops"), "work", command, listed)
            {
                assert!(!said.contains("start it again") && !said.contains("close it"));
            }
        }
    }

    #[test]
    fn a_reopen_of_a_task_the_person_asked_for_is_never_told_another_chat_dispatched_it() {
        let other = names(&["other"]).unwrap();
        let yolo = words(&["claude", "--dangerously-skip-permissions"]);
        for (command, listed) in [(None, Some(&other)), (Some(&yolo), None)] {
            let (command, listed) = (command.map(Vec::as_slice), listed.map(Vec::as_slice));
            let said = refusal_on(
                Again::ReopenedAsked,
                Some("devops"),
                "work",
                command,
                listed,
            )
            .expect("refused as a reopen is");
            assert!(said.starts_with("You started this task with Ask"), "{said}");
            assert!(said.contains("so it was not reopened"), "{said}");
            assert!(
                said.ends_with("Its report is still here to read."),
                "{said}"
            );
            assert!(!said.contains("dispatched by another chat"), "{said}");
        }
    }

    #[test]
    fn a_chat_the_person_asked_for_is_never_told_another_chat_dispatched_it() {
        let other = names(&["other"]).unwrap();
        let yolo = words(&["claude", "--dangerously-skip-permissions"]);
        for (command, listed) in [(None, Some(&other)), (Some(&yolo), None)] {
            let (command, listed) = (command.map(Vec::as_slice), listed.map(Vec::as_slice));
            let said = asked_again_refusal(Some("devops"), "work", command, listed)
                .expect("refused as a dispatched chat is");
            assert!(said.starts_with("You started this chat with Ask"), "{said}");
            assert!(!said.contains("dispatched by another chat"), "{said}");
            assert!(!said.contains("This chat was dispatched"), "{said}");
            assert!(started_again_refusal(Some("devops"), "work", command, listed).is_some());
        }
    }

    #[test]
    fn only_a_chat_a_dispatch_started_is_asked_again() {
        use crate::reopen::{Chat, HandedFrom, Mode, Owed};
        let root = tempfile::tempdir().expect("a project");
        std::fs::write(
            crate::names::manifest(root.path()),
            "[dispatch.profiles]\ndevops = [\"work\"]\n",
        )
        .unwrap();
        let dispatched = Chat {
            profile: Some("codex".to_owned()),
            persona: Some("devops".to_owned()),
            from: Some(HandedFrom {
                chat: 1,
                name: "steward 1".to_owned(),
                workspace: crate::active::Place::Workspace("alpha".to_owned()),
                report: Owed::Due,
                mode: Mode::Task,
                depth: 1,
                root: None,
                by_person: false,
            }),
            ..Chat::default()
        };
        let refused = may_start_again(root.path(), &dispatched).expect_err("refused");
        assert!(refused.contains("now lists only 'work'"), "{refused}");
        assert_eq!(
            task_profile_refusal(root.path(), Some("devops"), "codex"),
            Some(refused)
        );
        // On a listed profile it starts.
        let listed = Chat {
            profile: Some("work".to_owned()),
            ..dispatched.clone()
        };
        assert_eq!(may_start_again(root.path(), &listed), Ok(()));
        // A chat the person started is theirs to run on any profile they declared.
        let started = Chat {
            from: None,
            ..dispatched.clone()
        };
        assert_eq!(may_start_again(root.path(), &started), Ok(()));
        // And one on no profile has none to ask about.
        let plain = Chat {
            profile: None,
            ..dispatched
        };
        assert_eq!(may_start_again(root.path(), &plain), Ok(()));
    }

    #[test]
    fn a_name_that_is_not_a_personas_lists_nothing() {
        let read = listed(Some("[dispatch.profiles]\n\"../x\" = [\"work\"]\n"));
        assert_eq!(read.for_persona("../x"), None);
        assert_eq!(
            read.refused,
            vec!["dispatch.profiles.../x is not a persona's name, so it lists nothing".to_owned()]
        );
    }
}
