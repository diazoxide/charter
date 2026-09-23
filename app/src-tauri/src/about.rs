//! What this charter is, and what the version it brought brought — the title bar's
//! *About charter*.
//!
//! The operator asked for it in as many words: *"About Charter — that will open news of
//! charter e.g. current version News"*.
//!
//! **It reads the corpus the rest of charter reads, and there is no second one.**
//! [`charter_core::news`] is compiled into the binary by `build.rs`, so `charter news`, the
//! Release body, the plane pin's dialog (`crate::pin`) and this are four views of one set of
//! files. A dialog that carried its own "what's new" text would be a fifth copy of the
//! release notes with nobody to keep it honest — which is the drift the news module's own
//! docstring exists to prevent.
//!
//! **The version is the one the corpus names, not the crate's.** `news::shipped_version` is
//! the newest released entry's version, and [`charter_core::news`] argues at length why that
//! is the honest answer for a binary whose `CARGO_PKG_VERSION` is the workspace's `0.1.0` —
//! the corpus travels with the code that implements it, so the newest entry a build ships is
//! the newest thing that build brought. The same number the status line's pin item already
//! reports as `brought` (`crate::pin::PinReport`), from the same call.
//!
//! **So the list is never empty.** `shipped_version` is *derived from* the entries, so the
//! version it answers with has at least one, by construction. There is no "this version
//! brought nothing" state to draw, and the window does not pretend there is one.
//!
//! **Nothing here is about a plane.** The pin is (`crate::pin`): it asks what THIS control
//! plane pins against what this charter brought, and it lives on the status line next to the
//! project it is about. This is a fact about the binary, so it lives on the window's chrome
//! and is asked without a plane.

use charter_core::news;

/// One entry of the version's news, as the About dialog reads it.
///
/// Headline and body, and nothing else. `check:`/`adopt:` are about whether a PLANE has taken
/// an entry up — `charter news` answers that, with a plane to ask it of — and this dialog has
/// no plane in its hand. Reporting *unchecked* against every line would be the window adding a
/// column of the same word three times.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Note {
    /// The entry's one-line headline, as its author wrote it.
    pub headline: String,
    /// The prose under the frontmatter. Empty for an entry that is only a headline.
    pub body: String,
}

/// What charter says about itself.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct About {
    /// The release this charter brought (`news::shipped_version`).
    pub version: String,
    /// What that version brought, in the corpus's own order — which is
    /// [`charter_core::news::all`]'s, so a `lead:` entry is first here exactly as it is in the
    /// Release body and in `charter news`. Never empty; see this module's docstring.
    pub notes: Vec<Note>,
}

/// The About dialog's content.
///
/// A plain function of the compiled-in corpus: no plane, no disk, no state. That is why it
/// takes nothing — a dialog that needed a project open could not be on the window's chrome,
/// and the window holds no project for the first moments of every launch.
#[tauri::command]
#[specta::specta]
pub fn about_charter() -> About {
    let version = news::shipped_version();
    About {
        notes: news::for_version(&version)
            .into_iter()
            .map(|entry| Note {
                headline: entry.headline,
                body: entry.body,
            })
            .collect(),
        version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_names_the_release_this_build_brought() {
        let about = about_charter();

        assert_eq!(about.version, news::shipped_version());
        assert_ne!(about.version, "", "the corpus names a released version");
    }

    #[test]
    fn the_version_it_names_always_has_news_because_it_is_derived_from_the_news() {
        // The invariant the dialog leans on: `shipped_version` is the newest RELEASED entry's
        // version, so asking the corpus for that version's entries cannot come back empty.
        // If this ever fails, the About dialog has an empty state that nothing draws.
        let about = about_charter();

        assert!(!about.notes.is_empty(), "{about:?}");
    }

    #[test]
    fn every_note_carries_the_headline_its_entry_was_written_with() {
        let about = about_charter();
        let entries = news::for_version(&about.version);

        assert_eq!(about.notes.len(), entries.len());
        for (note, entry) in about.notes.iter().zip(entries.iter()) {
            assert_eq!(note.headline, entry.headline);
            assert_eq!(note.body, entry.body);
        }
    }

    #[test]
    fn it_lists_no_entry_from_another_version() {
        // The pin dialog lists a RANGE (`news::between`); this lists ONE version. A dialog
        // titled "what 0.62.1 brought" that carried 0.62.0's fifty entries would be the pin
        // item under another name.
        let about = about_charter();
        let mine: Vec<String> = news::for_version(&about.version)
            .into_iter()
            .map(|e| e.headline)
            .collect();

        for note in &about.notes {
            assert!(mine.contains(&note.headline), "{note:?}");
        }
        assert!(
            news::released()
                .iter()
                .filter(|e| e.version != about.version)
                .all(|e| !about.notes.iter().any(|n| n.headline == e.headline)),
        );
    }
}
