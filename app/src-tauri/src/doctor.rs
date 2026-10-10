//! `charter doctor`, as the window can reach it (ADR 0038's last named gap).
//!
//! **Why this exists, in one incident.** On 2026-09-21 a charter launched from Finder could not
//! find `claude`: macOS starts a GUI app from `launchd` with `PATH=/usr/bin:/bin:/usr/sbin:/sbin`
//! (charter-app#134). The doctor was ported, and it answers with the environment of the
//! process that runs it — so the one process whose answer mattered was the app, and the only
//! way to ask was to type `charter doctor` in a terminal, whose shell has the operator's full
//! `PATH` and so does not reproduce it. A doctor the app cannot run is a doctor that answers
//! for the wrong process.
//!
//! So this runs [`purlis_core::doctor::Doctor`] **in the app's own process**, on the plane the
//! window names, and hands the rows over unchanged. Thin by design, as `worktrees.rs` is: every
//! row, every sentence and every verdict is the core's, the same ones `charter doctor --json`
//! prints (and the recorded `doctor-*` scenarios hold byte for byte to Python's, ADR 0046).
//! Nothing here rewords a row.
//!
//! # Two depths, and who asks for which
//!
//! - **The preflight** (`full: false`) is what every SessionStart hook runs: no harness probe,
//!   no git call for one. The window asks it when a project opens, unprompted, which is safe
//!   precisely because a session start already pays it.
//! # One row that is the app's own, and is marked as one
//!
//! `charter doctor` prints the core's rows and this hands them over unchanged. The app has one
//! question of its own that no CLI can answer — whether the chats THIS app starts can record
//! their turns ([`purlis_core::footerclaim`]) — so it travels in [`DoctorReport::app_rows`],
//! beside the table rather than inside it. Keeping it out of `rows` is what lets the test below
//! hold "what the window draws is what `charter doctor --json` prints" as an equality.
//!
//! - **The full doctor** (`full: true`) also answers for each harness profile — whether its
//!   program can be found, the search a launch makes — and asks the forge who can read the
//!   project's remote and whether it refuses a pushed secret (`project remote`, SQ-8). The core
//!   keeps that to a doctor *a person asked for* (`doctor/profiles.rs`, ruling 11), so the
//!   window asks it only when the operator opens the doctor. Opening it is the asking.

use purlis_core::doctor::{Doctor, Row, Status};

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

/// One doctor row: the fields `charter doctor --json` prints, and two it does not (`checked`,
/// `settings`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DoctorRow {
    pub name: String,
    pub status: DoctorStatus,
    pub detail: String,
    pub hint: String,
    /// Whether this build runs this check at all ([`Row::deferred`]).
    ///
    /// `false` is about twenty rows on every plane, each a WARN that says *not checked (…)*, for
    /// checks this build does not run (planned in OB-8, #994, and FG-2, #802). They are drawn,
    /// because a doctor that dropped them would read as those problems being fixed — but a
    /// summary that COUNTED them would draw a warning count that never moves, and the one real
    /// warning among them would be invisible on its first day.
    pub checked: bool,
    /// The Settings group its fix is made in, by the group's stable address (`project.forges`),
    /// when the fix is a setting ([`Row::settings`], SE-22): the window links the row there.
    /// `charter doctor --json` does not print it.
    pub settings: Option<String>,
    /// The fix charter can make for this finding itself (FX-1, `purlis_core::doctor::fix`),
    /// by the id `charter doctor --fix <id>` takes: the dialog draws a Fix button for it, which
    /// calls [`plane_doctor_fix`]. `None` for a row charter cannot fix, and for the app's own.
    pub fix: Option<String>,
}

