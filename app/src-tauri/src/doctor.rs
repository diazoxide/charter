//! `charter doctor`, as the window can reach it (charter ADR 0038's last named gap).
//!
//! **Why this exists, in one incident.** On 2026-09-21 a charter launched from Finder could not
//! find `claude`: macOS starts a GUI app from `launchd` with `PATH=/usr/bin:/bin:/usr/sbin:/sbin`
//! (charter-app#134). The doctor was ported, and it answers with the environment of the
//! process that runs it — so the one process whose answer mattered was the app, and the only
//! way to ask was to type `charter doctor` in a terminal, whose shell has the operator's full
//! `PATH` and so does not reproduce it. A doctor the app cannot run is a doctor that answers
//! for the wrong process.
//!
//! So this runs [`charter_core::doctor::Doctor`] **in the app's own process**, on the plane the
//! window names, and hands the rows over unchanged. Thin by design, as `worktrees.rs` is: every
//! row, every sentence and every verdict is the core's, the same ones `charter doctor --json`
//! prints (and `tests/differential/run.py` compares byte for byte with Python's). Nothing here
//! rewords a row.
//!
//! # Two depths, and who asks for which
//!
//! - **The preflight** (`full: false`) is what every SessionStart hook runs: no harness probe,
//!   no git call for one. The window asks it when a project opens, unprompted, which is safe
//!   precisely because a session start already pays it.
//! - **The full doctor** (`full: true`) also probes each harness profile — it RUNS the harness,
//!   costs hundreds of milliseconds, and can write into the profile's config folder. The core
//!   says only a doctor *a person asked for* may do that (`doctor/profiles.rs`, ruling 11), so
//!   the window asks it only when the operator opens the doctor. Opening it is the asking.

use charter_core::doctor::{Doctor, Row, Status};

use crate::planes::{PlaneId, Planes};

/// A row's verdict, as the window draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DoctorStatus {
    Ok,
    Warn,
    Fail,
}

impl From<Status> for DoctorStatus {
    fn from(status: Status) -> Self {
        match status {
            Status::Ok => Self::Ok,
            Status::Warn => Self::Warn,
            Status::Fail => Self::Fail,
        }
    }
}

/// One doctor row: the four fields `charter doctor --json` prints, and one it does not.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DoctorRow {
    pub name: String,
    pub status: DoctorStatus,
    pub detail: String,
    pub hint: String,
    /// Whether this build runs this check at all ([`Row::deferred`]).
    ///
    /// `false` is about twenty rows on every plane, each a WARN that says *not checked (…not
    /// ported…)*. They are drawn, because a doctor that dropped them would read as those
    /// problems being fixed — but a summary that COUNTED them would draw a warning count that
    /// never moves, and the one real warning among them would be invisible on its first day.
    pub checked: bool,
}

impl From<Row> for DoctorRow {
    fn from(row: Row) -> Self {
        Self {
            checked: !row.deferred(),
            status: row.status.into(),
            name: row.name,
            detail: row.detail,
            hint: row.hint,
        }
    }
}

/// What the doctor said, and what it was asked with.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct DoctorReport {
    /// Every row, in the order `charter doctor` prints them.
    pub rows: Vec<DoctorRow>,
    /// Whether the harness profiles were probed — the full doctor, not the preflight.
    pub full: bool,
    /// The `PATH` this process has, which is the one every row that looks for a program was
    /// answered with.
    ///
    /// **Not a row, and not the doctor's**: `charter doctor` prints no such line, and this is
    /// not dressed as one. It is here for the incident this module exists for — a Finder
    /// launch hands the app a four-directory `PATH`, and a built-in profile the doctor does
    /// not list (it lists one only when it finds its program) makes no sense until you can see
    /// the `PATH` it was looked for on. `None` when the process has none at all, which is
    /// itself the answer.
    pub path: Option<String>,
}

/// Every row `charter doctor` would print for this plane, run inside the app.
///
/// On a blocking thread: every git question a row asks has a five-second deadline
/// (`doctor::CHECK_TIMEOUT`), and the full doctor runs a harness per profile. A window that
/// waited on that would stop drawing.
#[tauri::command]
#[specta::specta]
pub async fn plane_doctor(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    full: bool,
) -> Result<DoctorReport, String> {
    // Resolved on the thread that asked, as `workspace_repos` does: a plane that is not open
    // refuses here, with the registry's own sentence, rather than inside the blocking half.
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || report(&root, full))
        .await
        .map_err(|err| format!("the doctor did not finish: {err}"))
}

