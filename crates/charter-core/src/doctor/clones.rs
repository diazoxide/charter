//! `workspace clones`: clones behind their upstream, in EVERY workspace (#156).
//!
//! `sync` defaults to the active workspace, so a clone seven commits behind in another one
//! was reported by nothing — and a stale clone is not inert, it is what a session reads if it
//! happens to work there.
//!
//! **Read from remote-tracking refs; never fetched.** This runs from the SessionStart hook
//! and must not reach the network. It can therefore under-report — origin moving is
//! invisible until something fetches — but it can never invent staleness.

use super::fsx::{self, Unread};
use super::git::git_in;
use super::{Doctor, Row, Status};
use crate::memstore::py_strip;

pub(super) fn workspace_clones(d: &Doctor) -> Row {
    const NAME: &str = "workspace clones";
    if !d.has_plane {
        return Row::ok(NAME, "no control plane found");
    }
    let root = d.root.as_path();
    let mut behind: Vec<String> = Vec::new();
    let mut total = 0usize;
    let (workspaces, mut unseen): (Vec<String>, Vec<Unread>) = match fsx::read_workspaces(root) {
        Ok(found) => found,
        Err(e) => {
            return Row::not_checked(NAME, fsx::py_os_error(&e, &root.join("workspaces")));
        }
    };
    let looked_for = workspaces.len() + unseen.len();
    let mut unread_ws = unseen.len();
    for ws in &workspaces {
        // One workspace charter cannot list is named, and the others are still read (#1014):
        // costing the whole row would hide a stale clone one workspace over.
        let (found, unstatted) = match fsx::read_clones(root, ws) {
            Ok(found) => found,
            Err(e) => {
                unseen.push((root.join("workspaces").join(ws), e.raw_os_error()));
                unread_ws += 1;
                continue;
            }
        };
        unseen.extend(unstatted);
        for clone in found {
            total += 1;
            // `@{upstream}` fails cleanly where there is no tracking branch, which is not a
            // fault.
            let run = match git_in(&clone, &["rev-list", "--count", "HEAD..@{upstream}"]) {
                Ok(run) => run,
                Err(why) => return Row::not_checked(NAME, why),
            };
            if !run.ok() {
                continue;
            }
            let n: u64 = match py_strip(&run.out) {
                "" => 0,
                text => match text.parse() {
                    Ok(n) => n,
                    Err(_) => {
                        return Row::not_checked(
                            NAME,
                            format!("invalid literal for int() with base 10: '{text}'"),
                        );
                    }
                },
            };
            if n > 0 {
                let name = clone
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // A chat can name a directory anything, and a newline in it must not print a
                // row of its own (#353).
                let at = super::one_line(&format!("{ws}/{name}"), super::DISPLAY_LIMIT);
                behind.push(format!("{at} ({n} behind)"));
            }
        }
    }

    if behind.is_empty() {
        if !unseen.is_empty() {
            let row = Row::warn(
                NAME,
                format!(
                    "{total} clone(s) across {} of {looked_for} workspace(s), none behind",
                    looked_for - unread_ws
                ),
                "",
            );
            return fsx::beside_unread(root, row, &unseen);
        }
        if total == 0 {
            return Row::ok(NAME, "no clones in any workspace — nothing to check");
        }
        return Row::ok(
            NAME,
            format!("{total} clone(s) across all workspaces, none behind"),
        );
    }
    let mut detail = behind
        .iter()
        .take(4)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if behind.len() > 4 {
        detail.push_str(", …");
    }
    let row = Row {
        name: NAME.to_owned(),
        status: Status::Warn,
        detail,
        hint: "→ charter sync --all  (plain `sync` only touches the ACTIVE workspace, which is \
               how this stays hidden)  Counted from what the last fetch recorded, so it can \
               under-report — never a live query, this runs at SessionStart."
            .to_owned(),
    };
    fsx::beside_unread(root, row, &unseen)
}
