//! **Dispatch limits, as Settings shows them** (#1439, #1440): the project's row, each
//! workspace's and persona's own, yours on this machine, and what an administrator's policy
//! caps. Every rule is the core's ([`purlis_core::dispatchlimits`]); this lists what the files
//! hold and what the core says of it.
//!
//! **Nothing is written here.** A limit is one key of a settings file, so the window writes it
//! through `save_project_settings`, which refuses what the next read would refuse (a depth
//! above 8 is one).

use purlis_core::dispatchlimits::{self as limits, Level, Limit, Table};
use purlis_core::sandbox::policy::Locks;
use purlis_core::settings::{self, LayerText, Which};

use crate::PlaneId;
use crate::planes::Planes;

/// One limit, as a column of the table.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchLimit {
    /// The key it is written as: `running-per-chat`.
    pub word: String,
    pub label: String,
    /// One line on what it limits.
    pub help: String,
    /// What is in force where no file sets it; `null` is no cap.
    pub default: Option<u32>,
    /// Whether only a persona's row holds it.
    pub persona_only: bool,
    /// The most it may be set to.
    pub most: u32,
    /// The most an administrator's policy lets it be, where it says.
    pub ceiling: Option<u32>,
}

/// One row: what one level of one file sets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchRow {
    /// `project`, `workspace` or `persona`.
    pub scope: String,
    /// The workspace's or the persona's name; empty for the project's row.
    pub name: String,
    /// What it sets each limit to, in [`DispatchLimits::limits`]' order; `null` where it does
    /// not, and the level beneath it is in force.
    pub values: Vec<Option<u32>>,
    /// What is in force beneath this row for each limit, which an unset one shows; `null` is
    /// no cap.
    pub beneath: Vec<Option<u32>>,
    /// For a row of yours: each limit of it that is above the project's and so is ignored, as
    /// one sentence.
    pub ignored: Vec<String>,
}

/// What Settings › Project › Dispatch draws.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DispatchLimits {
    /// The committed file's name, as this project has it.
    pub file: String,
    /// This machine's own file's name.
    pub local_file: String,
    /// The committed file's text (`null`: not there): what a write is sent against.
    pub base: Option<String>,
    /// This machine's file's text (`null`: not there).
    pub local_base: Option<String>,
    /// Every limit, in the order the table draws them.
    pub limits: Vec<DispatchLimit>,
    /// The committed file's rows: the project's first, then each workspace's, then each
    /// persona's.
    pub rows: Vec<DispatchRow>,
    /// This machine's rows, in the same order: they only ever lower.
    pub mine: Vec<DispatchRow>,
    /// "Locked by policy, set by <who> in <file>.", where a policy caps any limit.
    pub locked_by: Option<String>,
    /// What either file holds in `[dispatch]` that is not read, each as one sentence.
    pub refused: Vec<String>,
    /// Why this machine's file is not read, where git would carry it.
    pub local_left_out: Option<String>,
}

fn most(limit: Limit) -> u32 {
    if limit == Limit::Depth {
        limits::DEEPEST
    } else {
        limits::MOST
    }
}

fn values(level: &Level) -> Vec<Option<u32>> {
    Limit::ALL
        .into_iter()
        .map(|limit| level.get(limit))
        .collect()
}

/// The rows of `table`, each with what `beneath` says is in force under it. `ignored` says
/// which of a row's own limits decide nothing.
fn rows(
    table: &Table,
    beneath: impl Fn(Option<&str>, Option<&str>) -> Vec<Option<u32>>,
    ignored: impl Fn(Option<&str>, Option<&str>, &Level) -> Vec<String>,
) -> Vec<DispatchRow> {
    let row = |scope: &str, name: &str, level: &Level| {
        let workspace = (scope == "workspace").then_some(name);
        let persona = (scope == "persona").then_some(name);
        DispatchRow {
            scope: scope.to_owned(),
            name: name.to_owned(),
            values: values(level),
            beneath: beneath(workspace, persona),
            ignored: ignored(workspace, persona, level),
        }
    };
    std::iter::once(row("project", "", &table.project))
        .chain(
            table
                .workspaces
                .iter()
                .map(|(name, level)| row("workspace", name, level)),
        )
        .chain(
            table
                .personas
                .iter()
                .map(|(name, level)| row("persona", name, level)),
        )
        .collect()
}

