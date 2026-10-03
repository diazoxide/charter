//! What the window asks about a project's sandbox (ADR 0067; ruling V78): what the new-chat
//! picker says before a chat starts, the one-time offer to an existing project, this machine's
//! opt-out count, and SD-30's install action.
//!
//! **The window names nothing that runs.** The install action sends a session number and no
//! text: the line typed is built here from charter's own table and what this machine is
//! missing ([`charter_core::sandbox::backend::install_command`]), and it is typed without a
//! newline, so the person reads it and presses Return. It needs `sudo` (ruling V78 c).

use charter_core::sandbox::{self, Ahead, backend};

use crate::planes::{PlaneId, Planes};

/// What the picker says about the sandbox for one profile, before anything starts.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SandboxAhead {
    /// `sandboxed`, `unsandboxed` (this system has no backend) or `refused`.
    pub state: String,
    /// The sentence the picker shows: why it is refused, or why it starts without the sandbox.
    /// Empty for `sandboxed`.
    pub said: String,
    /// SD-30's install command, shown before the person asks for it to be typed: where the
    /// refusal is a program this machine is missing and charter knows its distribution.
    pub install: Option<String>,
}

impl SandboxAhead {
    /// The row for `ahead`, or none where the project has not turned the sandbox on.
    pub fn of(ahead: Ahead) -> Option<Self> {
        let row = |state: &str, said: String, install| Self {
            state: state.to_owned(),
            said,
            install,
        };
        match ahead {
            Ahead::Off => None,
            Ahead::Sandboxed => Some(row("sandboxed", String::new(), None)),
            Ahead::Unsandboxed(lifted) => Some(row("unsandboxed", lifted.notice(), None)),
            Ahead::Refused { why, install } => Some(row("refused", why, install)),
        }
    }
}

/// The text of this machine's `/etc/os-release`, empty where there is none.
fn os_release() -> String {
    std::fs::read_to_string("/etc/os-release")
        .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default()
}

/// What the picker says beside a profile whose program it has not checked: nobody approved
/// it yet, so it is not run, not even to ask its `--version`.
pub const CHECKED_ONCE_APPROVED: &str = "charter checks this profile's program before it \
                                         starts sandboxed, once you approve it.";

/// [`SandboxAhead`] for a chat on `profile` in the project at `root`, on this machine: every
/// refusal the start would give, shown before anything starts — a harness charter holds back
/// (Codex, ruling V87f), and a program the sandbox will not bind (ruling V87g), which is asked
/// of the profile's own resolved words.
///
/// **Only an approved profile's program is run.** The V87g check asks the program its
/// `--version`, which runs it, outside any sandbox. A profile nobody has approved is a command
/// out of a file a chat can write, so its program is not asked: the row says it is checked
/// once approved, as the start does after the approval gate.
pub fn ahead_here(
    profile: &charter_core::profiles::Profile,
    root: &std::path::Path,
) -> Option<SandboxAhead> {
    ahead_with(
        profile,
        root,
        charter_core::profiletrust::approval_needed(root, profile).is_none(),
    )
}

/// [`ahead_here`], told whether the profile is approved: the start's own check
/// ([`charter_core::start::sandbox_ahead`]), which runs the program only when it is.
fn ahead_with(
    profile: &charter_core::profiles::Profile,
    root: &std::path::Path,
    approved: bool,
) -> Option<SandboxAhead> {
    let row = SandboxAhead::of(charter_core::start::sandbox_ahead(
        profile,
        root,
        approved,
        &sandbox::Machine::this(),
        &backend::installed,
        &os_release(),
        None,
    )?)?;
    Some(if !approved && row.state == "sandboxed" {
        SandboxAhead {
            said: CHECKED_ONCE_APPROVED.to_owned(),
            ..row
        }
    } else {
        row
    })
}

