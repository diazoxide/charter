//! The rows this binary does not check yet — each still printed, under its own name, as a
//! warning that says so.
//!
//! **Not dropped, and not green.** A doctor that stops printing a row tells its reader the
//! problem that row reported has gone, and one that prints it green says it looked. Both
//! would be false: nothing here looked. So each unported check keeps its place in the table
//! and in `--json`, in Python's own "not checked" shape, with the reason in the detail and a
//! hint saying where the check still runs.
//!
//! The reasons are grouped by what is missing rather than by row, because that is what
//! changes: the day vaults are ported, four rows stop being deferred at once, and the
//! differential test's list of deferred names is where that has to be said.

use super::Row;

/// What every deferred row says to do about it.
pub(crate) const DEFERRED_HINT: &str = "This charter does not run this check yet, so its \
                                        silence means nothing — it is not saying the check \
                                        passed. The Python charter's `charter doctor` still \
                                        runs it.";

/// A row for a check this binary does not run, saying why.
pub(crate) fn row(name: &str, why: &str) -> Row {
    Row::warn(name, format!("not checked ({why})"), DEFERRED_HINT)
}

/// Python's first row reports its own interpreter. This binary has none, and the Python
/// charter the plane's hooks still run is the one that needs it.
pub(crate) fn python3() -> Row {
    row("python3", PYTHON)
}

pub(crate) const PYTHON: &str = "this charter is not written in Python; the Python charter \
                                 this plane's hooks still run needs Python 3.11+, and its own \
                                 doctor reports whether it has it";
pub(crate) const FORGES: &str = "forges are not ported to this charter yet";
pub(crate) const GIT_POLICY: &str = "the one-credential git policy is not ported to this \
                                     charter yet";
pub(crate) const HARNESS: &str = "the harness registry's capability ceilings are not ported \
                                  to this charter yet";
pub(crate) const FRAME: &str = "the tmux frame is the Python charter's, and this charter does \
                                not check it";
pub(crate) const GUARD: &str = "the guard is not ported to this charter yet (M3)";
pub(crate) const WORKSPACE_LAYER: &str = "whether each workspace's generated layer is current \
                                          is not ported to this charter yet";
pub(crate) const CHANGES: &str = "cross-repo changes are not ported to this charter yet";
pub(crate) const VAULTS: &str = "vaults and the credentials they hold are not ported to this \
                                 charter yet (M3)";
pub(crate) const PERSONA_LINT: &str = "persona lint is not ported to this charter yet";
pub(crate) const NEWS: &str = "charter's release news is not ported to this charter yet";
pub(crate) const ASK_RULES: &str = "whether an ask rule shadows a persona's tools is not \
                                    ported to this charter yet";
pub(crate) const HANDOFF_GATE: &str = "the handoff gate is not ported to this charter yet";
pub(crate) const SHADOWED_DOCS: &str = "charter's shipped knowledge is not ported to this \
                                        charter yet";
pub(crate) const PLUGIN: &str = "the Claude Code plugin's install and version checks are not \
                                 ported to this charter yet";
