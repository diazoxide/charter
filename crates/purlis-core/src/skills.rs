//! charter's own skills, and how each harness is handed them (ADR 0063).
//!
//! The skills are one directory, `skills/` inside the bundled plugin ([`crate::plugin`]), with a
//! folder per skill holding its `SKILL.md`. That directory is the one source: every harness is
//! handed the same files, and none gets a copy.
//!
//! **One neutral model, one adapter per harness.** A skill is a [`Skill`]: its name, the
//! sentence that says when to use it, and where its `SKILL.md` is. How a chat comes to know them
//! is the harness's [`Route`], chosen in [`crate::harness::Harness::state_hooks`] for that chat
//! alone. Nothing is written into a harness's config, and nothing into the operator's tree.

use std::path::{Path, PathBuf};

/// Where the skills sit inside the bundled plugin directory. Claude Code reads a plugin's
/// `skills/` itself, which is why it is this name.
pub const DIR_IN_BUNDLE: &str = "skills";

/// The variable a chat whose harness cannot load charter's skills is started with: the skills
/// directory, which its `SessionStart` briefing then lists ([`listed_from`]). Set by the app for
/// such a chat alone, so a harness that loads the skills itself is never briefed on them twice.
pub const LISTED_ENV: &str = "PURLIS_SKILLS_DIR";

/// One of charter's skills, as a chat is told about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// The frontmatter's `name`.
    pub name: String,
    /// The frontmatter's `description`: what the skill does and when to use it.
    pub description: String,
    /// Its `SKILL.md`, which the model reads when the skill applies.
    pub path: PathBuf,
}

/// How a harness's chat comes to know charter's skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// The harness loads the bundled plugin, `skills/` included, for the chat alone.
    Plugin,
    /// The harness is told the skills directory for the chat alone, and discovers the skills in
    /// it as its own.
    Config,
    /// The harness has no way to take a skills directory for one chat, so the chat's
    /// `SessionStart` briefing lists each skill with the path to its `SKILL.md`.
    Briefing,
}

/// The skills directory inside the bundled plugin at `plugin`, when it is there.
pub fn in_bundle(plugin: &Path) -> Option<PathBuf> {
    Some(plugin.join(DIR_IN_BUNDLE)).filter(|dir| dir.is_dir())
}

/// Every skill in `dir`, by name. A folder with no `SKILL.md`, or one whose frontmatter names no
/// `name` or no `description`, is not a skill any harness would load, and is left out.
///
/// The frontmatter is read as a persona's is ([`crate::personas::frontmatter`]): one line per
/// key, which is how every skill charter ships is written.
pub fn read(dir: &Path) -> Vec<Skill> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut skills: Vec<Skill> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path().join(FILE);
            let text = std::fs::read_to_string(&path).ok()?;
            let pairs = crate::personas::frontmatter(&text);
            let value = |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
                    .filter(|v| !v.is_empty())
            };
            Some(Skill {
                name: value("name")?,
                description: value("description")?,
                path,
            })
        })
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// The briefing section that lists `skills`, or none when there are none.
///
/// It is what a harness with skills of its own shows its model: each skill's name, when to use
/// it, and the file to read when it applies. The model reads that file with its own tools.
pub fn listing(skills: &[Skill]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    let mut text = String::from(
        "⬢ **purlis's skills** — this harness does not load them itself, so they are listed \
         here. When a request matches one, read its `SKILL.md` and follow it before acting; a \
         prompt that names a skill by its name means this list.",
    );
    for skill in skills {
        text.push_str(&format!(
            "\n- `{}` — {} (`{}`)",
            skill.name,
            skill.description,
            skill.path.display()
        ));
    }
    Some(text)
}

/// The listing of the skills in the directory [`LISTED_ENV`] names, or none where it is unset.
pub fn listed_from(env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let dir = env(LISTED_ENV).filter(|dir| !dir.is_empty())?;
    listing(&read(Path::new(&dir)))
}

/// A skill's file inside its folder.
const FILE: &str = "SKILL.md";

#[cfg(test)]
#[path = "skills_tests.rs"]
mod tests;