/// `table` without the row of `workspace` or `persona`: what is in force beneath that row.
fn without(table: &Table, workspace: Option<&str>, persona: Option<&str>) -> Table {
    let mut out = table.clone();
    match (workspace, persona) {
        (Some(name), _) => {
            out.workspaces.remove(name);
        }
        (None, Some(name)) => {
            out.personas.remove(name);
        }
        (None, None) => out.project = Level::unset(),
    }
    out
}

/// Each limit as [`limits::in_force`] answers it for a chat in `workspace` as `persona`,
/// dispatching to `persona`.
fn in_force(
    project: &Table,
    mine: &Table,
    policy: &Level,
    workspace: Option<&str>,
    persona: Option<&str>,
) -> limits::Limits {
    limits::in_force(project, workspace, persona, persona, mine, policy)
}

/// The page, from the two files' texts and the policy in force.
fn page(
    file: &str,
    local_file: &str,
    base: Option<String>,
    local: &LayerText,
    local_base: Option<String>,
    locks: &Locks,
) -> DispatchLimits {
    let shared = limits::read(base.as_deref(), file);
    let own = limits::read(local.text(), local_file);
    let nothing = Table::default();
    let no_cap = Level::unset();
    let ceiling = locks.dispatch_ceiling();
    let all = |limits: &limits::Limits| -> Vec<Option<u32>> {
        Limit::ALL
            .into_iter()
            .map(|limit| limits.value(limit))
            .collect()
    };
    DispatchLimits {
        file: file.to_owned(),
        local_file: local_file.to_owned(),
        limits: Limit::ALL
            .into_iter()
            .map(|limit| DispatchLimit {
                word: limit.word().to_owned(),
                label: limit.label().to_owned(),
                help: limit.help().to_owned(),
                default: limit.when_unset(),
                persona_only: limit.persona_only(),
                most: most(limit),
                ceiling: ceiling.get(limit),
            })
            .collect(),
        // Beneath a committed row: the committed file without it, and no table of yours.
        rows: rows(
            &shared.table,
            |workspace, persona| {
                all(&in_force(
                    &without(&shared.table, workspace, persona),
                    &nothing,
                    &no_cap,
                    workspace,
                    persona,
                ))
            },
            |_, _, _| Vec::new(),
        ),
        // Beneath a row of yours: what the project's files give there.
        mine: rows(
            &own.table,
            |workspace, persona| {
                all(&in_force(
                    &shared.table,
                    &nothing,
                    &no_cap,
                    workspace,
                    persona,
                ))
            },
            |workspace, persona, level| {
                in_force(&shared.table, &own.table, &no_cap, workspace, persona)
                    .ignored
                    .iter()
                    // Only what this row itself asks for: a less specific row of yours says
                    // its own.
                    .filter(|one| level.get(one.limit) == Some(one.yours))
                    .map(ToString::to_string)
                    .collect()
            },
        ),
        locked_by: (!ceiling.is_unset()).then(|| locks.locked_by()),
        refused: shared.refused.into_iter().chain(own.refused).collect(),
        local_left_out: local.left_out().map(str::to_owned),
        base,
        local_base,
    }
}

/// [`dispatch_limits`], without a runtime.
pub(crate) fn limits_of(root: &std::path::Path) -> Result<DispatchLimits, String> {
    let shared = settings::read(root, Which::Shared)?;
    let local = settings::read(root, Which::Local)?;
    Ok(page(
        shared.file,
        local.file,
        shared.exists.then_some(shared.text),
        &settings::layer_text(root, Which::Local),
        local.exists.then_some(local.text),
        &Locks::of(root),
    ))
}