/// The report for one plane. The command above is this on a blocking thread.
///
/// **The plane is the one the window names, and the working directory is that plane.** The
/// app never resolves a plane from where it was launched after startup (ADR 0034), so asking
/// [`Doctor::new`] — which resolves from a working directory and `$CHARTER_ROOT` — would be a
/// doctor reporting on whichever plane THAT resolution landed on, while the window shows
/// another. Not pinned: the plane was chosen by the operator opening it, not by an
/// environment variable, and `pinned` is what makes the `nested plane` row talk about
/// `$CHARTER_ROOT`.
pub(crate) fn report(root: &std::path::Path, full: bool) -> DoctorReport {
    DoctorReport {
        rows: Doctor::at(root, root, false, !full)
            .run()
            .into_iter()
            .map(DoctorRow::from)
            .collect(),
        full,
        path: std::env::var_os("PATH").map(|p| p.to_string_lossy().into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane charter recognises, resolved (macOS temp dirs are links).
    fn plane() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("a manifest");
        for d in ["personas", "inventory", "workspaces"] {
            std::fs::create_dir_all(root.join(d)).expect("a directory");
        }
        (dir, root)
    }

    #[test]
    fn the_window_is_handed_the_rows_charter_doctor_prints_and_in_its_order() {
        // One set of rows, two surfaces. What the window draws is what `charter doctor --json`
        // prints for the same plane — the JSON the differential compares with Python's — so a
        // row the app reworded, dropped or reordered is a failure here and not a review note.
        let (_dir, root) = plane();

        let drawn = report(&root, false);
        let printed: serde_json::Value = serde_json::from_str(&charter_core::doctor::json(
            &Doctor::at(&root, &root, false, true).run(),
        ))
        .expect("the doctor's JSON parses");

        let printed = printed.as_array().expect("an array");
        assert_eq!(drawn.rows.len(), printed.len());
        for (row, json) in drawn.rows.iter().zip(printed) {
            assert_eq!(serde_json::json!(row.name), json["name"]);
            assert_eq!(serde_json::to_value(row.status).unwrap(), json["status"]);
            assert_eq!(serde_json::json!(row.detail), json["detail"]);
            assert_eq!(serde_json::json!(row.hint), json["hint"]);
        }
    }

    #[test]
    fn a_row_this_build_does_not_run_is_marked_and_a_ported_one_is_not() {
        let (_dir, root) = plane();

        let rows = report(&root, false).rows;
        let named = |name: &str| {
            rows.iter()
                .find(|r| r.name == name)
                .unwrap_or_else(|| panic!("no {name} row"))
        };

        assert!(!named("vaults").checked, "vaults are not ported");
        assert_eq!(named("vaults").status, DoctorStatus::Warn);
        assert!(named("charter.toml").checked);
        assert!(named("schema").checked);
    }

    #[test]
    fn the_unprompted_run_asks_no_profile_and_the_asked_for_one_does() {
        // The preflight is what the window runs unprompted, and it must reach no harness
        // profile — a probe runs the harness. The full doctor, which only an operator opening
        // it asks for, reports one row per profile. A declared profile nobody approved is
        // reported without being run ("not approved yet", or "not probed" where git would
        // carry the file), so this asks the question without starting a harness.
        let (_dir, root) = plane();
        std::fs::write(
            root.join("charter.local.toml"),
            "[harness.claude-work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
        )
        .expect("a profile");
        let profile_rows = |r: &DoctorReport| {
            r.rows
                .iter()
                .filter(|r| r.name == "profile claude-work")
                .count()
        };

        let unprompted = report(&root, false);
        let asked = report(&root, true);

        assert!(!unprompted.full);
        assert_eq!(
            profile_rows(&unprompted),
            0,
            "the preflight asked a profile"
        );
        assert!(asked.full);
        assert_eq!(profile_rows(&asked), 1, "the full doctor skipped a profile");
    }
}
