//! The `vault files` row: plain-file vaults whose registered file is outside the project or has
//! gone from it (#1345). Only printed when there is one, as `renamed leftovers` is: a project
//! with nothing to flag prints the rows it always printed.
//!
//! The deferred `vault registry` row stays as it is: this looks at where each file is, not at
//! the vaults or the credentials they hold.
//!
//! The `<program> for vaults` rows (#1516): for each program a registered vault's provider runs
//! (`op`, `vault`), whether purlis finds it and by which route. Printed only for a project with
//! such a vault. A warning where it is not found, where only this process's `PATH` finds it,
//! and where the only one found is where a chat may write, which is never run. The lookup is the providers' own ([`Ctx::program`]), asked of this process's
//! environment: the Doctor the app opens answers for the app, which is what resolves a
//! sandboxed chat's `secret exec`.
//!
//! The `vault tokens` row (#1526): for each vault read through an identity variable, where its
//! token is: marked as kept in the system keyring, in this process's environment only, or
//! nowhere. Printed only for a project with such a vault. Said from this machine's record and
//! the environment ([`crate::secrets::identity::held`]): the keyring is not read, so the row
//! makes it ask nothing, no token is ever in hand here, and "marked as kept" is what the record
//! says, not a read that succeeded. A warning for a token found nowhere, and for one only this
//! environment has, which is the app started from the Dock finding nothing where a terminal
//! finds everything. **No row inside a chat**: no chat is given an identity variable, so from
//! there an exported token cannot be told from one that is nowhere.

use std::collections::BTreeMap;

use super::{Doctor, Row};
use crate::secrets::program::{NotRun, Route};
use crate::secrets::registry::{self, Vault};
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

/// One row for each program a registered vault's provider runs, in the programs' order, for
/// the project `d` answers for. None for the preflight a chat's start runs: verifying a pinned
/// `op` runs a program, and a chat's start waits on the preflight.
pub(super) fn provider_programs(d: &Doctor) -> Vec<Row> {
    if !d.has_plane || d.preflight {
        return Vec::new();
    }
    program_rows(&Ctx::new(&d.root, Env::from_process()))
}

/// [`provider_programs`] for the project `ctx` names. A registry purlis cannot read is `purlis
/// vault list`'s to report, and gives no row here.
pub(super) fn program_rows(ctx: &Ctx) -> Vec<Row> {
    let Ok(doc) = registry::load_registry(ctx) else {
        return Vec::new();
    };
    let mut names: Vec<String> = registry::vaults(&doc).keys().cloned().collect();
    names.sort();
    let mut users: BTreeMap<&'static str, Vec<Vault>> = BTreeMap::new();
    for name in names {
        let Ok(vault) = registry::vault_in(&doc, &name) else {
            continue;
        };
        for program in crate::secrets::program::needed_by(ctx, &vault) {
            users.entry(program).or_default().push(vault.clone());
        }
    }
    users
        .iter()
        .map(|(program, vaults)| program_row(ctx, program, vaults))
        .collect()
}

/// The row for `program`, which `vaults` are read through. Names and paths, never a value.
fn program_row(ctx: &Ctx, program: &str, vaults: &[Vault]) -> Row {
    let name = format!("{program} for vaults");
    let shown =
        |path: &std::path::Path| crate::shown::readable(&path.display().to_string(), usize::MAX);
    let mut said: Vec<String> = Vec::new();
    let mut hints: Vec<String> = Vec::new();
    // A vault whose token purlis keeps runs the `op` pinned with it and no other; the rest run
    // the one the lookup finds.
    let mut looked_up: Vec<String> = Vec::new();
    for vault in vaults {
        let label = format!("'{}'", crate::personas::one_line(&vault.name));
        let pin = match program {
            "op" => crate::secrets::identity::pinned_op(ctx, vault),
            _ => Ok(None),
        };
        match pin {
            Ok(None) => looked_up.push(label),
            Ok(Some(path)) => said.push(format!(
                "{label} runs the one pinned when its token was stored, {}",
                shown(&path)
            )),
            Err(e) => {
                said.push(format!("{label} has no pinned {program} it can run"));
                hints.push(e.message);
            }
        }
    }
    if !looked_up.is_empty() {
        let used = format!("Used by {}", looked_up.join(", "));
        match ctx.program(program) {
            Ok(found) if found.route == Route::Always => said.insert(
                0,
                format!(
                    "{}, found in a directory purlis always searches. {used}",
                    shown(&found.path)
                ),
            ),
            // The issue's own failure, seen from the process that does find it.
            Ok(found) => {
                said.insert(
                    0,
                    format!(
                        "{}, found on PATH only, so a purlis started with another PATH does \
                         not find it. {used}",
                        shown(&found.path)
                    ),
                );
                hints.insert(
                    0,
                    match ctx.env.get("HOME").filter(|home| home.starts_with('/')) {
                        Some(home) => format!(
                            "Put a link to it in {home}/{}, which purlis searches however it \
                             is started.",
                            crate::programs::USER_BIN[0]
                        ),
                        None => {
                            "Start purlis with this PATH whenever it reads these vaults.".to_owned()
                        }
                    },
                );
            }
            Err(NotRun::NotFound(not)) => {
                said.insert(0, format!("not found. {used}"));
                hints.insert(
                    0,
                    format!(
                        "Install {program}, then run `purlis doctor` again. {}",
                        ctx.looked_in(&not.looked)
                    ),
                );
            }
            Err(NotRun::Writable { path, looked }) => {
                said.insert(
                    0,
                    format!(
                        "found only where a chat can write, {}, and never run. {used}",
                        shown(&path)
                    ),
                );
                hints.insert(
                    0,
                    format!(
                        "Keep {program} outside the project and outside what a chat may write. \
                         {}",
                        ctx.looked_in(&looked)
                    ),
                );
            }
        }
    }
    if hints.is_empty() {
        Row::ok(&name, said.join("; "))
    } else {
        Row::warn(&name, said.join("; "), hints.join(" "))
    }
}