/// The project's dispatch limits, for Settings › Project › Dispatch, a workspace's settings
/// and the persona view.
///
/// On a blocking thread: the Local file's check asks git whether it is ignored.
#[tauri::command]
#[specta::specta]
pub async fn dispatch_limits(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<DispatchLimits, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || limits_of(&root))
        .await
        .map_err(|err| format!("reading the dispatch limits did not finish: {err}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "purlis.toml";
    const LOCAL: &str = "purlis.local.toml";

    fn drawn(shared: &str, local: &str, locks: &Locks) -> DispatchLimits {
        page(
            FILE,
            LOCAL,
            Some(shared.to_owned()),
            &LayerText::Text(local.to_owned()),
            Some(local.to_owned()),
            locks,
        )
    }

    fn at(page: &DispatchLimits, word: &str) -> usize {
        page.limits
            .iter()
            .position(|limit| limit.word == word)
            .expect("a limit")
    }

    #[test]
    fn a_project_that_sets_nothing_draws_one_row_with_the_defaults_beneath_it() {
        let page = drawn("schema = 1\n", "", &Locks::none());
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].scope, "project");
        assert_eq!(page.rows[0].values, vec![None; 6]);
        assert_eq!(
            page.rows[0].beneath,
            vec![Some(6), Some(16), Some(3), Some(10), None, None]
        );
        assert_eq!(page.locked_by, None);
        assert_eq!(
            page.limits
                .iter()
                .map(|one| one.label.as_str())
                .collect::<Vec<_>>(),
            [
                "Running per chat",
                "Live per lineage",
                "Depth",
                "Messages per minute",
                "May dispatch",
                "May run at once"
            ]
        );
        assert_eq!(page.limits[at(&page, "depth")].most, 8);
    }

    #[test]
    fn each_override_is_a_row_with_what_is_in_force_beneath_it() {
        let page = drawn(
            "[dispatch]\nrunning-per-chat = 4\n\n[dispatch.workspaces.alpha]\n\
             running-per-chat = 12\n\n[dispatch.personas.devops]\ndepth = 1\n\
             may-run-at-once = 2\n",
            "",
            &Locks::none(),
        );
        let named: Vec<(&str, &str)> = page
            .rows
            .iter()
            .map(|row| (row.scope.as_str(), row.name.as_str()))
            .collect();
        assert_eq!(
            named,
            [
                ("project", ""),
                ("workspace", "alpha"),
                ("persona", "devops")
            ]
        );
        let running = at(&page, "running-per-chat");
        // The project's row sits on the default; the workspace's on the project's.
        assert_eq!(page.rows[0].values[running], Some(4));
        assert_eq!(page.rows[0].beneath[running], Some(6));
        assert_eq!(page.rows[1].values[running], Some(12));
        assert_eq!(page.rows[1].beneath[running], Some(4));
        let devops = &page.rows[2];
        assert_eq!(devops.values[at(&page, "depth")], Some(1));
        assert_eq!(devops.beneath[at(&page, "depth")], Some(3));
        assert_eq!(devops.values[at(&page, "may-run-at-once")], Some(2));
        assert_eq!(devops.beneath[at(&page, "may-run-at-once")], None);
    }

    #[test]
    fn a_limit_of_yours_above_the_projects_is_said_to_be_ignored_on_its_row() {
        let page = drawn(
            "[dispatch]\ndepth = 2\n",
            "[dispatch]\ndepth = 5\nrunning-per-chat = 1\n",
            &Locks::none(),
        );
        assert_eq!(page.mine.len(), 1);
        let mine = &page.mine[0];
        assert_eq!(mine.values[at(&page, "depth")], Some(5));
        assert_eq!(mine.beneath[at(&page, "depth")], Some(2));
        assert_eq!(
            mine.ignored,
            [
                "Your own limit of 5 for depth is above the project's 2, so it is ignored: a \
              limit on this machine can only lower one."
            ]
        );
    }

    #[test]
    fn a_policy_ceiling_is_shown_on_its_limit_with_who_set_it() {
        let locks = Locks::parse(
            r#"{"owner": "IT", "dispatch": {"depth": 2}}"#,
            std::path::Path::new("/etc/purlis/policy.json"),
        );
        let page = drawn("schema = 1\n", "", &locks);
        assert_eq!(page.limits[at(&page, "depth")].ceiling, Some(2));
        assert_eq!(page.limits[at(&page, "running-per-chat")].ceiling, None);
        assert_eq!(
            page.locked_by.as_deref(),
            Some("Locked by policy, set by IT in /etc/purlis/policy.json.")
        );
    }

    #[test]
    fn a_depth_of_nine_in_the_file_is_listed_as_refused_and_not_drawn_as_set() {
        let page = drawn("[dispatch]\ndepth = 9\n", "", &Locks::none());
        assert_eq!(page.rows[0].values[at(&page, "depth")], None);
        assert_eq!(page.refused.len(), 1);
        assert!(
            page.refused[0].contains("depth is never above 8"),
            "{:?}",
            page.refused
        );
    }

    #[test]
    fn a_local_file_git_would_carry_sets_nothing_and_says_why() {
        let page = page(
            FILE,
            LOCAL,
            None,
            &LayerText::LeftOut {
                why: "purlis.local.toml is tracked by git".to_owned(),
                text: "[dispatch]\ndepth = 1\n".to_owned(),
            },
            Some("[dispatch]\ndepth = 1\n".to_owned()),
            &Locks::none(),
        );
        assert_eq!(page.mine[0].values, vec![None; 6]);
        assert_eq!(
            page.local_left_out.as_deref(),
            Some("purlis.local.toml is tracked by git")
        );
    }
}