impl From<Row> for DoctorRow {
    fn from(row: Row) -> Self {
        Self {
            checked: !row.deferred(),
            fix: row.fix.map(|id| id.id().to_owned()),
            status: row.status.into(),
            name: row.name,
            detail: row.detail,
            hint: row.hint,
            settings: row.settings.map(|group| group.id().to_owned()),
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
    /// Rows about THIS APP that `charter doctor` does not print. See the module note.
    pub app_rows: Vec<DoctorRow>,
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
    let shipped = planes.shipped().clone();
    tauri::async_runtime::spawn_blocking(move || report(&root, full, &shipped))
        .await
        .map_err(|err| format!("the doctor did not finish: {err}"))
}

/// What applying a fix came to, as the Doctor dialog draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DoctorFixed {
    /// The fix that was asked for, by its id.
    pub fix: String,
    /// Why it was refused, before anything was written. `None` when it ran.
    pub refused: Option<String>,
    /// What it changed, line by line, in the words and marks `charter doctor --fix` prints.
    pub said: Vec<String>,
    /// Whether it did everything it was asked to. `false` for a refusal, and for a fix that
    /// ran and could not do part of it, which `said` names.
    pub complete: bool,
}

/// Apply one doctor fix by its id to the plane the window names (FX-1): the Doctor dialog's
/// Fix button. The same core entry point `charter doctor --fix <id>` calls, so the window and
/// the terminal make the same fix. The window asks the doctor again afterwards.
///
/// On a blocking thread, as [`plane_doctor`] is: a fix writes files and takes the plane's lock.
#[tauri::command]
#[specta::specta]
pub async fn plane_doctor_fix(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    fix: String,
) -> Result<DoctorFixed, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    let shipped = planes.shipped().clone();
    tauri::async_runtime::spawn_blocking(move || fixed(&root, &fix, &shipped))
        .await
        .map_err(|err| format!("the fix did not finish: {err}"))?
}

/// [`plane_doctor_fix`], on the calling thread. `plugin-install` installs a copy whose hooks run
/// the `charter` this app ships ([`crate::Shipped`]), never the app's own executable.
fn fixed(
    root: &std::path::Path,
    fix: &str,
    shipped: &crate::Shipped,
) -> Result<DoctorFixed, String> {
    use purlis_core::doctor::fix::{self as registry, FixId, Fixed};
    use purlis_core::plugin_install::Machine;
    let id = FixId::parse(fix).ok_or_else(|| {
        format!(
            "purlis has no fix '{}'; the fixes are {}",
            purlis_core::shown::short(fix),
            FixId::ALL.map(FixId::id).join(", ")
        )
    })?;
    let applied = match id {
        FixId::PluginInstall => match machine(shipped, Machine::from_env) {
            Ok(machine) => registry::apply_for(root, id, &machine),
            Err(why) => Fixed::Refused(why),
        },
        _ => registry::apply(root, id),
    };
    Ok(answered(id, applied))
}

/// A fix's outcome, as the dialog draws it.
fn answered(
    id: purlis_core::doctor::fix::FixId,
    outcome: purlis_core::doctor::fix::Fixed,
) -> DoctorFixed {
    use purlis_core::doctor::fix::Fixed;
    match outcome {
        Fixed::Ran { said, complete } => DoctorFixed {
            fix: id.id().to_owned(),
            refused: None,
            said,
            complete,
        },
        Fixed::Refused(why) => DoctorFixed {
            fix: id.id().to_owned(),
            refused: Some(why),
            said: Vec::new(),
            complete: false,
        },
    }
}

/// What the git identity form's submit came to (FX-3): the fix's outcome, or the input the
/// core refused, field by field, with nothing written.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DoctorIdentityFixed {
    Fixed {
        fixed: DoctorFixed,
    },
    /// Each field's reasons, in the core's words; empty for a field that is fine.
    Invalid {
        name: Vec<String>,
        email: Vec<String>,
    },
}

/// Apply the `git-identity` fix with the name and email the Doctor's form was given (FX-3):
/// the core checks both, then writes them to git's global config, as `charter doctor --fix
/// git-identity --name … --email …` does. Answers the fix's outcome, or each field's refusal.
///
/// The plane is the window's: the core reads the identity in force there (every scope, as the
/// doctor's row does) before it writes, and the answer is kept to the dialog that asked.
#[tauri::command]
#[specta::specta]
pub async fn plane_doctor_fix_identity(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
    email: String,
) -> Result<DoctorIdentityFixed, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || identity_fixed(&root, &name, &email))
        .await
        .map_err(|err| format!("the fix did not finish: {err}"))
}

/// git's global identity as it stands, for the form to show a key that is set, locked
/// (FX-3, D-FX3-8). Each value is one display line; empty when unset.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct DoctorIdentityNow {
    pub name: String,
    pub email: String,
}