/// The row's name.
pub(super) const TOKENS: &str = "vault tokens";

/// The `vault tokens` row for the project `d` answers for, or `None`. None for the preflight
/// a chat's start runs, as for any doctor run inside a chat ([`token_row`]).
pub(super) fn identity_tokens(d: &Doctor) -> Option<Row> {
    if !d.has_plane || d.preflight {
        return None;
    }
    token_row(&Ctx::new(&d.root, Env::from_process()))
}

/// [`identity_tokens`] for the project `ctx` names: `None` where no vault declares an identity,
/// where the registry cannot be read (`purlis vault list`'s to report), and inside a chat
/// ([`crate::hookwire::CHAT_ENV`]), which is given no identity variable and so cannot tell an
/// exported token from a missing one. Names, never a value.
pub(super) fn token_row(ctx: &Ctx) -> Option<Row> {
    use crate::secrets::identity::{self, Held};
    if ctx
        .env
        .get(crate::hookwire::CHAT_ENV)
        .is_some_and(|chat| !chat.is_empty())
    {
        return None;
    }
    let doc = registry::load_registry(ctx).ok()?;
    let mut names: Vec<String> = registry::vaults(&doc).keys().cloned().collect();
    names.sort();
    let mut said: Vec<String> = Vec::new();
    let (mut nowhere, mut here_only) = (false, false);
    for name in names {
        let Ok(vault) = registry::vault_in(&doc, &name) else {
            continue;
        };
        for bound in identity::held(ctx, &vault) {
            let source = crate::personas::one_line(&bound.source);
            let whereabouts = match bound.held {
                Held::Keyring => {
                    format!(
                        "is marked as kept in {}",
                        crate::secrets::keyring::STORE_NAME
                    )
                }
                Held::Environment => {
                    here_only = true;
                    "is in this environment only".to_owned()
                }
                Held::Unset => {
                    nowhere = true;
                    "is nowhere".to_owned()
                }
            };
            said.push(format!(
                "'{}': ${source} {whereabouts}",
                crate::personas::one_line(&vault.name)
            ));
        }
    }
    if said.is_empty() {
        return None;
    }
    let detail = said.join("; ");
    if !nowhere && !here_only {
        return Some(Row::ok(TOKENS, detail));
    }
    // True with a window and without one: the tab where the app is used, the export where it
    // is not (a server, CI), which is the only way there.
    let mut hint = format!(
        "Where the app is used, open each vault's tab and paste its token into the box there: \
         it goes into {}, where purlis finds it however it is started and no chat can read it. \
         On a machine with no window, export the variable where purlis runs.",
        crate::secrets::keyring::STORE_NAME
    );
    if here_only {
        hint.push_str(
            " A token in this environment only is found by a purlis started from it, and not \
             by the app opened from the Dock.",
        );
    }
    Some(Row::warn(TOKENS, detail, hint))
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

    // ---- `<program> for vaults` (#1516) ---------------------------------------------------

    /// A project with a 1Password vault `prod` and a plain-file one, read by a process whose
    /// `PATH` is `path` and whose home is `home`.
    fn on_1password(path: &str, home: &std::path::Path) -> Plane {
        crate::secrets::program::stand_ins_live_in_temp_folders();
        let plane = Plane::new(&[("PATH", path), ("HOME", &home.to_string_lossy())]);
        plane.plain("files", json!({"K": "never-printed-1516"}));
        plane.register("prod", "1password", json!({"op-vault": "Prod"}), None);
        plane
    }

    #[cfg(unix)]
    #[test]
    fn a_providers_program_is_named_with_where_it_was_found_and_by_which_route() {
        let home = tempfile::tempdir().unwrap();
        let local = home.path().join(".local/bin");
        std::fs::create_dir_all(&local).unwrap();
        stand_in::program(&local, "op", "#!/bin/sh\n");
        let plane = on_1password("/usr/bin:/bin", home.path());

        let rows = program_rows(&plane.ctx);

        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].name, "op for vaults");
        assert_eq!(rows[0].status, super::super::Status::Ok);
        assert_eq!(
            rows[0].detail,
            format!(
                "{}, found in a directory purlis always searches. Used by 'prod'",
                local.join("op").canonicalize().unwrap().display()
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_program_found_only_on_this_processs_path_warns_that_another_purlis_does_not_find_it() {
        let home = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        stand_in::program(bin.path(), "op", "#!/bin/sh\n");
        let plane = on_1password(&bin.path().to_string_lossy(), home.path());

        let rows = program_rows(&plane.ctx);

        assert_eq!(rows[0].status, super::super::Status::Warn);
        assert_eq!(
            rows[0].detail,
            format!(
                "{}, found on PATH only, so a purlis started with another PATH does not find \
                 it. Used by 'prod'",
                bin.path().join("op").canonicalize().unwrap().display()
            )
        );
        assert_eq!(
            rows[0].hint,
            format!(
                "Put a link to it in {}/.local/bin, which purlis searches however it is started.",
                home.path().display()
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_program_found_only_where_a_chat_can_write_warns_and_is_not_called_found() {
        let home = tempfile::tempdir().unwrap();
        let plane = on_1password("/usr/bin:/bin", home.path());
        let planted = plane.root().join("tools");
        std::fs::create_dir_all(&planted).unwrap();
        stand_in::program(&planted, "op", "#!/bin/sh\n");
        let ctx = Ctx::new(
            plane.root(),
            Env::of(&[
                ("PATH", &format!("{}:/usr/bin:/bin", planted.display())),
                ("HOME", &home.path().to_string_lossy()),
            ]),
        );

        let rows = program_rows(&ctx);

        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].status, super::super::Status::Warn);
        assert_eq!(
            rows[0].detail,
            format!(
                "found only where a chat can write, {}, and never run. Used by 'prod'",
                planted.join("op").display()
            )
        );
        assert!(
            rows[0].hint.starts_with(
                "Keep op outside the project and outside what a chat may write. It looked in: "
            ),
            "{}",
            rows[0].hint
        );
    }

    #[test]
    fn a_reference_this_version_cannot_read_asks_for_no_program() {
        let home = tempfile::tempdir().unwrap();
        let plane = Plane::new(&[
            ("PATH", "/usr/bin:/bin"),
            ("HOME", &home.path().to_string_lossy()),
        ]);
        let file = plane.ctx.vaults_dir().join("refs.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            json!({"A": "browser://example.com/login#password"}).to_string(),
        )
        .unwrap();
        plane.register(
            "refs",
            "reference",
            json!({"file": file.to_string_lossy()}),
            None,
        );
        assert_eq!(program_rows(&plane.ctx), Vec::new());
    }

    #[test]
    fn a_program_that_is_not_found_warns_with_every_directory_searched() {
        let home = tempfile::tempdir().unwrap();
        let plane = on_1password("/usr/bin:/bin", home.path());

        let rows = program_rows(&plane.ctx);

        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].status, super::super::Status::Warn);
        assert_eq!(rows[0].detail, "not found. Used by 'prod'");
        assert!(
            rows[0].hint.starts_with(
                "Install op, then run `purlis doctor` again. It looked in: /usr/bin, /bin, "
            ),
            "{}",
            rows[0].hint
        );
        let local = home.path().join(".local/bin").display().to_string();
        assert!(
            rows[0]
                .hint
                .ends_with(&format!("put a link to it in {local}.")),
            "{}",
            rows[0].hint
        );
    }

    #[test]
    fn a_reference_vault_is_asked_for_each_program_its_references_resolve_through() {
        let home = tempfile::tempdir().unwrap();
        let plane = on_1password("/usr/bin:/bin", home.path());
        let file = plane.ctx.vaults_dir().join("refs.json");
        std::fs::write(
            &file,
            json!({"A": "op://Eng/item/field", "B": "vault://secret/data/x#y"}).to_string(),
        )
        .unwrap();
        plane.register(
            "refs",
            "reference",
            json!({"file": file.to_string_lossy()}),
            None,
        );

        let rows = program_rows(&plane.ctx);

        let said: Vec<(&str, &str)> = rows
            .iter()
            .map(|row| (row.name.as_str(), row.detail.as_str()))
            .collect();
        assert_eq!(
            said,
            [
                ("op for vaults", "not found. Used by 'prod', 'refs'"),
                ("vault for vaults", "not found. Used by 'refs'"),
            ]
        );
    }

    // ---- `vault tokens` (#1526) -------------------------------------------------------------

    const KEPT: &str = "ops_fixture-doctor-kept-1526";
    const EXPORTED: &str = "ops_fixture-doctor-exported-1526";

    /// A 1Password vault `name` read through `$source`.
    fn read_through(plane: &Plane, name: &str, source: &str) {
        plane.register(
            name,
            "1password",
            json!({"op-vault": "Prod", "env": {"OP_SERVICE_ACCOUNT_TOKEN": source}}),
            None,
        );
    }

    #[test]
    fn each_vault_read_through_a_token_is_said_with_where_its_token_is_and_no_token() {
        let plane = Plane::new(&[("PATH", "/usr/bin:/bin"), ("OP_SHELL_TOKEN", EXPORTED)]);
        plane.plain("files", json!({"K": "never-printed-1526"}));
        read_through(&plane, "kept", "OP_KEPT_TOKEN");
        read_through(&plane, "lost", "OP_LOST_TOKEN");
        read_through(&plane, "shell", "OP_SHELL_TOKEN");
        let kept = registry::vault(&plane.ctx, "kept").unwrap();
        crate::secrets::identity::put_in_keyring(&plane.ctx, &kept, KEPT).unwrap();
        // A keyring that cannot be read at all: the row is said without reading it.
        std::fs::write(
            plane.ctx.state.join(crate::secrets::keyring::STUB_FILE),
            "not json",
        )
        .unwrap();

        let row = token_row(&plane.ctx).expect("a row");

        assert_eq!(row.name, "vault tokens");
        assert_eq!(row.status, super::super::Status::Warn);
        assert_eq!(
            row.detail,
            "'kept': $OP_KEPT_TOKEN is marked as kept in the system keyring; 'lost': \
             $OP_LOST_TOKEN is nowhere; 'shell': $OP_SHELL_TOKEN is in this environment only"
        );
        assert_eq!(
            row.hint,
            "Where the app is used, open each vault's tab and paste its token into the box \
             there: it goes into the system keyring, where purlis finds it however it is \
             started and no chat can read it. On a machine with no window, export the variable \
             where purlis runs. A token in this environment only is found by a purlis started \
             from it, and not by the app opened from the Dock."
        );
        let all = format!("{row:?}");
        assert!(!all.contains(KEPT) && !all.contains(EXPORTED), "{all}");
    }

    #[cfg(unix)]
    #[test]
    fn a_project_whose_tokens_are_all_kept_with_a_program_pinned_gets_a_green_row() {
        // The state a read works in: the token stored where a provider's program was found, so
        // the record pins one. A record that pins none is the programs row's to warn of.
        crate::secrets::program::stand_ins_live_in_temp_folders();
        let bin = tempfile::tempdir().unwrap();
        let op = stand_in::program(bin.path(), "op", "#!/bin/sh\n");
        let plane = Plane::new(&[("PATH", &bin.path().to_string_lossy())]);
        read_through(&plane, "kept", "OP_KEPT_TOKEN");
        let kept = registry::vault(&plane.ctx, "kept").unwrap();
        crate::secrets::identity::put_in_keyring(&plane.ctx, &kept, KEPT).unwrap();
        assert_eq!(
            crate::secrets::identity::pinned_op(&plane.ctx, &kept).unwrap(),
            Some(op)
        );

        let row = token_row(&plane.ctx).expect("a row");

        assert_eq!(row.status, super::super::Status::Ok);
        assert_eq!(
            row.detail,
            "'kept': $OP_KEPT_TOKEN is marked as kept in the system keyring"
        );
    }

    #[test]
    fn a_doctor_run_inside_a_chat_says_nothing_of_tokens_it_cannot_see() {
        // No chat is given an identity variable, so a token the person's shell exports would
        // be called "nowhere" from in here.
        let plane = Plane::new(&[("PATH", "/usr/bin:/bin"), ("PURLIS_CHAT", "7")]);
        read_through(&plane, "lost", "OP_LOST_TOKEN");
        assert_eq!(token_row(&plane.ctx), None);
    }

    #[test]
    fn a_project_with_no_vault_read_through_a_token_gets_no_token_row() {
        let plane = Plane::new(&[("PATH", "/usr/bin:/bin")]);
        plane.plain("files", json!({"K": "never-printed-1526"}));
        plane.register("prod", "1password", json!({"op-vault": "Prod"}), None);
        assert_eq!(token_row(&plane.ctx), None);
    }

    #[test]
    fn a_project_with_no_vault_read_through_a_program_gets_no_row() {
        let plane = Plane::new(&[("PATH", "/usr/bin:/bin")]);
        plane.plain("files", json!({"K": "never-printed-1516"}));
        assert_eq!(program_rows(&plane.ctx), Vec::new());
    }
}
