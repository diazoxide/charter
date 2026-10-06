//! The old spellings the rename's guards look for, and where a text spells one: shared by
//! `no_old_name_is_spelled_outside_the_name_module` (the Rust) and
//! `every_shipped_skill_names_only_purlis` (the skills), so the two hold one rule.

use purlis_core::names::{self, Kind};

/// The old spellings this guard looks for, longest first.
pub fn old_spellings() -> Vec<&'static str> {
    // Every kind is decided here, with no wildcard: a new kind fails to compile until it is.
    let guarded = |kind: Kind| match kind {
        Kind::File
        | Kind::Folder
        | Kind::Marker
        | Kind::KeychainPrefix
        | Kind::EnvPrefix
        | Kind::BranchPrefix
        | Kind::Trailer
        // The plugin's ids, skill namespace and tool prefix: RN-8 (#1266) moved every caller
        // onto them, so an old one spelled anywhere else is a leak (#1284).
        | Kind::PluginId => true,
        // Not yet: RN-9 (bundle id), RN-11a (program name, themes); #1284.
        Kind::BundleId | Kind::Binary | Kind::ThemeId => false,
    };
    let mut out: Vec<&'static str> = names::ALL
        .iter()
        .filter(|n| guarded(n.kind))
        // The marketplace's old name alone, `charter-app`, is also the old app repository's,
        // and prose cites its issues by it (`charter-app#274`). Its one use as a name is inside
        // the installed id, `charter@charter-app`, which is looked for.
        .filter(|n| n.id != names::PLUGIN_MARKETPLACE.id)
        .flat_map(|n| n.reads.iter().chain(n.history.iter()).copied())
        .filter(|s| !s.chars().all(char::is_alphanumeric))
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.len()));
    out.dedup();
    out
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The first old spelling `text` holds, at a word boundary: nothing word-like before it, and
/// nothing word-like after one that ends in a letter (`.charterx` is not `.charter`;
/// `dev.charter.app` is not `.charter`).
///
/// A namespace that ends in a colon (the skill namespace, `charter:`) counts only with a name
/// after it (`charter:handoff`): "charter: the app did not take this" is a sentence that starts
/// with the product's name, which is RN-11a's, not a skill.
pub fn spells<'s>(text: &str, spellings: &[&'s str]) -> Option<&'s str> {
    spellings.iter().copied().find(|s| {
        text.match_indices(s).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + s.len()..].chars().next();
            let open_ended = s.ends_with(|c: char| !c.is_alphanumeric());
            let names_one = !s.ends_with(':') || after.is_some_and(|c| c.is_ascii_lowercase());
            !before.is_some_and(word) && (open_ended || !after.is_some_and(word)) && names_one
        })
    })
}
