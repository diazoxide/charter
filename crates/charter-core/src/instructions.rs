//! The plane's instructions a chat reads once, when it starts — and whether they have changed
//! since (charter#369).
//!
//! A harness reads `CLAUDE.md`, its settings and its sub-agent definitions when a session
//! starts, and charter's briefing quotes the persona's charter then too. A chat already running
//! goes on with what it read, so when one of those files changes it is running on the old
//! ones until it is started fresh. The Python charter said so in the transcript on the next
//! prompt ("control plane updated"); the ruling on charter#369 moved it into the window, as a
//! mark on the chat's tab.
//!
//! **Only what a running chat cannot pick up.** Skills, docs and memory are read when they are
//! used, so a change to them changes nothing a chat could act on, and a mark for it would be
//! one the operator learns to ignore. Hence the short list in [`Stamp::of`].
//!
//! **What is on disk, not what is committed.** The chat read the file, not a commit, and an
//! edit is in force for the next chat to start the moment it is written.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// The files at the plane root a harness reads at start.
const AT_ROOT: [&str; 3] = ["CLAUDE.md", "AGENTS.md", ".claude/settings.json"];

/// What the plane's start-time instructions were at one moment: each file, by its path from the
/// plane root, and a digest of what it held.
///
/// The digest is only ever compared with another taken by the same program, which is the one
/// thing it is good for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stamp(BTreeMap<String, u64>);

impl Stamp {
    /// The instructions of the plane at `root` as they are on disk now: `CLAUDE.md`,
    /// `AGENTS.md`, `.claude/settings.json`, every `.claude/agents/*.md`, and every persona's
    /// charter — `personas/<name>/persona.md`, or the older flat `personas/<name>.md`. A file
    /// that is not there is simply not in it.
    pub fn of(root: &Path) -> Self {
        let mut files = BTreeMap::new();
        let mut take = |relative: String| {
            if let Ok(bytes) = std::fs::read(root.join(&relative)) {
                let mut digest = std::collections::hash_map::DefaultHasher::new();
                bytes.hash(&mut digest);
                files.insert(relative, digest.finish());
            }
        };
        for file in AT_ROOT {
            take(file.to_owned());
        }
        for name in names_in(&root.join(".claude/agents")) {
            if name.ends_with(".md") {
                take(format!(".claude/agents/{name}"));
            }
        }
        for name in names_in(&root.join("personas")) {
            if name.ends_with(".md") {
                take(format!("personas/{name}"));
            } else {
                take(format!("personas/{name}/persona.md"));
            }
        }
        Self(files)
    }

    /// The files a chat started as `persona` read that differ between `earlier` and this one
    /// — changed, added or gone — by their path from the plane root, sorted. Empty when nothing
    /// it read at its start moved.
    ///
    /// **Its own persona's charter, and no other.** The briefing quotes the persona the chat
    /// started as; a change to another persona's charter is nothing this chat read, and marking
    /// every open chat for it would be the noise the mark must not be. (Another persona's
    /// sub-agent definition, `.claude/agents/<name>.md`, IS read at start, and is kept.)
    pub fn changed_since(&self, earlier: &Stamp, persona: Option<&str>) -> Vec<String> {
        let its_own = |file: &str| {
            let Some(rest) = file.strip_prefix("personas/") else {
                return true;
            };
            persona.is_some_and(|name| {
                rest == format!("{name}/persona.md") || rest == format!("{name}.md")
            })
        };
        let mut changed: Vec<String> = self
            .0
            .iter()
            .filter(|(file, digest)| its_own(file) && earlier.0.get(*file) != Some(digest))
            .map(|(file, _)| file.clone())
            .chain(
                earlier
                    .0
                    .keys()
                    .filter(|file| its_own(file) && !self.0.contains_key(*file))
                    .cloned(),
            )
            .collect();
        changed.sort();
        changed
    }
}

/// The names in `dir`, or none. A name that is not UTF-8 is not one charter writes.
fn names_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, file: &str, text: &str) {
        let at = root.join(file);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }

    fn a_plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "CLAUDE.md", "be kind\n");
        write(
            dir.path(),
            "personas/steward/persona.md",
            "---\nname: steward\n---\n",
        );
        dir
    }

    #[test]
    fn nothing_moved_is_nothing_changed() {
        let plane = a_plane();
        let started = Stamp::of(plane.path());

        assert_eq!(
            Stamp::of(plane.path()).changed_since(&started, Some("steward")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn each_file_a_chat_reads_at_start_is_named_when_it_changes() {
        let plane = a_plane();
        let started = Stamp::of(plane.path());
        write(plane.path(), "CLAUDE.md", "be kinder\n");
        write(plane.path(), "AGENTS.md", "new\n");
        write(plane.path(), ".claude/settings.json", "{}");
        write(plane.path(), ".claude/agents/ops.md", "ops\n");
        std::fs::remove_file(plane.path().join("personas/steward/persona.md")).unwrap();

        assert_eq!(
            Stamp::of(plane.path()).changed_since(&started, Some("steward")),
            [
                ".claude/agents/ops.md",
                ".claude/settings.json",
                "AGENTS.md",
                "CLAUDE.md",
                "personas/steward/persona.md",
            ]
        );
    }

    #[test]
    fn another_personas_charter_is_not_what_this_chat_read() {
        let plane = a_plane();
        write(plane.path(), "personas/ops.md", "---\nname: ops\n---\n");
        let started = Stamp::of(plane.path());
        write(
            plane.path(),
            "personas/ops.md",
            "---\nname: ops\nrole: Ops\n---\n",
        );
        write(
            plane.path(),
            "personas/steward/persona.md",
            "---\nrole: Steward\n---\n",
        );

        let now = Stamp::of(plane.path());
        assert_eq!(
            now.changed_since(&started, Some("steward")),
            ["personas/steward/persona.md"]
        );
        assert_eq!(
            now.changed_since(&started, Some("ops")),
            ["personas/ops.md"]
        );
        assert_eq!(now.changed_since(&started, None), Vec::<String>::new());
    }

    #[test]
    fn what_a_chat_reads_when_it_uses_it_is_not_a_change() {
        let plane = a_plane();
        let started = Stamp::of(plane.path());
        write(plane.path(), "personas/steward/memory/a.md", "remembered\n");
        write(plane.path(), ".claude/skills/x/SKILL.md", "a skill\n");
        write(plane.path(), "docs/how.md", "docs\n");
        write(plane.path(), "workspaces/ops/workspace.md", "vision\n");
        write(plane.path(), ".claude/agents/notes.txt", "not an agent\n");

        assert_eq!(
            Stamp::of(plane.path()).changed_since(&started, Some("steward")),
            Vec::<String>::new()
        );
    }
}
