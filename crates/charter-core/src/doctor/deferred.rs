//! The rows this version of charter does not check yet — each still printed, under its own
//! name, as a warning that says so.
//!
//! **Not dropped, and not green.** A doctor that stops printing a row tells its reader the
//! problem that row reported has gone, and one that prints it green says it looked. Both
//! would be false: nothing here looked. So each unchecked row keeps its place in the table
//! and in `--json`, in the "not checked" shape every other doctor row uses, with the reason in
//! the detail and a hint saying its silence means nothing.
//!
//! **No hint points anywhere else.** Earlier builds said the Python charter's `charter doctor`
//! still ran these checks. This app is standalone (ADR 0044, ADR 0045): a row it does not check
//! says so, and sends nobody to another program to find out.
//!
//! The reasons are grouped by what is missing rather than by row, because that is what
//! changes: the day vaults land, four rows stop being deferred at once, and the differential
//! test's list of deferred names is where that has to be said.

use super::Row;

/// What every deferred row says to do about it.
pub(crate) const DEFERRED_HINT: &str = "This charter does not run this check yet, so its \
                                        silence means nothing — it is not saying the check \
                                        passed.";

/// A row for a check this binary does not run, saying why.
pub(crate) fn row(name: &str, why: &str) -> Row {
    Row::warn(name, format!("not checked ({why})"), DEFERRED_HINT)
}

pub(crate) const FORGES: &str = "this version of charter does not check forges yet";
pub(crate) const HARNESS: &str = "this version of charter does not check the harness \
                                  registry's capability ceilings yet";
pub(crate) const FRAME: &str = "this charter has no tmux frame; its window takes the frame's \
                                place";
pub(crate) const GUARD: &str = "this version of charter does not check the guard yet";
pub(crate) const WORKSPACE_LAYER: &str = "this version of charter does not check whether each \
                                          workspace's generated layer is current yet";
pub(crate) const VAULTS: &str = "this version of charter does not check vaults and the \
                                 credentials they hold yet";
pub(crate) const NEWS: &str = "this version of charter does not check release news";
pub(crate) const SHADOWED_DOCS: &str = "this version of charter does not check shadowed \
                                        knowledge docs yet";
