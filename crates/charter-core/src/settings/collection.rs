//! **The collection write seam** (ST-3, the spec on #1221, V91e–g and V91l): how a Settings
//! collection — a list of entries in a settings file, such as `[[forge]]` blocks or
//! `[harness.<name>]` profiles — is added to and taken from.
//!
//! **One module per collection, two functions each**, shared by every caller (the window today,
//! the CLI when it gains the words):
//!
//! - `add(root, base, &Entry) -> Result<(), Refusal>` checks the **whole entry** and writes it to
//!   the collection's home file;
//! - `remove(root, base, <which entry>) -> Result<(), Refusal>` takes the entry out, **unless
//!   something uses it**: then nothing is written and the refusal names each user. Nothing
//!   cascades.
//!
//! `base` is the file as the caller read it, exactly as [`super::save`] takes it: a file changed
//! on disk since is refused rather than written over. Every write ends in [`super::save`], so a
//! new entry is also refused for whatever the next read of the file would refuse, in that
//! reader's words, and for a secret — the collection adds rules only a writer can break (a
//! duplicate, one host declared as two kinds) and never repeats a reader's.
//!
//! **A refusal has three parts** ([`Refusal`]), so the window can put each sentence where it
//! belongs: [`Refusal::fields`] under the field of the Add form it is about (the field's name is
//! the entry's own key: `kind`, `host`), [`Refusal::referrers`] under the entry a Remove was
//! refused for, each with the Settings group it is changed in when it is a setting, and
//! [`Refusal::file`] for the whole write (the file moved, a secret, a file a form cannot edit).
//!
//! **Undo is not the seam's**: the one-level Undo of an add or a remove puts the file's text back
//! as it was, through the same [`super::save`] (the driver's raw write), so it restores exactly
//! what was there — comments included — and is refused if the file moved since.
//!
//! [`super::forges`] is the first collection. A new one copies its shape: an `Entry` with one
//! `String`/`Vec<String>` per form field, `check` for the field refusals, `add`, `remove`, and a
//! `referrers` that says who uses an entry.

/// Why one field of an entry is refused: the entry's key it is about, and the sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRefusal {
    /// The entry's own key: what the Add form's field writes (`kind`, `owner`, `host`).
    pub field: &'static str,
    pub why: String,
}

/// Something that uses an entry, and so stops its removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Referrer {
    /// One sentence naming it, and why it needs the entry.
    pub what: String,
    /// The Settings group it is changed in (`project.saving`) when it is a setting: the
    /// address a deep link opens. `None` for what is not changed in Settings (a catalogued repo).
    pub group: Option<&'static str>,
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
