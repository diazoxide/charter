//! The `harness` row: every harness the project has, and the most each one offers (OB-8, #994).
//!
//! **The registry is the declarations** (ADR 0073): the three purlis ships and each
//! `harnesses/<name>.toml` the project commits, read by [`harness_declaration::read`], the
//! same reader a chat's start asks. A harness's ceiling is the highest level its declaration
//! offers: level 3 where it names an ACP agent, level 2 where purlis ships the adapter that
//! arms its hooks, and level 1, its terminal alone, otherwise.
//!
//! **A warning for a declaration purlis refused**, with the reader's own sentence: a refused
//! file declares nothing, so a profile naming it starts no chat, and nothing else on the
//! project's surfaces says so before someone tries. Reading runs nothing and asks nothing, so
//! the preflight runs this row too.

use super::{Doctor, Row};
use crate::harness_declaration::{self, Declaration, Declarations};

/// The row's name, as the Python charter printed it.
pub(super) const NAME: &str = "harness";

/// The row for the project `d` answers for. Without a project, the built-ins alone.
pub(super) fn harness(d: &Doctor) -> Row {
    let declared = if d.has_plane {
        harness_declaration::read(&d.root)
    } else {
        Declarations {
            declared: harness_declaration::builtins().to_vec(),
            refused: Vec::new(),
        }
    };
    row_of(&declared)
}

/// The row for `declared`.
pub(super) fn row_of(declared: &Declarations) -> Row {
    let ceilings = declared
        .declared
        .iter()
        .map(ceiling)
        .collect::<Vec<_>>()
        .join("; ");
    let count = declared.declared.len();
    if declared.refused.is_empty() {
        return Row::ok(NAME, format!("{count} harness(es) — {ceilings}"));
    }
    let refused = declared
        .refused
        .iter()
        .map(|r| super::one_line(&r.reason, super::PATH_DISPLAY_LIMIT))
        .collect::<Vec<_>>()
        .join("; ");
    Row::warn(
        NAME,
        format!(
            "{} declaration(s) refused: {refused} ({count} harness(es) read — {ceilings})",
            declared.refused.len()
        ),
        "A refused declaration declares nothing, so a profile that names its harness starts \
         no chat. Fix the file as the reason says, or remove it from harnesses/; `purlis \
         harness list` shows what is read.",
    )
}

/// One harness and the highest level its declaration offers.
fn ceiling(d: &Declaration) -> String {
    let level = if d.levels.acp.is_some() {
        "level 3 (ACP)"
    } else if d.has_adapter() {
        "level 2 (hooks)"
    } else {
        "level 1 (terminal only)"
    };
    format!(
        "{}: {level}",
        super::one_line(&d.name, super::DISPLAY_LIMIT)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::Status;
    use crate::harness_declaration::{Origin, Refused};

    fn builtins() -> Declarations {
        Declarations {
            declared: harness_declaration::builtins().to_vec(),
            refused: Vec::new(),
        }
    }

    #[test]
    fn the_harnesses_purlis_ships_are_named_with_the_most_each_offers() {
        let row = row_of(&builtins());

        assert_eq!(row.name, NAME);
        assert_eq!(row.status, Status::Ok);
        assert_eq!(
            row.detail,
            "3 harness(es) — claude: level 2 (hooks); opencode: level 3 (ACP); codex: level 2 \
             (hooks)"
        );
    }

    #[test]
    fn a_projects_terminal_only_harness_is_held_to_level_1() {
        let mut declared = builtins();
        let text = "name = \"gemini\"\nprogram = \"gemini\"\n\n[session]\nchosen_by = \
                    \"harness\"\n\n[terminal]\nready_to_type = \"never\"\n\n[levels]\n\
                    terminal = true\n";
        declared.declared.push(
            harness_declaration::parse(text, Origin::Project, "harnesses/gemini.toml")
                .expect("a terminal-only declaration reads"),
        );

        let row = row_of(&declared);

        assert_eq!(row.status, Status::Ok);
        assert!(
            row.detail.ends_with("; gemini: level 1 (terminal only)"),
            "{}",
            row.detail
        );
        assert!(row.detail.starts_with("4 harness(es) — "), "{}", row.detail);
    }

    #[test]
    fn a_refused_declaration_is_a_warning_that_carries_the_readers_reason() {
        let mut declared = builtins();
        declared.refused.push(Refused {
            file: "harnesses/x.toml".to_owned(),
            reason: "harnesses/x.toml declares 'y', and a declaration's file is named after \
                     the harness it declares.\nRename it."
                .to_owned(),
        });

        let row = row_of(&declared);

        assert_eq!(row.status, Status::Warn);
        assert!(
            row.detail
                .starts_with("1 declaration(s) refused: harnesses/x.toml declares 'y'"),
            "{}",
            row.detail
        );
        // A reason is one line in the row, whatever the file held.
        assert!(!row.detail.contains('\n'), "{}", row.detail);
        assert!(
            row.detail.contains("(3 harness(es) read — claude"),
            "{}",
            row.detail
        );
        assert!(row.hint.contains("harnesses/"), "{}", row.hint);
        assert!(!row.deferred());
    }

    #[test]
    fn a_project_declaration_on_disk_is_read_by_the_row() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("harnesses")).unwrap();
        std::fs::write(
            root.join("harnesses/other.toml"),
            "name = \"else\"\nprogram = \"x\"\n",
        )
        .unwrap();

        let row = row_of(&harness_declaration::read(&root));

        assert_eq!(row.status, Status::Warn, "{row:?}");
        assert!(
            row.detail.contains("harnesses/other.toml"),
            "{}",
            row.detail
        );
    }
}
