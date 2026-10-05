//! **The fix registry** (FX-1, V91d, V91p): the doctor findings charter can fix itself, each
//! under one **fix id**, and the one entry point that applies a fix by that id.
//!
//! A row that charter can fix carries its id ([`super::Row::fix`]). `charter doctor --fix
//! [<id>]` and the Doctor dialog's Fix button both call [`apply`], so a fix is written once and
//! the terminal and the window can never disagree about what it does.
//!
//! # What is never a fix
//!
//! **Removing a git index lock** (V91p). Charter's rule is that it never removes a lock: a lock
//! that looks crashed can belong to a git that is still writing, and deleting it is how an
//! index is corrupted. The `index lock` row says how to check and leaves the removal to the
//! operator, so it carries no id, and no id here could name it.
//!
//! # What a fix answers
//!
//! What it changed, line by line, or why it refused ([`Fixed`]). A refusal is decided before
//! anything is written: a fix on a project this charter may not write (FR-24), or where there
//! is no project, writes nothing and says why.

use std::path::Path;

use crate::scaffold::Say;

/// A fix charter can make, by the id every surface names it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FixId {
    /// `charter reinit`: adds what the project is missing and never removes or replaces your
    /// content. It creates missing baseline folders and appends missing `.gitignore` lines. It
    /// also rewrites charter's own managed block in `.gitattributes` and merges charter's
    /// entries into `.claude/settings.json`. Offered by the `schema` row when a baseline folder
    /// is missing.
    Reinit,
}

impl FixId {
    /// Every fix, in the order `charter doctor --fix` applies them.
    pub const ALL: [FixId; 1] = [FixId::Reinit];

    /// The id, as `charter doctor --fix <id>`, `--json` and the window spell it.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Reinit => "reinit",
        }
    }

    /// The fix an id names, or `None` for an id that names no fix.
    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|fix| fix.id() == id)
    }
}

impl std::fmt::Display for FixId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

/// What applying a fix came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fixed {
    /// The fix ran. `said` is what it changed, in the words its command prints, including
    /// "nothing to do" when nothing needed changing. `complete` is false when part of it could
    /// not be done; `said` names that part.
    Ran { said: Vec<String>, complete: bool },
    /// Refused before anything was written, and why.
    Refused(String),
}

impl Fixed {
    /// Whether the fix did everything it was asked to.
    pub fn complete(&self) -> bool {
        matches!(self, Self::Ran { complete: true, .. })
    }

    /// Every line, as a terminal prints it.
    pub fn lines(&self) -> Vec<String> {
        match self {
            Self::Ran { said, .. } => said.clone(),
            Self::Refused(why) => vec![format!("✗ refused: {why}")],
        }
    }
}

/// Apply the fix `id` to the project at `root`.
///
/// The one entry point: the CLI and the window both call it. It refuses, writing nothing, where
/// there is no project and on a project this charter may not write.
pub fn apply(root: &Path, id: FixId) -> Fixed {
    if let Some(why) = refusal(root) {
        return Fixed::Refused(why);
    }
    match id {
        FixId::Reinit => ran(crate::scaffold::reinit(&crate::plane::Place {
            root: root.to_path_buf(),
            is_plane: true,
        })),
    }
}

/// Why no fix may write at `root`, or `None` when one may.
fn refusal(root: &Path) -> Option<String> {
    // A `charter.toml` that is a link out of the project: reinit's own containment gate refuses
    // it with the words that name the link, and writes nothing.
    if crate::scaffold::manifest_escapes(root) {
        return None;
    }
    if !root.join(crate::plane::MANIFEST).is_file() {
        return Some(format!(
            "no project at {} (it has no charter.toml), so there is nothing to fix",
            super::fsx::path_field(root)
        ));
    }
    match crate::compat::read(root) {
        crate::compat::Compat::Writable => None,
        crate::compat::Compat::ReadOnly(why) => Some(why.to_string()),
    }
}

fn ran(outcome: crate::scaffold::Outcome) -> Fixed {
    // A command that wrote nothing and only said why is a refusal, whatever it was named.
    let refused = outcome.code != 0 && outcome.said.iter().all(|s| matches!(s, Say::Err(_)));
    if refused {
        let why: Vec<&str> = outcome
            .said
            .iter()
            .filter_map(|s| match s {
                Say::Err(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        return Fixed::Refused(why.join(" "));
    }
    Fixed::Ran {
        said: outcome.said.iter().map(Say::marked).collect(),
        complete: outcome.code == 0,
    }
}
