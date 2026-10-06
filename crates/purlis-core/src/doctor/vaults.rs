//! The `vault files` row: plain-file vaults whose registered file is outside the project or has
//! gone from it (#1345). Only printed when there is one, as `renamed leftovers` is: a project
//! with nothing to flag prints the rows it always printed.
//!
//! The deferred `vault registry` row stays as it is: this looks at where each file is, not at
//! the vaults or the credentials they hold.

use super::{Doctor, Row};
use crate::secrets::vaultcmd::Misplaced;
use crate::secrets::{Ctx, Env};

/// The row's name.
pub(super) const NAME: &str = "vault files";

/// The row for the project `d` answers for, or `None`.
pub(super) fn vault_files(d: &Doctor) -> Option<Row> {
    if !d.has_plane {
        return None;
    }
    row_for(&Ctx::new(&d.root, Env::from_process()))
}

/// The row for the project `ctx` names. A registry purlis cannot read is `purlis vault list`'s
/// to report, and gives no row here.
pub(super) fn row_for(ctx: &Ctx) -> Option<Row> {
    row_of(&crate::secrets::vaultcmd::misplaced(ctx).ok()?)
}

/// The row for `found`, or `None` when it is empty.
///
/// **A warning only for what to act on**: a file that is missing, gone, or in a temp
/// directory. A vault kept outside the project on purpose, and there, is only worth knowing,
/// and is said on a green row.
fn row_of(found: &[Misplaced]) -> Option<Row> {
    if found.is_empty() {
        return None;
    }
    let listed = found.iter().map(said).collect::<Vec<_>>().join("; ");
    let count = found.len();
    let vaults = if count == 1 { "vault" } else { "vaults" };
    if !found.iter().any(Misplaced::to_act_on) {
        return Some(Row::ok(
            NAME,
            format!("{count} {vaults} kept outside the project: {listed}"),
        ));
    }
    Some(Row::warn(
        NAME,
        format!("{count} {vaults} name a file outside the project or one that is gone: {listed}"),
        "Every vault file the registry names is denied to each sandboxed chat, wherever it \
         is. Remove a leftover registration with `purlis vault remove <name>`; a vault kept \
         outside the project on purpose can stay.",
    ))
}

/// One vault, as the row lists it: its name, its file and what is wrong with it. Never a value.
fn said(m: &Misplaced) -> String {
    let what = match (m.outside, m.missing, m.temp) {
        (true, true, _) => "outside the project, and missing",
        (true, false, true) => "in a temp directory",
        (true, false, false) => "outside the project",
        (false, _, _) => "gone: missing, though values were written to it",
    };
    format!(
        "'{}' ({}, {what})",
        crate::personas::one_line(&m.name),
        super::short_path(&m.file)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::cmd::tests::Plane;
    use serde_json::json;

    #[test]
    fn a_vault_registered_at_a_path_outside_the_project_is_named_with_the_way_to_remove_it() {
        let plane = Plane::new(&[]);
        plane.plain("home", json!({"K": "never-printed-77c1"}));
        plane.register("x", "plain-file", json!({"file": "/x-1345-nowhere"}), None);

        let row = row_for(&plane.ctx).expect("a row");

        assert_eq!(row.name, NAME);
        assert_eq!(row.status, super::super::Status::Warn);
        assert!(
            row.detail
                .contains("'x' (/x-1345-nowhere, outside the project, and missing)"),
            "{}",
            row.detail
        );
        assert!(!row.detail.contains("'home'"), "{}", row.detail);
        assert!(
            row.hint.contains("purlis vault remove <name>"),
            "{}",
            row.hint
        );
        assert!(!row.detail.contains("never-printed-77c1"));
    }

    fn kept(name: &str, file: &str, missing: bool, temp: bool) -> Misplaced {
        Misplaced {
            name: name.to_owned(),
            file: file.into(),
            outside: true,
            missing,
            temp,
        }
    }

    #[test]
    fn a_vault_kept_outside_the_project_and_there_is_said_on_a_green_row() {
        let row = row_of(&[kept("ops", "/srv/ops.json", false, false)]).expect("a row");
        assert_eq!(row.status, super::super::Status::Ok);
        assert!(
            row.detail
                .contains("'ops' (/srv/ops.json, outside the project)"),
            "{}",
            row.detail
        );
    }

    #[test]
    fn a_missing_or_temp_vault_file_is_a_warning_beside_one_kept_on_purpose() {
        for leftover in [
            kept("x", "/x", true, false),
            kept("t", "/tmp/devops.json", false, true),
        ] {
            let row = row_of(&[kept("ops", "/srv/ops.json", false, false), leftover.clone()])
                .expect("a row");
            assert_eq!(row.status, super::super::Status::Warn, "{leftover:?}");
            assert!(row.detail.contains("'ops'"), "{}", row.detail);
        }
    }

    #[test]
    fn a_project_whose_vault_files_are_all_inside_it_prints_no_row() {
        let plane = Plane::new(&[]);
        plane.plain("home", json!({"K": "v"}));
        assert_eq!(row_for(&plane.ctx), None);
    }
}
