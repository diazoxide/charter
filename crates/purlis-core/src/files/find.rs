//! ⌘P: **a file found by its fuzzy name** (FM-7, #1110; #1103, V86 F10), across a scope of
//! branches: one branch, every branch of a project, or every branch of every open project.
//!
//! **What is found is what opens.** The names come from the branch's offered list — what git
//! tracks, and what it does not track and does not ignore ([`super::list`]) — so an ignored
//! build output, a secret file git ignores or git's own `.git` is never a hit. A file git does
//! not ignore is offered, whatever is in it: this is the light editor's line, not a second one.
//! A link is a hit only when it leads to another offered file of the same branch, inside it,
//! which is [`super::open`]'s rule; a submodule or a nested repository is a folder, and only
//! files are hits.
//!
//! **Listed once per palette session, matched per keystroke.** A [`Finder`] keeps each
//! branch's listing the first time a scope names it, so typing costs a match over names
//! already in memory and never another `git ls-files`. The window drops its finder when the
//! palette closes, so a session never shows a listing older than its own opening. A branch
//! past [`LISTED`] files is listed up to it, and the answer says so.
//!
//! **Ranked by `nucleo-matcher`** (helix's matcher, MPL-2.0): fzf's algorithm, with the
//! bonuses for path boundaries its `match_paths` configuration gives. Ties go to the place
//! nearer the front of the scope, then to the shorter path: **the caller puts the focused
//! branch first**, and the app does (`findfiles.rs`), so the nearest of equal hits leads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use unicode_segmentation::UnicodeSegmentation as _;

use super::{Branch, Reader, Refused};

/// The most files of one branch a session lists: a branch of a million generated files is
/// searched through its first 200,000 (in git's order), about 18 MB of names held while the
/// palette is up, and the answer says the rest were not looked at.
pub const LISTED: usize = 200_000;

/// One branch of one project: where a scope looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place<'a> {
    /// The project's root, which the app's registry vouched for.
    pub plane: &'a Path,
    pub branch: Branch<'a>,
}

/// A branch named by owned names, as [`branches`] answers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Named {
    pub ws: String,
    pub repo: String,
    /// No piece is the repo's own folder.
    pub piece: Option<String>,
}

impl Named {
    pub fn branch(&self) -> Branch<'_> {
        match &self.piece {
            Some(piece) => Branch::piece(&self.ws, &self.repo, piece),
            None => Branch::repo(&self.ws, &self.repo),
        }
    }
}

/// One file found: which place of the scope, by its index, its path in that branch, and which
/// of the path's letters the query matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub at: usize,
    pub path: String,
    /// The matched letters, as indices of the path's characters (Unicode scalar values, not
    /// bytes), ascending and each once: what the row marks (#1131).
    pub matched: Vec<u32>,
}

/// What a find answered: the hits, best first; every place of the scope that could not be
/// listed, by its index, with the core's sentence; and every place listed only in part, with
/// the sentence saying how much.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub hits: Vec<Hit>,
    pub refused: Vec<(usize, String)>,
    pub partial: Vec<(usize, String)>,
}

/// Every branch of a project, in the explorer's order: each workspace, each of its repos'
/// own folders, then that repo's pieces. A workspace or repo charter cannot read is left out
/// here — the explorer says why on its own row — so one bad clone does not empty the scope.
pub fn branches(plane: &Path) -> Vec<Named> {
    let workspaces = crate::workspaces::Plane::open(plane)
        .workspaces()
        .unwrap_or_default();
    let mut out = Vec::new();
    for ws in workspaces {
        let Ok(clones) = crate::repos::clones(plane, &ws) else {
            continue;
        };
        for repo in clones.repos {
            out.push(Named {
                ws: ws.clone(),
                repo: repo.name.clone(),
                piece: None,
            });
            for piece in crate::worktree::list(plane, &ws, &repo.name).unwrap_or_default() {
                if piece.prunable.is_none() {
                    out.push(Named {
                        ws: ws.clone(),
                        repo: repo.name.clone(),
                        piece: Some(piece.piece),
                    });
                }
            }
        }
    }
    out
}

