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
//! file. An entry of a list that is not a profile's name is dropped, which narrows too. Each
//! is said in a sentence ([`Listed::refused`]), with the rest of what `[dispatch]` holds that
//! is not read. A persona is listed by its own name: one that `extends:` a listed persona is
//! not held to its parent's list.

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
    /// The profiles listed for `persona`: `None` where the project lists none for it, so every
    /// profile it offers may be chosen. `Some` of an empty list is a list that could not be
    /// read, or one that holds nothing: no profile may be chosen.
    pub fn for_persona(&self, persona: &str) -> Option<Vec<String>> {
        if self.unreadable {
            return Some(Vec::new());
        }
        self.personas.get(persona).cloned()
    }
}

/// **The lists as `text`, the whole project file, writes them** (`None`: no file). A file that
/// is not TOML lists nothing: every other reader of it refuses it already.
pub fn listed(text: Option<&str>) -> Listed {
    let mut out = Listed::default();
    let Some(top) = text.and_then(|text| text.parse::<toml::Table>().ok()) else {
        return out;
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

/// The profiles the project at `root` lists for `persona`, read as the sandbox reads the
/// project file: never through a link, and none from a file that cannot be read.
pub fn listed_at(root: &Path, persona: &str) -> Option<Vec<String>> {
    let text = crate::sandbox::read_plane_file(&crate::names::manifest(root))
        .ok()
        .flatten();
    listed(text.as_deref()).for_persona(persona)
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
    fn a_name_that_is_not_a_personas_lists_nothing() {
        let read = listed(Some("[dispatch.profiles]\n\"../x\" = [\"work\"]\n"));
        assert_eq!(read.for_persona("../x"), None);
        assert_eq!(
            read.refused,
            vec!["dispatch.profiles.../x is not a persona's name, so it lists nothing".to_owned()]
        );
    }
}
