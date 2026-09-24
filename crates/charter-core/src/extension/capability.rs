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
}

impl Capability {
    /// The word a manifest writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Probe => "probe",
        }
    }

    /// Every capability this build of charter knows, in the order a prompt lists them.
    pub fn known() -> Vec<Self> {
        let mut known = Vec::new();
        if crate::fence::FENCED {
            known.push(Self::Probe);
        }
        known
    }

    /// The capability `word` names, if this build knows it.
    pub fn parse(word: &str) -> Option<Self> {
        Self::known().into_iter().find(|it| it.as_str() == word)
    }

    /// What the approval prompt says about it, in charter's words.
    pub fn asks(self) -> String {
        let what = match self {
            Self::Probe => "charter's test capability, which grants nothing",
        };
        format!("the capability “{}” — {what}", self.as_str())
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
            let known: Vec<&str> = Capability::known()
                .into_iter()
                .map(Capability::as_str)
                .collect();
            return Err(format!(
                "asks for the capability \"{}\", which this charter does not know, so it loads \
                 none of this extension. {} A newer charter may know it.",
                crate::shown::readable(word, MOST_WORD_SHOWN),
                if known.is_empty() {
                    "This charter grants no capabilities yet.".to_owned()
                } else {
                    format!("This charter knows {}.", known.join(", "))
                }
            ));
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
