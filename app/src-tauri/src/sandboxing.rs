//! What the window asks about a project's sandbox (ADR 0067; ruling V78): what the new-chat
//! picker says before a chat starts, the one-time offer to an existing project, this machine's
//! opt-out count, and SD-30's install action.
//!
//! **The window names nothing that runs.** The install action sends a session number and no
//! text: the line typed is built here from charter's own table and what this machine is
//! missing ([`purlis_core::sandbox::backend::install_command`]), and it is typed without a
//! newline, so the person reads it and presses Return. It needs `sudo` (ruling V78 c).

use purlis_core::sandbox::{self, Ahead, backend};

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
pub const CHECKED_ONCE_APPROVED: &str = "purlis checks this profile's program before it \
                                         starts sandboxed, once the profile may start: approved, \
                                         and declared in a file git does not carry.";

/// [`SandboxAhead`] for a chat on `profile` in the project at `root`, on this machine: every
/// refusal the start would give, shown before anything starts — a harness charter holds back
/// (Codex, ruling V87f), and a program the sandbox will not bind (ruling V87g), which is asked
/// of the profile's own resolved words.
///
/// **Only a profile the start would run has its program run.** The V87g check asks the
/// program its `--version`, which runs it, outside any sandbox. So it is asked only past the
/// start's own gate ([`purlis_core::start::may_check_program`], which is
/// `wiring::refusal`): a startable kind, a `charter.local.toml` git would not carry, an
/// approved command. Any other row says the program is checked once the profile may start.
pub fn ahead_here(
    profile: &purlis_core::profiles::Profile,
    root: &std::path::Path,
) -> Option<SandboxAhead> {
    ahead_with(
        profile,
        root,
        purlis_core::start::may_check_program(profile, root),
    )
}