/// One branch's offered files, read once.
struct Listing {
    /// The branch's folder, resolved.
    base: PathBuf,
    /// Sorted, as [`super::list`] answers, and no more than the finder's cap.
    paths: Vec<String>,
    /// Said when the branch held more than the cap.
    partial: Option<String>,
}

/// The listings a palette session has read, by project and branch, and the projects' branches.
pub struct Finder {
    listings: HashMap<(PathBuf, Named), Result<Listing, String>>,
    projects: HashMap<PathBuf, Vec<Named>>,
    cap: usize,
    /// What finds and lists each branch: the bounded reader's child, or, with none, this
    /// process.
    reader: Option<Reader>,
}

impl Default for Finder {
    fn default() -> Self {
        Self::listing_at_most(LISTED)
    }
}

impl Finder {
    /// A finder that lists at most `cap` files of each branch: [`LISTED`] by default.
    pub fn listing_at_most(cap: usize) -> Self {
        Self {
            listings: HashMap::new(),
            projects: HashMap::new(),
            cap,
            reader: None,
        }
    }

    /// The same finder, finding and listing each branch by `reader`'s child, so listing a
    /// branch starts no git in this process (#1189, FM-11). Each listing is cut at the cap in
    /// the child, which says how long it was.
    pub fn reading_with(mut self, reader: Reader) -> Self {
        self.reader = Some(reader);
        self
    }

    /// Every branch of `plane`, read once per session.
    pub fn branches(&mut self, plane: &Path) -> &[Named] {
        self.projects
            .entry(plane.to_path_buf())
            .or_insert_with(|| branches(plane))
    }

