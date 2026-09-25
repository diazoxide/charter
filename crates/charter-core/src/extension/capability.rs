//! The capabilities an extension may ask charter for, and the refusal of any this charter does
//! not know (ADR 0053).
//!
//! **A capability is something charter does for an extension that asked for it and was
//! approved.** It is charter's conduct, never a limit on the extension — [`super::RUNS_AS_YOU`]
//! stays true of every one of them. A manifest names the capabilities it uses in a top-level
//! `capabilities` list; each capability that has a shape declares it in `contributes` under the
//! same word. The list is inside the manifest's bytes, so the fingerprint covers it and adding
//! one to an approved extension asks the operator again.
//!
//! **The vocabulary is closed, and it grows one capability at a time**, each in the change that
//! builds it, with its own amendment to ADR 0041's threat model. A word this charter does not
//! know refuses the whole manifest with a sentence naming it. Loading the rest would be an
//! extension the operator approved for less than it asked for, and one that later behaves
//! differently on a charter that does know the word — which is the half-load this refusal
//! exists to prevent.
//!
//! A manifest without the list asks for no capability, and loads exactly as it did before the
//! list existed.

/// One capability, as a manifest names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    /// charter's own test capability, asked for by the `extension-probe` crate. It grants
    /// nothing. **Only a build carrying the plane fence knows it** — which is a test build and
    /// never a release (`crate::fence`) — so the vocabulary's machinery is proven through the
    /// real executor while a release build refuses the word like any other it does not know.
    Probe,
    /// Commands in the palette, named with the extension's name, that open one of its views or
    /// run one of its actions (charter-app#341). Declared under `contributes.palette`.
    Palette,
    /// Actions on the rows of its views, each a request of its own to its program — *run action
    /// `<id>` on `<subject>`* (charter-app#341, protocol 2). Declared under
    /// `contributes.actions`.
    Actions,
    /// Plane paths it writes, as plane-relative globs: handed resolved with each request, and
    /// any change outside them reported after it (charter-app#341, protocol 2). Declared under
    /// `contributes.writes`.
    Writes,
    /// Status badges: values from the extension's facts file, drawn in the app's status bar
    /// and in `charter statusline`'s terminal footer without starting its program
    /// ([`super::facts`], charter-app#340). Its shape is `contributes.badges`.
    Badges,
    /// Extra columns in the bottom bar's repo table, filled per repo from the same facts file,
    /// shown only while the extension is on for that project or workspace (ADR 0048). Its shape
    /// is `contributes.repo-columns`.
    RepoColumns,
    /// Events: charter asks its program one question after each core action the manifest says
    /// it hears — a workspace focused, created, forked or removed, a handoff, a session start, a
    /// plane save ([`super::events`], charter-app#343). Its shape is `contributes.events`.
    Events,
    /// A briefing section: `charter hook sessionstart` asks its program for a section and quotes
    /// it, as data under the extension's name, in every chat's first message
    /// ([`super::briefing`], charter-app#343). Its shape is `contributes.briefing`.
    Briefing,
}

impl Capability {
    /// The word a manifest writes, and what the approval prompt says it does — one row per
    /// capability, so that adding one is one arm here and one in [`Self::known`].
    fn spelled(self) -> (&'static str, &'static str) {
        match self {
            Self::Probe => ("probe", "charter's test capability, which grants nothing"),
            Self::Palette => (
                "palette",
                "adds commands to the palette, named with this extension's name, that open one \
                 of its views or run one of its actions",
            ),
            Self::Actions => (
                "actions",
                "puts actions on the rows of its views; running one starts its program, and \
                 charter asks you first when the action says so, and always before one that \
                 deletes",
            ),
            Self::Writes => (
                "writes",
                "writes to the plane paths listed below; charter hands it those paths with each \
                 question and tells you when anything outside them changed while it answered — \
                 it reports, it does not stop it",
            ),
            Self::Badges => (
                "badges",
                "charter draws values from its facts file as badges in the status bar and the \
                 terminal footer",
            ),
            Self::RepoColumns => (
                "repo-columns",
                "charter draws values from its facts file as columns in the repo table",
            ),
            Self::Events => (
                "events",
                "charter starts its program once after each thing it hears about, when that \
                 thing is already done",
            ),
            Self::Briefing => (
                "briefing",
                "adds text to every chat's first message, quoted as data under its name",
            ),
        }
    }