/// [`ahead_here`], told whether the start's gate lets the program be checked: the start's own
/// check ([`purlis_core::start::sandbox_ahead`]), which asks the same gate itself and runs
/// the program only past it. `approved` only phrases the row.
fn ahead_with(
    profile: &purlis_core::profiles::Profile,
    root: &std::path::Path,
    approved: bool,
) -> Option<SandboxAhead> {
    let row = SandboxAhead::of(purlis_core::start::sandbox_ahead(
        profile,
        root,
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
    /// The harnesses whose chats never start sandboxed on this machine, whatever the project,
    /// each with why (`sandbox::never_on`): Codex and opencode off macOS. The offer
    /// says them, so "every new chat runs sandboxed" is never read as covering them.
    pub never: Vec<String>,
    /// How the project's own hosts changed since this machine last told the person (#1341):
    /// the one-time Notice each teammate sees. `null` when nothing did.
    pub hosts_changed: Option<HostsChanged>,
}

/// The project's hosts as they changed (`sandbox::local::HostsChange`): each spelled as the
/// sandbox writes it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct HostsChanged {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// The whole list now: what the Notice sends back once it is read.
    pub now: Vec<String>,
}

fn state_of(root: &std::path::Path) -> SandboxState {
    state_on(root, sandbox::Os::this())
}

/// [`state_of`] on `os`.
fn state_on(root: &std::path::Path, os: sandbox::Os) -> SandboxState {
    let on = sandbox::Plane::read(root).said().policy.is_some();
    SandboxState {
        on,
        offer: sandbox::local::offer_due(root),
        said: on.then(|| sandbox::local::tally(root).said()),
        never: never_here(os),
        hosts_changed: sandbox::local::hosts_changed(root).map(|change| HostsChanged {
            added: change.added,
            removed: change.removed,
            now: change.now,
        }),
    }
}

/// Each harness that never starts sandboxed on `os`, as "<title>: <why>".
fn never_here(os: sandbox::Os) -> Vec<String> {
    purlis_core::harness::Harness::ALL
        .into_iter()
        .filter_map(|harness| {
            sandbox::never_on(harness, os).map(|why| format!("{}: {why}", harness.title()))
        })
        .collect()
}

/// The project's sandbox, for the offer notice and Settings.
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

/// The person read the Notice of the project's hosts as it showed them, `shown` (#1341): it is
/// not shown again until they change from that.
#[tauri::command]
#[specta::specta]
pub fn acknowledge_project_hosts(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    shown: Vec<String>,
) -> Result<SandboxState, String> {
    let held = planes.held(&plane)?;
    acknowledge(held.root(), &shown)
}

fn acknowledge(root: &std::path::Path, shown: &[String]) -> Result<SandboxState, String> {
    sandbox::local::acknowledge_hosts(root, shown).map_err(|err| err.to_string())?;
    Ok(state_of(root))
}

/// The line [`type_sandbox_install`] types: SD-30's command for what `missing` names on the
/// distribution `os_release` says this is, **with no newline** — the person runs it.
fn install_line(missing: Option<&backend::Missing>, os_release: &str) -> Result<String, String> {
    missing
        .and_then(|missing| backend::install_command(missing, os_release))
        .ok_or_else(|| {
            "purlis has nothing to install for the sandbox on this machine, so nothing was \
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

/// **A Report of a sandbox block of purlis's own** (#1338), as the window shows it before
/// anything is sent: the scrubbed draft `purlis report bug` would file, and its digest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct BlockReport {
    /// The repository it would be filed on.
    pub repository: String,
    pub title: String,
    pub body: String,
    /// What filing names, so only exactly this draft is filed.
    pub digest: String,
}

/// The draft for a block of purlis's own `operation` on `kind`, from the chat running `harness`.
///
/// **Made from the fixed words alone.** The window hands back the two words the Notice was told,
/// and a word that is not one of [`purlis_core::sandboxblock`]'s is refused, so no text from the
/// window, or from the chat that sent the block, can reach the draft.
fn block_draft(
    operation: &str,
    kind: &str,
    harness: Option<&str>,
) -> Result<purlis_core::report::Draft, String> {
    use purlis_core::sandboxblock::{Block, Kind, Operation};
    let block = Block {
        operation: Operation::of_word(operation)
            .ok_or_else(|| "That is not an operation the sandbox reports.".to_owned())?,
        kind: Kind::of_word(kind)
            .ok_or_else(|| "That is not a kind of path or host the sandbox reports.".to_owned())?,
        ours: true,
    };
    let harness = harness.and_then(purlis_core::harness::Harness::of_kind);
    purlis_core::report::Draft::of_sandbox_block(&block, harness).map_err(|refused| refused.0)
}

/// The draft of a Report for a sandbox block of purlis's own (#1338). Nothing is sent: this only
/// drafts, here, with no network.
#[tauri::command]
#[specta::specta]
pub fn sandbox_block_report(
    operation: String,
    kind: String,
    harness: Option<String>,
) -> Result<BlockReport, String> {
    let draft = block_draft(&operation, &kind, harness.as_deref())?;
    Ok(BlockReport {
        repository: purlis_core::report::UPSTREAM.to_owned(),
        digest: draft.digest(),
        title: draft.title,
        body: draft.body,
    })
}

/// Files the Report the window showed (#1338), on the person's press and never otherwise: by the
/// app, under the person's own `gh` login and never a token from the environment
/// ([`purlis_core::report::file`]), not by anything inside a chat's sandbox. Only the draft whose
/// digest is `digest` is filed; one that changed since it was shown is refused unsent. Answers
/// the new issue's address, or why it was not filed with the link that files it in a browser.
#[tauri::command]
#[specta::specta]
pub async fn file_sandbox_block_report(
    operation: String,
    kind: String,
    harness: Option<String>,
    digest: String,
) -> Result<String, String> {
    let draft = shown_draft(&operation, &kind, harness.as_deref(), &digest)?;
    tauri::async_runtime::spawn_blocking(move || {
        purlis_core::report::file(&draft).map_err(|why| {
            let (url, whole) = draft.fallback_url();
            format!(
                "It was not filed: gh said {why}. Open this link to file it in a browser{}: {url}",
                if whole {
                    ""
                } else {
                    ", and paste the body from the draft"
                }
            )
        })
    })
    .await
    .map_err(|err| format!("The report did not finish: {err}"))?
}

/// The draft for a block, only if its digest is the one the window showed.
fn shown_draft(
    operation: &str,
    kind: &str,
    harness: Option<&str>,
    digest: &str,
) -> Result<purlis_core::report::Draft, String> {
    let draft = block_draft(operation, kind, harness)?;
    if draft.digest() != digest {
        return Err(
            "The draft is not the one that was shown, so nothing was sent. Open Report again."
                .to_owned(),
        );
    }
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_report_is_drafted_from_the_fixed_words_alone() {
        let report = sandbox_block_report(
            "write".to_owned(),
            "project-files".to_owned(),
            Some("claude".to_owned()),
        )
        .unwrap();
        assert_eq!(report.repository, purlis_core::report::UPSTREAM);
        assert_eq!(
            report.title,
            "Sandbox blocked purlis's own write (project-files)"
        );
        assert!(
            report.body.contains("- **harness:** claude"),
            "{}",
            report.body
        );
        // A harness word purlis does not start is not one, and says nothing of itself.
        let other = sandbox_block_report(
            "write".to_owned(),
            "project-files".to_owned(),
            Some("/home/dev/my-harness".to_owned()),
        )
        .unwrap();
        assert!(!other.body.contains("/home/dev"), "{}", other.body);
        assert!(
            other.body.contains("- **harness:** not known"),
            "{}",
            other.body
        );
    }

    #[test]
    fn a_block_report_refuses_words_the_sandbox_does_not_report() {
        for (operation, kind) in [
            ("write", "/Users/dev/plane/workspaces/a/sessions"),
            ("rm -rf /", "project-files"),
            ("", ""),
        ] {
            assert!(
                sandbox_block_report(operation.to_owned(), kind.to_owned(), None).is_err(),
                "{operation} {kind}"
            );
        }
    }

    #[test]
    fn only_the_draft_that_was_shown_is_filed() {
        let shown = sandbox_block_report("connect".to_owned(), "host".to_owned(), None).unwrap();
        assert!(shown_draft("connect", "host", None, &shown.digest).is_ok());
        let refused = shown_draft("connect", "host", Some("claude"), &shown.digest).unwrap_err();
        assert!(refused.contains("nothing was sent"), "{refused}");
        assert!(shown_draft("write", "host", None, &shown.digest).is_err());
    }

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
            "a distribution purlis does not know"
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
                never: never_here(sandbox::Os::this()),
                hosts_changed: None,
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
                never: never_here(sandbox::Os::this()),
                hosts_changed: None,
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
                never: never_here(sandbox::Os::this()),
                hosts_changed: None,
            }
        );
        assert_eq!(
            std::fs::read_to_string(project.path().join("charter.toml")).expect("toml"),
            "schema = 1\n"
        );
    }

    /// Must-fix (verifier r7): the picker never runs a profile's program before the start's
    /// gate lets it start (approved, among the rest). Past the gate, its program is checked,
    /// which runs it. The program answers as something other than Claude Code, so whether it
    /// ran shows in the row: the probe runs inside charter's wrap (D-88k), where it can leave
    /// no other trace.
    #[cfg(unix)]
    #[test]
    fn the_picker_runs_no_program_nobody_approved() {
        let project = tempfile::tempdir().expect("a project");
        // A folder no chat writes: the start refuses a program, or any file its command
        // names, in a temp folder (ruling V87g, D-88g).
        let outside = stand_in::NoChatWrites::new();
        std::fs::write(
            project.path().join("charter.toml"),
            "[sandbox]\nmode = \"on\"\n",
        )
        .expect("charter.toml");
        // Run as `/bin/sh <script>`.
        let script = outside.path().join("claude");
        std::fs::write(&script, "echo 'not claude'\n").expect("the script");
        std::fs::write(
            project.path().join(purlis_core::profiles::LOCAL_FILE),
            format!(
                "[harness.work]\nkind = \"claude\"\ncommand = [\"/bin/sh\", {:?}]\n",
                script.display().to_string()
            ),
        )
        .expect("a profile");
        let set = purlis_core::profiles::current(project.path());
        let profile = set.get("work").expect("declared");

        let row = ahead_here(profile, project.path()).expect("a row");

        // Where this machine can apply the sandbox at all (not a CI runner without bubblewrap):
        // the row says the check waits for the approval, so the program was not run; once
        // approved, it is checked, and its answer refuses it.
        if row.state == "sandboxed" {
            assert_eq!(
                row.said, CHECKED_ONCE_APPROVED,
                "an unapproved program was run"
            );
            purlis_core::profiletrust::record_launched(
                project.path(),
                "work",
                &purlis_core::profiletrust::fingerprint(profile),
            )
            .expect("approved");
            let approved = ahead_here(profile, project.path()).expect("a row");
            assert_eq!(approved.state, "refused", "an approved program is checked");
            assert!(
                approved.said.contains("does not answer as Claude Code"),
                "{approved:?}"
            );
        }
    }

    /// #1341: a teammate is told once of the project's hosts, and not again once it was read.
    #[test]
    fn the_project_s_hosts_are_told_once_and_not_again_once_read() {
        let project = tempfile::tempdir().expect("a project");
        std::fs::write(
            project.path().join("charter.toml"),
            "[sandbox]\nmode = \"on\"\nhosts = [\"10.100.39.145:6443\"]\n",
        )
        .expect("toml");
        let told = state_of(project.path()).hosts_changed.expect("told");
        assert_eq!(told.added, ["10.100.39.145:6443"]);
        assert_eq!(told.removed, Vec::<String>::new());

        let after = acknowledge(project.path(), &told.now).expect("read");

        assert_eq!(after.hosts_changed, None);
    }

    /// Fold-in (round 11): the offer never reads as covering a harness that is never sandboxed
    /// on this system, whatever the project.
    #[test]
    fn the_offer_names_each_harness_never_sandboxed_on_this_system() {
        // #1123: charter wraps Codex as it wraps opencode, on macOS so far.
        assert!(never_here(sandbox::Os::MacOs).is_empty());
        assert_eq!(
            never_here(sandbox::Os::Linux),
            [
                "Codex: purlis can wrap it on macOS only, so far",
                "opencode: purlis can wrap it on macOS only, so far",
            ]
        );
    }
}