/// What git's global identity holds now: the form locks the keys that are set, because the
/// fix writes only what is missing. The core reads it again before it writes.
#[tauri::command]
#[specta::specta]
pub async fn plane_doctor_identity(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<DoctorIdentityNow, String> {
    planes.held(&plane)?;
    tauri::async_runtime::spawn_blocking(|| {
        purlis_core::doctor::fix::identity::current().map(|now| DoctorIdentityNow {
            name: purlis_core::shown::line(&now.name),
            email: purlis_core::shown::line(&now.email),
        })
    })
    .await
    .map_err(|err| format!("reading git's identity did not finish: {err}"))?
}

/// [`plane_doctor_fix_identity`], on the calling thread.
fn identity_fixed(root: &std::path::Path, name: &str, email: &str) -> DoctorIdentityFixed {
    identity_answer(purlis_core::doctor::fix::identity::apply(root, name, email))
}

/// The core's answer to the form, as the window reads it.
fn identity_answer(
    answer: Result<purlis_core::doctor::fix::Fixed, purlis_core::doctor::fix::identity::Invalid>,
) -> DoctorIdentityFixed {
    use purlis_core::doctor::fix::FixId;
    match answer {
        Ok(outcome) => DoctorIdentityFixed::Fixed {
            fixed: answered(FixId::GitIdentity, outcome),
        },
        Err(invalid) => DoctorIdentityFixed::Invalid {
            name: invalid.name,
            email: invalid.email,
        },
    }
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
///
/// **The plugin rows read this machine only in the full doctor**, as the harnesses the `charter`
/// this app ships would find them, so the dialog can offer the `plugin-install` fix. The
/// preflight a project opens with reads no harness configuration, as before.
pub(crate) fn report(root: &std::path::Path, full: bool, shipped: &crate::Shipped) -> DoctorReport {
    let machine = if full {
        machine(
            shipped,
            purlis_core::plugin_install::Machine::from_env_to_read,
        )
        .ok()
    } else {
        None
    };
    report_in(
        root,
        full,
        purlis_core::machine::config_root_if_there().as_deref(),
        machine,
    )
}

/// This machine's harnesses, with the `charter` this app ships as the one the hooks run and its
/// plugin as the one copied: `from_env` for a writer, `from_env_to_read` for the doctor.
fn machine(
    shipped: &crate::Shipped,
    from_env: fn(
        std::path::PathBuf,
        Option<std::path::PathBuf>,
    ) -> Result<purlis_core::plugin_install::Machine, String>,
) -> Result<purlis_core::plugin_install::Machine, String> {
    let binary = shipped.binary.as_ref().ok_or(
        "this app has no `purlis` beside it for the hooks to run, so nothing was installed",
    )?;
    // Resolved, as the `charter` that installed a copy named itself.
    let binary = binary.canonicalize().unwrap_or_else(|_| binary.clone());
    from_env(binary, shipped.plugin.clone())
}

/// [`report`], reading the forge request budgets (`forge budget` rows, FW-4) kept under the
/// machine store `config_root`, when there is one. A test names its own, so it never reads the
/// operator's.
fn report_in(
    root: &std::path::Path,
    full: bool,
    config_root: Option<&std::path::Path>,
    machine: Option<purlis_core::plugin_install::Machine>,
) -> DoctorReport {
    let doctor = Doctor::at(root, root, false, !full);
    let doctor = match machine {
        Some(machine) => doctor.with_machine(machine),
        None => doctor,
    };
    let doctor = match config_root {
        Some(config_root) => doctor.reading_budgets_in(config_root),
        None => doctor,
    };
    // The full doctor is one the operator opened, so it asks the forge about the project's
    // remote, as a typed `charter doctor` does (SQ-8). The preflight asks no forge.
    let doctor = if full {
        doctor.asking_forges(
            purlis_core::forge::Caller::window(),
            std::sync::Arc::new(purlis_core::forge::cli::Cli::default()),
        )
    } else {
        doctor
    };
    DoctorReport {
        rows: doctor.run().into_iter().map(DoctorRow::from).collect(),
        app_rows: [
            chat_footer(root),
            event_log(crate::EVENT_LOG.get()),
            file_events(
                crate::planewatch::windows().missed(root),
                std::time::Instant::now(),
            ),
        ]
        .into_iter()
        .chain(config_root.and_then(|config_root| local_project(root, config_root)))
        .collect(),
        full,
        path: std::env::var_os("PATH").map(|p| p.to_string_lossy().into_owned()),
    }
}

/// `chat footer`: can the chats this app starts record what a turn cost?
///
/// **The app's own row, not `charter doctor`'s**, because it is about what the app does when it
/// starts a chat — which no CLI invocation can answer. Claude Code hands the context and cache
/// numbers to its `statusLine` command and to nothing else, so a chat whose footer somebody
/// else fills records nothing, and its `ctx`/`cache` gauge is dark. The operator ruled on
/// 2026-09-22 that charter must never replace a `statusLine` they wrote; this row is the other
/// half of that ruling — the missing gauge explains itself instead of looking like breakage.
///
/// **It answers for the plane's own directory**, which is where a chat started in the plane
/// reads its project settings; a chat started in a workspace or a worktree reads that
/// directory's, and the row says so rather than implying it asked for every chat.
fn chat_footer(root: &std::path::Path) -> DoctorRow {
    use purlis_core::footerclaim::{Claim, UNSEEN, status_line};

    const NAME: &str = "chat footer";
    // A row's own words, in the doctor's register. Built here rather than through the core's
    // `Row`, whose constructors are the core's to call: this is the app's row.
    let row = |status: DoctorStatus, detail: String, hint: String| DoctorRow {
        name: NAME.to_owned(),
        status,
        detail,
        hint,
        // This build runs this check: it is not one of the ported table's deferred rows.
        checked: true,
        settings: None,
        fix: None,
    };
    let where_it_looked = format!(
        "This is the answer for {}; a chat started somewhere else reads that directory's \
         project settings. {UNSEEN}",
        root.display()
    );
    match status_line(Some(root)) {
        // Green, and still saying what it did not look at: a row that claimed more than it
        // asked would be the shape ADR 0013 refuses.
        Claim::Free => row(
            DoctorStatus::Ok,
            "purlis fills Claude Code's status line in the chats it starts, so each turn's \
             context and cache are recorded"
                .to_owned(),
            where_it_looked,
        ),
        Claim::Held {
            file,
            charters_own: true,
        } => row(
            DoctorStatus::Ok,
            format!(
                "{} already runs purlis's own statusline, so turns are recorded and purlis \
                 arms none of its own",
                file.display()
            ),
            where_it_looked,
        ),
        Claim::Held {
            file,
            charters_own: false,
        } => row(
            DoctorStatus::Warn,
            format!(
                "{} fills Claude Code's status line, so purlis arms none of its own and a \
                 chat's ctx/cache gauge stays dark",
                file.display()
            ),
            format!(
                "purlis will not replace a status line you wrote. Point that one at `purlis \
                 statusline` — it draws the footer you asked for and records the turn — or \
                 remove the key, and chats started after that record theirs. {where_it_looked}"
            ),
        ),
        Claim::Suppressed { file, key } => row(
            DoctorStatus::Warn,
            format!(
                "{} sets {key}, which narrows the status line to a managed one, so purlis \
                 arms none and a chat's ctx/cache gauge stays dark",
                file.display()
            ),
            format!(
                "Claude Code skips an unmanaged status line under that key without a word, so \
                 purlis does not arm one it knows would be ignored. {where_it_looked}"
            ),
        ),
        Claim::Unknown { file, why } => row(
            DoctorStatus::Warn,
            format!(
                "not checked (purlis could not read {}: {why}), so it armed no status line \
                 and a chat's ctx/cache gauge stays dark",
                file.display()
            ),
            format!(
                "purlis arms a status line only where it can see that nothing else fills it, \
                 so a settings file it cannot read leaves the operator's configuration \
                 untouched. {where_it_looked}"
            ),
        ),
    }
}

/// `event log`: is this app recording its chats' hook calls (FD-9), and if not, why not?
///
/// **The app's own row**, for `chat_footer`'s reason: the log is opened by the app, and only the
/// app knows whether that worked. `opened` is what this process's open answered; `None` is a
/// process that never tried, which is not health either. The hint is the repair for the reason
/// it failed, not one repair for every reason.
fn event_log(opened: Option<&Result<std::path::PathBuf, crate::EventLogRefused>>) -> DoctorRow {
    use std::io::ErrorKind;
    let row = |status: DoctorStatus, detail: String, hint: String| DoctorRow {
        name: "event log".to_owned(),
        status,
        detail,
        hint,
        checked: true,
        settings: None,
        fix: None,
    };
    match opened {
        Some(Ok(dir)) => row(
            DoctorStatus::Ok,
            format!(
                "every hook call a chat makes is recorded in {}",
                dir.display()
            ),
            "readable by your user on this machine and by a backup that carries it; a tool \
             call's arguments are never written, only a digest keyed to this device"
                .to_owned(),
        ),
        Some(Err(refused)) => row(
            DoctorStatus::Warn,
            format!("hook calls are not recorded ({})", refused.why),
            match refused.kind {
                ErrorKind::PermissionDenied | ErrorKind::NotFound => {
                    "set CHARTER_DATA_HOME to a directory outside any project or repository, \
                     and start purlis again; every chat works meanwhile"
                }
                ErrorKind::WouldBlock => {
                    "another purlis on this machine is writing this device's log; quit it, \
                     and start this one again"
                }
                ErrorKind::InvalidData => {
                    "the log holds nothing purlis can read; move events.jsonl aside (purlis \
                     never rewrites it), and start purlis again"
                }
                _ => "start purlis again; every chat works meanwhile",
            }
            .to_owned(),
        ),
        None => row(
            DoctorStatus::Warn,
            "not checked (this app has not opened its event log)".to_owned(),
            "start purlis again; the log is opened as the app starts".to_owned(),
        ),
    }
}

/// `file events`: do the platform's file events keep the window up with the project (#756)?
///
/// **The app's own row**, for `chat_footer`'s reason: the watch is the app's, and only the app
/// knows what its own looks had to tell ([`crate::planewatch::Windows`]). `missed` is what
/// they told for this project, or `None` when nothing watches it. A warning lasts
/// [`crate::planewatch::LATELY`] after the last change the events did not tell, so a stall the
/// operator noticed is still on the row when they open the doctor, and a stall long past does
/// not hold the count up for the rest of the day.
fn file_events(missed: Option<crate::planewatch::Missed>, now: std::time::Instant) -> DoctorRow {
    let row = |status: DoctorStatus, detail: String, hint: String| DoctorRow {
        name: "file events".to_owned(),
        status,
        detail,
        hint,
        checked: true,
        settings: None,
        fix: None,
    };
    let looks = format!(
        "While a window is shown, purlis also looks at the project's folders every {} seconds \
         and each time you come back to the window, and catches up on any change the events \
         did not tell.",
        crate::planewatch::SWEEP_EVERY.as_secs()
    );
    let lately = crate::planewatch::LATELY.as_secs() / 60;
    let Some(missed) = missed else {
        return row(
            DoctorStatus::Warn,
            "not checked (purlis is not watching this project's files)".to_owned(),
            "changes made outside this window reach its panels only when they are read again; \
             close the project and open it again to start the watch"
                .to_owned(),
        );
    };
    let ago = missed.last.map(|last| now.saturating_duration_since(last));
    match ago {
        Some(ago) if ago < crate::planewatch::LATELY => {
            let minutes = ago.as_secs() / 60;
            let when = match minutes {
                0 => "less than a minute ago".to_owned(),
                1 => "1 minute ago".to_owned(),
                n => format!("{n} minutes ago"),
            };
            let changes = match missed.count {
                1 => "1 change".to_owned(),
                n => format!("{n} changes"),
            };
            row(
                DoctorStatus::Warn,
                format!(
                    "file events are late or missing: purlis's own look found {changes} they \
                     had not told since it opened this project, the last {when}"
                ),
                format!(
                    "The window caught up each time, so nothing is lost: changes made outside \
                     it reach it a few seconds late. {looks} This clears {lately} minutes after \
                     the last late change."
                ),
            )
        }
        _ => row(
            DoctorStatus::Ok,
            match missed.count {
                0 => "file events have told every change purlis's own look found".to_owned(),
                n => format!(
                    "file events are keeping up again; purlis's own look found {n} they had \
                     not told since it opened this project, none in the last {lately} minutes"
                ),
            },
            looks,
        ),
    }
}

/// `local project`: only on this machine's local project while it is still in the config home,
/// where no sandboxed chat starts (#1670). The launch moves it when nothing works in it
/// (`purlis_core::localproject`); this says so where the person meets it, every time the
/// project is checked, until it has moved.
fn local_project(root: &std::path::Path, config_root: &std::path::Path) -> Option<DoctorRow> {
    let why = purlis_core::localproject::still_in_the_config_home(root, config_root)?;
    Some(DoctorRow {
        name: "local project".to_owned(),
        status: DoctorStatus::Warn,
        detail: why,
        hint: "purlis moves it to its data home at a launch when nothing works in it: quit every \
               chat, terminal and editor working in it, then quit purlis and open it again"
            .to_owned(),
        checked: true,
        settings: None,
        fix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`super::report`] reading no machine store: the real one reaches the operator's own
    /// config directory, which the plane fence refuses a test. Shadows the glob import.
    fn report(root: &std::path::Path, full: bool) -> DoctorReport {
        report_in(root, full, None, None)
    }

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

    /// #756: the row reads the watch's own looks: a warning while a change the events did not
    /// tell is recent, green before any and again once the last is long past.
    #[test]
    fn the_file_events_row_warns_after_a_change_the_events_did_not_tell() {
        use crate::planewatch::Missed;
        let now = std::time::Instant::now();
        let ago = |secs: u64| now.checked_sub(std::time::Duration::from_secs(secs));

        let kept_up = file_events(Some(Missed::default()), now);
        assert_eq!(kept_up.name, "file events");
        assert_eq!(kept_up.status, DoctorStatus::Ok);
        assert!(kept_up.checked);

        let late = file_events(
            Some(Missed {
                count: 3,
                last: ago(120),
            }),
            now,
        );
        assert_eq!(late.status, DoctorStatus::Warn);
        assert!(late.detail.contains("3 changes"), "{late:?}");
        assert!(late.detail.contains("2 minutes ago"), "{late:?}");

        if let Some(long_ago) = ago(3600) {
            let past = file_events(
                Some(Missed {
                    count: 3,
                    last: Some(long_ago),
                }),
                now,
            );
            assert_eq!(past.status, DoctorStatus::Ok);
            assert!(past.detail.contains("keeping up again"), "{past:?}");
        }

        let unwatched = file_events(None, now);
        assert_eq!(
            unwatched.status,
            DoctorStatus::Warn,
            "an absent answer is not health"
        );
        assert!(unwatched.detail.starts_with("not checked"), "{unwatched:?}");
    }

    /// #1670: the local project still in the config home is said, where it opens; any other
    /// project, and the local project once it moved, has no such row.
    #[test]
    fn a_local_project_left_in_the_config_home_has_a_row_and_no_other_project_does() {
        let dir = tempfile::tempdir().expect("a directory");
        let config_root = std::fs::canonicalize(dir.path()).expect("it resolves");
        let old = purlis_core::machine::dir(&config_root).join(purlis_core::localproject::OLD_NAME);
        std::fs::create_dir_all(&old).expect("the old place");
        let other = config_root.join("elsewhere");
        std::fs::create_dir_all(&other).expect("another project's folder");

        let row = local_project(&old, &config_root).expect("a row");

        assert_eq!(row.name, "local project");
        assert_eq!(row.status, DoctorStatus::Warn);
        assert!(
            row.detail.contains("no sandboxed chat starts here"),
            "{}",
            row.detail
        );
        assert_eq!(local_project(&other, &config_root), None);
    }

    #[test]
    fn the_window_is_handed_the_rows_charter_doctor_prints_and_in_its_order() {
        // One set of rows, two surfaces. What the window draws is what `charter doctor --json`
        // prints for the same plane — the JSON the differential compares with Python's — so a
        // row the app reworded, dropped or reordered is a failure here and not a review note.
        let (_dir, root) = plane();

        let drawn = report_in(&root, false, None, None);
        let printed: serde_json::Value = serde_json::from_str(&purlis_core::doctor::json(
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
            assert_eq!(
                serde_json::json!(row.fix),
                json.get("fix").cloned().unwrap_or(serde_json::Value::Null)
            );
        }
    }

    #[test]
    fn a_fixable_row_names_its_fix_and_the_fix_leaves_the_doctor_clean() {
        // FX-1: the window's Fix button, from the row to the re-check.
        let (_dir, root) = plane();
        std::fs::remove_dir(root.join("personas")).expect("removed");
        let schema = |report: &DoctorReport| {
            report
                .rows
                .iter()
                .find(|r| r.name == "schema")
                .cloned()
                .expect("a schema row")
        };
        let before = schema(&report(&root, false));
        assert_eq!(before.status, DoctorStatus::Warn, "{before:?}");
        assert_eq!(before.fix.as_deref(), Some("reinit"));

        let done =
            fixed(&root, "reinit", &crate::Shipped::default()).expect("a fix the registry has");
        assert_eq!(done.refused, None, "{done:?}");
        assert!(done.complete, "{done:?}");
        assert!(done.said.iter().any(|l| l.contains("personas")), "{done:?}");

        let after = schema(&report(&root, false));
        assert_eq!(after.status, DoctorStatus::Ok, "{after:?}");
        assert_eq!(after.fix, None);
    }

    #[test]
    fn the_full_doctor_offers_plugin_install_on_a_machine_without_it() {
        // FX-2: the plugin rows answer for the machine the window was handed, so the dialog
        // draws a Fix button on them.
        let (dir, root) = plane();
        let place = dir
            .path()
            .canonicalize()
            .expect("it resolves")
            .join("machine");
        std::fs::create_dir_all(place.join("codex")).expect("a codex home");
        std::fs::write(place.join("charter-bin"), "").expect("a binary");
        let machine = purlis_core::plugin_install::Machine {
            claude_config: place.join("claude"),
            codex_home: place.join("codex"),
            opencode_config: place.join("opencode"),
            charter_dir: place.join("config/charter"),
            binary: place.join("charter-bin"),
            bundle: None,
        };
        let drawn = report_in(&root, true, None, Some(machine));
        let install = drawn
            .rows
            .iter()
            .find(|r| r.name == "plugin install")
            .expect("a plugin install row");
        assert_eq!(install.status, DoctorStatus::Warn, "{install:?}");
        assert_eq!(install.fix.as_deref(), Some("plugin-install"));
    }

    #[test]
    fn plugin_install_from_an_app_that_ships_no_charter_is_refused() {
        let (_dir, root) = plane();
        let done = fixed(&root, "plugin-install", &crate::Shipped::default())
            .expect("a fix the registry has");
        assert!(!done.complete, "{done:?}");
        assert!(
            done.refused
                .as_deref()
                .is_some_and(|why| why.contains("no `purlis` beside it")),
            "{done:?}"
        );
    }

    #[test]
    fn the_identity_form_reads_the_cores_answer_as_a_fix_or_as_field_refusals() {
        // The core's answer, not a call: the core writes git's global config, and this
        // process's HOME is the operator's. The write is the core's test, on a temporary home.
        use purlis_core::doctor::fix::{Fixed, identity::Invalid};
        let refused = Invalid {
            name: vec![],
            email: vec!["not an email".to_owned()],
        };
        assert_eq!(
            identity_answer(Err(refused)),
            DoctorIdentityFixed::Invalid {
                name: vec![],
                email: vec!["not an email".to_owned()],
            }
        );
        assert_eq!(
            identity_answer(Ok(Fixed::Refused("already set".to_owned()))),
            DoctorIdentityFixed::Fixed {
                fixed: DoctorFixed {
                    fix: "git-identity".to_owned(),
                    refused: Some("already set".to_owned()),
                    said: vec![],
                    complete: false,
                }
            }
        );
    }

    #[test]
    fn a_fix_the_registry_does_not_have_is_refused_by_name() {
        let (_dir, root) = plane();
        let refused =
            fixed(&root, "index-lock", &crate::Shipped::default()).expect_err("no such fix");
        assert!(
            refused.contains("index-lock") && refused.contains("reinit"),
            "{refused}"
        );
    }

    #[test]
    fn the_window_shows_each_forge_accounts_request_budget() {
        use purlis_core::forge::budget::{Meter, SystemClock};
        let (_dir, root) = plane();
        let config = tempfile::tempdir().expect("a directory");
        let budget_rows = |report: &DoctorReport| {
            report
                .rows
                .iter()
                .filter(|r| r.name == "forge budget")
                .map(|r| r.detail.clone())
                .collect::<Vec<_>>()
        };
        assert!(budget_rows(&report_in(&root, false, Some(config.path()), None)).is_empty());
        let account = purlis_core::forge::Account {
            kind: purlis_core::forge::Kind::GitLab,
            host: "gitlab.com".into(),
            login: "octocat".into(),
        };
        Meter::kept_in(config.path(), &account, std::sync::Arc::new(SystemClock)).record(None);
        assert_eq!(
            budget_rows(&report_in(&root, false, Some(config.path()), None)),
            [
                "gitlab octocat@gitlab.com: 1 of 1000 counted requests this hour (1 sent, 0 \
              answered 304 Not Modified); the forge has stated no limit"
            ]
        );
    }

    #[test]
    fn the_apps_own_row_says_where_hook_calls_are_recorded_or_why_they_are_not() {
        let kept = event_log(Some(&Ok(std::path::PathBuf::from("/data/events/DEVICE"))));
        let refused = event_log(Some(&Err(crate::EventLogRefused {
            kind: std::io::ErrorKind::PermissionDenied,
            why: "/repo/data is inside the git work tree at /repo".to_owned(),
        })));
        let busy = event_log(Some(&Err(crate::EventLogRefused {
            kind: std::io::ErrorKind::WouldBlock,
            why: "another host is already writing this device's event log".to_owned(),
        })));
        let never = event_log(None);

        assert_eq!(kept.name, "event log");
        assert_eq!(kept.status, DoctorStatus::Ok);
        assert!(
            kept.detail.contains("/data/events/DEVICE"),
            "{}",
            kept.detail
        );
        assert_eq!(refused.status, DoctorStatus::Warn);
        assert!(
            refused.detail.contains("inside the git work tree"),
            "{}",
            refused.detail
        );
        assert!(
            refused.hint.contains("CHARTER_DATA_HOME"),
            "{}",
            refused.hint
        );
        assert_eq!(
            never.status,
            DoctorStatus::Warn,
            "an absent answer is not health"
        );
        assert!(
            busy.hint.contains("another purlis"),
            "the repair for that reason: {}",
            busy.hint
        );
        assert!(!busy.hint.contains("CHARTER_DATA_HOME"), "{}", busy.hint);
    }

    #[test]
    fn the_apps_own_row_says_whether_a_chat_here_can_record_what_a_turn_cost() {
        let (_dir, root) = plane();

        let free = report(&root, false);

        let row = &free.app_rows[0];
        assert_eq!(row.name, "chat footer");
        assert_eq!(row.status, DoctorStatus::Ok);
        assert!(row.detail.contains("purlis fills"), "{row:?}");
        // Even green, it says what it did not look at.
        assert!(row.hint.contains("MDM"), "{row:?}");
        // And it is NOT in the table `charter doctor` prints.
        assert!(free.rows.iter().all(|r| r.name != "chat footer"));
    }

    #[test]
    fn the_apps_own_row_names_the_file_that_keeps_the_gauge_dark() {
        let (_dir, root) = plane();
        std::fs::create_dir_all(root.join(".claude")).expect(".claude");
        std::fs::write(
            root.join(".claude/settings.json"),
            r#"{"statusLine": {"type": "command", "command": "my-own-line"}}"#,
        )
        .expect("their settings");

        let row = report(&root, false).app_rows.remove(0);

        assert_eq!(row.status, DoctorStatus::Warn);
        assert!(row.checked, "it is a check this build runs");
        assert!(
            row.detail
                .contains(&root.join(".claude/settings.json").display().to_string()),
            "the row does not name the file in force: {row:?}"
        );
        assert!(row.detail.contains("stays dark"), "{row:?}");
        assert!(row.hint.contains("will not replace"), "{row:?}");
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
