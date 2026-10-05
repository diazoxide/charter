//! **The collection write seam** (ST-3, the spec on #1221, V91e–g and V91l): how a Settings
//! collection — a list of entries in a settings file, such as `[[forge]]` blocks or
//! `[harness.<name>]` profiles — is added to and taken from.
//!
//! **One module per collection, three functions each**, shared by every caller (the window today,
//! the CLI when it gains the words):
//!
//! - `listed(text) -> Vec<Listed>` lists the entries a file holds, each with an **opaque
//!   identity** ([`Listed::id`]), its label, where its keys are, and the Add form's values that
//!   would write it again;
//! - `add(root, base, &Entry) -> Result<String, Refusal>` checks the **whole entry**, writes it
//!   to the collection's home file and answers the new entry's identity;
//! - `remove(root, base, id) -> Result<Entry, Refusal>` takes the entry called `id` out, **unless
//!   something uses it**: then nothing is written and the refusal names each user. Nothing
//!   cascades. It answers the entry it removed, as `add` takes it.
//!
//! **`base` is the text the entries were drawn from** — the file as the caller read it when the
//! person pressed Add or Remove, never as it stands when the write runs — exactly as
//! [`super::save`] takes it: a file changed on disk since is refused rather than written over,
//! and an identity is looked up in `base`, so a remove can never land on another entry. Every
//! write ends in [`super::save`], so a new entry is also refused for whatever the next read of the
//! file would refuse, in that reader's words, and for a secret — the collection adds rules only a
//! writer can break (a duplicate, one host declared as two kinds) and never repeats a reader's.
//!
//! **A refusal has three parts** ([`Refusal`]), so the window can put each sentence where it
//! belongs: [`Refusal::fields`] under the field of the Add form it is about (the field's name is
//! the entry's own key: `kind`, `host`), [`Refusal::referrers`] under the entry a Remove was
//! refused for, each with the Settings group it is changed in when it is a setting, and
//! [`Refusal::file`] for the whole write (the file moved, a secret, a file a form cannot edit).
//!
//! **Undo is the inverse operation, through these same functions** (D-ST3-i): the Undo of an add
//! is a `remove` of the identity `add` answered, with its reference check, and the Undo of a
//! remove is an `add` of the entry `remove` answered. So an Undo is refused for what the
//! operation would be — a repo catalogued on the host since, say — and never writes around the
//! collection's rules.
//!
//! [`super::forges`] is the first collection. A new one copies its shape: an `Entry` with one
//! `String`/`Vec<String>` per form field, `check` for the field refusals, `listed`, `add`,
//! `remove`, and a `referrers` that says who uses an entry.

/// Why one field of an entry is refused: the entry's key it is about, and the sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRefusal {
    /// The entry's own key: what the Add form's field writes (`kind`, `owner`, `host`).
    pub field: &'static str,
    pub why: String,
}

/// **One entry of a collection, as the window lists it** (D-ST3-j): its identity, which a remove
/// is sent by, what it is called, where its keys are in its file, and the Add form's values that
/// would write it again — what the Undo of its removal adds back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// Opaque to the window, and different once the entry moved or changed, so a write sent for
    /// what was shown is refused rather than made to another entry.
    pub id: String,
    pub label: String,
    /// The path every key of the entry is under: `forge`, then its place.
    pub keys: Vec<super::Step>,
    /// Each field of the Add form, by the entry's key, as the form holds it (a list one entry
    /// per line).
    pub values: Vec<(&'static str, String)>,
}

/// Something that uses an entry, and so stops its removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Referrer {
    /// One sentence naming it, and why it needs the entry.
    pub what: String,
    /// The Settings group it is changed in when it is a setting: the address a deep link
    /// opens (SE-22). `None` for what is not changed in Settings (a catalogued repo).
    pub group: Option<crate::doctor::SettingsGroup>,
}

/// Every reason an add or a remove wrote nothing. At least one part is not empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refusal {
    pub fields: Vec<FieldRefusal>,
    pub referrers: Vec<Referrer>,
    pub file: Vec<String>,
}

impl Refusal {
    /// A refusal of the whole write.
    pub fn file(reasons: Vec<String>) -> Self {
        Self {
            file: reasons,
            ..Self::default()
        }
    }
}