    /// The files of `scope` whose path matches `query`, best first, at most `most`.
    ///
    /// Each place is listed the first time this finder sees it, and that listing serves every
    /// later query. An empty query finds nothing but still lists the scope, so the window asks
    /// with one as the palette opens and the first keystroke pays only for its match.
    pub fn find(&mut self, scope: &[Place<'_>], query: &str, most: usize) -> Found {
        let mut found = Found::default();
        let pattern = Pattern::new(
            query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut listed: Vec<(usize, &Listing)> = Vec::with_capacity(scope.len());
        let cap = self.cap;
        let reader = self.reader.as_ref();
        for place in scope {
            let key = (place.plane.to_path_buf(), named(place.branch));
            self.listings
                .entry(key)
                .or_insert_with(|| listing(reader, place.plane, place.branch, cap));
        }
        for (at, place) in scope.iter().enumerate() {
            let key = (place.plane.to_path_buf(), named(place.branch));
            match &self.listings[&key] {
                Ok(listing) => {
                    if let Some(said) = &listing.partial {
                        found.partial.push((at, said.clone()));
                    }
                    listed.push((at, listing));
                }
                Err(why) => found.refused.push((at, why.clone())),
            }
        }
        if pattern.atoms.is_empty() || most == 0 {
            return found;
        }

        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let mut buf = Vec::new();
        // (score, place, path index): every match, ranked below.
        let mut scored: Vec<(u32, usize, usize)> = Vec::new();
        for (rank, (_, listing)) in listed.iter().enumerate() {
            for (i, path) in listing.paths.iter().enumerate() {
                if let Some(score) = pattern.score(Utf32Str::new(path, &mut buf), &mut matcher) {
                    scored.push((score, rank, i));
                }
            }
        }
        let path_of = |rank: usize, i: usize| listed[rank].1.paths[i].as_str();
        let better = |a: &(u32, usize, usize), b: &(u32, usize, usize)| {
            b.0.cmp(&a.0)
                .then(a.1.cmp(&b.1))
                .then_with(|| path_of(a.1, a.2).len().cmp(&path_of(b.1, b.2).len()))
                .then_with(|| path_of(a.1, a.2).cmp(path_of(b.1, b.2)))
        };
        // **Only the best are sorted**, a batch at a time: the best `2 × most` matches are
        // selected in linear time and sorted, and the next batch is taken only when links and
        // files gone since the listing left too few. A keystroke matching a hundred thousand
        // paths sorts a hundred of them, not all.
        let batch = most.saturating_mul(2);
        let mut rest: &mut [(u32, usize, usize)] = &mut scored;
        while !rest.is_empty() {
            let take = batch.min(rest.len());
            if take < rest.len() {
                rest.select_nth_unstable_by(take - 1, better);
            }
            let (head, tail) = rest.split_at_mut(take);
            head.sort_unstable_by(better);
            for &(_, rank, i) in head.iter() {
                let (at, listing) = listed[rank];
                let path = &listing.paths[i];
                // Checked as it is shown, so a branch of a hundred thousand files pays for
                // the hits it answers with and not for every match.
                if opens(listing, path) {
                    found.hits.push(Hit {
                        at,
                        path: path.clone(),
                        matched: letters(&pattern, &mut matcher, path),
                    });
                    if found.hits.len() == most {
                        return found;
                    }
                }
            }
            rest = tail;
        }
        found
    }
}

/// The files of `scope` whose path matches `query`, best first, at most `most`: a
/// [`Finder`] used once.
pub fn find(scope: &[Place<'_>], query: &str, most: usize) -> Found {
    Finder::default().find(scope, query, most)
}

/// Which of `path`'s characters `pattern` matched, ascending and each once. Asked only for the
/// hits answered, so a keystroke pays for at most `most` of them.
///
/// nucleo does not count in characters: it reads the path's bytes when every letter of it (a
/// grapheme cluster) starts with an ASCII character, else one unit per letter. Each unit it
/// matched is turned back into all the characters of its letter, so the window, which counts
/// characters, marks the right letters, and marks an accent with the letter it sits on.
fn letters(pattern: &Pattern, matcher: &mut Matcher, path: &str) -> Vec<u32> {
    let mut buf = Vec::new();
    let mut units = Vec::new();
    let haystack = Utf32Str::new(path, &mut buf);
    let by_byte = matches!(haystack, Utf32Str::Ascii(_));
    pattern.indices(haystack, matcher, &mut units);
    let mut marked = if path.is_ascii() {
        units
    } else {
        // Each letter: the byte it starts at, and its characters.
        let mut letters = Vec::new();
        let mut at = 0u32;
        for (byte, letter) in path.grapheme_indices(true) {
            let n = letter.chars().count() as u32;
            letters.push((byte, at..at + n));
            at += n;
        }
        units
            .iter()
            .filter_map(|&unit| {
                let unit = unit as usize;
                let letter = if by_byte {
                    letters
                        .partition_point(|(byte, _)| *byte <= unit)
                        .checked_sub(1)
                } else {
                    Some(unit)
                };
                letter.and_then(|letter| letters.get(letter))
            })
            .flat_map(|(_, chars)| chars.clone())
            .collect()
    };
    // Each atom's indices are appended as it matched them (nucleo's own note on `indices`).
    marked.sort_unstable();
    marked.dedup();
    marked
}

fn named(branch: Branch<'_>) -> Named {
    Named {
        ws: branch.ws.to_string(),
        repo: branch.repo.to_string(),
        piece: branch.piece.map(str::to_owned),
    }
}

fn listing(
    reader: Option<&Reader>,
    plane: &Path,
    branch: Branch<'_>,
    cap: usize,
) -> Result<Listing, String> {
    let read = || -> Result<Listing, Refused> {
        // With a reader, its child finds the folder and lists it, cut at the cap there, so this
        // process starts no git (#1189); with none, both are done here.
        // A branch whose index the child could not read is listed here (#1130).
        let here = |folder: PathBuf| -> Result<(PathBuf, Vec<String>, usize), Refused> {
            let paths = super::files_in(&folder, branch)?;
            let total = paths.len();
            Ok((folder, paths, total))
        };
        let (folder, mut paths, total) = match reader {
            Some(reader) => {
                let folder = super::root(reader, plane, branch)?.path().to_path_buf();
                let never = std::sync::atomic::AtomicBool::new(false);
                match super::offered_by(reader, plane, branch, Some(cap), &never)? {
                    super::Told::Answered(offered) => (folder, offered.files, offered.total),
                    super::Told::Unindexed => here(folder)?,
                    super::Told::Stopped => return Err(super::answered_else()),
                }
            }
            None => here(super::folder_of(plane, branch)?)?,
        };
        let partial = (total > cap).then(|| {
            format!(
                "{} has {} files, and only the first {} of them are searched by name",
                branch.called(),
                total,
                cap
            )
        });
        paths.truncate(cap);
        let base = std::fs::canonicalize(&folder).map_err(|e| Refused::Unreadable {
            what: branch.called().to_string(),
            why: e.to_string(),
        })?;
        Ok(Listing {
            base,
            paths,
            partial,
        })
    };
    read().map_err(|refused| refused.to_string())
}

/// Whether an offered path is a file that opens: a plain file, or a link to another offered
/// file of the same branch — never a folder (a submodule, a nested repository), never a link
/// out of the branch or to what git ignores, never one that is gone since the listing.
fn opens(listing: &Listing, path: &str) -> bool {
    let Ok(relative) = super::inside(path) else {
        return false;
    };
    let at = listing.base.join(relative);
    let Ok(meta) = std::fs::symlink_metadata(&at) else {
        return false;
    };
    if meta.file_type().is_file() {
        return !super::names_git(&at, &listing.base);
    }
    if !meta.file_type().is_symlink() {
        return false;
    }
    let Ok(resolved) = std::fs::canonicalize(&at) else {
        return false;
    };
    resolved != listing.base
        && resolved.is_file()
        && !super::names_git(&resolved, &listing.base)
        && resolved
            .strip_prefix(&listing.base)
            .is_ok_and(|inside| listing.paths.binary_search(&super::slashed(inside)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1189: a finder reading with a reader finds each branch's folder through it: a branch
    /// the reader refuses is refused in the reader's own sentence.
    #[cfg(unix)]
    #[test]
    fn a_finder_reading_with_a_reader_finds_each_folder_through_it() {
        let nowhere = tempfile::tempdir().unwrap();
        let said = "thing in workspace 'alpha' has no branch folder called 'piece'";
        let answer = serde_json::json!({ "Err": said }).to_string();
        let (_answer, reader) = crate::files::tests::reader_answering(&answer);
        let scope = [Place {
            plane: nowhere.path(),
            branch: Branch::piece("alpha", "thing", "piece"),
        }];

        let found = Finder::default().reading_with(reader).find(&scope, "a", 10);

        assert_eq!(found.refused, [(0, said.to_string())]);
    }

    fn matched(query: &str, path: &str) -> Vec<u32> {
        let pattern = Pattern::new(
            query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        letters(
            &pattern,
            &mut Matcher::new(Config::DEFAULT.match_paths()),
            path,
        )
    }

    #[test]
    fn the_letters_a_query_matched_are_named_by_their_place_in_the_path() {
        assert_eq!(matched("pal", "app/src/Palette.tsx"), [8, 9, 10]);
    }

    #[test]
    fn two_words_mark_each_letter_once_in_order() {
        let marked = matched("src pal", "app/src/Palette.tsx");

        assert_eq!(marked, [4, 5, 6, 8, 9, 10]);
    }

    #[test]
    fn a_letter_is_counted_as_a_character_not_a_byte() {
        // `é` is two bytes and one character: the `m` after it is character 5, byte 6.
        assert_eq!(matched("mn", "café/menu.md"), [5, 7]);
    }

    #[test]
    fn a_letter_after_a_combining_mark_is_counted_as_a_character() {
        // `e` and U+0301 are one letter to nucleo and two characters to the window: when every
        // letter starts with an ASCII one, nucleo matches the path's bytes, and the `m` is
        // byte 7 but character 6. The `e` and its accent are marked together.
        assert_eq!(matched("mn", "cafe\u{301}/menu.md"), [6, 8]);
        assert_eq!(matched("ce", "cafe\u{301}/x.md"), [0, 3, 4]);
    }

    #[test]
    fn a_letter_after_a_cluster_is_counted_as_a_character() {
        // A thumb with a skin tone is one letter to nucleo and two characters to the window.
        assert_eq!(matched("mn", "\u{1f44d}\u{1f3fd}/mn.md"), [3, 4]);
    }
}