/// What the project view says about the sandbox.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SandboxState {
    /// Whether the project turned the sandbox on.
    pub on: bool,
    /// Whether the one-time offer is due: an existing project that has not turned it on, and
    /// nobody on this machine has answered (ruling V21 1).
    pub offer: bool,
    /// This machine's opt-out count in one sentence (V12), where the project has it on. Local
    /// only, never sent (ruling V78 d).
    pub said: Option<String>,
}

fn state_of(root: &std::path::Path) -> SandboxState {
    let on = sandbox::Plane::read(root).said().policy.is_some();
    SandboxState {
        on,
        offer: sandbox::local::offer_due(root),
        said: on.then(|| sandbox::local::tally(root).said()),
    }
}

/// The project's sandbox, for the offer notice and Project settings.
#[tauri::command]
#[specta::specta]
pub fn sandbox_state(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<SandboxState, String> {
    Ok(state_of(planes.held(&plane)?.root()))
}

/// The person's answer to the one-time offer: `turn_on` writes `[sandbox] mode = "on"` into
/// the project's `charter.toml`; either answer is the last time it is asked on this machine.
#[tauri::command]
#[specta::specta]
pub fn answer_sandbox_offer(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    turn_on: bool,
) -> Result<SandboxState, String> {
    let held = planes.held(&plane)?;
    answer(held.root(), turn_on)
}

fn answer(root: &std::path::Path, turn_on: bool) -> Result<SandboxState, String> {
    let answer = if turn_on {
        sandbox::local::Answer::TurnOn
    } else {
        sandbox::local::Answer::KeepItOff
    };
    sandbox::local::answer(root, answer).map_err(|err| err.to_string())?;
    Ok(state_of(root))
}

/// The line [`type_sandbox_install`] types: SD-30's command for what `missing` names on the
/// distribution `os_release` says this is, **with no newline** — the person runs it.
fn install_line(missing: Option<&backend::Missing>, os_release: &str) -> Result<String, String> {
    missing
        .and_then(|missing| backend::install_command(missing, os_release))
        .ok_or_else(|| {
            "charter has nothing to install for the sandbox on this machine, so nothing was \
             typed."
                .to_owned()
        })
}

/// Types SD-30's install command into shell session `session` at the project root, and does
/// not run it (ruling V78 c): installing needs `sudo`, so the person reads it and presses
/// Return. The window sends no text; the line is built here, and it is typed only into a shell
/// tab at the project root.
#[tauri::command]
#[specta::specta]
pub fn type_sandbox_install(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
) -> Result<(), String> {
    let held = planes.held(&plane)?;
    // Into the operator's own shell at the project root, which the window opened for it, and
    // never into an agent's pane or a shell elsewhere.
    if !held.chats().is_shell_at(session, held.root()) {
        return Err(
            "that tab is not a shell at the project root, so the install command was not \
             typed."
                .to_owned(),
        );
    }
    let missing = backend::missing(sandbox::Os::this(), &backend::installed);
    let line = install_line(missing.as_ref(), &os_release())?;
    held.operator_input(session, line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_without_the_sandbox_shows_the_picker_nothing() {
        assert_eq!(SandboxAhead::of(Ahead::Off), None);
    }

    #[test]
    fn the_picker_is_told_a_refusal_with_its_install_command() {
        assert_eq!(
            SandboxAhead::of(Ahead::Refused {
                why: "socat is not installed".to_owned(),
                install: Some("sudo apt install socat".to_owned()),
            }),
            Some(SandboxAhead {
                state: "refused".to_owned(),
                said: "socat is not installed".to_owned(),
                install: Some("sudo apt install socat".to_owned()),
            })
        );
    }

    #[test]
    fn on_windows_the_picker_is_told_the_chat_starts_without_it_and_why() {
        let lifted = sandbox::Lifted {
            by: sandbox::By::NoBackend(sandbox::Os::Windows),
            reason: None,
        };
        let row = SandboxAhead::of(Ahead::Unsandboxed(lifted.clone())).expect("a row");
        assert_eq!(row.state, "unsandboxed");
        assert_eq!(row.said, lifted.notice());
    }

    /// Ruling V78 c: the line is the compiled-in command for what is missing, typed and never
    /// run, so it carries no newline.
    #[test]
    fn the_install_line_is_charters_own_command_with_no_return_at_the_end() {
        let missing = backend::missing(sandbox::Os::Linux, &|_| false);
        assert_eq!(
            install_line(missing.as_ref(), "ID=fedora\n").as_deref(),
            Ok("sudo dnf install bubblewrap socat")
        );
        assert!(
            install_line(None, "ID=fedora\n").is_err(),
            "nothing missing"
        );
        assert!(
            install_line(missing.as_ref(), "ID=nixos\n").is_err(),
            "a distribution charter does not know"
        );
    }

    #[test]
    fn taking_the_offer_turns_the_project_on_and_it_is_not_offered_again() {
        let project = tempfile::tempdir().expect("a project");
        std::fs::write(project.path().join("charter.toml"), "schema = 1\n").expect("toml");
        assert_eq!(
            state_of(project.path()),
            SandboxState {
                on: false,
                offer: true,
                said: None,
            }
        );

        let after = answer(project.path(), true).expect("answered");

        assert_eq!(
            after,
            SandboxState {
                on: true,
                offer: false,
                said: Some(
                    "no chat has started under this project's sandbox on this machine yet"
                        .to_owned()
                ),
            }
        );
    }

    #[test]
    fn keeping_it_off_leaves_the_project_as_it_was_and_asks_no_more() {
        let project = tempfile::tempdir().expect("a project");
        std::fs::write(project.path().join("charter.toml"), "schema = 1\n").expect("toml");

        let after = answer(project.path(), false).expect("answered");

        assert_eq!(
            after,
            SandboxState {
                on: false,
                offer: false,
                said: None,
            }
        );
        assert_eq!(
            std::fs::read_to_string(project.path().join("charter.toml")).expect("toml"),
            "schema = 1\n"
        );
    }

    /// Must-fix (verifier r7): the picker never runs a profile's program before a person
    /// approved it. Once approved, its program is checked, which runs it.
    #[cfg(unix)]
    #[test]
    fn the_picker_runs_no_program_nobody_approved() {
        let project = tempfile::tempdir().expect("a project");
        let outside = tempfile::tempdir().expect("outside");
        let marker = outside.path().join("ran");
        std::fs::write(
            project.path().join("charter.toml"),
            "[sandbox]\nmode = \"on\"\n",
        )
        .expect("charter.toml");
        // Run as `/bin/sh <script>`: a program in a temp folder is one a chat could write, which
        // the start refuses before it runs anything (ruling V87g).
        let script = outside.path().join("claude");
        std::fs::write(
            &script,
            format!("touch {:?}\necho '2.1.288 (Claude Code)'\n", marker),
        )
        .expect("the script");
        std::fs::write(
            project.path().join(charter_core::profiles::LOCAL_FILE),
            format!(
                "[harness.work]\nkind = \"claude\"\ncommand = [\"/bin/sh\", {:?}]\n",
                script.display().to_string()
            ),
        )
        .expect("a profile");
        let set = charter_core::profiles::current(project.path());
        let profile = set.get("work").expect("declared");

        let row = ahead_with(profile, project.path(), false);

        assert!(!marker.exists(), "an unapproved program was run");
        let row = row.expect("a row");
        // Where this machine can apply the sandbox at all (not a CI runner without bubblewrap):
        // the row says the check waits for the approval, and an approved program is checked.
        if row.state == "sandboxed" {
            assert_eq!(row.said, CHECKED_ONCE_APPROVED);
            let approved = ahead_with(profile, project.path(), true).expect("a row");
            assert!(marker.exists(), "an approved program is checked");
            assert_eq!(approved.state, "sandboxed", "{approved:?}");
            assert_eq!(approved.said, "");
        }
    }
}