    /// The first protocol that carries what this capability needs. A manifest naming an earlier
    /// `version` is refused rather than asked a question it was not written to read (ADR 0053).
    pub fn since(self) -> u32 {
        match self {
            Self::Probe | Self::Badges | Self::RepoColumns | Self::Palette => 1,
            // Events and the briefing are request kinds protocol 2 grew in charter-app#343,
            // beside #341's run-action request: 2 was not yet released, so it holds all three.
            Self::Actions | Self::Writes | Self::Events | Self::Briefing => 2,
        }
    }

    /// The word a manifest writes.
    pub fn as_str(self) -> &'static str {
        self.spelled().0
    }

    /// Every capability this build of charter knows, in the order a prompt lists them.
    pub fn known() -> Vec<Self> {
        let mut known = Vec::new();
        if crate::fence::FENCED {
            known.push(Self::Probe);
        }
        known.extend([
            Self::Badges,
            Self::RepoColumns,
            Self::Palette,
            Self::Actions,
            Self::Writes,
            Self::Events,
            Self::Briefing,
        ]);
        known
    }

    /// The capability `word` names, if this build knows it.
    pub fn parse(word: &str) -> Option<Self> {
        Self::known().into_iter().find(|it| it.as_str() == word)
    }

    /// Whether it declares a shape in `contributes`, under its own word (ADR 0053). One that
    /// does must declare one, and a shape under its word is refused without it.
    pub fn has_shape(self) -> bool {
        match self {
            Self::Probe => false,
            Self::Badges
            | Self::RepoColumns
            | Self::Palette
            | Self::Actions
            | Self::Writes
            | Self::Events
            | Self::Briefing => true,
        }
    }

    /// What the approval prompt says about it, in charter's words.
    pub fn asks(self) -> String {
        let (word, what) = self.spelled();
        format!("the capability “{word}” — {what}")
    }
}

/// The most bytes of an unknown word a refusal repeats. A manifest is bounded, but a sentence
/// on the Extensions list is not the place for kilobytes of it.
const MOST_WORD_SHOWN: usize = 64;

/// The capabilities a manifest's top-level `capabilities` asks for, or why charter will not
/// load it.
pub(super) fn declared(value: &serde_json::Value) -> Result<Vec<Capability>, String> {
    let list = value
        .as_array()
        .ok_or("has a 'capabilities' that is not a list of words")?;
    let mut out: Vec<Capability> = Vec::with_capacity(list.len());
    for raw in list {
        let word = raw
            .as_str()
            .ok_or("has a 'capabilities' entry that is not a word")?;
        let Some(capability) = Capability::parse(word) else {
            return Err(unknown(word, &Capability::known()));
        };
        if out.contains(&capability) {
            return Err(format!(
                "asks for the capability \"{}\" twice",
                capability.as_str()
            ));
        }
        out.push(capability);
    }
    Ok(out)
}

/// The refusal of `word`, saying what this build knows instead. `known` is a parameter so that
/// the sentence a release build says — which does not know `probe`, and which no fenced test
/// build can be — is tested too.
fn unknown(word: &str, known: &[Capability]) -> String {
    let known: Vec<&str> = known.iter().map(|it| it.as_str()).collect();
    format!(
        "asks for the capability \"{}\", which this charter does not know, so it loads none of \
         this extension. This charter knows {}. A newer charter may know it.",
        crate::shown::readable(word, MOST_WORD_SHOWN),
        known.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_build_names_the_words_it_knows_and_not_the_test_one() {
        assert_eq!(
            unknown("teleport", &[Capability::Badges, Capability::RepoColumns]),
            "asks for the capability \"teleport\", which this charter does not know, so it \
             loads none of this extension. This charter knows badges, repo-columns. A newer \
             charter may know it."
        );
    }

    #[test]
    fn a_refusal_repeats_at_most_a_line_of_the_word_it_refuses() {
        let said = unknown(&"x".repeat(10_000), &[Capability::Probe]);
        assert!(said.len() < 300, "{said}");
        assert!(said.contains("This charter knows probe."), "{said}");
    }

    #[test]
    fn a_release_build_knows_badges_and_repo_columns() {
        assert_eq!(Capability::parse("badges"), Some(Capability::Badges));
        assert_eq!(
            Capability::parse("repo-columns"),
            Some(Capability::RepoColumns)
        );
    }
}
